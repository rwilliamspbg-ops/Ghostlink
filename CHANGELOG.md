# CHANGELOG

All notable changes to Ghostlink Studio are documented here.

---

## [Unreleased] - GUI accessibility, MCP server editor, chat attachments, model-list fixes, iGPU perf tuning

### Added

- **MCP server editor** (`McpTab.tsx`, `crates/ghost-link/src/mcp/{config,registry}.rs`): add/edit/delete MCP servers (stdio or HTTP transport) directly from the GUI instead of hand-editing `mcp_servers.toml`. Changes take effect immediately — connects/disconnects the affected server live via new `McpConfigManager::add/update/remove` + `McpRegistry::add_server/update_server/remove_server`, no restart required. Also wired up the enable/disable toggle switch, which was calling `POST /api/mcp/servers/:name/toggle` — a route that didn't exist; the connect/disconnect logic (`McpRegistry::set_enabled`) was already fully implemented and tested but had no caller.
- **Text-file attachments in chat** (`ChatTab.tsx`): paperclip button + drag-and-drop onto the composer. Scoped to text-based files (code, markdown, JSON, CSV, logs, config — no images/binaries, which are rejected with an explicit toast rather than silently doing nothing) since there's no multimodal/upload endpoint; files are read client-side (256KB/file cap) and inlined as labeled fenced code blocks ahead of the message.
- **`eslint-plugin-jsx-a11y` + axe-core Playwright suite** (`ghostlink_gui_modern/eslint.config.js`, `e2e/accessibility.spec.ts`): first automated accessibility regression gate for the GUI — scans the app shell and every primary tab (plus the new MCP add-server dialog) for WCAG 2 A/AA violations. `color-contrast` is deliberately excluded for now (see Documentation below).

### Fixed

- **Models tab listed the same model twice**: `handle_gui_model_download`'s completion handler (`crates/ghost-link/src/main.rs`) removed only the in-flight placeholder record (keyed by the original request name) before pushing the completed one, so re-downloading an already-installed model left the old record behind instead of replacing it — both records then survived indefinitely with the same name but different sizes. Also removed four hardcoded "Ready" placeholder models (`load_persistent_models`) that were seeded on first run with no real file behind them and never reconciled against a real scanned/downloaded model, since the merge only matched on exact name equality.
- **Delete button missing for native/llama.cpp models**: gated to `currentEngine === 'ollama'` in `ModelsTab.tsx`, even though the backend's `DELETE /api/models/:name` already fully supports removing local GGUF files for any engine. Split into its own `canDeleteModel` flag — shown for native and Ollama, still hidden for vLLM (genuinely server-managed).
- **iGPU VRAM misdetection on the native launch path** (`launch-native.ps1`): `Win32_VideoController.AdapterRAM` (WMI) and DXGI's `DedicatedVideoMemory` both undercount a unified-memory iGPU — neither reads `SharedSystemMemory` — so hardware auto-detection was landing on `native_engine.rs`'s worst-case "<4GB" perf tier regardless of the real hardware. `GHOSTLINK_GPU_NAME`/`GHOSTLINK_VRAM_GB`/`GHOSTLINK_COMPUTE_CAPABILITY` now pinned explicitly (`ghostlink-core::system_profile::detect_gpu_from_env` takes absolute priority over the flawed probes). `VRAM_GB=8` was picked empirically — benchmarked 4 vs 8 on the reference machine (AMD Radeon 860M) with a small (~0.6GB) model; the larger prompt micro-batch it unlocks measured ~2.3x throughput (31.3 → 71.8 tok/s).
- **Full GPU offload duplicates large models in system RAM on an integrated GPU** (`native_engine.rs`, `launch-native.ps1`, `main.rs`): the perf tuning above was validated only against a small model. Live-testing it against the actual 30B-class daily-driver model (13.6GB) surfaced this. "VRAM" on an integrated GPU is the same physical RAM as everything else; llama.cpp's Vulkan backend offloading a layer doesn't move its weights out of system RAM the way it would on a discrete GPU, it *duplicates* them into a separate device-local allocation. A controlled, matched comparison (same model, same prompt, same seed, same direct llama-server timings) at `ngl` 0 / 24 / -1: **8.78 / 8.03 / 16.84 tok/s**, at **0.54GB / 7.35GB / 14.15GB** committed memory, leaving **~18GB / ~11GB / ~0.4GB** free on this 27.6GB host. Partial offload (24 layers) is confirmed not a viable middle ground — same or worse speed than CPU-only, for 13x the memory. Full offload really is ~1.9x faster than CPU-only, at the cost of leaving well under 1GB free system-wide while a model is loaded (reproduced twice).
  `NativeEngineClient::get_ngl()`/`get_ctx_size()` now cap large (≥10GB) models toward CPU-only *by default* when `GHOSTLINK_LLAMA_NGL`/`GHOSTLINK_CTX_SIZE` aren't set — a safety net for hosts that haven't made a measured call either way. This reference machine's `launch-native.ps1` explicitly opts back into full offload (`GHOSTLINK_LLAMA_NGL=-1`) as a deliberate, informed choice given the numbers above, not the default `get_ngl()` would pick on its own.
  Also removed two unconditional overrides that had been masking the model-size-aware capping entirely regardless of which default was wanted: `launch-native.ps1` previously pinned `GHOSTLINK_LLAMA_NGL`/`GHOSTLINK_CTX_SIZE` unconditionally on every launch (now scoped to the deliberate full-offload choice above), and `main.rs` had its own startup auto-config block guessing an `ngl` from VRAM alone before any model was even chosen — removed, since it made itself indistinguishable from a genuine user override and defeated per-model sizing either way.
- **A failed `/api/inference/engines` fetch silently displayed as "you've selected Ollama," disabling tool calling with no error shown** (`api.ts`, `useInferenceEngines.ts`, `types/engines.ts`): both the hook's initial/error state and `api.ts`'s 404 handler fabricated a complete fake "Ollama, active" engine list — the 404 branch didn't even set an `error` field, making it indistinguishable from a real successful response. Any transient failure to reach the backend (a restart, one dropped request, a stale proxy) then looked exactly like a genuine Ollama selection, permanently hiding tool-calling controls. Compounding it, the fabricated data's own hardcoded capability table (`ENGINE_CAPABILITIES` in `types/engines.ts`) was itself wrong for `native` — `tool_calls: false` there vs. `true` from the real backend — so even a user who *did* have native selected could see this if the request ever failed once. Now the hook starts and stays empty/unknown (not a guessed identity) until a real fetch succeeds, preserves the last known-good engine across a later transient failure instead of reverting, and `createInferenceEngineDescriptors`/its capability tables were deleted rather than left as a fallback a future caller could reach for again.

- **`launch.sh`'s own readiness checks always failed with 401, aborting every fresh launch**: the `/api/settings`/`/api/models`/`/api/inference/chat` verification probes added before real bearer-token auth existed (`auth.rs`) never got updated once every route but `/health` started requiring `Authorization: Bearer <token>` — so a correctly-running server always failed its own startup self-check and the script tore everything back down right after reporting success on `/api/health`. Now reads the API key `ghost-link` just persisted (`$PROJECT_ROOT/api_key.txt`, or `GHOSTLINK_API_KEY_PATH` if set) and sends it on those checks. (`launch-native.ps1`'s equivalent check happened to survive this because it treats any code under 500 as "ready.")
- **`launch.sh`'s `detect_gpu()` misclassified GPUs with no cached `lspci` model name as CPU-only** (observed live as `GPU: Device 800e`, wrongly falling to the generic/unknown branch): vendor matching only ever inspected the device/model name field (`lspci -mm`'s 3rd quoted field), never the vendor field (2nd), even though vendor names resolve far more reliably than model names — `pci.ids`' vendor list is small and stable, while its device-model list constantly lags new chip releases, especially on minimal cloud/container images. Now checks both fields, adds an NVIDIA-without-`nvidia-smi` branch (previously silently dropped into "other"), and — using positional field extraction instead of the old `tail -1`, which grabbed the *subsystem* device name instead of the real one on any discrete card that reports subsystem IDs — falls back to printing the raw numeric PCI vendor:device ID (always available via `lspci -n`, independent of `pci.ids`) when even the vendor string can't be resolved, instead of an opaque, undebuggable "Device \<hex\>".
- **`launch.sh`'s ngl/ctx tiering had drifted out of sync with the Rust-side fix** (see PR #264, merged without a changelog entry): independent, hand-duplicated tiering logic defaulted to `ngl=99` (llama.cpp's "offload every layer" sentinel) for any GPU `VRAM_GB` couldn't detect — which is every non-NVIDIA GPU (`rocm-smi`/`lspci`/Metal never populate it) — instead of degrading to CPU-only. Now matches `native_engine.rs::get_ngl()`: unknown VRAM defaults to `ngl=0`, and large (≥10GB) models on the Vulkan backend are capped toward CPU-only by default for the same memory-duplication reason documented above, scoped away from CUDA/ROCm (real discrete VRAM) and left unassumed for Metal.

### Changed

- **Accessibility pass across the GUI**: live-region announcements for streaming chat replies and Metrics tab state changes (throttled to meaningful transitions, not every poll tick, to avoid drowning a screen reader); skip-to-content link and landmark wiring; restored focus rings on the Command Palette input and chat composer (both had stripped the default outline with no replacement); `prefers-reduced-motion` support; every scrollable tab body made keyboard-scrollable (`tabindex`/`role="region"`) — a real WCAG 2.1.1 gap the new axe suite caught that wasn't part of the original ask.
- **Chat markdown readability**: assistant replies bumped from `prose-sm` to base `prose` (14px → 16px) outside Compare Mode; heading sizes tamed to fit a chat bubble instead of a full-width article; GFM tables wrap in a scroll container instead of blowing out bubble/page width; inline `` `code` `` swapped from the default backtick-quote style to a background pill, scoped so it can't also shrink code inside fenced blocks; code block font bumped 12px → 13px.

### Documentation

- Deliberately did **not** fix pervasive `color-contrast` failures the axe suite surfaced (the app's muted `slate-500`/`slate-600`-on-dark text falls short of WCAG AA in dozens of places across every tab) — that's a design-system change affecting the whole app's visual character, not a mechanical accessibility fix, and is out of scope for this change. Excluded from `e2e/accessibility.spec.ts` with a comment explaining why, so the suite still gates real regressions in the meantime.
- Also not addressed here: no user-facing theme/keyboard-shortcut persistence, no MCP config hot-reload for hand-edited `mcp_servers.toml` beyond the enable/disable toggle fixed above.

### Validation

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` — all clean (425 tests across `ghost-link`, `ghostlink-core`, `mcp-calculator`, `mcp-rag`, `mcp-vision`, 0 failures).
- `cargo audit` — 3 pre-existing advisory warnings (`paste`, `anyhow`, `lru`; unmaintained/unsound transitive deps), unrelated to this change, no new findings.
- GUI: `tsc --noEmit` and `eslint .` clean, 142/142 `vitest` unit tests passing, 11/11 new axe accessibility tests passing.
- All of the above verified live against a real running stack (Rust `ghost-link serve` release build + Go control-plane + Vite dev server), not just automated tests: real chat completions with real streamed tokens, real MCP server add/edit/delete/toggle round-trips, real model list deduplication against the actual `models.json` on disk. For the GPU-offload memory finding specifically: a controlled, matched comparison (same model, same prompt, same seed, same measurement method, `ngl` 0/24/-1 tested back-to-back) with real `Get-Process`/`Win32_OperatingSystem` memory readings, not simulated — first pass compared mismatched measurement methods and got the tradeoff wrong (see PR discussion), corrected before merging.

## [1.17.0] - 2026-08-08 (Real distributed-inference testing, three bug fixes, RPC allowlist, install script, JS SDK)

### Added

- **Real E2E CI gate for distributed inference** (`.github/workflows/distributed-e2e.yml`, `Dockerfile.rpc-fabric`, `docker-compose.rpc-fabric.yml`, `scripts/rpc_fabric_assert.py`): a two-container Docker fabric proving Ghostlink's `ggml-rpc`-backed distributed inference actually executes across containers (`real_inference: true`, live RPC connection log evidence), not just that peer discovery found a node count.
- **Real multi-node benchmark harness** (`docker-compose.rpc-fabric-benchmark.yml`, `scripts/rpc_fabric_benchmark.py`), plus extensive real findings from testing on genuinely separate physical hardware documented in `docs/BENCHMARKS.md`: real single-node-vs-distributed throughput comparisons, and — the actual proof this project's roadmap has been chasing — a real 30B-class model that cannot load on one machine alone (`ErrorOutOfDeviceMemory`) loading and serving correctly once split across two real machines.
- **RPC contributor IP allowlist** (`rpc_allowed_peers` setting, `crates/ghost-link/src/rpc_cluster.rs`): `ggml-rpc-server` has no authentication of its own (an upstream llama.cpp limitation); Ghostlink now optionally fronts it with a Ghostlink-controlled TCP proxy that only forwards connections from allowlisted IPs/CIDR ranges. Empty allowlist (the default) is byte-for-byte the old direct-bind behavior — zero overhead, zero change, for anyone not using the feature.
- **Version-mismatch detection for RPC peers** (`rpc_build_id` field on `NodeResources`, carried through all three discovery wire paths — the shared binary encoder, `DiscoveryFrame`'s UDP encoder, and mDNS TXT records): a coordinator now refuses to route distributed inference through a peer running a different `llama.cpp` build, closing a real bug found this session where mismatched builds silently corrupted output on larger models while the API reported healthy throughout. Only excludes on a *confirmed* mismatch — a peer that predates this field is still used, so this rolls out without breaking anyone mid-upgrade.
- **One-line install script** (`scripts/install.sh`, `scripts/install.ps1`): `curl -fsSL .../install.sh | sh` downloads, SHA256-verifies, and installs the real published `ghost-link` release binary — no sudo, no package manager, no Rust toolchain required.
- **JS/TS client SDK** (`sdks/js/`, package `ghostlink-client`): mirrors `sdks/python`'s shape (`chat.completions.create`, real SSE streaming via `/api/inference/chat`, typed error hierarchy), built on native `fetch`/`ReadableStream`, ships ESM + CJS + `.d.ts`.
- **Full per-crate READMEs** for all five workspace crates (`ghost-link`, `ghostlink-core`, `mcp-calculator`, `mcp-rag`, `mcp-vision`) — each `Cargo.toml`'s `readme` field now points at its own crate's README instead of the repo-wide root README.

### Fixed

- **Silent output corruption from version-mismatched `ggml-rpc` peers** — see "Added" above; this is the fix, `rpc_build_id` detection is the mechanism.
- **Unsupervised RPC contributor child process**: `rpc_cluster::ensure_contributing()` already had working respawn logic but was only ever called once at server startup — if the spawned `ggml-rpc-server` child later crashed (e.g. the quantized-KV-cache/RPC-CPU-backend crash found this session), the node kept advertising RPC capability via discovery while actually unreachable. Now called every 30s on a background thread for the process lifetime whenever `contribute_compute` is on.
- **90-second model-ready timeout too short for real distributed loads**: `native_engine.rs` used a flat 90s health-check budget for both single-node and distributed loads. Real distributed loads measured this session took anywhere from 168s to over 900s depending on model size, all previously aborted as false failures. Now scales to 600s specifically when a load attempt's args include `--rpc`, stays at 90s for single-node; `GHOSTLINK_MODEL_READY_TIMEOUT_SECS` env override for further tuning.

### Changed

- `ghost-link` and `ghostlink-core` bumped `1.16.1` → `1.17.0` (new backward-compatible settings/protocol fields, no breaking changes — a minor bump per semver). `ghostlink_gui_modern`'s `package.json` bumped to match, keeping the whole repo on one coordinated version number.

### Documentation

- `docs/ROADMAP.md` and `docs/BENCHMARKS.md` updated extensively with the real findings above — hardware tables, methodology, honest caveats about what wasn't yet proven (e.g. "usable speed" for the 30B distributed result is not yet there, even though the capacity proof is real).

### Validation

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` — all clean (164 ghost-link + 183 ghostlink-core tests, 0 failures).
- Real Docker E2E fabric rebuilt and rerun after the allowlist change, confirming zero regression to the existing passing gate.
- JS SDK: `tsc --noEmit` clean, real `tsup` build (ESM + CJS + `.d.ts`), 17/17 `vitest` tests passing.
- Install scripts: both actually run end-to-end against the real live `v1.16.1` release (not just syntax-checked) — real binary downloaded, checksum verified against the published `SHA256SUMS`, installed binary executed successfully.

## [1.16.1] - 2026-08-05 (CI fix: release-artifacts.yml release build)

### Fixed

- `release-artifacts.yml`'s "Run release validation gates" step runs `cd control-plane && go test ./...` but never installed a Go toolchain first (unlike `ci.yml`'s Go job, which does). This only surfaced when the `v1.16.0` tag push exercised the workflow for real for the first time — it only triggers on `push: tags: v*`, so no PR check had ever run it. Both matrix legs failed: `macos-latest` with `go: command not found` (no Go on that runner image at all), `windows-latest` with a transient TLS handshake timeout fetching a Go module (plausible without a real `setup-go` step warming the module cache). Fixed by adding `actions/setup-go@v7` with `go-version: stable`, matching the already-working pattern in `ci.yml`.
- No functional code changes — this release exists solely to get `v1.16.0`'s actual content (see below) published with working release binaries. `v1.16.0` itself published successfully to crates.io; it just never got a GitHub Release with binaries attached.

This patch is cut from the commit immediately after the CI fix landed on `main`, before later unrelated work (an LLM-shaped benchmarking suite) merged — it carries `v1.16.0`'s code unchanged plus only this workflow fix, not that follow-on feature work.

---

## [1.16.0] - 2026-08-05 (Reliability fixes: GPU probe timeout, TCP circuit breaker, model-list caching)

### Fixed

- GPU hardware detection (`system_profile.rs`) had a probe-timeout regression: each `probe_*_with_timeout` wrapper unconditionally slept the full timeout duration before checking whether the probe had already finished, so every startup paid the full 5-10s per probe instead of returning as soon as the fast path completed. Replaced with a real bounded wait (detached thread + `mpsc::recv_timeout`) that returns immediately on completion and only blocks up to the timeout on a genuinely slow/hung probe. Full profile detection now completes in ~1.5s on a typical dev machine instead of a guaranteed multi-second floor.
- GUI production build (`npm run build`) was broken: `vite-plugin-monaco-editor-esm`'s built-in worker entries hardcode `monaco-editor/esm/vs/...` paths that, against `monaco-editor`'s current package.json `"exports"` map, resolve to a doubled `esm/vs/esm/vs/...` path that doesn't exist ("Could not resolve"). Reconfigured [ghostlink_gui_modern/vite.config.ts](ghostlink_gui_modern/vite.config.ts) to supply the same 5 workers (editor core, CSS, HTML, JSON, TypeScript) via `customWorkers` with the prefix stripped, which the exports map re-adds correctly. Affects both `npm run build` and `npm run dev`; this was blocking the `release-artifacts.yml` CI gate outright.

### Added

- `circuit_breaker` module in [crates/ghostlink-core/src/circuit_breaker.rs](crates/ghostlink-core/src/circuit_breaker.rs): a 3-state (Closed/Open/Half-Open) circuit breaker with jittered exponential backoff. Wired into the TCP transport bridge's reconnect loop ([crates/ghostlink-core/src/runtime.rs](crates/ghostlink-core/src/runtime.rs) `spawn_tcp_bridge`) via a new per-node breaker registry on `ClusterState` (`circuit_breaker_for`) — failure history now persists *across* pipeline executions targeting the same remote node, so a chronically-unreachable node fails fast on later calls instead of repeating the full connect/backoff sequence every time. Opt-in per call site (`Option<CircuitBreaker>`); the loopback benchmarking path passes `None` and is unaffected.
- `api_response_cache` module in [crates/ghostlink-core/src/api_response_cache.rs](crates/ghostlink-core/src/api_response_cache.rs): a TTL + ETag response cache. Wired into `GET /api/models`, which previously ran a real `fs::read_dir`/`fs::metadata` disk scan on every request — now cached for 5s and explicitly invalidated the moment a download completes or a model is deleted.
- `LayerKvCache::write_kv_batch` in [crates/ghostlink-core/src/kv_cache.rs](crates/ghostlink-core/src/kv_cache.rs): writes multiple tokens' KV entries under a single write-lock acquisition, validating every entry upfront so a bad entry fails the whole batch atomically. Available as a primitive; like the rest of `kv_cache.rs`, it has no current caller in `runtime.rs` (Ghostlink delegates model execution to an external inference engine).

### Removed

- Three modules from an in-progress performance pass didn't hold up under review and were cut before landing: a churn-coalescing module that duplicated `ClusterState`'s existing lock-free snapshot cache, an MCP "server pool" that pooled against a per-call subprocess-spawn cost the real MCP client (`mcp/registry.rs`, persistent connections) doesn't have, and a protocol buffer pool targeting `DiscoveryFrame::encode()`, which already encodes into a stack buffer on a low-frequency discovery path.

### Validation

- `cargo fmt --all --check` — OK
- `cargo clippy --workspace --all-targets -- -D warnings` — OK
- `cargo test --workspace` — OK
- `cd control-plane && go test ./...` — OK
- `cd ghostlink_gui_modern && npm run test` — OK (14 files, 142 tests)
- `cd ghostlink_gui_modern && npm run build` — OK

---

## [1.3.2] - 2026-08-02 (vLLM Integration & Release Packaging Readiness)

### Added

- New inference engine abstraction in [crates/ghost-link/src/inference_engine.rs](crates/ghost-link/src/inference_engine.rs) with capability descriptors for `ollama`, `native`, and `vllm`.
- New vLLM client in [crates/ghost-link/src/vllm.rs](crates/ghost-link/src/vllm.rs) for:
  - health probing
  - model listing
  - chat-completion generation via OpenAI-compatible endpoints
- New GUI/backend routes for engine and observability workflows:
  - `/api/inference/engines`
  - `/api/vllm/health`
  - `/api/vllm/models`
  - `/api/cluster/topology`
  - `/api/metrics/history`

### Changed

- Backend inference selection now supports `GHOSTLINK_INFERENCE_BACKEND=vllm` in addition to `ollama|native`.
- Runtime settings now include vLLM connection fields (`vllm_base_url`, `vllm_api_key`) and propagate them through runtime updates.
- Control-plane endpoints can now enforce bearer-token auth when control-plane/discovery auth tokens are configured.
- Release CI workflow in [.github/workflows/release-artifacts.yml](.github/workflows/release-artifacts.yml) now:
  - installs Node.js dependencies for the GUI
  - executes Rust, Go, and GUI validation gates before packaging
- Release bundling script in [scripts/release_bundle.sh](scripts/release_bundle.sh) now:
  - validates Node.js toolchain presence
  - builds and packages GUI dist artifacts
  - generates SHA256 checksums across bundled files

### GUI UX and Test Coverage

- Added capability-aware engine UI behavior across:
  - [ghostlink_gui_modern/src/components/SettingsTab.tsx](ghostlink_gui_modern/src/components/SettingsTab.tsx)
  - [ghostlink_gui_modern/src/components/ModelsTab.tsx](ghostlink_gui_modern/src/components/ModelsTab.tsx)
  - [ghostlink_gui_modern/src/components/ChatTab.tsx](ghostlink_gui_modern/src/components/ChatTab.tsx)
- Added metrics history visualizations and topology inspection in:
  - [ghostlink_gui_modern/src/components/MetricsTab.tsx](ghostlink_gui_modern/src/components/MetricsTab.tsx)
  - [ghostlink_gui_modern/src/components/WorkersTab.tsx](ghostlink_gui_modern/src/components/WorkersTab.tsx)
- Added/updated tests for engine capabilities, vLLM flows, topology/metrics history, and control-plane auth behaviors.

### Validation

- `cargo fmt --all --check` — OK
- `cargo clippy --workspace --all-targets -- -D warnings` — OK
- `cargo test --workspace` — OK
- `cd control-plane && go test ./...` — OK
- `cd ghostlink_gui_modern && npm run test -- --reporter=basic` — OK (11 files, 113 tests)
- `cd ghostlink_gui_modern && npm run build` — OK

### Release Packaging and Signing

- Unsigned and signed bundle paths validated through [scripts/release_bundle.sh](scripts/release_bundle.sh).
- GPG signing key generated in the development environment and used to produce checksum signature artifacts (`SHA256SUMS.asc`).

## [1.16.0] - 2026-07-30 (Editor tab: in-GUI code editor + copilot features)

Ghostlink Studio was chat-only — code blocks rendered as read-only Markdown,
with no way to browse, open, or edit a real project file from the GUI, and no
diff-preview step before an AI-proposed change touched disk. This release
adds a Monaco-based Editor tab wired directly into the existing chat/MCP
infrastructure to close that gap.

### ✨ Features

- **Editor tab** (`ghostlink_gui_modern/src/components/EditorTab.tsx`): a
  Monaco editor over three new backend routes —
  `GET /api/workspace/tree`, `GET`/`PUT /api/workspace/file` — confined to a
  canonicalized workspace root (`GHOSTLINK_WORKSPACE_ROOT`, defaults to the
  launch directory) with a path-traversal guard verified against real `../`
  escape attempts on read, tree, and write. Distinct from the sandboxed
  `file_operations` MCP tool: this is the GUI talking to real project files
  directly, not a model-invoked tool call.
- **Explain / Fix / Refactor** — scoped to the current selection or the
  whole file. Fix/Refactor render their proposed change as a side-by-side
  `DiffEditor` with explicit Accept/Reject; nothing is written until
  accepted.
- **Multi-file refactor** — select several files via tree checkboxes, send
  them in one prompt (`### FILE: <path>` sections), then step through each
  proposed change individually (Accept/Reject/Skip).
- **Ghost-text autocomplete** (opt-in toggle) — Monaco's native
  inline-completions provider, debounced against the same chat-completion
  endpoint. Explicitly an MVP: no fill-in-the-middle model support, no
  suffix awareness — continuation-only, and a real network round trip per
  suggestion rather than a fast local model.
- **Repo-aware chat context**: `POST /api/workspace/index` walks the
  workspace (skipping `node_modules`/`target`/`.git`/etc., capped at 400
  files / 4MB) and feeds eligible text files into the `rag` MCP server's
  `index_document` tool directly — not through an LLM tool-calling loop,
  which would be slow and unreliable for bulk indexing. The Editor tab
  triggers this once per page load; a `"skipped"` (not error) status is the
  expected outcome when `rag`/Ollama isn't reachable, verified by probing
  Ollama's `/api/tags` before the indexing loop rather than trusting MCP
  connection state (`rag`'s own handshake never touches Ollama, so it
  reports "connected" even with Ollama down).
- **`rag` MCP server enabled by default** (`mcp_servers.example.toml`) — was
  disabled out of the box; needs `ollama pull nomic-embed-text` (or another
  embedding model via `OLLAMA_EMBED_MODEL`) to actually do anything, and
  degrades to the `"skipped"` status above otherwise.
- **Real security audit log** (`/api/security/audit-log`): was a hardcoded
  stub that always returned an empty list. Now records failed auth attempts,
  JWT refresh, PQC/TLS enable, and tool-call approve/deny decisions
  in-memory, capped at 500 entries, most-recent-first — verified live
  through the Security tab (triggered a JWT refresh, watched the real entry
  appear).

### 🐛 Fixed

- **`mcp-rag`'s `index_document` only ever appended chunks, never removed a
  document's prior ones.** Re-indexing the same file (which the Editor tab's
  auto-index does on every page load) silently grew `rag_index.json` with
  duplicate chunks forever, and `search()` would return multiple stale
  copies of the same source. `index_document` now replaces a document's
  existing chunks by id prefix before inserting the new ones. Verified live:
  indexed a directory (10 chunks), re-indexed the same files, still exactly
  10.

### 📚 Documentation

- `README.md`: new "Editor Tab & Copilot Features" section, updated API
  endpoint table (`/api/workspace/*`, corrected `/api/security/audit-log`
  description), updated MCP tools table (`rag` now enabled by default).

---

## [1.15.1] - 2026-07-28 (Release workflow fix)

- **`release-artifacts.yml`'s "Build release bundle" step was missing an
  explicit `shell: bash`.** On `ubuntu-latest`/`macos-latest` that default
  shell already is bash, so it went unnoticed there — but `windows-latest`
  defaults to PowerShell, which fails immediately on the step's bash `[[
  ]]` syntax. v1.15.0's release shipped with Linux and macOS binaries only;
  this release adds the missing Windows binary under a clean version tag
  rather than rewriting v1.15.0's already-published release.

---

## [1.15.0] - 2026-07-28 (Real distributed inference via llama.cpp RPC backend)

Closes the gap between what Ghostlink's clustering claimed to do and what
`/v1/chat/completions` actually executed: peer discovery and a distributed
*planning/benchmark* engine existed, but no request path ever ran a model
split across more than one machine. Verified before writing any integration
code that the existing `ghost-link flow`/`stage-worker` pipeline moves
synthetic benchmark payloads, not real model layers — so this uses
llama.cpp's own RPC backend (`ggml-rpc`) instead, which does real
cross-process tensor execution.

### ✨ Features

- **Real distributed inference** (`ghost-link::rpc_cluster`): a node opts in
  to contributing compute (`contribute_compute` + `rpc_port` in settings)
  and runs `ggml-rpc-server`, exposing its GPU/CPU over TCP. A node serving
  a request (`distributed_inference: true`) discovers healthy
  RPC-contributing peers from live cluster state, computes a
  VRAM-proportional `--tensor-split`, and launches its local `llama-server`
  with `--rpc`/`-ts` — zero manual flags from the operator. Off by default;
  single-node deployments see no behavior change. Verified live: a model
  forced entirely onto a second process's device via `-ts 0,1` produced
  real generated text, and two full `ghost-link serve` processes with real
  UDP discovery between them auto-negotiated the RPC args end to end.
- **`NodeResources.rpc_port`**: UDP discovery frames and mDNS TXT records
  now carry each node's RPC-contribution port, so peers can be selected for
  distributed inference without any manual configuration.

### 🐛 Fixed

- **Every `ghost-link serve` instance previously hardcoded its cluster node
  id to the literal string `"studio-api"`, regardless of machine.** Two
  real Ghostlink installs on two real machines would collide in
  `ClusterState`'s id-keyed map — meaning no distributed feature (old or
  new, UDP or mDNS) ever worked across genuinely separate hardware,
  independent of this release. Now derived from the hostname
  (`GHOSTLINK_NODE_ID` env var to override).
- **`DiscoveryFrame::encode()`** — the function UDP discovery actually
  calls — is a separate, hand-duplicated serializer from
  `NodeResources::encode_payload_into` (kept for a zero-copy calling
  convention), discovered mid-implementation to silently drop the new
  `rpc_port` field entirely. mDNS discovery (which reuses the shared
  encoder) carried it correctly the whole time; UDP discovery didn't, and
  because UDP is tried first and wins ties in `/api/workers/discover`'s
  merge, its `None` silently shadowed mDNS's correct value.

### 📚 Documentation

- `docs/ROADMAP.md` documents the full investigation, what was originally
  planned versus what actually shipped and why, and the verification
  performed at each step.

---

## [1.14.0] - 2026-07-28 (mDNS discovery, custom backend plugins, Python SDK)

A review pass over the project surfaced a punch list of usability, performance,
and extensibility gaps. This release closes the largest items.

### ✨ Features

- **mDNS peer discovery** (`ghostlink-core::mdns`), alongside the existing UDP
  broadcast fallback (`discovery.rs`) — for networks (managed VLANs, cloud
  VPCs) that filter broadcast traffic but still carry multicast. The server
  advertises itself under `_ghostlink._tcp.local.` at startup, and
  `GET /api/workers/discover` now runs UDP broadcast and mDNS browsing
  concurrently, merging results by node id.
- **Custom inference backend plugins** (`ghost-link::backend_plugin`): an
  object-safe `InferenceBackendPlugin` trait + registry, checked by
  `/v1/chat/completions` and `/v1/completions` *before* the existing
  Native/Ollama dispatch (left unmodified) — adding a backend needs no core
  dispatch changes, just an implementation of the trait registered by name.
  Ships a reference `OpenAiCompatPlugin` that forwards to any
  OpenAI-compatible server (vLLM, LM Studio, a hosted API, ...), auto-registered
  via `GHOSTLINK_OPENAI_COMPAT_BASE_URL` (optionally
  `GHOSTLINK_OPENAI_COMPAT_NAME`, `GHOSTLINK_OPENAI_COMPAT_API_KEY`).
- **Python client SDK** (`sdks/python`, package `ghostlink-client`): wraps the
  OpenAI-compatible endpoints (`chat.completions`, `completions`, `embeddings`,
  `models`) plus Ghostlink-native `workers`/`sessions`/`settings`, JSON and
  Prometheus metrics, and real token-by-token streaming chat via
  `stream_chat()` against `/api/inference/chat`'s SSE stream — the only
  endpoint with genuine incremental streaming today.
- **Prometheus `/metrics` endpoint**, alongside the existing JSON
  `/api/metrics` — same underlying snapshot (throughput, CPU/GPU/memory,
  latency percentiles, cluster node count, VRAM, uptime), reformatted for a
  Prometheus scrape config instead of the GUI's polling loop.
- **Per-IP request rate limiting** (`tower_governor`) on the API server,
  applied as the outermost layer so it gates requests before CORS/auth do any
  work.
- **Release artifacts now build on Linux, Windows, and macOS** (previously
  Linux-only), each producing its own binary + checksum; SBOM and provenance
  attestation remain Linux-only.

### 🔒 Security / Hardening

- **`/v1/chat/completions` and `/v1/completions` now validate input**: empty
  `messages`/`prompt`, oversized prompts (>200k chars), and embedded non-
  whitespace control characters are rejected with a 400 before reaching the
  backend. `temperature`/`top_p`/`top_k`/`penalty` are now clamped to sane
  ranges, extending the pre-existing `max_tokens` clamp.
- **`ghostlink.toml`'s `[flow]`/`[cluster_start]`/`[discovery]`/`[tcp]`/`[gui]`
  sections and `[compute]` now reject unknown keys** (`deny_unknown_fields`)
  instead of silently no-op'ing a typo'd setting.

### 📚 Documentation

- Un-archived and expanded the platform comparison sheet
  (`docs/COMPARISON.md`) with Ollama, LM Studio, llama.cpp server, OpenWebUI,
  and Kubernetes-based setups; linked from the README.
- Added a request/cluster-flow architecture diagram to `docs/ARCHITECTURE.md`.

---

## [1.13.0] - 2026-07-26 (Tool-call context overflow fix)

Found in the wild: a `fetch` tool call that pulled an entire webpage (site
nav, a trivia quiz, promoted-songs list, footer — none of it relevant)
got folded straight into the prompt with no size limit, pushing a single
chat turn over the model's context window and failing outright with
`llama_server request failed with status 400 Bad Request:
exceed_context_size_error`.

### 🐛 Fixed

- **Tool observations are now capped at 4000 characters** before being
  folded back into the prompt (`mcp::toolcall::format_observation`), with
  a `[truncated, N more characters omitted]` marker so the model (and
  anyone reading the transcript) knows content is missing rather than
  silently seeing a shortened result as complete. This bounds the damage
  any single tool call can do to the context budget, independent of how
  `--ctx-size` is configured.

### ✨ Changed

- **Default context size (`-c`) doubled across every VRAM tier** in
  `native_engine::get_ctx_size` — 8192→16384→32768 for 8/12/16GB+ (was
  4096→8192→16384), floor raised 2048→4096 for <8GB, and the
  no-VRAM-info fallback raised 4096→8192. The previous defaults were
  tight enough that ordinary tool-calling chat (system prompt + a few
  turns + one tool observation) could approach the ceiling even without
  the truncation bug above. `GHOSTLINK_CTX_SIZE` still overrides directly
  if you want a different value.
- `RuntimeSettings::DEFAULT_CTX_SIZE` (the GUI's own conversation-budget
  default, separate from the value above) raised 4096→8192 to match, so
  the two don't drift out of sync.

---

## [1.12.0] - 2026-07-26 (Real HTTP/SSE MCP transport)

Closes the other stub found while auditing the codebase for leftover
placeholders: `McpTransport::Http` was a real, user-configurable entry in
`mcp_servers.toml`'s schema, but connecting to one always failed with
`"HTTP/SSE transport is not implemented yet"` — the config accepted it,
the runtime never delivered it.

### ✨ Features

- **Real streamable HTTP/SSE MCP transport**, built on `rmcp`'s own
  `StreamableHttpClientTransport` (the same SDK already used for the stdio
  transport) — connecting to a remote MCP server over a URL now actually
  works, instead of erroring at connect time regardless of config.
- **`${VAR_NAME}` header resolution**, matching the existing stdio `env`
  behavior: header values written as `"${VAR_NAME}"` are resolved from the
  host process environment at connect time, never stored as literal
  secrets in `mcp_servers.toml`.

### 🐛 Fixed

- **Literal-secret validation now covers HTTP headers, not just stdio env
  vars.** `McpConfigManager::save` rejected a literal-looking secret in a
  stdio server's `env` map, but the same check never ran against an HTTP
  server's `headers` map — meaning the one MCP transport where a real
  bearer token or API key is the *normal* case for a header value had no
  guard against saving it in plaintext. Both transports now go through the
  same rejection.

---

## [1.11.0] - 2026-07-26 (Real bearer-token auth + PQC-hybrid TLS)

Closes the last item from a gap analysis against LM Studio/vLLM: the API
server had no authentication anywhere, and the existing `/api/security/*`
endpoints were fully mocked — `handle_gui_jwt_refresh` always returned a
hardcoded `"new-token-123"`, and the PQC endpoints always reported
`enabled: true` regardless of anything. Both are now real.

### ✨ Features

- **Real bearer-token auth on every route but `/health`.** A 256-bit API
  key is generated once on first run, persisted to `api_key.txt`, and
  printed to the console — the only way to learn it, since it's never
  returned by any API response. Send it directly as
  `Authorization: Bearer <key>`, or exchange it for a short-lived JWT via
  `POST /api/security/jwt/refresh` (`jsonwebtoken`, HS256, signed with the
  same key — genuine issuance/verification, not the old stub).
- **Real HTTPS with a genuine PQC-hybrid (X25519MLKEM768) key exchange
  preference**, via `rustls`'s `prefer-post-quantum` feature (aws-lc-rs
  backend) — the same mechanism Chrome/Cloudflare/AWS use today, not a
  bespoke handshake. Opt-in via a new `parallel_slots`-style
  `enable_tls` setting, off by default for today's plain-localhost dev
  flow, forced on when the server binds a non-loopback address (the
  LAN/remote scenario this actually protects). A self-signed cert is
  generated once via `rcgen` and reused across restarts.
- **`/api/security/pqc/state` and `pqc/enable` are now real**: `state`
  reports whether *this running process's* listener is actually serving
  HTTPS (not the persisted setting, which only applies on next restart —
  tracked separately so the two can't be conflated); `enable` writes the
  setting and honestly says a restart is required rather than pretending
  it's already live.
- **Go control-plane gateway now verifies the same shared secret** before
  proxying — real JWT signature verification (`golang-jwt/jwt/v5`), not a
  shape-only check, so it doesn't reject legitimate short-lived tokens the
  GUI uses. Degrades gracefully (no extra edge rejection, not a lockout)
  if the key file isn't readable — the proxy already forwarded
  `Authorization` through to ghost-link's own auth either way.
- **GUI now sends the token on every request** — an axios interceptor plus
  the two hand-rolled `fetch` calls that bypassed it, reading a key the
  user pastes into a new "API Key" field on the Security tab (persisted to
  `localStorage`). The PQC panel's copy was also corrected — it previously
  claimed "Kyber-768/Dilithium... across all distributed nodes" and
  "AES-GCM 256-bit encryption" when disabled, neither of which was ever
  real; it now accurately describes the actual TLS/PQC-hybrid mechanism
  and states plainly that disabled means unencrypted plain HTTP.

### 🐛 Fixed

- The API key would only ever have been generated (and its one-time
  console banner printed) lazily on first *authenticated* request — a
  fresh install with zero traffic yet would have had no way to discover
  it at all. Now generated eagerly at server startup.

### ✅ Validation

- **Live, end-to-end manual verification** (not just unit tests): started
  a real server, confirmed 401 with no token, 200 with the raw key, a real
  JWT round-trip (issue → use → success), and real `enabled:false` →
  `enable` → restart → `enabled:true` PQC state transitions. Independently
  proved the PQC claim using `openssl s_client -tls1_3 -groups
  X25519MLKEM768` against the running HTTPS listener — output confirmed
  `Negotiated TLS1.3 group: X25519MLKEM768`, and a normal client with no
  forced group still connected fine (not a hard requirement, a
  preference).
- New Rust tests: `auth.rs` (key generation/persistence, bearer
  verification, tampered/garbage rejection), `tls.rs` (loopback
  detection, idempotent cert generation with real file I/O).
- New Go tests: `pkg/auth` (key loading, bearer/JWT verification including
  a genuinely expired and a genuinely tampered token, full middleware
  integration via `httptest`).
- New frontend tests: `SecurityTab.test.tsx` (API key persistence, and
  that enabling PQC shows a real "restart required" message rather than
  falsely claiming it's already active).
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo test --workspace` all green. `go build`, `go vet`,
  `go test ./...` all green. `tsc --noEmit`, `vitest run` (118 passed)
  clean.

## [1.10.0] - 2026-07-26 (Real request batching and multi-turn context reuse)

Closes the top item from a gap analysis against LM Studio/vLLM: Ghostlink
could only ever process one generation at a time (`llama-server` spawned
with `-np` hardcoded to `1`), and every conversation turn reprocessed the
full prior transcript from scratch instead of reusing llama-server's own
KV cache.

### ✨ Features

- **Configurable parallel inference slots.** New `parallel_slots` setting
  (Settings tab → Inference Parameters → "Parallel Slots") replaces the
  hardcoded `-np 1` passed to `llama-server`; raising it also adds
  `--cont-batching` so llama-server actually interleaves concurrent
  generations instead of just accepting more connections. Defaults to `1`
  — today's exact prior behavior — until changed.
- **Real admission control**, not just a bigger `-np`: `RequestTracker`
  (`runtime_switcher.rs`) gained a `tokio::sync::Semaphore` sized to
  `parallel_slots`, acquired before every real call into llama-server
  across all four chat-completion code paths (`/v1/chat/completions`, GUI
  chat streaming and non-streaming, tool-confirm). Requests beyond
  capacity wait for a free slot instead of firing unbounded concurrent
  HTTP calls at a server that may only have one real slot.
- **`/api/queue` reports a real depth** instead of a hardcoded
  `{"depth": 0}` — derived from admitted-but-not-yet-slotted requests, not
  an estimate.
- **Multi-turn context reuse**: the GUI's chat path now passes `id_slot`
  and `cache_prompt: true` on every generation request, so repeat turns in
  the same conversation reuse llama-server's existing KV state for the
  common prefix instead of reprocessing the whole transcript every call.
  The stateless `/v1/chat/completions` REST endpoint is unchanged — no
  session to pin a slot to, so no slot reuse there, matching prior
  behavior exactly.

### ✅ Validation

- New tests: `native_engine`'s `get_parallel_slots` env/clamp behavior; a
  real-socket test (`generate_sends_the_requested_id_slot_and_cache_prompt_to_llama_server`,
  a raw `TcpListener` capturing the actual outgoing HTTP body — no mocking
  library) proving `id_slot`/`cache_prompt` genuinely reach the request;
  `runtime_switcher`'s new semaphore tests proving a second concurrent
  `acquire_slot` on a 1-slot tracker really blocks (via a timeout race,
  not just call-count assertions), plus resize and queue-depth coverage.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo test --workspace` all green (138 ghostlink-core + 106
  ghost-link unit + 1 integration + other crates). `tsc --noEmit` and
  `vitest run` (114 passed) clean for the new Settings field.

## [1.9.0] - 2026-07-26 (Multi-node: real cross-process execution, replacing the fabricated flow demo)

### 🐛 Fixed (fabrication)

- **`ghost-link flow` never actually reached a second machine, even when
  given a `--remote-addr`-shaped setup** (it had no such flag at all). It
  registered a fake "remote" node and hand-seeded its metrics
  (`record_latency(3.2)`, `record_delivery_ratio(0.95)`) — the entire
  "distributed" demo ran in one process. `docker-compose.test-fabric.yml`
  stood up real worker containers whose IPs were never dialed.

### ✨ Features

- **New `ghost-link stage-worker --bind <addr>` process**: binds a TCP
  listener, accepts exactly one coordinator connection, reads a handshake
  describing its assigned stage, then loops real batch exchange
  (`read_transport_batch` → compute → `write_transport_batch`) until the
  coordinator disconnects — a genuine one-shot worker, not a simulation.
- **New `flow --remote-addr <host:port>` flag**: when given, the
  coordinator does a real outbound `TcpStream::connect` to a running
  `stage-worker` and executes that node's stage(s) across the real
  socket, deriving the remote node's health metrics from the actual
  measured round-trip time instead of placeholder constants. Because real
  layer assignment splits one node's range into multiple raw pipeline
  stages (verified: 60 layers / 2 nodes → 11 raw stages), a new
  `merge_stages_for_node` helper collapses all of a node's stages into one
  logical placement before executing it remotely.
  Omitting `--remote-addr` keeps today's single-process behavior exactly
  as before, but now prints `SIMULATED execution: ...` instead of
  silently implying a second machine was involved.
- **`docker-compose.test-fabric.yml` fixed to match its own apparent
  intent**: `ghostlink-worker-2` now runs `stage-worker` and the
  coordinator's `flow` command connects to it via `--remote-addr` across
  the compose network, instead of both sides running unrelated commands
  that never talked to each other.
- **New benchmark harness** (`scripts/remote_flow_benchmark.py`) drives
  the real `stage-worker`/`flow --remote-addr` path repeatedly and reports
  measured throughput and real remote round-trip time — for use across
  two physical machines to get honest multi-host numbers.

### 🐛 Fixed (latent, found while building the harness)

- `ghost-link stage-worker` ignored `GHOSTLINK_TCP_AUTH_TOKEN` and other
  TCP transport env vars entirely, always using
  `TcpTransportConfig::default()` — meaning a coordinator configured with
  a non-default auth token (exactly what the docker-compose fix above
  does) would have its connection reset by the worker. Now reads the same
  `tcp_transport_config_from_env()` the coordinator already uses.

### 📝 Docs

- `docs/DEPLOYMENT.md` gained a "Stage 3b: Real Cross-Machine Flow
  Execution" section documenting `stage-worker`/`flow --remote-addr`, with
  an explicit callout that the transport is genuinely cross-process but
  `run_stage_compute` remains a synthetic timing proxy, not real
  distributed LLM inference.
- `docs/BENCHMARKS.md`'s "Multi-Node Performance / LAN Performance" table
  — untraceable to any real run, and impossible to have produced before
  this PR since `flow` couldn't reach a second machine — is now explicitly
  labeled "unverified, pending a real multi-host run" instead of presented
  as measured. A real (loopback smoke-test) run of the new harness is
  documented alongside it.

### ✅ Validation

- New `crates/ghost-link/tests/stage_worker_integration.rs`: spawns the
  real `ghost-link` binary as two separate OS processes (not threads, not
  in-process calls) via `CARGO_BIN_EXE_ghost-link`, and asserts the `flow`
  process actually took the `REAL execution` path and the `stage-worker`
  process actually processed a nonzero batch count.
- 4 new `ghostlink-core` unit tests for the handshake, remote-stage
  rejection on missing stages, stage-merging, and a real (non-loopback
  simulation) TCP round trip.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
  warnings`, and `cargo test --workspace` all pass: 138 `ghostlink-core`
  tests, 99 `ghost-link` unit tests, 1 new integration test, 10 `mcp-rag`
  tests — all green.

## [1.8.0] - 2026-07-25 (Voice input in chat)

### ✨ Features

- **Chat gains browser-native voice input.** A mic toggle button next to
  Send in `ChatTab.tsx` uses the Web Speech API (`SpeechRecognition` /
  `webkitSpeechRecognition`) to transcribe speech directly into the message
  box — interim results update live, final results accumulate, and the
  button never renders on browsers without support (Firefox/Safari) rather
  than showing a dead control.
- Worth being upfront about: this is cloud-backed (the browser's built-in
  recognizer needs internet), a real tension with the rest of this app's
  local-first inference story. A local Whisper.cpp integration (mirroring
  `native_engine.rs`'s process-management pattern) would resolve that but
  is a substantially bigger lift — left as explicit future work, not
  silently glossed over.

### ✅ Validation

- `tsc --noEmit` clean, `vitest run` (108 passed, 0 failed) including 2 new
  tests: mic button absence when `SpeechRecognition` is unavailable, and
  start/stop/transcript-into-input behavior with a mocked recognizer.
- Manually verified in a live browser: clicking the mic button triggers a
  real microphone permission request (confirming the Web Speech API call is
  wired correctly end to end), and the button correctly resets to its
  initial state when permission is denied rather than getting stuck
  "recording."

## [1.7.1] - 2026-07-25 (Fix: concurrent model load/unload requests corrupted state and could kill llama-server)

Chasing a report of chat suddenly failing with `error sending request for url
(http://127.0.0.1:8080/v1/chat/completions)` — llama-server was dead despite
ghost-link's own log claiming "Successfully loaded model" moments earlier.

### 🐛 Correctness

- **`/api/models/load` and `/api/models/unload` had no mutual exclusion.**
  `handle_gui_model_load`/`handle_gui_model_unload` deliberately drop the
  `BackendState` lock before calling the (intentionally blocking)
  `NativeEngineClient::load_model_into_slot`/`unload_model`, so two
  overlapping requests (a double-click, a fast model switch before the
  first request's promise resolved, etc.) could run the whole
  stage-on-scratch-port → kill-existing → bind-real-port sequence
  concurrently. `free_llama_port` kills by image name
  (`taskkill /F /IM llama-server.exe` on Windows) rather than by PID, so one
  request's cleanup could kill the *other* request's freshly-staged or
  just-promoted process. Confirmed by firing 3 concurrent `/api/models/load`
  requests: each response reported a *different*, wrong `current_model`
  (`backend.current_model = selected_model` was a plain last-writer-wins
  race with no relation to which underlying process actually survived).
- Fixed with a new `model_lifecycle_lock` (`Arc<tokio::sync::Mutex<()>>`) on
  `BackendState`, held for the full duration of both handlers — from model
  resolution through the `BackendState` updates that follow the load/unload
  call. Overlapping requests now queue instead of racing.

### ✅ Validation

- Reproduced the corruption pre-fix: 3 concurrent `/api/models/load` calls
  each returned a different `current_model` field, with the shared
  `model_path` field also cross-contaminated between requests.
- Post-fix: the same 3-way concurrent load re-run — each response now
  correctly reports its own requested model, `/api/models/status` and the
  actual running `llama-server.exe` process agree, and a follow-up chat
  request against the settled model succeeds.
- Single-request stability: model loaded via the real API, then polled the
  live `llama-server.exe` PID every 250ms for 90s (spanning a real chat
  turn) — stayed alive the whole time, confirming the earlier crash needed
  the concurrent-request race, not just normal single-session use.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo test --workspace` (281 passed, 0 failed, 6 ignored) all
  clean.

## [1.7.0] - 2026-07-25 (Chat gains conversation memory; README overhaul with live demo)

Chat turns previously carried zero history — every request to the model was
built from the single latest message, so a second turn had no memory of the
first. This release wires the GUI's full transcript through to the backend,
adds a configurable token budget for that history (separate from the
per-response `max_tokens`), and gives the GUI live feedback on how close a
conversation is to that budget. Also includes a full README reorganization
with a real recorded GUI walkthrough (Llama 3.2 1B) embedded as an inline demo.

### ✨ Features

- **Chat now sends full conversation history, not just the latest turn.**
  `GuiChatRequest` gains a `messages: Vec<{role, content}>` field (the old
  single `message` string is kept only as a fallback for un-upgraded
  clients). `handle_gui_chat` builds the model prompt from the whole
  (windowed) transcript via a new `build_conversation_prompt` helper instead
  of `req.message` alone.
- **New `conversation_token_limit` setting** — a token budget for chat
  history, distinct from `max_tokens` (which only caps the response length).
  Default derives from `ctx_size − max_tokens − margin` (currently 1920 on
  stock settings) via shared constants, rather than a flat guess, so the
  default doesn't immediately exceed the model's context window. The
  effective limit is additionally clamped to `ctx_size` at request time, so a
  manually-raised or stale `settings.json` value can't overflow the context
  window — it just truncates history harder instead.
- **Newest-first truncation**: once history + the reserved response budget
  would exceed the limit, oldest turns are dropped first; the single newest
  turn is always kept even if it alone exceeds the budget. The server reports
  `truncated: true` back to the GUI (both streaming and non-streaming) so a
  shortened memory is visible instead of silently looking like the model
  forgot something.

### 🐛 Correctness

- **The system prompt was silently dropped on any turn with no tools
  enabled.** The old prompt-building branch only spliced in `system_prompt`
  when tool instructions were non-empty. `build_conversation_prompt` always
  includes it now.

### 🎨 UI / Accessibility

- Chat header gains a live token-budget chip (`~N/limit`, color escalates
  blue → amber → red as it fills), computed client-side from the same
  chars/4 heuristic the backend uses, updating as the user types.
- A subtle "earlier messages trimmed to fit memory" divider renders above a
  reply when the server actually had to truncate history.
- Settings tab gains a **Conversation Token Limit** slider next to Max
  Tokens, with an inline warning if history + Max Tokens would exceed a
  4096-token window.

### 📚 Documentation

- `README.md` reorganized: hero section with a full badge row (CI, Tests,
  Security, MSRV, License, Docs, Version, Rust, Platforms, PRs-welcome,
  Stars), a Table of Contents, and a **Demo** section with an inline-playing
  GIF (`docs/assets/demo/ghostlink-walkthrough.gif`) captured from a real
  install → load Llama 3.2 1B → chat walkthrough. All prior content
  preserved, just regrouped.

### ✅ Validation

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo test --workspace` (281 passed, 0 failed, 6 ignored).
- Frontend: `tsc --noEmit` clean, `vitest run` (106 passed, 0 failed),
  including new coverage for truncation, the system-prompt regression,
  history forwarding, and the streamed `truncated` flag.

## [1.6.0] - 2026-07-25 (Go control-plane becomes the real gateway; session benchmark pass)

The Go control-plane moves from an underused, partially-duplicate component to
the actual front door for both native dev and docker-compose: it now owns
CORS, request logging, and rate limiting, proxies everything through to
ghost-link (which keeps all cluster/inference state — UDP discovery was
deliberately not ported to Go), and streams SSE chat responses correctly
instead of silently buffering them. Also includes a full-spectrum performance
benchmark pass documented in `docs/BENCHMARKS.md`.

### 🐛 Correctness

- **Go's reverse proxy silently broke SSE token streaming.** `forward()` used
  a single buffered `io.Copy`, so real-time chat streaming (and the Ollama
  pull-progress stream) sat in Go's write buffer until it filled or the
  response ended — turning streaming into a long wait-then-dump for anything
  routed through the gateway. Rewritten to read/write/flush per chunk.
- **Duplicate `Access-Control-Allow-Origin` headers broke every proxied `/api/*`
  call in the browser.** The gateway's own CORS middleware set the header via
  `Set()`, but `forward()` then copied ghost-link's own permissive CORS
  headers on top via `Add()`, producing two values for the same header —
  invalid per the Fetch spec, so browsers silently failed the request
  (`net::ERR_FAILED`) even though curl/server-to-server callers never noticed.
  `/health` (no backend hop) worked throughout, which is what made this easy
  to miss. Fixed by having `forward()` skip headers the gateway middleware
  already owns.
- **`public/env-config.js` silently pinned the GUI to ghost-link's port,
  overriding every other config layer.** This static file (loaded before any
  app JS runs) had first priority in `resolveApiBase()` and was hardcoded to
  `:8003` with a comment claiming a launch script regenerates it — but no
  launch script actually did. Updated the committed default to `:8000` and
  added regeneration to `launch-native.ps1` (mirroring the pattern the GUI's
  `Dockerfile` already used for the containerized deploy).
- Removed Go's local in-memory worker registry (`pkg/registry`) — it had no
  knowledge of ghost-link's real UDP peer discovery / cluster state, so it was
  a second, disconnected source of truth. Worker routes now always defer to
  ghost-link's actual implementation.

### ✨ Features

- Request logging and a stdlib-only sliding-window rate limiter
  (`pkg/ratelimit`) on the Go gateway.
- `docker-compose.yml`'s `ghostlink-gui` service now points at
  `ghostlink-control-plane:8000` instead of `ghostlink-api:8003` directly.

### 📊 Performance

- Full-spectrum benchmark session (Criterion primitives, `flow_perf_snapshot.py`
  full-pipeline runs, TCP autotune investigation, llama-server flag tuning) —
  see `docs/BENCHMARKS.md` for hardware, methodology, and results. No
  regressions found; all drift/stage-tail/canary/schema-contract gates passed.

---

## [1.5.0] - 2026-07-25 (GUI overhaul: command palette, compare mode, real session persistence; two backend correctness fixes)

A broad GUI improvement pass (command palette, accessibility, chat/metrics
depth, multi-model comparison) plus two backend bugs found and fixed along
the way: saved chat sessions silently discarded their content, and
interrupted HuggingFace downloads could leave a corrupt `.gguf` on disk
that the UI then offered as a normal model.

### 🐛 Correctness

- **`download_hf_model` could leave a corrupt `.gguf` at the trusted
  filename.** A dropped connection mid-transfer returned an `Err` from
  `stream.chunk()` that propagated via `?` immediately, skipping the
  cleanup that only ran for the other failure mode (a short byte count on
  a clean-looking EOF). `scan_local_models_dir` has no integrity check of
  its own — any `.gguf` file it finds is listed as `"Ready"` — so a
  truncated file was then offered to the user as a working model. Now
  streams into a `<name>.gguf.part` sibling and only renames it into place
  after the byte count is verified; no interruption, of any kind, can
  produce a file at the trusted name anymore.
- **Saved chat sessions never stored their messages.** `SessionRecord` had
  no `messages` field — `handle_gui_session_save` received the full
  conversation from the frontend and discarded it after computing a token
  count, so `handle_gui_session_load` had nothing to return. The frontend
  compounded this: `handleLoadSession` didn't apply anything from a
  successful response either. Added `name`/`messages` to `SessionRecord`,
  `sessions.json` persistence (mirroring the existing `models.json`
  pattern — sessions were pure in-memory before this and never survived a
  restart), and wired the frontend to actually restore `messages` on load.
- **Live-inference metrics could corrupt a saved session's metadata.** The
  tracker matched via `backend.sessions.first_mut()` — whichever session
  happened to be first — so a saved chat landing at index 0 would have its
  `tokens`/`model`/`status` silently overwritten by unrelated chat
  activity. Now matched by its own well-known id.
- Chat markdown (paragraphs, lists, headings) rendered with zero spacing —
  `@tailwindcss/typography` was never installed despite the `prose`
  classes already being applied, so they were dead no-ops; combined with
  the app's global CSS reset, every markdown element collapsed to zero
  margin.

### ✨ Features

- **Command palette (Ctrl/Cmd+K).** The shortcut previously had no
  listener behind it — real fuzzy search, arrow-key navigation, and a
  registry of actions (jump to any tab, new chat).
- **Compare Mode.** Send one message to two models and see both replies
  side by side. The backend serves exactly one model at a time
  (`GuiChatRequest.model` is dead code server-side — chat always uses
  whichever model was last explicitly loaded), so this runs the two
  halves sequentially with an explicit `/api/models/load` between them
  and visible "Loading model…" feedback, then reloads the user's original
  model afterward so the turn doesn't silently strand the app on whichever
  model ran last.
- Syntax-highlighted, copyable code blocks in chat (`rehype-highlight`,
  themed from the app's own token palette).
- Metrics: rolling sparklines on throughput/latency stat cards, plus a
  full CPU/Memory/GPU utilization history chart (recharts, ~6 minutes of
  rolling history).
- Message delete and edit-and-resend (truncates history back to the
  edited turn and reloads it into the composer).
- Interrupted-download cleanup: stray `.gguf.part` files are now listed
  with size/age and can be discarded from the Models tab, via two new
  endpoints (`GET /api/models/partial`, `POST /api/models/partial/discard`).
- Shared toast notifications, replacing two banners that had no dismiss
  affordance at all (ChatTab's error toast's "X" was decorative; ModelsTab's
  status banner just accumulated the latest string forever).
- Keyboard shortcuts: Ctrl/Cmd+Shift+O for new chat (Ctrl+N is reserved by
  most browsers for "new window" and isn't reliably interceptable), Escape
  blurs the chat composer, Arrow Up/Down moves through the sessions list.

### 🎨 UI / Accessibility

- Full accessibility pass: roles/`aria-*` on every custom dropdown, modal,
  and toggle; Escape-to-close on all popups; `aria-live` regions for
  streaming/error/status text; labels on icon-only buttons that had none.
- Semantic color tokens (`success`/`warning`/`danger`/`info`/`accent`)
  added to `tailwind.config.js` as the status-color API going forward.
- Mobile: the sidebar was permanently open at 256px on a 375px viewport,
  leaving ~119px for actual content. Now starts collapsed below the `md`
  breakpoint and renders as a fixed overlay with a backdrop instead of
  pushing content.
- Removed the "Grid view" / "Cloud sync" / "Voice input" buttons — none
  had an `onClick` at all; no backend exists for any of them.
- Removed 8 unused PNGs in `/assets/icons` (the live UI has used
  `lucide-react` exclusively) and a ~60-line dead, unreachable duplicate
  JSX branch in `ModelsTab.tsx`.
- Fixed a real CSS bug found in passing: the sidebar-collapse button used
  `-translate_y-1/2` (underscore instead of hyphen), silently breaking its
  vertical centering.

### ✅ Validation

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
  warnings` (zero warnings), `cargo test --workspace` (134+7+28+19, all
  passing), `cargo audit` (0 vulnerabilities; 3 pre-existing unmaintained/
  unsound advisories on transitive deps, none introduced by this change)
  — all clean.
- Frontend: `tsc --noEmit` clean, all 104 Vitest tests passing.
- Every feature above was verified live against the running backend, not
  just unit-tested: Compare Mode with two genuinely different local
  models producing genuinely different output; Load Session round-tripped
  through a real save/clear/load cycle; the partial-download UI against a
  real interrupted-download file (created, listed, discarded, confirmed
  removed from disk); the mobile sidebar at both 375px and the 768px `md`
  boundary; keyboard shortcuts via dispatched `KeyboardEvent`s.

### ⚠️ Operational caveats

- This fix does not retroactively clean up any `.gguf` files that are
  already corrupt from before it — those still need a manual look (or use
  the new "Interrupted Downloads" UI if a matching `.gguf.part` happens to
  still be present, which it won't be for a download that "completed"
  under the old bug).
- Compare Mode's sequential model-swap costs real time per turn (a model
  load spawns a fresh `llama-server` subprocess) — it is not, and cannot
  currently be, two simultaneous generations.

---

## [1.4.5] - 2026-07-24 (HuggingFace downloads: hardware-aware quantization, complete shard downloads)

A user-reported issue ("downloads doesn't work properly and completely for my
machine") traced to `download_hf_model` always taking whichever `.gguf` file
HuggingFace's API happened to list first, with no regard for size or for
models split across multiple shard files.

### 🐛 Correctness

- **`download_hf_model` ignored file size entirely.** It always downloaded
  `gguf_files[0]` — the first `.gguf` sibling HuggingFace's API returned —
  regardless of whether that was a tiny IQ2 quant or a multi-GB F16/Q8_0
  file. On a repo that lists largest-first, this could grab a file far too
  big for the local machine's VRAM. Added `quant_rank` (scores common GGUF
  quantization tags — IQ1 through F32 — by relative size/fidelity) and
  `target_quant_rank_for_vram` (maps detected local VRAM to a sensible
  target rank, e.g. Q4_K_M-tier for this class of hardware, lower for
  genuinely constrained VRAM) so the download picks a quantization that
  actually fits, instead of an arbitrary one.
- **Split (sharded) GGUF files were silently broken.** HuggingFace repos
  commonly split one quantization across multiple files named
  `<name>-00001-of-00005.gguf`, etc. The old code could pick just one shard
  of a multi-file split, "successfully" complete the download, and leave an
  unloadable partial model on disk (llama.cpp requires every shard present
  alongside the first). Added `group_gguf_shards`, which detects the
  `-NNNNN-of-NNNNN` naming convention and groups a split model's files
  together so every shard of the chosen quantization downloads, not just
  the first one encountered.
- Moved these three new functions (plus the selection logic) out of their
  original nesting inside `start_openai_api_server` to module scope — they're
  pure functions with no dependency on handler state, and nesting made them
  unreachable from the existing test module. `download_hf_model` itself
  (the actual HTTP-calling code) stays where it was and calls out to them.

### ✅ Validation

- 4 new unit tests: quantization ranking order, VRAM-based target scaling,
  selection avoiding the naive first-listed file, and shard grouping keeping
  a split set together in the correct order.
- Live end-to-end test against the real HuggingFace API (isolated test
  instance, separate port and models directory — never touching the
  actual running server): queried `bartowski/Llama-3.2-1B-Instruct-GGUF`
  (18 real quantization files), correctly selected `IQ3_M` for this
  machine's ~4GB VRAM rather than the first-listed file, downloaded
  completely (657MB, valid `GGUF` magic header confirmed, no truncation).
- `cargo build --workspace --all-targets`, `cargo clippy --workspace
  --all-targets -- -D warnings` (zero warnings), `cargo fmt --all --check`,
  `cargo test --workspace` (87+134+7+28+19, all passing) — all clean.

### ⚠️ Operational caveats

- The quantization-rank-to-VRAM mapping is a heuristic based on the
  quantization tag alone — it doesn't account for the underlying model's
  parameter count (a 1B model in Q8_0 and a 70B model in Q8_0 have wildly
  different memory footprints for the same tag). It errs conservative
  (prefers a rank at or below the target on a tie) rather than risking an
  out-of-memory load, which may pick a lower-fidelity quantization than
  strictly necessary for small models — a correctness/safety tradeoff, not
  an oversight.
- While validating this live, a llama-server PID change was briefly
  suspected to be caused by the test instance's model-load path (Windows'
  port-cleanup step kills `llama-server.exe` by image name, not by port —
  a real hazard for any second instance on the same machine). Investigation
  confirmed it was unrelated concurrent model-switching activity on the
  live instance, not this test — but the model-*load* step was deliberately
  not re-exercised against the live server to avoid that risk entirely;
  the download/file-integrity validation above stood on its own.

---

## [1.4.4] - 2026-07-24 (Real incremental streaming for chat and model pulls)

Chat "streaming" and Ollama model-pull "streaming" both looked like streaming
downstream but weren't upstream: the client-facing plumbing existed, but
every path still waited for the entire backend response before relaying
anything. Verified live against a real running `llama-server` throughout
(a throwaway instance on a separate port, never touching the actual running
server or its process — see Validation below).

### ⚡ Performance / correctness

- **`handle_gui_chat`'s SSE response was fake streaming.** It waited for
  `run_tool_loop`/`generate_once` to fully finish generation (blocking on the
  entire response, for both backends), *then* split the already-complete
  text into word chunks and dripped them out over SSE. For a longer response,
  a client saw zero output for the entire generation time, then everything
  at once — the request explicitly asked for `stream: true` but got none of
  the actual benefit (reduced time-to-first-token). Added
  `NativeEngineClient::generate_chat_stream` (llama-server had no streaming
  client method at all before this) and wired real streaming into
  `handle_gui_chat` for the common case: `stream: true` with no MCP tools
  enabled. Tool-calling requests still use the existing buffer-then-chunk
  path — tool-call marker detection needs the complete text, so real-time
  interleaving there is a separate, larger piece of work left for later
  rather than rushed into this change.
- **`ollama.rs`'s existing "streaming" methods buffered the whole response
  first.** `generate_stream`/`chat_stream`/`stream_pull_progress` all called
  `resp.bytes().await` — which waits for the entire HTTP body — before
  processing any of it, then faked incremental delivery downstream via an
  mpsc channel. Ghostlink itself never got any earlier data than a
  non-streaming call would. `stream_pull_progress` is the one of the three
  that's actually wired in and used (model download progress in the GUI),
  which made this the more consequential half of the bug: a multi-minute
  model pull would show a frozen progress bar for the entire download, then
  jump straight to 100%. `generate_stream`/`chat_stream` are unwired dead
  code today (confirmed via full-crate grep) but fixed for correctness and
  in case they're wired in later. All three now use `bytes_stream()` with
  explicit partial-line buffering across chunk boundaries (a network chunk
  boundary rarely lines up with a JSON-line boundary).
- Extracted `record_generation_metrics` (request counter, `inference_metrics`,
  dashboard session record) out of `finish_chat_response` so the new
  streaming path shares the exact same side effects instead of a second,
  divergence-prone copy of this bookkeeping.
- `reqwest`'s `stream` feature enabled in `ghost-link`'s `Cargo.toml` —
  required for `bytes_stream()`; wasn't needed before since nothing actually
  streamed incrementally.

### ✅ Validation

- Direct client-method test against the live `llama-server`: 10 chunks
  arrived over 190ms (43ms, 54ms, 68ms, 80ms, 93ms, ...) — incremental
  delivery, not one blob at the end.
- Full HTTP path (`POST /api/inference/chat`, `stream: true`, no tools)
  through a throwaway `ghost-link` instance on a separate port pointed at
  the same live `llama-server`: 15 chunks arrived progressively from 0.27s
  to 0.45s, correct generated text.
- Regression-checked: non-streaming requests unchanged; streaming requests
  with tools enabled still correctly fall back to the existing
  buffer-then-chunk path.
- Confirmed the live `ghost-link`/`llama-server` instances were completely
  unaffected throughout (same `llama-server` PID before and after; the
  Windows port-cleanup path (`taskkill /IM llama-server.exe`) that model
  hot-swapping uses was checked and confirmed *not* reachable from plain
  `serve` startup, only from explicit model-load/switch calls, before
  running a second instance alongside the live one).
- A found-and-fixed-before-shipping bug during this work: the new streaming
  task's client-disconnect early-return bypassed releasing the
  graceful-shutdown request-tracker counter (a real leak under concurrent
  use). Restructured to a single exit point so the tracker release and
  metrics recording can't be skipped by any of the three ways the stream
  can end (normal completion, backend error, client disconnect).
- `cargo build --workspace --all-targets`, `cargo clippy --workspace
  --all-targets -- -D warnings` (zero warnings), `cargo fmt --all --check`,
  `cargo test --workspace` (83+134+7+28+19, all passing) — checked 3x
  consecutively for stability. Added one `#[ignore]`d test
  (`generate_chat_stream_yields_incremental_chunks_against_live_server`,
  run manually with `--ignored` against a live server) asserting more than
  one chunk arrives, specifically to catch a regression back to buffering.

### ⚠️ Operational caveats

- Real streaming only covers the no-tools-enabled case for both backends.
  Streaming *with* tool-calling enabled still uses the old buffer-then-chunk
  behavior — implementing real-time streaming that also interleaves
  tool-call detection mid-generation is a larger redesign, intentionally out
  of scope here.
- `generate_chat_stream`'s fallback-on-no-chat-template path (HTTP 400 from
  the chat endpoint) degrades to the existing non-streaming `/completion`
  call and presents the whole result as a single SSE chunk, rather than
  duplicating a second incremental parser for llama.cpp's native
  `/completion` streaming shape — an intentional scope boundary, not an
  oversight.

---

## [1.4.3] - 2026-07-24 (Correctness sweep, GPU offload fix, hardware-detection latency, flaky-test root cause)

A broad correctness and performance pass across `ghostlink-core` and `ghost-link`,
followed by two targeted sweeps on inference-path overhead and GPU utilization.
Prioritized by measured evidence (`cargo bench`, repeated `cargo test --workspace`
runs) over assumption throughout.

### 🔒 Security

- **`discovery.rs`: `enforce_auth: true` with no token configured silently
  accepted unauthenticated UDP discovery frames.** `decode_datagram_with_options`
  only gated authentication on whether `auth_token` was `Some`, never checking
  `enforce_auth` itself. An operator setting `enforce_auth: true` while
  `auth_token` was accidentally left `None` (e.g. an empty env-var lookup) got
  silent fail-open behavior instead of an error — any well-formed datagram from
  any sender on the LAN was accepted into a cluster believed to be running in
  secure mode. Now fails closed: all three discovery entry points
  (`broadcast_and_collect`, `respond_once`, `serve_discovery_with_stats`) return
  a config error immediately when this combination is detected.
- **`main.rs`: path traversal in `download_hf_model`.** The local write path for
  a downloaded model was built directly from a filename taken out of the
  HuggingFace API's JSON response (`rfilename`), with no sanitization. A
  malicious or mirrored HF-compatible repo could supply an absolute or
  traversal-laced filename and write outside the models directory. Fixed to use
  only the basename (`Path::file_name()`) for the local destination; the
  outbound download URL still uses the original remote filename.

### ⚡ Performance

- **GPU offload (`-ngl`) was silently dropped, forcing CPU-only inference —
  the most significant finding of this pass.** Two compounding bugs, both
  rooted in the same "`-1` = let llama-server auto-decide" design intent being
  implemented inconsistently:
  - `main.rs`'s startup auto-configuration computed `ngl = -1` for the
    below-4GB-VRAM / no-GPU-detected case (by far the most common real-world
    case on a GPU-less or low-VRAM host) but only applied the result — and
    logged anything at all — when `ngl > 0`, so this case silently no-oped.
  - `native_engine.rs`'s command builder only passed `-ngl` to `llama-server`
    when `ngl >= 0`, omitting the flag entirely for `-1`. llama-server's own
    default when `-ngl` is absent is `0` (CPU-only) — the opposite of the
    documented "auto-offload" intent. Confirmed against the project's own
    validated launchers (`scripts/run_native_llama_server_stack.sh`,
    `scripts/validate_native_llama_server.sh`), which already pass `-ngl -1`
    as literal CLI text, that this llama-server build honors `-1` correctly.

  Net effect before this fix: any launch path bypassing `launch.sh`'s own
  env-var wiring (running the binary directly, Docker, a different launcher),
  or simply having <4GB VRAM or an undetected GPU, resulted in fully CPU-only
  inference with zero indication to the user. Both now apply/pass `-ngl`
  unconditionally.
- **Hardware detection (`SystemProfile::detect()`, Full mode): 3.96s → 1.41s
  (~65%), measured via `cargo bench` on the same machine.** This runs once at
  `SystemProfileWatcher::new()` (server startup) and on-demand from CLI/API
  diagnostic paths. Root cause: ~8-10 external-process probes (PowerShell/CIM
  queries, `wmic`, `nvidia-smi`) running strictly sequentially with no shared
  state between them.
  - Parallelized the six independent top-level probes (hostname/cpu/memory/
    gpu/npu/network) via `std::thread::scope`.
  - Found via in-process instrumentation that `detect_cpu_info()` alone cost
    2.79s from two *separate* sequential `Get-CimInstance Win32_Processor`
    calls (brand string, physical-core count) — parallelized those too
    (down to 1.33s, then the memory total/available probes the same way).
  - Replaced the DXGI `Add-Type` C# VRAM fallback (compiles inline C# via
    PowerShell — ~1-2s) with a plain registry read of
    `HardwareInformation.qwMemorySize` (~0.3s, the same technique GPU-Z uses),
    falling back to `Add-Type` only if the registry value is absent.
  - **Thundering-herd cache bug introduced by the above and fixed in the same
    pass**: the cache lock was held only to check freshness, not across the
    detection itself, so concurrent Fast-mode callers within the same instant
    all missed the cache and independently launched their own full probe
    battery — observed spiking to dozens of concurrent PowerShell processes
    and destabilizing unrelated tests. Fixed by holding the lock across the
    whole check-compute-populate sequence so concurrent callers serialize on
    one detection. This is a real concurrent-request fix, not just a test fix.
- **Chat completion hot path ran a full synthetic pipeline simulation on every
  real request.** `handle_chat_completions` (`/v1/chat/completions`) executed
  `execute_pipeline_tcp_loopback`/`execute_pipeline_distributed` — the same
  benchmark-harness code with `sin()`-based fake compute used in
  `tensor_streaming_fabric` benchmarks — synchronously before calling the real
  backend, purely to produce a throughput/latency string that appeared only in
  one narrow error-fallback message and was otherwise discarded. Measured cost
  via `cargo bench`: ~0.3ms (single stage) up to 40ms average / 112ms peak
  (multi-stage). Removed entirely; replaced with real measured latency/
  throughput from the actual generation call (mirroring the pattern
  `finish_chat_response` already used for the GUI chat path). The dashboard
  metrics this fed now reflect real generation numbers instead of fabricated
  ones.
- **`ClusterState::nodes()` vs `nodes_snapshot()`: 25x cost gap (463ns vs
  18ns).** `.nodes()` deep-clones every `NodeResources` (including owned
  `String` fields) into a fresh `Vec`; `.nodes_snapshot()` is a cheap `Arc`
  load. Fixed three production call sites in `main.rs` that only needed a
  borrow — including `handle_gui_metrics` (a metrics-polling endpoint), which
  was cloning the entire cluster node list solely to call `.len()` on it;
  replaced with the already-existing zero-clone `cluster.node_count()`.
- **`ring.rs`: `push_batch()` never tracked `overflow_count`.** The batched
  hot path silently hid backpressure from monitoring while the single-item
  `push()` path correctly counted it. Fixed to track both full-ring rejection
  and partial-batch overflow.

### 🐛 Correctness

- **`protocol.rs`**: an off-by-one in `encode_payload_into`'s length check
  spuriously rejected valid max-size payloads with a GPU name set; a missing
  combined-length check in `DiscoveryFrame::encode` could panic (slice
  out-of-bounds) on oversized combined field lengths instead of falling back
  gracefully like its sibling overflow checks.
- **`planning.rs`**: trailing zero-VRAM layers (e.g. bias-only layers) were
  silently dropped from the placement plan instead of flushed; a
  divide-by-zero/NaN path existed when all cluster nodes are marked `Failed`
  (a real path via heartbeat timeout / network partition) while the node list
  is still non-empty. Reconciled with the independent `active_nodes_count()`/
  single-pass-lock optimization to this same function that landed on `main`
  in parallel (PR #158): kept that PR's more efficient single-lock-pass
  implementation, but changed its zero-active-nodes fallback from `1.0` to
  `0.0` — assuming a perfect delivery ratio when there is literally no
  corroborating health data is optimistic in exactly the scenario (all nodes
  failed/degraded) where it's least likely to be true; `0.0` selects the most
  conservative quantization mode instead.
- **`load_balance.rs`**: the first `update_balance_ratio` call could be
  diluted by a stale EMA (`0.0 * 0.9 + ratio * 0.1`) instead of initializing
  directly, because its "first call" detector incorrectly used an unrelated
  counter as a proxy.
- **`health.rs`**: a node already marked `Degraded` whose heartbeat then timed
  out completely never transitioned to `Failed` (the guard only checked for
  `Active`); `get_recommendation()` compared an averaged delivery ratio against
  a stale running-*minimum* latency instead of the averaged latency, letting
  one early lucky fast sample mask permanent degradation forever.
- **`native_engine.rs`**: `has_running_llama_server` checked only that a
  `Child` handle existed (true for any real PID), so a crashed-but-unreaped
  llama-server process was reported as still running; fixed to use
  `try_wait()`.
- **`ollama.rs`**: HTTP error responses were silently swallowed and reported
  as success in three places — streaming methods (`generate_stream`,
  `chat_stream`, `pull_model_stream`) didn't check status before parsing,
  silently yielding an empty-but-`Ok` stream on error; non-streaming write
  methods (`pull_model`, `create_model`, `copy_model`, `delete_model`)
  returned `Ok` for any readable body regardless of status; `unload_model`
  treated a failed `/api/ps` call as "nothing running" instead of propagating
  the connectivity error.
- **`system_profile.rs`** (carried over from the hardware-detection
  parallelization pass): a Windows dual-socket core-count bug undercounted
  physical cores by half (fed directly into worker-count tuning); an AMX
  capability check always reported `false` on real AMX hardware because it
  tested a compile-time build flag instead of runtime CPUID.
- **`runtime.rs`**: a p95-latency calculation used non-saturating
  subtraction, inconsistent with two sibling implementations in the same
  file that both defensively use `saturating_sub` for exactly this reason.

### 🧪 Test reliability

Root-caused two classes of flaky test rather than papering over symptoms:

- **`native_engine` tests failing together under `cargo test --workspace`**:
  traced to `has_running_llama_server_reflects_actual_process_state` using a
  fixed `sleep(300ms)` and hoping a helper process had exited by then — flaky
  under system load, and a panic here while holding a shared test-only mutex
  poisoned it for two *unrelated* tests in the same file. Replaced the sleep
  with a real `child.wait()` (deterministic regardless of scheduling delays).
- **`discovery` UDP timing tests failing under load**: five tests shared a
  "spawn responder thread, `sleep(50ms)`, then send traffic" pattern with two
  additionally-tight per-attempt timeouts (45ms/260ms); widened for headroom.
  The deeper root cause, however, was the `SystemProfile` thundering-herd
  cache bug above — concurrent Fast-mode probes from unrelated tests spiking
  CPU/process-table contention was starving these UDP tests of scheduling
  time. Fixing the cache is what actually stabilized the suite.
- Result: 11 consecutive full-workspace `cargo test` runs clean, versus
  roughly 1-in-2 failing before.

### 🛠️ Tooling

- **`benches/baseline.rs`** hardcoded 5,000 iterations for the Full hardware
  probe benchmark — at ~1.4s/call that's ~2 hours, silently making this
  benchmark file unusable end-to-end. Fixed independently in both this branch
  and `main` (see the `[1.4.2]` entry below) while this work was in progress;
  reconciled to `main`'s 20-iteration count.
- **`scripts/summarize_criterion_report.py`** built benchmark keys via
  `str(Path)`, which renders with backslashes on Windows — silently producing
  keys like `autotune\accelerator_scale_f32_slice` instead of
  `autotune/accelerator_scale_f32_slice`, breaking any cross-platform
  diff/trend comparison of `artifacts/criterion-summary.json` between CI
  runners (ubuntu/windows/macos). Fixed to use `.as_posix()`.
- `artifacts/criterion-summary.json` refreshed via the project's own
  `summarize_criterion_report.py` against a fresh `cargo bench` run (see
  Operational caveats below on why `docs/PERF_BASELINE.json` was
  deliberately **not** touched).

### ✅ Validation

- `cargo build --workspace --all-targets`, `cargo clippy --workspace
  --all-targets` (zero warnings), `cargo test --workspace` (83+134+7+28+19
  tests, all passing) — checked repeatedly (11+ consecutive full-workspace
  runs) specifically to confirm the flaky-test root causes above are actually
  fixed, not just quieter.
- `cargo bench --package ghostlink-core` (criterion + `baseline.rs` +
  `tensor_streaming_fabric`) — current numbers captured in
  `artifacts/criterion-summary.json`.

### ⚠️ Operational caveats

- **`docs/PERF_BASELINE.json` was deliberately not refreshed.** It's a real
  CI-gating file (`production-gate.yml`'s drift check), generated by
  `scripts/flow_perf_snapshot.py` and compared by `scripts/check_perf_drift.py`.
  A supplementary local run on this dev machine (release, `exec_tokens=512`,
  `micro_batch=8`, 5 runs, matching the committed profile's methodology)
  showed `tcp` mode at `throughput_avg=180,659` / `p95_avg=2.80ms` /
  `wall_avg=2.85ms` vs the committed `256,020` / `1.97ms` / `2.03ms`, and
  `inmem` roughly flat (`468,035` vs `506,809`). This is **not** presented as
  a regression — the committed baseline's `local_id`/`remote_id` values
  (`iprada-16gb`/`zenbook-32gb`) indicate it was captured on different
  physical hardware, making a direct comparison invalid. Refreshing this file
  should go through the proper CI-driven re-baseline process on the actual
  target runner, not an ad hoc single dev-box run.
- All new benchmark numbers in this changelog and in
  `artifacts/criterion-summary.json` were measured on one Windows dev machine
  (AMD Ryzen AI 7 350, integrated Radeon 860M) — cross-reference against
  README.md's benchmark table (measured on a different machine/OS/build
  flags) only qualitatively, not as a direct before/after.
- The GPU offload (`-ngl`) fix is a categorical correctness change (GPU used
  vs. silently not used) that no microbenchmark in this repo captures — there
  is no live `llama-server` + loaded model in this environment to measure
  real tokens/sec against. Recommend a spot-check on real hardware with a
  loaded model before/after this change.

---

## [1.4.2] - 2026-07-23 (Fix: `cargo bench` effectively hung for hours)

### 🐛 Benchmarks

- **`benches/baseline.rs`: `cargo bench --package ghostlink-core` looked hung** —
  every other benchmark printed its result within seconds, then the run sat
  with no output for a very long time before a maintainer would reasonably
  conclude something had crashed. Root cause: `detect_runtime_profile_full`'s
  bench call ran `ProbeMode::Full` for 5,000 measured iterations (plus this
  harness's own warmup, `1000.min(iters/10)` = 500 more — 5,500 real calls
  total). Unlike `ProbeMode::Fast` (TTL-cached), Full mode's `detect_gpus()`
  spawns several real OS subprocesses per call with **no caching at all**
  (`powershell`, `wmic`, `nvidia-smi`, `rocm-smi`, `vulkaninfo` — see
  `system_profile.rs`). Measured directly: **3.92 seconds per call**. At
  5,500 calls that's ~6 hours for one line of a benchmark suite meant to run
  in a couple of minutes — not a hang, just an iteration count that was fine
  for a cheap function and catastrophic for one that shells out repeatedly.
  Reduced to 20 iterations (matching how expensive the operation actually
  is); the full `cargo bench --package ghostlink-core --bench baseline` now
  completes in ~1m30s end to end (warm build cache), verified by letting it
  run to completion rather than assuming the fix worked.

### ✅ Validation

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`, `cargo audit` — all clean. One flaky failure
  (`discovery::tests::respond_once_ignores_auth_mismatch_then_accepts_valid_request`)
  seen once under full parallel test load, passed in isolation and on a
  clean re-run of the full suite immediately after — pre-existing test
  flakiness unrelated to this change (a one-line edit to a benchmark's
  iteration count), not a regression it introduced.
- Confirmed the fix by actually running the full benchmark suite to
  completion (exit code 0, all expected output lines present, process exits
  promptly) rather than assuming a smaller iteration count would be enough.

---

## [1.4.1] - 2026-07-23 (Performance: HTTP client reuse, LTO, KV cache primitive)

A profiling pass across the core primitives (ring buffer, protocol, planning)
and the native inference request path, prioritized by measured evidence
rather than assumption — this codebase is a distributed scheduling/transport
fabric around an external inference engine (llama-server/Ollama), not an
inference engine itself, so the audit focused on what Ghostlink's own Rust
code actually controls: per-request overhead and cross-node transport, not
token-generation kernels.

### ⚡ Performance

- **`native_engine.rs`: every chat request rebuilt its HTTP client.**
  `generate_with_llama_server()` — the hot path for every request on the
  default `native` backend — called `reqwest::Client::builder()...build()`
  fresh on every single call: a new connection pool, no keep-alive reuse,
  meaning a brand-new TCP connection to llama-server on every chat turn.
  `NativeEngineClient` now holds one shared, connection-pooled client, built
  once and cloned (a cheap `Arc` refcount bump) per request; the configurable
  timeout moved from client-level to request-level so behavior is otherwise
  identical. Measured against a real local `llama-server` on loopback:
  **376µs → 45µs per-request HTTP overhead (8.35x)**. Client *construction*
  alone measured ~540x more expensive than a clone (4.86µs vs 9ns), entirely
  independent of network cost.
- **New `[profile.release]`**: `lto = "thin"`, `codegen-units = 1`, enabling
  cross-crate inlining between `ghostlink-core`'s hot paths (ring buffer,
  protocol, planning) and `ghost-link`. A same-machine, single-run-per-config
  A/B showed the deterministic, single-threaded, CPU-bound paths 6–19%
  faster (ring push+pop -19%, protocol decode -17%, planning autotuned
  -19%); two thread-scheduling/syscall-bound benchmarks came back noisier
  instead (SPSC cross-thread 10k +12%, autotune detect_fast +40%) — reported
  here rather than cherry-picked, since that's far more likely scheduler
  variance than a real regression from this change. `panic = "abort"` was
  considered and deliberately **not** set: verified zero `catch_unwind` usage
  in the codebase, but for a long-running server, abort-on-any-panic crashes
  the whole process instead of failing one request/task — a reliability
  regression, not a pure win, for this binary.

### 🧹 Correctness / cleanup

- **`kv_cache.rs` was dead code** — not declared as a module in `lib.rs`, so
  it was never compiled into the crate and its own tests never ran. Redesigned
  before wiring it in: the old design called `resize_with` on a
  `Vec<KVCacheEntry>` where every entry independently allocated its own
  `keys`/`values` `Vec<f32>` (up to 16,384 separate small heap allocations
  for a full 8192-token sequence); `current_len` was also duplicated on every
  single entry instead of being one cache-level field. Now: one contiguous
  buffer allocated once per layer/sequence, `current_len` tracked once,
  `Mutex` replaced with `RwLock` (attention reads vastly outnumber writes),
  and a new `read_range()` for a single batched read over a token span
  instead of N separate per-token reads. Declared as a real module and
  re-exported from `lib.rs`; 11 new tests added. Documented plainly that it
  has **no current caller** — this is a ready primitive for a future local
  execution path, not something wired into live serving today.

### ✅ Validation

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` (82+124+7+28+19, all passing, including under
  `--release` with the new LTO profile — specifically re-checked since
  aggressive optimization can surface latent UB in `unsafe` code, e.g. the
  ring buffer's SPSC implementation, that a non-LTO build masks), `cargo audit`
  (only pre-existing unmaintained/unsound advisories on transitive deps, no
  actionable vulnerabilities) — all clean.
- Per the pre-push checklist's benchmark-reporting requirement (ring buffer /
  transport / pipeline code changed): see the LTO A/B numbers above and in
  the PR body.

### ⚠️ Operational caveats

- The LTO A/B is a single run per configuration on one machine, not an
  averaged/statistical comparison — treat the regression numbers especially
  as noise-until-proven-otherwise, not confirmed effects.
- While benchmarking, both configurations' `cargo bench` runs printed all
  their output and finished their real work within ~30–40s, but the process
  then took a long time to actually exit afterward. Not diagnosed as part of
  this change (out of scope), but worth a maintainer's attention separately —
  likely in the benchmark harness's shutdown path, not the library code
  itself.

---

## [1.4.0] - 2026-07-23 (Real MCP Server Support for Chat)

Ghostlink chat's "Tools & MCP" feature was entirely fake: the 8 tool checkboxes
dispatched to a hardcoded `ToolDispatcher` that always returned canned strings
(calculator always said "42"), with no real execution, no argument passing,
and no model involvement in deciding to call a tool. This replaces it with
real [MCP](https://modelcontextprotocol.io) server integration end to end.

### ✨ Backend

- **New native Rust MCP client** (`crates/ghost-link/src/mcp/`, built on the
  official `rmcp` SDK) spawns real MCP servers over stdio, with a Windows
  `cmd /C` fix for `.cmd`-shim commands (`npx`/`uvx`) and real process-tree
  teardown (`rmcp`'s own cleanup only kills the direct child; a `cmd /C
  npx ...`-spawned server's own children would otherwise be orphaned).
- **Model-driven tool-calling loop**: the model decides whether and which
  tool to call via a ReAct-style prompt (works with any local GGUF/Ollama
  model), with real arguments extracted from the model's own output and fed
  back as an "Observation" for up to 3 round-trips per turn. Ollama models
  whose chat template declares native tool-calling support use Ollama's
  `tools` API directly instead, when available.
- **Confirmation gate**: tools marked `requires_confirmation` in
  `mcp_servers.toml` (terminal, code_execution) pause and return a
  `pending_tool_call` instead of executing; a new
  `POST /api/inference/chat/tool-confirm` endpoint resumes the same turn on
  approval or denial.
- **All 8 chat tool slots now have real backing servers**: `filesystem`,
  `fetch`, a new custom `mcp-calculator` server (`evalexpr`-backed, replacing
  the old "42" stub), and `sqlite` are enabled by default; `brave-search`
  (needs `BRAVE_API_KEY`), Docker MCP Toolkit-routed `terminal`/
  `code_execution` (needs Docker Desktop — deliberately never backed by a raw
  host-shell server), and `image_generation` (no backend chosen yet) ship
  disabled.
- **Two standalone additions**: the official `sequential-thinking` reference
  server (enabled by default), and a new custom `mcp-vision` server wrapping
  a local Ollama vision model (llava/moondream/...) — stays local-model-first
  instead of adding a cloud dependency.
- `mcp_servers.toml` follows the existing `ghostlink.toml`/
  `ghostlink.example.toml` pattern: the real file is gitignored (the GUI
  writes enable/disable toggles back to it) and auto-bootstraps from the
  checked-in `mcp_servers.example.toml` on first run.

### ✨ Frontend

- New **MCP tab** listing every configured server with live connected/
  disabled status and working enable/disable toggles that take effect
  immediately (no restart needed).
- `ChatTab`'s tool checklist now reflects real configured servers instead of
  a hardcoded 8-tool array; messages show a trace of which tool actually ran
  and its real result, plus an inline approve/deny card for tool calls
  awaiting confirmation. Tool-enabled turns skip token streaming, since tool
  traces and confirmation cards only ever arrive on the plain JSON response.

### ✅ Validation

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`, `cargo audit` — all clean (workspace now includes
  the new `mcp-calculator` and `mcp-vision` crates).
- `tsc --noEmit`, `npx vitest run` (104 tests) — all clean.
- Live-verified end to end, not just unit-tested: real filesystem/fetch/
  calculator/sqlite/sequential-thinking servers connecting and executing for
  real; the full tool loop (parse → execute → feed back → final answer) with
  both a scripted mock model and a real loaded model (`gemma-4-E4B-it-Q4_K_M`);
  the confirmation approve/deny round-trip; the MCP tab's toggle causing an
  immediate live reconnect in the browser; clean process teardown (no
  orphaned `node.exe`/`cmd.exe`) after graceful shutdown.
- A smaller model (`Llama-3.2-1B-Instruct-IQ3_M`) followed the tool-call
  format inconsistently during testing — a known limitation of ~1B-class
  models with structured-output prompting, not a bug in the mechanism itself;
  noted here as an operational caveat rather than something this PR can fix.

### ⚠️ Operational caveats

- Requires `npx`/`node` (bundled MCP servers) and `uvx`/`python` (Python-
  distributed ones) on `PATH`; Docker Desktop for the terminal/code_execution
  slots. None of these are vendored — a fresh checkout without them will see
  those specific servers fail to connect (logged and skipped), not a crash.
- Docker MCP Toolkit and the native-Ollama-tool-calling path could not be
  live-verified in the development sandbox (no running Docker daemon / no
  Ollama instance with a tool-capable model pulled there) — both share code
  paths already proven live for other servers, but call this out per the
  pre-push checklist's platform-awareness guidance.

---

## [1.3.9] - 2026-07-22 (Launch Verification: GUI UX Fixes)

A full end-to-end verification pass of both launch entrypoints (`launch.sh` directly under WSL, and `launch.bat` on Windows delegating to WSL) — driven through the real browser UI, not just curl — surfaced two real, reproducible GUI bugs. (A third finding from the same pass, the System Info panel misreporting Node.js/npm as "not installed," turned out to already be fixed on `main` by [1.3.8] via a parallel effort; no change needed here.)

### 🐛 Frontend

- **`SettingsTab.tsx`: the "Inference Runtime" section was rendered twice**, back to back, with identical fields and the same `onChange` handler — a copy-paste leftover. Confirmed as a genuine duplicate render (two visible headings at different screen coordinates) before removing the second occurrence, not a text-extraction artifact.
- **`App.tsx`: the active model never synced into the UI on page load.** `fetchModels()` already received `current_model` from the backend via `api.getModels()`, but only used `result.models` — the store's `currentModel` stayed at its default `'none'` regardless of what the backend actually had loaded. A user reloading the app saw "Select Model" in the header and new chat replies labeled "N / none," even while a model was already loaded and actively serving requests. Now syncs `currentModel` from the backend's `current_model` field when the store is still at its default.

### ℹ️ Environment note (not a code change)

Edits made from the Windows side did not reliably trigger the WSL-side Vite dev server's file watcher across the `/mnt/c` boundary during this verification — each fix above needed the dev server restarted from inside WSL before it took effect in the browser. This is a WSL2/NTFS file-watching limitation, not a Ghostlink bug; noted here so it isn't mistaken for one during future WSL-backed development.

### ✅ Validation

- Both launch paths run fresh, start to finish: hardware detection, backend build/start, and all health checks passed on the first attempt for both `launch.sh` and `launch.bat`.
- Each fix verified live in the browser, before and after: duplicate section confirmed via DOM inspection then confirmed gone; model label confirmed stuck at "none" via a live chat message, then confirmed correct after the fix with a second live chat message.
- Real inference cross-checked three ways on the same request (direct API call, browser network panel, live Metrics tab): 601.9 tok/s, 102.3 ms p50/p95 latency, `real_inference: true` — all three agree.
- `cargo bench --package ghostlink-core` run directly against the compiled binaries (all three targets: `baseline`, `criterion`, `tensor_streaming_fabric`); no Rust source changed in this PR, included for the record per the pre-push checklist.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `tsc --noEmit`, `npm run build`, `npx vitest run` — all clean.

---

## [1.3.7] - 2026-07-23 (Repo-Wide Correctness Review)

A broad review pass across backend handlers, cluster/health/load-balance logic, a launch script, and the frontend — dispatched as three independent research agents scanning previously-unreviewed areas, then triaged and fixed directly. Every fix below is a confirmed, reproducible bug, not a style nitpick.

### 🐛 Backend

- **`POST /api/models/delete` never deleted the file.** It only removed the in-memory record; `POST /api/models/:name` (DELETE, a second route doing "the same thing") correctly removed the `.gguf`. Since `GET /api/models` re-scans the models directory on every call, a model "deleted" via the first route reappeared on the next refresh and disk space was never freed. Both routes now share one deletion path.

### 🐛 Cluster Health / Load Balancing / Auto-Tuning

- **`health.rs`: a node could almost never be marked `Failed`.** `get_health_status()` classified a node as `Degraded` if *either* delivery ratio or latency was still acceptable (an OR) — meaning `Failed` only triggered when *both* metrics were simultaneously catastrophic. A node with perfect delivery but 10x-over-threshold latency (or vice versa) stayed `Degraded` forever. Now requires both metrics within their degraded floor to stay `Degraded`; either one crossing it fails the node.
- **`load_balance.rs`: CPU-only clusters always reported needing rebalance.** `LoadBalanceConfig::autotuned()`'s CPU/generic tier used `min_load_threshold = 0.95`, below the mathematical minimum of `skew_ratio` (`max_available / min_available`, always `>= 1.0`). `rebalance()` returned `true` unconditionally for any CPU cluster with 2+ nodes, including a perfectly balanced one (skew_ratio pinned to exactly 1.0 when all nodes report 0 VRAM). Threshold raised to `1.02`, strictly above the floor.
- **`autotune.rs`: `load_cache()`'s in-memory fast path never checked the hardware fingerprint**, unlike `from_system_profile()` and this same function's own disk-fallback path — both of which do. A hardware change mid-process (GPU hot-plug/unplug, VRAM change) kept serving stale tuning forever once anything had populated the in-memory cache. Now validates against the current fingerprint before returning the in-memory entry, same as the disk path.

### 🐛 Launch Scripts

- **`scripts/run_native_llama_server_stack.sh` crashed on every fresh checkout.** `local` was used outside any function (a plain `if` block at script top level) — a hard error under this script's `set -euo pipefail`, aborting the entire from-scratch build branch, the only branch that ever runs on a first checkout.
- Same script's `wait_http()` also only checked for a bare 2xx status — the same false-positive class already fixed in `launch.sh` twice this cycle (a different service already bound to the target port fools a status-only check). Now supports the same content-marker verification (`"llamacpp"` for llama-server via `/v1/models`, `"inference_backend"` for the Ghostlink API).

### 🐛 Frontend

- **`api.loadModel()` (and `downloadModel()`) silently swallowed backend rejections.** The backend returns HTTP 200 with an `error` field in the body when it rejects a load (e.g. a catalog/placeholder model with no local `.gguf`) — it never throws for this case, so the code only checked in `catch`, never in the success path. Picking an undownloaded model looked identical to a real selection; the failure only surfaced later, opaquely, when chat was attempted. Both methods now check the body for an `error` field.
- **`ModelsTab.tsx` hardcoded a green "Ready" badge for every model**, including catalog placeholders with no local file — the backend marks those `status: "Ready"` too (status alone can't distinguish them). Fixed the *root* signal in `api.getModels()`: `usable` now requires either `status === 'Loaded'` or (`status === 'Ready'` AND a non-empty `local_path`). The badge now correctly shows "Not downloaded" for placeholders, with a tooltip on the "Use" button explaining why it may error.
- **`SecurityTab.tsx`'s "Refresh Token" button called `api.refreshJWT()`, a method that didn't exist** on the `GhostlinkAPI` class — clicking it threw a `TypeError` with no error handling, so `setLoading(false)` never ran and the button was stuck spinning forever (page reload required). Added the missing method and wrapped both this and the PQC-enable handler in `try/finally` so loading state always clears.

### ✅ Validation

- `cargo fmt --all --check` / `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo test -p ghost-link -p ghostlink-core` — all passing, run 3x with zero flakiness (including the previously-flaky `test_environment_manager_set_env`, not touched by this change)
- `tsc --noEmit`, `npm run build` (production build), `npx vitest run` — all clean, 104/104 frontend tests passing (one test failure surfaced and was resolved during this pass — see below)
- `bash -n scripts/run_native_llama_server_stack.sh` — syntax OK; the top-level-`local` fix verified directly against a minimal `set -euo pipefail` repro

### ⚠️ Process note

An earlier draft of the `ModelsTab.tsx` fix also disabled the "Use" button entirely for non-`usable` models. `ModelsTab.test.tsx`'s existing mock data (`usable: false` on a non-current model, with an assertion that clicking "Use" still calls `loadModel`) revealed this was the wrong scope — the intended design lets the request through so the now-fixed error surfaces clearly, rather than silently blocking it. Reverted to a tooltip-only affordance; the existing test caught this before it shipped.

---

## [1.3.6] - 2026-07-22 (GPU Backend Selection: DirectML/NPU/Vulkan)

### 🐛 GPU Backend Selection

- `ComputeBackend` (the enum backing `/api/backends` and `/api/backends/switch`) had no representation at all for DirectML, Vulkan, or NPU. `BackendRegistry::discover()`'s mapping from the shared `GpuBackend` detection collapsed both `Directml` and `Vulkan` onto `OneAPI` (a real, distinct technology — genuine Intel oneAPI/SYCL — unrelated to either), and silently dropped `Npu` entirely. Confirmed this directly affects real Windows hardware: `probe_windows_wmi_gpu()` tags AMD GPUs as `GpuBackend::Directml` via PCI vendor ID whenever the `rocm` feature isn't compiled in, and NPU-equipped hosts (e.g. AMD Ryzen AI, Intel Core Ultra) never saw their NPU listed as selectable at all. Added proper `Directml`, `Vulkan`, and `Npu` variants mirroring `GpuBackend` one-to-one.
- `discover()`'s backend-list dedup compared each new GPU's backend against `current` (which only ever held the *first* backend seen) instead of the GPU's own backend — silently dropping every subsequent distinct backend type on any host with more than one kind of accelerator. Now dedups correctly per-backend, with `current` set from the resulting list afterward.
- Even when correctly listed, `POST /api/backends/switch` unconditionally failed for `metal`/`oneapi`/`directml`/`vulkan`/`npu` with "No environment configuration for backend: X" — `SwitchingConfig::default()`'s env-var table only ever had entries for `rocm`/`cuda`/`cpu`, and `EnvironmentManager::set_backend_env`/`restore_env` treated a missing entry as a hard error rather than "this backend needs no special env vars" (true for all five — none of them need ghost-link to set anything the way ROCm/CUDA do). A backend could be discovered and listed as available, but never actually selected.

### ✅ Validation

- `cargo fmt --all --check` / `cargo clippy --workspace --all-targets -- -D warnings` — clean
- `cargo test -p ghost-link -p ghostlink-core` — 67 (+2 new) / 229 total passing, run 3x to confirm no new flakiness (the one observed failure was the pre-existing, already-tracked `test_environment_manager_set_env` race, untouched by this change — the new test here deliberately uses `Metal`, which touches no real env vars, to avoid adding to that surface)
- Live end-to-end: simulated a DirectML GPU and an NPU via env override on a running instance — `GET /api/backends` now correctly reports `"directml"`/`"npu"` (previously `"oneapi"`/absent), and `POST /api/backends/switch` now succeeds for both (previously failed for both)
- Confirmed VRAM flows correctly through the full pipeline once a GPU is properly detected: `/api/health`, `/api/metrics`, and `cargo run -- probe --full` all report the same VRAM figure consistently, and the auto-tuner responds to it correctly (256 max-inflight for a 12GB GPU vs. 128 for CPU-only, observed directly)

## [1.3.5] - 2026-07-22 (Launch: llama-server Port-Conflict Detection)

### 🐛 Launch Reliability

- Extended the port-conflict detection added in [1.3.4] to llama-server's own readiness check. A user hit a live chat failure — `Native error: llama_server request failed with status 405 Method Not Allowed: {"detail":"Method Not Allowed"}` — traced to `open-webui` (also FastAPI/uvicorn-based) already bound to `127.0.0.1:8080`, the same host:port llama-server wants. The old check (`GET /health` → `{"status":"ok"}`) was too generic to catch this: `open-webui` answers its own `/health` too, so `launch.sh` reported "llama-server ready" while the real llama.cpp process had actually failed to bind, and every native chat request silently went to `open-webui` instead.
- The readiness check now targets `GET /v1/models` and requires `"llamacpp"` (from llama.cpp's own `"owned_by":"llamacpp"` field) to appear in the response — nothing else plausibly returns that. On mismatch, prints a diagnostic naming the real cause and pointing at `GHOSTLINK_LLAMA_SERVER_PORT=<port>` as the override.
- Also fixed the diagnostic message itself to be accurate for whichever check failed: it previously always said "isn't Ghostlink" and suggested `GHOSTLINK_API_PORT`, which was wrong advice for the llama-server case (the correct override there is `GHOSTLINK_LLAMA_SERVER_PORT`, a different variable entirely). `wait_for_http()` now takes the expected-service label and the correct override variable as explicit arguments instead of hardcoding Ghostlink's own.

### ✅ Validation

- `bash -n launch.sh` — syntax OK
- Confirmed real llama-server's actual `/v1/models` response includes `"owned_by":"llamacpp"` (checked directly against a running instance)
- Reproduced the reported failure mode locally: a throwaway HTTP server on the llama-server port returning `{"status":"ok"}` to everything is now correctly rejected instead of accepted; a genuine llama-server on the same port is correctly recognized and accepted
- Confirmed the positive case (no conflict) is unaffected — full `launch.sh` run reaches healthy state, real chat inference succeeds end to end

---

## [1.3.4] - 2026-07-22 (Launch: Port-Conflict Detection)

### 🐛 Launch Reliability

- Root-caused a real user-reported failure: on one machine, `launch.sh` consistently reported `Ghostlink API ready` / `API /api/health ready` and then failed with `GET /api/settings ... HTTP 404` — even after the `cargo run` fallback rebuild (added in [1.3.3]) confirmed a live, correctly-routed process. Turned out an unrelated Python/uvicorn service was already bound to port 8003 on that machine, answering `/health` and `/api/models` with its own (different-shaped) 200 responses. `free_port`'s kill couldn't keep it off the port, and a bare "curl succeeded" was never proof that *Ghostlink* was what answered.
- `wait_for_http()` now accepts an optional content-marker argument: for the two `/health` and `/api/health` checks (both the initial and `cargo run`-fallback paths), it now requires `"inference_backend"` to actually appear in the response body, not just a 2xx status. Any unrelated service on the port — even one that respawns faster than `free_port` can evict it — is now correctly rejected instead of silently accepted, with a clear diagnostic naming the real cause and pointing at `GHOSTLINK_API_PORT=<port>` as an immediate workaround.

### ✅ Validation

- `bash -n launch.sh` — syntax OK
- Reproduced the exact failure locally with a throwaway Python HTTP server bound to port 8003 (both a one-shot and an auto-respawning variant, matching the reported symptom) — confirmed `wait_for_http` now correctly times out and reports the new diagnostic instead of a false "ready"
- Confirmed the positive case is unaffected: full `launch.sh` run with no port conflict reaches healthy state exactly as before, `/api/settings` returns `200`

---

## [1.3.3] - 2026-07-22 (Launch Script: Stale-Process Detection Hardening)

### 🐛 Launch Reliability

- `free_port()` previously relied entirely on `fuser`/`lsof`/`ss`/`netstat` being installed to find and kill a stale listener on a port before (re)starting a service. On a host with none of those tools, it silently did nothing. Added a tool-independent fallback (`proc_net_pids_for_port`) that parses `/proc/net/tcp{,6}` directly, so a stale process can be found and killed on any Linux host regardless of what's installed.
- The `cargo run` fallback path (used when the prebuilt API binary 404s on a route it predates) now verifies the replacement process is actually still alive (`kill -0`) after its health checks pass, before trusting them. Previously, if the old process was never actually killed (exactly the scenario above), the new process would fail to bind the already-occupied port and exit — but the health checks would keep silently succeeding against the still-running old process the whole time, only failing later with a confusing 404 on whatever route the stale binary happened to predate. This now fails fast with a diagnostic that names the real cause.

### ✅ Validation

- `bash -n launch.sh` — syntax OK
- `proc_net_pids_for_port` verified against a real listening process — correctly identifies its PID, matching `pgrep` ground truth
- `free_port` verified end-to-end — confirmed it terminates a test server and the port becomes unreachable afterward
- Full `launch.sh` run on the normal fast path (prebuilt binary works, no fallback triggered) — unaffected, reaches healthy state as before

---

## [1.3.2] - 2026-07-22 (GPU/CPU Auto-Discovery Fix & Worker Discovery)

### 🐛 Hardware Auto-Discovery

- Fixed a false-positive in `detect_gpu_from_env()` (`ghostlink-core/src/system_profile.rs`): the mere presence of `GHOSTLINK_VRAM_GB` (which `launch.sh` exports unconditionally, defaulting to `"0"` in CPU-only mode) was treated as an explicit GPU override, short-circuiting every real hardware probe (nvidia-smi/rocm-smi/WMI/lspci/Vulkan) and injecting a fake `"env-gpu"` device. `GET /api/health` reported `gpu_available: true` on pure-CPU hosts launched via the shipped launch scripts. Now requires a genuine signal (name, compute capability, or VRAM > 0).
- Added a regression case for the zero-VRAM-default scenario, folded into the existing `detect_gpu_handles_env_overrides` test (sequentially, not as a separate `#[test]`) — Rust runs tests in parallel by default, and process-global env vars mean two independent tests setting/clearing the same vars race regardless of what either asserts. A separate test was tried first and observed to fail intermittently under `cargo test --workspace` for exactly this reason.

### 🔌 Worker Discovery

- `GET /api/workers/discover` was a hardcoded stub returning `{"count": 2}`, disconnected from the real HMAC-authenticated UDP discovery module (`ghostlink_core::discovery`) already running in background threads. It now performs a real `broadcast_and_collect`, registers replies into the live `ClusterState`, and returns genuine counts.
- `GET /api/workers` now merges auto-discovered cluster peers with manually-added workers (previously showed only the latter), deduplicated by node id.

### 🔧 Launch Scripts

- `launch.bat`: fixed a 100%-reproducible failure where `%~dp0`'s trailing backslash broke WSL's argument parsing in `wsl wslpath -a "...\"` (bash saw an unterminated quote), causing every invocation to fail with "Failed to resolve repository path inside WSL."
- `launch.sh`: removed two blind, system-wide `pkill -f llama-server` / `pkill -f ghost-link` calls in favor of the already-correct port-scoped `free_port` cleanup — the blind form could kill an unrelated process from a different user or session sharing the same binary name. Now also passes the real detected GPU name through (`GHOSTLINK_GPU_NAME`) when a vendor is actually found, so the env override reports accurate hardware instead of a generic placeholder when it legitimately applies.

### ✅ Validation

- `cargo fmt --all --check` — OK
- `cargo clippy --workspace --all-targets -- -D warnings` — OK
- `cargo test -p ghost-link -p ghostlink-core` — 229/229 passing
- `cargo test --workspace` — passing; also surfaced a pre-existing, unrelated flaky test (`runtime_switcher::tests::test_environment_manager_set_env`, same process-global-env-var race pattern, not touched by this change) — flagged separately, not fixed here to keep this PR scoped
- `cargo run -p ghost-link -- probe my-node --full` — no regression (correctly reports `GPU: cpu`, `GPU VRAM: 0.0 GB`, `Acceleration: AVX-512` on this CPU-only host)
- Live end-to-end verification on Windows 11 + WSL2 (AMD Ryzen AI 7 350, no functioning GPU driver in-guest): both `launch.sh` and `launch.bat` reach healthy state; `/api/health` correctly reports `gpu_available: false`; native↔Ollama runtime switching verified with two models each (SmolLM2-360M-Instruct + stories15M native, smollm2:135m + qwen2.5:0.5b via Ollama), each producing distinct, correctly-attributed, real inference responses; `/api/backends/switch` succeeds for `cpu` and cleanly rejects `cuda`/`rocm` as unavailable

### ⚠️ Known Caveats (host-specific)

- GPU-accelerated inference was **not** verified on real GPU hardware in this change — no CUDA/ROCm/functioning-Vulkan device was available in the test environment (WSL2 exposes the GPU device node but this Ubuntu image's Mesa build lacks the D3D12/"dozen" driver needed to bridge to it, and `/dev/dri` is absent). The fixed code paths are covered by existing unit tests (`infer_backend_cuda`, `infer_backend_rocm`, etc.) but not exercised against physical GPU hardware.
- The React frontend (GUI) was verified only through its backend API surface (curl/HTTP), not by driving the rendered UI in a browser.

---

## [1.3.1] - 2026-07-22 (Launch Reliability & CI Stabilization)

### 🔧 Launch Hardening

- `launch.sh` now auto-recovers when a stale prebuilt API binary responds with mismatched routes:
  - On `404`/`405` from critical route checks (`/api/settings`, `/api/models`), launcher stops the stale process.
  - Launcher retries API startup via `cargo run -p ghost-link -- serve ...` and re-validates route health.
- Preserves strict route validation while preventing false-negative startup failures caused by outdated local binaries.

### 🩹 Backend API Stability

- Removed duplicate `handle_gui_model_download_progress` implementation in `crates/ghost-link/src/main.rs`.
- Removed duplicate `GET /api/models/download/progress` route registration and duplicate route-list print.
- Eliminated startup panic from overlapping Axum route registration and restored clean API boot for smoke tests.

### ✅ Validation

- `cargo fmt --all --check` — OK
- `cargo clippy --workspace --all-targets -- -D warnings` — OK
- `cargo test --workspace` — OK
- `cargo audit` — completed with existing allowed advisory warnings
- `python3 scripts/ci_gui_backend_smoke.py` — OK

---

## [1.3.0] - 2026-07-19 (Performance Overhaul & Auto-Discovery)

### 🚀 Performance

#### SPSC Ring Buffer Spin-Wait
- Replaced OS scheduler `yield_now()` polling with exponential-backoff spin-wait (`wait_for_data()` / `wait_for_space()`)
- Stage threads now stay hot on core — no scheduler trip during hot-path communication
- In-process pipeline throughput: **866K tok/s** at 1024 tokens (1.18 ms latency)

#### `target-cpu=native` Compilation
- `.cargo/config.toml` enables `-C target-cpu=native` for automatic CPU feature utilization
- AVX-512, AVX2, FMA, and other ISA extensions enabled without manual flags

#### Unix Domain Socket Transport
- New `TransportKind::Unix` variant alongside existing `Tcp`
- `BridgeListener`, `BridgeStream`, `BridgeAddr` enums wrapping platform-specific types
- Socket path: `%TEMP%/ghostlink-bridge-{stage}.sock`
- Linux/macOS only (runtime error on Windows)
- TCP loopback benchmark: **497K tok/s** at 1024 tokens

#### Pipeline Benchmarking
- Added per-phase breakdown (recv / compute / send) to all transport benchmarks
- Benchmarks confirm ~98% of pipeline latency is OS scheduling overhead, not data movement

### 🧠 Auto-Discovery & System Profile

#### Unified SystemProfile
- Cross-platform hardware detection (CPU, GPU, NPU) consolidated into `system_profile.rs`
- Memory detection via `/proc/meminfo` (Linux), `sysctl` (macOS), WMI (Windows)
- Env overrides: `GHOSTLINK_SYSTEM_MEMORY_GB`, `NPU_DEVICE`, `QUALCOMM_NPU`

#### AutoTuner with Persistent Cache
- Hardware fingerprinting with JSON cache file
- Tunable parameters (batch sizes, worker counts, chunk sizes) derived from detected hardware
- Cache invalidates on hardware change
- Wired into `probe` CLI command

#### Dynamic SystemProfileWatcher
- Background thread polls hardware state every N seconds
- Detects hot-plug GPU/NPU changes at runtime
- Feeds into health monitor and load balancer for live reconfiguration
- Subscribe/notify pattern for downstream consumers

### 🔒 Session-Level Transport Authentication

- Transport frames now carry session keys
- Mismatched auth tokens are rejected at the protocol level
- Configurable via `auth_token` in `ghostlink.toml` `[tcp]` section

### 🔧 Backend Switching & API

- New `/api/backend/status` endpoint — reports current backend + available backends
- New `/api/backend/switch` endpoint — switch inference backend at runtime
- Backend registry refactored to delegate detection to `SystemProfile`
- `RuntimeDetector` and `BackendRegistry` now source hardware info from unified profile

### 🧪 CI & Quality

- Cross-platform CI matrix: **ubuntu-latest**, **windows-latest**, **macos-latest**
- Formatting and clippy enforcement on all three platforms
- MSRV pinned at **1.85.0** with `rust-version` field in `Cargo.toml`
- All 216 tests pass across all targets

### 🐛 Clippy Fixes

- 8 lints resolved across 3 crates:
  - `needless_range_loop` → `iter_mut().enumerate().take()`
  - `redundant_closure_call` → inline block expression
  - `collapsible_if` → combined condition
  - `clone_on_copy` (5 instances) → removed redundant `.clone()` calls
  - `redundant_pattern_matching` → `.is_some()` idiom
  - `unreachable_code` → extracted cfg-gated platform functions
  - `unused_import` → cfg-gated Unix import

### ✅ Build Verification

- `cargo fmt --all --check` — **OK**
- `cargo clippy --workspace --all-targets -- -D warnings` — **OK**
- `cargo test --workspace` — **216/216 passed**
- `cargo bench --package ghostlink-core` — **baseline updated**

---

## [1.2.1] - 2026-07-18 (Repository Cleanup)

### 🧹 Documentation Hygiene

- Moved obsolete root remediation docs into `docs/archive/legacy-root-docs/` so the repository root keeps only active reference material.
- Added `docs/archive/TESTING.md` as the archived pointer for the live top-level testing guide.
- Updated `README.md` to prefer `launch-complete.sh` for the Linux/macOS full-stack launch path.
- Kept the changelog and archive index aligned with the current documentation layout.

### ✅ Verification

- Local workflow-equivalent validation is run after this cleanup to confirm the repo remains green.

## [1.2.0] - 2026-07-15 (Reliability & Resilience)

### 🛡️ API Reliability Hardening

#### Frontend API Client (`ghostlink_gui_modern/src/api.ts`)
- **Retry logic**: Exponential backoff (3 retries, 1s base delay, 30s max) for 5xx, 429, 408 errors
- **Circuit breaker**: Opens after 5 failures, 30s timeout, half-open state after 2 successes
- **Request deduplication**: Identical GET requests within 5s window share single response
- **URL validation**: Trims whitespace, validates protocol/host — fixes trailing space bug from Session 5
- **Structured errors**: Typed `ApiError` with status, code, retryable flag

#### Frontend Error Boundaries & Resilience
- **`ErrorBoundary`**: Catches React errors, shows retry button + error details
- **`OfflineBanner`**: Auto-shows on network disconnect, auto-hides on reconnect
- **`useApiRetry` hook**: Generic retry wrapper with configurable backoff
- **`useOnlineStatus`**: Browser online/offline event listener
- **`useApi`**: Retry-wrapped versions of all 25 API methods

#### Config Validation
- **`src/config.ts`**: Zod schema for all 25 settings with validation rules
- **`validateEnvVars()`**: Runtime check for `VITE_GHOSTLINK_API_BASE` format

### 🔧 Launch Script Hardening
- **`launch-complete.bat`**: Pre-flight validation (URL format, required commands), trims `VITE_GHOSTLINK_API_BASE`, waits for `/api/health` endpoint
- **`launch-complete.sh`**: Same validation + mirror download support (hf-mirror.com), resume capability (Range headers), SHA256 verification
- **`launch.sh`**: Added `/api/health` readiness check

### 🔧 Backend Resilience (`crates/ghost-link/src/main.rs`)
- **`/api/health` endpoint**: Returns `gpu_available`, `inference_backend`, `native_engine`
- **`/health` endpoint**: Enhanced with GPU availability detection (NVIDIA/AMD/Apple)
- **Model downloads**: Mirror fallback (hf-mirror.com), HTTP Range resume, checksum verification
- **Metrics**: Added `gpu_available` field for graceful degradation

### 🧪 Integration Tests & Monitoring
- **`tests/integration/reliability.test.ts`**: 16 tests for URL validation, retry delays, retryable errors
- **`src/config.test.ts`**: 21 tests for Zod config schema validation
- **All existing tests pass**: 94 frontend + 28 backend = 122 total

### ✅ Build Verification
- `npx vitest run` — **94/94 passed**
- `npx tsc --noEmit` — **OK**
- `cargo fmt --all --check` — **OK**
- `cargo clippy --workspace --all-targets -- -D warnings` — **OK**
- `cargo test --workspace` — **122/122 passed**

---

## [1.1.0] - 2025-07-14 (Runtime Fixes & Performance)

### 🚀 Features

#### Model Management Enhancements
- **Real llama-server integration** — Model loading now spawns llama-server with correct GPU layers (`-ngl`), threads, and context size
- **Proper model unload** — Kills llama-server process, resets to simulated mode, cleans environment variables
- **Model download with progress** — Real-time download progress via `/api/models/download/progress`
- **HuggingFace model search** — Search and download GGUF models directly from UI

#### Runtime Detection & Selection
- **Enhanced hardware detection** — AMD GPU (DirectML/Vulkan), NPU (Ryzen AI/XDNA), Intel ARC, NVIDIA CUDA
- **Runtime selection API** — `/api/runtime/select` to switch between CPU, DirectML, Vulkan, CUDA, ROCm, Metal, NPU
- **Model recommendations per runtime** — `/api/runtime/recommend` suggests models fitting available VRAM/memory
- **Models by runtime** — `/api/runtime/models?runtime=directml` filters compatible models

#### Real System Metrics
- **Real system metrics** — CPU usage, memory %, GPU utilization, GPU memory via WMI/nvidia-smi/rocm-smi
- **Latency tracking** — Real P50/P95 latency from actual inference runs
- **Throughput metrics** — Tokens/sec from actual llama-server execution

#### Settings Persistence
- **Full settings persistence** — Temperature, max_tokens, ngl, threads, ctx_size, penalties all saved to `settings.json`
- **Live settings API** — GET/POST `/api/settings` with immediate effect

### 🐛 Critical Fixes

#### Chat Inference
- **Fixed simulated responses** — Chat now uses llama-server for real inference when model is loaded (`real_inference: true`)
- **Fixed URL malformation** — llama-server URL properly constructed with port and path
- **Fixed environment propagation** — Launch scripts now set `GHOSTLINK_NATIVE_ENGINE=llama_server` before starting API

#### Launch Scripts
- **Port conflict detection** — Both `launch.bat` and `launch-fast.bat` check for port conflicts before starting
- **Environment variable propagation** — Fixed `start` command env var passing in batch scripts
- **Health check ordering** — Waits for llama-server → API → GUI in correct order
- **Port availability checks** — Prevents "address already in use" errors

#### Model Management
- **Fixed model loading race condition** — Checks if llama-server already running before spawning new instance
- **Fixed model path resolution** — Correctly resolves local GGUF paths from `models/` directory
- **Fixed model status tracking** — Properly tracks "Loaded" vs "Ready" vs "Downloading" states

#### Runtime Detection
- **AMD NPU detection** — Detects Ryzen AI / XDNA NPUs via WMI PnPEntity queries
- **DirectML detection** — Finds AMD/Intel GPUs via Win32_VideoController on Windows
- **Vulkan detection** — Validates `vulkan-1.dll` presence for AMD/Intel GPU acceleration

### 📊 Performance Improvements

- **CPU inference optimized** — AVX-512 backend achieves ~850K tokens/sec on stories15M model
- **llama-server reuse** — Reuses running llama-server when switching models instead of restarting
- **Reduced launch time** — `launch-fast.bat` skips cargo build when binary exists
- **Health check optimization** — Faster health check intervals with exponential backoff

### 📚 Documentation Updates

- **README.md** — Complete rewrite with current architecture, hardware detection table, launch scripts, API endpoints, env vars
- **CHANGELOG.md** — This entry
- **API documentation** — Updated with all new endpoints

### 🔧 Build System

- **llama.cpp Vulkan build** — `GGML_VULKAN=ON` for AMD GPU acceleration (requires Vulkan SDK)
- **CPU fallback** — CPU build with AVX-512/AVX2/FMA works out of the box
- **llama-server binary** — Built at `third_party/llama.cpp/build/bin/Release/llama-server.exe`

### 🐛 Bug Fixes

| Issue | Fix |
|-------|-----|
| Chat returned placeholder text | Fixed native engine to call llama-server HTTP API |
| Model unload didn't kill llama-server | Now kills child process and resets env vars |
| Port conflicts on restart | Launch scripts check netstat before binding |
| Settings not persisting | Added `save_settings` call to all update paths |
| Runtime selection ignored | Added `/api/runtime/select` endpoint |
| NPU not detected | Expanded WMI PnPEntity keyword search |
| Model download silent failure | Added progress endpoint and error handling |

---

## [1.0.0] - 2024-12-19 (Production Release)

### ✨ Features

#### Distributed Inference Fabric
- Zero-copy SPSC ring buffers for DMA-style hand-off
- Binary protocol with CRC32 checksums for frame integrity
- TCP transport with configurable max inflight batches
- AF_XDP kernel bypass support (with graceful fallback)
- Layer assignment with fault tolerance
- Network health monitoring and load balancing

#### Chat Tab
- Model selector dropdown (filters usable models only)
- Real-time parameter controls (Temperature, Top-P, Top-K, Penalty, Max Tokens)
- System prompt customization
- **NEW**: 8 built-in tools integration
- **NEW**: Custom MCP server support
- Live streaming responses

#### Models Tab
- Browse local models with real-time status display
- Load/Unload/Delete operations
- HuggingFace integration (10 popular models pre-loaded)
- Search and filter capabilities
- One-click download from HuggingFace
- Model details (size, type, quantization, status)

#### Metrics Tab
- **NEW**: Live digital gauge dashboard
- 6 real-time metrics updating every 5 seconds
- Throughput (requests/second)
- CPU, Memory, GPU usage
- Latency P50 and P95 percentiles
- Color-coded health indicators (Green/Yellow/Red)
- Raw JSON data display
- Smooth SVG animations

#### Sessions Tab
- Active session monitoring
- Real-time statistics
- Cancel sessions capability
- Session details display

#### Workers Tab
- Worker node management
- Add workers (host:port)
- Peer discovery functionality
- Network health monitoring
- Load visualization
- Disconnect workers
- Online/offline status tracking

#### Security Tab
- Digital vault interface
- JWT token management with countdown timer
- Post-Quantum Cryptography (PQC) support
- Security level indicator
- Comprehensive audit logging
- Security recommendations

#### Tools & MCP Support
- **NEW**: 8 built-in tools:
  - web_search
  - calculator
  - code_execution
  - file_operations
  - terminal
  - database_query
  - api_call
  - image_generation
- **NEW**: Custom MCP server integration
- Enable/disable tools per prompt
- Add/remove MCP servers via UI
- Tool execution tracking
- Response includes "Tools used" information

### 🐛 Critical Fixes (Production Release)

#### GUI Component Fixes
- **[HIGH]** ChatTab: Captured input message before clearing state, preventing empty API calls
- **[HIGH]** WorkersTab: Added 5-second polling interval for real-time updates
- **[HIGH]** WorkersTab: Added disconnect handler for power button click events
- **[HIGH]** App.tsx: Fixed apiBase initialization to enable backend auto-discovery

#### Configuration Fixes
- **[MEDIUM]** vite.config.ts: Added proxy configuration for CORS support
- **[LOW]** .env.example: Created environment variable template with secure defaults

### 🔒 Security Hardening

- Secrets baseline configured (`.secrets.baseline`)
- No hardcoded credentials in source code
- Input validation on all API endpoints
- Rate limiting ready (configurable via env vars)
- Tool execution sandboxed
- File operations restricted to designated directories
- MCP server validation before use

### 📊 Performance Enhancements

- TCP autotune for optimal inflight batches
- XDP kernel bypass support with graceful fallback
- Zero-copy SPSC ring buffers validated
- Layer assignment with fault tolerance
- Comprehensive metrics tracking (throughput, latency percentiles)

### 📚 Documentation Improvements

- Added `PRODUCTION_READINESS.md` - Complete production checklist
- Added `RELEASE_SUMMARY.md` - Release notes and features
- Added `FINAL_PRODUCTION_REPORT.md` - Comprehensive assessment report
- Updated README with native llama-server mode guide
- Added troubleshooting guides for common issues
- Comprehensive API documentation

### 🚀 Launch & Deployment

#### Auto-Launch Scripts
- `launch-complete.sh` - One-command startup (Linux/macOS)
- `launch-complete.bat` - One-command startup (Windows)
- `scripts/run_native_llama_server_stack.sh` - Native inference mode
- Backend auto-detection and dependency auto-install
- Browser auto-open and service URL display

#### Docker Compose
- Complete production stack (`docker-compose.production.yml`)
- Launch compose (`docker-compose.launch.yml`)
- Test compose (`docker-compose.test.yml`)
- Health checks configured
- Data persistence volumes
- Auto-restart policies
- Network isolation

### 🔧 Build System

- Release binaries: `cargo build --release`
- Multi-stage Dockerfile for minimal images
- Non-root users in production images
- Vite build (75 KB gzipped)
- Reproducible builds with `Cargo.lock` and `package-lock.json`

### 📦 Architecture

#### Frontend
- React 18 with TypeScript
- Tailwind CSS styling
- Zustand state management
- Vite 5 build tool
- 100% type-safe codebase

#### API Server
- Axum + Rust backend
- OpenAI-compatible API endpoints
- Tool dispatcher for built-in tools
- Native llama.cpp integration

#### Core Runtime
- Shared primitives in `ghostlink-core`
- Zero-copy ring buffers
- Cluster state management
- Planning and fault tolerance

### 📚 Documentation

- README.md - Feature overview and quick start
- CHANGELOG.md - Version history
- PRODUCTION_READINESS.md - Production checklist
- RELEASE_SUMMARY.md - Release notes
- FINAL_PRODUCTION_REPORT.md - Comprehensive assessment report
- QUICK_REFERENCE.md - Command reference
- LAUNCH_GUIDE.md - Deployment guide
- TOOLS_AND_MCP_GUIDE.md - Tool integration
- TESTING.md - Test commands and CI checks

### 🧪 Testing

- Rust unit tests passing
- GUI test suite (25 tests) all passing
- Clippy linting with no warnings
- Code formatting compliant
- Production gate workflow comprehensive

### 🔒 Security

- Sandboxed tool execution
- File operation restrictions
- Safe command subset
- Rate-limited API calls
- MCP server validation
- No secrets in frontend code
- JWT token management
- Post-Quantum Cryptography (PQC) support

---

## Features by Category

### Chat Capabilities ✅
- [x] Model selection
- [x] Parameter tuning
- [x] System prompts
- [x] Tool integration
- [x] MCP servers
- [x] Live responses

### Model Management ✅
- [x] Load/unload/delete
- [x] Local browsing
- [x] HuggingFace search
- [x] One-click download
- [x] Status display

### Monitoring ✅
- [x] Live metrics (6 gauges)
- [x] 5-second refresh
- [x] Health indicators
- [x] Session tracking
- [x] Worker monitoring
- [x] Network health

### Tools ✅
- [x] 8 built-in tools
- [x] Tool selection UI
- [x] MCP servers
- [x] Tool execution
- [x] Response tracking

### Deployment ✅
- [x] Auto-launch scripts
- [x] Docker image
- [x] Docker Compose
- [x] Health checks
- [x] Data persistence

### Security ✅
- [x] JWT management
- [x] PQC support
- [x] Audit logging
- [x] Security vault
- [x] Sandboxed execution

---

## API Endpoints

```
GET  /health                          ✅ Health check
GET  /api/models                      ✅ List models
POST /api/models/load                 ✅ Load model
POST /api/models/download             ✅ Download model
POST /api/models/{name}/unload        ✅ Unload model
DELETE /api/models/{name}             ✅ Delete model
POST /api/inference/chat              ✅ Chat completion
GET  /api/metrics                     ✅ Performance metrics
GET  /api/sessions                    ✅ List sessions
POST /api/sessions/{id}/cancel        ✅ Cancel session
GET  /api/workers                     ✅ List workers
POST /api/workers/add                 ✅ Add worker
POST /api/workers/connect             ✅ Connect worker
GET  /api/workers/discover            ✅ Discover workers
GET  /api/runtime/detect              ✅ Detect runtimes
POST /api/runtime/select              ✅ Select runtime
GET  /api/runtime/models?runtime=X    ✅ Models by runtime
GET  /api/runtime/recommend           ✅ Model recommendations
GET  /api/models/search/huggingface   ✅ Search HF models
GET  /api/models/status               ✅ Model status
GET  /api/ollama/health               ✅ Ollama health
POST /api/settings                    ✅ Update settings
GET  /api/settings                    ✅ Get settings
POST /api/runtime/recommend           ✅ Recommend models
```

---

## Browser Compatibility

| Browser | Min Version | Status |
|---------|------------|--------|
| Chrome | 90 | ✅ Full |
| Firefox | 88 | ✅ Full |
| Safari | 14 | ✅ Full |
| Edge | 90 | ✅ Full |
| Mobile | iOS 14+ | ✅ Responsive |

---

## Node.js Requirements

- **Node.js**: 18.0.0+
- **npm**: 9.0.0+

---

## Rust Requirements

- **Rust**: 1.85.0 minimum (MSRV)
- **edition**: 2021
- **Cargo.lock**: Committed for reproducible builds

---

## Known Limitations

- MCP servers must be accessible from client (same network)
- Tool execution timeout varies by tool complexity
- File operations limited to designated directories (sandboxing)
- Code execution: Python sandbox (60s timeout, 512MB memory limit)
- Worker operations simulated in single-node mode (no real distributed cluster)

---

## Roadmap (Post v1.1.0)

### v1.2.0 - Analytics Release
- [ ] Export metrics to CSV/JSON
- [ ] API key management UI
- [ ] Rate limiting dashboard

### v1.3.0 - GPU Release
- [ ] Vulkan build pipeline in CI
- [ ] AMD GPU benchmark suite
- [ ] NPU support for Ryzen AI

### v2.0.0 - Major Release
- [ ] WebSocket real-time updates (vs polling)
- [ ] Multi-user support with authentication
- [ ] Real distributed cluster support

---

## Version History

| Version | Date | Status | Notes |
|---------|------|--------|-------|
| 1.3.0 | 2026-07-19 | ✅ Release | Performance overhaul, auto-discovery, Unix sockets, auth, CI matrix |
| 1.2.1 | 2026-07-18 | ✅ Release | Repository cleanup, docs hygiene |
| 1.2.0 | 2026-07-15 | ✅ Release | Reliability, retry, circuit breaker, config validation |
| 1.1.0 | 2025-07-14 | ✅ Release | Runtime fixes, model load/unload, real inference, runtime selection, real metrics |
| 1.0.0 | 2024-12-19 | ✅ Production | All critical bugs fixed, production hardened |
| 0.x | - | ❌ Archived | Alpha development phase |

---

## Credits

Built with:
- Rust 1.85.0+
- React 18
- TypeScript 5.3+
- Tailwind CSS 3.4+
- Vite 5
- Zustand 4.4+
- Axum 0.7
- Ollama (optional)
- llama.cpp (optional native mode)

---

## License

MIT License - See LICENSE file for details

---

**Status**: ✅ Production Ready  
**Last Updated**: 2026-07-19  
**Maintainer**: Ghostlink Team  

(End of file)