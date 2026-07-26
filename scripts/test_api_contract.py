#!/usr/bin/env python3
"""Contract assertions for core Ghostlink backend API endpoints."""

from __future__ import annotations

import json
import signal
import subprocess
import sys
import time
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parent.parent
HOST = "127.0.0.1"
PORT = 18014
BASE_URL = f"http://{HOST}:{PORT}"


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


def _get(path: str, timeout: float = 5.0, auth: bool = True) -> dict:
    headers = _auth_headers() if auth else {}
    req = Request(f"{BASE_URL}{path}", method="GET", headers=headers)
    with urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read().decode("utf-8"))


def _post(path: str, payload: dict, timeout: float = 10.0, auth: bool = True) -> dict:
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


def _wait_ready(max_wait_s: int = 45) -> None:
    deadline = time.time() + max_wait_s
    while time.time() < deadline:
        try:
            data = _get("/health", timeout=1.5, auth=False)
            if data.get("status") == "healthy":
                return
        except (URLError, HTTPError, TimeoutError, OSError, ValueError):
            time.sleep(0.5)
    raise RuntimeError("backend failed to become healthy")


def _assert_keys(obj: dict, keys: list[str], context: str) -> None:
    missing = [key for key in keys if key not in obj]
    if missing:
        raise AssertionError(f"{context} missing keys: {', '.join(missing)}")


def main() -> int:
    proc = subprocess.Popen(
        ["cargo", "run", "-p", "ghost-link", "--", "serve", HOST, str(PORT)],
        cwd=str(ROOT),
        stdout=subprocess.DEVNULL,
        stderr=subprocess.STDOUT,
    )

    try:
        _wait_ready()

        models = _get("/api/models")
        _assert_keys(
            models,
            ["models", "current_model", "total_models", "loaded_count"],
            "/api/models response",
        )
        if not isinstance(models["models"], list):
            raise AssertionError("/api/models 'models' must be a list")

        model_status = _get("/api/models/status")
        _assert_keys(
            model_status,
            ["loaded_models", "downloading_models", "current_model"],
            "/api/models/status response",
        )
        if not isinstance(model_status["loaded_models"], list):
            raise AssertionError("/api/models/status 'loaded_models' must be a list")

        runtime_recommend = _get("/api/runtime/recommend")
        _assert_keys(
            runtime_recommend,
            ["detected_runtime", "available_memory_gb", "recommended_models", "count"],
            "/api/runtime/recommend response",
        )
        if not isinstance(runtime_recommend["recommended_models"], list):
            raise AssertionError("/api/runtime/recommend 'recommended_models' must be a list")

        ollama_health = _get("/api/ollama/health")
        _assert_keys(
            ollama_health,
            ["reachable", "ollama_url", "model_count", "detail"],
            "/api/ollama/health response",
        )

        chat = _post(
            "/api/inference/chat",
            {
                "message": "contract-test",
                "model": "neural-chat",
                "max_tokens": 32,
                "ollama_url": "http://127.0.0.1:11434",
            },
            timeout=20.0,
        )
        _assert_keys(
            chat,
            [
                "response",
                "request_id",
                "session_id",
                "model",
                "ollama_url",
                "tokens_estimated",
                "exec_tokens",
                "exec_micro_batch",
            ],
            "/api/inference/chat response",
        )

        if not isinstance(chat["response"], str) or not chat["response"]:
            raise AssertionError("/api/inference/chat response must be non-empty string")

        session_id = str(chat["session_id"])
        cancel_result = _post(f"/api/sessions/{session_id}/cancel", {}, timeout=10.0)
        _assert_keys(cancel_result, ["status", "session_id", "cancelled"], "session cancel response")

        print("API contract tests passed")
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
        print(f"API contract tests failed: {exc}", file=sys.stderr)
        raise
