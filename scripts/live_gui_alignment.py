"""Live alignment check: does the GUI's data source agree with the servers?

The GUI's MCP tab reads `/api/mcp/servers` and renders `connected` + `tool_count`
per server. This starts ghost-link on the real config, waits for the background
`connect_enabled` to finish, and compares what that endpoint reports against what each
server actually advertises over the protocol.

A mismatch is the "aligned" failure the user asked about: the GUI would show a tool
count the server does not serve, or a server as connected when it is not.

Usage:  python tmp/live_gui_alignment.py
"""

import http.client
import json
import os
import ssl
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
GHOSTLINK = os.path.join(ROOT, "target", "release", "ghost-link.exe")
CONFIG = os.path.join(ROOT, "mcp_servers.toml")
LOG = os.path.join(ROOT, "tmp", "alignment.log")
PORT = 8055

CTX = ssl.create_default_context()
CTX.check_hostname = False
CTX.verify_mode = ssl.CERT_NONE

sys.path.insert(0, os.path.join(ROOT, "scripts"))
from mcp_server_probe import parse_config, probe  # noqa: E402


def api_key():
    p = os.path.join(ROOT, "api_key.txt")
    return open(p, encoding="utf-8").read().strip() if os.path.exists(p) else None


def wait_connected(timeout=180):
    """Wait until the background connect task has settled: stop seeing new connects."""
    import re
    ANSI = re.compile(chr(27) + re.escape("[") + "[0-9;]*m")
    pat = re.compile("mcp: connected ([0-9]+) server")
    deadline = time.time() + timeout
    last = (-1, 0.0)
    while time.time() < deadline:
        try:
            txt = ANSI.sub("", open(LOG, encoding="utf-8", errors="replace").read())
            m = pat.search(txt)
            if m:
                n = int(m.group(1))
                if n == last[0]:
                    return n, time.time() - last[1]
                last = (n, time.time())
        except Exception:
            pass
        time.sleep(1)
    return last[0], 0.0


def fetch_servers():
    headers = {}
    k = api_key()
    if k:
        headers["Authorization"] = f"Bearer {k}"
    c = http.client.HTTPSConnection("127.0.0.1", PORT, timeout=20, context=CTX)
    c.request("GET", "/api/mcp/servers", headers=headers)
    r = c.getresponse()
    raw = r.read().decode("utf-8", "replace")
    c.close()
    if r.status != 200:
        return None, f"HTTP {r.status}: {raw[:200]}"
    return json.loads(raw).get("servers", []), None


def main():
    if not os.path.exists(GHOSTLINK):
        print(f"missing {GHOSTLINK} -- run cargo build --release -p ghost-link first")
        return 2

    cfg = parse_config(CONFIG)
    expected_enabled = {c["name"] for c in cfg if c.get("enabled") == "true"}
    print(f"config: {len(cfg)} servers, {len(expected_enabled)} enabled")
    print(f"launching ghost-link on {PORT} ...")

    logf = open(LOG, "wb")
    env = dict(os.environ)
    env["GHOSTLINK_LLAMA_SERVER_URL"] = "http://127.0.0.1:8093"
    p = subprocess.Popen([GHOSTLINK, "serve", str(PORT), "127.0.0.1"],
                         stdout=logf, stderr=subprocess.STDOUT, env=env)
    t0 = time.time()
    try:
        n, _ = wait_connected()
        print(f"connected {n} server(s) in {time.time() - t0:.1f}s")

        servers, err = fetch_servers()
        if err:
            print(f"FAIL /api/mcp/servers: {err}")
            return 1

        print("\n--- what /api/mcp/servers reports (the GUI's data source) ---")
        print(f"{'server':24} {'enabled':8} {'connected':10} {'tool_count':>10}")
        print("-" * 58)
        for s in sorted(servers, key=lambda s: s["name"]):
            print(f"{s['name']:24} {str(s['enabled']):8} {str(s['connected']):10} {s['tool_count']:>10}")

        # ── alignment: endpoint vs the servers themselves ──
        print("\n--- alignment: endpoint vs each live server ---")
        bad = []
        for s in servers:
            if not s["enabled"]:
                continue
            live = probe(next(c for c in cfg if c["name"] == s["name"]))
            if s["connected"] != live["ok"]:
                bad.append(f"{s['name']}: endpoint connected={s['connected']}, "
                           f"server {'answered' if live['ok'] else 'did NOT answer'}")
            elif s["tool_count"] != live["tool_count"]:
                bad.append(f"{s['name']}: endpoint tool_count={s['tool_count']}, "
                           f"server advertises {live['tool_count']}")
            else:
                print(f"  ok   {s['name']:24} {s['tool_count']:>3} tools, connected")

        # a disabled server must not be reported connected
        for s in servers:
            if not s["enabled"] and s["connected"]:
                bad.append(f"{s['name']}: disabled but reported connected")

        print("\n" + "=" * 62)
        if bad:
            print("MISALIGNED:")
            for b in bad:
                print(f"  {b}")
            return 1
        print(f"ALIGNED: all {len(expected_enabled)} enabled servers report the same "
              f"connected state and tool count the servers themselves serve")
        return 0
    finally:
        try:
            p.terminate()
            p.wait(timeout=20)
        except Exception:
            try:
                p.kill()
            except Exception:
                pass
        logf.close()


if __name__ == "__main__":
    sys.exit(main())
