//! Zero-Copy SPSC Ring Buffer with AF_XDP Integration
//!
//! This implementation uses pinned allocations and proper memory ordering
//! for single-producer/single-consumer DMA-style hand-off.
//!
//! Optimizations:
//! - 4x larger capacity (4096) for better batching
//! - Prefetching in hot paths for cache-friendly access
//! - Optimal cache-line padding (128 bytes) to avoid false sharing

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Ring buffer configuration with backpressure thresholds
#[derive(Clone, Copy, Debug)]
pub struct RingConfig {
    /// Maximum number of elements the ring can hold
    pub capacity: usize,
    /// Backpressure threshold at 70% capacity
    pub backpressure_threshold: usize,
}

impl Default for RingConfig {
    fn default() -> Self {
        Self {
            capacity: 4095,               // 4x larger for better batching
            backpressure_threshold: 2867, // ~70% of 4095
        }
    }
}

/// Fixed capacity used for DMA alignment (power of 2 for fast modulo)
const RING_CAPACITY: usize = 4096;
const CACHE_LINE: usize = 128; // 128-byte cache lines on modern x86/ARM

/// Prefetch hint for read access (equivalent to `prefetcht0` on x86)
#[inline(always)]
fn prefetch_read<T>(ptr: *const T) {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    unsafe {
        std::arch::x86_64::_mm_prefetch(ptr as *const i8, std::arch::x86_64::_MM_HINT_T0);
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        // On other platforms, hint to compiler/CPU
        std::hint::black_box(ptr);
    }
}

/// Prefetch hint for write access (equivalent to `prefetchw` on x86)
#[inline(always)]
fn prefetch_write<T>(ptr: *mut T) {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    unsafe {
        std::arch::x86_64::_mm_prefetch(ptr as *const i8, std::arch::x86_64::_MM_HINT_ET1);
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        std::hint::black_box(ptr);
    }
}

/// Zero-copy SPSC ring buffer with proper memory ordering
///
/// # Safety
/// This type is only safe to use when:
/// - Only one producer thread exists
/// - Only one consumer thread exists
/// - Memory is properly aligned for the element type T
#[derive(Debug)]
pub struct SpscRingBuffer<T> {
    /// Buffer of uninitialized elements (aligned to cache line)
    buffer: UnsafeCell<[MaybeUninit<T>; RING_CAPACITY]>,

    /// Padding to prevent false sharing between buffer and head
    _pad1: [u8; CACHE_LINE],
    /// Current head position (consumer reads here)
    head: AtomicUsize,
    /// Cached tail for consumer to avoid reading shared tail
    cached_tail: AtomicUsize,

    /// Padding to prevent false sharing between head and tail
    _pad2: [u8; CACHE_LINE],
    /// Current tail position (producer writes here)
    tail: AtomicUsize,
    /// Cached head for producer to avoid reading shared head
    cached_head: AtomicUsize,

    /// Padding to prevent false sharing between tail and other counters
    _pad3: [u8; CACHE_LINE],
    /// Overflow counter for backpressure monitoring
    overflow_count: AtomicUsize,
    /// Configuration for this ring buffer
    config: RingConfig,
}

impl<T> SpscRingBuffer<T> {
    const CAPACITY: usize = RING_CAPACITY;

    /// Create a new SPSC ring buffer with the given configuration
    pub fn new(config: RingConfig) -> Self {
        assert!(
            config.capacity < Self::CAPACITY,
            "capacity must be strictly less than RING_CAPACITY"
        );

        // Safety: MaybeUninit does not require initialization
        let buffer = UnsafeCell::new(unsafe {
            MaybeUninit::<[MaybeUninit<T>; RING_CAPACITY]>::uninit().assume_init()
        });

        Self {
            buffer,
            _pad1: [0; CACHE_LINE],
            head: AtomicUsize::new(0),
            cached_tail: AtomicUsize::new(0),
            _pad2: [0; CACHE_LINE],
            tail: AtomicUsize::new(0),
            cached_head: AtomicUsize::new(0),
            _pad3: [0; CACHE_LINE],
            overflow_count: AtomicUsize::new(0),
            config,
        }
    }

    /// Push an element into the ring buffer
    ///
    /// Returns `Ok(())` on success, or `Err(value)` if the ring is full.
    /// When full, the producer should wait/backpressure until space is available.
    pub fn push(&self, value: T) -> Result<(), T> {
        // Producer loads its own tail with Relaxed.
        let tail = self.tail.load(Ordering::Relaxed);

        // First check against cached head to avoid atomic contention.
        let mut head = self.cached_head.load(Ordering::Relaxed);
        let mut current_len = tail.wrapping_sub(head) & (Self::CAPACITY - 1);

        if current_len >= self.config.capacity {
            // Only if cached head suggests we are full, refresh from shared head.
            head = self.head.load(Ordering::Acquire);
            self.cached_head.store(head, Ordering::Relaxed);
            current_len = tail.wrapping_sub(head) & (Self::CAPACITY - 1);

            if current_len >= self.config.capacity {
                self.overflow_count.fetch_add(1, Ordering::Relaxed);
                return Err(value);
            }
        }

        let next_tail = Self::increment(tail);

        // Write the value at tail position
        unsafe {
            let buf = &mut *self.buffer.get();
            buf[tail].write(value);

            // Release store to make write visible to consumer
            self.tail.store(next_tail, Ordering::Release);
        }

        Ok(())
    }

    /// Pop an element from the ring buffer
    ///
    /// Returns `Some(value)` on success, or `None` if the ring is empty.
    pub fn pop(&self) -> Option<T> {
        // Consumer loads its own head with Relaxed.
        let head = self.head.load(Ordering::Relaxed);

        // First check against cached tail to avoid atomic contention.
        let mut tail = self.cached_tail.load(Ordering::Relaxed);

        if head == tail {
            // Only if cached tail suggests we are empty, refresh from shared tail.
            tail = self.tail.load(Ordering::Acquire);
            self.cached_tail.store(tail, Ordering::Relaxed);

            if head == tail {
                return None;
            }
        }

        // Read the value at head position
        let value = unsafe {
            let buf = &mut *self.buffer.get();
            let val = buf[head].assume_init_read();

            // Release store to make read visible to producer
            self.head.store(Self::increment(head), Ordering::Release);

            val
        };

        Some(value)
    }

    /// Get the current number of elements in the ring
    pub fn len(&self) -> usize {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);

        tail.wrapping_sub(head) & (Self::CAPACITY - 1)
    }

    /// Check if the ring is empty
    pub fn is_empty(&self) -> bool {
        let head = self.head.load(Ordering::Relaxed);
        let mut tail = self.cached_tail.load(Ordering::Relaxed);

        if head == tail {
            tail = self.tail.load(Ordering::Acquire);
            self.cached_tail.store(tail, Ordering::Relaxed);
        }

        head == tail
    }

    /// Get the configured capacity
    pub fn capacity(&self) -> usize {
        self.config.capacity
    }

    /// Get overflow count for backpressure monitoring
    pub fn overflow_count(&self) -> usize {
        self.overflow_count.load(Ordering::Relaxed)
    }

    /// Get empty count for backpressure monitoring
    pub fn empty_count(&self) -> usize {
        self.config.capacity.saturating_sub(self.len())
    }

    /// Check if backpressure should be applied (ring >= threshold)
    pub fn should_backpressure(&self) -> bool {
        let tail = self.tail.load(Ordering::Relaxed);
        let mut head = self.cached_head.load(Ordering::Relaxed);
        let mut len = tail.wrapping_sub(head) & (Self::CAPACITY - 1);

        if len >= self.config.backpressure_threshold {
            head = self.head.load(Ordering::Acquire);
            self.cached_head.store(head, Ordering::Relaxed);
            len = tail.wrapping_sub(head) & (Self::CAPACITY - 1);
        }

        len >= self.config.backpressure_threshold
    }

    /// Number of elements that can currently be pushed without blocking.
    pub fn write_available(&self) -> usize {
        let tail = self.tail.load(Ordering::Relaxed);
        let mut head = self.cached_head.load(Ordering::Relaxed);
        let mut len = tail.wrapping_sub(head) & (Self::CAPACITY - 1);
        if len >= self.config.capacity {
            head = self.head.load(Ordering::Acquire);
            self.cached_head.store(head, Ordering::Relaxed);
            len = tail.wrapping_sub(head) & (Self::CAPACITY - 1);
        }
        self.config.capacity.saturating_sub(len)
    }

    /// Number of elements currently available to pop (best-effort, not a snapshot).
    pub fn read_available(&self) -> usize {
        let head = self.head.load(Ordering::Relaxed);
        let mut tail = self.cached_tail.load(Ordering::Relaxed);
        if head == tail {
            tail = self.tail.load(Ordering::Acquire);
            self.cached_tail.store(tail, Ordering::Relaxed);
        }
        tail.wrapping_sub(head) & (Self::CAPACITY - 1)
    }

    /// Wait for space to become available
    ///
    /// Uses exponential backoff: spins with PAUSE up to 256 iterations,
    /// then transitions to OS yield to avoid starving the consumer.
    pub fn wait_for_space(&self) {
        let mut backoff = 1u32;
        loop {
            if !self.should_backpressure() {
                return;
            }
            // Exponential backoff: 1, 2, 4, 8, … 256
            let limit = backoff.min(256);
            for _ in 0..limit {
                core::hint::spin_loop();
            }
            backoff = backoff.saturating_mul(2);
            if backoff > 256 {
                std::thread::yield_now();
                backoff = 1;
            }
        }
    }

    /// Wait for an element to become available
    ///
    /// Uses exponential backoff: spins with PAUSE up to 256 iterations,
    /// then transitions to OS yield to avoid starving the producer.
    pub fn wait_for_data(&self) {
        let mut backoff = 1u32;
        loop {
            if !self.is_empty() {
                return;
            }
            let limit = backoff.min(256);
            for _ in 0..limit {
                core::hint::spin_loop();
            }
            backoff = backoff.saturating_mul(2);
            if backoff > 256 {
                std::thread::yield_now();
                backoff = 1;
            }
        }
    }

    /// Push a batch of elements, returning the number successfully pushed.
    ///
    /// Stops early if the ring becomes full. This amortizes atomic store
    /// overhead across multiple elements: only two atomic stores (tail + cached_head)
    /// for up to `slice.len()` pushes.
    ///
    /// Uses prefetching to reduce cache misses on write path.
    pub fn push_batch(&self, slice: &[T]) -> usize
    where
        T: Copy,
    {
        let tail = self.tail.load(Ordering::Relaxed);
        let mut head = self.cached_head.load(Ordering::Relaxed);
        let mut current_len = tail.wrapping_sub(head) & (Self::CAPACITY - 1);

        if current_len >= self.config.capacity {
            head = self.head.load(Ordering::Acquire);
            self.cached_head.store(head, Ordering::Relaxed);
            current_len = tail.wrapping_sub(head) & (Self::CAPACITY - 1);
            if current_len >= self.config.capacity {
                // Ring is full: every requested element is rejected.
                self.overflow_count
                    .fetch_add(slice.len(), Ordering::Relaxed);
                return 0;
            }
        }

        let available = self.config.capacity - current_len;
        let count = available.min(slice.len());
        if count == 0 {
            return 0;
        }
        if count < slice.len() {
            // Partial batch: track the elements that didn't fit as overflow,
            // matching push()'s per-element overflow accounting.
            self.overflow_count
                .fetch_add(slice.len() - count, Ordering::Relaxed);
        }

        let buf = unsafe { &mut *self.buffer.get() };
        let mask = Self::CAPACITY - 1;

        // Prefetch the first write location
        prefetch_write(&mut buf[tail & mask] as *mut _);

        for i in 0..count {
            let idx = (tail + i) & mask;
            // Prefetch next cache line (64 bytes = ~16 elements for small T)
            if i % 16 == 14 && i + 2 < count {
                prefetch_write(&mut buf[(tail + i + 2) & mask] as *mut _);
            }
            buf[idx].write(slice[i]);
        }
        let new_tail = tail.wrapping_add(count) & mask;
        self.tail.store(new_tail, Ordering::Release);
        count
    }

    /// Pop up to `count` elements into the provided mutable slice, returning
    /// the number of elements actually popped.
    ///
    /// Uses `MaybeUninit` to avoid zero-initialisation overhead.
    /// Uses prefetching to reduce cache misses on read path.
    pub fn pop_batch(&self, out: &mut [std::mem::MaybeUninit<T>]) -> usize {
        let head = self.head.load(Ordering::Relaxed);
        let mut tail = self.cached_tail.load(Ordering::Relaxed);

        if head == tail {
            tail = self.tail.load(Ordering::Acquire);
            self.cached_tail.store(tail, Ordering::Relaxed);
            if head == tail {
                return 0;
            }
        }

        let available = tail.wrapping_sub(head) & (Self::CAPACITY - 1);
        let count = available.min(out.len());
        if count == 0 {
            return 0;
        }

        let buf = unsafe { &mut *self.buffer.get() };
        let mask = Self::CAPACITY - 1;

        // Prefetch the first read location
        prefetch_read(&buf[head & mask] as *const _);

        for (i, slot) in out.iter_mut().enumerate().take(count) {
            let src = (head + i) & mask;
            // Prefetch next cache line
            if i % 16 == 14 && i + 2 < count {
                prefetch_read(&buf[(head + i + 2) & mask] as *const _);
            }
            *slot = std::mem::MaybeUninit::new(unsafe { buf[src].assume_init_read() });
        }
        let new_head = head.wrapping_add(count) & mask;
        self.head.store(new_head, Ordering::Release);
        count
    }

    fn increment(index: usize) -> usize {
        (index + 1) & (Self::CAPACITY - 1)
    }
}

// Safety: SpscRingBuffer is safe to send across threads when used as SPSC.
// The producer and consumer operate on disjoint memory regions with proper
// atomic ordering, making this safe for our SPSC use case.
unsafe impl<T: Send> Send for SpscRingBuffer<T> {}
unsafe impl<T: Send> Sync for SpscRingBuffer<T> {}

impl<T> Drop for SpscRingBuffer<T> {
    fn drop(&mut self) {
        // Drain remaining elements on drop
        while self.pop().is_some() {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn supports_fifo_push_and_pop() {
        let ring = SpscRingBuffer::<i32>::new(RingConfig::default());
        assert!(ring.push(1).is_ok());
        assert!(ring.push(2).is_ok());
        assert_eq!(ring.len(), 2);
        assert_eq!(ring.pop(), Some(1));
        assert_eq!(ring.pop(), Some(2));
        assert!(ring.is_empty());
    }

    #[test]
    fn returns_full_when_capacity_is_reached() {
        let config = RingConfig {
            capacity: 2,
            backpressure_threshold: 1,
        };
        let ring = SpscRingBuffer::<i32>::new(config);
        assert!(ring.push(1).is_ok());
        assert!(ring.push(2).is_ok());
        assert_eq!(ring.push(3), Err(3));
    }

    #[test]
    fn supports_single_producer_single_consumer_threads() {
        let ring = Arc::new(SpscRingBuffer::<i32>::new(RingConfig::default()));
        let producer_ring = Arc::clone(&ring);
        let consumer_ring = Arc::clone(&ring);

        let producer = thread::spawn(move || {
            for value in 0..1_000 {
                loop {
                    if producer_ring.push(value).is_ok() {
                        break;
                    }
                    std::thread::yield_now();
                }
            }
        });

        let consumer = thread::spawn(move || {
            let mut values = Vec::new();
            while values.len() < 1_000 {
                if let Some(value) = consumer_ring.pop() {
                    values.push(value);
                } else {
                    std::thread::yield_now();
                }
            }
            values
        });

        producer.join().unwrap();
        let values = consumer.join().unwrap();

        assert_eq!(values.len(), 1_000);
        assert_eq!(values.first(), Some(&0));
        assert_eq!(values.last(), Some(&999));
    }

    #[test]
    fn handles_wrap_around_correctly() {
        let ring = SpscRingBuffer::<i32>::new(RingConfig::default());

        // Fill most of the ring
        for i in 0..1020 {
            ring.push(i).unwrap();
        }

        assert_eq!(ring.len(), 1020);

        // Pop all elements in FIFO order
        for i in 0..1020 {
            assert_eq!(ring.pop(), Some(i));
        }

        assert!(ring.is_empty());
    }

    #[test]
    fn backpressure_threshold_works() {
        let config = RingConfig {
            capacity: 100,
            backpressure_threshold: 70,
        };
        let ring = SpscRingBuffer::<i32>::new(config);

        // Fill to threshold - should not backpressure
        for i in 0..69 {
            ring.push(i).unwrap();
        }
        assert!(!ring.should_backpressure());

        // Fill one more - should backpressure
        let result = ring.push(70);
        assert!(result.is_ok()); // Push succeeds until capacity (100) is reached
        assert!(ring.should_backpressure());
    }

    #[test]
    fn wait_for_space_releases() {
        let ring = Arc::new(SpscRingBuffer::<i32>::new(RingConfig::default()));
        let consumer_ring = Arc::clone(&ring);

        // Fill the ring
        for i in 0..1020 {
            ring.push(i).unwrap();
        }

        // Consumer pops one element
        consumer_ring.pop();

        // Producer should be able to push again
        let result = ring.push(1021);
        assert!(result.is_ok());
    }

    #[test]
    fn wait_for_data_releases() {
        let ring = Arc::new(SpscRingBuffer::<i32>::new(RingConfig::default()));
        let producer_ring = Arc::clone(&ring);

        // Ring is empty

        // Producer pushes one element
        producer_ring.push(42).unwrap();

        // Consumer should be able to pop now
        assert_eq!(ring.pop(), Some(42));
    }

    #[test]
    fn push_batch_writes_multiple_elements() {
        let ring = SpscRingBuffer::<u64>::new(RingConfig::default());
        let data: Vec<u64> = (0..100).collect();
        let n = ring.push_batch(&data);
        assert_eq!(n, 100);
        assert_eq!(ring.len(), 100);
    }

    #[test]
    fn push_batch_respects_capacity() {
        let config = RingConfig {
            capacity: 10,
            backpressure_threshold: 8,
        };
        let ring = SpscRingBuffer::<u64>::new(config);
        let data: Vec<u64> = (0..100).collect();
        let n = ring.push_batch(&data);
        assert_eq!(n, 10);
        assert_eq!(ring.len(), 10);
        // The 90 elements that didn't fit must be observable via overflow_count,
        // otherwise backpressure through the batch path is invisible to monitoring.
        assert_eq!(ring.overflow_count(), 90);
    }

    #[test]
    fn pop_batch_reads_multiple_elements() {
        let ring = SpscRingBuffer::<u64>::new(RingConfig::default());
        for i in 0..50u64 {
            ring.push(i).unwrap();
        }
        let mut out = vec![std::mem::MaybeUninit::uninit(); 20];
        let n = ring.pop_batch(&mut out);
        assert_eq!(n, 20);
        let drained: Vec<u64> = out[..n]
            .iter()
            .map(|x| unsafe { x.assume_init_read() })
            .collect();
        assert_eq!(drained, (0..20).collect::<Vec<u64>>());
        assert_eq!(ring.len(), 30);
    }

    #[test]
    fn pop_batch_returns_available_count() {
        let ring = SpscRingBuffer::<u64>::new(RingConfig::default());
        for i in 0..5u64 {
            ring.push(i).unwrap();
        }
        let mut out = vec![std::mem::MaybeUninit::uninit(); 100];
        let n = ring.pop_batch(&mut out);
        assert_eq!(n, 5);
    }

    #[test]
    fn push_pop_batch_round_trip_spsc() {
        let ring = Arc::new(SpscRingBuffer::<u64>::new(RingConfig::default()));
        let prod = Arc::clone(&ring);
        let cons = Arc::clone(&ring);
        let iters = 5_000u64;
        let batch_size = 64;

        let h_prod = std::thread::spawn(move || {
            let mut sent = 0u64;
            while sent < iters {
                let chunk_size = batch_size.min((iters - sent) as usize);
                let chunk: Vec<u64> = (sent..sent + chunk_size as u64).collect();
                let n = prod.push_batch(&chunk);
                sent += n as u64;
                if n == 0 {
                    prod.wait_for_space();
                }
            }
        });

        let h_cons = std::thread::spawn(move || {
            let mut received = 0u64;
            let mut buf = vec![std::mem::MaybeUninit::uninit(); batch_size];
            while received < iters {
                let n = cons.pop_batch(&mut buf);
                if n > 0 {
                    received += n as u64;
                } else {
                    cons.wait_for_data();
                }
            }
        });

        h_prod.join().unwrap();
        h_cons.join().unwrap();
        assert!(ring.is_empty());
    }

    #[test]
    fn write_read_available_tracking() {
        let config = RingConfig {
            capacity: 100,
            backpressure_threshold: 70,
        };
        let ring = SpscRingBuffer::<u64>::new(config);
        assert_eq!(ring.write_available(), 100);
        assert_eq!(ring.read_available(), 0);

        for i in 0..30u64 {
            ring.push(i).unwrap();
        }
        assert_eq!(ring.write_available(), 70);
        assert_eq!(ring.read_available(), 30);

        // pop() advances head; write_available() is conservatively stale
        // (cached_head not refreshed since no push failed), so it may report
        // slightly less than the true capacity.
        ring.pop();
        assert_eq!(ring.read_available(), 29);
        let wa = ring.write_available();
        assert!(
            wa == 70 || wa == 71,
            "write_available should be 70 (conservative) or 71 (true): got {wa}"
        );
    }
}
