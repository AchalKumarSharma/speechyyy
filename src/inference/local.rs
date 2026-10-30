use crate::audio::AudioChunk;
use crate::inference::{InferenceError, SpeechInference};
use crate::model::{LocalModelType, ModelManager};
use sherpa_rs::moonshine::{MoonshineConfig, MoonshineRecognizer};
use sherpa_rs::whisper::{WhisperConfig, WhisperRecognizer};
use std::path::PathBuf;

enum ActiveRecognizer {
    Moonshine(MoonshineRecognizer),
    Whisper(WhisperRecognizer),
}

pub struct LocalSpeechInference {
    pub model_type: LocalModelType,
    pub model_dir: PathBuf,
    recognizer: Option<ActiveRecognizer>,
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
            recognizer: None,
        }
    }

    fn init_recognizer(&mut self) -> Result<(), InferenceError> {
        if self.recognizer.is_some() {
            return Ok(());
        }

        match self.model_type {
            LocalModelType::MoonshineTiny => {
                let config = MoonshineConfig {
                    preprocessor: self
                        .model_dir
                        .join("preprocess.onnx")
                        .to_string_lossy()
                        .to_string(),
                    encoder: self
                        .model_dir
                        .join("encode.int8.onnx")
                        .to_string_lossy()
                        .to_string(),
                    uncached_decoder: self
                        .model_dir
                        .join("uncached_decode.int8.onnx")
                        .to_string_lossy()
                        .to_string(),
                    cached_decoder: self
                        .model_dir
                        .join("cached_decode.int8.onnx")
                        .to_string_lossy()
                        .to_string(),
                    tokens: self
                        .model_dir
                        .join("tokens.txt")
                        .to_string_lossy()
                        .to_string(),
                    provider: Some("cpu".to_string()),
                    num_threads: Some(2),
                    debug: false,
                };
                let recognizer = MoonshineRecognizer::new(config).map_err(|e| {
                    InferenceError::LocalEngine(format!(
                        "Failed to initialize Moonshine ONNX recognizer: {}",
                        e
                    ))
                })?;
                self.recognizer = Some(ActiveRecognizer::Moonshine(recognizer));
            }
            LocalModelType::WhisperTiny => {
                let config = WhisperConfig {
                    encoder: self
                        .model_dir
                        .join("tiny.en-encoder.int8.onnx")
                        .to_string_lossy()
                        .to_string(),
                    decoder: self
                        .model_dir
                        .join("tiny.en-decoder.int8.onnx")
                        .to_string_lossy()
                        .to_string(),
                    tokens: self
                        .model_dir
                        .join("tiny.en-tokens.txt")
                        .to_string_lossy()
                        .to_string(),
                    language: "en".to_string(),
                    bpe_vocab: None,
                    tail_paddings: None,
                    provider: Some("cpu".to_string()),
                    num_threads: Some(2),
                    debug: false,
                };
                let recognizer = WhisperRecognizer::new(config).map_err(|e| {
                    InferenceError::LocalEngine(format!(
                        "Failed to initialize Whisper ONNX recognizer: {}",
                        e
                    ))
                })?;
                self.recognizer = Some(ActiveRecognizer::Whisper(recognizer));
            }
        }

        Ok(())
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

        // Lazily initialize and cache the ONNX recognizer session
        self.init_recognizer()?;

        let sample_rate = 16000;
        let text = match self.recognizer.as_mut().unwrap() {
            ActiveRecognizer::Moonshine(rec) => {
                let res = rec.transcribe(sample_rate, &chunk.samples);
                res.text
            }
            ActiveRecognizer::Whisper(rec) => {
                let res = rec.transcribe(sample_rate, &chunk.samples);
                res.text
            }
        };

        let trimmed = text.trim();
        eprintln!(
            "Local INT8 decoder ({}) decoded {:.2}s chunk: \"{}\"",
            self.model_type.display_name(),
            chunk.duration_secs(),
            trimmed
        );

        Ok(trimmed.to_string())
    }

    fn provider_name(&self) -> &'static str {
        "local"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

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
    fn test_local_verified_moonshine_transcribes_silence() {
        let model_dir = ModelManager::model_dir(LocalModelType::MoonshineTiny);
        if !model_dir.exists() {
            eprintln!(
                "Skipping Moonshine test: model not installed at {}",
                model_dir.display()
            );
            return;
        }

        let mut local = LocalSpeechInference::new(LocalModelType::MoonshineTiny, model_dir);
        let silence_chunk = AudioChunk::new(vec![0.0f32; 16000]);
        let text = local.transcribe(&silence_chunk).unwrap();
        assert_eq!(text, "");
    }

    #[test]
    fn test_local_verified_whisper_transcribes_silence() {
        let model_dir = ModelManager::model_dir(LocalModelType::WhisperTiny);
        if !model_dir.exists() {
            eprintln!(
                "Skipping Whisper test: model not installed at {}",
                model_dir.display()
            );
            return;
        }

        let mut local = LocalSpeechInference::new(LocalModelType::WhisperTiny, model_dir);
        let silence_chunk = AudioChunk::new(vec![0.0f32; 16000]);
        let text = local.transcribe(&silence_chunk).unwrap();
        assert!(text.is_empty() || text.to_lowercase().contains("silence"));
    }

    #[test]
    fn test_local_moonshine_transcribes_real_speech_audio() {
        let model_dir = ModelManager::model_dir(LocalModelType::MoonshineTiny);
        let wav_path = Path::new("tests/test_data/0.wav");

        if !model_dir.exists() || !wav_path.exists() {
            eprintln!("Skipping real audio test: model or test wav not found");
            return;
        }

        let (samples, _) = sherpa_rs::read_audio_file(wav_path.to_str().unwrap()).unwrap();
        let chunk = AudioChunk::new(samples);

        let mut local = LocalSpeechInference::new(LocalModelType::MoonshineTiny, model_dir);
        let transcript = local.transcribe(&chunk).unwrap();

        eprintln!("Real decoded transcript: {}", transcript);
        let lower = transcript.to_lowercase();
        assert!(
            lower.contains("yellow lamps"),
            "Expected transcript to contain 'yellow lamps', got: {}",
            transcript
        );
        assert!(
            lower.contains("brothels"),
            "Expected transcript to contain 'brothels', got: {}",
            transcript
        );
    }

    #[test]
    fn test_local_whisper_transcribes_real_speech_audio() {
        let model_dir = ModelManager::model_dir(LocalModelType::WhisperTiny);
        let wav_path = Path::new("tests/test_data/0.wav");

        if !model_dir.exists() || !wav_path.exists() {
            eprintln!("Skipping real audio test: Whisper model or test wav not found");
            return;
        }

        let (samples, _) = sherpa_rs::read_audio_file(wav_path.to_str().unwrap()).unwrap();
        let chunk = AudioChunk::new(samples);

        let mut local = LocalSpeechInference::new(LocalModelType::WhisperTiny, model_dir);
        let transcript = local.transcribe(&chunk).unwrap();

        eprintln!("Whisper real decoded transcript: {}", transcript);
        let lower = transcript.to_lowercase();
        assert!(
            lower.contains("yellow lamps"),
            "Expected Whisper transcript to contain 'yellow lamps', got: {}",
            transcript
        );
    }
}
