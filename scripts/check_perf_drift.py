#!/usr/bin/env python3
"""Compare flow perf snapshot summary against a committed baseline."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


# Allow a small comparison margin for noisy CI measurements at threshold boundaries.
RATIO_COMPARISON_EPSILON = 0.01


def fail(msg: str) -> int:
    print(f"Perf drift check failed: {msg}")
    return 1


def resolve_threshold(
    cli_value: float | None,
    mode_policy: dict,
    global_policy: dict,
    key: str,
    fallback: float,
) -> float:
    """Resolve threshold with precedence: CLI override > mode policy > global policy > fallback."""
    if cli_value is not None:
        return float(cli_value)
    if key in mode_policy:
        return float(mode_policy[key])
    if key in global_policy:
        return float(global_policy[key])
    return fallback


def extract_metric(payload: dict, key: str, mode: str, label: str) -> float:
    if key not in payload:
        raise KeyError(f"{label} missing {key} for mode {mode}")
    return float(payload[key])


def main() -> int:
    parser = argparse.ArgumentParser(description="Check perf drift against baseline")
    parser.add_argument("--baseline", required=True)
    parser.add_argument("--current", required=True)
    parser.add_argument(
        "--max-throughput-drop-ratio",
        type=float,
        default=None,
        help="Allowed relative throughput drop ratio (0.30 = 30%%)",
    )
    parser.add_argument(
        "--max-p95-rise-ratio",
        type=float,
        default=None,
        help="Allowed relative p95 rise ratio (0.60 = 60%%)",
    )
    args = parser.parse_args()

    baseline_path = Path(args.baseline)
    current_path = Path(args.current)

    if not baseline_path.exists():
        return fail(f"baseline file missing: {baseline_path}")
    if not current_path.exists():
        return fail(f"current summary file missing: {current_path}")

    try:
        baseline_payload = json.loads(baseline_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as err:
        return fail(f"invalid baseline JSON ({err})")

    try:
        current_payload = json.loads(current_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as err:
        return fail(f"invalid current summary JSON ({err})")

    global_policy = baseline_payload.get("drift_policy", {})
    baseline_modes = baseline_payload.get("modes", {})
    if not isinstance(baseline_modes, dict):
        return fail("baseline modes must be an object")

    current_modes = current_payload.get("modes", current_payload)
    if not isinstance(current_modes, dict):
        return fail("current summary modes must be an object")

    for mode in ("tcp", "inmem"):
        if mode not in baseline_modes:
            return fail(f"baseline missing mode: {mode}")
        if mode not in current_modes:
            return fail(f"current summary missing mode: {mode}")

        base = baseline_modes[mode]
        curr = current_modes[mode]
        mode_policy = base.get("drift_policy", {})

        max_throughput_drop_ratio = resolve_threshold(
            args.max_throughput_drop_ratio,
            mode_policy,
            global_policy,
            "max_throughput_drop_ratio",
            0.30,
        )
        max_p95_rise_ratio = resolve_threshold(
            args.max_p95_rise_ratio,
            mode_policy,
            global_policy,
            "max_p95_rise_ratio",
            0.60,
        )

        if max_throughput_drop_ratio < 0:
            return fail(f"invalid throughput drop ratio for {mode}: {max_throughput_drop_ratio}")
        if max_p95_rise_ratio < 0:
            return fail(f"invalid p95 rise ratio for {mode}: {max_p95_rise_ratio}")

        try:
            base_throughput = extract_metric(base, "throughput_avg", mode, "baseline")
            curr_throughput = extract_metric(curr, "throughput_avg", mode, "current")
            base_p95 = extract_metric(base, "p95_avg", mode, "baseline")
            curr_p95 = extract_metric(curr, "p95_avg", mode, "current")
        except (KeyError, TypeError, ValueError) as err:
            return fail(str(err))

        if base_throughput <= 0:
            return fail(f"invalid baseline throughput for {mode}: {base_throughput}")

        throughput_drop = (base_throughput - curr_throughput) / base_throughput
        p95_rise = (curr_p95 - base_p95) / base_p95 if base_p95 > 0 else 0.0

        if throughput_drop > (max_throughput_drop_ratio + RATIO_COMPARISON_EPSILON):
            return fail(
                f"{mode} throughput dropped {throughput_drop:.2%} "
                f"(baseline={base_throughput:.2f}, current={curr_throughput:.2f})"
            )

        if p95_rise > (max_p95_rise_ratio + RATIO_COMPARISON_EPSILON):
            return fail(
                f"{mode} p95 rose {p95_rise:.2%} "
                f"(baseline={base_p95:.2f}, current={curr_p95:.2f})"
            )

        print(
            f"Drift check passed for {mode}: "
            f"throughput {curr_throughput:.2f} vs baseline {base_throughput:.2f}, "
            f"p95 {curr_p95:.2f} vs baseline {base_p95:.2f}, "
            f"thresholds(drop={max_throughput_drop_ratio:.2%}, rise={max_p95_rise_ratio:.2%})"
        )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
