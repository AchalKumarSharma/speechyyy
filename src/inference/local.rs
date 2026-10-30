use crate::audio::AudioChunk;
use crate::inference::{InferenceError, SpeechInference};

#[derive(Debug, Default)]
pub struct LocalSpeechInference;

impl LocalSpeechInference {
    pub fn new() -> Self {
        Self
    }
}

impl SpeechInference for LocalSpeechInference {
    fn transcribe(&mut self, _chunk: &AudioChunk) -> Result<String, InferenceError> {
        Err(InferenceError::NotConfigured(
            "Local speech inference engine stub: model weights not installed (planned for Sprint 4)."
                .to_string(),
        ))
    }

    fn provider_name(&self) -> &'static str {
        "local"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_stub_returns_not_configured() {
        let mut local = LocalSpeechInference::new();
        assert_eq!(local.provider_name(), "local");

        let chunk = AudioChunk::new(vec![0.0; 1600]);
        let err = local.transcribe(&chunk).unwrap_err();
        match err {
            InferenceError::NotConfigured(msg) => {
                assert!(msg.contains("Sprint 4"));
            }
            other => panic!("Expected NotConfigured, got {:?}", other),
        }
    }
}
