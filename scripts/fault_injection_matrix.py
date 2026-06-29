#!/usr/bin/env python3
"""Run a lightweight fault-injection style matrix for flow runtime.

This is designed for CI/dev environments where full LAN fault simulation is not available.
It varies transport/runtime knobs to catch recovery and tail-latency regressions early.
"""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import subprocess
import sys
from dataclasses import dataclass


@dataclass
class Scenario:
    name: str
    transport: str
    extra_env: dict[str, str]


def run_cmd(cmd: list[str], env: dict[str, str]) -> None:
    proc = subprocess.run(cmd, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    if proc.returncode != 0:
        print(proc.stdout)
        raise RuntimeError(f"command failed ({proc.returncode}): {' '.join(cmd)}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run fault-injection matrix for ghost-link flow")
    parser.add_argument("--output-dir", default="./tmp/fault_matrix", help="Directory for metrics outputs")
    parser.add_argument("--exec-tokens", type=int, default=128)
    parser.add_argument("--micro-batch", type=int, default=4)
    parser.add_argument("--strict", action="store_true", help="Fail if p95 exceeds limit")
    parser.add_argument("--max-p95-ms", type=float, default=35.0)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    out_dir = pathlib.Path(args.output_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    scenarios = [
        Scenario("tcp_baseline", "tcp", {"GHOSTLINK_TCP_MAX_INFLIGHT": "256"}),
        Scenario(
            "tcp_constrained_inflight",
            "tcp",
            {
                "GHOSTLINK_TCP_MAX_INFLIGHT": "32",
                "GHOSTLINK_TCP_RECONNECT_ATTEMPTS": "5",
                "GHOSTLINK_TCP_RECONNECT_BACKOFF_MS": "8",
            },
        ),
        Scenario("inmem_baseline", "inmem", {}),
    ]

    summary: dict[str, dict[str, float]] = {}

    for scenario in scenarios:
        metrics_path = out_dir / f"{scenario.name}.json"
        env = os.environ.copy()
        env.update(scenario.extra_env)
        env["GHOSTLINK_FLOW_METRICS_JSON"] = str(metrics_path)
        env.setdefault("GHOSTLINK_TCP_AUTH_TOKEN", "fault-matrix-token")

        cmd = [
            "cargo",
            "run",
            "-p",
            "ghost-link",
            "--",
            "flow",
            "iprada-16gb",
            "zenbook-32gb",
            "32",
            "32",
            str(args.exec_tokens),
            str(args.micro_batch),
            scenario.transport,
        ]

        run_cmd(cmd, env)

        payload = json.loads(metrics_path.read_text(encoding="utf-8"))
        throughput = float(payload.get("throughput_tokens_per_sec", 0.0))
        p95 = float(payload.get("p95_token_latency_ms", 0.0))
        if throughput <= 0:
            raise RuntimeError(f"scenario {scenario.name} produced non-positive throughput")
        if args.strict and p95 > args.max_p95_ms:
            raise RuntimeError(
                f"scenario {scenario.name} p95 too high: {p95:.3f} ms > {args.max_p95_ms:.3f} ms"
            )

        summary[scenario.name] = {
            "throughput_tokens_per_sec": throughput,
            "p95_token_latency_ms": p95,
        }

    (out_dir / "summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(summary, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
