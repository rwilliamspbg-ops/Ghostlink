//! Circuit Breaker Pattern for Resilient Network Communication
//!
//! Implements the circuit breaker pattern to prevent cascading failures when
//! communicating with remote nodes. Transitions through three states:
//! - Closed: Normal operation, requests pass through
//! - Open: Failures detected, requests fail fast without attempting connection
//! - Half-Open: Recovery probe window, allows single connection attempt

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Circuit breaker states
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation - requests pass through
    Closed = 0,
    /// Failure threshold exceeded - fail-fast without retrying
    Open = 1,
    /// Recovery probing - allows single request attempt
    HalfOpen = 2,
}

impl CircuitState {
    fn from_u8(val: u8) -> Self {
        match val {
            0 => CircuitState::Closed,
            1 => CircuitState::Open,
            2 => CircuitState::HalfOpen,
            _ => CircuitState::Closed,
        }
    }
}

/// Per-node circuit breaker for detecting and responding to failures
#[derive(Debug)]
pub struct CircuitBreaker {
    /// Current state (0=closed, 1=open, 2=half-open)
    state: Arc<AtomicU8>,
    /// Count of consecutive failures
    failures: Arc<AtomicUsize>,
    /// Timestamp of last failure
    last_failure: Arc<Mutex<Instant>>,
    /// Set while a half-open recovery probe is in flight, so only one caller
    /// gets to probe per recovery window instead of every concurrent caller
    /// piling onto the just-recovered node at once.
    half_open_probe_in_flight: Arc<AtomicBool>,
    /// Configuration
    config: Arc<CircuitBreakerConfig>,
}

/// Configuration for circuit breaker behavior
#[derive(Clone, Debug)]
pub struct CircuitBreakerConfig {
    /// Number of failures before opening circuit
    pub failure_threshold: usize,
    /// Time to wait in open state before transitioning to half-open
    pub open_duration_secs: u64,
    /// Time window for resetting failure counter (after successful request)
    pub reset_timeout_secs: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            open_duration_secs: 30,
            reset_timeout_secs: 60,
        }
    }
}

impl CircuitBreaker {
    /// Create a new circuit breaker with default configuration
    pub fn new() -> Self {
        Self::with_config(CircuitBreakerConfig::default())
    }

    /// Create a circuit breaker with custom configuration
    pub fn with_config(config: CircuitBreakerConfig) -> Self {
        Self {
            state: Arc::new(AtomicU8::new(0)), // Closed
            failures: Arc::new(AtomicUsize::new(0)),
            last_failure: Arc::new(Mutex::new(Instant::now())),
            half_open_probe_in_flight: Arc::new(AtomicBool::new(false)),
            config: Arc::new(config),
        }
    }

    /// Get current circuit state
    pub fn current_state(&self) -> CircuitState {
        let state_byte = self.state.load(Ordering::Relaxed);
        CircuitState::from_u8(state_byte)
    }

    /// Check if the circuit allows a request attempt.
    ///
    /// At most one caller gets `true` per half-open recovery window: a
    /// caller must win the `half_open_probe_in_flight` CAS *before* the
    /// `Open -> HalfOpen` state transition becomes visible, so a caller
    /// that observes `HalfOpen` can never race in against a flag that
    /// hasn't been claimed yet.
    pub fn should_attempt(&self) -> bool {
        match self.current_state() {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if it's time to transition to half-open
                let last_failure = *self.last_failure.lock().unwrap();
                let elapsed = last_failure.elapsed();

                if elapsed <= Duration::from_secs(self.config.open_duration_secs) {
                    return false;
                }

                // Claim the probe slot before flipping the externally
                // visible state. If we flipped state first, a thread that
                // observes HalfOpen before we store the flag could win the
                // flag CAS itself, granting two probes for one window.
                if self
                    .half_open_probe_in_flight
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
                    .is_err()
                {
                    return false;
                }

                if self
                    .state
                    .compare_exchange(1, 2, Ordering::AcqRel, Ordering::Relaxed)
                    .is_ok()
                {
                    true
                } else {
                    // State changed underneath us (e.g. a concurrent reset)
                    // before we could complete the transition; release the
                    // probe slot we speculatively claimed.
                    self.half_open_probe_in_flight
                        .store(false, Ordering::Release);
                    false
                }
            }
            CircuitState::HalfOpen => self
                .half_open_probe_in_flight
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok(),
        }
    }

    /// Record a successful request - reset failure counter
    pub fn record_success(&self) {
        // Reset only if enough time has passed (reset_timeout_secs)
        let last_failure = *self.last_failure.lock().unwrap();
        if last_failure.elapsed() > Duration::from_secs(self.config.reset_timeout_secs) {
            self.failures.store(0, Ordering::Relaxed);
            self.state.store(0, Ordering::Relaxed); // Transition to Closed
            self.half_open_probe_in_flight
                .store(false, Ordering::Release);
        } else if self.current_state() == CircuitState::HalfOpen {
            // Transition from half-open to closed on successful probe
            self.failures.store(0, Ordering::Relaxed);
            self.state.store(0, Ordering::Relaxed);
            self.half_open_probe_in_flight
                .store(false, Ordering::Release);
        }
    }

    /// Record a failed request - increment failure counter
    pub fn record_failure(&self) {
        let failure_count = self.failures.fetch_add(1, Ordering::Relaxed) + 1;
        *self.last_failure.lock().unwrap() = Instant::now();

        // A failed half-open probe reopens the circuit immediately, same as
        // crossing the failure threshold from closed.
        if failure_count >= self.config.failure_threshold
            || self.current_state() == CircuitState::HalfOpen
        {
            self.state.store(1, Ordering::Relaxed); // Open
            self.half_open_probe_in_flight
                .store(false, Ordering::Release);
        }
    }

    /// Get failure count
    pub fn failure_count(&self) -> usize {
        self.failures.load(Ordering::Relaxed)
    }

    /// Get time since last failure
    pub fn time_since_last_failure(&self) -> Duration {
        self.last_failure.lock().unwrap().elapsed()
    }

    /// Reset the circuit breaker to closed state
    pub fn reset(&self) {
        self.state.store(0, Ordering::Relaxed);
        self.failures.store(0, Ordering::Relaxed);
        self.half_open_probe_in_flight
            .store(false, Ordering::Release);
        *self.last_failure.lock().unwrap() = Instant::now();
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for CircuitBreaker {
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
            failures: Arc::clone(&self.failures),
            last_failure: Arc::clone(&self.last_failure),
            half_open_probe_in_flight: Arc::clone(&self.half_open_probe_in_flight),
            config: Arc::clone(&self.config),
        }
    }
}

/// Jittered exponential backoff for retry delays
pub struct JitteredBackoff {
    attempt: u32,
    max_backoff_ms: u64,
}

impl JitteredBackoff {
    /// Create a new jittered backoff starting at attempt 0
    pub fn new() -> Self {
        Self {
            attempt: 0,
            max_backoff_ms: 30_000, // 30 seconds max
        }
    }

    /// With custom max backoff duration
    pub fn with_max_backoff(max_backoff_ms: u64) -> Self {
        Self {
            attempt: 0,
            max_backoff_ms,
        }
    }

    /// Calculate backoff duration for current attempt (with jitter)
    pub fn backoff_duration(&self) -> Duration {
        // Base exponential: 100 * 2^attempt, capped at max_backoff_ms
        let base_ms = (100u64 * 2_u64.pow(self.attempt.min(10))).min(self.max_backoff_ms);

        // Add jitter: 0–10% of base
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let jitter_ms = rng.gen_range(0..=(base_ms / 10));

        Duration::from_millis(base_ms + jitter_ms)
    }

    /// Advance to next attempt, return backoff duration
    pub fn next_backoff(&mut self) -> Duration {
        let duration = self.backoff_duration();
        self.attempt += 1;
        duration
    }

    /// Reset to attempt 0
    pub fn reset(&mut self) {
        self.attempt = 0;
    }
}

impl Default for JitteredBackoff {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circuit_starts_closed() {
        let cb = CircuitBreaker::new();
        assert_eq!(cb.current_state(), CircuitState::Closed);
        assert!(cb.should_attempt());
    }

    #[test]
    fn circuit_opens_after_threshold_failures() {
        let cb = CircuitBreaker::new();
        for _ in 0..5 {
            cb.record_failure();
        }
        assert_eq!(cb.current_state(), CircuitState::Open);
        assert!(!cb.should_attempt());
    }

    #[test]
    fn circuit_half_opens_after_timeout() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            open_duration_secs: 0,
            reset_timeout_secs: 60,
        };
        let cb = CircuitBreaker::with_config(config);
        cb.record_failure();
        assert_eq!(cb.current_state(), CircuitState::Open);

        std::thread::sleep(Duration::from_millis(100));
        assert!(cb.should_attempt());
        assert_eq!(cb.current_state(), CircuitState::HalfOpen);
    }

    #[test]
    fn circuit_closes_on_successful_probe() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            open_duration_secs: 0,
            reset_timeout_secs: 0,
        };
        let cb = CircuitBreaker::with_config(config);
        cb.record_failure();
        assert_eq!(cb.current_state(), CircuitState::Open);

        // Trigger transition to half-open
        std::thread::sleep(Duration::from_millis(100));
        let can_attempt = cb.should_attempt();
        assert!(can_attempt);
        assert_eq!(cb.current_state(), CircuitState::HalfOpen);

        // Record success and verify transition to closed
        cb.record_success();
        assert_eq!(cb.current_state(), CircuitState::Closed);
        assert_eq!(cb.failure_count(), 0);
    }

    #[test]
    fn jittered_backoff_increases() {
        let mut backoff = JitteredBackoff::new();
        let backoff1 = backoff.next_backoff();
        let backoff2 = backoff.next_backoff();
        let backoff3 = backoff.next_backoff();

        assert!(backoff1.as_millis() >= 100 && backoff1.as_millis() <= 220);
        assert!(backoff2.as_millis() >= 200 && backoff2.as_millis() <= 440);
        assert!(backoff3.as_millis() >= 400 && backoff3.as_millis() <= 880);
    }

    #[test]
    fn jittered_backoff_has_jitter() {
        let mut backoffs = Vec::new();
        for _ in 0..10 {
            let backoff = JitteredBackoff::new().next_backoff();
            backoffs.push(backoff.as_millis());
        }

        let min = *backoffs.iter().min().unwrap();
        let max = *backoffs.iter().max().unwrap();
        assert!(max > min, "Jitter should produce different values");
    }

    #[test]
    fn half_open_grants_probe_to_exactly_one_concurrent_caller() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            open_duration_secs: 0,
            reset_timeout_secs: 60,
        };
        let cb = CircuitBreaker::with_config(config);
        cb.record_failure();
        assert_eq!(cb.current_state(), CircuitState::Open);
        std::thread::sleep(Duration::from_millis(50));

        let granted = Arc::new(AtomicUsize::new(0));
        let handles: Vec<_> = (0..16)
            .map(|_| {
                let cb = cb.clone();
                let granted = Arc::clone(&granted);
                std::thread::spawn(move || {
                    if cb.should_attempt() {
                        granted.fetch_add(1, Ordering::Relaxed);
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(
            granted.load(Ordering::Relaxed),
            1,
            "exactly one concurrent caller should win the half-open probe slot"
        );
        assert_eq!(cb.current_state(), CircuitState::HalfOpen);
    }

    #[test]
    fn circuit_breaker_is_cloneable() {
        let cb1 = CircuitBreaker::new();
        cb1.record_failure();
        let cb2 = cb1.clone();

        assert_eq!(cb2.failure_count(), 1);
        cb2.record_failure();
        assert_eq!(cb1.failure_count(), 2);
    }
}
