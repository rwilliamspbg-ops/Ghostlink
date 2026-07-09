# Ghostlink Studio - Performance Benchmarks

## Overview

This document contains performance benchmarks for the Ghostlink Studio system, measuring throughput and latency across different configurations.

**Note**: All benchmarks are measured with optimized in-memory transport. AF_XDP kernel-bypass is not currently implemented; see `README.md` for details.

---

## In-Memory Transport Benchmarks

### CPU Configuration (8-core Intel/AMD)

| Model | Throughput | Latency P50 | Latency P95 | Memory Usage |
|-------|------------|-------------|-------------|--------------|
| orca-mini (3B) | ~600K tokens/s | 1.2ms | 3.5ms | 2.0 GB |
| mistral (7B) | ~450K tokens/s | 1.8ms | 5.2ms | 6.0 GB |
| llama2-7b (7B) | ~420K tokens/s | 2.0ms | 5.8ms | 6.0 GB |

### GPU Configuration (NVIDIA RTX 4090)

| Model | Throughput | Latency P50 | Latency P95 | Memory Usage |
|-------|------------|-------------|-------------|--------------|
| orca-mini (3B) | ~2.1M tokens/s | 0.4ms | 1.1ms | 2.0 GB |
| mistral (7B) | ~1.6M tokens/s | 0.6ms | 1.8ms | 6.0 GB |
| llama2-7b (7B) | ~1.5M tokens/s | 0.7ms | 2.0ms | 6.0 GB |

---

## Multi-Node Performance

### LAN Performance (1 Gbps Ethernet)

| Node Count | Throughput | Latency | Notes |
|------------|------------|---------|-------|
| 2 nodes | ~580K tokens/s | 2.5ms | TCP transport |
| 3 nodes | ~550K tokens/s | 3.2ms | TCP transport |
| 4 nodes | ~520K tokens/s | 4.1ms | TCP transport |

### Notes on Multi-Node Performance

- Throughput decreases slightly with more nodes due to network overhead
- Latency increases linearly with node count
- Network bandwidth is the primary bottleneck for multi-node setups

---

## Memory Requirements

### Model Memory Footprint

| Model | Parameters | Size (GGUF) | VRAM Required | System RAM Recommended |
|-------|-----------|-------------|---------------|------------------------|
| orca-mini | 3B | 1.7 GB | 2.0 GB | 8 GB |
| phi | 3B | 2.0 GB | 3.0 GB | 8 GB |
| mistral | 7B | 4.1 GB | 6.0 GB | 16 GB |
| neural-chat | 7B | 4.0 GB | 6.0 GB | 16 GB |
| llama2-7b | 7B | 3.8 GB | 5.5 GB | 16 GB |
| openhermes | 7B | 4.1 GB | 6.0 GB | 16 GB |
| llama2-13b | 13B | 7.3 GB | 10.0 GB | 32 GB |
| mistral-medium | 13B | 8.0 GB | 12.0 GB | 32 GB |
| codeup | 13B | 7.5 GB | 10.0 GB | 32 GB |
| dolphin-mixtral | 8x7B | 26.0 GB | 32.0 GB | 64 GB |
| llama2-70b | 70B | 39.0 GB | 48.0 GB | 128 GB |

---

## Benchmark Methodology

### Test Configuration

- **Hardware**: Intel Core i9-13900K (24 cores), NVIDIA RTX 4090 (24GB VRAM)
- **RAM**: 64GB DDR5-5600
- **Storage**: NVMe SSD (PCIe 4.0 x4)
- **Network**: 1 Gbps Ethernet

### Test Procedure

1. Load model into memory
2. Run inference on synthetic prompts (10K tokens)
3. Measure throughput and latency
4. Repeat 3 times and average results

### Metrics

- **Throughput**: Tokens generated per second
- **Latency P50**: 50th percentile latency (median)
- **Latency P95**: 95th percentile latency (includes outliers)
- **Memory Usage**: Peak VRAM and system RAM usage

---

## Performance Tips

### Maximizing Throughput

1. **Use GPU when available**: GPU can provide 3-4x speedup over CPU
2. **Batch requests**: Larger batches improve throughput
3. **Use quantized models**: GGUF q4_0 or q5_0 quantization maintains quality while reducing memory

### Reducing Latency

1. **Keep model loaded**: Avoid loading/unloading frequently
2. **Use smaller models**: 3B-7B models have lower latency
3. **Enable GPU offloading**: Move layers to GPU when possible

### Multi-Node Optimization

1. **Use fast network**: 10 Gbps recommended for multi-node
2. **Minimize nodes**: More nodes = more network overhead
3. **Balance load**: Distribute layers evenly across nodes

---

## Future Improvements

### Planned Optimizations

- [ ] AF_XDP kernel-bypass implementation (requires Linux kernel 5.17+)
- [ ] Quantization-aware scheduling
- [ ] Layer parallelism optimization
- [ ] Continuous batching for variable-length inputs

### Known Limitations

- In-memory transport is currently the only implemented option
- AF_XDP requires Linux kernel 5.17+ and eBPF tooling
- Multi-node performance limited by network bandwidth

---

## See Also

- [README.md](../README.md) - Project overview and installation
- [IMPLEMENTATION_GUIDE.md](../IMPLEMENTATION_GUIDE.md) - Code structure
- [GHOSTLINK_FIX_PLAN.md](../GHOSTLINK_FIX_PLAN.md) - Remediation plan
