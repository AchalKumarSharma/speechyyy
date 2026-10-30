use crate::audio::AudioChunk;
use crate::inference::{InferenceError, SpeechInference};

#[derive(Debug, Default)]
pub struct MockSpeechInference {
    pub queued_transcripts: Vec<String>,
    pub transcribed_chunks: Vec<AudioChunk>,
    pub call_count: usize,
}

impl MockSpeechInference {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_transcript(text: &str) -> Self {
        Self {
            queued_transcripts: vec![text.to_string()],
            transcribed_chunks: Vec::new(),
            call_count: 0,
        }
    }

    pub fn with_queued_transcripts(transcripts: Vec<String>) -> Self {
        Self {
            queued_transcripts: transcripts,
            transcribed_chunks: Vec::new(),
            call_count: 0,
        }
    }
}

impl SpeechInference for MockSpeechInference {
    fn transcribe(&mut self, chunk: &AudioChunk) -> Result<String, InferenceError> {
        self.call_count += 1;
        self.transcribed_chunks.push(chunk.clone());
        if !self.queued_transcripts.is_empty() {
            Ok(self.queued_transcripts.remove(0))
        } else {
            Ok("Hello from Speechyyy".to_string())
        }
    }

    fn provider_name(&self) -> &'static str {
        "mock"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_inference_default_transcript() {
        let mut mock = MockSpeechInference::new();
        assert_eq!(mock.provider_name(), "mock");
        assert_eq!(mock.call_count, 0);

        let chunk = AudioChunk::new(vec![0.1; 1600]);
        let res = mock.transcribe(&chunk).unwrap();
        assert_eq!(res, "Hello from Speechyyy");
        assert_eq!(mock.call_count, 1);
        assert_eq!(mock.transcribed_chunks.len(), 1);
    }

    #[test]
    fn test_mock_inference_queued_transcripts() {
        let mut mock = MockSpeechInference::with_queued_transcripts(vec![
            "First sentence".into(),
            "Second sentence".into(),
        ]);

        let chunk = AudioChunk::new(vec![0.2; 800]);
        assert_eq!(mock.transcribe(&chunk).unwrap(), "First sentence");
        assert_eq!(mock.transcribe(&chunk).unwrap(), "Second sentence");
        assert_eq!(mock.transcribe(&chunk).unwrap(), "Hello from Speechyyy");
        assert_eq!(mock.call_count, 3);
    }
}
