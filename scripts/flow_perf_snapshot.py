#!/usr/bin/env python3
"""Run repeatable flow performance snapshots and print aggregate stats."""

from __future__ import annotations

import argparse
import json
import os
import statistics
import subprocess
from pathlib import Path


def quantile_linear(sorted_values: list[float], q: float) -> float:
    if not sorted_values:
        raise ValueError("quantile requires non-empty values")
    if q <= 0.0:
        return sorted_values[0]
    if q >= 1.0:
        return sorted_values[-1]

    position = (len(sorted_values) - 1) * q
    lower = int(position)
    upper = min(lower + 1, len(sorted_values) - 1)
    frac = position - lower
    return sorted_values[lower] * (1.0 - frac) + sorted_values[upper] * frac


def load_tuning_profile(path: str, profile_mode: str) -> dict[str, int]:
    artifact_path = Path(path)
    if not artifact_path.exists():
        raise FileNotFoundError(f"tuning artifact not found: {artifact_path}")

    payload = json.loads(artifact_path.read_text(encoding="utf-8"))
    profiles = payload.get("profiles")
    if not isinstance(profiles, dict):
        raise ValueError("tuning artifact missing 'profiles' object")

    profile = profiles.get(profile_mode)
    if not isinstance(profile, dict):
        raise ValueError(f"profile '{profile_mode}' missing in tuning artifact")

    micro_batch = int(profile.get("micro_batch", 0))
    tcp_max_inflight = int(profile.get("tcp_max_inflight", 0))
    if micro_batch <= 0:
        raise ValueError(f"invalid micro_batch for profile '{profile_mode}': {micro_batch}")
    if tcp_max_inflight <= 0:
        raise ValueError(
            f"invalid tcp_max_inflight for profile '{profile_mode}': {tcp_max_inflight}"
        )

    return {
        "micro_batch": micro_batch,
        "tcp_max_inflight": tcp_max_inflight,
    }


def run_once(mode: str, run_index: int, args: argparse.Namespace, output_dir: Path) -> Path:
    out_file = output_dir / f"{mode}-{run_index}.json"
    env = {
        "GHOSTLINK_FLOW_METRICS_JSON": str(out_file),
        "GHOSTLINK_FLOW_HIDDEN_DIM": str(args.hidden_dim),
        "GHOSTLINK_FLOW_DTYPE_BYTES": str(args.dtype_bytes),
    }
    if mode == "tcp":
        env["GHOSTLINK_TCP_AUTH_TOKEN"] = args.tcp_auth_token
        if args.applied_tcp_max_inflight is not None:
            env["GHOSTLINK_TCP_MAX_INFLIGHT"] = str(args.applied_tcp_max_inflight)
    elif mode == "xdp":
        env["GHOSTLINK_XDP_INTERFACE"] = args.xdp_interface
        env["GHOSTLINK_TCP_AUTH_TOKEN"] = args.tcp_auth_token

    # Disable dynamic in-memory rebalance feedback by default for stable perf sampling.
    env["GHOSTLINK_FLOW_ENABLE_REBALANCE"] = "1" if args.enable_rebalance_feedback else "0"

    command = ["cargo", "run"]
    if args.release:
        command.append("--release")

    command.extend(
        [
            "-p",
            "ghost-link",
            "--",
            "flow",
            args.local_id,
            args.remote_id,
            str(args.remote_vram_gb),
            str(args.remote_mem_gb),
            str(args.exec_tokens),
            str(args.applied_micro_batch),
            mode,
        ]
    )

    merged_env = dict(os.environ)
    merged_env.update(env)
    subprocess.run(
        command,
        check=True,
        cwd=args.repo_root,
        env=merged_env,
        stdout=subprocess.DEVNULL,
    )
    return out_file


def run_warmup(mode: str, _warmup_index: int, args: argparse.Namespace) -> None:
    env = {
        "GHOSTLINK_FLOW_HIDDEN_DIM": str(args.hidden_dim),
        "GHOSTLINK_FLOW_DTYPE_BYTES": str(args.dtype_bytes),
    }
    if mode == "tcp":
        env["GHOSTLINK_TCP_AUTH_TOKEN"] = args.tcp_auth_token
        if args.applied_tcp_max_inflight is not None:
            env["GHOSTLINK_TCP_MAX_INFLIGHT"] = str(args.applied_tcp_max_inflight)
    elif mode == "xdp":
        env["GHOSTLINK_XDP_INTERFACE"] = args.xdp_interface
        env["GHOSTLINK_TCP_AUTH_TOKEN"] = args.tcp_auth_token

    env["GHOSTLINK_FLOW_ENABLE_REBALANCE"] = "1" if args.enable_rebalance_feedback else "0"

    command = ["cargo", "run"]
    if args.release:
        command.append("--release")

    command.extend(
        [
            "-p",
            "ghost-link",
            "--",
            "flow",
            args.local_id,
            args.remote_id,
            str(args.remote_vram_gb),
            str(args.remote_mem_gb),
            str(args.exec_tokens),
            str(args.applied_micro_batch),
            mode,
        ]
    )

    merged_env = dict(os.environ)
    merged_env.update(env)
    subprocess.run(
        command,
        check=True,
        cwd=args.repo_root,
        env=merged_env,
        stdout=subprocess.DEVNULL,
    )


def summarize(files: list[Path]) -> tuple[dict[str, float], list[float]]:
    values = [json.loads(path.read_text(encoding="utf-8")) for path in files]
    throughput = [float(v["throughput_tokens_per_sec"]) for v in values]
    p95 = [float(v["p95_token_latency_ms"]) for v in values]
    p99 = [float(v.get("p99_token_latency_ms", v["p95_token_latency_ms"])) for v in values]
    wall = [float(v["total_time_ms"]) for v in values]
    bandwidth = [float(v.get("bandwidth_gbps", 0.0)) for v in values]
    sorted_throughput = sorted(throughput)
    throughput_p10 = quantile_linear(sorted_throughput, 0.10)
    throughput_p90 = quantile_linear(sorted_throughput, 0.90)

    # Every sample from every run, in run order — the real per-token
    # latency distribution (not just each run's precomputed percentiles),
    # used for --histogram.
    all_latencies: list[float] = []
    for v in values:
        all_latencies.extend(float(x) for x in v.get("token_latencies_ms", []))

    summary = {
        "runs": len(values),
        "throughput_avg": statistics.mean(throughput),
        "throughput_min": min(throughput),
        "throughput_max": max(throughput),
        "throughput_p10": throughput_p10,
        "throughput_p90": throughput_p90,
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

    first = values[0] if values else {}
    if "transport_mode" in first:
        summary["effective_transport_mode"] = first["transport_mode"]
    for key in (
        "tcp_max_inflight_batches",
        "tcp_reconnect_attempts",
        "tcp_reconnect_backoff_ms",
        "bytes_per_token",
        "simulated_hidden_dim",
        "simulated_dtype_bytes",
    ):
        if key in first:
            summary[key] = first[key]

    return summary, all_latencies


def render_ascii_histogram(values: list[float], buckets: int = 20, width: int = 50) -> str:
    """Bucket `values` into `buckets` equal-width bins and render bar counts."""
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
        bar = "#" * bar_len
        lines.append(f"  {bucket_lo:8.3f} ms | {bar:<{width}} {count}")
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(description="Run repeatable flow performance snapshots")
    parser.add_argument("--repo-root", default=".")
    parser.add_argument("--output-dir", default="tmp/perf_snapshot")
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument(
        "--modes",
        nargs="+",
        default=["tcp", "inmem"],
        choices=["tcp", "inmem", "xdp"],
    )
    parser.add_argument("--local-id", default="iprada-16gb")
    parser.add_argument("--remote-id", default="zenbook-32gb")
    parser.add_argument("--remote-vram-gb", type=float, default=32.0)
    parser.add_argument("--remote-mem-gb", type=float, default=32.0)
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
    parser.add_argument(
        "--histogram",
        action="store_true",
        help="Print an ASCII histogram of the merged per-token latency distribution",
    )
    parser.add_argument("--release", action="store_true", help="Run with --release")
    parser.add_argument(
        "--warmup-runs",
        type=int,
        default=1,
        help="Warmup executions per mode (not included in summary)",
    )
    parser.add_argument("--tcp-auth-token", default="local-token")
    parser.add_argument("--xdp-interface", default="eth0")
    parser.add_argument(
        "--enable-rebalance-feedback",
        action="store_true",
        help="Enable dynamic in-memory runtime feedback/rebalance during snapshots",
    )
    parser.add_argument(
        "--profile-mode",
        choices=["off", "latency", "balanced", "throughput"],
        default="off",
        help="Apply profile-specific micro-batch/TCP inflight from tuning artifact",
    )
    parser.add_argument(
        "--tuning-artifact",
        default="docs/FLOW_PERF_TUNING.json",
        help="Path to tuning artifact consumed when --profile-mode is not 'off'",
    )
    args = parser.parse_args()

    if args.runs <= 0:
        parser.error("--runs must be greater than 0")
    if args.warmup_runs < 0:
        parser.error("--warmup-runs must be 0 or greater")

    args.applied_micro_batch = args.micro_batch
    args.applied_tcp_max_inflight = None
    if args.profile_mode != "off":
        selected = load_tuning_profile(args.tuning_artifact, args.profile_mode)
        args.applied_micro_batch = selected["micro_batch"]
        args.applied_tcp_max_inflight = selected["tcp_max_inflight"]

    print(
        f"profile_mode={args.profile_mode} micro_batch={args.applied_micro_batch} "
        f"tcp_max_inflight={args.applied_tcp_max_inflight if args.applied_tcp_max_inflight is not None else 'env/default'} "
        f"hidden_dim={args.hidden_dim} dtype_bytes={args.dtype_bytes} "
        f"(simulated {args.hidden_dim * args.dtype_bytes} bytes/token)"
    )

    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    all_summary = {}
    all_latencies_by_mode: dict[str, list[float]] = {}
    for mode in args.modes:
        for warmup_idx in range(1, args.warmup_runs + 1):
            run_warmup(mode, warmup_idx, args)
        files: list[Path] = []
        for i in range(1, args.runs + 1):
            files.append(run_once(mode, i, args, output_dir))
        summary, latencies = summarize(files)
        all_summary[mode] = summary
        all_latencies_by_mode[mode] = latencies

    for mode, summary in all_summary.items():
        summary["rebalance_feedback_enabled"] = bool(args.enable_rebalance_feedback)
        summary["profile_mode"] = args.profile_mode
        summary["micro_batch"] = int(args.applied_micro_batch)
        summary["tcp_max_inflight_selected"] = args.applied_tcp_max_inflight
        print(
            mode,
            int(summary["runs"]),
            f"throughput_avg={summary['throughput_avg']:.2f}",
            f"throughput_min={summary['throughput_min']:.2f}",
            f"throughput_max={summary['throughput_max']:.2f}",
            f"p95_avg={summary['p95_avg']:.2f}",
            f"p95_min={summary['p95_min']:.2f}",
            f"p95_max={summary['p95_max']:.2f}",
            f"p99_avg={summary['p99_avg']:.2f}",
            f"p99_min={summary['p99_min']:.2f}",
            f"p99_max={summary['p99_max']:.2f}",
            f"bandwidth_gbps_avg={summary['bandwidth_gbps_avg']:.4f}",
            f"wall_avg={summary['wall_avg']:.2f}",
        )
        if args.histogram:
            print(render_ascii_histogram(all_latencies_by_mode[mode]))
            print()

    (output_dir / "summary.json").write_text(
        json.dumps(all_summary, indent=2), encoding="utf-8"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
