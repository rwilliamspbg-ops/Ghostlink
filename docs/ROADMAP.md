# Ghostlink: Path to Category Leadership

> [!NOTE]
> **Current truth (Verified: 2026-08-18)**
> - **Proven:** Distributed VRAM/RAM capacity splitting across heterogeneous LAN nodes via `ggml-rpc` (e.g., loading and running a 30B-class MoE model split across nodes that individual machines cannot hold alone).
> - **Unproven / In Progress:** High usable tokens-per-second (tok/s) throughput on network-bound split layers, and a fully automated zero-touch "one-command cluster" setup without manual network/contributor configuration.


This is a competitive strategy document, not a task backlog. It answers one
question: what does Ghostlink need to become the obvious choice instead of
vLLM, Ollama, LM Studio, llama.cpp server, OpenWebUI, or a Kubernetes-based
setup (KServe/Triton) — not "as good as," but categorically better for the
niche it can own outright.

## The category nobody else occupies

| Platform | What it actually is | What it can't do |
| --- | --- | --- |
| vLLM / TensorRT-LLM | Datacenter-grade single/multi-GPU serving | Assumes homogeneous, co-located GPUs; heavy to run standalone; no zero-config LAN clustering |
| Ollama | Single-node local model runner | No real multi-node distribution; no cluster discovery; no auth/rate-limiting story |
| LM Studio | Closed-source desktop GUI | Single machine only; not API-extensible; no plugin system |
| llama.cpp server | Minimal single-binary inference | No orchestration, no discovery, no cluster, no auth |
| OpenWebUI | A chat UI | Not an inference engine — pairs with one of the above |
| KServe / Triton | Enterprise k8s model serving | Needs a cluster, ops staff, and YAML; wildly overkill for a household/small-team LAN |

**The gap**: nobody combines *zero-config discovery of heterogeneous
consumer/prosumer hardware already sitting on a LAN* (gaming GPU + old
laptop + NPU-equipped ultrabook + Mac) with *real distributed inference
across it*, wrapped in a single self-hosted binary with production-grade
auth, rate limiting, and observability. That's the wedge. Every initiative
below either widens that wedge or removes a reason to pick something else
instead.

**North star**: *"Point Ghostlink at every machine on your network. It
figures out what you have and turns it into one inference cluster —
correctly sized, authenticated, observable, and extensible — in under five
minutes, with zero YAML."*

---

## Priority Zero: close the gap between the pitch and the product

**STATUS: SHIPPED.** `/v1/chat/completions` and `/v1/completions` now
genuinely execute across multiple nodes when available — verified live with
two real `ghost-link` processes, real peer discovery, and a real model
loaded with layers split across both. See "What actually shipped" below for
what was built and why it differs from the original plan in this section.

This isn't a new feature — it's fixing the single biggest thing standing
between "Ghostlink" and "Ghostlink over the top of competitors." Do this
before anything else on this list; it changes what every later initiative is
worth.

### The real vs. the marketed architecture (as of the original write-up)

Today, two systems exist side by side and **don't talk to each other**:

- **The distributed engine is real**: `planning.rs`, `load_balance.rs`,
  `runtime.rs` (TCP/Unix pipeline bridging), `cluster.rs`, `health.rs`, and
  the `ghost-link flow` / `ghost-link stage-worker` CLI commands do genuine
  cross-process, cross-node pipeline execution with real layer placement
  based on detected VRAM/compute.
- **The API server is not distributed**: `POST /v1/chat/completions` (the
  endpoint every real client — the SDK, curl, an app — actually hits) reads
  `BackendState.inference_backend` and calls either the local native
  llama.cpp engine or a local Ollama instance. It never touches `cluster`,
  `planning`, or `runtime`'s pipeline execution. `/api/workers/discover`
  finds peers and registers them in `ClusterState`, but nothing downstream
  of a chat request ever asks that cluster for a placement plan.

So today, a user who sets up three machines gets: peer discovery that
works, a pretty dashboard showing three nodes, and a chat API that only
ever uses whichever single machine is running the server process. That's
not a distributed inference fabric with a demo CLI bolted on — it's a
single-node inference server with a distributed inference *demo* bolted on.
Closing this gap is worth more than every other item on this roadmap
combined, because it's the difference between the product being what it
claims to be and not.

### The fix

1. **Wire `handle_chat_completions`/`handle_completions` to ask `cluster` +
   `planning` for a placement plan before falling to the single-node
   Native/Ollama path** — when `ClusterState` has more than one healthy
   node and the model/VRAM math says it should shard, route through
   `runtime`'s pipeline execution instead of the local backend. Single-node
   deployments keep today's exact behavior (this is additive, same
   discipline as the plugin-registry check added this sprint).
2. **Make model sharding automatic, not a CLI flag.** `ghost-link flow`
   takes `local_id`/`remote_id`/`remote_vram_gb` as arguments today; the API
   path needs to derive an equivalent plan itself from `ClusterState`'s live
   node list plus the model's declared size, with zero user input beyond
   "here's a model, here are the nodes I found."
3. **Prove it, publicly.** A benchmark showing a 30B+ model running at
   usable tokens/sec split across e.g. a 12GB card + an 8GB card — something
   neither of those cards could serve alone — is the single most convincing
   asset this project could publish. Ollama and LM Studio structurally
   cannot do this. vLLM can, but not with zero-config discovery on
   heterogeneous consumer hardware.

**Everything else in this document assumes this gets fixed.** A faster
Prometheus endpoint or a nicer plugin system on top of a secretly-single-node
server is polish on the wrong product.

### What actually shipped (and why it's a different — better — fix)

The plan above assumed `ghostlink_core::runtime`'s pipeline execution
(`ghost-link flow`, `stage-worker`) just needed wiring to the API. It didn't:
that path moves synthetic `f32` benchmark payloads through the ring
buffer/TCP bridge to prove out transport latency — it never ran real model
layers, on any hardware, ever. Verified directly before writing any
integration code: `execute_pipeline_with_remote_stage` sends
`(batch_idx * 0.01) + (idx * 0.0001)` as its "work," not a GGUF tensor.
Wiring the chat API to that would have shipped something that *looked* like
distributed inference in a demo and did nothing real.

The actual fix uses **llama.cpp's own RPC backend** (`ggml-rpc`) instead —
already vendored in `third_party/llama.cpp` but not compiled into the
shipped binaries (`GGML_RPC` was off). This is real, upstream, production
tensor/pipeline-parallel execution:

- A node opts in to contributing compute (`contribute_compute` +
  `rpc_port` in settings) and runs `ggml-rpc-server`, exposing its GPU/CPU
  as a device over TCP (`crates/ghost-link/src/rpc_cluster.rs`).
- The node serving a request (`distributed_inference: true`) discovers
  healthy RPC-contributing peers from live `ClusterState`
  (`rpc_cluster::discover_rpc_peers`), computes a VRAM-proportional
  `--tensor-split`, and launches its local `llama-server` with
  `--rpc host:port,... -ts a,b,...` (`native_engine.rs`). llama.cpp's own
  backend scheduler does the real cross-process tensor execution — nothing
  in Ghostlink simulates or approximates it.
- `NodeResources`'s wire format (UDP discovery + mDNS TXT records) carries
  each node's `rpc_port` so this is genuinely zero-config: point two
  Ghostlink instances at each other, no manual `--rpc` flag from a human.

**Verification, in order of what it actually proves:**
1. Raw binaries first, no Rust orchestration involved: `ggml-rpc-server`
   bound to a second process, `llama-server -ts 0,1` forcing **100%** of a
   model's layers onto that remote device. Real coherent generated text
   came back, and the RPC server's log showed real connection/tensor
   traffic against its real GPU device. This isolated "does llama.cpp's RPC
   backend actually work on this hardware" from "did Ghostlink wire it up
   correctly," and answered the first question independently.
2. Two full `ghost-link serve` processes, real UDP discovery between them,
   `distributed_inference` toggled via the live `/api/settings` API (not a
   config file hand-edit), a model loaded with **zero manual RPC flags** —
   Ghostlink's own logs showed it discovering the peer and building
   `--rpc 127.0.0.1:50054 -ts 3.9990,3.9990` itself. Real chat completions
   came back through both `/completion` and `/v1/chat/completions` with
   `real_inference: true`.

**A real, independent bug found and fixed along the way**: every
`ghost-link serve` instance previously hardcoded its cluster node id to the
literal string `"studio-api"`, regardless of machine (`main.rs`,
`detect_runtime_profile("studio-api")`). Two *real* Ghostlink installs on
two *real* machines would have collided in `ClusterState`'s
id-keyed map exactly like the two same-machine test processes in step 2
above did before the fix — meaning **no distributed feature, old or new,
UDP or mDNS, ever worked across genuinely separate hardware**, independent
of anything in this section. Fixed by deriving the id from the hostname
(`GHOSTLINK_NODE_ID` env var to override). This was pre-existing and would
have silently undermined every multi-node claim in this document, not just
Priority Zero — worth knowing if any earlier multi-node testing "worked" in
a way that didn't actually exercise cross-machine discovery.

A second, narrower bug surfaced during live testing of the fix above:
`DiscoveryFrame::encode()` — the function UDP discovery actually calls — is
a separate, hand-duplicated serializer from `NodeResources::encode_payload_into`
(kept for a zero-copy calling convention), and it silently dropped the new
`rpc_port` field entirely. mDNS discovery (which reuses the shared encoder)
carried `rpc_port` correctly the whole time; UDP discovery didn't, and
because UDP is tried first and wins ties in `/api/workers/discover`'s merge
logic, its `None` silently shadowed mDNS's correct value. Both encoders now
carry the field; a regression test locks in `DiscoveryFrame::encode()`
specifically, not just the shared helper, since that's the gap that let
this slip past unit tests for the shared helper alone.

**Not yet done, deliberately deferred**: automatic *decision-making* about
when to shard (today it's an explicit `distributed_inference` toggle, not
"the API automatically decides a model won't fit locally and shards it") —
that's a reasonable, smaller Horizon-1 follow-up now that the underlying
mechanism is real and proven, not a research problem like the original
plan's approach would have been.

---

## Horizon 1 (0–3 months): Harden what's already differentiated

Ship things that are cheap relative to their credibility payoff, and that
make the Priority Zero fix land on solid ground.

1. **End-to-end distributed inference test in CI.** A multi-process
   integration test that starts two `ghost-link` instances, has one
   discover the other (UDP or mDNS), and asserts a chat completion actually
   executed across both — not just that `cluster.node_count() == 2`. This
   is the regression gate that makes Priority Zero durable.
2. **Reproducible, published benchmarks** (`benches/`, `benchmark-throughput.sh`
   already exist) comparing: Ollama single-node vs. Ghostlink single-node
   (should be roughly at parity — same llama.cpp core), and Ghostlink
   single-node vs. Ghostlink multi-node on a model too big for one node
   alone. Publish hardware specs, model, prompt set, concurrency — exactly
   as CONTRIBUTING.md's release rubric already asks for. **Status check
   (2026-08-08, updated): the real-path benchmark now exists, and it found
   something more important than a throughput number.** `BENCHMARKS.md`'s
   2026-08-03 entry predates the ggml-rpc work above and only exercises the
   old `stage-worker`/`flow` synthetic timing harness — that gap is now
   closed. A new 2026-08-08 "Real ggml-rpc distributed-inference run" entry
   (`docker-compose.rpc-fabric-benchmark.yml` + `scripts/rpc_fabric_benchmark.py`,
   reusing the same fabric as Horizon 1 item 1's CI gate) drives the actual
   `distributed_inference: true` / `rpc_cluster` path with a real 1.5B model
   (Qwen2.5-1.5B-Instruct-Q4_K_M) and real, checked evidence
   (`real_inference: true` plus contributor RPC-log connection counts), not
   just a settings flag.

   **What it found**: with `-ngl 0` — the CPU-safety default this Docker
   fabric forces everywhere, including in the CI gate — `--rpc`/`-ts` are
   real flags producing a real RPC connection (462 genuine "Accepted client
   connection" log lines over 5 runs) that carries **zero compute**:
   throughput (32.70 vs 32.53 tok/s) and memory were statistically
   identical single-node vs. distributed, because `-ngl 0` assigns zero
   transformer layers to any non-CPU-primary backend, GPU or RPC alike. A
   control run with real layer offload (`-ngl -1`) confirmed the mechanism
   works — the coordinator's real committed memory dropped ~741MB while the
   contributor's grew by almost exactly that — but also that, on **one
   physical host with two containers sharing its CPU**, real distributed
   compute made throughput ~48% *worse* (32.53 → 17.01 tok/s), since
   splitting adds real RPC round-trip cost without adding real hardware.

   **Net effect on this roadmap item**: the "prove it, publicly" mechanism
   now exists, is reproducible (`docs/BENCHMARKS.md`'s entry has full
   repro commands), and is honestly documented rather than oversold — but
   it does not yet deliver the "model too large for one node, running at
   usable speed, split across ≥2 heterogeneous nodes" proof this item and
   the roadmap's "How we'll know it's working" section actually call for.
   That specifically requires genuinely separate hardware (like the
   2026-08-03 LAN entry's real second machine, just exercising the real
   `ggml-rpc` path instead of the synthetic one) — a single Docker host
   cannot produce it, no matter how the benchmark is tuned. **Remaining
   work, now much better scoped**: run this same fabric's benchmark script
   against two real separate machines with `-ngl -1` (or a real GPU split),
   on a model that's actually too large for either machine alone. A second,
   equally real finding worth carrying into item 3 below (automatic
   sharding) and any future default-tuning work: a CPU-safe `-ngl 0`
   default silently makes `distributed_inference: true` a no-op rather than
   an error or a warning — worth surfacing to the user explicitly, not just
   documenting in a benchmark footnote.

   **Status check (2026-08-08, second update — the real separate-hardware
   test now exists.)** Two real machines, native processes (not Docker),
   `ghost-link` on each — see `BENCHMARKS.md`'s "native processes, two
   genuinely separate physical machines" entry for the full account. The
   real-hardware result: single-node 55.11 tok/s avg vs. distributed 53.57
   tok/s avg on the 1.5B model — a small real cost (~2.8%), not the
   hoped-for benefit, because `compute_tensor_split`'s VRAM-only
   proportionality floors any non-GPU contributor (Iprada declared 0GB
   VRAM) to a token ~2.4% share regardless of its real CPU/RAM capacity.
   Attempting to force a genuine capacity benefit with a bigger model (7B,
   then a second 5GB model) did **not** produce the "too large for one
   node, works when split" proof either — instead it surfaced four real,
   distinct, previously-unknown bugs (a quantized-KV-cache crash on the
   RPC/CPU backend, an unsupervised contributor child process, a
   90-second readiness timeout too short for real cross-machine RPC loads,
   and — the serious one — silent output corruption from unversioned
   `ggml-rpc` peers, confirmed via a controlled test). See the Risks
   section below and `BENCHMARKS.md` for full detail on each. **Net
   effect**: this item is now blocked less by "we haven't tested real
   hardware" and more by "real hardware testing found real bugs that
   should be fixed before the compelling benchmark is published" —
   publishing a flattering number before the version-mismatch corruption
   bug is addressed would be actively misleading given how easy that
   failure mode is to hit unknowingly (any two machines that haven't been
   deliberately kept in version lockstep).

   **Status check (2026-08-08, third update — the actual proof landed.)**
   With `llama.cpp` versions matched (closing the corruption bug above),
   the same two-machine pairing loaded and correctly served a real
   30B-class model (`Qwen3-Coder-30B-A3B-Instruct-Q3_K_L`, 13.58 GiB) that
   **genuinely cannot load on the coordinator alone** — confirmed via a
   real `ErrorOutOfDeviceMemory` single-node failure, not a timeout or a
   contrived constraint. The distributed path found and worked around a
   real driver-level bug along the way (Vulkan/Mesa on the contributor's
   iGPU not reclaiming freed device memory across repeated large
   alloc→free cycles, diagnosed down to the exact command sequence via
   upstream llama.cpp's own `GGML_RPC_DEBUG` logging — see
   `BENCHMARKS.md`'s Result 3 for the full diagnostic trail) by reducing
   `-ngl` to send fewer total layers to the constrained device. Real,
   correct, coherent output followed, including working Python code
   generation. **This is genuinely the "too large for one node, works
   when split" proof this item has been chasing all along** — half of the
   roadmap's target claim, confirmed on real hardware for the first time.
   The other half, **usable speed**, is not yet there: 1.5-2.5 tok/s
   generation is real and correct but slow, not the "usable speed"
   language in "How we'll know it's working" below describes. Don't
   publish this as the flagship benchmark number without that caveat
   attached — it's the capacity proof, not the performance proof.
3. **JS/TS client SDK**, mirroring `sdks/python`'s shape (nested
   `chat.completions`, real SSE streaming, typed errors). This is the
   difference between "has an API" and "has an ecosystem" — most people
   integrating a self-hosted LLM tool reach for JS first.
4. **Grafana dashboard JSON checked into the repo**, built on the new
   `/metrics` endpoint — `docker-compose.yml` already exists; add a
   Prometheus + Grafana profile so `docker compose up --profile monitoring`
   gives a working dashboard immediately. Zero-config observability is a
   real gap vs. every competitor here. **Status check (2026-08-15):
   shipped.** `deploy/prometheus/prometheus.yml` + `deploy/grafana/` (a
   provisioned datasource and a real dashboard JSON covering every metric
   `/metrics` exposes — throughput, p50/p95 latency, CPU/memory/GPU,
   cluster node count and VRAM, uptime, sample rate); `docker-compose.yml`
   gained `prometheus`/`grafana` services under `profiles: ["monitoring"]`,
   so a plain `docker compose up` is unaffected and
   `docker compose up --profile monitoring` brings both up pre-wired, no
   manual "add datasource"/"import dashboard" click needed. One real,
   non-obvious correctness catch along the way: `ghostlink-api` binds
   `0.0.0.0` (`docker-compose.yml`'s `command: serve 0.0.0.0 8003`), and
   `tls::is_loopback_host("0.0.0.0")` is deliberately `false` (it has its
   own test asserting exactly that) — so the container unconditionally
   forces HTTPS with a self-signed cert regardless of the `enable_tls`
   setting. A first pass at the scrape config assumed plain HTTP and would
   have silently scraped nothing; `prometheus.yml` now scrapes
   `https://` with `insecure_skip_verify` (no CA to validate a self-signed
   cert against in this deployment). Verified: YAML validated with
   `docker compose config`, the dashboard JSON validated, and a real
   locally-run `ghost-link serve 0.0.0.0` process confirmed to only answer
   on HTTPS, not plain HTTP, exactly as the fix assumes. **Not verified**:
   an actual live container scrape end-to-end — the Docker daemon wasn't
   running in the environment this shipped from (CLI present, engine not
   started), so this could only be validated statically plus against a
   real non-containerized server process, not a genuine `docker compose up
   --profile monitoring` run. The same 0.0.0.0-forces-HTTPS behavior also
   surfaced a separate, likely pre-existing bug worth its own fix:
   `ghostlink-api`'s Docker healthcheck curls plain `http://`, which this
   same logic implies should already be failing — flagged separately
   rather than fixed here, since confirming and fixing it needs a working
   Docker daemon this environment didn't have.
5. **One-line install script** (`curl | sh` / a signed installer per
   platform) using the now-multi-OS release artifacts. Ollama's biggest UX
   win is `curl -fsSL https://ollama.com/install.sh | sh`; Ghostlink should
   have the exact equivalent, now that Windows/macOS binaries actually
   exist.
6. **Config hot-reload for the settings that are safe to change live**
   (already flagged as a gap in the earlier review pass) — `/api/settings`
   exists for runtime changes; add a file-watch path for
   `ghostlink.toml`'s safe subset so a headless/server deployment doesn't
   need a restart for routine tuning.

## Horizon 2 (3–9 months): Build the moat

Once Priority Zero is real and provable, these make it hard to copy.

1. **Continuous rebalancing.** `load_balance.rs` already computes
   distribution plans; extend it to *re-plan live* when a node joins,
   leaves, or its health degrades mid-generation — graceful migration of
   in-flight pipeline stages, not just a static plan computed once at
   startup. This is the feature that turns "distributed" into "resilient,"
   and it's the kind of thing vLLM's static deployment model doesn't
   attempt and Kubernetes-based setups solve with a completely different
   (much heavier) toolset. **Status check (2026-08-08): more groundwork
   than "not started."** `load_balance.rs` already has a `rebalance()`
   function and real config (`max_concurrent_rebalances`,
   `recommended_workers`-derived defaults, a load threshold). What's
   unconfirmed is whether anything calls `rebalance()` in response to a
   live node join/leave/health-degrade event today, or whether it's only
   invoked from a static planning pass — worth a quick read of the call
   sites before scoping this as greenfield. **Status check (2026-08-15):
   read the call sites — it's greenfield after all, for the path that
   actually matters.** `execute_pipeline_with_rebalance*`/`rebalance()`
   has exactly one real caller (`main.rs`'s `ghost-link flow` CLI command,
   gated behind `GHOSTLINK_FLOW_ENABLE_REBALANCE`), and that command is the
   *synthetic* pipeline-benchmark path this roadmap's Priority Zero section
   already established doesn't run real model layers — `PipelinePlan`
   built from a fabricated 60-layer/0.5GB-per-layer spec, not a real GGUF.
   The real distributed-inference serving path (`handle_gui_model_load` →
   `rpc_cluster::discover_rpc_peers` → a single `llama-server --rpc ...`
   process launch, see Priority Zero and Enterprise Trust Track item #2)
   has no rebalancing concept at all — it's one process invocation per
   model load, not a per-token pipeline Ghostlink's own runtime coordinates
   step-by-step the way `runtime.rs`'s synthetic path does. So: real
   continuous rebalancing for real distributed inference is still fully
   unbuilt, not "needs live-event wiring added to an existing mechanism."
   The Risks section's own assessment stands — "mid-generation migration
   without corrupting output is a real research-adjacent problem... a
   simpler 'drain and restart the affected request' fallback is an
   acceptable first cut" — and that first cut hasn't been attempted either.
   Deliberately not attempted in this pass: this needs its own dedicated
   scoping, not a fast-follow bolted onto an unrelated feature.
2. **Speculative decoding across heterogeneous nodes** — a small/fast node
   (or NPU) drafts, a large/slow node verifies. This is a genuinely novel
   angle: nobody targets *consumer heterogeneous* hardware for this pattern
   the way Ghostlink's discovery+placement layer already could.
3. **Hugging Face Hub integration**: model search/download/quantization
   selection directly from the GUI and CLI (`ghost-link models pull
   <hf-repo>`), auto-picking a GGUF quant that fits the *cluster's*
   aggregate VRAM, not just one node's. Ollama's model library is one of
   its biggest UX wins; Ghostlink should match it while adding the
   cluster-aware sizing Ollama structurally can't do. **Status check
   (2026-08-08): partially shipped.** HF search/pull already exists
   (`ModelsTab.tsx`, HF handling in `main.rs`, a nightly
   `hf-model-verify.yml` CI job that checks downloads keep working). The
   part that's genuinely still missing is narrower than the original
   framing: *cluster-aggregate-VRAM-aware* quant selection specifically —
   picking a quant sized to what the whole discovered cluster can hold,
   not one node.
4. **LoRA / adapter support** in the native engine path — increasingly
   table-stakes, currently absent.
5. **RBAC + audit logging that's actually populated.** **Status check
   (2026-08-08): audit logging and API key role gating (Admin/Operator/Viewer) are done; full multi-user/multi-tenant RBAC is not.**
   `/api/security/audit-log` used to be hardcoded to always return empty;
   it's now backed by a real in-memory, capped (`AUDIT_LOG_CAP`) log of
   actual security events (failed auth, PQC/JWT actions, tool-call
   approvals — see `audit_log`/`record_audit_event` in `main.rs`). It
   resets on restart and isn't a persistent append-only trail, but "empty"
   is no longer accurate. What's still genuinely missing: multi-user API
   keys with scoped permissions (RBAC) — that's the remaining piece for
   any team/household deployment beyond a single operator. **This is the
   #1 priority in the "Enterprise Trust Track" section below** — see that
   section for scope and sequencing relative to RPC peer auth.
6. **Plugin marketplace-lite**: a `plugins.toml`-style registry (mirroring
   the existing `mcp_servers.toml` pattern) so third-party
   `InferenceBackendPlugin`/MCP-tool implementations can be discovered and
   installed by name/URL instead of hand-written and compiled in. Lowers
   the bar from "fork and add a Rust file" to "add one line of config" —
   the same shift that made VS Code's and Ollama's ecosystems take off.

## Horizon 3 (9–18 months): Category-defining bets

Bigger, riskier, higher payoff if the moat above is already real.

1. **True service discovery beyond a LAN**: an opt-in relay/rendezvous mode
   so a Ghostlink cluster can span two networks (home + office, or two
   contributors' machines) without VPN setup — while keeping the
   zero-broadcast-trust security model (HMAC + PQC-hybrid TLS) already
   built. This is the step from "LAN fabric" to "sovereign compute mesh,"
   without becoming "yet another cloud service."
2. **A real plugin ABI for planners**, not just backends — let a
   third-party crate supply an alternative `planning.rs`/`load_balance.rs`
   strategy (e.g. cost-aware placement, power-aware placement for
   battery-powered nodes) via the same trait-object pattern
   `backend_plugin.rs` established this sprint.
3. **First-class multi-modal pipeline**: the existing `mcp-vision` crate is
   a start; extend the same hardware-aware placement logic to route a
   vision or audio stage to whichever node actually has the NPU/GPU for it,
   inside one pipeline plan.
4. **A hosted "bring your own cluster" control plane** (fully optional,
   fully self-hostable fallback preserved) — a thin SaaS layer for fleet
   management/updates/telemetry across many Ghostlink clusters, monetizing
   the "commercial support path" the README already gestures at, without
   compromising the open-source, self-hosted core.

---

## Enterprise Trust Track: reviewing the "default enterprise harness" proposal (2026-08-15)

An external review proposed hardening Ghostlink into "the system teams
actually run production private LLM workloads on" — full RBAC/multi-tenancy,
mTLS everywhere, SSO/OIDC/SAML, SIEM-exportable audit trails, OpenTelemetry,
continuous rebalancing, model governance/canary promotion, WAN connectivity,
and Terraform/Ansible IaC. Rather than bolt this on as a parallel plan, it's
worth reconciling against what's actually in the tree and against this
roadmap's own stated identity — because about half the proposal either
duplicates something already tracked above, or cuts against the "zero-config,
single binary" differentiator the Risks section already warns not to
dilute.

**Fact-check against the current codebase**, since the proposal was written
from the outside without reading the code:

| Proposal's claim | Actual state, verified 2026-08-15 |
| --- | --- |
| "Basic auth/JWT" | Confirmed as described — a single global API key (`auth.rs`) signs JWTs; no scoped keys, no tenancy |
| "Optional PQC-hybrid TLS" | Undersold, not overstated — ML-KEM-768 hybrid TLS is real and implemented (`tls.rs`), not a stub |
| "Prometheus metrics" | Real — `/metrics` in Prometheus text-exposition format exists (`handle_metrics_prometheus` in `main.rs`), Grafana already referenced in `.env.example`/compose files |
| "Populate the real audit endpoint" | Already done as of the 2026-08-08 status check above — proposal was written against a stale picture |
| RBAC / scoped multi-user keys | **Confirmed genuinely absent** — correctly identified as the top gap |
| mTLS / authenticated node-to-node RPC | **Confirmed genuinely absent** — `ggml-rpc` has an IP allowlist and a build-version-mismatch check (see Risks below) but no protocol-level auth |
| OpenTelemetry tracing | Not found anywhere in the crates — correctly identified as missing |
| SSO/OIDC/SAML | Not found — correctly identified as missing |

So the proposal's two most emphasized items — RBAC and node-to-node
authentication — are exactly the two gaps this roadmap's own Horizon 2
item 5 and Risks section already flag as the most severe open issues,
arrived at independently. That convergence is the strongest signal in this
whole review: treat those two as the actual next security work, not
speculative gold-plating.

### Fold into Horizon 1/2 as-is (no identity risk, closes gaps this doc already flagged)

1. **Scoped API keys / RBAC** — teams/projects namespacing, least-privilege
   MCP tool access, model-level permissions. This *is* Horizon 2 item 5's
   remaining piece, not a new item; sequencing note below. **Status check
   (2026-08-15): the core 3-role API key access control shipped.** `auth.rs` now persists a
   hashed, multi-key store (`api_keys.json`) instead of one shared global
   key — each key carries a role (`Admin`/`Operator`/`Viewer`), checked by
   `main.rs`'s `required_role()` against every route (GET/HEAD default to
   `Viewer`, mutations to `Operator`, key management and
   `POST /api/security/pqc/enable` to `Admin`). `/api/security/keys`
   (GET/POST/DELETE) lets an Admin create and revoke narrowly-scoped keys
   via the API, shown once at creation like the original bootstrap key. An
   existing deployment's `api_key.txt` migrates automatically into the new
   store as the sole Admin key on first run — zero manual steps, verified
   live end-to-end (fresh boot, created an Operator and a Viewer key,
   confirmed 403s land exactly where the role model says they should,
   confirmed revoking a key invalidates its outstanding JWTs immediately
   rather than waiting out the 1h token lifetime, confirmed deleting the
   last Admin key is refused). Deliberately out of scope for this pass, and
   still open: team/project namespacing, per-model and per-MCP-tool
   permissions, and a GUI (Security tab) for key management — all
   backend/API-only today, reachable via `curl` the same way the rest of
   this server already is.
2. **RPC peer authentication** (mTLS or an equivalent handshake) — closes
   the Risks section's top-ranked gap directly: `ggml-rpc` today has zero
   protocol-level auth, only IP allowlisting and a version-mismatch check.
   IP allowlisting stops "anyone on the LAN"; it does not stop a device
   already inside an allowlisted range or one spoofing a source address.
   **Status check (2026-08-15): shipped, as an equivalent handshake rather
   than mTLS.** Real mTLS turned out to be structurally blocked: upstream
   llama.cpp's `--rpc` client speaks zero custom handshake and starts
   sending raw `ggml-rpc` binary protocol the instant it connects, so any
   additional auth step has to live outside that TCP stream, done by
   Ghostlink's own processes on both ends — not inside it. The shipped
   design (`rpc_cluster.rs`): a new `rpc_shared_secret` setting (empty/off
   by default, manually distributed across nodes like `rpc_allowed_peers`
   already is) gates a dedicated auth port
   (`rpc_port + RPC_AUTH_PORT_OFFSET`). Before opening the real `--rpc`
   connection, the coordinator does a nonce-based HMAC-SHA256 handshake
   there (`admit_via_secret`); success temporarily admits its source IP
   (`RPC_ADMISSION_TTL`, 30s) and the allowlist proxy now requires a *live*
   admission, not just allowlist membership, before splicing a connection
   through. A fresh nonce per handshake means a captured response can't be
   replayed. Verified live against a real running peer process: an
   unadmitted connection to the real RPC port is reset; a wrong-secret
   handshake is acked as a mismatch and leaves the port still rejecting;
   the correct secret is acked as a match and the *same* connection the
   proxy previously reset is then accepted and forwarded toward the local
   `ggml-rpc-server` (confirmed via the proxy's own "could not reach
   ggml-rpc-server" log line firing only because no real binary was present
   in that dev checkout, not because gating failed). Honestly scoped, same
   as the existing IP-allowlist caveat: this proves the connecting node held
   the secret at admission time and authorizes its source IP for a short
   window — it does **not** encrypt the actual `ggml-rpc` byte stream itself
   (still plain TCP), and an on-path attacker riding the same source IP
   during the admission window isn't stopped by it. True wire-level
   confidentiality would need a dual-proxy TLS tunnel on both ends (reusing
   `tls.rs`'s existing cert infra) — noted as a further follow-up, not
   attempted here.
3. **Durable, exportable audit log** — upgrade the existing in-memory
   capped log (`AUDIT_LOG_CAP`) to append-only persistent storage plus a
   JSON/CEF export path. This extends what already shipped rather than
   building a new system, and is a prerequisite for any real SIEM story.
   **Status check (2026-08-15): shipped.** New `audit_log.rs` module: every
   `record_audit_event` call now also appends the entry as one JSON line to
   an append-only `audit_log.jsonl` (`GHOSTLINK_AUDIT_LOG_PATH` override) —
   the in-memory capped `VecDeque` is untouched and still serves the GUI's
   live feed exactly as before. New `GET /api/security/audit-log/export`
   (`?format=json|cef`, default json) reads the *full* durable history,
   gated `Admin`-only (stricter than the existing capped live endpoint,
   which stays `Viewer`-readable) since a bulk historical export is a
   different exposure than a live tail. CEF output is real Common Event
   Format with correct extension-value escaping — not a cosmetic detail:
   several existing audit `detail` strings already contain raw `=`
   characters (e.g. `"name='{}' id={}"` on the key-revocation event), which
   would otherwise corrupt the field boundary for any real SIEM parser.
   Verified live: durable file confirmed on disk with real events including
   one deliberately containing both `=` and `|`; in-memory feed correctly
   empties on restart while the durable export still returns the
   pre-restart history; CEF export correctly escapes every `=` in the
   stress-test event while leaving the literal `|` alone (valid inside an
   extension value, unlike the pipe-delimited CEF header). Log
   rotation/retention policy remains a deliberate fast-follow, not
   attempted here — the file is append-only and unbounded.
4. **OpenTelemetry tracing**, layered on top of the *already-real*
   Prometheus metrics, plus finally checking in the Grafana dashboard JSON
   that Horizon 1 item 4 has called for since before v1.17 and that's
   still open. **Status check (2026-08-15): both halves shipped.** The
   Grafana dashboard/monitoring-profile half shipped first — see Horizon 1
   item 4's own status check for the full account. OpenTelemetry tracing
   followed: new `otel.rs` (`opentelemetry`/`opentelemetry_sdk`/
   `opentelemetry-otlp`/`tracing-opentelemetry`), entirely opt-in via
   `GHOSTLINK_OTEL_EXPORTER_ENDPOINT` — unset reproduces the exact
   plain-text console logging this codebase has always had, byte-for-byte.
   When set: `tower-http`'s `TraceLayer` gives an automatic root span per
   HTTP request (method/URI/status/latency, zero hand-written span code),
   plus three hand-instrumented phase spans in the distributed-inference
   model-load path (`rpc_peer_discovery_and_admission`, `model_load`, and
   `inference_generate` covering the whole chat turn including any
   tool-calling round trips). **A real architectural limit surfaced and is
   documented rather than glossed over**: the same class of constraint RPC
   peer auth hit — upstream llama.cpp's `--rpc` client speaks a raw binary
   protocol with no header/metadata slot for W3C trace-context propagation,
   so a trace cannot span *across* the actual `ggml-rpc` TCP hop the way a
   textbook distributed trace would; it ends at "launched llama-server,
   here's how long it took," the same honest boundary `rpc_cluster.rs`
   already draws. Per the user's explicit choice, no trace backend is
   bundled (no Jaeger/Collector added to docker-compose) — an operator
   points the endpoint at whatever they already run. Uses the SDK's
   synchronous `SimpleSpanProcessor` with a blocking HTTP client rather
   than the batched async one, deliberately: this initializes in `main()`
   before any tokio runtime exists (`main()` also dispatches non-`serve`
   subcommands like `flow`/`stage-worker`/`probe`), so the exporter can't
   depend on an ambient async reactor. Verified live: a real stub OTLP/HTTP
   receiver confirmed genuine `application/x-protobuf` POST bodies arriving
   from a running server issuing real requests — the full span-creation →
   OTel-layer → OTLP-exporter → HTTP-POST pipeline proven end-to-end, not
   just compiled.
5. Horizon 1 item 6 (config hot-reload) is a direct prerequisite for any
   of the above being usable in a headless/server deployment — still open,
   still worth doing first or alongside.

### Fold into Horizon 2/3, but strictly as opt-in layers on top of the zero-config default

6. **OIDC** as an *additional* auth provider layered on top of, not
   replacing, the existing API-key/JWT path — a household user on a LAN
   should never see this. Deliberately **not SAML**: SAML is legacy,
   heavier to implement and audit than OIDC, and covers no realistic
   near-term customer this project has; only build it if a specific buyer
   asks for it by name.
7. **Confirm/finish continuous rebalancing** (Horizon 2 item 1) —
   prerequisite groundwork for treating this as production-grade multi-
   tenant infrastructure at all; a node degrading mid-generation under a
   real RBAC'd multi-team deployment is a much bigger deal than in a
   single-operator lab.
8. **Optional guardrail/policy middleware** (PII redaction, prompt/response
   filtering, usage quotas) — pluggable and off by default, sitting in
   front of the request path rather than inside the low-latency ring-buffer
   transport the Risks section already protects from feature creep.
9. **Lightweight model registry versioning** with pin/rollback — extends
   the existing HF integration and `plugins.toml`-style registry pattern
   (Horizon 2 items 3 and 6). Deliberately scoped down from the proposal's
   "canary/blue-green promotion" language: that's k8s-deployment-shaped
   machinery this project has no runtime to support without the
   rearchitecture the Risks section explicitly rules out.
10. **Usage attribution and quotas per tenant** — only meaningful once #1
    (RBAC) exists to define what a "tenant" is; sequence strictly after it,
    not in parallel.

### Explicitly rejected, or descoped hard, from the proposal

- **No re-architecting around Kubernetes.** The proposal's "Scale &
  Connectivity" and "Operational Polish" sections lean on
  Terraform/Ansible/canary-promotion language that implicitly assumes a
  k8s-shaped deployment model. This directly contradicts the Risks
  section's existing guardrail: *"Don't chase Kubernetes-based competitors
  on their home turf... If enterprise demand for a k8s deployment mode
  materializes, ship it as an optional deployment target, not a
  rearchitecture."* Terraform/Ansible are fine as thin, optional,
  Horizon-3-or-later wrappers around the existing single binary — never a
  prerequisite for using it.
- **No heavyweight approval-workflow engine** for model promotion — the
  realistic Ghostlink deployment is a small team or a household, not an
  org with a change-approval board. Pin/rollback (item 9 above) gets the
  real safety property (bad model doesn't silently stay hot) without the
  process overhead.
- **No cost/power-aware scheduling as a near-term item** — real, and
  already partially covered by Horizon 3 item 2's speculative-decoding
  work and the planner-ABI item, but it's speculative distributed-systems
  work layered on top of RBAC/tenancy that doesn't exist yet. Revisit once
  item 10 lands.

### Recommended sequencing

RBAC (#1) and RPC peer auth (#2) first, in parallel if there's bandwidth —
both are prerequisites for nearly everything else in this section (tenancy,
quotas, cost attribution, even a meaningful audit export all assume scoped
identity exists), and both are the two gaps this roadmap already flagged
independently of the external review. Durable audit log (#3) and OTel (#4)
are the natural next pair once identity exists to attribute events to.
Everything under "fold into Horizon 2/3" should wait for that foundation
rather than being started opportunistically — a policy engine or quota
system built against a single global API key will need rework once RBAC
lands.

---

## Competitive scorecard (target state after Horizon 2)

| Dimension | vLLM | Ollama | LM Studio | Ghostlink (target) |
| --- | --- | --- | --- | --- |
| Zero-config LAN clustering | ✗ | ✗ | ✗ | ✓ (UDP + mDNS today) |
| Heterogeneous hardware (mixed GPU/NPU/CPU) | Partial | ✗ | ✗ | ✓ |
| Real cross-node model sharding for a live chat request | ✓ (homogeneous) | ✗ | ✗ | ✓ *(after Priority Zero)* |
| Single self-hosted binary, no k8s/Python env | ✗ | ✓ | ✓ | ✓ |
| Prometheus + Grafana out of the box | Partial | ✗ | ✗ | ✓ |
| Plugin system for custom backends | ✗ | ✗ | ✗ | ✓ (shipped this sprint) |
| Official client SDKs | Partial | ✗ | ✗ | ✓ (Python shipped; JS planned H1) |
| Continuous rebalancing on node join/leave/degrade | ✗ | n/a | n/a | ✓ *(H2 target)* |

## How we'll know it's working

- A published, reproducible benchmark showing a model too large for any
  single owned machine running at usable speed across ≥2 heterogeneous
  nodes, with a one-command setup — becomes the canonical "why Ghostlink"
  demo, replacing prose claims with a number. **Status (2026-08-08): half
  done, on real hardware.** `BENCHMARKS.md`'s native two-machine entry,
  Result 3, has the "too large for any single owned machine" half —
  confirmed with a real single-node `ErrorOutOfDeviceMemory` failure and a
  real, correct distributed success on the same model. "Usable speed" is
  not yet there (1.5-2.5 tok/s) and the setup was not one-command (manual
  native builds on both machines, plus real debugging of a driver-level
  bug to get there). Don't call this item done from that entry alone.
- Time-to-first-distributed-chat-completion on fresh hardware: target
  under 5 minutes from `curl | sh` to a cluster-routed response.
- GitHub stars/forks growth rate and PyPI/npm SDK download counts as
  leading indicators of ecosystem pull, once Horizon 1's SDKs and install
  script ship.

## Risks

- **`ggml-rpc` has no version-compatibility check between peers, and
  mismatched builds silently corrupt output rather than failing.**
  Confirmed on real, genuinely separate hardware (2026-08-08,
  `BENCHMARKS.md`'s native two-machine entry — read it in full, this is
  the most important finding in that document): two machines running
  `llama.cpp` builds 10 days apart connected and exchanged data without
  complaint, and larger-model inference through the real distributed path
  came back as reproducible garbage (`"pérdida RencontreDBus并不是很
  pérdida..."`) while the API reported `healthy`/HTTP 200 the entire time.
  A controlled before/after test (same hardware, same model, only the
  peer's `llama.cpp` commit changed) confirmed this as the root cause —
  rebuilding both sides at an identical commit produced correct output.
  Smaller models (1.5B) worked correctly despite the same mismatch, so the
  exact trigger threshold isn't fully characterized, but the core problem
  is clear and severe: **there is currently no way for an operator to
  detect this from the API surface alone.** This ranks above the
  authentication gap below in severity — an unauthenticated-but-correct
  compute contribution is a trust problem; a silently-wrong answer that
  reports itself as healthy is a correctness problem, and a much easier
  one to ship without noticing. Needs a real fix before any "production
  trustworthy" claim: at minimum, a version/build-fingerprint handshake at
  RPC-connect time that refuses (or loudly warns on) a mismatch, not a
  documentation-only caveat.
- **Two related resilience gaps found in the same session**, lower
  severity but real: (1) the spawned `ggml-rpc-server` contributor child
  process isn't supervised — if it crashes (a real, separate bug: quantized
  KV cache combined with the RPC-CPU backend aborts on an unimplemented op,
  see `BENCHMARKS.md`), the node keeps *advertising* itself as RPC-capable
  via discovery while actually unreachable, and nothing restarts it. (2)
  the hardcoded 90-second model-ready timeout in `native_engine.rs` doesn't
  account for real cross-machine RPC transfer time — a real load that
  would have succeeded at 5 minutes gets aborted and reported as a failure
  well before that. Both are concrete, scoped fixes, not research problems.
- **Priority Zero shipped as an additive, off-by-default change**
  (`distributed_inference` defaults false; a single node reproduces prior
  behavior exactly) — still needs the CI integration test from Horizon 1
  (two real processes, real discovery, asserted cross-process execution,
  not just node count) before it's trusted at the level the rest of this
  roadmap assumes.
- **Continuous rebalancing (H2) is genuinely hard distributed-systems
  work** — mid-generation migration without corrupting output is a real
  research-adjacent problem, not a weekend feature. Budget accordingly; a
  simpler "drain and restart the affected request" fallback is an
  acceptable first cut.
- **Don't chase Kubernetes-based competitors on their home turf.** Adding
  full k8s-operator support would dilute the "zero-config, single binary"
  identity that's the actual differentiator. If enterprise demand for a
  k8s deployment mode materializes, ship it as an optional deployment
  target, not a rearchitecture.
- **`ggml-rpc-server` (the new distributed-inference contributor process)
  has no built-in authentication** — an upstream llama.cpp limitation, not
  something Ghostlink's layer can patch directly. `contribute_compute` is
  off by default and the startup log carries an explicit warning, and this
  is still the same trust-the-LAN posture as UDP/mDNS discovery, now
  backing a service that accepts arbitrary compute jobs rather than just
  broadcasting hardware specs — a meaningfully bigger blast radius if a
  hostile device is already on the network.
  **Status check (2026-08-08): partially addressed.** An IP allowlist now
  exists: `rpc_allowed_peers` in settings.json (plain IPv4 addresses or IPv4
  CIDR ranges) is enforced by a Ghostlink-owned TCP proxy in
  `rpc_cluster.rs` — when the allowlist is non-empty, `ggml-rpc-server`
  binds loopback-only and the proxy fronts the publicly-advertised port,
  splicing through only allowed source IPs and closing everything else.
  Empty (the default) is unchanged: `ggml-rpc-server` binds the public
  address directly, zero overhead, zero behavior change. This is real
  access control — "only these hosts/subnets," not "anyone on the LAN" —
  but it is **not** authentication of the RPC protocol itself: a device
  already inside an allowlisted range, or one able to spoof a source IP on
  the LAN, is not stopped by it. Given `contribute_compute` is a real
  security boundary once RBAC/multi-user work (Horizon 2 item 5) starts
  inviting more than a single trusted operator, genuine protocol-level
  auth (not just IP allowlisting) is still worth pulling forward alongside
  the Horizon 2 plugin-marketplace work rather than treating this as fully
  closed. **This is item #2 in the "Enterprise Trust Track" section above**,
  paired with RBAC as the two highest-priority security items on this
  roadmap. **Status check (2026-08-15): shipped** — see that section's status
  check for the `rpc_shared_secret` handshake design and what it does and
  doesn't close (a device already inside an allowlisted range can no longer
  ride through without the secret; the actual RPC byte stream itself is
  still unencrypted plain TCP, an honestly-documented remaining gap, not a
  silent one).
