#!/usr/bin/env python3
"""CI smoke test for Ghostlink GUI backend API surfaces."""

from __future__ import annotations

import json
import os
import signal
import subprocess
import sys
import time
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parent.parent
HOST = "127.0.0.1"
PORT = 18013
BASE_URL = f"http://{HOST}:{PORT}"
BINARY_PATH = Path(os.environ.get("CARGO_TARGET_DIR") or ROOT / "target") / "debug" / (
    "ghost-link.exe" if sys.platform == "win32" else "ghost-link"
)


API_KEY_PATH = ROOT / "api_key.txt"


def _wait_for_api_key(max_wait_s: int = 20) -> str:
    """The server generates and persists a real API key at startup (see
    crates/ghost-link/src/auth.rs) — every route but /health now requires
    it as a bearer token. Polls for the file rather than assuming it's
    already there the instant the process starts."""
    deadline = time.time() + max_wait_s
    while time.time() < deadline:
        try:
            key = API_KEY_PATH.read_text(encoding="utf-8").strip()
            if key:
                return key
        except FileNotFoundError:
            pass
        time.sleep(0.2)
    raise RuntimeError(f"API key file never appeared at {API_KEY_PATH}")


def _auth_headers() -> dict:
    return {"Authorization": f"Bearer {_wait_for_api_key()}"}


def _get_json(path: str, timeout: float = 5.0, auth: bool = True) -> dict:
    headers = _auth_headers() if auth else {}
    req = Request(f"{BASE_URL}{path}", method="GET", headers=headers)
    with urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read().decode("utf-8"))


def _post_json(path: str, payload: dict, timeout: float = 10.0, auth: bool = True) -> dict:
    body = json.dumps(payload).encode("utf-8")
    headers = {"Content-Type": "application/json"}
    if auth:
        headers.update(_auth_headers())
    req = Request(
        f"{BASE_URL}{path}",
        data=body,
        headers=headers,
        method="POST",
    )
    with urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read().decode("utf-8"))


def _wait_for_health(max_wait_s: int = 45) -> None:
    deadline = time.time() + max_wait_s
    while time.time() < deadline:
        try:
            health = _get_json("/health", timeout=1.5, auth=False)
            if health.get("status") == "healthy":
                return
        except (URLError, HTTPError, TimeoutError, OSError, ValueError):
            time.sleep(0.5)
    raise RuntimeError("backend health endpoint did not become ready")


def _build_backend() -> None:
    subprocess.run(
        ["cargo", "build", "-p", "ghost-link", "--bin", "ghost-link"],
        cwd=str(ROOT),
        check=True,
        stdout=None,
        stderr=None,
    )


def main() -> int:
    _build_backend()

    proc = subprocess.Popen(
        [str(BINARY_PATH), "serve", HOST, str(PORT)],
        cwd=str(ROOT),
        stdout=None,
        stderr=None,
        preexec_fn=None,
    )

    try:
        _wait_for_health()

        health = _get_json("/health", auth=False)
        if health.get("status") != "healthy":
            raise RuntimeError("health endpoint returned non-healthy status")

        models = _get_json("/api/models")
        if not isinstance(models.get("models"), list):
            raise RuntimeError("/api/models did not return a models list")

        ollama_health = _get_json("/api/ollama/health")
        if "reachable" not in ollama_health:
            raise RuntimeError("/api/ollama/health missing reachable field")

        chat = _post_json(
            "/api/inference/chat",
            {
                "message": "ci smoke",
                "model": "neural-chat",
                "max_tokens": 32,
                "ollama_url": "http://127.0.0.1:11434",
            },
            timeout=20.0,
        )
        response = str(chat.get("response", ""))
        if not response:
            raise RuntimeError("chat response was empty")

        print("GUI backend smoke passed")
        return 0
    finally:
        if proc.poll() is None:
            proc.send_signal(signal.SIGTERM)
            try:
                proc.wait(timeout=8)
            except subprocess.TimeoutExpired:
                proc.kill()


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"GUI backend smoke failed: {exc}", file=sys.stderr)
        raise
