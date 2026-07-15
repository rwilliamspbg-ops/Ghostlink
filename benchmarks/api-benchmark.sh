#!/usr/bin/env bash
# Ghostlink API Benchmark Suite
# Usage: ./benchmarks/api-benchmark.sh [base_url]
#   base_url defaults to http://localhost:8003

set -euo pipefail

BASE="${1:-http://localhost:8003}"
REQUESTS="${REQUESTS:-20}"
CONCURRENCY="${CONCURRENCY:-4}"
TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

echo "========================================="
echo " Ghostlink API Benchmark"
echo " Target:      $BASE"
echo " Requests:    $REQUESTS"
echo " Concurrency: $CONCURRENCY"
echo "========================================="
echo ""

bench_endpoint() {
    local name="$1"
    local method="$2"
    local path="$3"
    local data="$4"
    local total_s=0
    local ok=0
    local fail=0

    for i in $(seq 1 "$REQUESTS"); do
        local start_ms
        start_ms=$(date +%s%3N)
        if [ "$method" = "GET" ]; then
            code=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 "$BASE$path" 2>/dev/null || echo "000")
        else
            code=$(curl -s -o /dev/null -w "%{http_code}" -X "$method" -H "Content-Type: application/json" -d "$data" --max-time 10 "$BASE$path" 2>/dev/null || echo "000")
        fi
        local end_ms
        end_ms=$(date +%s%3N)
        local elapsed=$(( end_ms - start_ms ))
        total_s=$(( total_s + elapsed ))
        if [ "$code" = "200" ] || [ "$code" = "201" ]; then
            ok=$(( ok + 1 ))
        else
            fail=$(( fail + 1 ))
        fi
    done

    local avg_ms=0
    if [ "$REQUESTS" -gt 0 ]; then
        avg_ms=$(( total_s / REQUESTS ))
    fi

    printf "  %-30s %s/%s ok  avg %4dms\n" "$name" "$ok" "$((ok+fail))" "$avg_ms"
}

echo "--- Health & Status ---"
bench_endpoint "GET /health" GET "/health" ""
bench_endpoint "GET /api/models" GET "/api/models" ""
bench_endpoint "GET /api/metrics" GET "/api/metrics" ""
bench_endpoint "GET /api/sessions" GET "/api/sessions" ""
bench_endpoint "GET /api/settings" GET "/api/settings" ""

echo ""
echo "--- Security Endpoints ---"
bench_endpoint "POST /api/security/jwt/refresh" POST "/api/security/jwt/refresh" '{}'
bench_endpoint "POST /api/security/pqc/enable" POST "/api/security/pqc/enable" '{}'
bench_endpoint "GET /api/security/audit-log" GET "/api/security/audit-log" ""

echo ""
echo "--- Chat & Inference ---"
bench_endpoint "POST /api/inference/chat" POST "/api/inference/chat" '{"message":"Hello, what is Ghostlink?"}'

echo ""
echo "--- Registry ---"
bench_endpoint "GET /api/workers" GET "/api/workers" ""
bench_endpoint "POST /api/workers/add" POST "/api/workers/add" '{"host":"192.168.1.10","port":8081}'
bench_endpoint "POST /api/workers/discover" POST "/api/workers/discover" '{}'

echo ""
echo "========================================="
echo " Benchmark complete."
echo "========================================="
