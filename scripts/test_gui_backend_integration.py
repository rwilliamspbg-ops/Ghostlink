#!/usr/bin/env python3
"""Functional integration test for Ghostlink GUI and Backend."""

import requests
import json
import time
import subprocess
import os
import signal
import sys

def test_endpoint(name, url, method="GET", payload=None, stream=False):
    print(f"Testing {name} ({method} {url})...", end=" ", flush=True)
    try:
        if method == "GET":
            resp = requests.get(url, timeout=10)
        else:
            if stream:
                resp = requests.post(url, json=payload, stream=True, timeout=15)
                count = 0
                for line in resp.iter_lines():
                    if line:
                        count += 1
                    if count > 2:
                        break
                if count > 0:
                    print("OK (stream started)")
                    return True
                else:
                    print("FAILED (no stream data)")
                    return False
            else:
                resp = requests.post(url, json=payload, timeout=10)

        if resp.status_code == 200:
            print("OK")
            return True
        else:
            print(f"FAILED (Status {resp.status_code})")
            return False
    except Exception as e:
        print(f"FAILED (Exception: {e})")
        return False

def main():
    host = "127.0.0.1"
    port = 8006
    base_url = f"http://{host}:{port}"

    print("Starting backend...")
    proc = subprocess.Popen(
        ["cargo", "run", "-p", "ghost-link", "--", "serve", host, str(port)],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        preexec_fn=os.setsid
    )

    # Wait for health
    success = False
    for i in range(60):
        try:
            r = requests.get(f"{base_url}/health", timeout=1)
            if r.status_code == 200:
                print("Backend ready.")
                success = True
                break
        except:
            pass
        time.sleep(1)

    if not success:
        print("Backend failed to start.")
        os.killpg(os.getpgid(proc.pid), signal.SIGTERM)
        sys.exit(1)

    results = []
    results.append(test_endpoint("Health", f"{base_url}/health"))
    results.append(test_endpoint("Models List", f"{base_url}/api/models"))
    results.append(test_endpoint("Metrics", f"{base_url}/api/metrics"))
    results.append(test_endpoint("Workers", f"{base_url}/api/workers"))
    results.append(test_endpoint("Sessions", f"{base_url}/api/sessions"))

    # Use a real tiny repo for validation test
    results.append(test_endpoint("Model Download/Verify", f"{base_url}/api/models/download", "POST",
                                 {"model_id": "sshleifer/tiny-gpt2"}))

    results.append(test_endpoint("Chat Stream", f"{base_url}/api/inference/chat", "POST",
                                 {"message": "test message", "temperature": 0.7, "top_p": 1.0, "top_k": 50, "penalty": 1.0, "max_tokens": 10},
                                 stream=True))

    os.killpg(os.getpgid(proc.pid), signal.SIGTERM)

    if all(results):
        print("\nALL INTEGRATION TESTS PASSED")
        sys.exit(0)
    else:
        print("\nSOME TESTS FAILED")
        sys.exit(1)

if __name__ == "__main__":
    main()
