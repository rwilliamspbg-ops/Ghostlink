#!/usr/bin/env python3
"""Summarize production-gate artifacts into one markdown report."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def write_json(path: Path, payload: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


def parse_doctor_totals(path: Path) -> tuple[int | None, int | None, int | None]:
    if not path.exists():
        return (None, None, None)
    passed = warn = fail = None
    for raw in path.read_text(encoding="utf-8").splitlines():
        stripped = raw.strip()
        if stripped.startswith("- PASS:"):
            try:
                passed = int(stripped.split(":", 1)[1].strip())
            except ValueError:
                pass
        elif stripped.startswith("- WARN:"):
            try:
                warn = int(stripped.split(":", 1)[1].strip())
            except ValueError:
                pass
        elif stripped.startswith("- FAIL:"):
            try:
                fail = int(stripped.split(":", 1)[1].strip())
            except ValueError:
                pass
    return (passed, warn, fail)


def parse_gui_smoke_overall(path: Path) -> str | None:
    if not path.exists():
        return None
    for raw in path.read_text(encoding="utf-8").splitlines():
        stripped = raw.strip().lower()
        if stripped.startswith("- overall:"):
            return stripped.split(":", 1)[1].strip().upper()
    return None


def classify_probe(active_probe: dict) -> str:
    ratio = active_probe.get("failure_ratio")
    failures = active_probe.get("failures")
    try:
        ratio_value = float(ratio)
        failures_value = int(failures)
    except (TypeError, ValueError):
        return "WARN"
    return "PASS" if ratio_value <= 0.0 and failures_value == 0 else "WARN"


def classify_xdp(xdp: dict) -> str:
    has_bpftool = xdp.get("has_bpftool")
    has_ethtool = xdp.get("has_ethtool")
    if has_bpftool is True and has_ethtool is True:
        return "PASS"
    return "WARN"


def classify_perf(payload: dict) -> str:
    missing_modes = [mode for mode in ("tcp", "inmem") if not isinstance(payload.get(mode), dict)]
    return "WARN" if missing_modes else "PASS"


def verdict_line(name: str, status: str, detail: str) -> str:
    return f"- {name}: {status} ({detail})"


def compute_overall_status(statuses: list[str]) -> str:
    if any(status == "FAIL" for status in statuses):
        return "FAIL"
    if any(status == "WARN" for status in statuses):
        return "WARN"
    return "PASS"


def normalize_blocking_status(name: str, status: str) -> str:
    blocking_domains = {"doctor", "gui_smoke", "perf_deterministic", "perf_stress"}
    if status == "FAIL" and name not in blocking_domains:
        return "WARN"
    return status


def load_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def summarize_markdown(path: Path, limit: int = 6) -> tuple[list[str], int]:
    if not path.exists():
        return [], 0
    raw_lines = path.read_text(encoding="utf-8").splitlines()
    bullets: list[str] = []
    for raw in raw_lines:
        stripped = raw.strip()
        if not stripped:
            continue
        if stripped.startswith("#"):
            continue
        if stripped.startswith("Source:"):
            continue
        if stripped.startswith("- "):
            bullets.append(stripped)
            continue
        bullets.append(f"- {stripped}")
    if not bullets:
        return [], len(raw_lines)
    return bullets[:limit], len(raw_lines)


def format_markdown_summary(title: str, path: Path, limit: int = 6) -> list[str]:
    lines = [f"## {title}", ""]
    if not path.exists():
        return lines + [f"- missing summary: `{path}`", ""]
    bullets, source_lines = summarize_markdown(path, limit=limit)
    if not bullets:
        lines.append("- no summary items")
        lines.append("")
        return lines
    lines.extend(bullets)
    if source_lines > len(bullets):
        lines.append(f"- ... ({source_lines - len(bullets)} additional lines omitted)")
    lines.append("")
    return lines


def format_perf_section(title: str, payload: dict) -> list[str]:
    lines = [f"## {title}", ""]
    for mode in ("tcp", "inmem"):
        current = payload.get(mode)
        if not isinstance(current, dict):
            lines.append(f"- {mode}: missing")
            continue
        lines.append(f"### {mode}")
        lines.append("")
        lines.append(f"- runs: {current.get('runs')}")
        lines.append(f"- throughput_avg: {current.get('throughput_avg')}")
        lines.append(f"- throughput_min: {current.get('throughput_min')}")
        lines.append(f"- throughput_max: {current.get('throughput_max')}")
        lines.append(f"- p95_avg: {current.get('p95_avg')}")
        lines.append(f"- wall_avg: {current.get('wall_avg')}")
        lines.append("")
    return lines


def main() -> int:
    parser = argparse.ArgumentParser(description="Summarize production-gate artifacts")
    parser.add_argument("--doctor-summary", help="Path to doctor markdown summary", default="")
    parser.add_argument("--doctor-probe-summary", help="Path to doctor probe markdown summary", default="")
    parser.add_argument("--gui-summary", required=True, help="Path to GUI diagnostics markdown summary")
    parser.add_argument("--gui-smoke-summary", required=True, help="Path to GUI dashboard smoke markdown summary")
    parser.add_argument("--active-probe", required=True, help="Path to active probe JSON")
    parser.add_argument("--xdp-preflight", required=True, help="Path to XDP preflight JSON")
    parser.add_argument("--perf-deterministic", required=True, help="Path to deterministic perf summary JSON")
    parser.add_argument("--perf-stress", required=True, help="Path to stress perf summary JSON")
    parser.add_argument(
        "--fail-on-fail",
        action="store_true",
        help="Exit non-zero when overall gate status is FAIL",
    )
    parser.add_argument(
        "--verdict-json",
        default="",
        help="Optional path to write machine-readable gate verdict JSON",
    )
    parser.add_argument("--output", default="artifacts/production-gate-summary.md")
    args = parser.parse_args()

    active_probe = load_json(Path(args.active_probe))
    xdp = load_json(Path(args.xdp_preflight))
    perf_det = load_json(Path(args.perf_deterministic))
    perf_stress = load_json(Path(args.perf_stress))
    tcp_cfg = perf_det.get("tcp") if isinstance(perf_det.get("tcp"), dict) else {}
    doctor_path = Path(args.doctor_summary) if args.doctor_summary else None
    probe_path = Path(args.doctor_probe_summary) if args.doctor_probe_summary else None
    gui_summary_path = Path(args.gui_summary)
    gui_smoke_path = Path(args.gui_smoke_summary)

    doctor_pass, doctor_warn, doctor_fail = (
        parse_doctor_totals(doctor_path) if doctor_path else (None, None, None)
    )
    doctor_status = "WARN"
    doctor_detail = "missing doctor summary"
    if doctor_pass is not None and doctor_warn is not None and doctor_fail is not None:
        if doctor_fail > 0:
            doctor_status = "FAIL"
        elif doctor_warn > 0:
            doctor_status = "WARN"
        else:
            doctor_status = "PASS"
        doctor_detail = f"pass={doctor_pass}, warn={doctor_warn}, fail={doctor_fail}"

    probe_status = classify_probe(active_probe)
    probe_detail = (
        f"failure_ratio={active_probe.get('failure_ratio')}, failures={active_probe.get('failures')}"
    )

    xdp_status = classify_xdp(xdp)
    xdp_detail = (
        f"has_bpftool={xdp.get('has_bpftool')}, has_ethtool={xdp.get('has_ethtool')}"
    )

    gui_diag_status = "PASS" if gui_summary_path.exists() else "WARN"
    gui_diag_detail = "summary present" if gui_summary_path.exists() else "summary missing"

    gui_smoke_overall = parse_gui_smoke_overall(gui_smoke_path)
    if gui_smoke_overall == "PASS":
        gui_smoke_status = "PASS"
        gui_smoke_detail = "overall=PASS"
    elif gui_smoke_overall == "FAIL":
        gui_smoke_status = "FAIL"
        gui_smoke_detail = "overall=FAIL"
    elif gui_smoke_path.exists():
        gui_smoke_status = "WARN"
        gui_smoke_detail = "overall not found"
    else:
        gui_smoke_status = "WARN"
        gui_smoke_detail = "summary missing"

    perf_det_status = classify_perf(perf_det)
    perf_stress_status = classify_perf(perf_stress)

    verdicts = [
        ("doctor", normalize_blocking_status("doctor", doctor_status), doctor_detail),
        ("doctor_probe", normalize_blocking_status("doctor_probe", probe_status), probe_detail),
        ("gui_diagnostics", normalize_blocking_status("gui_diagnostics", gui_diag_status), gui_diag_detail),
        ("gui_smoke", normalize_blocking_status("gui_smoke", gui_smoke_status), gui_smoke_detail),
        ("xdp_preflight", normalize_blocking_status("xdp_preflight", xdp_status), xdp_detail),
        (
            "perf_deterministic",
            normalize_blocking_status("perf_deterministic", perf_det_status),
            "tcp+inmem present" if perf_det_status == "PASS" else "missing tcp or inmem mode",
        ),
        (
            "perf_stress",
            normalize_blocking_status("perf_stress", perf_stress_status),
            "tcp+inmem present" if perf_stress_status == "PASS" else "missing tcp or inmem mode",
        ),
    ]
    verdict_rows = [verdict_line(name, status, detail) for name, status, detail in verdicts]

    overall_status = compute_overall_status(
        [
            status
            for _, status, _ in verdicts
        ]
    )
    overall_detail = "blocking failure detected" if overall_status == "FAIL" else "no blocking failures"

    lines = [
        "# Production Gate Summary",
        "",
        "## Gate Verdict",
        "",
        verdict_line("overall", overall_status, overall_detail),
        *verdict_rows,
        "",
        "## Operational Snapshot",
        "",
        f"- active_probe_failure_ratio: {active_probe.get('failure_ratio')}",
        f"- active_probe_failures: {active_probe.get('failures')}",
        f"- xdp_kernel_release: {xdp.get('kernel_release')}",
        f"- xdp_has_bpftool: {xdp.get('has_bpftool')}",
        f"- xdp_has_ethtool: {xdp.get('has_ethtool')}",
        f"- tcp_max_inflight_batches: {tcp_cfg.get('tcp_max_inflight_batches')}",
        f"- tcp_reconnect_attempts: {tcp_cfg.get('tcp_reconnect_attempts')}",
        f"- tcp_reconnect_backoff_ms: {tcp_cfg.get('tcp_reconnect_backoff_ms')}",
        "",
    ]

    if args.doctor_summary:
        lines.extend(
            format_markdown_summary("Doctor Summary", Path(args.doctor_summary), limit=8)
        )
    if args.doctor_probe_summary:
        lines.extend(
            format_markdown_summary(
                "Doctor Accessibility Probe Summary",
                Path(args.doctor_probe_summary),
                limit=8,
            )
        )

    lines.extend(
        format_markdown_summary("GUI Diagnostics Summary", Path(args.gui_summary), limit=8)
    )
    lines.extend(
        format_markdown_summary(
            "GUI Dashboard Smoke Summary",
            Path(args.gui_smoke_summary),
            limit=8,
        )
    )
    lines.extend(format_perf_section("Deterministic Performance Summary", perf_det))
    lines.extend(format_perf_section("Stress Performance Summary", perf_stress))

    output_path = Path(args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"Wrote production gate summary to {output_path}")

    verdict_payload = {
        "schema_version": "1",
        "overall": {
            "status": overall_status,
            "detail": overall_detail,
        },
        "domains": {
            "doctor": {"status": normalize_blocking_status("doctor", doctor_status), "detail": doctor_detail},
            "doctor_probe": {"status": normalize_blocking_status("doctor_probe", probe_status), "detail": probe_detail},
            "gui_diagnostics": {"status": normalize_blocking_status("gui_diagnostics", gui_diag_status), "detail": gui_diag_detail},
            "gui_smoke": {"status": normalize_blocking_status("gui_smoke", gui_smoke_status), "detail": gui_smoke_detail},
            "xdp_preflight": {"status": normalize_blocking_status("xdp_preflight", xdp_status), "detail": xdp_detail},
            "perf_deterministic": {
                "status": normalize_blocking_status("perf_deterministic", perf_det_status),
                "detail": "tcp+inmem present" if perf_det_status == "PASS" else "missing tcp or inmem mode",
            },
            "perf_stress": {
                "status": normalize_blocking_status("perf_stress", perf_stress_status),
                "detail": "tcp+inmem present" if perf_stress_status == "PASS" else "missing tcp or inmem mode",
            },
        },
    }

    if args.verdict_json:
        verdict_path = Path(args.verdict_json)
        write_json(verdict_path, verdict_payload)
        print(f"Wrote production gate verdict JSON to {verdict_path}")

    if args.fail_on_fail and overall_status == "FAIL":
        print("Production gate summary indicates FAIL; exiting non-zero due to --fail-on-fail")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
