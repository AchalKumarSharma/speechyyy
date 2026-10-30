pub mod capture;
pub mod resample;
pub mod ring_buffer;
pub mod vad;

pub const TARGET_SAMPLE_RATE: u32 = 16_000;

/// A discrete contiguous vector of 16,000 Hz single-channel PCM audio samples
/// produced by downmixing and resampling.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioChunk {
    pub samples: Vec<f32>,
}

impl AudioChunk {
    pub fn new(samples: Vec<f32>) -> Self {
        Self { samples }
    }

    pub fn duration_secs(&self) -> f32 {
        self.samples.len() as f32 / TARGET_SAMPLE_RATE as f32
    }

    pub fn sample_rate(&self) -> u32 {
        TARGET_SAMPLE_RATE
    }

    pub fn channels(&self) -> u16 {
        1
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AudioError {
    NoDevice,
    DeviceConfig(String),
    UnsupportedFormat(String),
    Stream(String),
}

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoDevice => write!(f, "No audio input device found"),
            Self::DeviceConfig(msg) => write!(f, "Audio device config error: {}", msg),
            Self::UnsupportedFormat(msg) => write!(f, "Unsupported audio format: {}", msg),
            Self::Stream(msg) => write!(f, "Audio stream error: {}", msg),
        }
    }
}

impl std::error::Error for AudioError {}

pub trait AudioCapture: Send {
    fn start_capture(&mut self) -> Result<(), AudioError>;
    fn stop_capture(&mut self) -> Result<AudioChunk, AudioError>;
    fn is_capturing(&self) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_chunk_duration() {
        let chunk = AudioChunk::new(vec![0.0; 32000]);
        assert_eq!(chunk.duration_secs(), 2.0);
        assert_eq!(chunk.sample_rate(), 16000);
        assert_eq!(chunk.channels(), 1);

        let empty_chunk = AudioChunk::new(vec![]);
        assert_eq!(empty_chunk.duration_secs(), 0.0);
    }

    #[test]
    fn test_audio_error_display() {
        assert_eq!(
            AudioError::NoDevice.to_string(),
            "No audio input device found"
        );
        assert_eq!(
            AudioError::Stream("device disconnected".into()).to_string(),
            "Audio stream error: device disconnected"
        );
    }
}
