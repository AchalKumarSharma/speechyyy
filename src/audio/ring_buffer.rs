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

    /// Push audio samples strictly aligned to full audio frames (`channels` samples per frame).
    /// Prevents channel interleaving desynchronization during buffer overflows.
    pub fn push_frames(&mut self, samples: &[f32], channels: usize) -> usize {
        if channels == 0 || samples.is_empty() {
            return 0;
        }
        let vacant = self.producer.vacant_len();
        let max_frames = vacant / channels;
        let requested_frames = samples.len() / channels;
        let frames_to_push = requested_frames.min(max_frames);
        let samples_to_push = frames_to_push * channels;
        if samples_to_push == 0 {
            return 0;
        }
        self.producer.push_slice(&samples[..samples_to_push])
    }

    /// Number of sample slots currently free in the ring buffer.
    pub fn vacant_len(&self) -> usize {
        self.producer.vacant_len()
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
        while self.consumer.try_pop().is_some() {}
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
    fn test_push_frames_maintains_channel_alignment() {
        // Buffer capacity of 5 samples (can fit at most two 2-channel stereo frames = 4 samples)
        let (mut prod, mut cons) = AudioRingBuffer::new(5);
        assert_eq!(prod.vacant_len(), 5);

        // 3 stereo frames = 6 samples [L0, R0, L1, R1, L2, R2]
        let input = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let written = prod.push_frames(&input, 2);
        // Even though capacity is 5, it must ONLY write 4 samples (2 complete frames)
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
