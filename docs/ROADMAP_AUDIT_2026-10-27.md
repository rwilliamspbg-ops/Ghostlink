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

- [ ] Wire `rpc_tls.rs` into `rpc_cluster.rs` + coordinator client; remove dead-code allow; add e2e test.
- [ ] Expose build-id mismatch as API-level failure/warning; document threshold.
- [ ] Add `real_inference` + contributor-connection assertion to `distributed-e2e.yml`.
- [ ] Implement `notify`-based hot-reload for safe `ghostlink.toml` subset.
- [ ] Implement drain-and-restart rebalancing on real serving path.
- [ ] Make HF quant selection cluster-VRAM-aware.
- [ ] Add `plugins.toml` registry + loader.
- [ ] Add cross-node speculative decoding.
- [ ] Add LoRA/adapter passthrough + API.
- [ ] Add OIDC provider (layered).
- [ ] Add pluggable guardrail middleware (off by default).
- [ ] Add model pin/rollback over HF integration.
- [ ] Add per-tenant usage attribution/quotas (after tenancy).
- [ ] Update `docs/ROADMAP.md` status lines per §5.
- [ ] Add "last audited: 2026-10-27 @ 6587719" to "Current truth".

## 9. What was NOT verified

- No `cargo build`/`cargo test` was executed; module presence was inferred from source, not compilation.
- No Docker daemon was available in this environment (per roadmap's own note), so no live container scrape/E2E was run.
- No multi-node runtime behavior was exercised; findings are static.
- `ghostlink_gui_modern/dist` and `target/` are build artifacts and were not audited.
- Roadmap's throughput claims (1.5–2.5 tok/s etc.) were not re-measured.

## 10. Bottom line

The product is materially further along than `docs/ROADMAP.md`'s status text in several places (audit log rotation, RBAC roles, OTel, Grafana, JS SDK, installers, task fan-out, dynamic timeout, child supervision), and materially behind it in a few that matter most for the "production trustworthy" claim — **wire-level RPC encryption, real-path rebalancing, plugin marketplace, cluster-aware model sizing, and cross-node speculative decoding**. The single highest-leverage next action is wiring the already-scaffolded `rpc_tls.rs` into the live RPC path, because every other enterprise-trust item assumes confidential node-to-node transport.