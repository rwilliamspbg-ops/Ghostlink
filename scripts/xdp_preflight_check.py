#!/usr/bin/env python3
"""Minimal AF_XDP/eBPF preflight signal collection.

Collects host capability hints to support Phase 4 rollout decisions.
"""

from __future__ import annotations

import argparse
import json
import platform
import shutil
import subprocess
import sys
from pathlib import Path


def command_exists(name: str) -> bool:
    return shutil.which(name) is not None


def run_capture(cmd: list[str]) -> str:
    proc = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    return proc.stdout.strip()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Collect XDP/eBPF preflight signals")
    parser.add_argument("--strict", action="store_true", help="Exit non-zero if required capabilities are missing")
    parser.add_argument("--output", default="", help="Optional JSON output path")
    return parser.parse_args()


def main() -> int:
    args = parse_args()

    report = {
        "kernel_release": platform.release(),
        "has_sys_fs_bpf": Path("/sys/fs/bpf").exists(),
        "has_proc_net": Path("/proc/net").exists(),
        "has_ip_cli": command_exists("ip"),
        "has_bpftool": command_exists("bpftool"),
        "has_ethtool": command_exists("ethtool"),
    }

    if report["has_ip_cli"]:
        report["ip_link_sample"] = run_capture(["ip", "-brief", "link"]).splitlines()[:5]

    if args.output:
        Path(args.output).parent.mkdir(parents=True, exist_ok=True)
        Path(args.output).write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")

    print(json.dumps(report, indent=2))

    if args.strict:
        required = [
            report["has_sys_fs_bpf"],
            report["has_ip_cli"],
        ]
        if not all(required):
            return 2

    return 0


if __name__ == "__main__":
    sys.exit(main())
