use crate::audio::AudioChunk;
use crate::inference::wire::{build_in_memory_wav, Secret};
use crate::inference::{InferenceError, SpeechInference};
use serde::Deserialize;

pub use crate::inference::wire::build_multipart_body;

pub const DEFAULT_SARVAM_ENDPOINT: &str = "https://api.sarvam.ai/speech-to-text";
pub const DEFAULT_SARVAM_MODEL: &str = "saaras:v4";
pub const DEFAULT_SARVAM_MODE: &str = "translit";
pub const DEFAULT_SARVAM_LANGUAGE: &str = "unknown";

/// Resolves the Sarvam request timeout, defaulting 0 or None to 30 seconds.
pub fn resolve_request_timeout(timeout_secs: Option<u64>) -> std::time::Duration {
    match timeout_secs {
        Some(s) if s > 0 => std::time::Duration::from_secs(s),
        _ => std::time::Duration::from_secs(30),
    }
}

#[derive(Debug)]
pub struct SarvamSpeechInference {
    pub api_key: Secret,
    pub model: String,
    pub language_code: String,
    pub mode: String,
    pub endpoint: String,
    pub timeout: std::time::Duration,
}

impl SarvamSpeechInference {
    pub fn new(api_key: Secret, model: String, language_code: String, mode: String) -> Self {
        let sanitized_key = Secret::new(api_key.expose().trim().trim_matches('"'));
        Self {
            api_key: sanitized_key,
            model,
            language_code,
            mode,
            endpoint: DEFAULT_SARVAM_ENDPOINT.to_string(),
            timeout: resolve_request_timeout(None),
        }
    }

    pub fn from_config(config: &crate::config::InferenceConfig) -> Result<Self, InferenceError> {
        let api_key = config.resolve_api_key().ok_or_else(|| {
            InferenceError::MissingApiKey("SARVAM_API_KEY is not configured".to_string())
        })?;
        let timeout = resolve_request_timeout(config.request_timeout_secs);
        Ok(Self::new(
            Secret::new(api_key),
            config.model.clone(),
            config.language_code.clone(),
            config.mode.clone(),
        )
        .with_timeout(timeout))
    }

    pub fn with_endpoint(mut self, endpoint: &str) -> Self {
        self.endpoint = endpoint.to_string();
        self
    }

    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

#[derive(Debug, Deserialize)]
pub struct SarvamResponse {
    pub request_id: Option<String>,
    pub transcript: String,
    pub language_code: Option<String>,
}

impl SpeechInference for SarvamSpeechInference {
    fn transcribe(&mut self, chunk: &AudioChunk) -> Result<String, InferenceError> {
        if self.api_key.is_empty() {
            return Err(InferenceError::MissingApiKey("SARVAM_API_KEY".to_string()));
        }

        let wav_bytes = build_in_memory_wav(&chunk.samples);
        let boundary = format!(
            "----Speechyyy{:x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let body = build_multipart_body(
            &boundary,
            &wav_bytes,
            &self.model,
            &self.mode,
            &self.language_code,
        );

        let agent = ureq::builder()
            .timeout_connect(std::time::Duration::from_secs(10))
            .timeout(self.timeout)
            .build();

        let response = agent
            .post(&self.endpoint)
            .set("api-subscription-key", self.api_key.expose())
            .set(
                "Content-Type",
                &format!("multipart/form-data; boundary={}", boundary),
            )
            .send_bytes(&body)
            .map_err(|err| match err {
                ureq::Error::Status(status, resp) => {
                    let msg = resp
                        .into_string()
                        .unwrap_or_else(|_| "Unknown HTTP error".into());
                    match status {
                        401 => {
                            InferenceError::Authentication(
                                "Invalid or expired Sarvam API key. Check 'speechyyy doctor' or replenish credentials at https://dashboard.sarvam.ai".into(),
                            )
                        }
                        429 => {
                            let details = if msg.is_empty() {
                                String::new()
                            } else {
                                format!(": {}", msg)
                            };
                            InferenceError::RateLimited(format!(
                                "Sarvam AI rate limit exceeded or credit balance exhausted{}",
                                details
                            ))
                        }
                        _ => InferenceError::Http {
                            status,
                            message: msg,
                        },
                    }
                }
                ureq::Error::Transport(e) => {
                    let msg = e.to_string();
                    let lower = msg.to_lowercase();
                    let is_timeout = lower.contains("timed out")
                        || lower.contains("timeout")
                        || lower.contains("deadline")
                        || std::error::Error::source(&e)
                            .and_then(|s| s.downcast_ref::<std::io::Error>())
                            .map(|io| io.kind() == std::io::ErrorKind::TimedOut)
                            .unwrap_or(false);

                    if is_timeout {
                        InferenceError::Timeout(format!(
                            "Sarvam AI connection timed out after {:?}. Check your network connection or raise 'request_timeout_secs' in config.toml",
                            self.timeout
                        ))
                    } else {
                        InferenceError::Network(msg)
                    }
                }
            })?;

        let parsed: SarvamResponse = response
            .into_json()
            .map_err(|e| InferenceError::Serialization(e.to_string()))?;

        Ok(parsed.transcript.trim().to_string())
    }

    fn provider_name(&self) -> &'static str {
        "sarvam"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multipart_body_construction() {
        let boundary = "test_boundary_12345";
        let wav = b"RIFFFAKEWAVEDATA";
        let body = build_multipart_body(boundary, wav, "saaras:v4", "transcribe", "hi-IN");

        let body_str = String::from_utf8_lossy(&body);
        assert!(body_str.contains("--test_boundary_12345\r\n"));
        assert!(body_str
            .contains("Content-Disposition: form-data; name=\"file\"; filename=\"audio.wav\"\r\n"));
        assert!(body_str.contains("Content-Type: audio/wav\r\n\r\nRIFFFAKEWAVEDATA\r\n"));
        assert!(body_str.contains("name=\"model\"\r\n\r\nsaaras:v4\r\n"));
        assert!(body_str.contains("name=\"mode\"\r\n\r\ntranscribe\r\n"));
        assert!(body_str.contains("name=\"language_code\"\r\n\r\nhi-IN\r\n"));
        assert!(body_str.ends_with("--test_boundary_12345--\r\n"));
    }

    #[test]
    fn test_multipart_body_omits_unknown_language() {
        let boundary = "bound_xyz";
        let wav = b"data";
        let body = build_multipart_body(boundary, wav, "saaras:v4", "codemix", "unknown");

        let body_str = String::from_utf8_lossy(&body);
        assert!(!body_str.contains("name=\"language_code\""));
        assert!(body_str.contains("name=\"mode\"\r\n\r\ncodemix\r\n"));
    }

    #[test]
    fn test_sarvam_response_json_deserialization() {
        let json_data = r#"{
            "request_id": "20261030_req_123",
            "transcript": "Hello and welcome to Speechyyy",
            "language_code": "en-IN"
        }"#;

        let parsed: SarvamResponse = serde_json::from_str(json_data).unwrap();
        assert_eq!(parsed.transcript, "Hello and welcome to Speechyyy");
        assert_eq!(parsed.language_code, Some("en-IN".into()));
        assert_eq!(parsed.request_id, Some("20261030_req_123".into()));
    }

    #[test]
    fn test_missing_api_key_returns_error() {
        let mut sarvam = SarvamSpeechInference::new(
            "".into(),
            "saaras:v4".into(),
            "unknown".into(),
            "transcribe".into(),
        );

        let chunk = AudioChunk::new(vec![0.1; 1600]);
        let err = sarvam.transcribe(&chunk).unwrap_err();
        match err {
            InferenceError::MissingApiKey(key_name) => {
                assert_eq!(key_name, "SARVAM_API_KEY");
            }
            other => panic!("Expected MissingApiKey, got {:?}", other),
        }
    }

    #[test]
    fn test_api_key_sanitization_trims_quotes_and_whitespace() {
        let sarvam = SarvamSpeechInference::new(
            "  \"my_secret_token_123\"  ".into(),
            "saaras:v4".into(),
            "unknown".into(),
            "transcribe".into(),
        );
        assert_eq!(sarvam.api_key, "my_secret_token_123");

        let mut sarvam_empty_quotes = SarvamSpeechInference::new(
            "  \"\"  ".into(),
            "saaras:v4".into(),
            "unknown".into(),
            "transcribe".into(),
        );
        let chunk = AudioChunk::new(vec![0.1; 1600]);
        let err = sarvam_empty_quotes.transcribe(&chunk).unwrap_err();
        assert!(matches!(err, InferenceError::MissingApiKey(_)));
    }
}
