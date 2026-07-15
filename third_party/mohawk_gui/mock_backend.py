"""Lightweight backend API used by docker-compose for local GUI verification."""

from datetime import datetime
from typing import Any
from fastapi import FastAPI

app = FastAPI(title="Mohawk GUI Mock Backend", version="1.0.0")

MODELS: list[dict[str, Any]] = [
    {
        "name": "Llama-3-8B",
        "size_gb": 14,
        "type": "LLM",
        "quantization": "Q4_K_M",
        "status": "Ready",
    },
    {
        "name": "Mistral-7B",
        "size_gb": 8,
        "type": "LLM",
        "quantization": "Q4_K_M",
        "status": "Ready",
    },
]
CURRENT_MODEL = "Llama-3-8B"


@app.get("/health")
async def health() -> dict:
    return {"status": "healthy", "service": "mohawk-gui-backend"}


@app.get("/api/workers")
async def list_workers() -> dict:
    return {
        "workers": [
            {
                "id": "worker_0",
                "host": "localhost",
                "port": 8003,
                "status": "Connected",
                "model": "Llama-3-8B",
                "threads": 8,
                "load": 25,
            },
            {
                "id": "worker_1",
                "host": "localhost",
                "port": 8004,
                "status": "Connected",
                "model": "Mistral-7B",
                "threads": 8,
                "load": 18,
            },
        ]
    }


@app.get("/api/models")
async def list_models() -> dict:
    return {"models": MODELS, "current_model": CURRENT_MODEL}


@app.post("/api/models/load")
async def load_model(payload: dict[str, Any]) -> dict:
    global CURRENT_MODEL
    model_name = str(payload.get("model", "")).strip()
    if not model_name:
        return {"status": "error", "error": "missing model name"}

    known_names = {m["name"] for m in MODELS}
    if model_name not in known_names:
        return {"status": "error", "error": f"model not found: {model_name}"}

    CURRENT_MODEL = model_name
    return {"status": "ok", "current_model": CURRENT_MODEL}


@app.post("/api/models/download")
async def download_model(payload: dict[str, Any]) -> dict:
    model_id = str(payload.get("model_id", "")).strip()
    if not model_id:
        return {"status": "error", "error": "missing model_id"}

    new_model = {
        "name": model_id,
        "size_gb": 6,
        "type": "LLM",
        "quantization": "Q8_0",
        "status": "Ready",
    }
    if not any(m["name"] == model_id for m in MODELS):
        MODELS.append(new_model)
    return {"status": "ok", "model": model_id}


@app.post("/api/workers/connect")
async def connect_workers() -> dict:
    return {"status": "ok", "connected": 2}


@app.post("/api/workers/add")
async def add_worker(payload: dict[str, Any]) -> dict:
    host = str(payload.get("host", "")).strip()
    port = int(payload.get("port", 0))
    if not host or port <= 0:
        return {"status": "error", "error": "host and valid port are required"}
    return {
        "status": "ok",
        "worker": {
            "id": "worker_new",
            "host": host,
            "port": port,
            "status": "Connected",
        },
    }


@app.post("/api/queue")
async def enqueue_job(payload: dict[str, Any]) -> dict:
    priority = str(payload.get("priority", "normal")).strip().lower()
    if priority not in {"high", "normal"}:
        return {"status": "error", "error": f"unsupported priority: {priority}"}
    return {"status": "ok", "priority": priority, "queued": True}


@app.post("/api/security/jwt/refresh")
async def refresh_jwt() -> dict:
    return {"status": "ok", "refreshed": True}


@app.post("/api/security/pqc/enable")
async def enable_pqc() -> dict:
    return {"status": "ok", "pqc_enabled": True}


@app.get("/api/metrics")
async def metrics() -> dict:
    # Keep values in GUI progress bar ranges.
    return {
        "metrics": {
            "throughput": 118420,
            "latency_p50": 2,
            "latency_p95": 4,
            "cpu": 31,
            "memory": 44,
            "gpu": 27,
            "timestamp": datetime.utcnow().isoformat() + "Z",
        }
    }


@app.get("/api/sessions")
async def sessions() -> dict:
    return {
        "sessions": [
            {
                "id": "sess_001",
                "model": "Llama-3-8B",
                "status": "Running",
                "throughput": 420,
                "latency": 23,
                "tokens": 1980,
            }
        ]
    }


@app.post("/api/inference/chat")
async def inference_chat(payload: dict[str, Any]) -> dict:
    message = str(payload.get("message", "")).strip()
    if not message:
        return {"status": "error", "error": "message is required"}

    mcp = payload.get("mcp")
    if mcp is not None and not isinstance(mcp, dict):
        return {"status": "error", "error": "mcp must be a JSON object"}

    response = f"Mock response from {CURRENT_MODEL}: {message[:120]}"
    return {
        "status": "ok",
        "response": response,
        "model": CURRENT_MODEL,
        "mcp_received": mcp is not None,
    }


@app.post("/api/sessions/{session_id}/cancel")
async def cancel_session(session_id: str) -> dict:
    return {"status": "ok", "cancelled": session_id}
