"""Drive the browser server for real, with argument shapes read from the server.

The earlier drive probe asserted output formats it had never seen and passed
arguments the tools do not accept. Both failures were the probe's. The shapes below
come from the server's own inputSchema and its observed output:

  browser_click            REQUIRES `target` (the exact ref from the snapshot, or a
                           unique selector). `element` is only the human-readable
                           description for the permission prompt -- it locates
                           nothing. Passing only `element` produces
                           "Invalid input: expected string, received undefined -> at target".
  browser_network_requests REQUIRES `static` (boolean). Without it the static
                           document request is filtered out and the list looks empty.
  browser_find             `text` or `regex`, neither required; output is
                           "Found N match for ..." with [ref=eN] elements.

Observed output formats the assertions are written against:
  snapshot     '- heading "Ghostlink drive probe" [level=1] [ref=e2]'
  find         'Found 1 match for "Ghostlink drive probe":'
  console      'Total messages: 2 (Errors: 0, Warnings: 0)'
  network      '1. [GET] http://127.0.0.1:PORT/probe => [200] OK'
  evaluate     '42' plus the code that produced it
  click        returns the Playwright code that ran, e.g. getByTestId('go').click()

Usage:  python tmp/browser_drive_probe.py
"""

import http.server
import json
import os
import re
import socketserver
import sys
import threading
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))
from mcp_server_probe import Server, parse_config  # noqa: E402

PAGE = """<!doctype html>
<html><head><meta charset="utf-8"><title>Ghostlink drive probe</title></head>
<body>
  <h1>Ghostlink drive probe</h1>
  <p id="status" data-testid="status">ready</p>
  <button id="go" data-testid="go">activate</button>
  <a id="fetch-link" data-testid="fetch-link" href="/data.json">load</a>
  <script>
    console.log("probe: page loaded");
    document.getElementById("go").addEventListener("click", function () {
      document.getElementById("status").textContent = "activated";
      console.log("probe: button clicked");
    });
    document.getElementById("fetch-link").addEventListener("click", function (e) {
      e.preventDefault();
      fetch("/data.json").then(r => r.json()).then(d => {
        document.getElementById("status").textContent = "fetched " + d.value;
      });
    });
  </script>
</body></html>
"""
DATA = json.dumps({"value": 42}).encode()


class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        body = PAGE.encode() if self.path in ("/", "/probe") else DATA
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *a):
        pass


def call(server, tool, args, timeout=90):
    """Returns (text, error, seconds)."""
    t0 = time.time()
    server.send({"jsonrpc": "2.0", "id": 900, "method": "tools/call",
                 "params": {"name": tool, "arguments": args}})
    r = server.await_id(900, timeout)
    dt = time.time() - t0
    if r is None:
        return None, f"NO RESPONSE after {dt:.1f}s", dt
    if "error" in r:
        return None, f"error: {json.dumps(r['error'])[:300]}", dt
    res = (r or {}).get("result", {})
    if res.get("isError"):
        c = res.get("content") or []
        txt = " ".join(x.get("text", "") for x in c if isinstance(x, dict))
        return None, f"isError: {txt[:300]}", dt
    texts = [c.get("text", "") for c in (res.get("content") or [])
             if isinstance(c, dict) and c.get("type") == "text"]
    return "\n".join(t for t in texts if t), None, dt


def ref_for(snapshot, needle):
    for line in (snapshot or "").splitlines():
        if needle in line.lower():
            m = re.search(r"\[ref=(e\d+)\]", line)
            if m:
                return m.group(1)
    return None


def main():
    cfg = parse_config(os.path.join(ROOT, "mcp_servers.toml"))
    spec = next(c for c in cfg if c["name"] == "playwright")
    s = Server(spec)

    httpd = socketserver.TCPServer(("127.0.0.1", 0), Handler)
    url = f"http://127.0.0.1:{httpd.server_address[1]}/probe"
    threading.Thread(target=httpd.serve_forever, daemon=True).start()

    checks = []

    def ok(label, cond, detail=""):
        checks.append((label, bool(cond)))
        print(f"  {'ok  ' if cond else 'FAIL'}  {label}"
              + (f"   -- {detail}" if detail else ""))

    try:
        s.start()
        t0 = time.time()
        _, err = s.handshake()
        if err:
            print(f"handshake failed: {err}")
            for line in s.err[-12:]:
                print(f"    {line}")
            return 1
        print(f"playwright handshake ok in {time.time() - t0:.2f}s\n")

        # -- READ: observation tools --------------------------------------------
        print("READ tools (must return content and change nothing):")

        out, err, _ = call(s, "browser_navigate", {"url": url})
        ok("browser_navigate loads the page",
           err is None and out is not None and "Ghostlink drive probe" in (out or ""),
           (err or "")[:120])

        snap1, err, _ = call(s, "browser_snapshot", {})
        ok("browser_snapshot returns the a11y tree with refs",
           err is None and "[ref=" in (snap1 or ""), (err or "")[:120])

        out, err, _ = call(s, "browser_find", {"text": "Ghostlink drive probe"})
        ok("browser_find locates real text",
           err is None and "Found 1 match" in (out or ""), (err or "")[:120])

        out, err, _ = call(s, "browser_console_messages", {})
        ok("browser_console_messages captured the load log",
           err is None and "probe: page loaded" in (out or ""), (err or "")[:120])

        out, err, _ = call(s, "browser_network_requests", {"static": True})
        ok("browser_network_requests records the document request",
           err is None and "[GET]" in (out or "") and "=> [200]" in (out or ""),
           (err or "")[:120])

        snap2, err, _ = call(s, "browser_snapshot", {})
        ok("reads left the page unchanged",
           err is None and snap2 == snap1, (err or "")[:120])

        # -- WRITE: interaction changes observable state ------------------------
        print("\nWRITE tools (must change observable state, which is why they're gated):")

        go_ref = ref_for(snap1, "activate")
        out, err, _ = call(s, "browser_click",
                           {"element": "the activate button",
                            "target": go_ref or "#go"})
        ok("browser_click succeeds with a ref target",
           err is None and "getByTestId('go').click()" in (out or ""),
           (err or "")[:160])

        snap3, err, _ = call(s, "browser_snapshot", {})
        ok("browser_click changed the page state",
           err is None and snap3 != snap1 and "activated" in (snap3 or ""),
           (err or "status did not become 'activated'")[:160])

        out, err, _ = call(s, "browser_console_messages", {})
        ok("browser_click emitted the handler's console log",
           err is None and "probe: button clicked" in (out or ""), (err or "")[:120])

        # -- EXEC: code execution and data egress -------------------------------
        print("\nEXEC tools (run code / move data -- must not be grantable):")

        out, err, _ = call(s, "browser_evaluate", {"function": "() => 6 * 7"})
        ok("browser_evaluate runs arbitrary JS in the page",
           err is None and "42" in (out or ""), (err or "")[:120])

        # evaluate a fetch, then confirm it is visible to the network view --
        # this is the exfiltration half of the Exec classification
        out, err, _ = call(s, "browser_evaluate",
                           {"function": "() => fetch('/data.json').then(r => r.text())"})
        ok("browser_evaluate can issue its own network requests",
           err is None and "42" in (out or ""), (err or "")[:120])
        time.sleep(1)

        out, err, _ = call(s, "browser_network_requests", {"static": True})
        ok("browser_evaluate's request is visible to the network view",
           err is None and "data.json" in (out or ""), (err or "")[:120])

        # -- what the classification now rests on -------------------------------
        print("\n" + "=" * 70)
        print("what driving the browser established about the classification:")
        print("  Read   returned content and left the snapshot byte-identical")
        print("  Write  changed the snapshot, so gating it is not theatre")
        print("  Exec   ran code AND issued its own network requests -- exactly")
        print("         the two capabilities a browser must not have unapproved")
        print("=" * 70)

        failed = [l for l, c in checks if not c]
        print(f"\n{len(checks) - len(failed)}/{len(checks)} checks passed")
        if failed:
            print("failed:")
            for f in failed:
                print(f"  {f}")
            return 1
        return 0
    finally:
        try:
            s.stop()
        except Exception:
            pass
        httpd.shutdown()
        httpd.server_close()


if __name__ == "__main__":
    sys.exit(main())
