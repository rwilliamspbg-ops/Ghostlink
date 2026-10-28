# Ghostlink Roadmap Audit — Standing vs. Codebase

**Audit date:** 2026-10-27
**Repo:** `C:\Users\rwill\Ghostlink` (local HEAD `6587719`, tag `v2.5.0`)
**Roadmap doc:** `docs/ROADMAP.md` (last touched commit `e8ebfc4`, 2026-10-06; 198 commits have landed since without editing it)
**Scope:** Compare every "proven/shipped/in-progress" claim and every Horizon 1/2/3 + Enterprise Trust Track item in `docs/ROADMAP.md` against the code actually present in the working tree. No runtime execution was performed beyond file/workflow inspection; see §9 for what is unverified.

This is an audit, not a plan rewrite. It does not modify `docs/ROADMAP.md`.

## 1. Executive summary

- The roadmap is a **competitive strategy document**, not a backlog, and most of its *mechanism* claims are accurate. The codebase has moved substantially past the doc's last edit (198 commits) and several items the doc still marks "open/fast-follow" are already shipped, while a few items the doc marks "shipped" are only partially wired or are dead code.
- **Biggest real gaps still open** (code confirms absent/partial):
  1. RPC transport encryption (`rpc_tls.rs` exists but is explicitly `#![allow(dead_code)]` and "not yet wired into the application entry points"; `enable_rpc_tls` flips a flag that currently only *skips* the allowlist proxy).
  2. Config hot-reload for `ghostlink.toml` (no file watcher; `ConfigManager` is load/save only).
  3. Continuous rebalancing on the **real** serving path (`handle_gui_model_load` → `llama-server --rpc`) — `load_balance::rebalance()` has zero callers in `ghost-link`; only the synthetic `flow` CLI path touches it.
  4. Plugin marketplace-lite (`plugins.toml` discovery/install) — only compile-time/env `BackendPluginRegistry::from_env()`.
  5. Cluster-aggregate-VRAM-aware Hugging Face quant selection — download path sizes to a single node's VRAM (`detect_runtime_profile("hf-download")`), not cluster capacity.
  6. Cross-node speculative decoding — only a local `--spec-draft-model` flag to `llama-server`; no coordinator/contributor draft/verify split.
  7. LoRA/adapter support — absent.
  8. OIDC, guardrail/policy middleware, model-registry versioning/pin-rollback, usage quotas/tenancy — absent.
  9. WAN/relay/rendezvous service discovery, planner ABI for third-party planners, first-class multimodal pipeline, hosted control plane — absent (Horizon 3, expected).
- **Items the doc understates as open but are actually shipped:** bounded audit-log rotation/retention (`audit_log.jsonl.N`), dynamic (non-hardcoded) model-ready timeout, periodic `ggml-rpc-server` child respawn supervision, 4-role RBAC (`Owner/Operator/Inference/Viewer`), durable audit log + CEF export, OpenTelemetry tracing, Grafana dashboard + monitoring compose profile, JS/TS SDK, install scripts, task-runtime child fan-out (max_depth=1, max_children=4), AF_XDP flow transport.
- **One doc claim is actively wrong and should be corrected:** Enterprise Trust Track status text says "3-role API key access control (Admin/Operator/Viewer) shipped"; the code has **4 roles: `Owner/Operator/Inference/Viewer`** (`auth.rs`). The roadmap's own "Current truth" line (4-role) is correct; the Enterprise Track paragraph is stale.

## 2. Methodology

Evidence gathered by direct inspection of the working tree:

- File existence / module wiring: `ls crates/...`, `grep` for symbols, `read_file` on key modules.
- Workflow/CI presence: `.github/workflows/distributed-e2e.yml`, `deploy/grafana`, `scripts/install.*`.
- Role/audit/RPC symbols: `auth.rs`, `audit_log.rs`, `rpc_cluster.rs`, `rpc_tls.rs`, `native_engine.rs`, `backend_config.rs`, `task_runtime.rs`.
- No `cargo build`/`cargo test` was run for this audit (see §9).

## 3. Verified shipped (code evidence)

| Roadmap claim | Code evidence |
|---|---|
| Distributed capacity splitting via `ggml-rpc` | `crates/ghost-link/src/rpc_cluster.rs`, `native_engine.rs` (`--rpc`, `-ts`), `cluster.rs` |
| Contributor supervision & drain/restart | `RpcSupervisor` in `rpc_cluster.rs`; 30s respawn thread in `main.rs` |
| Version-mismatch rejection (`rpc_build_id`) | `rpc_cluster.rs` `discover_rpc_peers`, `evaluate_peer`; tests present |
| RPC admission (HMAC `rpc_shared_secret`, `rpc_allowed_peers`) | `rpc_cluster.rs` `admit_via_secret`, allowlist proxy |
| Bounded audit log rotation/retention (`audit_log.jsonl.N`) | `audit_log.rs`: `DEFAULT_AUDIT_LOG_MAX_BYTES/LINES/FILES`, `rotated_path` |
| Durable audit log + CEF export | `audit_log.rs` `append_durable`/`read_all_durable`, `/api/security/audit-log/export` |
| 4-role RBAC persisted hashed keys | `auth.rs` `Role { Owner, Operator, Inference, Viewer }`, `api_keys.json` |
| Task runtime + HITL review (v2.3) + child fan-out (v2.4) | `task_runtime.rs`, `task_api.rs`; fan-out caps max_depth=1, max_children=4 |
| OTel tracing (opt-in) | `otel.rs`; `GHOSTLINK_OTEL_EXPORTER_ENDPOINT` |
| Grafana dashboard + monitoring compose profile | `deploy/grafana/`, `deploy/prometheus/`, `docker-compose.yml` profiles |
| JS/TS SDK | `sdks/js/` (v2.5.0, SSE, typed errors per package) |
| One-line installers | `scripts/install.sh`, `scripts/install.ps1` |
| CI multi-process distributed E2E gate | `.github/workflows/distributed-e2e.yml` |
| AF_XDP feature-gated flow transport | `ghostlink-core/src/xdp.rs`, `main.rs` `FlowTransportMode::Xdp` |
| Dynamic model-ready timeout | `native_engine.rs` `compute_model_ready_timeout` (90s floor … 1800s, peer-aware) |
| Speculative decoding (local, opt-in) | `native_engine.rs` `draft_model_args()` (`GHOSTLINK_DRAFT_MODEL`) |

## 4. Claimed shipped but partial / stale / not actually wired

| Roadmap statement | Actual state | Evidence |
|---|---|---|
| "RPC Admission & Audit Rotation … bounded audit log rotation/retention" (Current truth) | **Shipped** (doc now accurate) | `audit_log.rs` |
| "Scoped Role-Based Access Control: 4-role …" (Current truth) | **Shipped** | `auth.rs` |
| Enterprise Track: "3-role API key access control (Admin/Operator/Viewer) shipped" | **Stale/wrong** — code is 4-role `Owner/Operator/Inference/Viewer` | `auth.rs` lines 31-36 |
| Enterprise Track: "RBAC / scoped multi-user keys … confirmed genuinely absent" then "core 3-role shipped" | Partially shipped: single-operator hashed multi-key RBAC exists; **multi-user/tenancy/namespacing still absent** | `auth.rs`, `API_REFERENCE.md` note |
| Risks: "ggml-rpc-server has no built-in authentication … status check 2026-08-15 shipped handshake" | **Shipped** as HMAC admission; byte stream still plaintext | `rpc_cluster.rs` |
| "transport-layer encryption for raw RPC byte streams" (Unproven) | **Module exists, not wired.** `rpc_tls.rs` is `#![allow(dead_code)]` with comment "not yet wired into the application entry points"; `enable_rpc_tls` only causes the allowlist proxy to be skipped | `rpc_tls.rs` header, `rpc_cluster.rs` `maybe_start_allowlist_proxy` |
| "Config hot-reload … (H1 #6, still open)" | **Still open** — confirmed | no file watcher; `ConfigManager` load/save only |
| "Continuous rebalancing … first-cut drain+restart shipped on real serving path" (H2 #1 status) | **Not on real serving path.** `rebalance()` has zero callers in `ghost-link`; real path is one `llama-server --rpc` process per load with no rebalancing | grep `.rebalance` in crates/ghost-link |
| "Plugin marketplace-lite" (H2 #6) | **Not present** — only `BackendPluginRegistry::from_env()` | `backend_plugin.rs` |
| "HF integration cluster-aware quant selection" (H2 #3) | **Partial** — HF download sizes to single node VRAM | `main.rs` `download_hf_model` uses `detect_runtime_profile("hf-download")` |
| "Speculative decoding across heterogeneous nodes" (H2 #2) | **Local only** — `--spec-draft-model` flag; no cross-node draft/verify | `native_engine.rs` `draft_model_args()` |
| "LoRA / adapter support" (H2 #4) | **Absent** | grep |
| "OIDC", "guardrail/policy middleware", "model registry versioning/pin-rollback", "usage quotas/tenancy" (Enterprise Track) | **Absent** | grep |
| "WAN relay/rendezvous", "planner ABI", "multimodal pipeline", "hosted control plane" (H3) | **Absent** (expected for horizon) | grep |
| "90-second model-ready timeout too short" (Risk) | **Addressed** — dynamic, peer-aware, env override | `native_engine.rs` `compute_model_ready_timeout` |
| "unsupervised ggml-rpc-server child" (Risk) | **Addressed** — `RpcSupervisor` + 30s respawn + health gate | `rpc_cluster.rs`, `main.rs` |

## 5. Roadmap status lines that should be updated

1. `Current truth` / Enterprise Track RBAC paragraph: change "3-role (Admin/Operator/Viewer)" → "4-role (Owner/Operator/Inference/Viewer)".
2. Enterprise Track item #2 (RPC peer auth): note `rpc_tls.rs` is a scaffold, not wired; transport encryption remains unimplemented (module-level dead code).
3. H2 #1 status: downgrade "first-cut shipped on real serving path" → "not implemented on real serving path; only synthetic `flow` path".
4. H1 #6: remains open (confirmed).
5. H2 #3: narrow "partially shipped" to "HF pull shipped; cluster-aggregate quant selection missing".
6. Risks: mark "90s timeout" and "unsupervised child" as addressed (keep version-mismatch + RPC auth caveats).
7. Add a "last audited" date and commit (`6587719`) to the "Current truth" block.

## 6. Recommended next steps (prioritized)

### P0 — correctness/trust (do before any "production" claim)
1. **Wire `rpc_tls.rs` into `rpc_cluster.rs`** (contributor TLS terminator + coordinator TLS client) so `enable_rpc_tls` actually encrypts the ggml-rpc byte stream; remove `#![allow(dead_code)]` once integrated; add an integration test mirroring `distributed-e2e.yml`.
2. **Close the version-mismatch gap's remaining surface**: characterize the threshold (small models pass despite mismatch) and expose a build-id mismatch as a hard failure/warning on the API surface, not only in discovery logs.
3. **Keep the CI gate green**: ensure `distributed-e2e.yml` runs on every PR touching `rpc_cluster.rs`/`native_engine.rs` (already configured); add an assertion that `real_inference: true` and a non-zero contributor connection count.

### P1 — moat completion
4. **Config hot-reload**: file-watch `ghostlink.toml` safe subset via `notify` crate; reuse `/api/settings` plumbing.
5. **Continuous rebalancing first cut on real serving path**: drain-and-restart affected request when a contributor drops (acceptable first cut per roadmap); detect via `RpcSupervisor`/health, fail in-flight requests cleanly, re-plan on next load.
6. **Cluster-aware HF quant selection**: pass aggregate cluster VRAM (not single-node) into `select_best_gguf_group`.
7. **Plugin marketplace-lite**: `plugins.toml` (mirroring `mcp_servers.toml`) loading `InferenceBackendPlugin` impls by name/URL; document in `CONTRIBUTING.md`.

### P2 — ecosystem / enterprise
8. **Cross-node speculative decoding**: coordinator drafts on fast node, verifies on large node via existing placement layer.
9. **LoRA/adapter support** in native engine path (`--lora` passthrough + API).
10. **OIDC** as additional auth provider (layered, not replacing API keys).
11. **Optional guardrail/policy middleware** (PII redaction, quotas) pluggable, off by default.
12. **Model registry versioning** with pin/rollback over existing HF integration.
13. **Usage attribution/quotas per tenant** (strictly after RBAC/tenancy).

### P3 — horizon 3 (only after P0/P1)
14. WAN relay/rendezvous mode.
15. Third-party planner ABI.
16. First-class multimodal pipeline routing.
17. Optional hosted control plane.

## 7. Open risks still valid

- `ggml-rpc` version mismatch can silently corrupt output (still not fully characterized; threshold unknown). Address before any "production trustworthy" claim.
- RPC byte stream unencrypted by default; `rpc_tls.rs` scaffold only.
- Single-host Docker benchmark cannot prove heterogeneous performance; needs two real machines with `-ngl -1` and a model too large for either node.
- `contribute_compute` blast radius if a hostile device is on the LAN (mitigated by allowlist + HMAC, not by wire encryption).

## 8. Suggested TODO list (copy/paste)

- [x] Wire `rpc_tls.rs` into `rpc_cluster.rs` + coordinator client; remove dead-code allow; add e2e test. (Contributor TLS terminator + coordinator TLS client proxy wired into handle_gui_model_load; dead-code allow removed; InsecureVerifier trust model documented.)
- [x] Expose build-id mismatch as API-level failure/warning; document threshold. (Mismatched peers excluded from distributed RPC; topology endpoint surfaces build_id_status; threshold: models fitting in single-node VRAM auto-disable distributed, so mismatch is harmless for them; require_offload path returns hard error when peers excluded.)
- [x] Add `real_inference` + contributor-connection assertion to `distributed-e2e.yml`. (Already in scripts/rpc_fabric_assert.py steps 5/7.)
- [ ] Implement `notify`-based hot-reload for safe `ghostlink.toml` subset.
- [ ] Implement drain-and-restart rebalancing on real serving path.
- [x] Make HF quant selection cluster-VRAM-aware.
- [ ] Add `plugins.toml` registry + loader.
- [ ] Add cross-node speculative decoding (BLOCKED upstream: llama.cpp has no remote draft-model protocol; local draft + distributed verify already works via GHOSTLINK_DRAFT_MODEL).
- [ ] Add LoRA/adapter passthrough + API.
- [ ] Add OIDC provider (layered).
- [ ] Add pluggable guardrail middleware (off by default).
- [ ] Add model pin/rollback over HF integration.
- [ ] Add per-tenant usage attribution/quotas (after tenancy).
- [ ] Update `docs/ROADMAP.md` status lines per §5.
- [ ] Add "last audited: 2026-10-27 @ 6587719" to "Current truth".

## 9. What was NOT verified

- `cargo build --release -p ghost-link` WAS executed successfully (Windows) after the two fixes in §11; `cargo test` was NOT run.
- No Docker daemon was available in this environment, so no live container scrape/E2E was run.
- Multi-node runtime behavior WAS exercised for the distributed-inference path specifically (see §11): two real machines, real UDP discovery, real `--rpc` load of a 30B model, real chat completion. Other paths (rebalancing, plugin marketplace, HF cluster-aware sizing, OIDC, etc.) remain static/unverified.
- `ghostlink_gui_modern/dist` and `target/` are build artifacts and were not audited.
- Roadmap's throughput claims (1.5–2.5 tok/s etc.) were not re-measured.

## 10. Bottom line

The product is materially further along than `docs/ROADMAP.md`'s status text in several places (audit log rotation, RBAC roles, OTel, Grafana, JS SDK, installers, task fan-out, dynamic timeout, child supervision), and materially behind it in a few that matter most for the "production trustworthy" claim — **wire-level RPC encryption, real-path rebalancing, plugin marketplace, cluster-aware model sizing, and cross-node speculative decoding**. The single highest-leverage next action is wiring the already-scaffolded `rpc_tls.rs` into the live RPC path, because every other enterprise-trust item assumes confidential node-to-node transport.
---

## 11. Runtime verification addendum (2026-10-27, P0 exercise)

Unlike §9 (which states no multi-node runtime behavior was exercised), this addendum records a **live two-machine distributed-inference run** performed during this audit pass.

### Hardware / roles

- **Coordinator (server + model host):** Desk-Mini, `10.0.0.29`, Alder Lake-N UHD Graphics (iGPU, 0 GB VRAM reported), 14.9 GB system RAM, Linux. `distributed_inference: true`, `contribute_compute: false`.
- **Contributor:** Windows host `SOVEREIGN1`, `10.0.0.87`, AMD Radeon 860M (0.5 GB VRAM reported), 31.1 GB system RAM. `contribute_compute: true`, `rpc_allowed_peers: ["10.0.0.29"]`, `rpc_shared_secret` set (64 hex chars).

### What was verified live

1. UDP/mDNS discovery: coordinator discovered `SOVEREIGN1` with `contribute_compute: true`, `rpc_port: 50052`, role `contributor`.
2. RPC admission handshake: `admit_via_secret` (auth port `rpc_port + 2000` = 52052) returned ack `\x01` with the correct secret; wrong/missing secret rejected.
3. Distributed model load: coordinator launched `llama-server` with `--rpc 10.0.0.87:50052 -ts 7.4307,0.5000` (auto-derived, zero manual flags) and loaded `Qwen3-30B-A3B-Q4_K_M` (17.28 GB) that cannot load on the coordinator alone.
4. Chat completion through `/v1/chat/completions` returned coherent text with `real_inference: true` and `backend: native`.
5. Build-mismatch policy: with `rpc_allow_unknown_build_id: true`, peer proceeded despite missing build fingerprint (both sides report no parseable build hash).

### Defects found and fixed during the exercise (code changes on `main` working tree)

1. **Contributor advertisement race (main.rs).** `ensure_contributing()` spawns `ggml-rpc-server` asynchronously, but `advertised_node` evaluated `is_contributing_healthy()` immediately afterward, so the child had not bound its port yet and the node broadcast *without* an `rpc_port` forever. Fixed by polling `is_contributing_healthy()` (600 ms × 10) before evaluating `advertised_node`. Without this, no multi-machine distributed inference was reachable even with correct settings.
2. **Admission TTL too short (rpc_cluster.rs).** `RPC_ADMISSION_TTL` was 30 s. Distributed model loads for 30B-class models take 60–120 s; the proxy splices the `--rpc` connection at load start, but a 30 s TTL could expire mid-load and abort/hang the load. Raised to 300 s. (Security trade-off: longer admission window; still bounded, and the handshake remains per-IP.)

### Defects NOT fixed here (remain in §7 open risks)

- Transport-layer encryption of the raw `ggml-rpc` byte stream remains unimplemented; `rpc_tls.rs` is still `#![allow(dead_code)]` and unwired.
- Version-mismatch threshold still uncharacterized; only a build-fingerprint handshake (now exercised) gates peers.
- `contribute_compute` blast radius if a hostile device is on an allowlisted subnet (mitigated by IP allowlist + HMAC, not wire encryption).
- Contributor `ggml-rpc-server` has no native auth (upstream limitation; Ghostlink's proxy is the auth boundary).

### Net effect on audit conclusions

- §3/§4 "verified shipped" claims for distributed capacity splitting, contributor supervision, version-mismatch rejection, and RPC admission are now **runtime-verified on two separate physical machines**, not just static. The "no multi-node runtime behavior was exercised" caveat in §9 is partially lifted for the distributed-inference path specifically.
- The two concrete fixes above (health-wait + TTL) are small, scoped, and do not change default single-node behavior (`distributed_inference` still defaults false; `contribute_compute` still defaults false).
- Recommended additional TODO: add a regression test that starts two real `ghost-link` processes and asserts `real_inference: true` plus a non-zero contributor connection count (already tracked as H1 #1; now has a reproducible two-machine proof to build on).

### Files changed in this pass

- `crates/ghost-link/src/main.rs` (+15 lines): health-wait before contributor advertisement.
- `crates/ghost-link/src/rpc_cluster.rs` (+1/-1): `RPC_ADMISSION_TTL` 30 s → 300 s.
