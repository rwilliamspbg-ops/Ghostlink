#![allow(unexpected_cfgs)]

#[cfg(loom)]
mod loom_tests {
    use ghostlink_core::ring::{RingConfig, SpscRingBuffer};

    #[test]
    fn test_spsc_ring_loom_ordering() {
        let mut builder = loom::model::Builder::new();
        builder.max_branches = 64;
        builder.preemption_bound = Some(2);
        builder.check(|| {
            let ring = SpscRingBuffer::<usize>::new(RingConfig {
                capacity: 4,
                backpressure_threshold: 3,
            });
            assert!(ring.is_empty());
            assert!(ring.push(1).is_ok());
            assert_eq!(ring.pop(), Some(1));
            std::mem::forget(ring);
        });
    }
}

#[test]
#[cfg(not(loom))]
fn test_spsc_ring_ordering_smoke() {
    use ghostlink_core::ring::{RingConfig, SpscRingBuffer};
    use std::sync::Arc;
    use std::thread;

    let ring = Arc::new(SpscRingBuffer::<usize>::new(RingConfig {
        capacity: 16,
        backpressure_threshold: 12,
    }));

    let r_prod = ring.clone();
    let producer = thread::spawn(move || {
        for i in 1..=100 {
            while r_prod.push(i).is_err() {
                thread::yield_now();
            }
        }
    });

    let r_cons = ring.clone();
    let consumer = thread::spawn(move || {
        let mut received = Vec::new();
        while received.len() < 100 {
            if let Some(val) = r_cons.pop() {
                received.push(val);
            } else {
                thread::yield_now();
            }
        }
        received
    });

    producer.join().unwrap();
    let values = consumer.join().unwrap();
    let expected: Vec<usize> = (1..=100).collect();
    assert_eq!(values, expected);
}
