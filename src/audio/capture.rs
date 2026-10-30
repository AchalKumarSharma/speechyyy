use crate::audio::resample::process_raw_audio;
use crate::audio::ring_buffer::{AudioConsumer, AudioRingBuffer};
use crate::audio::{AudioCapture, AudioChunk, AudioError};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};

const DEFAULT_BUFFER_CAPACITY: usize = 480_000; // 10 seconds of 48kHz audio

pub struct CpalAudioCapture {
    consumer: AudioConsumer,
    stream: Option<Stream>,
    input_sample_rate: u32,
    channels: u16,
    is_recording: bool,
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

        let err_fn = |err| {
            eprintln!("CPAL input stream error: {}", err);
        };

        let stream = match sample_format {
            SampleFormat::F32 => device.build_input_stream(
                config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    let written = producer.push_slice(data);
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
            SampleFormat::I16 => device.build_input_stream(
                config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    let mut buffer = [0.0f32; 512];
                    for chunk in data.chunks(512) {
                        for (i, &s) in chunk.iter().enumerate() {
                            buffer[i] = s as f32 / 32768.0;
                        }
                        let written = producer.push_slice(&buffer[..chunk.len()]);
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
            ),
            other => {
                return Err(AudioError::UnsupportedFormat(format!("{:?}", other)));
            }
        }
        .map_err(|e| AudioError::Stream(e.to_string()))?;

        Ok(Self {
            consumer,
            stream: Some(stream),
            input_sample_rate,
            channels,
            is_recording: false,
        })
    }
}

impl AudioCapture for CpalAudioCapture {
    fn start_capture(&mut self) -> Result<(), AudioError> {
        self.consumer.clear();
        if let Some(stream) = &self.stream {
            stream
                .play()
                .map_err(|e| AudioError::Stream(e.to_string()))?;
        }
        self.is_recording = true;
        Ok(())
    }

    fn stop_capture(&mut self) -> Result<AudioChunk, AudioError> {
        if let Some(stream) = &self.stream {
            let _ = stream.pause();
        }
        self.is_recording = false;

        let raw_samples = self.consumer.drain_all();
        let chunk = process_raw_audio(&raw_samples, self.input_sample_rate, self.channels);
        Ok(chunk)
    }

    fn is_capturing(&self) -> bool {
        self.is_recording
    }
}

/// Deterministic mock capture implementation for headless unit and integration testing.
#[derive(Debug, Default)]
pub struct MockAudioCapture {
    pub is_recording: bool,
    pub queued_chunks: Vec<AudioChunk>,
    pub captured_chunks: Vec<AudioChunk>,
    pub start_calls: usize,
    pub stop_calls: usize,
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
}
