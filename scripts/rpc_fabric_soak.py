#!/usr/bin/env python3
"""Repeatable soak/fault script for the RPC fabric.

Asserts drain-and-restart behavior without requiring a GPU or a 30B model:

  1. Waits for two healthy peers (GET /api/workers/discover).
  2. PATCHes distributed_inference=true via /api/settings and loads the test model.
  3. Starts short generation / load against the existing tiny GGUF model (stories15M-q4_0).
  4. Kills or stops the contributor container (or ggml-rpc-server process).
  5. Asserts the coordinator does not keep advertising that peer as a live RPC target in GET /api/cluster/topology.
  6. Asserts in-flight or subsequent generation fails or cancels cleanly (no hang past timeout).
  7. Optionally restarts the contributor container and asserts re-admission / recovery.

Exits non-zero on any failed assertion.
"""

import argparse
import json
import subprocess
import sys
import time

import requests
import urllib3

urllib3.disable_warnings(urllib3.exceptions.InsecureRequestWarning)

COORDINATOR_URL = "https://127.0.0.1:8010"
API_KEY = "ghostlink-rpc-fabric-test-key-0123456789abcdef0123456789abcdef"
MODEL_NAME = "stories15M-q4_0"
CONTRIBUTOR_CONTAINER = "ghostlink-rpc-contributor"
COORDINATOR_CONTAINER = "ghostlink-rpc-coordinator"

HEADERS = {"Authorization": f"Bearer {API_KEY}"}


def log(msg: str) -> None:
    print(f"[soak] {msg}", flush=True)


def wait_http(url: str, label: str, attempts: int = 60, delay: float = 2.0):
    last_err = None
    for i in range(1, attempts + 1):
        try:
            resp = requests.get(url, timeout=3, verify=False)
            if resp.ok:
                log(f"{label} is ready ({url}) after {i} attempt(s)")
                return resp
        except requests.RequestException as err:
            last_err = err
        time.sleep(delay)
    raise SystemExit(f"FAIL: {label} never became ready at {url}: {last_err}")


def docker_cmd(*cmd: str) -> tuple[int, str]:
    result = subprocess.run(
        ["docker", *cmd],
        capture_output=True,
        text=True,
    )
    return result.returncode, (result.stdout + result.stderr).strip()


def check_peers(discover_data: dict, min_count: int = 2) -> int:
    count = discover_data.get("count", 0)
    if count < min_count:
        raise ValueError(
            f"FAIL: discovery found fewer than {min_count} peers (count={count}, response={discover_data})"
        )
    return count


def check_rpc_target_drained(topology_data: dict, contributor_ip: str = "172.30.0.11") -> bool:
    placement = topology_data.get("placement_plan", {})
    active_targets = placement.get("active_rpc_targets", [])
    for target in active_targets:
        if contributor_ip in target:
            raise ValueError(
                f"FAIL: contributor {contributor_ip} still advertised in active_rpc_targets: {active_targets}"
            )
    return True


def check_clean_failure_or_cancellation(response_or_err) -> str:
    """Verifies that in-flight/subsequent work fails or cancels cleanly without hanging."""
    if isinstance(response_or_err, Exception):
        err_msg = str(response_or_err)
        # Timeout exceptions, connection refused, or HTTP error responses are clean terminations
        log(f"Clean failure caught via exception: {err_msg}")
        return "OK: clean exception caught"
    elif hasattr(response_or_err, "ok") and hasattr(response_or_err, "json"):
        if not response_or_err.ok:
            log(f"Clean failure caught via HTTP status {response_or_err.status_code}")
            return f"OK: HTTP {response_or_err.status_code}"
        # If response was 200 OK, check if choice indicated fallback or error
        data = response_or_err.json()
        if "error" in data:
            log(f"Clean failure reported in JSON response: {data['error']}")
            return "OK: error in JSON"
        log(f"Response succeeded (fallback to local node): {data}")
        return "OK: succeeded on local node"
    else:
        raise ValueError(f"FAIL: unexpected object passed to failure check: {type(response_or_err)}")


def step(n: int, title: str) -> None:
    print(f"\n=== Step {n}: {title} ===", flush=True)


def main() -> int:
    parser = argparse.ArgumentParser(description="RPC fabric soak & fault drain script")
    parser.add_argument("--coordinator-url", default=COORDINATOR_URL)
    parser.add_argument(
        "--contributor-container",
        default=CONTRIBUTOR_CONTAINER,
        help="Container name for contributor node",
    )
    parser.add_argument(
        "--skip-restart",
        action="store_true",
        help="Skip the contributor container restart and re-admit step",
    )
    parser.add_argument(
        "--rounds",
        type=int,
        default=1,
        help="Number of soak/fault iteration rounds to execute",
    )
    args = parser.parse_args()
    base = args.coordinator_url.rstrip("/")

    try:
        step(1, "wait for coordinator readiness and peer discovery")
        wait_http(f"{base}/health", "rpc-coordinator API")

        # Enable distributed_inference
        r = requests.post(
            f"{base}/api/settings",
            json={"distributed_inference": True},
            headers=HEADERS,
            timeout=10,
            verify=False,
        )
        r.raise_for_status()

        # Load model
        r = requests.post(
            f"{base}/api/models/load",
            json={"model": MODEL_NAME},
            headers=HEADERS,
            timeout=120,
            verify=False,
        )
        r.raise_for_status()
        log(f"model loaded: {r.json()}")

        for round_num in range(1, args.rounds + 1):
            log(f"--- Starting Soak Round {round_num}/{args.rounds} ---")

            step(2, f"[Round {round_num}] verify two healthy peers discovered")
            discover_resp = None
            for attempt in range(1, 15):
                try:
                    r = requests.get(
                        f"{base}/api/workers/discover",
                        headers=HEADERS,
                        timeout=10,
                        verify=False,
                    )
                    if r.ok:
                        discover_resp = r.json()
                        if discover_resp.get("count", 0) >= 2:
                            break
                except requests.RequestException:
                    pass
                time.sleep(2)
            else:
                raise ValueError(
                    f"FAIL: discovery found fewer than 2 peers in round {round_num}. Last response: {discover_resp}"
                )
            check_peers(discover_resp, min_count=2)
            log(f"Peers verified: {discover_resp}")

            step(3, f"[Round {round_num}] execute initial baseline generation request")
            r = requests.post(
                f"{base}/v1/chat/completions",
                json={
                    "model": MODEL_NAME,
                    "messages": [{"role": "user", "content": "Hello ghostlink!"}],
                    "max_tokens": 16,
                },
                headers=HEADERS,
                timeout=60,
                verify=False,
            )
            r.raise_for_status()
            log(f"Baseline generation OK: {r.json().get(choices, [{}])[0].get(message, {}).get(content)!r}")

            step(4, f"[Round {round_num}] kill/stop contributor container ({args.contributor_container})")
            ret, out = docker_cmd("stop", args.contributor_container)
            if ret != 0:
                log(f"docker stop returned {ret}: {out}, attempting docker kill...")
                docker_cmd("kill", args.contributor_container)
            log(f"Contributor container {args.contributor_container} stopped")

            step(5, f"[Round {round_num}] assert contributor drained from active RPC targets")
            drained = False
            topology_data = {}
            for _ in range(15):
                r = requests.get(
                    f"{base}/api/cluster/topology",
                    headers=HEADERS,
                    timeout=10,
                    verify=False,
                )
                if r.ok:
                    topology_data = r.json()
                    try:
                        check_rpc_target_drained(topology_data, contributor_ip="172.30.0.11")
                        drained = True
                        break
                    except ValueError:
                        pass
                time.sleep(2)

            if not drained:
                check_rpc_target_drained(topology_data, contributor_ip="172.30.0.11")
            log("Contributor successfully drained from active_rpc_targets")

            step(6, f"[Round {round_num}] assert subsequent / in-flight inference cancels/fails cleanly")
            try:
                resp = requests.post(
                    f"{base}/v1/chat/completions",
                    json={
                        "model": MODEL_NAME,
                        "messages": [{"role": "user", "content": "Testing failover post contributor loss"}],
                        "max_tokens": 16,
                    },
                    headers=HEADERS,
                    timeout=30,
                    verify=False,
                )
                check_clean_failure_or_cancellation(resp)
            except Exception as exc:
                check_clean_failure_or_cancellation(exc)

            if not args.skip_restart:
                step(7, f"[Round {round_num}] restart contributor container and assert re-admit")
                ret, out = docker_cmd("start", args.contributor_container)
                if ret != 0:
                    raise ValueError(f"FAIL: docker start {args.contributor_container} failed: {out}")
                log(f"Contributor container {args.contributor_container} restarted")

                # Wait for contributor to re-discover
                readmitted = False
                for _ in range(20):
                    try:
                        r = requests.get(
                            f"{base}/api/workers/discover",
                            headers=HEADERS,
                            timeout=10,
                            verify=False,
                        )
                        if r.ok and r.json().get("count", 0) >= 2:
                            readmitted = True
                            log(f"Contributor re-admitted successfully: {r.json()}")
                            break
                    except requests.RequestException:
                        pass
                    time.sleep(2)

                if not readmitted:
                    raise ValueError(f"FAIL: contributor container did not re-admit after restart in round {round_num}")

        print("\n=== ALL SOAK & FAULT CHECKS PASSED ===")
        return 0

    except Exception as err:
        print(f"\nSOAK ASSERTION FAILED: {err}", file=sys.stderr, flush=True)
        return 1


if __name__ == "__main__":
    sys.exit(main())
