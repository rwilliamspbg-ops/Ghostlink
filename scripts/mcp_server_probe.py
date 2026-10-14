"""Probe every MCP server in mcp_servers.toml over the real stdio protocol.

Speaks the actual handshake -- `initialize`, `notifications/initialized`, `tools/list` --
rather than just checking that the command exists. A server that spawns and then never
answers is the failure this is built to catch, and it is invisible to a liveness check.

Reports per server: handshake latency, tools/list latency, tool count and names, or the
error (including captured stderr) when it fails.

Usage:
    python scripts/mcp_server_probe.py                # every server in the config
    python scripts/mcp_server_probe.py filesystem rag # only these
    python scripts/mcp_server_probe.py --json out.json
"""

import argparse
import json
import os
import queue
import re
import subprocess
import sys
import threading
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CONFIG = os.path.join(ROOT, "mcp_servers.toml")

PROTOCOL_VERSION = "2024-11-05"
HANDSHAKE_TIMEOUT = 90     # npx/uvx may fetch a package on first run
TOOLS_TIMEOUT = 60


def parse_config(path):
    """Minimal TOML reader for this one file: [[mcp_servers]] blocks + [mcp_servers.env].

    Handles the one shape this file uses -- `args` as a multi-line array -- by tracking
    whether an array is still open, rather than trying to match the arg lines as
    key/value pairs (they have no `=`, so a key/value regex silently skips every one).
    """
    servers = []
    cur = None
    section = None
    args_open = False
    with open(path, encoding="utf-8") as fh:
        for raw in fh:
            line = raw.split("#", 1)[0].rstrip("\n")
            stripped = line.strip()
            if not stripped:
                continue
            if stripped == "[[mcp_servers]]":
                cur = {"env": {}, "args": []}
                servers.append(cur)
                section = "server"
                args_open = False
                continue
            if stripped == "[mcp_servers.env]":
                section = "env"
                args_open = False
                continue
            if cur is None:
                continue
            if args_open:
                cur["args"].extend(re.findall(r'"([^"]*)"', line))
                if "]" in line:
                    args_open = False
                continue
            m = re.match(r"\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)$", line)
            if not m:
                continue
            key, val = m.group(1), m.group(2).strip()
            if section == "env":
                cur["env"][key] = val.strip('"')
                continue
            if key == "args":
                if val.startswith("["):
                    if "]" in val:
                        cur["args"].extend(re.findall(r'"([^"]*)"', val))
                    else:
                        args_open = True
                continue
            cur[key] = val.strip('"')
    return servers


class Server:
    def __init__(self, spec):
        self.spec = spec
        self.name = spec.get("name", "?")
        self.proc = None
        self.q = queue.Queue()
        self.err = []
        self.latency = {}

    def build_cmd(self):
        """Mirror ghost-link's `mcp::client::build_command` exactly.

        On Windows, `CreateProcess` does not resolve `PATHEXT` shims, so spawning `npx`
        bare fails with "program not found" even though `npx` works in a terminal --
        it is a `.cmd` shim. ghost-link routes through `cmd /C` for this reason, and
        the probe must do the same or it reports working servers as broken. The forward
        slash normalisation is also ghost-link's: `cmd /C` treats an unquoted `/` in the
        program-name position as a switch separator.
        """
        cmd = self.spec["command"]
        args = list(self.spec.get("args") or [])
        if os.name == "nt":
            return ["cmd", "/C", cmd.replace("/", "\\")] + args
        return [cmd] + args

    def start(self):
        env = dict(os.environ)
        for k, v in (self.spec.get("env") or {}).items():
            # ${VAR} means "inherit from the environment"; leave it out when unset so the
            # server's own missing-credential error is what surfaces.
            m = re.fullmatch(r"\$\{(\w+)\}", v or "")
            if m:
                if m.group(1) in os.environ:
                    env[k] = os.environ[m.group(1)]
            else:
                env[k] = v
        cmd = self.build_cmd()
        self.cmd = cmd
        self.proc = subprocess.Popen(
            cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE, env=env, cwd=ROOT,
        )
        threading.Thread(target=self._pump, args=(self.proc.stdout,), daemon=True).start()
        threading.Thread(target=self._pump_err, args=(self.proc.stderr,), daemon=True).start()

    def _pump(self, stream):
        for line in iter(stream.readline, b""):
            self.q.put(line)

    def _pump_err(self, stream):
        for line in iter(stream.readline, b""):
            if len(self.err) < 40:
                self.err.append(line.decode("utf-8", "replace").rstrip())

    def send(self, obj):
        data = (json.dumps(obj) + "\n").encode("utf-8")
        self.proc.stdin.write(data)
        self.proc.stdin.flush()

    def await_id(self, want_id, timeout):
        """Read until a JSON-RPC message with this id arrives; ignores notifications."""
        deadline = time.time() + timeout
        while time.time() < deadline:
            try:
                line = self.q.get(timeout=max(0.2, deadline - time.time()))
            except queue.Empty:
                return None
            text = line.decode("utf-8", "replace").strip()
            if not text:
                continue
            try:
                obj = json.loads(text)
            except Exception:
                continue
            # A bare JSON scalar (a quoted string, a number) is valid JSON but not a
            # JSON-RPC message; `--help`-style output produces those. Guard rather than
            # letting `.get` raise and abort the whole probe.
            if not isinstance(obj, dict):
                continue
            if obj.get("id") == want_id:
                return obj
        return None

    def handshake(self):
        t0 = time.time()
        self.send({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": "ghostlink-mcp-probe", "version": "1.0"},
            },
        })
        init = self.await_id(1, HANDSHAKE_TIMEOUT)
        self.latency["initialize_s"] = round(time.time() - t0, 2)
        if init is None:
            return None, "no response to initialize"
        if "error" in init:
            return None, f"initialize error: {init['error']}"

        self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})

        t1 = time.time()
        self.send({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}})
        tools = self.await_id(2, TOOLS_TIMEOUT)
        self.latency["tools_list_s"] = round(time.time() - t1, 2)
        if tools is None:
            return None, "no response to tools/list"
        if "error" in tools:
            return None, f"tools/list error: {tools['error']}"
        return tools.get("result", {}), None

    def stop(self):
        if not self.proc:
            return
        try:
            self.proc.stdin.close()
        except Exception:
            pass
        try:
            self.proc.terminate()
            self.proc.wait(timeout=10)
        except Exception:
            try:
                self.proc.kill()
            except Exception:
                pass


def probe(spec):
    s = Server(spec)
    out = {
        "name": s.name,
        "slot": spec.get("slot"),
        "enabled": spec.get("enabled") == "true",
        "transport": spec.get("transport"),
        "command": " ".join([spec.get("command", "")] + list(spec.get("args") or [])),
        "ok": False,
        "tools": [],
        "tool_count": 0,
        "error": None,
        "latency": {},
        "stderr_tail": [],
    }
    try:
        s.start()
        result, err = s.handshake()
        out["latency"] = s.latency
        if err:
            out["error"] = err
        else:
            server_info = (result or {}).get("serverInfo") or {}
            out["server_info"] = server_info
            tools = (result or {}).get("tools") or []
            out["tools"] = [t.get("name") for t in tools]
            out["tool_count"] = len(tools)
            out["ok"] = True
    except FileNotFoundError as e:
        out["error"] = f"command not found: {e}"
    except Exception as e:
        out["error"] = f"{type(e).__name__}: {e}"
    finally:
        out["stderr_tail"] = s.err[-6:]
        s.stop()
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("names", nargs="*", help="only these server names")
    ap.add_argument("--json", help="write full results here")
    ap.add_argument("--all", action="store_true",
                    help="include disabled servers too (default: every server in the file)")
    args = ap.parse_args()

    if not os.path.exists(CONFIG):
        print(f"missing {CONFIG}")
        return 2

    servers = parse_config(CONFIG)
    if args.names:
        wanted = set(args.names)
        servers = [s for s in servers if s.get("name") in wanted]
        missing = wanted - {s.get("name") for s in servers}
        if missing:
            print(f"not in config: {', '.join(sorted(missing))}")

    print(f"probing {len(servers)} MCP server(s) from {os.path.relpath(CONFIG, ROOT)}")
    print("=" * 96)
    results = []
    for spec in servers:
        label = spec.get("name", "?")
        state = "enabled" if spec.get("enabled") == "true" else "disabled"
        print(f"\n[{label}]  slot={spec.get('slot')}  {state}")
        print(f"  cmd: {spec.get('command')} {' '.join(spec.get('args') or [])}")
        t0 = time.time()
        r = probe(spec)
        r["wall_s"] = round(time.time() - t0, 2)
        results.append(r)
        if r["ok"]:
            lat = r["latency"]
            print(f"  OK   {r['tool_count']} tool(s)   "
                  f"initialize={lat.get('initialize_s')}s  tools/list={lat.get('tools_list_s')}s  "
                  f"wall={r['wall_s']}s")
            for t in r["tools"][:12]:
                print(f"         - {t}")
            if r["tool_count"] > 12:
                print(f"         ... +{r['tool_count'] - 12} more")
        else:
            print(f"  FAIL {r['error']}   wall={r['wall_s']}s")
            for line in r["stderr_tail"][-4:]:
                print(f"         | {line[:150]}")

    print("\n" + "=" * 96)
    ok = [r for r in results if r["ok"]]
    bad = [r for r in results if not r["ok"]]
    print(f"{len(ok)}/{len(results)} answered the handshake")
    if bad:
        print("\nfailed:")
        for r in bad:
            print(f"  {r['name']:24} {r['error']}")
    slow = sorted((r for r in ok), key=lambda r: -(r["latency"].get("initialize_s") or 0))[:5]
    if slow:
        print("\nslowest handshakes:")
        for r in slow:
            print(f"  {r['name']:24} {r['latency'].get('initialize_s')}s")

    if args.json:
        with open(args.json, "w", encoding="utf-8") as fh:
            json.dump(results, fh, indent=2)
        print(f"\nwrote {args.json}")
    return 0 if not bad else 1


if __name__ == "__main__":
    sys.exit(main())
