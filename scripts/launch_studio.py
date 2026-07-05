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
import shutil
from pathlib import Path

ROOT_DIR = Path(__file__).parent.parent
os.chdir(ROOT_DIR)
TAURI_GUI_DIR = ROOT_DIR / 'crates' / 'ghostlink-gui' / 'src-tauri'
FRONTEND_DIR = ROOT_DIR / 'crates' / 'ghostlink-gui' / 'frontend'

DEFAULT_PERF_ENV = {
    # Stable chat-oriented defaults for multi-machine layer splitting.
    'GHOSTLINK_FLOW_DEFAULT_TRANSPORT': 'tcp',
    'GHOSTLINK_TCP_MAX_INFLIGHT': '256',
    'GHOSTLINK_TCP_AUTOTUNE': '1',
    'GHOSTLINK_FLOW_ENABLE_REBALANCE': '1',
    'GHOSTLINK_CHAT_EXEC_TOKENS': '256',
    'GHOSTLINK_CHAT_MICRO_BATCH': '8',
}

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

def command_exists(name):
    return shutil.which(name) is not None

def can_launch_tauri():
    if not command_exists('cargo'):
        return False
    if not command_exists('npm'):
        return False
    if not TAURI_GUI_DIR.exists() or not FRONTEND_DIR.exists():
        return False
    try:
        result = subprocess.run(
            ['cargo', 'tauri', '--version'],
            cwd=TAURI_GUI_DIR,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        return result.returncode == 0
    except Exception:
        return False

def ensure_frontend_deps():
    node_modules = FRONTEND_DIR / 'node_modules'
    if node_modules.exists():
        return
    log("Installing Ghostlink Studio frontend dependencies (npm ci)...")
    subprocess.run(['npm', 'ci'], cwd=FRONTEND_DIR, check=True)

def main():
    check_only = '--check' in sys.argv
    chat_backend_mode = os.getenv('GHOSTLINK_STUDIO_CHAT_BACKEND', 'backend').strip().lower()
    if chat_backend_mode not in {'backend', 'ollama'}:
        chat_backend_mode = 'backend'
    requested_gui_mode = os.getenv('GHOSTLINK_STUDIO_GUI', 'tauri').strip().lower()
    if requested_gui_mode not in {'tauri', 'tkinter'}:
        requested_gui_mode = 'tauri'
    tauri_ready = can_launch_tauri()
    effective_gui_mode = requested_gui_mode if requested_gui_mode == 'tkinter' else ('tauri' if tauri_ready else 'tkinter')

    log("Starting Ghostlink Studio initialization...")
    log(f"GUI mode requested: {requested_gui_mode}")
    log(f"GUI mode effective: {effective_gui_mode}")

    for key, value in DEFAULT_PERF_ENV.items():
        os.environ.setdefault(key, value)

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
        if requested_gui_mode == 'tauri' and not tauri_ready:
            log("  [WARN] Tauri GUI prerequisites missing (cargo tauri and/or npm). Launcher will fallback to Tkinter.")
        else:
            log(f"  [OK] GUI launch mode: {effective_gui_mode}")
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
        proc = subprocess.Popen(args, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=os.environ.copy())
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
    start_proc([sys.executable, 'real_llm_proxy.py', chat_backend_mode], "Gateway Proxy")

    # 3. Launch GUI
    log("Launching Ghostlink Studio GUI...")
    try:
        if effective_gui_mode == 'tauri':
            ensure_frontend_deps()
            subprocess.run(
                ['cargo', 'tauri', 'dev'],
                cwd=TAURI_GUI_DIR,
                check=False,
                env=os.environ.copy(),
            )
        else:
            # Pass the Proxy URL as the backend URL
            subprocess.run([
                sys.executable,
                'ghostlink_gui.py',
                '--backend-url', 'http://127.0.0.1:9999',
            ], check=False)
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
