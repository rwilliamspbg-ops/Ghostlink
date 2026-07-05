#!/usr/bin/env bash
set -euo pipefail

HOST="127.0.0.1"
PORTS=(8001 8003 9999 10000)

# Only terminate processes we can confidently attribute to Ghostlink local dev stack.
GHOSTLINK_PATTERNS=(
  "ghost-link"
  "real_llm_proxy.py"
  "model_manager.py"
  "launch_studio.py"
)

matches_ghostlink() {
  local cmd="$1"
  for pattern in "${GHOSTLINK_PATTERNS[@]}"; do
    if [[ "$cmd" == *"$pattern"* ]]; then
      return 0
    fi
  done
  return 1
}

listener_pids_for_port() {
  local port="$1"
  if command -v lsof >/dev/null 2>&1; then
    lsof -nP -iTCP:"$port" -sTCP:LISTEN -t 2>/dev/null || true
  else
    # ss output format contains pid=NNN entries.
    ss -ltnp "( sport = :$port )" 2>/dev/null | sed -n 's/.*pid=\([0-9]\+\).*/\1/p' || true
  fi
}

if [[ "${1:-}" == "--help" ]]; then
  cat <<'EOF'
Usage: scripts/cleanup_ghostlink_ports.sh

Safely stops Ghostlink local dev listeners on:
  8001 (model manager)
  8003 (backend)
  9999 (gateway default)
  10000 (gateway fallback)

Only processes with command lines matching known Ghostlink patterns are terminated.
EOF
  exit 0
fi

echo "[cleanup] scanning Ghostlink ports on ${HOST}: ${PORTS[*]}"

killed_any=0
for port in "${PORTS[@]}"; do
  mapfile -t pids < <(listener_pids_for_port "$port")
  if [[ "${#pids[@]}" -eq 0 ]]; then
    continue
  fi

  for pid in "${pids[@]}"; do
    [[ -z "$pid" ]] && continue
    if ! ps -p "$pid" >/dev/null 2>&1; then
      continue
    fi

    cmd="$(ps -p "$pid" -o args= 2>/dev/null || true)"
    if ! matches_ghostlink "$cmd"; then
      echo "[cleanup] skip pid=${pid} port=${port} (not Ghostlink): ${cmd}"
      continue
    fi

    echo "[cleanup] stopping pid=${pid} port=${port}: ${cmd}"
    kill "$pid" 2>/dev/null || true
    killed_any=1
  done
done

if [[ "$killed_any" -eq 0 ]]; then
  echo "[cleanup] no Ghostlink listeners needed cleanup"
  exit 0
fi

# Give graceful shutdown a moment.
for _ in 1 2 3 4 5; do
  sleep 0.2
  remaining=0
  for port in "${PORTS[@]}"; do
    mapfile -t pids < <(listener_pids_for_port "$port")
    for pid in "${pids[@]}"; do
      [[ -z "$pid" ]] && continue
      if ps -p "$pid" >/dev/null 2>&1; then
        cmd="$(ps -p "$pid" -o args= 2>/dev/null || true)"
        if matches_ghostlink "$cmd"; then
          remaining=1
        fi
      fi
    done
  done
  [[ "$remaining" -eq 0 ]] && break
done

# Force-kill remaining Ghostlink listeners on target ports.
for port in "${PORTS[@]}"; do
  mapfile -t pids < <(listener_pids_for_port "$port")
  for pid in "${pids[@]}"; do
    [[ -z "$pid" ]] && continue
    if ! ps -p "$pid" >/dev/null 2>&1; then
      continue
    fi
    cmd="$(ps -p "$pid" -o args= 2>/dev/null || true)"
    if matches_ghostlink "$cmd"; then
      echo "[cleanup] force stop pid=${pid} port=${port}: ${cmd}"
      kill -9 "$pid" 2>/dev/null || true
    fi
  done
done

echo "[cleanup] complete"
