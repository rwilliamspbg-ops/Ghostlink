#!/usr/bin/env bash
# Entrypoint for the docker-compose.rpc-fabric.yml containers.
#
# ghost-link's `load_settings()` (crates/ghost-link/src/main.rs) does
# `serde_json::from_str::<RuntimeSettings>(&data).unwrap_or_default()` — if
# ANY field without a `#[serde(default...)]` attribute is missing from
# settings.json, the whole parse fails and it SILENTLY falls back to
# RuntimeSettings::default() (contribute_compute/distributed_inference both
# false). A partial JSON with only the fields we care about would therefore
# silently do nothing. This script writes every field RuntimeSettings
# declares (mirroring RuntimeSettings::default() in main.rs) so the file
# always deserializes successfully.
set -euo pipefail

NODE_ID="${GHOSTLINK_NODE_ID:?GHOSTLINK_NODE_ID must be set}"
API_HOST="${GHOSTLINK_API_HOST:-0.0.0.0}"
API_PORT="${GHOSTLINK_API_PORT:-8003}"
CONTRIBUTE_COMPUTE="${GHOSTLINK_CONTRIBUTE_COMPUTE:-false}"
DISTRIBUTED_INFERENCE="${GHOSTLINK_DISTRIBUTED_INFERENCE:-false}"
RPC_PORT="${GHOSTLINK_RPC_PORT:-50052}"
MODEL_PATH="${GHOSTLINK_MODEL_PATH:-/app/models/stories15M-q4_0.gguf}"
MODELS_DIR="${GHOSTLINK_MODELS_DIR:-/app/models}"
DISCOVERY_LISTEN="${GHOSTLINK_DISCOVERY_LISTEN:-0.0.0.0:45885}"
DISCOVERY_BROADCAST="${GHOSTLINK_DISCOVERY_BROADCAST:-255.255.255.255:45885}"
SETTINGS_PATH="${GHOSTLINK_SETTINGS_PATH:-/app/settings.json}"
# main.rs forces TLS on whenever the bind host isn't loopback
# (`let use_tls = settings.enable_tls || !tls::is_loopback_host(host);`),
# regardless of `enable_tls` in settings.json. Binding 0.0.0.0 (required so
# the other container / published host port can reach this API) therefore
# always serves HTTPS with a self-signed cert — callers need `curl -k` /
# `verify=False`, not plain http://.
API_KEY_PATH="${GHOSTLINK_API_KEY_PATH:-/app/api_key.txt}"
# auth::load_or_create_api_key() reuses whatever's already at this path
# verbatim (trimmed) instead of generating a random one, so pre-seeding it
# here gives scripts/rpc_fabric_assert.py a fixed, known bearer token
# instead of having to scrape it out of container startup logs.
API_KEY="${GHOSTLINK_API_KEY:-ghostlink-rpc-fabric-test-key-0123456789abcdef0123456789abcdef}"

log() { printf '[entrypoint:%s] %s\n' "$NODE_ID" "$*"; }

cat > "$SETTINGS_PATH" <<JSON
{
  "inference_backend": "native",
  "native_engine": "llama_server",
  "ngl": 0,
  "model_path": "${MODEL_PATH}",
  "models_dir": "${MODELS_DIR}",
  "llama_server_url": "http://127.0.0.1:8080/completion",
  "vllm_base_url": "http://127.0.0.1:8000",
  "vllm_api_key": "",
  "llama_port": 8080,
  "api_host": "${API_HOST}",
  "api_port": ${API_PORT},
  "gui_port": 5173,
  "threads": 4,
  "ctx_size": 4096,
  "parallel_slots": 1,
  "temperature": 0.7,
  "top_p": 0.9,
  "top_k": 40,
  "repeat_penalty": 1.1,
  "max_tokens": 256,
  "chat_exec_tokens": 256,
  "chat_micro_batch": 4,
  "tcp_max_inflight": 256,
  "discovery_listen": "${DISCOVERY_LISTEN}",
  "discovery_broadcast": "${DISCOVERY_BROADCAST}",
  "discovery_auth_token": "",
  "tcp_auth_token": "",
  "xdp_interface": "eth0",
  "conversation_token_limit": 1920,
  "enable_tls": false,
  "distributed_inference": ${DISTRIBUTED_INFERENCE},
  "contribute_compute": ${CONTRIBUTE_COMPUTE},
  "rpc_port": ${RPC_PORT}
}
JSON

log "wrote $SETTINGS_PATH:"
cat "$SETTINGS_PATH"

printf '%s' "$API_KEY" > "$API_KEY_PATH"
log "seeded fixed API key at $API_KEY_PATH (len=${#API_KEY})"

# CPU-only container: force llama-server's -ngl to 0 regardless of any
# VRAM-based auto-detection main.rs might otherwise attempt.
export GHOSTLINK_LLAMA_NGL="${GHOSTLINK_LLAMA_NGL:-0}"
export GHOSTLINK_NODE_ID="$NODE_ID"
export GHOSTLINK_DISCOVERY_LISTEN="$DISCOVERY_LISTEN"
export GHOSTLINK_DISCOVERY_BROADCAST="$DISCOVERY_BROADCAST"
export GHOSTLINK_API_KEY_PATH="$API_KEY_PATH"

log "starting: ghost-link serve $API_HOST $API_PORT"
exec /app/bin/ghost-link serve "$API_HOST" "$API_PORT"
