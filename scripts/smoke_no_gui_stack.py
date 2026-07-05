#!/usr/bin/env python3
"""
Smoke test for Ghostlink launcher in --no-gui mode.
Starts services, verifies gateway health + model status, and shuts down cleanly.
"""

from __future__ import annotations

import json
import os
import signal
import socket
import subprocess
import sys
import time
from pathlib import Path
from urllib.request import urlopen, Request
from urllib.error import URLError, HTTPError

ROOT = Path(__file__).resolve().parent.parent
LAUNCHER = ROOT / "scripts" / "launch_studio.py"


def find_free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def wait_json(url: str, timeout_s: float = 30.0) -> dict:
    deadline = time.time() + timeout_s
    last_error = None
    while time.time() < deadline:
        try:
            req = Request(url, method="GET")
            with urlopen(req, timeout=2.0) as response:  # nosec B310 - local smoke endpoint
                data = response.read().decode("utf-8")
                return json.loads(data)
        except (URLError, HTTPError, TimeoutError, json.JSONDecodeError) as exc:
            last_error = exc
            time.sleep(0.5)
    raise RuntimeError(f"Timed out waiting for {url}: {last_error}")


def stop_process_tree(proc: subprocess.Popen[str]) -> None:
    if proc.poll() is not None:
        return

    proc.send_signal(signal.SIGINT)
    try:
        proc.wait(timeout=10)
        return
    except subprocess.TimeoutExpired:
        pass

    proc.terminate()
    try:
        proc.wait(timeout=5)
        return
    except subprocess.TimeoutExpired:
        proc.kill()


def main() -> int:
    model_manager_port = find_free_port()
    backend_port = find_free_port()
    proxy_port = find_free_port()

    env = os.environ.copy()
    env["GHOSTLINK_STUDIO_GUI"] = "tkinter"
    env["GHOSTLINK_MODEL_MANAGER_PORT"] = str(model_manager_port)
    env["GHOSTLINK_BACKEND_PORT"] = str(backend_port)
    env["GHOSTLINK_PROXY_PORT"] = str(proxy_port)
    env["PYTHONUNBUFFERED"] = "1"

    command = [sys.executable, str(LAUNCHER), "--no-gui"]
    print(f"[smoke] launch command: {' '.join(command)}")
    print(
        "[smoke] ports: "
        f"model_manager={model_manager_port} backend={backend_port} gateway={proxy_port}"
    )

    proc = subprocess.Popen(
        command,
        cwd=str(ROOT),
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=1,
    )

    try:
        # Wait for startup signal in logs first, then verify endpoints.
        startup_deadline = time.time() + 90
        saw_headless = False
        while time.time() < startup_deadline:
            if proc.poll() is not None:
                raise RuntimeError("launch_studio.py exited before services became ready")

            line = proc.stdout.readline() if proc.stdout else ""
            if line:
                line = line.rstrip("\n")
                print(f"[smoke][launcher] {line}")
                if "Headless mode active" in line:
                    saw_headless = True
                    break

        if not saw_headless:
            raise RuntimeError("did not observe headless startup confirmation")

        health = wait_json(f"http://127.0.0.1:{proxy_port}/health", timeout_s=30)
        model_status = wait_json(f"http://127.0.0.1:{proxy_port}/api/models/status", timeout_s=30)

        print(f"[smoke] /health -> {json.dumps(health, sort_keys=True)}")
        print(f"[smoke] /api/models/status -> {json.dumps(model_status, sort_keys=True)}")

        if str(health.get("status", "")).lower() != "healthy":
            raise RuntimeError("gateway health did not report healthy")

        if "loaded_models" not in model_status:
            raise RuntimeError("model status response missing loaded_models")

        print("[smoke] PASS: no-gui stack health checks succeeded")
        return 0
    finally:
        stop_process_tree(proc)


if __name__ == "__main__":
    raise SystemExit(main())
