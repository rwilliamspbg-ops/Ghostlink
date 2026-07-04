#!/usr/bin/env python3
"""
Ghostlink Studio - Cross-platform Launch Script
"""
import subprocess
import sys
import time
import os
import socket
import requests
from pathlib import Path

ROOT_DIR = Path(__file__).parent.parent
os.chdir(ROOT_DIR)

def log(msg):
    print(f"\033[1;34m[Ghostlink]\033[0m {msg}")

def fail(msg):
    print(f"\033[1;31m[Ghostlink][error]\033[0m {msg}", file=sys.stderr)
    sys.exit(1)

def check_port(port):
    try:
        requests.get(f'http://127.0.0.1:{port}/health', timeout=2)
        return True
    except:
        return False

def main():
    check_only = '--check' in sys.argv
    
    log("Starting Ghostlink Studio initialization...")
    
    if check_only:
        log("Running preflight checks...")
        
        # Check Ollama
        if check_port(11434):
            log("  [OK] Ollama running on port 11434")
        else:
            fail("  [ERROR] Ollama not running on port 11434")
        
        # Check neural-chat
        try:
            result = subprocess.run(['ollama', 'list'], capture_output=True, text=True)
            if 'neural-chat' in result.stdout:
                log("  [OK] neural-chat model available")
            else:
                log("  [WARN] neural-chat model not loaded - will be pulled on first use")
        except:
            log("  [WARN] Could not check ollama models")
        
        log("  [OK] Backend will run on port 8003")
        log("  [OK] GUI proxy will run on port 9999")
        log("Preflight completed successfully")
        return 0
    
    # Build
    log("Building Ghostlink backend (release)...")
    result = subprocess.run(['cargo', 'build', '--release', '-p', 'ghost-link'], 
                          capture_output=True, text=True)
    if result.returncode != 0:
        fail(f"Build failed: {result.stderr}")
    log("Build complete")
    
    # Start services
    log("Starting services...")
    
    # Check Ollama
    if not check_port(11434):
        log("Starting Ollama...")
        subprocess.Popen(['ollama', 'serve'], 
                        stdout=subprocess.DEVNULL, 
                        stderr=subprocess.DEVNULL)
        time.sleep(3)
    
    # Ensure neural-chat
    try:
        result = subprocess.run(['ollama', 'list'], capture_output=True, text=True)
        if 'neural-chat' not in result.stdout:
            log("Pulling neural-chat model (4.1GB)...")
            subprocess.run(['ollama', 'pull', 'neural-chat'], timeout=600)
    except:
        pass
    
    # Start backend
    log("Starting backend on port 8003...")
    backend_proc = subprocess.Popen([
        str(ROOT_DIR / 'target' / 'release' / 'ghost-link'),
        'serve'
    ], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(2)
    
    # Start proxy
    log("Starting LLM proxy on port 9999...")
    proxy_proc = subprocess.Popen(['python3', 'real_llm_proxy.py'],
                                  stdout=subprocess.DEVNULL,
                                  stderr=subprocess.DEVNULL)
    time.sleep(2)
    
    # Start GUI
    log("Launching Ghostlink Studio GUI...")
    try:
        subprocess.run(['python3', 'ghostlink_gui.py', 
                       '--backend-url', 'http://127.0.0.1:9999'])
    except KeyboardInterrupt:
        log("Shutting down...")
    finally:
        backend_proc.terminate()
        proxy_proc.terminate()
        backend_proc.wait(timeout=5)
        proxy_proc.wait(timeout=5)
    
    return 0

if __name__ == '__main__':
    sys.exit(main())
