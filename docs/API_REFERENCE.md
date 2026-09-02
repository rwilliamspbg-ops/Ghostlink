# API Reference

Covers the OpenAI-compatible inference routes and the `/api/security/*`
auth routes. For the full endpoint list (including GUI-internal routes like
model management and worker discovery) see the table in
[../README.md#api-endpoints](../README.md#api-endpoints). For a
machine-readable spec of the routes documented here, see
[openapi.yaml](openapi.yaml).

## Authentication & Key Roles

Every route except `/health` requires a bearer token:

```
Authorization: Bearer <token>
```

On first run, the server generates a 256-bit API key, persists it to
`api_key.txt` (or the path in `GHOSTLINK_API_KEY_PATH`), and prints it once
to the console — that initial key carries the `Admin` role.

The server enforces **role-based API key access control** (`crates/ghost-link/src/auth.rs`):
- **`Admin`**: Full system administration. Can manage API keys (`/api/security/keys`), enable PQC TLS, and export audit logs.
- **`Operator`**: Full operational capability. Can load/unload models, submit inference requests (`/v1/chat/completions`), and edit workspace files.
- **`Viewer`**: Read-only inspection. Can view health, metrics, cluster status, audit logs, and exchange valid keys for JWTs.

*Note: Role gating applies to API keys for operator access control; full multi-user / multi-tenant RBAC (user accounts, team namespacing, resource isolation) is not implemented.*

Two kinds of token are accepted:
1. **The raw API key itself** — simplest for a script or `curl`.
2. **A short-lived JWT** (1 hour, HS256) exchanged for a valid key via `POST /api/security/jwt/refresh`.

A request with no token, an expired JWT, or a wrong value gets `401`:

```json
{
  "error": {
    "message": "missing or invalid Authorization: Bearer <token> — see the API key printed at server startup, or POST /api/security/jwt/refresh with it to get a short-lived token",
    "type": "unauthorized"
  }
}
```

### Exchange the API key for a JWT

```bash
curl -X POST http://127.0.0.1:8000/api/security/jwt/refresh \
  -H "Authorization: Bearer <api-key>"
```

```json
{ "status": "ok", "token": "<jwt-token>" }
```

### Check PQC/TLS state

`GET /api/security/pqc/state` reports whether *this running process* is
actually serving HTTPS with the X25519MLKEM768 post-quantum-hybrid key
exchange — not just the persisted setting, since enabling TLS
(`POST /api/security/pqc/enable`) only takes effect on the next restart.

```bash
curl http://127.0.0.1:8000/api/security/pqc/state \
  -H "Authorization: Bearer $GHOSTLINK_API_KEY"
```

```json
{
  "enabled": false,
  "algorithm": "X25519MLKEM768",
  "note": "TLS is not active on this server (plain HTTP) — no key exchange is happening. Enable via POST /api/security/pqc/enable and restart."
}
```

### Key Management (`Admin`-gated)

- `GET /api/security/keys`: List all active API keys (returns ID, name, role, created timestamp, and last 4 chars preview).
- `POST /api/security/keys`: Create a new key (body: `{"name": "ci-bot", "role": "Operator"}`). Returns newly generated raw key once.
- `DELETE /api/security/keys/:id`: Revoke a key by ID (immediately invalidating any outstanding JWTs for it). Refuses to delete the last remaining `Admin` key.

### Audit log

`GET /api/security/audit-log` records real security-relevant events —
failed authentication attempts, JWT refresh, PQC/TLS enable, and tool-call
approve/deny decisions — in-memory, capped at the most recent 500, returned
most-recent-first. Resets on restart (no persistent trail across restarts
yet).

```bash
curl http://127.0.0.1:8000/api/security/audit-log \
  -H "Authorization: Bearer $GHOSTLINK_API_KEY"
```

```json
{
  "entries": [
    { "event": "jwt_refresh", "status": "SUCCESS", "ip": "127.0.0.1", "time": "2026-07-30T00:56:16.043249300+00:00", "detail": null },
    { "event": "auth", "status": "FAILED", "ip": "127.0.0.1", "time": "2026-07-30T00:56:16.000337400+00:00", "detail": "GET /api/models" }
  ]
}
```

## Workspace (Editor tab)

Backs the GUI's Editor tab — file tree browsing, open/save, and repo-aware
chat indexing. Confined to a canonicalized workspace root
(`GHOSTLINK_WORKSPACE_ROOT`, defaults to the launch directory); every route
rejects a `path` that resolves outside it.

### List a directory

```bash
curl "http://127.0.0.1:8000/api/workspace/tree?path=" \
  -H "Authorization: Bearer $GHOSTLINK_API_KEY"
```

```json
{ "path": "", "entries": [{ "name": "README.md", "path": "README.md", "is_dir": false, "size": 23257 }] }
```

### Read / write a file

```bash
curl "http://127.0.0.1:8000/api/workspace/file?path=README.md" \
  -H "Authorization: Bearer $GHOSTLINK_API_KEY"

curl -X PUT http://127.0.0.1:8000/api/workspace/file \
  -H "Authorization: Bearer $GHOSTLINK_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"path": "README.md", "content": "..."}'
```

Files over 5MB, directories, and non-UTF-8 (binary) content are rejected
with a JSON `{"error": "..."}` body rather than a partial read/write.

### Index the workspace for repo-aware chat

```bash
curl -X POST http://127.0.0.1:8000/api/workspace/index \
  -H "Authorization: Bearer $GHOSTLINK_API_KEY"
```

```json
{ "status": "ok", "scanned": 2, "indexed": 2, "failed": 0, "capped": false }
```

Returns `{"status": "skipped", "reason": "..."}` instead — not an error —
when the `rag` MCP server isn't configured or Ollama isn't reachable at the
configured `OLLAMA_URL`.

## `POST /v1/chat/completions`

OpenAI-compatible chat completion. `messages` accepts arbitrary role/content
objects; only the last message's `content` is used as the prompt today (no
multi-turn context is reconstructed server-side for this stateless endpoint
— use the GUI's own session-backed chat for that).

```bash
curl -X POST http://127.0.0.1:8000/v1/chat/completions \
  -H "Authorization: Bearer $GHOSTLINK_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
        "model": "llama-3.2-1b-instruct",
        "messages": [{"role": "user", "content": "Say hi in five words."}],
        "temperature": 0.7,
        "max_tokens": 64
      }'
```

```json
{
  "id": "chatcmpl-...",
  "object": "chat.completion",
  "created": 1732800000,
  "model": "llama-3.2-1b-instruct",
  "choices": [
    {
      "index": 0,
      "message": { "role": "assistant", "content": "Hello! Five words, done." },
      "finish_reason": "stop"
    }
  ]
}
```

## `POST /v1/completions`

Legacy (non-chat) completion: a plain `prompt` string instead of a
`messages` array. Same generation parameters and backend routing as
`/v1/chat/completions`.

```bash
curl -X POST http://127.0.0.1:8000/v1/completions \
  -H "Authorization: Bearer $GHOSTLINK_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
        "model": "llama-3.2-1b-instruct",
        "prompt": "The capital of France is",
        "max_tokens": 16
      }'
```

```json
{
  "id": "cmpl-...",
  "object": "text_completion",
  "created": 1732800000,
  "model": "llama-3.2-1b-instruct",
  "choices": [{ "text": " Paris.", "index": 0, "finish_reason": "stop" }]
}
```

| Field | Type | Default | Notes |
|---|---|---|---|
| `model` | string | current loaded model | Empty string falls back to whatever model is currently active |
| `prompt` | string | required | |
| `temperature` | number | `0.7` | |
| `top_p` | number | `0.9` | |
| `top_k` | integer | `40` | |
| `penalty` | number | `1.1` | Repetition penalty |
| `max_tokens` | integer | `1024` | Clamped to `[16, 4096]` |
| `stream` | boolean | — | Accepted but currently ignored; response is always non-streaming |

## `POST /v1/embeddings`

**Ollama backend only.** The native `llama-server` engine has no embedding
support wired in — requesting embeddings while running the native backend
gets a real `501`, not a faked vector:

```json
{
  "error": {
    "message": "embeddings are only available with the Ollama backend today — the native llama-server engine has no embedding support wired in",
    "type": "not_implemented"
  }
}
```

`input` accepts either a single string or an array of strings, matching the
real OpenAI API:

```bash
curl -X POST http://127.0.0.1:8000/v1/embeddings \
  -H "Authorization: Bearer $GHOSTLINK_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"model": "nomic-embed-text", "input": ["hello world"]}'
```

```json
{
  "object": "list",
  "data": [{ "object": "embedding", "embedding": [0.012, -0.034, ...], "index": 0 }],
  "model": "nomic-embed-text",
  "usage": { "prompt_tokens": 2, "total_tokens": 2 }
}
```

## `GET /v1/models`

```bash
curl http://127.0.0.1:8000/v1/models -H "Authorization: Bearer $GHOSTLINK_API_KEY"
```

```json
{
  "object": "list",
  "data": [{ "id": "llama-3.2-1b-instruct", "object": "model", "created": 1700000000, "owned_by": "ghostlink" }]
}
```

## Client library compatibility

Because these routes mirror OpenAI's request/response shapes, the official
`openai` Python/JS SDKs work by pointing `base_url` at Ghostlink and passing
the API key as the SDK's own bearer token:

```python
from openai import OpenAI

client = OpenAI(base_url="http://127.0.0.1:8000/v1", api_key=GHOSTLINK_API_KEY)
resp = client.chat.completions.create(
    model="llama-3.2-1b-instruct",
    messages=[{"role": "user", "content": "Say hi in five words."}],
)
```
