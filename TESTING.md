# Testing Guide for Ghostlink

This document explains the test organization, how to run tests, and guidelines for adding new tests.

---

## Test Organization

The test suite is organized into unit tests (in-module) and integration tests:

```
crates/ghostlink-core/src/
├── ring.rs              # 7 unit tests (SPSC ring buffer, wrap-around, backpressure)
├── protocol.rs          # 4+ unit tests (encode/decode, CRC) + property tests
├── cluster.rs           # 14+ unit tests (registration, heartbeat, health, metrics)
├── health.rs            # 6+ unit tests (health monitoring, degradation detection)
├── planning.rs          # 5+ unit tests (layer assignment, quantization)
├── load_balance.rs      # 5+ unit tests (distribution, rebalancing)
├── xdp.rs               # 6+ unit tests (frame processing, stats)
└── dashboard.rs         # 3+ unit tests (rendering, state)

tests/
├── integration.rs       # 20+ integration tests (multi-node, cascades, stress)
└── common.rs            # Shared test utilities (sample data, assertions)
```

### Test Coverage by Component

| Component | Unit Tests | Integration | Coverage % | Notes |
|-----------|-----------|-------------|-----------|-------|
| Ring Buffer | 7 | 2 | **95%** | FIFO, wrap-around, backpressure, stress |
| Protocol | 4 | 7 | **85%** | Encode/decode, CRC, corruption detection |
| Cluster | 14 | 3 | **85%** | Registration, heartbeat, health, metrics |
| Health | 6 | 1 | **70%** | Monitoring, thresholds, degradation |
| Planning | 5 | 2 | **80%** | Layer assignment, capacity checking |
| Load Balance | 5 | 1 | **75%** | Distribution, rebalancing |
| XDP | 6 | 1 | **40%** | Frame processing (no kernel tests) |
| Dashboard | 3 | 0 | **50%** | Rendering only |
| **Total** | **50** | **17** | **75%** | Production-ready for core paths |

---

## Running Tests

### Quick Development Loop (< 5 seconds)
```bash
# Unit tests only (no sleep, fast)
cargo test --lib

# Watch mode (requires cargo-watch)
cargo watch -x "test --lib"
```

### Full Test Suite (including integration tests)
```bash
# All tests with output
cargo test --workspace -- --nocapture

# All tests, verbose
cargo test --workspace --verbose

# Test documentation examples
cargo test --doc
```

### Specific Module Tests
```bash
# Ring buffer tests
cargo test ring_buffer

# Protocol tests
cargo test protocol

# Network failure tests
cargo test network_failure
cargo test protocol_handles_truncated

# Node failure cascade tests
cargo test cluster_handles

# Only ignore slow/special tests
cargo test -- --skip heartbeat_timeout
```

### Test Categories

**Fast tests** (< 100ms, runs in CI):
```bash
cargo test --lib
```

**Slow tests** (> 100ms, marked with `#[ignore]`):
```bash
cargo test -- --ignored --test-threads=1
```

**Property-based tests** (proptest):
```bash
cargo test proptest_protocol
```

**Benchmarks**:
```bash
# Run all criterion benchmarks
cargo bench --package ghostlink-core --bench criterion

# Run specific benchmark
cargo bench --package ghostlink-core --bench criterion -- ring_buffer

# Compare to baseline
cargo bench --package ghostlink-core --bench criterion -- --baseline stable
```

---

## Coverage Reporting

### Generate HTML Coverage Report
```bash
# Install tarpaulin (one-time)
cargo install cargo-tarpaulin

# Generate coverage (outputs to coverage/index.html)
cargo tarpaulin --workspace --out Html --output-dir coverage
```

### View Coverage in Terminal
```bash
cargo tarpaulin --workspace --out Stdout
```

### Expected Coverage

**Current** (after improvements):
- Ring Buffer: 95%
- Protocol: 85%
- Cluster: 85%
- Health: 70%
- Planning: 80%
- Overall: **75%**

**Target for 1.0 Release**: 80%+ statement coverage, 50%+ branch coverage

---

## Test Architecture

### Unit Tests (In-Module)
Located in `#[cfg(test)] mod tests { ... }` blocks within each source file.

**Purpose**: Test individual functions/types in isolation.

**Example**:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_push_pop_fifo() {
        let ring = SpscRingBuffer::<i32>::new(RingConfig::default());
        ring.push(1).unwrap();
        ring.push(2).unwrap();
        assert_eq!(ring.pop(), Some(1));
        assert_eq!(ring.pop(), Some(2));
    }
}
```

### Integration Tests (tests/ Directory)
Located in `tests/integration.rs` and test multiple components together.

**Purpose**: Verify end-to-end workflows and failure scenarios.

**Example**:
```rust
#[test]
fn multi_node_discovery_and_layer_assignment() {
    let cluster = ClusterState::new();
    cluster.register(NodeResources::new("node-a", 24.0, 64.0, "8.9", None));
    cluster.register(NodeResources::new("node-b", 24.0, 64.0, "8.9", None));
    
    let layers = common::sample_layers(33, 1.0);
    let assignments = assign_layers_sequentially(&nodes, &layers).unwrap();
    
    assert_eq!(assignments.len(), 2);
    assert_eq!(assignments[0].end_layer, 24);
}
```

### Test Utilities (tests/common.rs)
Shared functions used by multiple tests:
- `sample_layers()` - Generate test layer specs
- `sample_nodes()` - Generate heterogeneous test nodes
- `setup_cluster()` - Create pre-populated cluster
- `assert_cluster_state()` - Verify cluster properties
- `corrupt_frame_bits()` - Simulate network corruption

**Usage**:
```rust
use crate::common;

#[test]
fn my_test() {
    let cluster = common::setup_cluster(5, 24.0);
    common::assert_cluster_state(&cluster, 5, 240.0);
}
```

---

## Key Test Scenarios

### Network Failures (Protocol & Ring Buffer)
- ✅ Truncated frames (network drop mid-packet)
- ✅ Single-bit corruption (CRC detection)
- ✅ Multi-byte corruption (payload changes)
- ✅ Frame recovery (valid frame after corruption)
- ✅ Ring buffer backpressure under producer/consumer rate mismatch
- ⚠️ Out-of-order delivery (roadmap: v0.2)
- ⚠️ Timeout/retry (roadmap: v0.2)

### Node Failures (Cluster & Planning)
- ✅ Single node failure (deregistration)
- ✅ Concurrent node failures (quorum retained)
- ✅ Node rejoin after failure (resource updates)
- ✅ Rapid registration churn (stability under load)
- ⚠️ Split-brain detection (roadmap: v0.2)
- ⚠️ Cascade failure handling (roadmap: v0.2)

### Health Monitoring (Metrics & Degradation)
- ✅ EMA (exponential moving average) latency tracking
- ✅ Delivery ratio monitoring
- ✅ Min/max latency tracking
- ⚠️ Adaptive quantization triggers (in progress)
- ⚠️ Real latency injection (roadmap: v0.2)

### Load Balancing & Rebalancing
- ✅ Greedy layer assignment across heterogeneous nodes
- ✅ Capacity overflow detection
- ✅ Layer placement accuracy
- ⚠️ Dynamic rebalancing (roadmap: v0.2)
- ⚠️ Tensor migration without loss (roadmap: v0.2)

---

## Known Test Limitations

### AF_XDP/eBPF Integration ⚠️
- **Status**: Unit tests only (no kernel socket)
- **Why**: Requires Linux kernel, special capabilities
- **Workaround**: Mocked frame processing, protocol testing
- **Roadmap**: Real AF_XDP tests in v0.2

### macOS/Windows Testing ⚠️
- **Status**: Uses standard socket fallback (untested on those platforms)
- **Why**: Platform-specific socket APIs differ
- **Workaround**: Cross-platform CI tests socket fallback path
- **Roadmap**: Native macOS/Windows testing in v0.2

### Health Monitoring ⚠️
- **Status**: Uses synthetic data (no real network probes)
- **Why**: Difficult to simulate latency/jitter reliably
- **Workaround**: Mocked latency values, threshold-based tests
- **Roadmap**: Real latency injection in v0.2

### Chaos Engineering ⚠️
- **Status**: No failure injection framework
- **Why**: Requires distributed system test harness
- **Workaround**: Manual failure scenario tests
- **Roadmap**: `fail` crate integration in v0.2

---

## Contributing Tests

### Before Adding a Test

1. **Check if similar test exists** - Use `common` utilities if available
2. **Consider test scope** - Unit vs integration vs property-based?
3. **Avoid sleep()** - Fast tests required for CI (no `thread::sleep()` > 10ms)
4. **Document purpose** - Add comment explaining what's being tested

### Test Checklist

```markdown
- [ ] Test has descriptive name (`test_<component>_<behavior>`)
- [ ] Test has doc comment explaining scenario
- [ ] Fast path tests complete < 100ms
- [ ] No `unwrap()` in test code (use `expect()` with message)
- [ ] Uses `common` utilities where possible
- [ ] Assertions have descriptive messages
- [ ] Property tests use proptest correctly
- [ ] No external dependencies (network calls, files)
```

### Example: Adding a New Test

**File**: `crates/ghostlink-core/src/ring.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Verify ring buffer doesn't lose data when consumer temporarily stalls
    #[test]
    fn ring_buffer_handles_consumer_stall() {
        let ring = Arc::new(SpscRingBuffer::<u32>::new(RingConfig::default()));
        let prod = Arc::clone(&ring);
        let cons = Arc::clone(&ring);

        let producer = thread::spawn(move || {
            for i in 0..1000 {
                loop {
                    if prod.push(i).is_ok() {
                        break;
                    }
                    std::thread::yield_now();
                }
            }
        });

        let consumer = thread::spawn(move || {
            let mut values = Vec::new();
            let mut stalled = false;
            
            while values.len() < 1000 {
                if let Some(val) = cons.pop() {
                    values.push(val);
                    
                    // Simulate stall at midpoint
                    if values.len() == 500 && !stalled {
                        std::thread::sleep(Duration::from_millis(10));
                        stalled = true;
                    }
                } else {
                    std::thread::yield_now();
                }
            }
            values
        });

        producer.join().unwrap();
        let values = consumer.join().unwrap();

        assert_eq!(values.len(), 1000, "No data should be lost during stall");
        assert_eq!(values.first(), Some(&0), "FIFO order maintained");
    }
}
```

---

## Performance Regression Prevention

### Benchmark Baselines
```bash
# Create baseline (first run)
cargo bench --package ghostlink-core --bench criterion -- --save-baseline main

# Compare against baseline (subsequent runs)
cargo bench --package ghostlink-core --bench criterion -- --baseline main
```

### Performance Alerts
Tests will fail if performance degrades by > 5% from baseline for:
- Ring buffer push/pop: target < 5ns
- Protocol encode: target < 120ns  
- Protocol decode: target < 120ns
- Cluster register: target < 200ns

---

## CI/CD Integration

### GitHub Actions Workflow
The `.github/workflows/ci.yml` runs:

1. **Format Check** - `cargo fmt --all --check`
2. **Lint** - `cargo clippy --all-targets -- -D warnings`
3. **Test Matrix** - Linux, macOS, Windows (all test --workspace)
4. **Coverage** - tarpaulin + Codecov upload
5. **Benchmarks** - Criterion comparison against baseline

### Local CI Simulation
```bash
# Run everything CI does locally
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --verbose
cargo bench --package ghostlink-core --bench criterion
cargo tarpaulin --workspace --out Stdout
```

---

## Troubleshooting

### Test Hangs
- Check for `thread::sleep()` without timeout
- Look for deadlocks in Arc/Mutex usage
- Use `RUST_BACKTRACE=1 cargo test -- --nocapture --test-threads=1`

### Flaky Tests
- Tests with timing assumptions (< 100% reliable)
- Solution: Use deterministic mocks instead of real time
- Flag with `#[ignore]` if timing is essential

### Coverage Gaps
- Check `.github/workflows/ci.yml` coverage report
- Focus on error paths and branches (`if/else`, `match`)
- Property tests help cover more input space

---

## Resources

- [Rust Testing Book](https://doc.rust-lang.org/book/ch11-00-testing.html)
- [Criterion.rs Guide](https://bheisler.github.io/criterion.rs/book/)
- [Proptest Book](https://docs.rs/proptest/latest/proptest/)
- [Test Organization](https://doc.rust-lang.org/rust-by-example/testing.html)

---

**Questions?** See [CONTRIBUTING.md](../CONTRIBUTING.md) or open an issue.
