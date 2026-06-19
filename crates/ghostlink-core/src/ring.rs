use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct SpscRingBuffer<T> {
    buffer: Box<[UnsafeCell<MaybeUninit<T>>]>,
    capacity: usize,
    head: AtomicUsize,
    tail: AtomicUsize,
}

unsafe impl<T: Send> Send for SpscRingBuffer<T> {}
unsafe impl<T: Send> Sync for SpscRingBuffer<T> {}

impl<T> SpscRingBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 1, "capacity must be greater than 1");
        let buffer = (0..capacity)
            .map(|_| UnsafeCell::new(MaybeUninit::uninit()))
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Self {
            buffer,
            capacity,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    pub fn push(&self, value: T) -> Result<(), T> {
        let tail = self.tail.load(Ordering::Relaxed);
        let next_tail = self.increment(tail);
        let head = self.head.load(Ordering::Acquire);

        if next_tail == head {
            return Err(value);
        }

        unsafe {
            (*self.buffer[tail].get()).write(value);
        }
        self.tail.store(next_tail, Ordering::Release);
        Ok(())
    }

    pub fn pop(&self) -> Option<T> {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);

        if head == tail {
            return None;
        }

        let value = unsafe { (*self.buffer[head].get()).assume_init_read() };
        self.head.store(self.increment(head), Ordering::Release);
        Some(value)
    }

    pub fn capacity(&self) -> usize {
        self.capacity - 1
    }

    pub fn len(&self) -> usize {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);
        if tail >= head {
            tail - head
        } else {
            self.capacity - head + tail
        }
    }

    pub fn is_empty(&self) -> bool {
        self.head.load(Ordering::Acquire) == self.tail.load(Ordering::Acquire)
    }

    fn increment(&self, index: usize) -> usize {
        (index + 1) % self.capacity
    }
}

impl<T> Drop for SpscRingBuffer<T> {
    fn drop(&mut self) {
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
        let ring = SpscRingBuffer::new(4);
        assert!(ring.push(1).is_ok());
        assert!(ring.push(2).is_ok());
        assert_eq!(ring.len(), 2);
        assert_eq!(ring.pop(), Some(1));
        assert_eq!(ring.pop(), Some(2));
        assert!(ring.is_empty());
    }

    #[test]
    fn returns_full_when_capacity_is_reached() {
        let ring = SpscRingBuffer::new(3);
        assert!(ring.push(1).is_ok());
        assert!(ring.push(2).is_ok());
        assert_eq!(ring.push(3), Err(3));
    }

    #[test]
    fn supports_single_producer_single_consumer_threads() {
        let ring = Arc::new(SpscRingBuffer::new(128));
        let producer_ring = Arc::clone(&ring);
        let consumer_ring = Arc::clone(&ring);

        let producer = thread::spawn(move || {
            for value in 0..1_000 {
                loop {
                    if producer_ring.push(value).is_ok() {
                        break;
                    }
                    thread::yield_now();
                }
            }
        });

        let consumer = thread::spawn(move || {
            let mut values = Vec::new();
            while values.len() < 1_000 {
                if let Some(value) = consumer_ring.pop() {
                    values.push(value);
                } else {
                    thread::yield_now();
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
}
