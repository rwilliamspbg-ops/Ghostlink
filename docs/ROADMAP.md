# Ghostlink: Path to Category Leadership

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
   as CONTRIBUTING.md's release rubric already asks for.
3. **JS/TS client SDK**, mirroring `sdks/python`'s shape (nested
   `chat.completions`, real SSE streaming, typed errors). This is the
   difference between "has an API" and "has an ecosystem" — most people
   integrating a self-hosted LLM tool reach for JS first.
4. **Grafana dashboard JSON checked into the repo**, built on the new
   `/metrics` endpoint — `docker-compose.yml` already exists; add a
   Prometheus + Grafana profile so `docker compose up --profile monitoring`
   gives a working dashboard immediately. Zero-config observability is a
   real gap vs. every competitor here.
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
   (much heavier) toolset.
2. **Speculative decoding across heterogeneous nodes** — a small/fast node
   (or NPU) drafts, a large/slow node verifies. This is a genuinely novel
   angle: nobody targets *consumer heterogeneous* hardware for this pattern
   the way Ghostlink's discovery+placement layer already could.
3. **Hugging Face Hub integration**: model search/download/quantization
   selection directly from the GUI and CLI (`ghost-link models pull
   <hf-repo>`), auto-picking a GGUF quant that fits the *cluster's*
   aggregate VRAM, not just one node's. Ollama's model library is one of
   its biggest UX wins; Ghostlink should match it while adding the
   cluster-aware sizing Ollama structurally can't do.
4. **LoRA / adapter support** in the native engine path — increasingly
   table-stakes, currently absent.
5. **RBAC + audit logging that's actually populated** (`/api/security/audit-log`
   exists today but is permanently empty — closing that is cheap and
   directly serves the "enterprise-adjacent trust" wedge; multi-user API
   keys with scoped permissions is the natural next step for any
   team/household deployment beyond a single operator).
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
  demo, replacing prose claims with a number.
- Time-to-first-distributed-chat-completion on fresh hardware: target
  under 5 minutes from `curl | sh` to a cluster-routed response.
- GitHub stars/forks growth rate and PyPI/npm SDK download counts as
  leading indicators of ecosystem pull, once Horizon 1's SDKs and install
  script ship.

## Risks

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
  something Ghostlink's layer can currently paper over. `contribute_compute`
  is off by default and the startup log carries an explicit warning, but
  this is the same trust-the-LAN posture as UDP/mDNS discovery, now backing
  a service that accepts arbitrary compute jobs rather than just
  broadcasting hardware specs — a meaningfully bigger blast radius if a
  hostile device is already on the network. Worth an explicit allowlist or
  auth shim ahead of any "enable this on an untrusted network" claim.
