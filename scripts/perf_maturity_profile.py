#!/usr/bin/env python3
"""Build a ranked performance maturity scorecard from flow snapshot summaries."""

from __future__ import annotations

import argparse
import glob
import json
from pathlib import Path


def clamp(value: float, low: float = 0.0, high: float = 1.0) -> float:
    return max(low, min(high, value))


def quantile_linear(sorted_values: list[float], q: float) -> float:
    if not sorted_values:
        return 0.0
    if q <= 0.0:
        return sorted_values[0]
    if q >= 1.0:
        return sorted_values[-1]
    position = (len(sorted_values) - 1) * q
    lower = int(position)
    upper = min(lower + 1, len(sorted_values) - 1)
    frac = position - lower
    return sorted_values[lower] * (1.0 - frac) + sorted_values[upper] * frac


def load_stage_tail(mode: str, stage_glob_template: str | None) -> dict[str, float] | None:
    if not stage_glob_template:
        return None
    pattern = stage_glob_template.format(mode=mode)
    files = sorted(glob.glob(pattern))
    if not files:
        return None

    by_stage_recv: dict[int, list[float]] = {}
    by_stage_bridge: dict[int, list[float]] = {}

    for file_path in files:
        payload = json.loads(Path(file_path).read_text(encoding="utf-8"))
        for stage in payload.get("stage_stats", []):
            stage_idx = int(stage.get("stage_idx", -1))
            if stage_idx < 0:
                continue

            recv_value = stage.get("avg_recv_wait_ms")
            if recv_value is not None:
                by_stage_recv.setdefault(stage_idx, []).append(float(recv_value))

            bridge_value = stage.get("avg_bridge_read_ms")
            if bridge_value is not None:
                by_stage_bridge.setdefault(stage_idx, []).append(float(bridge_value))

    if not by_stage_recv and not by_stage_bridge:
        return None

    recv_stages = sorted(by_stage_recv)
    bridge_stages = sorted(by_stage_bridge)

    recv_first_p95 = 0.0
    recv_last_p95 = 0.0
    if recv_stages:
        recv_first_values = sorted(by_stage_recv[recv_stages[0]])
        recv_last_values = sorted(by_stage_recv[recv_stages[-1]])
        recv_first_p95 = quantile_linear(recv_first_values, 0.95)
        recv_last_p95 = quantile_linear(recv_last_values, 0.95)

    bridge_last_p95 = 0.0
    bridge_max_p95 = 0.0
    if bridge_stages:
        bridge_last_values = sorted(by_stage_bridge[bridge_stages[-1]])
        bridge_last_p95 = quantile_linear(bridge_last_values, 0.95)
        bridge_max_p95 = max(
            quantile_linear(sorted(by_stage_bridge[stage_idx]), 0.95)
            for stage_idx in bridge_stages
        )

    return {
        "files": float(len(files)),
        "recv_first_p95_ms": recv_first_p95,
        "recv_last_p95_ms": recv_last_p95,
        "recv_growth_p95_ms": max(0.0, recv_last_p95 - recv_first_p95),
        "bridge_last_p95_ms": bridge_last_p95,
        "bridge_max_p95_ms": bridge_max_p95,
    }


def classify_attention(score: float) -> str:
    if score >= 70.0:
        return "optimize-now"
    if score >= 40.0:
        return "next-batch"
    return "likely-noise"


def compute_mode_score(
    mode: str,
    summary: dict[str, float],
    baseline: dict[str, float] | None,
    stage_tail: dict[str, float] | None,
) -> dict[str, object]:
    throughput_avg = float(summary["throughput_avg"])
    p10 = float(summary["throughput_p10"])
    p90 = float(summary["throughput_p90"])
    throughput_min = float(summary["throughput_min"])
    throughput_max = float(summary["throughput_max"])
    p95_avg = float(summary["p95_avg"])
    p95_min = float(summary["p95_min"])
    p95_max = float(summary["p95_max"])

    spread_p10_p90 = (p90 - p10) / throughput_avg if throughput_avg > 0.0 else 1.0
    spread_min_max = (throughput_max - throughput_min) / throughput_avg if throughput_avg > 0.0 else 1.0
    p95_rel_range = (p95_max - p95_min) / p95_avg if p95_avg > 0.0 else 1.0

    stability_volatility = (
        0.60 * clamp(spread_p10_p90 / 0.35)
        + 0.40 * clamp(p95_rel_range / 1.50)
    )
    stability_score = round(100.0 * (1.0 - stability_volatility), 1)

    if stage_tail:
        recv_growth = float(stage_tail["recv_growth_p95_ms"])
        recv_last = float(stage_tail["recv_last_p95_ms"])
        bridge_max = float(stage_tail["bridge_max_p95_ms"])
        tail_risk_raw = (
            clamp(recv_growth / 0.080)
            + clamp(recv_last / 0.120)
            + clamp(bridge_max / 0.140)
        ) / 3.0
        tail_driver = (
            f"recv_p95(last={recv_last:.3f}ms, growth={recv_growth:.3f}ms), "
            f"bridge_p95(max={bridge_max:.3f}ms)"
        )
    else:
        tail_risk_raw = clamp(p95_rel_range / 2.0)
        tail_driver = "fallback-to-p95-range (no stage metrics)"
    tail_risk_score = round(100.0 * tail_risk_raw, 1)

    noise_raw = (
        0.50 * clamp(spread_p10_p90 / 0.50)
        + 0.30 * clamp(p95_rel_range / 2.00)
        + 0.20 * clamp(spread_min_max / 2.00)
    )
    noise_index = round(100.0 * noise_raw, 1)

    if baseline:
        baseline_throughput = float(baseline["throughput_avg"])
        baseline_p95 = float(baseline["p95_avg"])
        throughput_ratio = throughput_avg / baseline_throughput if baseline_throughput > 0.0 else 1.0
        p95_ratio = baseline_p95 / p95_avg if p95_avg > 0.0 else 1.0
        baseline_headroom = round(
            clamp(
                0.50 + 0.25 * (throughput_ratio - 1.0) + 0.25 * (p95_ratio - 1.0)
            )
            * 100.0,
            1,
        )
        baseline_source = "baseline"
    else:
        baseline_headroom = 50.0
        baseline_source = "neutral-no-baseline"

    attention_priority = round(
        0.35 * (100.0 - stability_score)
        + 0.30 * tail_risk_score
        + 0.25 * noise_index
        + 0.10 * (100.0 - baseline_headroom),
        1,
    )

    return {
        "mode": mode,
        "stability": stability_score,
        "tail_risk": tail_risk_score,
        "baseline_headroom": baseline_headroom,
        "noise_index": noise_index,
        "attention_priority": attention_priority,
        "classification": classify_attention(attention_priority),
        "drivers": {
            "spread_p10_p90": round(spread_p10_p90, 4),
            "spread_min_max": round(spread_min_max, 4),
            "p95_rel_range": round(p95_rel_range, 4),
            "tail_driver": tail_driver,
            "baseline_source": baseline_source,
        },
    }


def print_markdown(results: list[dict[str, object]]) -> None:
    print("| Rank | Mode | Stability | Tail Risk | Baseline Headroom | Noise Index | Attention Priority | Class |")
    print("|---:|:---|---:|---:|---:|---:|---:|:---|")
    for idx, row in enumerate(results, start=1):
        print(
            "| "
            f"{idx} | {row['mode']} | {row['stability']:.1f} | {row['tail_risk']:.1f} | "
            f"{row['baseline_headroom']:.1f} | {row['noise_index']:.1f} | "
            f"{row['attention_priority']:.1f} | {row['classification']} |"
        )

    print("\nTop drivers:")
    for row in results:
        drivers = row["drivers"]
        print(
            "- "
            f"{row['mode']}: spread(p10..p90)={drivers['spread_p10_p90']:.4f}, "
            f"spread(min..max)={drivers['spread_min_max']:.4f}, "
            f"p95-range/avg={drivers['p95_rel_range']:.4f}, "
            f"tail={drivers['tail_driver']}"
        )


def print_text(results: list[dict[str, object]]) -> None:
    for idx, row in enumerate(results, start=1):
        print(
            f"#{idx} {row['mode']} class={row['classification']} "
            f"attention={row['attention_priority']:.1f} "
            f"stability={row['stability']:.1f} "
            f"tail_risk={row['tail_risk']:.1f} "
            f"headroom={row['baseline_headroom']:.1f} "
            f"noise={row['noise_index']:.1f}"
        )


def main() -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Build a ranked maturity profile from a flow snapshot summary. "
            "Higher attention_priority means higher optimization priority."
        )
    )
    parser.add_argument(
        "--summary",
        required=True,
        help="Path to snapshot summary JSON, e.g. tmp/perf_maturity_det/summary.json",
    )
    parser.add_argument(
        "--baseline",
        default="",
        help="Optional baseline summary JSON (docs/PERF_BASELINE*.json)",
    )
    parser.add_argument(
        "--stage-glob-template",
        default="",
        help=(
            "Optional glob template for run files with {mode}, "
            "e.g. 'tmp/perf_maturity_det/{mode}-*.json'"
        ),
    )
    parser.add_argument(
        "--format",
        choices=["markdown", "text"],
        default="markdown",
    )
    parser.add_argument(
        "--output-json",
        default="",
        help="Optional path to write scorecard JSON artifact",
    )
    args = parser.parse_args()

    summary_path = Path(args.summary)
    summary_data = json.loads(summary_path.read_text(encoding="utf-8"))

    baseline_data: dict[str, dict[str, float]] | None = None
    if args.baseline:
        baseline_payload = json.loads(Path(args.baseline).read_text(encoding="utf-8"))
        if "modes" in baseline_payload and isinstance(baseline_payload["modes"], dict):
            baseline_data = baseline_payload["modes"]
        else:
            baseline_data = baseline_payload

    stage_template = args.stage_glob_template.strip() or None
    if stage_template and "{mode}" not in stage_template:
        parser.error("--stage-glob-template must include '{mode}' placeholder")

    results: list[dict[str, object]] = []
    for mode, mode_summary in summary_data.items():
        baseline_mode = baseline_data.get(mode) if baseline_data else None
        stage_tail = load_stage_tail(mode, stage_template)
        results.append(
            compute_mode_score(
                mode=mode,
                summary=mode_summary,
                baseline=baseline_mode,
                stage_tail=stage_tail,
            )
        )

    results.sort(key=lambda row: (-float(row["attention_priority"]), str(row["mode"])))

    if args.format == "markdown":
        print_markdown(results)
    else:
        print_text(results)

    output = {
        "summary": str(summary_path),
        "baseline": args.baseline or None,
        "stage_glob_template": stage_template,
        "scorecard": results,
    }

    if args.output_json:
        output_path = Path(args.output_json)
        output_path.parent.mkdir(parents=True, exist_ok=True)
        output_path.write_text(json.dumps(output, indent=2), encoding="utf-8")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())