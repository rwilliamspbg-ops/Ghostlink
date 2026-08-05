#!/usr/bin/env python3
"""Ray actor-to-actor transfer baseline, for comparison against Ghostlink's
own TCP-loopback pipeline benchmark (scripts/flow_perf_snapshot.py).

Fair-comparison note: this measures Ray's own actor-to-actor object-transfer
overhead (task submission + serialization + the object store) — not "Ray
Serve," a full model-serving framework and a different layer entirely, not a
transport benchmark. Both this script and flow_perf_snapshot.py run
single-machine (no real multi-node Ray cluster here either), so this is a
genuine apples-to-apples comparison point for the same synthetic workload:
move token_count tokens, bytes_per_token bytes each, in micro_batch-sized
batches, and measure wall-clock throughput/latency/bandwidth.

Not installed by default — this is a benchmark-only dev tool, not a project
dependency. Install with: pip install ray

Ray does not support Python 3.13+ as of writing. If your default `python` is
newer than that (this repo's dev machine defaults to 3.14), install and run
this under a 3.10-3.12 interpreter instead, e.g. on Windows:
  C:\\path\\to\\Python310\\python.exe -m pip install ray
  C:\\path\\to\\Python310\\python.exe scripts\\ray_transfer_baseline.py
"""

from __future__ import annotations

import argparse
import json
import statistics
import time
from pathlib import Path

import ray


@ray.remote
class ReceiverActor:
    """The "far side" of one transfer — receives a payload and returns its
    length. Does no other work, same posture as Ghostlink's synthetic
    pipeline stages (this is a transport benchmark, not a compute one)."""

    def receive(self, payload: bytes) -> int:
        return len(payload)


def quantile_nearest(sorted_values: list[float], q: float) -> float:
    if not sorted_values:
        return 0.0
    idx = round((len(sorted_values) - 1) * q)
    return sorted_values[idx]


def run_once(token_count: int, micro_batch: int, bytes_per_token: int) -> dict:
    receiver = ReceiverActor.remote()
    micro_batch = max(micro_batch, 1)
    batch_count = (token_count + micro_batch - 1) // micro_batch

    token_latencies_ms: list[float] = []
    exec_start = time.perf_counter()
    for batch_idx in range(batch_count):
        tokens_in_batch = min(micro_batch, token_count - batch_idx * micro_batch)
        payload = bytes(max(tokens_in_batch, 1) * bytes_per_token)

        batch_start = time.perf_counter()
        ray.get(receiver.receive.remote(payload))
        batch_latency_ms = (time.perf_counter() - batch_start) * 1000.0
        token_latencies_ms.extend([batch_latency_ms] * tokens_in_batch)

    total_time_ms = (time.perf_counter() - exec_start) * 1000.0
    throughput = token_count / (total_time_ms / 1000.0) if total_time_ms > 0 else 0.0
    total_bytes = token_count * bytes_per_token
    bandwidth_gbps = (
        (total_bytes / (total_time_ms / 1000.0) / 1e9) if total_time_ms > 0 else 0.0
    )
    sorted_latencies = sorted(token_latencies_ms)

    ray.kill(receiver)

    return {
        "backend": "ray",
        "token_count": token_count,
        "micro_batch": micro_batch,
        "batch_count": batch_count,
        "total_time_ms": total_time_ms,
        "throughput_tokens_per_sec": throughput,
        "avg_token_latency_ms": statistics.mean(token_latencies_ms)
        if token_latencies_ms
        else 0.0,
        "p95_token_latency_ms": quantile_nearest(sorted_latencies, 0.95),
        "p99_token_latency_ms": quantile_nearest(sorted_latencies, 0.99),
        "bytes_per_token": bytes_per_token,
        "bandwidth_gbps": bandwidth_gbps,
        "token_latencies_ms": token_latencies_ms,
    }


def render_ascii_histogram(values: list[float], buckets: int = 20, width: int = 50) -> str:
    if not values:
        return "(no latency samples to histogram)"
    lo, hi = min(values), max(values)
    if lo == hi:
        return f"all {len(values)} samples == {lo:.3f} ms (zero spread, nothing to bucket)"

    bucket_width = (hi - lo) / buckets
    counts = [0] * buckets
    for v in values:
        idx = min(int((v - lo) / bucket_width), buckets - 1)
        counts[idx] += 1

    max_count = max(counts)
    lines = [f"Latency histogram - {len(values)} samples, {lo:.3f}-{hi:.3f} ms"]
    for i, count in enumerate(counts):
        bucket_lo = lo + i * bucket_width
        bar_len = round((count / max_count) * width) if max_count else 0
        lines.append(f"  {bucket_lo:8.3f} ms | {'#' * bar_len:<{width}} {count}")
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Ray actor-to-actor transfer baseline (comparison for flow_perf_snapshot.py)"
    )
    parser.add_argument("--exec-tokens", type=int, default=256)
    parser.add_argument("--micro-batch", type=int, default=4)
    parser.add_argument(
        "--hidden-dim",
        type=int,
        default=4096,
        help="Simulated per-token hidden-state width (4096 = 7B-class model)",
    )
    parser.add_argument(
        "--dtype-bytes",
        type=int,
        default=2,
        help="Simulated per-element byte width (2 = FP16/BF16, 4 = FP32)",
    )
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--output-dir", default="tmp/ray_baseline")
    parser.add_argument(
        "--histogram",
        action="store_true",
        help="Print an ASCII histogram of the merged per-token latency distribution",
    )
    args = parser.parse_args()

    if args.runs <= 0:
        parser.error("--runs must be greater than 0")

    bytes_per_token = args.hidden_dim * args.dtype_bytes
    print(
        f"exec_tokens={args.exec_tokens} micro_batch={args.micro_batch} "
        f"hidden_dim={args.hidden_dim} dtype_bytes={args.dtype_bytes} "
        f"(simulated {bytes_per_token} bytes/token)"
    )

    ray.init(logging_level="ERROR", include_dashboard=False)
    try:
        results = [
            run_once(args.exec_tokens, args.micro_batch, bytes_per_token)
            for _ in range(args.runs)
        ]
    finally:
        ray.shutdown()

    throughput = [r["throughput_tokens_per_sec"] for r in results]
    p95 = [r["p95_token_latency_ms"] for r in results]
    p99 = [r["p99_token_latency_ms"] for r in results]
    bandwidth = [r["bandwidth_gbps"] for r in results]
    wall = [r["total_time_ms"] for r in results]
    all_latencies: list[float] = []
    for r in results:
        all_latencies.extend(r["token_latencies_ms"])

    summary = {
        "backend": "ray",
        "runs": len(results),
        "token_count": args.exec_tokens,
        "micro_batch": args.micro_batch,
        "bytes_per_token": bytes_per_token,
        "simulated_hidden_dim": args.hidden_dim,
        "simulated_dtype_bytes": args.dtype_bytes,
        "throughput_avg": statistics.mean(throughput),
        "throughput_min": min(throughput),
        "throughput_max": max(throughput),
        "p95_avg": statistics.mean(p95),
        "p95_min": min(p95),
        "p95_max": max(p95),
        "p99_avg": statistics.mean(p99),
        "p99_min": min(p99),
        "p99_max": max(p99),
        "bandwidth_gbps_avg": statistics.mean(bandwidth),
        "bandwidth_gbps_min": min(bandwidth),
        "bandwidth_gbps_max": max(bandwidth),
        "wall_avg": statistics.mean(wall),
    }

    print(
        "ray",
        summary["runs"],
        f"throughput_avg={summary['throughput_avg']:.2f}",
        f"throughput_min={summary['throughput_min']:.2f}",
        f"throughput_max={summary['throughput_max']:.2f}",
        f"p95_avg={summary['p95_avg']:.2f}",
        f"p99_avg={summary['p99_avg']:.2f}",
        f"bandwidth_gbps_avg={summary['bandwidth_gbps_avg']:.4f}",
        f"wall_avg={summary['wall_avg']:.2f}",
    )
    if args.histogram:
        print(render_ascii_histogram(all_latencies))

    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)
    (output_dir / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print(f"Ray baseline summary written to: {output_dir / 'summary.json'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
