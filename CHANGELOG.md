# CHANGELOG

All notable changes to Ghostlink Studio are documented here.

---

## [Unreleased]

### Fixed
- **Tool schemas no longer cost ~3,800 prompt tokens on every request.** `build_tool_instructions` inlined every enabled tool's complete JSON input schema into the prompt prefix, and that prefix is prepended to *every* turn (`format!("{tool_instructions}Question: {user_message}")`). Measured by proxying real request bodies: a five-word question produced a **17,149-character** user message and **3,814 prompt tokens**, of which ~3,700 were the tool catalog — about 13 seconds of prefill at the measured rate before the model read the question.

  Tool entries are now compact signatures:

  ```
  - read_file(required: path; optional: content, encoding) — Read a file from disk.
  ```

  Full schemas remain reachable through `tool_schema_detail`, and a tool whose schema is not a shape we can summarise falls back to the full schema for that one tool rather than being reduced to nothing.

  This was invisible to every benchmark written so far, because all of them used long synthetic prompts where 3,800 tokens of fixed overhead is noise against 5,000 tokens of user text. The overhead only shows when the user text is short, which is the normal case in a chat GUI.

  Measured, same request, before and after:

  | | prompt tokens | prefill | wall |
  |---|---|---|---|
  | before | 3,814 | 292 tok/s | ~13.1s of prefill |
  | after | **1,047** | **~394 tok/s** | ~5.9s |

  Tool calling verified working after the change: `tools_run: 1` on a real `read_file` with correct arguments, and a workspace-boundary probe correctly returned a denial rather than escaping the sandbox.


### Added
- **Speculative decoding is now wired to `llama-server`.** `GHOSTLINK_DRAFT_MODEL`, `GHOSTLINK_DRAFT_MAX` and `GHOSTLINK_DRAFT_P_MIN` were documented in `LOCAL_INFERENCE_TUNING.md` with a specific promise about which flags they set, and **no code in the repository read any of them** -- setting them did nothing.

  The documented flag names were also stale for this build. `llama-server --help`:
  ```
  --draft, --draft-n, --draft-max N   the argument has been removed.
                                     use --spec-draft-n-max or ...
  ```
  Passing `--draft-max` as documented would make `llama-server` exit on an unknown argument. Now passes `--spec-draft-model`, `--spec-draft-n-max`, `--spec-draft-p-min`.

  Off by default, and not enabled automatically: a draft model close in size to the target costs more than it saves. A `GHOSTLINK_DRAFT_MODEL` that does not exist disables the feature with a log line rather than failing the load -- the primary model is fine and this was only ever an optimization.

- **`-t` now uses performance cores on a hybrid CPU.** `SystemProfile::cpu.performance_cores` was measured on every probe and never read; `get_threads` returned `available_parallelism`. Measured on this host (Ryzen AI 7 350, `-ngl 0` so the question is genuinely CPU-bound, 4 runs, medians):

  | `-t` | prefill | decode |
  |---|---|---|
  | 4 | 254.2 tok/s | 22.36 tok/s |
  | 8 (P-cores + SMT) | 254.1 tok/s | 21.55 tok/s |
  | 16 (all logical) | 246.2 tok/s | 20.18 tok/s |

  **1.07x decode.** Modest rather than 2x -- llama.cpp's own thread scaling is good -- but it was not being applied at all. `-t 8` rather than `-t 4`: `performance_cores` is a physical count, so it is scaled by `logical / physical` to keep SMT on the performance cores. Falls back to `available_parallelism` unchanged when there is no real P/E split, since halving threads on a uniform-core CPU would be a large invisible regression.

### Changed
- **`docs/LOCAL_INFERENCE_TUNING.md` records the draft-model flags that actually exist**, and notes that the host's CPU topology is not what its name suggests: "Ryzen AI 7 350" reads as uniform-core, but `GetLogicalProcessorInformationEx` reports 8 processor-core records with `EfficiencyClass = [1,0,1,0,1,0,1,0]` -- 4 performance, 4 efficiency. Read, not assumed.
- **Hardened compose stacks now keep auth/state on writable mounts instead of tmpfs.** `docker-compose.yml`, `docker-compose.launch.yml`, and `docker-compose.production.yml` keep `read_only: true` but move API key/key-store and durable session/schedule/approval paths onto mounted volumes (`/shared`, `/state`) so normal restarts do not rotate credentials or erase runtime state.


### Added
- **Single-node knob A/B harness** (`scripts/llama_knob_ab.py`): A/Bs batch size, Flash Attention and KV-cache precision against the vendored `llama-server`, one variable at a time, on a single node. Prefill and decode are reported separately and a `-` is printed for anything llama.cpp did not report.

  Item 4 of the inference audit is a list of knobs with upstream numbers attached. This makes each one checkable instead of assumed.

  Waits for `/completion` to answer rather than for the port to bind. llama-server binds while the model is still loading and returns HTTP 503 until it is done, so a socket check reports "ready" immediately and the first measured request dies -- which is exactly what the first version of this script did.

### Changed
- **`docs/LOCAL_INFERENCE_TUNING.md` no longer claims `-b 2048 -ub 512` is the difference between ~70 and 300+ prompt tok/s.** Measured on this project's reference hardware (Radeon 860M / Vulkan / 16.4GB, Llama-3.2-3B Q3_K_M, 3,372-token prompt where prefill dominates, 4 runs, medians):

  | config | prefill | decode |
  |---|---|---|
  | `-b 512 -ub 128` | 303.0 tok/s | 12.55 tok/s |
  | `-b 2048 -ub 512` | 327.2 tok/s | 12.77 tok/s |
  | `-b 2048 -ub 2048` | 321.0 tok/s | 12.71 tok/s |

  **+8% prefill**, not 4x. That figure is presumably about discrete-GPU CUDA with a much larger batch; it does not reproduce here. `-ub 2048` is slightly worse than `-ub 512`, so raising it further buys nothing.

  Prompt size changes the sign of the result. At 893 prompt tokens the same change measured as a *regression* (0.62x prefill, 3.04x slower decode) because wall time there is decode-dominated and the larger batch just holds more VRAM. Neither number is wrong; averaging them is.


### Added
- **Tensor-class planning for distributed offload** (`crates/ghost-link/src/tensor_plan.rs`): `-ts` says *how much* goes to each device; it does not say *which tensors*. llama-server therefore spreads whole layers -- attention, KV cache and `lm_head` included -- across the link, paying a network round trip per token for tensors that are tiny or reused constantly. The new planner routes FFN and MoE experts remotely and pins embeddings, attention, KV and `lm_head` locally, with a latency term that refuses a peer whose RPC RTT exceeds the local CPU-offload penalty.

  `GHOSTLINK_RPC_RTT_MS` and `GHOSTLINK_CPU_OFFLOAD_PENALTY_MS` configure the comparison.

- **`-ot` now actually reaches `llama-server`.** `GHOSTLINK_LLAMA_OVERRIDE_TENSOR` already named the right split (`ffn=RPC,exps=RPC`) but existed only as a string literal inside a unit test -- there was no code path from it to the process. The plan is now computed at peer discovery and passed through `load_model_into_slot`.

### Fixed
- **Distributed inference no longer hands `llama-server` flags it rejects.** This repo's vendored `llama.cpp` was built with `GGML_RPC:BOOL=OFF`, verified three ways:
  ```
  $ llama-server ... --rpc 127.0.0.1:59999
  error: invalid argument: --rpc          (exit 1)
  $ grep GGML_RPC build/CMakeCache.txt
  GGML_RPC:BOOL=OFF
  $ grep -- --rpc <(llama-server --help)  -> no match
  ```
  A load with `distributed_inference` on was being passed `--rpc` and `-ts` and failed with a bare `invalid argument: --rpc`, which reads like a malformed command line rather than a build without distributed-inference support. `native_engine::binary_supports_rpc` now probes the configured binary once and returns a specific, actionable error naming `GGML_RPC=ON`.

  This was never caught because the RPC path has never been exercised on this machine. It is a fix to the failure mode, not a claim that distributed inference now works -- it cannot, until llama.cpp is rebuilt with RPC enabled.

- **`-ot` buffer types are addressed the way llama.cpp resolves them.** `arg.cpp:272` resolves every `-ot` value against the buffer types of *registered* devices and throws `unknown buffer type` otherwise; `ggml-rpc.cpp:1092` builds a remote device's name as `RPC0[endpoint]`. The bare word `RPC` was tested against the real binary and rejected:
  ```
  $ llama-server -m model.gguf -ot ffn=RPC
  error while handling argument "-ot": unknown buffer type
  ```
  The emitted value is now `RPC0[<endpoint>]`, built from the same peer address passed to `--rpc`.

- **"Fits locally" now accounts for the KV cache** (`rpc_cluster::model_fits_locally`). The check was `model_size + 1.0 <= vram`, which passes for a model whose *weights* fit but whose KV cache does not -- and that is a failed model load, not a slow one. Now weights + KV (scaled by context length) + 1 GB headroom.

- **`TCP_NODELAY` on both legs of the RPC allowlist proxy.** `ggml-rpc` moves small, latency-critical frames -- one per matmul, one result per token. Nagle's algorithm can hold a small write for up to the delayed-ACK timeout (~40ms) in each direction, and forwarding through a userspace proxy built on default sockets silently reintroduced it after llama.cpp had set it on its own. Best-effort: a socket that cannot be configured still works, just slower, and this is never a reason to drop a legitimate peer. Security checks are unchanged.

  Scoped honestly: the proxy only runs when `rpc_allowed_peers` or `rpc_shared_secret` is configured. With both empty, `ggml-rpc-server` binds the public address directly and there is no data path to slow down.


### Added
- **Sliding window for chat history** (`crates/ghost-link/src/context_window.rs`): the audit's item 5 said sliding window / compact policies were "documented and not implemented on the chat path". `GHOSTLINK_KEEP_LAST_TURNS` did not exist anywhere in the codebase. Now there is a window with a completion reserve and a bounded keep-last floor, plus `GHOSTLINK_KEEP_LAST_TURNS` (default 4) and `GHOSTLINK_COMPLETION_RESERVE_TOKENS`.

  Every turn re-evaluated the whole history against the token ceiling. With prefill measured at 245 tok/s, a long conversation paid a ~21.7s prefill on *every* turn, so TTFT grew without bound until the ceiling finally bit.

- **History budget clamped to the context the model actually runs** (`native_engine::probe_running_ctx_size`). `conversation_token_limit` is a user preference; the running context is derived from VRAM and model size. They disagreed, and budgeting against the preference alone produced requests the model rejects outright:
  ```
  request (18848 tokens) exceeds the available context size (8192 tokens)
  ```
  That is an HTTP 400, not a degraded answer. The chat path now asks the server itself (`/props`) rather than predicting. `n_ctx` is also divided by `total_slots` — llama-server splits context across slots, so `-np 2` on an 8192 ctx is two 4096 contexts, and budgeting against 8192 is rejected at ~4096.

- **Summarization prompt is bounded as a whole** (`context_window::bound_turns`). It previously clipped each turn to 2000 characters and nothing else, so a trim that dropped 80 turns still assembled 80 x 2000 chars — a 21,485-token request against an 8192 context. The user saw nothing: summarization runs detached, so they got an answer and silently lost the summary.

### Changed
- **Fallback token estimate is conservative** (`context_window::conservative_token_estimate`). Whitespace counting undercounted by ~15% against the real tokenizer (420 words vs 481 tokens, measured on the running server's `/tokenize`). Undercounting is the dangerous direction — it admits prompts the server then rejects. Now words plus 15% of characters.

  Worth being explicit about: the native path uses the real tokenizer, so this only affects non-native backends and tokenizer failures. The measured undercount was on real English prose, not a synthetic edge case.

### Fixed
- **A long conversation no longer fails outright.** Verified live: an 80-turn, ~60,000-token history previously returned HTTP 400 on every attempt and now answers with 6,743 prompt tokens at 195 tok/s prefill, 8.6 tok/s decode, and summarization succeeding.

  This took four attempts to fix because each attempt fixed a real defect and none of them was the one causing the 400. In order: the budget ignored the completion reserve; the budget exceeded the real context; the derived context size was absent whenever Ghostlink reused a server it did not launch; and the failing request was not the chat request at all but a detached summarization with no total bound.

  That last one is the actual lesson, and it is why the harness came first. Reading the chat path carefully, four times over, would not have found it — the log line `summarization failed (llama_server request failed with status 400 ... request (21485 tokens))` named a different function than the one under inspection. Grep the log, not the code you assume is running.

  26 new tests. The load-bearing ones assert that a per-item cap is not a total cap (80 x 2000 chars must not assemble into 21,485 tokens), that the newest turn survives an overflowing budget, and that the fallback estimate never lands below the real tokenizer's answer.


### Added
- **Inference measurement harness** (`scripts/inference_bench.py`, `crates/ghost-link/src/native_engine.rs`):
  reports **prefill and decode separately**, because a single tok/s figure over a chat request averages two unrelated costs. Prefill is compute-bound and batches; decode is bandwidth-bound and does not. Averaged together they move for reasons that have nothing to do with each other, which makes them useless for deciding whether a change helped.

  The server was already returning llama.cpp's `prompt_n`, `prompt_ms` and `prompt_per_second` in the same `timings` object as the decode fields — and never read them. Only `predicted_*` was parsed, so prefill throughput, the number that decides whether a long history is affordable, was unmeasurable and every local-vs-RPC comparison had to fall back on end-to-end latency.

  `NativeGeneration` and the chat response now carry `prompt_tokens`, `prompt_ms`, `prompt_tokens_per_sec` and `decode_tokens_per_sec`; the tool-loop path carries the same fields so both paths report the same split. `null` means llama.cpp did not report it — deliberately not `0`, which would read as "measured, and zero".

  The harness refuses to guess. A missing measurement prints as `-`; `--compare` reports `INCONCLUSIVE` rather than picking a winner; a degraded backend returning error text instead of a generation is recorded as a failure rather than counted from its word count. It also flags the case it exists to catch: when a distributed configuration decodes measurably slower than a local one on a model that fits in a single device.

  Measured on this machine (Llama-3.2-3B, ~5,300-token prompt, 3 runs):

  ```text
  prompt 245 tok/s   decode 13.3 tok/s   ttft 28,790 ms
  ```

  A 16x gap between the two halves, invisible before. The TTFT is consistent with the prefill figure (5324 / 245 ≈ 21.7s plus queueing), which is the cross-check that makes both believable.

  6 new tests cover the timings parser, including that an absent field stays absent rather than becoming `Some(0.0)` — "not measured" and "measured as zero" must not look alike in a report.

### Fixed
- **TTFT is no longer recorded only on the streaming path.** Both `record_ttft` call sites were inside the SSE finalizers, so a buffered request — what an automated benchmark wants, since it can read the `timings` object — recorded no TTFT at all. The buffered path now cannot produce one (it never observes its own first token), so instead of a proxy the harness measures TTFT on the streaming path and the response exposes the rolling `ttft_p50_ms`/`ttft_p95_ms` as context.

  Found by running the harness, not by reading the code: the first run reported `-` for TTFT on a path that had always claimed to measure it.


### Added

- **The chat shows why it answered the way it did** (`ghostlink_gui_modern/src/components/ChatTab.tsx`, `store.ts`):
  the server was already reporting four explanatory fields and the GUI read **none** of them — `recalled_memories`, `recalled_documents`, `action_claim_corrected` and `tools_run` were all sent on every response and all discarded. Every fact needed to answer "why did it say that" was on the wire and thrown away.
  Three new badges on assistant replies, alongside the existing dropped-turns and summarized-history indicators:
  | badge | meaning |
  |---|---|
  | **N memories recalled** | stored memories were injected as context for this turn |
  | **N documents found** | indexed documents `rag.search` contributed |
  | **corrected: claimed an action that didn't run** | the model asserted something no tool backed, and the server appended a correction |

  The recall badges show **counts only**. The server never sends the recalled text, so the client cannot display it, store it, or leak it into browser storage — the badge confirms recall happened without duplicating the user's own memories into a second place.
  The correction badge is styled distinctly (rose, not a metadata colour) because it means something different in kind: not "context was trimmed" but "this reply was wrong and the server caught it". Reading a transcript, that is the one you want to notice.
  6 new tests, including singular/plural forms, the zero case rendering nothing, and that none of the three ever appears on a user message.

- **Session titles are generated, not truncated** (`crates/ghost-link/src/title.rs`):
  the GUI named every thread from `firstUser.content.slice(0, 32)`, so a thread opened with *"can you look at why the auth middleware is failing on the staging host"* was titled **"can you look at why the auth mid"** — cut mid-word, lowercase, and identical for every thread starting with the same tokens. A thread opened with a pasted stack trace got a title made of code.
  The server now generates a 3-6 word title on a session's first turn, writing it to `SessionRecord::name` and returning it as `session_title`. The GUI adopts it via `applyServerTitle`, which **never overwrites a title the user set by hand** — a late-arriving generation cannot clobber a manual name.
  Generation is detached and bounded: 32 max tokens, low temperature, and a 5s ceiling on the wait, because a title is cosmetic and a chat turn must not block on it. On failure it falls back to a local word-bounded derivation, which still beats a raw character slice. A 9B/3B model asked for a title also tends to answer with prose, so `clean_title` strips labels, markdown, quotes and trailing sentences, and rejects refusals outright rather than rendering *"I cannot generate a title"* in a sidebar.
  The GUI keeps its own first-30-characters fallback for the streaming path, where the title has not arrived yet; the server title replaces it on the next turn.
  16 new Rust tests and 5 new GUI store tests, including that a user-set title wins over a generated one.

- **Proactive memory recall** (`crates/ghost-link/src/recall.rs`, wired in `main.rs`):
  on the first turn of a session the server now calls `memory_search` and `rag.search` itself and injects the hits as system context, instead of waiting for the model to decide it should go looking.
  Until now every reference to those tools lived in `capability.rs`, classifying them -- none of them ever called them. The model *could* search its own memory, but only if it thought to, which made the memory server a database with extra steps rather than memory.
  Design points worth stating:
  - **Bounded.** Top 5 per retriever, 600 chars per item, 2400 chars per block. Injected context is context every later turn still pays for, so an unbounded recall would be a slow leak.
  - **Best-effort.** A missing, disconnected, slow or failing retriever yields nothing and the turn proceeds — retrieval must never be able to fail a request. A 4s budget covers both subprocess round trips; a slow retriever is dropped rather than made the user wait.
  - **Read-only.** Only the two `Read`-class tools are called, so no approval is involved and nothing is written by this path. A test asserts no write tool appears in the call path.
  - **Workspace-scoped.** The scope is stamped server-side from the request's `workspace_id`, exactly as a model-issued call would be, so recall cannot read another workspace's memories.
  - **First turn only.** With no history there is nothing else to go on; on later turns the transcript already carries the material and a second retrieval would be latency for nothing.
  - **Counts, not content.** `recalled_memories` / `recalled_documents` are exposed on the response and the log carries counts only — never the recalled text, which may contain user content.
  Recalled context is merged into the existing extra-system-message slot used by the session summary, so all backends pick it up unchanged rather than threading a new parameter through five call sites and three streaming variants.
  18 new tests cover both MCP envelope shapes, bare arrays, wrapped objects, unparseable payloads, the token cap, per-item truncation, top-k bounding, and that a blank summary no longer injects an empty system message (a real bug the merge tests caught).

- **One-time bootstrap code so the GUI can authenticate without the raw API key** (`crates/ghost-link/src/bootstrap.rs`, `crates/ghost-link/src/main.rs`):
  the GUI holds credentials in memory only (`api.ts` keeps `apiKey` out of `localStorage` deliberately), so a freshly loaded page has none and every request 401s until an operator pastes the Admin key into SecurityTab. The symptom was a bare `Failed to load agent task execution (401)`.
  The obvious fix -- an endpoint returning `api_key.txt` -- is the wrong shape: it makes a permanent, full-privilege credential retrievable over HTTP, and ghost-link's TLS is self-signed, so a client that cannot pin the cert cannot verify the channel either.
  Instead the server prints a single-use bootstrap code at startup, and `POST /api/security/bootstrap` trades it for a **short-lived JWT** through the existing `auth::issue_jwt` path. `auth::authenticate` already accepts a JWT as a bearer token, honours it only while its subject key still exists, and reads role fresh from the live record.
  The code is single-use (consumed on redemption, whatever the outcome), expires after `GHOSTLINK_BOOTSTRAP_TTL_SECS` (default 300), is 128 bits from the OS CSPRNG via `rand::rngs::OsRng`, and is **refused from anything but loopback**. `X-Forwarded-For` is honoured only when the immediate peer is itself loopback, so a remote caller cannot spoof loopback to get past that check.
  `/api/security/bootstrap` is unauthenticated by necessity, not oversight -- the caller has no credential yet, which is why it is calling. It is constrained by the properties above and returns a JWT, never key material. Audited on both success and failure like any other auth event.
  9 tests cover single-use, expiry, loopback refusal, `X-Forwarded-For` spoofing from a remote peer, IPv6 loopback, and rejection of empty/garbage/oversized input.

- **The chat now shows when context was dropped or condensed** (`ghostlink_gui_modern/src/components/ChatTab.tsx`, `ghostlink_gui_modern/src/store.ts`):
  two indicators above an assistant reply, kept deliberately distinct:
  - **earlier turns dropped** (amber) — the server trimmed older turns from *this* reply's context to fit the token limit.
  - **answering from summary** (indigo) — the model answered from a running summary of turns trimmed in *earlier* requests.
  Merging them would hide the case where the gap is still growing: "we dropped something" and "we are still holding a condensed memory" are different facts, and a user debugging a forgotten detail needs to tell them apart.
  `summarized_history` was already in the API response but nothing consumed it, and `truncatedBefore` was being set on the message but never rendered -- so both signals were invisible. Both now surface, with `title` and `aria-label` text explaining what actually happened.
  5 new tests cover each indicator, both together, neither, and that a user message never shows them.

- **Local schedule driver** (`crates/ghost-link/src/scheduler.rs`, `crates/ghost-link/src/main.rs`, `.gitignore`):
  schedules are JSON rows plus a tokio task that sleeps until the next firing. Each run is an ordinary agent turn through the native path, so the capability gate, approval queue, and workspace scoping apply exactly as they do to interactive chat -- **a schedule is not a privileged route**. A schedule that produces a write or exec call lands in the approval queue rather than executing; the run is recorded as `needs_approval`, and `last_run_was_read_only` is what distinguishes a schedule safe to repeat unattended from one that isn't.
  Routes: `GET/POST /api/inference/schedules`, plus `get`, `toggle`, `delete`, and `run-now`. Every operation is scoped to the owning `workspace_id`, and another workspace's schedule id is indistinguishable from a nonexistent one, so the endpoints can't be used to enumerate ids.
  **Cron is a documented 5-field subset that rejects what it doesn't implement.** With no new dependency, the temptation is a partial cron parser whose failure mode is a job that silently never fires. `CronSpec::parse` therefore refuses out-of-range values, inverted ranges, zero steps, wrong field counts, and empty list entries at *insert* time, so the user finds out immediately. `next_after` searches a bounded four-year horizon so an impossible spec (`0 0 30 2 *`) returns `None` instead of spinning forever.
  A never-run schedule is due immediately, whatever its trigger says. The first implementation anchored on `created_at`, which meant a schedule created for 09:00 sat idle until 09:00 the next day -- not what "create this schedule" means to anyone. A test pins both the one-shot and the cron case.
  Missed runs are **not backfilled**: a schedule missed while the machine was asleep fires once on restart, because `last_run` moves to now and the missed occurrences are dropped. A laptop waking up should not execute thirty catch-up copies of a prompt that writes files.
  `run-now` marks a schedule due rather than executing it inline, so a manual run and a timer firing go through one identical execution path -- meaning a read-only schedule's auto-run permission can't diverge from what run-now would do.
  List responses carry no prompt bodies (`ScheduleSummary` has no `prompt` field), mirroring `ActionSummary`: a prompt may quote a file or a pasted secret, and it should only be served to someone who asked for that specific schedule.
  Persistence follows the crate's existing JSON-store convention (`schedules.json`, `GHOSTLINK_SCHEDULES_PATH`) rather than adding SQLite to the server binary for tens of rows. A corrupt file degrades to empty and is left on disk for inspection. `schedules.json` added to `.gitignore` alongside `approvals.json` and `ghostlink_memory.db`.
  Known gap, stated rather than hidden: a scheduled turn is offered **no tools** in this pass. Wiring the GUI's tool-slot selection into a headless run needs a persisted per-schedule tool list, which the brief leaves open. This narrows what a schedule can do and never widens it -- the capability gate still applies to anything the turn reaches.

- **Bounded agent loop with three independent budgets, and skills that cannot widen the boundary** (`crates/ghost-link/src/agent.rs`, `crates/ghost-link/src/skills.rs`, `crates/ghost-link/src/main.rs`, `crates/ghost-link/src/capability.rs`):
  the tool loops threaded a bare `iterations_left: usize` — one counter, one real inference call per tick, no way to bound a turn by cost. An iteration cap alone does not stop a single iteration from being expensive, so a stuck loop could still burn an unbounded number of tokens. `agent::Budget` replaces it with independent `max_steps` / `max_tool_calls` / `max_tokens` limits (`GHOSTLINK_AGENT_MAX_STEPS`, `GHOSTLINK_AGENT_MAX_TOOL_CALLS`, `GHOSTLINK_AGENT_MAX_TOKENS`), each clamped to at least one so a zero can't produce a turn that can never take a step.
  `max_steps` deliberately defaults to the existing `MAX_TOOL_ITERATIONS` of 6 rather than a new number — that value was already tuned (raised from 3 on 2026-08-10) and changing it here would silently alter behavior for every existing deployment. A test pins the two together so they can't drift.
  Exhaustion is now reported honestly. The old message was "stopped after N tool round-trips" regardless of cause; `exhaustion_message` names which limit was hit, how much was actually consumed, and which env var to raise. A refused tool-call batch is tracked explicitly rather than inferred from spend, because a refused batch leaves the counter *below* the cap and the caller would otherwise get no stop reason at all.
  Tool-call charging is all-or-nothing per batch: dispatching "as many as fit" would silently drop the tail of a batch the model asked for, and a model seeing a partial result set could reasonably conclude the rest returned nothing.
  `GHOSTLINK_AGENT_TURN_TIMEOUT_SECS` (default 0 = disabled) adds a wall-clock backstop for the whole turn. It is separate from the budget limits because it bounds latency rather than cost — a turn can be cheap but slow, or expensive but fast. Its message names the wall clock rather than reusing a budget-exhaustion message, since the budget may well have had room left.
  Resumed turns seed `max_steps` from their remaining iterations, so a turn paused on an approval cannot reset its ceiling by resuming.
  Skills (`skills.rs`) are loadable procedures — front-matter markdown or JSON under `.agents/skills` or `GHOSTLINK_SKILLS_DIR` — carrying the tools they may use. **A skill can only narrow the capability boundary, never widen it**, and that is structural rather than conventional: `SkillSet::allows` intersects the skill's declared tools with the server-side classification and pre-authorizes only `read`. A skill naming `write_file` or `run_command` grants nothing; those stay behind the approval gate. This matters because a skill is authored data — plausibly model- or user-written — and treating it as trusted input would hand it the ability to escalate. An empty tool list grants nothing rather than everything, so a typo'd skill file can't become a blank cheque.
  `GET /api/inference/skills` returns names, descriptions, and tool counts without procedure bodies; `?skill=<name>` returns one body, which is the deliberate exception since the caller asked for that specific procedure. The same explicit-retrieval discipline `memory_catalog` uses, so a large skill library doesn't sit in every prompt. The front-matter parser is intentionally the narrowest thing that works — no nested YAML, no includes, no interpolation — because a skill file is untrusted-adjacent input, and a malformed file is skipped rather than failing the load.

- **Approval tray: gated tool calls are queued, not dropped** (`crates/ghost-link/src/approvals.rs`, `crates/ghost-link/src/main.rs`, `crates/ghost-link/src/trace.rs`, `ghostlink_gui_modern/src/components/ApprovalTray.tsx`, `ghostlink_gui_modern/src/api.ts`, `ghostlink_gui_modern/src/store.ts`):
  a `write` or `exec` call used to return a `ToolResult` error saying it was blocked, so the model saw a failure and either retried the same gated call or told the user the action had failed. The call is now recorded as a pending action and the model is handed an approval id it can reason about, so the chat turn finishes normally and the user decides in a tray.
  `POST /api/inference/chat`-adjacent routes added: `GET /api/inference/approvals` (pending by default; `?all` or `?status=<s>` widens) and `POST /api/inference/approvals/:id/decide`. Approving executes the action immediately on the approved (or edited) arguments and returns the tool's result, so approving isn't a blind act of faith.
  The queued action's class is re-derived server-side from the capability table rather than read from the stored row, so a hand-edited `approvals.json` cannot promote a tool to a weaker class. `approve_for_session` still refuses `exec` via the existing `grant_for_session` guard.
  **Reachability is now checked before the capability gate.** A tool whose MCP server isn't connected previously produced a queued approval that could never be acted on — the model would report "awaiting your approval" indefinitely. It now returns an error instead. Caught by the pre-existing `test_mcp_tool_hashmap_lookup`, which is the reason that test is worth keeping.
  The queue is persistent (`approvals.json`, `GHOSTLINK_APPROVALS_PATH`), because a queued write is a promise to the user and losing it on restart would silently drop the request. A corrupt file degrades to an empty tray and is **left in place** for inspection rather than overwritten. Resolved rows are pruned at startup after `GHOSTLINK_APPROVAL_MAX_AGE_DAYS` (default 30); pending rows are never pruned at any age.
  `build_preview` extracts a short, bounded description of the intended effect — a target path, a SQL statement, a command — and reads only specific named argument fields. There is deliberately **no** "render the arguments as JSON" fallback, because the argument bag is exactly where a credential appears; an unrecognized shape yields `invoke <server>/<tool>`. Two tests pin this.
  `to_summary` (what the list endpoint returns) has no `args` field on the type, so tool arguments cannot reach the tray even by accident. `resolve` is scoped to the owning workspace and returns the same `None` for "not yours" as for "doesn't exist", so a caller can't probe another workspace's ids. A duplicate click is idempotent — it can't flip a denial into an approval.
  `model_observation()` is worded so the model reports the pending state rather than claiming the action happened; the common failure mode is a model reading "queued" and telling the user the file was written.
  GUI: `ApprovalTray` on the chat, polling the queue every 5s and rendering nothing when empty. It hides "Approve for session" for `exec`-class tools (the backend refuses it anyway, so the button would be a dead end) and shows the executed result inline after approving. Entry point remains the control-plane gateway on `:8000`.
  Two runtime-state leaks found by the existing test suite while wiring this up, both fixed: `test_mcp_tool_hashmap_lookup` reached the capability gate and wrote real rows into `approvals.json` beside the binary, so tests now redirect `GHOSTLINK_APPROVALS_PATH` to a temp file before touching the queue (which is memoized in a `OnceLock`, hence the ordering requirement); and `approvals.json` / `ghostlink_memory.db` were missing from `.gitignore`, so the queue — which holds the arguments of pending actions — could have been committed.

- **Turn traces with content exclusion enforced by construction** (`crates/ghost-link/src/trace.rs`, `crates/ghost-link/src/main.rs`):
  assistant turns now emit structured trace events (`chat`, `tool_approval`) carrying tool name, capability class, decision, token counts, and latency, appended to both a bounded in-memory tail and the durable audit trail.
  The redaction rule is enforced by the *type*, not by convention: `TraceEvent` has no payload/body/arguments field and no free-form constructor, so prompts, completions, file bodies, SQL, and command lines have nowhere to go. Adding one later is a deliberate, reviewable diff rather than an accident. Labels are truncated at 64 chars on a char boundary with a visible `…`, so a caller that mistakenly passes content instead of a name is loud rather than quietly durable.
  `GET /api/inference/traces` (Inference role) serves the tail with an optional `?kind=` filter, derived from the same capped feed the Security tab already reads — a filter, not a new exposure. Bulk history still goes through the owner-gated audit-log export.
  The chat trace is recorded *after* generation so it carries real token counts and end-to-end latency. Input token count is reported as absent rather than zero: the engines don't surface it, and a fabricated number in an observability feed is worse than a missing one.
  Tool dispatch and gate refusals emit on the `assistant_trace` structured-log target from inside `invoke_mcp_tool`, which has no `BackendState` to append to. Threading one through the six engine-loop call sites was judged a poor trade for this pass; the durable record of a gated call is still written by the approval handler that resolves it.
  `PendingApproval`/`Edited` trace statuses and the `invoke_agent`/`execute_tool` constructors are deliberately absent rather than present-and-unused — they belong to the approval tray (phase 3) and bounded loop (phase 4), which will add them with real call sites.

- **Durable per-workspace memory for the local assistant** (`crates/mcp-memory/`, `crates/ghost-link/src/capability.rs`, `crates/ghost-link/src/main.rs`, `mcp_servers.example.toml`, `Cargo.toml`):
  A new internal stdio MCP server (`publish = false`, shaped like `mcp-rag`/`mcp-vision`) providing `memory_catalog`, `memory_search`, `memory_remember`, and `memory_forget` over a SQLite store. Memory kinds are `preference`, `project_fact`, `decision`, `person`, `open_loop`, `summary`; each row carries `workspace_id`, `source` (`user`/`compaction`/`tool`), timestamps, and `pinned`/`shared` flags.
  **Scoping is enforced server-side, not by the model.** Every statement filters on `workspace_id`, and `capability::stamp_workspace_scope` *overwrites* that argument at dispatch with the chat's own binding. A model that emits `{"workspace_id": "ws_other", ...}` — which a prompt-injected turn can trivially do — gets its value discarded rather than honored, because a model-chosen scope would make per-workspace grants purely decorative. The supplied value is discarded rather than rejected so an injection attempt degrades to a correct result instead of a confusing tool error.
  **Explicit memory is enforced by the type surface, not by convention.** `memory_catalog` returns a `CatalogEntry`, which has no body field at all, so the "titles and kinds only" rule cannot be violated by a later edit; `memory_search` returns bodies solely for rows that matched. A non-matching query returns an empty result rather than the whole store.
  Ranking is keyword-weighted (title hit 3.0, body hit 1.0, pinned x1.5) with ties broken by recency then id for stable ordering. Deliberately not an embedding model: a single local user's memories are a few hundred rows, and a keyword score is explainable, needs no model pull, and cannot leak the query to a network service.
  `rusqlite` with the `bundled` feature is the workspace's first SQLite dependency — chosen over the JSON-file pattern `mcp-rag` uses for its index because ranked search and concurrent access from the chat path, the approval queue, and the scheduler would otherwise contend on one global file lock. `bundled` compiles SQLite from source, so there is no system package to install and CI stays green on all three platforms.
  An unrecognized `kind`/`source` in an existing row degrades to a safe default instead of dropping the row — losing a memory because a future version added a kind is worse than mislabeling it. `memory_forget` deletes scoped to the workspace, so an id carried over from elsewhere is a no-op rather than a cross-workspace delete. WAL is enabled so readers never block the writer.

- **Tool calls are capability-gated in Rust before they run** (`crates/ghost-link/src/capability.rs`, `crates/ghost-link/src/workspace.rs`, `crates/ghost-link/src/main.rs`, `crates/ghost-link/Cargo.toml`):
  The chat tool loop could dispatch any advertised MCP tool with no server-side check on what it was about to do. The only existing control, `McpServerConfig::requires_confirmation`, is per-*server*, so it cannot tell `read_text_file` from `write_file` on the same filesystem server, and it cannot express "this one tool is fine to auto-apply". A system prompt asking the model to be careful was the only other thing standing between a chat turn and arbitrary command execution, which is not a control — the model is not the trust boundary, the server is.
  `capability.rs` classifies every call as `read` / `write` / `exec` from a `(server, tool)` table and **`classify` fails closed**: an unlisted tool on an unlisted server, and an unlisted tool on a *known* server, both default to `exec`. That default is the point — the cost of a wrong `read` is arbitrary command execution, the cost of a spurious `exec` is one extra approval prompt. `decide()` is the enforcement point, called in `invoke_mcp_tool` before `call_tool`, so all three engine loops (native `TOOL_CALL:`, Ollama, vLLM) are covered by one gate rather than three.
  `approved_for_session` grants are keyed by `(workspace, server, tool)` and **never cover `exec`** — "approve for the session" must not become a standing shell (`CapabilityClass::session_grant_allowed`). `grant_for_session` refuses an `exec` class outright rather than trusting the caller to check, and `POST /api/inference/chat/tool-confirm` classifies server-side instead of accepting a class from the request.
  The vetted auto-apply path (`is_vetted_auto_apply`) admits a `write` only when its target canonicalizes *inside* the workspace root; `exec` and `read` are never auto-applied, and a missing resolved path is refused rather than assumed safe.
  Adds `GET /api/inference/capabilities`, reporting each connected tool's class, whether it needs approval, and whether a session grant is active, so the boundary is auditable without inferring it from a blocked tool result mid-conversation.
  Adds `workspace_id` to `GuiChatRequest` and `workspace.rs`. A client-supplied id selects *which* workspace's memories, RAG index, and grants apply — it never selects a filesystem root, which would be free path traversal; the root stays server-configured. Absent means the configured root, so no existing client's behavior changes. Ids are sanitized and bounded (64 chars, no separators) before use as a filename or key.
  `resolve_within` is now the single traversal check: `main.rs`'s `resolve_workspace_path` delegates to it rather than keeping a second copy of the canonicalize-and-prefix logic, so the Editor tab's file routes and the tool caller's scoping cannot drift apart.
  Adds a `chat` audit event recording the workspace binding for each turn. Workspace id and session only — never the prompt or completion, which stay off the trail by policy.

- **Release workflows reported success while publishing nothing** (`.github/workflows/publish-crates.yml`, `.github/workflows/publish-sdks.yml`, `.github/workflows/release-artifacts.yml`):
  All three carry `workflow_dispatch` alongside a `push: tags: v*` trigger, and every release-critical step is guarded by `if: startsWith(github.ref, 'refs/tags/')`. A manual dispatch defaults to a **branch** (`refs/heads/main`), so those guards evaluate false and the steps silently **skip** — while the job still concludes `success`. The result is a green run that publishes nothing, which is indistinguishable from a real release until someone checks the registry.
  This is not hypothetical: `Publish SDKs` ran this way on 2026-09-26, 2026-09-27, and again during the v2.3.0 release, reporting success each time with npm and PyPI left empty. `Release Artifacts` behaved identically and produced a GitHub Release with **zero assets**. `Publish Crates` was unaffected only by luck — its publish steps happen to carry no tag guard, so they ran anyway, but its tag-format and version-match verification steps skipped, meaning it could publish an unverified version.
  Each of the three workflows now begins with a **`Require a tag ref`** step that fails with an explicit `::error::` (including how to dispatch correctly) when `GITHUB_REF` is not `refs/tags/*`. A failing step was chosen over a job-level `if:` deliberately: a skipped job still reports success, which is the exact failure being fixed. Verified locally for `refs/heads/main` (fails), `refs/heads/release/v2.3.0` (fails), and `refs/tags/v2.3.0` (passes).
---

- **A conversation can start a subagent, and a schedule can use tools**:
  - `spawn_agent` on `POST /api/inference/chat` starts a detached implementer run and returns its project, task and run ids. The turn returns immediately with `inference_backend: "agent"` and no generation, since the real work happens in the background; progress is polled via `/api/tasks/:id/events` and the proposed diff via `/api/tasks/:id/review`.
    Deliberately a request field rather than a model-callable tool. A subagent writes proposed files and runs builds, so letting the model choose to spawn one would mean it deciding to spend minutes of compute — and every spawn would then need approval, which defeats the point. The user asks, the server starts it, and the review gate still applies before anything reaches the working tree. An empty goal is refused rather than run.
    `task_api::start_implementer` was extracted from the existing HTTP handler and both now call it, so a subagent started from a conversation and one started from the REST API cannot drift apart.
  - Scheduled turns now get the same read-only tool default as an interactive chat turn. They previously got none, so a schedule could talk but never check anything — "check the build each morning" was unrunnable, because the tools that would read the output were not offered. Read-only by construction rather than policy: the default selects slots with a `Read` tool, and `capability::decide` still runs per call, so a write queues for approval exactly as it would in a chat.
    Verified live: the log records `scheduled turn using the read-only tool default`, and the schedule fired in 5.0s.

- **Conversation summaries now survive a restart** (`crates/ghost-link/src/main.rs`, `.gitignore`):
  `BackendState::session_summaries` was in-memory only, so a long session that had been trimmed lost its condensed memory on restart and the next turn re-read from scratch — the summarization work was effectively undone by a crash or a redeploy.
  Summaries are now written to `session_summaries.json` (`GHOSTLINK_SESSION_SUMMARIES_PATH`) after each successful trim and loaded at startup. The on-disk shape is a separate `PersistedSessionSummary`, because the in-memory struct carries an `Instant` for LRU eviction that cannot be serialized and means nothing across a restart; wall-clock time is tracked separately for durable ordering.
  Failure paths degrade rather than break: an unreadable or corrupt store loads as "no summaries", which is exactly the pre-existing cold-start behavior, instead of failing boot. Text over `SUMMARY_MAX_CHARS` is trimmed on load too, so a store written by a build with a larger cap cannot reintroduce an oversized summary.
  Verified: a live restart returns `summarized_history: true` for `sess_local_001` purely from the restored file, and 6 new tests cover the round-trip, accumulation across restarts, corrupt/missing stores, oversized rows, and the cap.
  One bug caught by those tests and fixed here: the loader sorted ascending and truncated, which kept the **oldest** rows and silently discarded the most recent work — the opposite of what the cap is for. It now drops the excess oldest and keeps the newest.

- **Bootstrap-code tests no longer fail intermittently in parallel** (`crates/ghost-link/src/bootstrap.rs`):
  the code store is process-global — one `OnceLock<Mutex<HashMap<..>>>`, which is correct for the server since exactly one code is live per process. Under `cargo test`, though, tests share that store on parallel threads and `issue_code()` *clears* it before inserting, so one test issuing a code could wipe another's mid-assertion.
  Caught on CI, not locally: `redemption_is_single_use` and `refused_from_non_loopback` failed in `Production Gate` while passing on this machine, because the two machines have different core counts and therefore different thread scheduling. The tests now hold a serialising guard. Verified with 25 consecutive runs at `--test-threads 16`.

- **Model loads now warn before a GPU-offload OOM instead of dying inside Vulkan** (`crates/ghost-link/src/native_engine.rs`):
  full offload allocates a *second* copy of the weights in device memory alongside the host copy, so a model that loads comfortably at `-ngl 0` can fail at `-ngl -1` purely because something else on the machine grew.
  Measured on this machine with Qwen3.8-27B-UD-IQ3_S (12.04GB), 16 threads, 3 runs per configuration:
  | ngl | decode | TTFT | resident | loaded |
  |---|---|---|---|---|
  | 0 | 2.28 / 2.37 tok/s | 1.46s | 12.24 GB | 2/2 |
  | 12 | 2.21 tok/s | 1.64s | 12.27 GB | 1/1 |
  | 24 | 2.28 tok/s | 1.70s | 12.29 GB | 1/1 |
  | **-1** | **3.88 / 3.91 tok/s** | 2.25s | 12.64 GB | **2/2** |

  Two findings from that, both contrary to what the repo previously documented:
  - **Partial offload is not a middle ground here.** `ngl` 12 and 24 land within noise of CPU-only (2.21-2.28 vs 2.28-2.37). Only full offload helps, at **1.65x**.
  - **The OOM was a memory precondition, not a bad setting.** The same `-ngl -1` load succeeded twice at ~22GB free and failed three times at ~11GB free with `vk::Device::allocateMemory: ErrorOutOfDeviceMemory`. So `-ngl -1` is kept as the default and a precondition check warns when free memory is short, naming the requirement, what is available, and `GHOSTLINK_LLAMA_NGL=0` as the escape hatch.
  The check warns rather than refuses: the estimate is conservative and refusing to load a model that would in fact fit would be worse. CPU-only loads are exempt, since there is no duplicate allocation.
  Also corrects a memory figure in `launch-native.ps1`: CPU-only `ngl 0` shows **12.24 GB resident**, not the ~0.5 GB previously claimed, because resident memory includes the mmap'd model file at every `ngl`. Offload saves the duplicate device copy, not the whole model.

- **Review pane no longer drops the verification verdict, and its note field works** (`ghostlink_gui_modern/src/components/ReviewPane.tsx`, `ghostlink_gui_modern/src/api.ts`):
  the backend's `ReviewPacket` carries `verification: Vec<VerificationResult>` and `checks`, but `api.ts` never declared either and `ReviewPane` rendered neither. A reviewer saw only `risks: ["Verification produced no results — change is UNVERIFIED"]` next to an empty diff pane -- so the one piece of information that determines whether a change was tested was invisible, and an untested change looked the same as a verified one.
  The pane now renders a tri-state verdict banner in the header, matching the backend's `verification_passed() -> Option<bool>`, which deliberately returns `None` when nothing ran: **passed**, **failed**, or **NOT run — UNVERIFIED**. Each verification command shows pass / exit code / timeout plus the captured excerpt on failure, since the backend truncates output specifically so a reviewer can see *why* something failed. `checks` is now rendered too.
  Also fixed a typo that made the "Request Changes" note field unreachable: the setter was declared `setShowNoteNoteInput` (doubled `Note`) while the button called `setShowNoteInput`, so clicking it threw instead of opening the input.
  `ReviewPacket` was also not exported from `api.ts` despite being imported by `ChatTab`, `ReviewPane` and `TaskView`; it now is.
- Empty-diff state explains itself ("this run produced no diffs to review... check the verification panel") instead of showing a bare blank pane.

- **`launch-native.ps1` pins `GHOSTLINK_TLS_CERT_PATH`, without which the control-plane 503s every proxied request** (`launch-native.ps1`):
  the control-plane starts with its working directory set to `control-plane/`, but ghost-link writes `tls_cert.pem` to the repo root. So the proxy's pinned cert pool looked in the wrong place, fell back to system roots, failed verification against the self-signed cert, and returned `Backend unreachable` for every proxied request while `/health` kept reporting ok.
  This is the same cwd hazard the launcher already documents for `api_key.txt`, and it now gets the same explicit path. Verified both ways from `control-plane/` as cwd: without the variable `/api/metrics` returns 503, with it 200.
  Worth noting how it slipped through: the proxy's own tests and the live check both passed, because both ran the control-plane from the repo root. Only reading the launcher's actual `-WorkingDirectory` argument exposed it.

- **Windows: the three `uvx`-backed MCP servers could not start at all** (`mcp_servers.example.toml`, `mcp_servers.toml`):
  `mcp.os.win32.utilities` imports `pywintypes`, which `uvx`'s ephemeral environment does not include, so `fetch`, `git`, and `sqlite` died at import with `ModuleNotFoundError: No module named 'pywintypes'`. Adding `--with pywin32` to each fixes it; verified by MCP handshake, not by assuming.
  `mcp-server-sqlite` additionally needs an explicit `mcp` pin. Unpinned it resolves against a newer `mcp` whose decorator API dropped `Server.list_resources` and it dies at import; `mcp==1.9.4` works, as do 1.10.1/1.12.0/1.13.0. The existing `mcp==1.9.4` pins on `fetch` and `git` were already correct for a different reason and are kept.

- **Four more tools were defaulting to `Exec`, found by connecting the servers** (`crates/ghost-link/src/capability.rs`):
  auditing upstream documentation was not enough. Connecting all seven runnable MCP servers exposed **39 tools**, and four were unclassified: `git.git_create_branch`, `git.git_branch`, `sqlite.append_insight`, and `filesystem.read_file` (an alias of `read_text_file` the docs don't mention). Listing branches or reading a file demanded an approval.
  Branch creation and `append_insight` are classified `Write`, not `Read`: they move refs and append to a durable file respectively. `REAL_TOOLS` is now the live-observed tool set rather than a transcription, and the module comment says to re-derive it by connecting servers rather than by reading docs -- because the docs and the served tool list disagree.
  Verified live after the fix: 7/7 servers connected, 39/39 tools classified, **24 read / 15 write / 0 spurious exec**.

- **Six ordinary read tools were falling through to the `Exec` default** (`crates/ghost-link/src/capability.rs`):
  auditing the classification table against the *actual* tool set of every server in `mcp_servers.example.toml` found `git_diff_unstaged`, `git_diff_staged`, `sqlite.list_tables`, `sqlite.describe_table`, `fetch.fetch`, and both `brave-search` tools were unclassified and so defaulted to `Exec`.
  Failing closed is safe -- no read was wrongly permitted -- but it is not correct: each of these demanded a human approval just to look at a diff, list tables, or fetch a URL. That is exactly the routine false gate that trains people to click Approve without reading, which is the habit the approval tray depends on.
  `fetch.fetch` was missed for a structural reason worth recording: the table keys on `(server, tool)`, and the old loop filed `brave_web_search` under both `fetch` and `brave-search` while never filing the `fetch` tool that actually exists on the `fetch` server. A `REAL_TOOLS` table in the test module now pins every real tool name to its expected class, so adding a tool upstream without classifying it fails the build instead of silently demanding an approval.
  Also adds the `memory` server to the active `mcp_servers.toml`, which had only been added to `mcp_servers.example.toml` -- so the phase-1 memory tools were unreachable in a real deployment despite being implemented and classified.
### Changed

- **A chat request that names no tools now gets the server's read-only default** (`crates/ghost-link/src/toolselect.rs`):
  `req.mcp.tools` was the only source of enabled tool slots and defaulted to empty, so any client that omitted it received an assistant with **no tools at all** — silently, and with nothing in the response to indicate it. Scheduled turns and non-GUI clients hit this permanently; the GUI happens to send its checkbox list, which is why it went unnoticed.
  Now:
  | request | meaning |
  |---|---|
  | `mcp.tools` absent | server default: every connected slot with at least one `Read` tool |
  | `mcp.tools: []` | explicitly none — honored as sent |
  | `mcp.tools: [...]` | exactly those slots |

  The absent/empty distinction is deliberate. A client sending `[]` has decided it wants no tools, and overriding that would be the same class of bug in the opposite direction. A malformed `tools` value is treated as unspecified rather than as "none", so a client bug cannot silently disarm the assistant.
  **Visibility, not permission.** The default widens what the model can *see*, never what it can *do*: `capability::decide` still runs per call, so `memory_remember` routes through vetted auto-apply and `memory_forget` and the Docker gateway tools still require approval.
  A slot qualifies when it offers *any* read. The first implementation excluded slots containing a `Write` entirely, on the reasoning that partial availability is confusing — but that excluded `memory` and `rag`, whose servers each mix reads with writes, leaving the default offering exactly one slot while four servers were connected. "Use the default" then quietly meant "you get almost nothing", which is the silent-disablement this change exists to remove. An `Exec`-only slot still never qualifies.
  Measured live on this machine: default went from 1 slot / 1 tool to **4 slots / 21 tools**, and a request with no `mcp.tools` had the model call `rag.search` and `memory_search` on its own — impossible before.
  11 new tests, including that `memory_forget` and `docker-mcp-gateway/mcp-exec` still require approval under the default, and that the default derives from the live `classify` rather than a hand-maintained list.

- **`memory_remember` no longer requires approval** (`crates/ghost-link/src/capability.rs`, `crates/mcp-memory/src/main.rs`):
  it is now a vetted auto-apply write, gated by `capability::is_vetted_memory_write`.
  It was `CapabilityClass::Write` with no filesystem target, so the path-based auto-apply rule could never match it and every memory write queued for a human decision. Verified live: asked to remember something, the DB stayed at 0 rows. Nothing was ever remembered unless the user answered a prompt per fact, which left both the store and the recall that reads it close to dead weight.
  `memory_forget` is deliberately **not** included. Forgetting is irreversible — there is no un-forget — so a misfired delete should still cost a human decision. The scope, not the prompt, is what makes this safe: `is_scope_stamped` forces the workspace id at dispatch, so an auto-applied write lands in the chat's own workspace and cannot be redirected by the model.
  The tool description also said *"Requires user approval"*, which was actively harmful — the model read that and declined to call the tool at all, even when asked directly. Now describes the tool and states it takes effect immediately.
  7 new tests: remember auto-applies, forget never does, another server's same-named tool is not covered, sibling read tools are not covered, `rag.index_document` stays gated, an Exec tool on the memory server cannot pick up the path, and the exemption is workspace-scoped rather than path-scoped.

- **`mcp-rag` can now embed via llama-server, removing the Ollama dependency** (`crates/mcp-rag/src/main.rs`, `mcp_servers.example.toml`, `mcp_servers.toml`, `.gitignore`):
  `rag_embed` now selects a backend via `GHOSTLINK_EMBED_BACKEND`. `llama` targets llama-server's OpenAI-compatible `POST /v1/embeddings` (`data[0].embedding`); `ollama` keeps the original `POST /api/embeddings` (`embedding`) untouched; `auto` tries llama then falls back. **An existing Ollama setup keeps working with no config change** -- `auto` is the default.
  Verified live with Ollama pointed at a dead port: indexed and searched in 0.0s, 768-dim vectors from `nomic-embed-text-v1.5.Q4_K_M` (84MB), with real semantic separation (cosine 0.918 for a paraphrase vs 0.407 across unrelated topics).
  Requires a **second** llama-server on its own port -- an embedding model and a chat model cannot share one instance:
  `llama-server -m models/nomic-embed-text-v1.5.Q4_K_M.gguf --embedding --pooling mean --host 127.0.0.1 --port 8081`
  Vectors are only comparable within a model, so switching backend requires re-indexing; the config comment says so.

- **Bumped the last `actions/setup-node@v4` pin to `@v7`** (`.github/workflows/ci.yml`):
  GitHub removed the Node 20 runtime from Actions runners on 23 September 2026; JavaScript actions now run on Node 24. The `v4` tag still declares `runs.using: node20` (`v5` onward declare `node24`), and runners have been rewriting `node20` to `node24` since 16 June -- so this was never broken, just the one inconsistent pin in the repo. Every other workflow already used `@v7`.
  Note this is the **action runtime**, not the build Node: `node-version: 20` selects the Node that runs `npm ci` / `vitest` / `tsc` and is unaffected by the runner change. Verified no `ACTIONS_ALLOW_USE_UNSECURE_NODE_VERSION` opt-out is referenced anywhere, since that escape hatch stopped working on 23 September.
  Also checked before bumping: `setup-node` v5+ auto-enables npm caching when `packageManager` or `devEngines.packageManager` is set, which would change caching behavior. No `package.json` in this repo sets either field, so the bump is inert beyond the runtime version.

- **Control-plane no longer 503s every proxied request when ghost-link is on HTTPS** (`control-plane/pkg/proxy/proxy.go`):
  `NewChatProxy` computed the loopback check and threw it away (`_ = ...`), so an `https` backend URL got a default `http.Client` with full certificate verification. ghost-link serves TLS whenever `settings.enable_tls` is set, which includes loopback (`use_tls = enable_tls || !is_loopback_host(host)`), and presents a self-signed cert -- so the handshake failed and `forward()` answered `Backend unreachable` (503) for everything it proxied.
  `/health` kept reporting `status: ok` the whole time, because that handler echoes the configured backend URL without ever using the proxy client. The gateway looked healthy while every real API call failed.
  Loopback `https` backends now get an `InsecureSkipVerify` transport. Scoped to loopback deliberately: a non-loopback `https` backend keeps full verification, and a plaintext `http` backend is untouched. Covered by `control-plane/pkg/proxy/proxy_tls_test.go` -- 4 tests, including a direct regression test asserting a reachable loopback TLS backend does not produce 503.

- **Streaming chat turns now record a trace event** (`crates/ghost-link/src/main.rs`):
  `handle_gui_chat`'s three SSE arms each `return Sse::new(...)` before reaching the shared `record_trace` call, so the GUI's default streaming path recorded **no** turn trace at all. Only the non-streaming fallback and the OpenAI-compat server were traced. Added the trace to the Ollama and native stream finalizers, where the TTFT/tokens-per-second metrics were already being recorded -- so the trace carries the same real token count and latency rather than a second, separately-derived number.
  The third SSE path (replaying a completed `response_text`) already flows through the shared call and needed no change.
  Found by end-to-end test, not inspection: a live streaming chat left `/api/inference/traces` empty. Verified after the fix -- `{"kind":"chat","latency_ms":1328,"output_tokens":23}`.

- **RAG workspace indexing no longer requires Ollama** (`crates/ghost-link/src/main.rs`):
  the `/api/workspace/index` pre-flight probe hard-coded an Ollama health check, so a llama-only machine was answered `Ollama isn't reachable ... status: skipped` while its actual embedding backend was running fine. The probe now mirrors `mcp-rag`'s backend selection: an explicit backend probes only that one, and `auto` requires only that *either* answers.
  This was found by the live test above, not by inspection -- the first version of the change looked correct and still returned the Ollama error.
- Prebuilt llama.cpp binaries (`tools/llama.cpp/`) and locally-fetched GGUF weights (`models/nomic-embed-text*.gguf`) are gitignored. Neither belongs in the repo.

- **Replaced `brave-search` with a local, keyless web search** (`mcp_servers.example.toml`, `crates/ghost-link/src/capability.rs`):
  `brave-search` required `BRAVE_API_KEY` and a paid account, which sat awkwardly against this project's all-local constraint. It is replaced by `duckduckgo-mcp-server` (`npx -y duckduckgo-mcp-server`), verified live: it handshakes, advertises a single tool `duckduckgo_web_search`, and returns real results with no API key.
  **Stated plainly because it matters operationally:** DuckDuckGo's free HTML endpoint rate-limits aggressively and answers `DDG detected an anomaly in the request, you are likely making requests too quickly` under load. That was observed directly during verification. It suits occasional lookups and is not a high-throughput search backend. A self-hosted SearXNG instance is the sturdier option if that becomes a problem -- it needs a container but no account.
  Two other candidates were evaluated and rejected on evidence rather than reputation: `free-search-mcp` fails at import (its `selectolax` Modest backend was deprecated at 1.0 and now raises), and `one-search-mcp` requires a `.env` file plus a Chromium install before it will start.
  Classified `Read`: it queries the public web and writes nothing local.

- **Docker MCP Toolkit gateway connected and classified from its live tool set** (`mcp_servers.toml`, `mcp_servers.example.toml`, `crates/ghost-link/src/capability.rs`):
  `docker mcp gateway run` (Docker 29.8.1) serves 8 tools, all now classified `Exec`: `mcp-exec`, `code-mode`, `mcp-add`, `mcp-remove`, `mcp-config-set`, `mcp-create-profile`, `mcp-activate-profile`, `mcp-find`. `mcp-exec` and `code-mode` run commands outright; the config-mutating ones decide what the gateway is able to run; `mcp-find` is included as Exec rather than Read because it queries the catalog of servers the gateway can activate.
  The gateway's *dynamically* activated tools (containers, images, compose) arrive at runtime with names this table has never seen, so they hit the unknown-tool default of `Exec`. That fail-closed behavior is the point -- a tool the gateway invents at runtime cannot be quietly treated as a read.
  Also worth recording: `docker-code-execution`, `docker-terminal`, and `docker-mcp-gateway` were three entries running the **identical** command, `docker mcp gateway run` -- three duplicate connections to one gateway. The two redundant entries are disabled in the active config; `docker-mcp-gateway` is the one to enable.
### Fixed

- **Agent self-verification can now actually run** (`crates/ghost-link/src/task_runtime.rs`):

- **"Verification skipped" no longer reports itself as "produced no results"** (`crates/ghost-link/src/task_runtime.rs`):
  when the implementer proposes no file changes there is nothing to verify, and the review said *"Verification produced no results — change is UNVERIFIED"* — which implies a check ran and came back empty. It never ran at all.
  Observed across a live batch: 23 reviews carried that message when the real cause was a backend inference error three steps earlier, so the risk pointed at the wrong thing entirely. The two situations are now distinguished — nothing to verify, versus a verification that ran and found nothing.
  2 new tests exercise the verification path directly rather than end to end: a real crate is checked by its own `cargo test --workspace` and passes, and a crate that does not compile is recorded as a `FAIL` risk rather than a pass. The end-to-end runs could not distinguish "verification is broken" from "the model wrote no files", which is why the unit under test is now tested as a unit.
  verification builds the proposed change in a scratch copy of the project, and that copy included everything except `.git`, `target` and `node_modules`. Against this repository the copy was **40 GB** — 38.8 GB of it `models/*.gguf`, plus a vendored `third_party/llama.cpp`.
  That made the feature unusable rather than slow: the copy alone outran the 600s per-command timeout and filled the disk, so `cargo test --workspace` never started and every review came back with zero checks and the risk *"Verification produced no results — change is UNVERIFIED"*.
  The copy now skips `models`, `third_party`, `dist`, `build` and `node_modules`, plus any individual file over 64 MiB (a RAG index is a few hundred MB of embeddings and is regenerated by `/index`). Measured on this repo: **40,072 MB → 134 MB, a 299x reduction**.
  Also refuses a destination inside the source. `copy_dir_recursive` enumerates the source, so a nested destination is recursed into indefinitely — an unbounded copy and a stack overflow rather than a clean error. Production creates the scratch dir separately so this cannot fire today, but the failure mode is silent and total.
  3 new tests: the skip list, the oversized-file guard (written in chunks — a single 65 MB `vec!` overflows a test thread's stack), and the nested-destination refusal.

- **Scheduled turns now fire promptly instead of up to five minutes late** (`crates/ghost-link/src/main.rs`):
  the scheduler driver slept `MAX_TICK` (300s) whenever nothing was due, and nothing could interrupt it. A schedule created just after a tick therefore went unnoticed for up to five minutes — long enough that a freshly created `at` schedule looked broken, and a 09:00 cron entry could fire materially late.
  Verified live: a due `at` schedule took **over four minutes** to fire. Instrumenting the driver showed a single tick with `sleep_ms=300000` followed by nothing at all. After the fix the same schedule fires in **5.0s**.
  The driver now selects on its timer *or* a `tokio::sync::Notify` that the create / enable / disable / delete routes trip. A `Notify` rather than a `Condvar` because the driver is async and awaiting a notification must not block a runtime worker thread.
  2 new tests: a nudge wakes a driver that has nothing due (asserting the dispatch happens in ~5s, so a regression fails by timeout rather than passing slowly), and repeated nudges against an empty store are harmless.

- **The native launcher no longer serves a stale binary** (`launch-native.ps1`):
  the build step was guarded by `if (-not (Test-Path $ApiBin))`, so `ghost-link.exe` was compiled on first run only. Every launch after that reused whatever binary happened to exist — an edited source tree kept serving the code from whenever that file was created.
  This is not hypothetical. `ghost-link.exe` was current while `mcp-memory.exe` and `mcp-rag.exe` remained a day older than their sources, because the launcher never built the MCP servers at all. The result was a live server whose retrieval path predated its own relevance floor.
  Two changes:
  - `ghost-link` is now built on **every** launch. cargo is incremental, so an unchanged tree costs seconds.
  - The MCP servers referenced by path from `mcp_servers.toml` (`mcp-memory`, `mcp-rag`, `mcp-calculator`, `mcp-vision`) are now built too, guarded by a source-vs-binary timestamp check so a normal launch stays fast but a source change is never served stale. A failed MCP build warns rather than aborting: a missing optional server costs tools, not the server.
  Note the package name is `ghost-link`, not `ghostlink`. `cargo build -p ghostlink` matches no package and builds nothing — a silent no-op rather than an error.

- **RAG search no longer returns irrelevant documents** (`crates/mcp-rag/src/main.rs`):
  `search` had no relevance floor — it always returned the top-k *closest* entries, even when none of them matched. Measured on a one-document index: a topically unrelated query (*"quantum chromodynamics lattice gauge theory"*) still scored **0.454** against the only chunk, so the "closest" result was a document about credential rotation.
  That matters because proactive recall injects these hits into the model's context as established fact. A floorless search hands the model the least-relevant thing in the index dressed as an answer.
  Added `min_score` (cosine floor), applied **before** top-k truncation so a top-k full of weak matches cannot push a relevant result out of the list.
  The default is measured rather than guessed. Nine queries against one chunk on `nomic-embed-text-v1.5`:
  ```text
  related    0.629  0.695  0.625  0.591     (min 0.591)
  unrelated  0.454  0.429  0.441  0.404  0.458   (max 0.458)
  ```
  `DEFAULT_MIN_SCORE = 0.52` sits mid-gap, so it keeps all four related queries and drops all five unrelated ones. `min_score: 0.0` restores the old always-return-top-k behaviour.
  A first attempt used 0.45, picked from a two-sample comparison that did not separate the cases. A test pinning the measurement failed and caught it — which is why the full nine-query dataset is recorded in the source next to the constant.
  5 new tests: the floor drops a weak match, keeps a strong one, is applied before truncation, `0.0` disables it, and the constant still sits inside the measured gap.

- **The assistant no longer reports actions it never performed** (`crates/ghost-link/src/grounding.rs`, wired into the buffered and streaming chat paths):
  Asked to "remember that my favourite tea is sencha", Ghostlink replied *"I've saved your favorite tea as sencha"* — over an empty memory database. It had been offered **no tools at all**, since tools are opt-in per request via `mcp.tools`, and it filled the gap with a confident, specific, fictional action report.
  Two independent layers, because a prompt instruction is neither reliable nor enforcement:
  1. **A capability statement** placed immediately above the tool list, naming what the model can actually do this turn. When the list is empty it says so outright — an omitted list reads as "unmentioned", which the model completes with capability it does not have.
  2. **A server-side claim audit** (`grounding::unverified_action_claims`) that re-checks the reply against the server's own record of executed tools. It does not trust the model at all, so it still fires if the prompt layer is removed.
  The correction is appended as server-authored text prefixed `> **Note from Ghostlink:**`, so a user can tell a correction from a hallucination. `action_claim_corrected` and `tools_run` are exposed on the response, and the trace records the event as a `Blocked` tool decision — never the reply text, which is the content under suspicion.
  Grounding is deliberately conservative: the audit only fires when **no** tool ran, and an explicit list of non-claim patterns keeps truthful refusals ("I can't save that right now") from being corrected. A false positive would append a spurious notice to a fine reply and train the user to ignore it.
  The streaming path is covered too — the GUI streams by default, so fixing only the buffered path would have left the common case unprotected. Streamed text is accumulated to a bounded 32 KiB for the audit, and the notice is sent **before** the state lock is taken, since an `await` while holding a `std::MutexGuard` makes the spawned task non-`Send`.
  Verified live: the exact failing prompt now returns `action_claim_corrected: true` with the correction appended, and four ordinary replies (factual, code request, how-to, and one that *discusses* memory without claiming a write) produced no false positives.
  12 new tests, weighted toward the negative cases — an honest refusal must never be corrected, an ordinary answer must never be corrected, and the audit must fire with no prompt cooperation at all.


## [2.3.0] - 2026-10-03

### Added
- **Inference latency, streaming, and conversation memory** (`crates/ghost-link/src/native_engine.rs`, `crates/ghost-link/src/main.rs`, `crates/ghost-link/src/ollama.rs`, `crates/ghost-link/src/host_metrics.rs`, `crates/ghostlink-core/src/kv_cache.rs`, `ghostlink_gui_modern/src/api.ts`, `ghostlink_gui_modern/src/components/ChatTab.tsx`):
  A measured pass over the local inference path. Benchmarked on the reference host (Llama-3.2-1B Q3_K_M, `llama-server`): time-to-first-token, decode throughput, and the transport primitives, with every change verified against a live server rather than asserted.
  **Prompt caching was defeated by the system prompt.** `default_system_prompt()` interpolated `chrono::Local::now()` into the *system* message, so every request began with a different byte and `cache_prompt` could never reuse a slot's KV prefix. Split into `static_system_prompt()` (byte-stable, cacheable) and `dynamic_context_suffix()` appended to the *user* turn, on all three backend paths. Measured: warm TTFT **36ms** with a stable prefix vs **113ms** with a per-request one (**3.1x**), and the first request after a load dropped from ~134ms to ~90ms via a new best-effort warmup generation at the end of `load_model_into_slot`.
  **Conversations are now pinned to a llama-server slot.** `slot_for_session()` maps a session id to a stable slot (FNV-1a — stable across processes, unlike `DefaultHasher`) so a returning multi-turn conversation lands on the slot still holding its prefix. With `-np > 1` and `id_slot: -1`, llama-server could hand a follow-up turn a slot whose cached prefix belonged to a different conversation. Verified against llama-server's `/slots`: two sessions occupy two distinct slots, each retaining its own prompt.
  **TTFT is now measured and exposed.** `InferenceMetrics::record_ttft()` records time-to-first-token separately from total generation latency; `ttft_p50_ms`/`ttft_p95_ms`/`last_ttft_ms`/`ttft_samples` appear on `/api/metrics` and as `ghostlink_inference_ttft_p50_milliseconds` in the Prometheus output. Confirmed live: `/api/metrics` reported `ttft_p50=15.97ms`.
  **History is summarized instead of silently dropped.** Past `conversation_token_limit`, turns were discarded FIFO with no trace — a long conversation forgot its own beginning. Dropped turns are now folded into a per-session running summary (in-memory, LRU-capped at 64 sessions, text capped at 4000 chars) which is prepended to later requests as a second `system` message. Summarization runs detached from the request path, is incremental (the existing summary plus the newly dropped turns are rewritten together), and is best-effort — a failure leaves the previous summary untouched. Verified end-to-end: a turn sent with **no history at all** still answered correctly about facts that existed only in the dropped turns.
  **Ollama gained real streaming.** Its chat branch always called the non-streaming `generate` and the handler faked a token stream from the finished string, so nothing appeared until the whole answer existed; `OllamaClient::chat_stream` was fully implemented but had zero callers. It is now the live path for `stream: true`, emitting the same `{"token":…}` frames and terminal done frame as the native path, with the first-token timeout and TTFT recording applied.
  Also: a separate first-token timeout (`GHOSTLINK_FIRST_TOKEN_TIMEOUT_SECS`, default 30, `0` disables) distinct from llama-server's inter-chunk idle timeout; `finish_reason: "length"` surfaced as `truncated` the moment the backend reports it rather than only on the terminal chunk; GUI token updates batched to one React state write per animation frame instead of one per token; and `sessions.json` growth bounded by `GHOSTLINK_SESSION_MAX_AGE_DAYS` (default 30) and `GHOSTLINK_SESSION_MAX_BYTES` (default 20MB), never dropping the current session.
  Two bugs found while measuring: `api.ts` split each decoded network chunk on `\n` and parsed the fragments independently, silently dropping any SSE frame that straddled a read boundary (losing tokens from the rendered text); and `kv_cache.rs`'s owned `read_kv`/`read_range` allocate per call and are **100x–350,000x** slower than the zero-copy closure forms (`read_range_1024/wide`: 4.57ms vs 12.8ns). The module has no callers, so the owned forms are now `#[doc(hidden)]` with the measured cost documented in the module header, keeping the zero-copy API as the only supported read path.

- **Automatic verification: a project's own tests now gate acceptance** (`crates/ghost-link/src/task_runtime.rs`, `crates/ghost-link/src/task_api.rs`, `docs/TASK_AGENTS.md`):
  `ReviewPacket.checks` previously contained only commands the *model chose* to run, so "it passed checks" meant "the model ran something" rather than "the project's tests pass" — and a packet with zero checks was equally acceptable. That is how unverified work reaches `main`, and it is the same failure mode this repository has already been burned by twice (the fabricated benchmark tables, and a "Context Governor" that an audit marked `verified` despite not existing).
  `VerificationPlan::detect` now derives a project's definition of done from its own build files — `Cargo.toml` -> `cargo test --workspace`, a `package.json` with a `test` script -> `npx vitest run`, `pyproject.toml`/`pytest.ini` -> `python3 -m pytest` — so it works on any repository with no configuration. Before a `ReviewPacket` is produced, the staged files are overlaid onto a scratch copy of the project and the plan runs there; the live tree is never modified. Results are stored in `ReviewPacket.verification`, and `POST /api/reviews/:id/decide` refuses `accept` when verification failed **or was never run**, returning 409 with the failing commands. `override_verification: true` is the explicit, recorded way to accept anyway. Plan commands go through the same `Judge` policy as agent-issued commands, so a plan cannot smuggle in a denied command, and a plan that requests one is reported as a configuration error rather than executed.
  `ReviewPacket::verification_passed()` returns `Option<bool>` so "unverified" and "verified and failed" stay distinguishable — collapsing them is exactly how an unchecked change ends up looking as good as a tested one.

- **Task Agent End-to-End Wiring & Studio Chat Integration** (`crates/ghost-link/src/task_api.rs`, `crates/ghost-link/src/main.rs`, `ghostlink_gui_modern/src/api.ts`, `ghostlink_gui_modern/src/App.tsx`, `ghostlink_gui_modern/src/components/ProjectsTab.tsx`, `ghostlink_gui_modern/src/components/TaskView.tsx`, `ghostlink_gui_modern/src/components/ChatTab.tsx`, `ghostlink_gui_modern/src/api.test.ts`, `docs/TASK_AGENTS.md`):
  Added `POST /api/chat/agent` endpoint in `task_api.rs` gated by `Operator` RBAC role. Updated `GhostlinkAPI` client with `startChatAgent`, `getApiBaseUrl`, response interceptors for circuit-breaker failure tracking, and error propagation. Wired `App.tsx` shared authenticated `GhostlinkAPI` client through `ProjectsTab` and `TaskView` without fallback client credential loss. Enhanced `TaskView` SSE EventSource URL resolution using base URL and added polling fallback during disconnection. Added Agent Mode toggle and workspace root path validation to `ChatTab`, rendering live task cards with event streaming and inline `ReviewPane` decision handling (`accept`, `reject`, and `request_changes` respawning with note as brief). Expanded unit and component tests in `api.test.ts`.

- **Real Task Agent Tool Loop & v2.4 Child Fan-Out**: Replaced stub canned implementer loop with bounded tool loop using in-process `AgentBackend` trait, `Judge` policy enforcement, proposed file staging in `.ghostlink/tasks/<id>/proposed/`, real command execution, child task fan-out APIs (`/api/tasks/:id/children`), budget inheritance, and UI child task representation. (`crates/ghost-link/src/task_runtime.rs`, `crates/ghost-link/src/task_api.rs`, `ghostlink_gui_modern/src/components/TaskView.tsx`, `ghostlink_gui_modern/src/components/ProjectsTab.tsx`)

- **v2.3 Task Agent Server Engine & Router Extraction** (`crates/ghost-link/src/task_api.rs`, `crates/ghost-link/src/task_runtime.rs`, `crates/ghost-link/src/main.rs`, `ghostlink_gui_modern/src/components/TaskView.tsx`, `docs/TASK_AGENTS.md`):
  Extracted task API handlers from `main.rs` into `task_api.rs` and mounted the router in `main.rs`. Implemented Phase A Task Agent server with atomic JSON storage under `GHOSTLINK_DATA_DIR` (.ghostlink/data), workspace path canonicalization and staging isolation under `.ghostlink/tasks/{task_id}/proposed/`, deterministic `Judge` policy (allowlist/denylist/pause), implementer loop execution with budget limits (`max_steps`, `max_minutes`, `max_tokens`), SSE event stream with `?access_token=` authentication in `TaskView.tsx`, and human decision workflows (accept/reject/request_changes/cancel). Updated `docs/TASK_AGENTS.md` to document implemented v2.3 server capabilities vs planned v2.4 features.

- **Phase 3 KV Cache Microbenchmarks and Zero-Copy Read Paths** (`crates/ghostlink-core/src/kv_cache.rs`, `benches/kv_cache.rs`, `crates/ghostlink-core/Cargo.toml`, `docs/BENCHMARKS.md`):
  Added dedicated Criterion benchmark harness target (`benches/kv_cache.rs`) covering small, default, and wide configurations across 7 workloads. Implemented zero-copy read APIs (`with_read_kv`, `with_read_range` using `KVSpan`, `read_range_into`) in `LayerKvCache` to eliminate owned `Vec` allocations during attention read paths, achieving 15-18x speedups on decode-step loops and up to 3900x under reader concurrency.

- **Cross-Platform Launcher Bootstrap and Readiness Fixes** (`launch.bat`, `launch-native.ps1`, `launch.sh`):
  Added PowerShell Core compatibility, native Windows toolchain preflight checks, UTF-8 runtime GUI configuration, strict readiness status handling, and corrected the Linux success-screen URL.

### Fixed
- **Task-agent context budget, and correction of a fabricated context-governor claim** (`crates/ghost-link/src/task_runtime.rs`, `docs/TASK_AGENTS.md`, `docs/LOCAL_INFERENCE_TUNING.md`, `docs/CHANGELOG_AUDIT.md`):
  The agent loop's `messages` vector grew without bound — only individual tool outputs were truncated — so a long task on a small local context window hit a context overflow and the backend call failed. `task_runtime::ContextGovernor` now packs the transcript to fit a token budget before every backend call, pinning the system prompt and the original task goal and keeping the most recent turns; older turns are dropped oldest-first. Compaction is not silent: it is recorded as a `risks[]` entry on the `ReviewPacket` and emitted as a `context_compacted` SSE event, so a reviewer knows the agent worked from a truncated transcript.
  Separately, the `[2.2.2]` "Context Governor System" entry and the matching sections in `docs/LOCAL_INFERENCE_TUNING.md` and `docs/CHANGELOG_AUDIT.md` described settings and functions that **do not exist**: `policy` (sliding_window/truncate_oldest/compact), `keep_last_turns`, `reserve_completion_ratio`, `sticky_slot`, `n_ctx`, `n_ctx_auto`, `apply_context_governor_async`, a `get_ctx_size_budgeted` memory-cap change, a Context Meter UI, and IndexedDB thread storage. A repo-wide search finds none of them. Those claims are replaced with what is actually implemented (`conversation_token_limit` + `trim_conversation_history_async`), and `CHANGELOG_AUDIT.md`'s `verified` marking — which was wrong — is corrected.

- **Task-agent runtime: reversible applies, real diffs, retry, and honest token accounting** (`crates/ghost-link/src/task_runtime.rs`, `crates/ghost-link/src/task_api.rs`, `docs/TASK_AGENTS.md`, `.gitignore`):
  `apply_proposed_changes` was a blind `fs::copy` over the live tree with no backup and no rollback — an `accept` on a dirty working tree was unrecoverable, and the handler wrapped the call in `let _ =`, so a failed apply still marked the task `Accepted`. Every accept now snapshots the files it is about to overwrite into `.ghostlink/tasks/<task_id>/backup/` with a manifest of paths it created, `decision: "rollback"` restores them, a failed apply returns 500 instead of a false success, and a second rollback errors rather than silently restoring stale content. `ReviewPacket.diffs[].unified_diff` previously carried a hardcoded `@@ -0,0 +1,3 @@` hunk header regardless of content; it is now a real LCS-based unified diff with correct line numbers and hunk ranges (no new dependency). The agent loop now retries `AgentBackend::chat` up to 3 times with 1s/2s backoff and emits a `retry` event, instead of failing the whole task to `Blocked` on one transient backend error. Token accounting prefers real usage counts when the backend reports them (Ollama's `prompt_eval_count`/`eval_count`, threaded through a new `AgentResponse::usage_tokens`) and only falls back to the character estimate otherwise — previously it counted assistant text alone, charging a flat 10 tokens for tool-call-only turns. `POST /api/tasks/:id/spawn` now rejects an unknown `role` with 400 rather than silently treating it as a non-planner. `.ghostlink/` (the runtime store and per-task staging tree) is now gitignored, so an agent's proposed files can never be committed into a user's repository.

- **Phase 1 RPC Contributor Hardening, Secret Trimming, Derived-Port Validation, and Graceful Process Shutdown** (`crates/ghost-link/src/rpc_cluster.rs`, `crates/ghost-link/src/main.rs`):
  Trimmed `rpc_shared_secret` input whitespace across security validation and auth-port initialization, treating whitespace-only secrets as unset to prevent running `handle_auth_handshake` with whitespace keys. Implemented `validate_derived_ports` to enforce bounds and prevent port collisions between base `rpc_port`, internal loopback proxy (+1000), and auth handshake listener (+2000). Deduplicated unauthenticated `ggml-rpc-server` startup tracing warnings into `warn_unauthenticated_rpc`. Un-suppressed `stop_contributing()` and wired contributor process cleanup into server process shutdown on SIGINT/SIGTERM/Ctrl+C in `main.rs` (`mcp_shutdown_on_ctrl_c`), ensuring idempotent cleanup of supervised `ggml-rpc-server` child processes.
- **Gateway & Studio EventSource Auth Alignment**: Studio EventSource and control-plane now share jwt_secret.txt and accept ?access_token= on task SSE only.
- **Documentation-truth and repo-hygiene cleanup** (`AGENTS.md`, `CHANGELOG.md`, `crates/ghostlink-core/src/cluster.rs`, `crates/ghost-link/src/rpc_cluster.rs`):
  Corrected the `AGENTS.md` port hierarchy (the GUI reaches `:8000`; the Vite dev proxy forwards `/api`, `/health`, and `/v1` there, rather than the GUI calling `:8003` directly) and fixed a broken markdown link in the security section. Added RPC peer admission and contributor supervision to the security-sensitive-areas list under their real names (`validate_non_loopback_rpc_security`, `ip_allowed`, `admit_via_secret`, `RpcSupervisor`, `is_contributing_healthy`). Pointed the benchmark guidance at the repo's actual scripts (`scripts/flow_perf_snapshot.py`, `scripts/remote_flow_benchmark.py`) instead of a non-existent integration suite. De-duplicated four byte-identical copies of the "First-run health" / "Port alignment" pair that had been pasted into `[2.0.0]`, `[1.17.0]`, `[1.16.0]`, and `[1.3.2]`, and collapsed the `[Unreleased]` section's three duplicate `### Added` headers into one. Removed an unreachable, half-merged contributor-supervision implementation from `ghostlink-core`'s `ClusterState` (it spawned `ghost-link stage-worker` instead of `ggml-rpc-server`, derived its port from `node_id.len()`, and was called from nowhere) — the real, committed fix for the problem it claimed to solve is `rpc_cluster::RpcSupervisor`.
- **CI/workflow correctness and repo-hygiene follow-ups** (`.github/workflows/agentic-tests.yml`, `.github/workflows/ci.yml`, `.github/workflows/lint.yml`, `scripts/verify_no_case_collisions.sh`, `.jules/palette.md`, `docs/BENCHMARKS.md`):
  `agentic-tests.yml` declared a `services:` container on `image: ghostlink:latest`, which nothing in this repo builds or pushes, so the job could never start. It now builds `llama-server` from the vendored tree, downloads the same small GGUF the Docker fabric uses, starts `ghost-link serve`, waits on `/health`, loads the model, and runs the five agentic test scripts — failing with an explicit message when a prerequisite (vendored llama.cpp) is absent instead of silently passing. Removed a `performance-profile` job whose only step was an `echo` placeholder. Added `scripts/verify_no_case_collisions.sh` (wired into `ci.yml`'s new `repo-hygiene` job and `lint.yml`) after finding `.Jules/palette.md` and `.jules/palette.md` both tracked for months: on a case-insensitive filesystem they are the same file, so Windows contributors could never get a clean checkout. Merged the two files' unique journal entries into the canonical `.jules/palette.md` (32 entries) and deleted the duplicate. Restructured `docs/BENCHMARKS.md`'s five near-identical `## KV cache microbenches` sections — which re-pasted the same date/hardware header, NOTE, and baseline table five times — into five labelled raw run logs plus one shared header and the two distinct baseline tables; all 162 measurements and 171 unique table rows are preserved verbatim, and the mangled NOTE line ("memory copies in .") is corrected.


## [2.2.2] - 2026-09-26

- **Full First-Run Stack Launchers and Release Pipeline Automation** (`launch.bat`, `launch-native.ps1`, `launch.sh`, `.github/workflows/tag-release-on-main.yml`):
  Added native Windows and Unix launcher support, dependency preflight checks, lazy backend builds, default model bootstrap, gateway configuration, and manifest/changelog validation for release tagging.

- **Secure-by-Default RPC Fabric & IPv6 CIDR Support (PR 3)** (`crates/ghost-link/src/rpc_cluster.rs`, `docs/SECURITY_MODEL.md`):
  Enforced fail-closed security validation (`validate_non_loopback_rpc_security`) for non-loopback RPC listener binds, requiring both `rpc_shared_secret` and `rpc_allowed_peers` to be set when binding outside loopback. Added full IPv6 exact address and IPv6 CIDR range matching to `ip_allowed`. Documented HMAC challenge-response peer admission vs wire-level tensor payload encryption limits in `docs/SECURITY_MODEL.md`.

- **Repository Hygiene and Archive Consolidation (PR 2)** (`docs/archive/ENTERPRISE_PLAN.md`, `docs/archive/INDEX.md`, `crates/ghost-link/src/main.rs`, `docs/BENCHMARKS.md`):
  Consolidated parallel legacy root `_archived/` directory into `docs/archive/` and removed `_archived/` to maintain a single unified documentation archive index (`docs/archive/INDEX.md`). Moved commercial go-to-market plan `ENTERPRISE_PLAN.md` out of active user documentation into `docs/archive/`. Archived `docker-compose.demo.yml` and updated internal rust path exclusions in `crates/ghost-link/src/main.rs`.

- **Identity and Documentation Refactoring (PR 1)** (`README.md`, `docs/QUICKSTART.md`, `docs/ROADMAP.md`, `docs/KNOWN_LIMITATIONS.md`):
  Refactored core documentation to emphasize Ghostlink's primary product identity and happy path: peer discovery -> GGUF tensor split via `ggml-rpc` -> OpenAI-compatible `/v1/chat/completions`. Explicitly demoted synthetic pipeline tools (`flow`), SPSC ring buffers, and AF_XDP as experimental research components whose synthetic tok/s metrics do not represent LLM inference speed. Added a 2-node reproduce recipe in `docs/QUICKSTART.md` referencing real runs from `BENCHMARKS.md`. Resolved ROADMAP drift for installer scripts in `docs/ROADMAP.md`. Created `docs/KNOWN_LIMITATIONS.md` detailing unencrypted `ggml-rpc`, 30B split decode performance (~1.5–2.5 tok/s), x86_64 installer limits, standalone CLI binary release scope, and LAN-trust security model.

- **Long-Session Context Policy** (`crates/ghost-link/src/main.rs`, `ghostlink_gui_modern/src/store.ts`, `ghostlink_gui_modern/src/components/SettingsTab.tsx`, `docs/LOCAL_INFERENCE_TUNING.md`):
  Added `conversation_token_limit` as a typed `RuntimeSettings` field surfaced through `/api/settings` and the GUI, and `trim_conversation_history_async` on the chat path, which trims prior turns oldest-first to fit that budget (using the native tokenizer when available, a whitespace-word estimate otherwise). This was the first time the stored limit was actually applied to anything.

  **Correction (2026-10-02).** This entry previously claimed a "Context Governor System". A repo-wide search finds **none** of it: `apply_context_governor`, `keep_last_turns`, `reserve_completion_ratio`, `sticky_slot`, `n_ctx`, and `n_ctx_auto` appear nowhere in `crates/` or `ghostlink_gui_modern/src/`, and there is no Context Meter or IndexedDB usage in the GUI. The only parts that exist are `conversation_token_limit` (a `RuntimeSettings` field), `kv_cache_type`, `max_tokens`, and `trim_conversation_history_async` — which is what the entry has been cut back to. `docs/CHANGELOG_AUDIT.md` listed this entry as `verified`; it was not.

  A **real** context budget now exists for the task-agent loop specifically: `task_runtime::ContextGovernor` packs the agent transcript to fit before every backend call, pinning the system prompt and task goal and keeping the most recent turns. See the task-agent entry under `[Unreleased]`.

  The chat path still has only `trim_conversation_history_async`'s oldest-first truncation — no sliding-window/compact policies, no summarization, and no reserved completion budget.

- **Ops Tabs Fixes (Metrics, Sessions, Workers)**:
  - **Metrics Tab & Streaming Metrics Recording** (`crates/ghost-link/src/main.rs`, `ghostlink_gui_modern/src/api.ts`, `ghostlink_gui_modern/src/store.ts`, `ghostlink_gui_modern/src/components/MetricsTab.tsx`):
    Recorded generation latency and emitted tokens for native streaming SSE chat completions in `crates/ghost-link/src/main.rs` upon stream completion, ensuring real throughput (tok/s) and latency (ms) metrics are calculated. Mapped `timestamp_ms` to `t` in `api.ts` `getMetricsHistory()` and synchronized Zustand store history state with backend `/api/metrics/history` polling in `App.tsx`. Updated `MetricsTab.tsx` to render "no samples yet" and "Waiting for samples" when `samples == 0` rather than plotting false zero states.
  - **Sessions Tab & Live Session Tracking** (`crates/ghost-link/src/main.rs`, `ghostlink_gui_modern/src/api.ts`, `ghostlink_gui_modern/src/components/SessionsTab.tsx`):
    Updated `handle_gui_chat` in `crates/ghost-link/src/main.rs` to track active inference sessions and record token counts, throughput (tok/s), latency (ms), model name, and status (`Running`/`Degraded`). Unified `getSessions()` and `listSessions()` client methods in `api.ts`. Formatted session stats in `SessionsTab.tsx` to display "—" for missing/unmeasured metrics and verified saved session thread hydration with `user` and `assistant` message roles.
  - **Workers Tab & Cluster Setup Wizard** (`ghostlink_gui_modern/src/api.ts`, `ghostlink_gui_modern/src/components/WorkersTab.tsx`):
    Wired peer discovery in `api.ts` (`discoverWorkers` / `discoverPeers`) and updated discovery toast feedback to report peer count. Defaulted Add Worker port to `8003` with 1–65535 validation. Identified local coordinator dynamically by `role`/local ID rather than assuming index 0. Added peer status details (`build_id_status`, `secret_status`, `allowlist_status`) and `excluded_reason` banners to peer cards. Built a Cluster Setup Wizard section and added a hard confirmation prompt before disconnecting the local coordinator node.

## [2.2.1] - 2026-09-06

- **Metrics History, Saved Sessions, and GUI Dependency Lock Repair**:
  Normalized `/api/metrics/history`'s `timestamp_ms` payload into the GUI's timestamped history model so trend cards and utilization charts render real server samples. Saved sessions now display their names, open their persisted messages as a chat thread, and can be permanently deleted with `DELETE /api/sessions/:id`; session cancellation now persists its `Cancelled` status instead of returning a no-op success. Synchronized `ghostlink_gui_modern/pnpm-lock.yaml` with its declared dependencies so a frozen install is reproducible.

- **RPC Fabric Soak & Contributor-Kill Drain Harness**:
  Added `scripts/rpc_fabric_soak.py` and `tests/test_rpc_fabric_soak.py` to test and assert drain-and-restart behavior across Docker RPC peers without requiring a GPU or large model.
  Asserts peer discovery (2+ healthy nodes), baseline model generation, contributor container stop/kill (`ghostlink-rpc-contributor`), immediate coordinator purging of the dead node from `active_rpc_targets` in `/api/cluster/topology`, clean cancellation or failure of in-flight/subsequent work without hanging past timeout, and optional contributor restart and re-admission.
  Wired unit tests and an optional soak step into `.github/workflows/distributed-e2e.yml` with Docker availability guards.
  Updated `docs/TESTING.md` and `docs/DEPLOYMENT.md` with "how to soak" runbook instructions.

- **Strict Default Exclusion for Unknown ggml-rpc Build Fingerprints**:
  `rpc_cluster::discover_rpc_peers` and `evaluate_peer` now exclude peers with missing or unknown `rpc_build_id` build fingerprints by default (`excluded_reason: "RPC build fingerprint missing"`), matching the strict security stance of explicit build mismatches (`excluded_reason: "RPC build does not match coordinator"`). Added `rpc_allow_unknown_build_id` (settings.json, default `false`) and `GHOSTLINK_RPC_ALLOW_UNKNOWN_BUILD_ID` environment variable to opt back into admitting unknown build fingerprints in mixed-version lab environments.

- **Coordinated SDK Publication and Artifact Version Gates**:
  Aligned `ghostlink-core`, `ghost-link`, Ghostlink Studio, and the JavaScript and Python SDKs at `2.2.1`. Added tag-triggered npm and PyPI publication after each SDK's test, typecheck/build, and distribution validation; publication requires `NPM_TOKEN` and `PYPI_API_TOKEN`, respectively. Crate and artifact workflows now reject a release tag when its publishable manifest versions do not match the tag.

- **Published Rust MSRV Corrected to 1.89**:
  Updated `ghostlink-core` and `ghost-link` package metadata from stale `rust-version = "1.85.0"` to `1.89.0`, matching the repository's MSRV CI workflow and the current locked dependency graph.


## [2.2.0] - 2026-09-04

- **`rpc_cluster::compute_tensor_split` weighted capacity heuristic**: CPU-only nodes (0 VRAM) now use system RAM scaled by a conservative `CPU_RAM_HAIRCUT` factor (0.5) to receive a proportional tensor split share, enabling CPU-only peers to take real work when needed to fit models while maintaining GPU preference when discrete VRAM is present.
- **Production LAN Runbook & Post-Hardening Current Truth**:
  - Added production LAN deployment checklist to `docs/DEPLOYMENT.md` covering build/version matching, `rpc_shared_secret` HMAC challenges, `rpc_allowed_peers` allowlisting, opt-in `contribute_compute`, contributor process supervision (`RpcSupervisor`), drain-and-restart on contributor loss, and system port architecture (`:8000` public vs `:8003` internal API).
  - Updated `README.md` and `docs/ROADMAP.md` "Current truth" statements, strengths, remaining known gaps, and compact benchmark table.
  - Maintained canonical pointers in `docs/archive/INDEX.md` and updated `docs/COMPARISON.md` references.
- **Hardened Docker RPC Fabric E2E Assertions & Connectivity-Only Mode**:
  - Hardened `scripts/rpc_fabric_assert.py` to assert two healthy peers, `real_inference: true`, live RPC accept/connection evidence in `ggml-rpc-server.log`, and placement plan verification.
  - Correctly labels `-ngl 0` (CPU-only) runs as "connectivity-only" and asserts `distributed_active: false` (preventing false compute split claims when `-ngl 0`), while supporting `--require-compute-split` for GPU runners (`-ngl > 0`).
  - Added unit tests in `tests/test_rpc_fabric_assert.py` using mock log and topology fixtures (`scripts/testdata/`).
  - Updated `.github/workflows/distributed-e2e.yml` with documented comments/inputs for GPU vs CPU runners and unit test execution.
  - Added `docs/TESTING.md` documenting local execution of the Docker RPC fabric and assertion script.
- **Scoped Multi-Key RBAC for HTTP API**:
  - Expanded role-based API key access control (`crates/ghost-link/src/auth.rs`) to support explicit roles: `owner`, `operator`, `inference`, and `viewer`.
  - Keys are persisted in `api_keys.json` (SHA-256 hash + last-4 preview only, raw key shown once upon minting and never stored/recoverable).
  - First-run generates a single `owner` key printed to startup output and saved to `api_keys.json`.
  - `/api/security/keys` (list, create, revoke) is strictly `owner`-gated.
  - `/v1/*` and `/api/inference/*` routes are accessible by `inference`, `operator`, and `owner` roles.
  - `/health` remains public.

- **Audit Log Capping, Rotation, and Retention**:
  - Implemented file capping (`GHOSTLINK_AUDIT_LOG_MAX_BYTES`, default 10MB; `GHOSTLINK_AUDIT_LOG_MAX_LINES`, default disabled) and automatic file rotation (`audit_log.jsonl.1` .. `.N`) for the durable on-disk audit log.
  - Implemented retention purging (`GHOSTLINK_AUDIT_LOG_MAX_FILES`, default 5 files) to prevent unbounded disk growth.
  - Added support for `[audit]` configuration table in `ghostlink.toml` (`path`, `max_bytes`, `max_lines`, `max_files`).
  - Updated `read_all_durable()` and SIEM export endpoints (`GET /api/security/audit-log/export?format=json|cef`) to read across active and retained rotated log files in chronological order.

- **Drain and Restart on Contributor Loss for Serving Path**:
  - Implemented continuous rebalancing / resilience for the real llama-server RPC serving path (InferenceEngine::Native).
  - Added active --rpc target tracking in NativeEngineClient (active_rpc_servers()) and contributor health checking in rpc_cluster::check_active_rpc_peers_healthy.
  - When an RPC contributor disappears mid-load or mid-request, Ghostlink unloads/drains non-viable server topology, failing/cancelling requests cleanly without output corruption. Live mid-token stage migration remains an open roadmap item.
  - Updated handle_gui_session_cancel to update and persist session cancellation state.

- **No-Op Distributed Offload Prevention & Warning**:
  - When `distributed_inference: true`, if effective `-ngl` is `0` (CPU-only) or total remote tensor share is below the 1% minimum threshold (`MIN_REMOTE_SHARE_THRESHOLD = 0.01`), Ghostlink avoids passing `--rpc` and `-ts` flags to `llama-server`.
  - Emits a structured `tracing::warn!` log and includes a clear warning note in the placement plan `summary_text` shown by the GUI (`"Single-machine inference on local node for ... (Distributed offload warning: ...)"`), setting `distributed_active: false`.
  - Added `require_cluster_offload: bool` (`GHOSTLINK_REQUIRE_CLUSTER_OFFLOAD`) in `RuntimeSettings`: when enabled, a no-op distributed offload returns a hard load error rather than falling back to single-node.
- **Dynamic Model-Ready Timeout Scaling**:
  - Replaced hardcoded 90s/600s timeouts in `NativeEngineClient` with `compute_model_ready_timeout(args, model_size_gb)`:
    $$\text{timeout} = \text{clamp}(90\text{s (floor)} + (\text{model\_size\_gb} \times 15\text{s}) + (\text{peer\_count} \times 60\text{s}), 90\text{s}, 1800\text{s})$$
  - Fully overridable via `GHOSTLINK_MODEL_READY_TIMEOUT_SECS` env var or `RuntimeSettings`.

- **Supervised `ggml-rpc-server` Child Process & Dead Contributor Revocation**: `rpc_cluster::ensure_contributing()` now supervises the `ggml-rpc-server` child process with bounded exponential backoff (1s to 30s, capped at 10 consecutive restarts) and double-bind / zombie process cleanup. A node advertises `contribute_compute` over UDP/mDNS discovery and cluster map topology APIs *only* while its `ggml-rpc-server` child process is running AND listening on its RPC port. If the child process crashes (e.g. KV-cache OOM or backend panic), discovery advertisement is revoked immediately and cluster topology displays `contribute_compute: false` with `excluded_reason: "rpc child not running"`, plus optional supervisor fields (`rpc_child_pid`, `rpc_child_restarts`, `rpc_child_status`, `rpc_child_last_exit`).
### Documentation & Truth Alignment

- **Version Badge & Development Note**: Clarified that `main` branch development is ahead of the last tagged release (`v2.0.0`) and aligned version badge claims in `README.md`.
- **Distributed Inference vs Research Pipelines**: Clarified in `README.md` that production OpenAI-compatible endpoints (`/v1/chat/completions`) run via llama.cpp's `ggml-rpc` tensor splitting, while zero-copy SPSC ring buffers and `flow` pipelines serve as transport/latency research components.
- **Role-Based API Key Access Control Reconciliation**: Updated `README.md`, `docs/SECURITY_MODEL.md`, `docs/ROADMAP.md`, `docs/ENTERPRISE_PLAN.md`, and `docs/API_REFERENCE.md` to precisely distinguish role-based API key access control (`Admin`, `Operator`, `Viewer` roles on API keys in `crates/ghost-link/src/auth.rs`) from full multi-user / multi-tenant RBAC (user identities, team/project scoping, per-resource permissions).
- **Current Truth Callouts**: Added 'Current truth' callout boxes to `docs/ROADMAP.md` and `docs/BENCHMARKS.md` documenting verified capabilities (30B model capacity split across nodes) vs unproven/in-progress capabilities (usable high tok/s and zero-touch one-command cluster setup).

## [2.1.0] - 2026-08-18

### Phase 6: Polish and Consistency Pass (Single Source of Truth, Port Alignment & Command Palette)

- **Launcher Output & Health Recovery**: Updated `launch.sh` and `launch-native.ps1` to print `Open Studio at http://127.0.0.1:5173`. Improved `/api/health` error diagnosis to distinguish 401 Unauthorized from process connectivity failures.
- **Unified Port & Service Documentation**: Standardized the 3-step happy path across `README.md`, `docs/QUICKSTART.md`, `docs/TROUBLESHOOTING.md`, `ghostlink_gui_modern/README.md`, `ghostlink_gui_modern/GUI_README.md`, `MIGRATION.md`, `docs/API_REFERENCE.md`, `.env.example`, `ghostlink.example.toml`, and Docker Compose manifests. Explicitly labeled port `:8003` as internal API and `:8080` as llama-server. Eliminated stale references to port 3000.
- **Expanded Command Palette & Hotkeys**: Added searchable command entries in `CommandPalette.tsx` for all Phase 1–5 primary actions (Health retry, API key, Load/Download/Unload model, New Chat, Thread search, Prompt presets, Discover LAN peers, Use other machines, Enable Calculator/MCP, Index workspace, Toggle workspace context). Verified shortcut badges match active keydown listeners.

### Phase 4: Ghostlink Cluster Map & Human-Readable Placement Plan

- **Cluster Map Topology & Peer Evaluation Endpoint** (`crates/ghost-link/src/rpc_cluster.rs`, `crates/ghost-link/src/main.rs`, `ghostlink_gui_modern/src/api.ts`):
  Enhanced `/api/cluster/topology` and `/api/workers` payloads with detailed peer evaluations (`rpc_port`, `contribute_compute`, `build_id_status`, `secret_status`, `allowlist_status`, `role`, and `excluded_reason`). Discovered peers report exact non-silent exclusion reasons (`RPC build does not match coordinator`, `rpc_shared_secret missing or handshake mismatch`, `peer IP not in coordinator rpc_allowed_peers`, `contribute_compute off`, `unhealthy / stale heartbeat`).
- **Human-Readable Placement Plan Banner** (`crates/ghost-link/src/main.rs`, `ghostlink_gui_modern/src/components/WorkersTab.tsx`):
  Integrated dynamic placement plan evaluation returning plain English explanations (e.g. `Model llama3:30b with distributed on: split 0.60 on rig-a (12 GB), 0.40 on laptop-b (8 GB)`), node tensor split weights, and active `--rpc` target addresses.
- **Named Master Control & Advanced Contributor Settings** (`ghostlink_gui_modern/src/components/WorkersTab.tsx`):
  Replaced TOML configuration with a single human-worded named control (*"Use other machines when this model does not fit"* for `distributed_inference`) and an Advanced drawer for local compute contribution (`contribute_compute`), advertised `rpc_port`, write-only `rpc_shared_secret` set/clear, and `rpc_allowed_peers` IP allowlist.
- **LAN Security Confirmation & Join Recipe** (`ghostlink_gui_modern/src/components/WorkersTab.tsx`):
  Added a LAN-trust confirmation warning dialog when enabling compute contribution and a short step-by-step *"How to join"* recipe modal for empty cluster states.
- **Chat Attribution Hook** (`ghostlink_gui_modern/src/components/ChatTab.tsx`):
  Wired placement plan data into Chat header hardware attribution displaying `Split across rig-a, laptop-b` when distributed inference is active across multiple LAN nodes.

### Phase 3: Ghostlink Studio Chat Enhancements (Multi-turn, Stoppable Streaming, Thread Sidebar, Presets & Hardware Attribution)

- **Backend Multi-Turn Prompt Reconstruction for `/v1/chat/completions`** (`crates/ghost-link/src/main.rs`, `docs/API_REFERENCE.md`, `docs/openapi.yaml`): Reconstructed prompts from the full `req.messages` array in sequence role order (`system: ...`, `user: ...`, `assistant: ...`) rather than dropping earlier turns. Added a Rust unit test verifying multi-turn prompt concatenation and updated documentation.
- **Thread Management Sidebar & Persistence** (`ghostlink_gui_modern/src/store.ts`, `ghostlink_gui_modern/src/components/ChatTab.tsx`): Built a collapsible thread list sidebar with live search, New Chat shortcut (`Ctrl+Shift+O` / `⌘ShiftO`), inline thread title renaming, deletion with confirmation modal, and pinning. Thread state is persisted locally and synchronized with `SessionsTab` to prevent state drift.
- **Stoppable Token Streaming & Session Cancellation** (`ghostlink_gui_modern/src/api.ts`, `ghostlink_gui_modern/src/components/ChatTab.tsx`): Token-by-token rendering with a Stop control button and `Escape` keyboard hotkey that aborts the client fetch stream, issues `POST /api/sessions/{id}/cancel` to the backend, and preserves generated tokens up to that point for immediate regeneration.
- **In-place Turn Editing & Assistant Regeneration** (`ghostlink_gui_modern/src/components/ChatTab.tsx`): Added inline user message editing with history truncation from that point, and an assistant "Regenerate" control to re-trigger generation on any prior assistant response.
- **Mid-Thread Model Switch Dividers** (`ghostlink_gui_modern/src/components/ChatTab.tsx`): Switching models mid-thread retains complete conversation history while inserting visual divider messages (`Switched model to <model>`) into the transcript.
- **Per-Thread Knobs & System Prompt Presets** (`ghostlink_gui_modern/src/store.ts`, `ghostlink_gui_modern/src/components/ChatTab.tsx`): Added built-in system prompt presets (*Default Co-pilot*, *Concise*, *Code Expert*, plus user-created custom presets) and per-thread knobs (model, temperature, max tokens, top-p, repeat penalty) isolated from global application Settings.
- **Context Meter, Real-time tok/s & Hardware Attribution** (`ghostlink_gui_modern/src/components/ChatTab.tsx`): Added a header context meter (`Estimated: X / Y tokens`), live stream `tok/s` calculation, single-node (`Local`) or multi-node cluster hardware attribution, and engine capability status badges.
- **Prompt Template Library & Slash Command** (`ghostlink_gui_modern/src/store.ts`, `ghostlink_gui_modern/src/components/ChatTab.tsx`): Added a local prompt template library accessible via composer button or `/` slash command shortcut to insert pre-made prompt templates directly into the composer.


### Phase 2: Ghostlink Studio Model Management (GGUF, Fit Badges, Quant Picker & Cancellation)

- **Hugging Face GGUF Repo & File Inspection Endpoint** (`crates/ghost-link/src/main.rs`, `docs/API_REFERENCE.md`): Added `GET /api/models/huggingface/repo` (`handle_gui_models_hf_repo_details`) to fetch GGUF sibling files, quants, and exact byte sizes for Hub repositories. Updated HF search to filter out non-chat/encoder models while exposing `hidden_non_chat_count`.
- **Download Cancellation & Range Resume** (`crates/ghost-link/src/main.rs`, `ghostlink_gui_modern/src/api.ts`): Added `POST /api/models/download/cancel` (`handle_gui_model_download_cancel`) with active cancellation tokens checked during chunk streaming, cleaning up incomplete artifacts. Surfaced HTTP Range resume support on retries.
- **Hardware Fit Badge Calculator** (`ghostlink_gui_modern/src/utils/fitBadge.ts`): Implemented deterministic Memory & VRAM fit badge classification (*Fits this machine*, *Tight on this machine*, *Needs cluster*, *Likely too big for this machine*, *Will not load*, *Unknown*) derived from probed runtime RAM/VRAM and aggregate cluster worker capacity with detailed tooltip math breakdowns.
- **Multi-Quant Selection & Recommendation Engine** (`ghostlink_gui_modern/src/components/ModelsTab.tsx`): Multi-quant Hub repositories now expose a quant picker with automated recommendation (preferring default Q4_K_M or best local fit, followed by cluster fit) targeting individual GGUFs while preserving native filenames.
- **Load UX, Inline Tuning & OOM Suggestions** (`ghostlink_gui_modern/src/components/ModelsTab.tsx`): Added one-click Load CTA, inline native performance tuning controls (GPU offload `-ngl`, context `-c`, threads `-t`, flash attention) persisted per model in `localStorage`, clean Ollama/vLLM backend handling, and OOM error recovery with smaller quant suggestions.
- **Command Palette Action Shortcuts** (`ghostlink_gui_modern/src/components/CommandPalette.tsx`): Wired Download models, Load model, and Unload model command palette actions.

### Performance

- **Low-allocation SSE chat streaming & NativeEngineClient connection pooling** (`crates/ghost-link/src/native_engine.rs`, `crates/ghost-link/src/main.rs`): Replaced generic `serde_json::Value` dynamic map deserialization on incoming SSE token chunks with strongly typed `LlamaStreamChunk` structs in `generate_chat_stream`, eliminating per-token heap map allocations. Added `pool_max_idle_per_host(10)` and `tcp_keepalive(15s)` to `NativeEngineClient::new` to minimize TCP handshake overhead on native streaming completion requests, and streamlined outgoing SSE formatting in `main.rs` by avoiding temporary JSON map allocations per token.
- **Eliminated synthetic flow pipeline execution on hot chat completion paths** (`crates/ghost-link/src/main.rs`): Removed redundant `detect_runtime_profile("studio-api")`, layer assignment, and socket-backed `execute_pipeline_tcp_loopback` execution from `handle_chat_completions` and `handle_completions`. Requests previously executed a synthetic pipeline benchmark before dispatching to the inference engine, creating unnecessary CPU contention and latency overhead.
- **Real native SSE token streaming for chat requests** (`crates/ghost-link/src/main.rs`, `crates/ghost-link/src/native_engine.rs`): Updated `generate_chat_stream` in `NativeEngineClient` to pass conversation history and `response_format` to llama-server, and wired real incremental SSE token streaming in `handle_gui_chat` when `stream=true` (and no tool calls are pending). Delivers tokens to clients incrementally as llama-server produces them, substantially improving Time-To-First-Token (TTFT).
- **HTTP client connection pooling & TCP keep-alive for Ollama and vLLM backends** (`crates/ghost-link/src/ollama.rs`, `crates/ghost-link/src/vllm.rs`): Configured `OllamaClient` and `VllmClient` `reqwest::Client` instances with `pool_max_idle_per_host(10)` and `tcp_keepalive(15s)`, eliminating per-request TCP connection setup overhead for secondary backends.

### Fixed

- **GUI showed no active model/empty models list whenever `enable_tls` is on** (`launch.sh`, `ghostlink_gui_modern/vite.config.ts`): the GUI's `axios` client (`api.ts`) builds absolute request URLs from `VITE_GHOSTLINK_API_BASE`, which `launch.sh` pinned straight at `ghost-link`'s own `https://…:8003` — so every request left the page as a real cross-origin HTTPS fetch. A browser tab has no equivalent of `curl -k` or a server-side proxy's `InsecureSkipVerify`; it cannot be told from application code to trust `ghost-link`'s self-signed loopback cert, so every one of those requests failed outright with `ERR_CERT_AUTHORITY_INVALID` — with nothing surfaced anywhere but the browser devtools network tab. `launch.sh` now routes the GUI through the control-plane gateway (see below) instead, which already talks to `ghost-link` over TLS server-side (skipping verification there, where that's actually safe) and exposes plain HTTP to the browser. Falls back to the old direct-to-`ghost-link` URL only if the gateway didn't start. Vite's own dev-server proxy blocks (`/api`, `/health`, `/v1`) also gained `secure: false`, since server-side proxying to the same self-signed cert was failing the same way.
- **`launch.sh`'s own `/api/health` readiness check always timed out, misreporting a healthy server as "wrong process on port"**: every route but the exact path `/health` requires a bearer token (`auth_middleware`), including `/api/health` — but the script only loaded the persisted API key *after* that specific check already ran, so it 401'd on every attempt for the full 30s timeout (`curl -f` swallows the 401 body, making it indistinguishable from "not up yet"). Reordered to load the key first and thread it through `wait_for_http` (now takes an optional bearer-token argument).
- **`GET /api/workers` never reflected auto-discovered peers** (`crates/ghost-link/src/main.rs`): manually-added workers (`POST /api/workers/add`) and peers found via UDP broadcast discovery (`POST /api/workers/discover`, plus the periodic background broadcast `serve` already ran) lived in two disconnected stores — a plain `Vec<WorkerRecord>` vs. the `ClusterState` topology graph. A successful discovery round updated the topology view but the Workers panel's own list never changed. `GET /api/workers` now merges in cluster-registered peers (deduped by id, self excluded, tagged `status: "Discovered"`) so the list reflects real network state instead of only ever showing what was typed into the "Add Worker" form.

### Added

- **First-run health and recovery panel in Ghostlink Studio** (`ghostlink_gui_modern/src/components/HealthPanel.tsx`, `ghostlink_gui_modern/src/api.ts`, `ghostlink_gui_modern/src/store.ts`): Replaced generic "Connection Error" with typed health probes and actionable recovery paths across control-plane gateway (:8000), internal ghost-link API (:8003), authentication state (HTTP 401), inference backend, and loaded model status. Includes inline API key input field for 401 recovery, direct CTA buttons ("Go to Models Tab", "Switch API Base"), and Command Palette integration.

### Fixed

- **Port alignment & default API base URL in Studio GUI** (`ghostlink_gui_modern/src/config.ts`, `docs/TROUBLESHOOTING.md`, `docs/QUICKSTART.md`, `README.md`, `launch.sh`): Set default GUI API base URL to control-plane gateway `http://127.0.0.1:8000` (Go proxy) instead of internal ghost-link `:8003` directly, preventing 405/CORS errors and broken first-run connections. Updated launchers to print `Open Studio at http://127.0.0.1:5173` upon green health verification.

- **Control-plane gateway wired into `launch.sh`**: the Go reverse-proxy gateway (`control-plane/`) previously had no launch-time integration at all — nothing built or started it. `launch.sh` now resolves/builds it if missing (`go build`, skipped with a warning if no Go toolchain is present — this is additive, not a hard requirement), starts and health-checks it alongside `ghost-link` and Vite, and stops it on cleanup. Reachable at `GHOSTLINK_CONTROL_PLANE_PORT` (default `8000`).
- **`launch.sh` splash/success banners** now credit Sovereign Mohawk Proto LLC and carry a live version number read from `crates/ghost-link/Cargo.toml` (`ghostlink_version()`), instead of no version indicator at all.

---

## [2.0.0] - 2026-08-17 (RBAC & scoped API keys, RPC peer authentication, durable audit trail, Grafana/Prometheus + OpenTelemetry, GUI accessibility, MCP server editor, chat attachments, model-performance GUI controls)

First General Availability release. The jump from 1.17.0 to 2.0.0 isn't a
breaking-API-change bump in the strict semver sense — every item below is
either opt-in or auto-migrates an existing deployment with no manual step
(see each entry, and the compatibility notes called out explicitly where
they apply). It's a milestone bump: this release replaces the entire
authorization model (a single shared bearer token becomes a scoped,
revocable, role-based multi-key store), adds an authenticated handshake for
RPC peers, and stands up a durable, exportable audit trail plus
Grafana/Prometheus/OpenTelemetry observability — the full set of items this
project's roadmap tracked as its "Enterprise Trust Track" toward a
production-ready security posture. See [SECURITY_MODEL.md](docs/SECURITY_MODEL.md)
for the updated threat model and [SECURITY.md](SECURITY.md) for the
supported-version table.

### Added

- **RBAC with scoped, revocable API keys** (`crates/ghost-link/src/auth.rs`): the single shared bearer token is replaced by a persisted, hashed multi-key store (`api_keys.json`, `ApiKeyRecord` — SHA-256 hash + last-4 preview only, the raw value is never stored). Each key carries a `Role` (`Admin`/`Operator`/`Viewer`); `required_role()` gates every route — GET defaults to `Viewer`, mutating verbs (POST/PUT/DELETE) default to `Operator`, and key management (`GET`/`POST /api/security/keys`, `DELETE /api/security/keys/:id`) plus `/api/security/pqc/enable` are `Admin`-only. `delete_key_from_store` refuses to remove the last remaining Admin key, preventing accidental self-lockout. JWTs now sign with a dedicated `jwt_signing_secret` (persisted to `jwt_secret.txt`, `GHOSTLINK_JWT_SECRET_PATH` override) instead of the raw API key value, and a JWT is only honored while its subject key id is still present in the store — revoking a key immediately invalidates any outstanding JWT for it rather than waiting out its 1-hour lifetime. **Compatibility:** an existing `api_key.txt` migrates automatically on first run into the store as the sole `bootstrap` Admin key — no manual step, and that key's effective access is unchanged. One side effect: JWTs issued by a pre-upgrade server sign with the old (API-key-derived) secret and stop validating once the process restarts on this version — low impact given the 1-hour lifetime, but worth knowing if something caches tokens long-lived (see Troubleshooting below).
- **`rpc_shared_secret` handshake for RPC peer authentication** (`crates/ghost-link/src/rpc_cluster.rs`): closes a gap the existing `rpc_allowed_peers` IP allowlist (1.17.0) couldn't — an allowlisted range doesn't stop a device already inside it, or one able to spoof a source address. Since upstream llama.cpp's `--rpc` client starts sending raw `ggml-rpc` binary protocol the instant it connects (no slot for a custom handshake inside that stream), auth instead happens on a dedicated auth port before the real RPC connection is permitted: the coordinator sends a random nonce, the peer returns `HMAC-SHA256(rpc_shared_secret, nonce)`, and a match grants a time-limited admission for that source IP, which the allowlist proxy now requires in addition to plain IP membership whenever a secret is configured. A fresh nonce per handshake defeats replay. **Off by default** — `rpc_shared_secret` is empty/unset out of the box, and an empty allowlist remains byte-for-byte the old direct-bind behavior; distributing the secret across a cluster's nodes is a manual, opt-in step. Does **not** encrypt the RPC byte stream itself — an honest remaining gap, not something this closes.
- **Durable, append-only audit trail with CEF/JSON export** (`crates/ghost-link/src/audit_log.rs`): the audit log was previously an in-memory ring buffer capped at 500 entries that reset on every restart, with no export path — a blocker for real SIEM integration. Every audit event is now also appended as one JSON line to `audit_log.jsonl` (`GHOSTLINK_AUDIT_LOG_PATH` override), alongside — not instead of — the existing capped in-memory feed the GUI's Security tab reads live. New `GET /api/security/audit-log/export?format=json|cef` reads the full durable history and is gated `Admin`-only (vs. `Viewer` for the live feed), since a bulk historical export is a materially larger exposure than a live tail. CEF export implements real Common Event Format field escaping — several audit detail strings already contain raw `=` characters that would otherwise corrupt the extension-field boundary for a real SIEM parser. **Operator note:** the durable file has no cap, rotation, or retention policy in this release — plan disk monitoring/log rotation on long-running deployments.
- **Grafana dashboard + Prometheus monitoring profile** (`docker-compose.yml`, `deploy/grafana/`, `deploy/prometheus/prometheus.yml`): `docker compose up --profile monitoring` brings up Prometheus scraping the existing `/metrics` endpoint and Grafana pre-provisioned with a dashboard (throughput, p50/p95 latency, CPU/memory/GPU, cluster node count/VRAM, uptime, sample rate) — no manual setup. `prometheus.yml`'s scrape config correctly accounts for `ghostlink-api` always serving HTTPS with a self-signed cert once bound to `0.0.0.0` (`https` + `insecure_skip_verify` + the shared API key, since `/metrics` requires a bearer token like every route but `/health`). **Opt-in:** neither service has a default profile, so a plain `docker compose up` is unaffected. **Set `GRAFANA_ADMIN_PASSWORD`** before running this profile beyond local evaluation — it defaults to `admin` otherwise.
- **Opt-in OpenTelemetry tracing export** (`crates/ghost-link/src/otel.rs`): gated entirely on `GHOSTLINK_OTEL_EXPORTER_ENDPOINT` — unset reproduces the exact plain-text console logging this codebase has always had, zero behavior change. When set, `tower-http`'s `TraceLayer` adds an automatic root span per HTTP request, plus three hand-instrumented phase spans on the distributed-inference path (peer discovery/admission, model load, generation). `GHOSTLINK_OTEL_SERVICE_NAME` overrides the reported service name (default `ghost-link`). Same protocol limit as the RPC-auth handshake above: a trace ends at "launched llama-server, here's how long it took" rather than spanning the actual RPC hop. No bundled trace backend — point the endpoint at whatever collector you already run.
- **Standalone MCP servers' tools made reachable, opt-in per Tools-panel checkbox** (`crates/ghost-link/src/mcp/registry.rs`, `mcp_servers.example.toml`): `sequential-thinking`, `docker-mcp-gateway`, `git`, `vision`, and `rag` connected successfully at startup but were never actually usable in chat — the tool loop only built its list from checkbox-bound slots, and standalone servers had no checkbox. All five now have real slots so their tools are opt-in via the Tools panel instead of riding along on every request by default (see Changed below).
- **MCP server editor** (`McpTab.tsx`, `crates/ghost-link/src/mcp/{config,registry}.rs`): add/edit/delete MCP servers (stdio or HTTP transport) directly from the GUI instead of hand-editing `mcp_servers.toml`. Changes take effect immediately — connects/disconnects the affected server live via new `McpConfigManager::add/update/remove` + `McpRegistry::add_server/update_server/remove_server`, no restart required. Also wired up the enable/disable toggle switch, which was calling `POST /api/mcp/servers/:name/toggle` — a route that didn't exist; the connect/disconnect logic (`McpRegistry::set_enabled`) was already fully implemented and tested but had no caller. (`McpTab.tsx`, `crates/ghost-link/src/mcp/{config,registry}.rs`): add/edit/delete MCP servers (stdio or HTTP transport) directly from the GUI instead of hand-editing `mcp_servers.toml`. Changes take effect immediately — connects/disconnects the affected server live via new `McpConfigManager::add/update/remove` + `McpRegistry::add_server/update_server/remove_server`, no restart required. Also wired up the enable/disable toggle switch, which was calling `POST /api/mcp/servers/:name/toggle` — a route that didn't exist; the connect/disconnect logic (`McpRegistry::set_enabled`) was already fully implemented and tested but had no caller.
- **Text-file attachments in chat** (`ChatTab.tsx`): paperclip button + drag-and-drop onto the composer. Scoped to text-based files (code, markdown, JSON, CSV, logs, config — no images/binaries, which are rejected with an explicit toast rather than silently doing nothing) since there's no multimodal/upload endpoint; files are read client-side (256KB/file cap) and inlined as labeled fenced code blocks ahead of the message.
- **`eslint-plugin-jsx-a11y` + axe-core Playwright suite** (`ghostlink_gui_modern/eslint.config.js`, `e2e/accessibility.spec.ts`): first automated accessibility regression gate for the GUI — scans the app shell and every primary tab (plus the new MCP add-server dialog) for WCAG 2 A/AA violations. `color-contrast` is deliberately excluded for now (see Documentation below).
- **"Model Performance" GUI settings section** (`SettingsTab.tsx`, `store.ts`): GPU Offload, Context Length, CPU Threads, Batch Size, Micro-batch Size, KV Cache Type (Auto/F16/Q8_0/Q4_0), Flash Attention, and 3-state (Auto/On/Off) mlock/no-mmap controls — modeled on LM Studio's load-settings taxonomy (the closest real reference; Nous's Hermes desktop app largely delegates these to whichever backend it's pointed at rather than exposing its own). Hidden with a redirect note when the selected backend isn't native llama-server (Ollama/vLLM manage their own loading). `ngl`/`ctx_size`/`threads` already existed in `RuntimeSettings` but were dead — editing them via the API silently did nothing, since nothing ever translated them into the `GHOSTLINK_*` env vars `native_engine.rs` reads. Wiring them up required a real design decision: naively activating whatever value happened to already be stored in an old `settings.json` would resurrect the exact large-model OOM this repo already hit once (see the `ngl`/`ctx_size` entries below). Solution: `ngl_auto`/`ctx_size_auto`/`threads_auto` bool flags, decoupled from the numeric fields and defaulting to `true` via `#[serde(default = "default_true")]` — a key serde has never seen is always absent from a pre-existing file, so every old `settings.json` comes back "auto" (today's real behavior) regardless of whatever number is sitting in the field. New fields (`batch_size`, `ubatch_size`, `kv_cache_type`, `mlock`, `no_mmap`) use plain `Option<T>` instead, since they have no legacy stored value to worry about misinterpreting. The settings-to-env-var translation (`apply_native_engine_tuning_env`, `main.rs`) deliberately **only ever sets an env var, never clears one** — even when a field is back at "auto" — because this process's env may already carry a deliberate, permanent override from a launch script (e.g. `launch-native.ps1`'s reference-machine `GHOSTLINK_LLAMA_NGL=-1` pin); "auto" here means "don't introduce a new override," not "force detection no matter what already exists." Regression-tested directly: a synthetic `RuntimeSettings::default()` must not introduce a single new env var, and a pre-set env var must survive an all-auto settings object untouched.
- **Per-field `-b`/`-ub`/KV-cache-type/Flash-Attention overrides in `native_engine.rs`** (`GHOSTLINK_LLAMA_BATCH`, `GHOSTLINK_LLAMA_UBATCH`, `GHOSTLINK_LLAMA_KV_CACHE_TYPE`, `GHOSTLINK_LLAMA_FLASH_ATTN`): previously only the monolithic `GHOSTLINK_LLAMA_SERVER_ARGS` raw-string override could touch these, too coarse for the new GUI controls to compose against individually. Flash-Attention-off correctly skips `-ctk`/`-ctv` entirely rather than passing them without the flag llama.cpp requires for quantized KV cache.
- **`--mlock`/`--no-mmap` support in `native_engine.rs`**: previously only the shell launch scripts (`launch.sh`) could set these, so any model load that went through the Rust engine directly (GUI, API, `launch-native.ps1`) never got them. `get_mlock()` mirrors `launch.sh`'s existing RAM-tier heuristic (on only when `GHOSTLINK_SYSTEM_MEMORY_GB>=24`, off otherwise — deliberately not falling back to live OS detection, since that path is 30s-cached process-wide and mlock is risky enough that guessing beats a stale answer); `--no-mmap` is opt-in only (`GHOSTLINK_NO_MMAP=1`), no default heuristic, since nothing in this repo has measured a throughput case for disabling mmap by default.
- **Hybrid CPU (P/E-core) detection** (`system_profile.rs`, Windows only, via `GetLogicalProcessorInformationEx`): new `CpuInfo.performance_cores`/`efficiency_cores` fields. Verified live on this repo's own dev machine (AMD Ryzen AI 7 350): `physical_cores=8` splits as `performance_cores=Some(4)`/`efficiency_cores=Some(4)` — AMD's "Strix Point" mobile chips genuinely mix full Zen5 and compact Zen5c cores, this isn't Intel-only. Detection only, not yet wired into `native_engine.rs::get_threads()`'s default: Intel's P/E split has a large, well-documented per-thread speed gap where capping threads to P-cores is a known win; AMD's Zen5c is the same microarchitecture at a lower clock, a much smaller gap, and whether capping helps, hurts, or does nothing on this chip hasn't been measured (confirmed by a live 3-way `-t 15`/`-t 8`/`-t 4` A/B test on the reference machine — no measurable decode-throughput difference at full GPU offload, since compute is already GPU-bound there).

### Fixed

- **Model-switch race could return "connection refused" to an in-flight chat request** (`crates/ghost-link/src/main.rs`): `load_model_into_slot` kills the old `llama-server` and stages/warms the new one before rebinding the real port; a chat request landing in that window previously hit a dead port instead of ever reaching a model. `model_lifecycle_lock` is now a real `RwLock` — a model load takes a write lock across the whole swap, chat requests take a bounded read lock before dispatching, so requests queue behind an in-progress swap instead of racing it.
- **Circuit-breaker half-open probe race could admit two concurrent probes** (`crates/ghostlink-core/src/circuit_breaker.rs`): `should_attempt` claimed the in-flight-probe flag *after* flipping state Open→HalfOpen, leaving a window where a second thread could observe HalfOpen with the flag still false and also win a probe slot — defeating the breaker's "exactly one concurrent probe" half-open contract. Reordered to claim the flag before the state transition, rolling it back if the transition loses the race.
- **`docker compose config`/`up` broken repo-wide by a malformed `mcp-gateway` stub** (`docker-compose.override.yml`): the `mcp-gateway:` entry had only comments under it, which Compose parses as `null` instead of a mapping. No `mcp-gateway` service exists anywhere else in the compose files for this stub to override, so it's removed outright.
- **`ghostlink-api` healthchecks and control-plane's backend URL used plain HTTP against an HTTPS-only listener** (`docker-compose.yml`, `docker-compose.production.yml`, `docker-compose.launch.yml`): `ghostlink-api` binds `0.0.0.0`, which unconditionally forces TLS regardless of the `enable_tls` setting — so the container always served a self-signed HTTPS cert while every healthcheck curled `http://`, permanently marking it unhealthy and blocking dependents. Worse, `control-plane`'s `GHOSTLINK_BACKEND_URL` also pointed at `http://`, so chat-completion proxying was broken, not just the health probe. Fixed to `https://` with skip-verify (there's no CA for the self-signed cert). **Not a new breaking change** — the prior configuration was already non-functional; this restores intended behavior.
- **Error boundary's details-toggle control had no keyboard-accessible focus state** (`ghostlink_gui_modern/src/components/ErrorBoundary.tsx`): the crash-recovery UI's details toggle had no focus-visible ring, hover transition, or descriptive tooltip. Added, matching the accessibility pattern used elsewhere in the GUI.
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
- **`GHOSTLINK_VRAM_GB` was documented as a `launch.sh` override but silently did nothing**: the script only ever *wrote* that env var (to hand `VRAM_GB` through to the Rust process), never read a pre-set one — so a user following `docs/LOCAL_INFERENCE_TUNING.md`'s documented `export GHOSTLINK_VRAM_GB=N` had it clobbered before either the shell's own `ngl` tiering or the Rust process ever saw it. Now seeded first, so the override actually takes effect.
- **AMD/Intel iGPUs always landed on `ngl=0`/CPU-only on `launch.sh`, even on boxes with plenty of headroom for a small model**: there's no real "VRAM" figure to read for a Vulkan-backend iGPU (it's shared system RAM), so `VRAM_GB` stayed at its unknown/0 default and the ngl tiering always bottomed out, regardless of how much RAM the host actually had. Now estimates a tuning tier — 1/3 of total system RAM, capped at 8GB — when the Vulkan backend is detected and `GHOSTLINK_VRAM_GB` isn't set explicitly. This is a tuning input, not a safety boundary: the actual danger case (a large model on this backend) is already independently guarded by the model-size cap above, so the estimate can afford to be reasonably generous rather than maximally conservative.
- **`rpc_cluster::compute_tensor_split` degenerated to a uniform 0.1/0.1 split for an all-CPU-only distributed cluster with unequal RAM**: every device's weight floored to the same 0.1 whenever no node reported real VRAM, overloading the weaker node regardless of how differently capable the CPU-only nodes actually were. Now falls back to a RAM-proportional split specifically when *no* device in the split (local or peer) reports VRAM — a mixed GPU/CPU cluster still uses the plain VRAM-with-0.1-floor path for its CPU-only members, since comparing RAM directly against another device's VRAM would need an unmeasured conversion factor this repo hasn't benchmarked.
- **`rpc_cluster::discover_rpc_peers` had no real-time health signal, only a heartbeat-timeout status check**: a peer that's technically `Active` (heartbeat still arriving) but badly degraded could still be routed real inference layers. Now also excludes any peer with `delivery_ratio` below `0.90` (matching `planning::RebalanceTrigger`'s existing threshold for that metric). Deliberately does **not** also gate on `avg_latency_us` — that field is populated with millisecond-scale values in some real call sites despite its name, and copying an ambiguously-scaled cutoff risked silently excluding every real peer (or none) depending on which unit actually applied.
- **New "Model Performance" GUI section had no backend-conditional guard**: GPU offload/batch/flash-attention/mlock controls rendered live and editable even when Ollama/vLLM was the selected backend, where they have no effect since the settings only apply to the native llama-server path. Now shows a redirect note instead, matching the existing "Engine Connection" section's pattern for backend-specific content.
- **Sidebar nav buttons (Chat/Editor/Models/Metrics/Sessions/Workers/MCP/Security/Settings) had no accessible name** (`App.tsx`): visible text was present but not exposed to the accessibility tree — reproduced in the live accessibility tree, every button announced as bare "button" with no label. Added explicit `aria-label`/`title` to each, matching the pattern already used for the "Search commands" button in the same file; also added the focus-visible ring styling the rest of the app uses, which these buttons were missing.

### Changed

- **MCP tool availability is now opt-in per server, not automatic for standalone servers** (`mcp_servers.example.toml`): see "Added" above — previously-standalone servers (`sequential-thinking`, `docker-mcp-gateway`, `git`, `vision`) now require an explicit Tools-panel checkbox before their tools are sent to the model on every request. This is a behavior change for anyone relying on those tools being silently available without checking a box; `rag`'s Editor-tab auto-index integration is unaffected since it calls the server directly, not through a slot. As a side effect, a request with nothing checked went from an ~18–80s "say ready" turn to ~2s, since nothing is always-on by default anymore.
- **Accessibility pass across the GUI**: live-region announcements for streaming chat replies and Metrics tab state changes (throttled to meaningful transitions, not every poll tick, to avoid drowning a screen reader); skip-to-content link and landmark wiring; restored focus rings on the Command Palette input and chat composer (both had stripped the default outline with no replacement); `prefers-reduced-motion` support; every scrollable tab body made keyboard-scrollable (`tabindex`/`role="region"`) — a real WCAG 2.1.1 gap the new axe suite caught that wasn't part of the original ask.
- **Chat markdown readability**: assistant replies bumped from `prose-sm` to base `prose` (14px → 16px) outside Compare Mode; heading sizes tamed to fit a chat bubble instead of a full-width article; GFM tables wrap in a scroll container instead of blowing out bubble/page width; inline `` `code` `` swapped from the default backtick-quote style to a background pill, scoped so it can't also shrink code inside fenced blocks; code block font bumped 12px → 13px.
- **`mcp::toolcall::MAX_TOOL_ITERATIONS` raised from 3 to 6**: 3 was cutting off legitimate multi-step agent tasks before they could finish, forcing the "(stopped after N tool round-trips without a final answer)" bailout even on the happy path. Verified live: a filesystem task (list a directory, self-correct from an initially wrong path, list again, read a file, answer) took 4 real tool round-trips — the old cap would have cut it off mid-task; the new cap let it complete correctly. Each round trip is a real inference call, so this doubles the worst-case latency/compute for a turn where the model is genuinely stuck (weakens, doesn't remove, the existing runaway-loop safety net) — a deliberate tradeoff, not an oversight. Shared by all three backend loops (native/Ollama/vLLM read the same constant), so the change is uniform across engines.

### Documentation

- **`docs/SECURITY_MODEL.md`** rewritten to cover RBAC, the RPC peer-auth handshake, the durable audit trail, and OpenTelemetry — it previously described the audit-log endpoint as a stub returning an empty list, which was stale. **`SECURITY.md`**'s supported-version table bumped to the 2.0.0 lineage. **`docs/TROUBLESHOOTING.md`** gained entries for RPC peer version/secret mismatches and post-upgrade JWT invalidation.
- **`docs/ROADMAP.md`** updated with status checks for Enterprise Trust Track items #1–4 (RBAC, RPC peer auth, durable audit trail, Grafana/Prometheus + OpenTelemetry) as each landed.
- **Continuous rebalancing confirmed unwired for real distributed inference**: closes out an "unconfirmed" roadmap item rather than leaving it ambiguous for a GA release. Rebalancing has exactly one real caller — the `ghost-link flow` CLI's synthetic pipeline-benchmark path — not the actual distributed-inference serving path, which has no rebalancing concept at all. Recorded explicitly so the GA changelog and roadmap don't imply continuous rebalancing works for real inference traffic — it doesn't yet.
- **Repo-wide documentation audit for the GA release** — see [docs/RELEASE_AUDIT_v2.0.0.md](docs/RELEASE_AUDIT_v2.0.0.md) for the full writeup: seven stray, gitignored session-summary files were confirmed excluded from version control already; the one tracked stray root doc (`DEVELOPMENT_GUIDELINES.md`, which described a test setup that doesn't exist in this repo) moved to `docs/archive/legacy-root-docs/`. The root `AGENTS.md` — previously a dated development-session log, not repo guidance — replaced with real agent/contributor instructions; the old content preserved at `docs/archive/legacy-root-docs/SESSION_NOTES_2026-07-30.md`. `CONTRIBUTING.md`'s broken `docs/INDEX.md` reference fixed and its duplicated Code of Conduct text replaced with a link to `CODE_OF_CONDUCT.md`. `crates/ghostlink-gui` (an earlier Tauri/Svelte prototype, superseded by `ghostlink_gui_modern` and not wired into the workspace, launchers, or release pipeline) marked `publish = false` and given an explicit deprecation notice in its README, since nothing previously said so.
- Deliberately did **not** fix pervasive `color-contrast` failures the axe suite surfaced (the app's muted `slate-500`/`slate-600`-on-dark text falls short of WCAG AA in dozens of places across every tab) — that's a design-system change affecting the whole app's visual character, not a mechanical accessibility fix, and is out of scope for this change. Excluded from `e2e/accessibility.spec.ts` with a comment explaining why, so the suite still gates real regressions in the meantime.
- Also not addressed here: no user-facing theme/keyboard-shortcut persistence, no MCP config hot-reload for hand-edited `mcp_servers.toml` beyond the enable/disable toggle fixed above.
- **`docs/LOCAL_INFERENCE_TUNING.md`**: new sections documenting (1) the mlock/no-mmap/KV-cache-type/flash-attention/batch/ubatch env vars and GUI controls; (2) the RPC peer health gate and tensor-split RAM fallback; (3) hybrid CPU detection and why it isn't wired into thread defaults yet; (4) a measured finding that raising context length past its auto-tiered default is capped by the **Vulkan device-local GPU heap**, not general system RAM, at full offload on the reference iGPU — confirmed by pulling llama-server's actual `ErrorOutOfDeviceMemory` (normally discarded by `native_engine.rs`'s staging, captured by running the identical failing command directly) at three different KV-cache quantizations, all failing identically, and reproduced even with 9.6GB of general system RAM free at the time. Full GPU offload and a context window bigger than the auto-tiered default are mutually exclusive for a model this size on this hardware class — there's no tuning combination that gets both. `docs/BENCHMARKS.md` updated with a pointer from the existing all-CPU-only tensor-split measurement to the new RAM-proportional fallback behavior.
- **Full context/offload limits matrix**, measured live on the reference machine (AMD Radeon 860M, Qwen3-Coder-30B-A3B-Instruct-Q3_K_L): `-ngl -1` (full offload) context ceiling binary-searched to 4096–4223 (the existing auto-tiered default of 4096 is already essentially at the hardware ceiling, not a conservative guess); `-ngl 24` (partial offload) confirmed to reach at least 32768 ctx (8x) at 10.2 tok/s decode vs. full offload's 16.8 tok/s (~40% slower for 8x the context headroom — a real, usable tradeoff); `-ngl 0` (CPU-only) confirmed to reach the hard-coded 131072 ctx ceiling with 13GB+ RAM to spare; `-b`/`-ub` batch size ceiling at full offload bounded between 4096–6144 (well above the VRAM-tier default of 1024/512). Every failed staging attempt during this characterization (10 total across the two rounds) left the previously-running server untouched and healthy, confirming `load_model_into_slot`'s stage-on-a-scratch-port-then-swap design holds under repeated real failures, not just a single clean success case.

### Validation

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` — all clean across the full workspace at the point of this release: `ghost-link` 223 passed (6 ignored — hardware/network-dependent, not run in this environment) + 1 doc-test, `ghostlink-core` 188 + 7 + 28 + 19 across its lib and three integration suites, `mcp-rag` 13, `mcp-calculator`/`mcp-vision` 0 (no local model inference in either, both delegate to Ollama) — 0 failures workspace-wide, 0 clippy warnings. `cargo run -p ghost-link -- probe my-node --full` re-run after the `system_profile.rs` hardware-detection change — no regressions.
- GUI: `tsc --noEmit` and `eslint .` clean (one pre-existing, unrelated `react-hooks/exhaustive-deps` warning in `App.tsx`, not introduced by this change).
- All of the above verified live against the actual running stack (release `ghost-link` binary + Go control-plane + Vite dev server), not just automated tests: real GUI interaction end-to-end (toggled GPU Offload/mlock in the live Settings tab, saved, confirmed the resulting `llama-server` command line reflected exactly what was set, then confirmed a fresh process restart correctly returns to the auto-tiered baseline); real multi-round MCP tool-calling chat through the live server; real llama-server driver-level errors captured and diagnosed (`ggml_vulkan: ... ErrorOutOfDeviceMemory`) rather than inferred from symptoms; real memory readings (`Get-CimInstance Win32_OperatingSystem`/`Win32_Process`) throughout, not simulated; RBAC verified live end-to-end (fresh boot, created Operator/Viewer keys, confirmed 403s land exactly where the role model says they should, confirmed revoking a key invalidates its outstanding JWTs immediately, confirmed deleting the last Admin key is refused).

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