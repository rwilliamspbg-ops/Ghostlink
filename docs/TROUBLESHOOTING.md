# Ghost-Link Troubleshooting Guide

## Common Issues and Solutions

### Doctor Report Triage (PASS/WARN/FAIL)

Use this flow when reviewing `ghost-link doctor` output in local runs or CI artifacts.

Run command:

```bash
cargo run -p ghost-link -- doctor --strict --json ./tmp/doctor-report.json
```

Status interpretation:

- `PASS`: healthy for the current environment. No immediate action required.

- `WARN`: non-blocking risk or missing optional capability.
   Expected warning example: headless GUI environment in CI.
   Action: document rationale if expected, otherwise apply the `FIX` guidance and rerun doctor.

- `FAIL`: blocking readiness or accuracy issue.
   Action: treat as release blocker and do not merge until fail count is zero.

Area-based triage priorities:

- `accuracy`: highest priority. Any failure can invalidate runtime conclusions.

- `readiness`: missing runtime prerequisites and configuration gaps.

- `accessibility`: multi-device/operator usability checks; headless warnings may be expected with validated fallback.

- `environment`: toolchain/runtime availability checks.

Suggested reviewer checklist:

- Confirm `Summary` reports `fail = 0`.

- Review each `WARN` and classify as expected or fix-required.

- If network behavior matters for the change, run:

```bash
cargo run -p ghost-link -- doctor --network-probe --network-target <host:port>
```

- Attach or upload `./tmp/doctor-report.json` to the PR for traceability.

### Ring Buffer Issues

#### Issue: "Ring buffer is full" errors from producer

**Symptoms:**
```
error: Ring buffer overflow at position 1023
```

**Causes:**
- Consumer not keeping up with producer
- Backpressure threshold reached (70% capacity)

**Solutions:**
1. Increase ring buffer capacity in `RingConfig::new(capacity)`
2. Implement backpressure handling in producer loop:
   ```rust
   while ring.should_backpressure() {
       std::thread::sleep(Duration::from_millis(1));
   }
   ```
3. Check consumer throughput and optimize processing

#### Issue: Memory ordering violations causing data races

**Symptoms:**
- Occasional incorrect values read from ring buffer
- Data corruption in high-throughput scenarios

**Solutions:**
- Ensure proper `Ordering::Release` on tail store (producer)
- Ensure proper `Ordering::Acquire` on tail load (consumer)
- Use `AtomicUsize` for head/tail counters
- Never access ring buffer from multiple producers/consumers without synchronization

### Protocol Issues

#### Issue: "CRC mismatch" errors on frame decode

**Symptoms:**
```
error: CRC mismatch: expected 0x12345678, got 0x9abcdef0
```

**Causes:**
- Frame was modified in transit
- Wrong EtherType received (not Ghost-Link protocol)
- Network corruption

**Solutions:**
1. Verify network interface is configured for raw socket access
2. Check that only Ghost-Link frames are being sent/received
3. Implement frame validation before processing:
   ```rust
   if ether_type != GHOSTLINK_ETHERTYPE {
       return Err("wrong protocol".into());
   }
   ```

#### Issue: "Frame too short" errors

**Symptoms:**
```
error: frame too short
```

**Causes:**
- Incomplete frame received
- Network interruption during transmission
- Wrong buffer size allocated

**Solutions:**
1. Ensure receive buffers are at least `MAX_XDP_FRAME_SIZE` (2048 bytes)
2. Implement proper frame completion detection
3. Handle network errors gracefully with retry logic

### Cluster State Issues

#### Issue: "No active nodes" in dashboard

**Symptoms:**
- Dashboard shows "EMPTY" or "No active nodes"
- `cargo run -p ghost-link -- plan` fails

**Causes:**
- Nodes not registered with cluster
- Heartbeat timeouts marking nodes as failed
- Cluster initialization error

**Solutions:**
1. Verify node registration:
   ```bash
   cargo run -p ghost-link -- join node-01
   cargo run -p ghost-link -- join node-02
   ```
2. Check heartbeat timeout configuration (default: 5 seconds)
3. Reset failed nodes:
   ```rust
   cluster.recover_node("node-id");
   ```

#### Issue: "Insufficient cluster VRAM" errors

**Symptoms:**
```
error: insufficient cluster VRAM for layer 32 (needs 1.0 GB)
```

**Causes:**
- Model layers don't fit in available VRAM
- Nodes have less total VRAM than required

**Solutions:**
1. Use adaptive quantization to reduce VRAM requirements:
   ```rust
   let mode = select_quantization_mode(delivery_ratio);
   // Apply quantization when placing layers
   ```
2. Add more nodes to cluster
3. Reduce model size or use smaller variant

### Health Monitoring Issues

#### Issue: "Node marked as failed" unexpectedly

**Symptoms:**
- Node suddenly appears in failed status
- Dashboard shows degraded/failed node count increasing

**Causes:**
- Network partition between nodes
- CPU overload causing missed heartbeats
- High latency triggering timeout

**Solutions:**
1. Increase heartbeat timeout:
   ```rust
   let config = HealthConfig {
       timeout: Duration::from_secs(10), // Increased from 5s
       ..Default::default()
   };
   ```
2. Check network connectivity between nodes
3. Monitor CPU/memory usage on affected node

#### Issue: "Delivery ratio dropped below threshold"

**Symptoms:**
```
delivery_ratio=0.78 => Int4 (was None)
```

**Causes:**
- Network congestion
- CPU bottleneck
- Memory pressure

**Solutions:**
1. Monitor and address root cause (network, CPU, memory)
2. Consider load shedding to reduce per-node load
3. Implement graceful degradation with quantization fallback

### Load Balancing Issues

#### Issue: "Deadlock prevention timeout" errors

**Symptoms:**
```
error: deadlock prevention timeout
```

**Causes:**
- Circular wait condition during layer assignment
- Lock contention on shared resources
- Insufficient timeout for large models

**Solutions:**
1. Increase lock timeout in `LoadBalanceConfig`:
   ```rust
   let config = LoadBalanceConfig {
       lock_timeout_us: 5000, // Increased from 1000us
       ..Default::default()
   };
   ```
2. Use non-blocking layer assignment with retry logic
3. Implement fair scheduling to prevent starvation

#### Issue: "Insufficient VRAM" after load rebalancing

**Symptoms:**
- Rebalancing completes but layers don't fit
- Some nodes end up overloaded

**Solutions:**
1. Review layer sizes and consider splitting large layers
2. Use more granular layer assignments (smaller chunks)
3. Implement dynamic layer sizing based on node capabilities

### XDP Integration Issues (Linux-specific)

#### Issue: "AF_XDP sockets are Linux-only" errors

**Symptoms:**
```
error: AF_XDP sockets are Linux-only
```

**Causes:**
- Running on Windows/macOS
- Attempting to use AF_XDP features outside Linux

**Solutions:**
1. For non-Linux environments, use standard TCP/IP sockets instead
2. Implement platform-specific build configuration:
   ```toml
   [target.'cfg(target_os = "linux")'.dependencies]
   xdp-sys = "0.1"
   ```

#### Issue: "eBPF loading requires Linux kernel support" errors

**Symptoms:**
```
error: eBPF loading requires Linux kernel support
```

**Causes:**
- Kernel lacks eBPF support (older kernels)
- Missing eBPF helper functions
- Security module blocking eBPF programs

**Solutions:**
1. Upgrade to kernel 5.8+ with eBPF support
2. Verify eBPF is enabled: `sysctl net.core.bpf_jit_enable`
3. Check security module settings: `dmesg | grep -i ebpf`

### Dashboard Issues

#### Issue: "Failed to enable raw mode" errors

**Symptoms:**
```
Failed to enable raw mode: Os { code: 1, kind: PermissionDenied, .. }
```

**Causes:**
- Running in non-interactive terminal
- Terminal doesn't support raw mode
- Permission issues

**Solutions:**
1. Run in interactive terminal: `cargo run -p ghost-link -- dashboard`
2. Use ASCII fallback instead of ratatui:
   ```rust
   let dashboard = Dashboard::new(/* args */);
   println!("{}", dashboard.render_ascii());
   ```
3. Check terminal capabilities with `tput colors`

#### Issue: "Failed to show popup" errors

**Symptoms:**
```
Failed to show popup: Os { code: 1, kind: PermissionDenied, .. }
```

**Causes:**
- Terminal doesn't support popups
- Permission issues with terminal emulation

**Solutions:**
1. Use ASCII dashboard instead of ratatui
2. Run in modern terminal (iTerm2, GNOME Terminal, etc.)
3. Check terminal settings for popup support

### General Issues

#### Issue: Hugging Face model download verification fails

**Symptoms:**
```
FAILED: 1 repository checks failed
```

**Causes:**
- Invalid repo id or filename
- Missing authentication for gated/private models
- Network/DNS/proxy restrictions
- Rate limiting on unauthenticated requests

**Solutions:**
1. Verify repo and file names:
   ```bash
   python3 scripts/verify_hf_models.py --repo sshleifer/tiny-gpt2 --file config.json
   ```
2. Use an auth token for gated models and higher rate limits:
   ```bash
   export HF_TOKEN=your_token_here
   python3 scripts/verify_hf_models.py --repo your-org/your-model --file config.json
   ```
3. Check connectivity/proxy settings:
   ```bash
   curl -I https://huggingface.co
   ```
4. Retry with a known public tiny repo to isolate auth issues.

#### Issue: "Failed to decode discovery frame" errors

**Symptoms:**
```
error: Failed to decode discovery frame
```

**Causes:**
- Corrupted frame data
- Wrong protocol version
- Malformed payload

**Solutions:**
1. Verify sender is using compatible protocol version
2. Check network for packet corruption
3. Implement retry with exponential backoff

#### Issue: "Ring buffer capacity exceeded" errors

**Symptoms:**
```
error: Ring buffer overflow at position 1023
```

**Causes:**
- Producer outpacing consumer
- Insufficient ring buffer size

**Solutions:**
1. Increase ring buffer capacity:
   ```rust
   let ring = SpscRingBuffer::new(RingConfig {
       capacity: 4096, // Increased from 1024
       ..Default::default()
   });
   ```
2. Implement backpressure in producer

## Debugging Tips

### Enable Verbose Logging

```bash
RUST_LOG=debug cargo run -p ghost-link -- plan
```

### Check Ring Buffer Statistics

```rust
let (used, capacity) = ring_stats;
println!("Ring buffer: {}/{} ({:.1}%)", used, capacity, 
    (used as f32 / capacity as f32) * 100.0);
```

### Monitor Cluster Health

```bash
cargo run -p ghost-link -- dashboard
# Then press 'r' to refresh cluster state
```

### Test Individual Components

```bash
# Test ring buffer
cargo test --package ghostlink-core -- ring

# Test protocol
cargo test --package ghostlink-core -- protocol

# Test planning
cargo test --package ghostlink-core -- planning
```

### Check for Memory Issues

```bash
# Run with memory profiler (if available)
RUSTFLAGS="-C instrument-coverage" cargo test

# Or use valgrind on Linux
valgrind --leak-check=full cargo run -p ghost-link -- plan
```

## Performance Tuning

### Optimize Ring Buffer Throughput

1. Use appropriate capacity for workload:
   ```rust
   let config = RingConfig {
       capacity: 8192, // Higher for high-throughput workloads
       backpressure_threshold: 5734, // Still at 70%
   };
   ```

2. Align buffer size to cache line boundaries (64 bytes)

3. Use `pin-utils` for zero-copy allocations

### Optimize Protocol Encoding

1. Use fixed-width fields where possible
2. Minimize payload size with compression for large messages
3. Implement batch encoding for multiple frames

### Optimize Load Balancing

1. Use smaller layer chunks for better distribution
2. Implement predictive rebalancing based on metrics
3. Use non-blocking assignment with timeouts

## Getting Help

If you encounter issues not covered here:

1. Check the GitHub issues for similar problems
2. Review the architecture documentation for component details
3. Enable debug logging and capture error messages
4. Test components individually to isolate the issue

## Contributing

Found a bug or have a feature request? Please open an issue on the repository with:

- Clear description of the problem
- Steps to reproduce
- Expected vs actual behavior
- Environment details (OS, Rust version, dependencies)

Thank you for using Ghost-Link!
