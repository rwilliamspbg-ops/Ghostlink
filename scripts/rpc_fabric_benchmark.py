#!/usr/bin/env python3
"""Real single-node vs. multi-node benchmark for Ghostlink's ggml-rpc
distributed-inference path (docker-compose.rpc-fabric-benchmark.yml).

Unlike scripts/remote_flow_benchmark.py (which drives the synthetic
`flow`/`stage-worker` pipeline harness — fake f32 payloads timed through a
TCP bridge, no real model math) and scripts/rpc_fabric_assert.py (a pass/
fail CI correctness gate, not a benchmark), this script:

  1. Runs N repeated timed chat completions through the REAL distributed
     path — `distributed_inference: true` on the coordinator, a real
     `llama-server --rpc <contributor>:50052 -ts <split>` process, real
     cross-container RPC tensor execution (see crates/ghost-link/src/
     rpc_cluster.rs) — verified per-phase the same way
     scripts/rpc_fabric_assert.py does: `real_inference: true` in the
     response, plus fresh "Accepted client connection" lines in the
     contributor's ggml-rpc-server log (GHOSTLINK_RPC_SERVER_LOG).
  2. Also runs the same N trials in single-node mode
     (`distributed_inference: false`, coordinator alone) against the exact
     same model and prompt, so the numbers show what distribution actually
     costs or gains here — not one number in isolation.
  3. Records real per-container memory (cgroup memory.current, not `docker
     stats`' cache-inclusive summary) right after each phase's model load,
     to answer the "does a tensor-split reduce the coordinator's own local
     memory footprint" question empirically instead of by assumption.

Uses the OpenAI-compatible-but-metrics-bearing `/api/inference/chat` route
(the GUI's chat endpoint — NOT `/v1/chat/completions`, which returns no
token/timing data at all), which reports `tokens_generated`,
`tokens_estimated` (a word-count prompt-token estimate — see
`handle_gui_chat` in crates/ghost-link/src/main.rs, `token_estimate =
req.message.split_whitespace().count()...` — not an exact tokenizer count),
and `metrics.throughput` (tokens_out / measured llama-server generation
wall time, computed server-side).

Output shape mirrors scripts/remote_flow_benchmark.py: raw per-run JSON
plus a summary.json with avg/min/max, for each of the two modes.
"""

from __future__ import annotations

import argparse
import json
import statistics
import subprocess
import sys
import time
from pathlib import Path

import requests
import urllib3

urllib3.disable_warnings(urllib3.exceptions.InsecureRequestWarning)

DEFAULT_PROMPT = (
    "Explain in one paragraph why distributed systems need consensus "
    "protocols, and give one real-world example."
)


def log(msg: str) -> None:
    print(f"[bench] {msg}", flush=True)


def wait_http(url: str, label: str, attempts: int = 90, delay: float = 2.0):
    last_err = None
    for i in range(1, attempts + 1):
        try:
            resp = requests.get(url, timeout=3, verify=False)
            if resp.ok:
                log(f"{label} is ready ({url}) after {i} attempt(s)")
                return resp
        except requests.RequestException as err:
            last_err = err
        time.sleep(delay)
    raise SystemExit(f"FAIL: {label} never became ready at {url}: {last_err}")


def docker_exec(container: str, *cmd: str) -> str:
    result = subprocess.run(
        ["docker", "exec", container, *cmd],
        capture_output=True,
        text=True,
    )
    return (result.stdout + result.stderr).strip()


def read_cgroup_memory_bytes(container: str) -> dict:
    """Real resident memory (cgroup, not docker stats' derived/cached
    number) for `container`. Tries cgroup v2 first, falls back to v1.
    Returns current usage plus (when available from memory.stat) the
    anon-vs-file breakdown, which is the load-bearing distinction for the
    mmap question this script investigates: file-backed pages from an
    mmap'd GGUF are reclaimable cache, not the same as real committed
    anonymous memory.
    """
    current_raw = docker_exec(container, "cat", "/sys/fs/cgroup/memory.current")
    if current_raw and current_raw.strip().isdigit():
        stat_raw = docker_exec(container, "cat", "/sys/fs/cgroup/memory.stat")
        stat = {}
        for line in stat_raw.splitlines():
            parts = line.split()
            if len(parts) == 2 and parts[1].isdigit():
                stat[parts[0]] = int(parts[1])
        return {
            "cgroup_version": 2,
            "memory_current_bytes": int(current_raw.strip()),
            "anon_bytes": stat.get("anon"),
            "file_bytes": stat.get("file"),
        }

    # cgroup v1 fallback
    usage_raw = docker_exec(container, "cat", "/sys/fs/cgroup/memory/memory.usage_in_bytes")
    if usage_raw and usage_raw.strip().isdigit():
        stat_raw = docker_exec(container, "cat", "/sys/fs/cgroup/memory/memory.stat")
        stat = {}
        for line in stat_raw.splitlines():
            parts = line.split()
            if len(parts) == 2 and parts[1].isdigit():
                stat[parts[0]] = int(parts[1])
        return {
            "cgroup_version": 1,
            "memory_current_bytes": int(usage_raw.strip()),
            "anon_bytes": stat.get("total_rss"),
            "file_bytes": stat.get("total_cache"),
        }

    return {"cgroup_version": None, "memory_current_bytes": None, "anon_bytes": None, "file_bytes": None}


def count_rpc_log_lines(container: str, log_path: str, marker: str = "Accepted client connection") -> int:
    raw = docker_exec(container, "cat", log_path)
    return sum(1 for line in raw.splitlines() if marker in line)


def patch_distributed_inference(base: str, headers: dict, enabled: bool) -> dict:
    r = requests.post(
        f"{base}/api/settings",
        json={"distributed_inference": enabled},
        headers=headers,
        timeout=10,
        verify=False,
    )
    r.raise_for_status()
    settings = requests.get(f"{base}/api/settings", headers=headers, timeout=10, verify=False).json()
    if bool(settings.get("distributed_inference")) != enabled:
        raise SystemExit(f"FAIL: distributed_inference did not stick to {enabled}: {settings}")
    return settings


def load_model(base: str, headers: dict, model: str, timeout: int = 180) -> dict:
    r = requests.post(
        f"{base}/api/models/load",
        json={"model": model},
        headers=headers,
        timeout=timeout,
        verify=False,
    )
    r.raise_for_status()
    result = r.json()
    if "error" in result:
        raise SystemExit(f"FAIL: model load reported an error: {result}")
    return result


def run_chat(base: str, headers: dict, prompt: str, max_tokens: int, timeout: int = 180) -> dict:
    wall_start = time.time()
    r = requests.post(
        f"{base}/api/inference/chat",
        json={"message": prompt, "max_tokens": max_tokens},
        headers=headers,
        timeout=timeout,
        verify=False,
    )
    wall_ms = (time.time() - wall_start) * 1000.0
    r.raise_for_status()
    result = r.json()
    result["_measured_wall_ms"] = wall_ms
    return result


def run_phase(
    label: str,
    base: str,
    headers: dict,
    model: str,
    prompt: str,
    max_tokens: int,
    runs: int,
    output_dir: Path,
    coordinator_container: str,
    contributor_container: str,
    rpc_log_path: str,
    expect_distributed: bool,
) -> dict:
    log(f"=== Phase: {label} (distributed_inference={expect_distributed}) ===")
    patch_distributed_inference(base, headers, expect_distributed)

    connections_before = count_rpc_log_lines(contributor_container, rpc_log_path)
    load_started = time.time()
    load_result = load_model(base, headers, model)
    load_ms = (time.time() - load_started) * 1000.0
    log(f"model load took {load_ms:.0f} ms: {load_result}")

    # Settle briefly after llama-server reports ready before sampling
    # memory — RSS/page-cache after a fresh exec/mmap can still be settling
    # for a few hundred ms.
    time.sleep(2.0)
    post_load_memory = {
        "coordinator": read_cgroup_memory_bytes(coordinator_container),
        "contributor": read_cgroup_memory_bytes(contributor_container),
    }
    log(f"post-load memory: {post_load_memory}")

    run_files = []
    peak_memory = {"coordinator": 0, "contributor": 0}

    def sample_peak():
        for name, container in (("coordinator", coordinator_container), ("contributor", contributor_container)):
            mem = read_cgroup_memory_bytes(container)
            val = mem.get("memory_current_bytes") or 0
            if val > peak_memory[name]:
                peak_memory[name] = val

    sample_peak()

    for i in range(1, runs + 1):
        result = run_chat(base, headers, prompt, max_tokens)
        sample_peak()

        real_inference = result.get("real_inference")
        metrics = result.get("metrics", {})
        run_record = {
            "run": i,
            "phase": label,
            "real_inference": real_inference,
            "tokens_estimated_prompt": result.get("tokens_estimated"),
            "tokens_generated": result.get("tokens_generated"),
            "exec_tokens": result.get("exec_tokens"),
            "throughput_tokens_per_sec": metrics.get("throughput"),
            "latency_ms": metrics.get("latency_ms"),
            "p50_ms": metrics.get("p50_ms"),
            "p95_ms": metrics.get("p95_ms"),
            "measured_wall_ms": result.get("_measured_wall_ms"),
        }
        log(
            f"  run {i}/{runs}: tokens={run_record['tokens_generated']} "
            f"throughput={run_record['throughput_tokens_per_sec']} tok/s "
            f"latency={run_record['latency_ms']} ms real_inference={real_inference}"
        )
        if expect_distributed and not real_inference:
            raise SystemExit(f"FAIL: distributed phase run {i} did not report real_inference=true: {result}")

        out_file = output_dir / f"{label}-{i}.json"
        out_file.write_text(json.dumps(run_record, indent=2), encoding="utf-8")
        run_files.append(out_file)

    connections_after = count_rpc_log_lines(contributor_container, rpc_log_path)
    new_connections = connections_after - connections_before
    log(f"contributor rpc log: {connections_before} -> {connections_after} accepted connections (+{new_connections})")
    if expect_distributed and new_connections <= 0:
        raise SystemExit(
            "FAIL: distributed phase completed but the contributor's ggml-rpc-server log shows "
            "no new accepted connections — real_inference:true alone is not being trusted here."
        )

    values = [json.loads(f.read_text(encoding="utf-8")) for f in run_files]
    throughput = [v["throughput_tokens_per_sec"] for v in values if v["throughput_tokens_per_sec"] is not None]
    latency = [v["latency_ms"] for v in values if v["latency_ms"] is not None]
    wall = [v["measured_wall_ms"] for v in values if v["measured_wall_ms"] is not None]
    tokens_gen = [v["tokens_generated"] for v in values if v["tokens_generated"] is not None]

    summary = {
        "phase": label,
        "distributed_inference": expect_distributed,
        "runs": len(values),
        "new_rpc_connections_observed": new_connections,
        "load_ms": load_ms,
        "post_load_memory_bytes": post_load_memory,
        "peak_memory_bytes_during_runs": peak_memory,
        "tokens_generated_avg": statistics.mean(tokens_gen) if tokens_gen else None,
    }
    if throughput:
        summary["throughput_tok_s_avg"] = statistics.mean(throughput)
        summary["throughput_tok_s_min"] = min(throughput)
        summary["throughput_tok_s_max"] = max(throughput)
        summary["throughput_tok_s_stdev"] = statistics.pstdev(throughput) if len(throughput) > 1 else 0.0
    if latency:
        summary["latency_ms_avg"] = statistics.mean(latency)
        summary["latency_ms_min"] = min(latency)
        summary["latency_ms_max"] = max(latency)
    if wall:
        summary["measured_wall_ms_avg"] = statistics.mean(wall)

    return summary


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--coordinator-url", default="https://127.0.0.1:8020")
    parser.add_argument("--api-key", default="ghostlink-rpc-fabric-bench-test-key-0123456789abcdef0123456789ab")
    parser.add_argument("--model", default="qwen2.5-1.5b-instruct-q4_k_m")
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--max-tokens", type=int, default=128)
    parser.add_argument("--prompt", default=DEFAULT_PROMPT)
    parser.add_argument("--output-dir", default="tmp/rpc_fabric_benchmark")
    parser.add_argument("--coordinator-container", default="ghostlink-rpc-bench-coordinator")
    parser.add_argument("--contributor-container", default="ghostlink-rpc-bench-contributor")
    parser.add_argument("--rpc-log-path", default="/tmp/ggml-rpc-server.log")
    parser.add_argument(
        "--skip-single-node",
        action="store_true",
        help="Only run the distributed phase (e.g. for a quick real_inference smoke check).",
    )
    args = parser.parse_args()

    if args.runs <= 0:
        parser.error("--runs must be greater than 0")

    base = args.coordinator_url.rstrip("/")
    headers = {"Authorization": f"Bearer {args.api_key}"}
    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    log("waiting for coordinator health")
    wait_http(f"{base}/health", "rpc-bench-coordinator API")
    contributor_health = docker_exec(args.contributor_container, "curl", "-fsSk", "https://localhost:8003/health")
    if not contributor_health.strip():
        raise SystemExit("FAIL: rpc-bench-contributor /health returned nothing")
    log(f"rpc-bench-contributor /health: {contributor_health.strip()}")

    log("confirming UDP discovery found the peer")
    discover_resp = None
    for attempt in range(1, 11):
        r = requests.get(f"{base}/api/workers/discover", headers=headers, timeout=10, verify=False)
        r.raise_for_status()
        discover_resp = r.json()
        if discover_resp.get("count", 0) >= 2:
            break
        time.sleep(3)
    else:
        raise SystemExit(f"FAIL: discovery never found the contributor peer. Last response: {discover_resp}")
    log(f"discovery OK: {discover_resp}")

    phases = {}
    if not args.skip_single_node:
        phases["single-node"] = run_phase(
            "single-node",
            base,
            headers,
            args.model,
            args.prompt,
            args.max_tokens,
            args.runs,
            output_dir,
            args.coordinator_container,
            args.contributor_container,
            args.rpc_log_path,
            expect_distributed=False,
        )

    phases["distributed"] = run_phase(
        "distributed",
        base,
        headers,
        args.model,
        args.prompt,
        args.max_tokens,
        args.runs,
        output_dir,
        args.coordinator_container,
        args.contributor_container,
        args.rpc_log_path,
        expect_distributed=True,
    )

    summary = {
        "model": args.model,
        "runs_per_phase": args.runs,
        "max_tokens": args.max_tokens,
        "prompt": args.prompt,
        "phases": phases,
    }

    print("\n=== SUMMARY ===")
    for name, phase in phases.items():
        print(
            f"{name}: throughput_avg={phase.get('throughput_tok_s_avg', float('nan')):.2f} tok/s "
            f"(min={phase.get('throughput_tok_s_min', float('nan')):.2f} "
            f"max={phase.get('throughput_tok_s_max', float('nan')):.2f}) "
            f"latency_avg={phase.get('latency_ms_avg', float('nan')):.1f} ms "
            f"coordinator_post_load_mem={phase['post_load_memory_bytes']['coordinator'].get('memory_current_bytes')} bytes"
        )

    (output_dir / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    log(f"wrote {output_dir / 'summary.json'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
