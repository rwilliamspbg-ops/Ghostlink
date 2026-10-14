"""Measure what the 12-server default actually costs at startup.

The template now ships all 12 servers enabled. That number has to come from a
measurement, not a preference, so this measures it the way a user experiences it:
launch ghost-link, wait for `connect_enabled` to settle, read the summary line
ghost-link itself logs (`connected N server(s), M tool(s) in Tms`).

Reports three runs so the number is a median, not a fluke:

  1. all 12 servers            (the new template default)
  2. 11 without docker-gateway (when Docker Desktop is not running)
  3. the previous 6-server set (the baseline this is being compared against)

The comparison that matters is #3 -> #1: what did enabling everything cost.

Usage:  python tmp/measure_mcp_startup_budget.py
"""

import json
import os
import re
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
GHOSTLINK = os.path.join(ROOT, "target", "release", "ghost-link.exe")
CONFIG = os.path.join(ROOT, "mcp_servers.toml")
LOGDIR = os.path.join(ROOT, "tmp")

sys.path.insert(0, os.path.join(ROOT, "scripts"))
from mcp_server_probe import parse_config  # noqa: E402

ANSI = re.compile(chr(27) + re.escape("[") + "[0-9;]*m")
SUMMARY = re.compile("mcp: connected ([0-9]+) server\\(s\\), ([0-9]+) tool\\(s\\) in ([0-9]+)ms")


def api_key():
    p = os.path.join(ROOT, "api_key.txt")
    return open(p, encoding="utf-8").read().strip() if os.path.exists(p) else None


def run_once(label, servers, timeout=300):
    """Launch ghost-link with `servers` enabled, wait for the summary line."""
    import http.client
    import ssl

    port = 8056 + (int(time.time()) % 40)
    cfg_path = os.path.join(LOGDIR, "startup_budget.toml")
    write_cfg(cfg_path, servers)

    log = os.path.join(LOGDIR, f"startup_{int(time.time())}.log")
    lf = open(log, "wb")
    env = dict(os.environ)
    env["GHOSTLINK_LLAMA_SERVER_URL"] = "http://127.0.0.1:8093"
    env["GHOSTLINK_MCP_CONFIG"] = cfg_path
    t0 = time.time()
    p = subprocess.Popen([GHOSTLINK, "serve", str(port), "127.0.0.1"],
                         stdout=lf, stderr=subprocess.STDOUT, env=env)
    try:
        deadline = time.time() + timeout
        seen = None
        while time.time() < deadline:
            try:
                txt = ANSI.sub("", open(log, encoding="utf-8", errors="replace").read())
                m = SUMMARY.search(txt)
                if m:
                    seen = (int(m.group(1)), int(m.group(2)), int(m.group(3)))
                    break
            except Exception:
                pass
            time.sleep(0.5)
        wall = time.time() - t0
        if seen is None:
            return None
        return {"servers": seen[0], "tools": seen[1], "connect_ms": seen[2],
                "wall_s": round(wall, 2)}
    finally:
        try:
            p.terminate()
            p.wait(timeout=20)
        except Exception:
            try:
                p.kill()
            except Exception:
                pass
        lf.close()


def write_cfg(path, names):
    """Write a config enabling only `names`, from the real template."""
    all_srv = parse_config(CONFIG)
    lines = []
    for s in all_srv:
        if s["name"] not in names:
            continue
        lines.append("[[mcp_servers]]")
        lines.append(f'name = "{s["name"]}"')
        lines.append(f'enabled = true')
        lines.append(f'slot = "{s.get("slot", "misc")}"')
        lines.append(f'transport = "{s.get("transport", "stdio")}"')
        lines.append(f'command = "{s.get("command", "")}"')
        args = s.get("args") or []
        if len(args) == 1:
            lines.append(f'args = ["{args[0]}"]')
        else:
            lines.append("args = [")
            for a in args:
                lines.append(f'    "{a}",')
            lines.append("]")
        lines.append("requires_confirmation = false")
        lines.append("timeout_secs = 60")
        lines.append("")
        if s.get("env"):
            lines.append("[mcp_servers.env]")
            for k, v in s["env"].items():
                lines.append(f'{k} = "{v}"')
            lines.append("")
    open(path, "w", encoding="utf-8").write("\n".join(lines))


def main():
    if not os.path.exists(GHOSTLINK):
        print(f"missing {GHOSTLINK} -- cargo build --release -p ghost-link first")
        return 2

    all_srv = parse_config(CONFIG)
    names = [s["name"] for s in all_srv]
    docker = [n for n in names if n.startswith("docker")]
    nodocker = [n for n in names if not n.startswith("docker")]
    prev6 = ["filesystem", "duckduckgo", "sequential-thinking", "rag", "memory", "playwright"]

    print(f"template has {len(names)} servers: {sorted(names)}")
    print(f"measuring 3 configurations x 3 runs each\n")

    results = {}
    for label, setnames in [
        ("all 12 (new default)", names),
        (f"11 without docker gateway", nodocker),
        ("6 (previous default)", prev6),
    ]:
        runs = []
        for i in range(3):
            r = run_once(label, setnames)
            if r is None:
                print(f"  {label:26} run {i+1}: NO SUMMARY LINE (timeout)")
                continue
            runs.append(r)
            print(f"  {label:26} run {i+1}: {r['servers']} servers, "
                  f"{r['tools']:>3} tools, connect={r['connect_ms']}ms, "
                  f"wall={r['wall_s']}s")
            time.sleep(3)
        results[label] = runs

    print("\n" + "=" * 74)
    print(f"{'config':28} {'runs':>4} {'median connect':>14} {'tools':>6}")
    print("-" * 74)
    med = {}
    for label, runs in results.items():
        if not runs:
            continue
        ms = sorted(r["connect_ms"] for r in runs)
        median = ms[len(ms) // 2]
        med[label] = median
        print(f"{label:28} {len(runs):>4} {median:>12}ms {runs[0]['tools']:>6}")

    if "all 12 (new default)" in med and "6 (previous default)" in med:
        a, b = med["all 12 (new default)"], med["6 (previous default)"]
        print(f"\nenabling everything cost {a - b:+d}ms vs the previous 6 "
              f"({b}ms -> {a}ms), for "
              f"{sum(r['tools'] for r in results['all 12 (new default)'][:1])} "
              f"tools vs "
              f"{sum(r['tools'] for r in results['6 (previous default)'][:1])}")

    # is docker actually running?
    try:
        r = subprocess.run(["docker", "info", "--format", "{{.ServerVersion}}"],
                           capture_output=True, timeout=20, text=True)
        print(f"\ndocker desktop: {'RUNNING ' + r.stdout.strip() if r.returncode == 0 else 'NOT running'}")
    except Exception as e:
        print(f"\ndocker desktop: probe failed ({e})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
