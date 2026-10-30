use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::HeapRb;
use std::sync::Arc;

pub struct AudioProducer {
    producer: <Arc<HeapRb<f32>> as Split>::Prod,
}

pub struct AudioConsumer {
    consumer: <Arc<HeapRb<f32>> as Split>::Cons,
}

pub struct AudioRingBuffer;

impl AudioRingBuffer {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(capacity: usize) -> (AudioProducer, AudioConsumer) {
        let rb = Arc::new(HeapRb::<f32>::new(capacity));
        let (prod, cons) = rb.split();
        (
            AudioProducer { producer: prod },
            AudioConsumer { consumer: cons },
        )
    }
}

impl AudioProducer {
    /// Non-blocking, lock-free push of audio samples into ring buffer.
    /// Returns the number of samples successfully written.
    pub fn push_slice(&mut self, samples: &[f32]) -> usize {
        self.producer.push_slice(samples)
    }
}

impl AudioConsumer {
    /// Drains all available samples currently in the ring buffer.
    pub fn drain_all(&mut self) -> Vec<f32> {
        let mut out = Vec::with_capacity(self.consumer.occupied_len());
        while let Some(sample) = self.consumer.try_pop() {
            out.push(sample);
        }
        out
    }

    /// Discards all samples currently in the ring buffer.
    pub fn clear(&mut self) {
        self.consumer.skip(self.consumer.occupied_len());
    }

    /// Number of samples currently waiting in the ring buffer.
    pub fn occupied_len(&self) -> usize {
        self.consumer.occupied_len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_push_and_drain() {
        let (mut prod, mut cons) = AudioRingBuffer::new(1024);
        assert_eq!(cons.occupied_len(), 0);

        let input = [0.1, 0.2, 0.3, 0.4, 0.5];
        let written = prod.push_slice(&input);
        assert_eq!(written, 5);
        assert_eq!(cons.occupied_len(), 5);

        let drained = cons.drain_all();
        assert_eq!(drained, vec![0.1, 0.2, 0.3, 0.4, 0.5]);
        assert_eq!(cons.occupied_len(), 0);
    }

    #[test]
    fn test_clear_buffer() {
        let (mut prod, mut cons) = AudioRingBuffer::new(1024);
        prod.push_slice(&[1.0, 2.0, 3.0]);
        assert_eq!(cons.occupied_len(), 3);

        cons.clear();
        assert_eq!(cons.occupied_len(), 0);
        assert_eq!(cons.drain_all(), Vec::<f32>::new());
    }

    #[test]
    fn test_buffer_capacity_limit() {
        let (mut prod, mut cons) = AudioRingBuffer::new(4);
        let input = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let written = prod.push_slice(&input);
        // Capacity is 4, so at most 4 samples can be pushed
        assert_eq!(written, 4);

        let drained = cons.drain_all();
        assert_eq!(drained, vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn test_cross_thread_spsc() {
        let (mut prod, mut cons) = AudioRingBuffer::new(2048);
        let handle = thread::spawn(move || {
            for i in 0..100 {
                let chunk = [i as f32; 10];
                let mut written = 0;
                while written < chunk.len() {
                    written += prod.push_slice(&chunk[written..]);
                    thread::yield_now();
                }
            }
        });

        let mut collected = Vec::new();
        while collected.len() < 1000 {
            let drained = cons.drain_all();
            collected.extend(drained);
            thread::yield_now();
        }

        handle.join().unwrap();
        assert_eq!(collected.len(), 1000);
        for i in 0..100 {
            for j in 0..10 {
                assert_eq!(collected[i * 10 + j], i as f32);
            }
        }
    }
}
