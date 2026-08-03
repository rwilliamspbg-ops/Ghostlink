# Ghostlink vs. Alternative Platforms

| Platform | Best For | Strength | Weakness |
| --- | --- | --- | --- |
| **Ghostlink** | Zero-config distributed inference across heterogeneous LAN hardware | Auto-discovers mixed CPU/GPU/NPU boxes on a LAN and shards a real model across them (via llama.cpp's own RPC backend) with no manual `--rpc` flags; self-hosted, single binary | Early-stage ecosystem; no relay/WAN mode yet (LAN only) |
| vLLM / TensorRT-LLM | Datacenter-grade single/multi-GPU serving | Mature ecosystem, continuous batching, strong NVIDIA performance | Assumes homogeneous, co-located GPUs; heavy to run standalone; no zero-config LAN clustering |
| Ollama | Single-node local model serving | Trivial setup, large model library | No real multi-node distribution; no cluster discovery |
| LM Studio | Desktop-first local chat/inference | Polished GUI, easy model management | Single machine only, closed-source; not API-extensible |
| llama.cpp server | Lightweight single-binary inference | Minimal footprint, broad hardware support (CPU/GPU/NPU via backends) | No orchestration, discovery, or cluster layer — Ghostlink wraps this as one of its backends |
| OpenWebUI | Chat UI in front of existing backends | Polished UI, plugin ecosystem | Not an inference/scheduling engine itself — pairs with a backend rather than replacing one |
| DeepSpeed | Large-scale training and inference | Strong training integrations | More complex operational model; not built for ad-hoc LAN clusters |
| Ray | General distributed workloads | Broad ecosystem | Heavier abstraction overhead for a pure inference use case |
| Kubernetes-based setups (KServe, Triton, etc.) | Large fleets, org-wide model serving | Battle-tested scheduling, autoscaling | Needs a cluster, ops staff, and YAML — wildly overkill for a household/small-team LAN |

## Ghostlink positioning

Nobody else combines *zero-config discovery of heterogeneous consumer/prosumer
hardware already sitting on a LAN* (gaming GPU + old laptop + NPU-equipped
ultrabook + Mac) with *real distributed inference across it* — a model too
large for any single owned machine, running at usable speed, split across
two or more of them, discovered automatically. That's the gap: vLLM assumes
a homogeneous co-located GPU fleet; Ollama and LM Studio don't distribute at
all; Kubernetes-based serving solves this but needs a cluster and an ops
team. Ghostlink wraps proven single-node engines (llama.cpp, Ollama) rather
than competing with them, and adds hardware-aware placement, cluster
discovery, and cross-node sharding on top — in a single self-hosted binary
with production auth and rate limiting built in.

See [ROADMAP.md](ROADMAP.md) for the full competitive strategy behind this
positioning, and [BENCHMARKS.md](BENCHMARKS.md) for the real (and
honestly-labeled not-yet-real) numbers behind these claims.
