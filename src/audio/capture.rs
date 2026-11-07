use crate::audio::resample::process_raw_audio;
use crate::audio::ring_buffer::{AudioConsumer, AudioRingBuffer};
use crate::audio::{calculate_block_rms, AudioCapture, AudioChunk, AudioError};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

// 2,880,000 samples accommodates 30 seconds of 48kHz stereo or 60 seconds of 48kHz mono (~11.5 MB RAM)
const DEFAULT_BUFFER_CAPACITY: usize = 2_880_000;

pub struct CpalAudioCapture {
    consumer: AudioConsumer,
    stream: Option<Stream>,
    input_sample_rate: u32,
    channels: u16,
    is_recording: Arc<AtomicBool>,
    active_rms: Arc<AtomicU32>,
}

impl CpalAudioCapture {
    /// Initializes CPAL audio capture using the system default input device.
    pub fn new() -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or(AudioError::NoDevice)?;

        let default_config = device
            .default_input_config()
            .map_err(|e| AudioError::DeviceConfig(e.to_string()))?;

        let sample_format = default_config.sample_format();
        let config: StreamConfig = default_config.into();
        let input_sample_rate = config.sample_rate;
        let channels = config.channels;

        let (mut producer, consumer) = AudioRingBuffer::new(DEFAULT_BUFFER_CAPACITY);
        let is_recording = Arc::new(AtomicBool::new(false));
        let is_recording_cb = Arc::clone(&is_recording);
        let active_rms = Arc::new(AtomicU32::new(0));
        let active_rms_cb = Arc::clone(&active_rms);

        let err_fn = |err| {
            eprintln!("CPAL input stream error: {}", err);
        };

        let channels_usize = channels.max(1) as usize;
        let stream = match sample_format {
            SampleFormat::F32 => device.build_input_stream(
                config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    // Gating: Discard incoming audio immediately if not recording
                    if !is_recording_cb.load(Ordering::Relaxed) {
                        return;
                    }
                    let block_rms = calculate_block_rms(data);
                    active_rms_cb.store(block_rms.to_bits(), Ordering::Relaxed);
                    let written = producer.push_frames(data, channels_usize);
                    if written < data.len() {
                        eprintln!(
                            "Audio ring buffer overflow: dropped {} samples",
                            data.len() - written
                        );
                    }
                },
                err_fn,
                None,
            ),
            SampleFormat::I16 => {
                let active_rms_i16 = Arc::clone(&active_rms);
                device.build_input_stream(
                    config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        // Gating: Discard incoming audio immediately if not recording
                        if !is_recording_cb.load(Ordering::Relaxed) {
                            return;
                        }
                        let mut buffer = [0.0f32; 512];
                        let step = (512 / channels_usize) * channels_usize;
                        let step = if step == 0 { 512 } else { step };
                        for chunk in data.chunks(step) {
                            for (i, &s) in chunk.iter().enumerate() {
                                buffer[i] = s as f32 / 32768.0;
                            }
                            let block_rms = calculate_block_rms(&buffer[..chunk.len()]);
                            active_rms_i16.store(block_rms.to_bits(), Ordering::Relaxed);
                            let written =
                                producer.push_frames(&buffer[..chunk.len()], channels_usize);
                            if written < chunk.len() {
                                eprintln!(
                                    "Audio ring buffer overflow: dropped {} samples",
                                    chunk.len() - written
                                );
                            }
                        }
                    },
                    err_fn,
                    None,
                )
            }
            other => {
                return Err(AudioError::UnsupportedFormat(format!("{:?}", other)));
            }
        }
        .map_err(|e| AudioError::Stream(e.to_string()))?;

        // Ensure stream is paused when created while idle
        let _ = stream.pause();

        Ok(Self {
            consumer,
            stream: Some(stream),
            input_sample_rate,
            channels,
            is_recording,
            active_rms,
        })
    }
}

impl AudioCapture for CpalAudioCapture {
    fn start_capture(&mut self) -> Result<(), AudioError> {
        self.active_rms.store(0, Ordering::Relaxed);
        // 1. Drain/flush any stale residual samples from the ring buffer
        self.consumer.clear();

        // 2. Enable callback sample ingestion
        self.is_recording.store(true, Ordering::SeqCst);

        // 3. Unpause hardware stream
        if let Some(stream) = &self.stream {
            stream
                .play()
                .map_err(|e| AudioError::Stream(e.to_string()))?;
        }

        // 4. Clear once more right after play() to ensure no pre-trigger transients linger
        self.consumer.clear();
        Ok(())
    }

    fn stop_capture(&mut self) -> Result<AudioChunk, AudioError> {
        self.active_rms.store(0, Ordering::Relaxed);
        // 1. Immediately gate callback so new samples are dropped at the door
        self.is_recording.store(false, Ordering::SeqCst);

        // 2. Pause hardware stream
        if let Some(stream) = &self.stream {
            let _ = stream.pause();
        }

        // 3. Drain all captured samples
        let raw_samples = self.consumer.drain_all();
        let chunk = process_raw_audio(&raw_samples, self.input_sample_rate, self.channels);
        Ok(chunk)
    }

    fn is_capturing(&self) -> bool {
        self.is_recording.load(Ordering::Relaxed)
    }

    fn active_rms(&self) -> Option<Arc<AtomicU32>> {
        Some(Arc::clone(&self.active_rms))
    }
}

/// Deterministic mock capture implementation for headless unit and integration testing.
#[derive(Debug, Clone)]
pub struct MockAudioCapture {
    pub is_recording: bool,
    pub queued_chunks: Vec<AudioChunk>,
    pub captured_chunks: Vec<AudioChunk>,
    pub start_calls: usize,
    pub stop_calls: usize,
    pub active_rms: Arc<AtomicU32>,
}

impl Default for MockAudioCapture {
    fn default() -> Self {
        Self {
            is_recording: false,
            queued_chunks: Vec::new(),
            captured_chunks: Vec::new(),
            start_calls: 0,
            stop_calls: 0,
            active_rms: Arc::new(AtomicU32::new(0)),
        }
    }
}

impl MockAudioCapture {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_queued_chunk(chunk: AudioChunk) -> Self {
        Self {
            is_recording: false,
            queued_chunks: vec![chunk],
            captured_chunks: Vec::new(),
            start_calls: 0,
            stop_calls: 0,
            active_rms: Arc::new(AtomicU32::new(0)),
        }
    }

    pub fn with_queued_chunks(chunks: Vec<AudioChunk>) -> Self {
        Self {
            is_recording: false,
            queued_chunks: chunks,
            captured_chunks: Vec::new(),
            start_calls: 0,
            stop_calls: 0,
            active_rms: Arc::new(AtomicU32::new(0)),
        }
    }
}

impl AudioCapture for MockAudioCapture {
    fn start_capture(&mut self) -> Result<(), AudioError> {
        self.is_recording = true;
        self.start_calls += 1;
        Ok(())
    }

    fn stop_capture(&mut self) -> Result<AudioChunk, AudioError> {
        self.is_recording = false;
        self.stop_calls += 1;
        if !self.queued_chunks.is_empty() {
            let chunk = self.queued_chunks.remove(0);
            self.captured_chunks.push(chunk.clone());
            Ok(chunk)
        } else {
            let chunk = AudioChunk::new(vec![0.05; 8000]);
            self.captured_chunks.push(chunk.clone());
            Ok(chunk)
        }
    }

    fn is_capturing(&self) -> bool {
        self.is_recording
    }

    fn active_rms(&self) -> Option<Arc<AtomicU32>> {
        Some(Arc::clone(&self.active_rms))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_capture_lifecycle() {
        let mut mock = MockAudioCapture::new();
        assert!(!mock.is_capturing());
        assert_eq!(mock.start_calls, 0);
        assert_eq!(mock.stop_calls, 0);

        mock.start_capture().unwrap();
        assert!(mock.is_capturing());
        assert_eq!(mock.start_calls, 1);

        let chunk = mock.stop_capture().unwrap();
        assert!(!mock.is_capturing());
        assert_eq!(mock.stop_calls, 1);
        assert_eq!(chunk.sample_rate(), 16000);
        assert_eq!(chunk.channels(), 1);
    }

    #[test]
    fn test_mock_queued_chunks() {
        let custom_chunk = AudioChunk::new(vec![0.8; 3200]);
        let mut mock = MockAudioCapture::with_queued_chunk(custom_chunk.clone());

        mock.start_capture().unwrap();
        let chunk = mock.stop_capture().unwrap();
        assert_eq!(chunk, custom_chunk);
    }

    #[test]
    fn test_cpal_init_never_panics() {
        let result = CpalAudioCapture::new();
        match result {
            Ok(capture) => {
                assert!(!capture.is_capturing());
            }
            Err(e) => match e {
                AudioError::NoDevice
                | AudioError::DeviceConfig(_)
                | AudioError::UnsupportedFormat(_)
                | AudioError::Stream(_) => {}
            },
        }
    }

    #[test]
    fn test_capture_gating_drops_idle_samples() {
        let (mut producer, mut consumer) = AudioRingBuffer::new(1024);
        let is_recording = Arc::new(AtomicBool::new(false));

        let incoming_data = [0.5f32; 128];

        // While idle, callback must drop incoming data immediately
        if is_recording.load(Ordering::Relaxed) {
            producer.push_slice(&incoming_data);
        }
        assert_eq!(consumer.occupied_len(), 0);

        // Start recording
        is_recording.store(true, Ordering::SeqCst);
        if is_recording.load(Ordering::Relaxed) {
            producer.push_slice(&incoming_data);
        }
        assert_eq!(consumer.occupied_len(), 128);

        // Stop recording
        is_recording.store(false, Ordering::SeqCst);
        if is_recording.load(Ordering::Relaxed) {
            producer.push_slice(&incoming_data);
        }
        assert_eq!(consumer.occupied_len(), 128);

        // Flushing on press clears buffer
        consumer.clear();
        assert_eq!(consumer.occupied_len(), 0);
    }
}
