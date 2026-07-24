# CHANGELOG

All notable changes to Ghostlink Studio are documented here.

---

## [1.4.2] - 2026-07-24 (Correctness sweep, GPU offload fix, hardware-detection latency, flaky-test root cause)

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
  is still non-empty.
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
  benchmark file unusable end-to-end. Reduced to 10 iterations.
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
  --all-targets` (zero warnings), `cargo test --workspace` (83 + 134 + 7 + 28
  + 19, all passing) — checked repeatedly (11+ consecutive full-workspace
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