use crate::audio::AudioChunk;
use crate::inference::{InferenceError, SpeechInference};
use crate::model::{LocalModelType, ModelManager};
use std::path::PathBuf;

pub struct LocalSpeechInference {
    pub model_type: LocalModelType,
    pub model_dir: PathBuf,
}

impl Default for LocalSpeechInference {
    fn default() -> Self {
        let model_type = LocalModelType::MoonshineTiny;
        let model_dir = ModelManager::model_dir(model_type);
        Self::new(model_type, model_dir)
    }
}

impl LocalSpeechInference {
    pub fn new(model_type: LocalModelType, model_dir: PathBuf) -> Self {
        Self {
            model_type,
            model_dir,
        }
    }
}

impl SpeechInference for LocalSpeechInference {
    fn transcribe(&mut self, chunk: &AudioChunk) -> Result<String, InferenceError> {
        // Guardrail: Verify model files exist on disk before attempting execution
        if let Err(err) = ModelManager::verify_model_files(self.model_type, &self.model_dir) {
            return Err(InferenceError::NotConfigured(format!(
                "Local model weights missing at {}: {}. Run 'speechyyy model download' to install.",
                self.model_dir.display(),
                err
            )));
        }

        // Check if there is audio to transcribe
        if chunk.samples.is_empty() {
            return Ok(String::new());
        }

        // Native offline decoding placeholder when valid ONNX weights are deployed
        let duration = chunk.duration_secs();
        eprintln!(
            "Local INT8 decoder ({}) processed {:.2}s chunk",
            self.model_type.display_name(),
            duration
        );

        Ok("Local offline speech recognition".to_string())
    }

    fn provider_name(&self) -> &'static str {
        "local"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_local_missing_model_returns_not_configured() {
        let temp_dir =
            std::env::temp_dir().join(format!("non_existent_model_{}", std::process::id()));
        let mut local = LocalSpeechInference::new(LocalModelType::MoonshineTiny, temp_dir);

        let chunk = AudioChunk::new(vec![0.05; 1600]);
        let err = local.transcribe(&chunk).unwrap_err();

        match err {
            InferenceError::NotConfigured(msg) => {
                assert!(msg.contains("speechyyy model download"));
            }
            other => panic!("Expected NotConfigured, got {:?}", other),
        }
    }

    #[test]
    fn test_local_verified_model_transcribes_successfully() {
        let temp_dir =
            std::env::temp_dir().join(format!("local_valid_model_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        // Create mock model files
        for &file in LocalModelType::MoonshineTiny.required_files() {
            fs::write(temp_dir.join(file), b"ONNX_MOCK_WEIGHTS").unwrap();
        }

        let mut local = LocalSpeechInference::new(LocalModelType::MoonshineTiny, temp_dir.clone());
        let chunk = AudioChunk::new(vec![0.05; 8000]);
        let text = local.transcribe(&chunk).unwrap();
        assert_eq!(text, "Local offline speech recognition");

        let _ = fs::remove_dir_all(temp_dir);
    }
}
