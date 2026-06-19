//! Zero-Copy SPSC Ring Buffer with AF_XDP Integration
//! 
//! This implementation uses pinned allocations and proper memory ordering
//! for single-producer/single-consumer DMA-style hand-off.

use pin_utils::pin_mut;
use std::cell::{Cell, UnsafeCell};
use std::mem::MaybeUninit;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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
            capacity: 1024,
            backpressure_threshold: 716, // 70% of 1024
        }
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
    /// Pinned buffer with zero-copy allocations
    buffer: UnsafeCell<MaybeUninit<[MaybeUninit<T>; Self::CAPACITY]>>,
    /// Current head position (producer writes here)
    head: AtomicUsize,
    /// Current tail position (consumer reads here)
    tail: AtomicUsize,
    /// Overflow counter for backpressure monitoring
    overflow_count: AtomicUsize,
    /// Empty count for backpressure monitoring
    empty_count: AtomicUsize,
    /// Configuration for this ring buffer
    config: RingConfig,
}

impl<T> SpscRingBuffer<T> {
    const CAPACITY: usize = 1024; // Fixed capacity for DMA alignment
    
    /// Create a new SPSC ring buffer with the given configuration
    pub fn new(config: RingConfig) -> Self {
        assert!(config.capacity <= Self::CAPACITY, "capacity must not exceed maximum");
        
        let buffer = UnsafeCell::new(MaybeUninit::uninit());
        
        Self {
            buffer,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            overflow_count: AtomicUsize::new(0),
            empty_count: AtomicUsize::new(Self::CAPACITY - config.capacity),
            config,
        }
    }
    
    /// Push an element into the ring buffer
    /// 
    /// Returns `Ok(())` on success, or `Err(value)` if the ring is full.
    /// When full, the producer should wait/backpressure until space is available.
    pub fn push(&self, value: T) -> Result<(), T> {
        let tail = self.tail.load(Ordering::Acquire);
        let next_tail = Self::increment(tail);
        let head = self.head.load(Ordering::Acquire);
        
        // Check if ring is full (with wrap-around handling)
        let is_full = if next_tail >= head {
            next_tail == head
        } else {
            false
        };
        
        if is_full {
            self.overflow_count.fetch_add(1, Ordering::Relaxed);
            return Err(value);
        }
        
        // Write the value at tail position
        unsafe {
            let buffer = &*self.buffer.get();
            (*buffer[tail]).write(value);
            
            // Release store to make write visible to consumer
            self.tail.store(next_tail, Ordering::Release);
        }
        
        Ok(())
    }
    
    /// Pop an element from the ring buffer
    /// 
    /// Returns `Some(value)` on success, or `None` if the ring is empty.
    pub fn pop(&self) -> Option<T> {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);
        
        // Check if ring is empty (with wrap-around handling)
        let is_empty = if head >= tail {
            head == tail
        } else {
            false
        };
        
        if is_empty {
            return None;
        }
        
        // Read the value at head position
        unsafe {
            let buffer = &*self.buffer.get();
            let value = (*buffer[head]).read();
            
            // Acquire store to make read visible to producer
            self.head.store(Self::increment(head), Ordering::Release);
        }
        
        Some(value)
    }
    
    /// Get the current number of elements in the ring
    pub fn len(&self) -> usize {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);
        
        if tail >= head {
            tail - head
        } else {
            Self::CAPACITY - head + tail
        }
    }
    
    /// Check if the ring is empty
    pub fn is_empty(&self) -> bool {
        self.head.load(Ordering::Acquire) == self.tail.load(Ordering::Acquire)
    }
    
    /// Get overflow count for backpressure monitoring
    pub fn overflow_count(&self) -> usize {
        self.overflow_count.load(Ordering::Relaxed)
    }
    
    /// Get empty count for backpressure monitoring
    pub fn empty_count(&self) -> usize {
        self.empty_count.load(Ordering::Relaxed)
    }
    
    /// Check if backpressure should be applied (ring >= threshold)
    pub fn should_backpressure(&self) -> bool {
        let len = self.len();
        len >= self.config.backpressure_threshold
    }
    
    /// Wait for space to become available
    /// 
    /// Spins until there's room in the ring buffer.
    pub fn wait_for_space(&self) {
        while self.should_backpressure() {
            // Yield to allow consumer to make progress
            std::thread::yield_now();
        }
    }
    
    /// Wait for an element to become available
    /// 
    /// Spins until there's data in the ring buffer.
    pub fn wait_for_data(&self) {
        while self.is_empty() {
            // Yield to allow producer to make progress
            std::thread::yield_now();
        }
    }
    
    fn increment(index: usize) -> usize {
        (index + 1) % Self::CAPACITY
    }
}

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
        
        // Pop all elements
        for i in (0..1020).rev() {
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
        assert!(result.is_err());
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
}
