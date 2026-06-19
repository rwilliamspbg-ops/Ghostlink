# Contributing to Ghost-Link

Thank you for your interest in contributing to Ghost-Link! This document provides guidelines for contributing to the project.

## Getting Started

### Prerequisites

- Rust 1.70+ installed (`rustup install stable`)
- Cargo (comes with Rust)
- Git

### Building from Source

```bash
# Clone the repository
git clone https://github.com/your-org/ghostlink.git
cd ghostlink

# Build the workspace
cargo build --workspace

# Run tests
cargo test --workspace
```

## Code Style

### Rustfmt

We use `rustfmt` for code formatting. Format your code before submitting:

```bash
cargo fmt
```

### Clippy

Run clippy to catch common mistakes and style issues:

```bash
cargo clippy --workspace -- -D warnings
```

### Documentation

All public types, functions, and modules should have documentation comments:

```rust
/// This is a module-level comment explaining the module's purpose.
pub mod example;

/// A public struct with documentation.
#[derive(Debug)]
pub struct Example {
    /// Field documentation.
    pub field: String,
}

impl Example {
    /// Constructor function with documentation.
    pub fn new(value: i32) -> Self {
        // Implementation
    }
}
```

## Testing

### Unit Tests

Write unit tests for individual components:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_example_function() {
        assert_eq!(example_function(42), 84);
    }
}
```

Run tests with:

```bash
cargo test --package ghostlink-core
```

### Integration Tests

Integration tests are in the `tests/` directory. They test end-to-end functionality:

```bash
cargo test --test integration
```

### Test Coverage

Aim for at least 80% code coverage on core modules:

```bash
cargo tarpaulin --workspace
```

## Pull Request Process

1. **Fork the repository** and create a new branch:

   ```bash
   git checkout -b feature/your-feature-name
   ```

2. **Make your changes** following the code style guidelines above.

3. **Write tests** for new functionality or edge cases.

4. **Update documentation** if necessary.

5. **Run all checks:**

   ```bash
   cargo fmt
   cargo clippy --workspace -- -D warnings
   cargo test --workspace
   ```

6. **Commit your changes** with clear, descriptive messages:

   ```bash
   git commit -m "feat: add zero-copy ring buffer for AF_XDP integration"
   ```

7. **Push and create a pull request:**

   ```bash
   git push origin feature/your-feature-name
   ```

## Code Review

All pull requests will be reviewed by maintainers. The review process includes:

- Code correctness and safety
- Performance considerations
- Documentation quality
- Test coverage
- Adherence to project conventions

## Areas for Contribution

### High Priority

1. **AF_XDP/eBPF Integration** - Linux-specific raw socket handling
2. **Network Health Monitoring** - Ping/pong latency tracking
3. **Load Balancing** - Tensor distribution across nodes
4. **Terminal UI** - ratatui integration for production dashboard

### Medium Priority

1. **Documentation** - More examples and use cases
2. **Testing** - Additional edge case tests
3. **Error Handling** - Better error messages and recovery

### Nice to Have

1. **Benchmarks** - Performance benchmarking suite
2. **Examples** - Real-world usage scenarios
3. **CI/CD** - Continuous integration improvements

## Reporting Issues

When reporting a bug, please include:

- Clear description of the problem
- Steps to reproduce
- Expected vs actual behavior
- Environment details (OS, Rust version, dependencies)
- Error messages and stack traces

Example issue report:

```markdown
### Bug Report

**Description:** Ring buffer overflows when producer outpaces consumer

**Steps to Reproduce:**
1. Create ring buffer with default config
2. Run producer loop without backpressure handling
3. Fill buffer beyond capacity

**Expected Behavior:** Producer should wait for space

**Actual Behavior:** Panic with "Ring buffer overflow"

**Environment:**
- OS: Windows 11
- Rust: 1.75.0
- ghostlink-core: 0.1.0
```

## Code of Conduct

Please be respectful and considerate in all interactions. We aim to provide a welcoming environment for contributors from all backgrounds.

## License

By contributing to Ghost-Link, you agree that your contributions will be licensed under the MIT License.

## Questions?

Feel free to open an issue or discussion for any questions about contributing!

Thank you for helping make Ghost-Link better! 🚀
