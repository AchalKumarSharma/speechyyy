pub mod local;
pub mod mock;
pub mod openai;
pub mod sarvam;
pub mod wire;

pub use openai::{
    extract_endpoint_host, is_loopback_host, validate_endpoint, OpenAiSpeechInference, DEFAULT_GROQ_ENDPOINT,
    DEFAULT_GROQ_MODEL, DEFAULT_OPENAI_COMPATIBLE_MODEL, DEFAULT_OPENAI_ENDPOINT,
    DEFAULT_OPENAI_MODEL, GROQ_ENV_VAR, MAX_AUDIO_BYTES, OPENAI_COMPATIBLE_ENV_VAR, OPENAI_ENV_VAR,
};
pub use wire::Secret;

use crate::audio::AudioChunk;

#[derive(Debug, Clone, PartialEq)]
pub enum InferenceError {
    MissingApiKey(String),
    Authentication(String),
    RateLimited(String),
    Timeout(String),
    Network(String),
    Http { status: u16, message: String },
    Serialization(String),
    NotConfigured(String),
    LocalEngine(String),
}

impl std::fmt::Display for InferenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingApiKey(msg) => write!(f, "Missing API key: {}", msg),
            Self::Authentication(msg) => write!(f, "Authentication error: {}", msg),
            Self::RateLimited(msg) => write!(f, "Rate limit exceeded: {}", msg),
            Self::Timeout(msg) => write!(f, "Inference timeout: {}", msg),
            Self::Network(msg) => write!(f, "Network error: {}", msg),
            Self::Http { status, message } => write!(f, "HTTP error {}: {}", status, message),
            Self::Serialization(msg) => write!(f, "Serialization error: {}", msg),
            Self::NotConfigured(msg) => write!(f, "Inference provider not configured: {}", msg),
            Self::LocalEngine(msg) => write!(f, "Local inference engine error: {}", msg),
        }
    }
}

impl std::error::Error for InferenceError {}

pub trait SpeechInference: Send {
    fn transcribe(&mut self, chunk: &AudioChunk) -> Result<String, InferenceError>;
    fn provider_name(&self) -> &'static str;
}

impl SpeechInference for Box<dyn SpeechInference> {
    fn transcribe(&mut self, chunk: &AudioChunk) -> Result<String, InferenceError> {
        (**self).transcribe(chunk)
    }

    fn provider_name(&self) -> &'static str {
        (**self).provider_name()
    }
}

pub fn create_inference_provider(
    config: &crate::config::InferenceConfig,
) -> Result<Box<dyn SpeechInference>, String> {
    let prov = config.provider.to_lowercase();
    if prov != "mock"
        && prov != "custom"
        && prov != "groq"
        && prov != "openai"
        && prov != "openai-compatible"
        && config.model.trim().is_empty()
    {
        return Err(
            "No active model configured. Run 'speechyyy use <model>' to activate a model.".to_string(),
        );
    }
    match prov.as_str() {
        "mock" => Ok(Box::new(mock::MockSpeechInference::new())),
        "sarvam" => {
            let (api_key, _) = config.resolve_key_for_provider("sarvam").ok_or_else(|| {
                "SARVAM_API_KEY is not configured in config.toml or environment variable. Check 'speechyyy doctor'.".to_string()
            })?;
            Ok(Box::new(sarvam::SarvamSpeechInference::new(
                api_key,
                config.model.clone(),
                config.language_code.clone(),
                config.mode.clone(),
            )))
        }
        "groq" => {
            let (api_key, _) = config.resolve_key_for_provider("groq").ok_or_else(|| {
                "GROQ_API_KEY is not configured in config.toml or environment variable. Run 'speechyyy use groq' to configure it.".to_string()
            })?;
            let model = if !config.model.trim().is_empty() {
                config.model.clone()
            } else {
                DEFAULT_GROQ_MODEL.to_string()
            };
            let timeout = std::time::Duration::from_secs(config.request_timeout_secs.unwrap_or(30));
            let engine = OpenAiSpeechInference::new(
                "groq",
                Some(api_key),
                DEFAULT_GROQ_ENDPOINT,
                model,
                config.language.clone(),
                timeout,
            )?;
            Ok(Box::new(engine))
        }
        "openai" => {
            let (api_key, _) = config.resolve_key_for_provider("openai").ok_or_else(|| {
                "OPENAI_API_KEY is not configured in config.toml or environment variable. Run 'speechyyy use openai' to configure it.".to_string()
            })?;
            let model = if !config.model.trim().is_empty() {
                config.model.clone()
            } else {
                DEFAULT_OPENAI_MODEL.to_string()
            };
            let timeout = std::time::Duration::from_secs(config.request_timeout_secs.unwrap_or(30));
            let engine = OpenAiSpeechInference::new(
                "openai",
                Some(api_key),
                DEFAULT_OPENAI_ENDPOINT,
                model,
                config.language.clone(),
                timeout,
            )?;
            Ok(Box::new(engine))
        }
        "openai-compatible" => {
            let endpoint = config
                .openai_compatible_endpoint
                .as_ref()
                .filter(|e| !e.trim().is_empty())
                .ok_or_else(|| {
                    "OpenAI-compatible endpoint is not configured. Set 'openai_compatible_endpoint' in config.toml or run 'speechyyy use openai-compatible'.".to_string()
                })?;
            let key = config.resolve_key_for_provider("openai-compatible").map(|(k, _)| k);
            let model = config
                .openai_compatible_model
                .as_deref()
                .or_else(|| {
                    if !config.model.trim().is_empty() {
                        Some(config.model.as_str())
                    } else {
                        None
                    }
                })
                .unwrap_or(DEFAULT_OPENAI_COMPATIBLE_MODEL);
            let timeout = std::time::Duration::from_secs(config.request_timeout_secs.unwrap_or(30));
            let engine = OpenAiSpeechInference::new(
                "openai-compatible",
                key,
                endpoint,
                model,
                config.language.clone(),
                timeout,
            )?;
            Ok(Box::new(engine))
        }
        "local" => {
            let model_type = crate::model::LocalModelType::from_str_name(&config.model)
                .unwrap_or(crate::model::LocalModelType::ParakeetTdtV3);
            let model_dir = crate::model::ModelManager::model_dir(model_type);
            Ok(Box::new(local::LocalSpeechInference::new(
                model_type, model_dir,
            )))
        }
        "custom" => {
            let custom_cfg = config.custom.as_ref().ok_or_else(|| {
                "Inference provider is 'custom' but [inference.custom] is not configured in config.toml".to_string()
            })?;
            Ok(Box::new(local::CustomSpeechInference::new(custom_cfg.clone())))
        }
        #[cfg(target_os = "macos")]
        "apple" => {
            Ok(Box::new(mock::MockSpeechInference::new()))
        }
        other => Err(format!(
            "Unknown inference provider '{}'. Supported providers are: 'mock', 'sarvam', 'groq', 'openai', 'openai-compatible', 'local', 'custom'{}.",
            other,
            if cfg!(target_os = "macos") { ", 'apple'" } else { "" }
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inference_error_display() {
        assert_eq!(
            InferenceError::MissingApiKey("SARVAM_API_KEY".into()).to_string(),
            "Missing API key: SARVAM_API_KEY"
        );
        assert_eq!(
            InferenceError::Http {
                status: 401,
                message: "Unauthorized".into()
            }
            .to_string(),
            "HTTP error 401: Unauthorized"
        );
        assert_eq!(
            InferenceError::Network("Connection refused".into()).to_string(),
            "Network error: Connection refused"
        );
    }
}
