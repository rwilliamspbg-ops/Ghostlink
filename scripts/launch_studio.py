#!/usr/bin/env python3
"""
Ghostlink Studio - Cross-platform Launch Script
Orchestrates Rust Backend, Model Manager, Gateway Proxy, and GUI.
"""
import subprocess
import sys
import time
import os
import signal
from pathlib import Path

ROOT_DIR = Path(__file__).parent.parent
os.chdir(ROOT_DIR)

def log(msg):
    print(f"\033[1;34m[Ghostlink]\033[0m {msg}")

def fail(msg):
    print(f"\033[1;31m[Ghostlink][error]\033[0m {msg}", file=sys.stderr)
    sys.exit(1)

def check_service(url):
    try:
        import requests
        resp = requests.get(url, timeout=1)
        return resp.status_code == 200
    except:
        return False

def main():
    check_only = '--check' in sys.argv

    log("Starting Ghostlink Studio initialization...")

    # Preflight Check
    if check_only:
        log("Running preflight checks...")

        # Check Ollama - use /api/tags as it definitely exists if Ollama is up
        if check_service("http://127.0.0.1:11434/api/tags"):
            log("  [OK] Ollama running on port 11434")
        else:
            log("  [WARN] Ollama not detected on port 11434")
            log("         (Will attempt to start it during full launch)")

        log("  [OK] Backend will run on port 8003")
        log("  [OK] Model Manager will run on port 8001")
        log("  [OK] Gateway Proxy will run on port 9999")
        log("Preflight completed successfully")
        return 0

    # 1. Build Backend
    log("Building Ghostlink backend (release)...")
    try:
        subprocess.run(['cargo', 'build', '--release', '-p', 'ghost-link'],
                      check=True, capture_output=True, text=True)
        log("Build complete")
    except subprocess.CalledProcessError as e:
        fail(f"Build failed: {e.stderr}")

    # 2. Start Services
    processes = []

    def start_proc(args, name, wait_time=1):
        log(f"Starting {name}...")
        proc = subprocess.Popen(args, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        processes.append((proc, name))
        time.sleep(wait_time)
        return proc

    # Start Ollama if needed
    if not check_service("http://127.0.0.1:11434/api/tags"):
        log("Ollama not running. Attempting to start 'ollama serve'...")
        subprocess.Popen(['ollama', 'serve'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        time.sleep(3)

    # Start Model Manager (8001)
    start_proc([sys.executable, 'model_manager.py'], "Model Manager")

    # Start Backend (8003)
    backend_path = ROOT_DIR / 'target' / 'release' / 'ghost-link'
    if not backend_path.exists():
        # Fallback for debug build if release build failed/missing somehow
        backend_path = ROOT_DIR / 'target' / 'debug' / 'ghost-link'

    start_proc([str(backend_path), 'serve', '127.0.0.1', '8003'], "Ghostlink Backend")

    # Start Gateway Proxy (9999)
    start_proc([sys.executable, 'real_llm_proxy.py'], "Gateway Proxy")

    # 3. Launch GUI
    log("Launching Ghostlink Studio GUI...")
    try:
        # Pass the Proxy URL as the backend URL
        subprocess.run([sys.executable, 'ghostlink_gui.py',
                       '--backend-url', 'http://127.0.0.1:9999'], check=False)
    except KeyboardInterrupt:
        pass
    finally:
        log("Shutting down services...")
        for proc, name in reversed(processes):
            log(f"Stopping {name}...")
            proc.terminate()
            try:
                proc.wait(timeout=3)
            except subprocess.TimeoutExpired:
                proc.kill()
        log("Ghostlink Studio exited.")

    return 0

if __name__ == '__main__':
    sys.exit(main())
