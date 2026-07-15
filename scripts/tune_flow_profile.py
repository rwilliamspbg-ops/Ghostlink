#!/usr/bin/env python3
"""Deterministically tune flow profile knobs and emit a recommended artifact."""

from __future__ import annotations

import argparse
import json
import math
import os
import subprocess
from pathlib import Path


def run_snapshot(
    mode: str,
    output_dir: Path,
    runs: int,
    warmup_runs: int,
    exec_tokens: int,
    micro_batch: int,
    tcp_max_inflight: int | None,
    release: bool,
    enable_rebalance_feedback: bool,
) -> dict:
    env = os.environ.copy()
    env["GHOSTLINK_FLOW_ENABLE_REBALANCE"] = "1" if enable_rebalance_feedback else "0"
    if tcp_max_inflight is not None:
        env["GHOSTLINK_TCP_MAX_INFLIGHT"] = str(tcp_max_inflight)

    cmd = [
        "python3",
        "scripts/flow_perf_snapshot.py",
        "--modes",
        mode,
        "--runs",
        str(runs),
        "--warmup-runs",
        str(warmup_runs),
        "--exec-tokens",
        str(exec_tokens),
        "--micro-batch",
        str(micro_batch),
        "--output-dir",
        str(output_dir),
    ]
    if release:
        cmd.append("--release")

    subprocess.run(cmd, check=True, env=env)
    summary = json.loads((output_dir / "summary.json").read_text(encoding="utf-8"))
    return summary[mode]


def throughput_spread(summary: dict) -> float:
    p10 = float(summary.get("throughput_p10", 0.0))
    p90 = float(summary.get("throughput_p90", 0.0))
    if p10 <= 0.0:
        return math.inf
    return p90 / p10


def pick_best_tcp(candidates: list[dict], profile: str) -> dict:
    def spread(candidate: dict) -> float:
        s = candidate["summary"]
        p10 = float(s.get("throughput_p10", 0.0))
        p90 = float(s.get("throughput_p90", 0.0))
        if p10 <= 0.0:
            return float("inf")
        return p90 / p10

    if profile == "throughput":
        stable = [
            c
            for c in candidates
            if float(c["summary"]["p95_avg"]) <= 3.0 and spread(c) <= 1.8
        ]
        target = stable if stable else candidates
        return max(target, key=lambda c: c["summary"]["throughput_avg"])

    if profile == "latency":
        max_tp = max(float(c["summary"]["throughput_avg"]) for c in candidates)
        near_peak = [
            c
            for c in candidates
            if float(c["summary"]["throughput_avg"]) >= max_tp * 0.90
        ]
        target = near_peak if near_peak else candidates
        return min(target, key=lambda c: float(c["summary"]["p95_avg"]))

    # balanced
    def score(candidate: dict) -> float:
        s = candidate["summary"]
        tp = float(s["throughput_avg"])
        p95 = max(float(s["p95_avg"]), 1e-6)
        stability = max(spread(candidate), 1.0)
        return tp / (p95 * stability)

    return max(candidates, key=score)


def pick_best_inmem(candidates: list[dict], profile: str) -> dict:
    by_mb = {int(c["micro_batch"]): c for c in candidates}

    if profile == "throughput":
        # Prefer the throughput-oriented batch size when available.
        if 8 in by_mb:
            return by_mb[8]
        return max(candidates, key=lambda c: c["summary"]["throughput_avg"])

    if profile == "latency":
        # Prefer the latency-oriented batch size when available.
        if 4 in by_mb:
            return by_mb[4]
        max_tp = max(float(c["summary"]["throughput_avg"]) for c in candidates)
        near_peak = [
            c
            for c in candidates
            if float(c["summary"]["throughput_avg"]) >= max_tp * 0.70
        ]
        target = near_peak if near_peak else candidates
        return min(target, key=lambda c: float(c["summary"]["p95_avg"]))

    # balanced
    if 8 in by_mb:
        return by_mb[8]

    def score(candidate: dict) -> float:
        s = candidate["summary"]
        tp = float(s["throughput_avg"])
        p95 = max(float(s["p95_avg"]), 1e-6)
        spread = max(throughput_spread(s), 1.0)
        return tp / (p95 * spread)

    return max(candidates, key=score)


def parse_int_list(raw: str) -> list[int]:
    out = []
    for part in raw.split(","):
        part = part.strip()
        if not part:
            continue
        out.append(int(part))
    if not out:
        raise ValueError("expected at least one integer")
    return out


def main() -> int:
    parser = argparse.ArgumentParser(description="Tune flow profile and emit recommendation")
    parser.add_argument("--output", default="docs/FLOW_PERF_TUNING.json")
    parser.add_argument("--workspace", default="tmp/perf_tune_auto")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--warmup-runs", type=int, default=1)
    parser.add_argument("--exec-tokens", type=int, default=512)
    parser.add_argument("--release", action="store_true")
    parser.add_argument("--enable-rebalance-feedback", action="store_true")
    parser.add_argument("--tcp-inflight", default="128,256,512,1024")
    parser.add_argument("--inmem-micro-batch", default="4,8,16")
    args = parser.parse_args()

    tcp_inflight = parse_int_list(args.tcp_inflight)
    inmem_mbs = parse_int_list(args.inmem_micro_batch)

    workspace = Path(args.workspace)
    workspace.mkdir(parents=True, exist_ok=True)

    tcp_candidates = []
    for inflight in tcp_inflight:
        out_dir = workspace / f"tcp_inflight_{inflight}"
        summary = run_snapshot(
            mode="tcp",
            output_dir=out_dir,
            runs=args.runs,
            warmup_runs=args.warmup_runs,
            exec_tokens=args.exec_tokens,
            micro_batch=8,
            tcp_max_inflight=inflight,
            release=args.release,
            enable_rebalance_feedback=args.enable_rebalance_feedback,
        )
        tcp_candidates.append({"tcp_max_inflight": inflight, "summary": summary})

    inmem_candidates = []
    for micro_batch in inmem_mbs:
        out_dir = workspace / f"inmem_mb_{micro_batch}"
        summary = run_snapshot(
            mode="inmem",
            output_dir=out_dir,
            runs=args.runs,
            warmup_runs=args.warmup_runs,
            exec_tokens=args.exec_tokens,
            micro_batch=micro_batch,
            tcp_max_inflight=None,
            release=args.release,
            enable_rebalance_feedback=args.enable_rebalance_feedback,
        )
        inmem_candidates.append({"micro_batch": micro_batch, "summary": summary})

    throughput_tcp = pick_best_tcp(tcp_candidates, "throughput")
    latency_tcp = pick_best_tcp(tcp_candidates, "latency")
    balanced_tcp = pick_best_tcp(tcp_candidates, "balanced")

    throughput_inmem = pick_best_inmem(inmem_candidates, "throughput")
    latency_inmem = pick_best_inmem(inmem_candidates, "latency")
    balanced_inmem = pick_best_inmem(inmem_candidates, "balanced")

    payload = {
        "schema_version": 1,
        "description": "Auto-tuned flow profile recommendations",
        "exec_tokens": int(args.exec_tokens),
        "runs": int(args.runs),
        "warmup_runs": int(args.warmup_runs),
        "rebalance_feedback_enabled": bool(args.enable_rebalance_feedback),
        "profiles": {
            "latency": {
                "micro_batch": int(latency_inmem["micro_batch"]),
                "tcp_max_inflight": int(latency_tcp["tcp_max_inflight"]),
            },
            "balanced": {
                "micro_batch": int(balanced_inmem["micro_batch"]),
                "tcp_max_inflight": int(balanced_tcp["tcp_max_inflight"]),
            },
            "throughput": {
                "micro_batch": int(throughput_inmem["micro_batch"]),
                "tcp_max_inflight": int(throughput_tcp["tcp_max_inflight"]),
            },
        },
        "candidates": {
            "tcp": tcp_candidates,
            "inmem": inmem_candidates,
        },
    }

    output_path = Path(args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(payload, indent=2), encoding="utf-8")

    print(f"Wrote tuning artifact: {output_path}")
    print("Recommended profiles:")
    for name in ("latency", "balanced", "throughput"):
        profile = payload["profiles"][name]
        print(
            f"- {name}: micro_batch={profile['micro_batch']} tcp_max_inflight={profile['tcp_max_inflight']}"
        )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
