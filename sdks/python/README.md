# ghostlink-client

Python client for a [Ghostlink](https://github.com/rwilliamspbg-ops/Ghostlink) Studio API server — the OpenAI-compatible REST endpoints (`/v1/chat/completions`, `/v1/completions`, `/v1/embeddings`, `/v1/models`) plus Ghostlink's native `/api/*` surface (workers, sessions, settings, metrics, and real token-by-token streaming chat).

## Install

```bash
pip install -e ./sdks/python
```

(Not yet published to PyPI — install from a local checkout or a Git URL.)

## Quick start

```python
from ghostlink_client import GhostlinkClient

client = GhostlinkClient("http://127.0.0.1:8003", api_key="<your api key>")

resp = client.chat.completions.create(
    model="llama3.2:3b",
    messages=[{"role": "user", "content": "Say hello in one sentence."}],
)
print(resp.content)
```

The API key is the one Ghostlink generates on first run (printed at startup, persisted to `api_key.txt` by default — see `GHOSTLINK_API_KEY_PATH`).

## OpenAI-compatible endpoints

```python
# Chat
resp = client.chat.completions.create(model="llama3.2:3b", messages=[...])

# Legacy text completion
resp = client.completions.create(model="llama3.2:3b", prompt="Once upon a time")
print(resp.text)

# Embeddings
resp = client.embeddings.create(model="nomic-embed-text", input="hello world")

# List models
for m in client.list_models():
    print(m.id)
```

## Real streaming chat

`/v1/chat/completions` accepts a `stream` field but Ghostlink doesn't act on it yet — real token-by-token streaming is only available today through Ghostlink Studio's native chat endpoint:

```python
for chunk in client.stream_chat("Tell me a short story"):
    if chunk.error:
        print(f"\n[error: {chunk.token}]")
    elif not chunk.done:
        print(chunk.token, end="", flush=True)
```

## Cluster, metrics, settings

```python
client.workers.list()
client.workers.discover()       # UDP broadcast + mDNS sweep
client.workers.connect("192.168.1.42", 8003)

client.metrics()                # JSON snapshot
client.metrics_prometheus()     # Prometheus exposition format

client.settings.get()
client.settings.update(inference_backend="ollama")
```

## Error handling

```python
from ghostlink_client import GhostlinkAPIError, GhostlinkAuthError, GhostlinkConnectionError

try:
    client.chat.completions.create(model="x", messages=[])
except GhostlinkAuthError as err:
    print("bad or missing API key:", err)
except GhostlinkAPIError as err:
    print(f"server rejected the request: HTTP {err.status_code}: {err.message}")
except GhostlinkConnectionError as err:
    print("could not reach the server:", err)
```

## Context manager

```python
with GhostlinkClient("http://127.0.0.1:8003", api_key=key) as client:
    client.health()
```

## Custom inference backends

If the server has a custom backend plugin registered (see `backend_plugin.rs` / `GHOSTLINK_OPENAI_COMPAT_BASE_URL` in the main project), select it the same way as any built-in backend:

```python
client.settings.update(inference_backend="my_custom_backend")
resp = client.chat.completions.create(model="anything", messages=[...])
```

## Development

```bash
pip install -e ".[test]"
pytest
```
