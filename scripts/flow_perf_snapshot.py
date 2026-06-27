#!/usr/bin/env python3
"""Run repeatable flow performance snapshots and print aggregate stats."""

from __future__ import annotations

import argparse
import json
import os
import statistics
import subprocess
from pathlib import Path


def run_once(mode: str, run_index: int, args: argparse.Namespace, output_dir: Path) -> Path:
    out_file = output_dir / f"{mode}-{run_index}.json"
    env = {
        "GHOSTLINK_FLOW_METRICS_JSON": str(out_file),
    }
    if mode == "tcp":
        env["GHOSTLINK_TCP_AUTH_TOKEN"] = args.tcp_auth_token

    command = [
        "cargo",
        "run",
        "-p",
        "ghost-link",
        "--",
        "flow",
        args.local_id,
        args.remote_id,
        str(args.remote_vram_gb),
        str(args.remote_mem_gb),
        str(args.exec_tokens),
        str(args.micro_batch),
        mode,
    ]

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


def summarize(files: list[Path]) -> dict[str, float]:
    values = [json.loads(path.read_text(encoding="utf-8")) for path in files]
    throughput = [float(v["throughput_tokens_per_sec"]) for v in values]
    p95 = [float(v["p95_token_latency_ms"]) for v in values]
    wall = [float(v["total_time_ms"]) for v in values]
    return {
        "runs": len(values),
        "throughput_avg": statistics.mean(throughput),
        "throughput_min": min(throughput),
        "throughput_max": max(throughput),
        "p95_avg": statistics.mean(p95),
        "p95_min": min(p95),
        "p95_max": max(p95),
        "wall_avg": statistics.mean(wall),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Run repeatable flow performance snapshots")
    parser.add_argument("--repo-root", default=".")
    parser.add_argument("--output-dir", default="tmp/perf_snapshot")
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--modes", nargs="+", default=["tcp", "inmem"], choices=["tcp", "inmem"])
    parser.add_argument("--local-id", default="iprada-16gb")
    parser.add_argument("--remote-id", default="zenbook-32gb")
    parser.add_argument("--remote-vram-gb", type=float, default=32.0)
    parser.add_argument("--remote-mem-gb", type=float, default=32.0)
    parser.add_argument("--exec-tokens", type=int, default=256)
    parser.add_argument("--micro-batch", type=int, default=4)
    parser.add_argument("--tcp-auth-token", default="local-token")
    args = parser.parse_args()

    if args.runs <= 0:
        parser.error("--runs must be greater than 0")

    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    all_summary = {}
    for mode in args.modes:
        files: list[Path] = []
        for i in range(1, args.runs + 1):
            files.append(run_once(mode, i, args, output_dir))
        all_summary[mode] = summarize(files)

    for mode, summary in all_summary.items():
        print(
            mode,
            int(summary["runs"]),
            f"throughput_avg={summary['throughput_avg']:.2f}",
            f"throughput_min={summary['throughput_min']:.2f}",
            f"throughput_max={summary['throughput_max']:.2f}",
            f"p95_avg={summary['p95_avg']:.2f}",
            f"p95_min={summary['p95_min']:.2f}",
            f"p95_max={summary['p95_max']:.2f}",
            f"wall_avg={summary['wall_avg']:.2f}",
        )

    (output_dir / "summary.json").write_text(
        json.dumps(all_summary, indent=2), encoding="utf-8"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
