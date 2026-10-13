"""Sweep `-ngl` on the current build and compare against what `get_ngl` picks.

`get_ngl`'s `model_size_gb >= 10.0 -> 0` branch is the only part of the tuning
surface whose justification lives in a *test comment* rather than a measurement
in the code. The comment records a 13.6 GB model at 1.9x decode when fully
offloaded, but 14.15 GB committed and under 1 GB free RAM on a host that also
runs the UI. That is a memory argument, not a throughput one, and it was never
re-measured against the tier table it overrides.

This measures, per `-ngl` value: whether the model loaded, whether the layers
actually went to the device, and prefill and decode separately. Prefill and
decode are reported apart because they respond to different levers -- offload
moves prefill and leaves bandwidth-bound decode flat, which is exactly the
asymmetry the policy is (or is not) justified by.

Safety: on a shared-memory iGPU, offloading *duplicates* weights into a
device-local buffer rather than moving them, so `-ngl > 0` needs roughly twice
the model size in free RAM. This refuses to launch a config whose requirement
is not met rather than discovering it as an OOM reboot.

Usage:
    python scripts/ngl_sweep.py [model.gguf] [ngl,ngl,...] [ctx]
    python scripts/ngl_sweep.py Qwen3.8-9B-Q4_K_M.gguf 0,12,24,40,-1 8192
"""

import ctypes
import json
import os
import socket
import statistics
import subprocess
import sys
import time
import urllib.error
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCRATCH = os.path.join(os.environ.get("TEMP", ROOT), "ghostlink-ngl-sweep")
SERVER = os.path.join(ROOT, "third_party/llama.cpp/build/bin/Release/llama-server.exe")
MODELS_DIR = os.path.join(ROOT, "models")

THREADS = 8
# The documented baseline shape, held fixed so `-ngl` is the only variable.
FIXED = ["-fa", "on", "-b", "512", "-ub", "128", "-ctk", "q8_0", "-ctv", "q8_0"]

PROMPT = (
    "A deployment pipeline validates a rollout manifest, checksums every artifact, and "
    "refuses to promote a build whose rollback target is unreachable. "
) * 30


class MEMORYSTATUSEX(ctypes.Structure):
    _fields_ = [
        ("dwLength", ctypes.c_ulong),
        ("dwMemoryLoad", ctypes.c_ulong),
        ("ullTotalPhys", ctypes.c_ulonglong),
        ("ullAvailPhys", ctypes.c_ulonglong),
        ("ullTotalPageFile", ctypes.c_ulonglong),
        ("ullAvailPageFile", ctypes.c_ulonglong),
        ("ullTotalVirtual", ctypes.c_ulonglong),
        ("ullAvailVirtual", ctypes.c_ulonglong),
        ("ullAvailExtendedVirtual", ctypes.c_ulonglong),
    ]


def free_gb() -> float:
    st = MEMORYSTATUSEX()
    st.dwLength = ctypes.sizeof(MEMORYSTATUSEX)
    ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(st))
    return st.ullAvailPhys / 1e9


def policy_ngl(model_gb: float):
    """Mirror get_ngl's own decision, so the comparison is against the code.

    The tier comes from `GHOSTLINK_VRAM_GB`, which only the launch scripts set
    (`launch.sh` exports it; `launch-native.ps1` does not). A direct
    `ghost-link serve` therefore hits `unwrap_or(-1)` and gets full offload for
    any sub-10GB model. Reading free RAM here instead would invent a tier the
    code never consults.
    """
    raw = os.environ.get("GHOSTLINK_VRAM_GB")
    vram = None
    if raw is not None:
        try:
            vram = float(raw.strip())
        except ValueError:
            vram = None
    if vram is None:
        tier, why = -1, "GHOSTLINK_VRAM_GB unset -> unwrap_or(-1)"
    elif vram >= 12.0:
        tier, why = 40, f"GHOSTLINK_VRAM_GB={vram:g} >= 12 -> 40"
    elif vram >= 8.0:
        tier, why = 24, f"GHOSTLINK_VRAM_GB={vram:g} >= 8 -> 24"
    elif vram >= 4.0:
        tier, why = 12, f"GHOSTLINK_VRAM_GB={vram:g} >= 4 -> 12"
    else:
        tier, why = 0, f"GHOSTLINK_VRAM_GB={vram:g} < 4 -> 0"
    if model_gb >= 10.0:
        return 0, "model >= 10GB overrides the VRAM tier -> 0"
    return tier, why


def free_port() -> int:
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


def wait_ready(port: int, proc, limit: int = 900):
    """Ready means /completion answers, not that the socket is bound.

    llama-server binds while the model is still loading and answers 503 until it
    is done, so a socket-level poll reports ready immediately and the first
    measured request dies.
    """
    deadline = time.time() + limit
    while time.time() < deadline:
        if proc.poll() is not None:
            return False, "process exited during load"
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=2):
                body = json.dumps({"prompt": "hi", "n_predict": 1, "stream": False}).encode()
                urllib.request.urlopen(
                    urllib.request.Request(
                        f"http://127.0.0.1:{port}/completion", data=body,
                        headers={"Content-Type": "application/json"},
                    ),
                    timeout=180,
                ).read()
                return True, ""
        except Exception:
            time.sleep(3)
    return False, "timed out waiting for load"


def offload_claim(log_path: str) -> str:
    """What the log says actually went to the device.

    `-ngl -1` is not proof of offload: llama-server silently defaults it to 0 in
    some builds and the report only prints at higher verbosity. Read the claim
    out of the log instead of inferring it from the flag.
    """
    try:
        text = open(log_path, encoding="utf-8", errors="replace").read()
    except OSError:
        return ""
    for line in reversed(text.splitlines()):
        low = line.lower()
        if "offloaded" in low and "layer" in low:
            return line.strip()[-80:]
        if "offloading" in low and "layer" in low:
            return line.strip()[-80:]
    return ""


def load_error(log_path: str) -> str:
    try:
        text = open(log_path, encoding="utf-8", errors="replace").read()
    except OSError:
        return ""
    for needle in (
        "ErrorOutOfDeviceMemory",
        "failed to allocate compute",
        "error loading model",
        "exiting due to model loading error",
        "out of memory",
    ):
        if needle in text:
            for line in text.splitlines():
                if needle in line:
                    return line.strip()[-110:]
    return ""


def trial(model_path: str, ngl: int, ctx: int, runs: int = 2) -> dict:
    size_gb = os.path.getsize(model_path) / 1e9
    # iGPU offload duplicates the weights; a device-local copy is allocated
    # alongside the host mapping. Assume the pessimistic case.
    need = size_gb * (2.0 if ngl > 0 else 1.0)
    avail = free_gb()
    if need > avail:
        return {
            "ngl": ngl, "gb": size_gb, "skipped": True,
            "why": f"needs ~{need:.1f}GB free (model {size_gb:.1f}GB, "
                   f"{'weights duplicated for offload' if ngl > 0 else 'host only'}), "
                   f"only {avail:.1f}GB available",
        }

    port = free_port()
    os.makedirs(SCRATCH, exist_ok=True)
    stamp = time.strftime("%H%M%S")
    log_path = os.path.join(SCRATCH, f"ngl{ngl}-ctx{ctx}-{stamp}.log")
    cmd = [
        SERVER, "-m", model_path, "--alias", f"ngl{ngl}",
        "--host", "127.0.0.1", "--port", str(port),
        "--no-warmup", "-t", str(THREADS), "-c", str(ctx), "-np", "1",
        "-ngl", str(ngl),
    ] + FIXED
    started = time.time()
    log = open(log_path, "w", encoding="utf-8", errors="replace")
    proc = subprocess.Popen(cmd, stdout=log, stderr=subprocess.STDOUT)
    try:
        ok, reason = wait_ready(port, proc)
        if not ok:
            return {
                "ngl": ngl, "gb": size_gb, "loaded": False, "why": reason,
                "detail": load_error(log_path), "load_s": round(time.time() - started),
                "claim": offload_claim(log_path),
            }
        load_s = round(time.time() - started)
        claim = offload_claim(log_path)
        body = json.dumps({
            "prompt": PROMPT, "stream": False, "max_tokens": 32,
            "temperature": 0.0, "cache_prompt": False,
        }).encode()
        pre, dec, pn = [], [], []
        for _ in range(runs):
            d = json.loads(urllib.request.urlopen(
                urllib.request.Request(
                    f"http://127.0.0.1:{port}/completion", data=body,
                    headers={"Content-Type": "application/json"},
                ),
                timeout=900,
            ).read())
            tm = d.get("timings", {})
            pre.append(tm.get("prompt_per_second"))
            dec.append(tm.get("predicted_per_second"))
            pn.append(tm.get("prompt_n"))
        return {
            "ngl": ngl, "gb": size_gb, "loaded": True, "load_s": load_s,
            "claim": claim,
            "prompt_tokens": statistics.median([x for x in pn if x is not None]) if any(pn) else None,
            "prompt_tps": statistics.median([x for x in pre if x is not None]) if any(pre) else None,
            "decode_tps": statistics.median([x for x in dec if x is not None]) if any(dec) else None,
        }
    except Exception as e:
        return {
            "ngl": ngl, "gb": size_gb, "loaded": False, "why": repr(e)[:70],
            "detail": load_error(log_path), "load_s": round(time.time() - started),
        }
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=30)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=30)
        log.close()
        for _ in range(20):
            probe = socket.socket()
            if probe.connect_ex(("127.0.0.1", port)) != 0:
                probe.close()
                break
            probe.close()
            time.sleep(0.5)
        else:
            print(
                f"  WARNING: port {port} still answering after terminating ngl={ngl}; "
                "a llama-server may have leaked. Check for orphans before continuing.",
                file=sys.stderr,
            )


def main() -> int:
    if not os.path.exists(SERVER):
        print(f"error: no llama-server at {SERVER}", file=sys.stderr)
        return 2
    name = sys.argv[1] if len(sys.argv) > 1 else "Qwen3.8-9B-Q4_K_M.gguf"
    if not name.endswith(".gguf"):
        name += ".gguf"
    ngls = [int(x) for x in (sys.argv[2] if len(sys.argv) > 2 else "0,12,24,40,-1").split(",")]
    ctx = int(sys.argv[3]) if len(sys.argv) > 3 else 8192

    path = os.path.join(MODELS_DIR, name)
    if not os.path.exists(path):
        print(f"error: no model at {path}", file=sys.stderr)
        return 2

    gb = os.path.getsize(path) / 1e9
    pol, why = policy_ngl(gb)
    print(f"model : {name}  ({gb:.2f} GB)")
    print(f"ctx   : {ctx}   fixed args: {' '.join(FIXED)}")
    print(f"free  : {free_gb():.1f} GB")
    print(f"get_ngl policy for this model: -ngl {pol}   ({why})")
    print()
    print(f"{'ngl':>5} {'loaded':>7} {'load s':>7} {'prefill':>9} {'decode':>8}  offload claim")
    print("-" * 78)
    rows = []
    for ngl in ngls:
        r = trial(path, ngl, ctx)
        rows.append(r)
        if r.get("skipped"):
            print(f"{ngl:>5} {'SKIP':>7} {'-':>7} {'-':>9} {'-':>8}  {r['why'][:52]}")
            continue
        if not r.get("loaded"):
            note = (r.get("detail") or r.get("why") or "")[:34]
            print(f"{ngl:>5} {'NO':>7} {r.get('load_s', 0):>7} {'-':>9} {'-':>8}  {note}")
            continue
        pt = f"{r['prompt_tps']:.1f}" if r.get("prompt_tps") else "-"
        dt = f"{r['decode_tps']:.2f}" if r.get("decode_tps") else "-"
        print(f"{ngl:>5} {'yes':>7} {r['load_s']:>7} {pt:>9} {dt:>8}  {(r.get('claim') or '')[:34]}")

    ok = [
        r for r in rows
        if r.get("loaded") and r.get("prompt_tps") and r.get("decode_tps")
    ]
    print()
    if len(ok) > 1:
        # Reference on CPU-only when present: that is what the policy picks for a
        # >=10GB model, so the comparison answers "what does offload buy over it".
        base = next((r for r in ok if r["ngl"] == 0), ok[0])
        print(
            f"relative to -ngl {base['ngl']} "
            f"({base['prompt_tps']:.1f} prefill / {base['decode_tps']:.2f} decode tok/s):"
        )
        for r in ok:
            if r is base:
                continue
            print(
                f"  -ngl {r['ngl']:>3}  {r['prompt_tps'] / base['prompt_tps']:5.2f}x prefill"
                f"   {r['decode_tps'] / base['decode_tps']:5.2f}x decode"
                f"   ({r['prompt_tps']:.1f} / {r['decode_tps']:.2f} tok/s)"
            )
        print()
    if any(r.get("skipped") for r in rows):
        print("  SKIP means the config's memory requirement was not met, so it was")
        print("  never launched -- not that it failed. Reported, not discovered as an OOM.")
        print()
    print("A '-' means llama.cpp did not report that field. Reported, not imputed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
