#!/usr/bin/env python3
"""Run repeatable REAL cross-process `flow --remote-addr` benchmarks.

Unlike flow_perf_snapshot.py (which only ever runs the single-process
simulated path), this drives the genuine remote-worker path added for
multi-node execution: a `ghost-link stage-worker` process actually accepts
a TCP connection and exchanges batches with the coordinator's `flow`
process. Each run's metrics JSON (via GHOSTLINK_FLOW_METRICS_JSON) carries
a real measured `avg_bridge_write_ms` for the remote stage, not a
placeholder constant.

Two ways to use this:

1. Same-host smoke test (--spawn-local-worker): spawns a fresh
   stage-worker on loopback before each run. Useful for validating the
   harness and the transport path itself, but these are loopback numbers —
   NOT a real network benchmark. The summary is labeled accordingly.

2. Real two-machine run (default): point --remote-addr at a second
   machine's address. Because stage-worker is one-shot (it handles exactly
   one coordinator connection, then exits), you must start a fresh one on
   that machine before each run; the script pauses and waits for Enter
   between runs so you can do that by hand — it does not SSH out and start
   it for you.
"""

from __future__ import annotations

import argparse
import json
import os
import statistics
import subprocess
import time
from pathlib import Path

# stage-worker's TcpListener::accept() call handles exactly one connection
# for its whole lifetime, so readiness can't be polled with a real TCP
# connect (that probe connection would itself consume the one slot and
# starve the actual coordinator). A fixed settle delay is the only safe
# option here; 2s comfortably covers `cargo run`'s own build-check overhead
# even when the binary is already built.


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


def build_command(args: argparse.Namespace, subcommand: list[str]) -> list[str]:
    command = ["cargo", "run"]
    if args.release:
        command.append("--release")
    command.extend(["-p", "ghost-link", "--"])
    command.extend(subcommand)
    return command


def run_once(run_index: int, args: argparse.Namespace, output_dir: Path) -> Path:
    worker_proc = None
    if args.spawn_local_worker:
        worker_env = dict(os.environ)
        worker_env["GHOSTLINK_TCP_AUTH_TOKEN"] = args.tcp_auth_token
        worker_proc = subprocess.Popen(
            build_command(args, ["stage-worker", args.remote_addr]),
            cwd=args.repo_root,
            env=worker_env,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        time.sleep(args.worker_settle_seconds)
    else:
        input(
            f"[run {run_index}] Start a fresh `GHOSTLINK_TCP_AUTH_TOKEN={args.tcp_auth_token} "
            f"ghost-link stage-worker {args.remote_addr}` on the remote machine now (the auth "
            "token must match exactly, or the coordinator's connection will be reset), then "
            "press Enter to continue..."
        )

    out_file = output_dir / f"remote-{run_index}.json"
    env = dict(os.environ)
    env["GHOSTLINK_FLOW_METRICS_JSON"] = str(out_file)
    env["GHOSTLINK_TCP_AUTH_TOKEN"] = args.tcp_auth_token

    command = build_command(
        args,
        [
            "flow",
            args.local_id,
            args.remote_id,
            str(args.remote_vram_gb),
            str(args.remote_mem_gb),
            str(args.exec_tokens),
            str(args.micro_batch),
            "tcp",
            "--remote-addr",
            args.remote_addr,
        ],
    )

    result = subprocess.run(
        command,
        cwd=args.repo_root,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )

    if worker_proc is not None:
        worker_proc.wait(timeout=30)

    if result.returncode != 0 or "REAL execution" not in result.stdout:
        raise RuntimeError(
            f"run {run_index} did not take the real remote-execution path "
            f"(exit={result.returncode}):\n{result.stdout}"
        )

    return out_file


def summarize(files: list[Path]) -> dict[str, float]:
    values = [json.loads(path.read_text(encoding="utf-8")) for path in files]
    throughput = [float(v["throughput_tokens_per_sec"]) for v in values]
    p95 = [float(v["p95_token_latency_ms"]) for v in values]
    wall = [float(v["total_time_ms"]) for v in values]

    # Remote-stage round-trip time, straight from the real socket exchange —
    # this is the number that's meaningless on the simulated path (it isn't
    # emitted there at all) and is the whole point of this script.
    remote_bridge_write_ms = []
    for v in values:
        for stage in v.get("stage_stats", []):
            if stage.get("stage_idx") == 1:
                remote_bridge_write_ms.append(float(stage["avg_bridge_write_ms"]))

    sorted_throughput = sorted(throughput)
    summary = {
        "runs": len(values),
        "throughput_avg": statistics.mean(throughput),
        "throughput_min": min(throughput),
        "throughput_max": max(throughput),
        "throughput_p10": quantile_linear(sorted_throughput, 0.10),
        "throughput_p90": quantile_linear(sorted_throughput, 0.90),
        "p95_avg": statistics.mean(p95),
        "wall_avg": statistics.mean(wall),
    }
    if remote_bridge_write_ms:
        summary["remote_bridge_write_ms_avg"] = statistics.mean(remote_bridge_write_ms)
        summary["remote_bridge_write_ms_min"] = min(remote_bridge_write_ms)
        summary["remote_bridge_write_ms_max"] = max(remote_bridge_write_ms)
    return summary


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Run repeatable real cross-process flow --remote-addr benchmarks"
    )
    parser.add_argument("--repo-root", default=".")
    parser.add_argument("--output-dir", default="tmp/remote_flow_benchmark")
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument(
        "--remote-addr",
        required=True,
        help="host:port of the stage-worker to connect to (e.g. 192.168.1.50:9500 "
        "for a real LAN run, or 127.0.0.1:9500 with --spawn-local-worker for a smoke test)",
    )
    parser.add_argument(
        "--spawn-local-worker",
        action="store_true",
        help="Spawn a fresh local stage-worker before each run instead of pausing for a "
        "manually-started remote one. Produces LOOPBACK numbers only, not a real network benchmark.",
    )
    parser.add_argument("--worker-settle-seconds", type=float, default=2.0)
    parser.add_argument("--local-id", default="iprada-16gb")
    parser.add_argument("--remote-id", default="zenbook-32gb")
    parser.add_argument("--remote-vram-gb", type=float, default=32.0)
    parser.add_argument("--remote-mem-gb", type=float, default=32.0)
    parser.add_argument("--exec-tokens", type=int, default=256)
    parser.add_argument("--micro-batch", type=int, default=4)
    parser.add_argument("--release", action="store_true")
    parser.add_argument("--tcp-auth-token", default="local-token")
    args = parser.parse_args()

    if args.runs <= 0:
        parser.error("--runs must be greater than 0")

    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    files = [run_once(i, args, output_dir) for i in range(1, args.runs + 1)]
    summary = summarize(files)
    summary["mode"] = "loopback-smoke-test" if args.spawn_local_worker else "real-remote"

    print(
        summary["mode"],
        int(summary["runs"]),
        f"throughput_avg={summary['throughput_avg']:.2f}",
        f"p95_avg={summary['p95_avg']:.2f}",
        f"remote_bridge_write_ms_avg={summary.get('remote_bridge_write_ms_avg', float('nan')):.4f}",
    )
    if args.spawn_local_worker:
        print(
            "NOTE: --spawn-local-worker was used — this is a same-host loopback smoke test, "
            "not a real network benchmark. Re-run with a genuine --remote-addr on a second "
            "machine (no --spawn-local-worker) for real LAN numbers."
        )

    (output_dir / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
