# Local Testing & CI Gate Guide

This guide covers how to run Ghostlink tests and local end-to-end (E2E) RPC fabric testing.

---

## 1. Unit & Integration Tests

### Rust Workspace Tests
Run all Rust unit and integration tests across the workspace:
```bash
cargo test --workspace
```

To run individual crate tests:
```bash
cargo test -p ghost-link
cargo test -p ghostlink-core
```

### Python Assertion & Soak Unit Tests
Run unit tests for the RPC fabric assertion and soak scripts using mock log and topology fixtures:
```bash
python3 -m unittest tests/test_rpc_fabric_assert.py tests/test_rpc_fabric_soak.py
```

---

## 2. Running Docker RPC Fabric Locally

Ghostlink includes a two-container Docker Compose setup (`docker-compose.rpc-fabric.yml`) for verifying real distributed inference capabilities across process and network boundaries.

### Prerequisites
- Docker & Docker Compose
- Python 3.12+
- `requests` and `urllib3` Python packages (`pip install requests urllib3`)

### Execution Steps

1. **Build and start the two-container RPC fabric**:
   ```bash
   docker compose -f docker-compose.rpc-fabric.yml up -d --wait --wait-timeout 300
   ```

2. **Run the distributed inference assertion script**:
   ```bash
   python3 scripts/rpc_fabric_assert.py
   ```

   **Assertion Checks Performed**:
   - Asserts 2 healthy peers discovered via UDP (`GET /api/workers/discover`).
   - Asserts `distributed_inference: true` can be patched and persisted via `/api/settings`.
   - Asserts model loading (`POST /api/models/load`) succeeds on coordinator.
   - Asserts chat completion returns `real_inference: true` and non-empty content.
   - Inspects cluster topology (`GET /api/cluster/topology`):
     - In CPU mode (`-ngl 0`), labels run as `connectivity-only` and asserts `distributed_active` is `false` (no false compute split claimed).
     - In GPU mode (`-ngl > 0`), asserts `distributed_active` is `true` and active RPC targets exist.
   - Asserts contributor's `ggml-rpc-server` log contains live connection/accept evidence.

   If running on GPU hardware with GPU offloading enabled, pass `--require-compute-split` to enforce active tensor compute split:
   ```bash
   python3 scripts/rpc_fabric_assert.py --require-compute-split
   ```

3. **How to Soak & Fault Test**:
   Run the repeatable RPC fabric soak and contributor-kill drain harness (`python3 scripts/rpc_fabric_soak.py`), which asserts drain-and-restart behavior without requiring a GPU or large model. The harness sends baseline inference requests to the existing `stories15M-q4_0` model, kills/stops the contributor container (`ghostlink-rpc-contributor`), asserts that the coordinator immediately removes the dead peer from `active_rpc_targets` in `/api/cluster/topology`, verifies that in-flight or subsequent requests fail or cancel cleanly without hanging, and optionally restarts the contributor to verify re-admission and discovery recovery.

4. **Tear down the fabric**:
   ```bash
   docker compose -f docker-compose.rpc-fabric.yml down -v
   ```
