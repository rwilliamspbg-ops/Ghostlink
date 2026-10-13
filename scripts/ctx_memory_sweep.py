"""Memory headroom per context size -- the axis a context cap actually guards.

`get_ctx_size`'s size-tier caps are documented as protecting against an OOM on the
host that also runs the UI, but the evidence behind them was a throughput sweep.
Throughput is the wrong axis: context costs KV cache, so the question is how much
RAM a given `-c` actually consumes and how much is left.

Measures, per context size: the KV cache llama.cpp reports, total RAM consumed by
the process, and free RAM after the model is resident. Runs production-shaped args
(`-np 2` as `settings.json` sets, `-fa on`, `q8_0` KV) so the answer is about the
configuration Ghostlink actually launches.

Usage: python tmp/ctx_memory_sweep.py <model.gguf> <ctx,ctx,...> [ngl] [slots]
"""

import ctypes
import json
import os
import re
import socket
import subprocess
import sys
import time
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
# Forward slashes: a backslash path here turned "\build" into a backspace
# character and Popen failed with WinError 2.
SERVER = os.path.join(ROOT, "third_party/llama.cpp/build/bin/Release/llama-server.exe")
MODELS = os.path.join(ROOT, "models")
SCRATCH = os.path.join(os.environ.get("TEMP", ROOT), "ghostlink-ctx-memory")


class MSX(ctypes.Structure):
    _fields_ = [
        ("dwLength", ctypes.c_ulong), ("dwMemoryLoad", ctypes.c_ulong),
        ("ullTotalPhys", ctypes.c_ulonglong), ("ullAvailPhys", ctypes.c_ulonglong),
        ("ullTotalPageFile", ctypes.c_ulonglong), ("ullAvailPageFile", ctypes.c_ulonglong),
        ("ullTotalVirtual", ctypes.c_ulonglong), ("ullAvailVirtual", ctypes.c_ulonglong),
        ("ullAvailExtendedVirtual", ctypes.c_ulonglong),
    ]


def free_gb():
    st = MSX()
    st.dwLength = ctypes.sizeof(MSX)
    ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(st))
    return st.ullAvailPhys / 1e9


def total_gb():
    st = MSX()
    st.dwLength = ctypes.sizeof(MSX)
    ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(st))
    return st.ullTotalPhys / 1e9


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def policy_ctx(model_gb):
    """Mirror get_ctx_size's model_cap (the VRAM tier is a separate min())."""
    if model_gb >= 10.0:
        return 4096
    if model_gb >= 5.0:
        return 8192
    return None


def policy_ngl(model_gb):
    return 0 if model_gb >= 10.0 else -1


KV_RE = re.compile(r"llama_kv_cache: size =\s*([\d.]+) MiB \(\s*(\d+) cells")


def trial(path, gb, ctx, ngl, slots, margin_gb):
    base_free = free_gb()
    port = free_port()
    stamp = time.strftime("%H%M%S")
    os.makedirs(SCRATCH, exist_ok=True)
    log_path = os.path.join(SCRATCH, f"ctxmem_{os.path.basename(path)[:12]}_{ctx}_{stamp}.log")
    log = open(log_path, "w", encoding="utf-8", errors="replace")
    cmd = [
        SERVER, "-m", path, "--alias", "ctxmem",
        "--host", "127.0.0.1", "--port", str(port),
        "--no-warmup", "-t", "8", "-c", str(ctx), "-np", str(slots),
        "-ngl", str(ngl), "-fa", "on", "-b", "512", "-ub", "128",
        "-ctk", "q8_0", "-ctv", "q8_0", "-lv", "5",
    ]
    proc = subprocess.Popen(cmd, stdout=log, stderr=subprocess.STDOUT)
    ready, why = False, ""
    deadline = time.time() + 600
    while time.time() < deadline:
        if proc.poll() is not None:
            why = f"exited rc={proc.returncode}"
            break
        try:
            with urllib.request.urlopen(f"http://127.0.0.1:{port}/slots", timeout=4) as r:
                if r.status == 200:
                    ready = True
                    break
        except Exception:
            pass
        if free_gb() < margin_gb:
            why = f"aborted: free fell below {margin_gb} GB"
            break
        time.sleep(3)

    after_free = free_gb()
    kv_mib, cells = None, None
    try:
        text = open(log_path, encoding="utf-8", errors="replace").read()
        for line in text.splitlines():
            m = KV_RE.search(line)
            if m:
                kv_mib, cells = float(m.group(1)), int(m.group(2))
        if not ready and not why:
            for needle in ("ErrorOutOfDeviceMemory", "failed to allocate",
                           "out of memory", "error loading model"):
                if needle in text:
                    why = needle
                    break
    except OSError:
        pass

    proc.terminate()
    try:
        proc.wait(timeout=30)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=30)
    log.close()
    for _ in range(20):
        s = socket.socket()
        if s.connect_ex(("127.0.0.1", port)) != 0:
            s.close()
            break
        s.close()
        time.sleep(0.5)

    consumed = base_free - after_free
    return {
        "ctx": ctx,
        "per_slot": ctx // slots,
        "loaded": ready,
        "why": why,
        "kv_mib": kv_mib,
        "kv_kib_per_cell": round(kv_mib * 1024 / cells, 1) if kv_mib and cells else None,
        "ram_consumed_gb": round(consumed, 2),
        "free_after_gb": round(after_free, 2),
        "headroom_ok": after_free >= margin_gb,
    }


def main():
    name = sys.argv[1] if len(sys.argv) > 1 else "Qwen3.8-9B-Q4_K_M.gguf"
    if not name.endswith(".gguf"):
        name += ".gguf"
    ctxs = [int(x) for x in (sys.argv[2] if len(sys.argv) > 2 else "4096,8192,16384,32768").split(",")]
    path = os.path.join(MODELS, name)
    if not os.path.exists(path):
        print(f"error: no model at {path}", file=sys.stderr)
        return 2
    gb = os.path.getsize(path) / 1e9
    ngl = int(sys.argv[3]) if len(sys.argv) > 3 else policy_ngl(gb)
    slots = int(sys.argv[4]) if len(sys.argv) > 4 else 2
    margin = 4.0

    if not os.path.exists(SERVER):
        print(f"error: no llama-server at {SERVER}", file=sys.stderr)
        return 2

    print(f"model : {name} ({gb:.2f} GB)")
    print(f"total RAM: {total_gb():.1f} GB   free now: {free_gb():.1f} GB")
    print(f"args  : -ngl {ngl} -np {slots} -fa on -ctk/-ctv q8_0 -b 512 -ub 128")
    print(f"get_ctx_size model_cap for this model: {policy_ctx(gb)}")
    print(f"headroom floor: {margin:.1f} GB free after load")
    print()
    print(f"{'ctx':>7} {'/slot':>7} {'loaded':>7} {'KV MiB':>8} {'KiB/cell':>9} "
          f"{'RAM GB':>7} {'free GB':>8} {'ok':>4}  note")
    print("-" * 86)
    rows = []
    for ctx in ctxs:
        r = trial(path, gb, ctx, ngl, slots, margin)
        rows.append(r)
        note = r["why"][:26]
        kv = f"{r['kv_mib']:.0f}" if r["kv_mib"] else "-"
        kc = f"{r['kv_kib_per_cell']:.1f}" if r["kv_kib_per_cell"] else "-"
        print(f"{r['ctx']:>7} {r['per_slot']:>7} {'yes' if r['loaded'] else 'NO':>7} "
              f"{kv:>8} {kc:>9} {r['ram_consumed_gb']:>7.2f} {r['free_after_gb']:>8.2f} "
              f"{'y' if r['headroom_ok'] else 'N':>4}  {note}")

    ok = [r for r in rows if r["loaded"] and r["headroom_ok"]]
    print()
    if ok:
        best = max(ok, key=lambda r: r["ctx"])
        print(f"  largest ctx that loaded WITH >= {margin:.1f} GB free: {best['ctx']} "
              f"({best['per_slot']} per slot)")
        print(f"  policy cap for this model: {policy_ctx(gb)}")
        if policy_ctx(gb) is None:
            print("  => this model is uncapped, nothing withheld")
        elif best["ctx"] > policy_ctx(gb):
            print(f"  => cap withholds {best['ctx'] / policy_ctx(gb):.1f}x the context "
                  f"at this headroom")
        else:
            print("  => cap is at or above the measured safe maximum")
    if any(r["kv_kib_per_cell"] for r in rows):
        kcs = [r["kv_kib_per_cell"] for r in rows if r["kv_kib_per_cell"]]
        print(f"  KV cost is linear in ctx: {min(kcs):.1f}-{max(kcs):.1f} KiB/cell "
              f"across sizes (constant => per-cell, so ctx scales RAM linearly)")
    print("\nA '-' means llama.cpp did not report that field. Reported, not imputed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
