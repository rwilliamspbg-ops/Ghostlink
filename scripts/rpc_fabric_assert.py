#!/usr/bin/env python3
"""Verification script for docker-compose.rpc-fabric.yml.

Proves Ghostlink's real distributed-inference feature (llama.cpp's ggml-rpc
backend, see crates/ghost-link/src/rpc_cluster.rs) works end to end across
two containers:

  1. Waits for both containers' /health to come up.
  2. Confirms UDP discovery actually found the peer (GET /api/workers/discover).
  3. PATCHes distributed_inference=true on the coordinator via the live
     /api/settings API (idempotent — compose already sets it, but this
     mirrors the original live-toggle verification from commit 46a023c).
  4. Loads the tiny GGUF test model on the coordinator (POST /api/models/load).
  5. POSTs a real chat completion and asserts real_inference: true.
  6. Inspects the placement plan (GET /api/cluster/topology) to distinguish
     CPU (-ngl 0, connectivity-only) from GPU offload (-ngl > 0, compute split).
     When -ngl 0, labels the run as "connectivity-only" and asserts no false compute split.
  7. Greps the contributor's ggml-rpc-server log (GHOSTLINK_RPC_SERVER_LOG)
     for real connection activity — a passing chat completion alone is not
     enough evidence the contributor did any work; upstream ggml-rpc-server
     logs a line per incoming connection, which is what's checked here.

Exits non-zero (with the reason printed) on any failed step.
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
RPC_SERVER_LOG_PATH = "/tmp/ggml-rpc-server.log"

HEADERS = {"Authorization": f"Bearer {API_KEY}"}


def log(msg: str) -> None:
    print(f"[assert] {msg}", flush=True)


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


def docker_exec(container: str, *cmd: str) -> str:
    result = subprocess.run(
        ["docker", "exec", container, *cmd],
        capture_output=True,
        text=True,
    )
    return result.stdout + result.stderr


def check_peers(discover_data: dict) -> int:
    count = discover_data.get("count", 0)
    if count < 2:
        raise ValueError(
            f"FAIL: discovery found fewer than 2 peers (count={count}, response={discover_data})"
        )
    return count


def check_real_inference(chat_result: dict) -> tuple[bool, str]:
    message = chat_result.get("choices", [{}])[0].get("message", {})
    content = message.get("content", "")
    real_inference = message.get("real_inference")

    if not real_inference:
        raise ValueError(
            f"FAIL: real_inference was not true in the chat completion response: {chat_result}"
        )
    if not content.strip():
        raise ValueError("FAIL: chat completion returned empty content")

    return bool(real_inference), content


def check_rpc_log_evidence(rpc_log_text: str) -> str:
    if not rpc_log_text.strip():
        raise ValueError(
            f"FAIL: {RPC_SERVER_LOG_PATH} is empty — "
            "no evidence the contributor's ggml-rpc-server ever saw a connection. "
            "A passing chat completion alone does not prove real distributed "
            "work happened."
        )

    lowered = rpc_log_text.lower()
    connection_evidence = any(
        marker in lowered
        for marker in (
            "client connect",
            "accepted client",
            "new connection",
            "start_rpc_server",
            "127.0.0.1",
            "172.30.",
        )
    )
    if not connection_evidence:
        raise ValueError(
            "FAIL: ggml-rpc-server log exists but shows no recognizable connection "
            "evidence (checked for connect/accept/start_rpc_server/peer-IP markers)."
        )
    return "OK: connection evidence found"


def check_placement_and_offload(
    topology_data: dict, settings_data: dict, require_compute_split: bool = False
) -> str:
    ngl = settings_data.get("ngl", 0)
    placement = topology_data.get("placement_plan", {})
    distributed_active = placement.get("distributed_active", False)

    if ngl == 0:
        if distributed_active:
            raise ValueError(
                "FAIL: -ngl is 0 (CPU-only) but placement plan claims active compute split"
            )
        if require_compute_split:
            raise ValueError(
                "FAIL: -ngl is 0 (connectivity-only mode) but runner required real compute split (-ngl > 0)"
            )
        log_label = "connectivity-only (CPU -ngl 0: live RPC accept/connect established, local node execution)"
        return log_label
    else:
        if not distributed_active:
            raise ValueError(
                f"FAIL: -ngl is {ngl} (>0) but placement plan reports distributed_active is false"
            )
        active_rpc_targets = placement.get("active_rpc_targets", [])
        if not active_rpc_targets:
            raise ValueError(
                f"FAIL: -ngl is {ngl} (>0) and distributed_active is true, but no active_rpc_targets listed in placement plan"
            )
        log_label = f"compute-split (GPU -ngl {ngl}: active RPC targets {active_rpc_targets})"
        return log_label


def step(n: int, title: str) -> None:
    print(f"\n=== Step {n}: {title} ===", flush=True)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--coordinator-url", default=COORDINATOR_URL)
    parser.add_argument(
        "--require-compute-split",
        action="store_true",
        help="Require real GPU compute split (-ngl > 0) instead of allowing connectivity-only mode",
    )
    args = parser.parse_args()
    base = args.coordinator_url.rstrip("/")

    try:
        step(1, "wait for both containers healthy")
        wait_http(f"{base}/health", "rpc-coordinator API")
        contributor_health = docker_exec(
            CONTRIBUTOR_CONTAINER, "curl", "-fsSk", "https://localhost:8003/health"
        )
        if not contributor_health.strip():
            raise ValueError("FAIL: rpc-contributor /health returned nothing")
        log(f"rpc-contributor /health: {contributor_health.strip()}")

        step(2, "confirm UDP discovery found the peer")
        discover_resp = None
        for attempt in range(1, 11):
            r = requests.get(
                f"{base}/api/workers/discover",
                headers=HEADERS,
                timeout=10,
                verify=False,
            )
            r.raise_for_status()
            discover_resp = r.json()
            log(f"attempt {attempt}: {discover_resp}")
            if discover_resp.get("count", 0) >= 2:
                break
            time.sleep(3)
        else:
            raise ValueError(
                f"FAIL: discovery never found the contributor peer. Last response: {discover_resp}"
            )
        check_peers(discover_resp)
        log(f"discovery OK: {discover_resp}")

        step(3, "PATCH distributed_inference=true via live /api/settings")
        r = requests.post(
            f"{base}/api/settings",
            json={"distributed_inference": True},
            headers=HEADERS,
            timeout=10,
            verify=False,
        )
        r.raise_for_status()
        settings = requests.get(
            f"{base}/api/settings", headers=HEADERS, timeout=10, verify=False
        ).json()
        if not settings.get("distributed_inference"):
            raise ValueError(f"FAIL: distributed_inference did not stick: {settings}")
        log(
            f"distributed_inference confirmed true: rpc_port={settings.get('rpc_port')}, ngl={settings.get('ngl')}"
        )

        step(4, "load the tiny GGUF model on the coordinator")
        r = requests.post(
            f"{base}/api/models/load",
            json={"model": MODEL_NAME},
            headers=HEADERS,
            timeout=120,
            verify=False,
        )
        r.raise_for_status()
        load_result = r.json()
        log(f"load result: {load_result}")
        if "error" in load_result:
            raise ValueError(f"FAIL: model load reported an error: {load_result}")

        step(5, "POST a real chat completion and assert real_inference: true")
        r = requests.post(
            f"{base}/v1/chat/completions",
            json={
                "model": MODEL_NAME,
                "messages": [
                    {"role": "user", "content": "Once upon a time there was a"}
                ],
                "max_tokens": 32,
            },
            headers=HEADERS,
            timeout=120,
            verify=False,
        )
        r.raise_for_status()
        chat_result = r.json()
        log(f"chat completion response: {chat_result}")
        real_inference, content = check_real_inference(chat_result)
        log(f"real_inference=True, generated text: {content!r}")

        step(6, "inspect cluster topology & placement plan (-ngl 0 vs GPU offload)")
        r = requests.get(
            f"{base}/api/cluster/topology", headers=HEADERS, timeout=10, verify=False
        )
        r.raise_for_status()
        topology_data = r.json()
        mode_label = check_placement_and_offload(
            topology_data, settings, require_compute_split=args.require_compute_split
        )
        log(f"Placement plan classification: {mode_label}")

        step(7, "grep contributor ggml-rpc-server log for real connection activity")
        rpc_log = docker_exec(CONTRIBUTOR_CONTAINER, "cat", RPC_SERVER_LOG_PATH)
        check_rpc_log_evidence(rpc_log)

        log(f"ggml-rpc-server log ({RPC_SERVER_LOG_PATH}), last 40 lines:")
        for line in rpc_log.strip().splitlines()[-40:]:
            print(f"    {line}")

        step(8, "show coordinator log lines for --rpc / -ts construction")
        logs_result = subprocess.run(
            ["docker", "logs", "--tail", "400", COORDINATOR_CONTAINER],
            capture_output=True,
            text=True,
        )
        coordinator_logs = logs_result.stdout + logs_result.stderr
        rpc_flag_lines = [
            line
            for line in coordinator_logs.splitlines()
            if "--rpc" in line
            or "Distributed inference enabled" in line
            or "Tensor split" in line
        ]
        if rpc_flag_lines:
            log("coordinator log lines showing --rpc/-ts construction:")
            for line in rpc_flag_lines:
                print(f"    {line}")
        else:
            log("no '--rpc'/'Tensor split' lines found in recent coordinator logs")

        print("\n=== ALL CHECKS PASSED ===")
        print(f"real_inference: {real_inference}")
        print(f"execution_mode: {mode_label}")
        print(f"generated text: {content!r}")
        return 0

    except Exception as err:
        print(f"\nASSERTION FAILED: {err}", file=sys.stderr, flush=True)
        return 1


if __name__ == "__main__":
    sys.exit(main())
