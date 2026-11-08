use crate::audio::AudioChunk;
use crate::config::CustomModelConfig;
use crate::inference::{InferenceError, SpeechInference};
use crate::model::{find_file, LocalModelType, ModelManager};
use sherpa_rs::moonshine::{MoonshineConfig, MoonshineRecognizer};
use sherpa_rs::sense_voice::{SenseVoiceConfig, SenseVoiceRecognizer};
use sherpa_rs::transducer::{TransducerConfig, TransducerRecognizer};
use sherpa_rs::whisper::{WhisperConfig, WhisperRecognizer};
use std::path::{Path, PathBuf};

pub enum ActiveRecognizer {
    Moonshine(MoonshineRecognizer),
    Whisper(WhisperRecognizer),
    Transducer(TransducerRecognizer),
    SenseVoice(SenseVoiceRecognizer),
}

pub struct LocalSpeechInference {
    pub model_type: LocalModelType,
    pub model_dir: PathBuf,
    model_verified: bool,
    recognizer: Option<ActiveRecognizer>,
}

impl Default for LocalSpeechInference {
    fn default() -> Self {
        let model_type = LocalModelType::ParakeetTdtV3;
        let model_dir = ModelManager::model_dir(model_type);
        Self::new(model_type, model_dir)
    }
}

impl LocalSpeechInference {
    pub fn new(model_type: LocalModelType, model_dir: PathBuf) -> Self {
        let model_verified = ModelManager::verify_model_files(model_type, &model_dir).is_ok();
        Self {
            model_type,
            model_dir,
            model_verified,
            recognizer: None,
        }
    }

    fn init_recognizer(&mut self) -> Result<(), InferenceError> {
        if self.recognizer.is_some() {
            return Ok(());
        }

        match self.model_type {
            LocalModelType::ParakeetTdtV3 => {
                let encoder = find_file(&self.model_dir, &["encoder.int8.onnx"]).ok_or_else(|| {
                    InferenceError::NotConfigured(format!(
                        "Parakeet encoder not found in {}",
                        self.model_dir.display()
                    ))
                })?;
                let decoder = find_file(&self.model_dir, &["decoder.int8.onnx"]).ok_or_else(|| {
                    InferenceError::NotConfigured(format!(
                        "Parakeet decoder not found in {}",
                        self.model_dir.display()
                    ))
                })?;
                let joiner = find_file(&self.model_dir, &["joiner.int8.onnx"]).ok_or_else(|| {
                    InferenceError::NotConfigured(format!(
                        "Parakeet joiner not found in {}",
                        self.model_dir.display()
                    ))
                })?;
                let tokens = find_file(&self.model_dir, &["tokens.txt"]).ok_or_else(|| {
                    InferenceError::NotConfigured(format!(
                        "Parakeet tokens not found in {}",
                        self.model_dir.display()
                    ))
                })?;

                let config = TransducerConfig {
                    encoder: encoder.to_string_lossy().to_string(),
                    decoder: decoder.to_string_lossy().to_string(),
                    joiner: joiner.to_string_lossy().to_string(),
                    tokens: tokens.to_string_lossy().to_string(),
                    model_type: "nemo_transducer".to_string(),
                    feature_dim: 128,
                    sample_rate: 16000,
                    provider: Some("cpu".to_string()),
                    num_threads: 2,
                    ..Default::default()
                };
                let recognizer = TransducerRecognizer::new(config).map_err(|e| {
                    InferenceError::LocalEngine(format!(
                        "Failed to initialize Parakeet Transducer recognizer: {}",
                        e
                    ))
                })?;
                self.recognizer = Some(ActiveRecognizer::Transducer(recognizer));
            }
            LocalModelType::WhisperSmall | LocalModelType::WhisperLargeV3Turbo => {
                let enc_candidates = match self.model_type {
                    LocalModelType::WhisperSmall => &["small-encoder.int8.onnx", "encoder.int8.onnx"][..],
                    LocalModelType::WhisperLargeV3Turbo => {
                        &["turbo-encoder.int8.onnx", "encoder.int8.onnx"][..]
                    }
                    _ => unreachable!(),
                };
                let dec_candidates = match self.model_type {
                    LocalModelType::WhisperSmall => &["small-decoder.int8.onnx", "decoder.int8.onnx"][..],
                    LocalModelType::WhisperLargeV3Turbo => {
                        &["turbo-decoder.int8.onnx", "decoder.int8.onnx"][..]
                    }
                    _ => unreachable!(),
                };
                let tok_candidates = match self.model_type {
                    LocalModelType::WhisperSmall => &["small-tokens.txt", "tokens.txt"][..],
                    LocalModelType::WhisperLargeV3Turbo => {
                        &["turbo-tokens.txt", "tokens.txt"][..]
                    }
                    _ => unreachable!(),
                };

                let encoder = find_file(&self.model_dir, enc_candidates).ok_or_else(|| {
                    InferenceError::NotConfigured(format!(
                        "Whisper encoder not found in {}",
                        self.model_dir.display()
                    ))
                })?;
                let decoder = find_file(&self.model_dir, dec_candidates).ok_or_else(|| {
                    InferenceError::NotConfigured(format!(
                        "Whisper decoder not found in {}",
                        self.model_dir.display()
                    ))
                })?;
                let tokens = find_file(&self.model_dir, tok_candidates).ok_or_else(|| {
                    InferenceError::NotConfigured(format!(
                        "Whisper tokens not found in {}",
                        self.model_dir.display()
                    ))
                })?;

                let config = WhisperConfig {
                    encoder: encoder.to_string_lossy().to_string(),
                    decoder: decoder.to_string_lossy().to_string(),
                    tokens: tokens.to_string_lossy().to_string(),
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
            LocalModelType::MoonshineBase => {
                let preprocessor = find_file(&self.model_dir, &["preprocess.onnx"]).ok_or_else(|| {
                    InferenceError::NotConfigured(format!(
                        "Moonshine preprocessor not found in {}",
                        self.model_dir.display()
                    ))
                })?;
                let encoder = find_file(&self.model_dir, &["encode.int8.onnx"]).ok_or_else(|| {
                    InferenceError::NotConfigured(format!(
                        "Moonshine encoder not found in {}",
                        self.model_dir.display()
                    ))
                })?;
                let uncached_decoder = find_file(
                    &self.model_dir,
                    &["uncache_decode.int8.onnx", "uncached_decode.int8.onnx"],
                )
                .ok_or_else(|| {
                    InferenceError::NotConfigured(format!(
                        "Moonshine uncached decoder not found in {}",
                        self.model_dir.display()
                    ))
                })?;
                let cached_decoder =
                    find_file(&self.model_dir, &["cached_decode.int8.onnx"]).ok_or_else(|| {
                        InferenceError::NotConfigured(format!(
                            "Moonshine cached decoder not found in {}",
                            self.model_dir.display()
                        ))
                    })?;
                let tokens = find_file(&self.model_dir, &["tokens.txt"]).ok_or_else(|| {
                    InferenceError::NotConfigured(format!(
                        "Moonshine tokens not found in {}",
                        self.model_dir.display()
                    ))
                })?;

                let config = MoonshineConfig {
                    preprocessor: preprocessor.to_string_lossy().to_string(),
                    encoder: encoder.to_string_lossy().to_string(),
                    uncached_decoder: uncached_decoder.to_string_lossy().to_string(),
                    cached_decoder: cached_decoder.to_string_lossy().to_string(),
                    tokens: tokens.to_string_lossy().to_string(),
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
        }

        Ok(())
    }
}

impl SpeechInference for LocalSpeechInference {
    fn transcribe(&mut self, chunk: &AudioChunk) -> Result<String, InferenceError> {
        // Fast-path guardrail: Check verified model weights status from initialization without disk I/O
        if !self.model_verified {
            if let Err(err) = ModelManager::verify_model_files(self.model_type, &self.model_dir) {
                return Err(InferenceError::NotConfigured(format!(
                    "Local model weights missing at {}: {}. Run 'speechyyy model download' to install.",
                    self.model_dir.display(),
                    err
                )));
            }
            self.model_verified = true;
        }

        // Check if there is audio to transcribe
        if chunk.samples.is_empty() {
            return Ok(String::new());
        }

        // Lazily initialize and cache the ONNX recognizer session
        self.init_recognizer()?;

        let sample_rate = 16000;
        let text = match self.recognizer.as_mut().unwrap() {
            ActiveRecognizer::Moonshine(rec) => rec.transcribe(sample_rate, &chunk.samples).text,
            ActiveRecognizer::Whisper(rec) => rec.transcribe(sample_rate, &chunk.samples).text,
            ActiveRecognizer::Transducer(rec) => rec.transcribe(sample_rate, &chunk.samples),
            ActiveRecognizer::SenseVoice(rec) => rec.transcribe(sample_rate, &chunk.samples).text,
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

pub struct CustomSpeechInference {
    pub config: CustomModelConfig,
    recognizer: Option<ActiveRecognizer>,
}

impl CustomSpeechInference {
    pub fn new(config: CustomModelConfig) -> Self {
        Self {
            config,
            recognizer: None,
        }
    }

    fn init_recognizer(&mut self) -> Result<(), InferenceError> {
        if self.recognizer.is_some() {
            return Ok(());
        }

        match self.config.r#type.to_lowercase().as_str() {
            "transducer" | "nemo_transducer" => {
                let joiner = self.config.joiner.as_ref().ok_or_else(|| {
                    InferenceError::NotConfigured(
                        "Joiner path is required for custom transducer model".into(),
                    )
                })?;
                let is_nemo = self.config.r#type.to_lowercase() == "nemo_transducer";
                let cfg = TransducerConfig {
                    encoder: self.config.encoder.to_string_lossy().to_string(),
                    decoder: self.config.decoder.to_string_lossy().to_string(),
                    joiner: joiner.to_string_lossy().to_string(),
                    tokens: self.config.tokens.to_string_lossy().to_string(),
                    model_type: if is_nemo {
                        "nemo_transducer".to_string()
                    } else {
                        "transducer".to_string()
                    },
                    feature_dim: if is_nemo { 128 } else { 80 },
                    sample_rate: 16000,
                    provider: Some("cpu".to_string()),
                    num_threads: 2,
                    ..Default::default()
                };
                let rec = TransducerRecognizer::new(cfg).map_err(|e| {
                    InferenceError::LocalEngine(format!("Failed to init custom transducer: {}", e))
                })?;
                self.recognizer = Some(ActiveRecognizer::Transducer(rec));
            }
            "whisper" => {
                let cfg = WhisperConfig {
                    encoder: self.config.encoder.to_string_lossy().to_string(),
                    decoder: self.config.decoder.to_string_lossy().to_string(),
                    tokens: self.config.tokens.to_string_lossy().to_string(),
                    language: "en".to_string(),
                    bpe_vocab: None,
                    tail_paddings: None,
                    provider: Some("cpu".to_string()),
                    num_threads: Some(2),
                    debug: false,
                };
                let rec = WhisperRecognizer::new(cfg).map_err(|e| {
                    InferenceError::LocalEngine(format!("Failed to init custom whisper: {}", e))
                })?;
                self.recognizer = Some(ActiveRecognizer::Whisper(rec));
            }
            "moonshine" => {
                let parent = self.config.encoder.parent().unwrap_or(Path::new("."));
                let preprocessor = if parent.join("preprocess.onnx").exists() {
                    parent.join("preprocess.onnx")
                } else {
                    self.config.encoder.clone()
                };
                let cached_decoder = self.config.joiner.clone().unwrap_or_else(|| {
                    if parent.join("cached_decode.int8.onnx").exists() {
                        parent.join("cached_decode.int8.onnx")
                    } else {
                        self.config.decoder.clone()
                    }
                });
                let cfg = MoonshineConfig {
                    preprocessor: preprocessor.to_string_lossy().to_string(),
                    encoder: self.config.encoder.to_string_lossy().to_string(),
                    uncached_decoder: self.config.decoder.to_string_lossy().to_string(),
                    cached_decoder: cached_decoder.to_string_lossy().to_string(),
                    tokens: self.config.tokens.to_string_lossy().to_string(),
                    provider: Some("cpu".to_string()),
                    num_threads: Some(2),
                    debug: false,
                };
                let rec = MoonshineRecognizer::new(cfg).map_err(|e| {
                    InferenceError::LocalEngine(format!("Failed to init custom moonshine: {}", e))
                })?;
                self.recognizer = Some(ActiveRecognizer::Moonshine(rec));
            }
            "sensevoice" => {
                let cfg = SenseVoiceConfig {
                    model: self.config.encoder.to_string_lossy().to_string(),
                    tokens: self.config.tokens.to_string_lossy().to_string(),
                    language: "auto".to_string(),
                    use_itn: true,
                    provider: Some("cpu".to_string()),
                    num_threads: Some(2),
                    debug: false,
                };
                let rec = SenseVoiceRecognizer::new(cfg).map_err(|e| {
                    InferenceError::LocalEngine(format!("Failed to init custom sensevoice: {}", e))
                })?;
                self.recognizer = Some(ActiveRecognizer::SenseVoice(rec));
            }
            other => {
                return Err(InferenceError::NotConfigured(format!(
                    "Unsupported custom model type '{}'. Supported types: whisper, transducer, moonshine, sensevoice",
                    other
                )));
            }
        }

        Ok(())
    }
}

impl SpeechInference for CustomSpeechInference {
    fn transcribe(&mut self, chunk: &AudioChunk) -> Result<String, InferenceError> {
        if chunk.samples.is_empty() {
            return Ok(String::new());
        }

        self.init_recognizer()?;

        let sample_rate = 16000;
        let text = match self.recognizer.as_mut().unwrap() {
            ActiveRecognizer::Moonshine(rec) => rec.transcribe(sample_rate, &chunk.samples).text,
            ActiveRecognizer::Whisper(rec) => rec.transcribe(sample_rate, &chunk.samples).text,
            ActiveRecognizer::Transducer(rec) => rec.transcribe(sample_rate, &chunk.samples),
            ActiveRecognizer::SenseVoice(rec) => rec.transcribe(sample_rate, &chunk.samples).text,
        };

        let trimmed = text.trim();
        eprintln!(
            "Custom INT8 decoder ({}) decoded {:.2}s chunk: \"{}\"",
            self.config.r#type,
            chunk.duration_secs(),
            trimmed
        );

        Ok(trimmed.to_string())
    }

    fn provider_name(&self) -> &'static str {
        "custom"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_missing_model_returns_not_configured() {
        let temp_dir =
            std::env::temp_dir().join(format!("non_existent_model_{}", std::process::id()));
        let mut local = LocalSpeechInference::new(LocalModelType::ParakeetTdtV3, temp_dir);

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
    fn test_custom_transducer_joiner_required() {
        let temp_dir = std::env::temp_dir().join(format!("custom_test_{}", std::process::id()));
        let config = CustomModelConfig {
            r#type: "nemo_transducer".into(),
            encoder: temp_dir.join("enc.onnx"),
            decoder: temp_dir.join("dec.onnx"),
            joiner: None,
            tokens: temp_dir.join("tok.txt"),
        };
        let mut custom = CustomSpeechInference::new(config);
        let err = custom.init_recognizer().unwrap_err();
        match err {
            InferenceError::NotConfigured(msg) => {
                assert!(msg.contains("Joiner path is required"));
            }
            other => panic!("Expected NotConfigured for missing joiner, got {:?}", other),
        }
    }
}
