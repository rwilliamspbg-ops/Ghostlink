#!/usr/bin/env python3
"""Validate flow metrics JSON schema contracts for tcp and inmem payloads."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

COMMON_REQUIRED_KEYS = [
    "transport_mode",
    "token_count",
    "micro_batch",
    "batch_count",
    "stage_count",
    "total_time_ms",
    "throughput_tokens_per_sec",
    "avg_token_latency_ms",
    "p95_token_latency_ms",
    "stage_stats",
]

TCP_REQUIRED_KEYS = [
    "tcp_max_inflight_batches",
    "tcp_reconnect_attempts",
    "tcp_reconnect_backoff_ms",
]


def _fail(message: str) -> int:
    print(f"Schema contract failed: {message}")
    return 1


def _load_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def _assert_required(payload: dict, required: list[str], label: str) -> str | None:
    missing = [key for key in required if key not in payload]
    if missing:
        return f"{label} missing keys: {', '.join(missing)}"
    return None


def _assert_numeric(payload: dict, keys: list[str], label: str) -> str | None:
    for key in keys:
        value = payload.get(key)
        try:
            float(value)
        except (TypeError, ValueError):
            return f"{label} has non-numeric value for key '{key}': {value!r}"
    return None


def _validate_stage_stats(payload: dict, label: str) -> str | None:
    stage_stats = payload.get("stage_stats")
    if not isinstance(stage_stats, list):
        return f"{label} stage_stats must be an array"

    try:
        stage_count = int(payload.get("stage_count"))
    except (TypeError, ValueError):
        return f"{label} stage_count must be numeric"

    if len(stage_stats) != stage_count:
        return (
            f"{label} stage_stats length {len(stage_stats)} does not match "
            f"stage_count {stage_count}"
        )

    stage_required = [
        "stage_idx",
        "processed_batches",
        "avg_compute_ms",
        "avg_recv_wait_ms",
        "avg_send_wait_ms",
        "avg_bridge_write_ms",
        "avg_bridge_read_ms",
    ]
    for idx, stage in enumerate(stage_stats):
        if not isinstance(stage, dict):
            return f"{label} stage_stats[{idx}] must be an object"
        missing = [key for key in stage_required if key not in stage]
        if missing:
            return (
                f"{label} stage_stats[{idx}] missing keys: {', '.join(missing)}"
            )
    return None


def validate_payload(path: Path, expected_transport: str, require_tcp_keys: bool) -> int:
    if not path.exists():
        return _fail(f"{expected_transport} metrics file not found: {path}")

    try:
        payload = _load_json(path)
    except json.JSONDecodeError as err:
        return _fail(f"{expected_transport} invalid JSON ({err})")

    if not isinstance(payload, dict):
        return _fail(f"{expected_transport} payload must be an object")

    err = _assert_required(payload, COMMON_REQUIRED_KEYS, expected_transport)
    if err:
        return _fail(err)

    if payload.get("transport_mode") != expected_transport:
        return _fail(
            f"{expected_transport} transport mismatch: {payload.get('transport_mode')!r}"
        )

    numeric_common = [
        "token_count",
        "micro_batch",
        "batch_count",
        "stage_count",
        "total_time_ms",
        "throughput_tokens_per_sec",
        "avg_token_latency_ms",
        "p95_token_latency_ms",
    ]
    err = _assert_numeric(payload, numeric_common, expected_transport)
    if err:
        return _fail(err)

    if require_tcp_keys:
        err = _assert_required(payload, TCP_REQUIRED_KEYS, expected_transport)
        if err:
            return _fail(err)
        err = _assert_numeric(payload, TCP_REQUIRED_KEYS, expected_transport)
        if err:
            return _fail(err)

    err = _validate_stage_stats(payload, expected_transport)
    if err:
        return _fail(err)

    print(f"Schema contract passed: {expected_transport} ({path})")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Validate flow metrics schema contracts for tcp/inmem artifacts"
    )
    parser.add_argument("--tcp-file", required=True, help="Path to tcp metrics JSON")
    parser.add_argument(
        "--inmem-file", required=True, help="Path to inmem metrics JSON"
    )
    args = parser.parse_args()

    tcp_rc = validate_payload(Path(args.tcp_file), "tcp", require_tcp_keys=True)
    if tcp_rc != 0:
        return tcp_rc

    inmem_rc = validate_payload(Path(args.inmem_file), "inmem", require_tcp_keys=False)
    if inmem_rc != 0:
        return inmem_rc

    print("Schema contract passed: tcp + inmem")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
