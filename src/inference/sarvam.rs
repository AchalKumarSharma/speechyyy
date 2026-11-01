use crate::audio::AudioChunk;
use crate::inference::wav::encode_wav_16k_mono;
use crate::inference::{InferenceError, SpeechInference};
use serde::Deserialize;

pub const DEFAULT_SARVAM_ENDPOINT: &str = "https://api.sarvam.ai/speech-to-text";
pub const DEFAULT_SARVAM_MODEL: &str = "saaras:v4";
pub const DEFAULT_SARVAM_MODE: &str = "translit";
pub const DEFAULT_SARVAM_LANGUAGE: &str = "unknown";

pub struct SarvamSpeechInference {
    pub api_key: String,
    pub model: String,
    pub language_code: String,
    pub mode: String,
    pub endpoint: String,
    pub timeout: Option<std::time::Duration>,
}

impl SarvamSpeechInference {
    pub fn new(api_key: String, model: String, language_code: String, mode: String) -> Self {
        Self {
            api_key,
            model,
            language_code,
            mode,
            endpoint: DEFAULT_SARVAM_ENDPOINT.to_string(),
            timeout: None,
        }
    }

    pub fn from_config(config: &crate::config::InferenceConfig) -> Result<Self, InferenceError> {
        let api_key = config.resolve_api_key().ok_or_else(|| {
            InferenceError::MissingApiKey("SARVAM_API_KEY is not configured".to_string())
        })?;
        Ok(Self::new(
            api_key,
            config.model.clone(),
            config.language_code.clone(),
            config.mode.clone(),
        ))
    }

    pub fn with_endpoint(mut self, endpoint: &str) -> Self {
        self.endpoint = endpoint.to_string();
        self
    }

    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

#[derive(Debug, Deserialize)]
pub struct SarvamResponse {
    pub request_id: Option<String>,
    pub transcript: String,
    pub language_code: Option<String>,
}

/// Constructs a valid RFC 7578 multipart/form-data payload with boundaries.
pub fn build_multipart_body(
    boundary: &str,
    wav_bytes: &[u8],
    model: &str,
    mode: &str,
    language_code: &str,
) -> Vec<u8> {
    let mut body = Vec::new();

    // 1. file field
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"audio.wav\"\r\n",
    );
    body.extend_from_slice(b"Content-Type: audio/wav\r\n\r\n");
    body.extend_from_slice(wav_bytes);
    body.extend_from_slice(b"\r\n");

    // 2. model field
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"model\"\r\n\r\n");
    body.extend_from_slice(model.as_bytes());
    body.extend_from_slice(b"\r\n");

    // 3. mode field
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"mode\"\r\n\r\n");
    body.extend_from_slice(mode.as_bytes());
    body.extend_from_slice(b"\r\n");

    // 4. language_code field (if not "unknown" or empty)
    if !language_code.is_empty() && language_code != "unknown" {
        body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"language_code\"\r\n\r\n");
        body.extend_from_slice(language_code.as_bytes());
        body.extend_from_slice(b"\r\n");
    }

    // End boundary
    body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());
    body
}

impl SpeechInference for SarvamSpeechInference {
    fn transcribe(&mut self, chunk: &AudioChunk) -> Result<String, InferenceError> {
        if self.api_key.trim().is_empty() {
            return Err(InferenceError::MissingApiKey("SARVAM_API_KEY".to_string()));
        }

        let wav_bytes = encode_wav_16k_mono(&chunk.samples);
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

        let mut agent_builder = ureq::builder();
        if let Some(timeout) = self.timeout {
            agent_builder = agent_builder.timeout(timeout);
        }
        let agent = agent_builder.build();

        let response = agent
            .post(&self.endpoint)
            .set("api-subscription-key", &self.api_key)
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
                            let details = if msg.is_empty() {
                                String::new()
                            } else {
                                format!(": {}", msg)
                            };
                            InferenceError::Authentication(format!(
                                "Invalid or expired Sarvam API key. Check 'speechyyy doctor' or replenish credentials at https://dashboard.sarvam.ai{}",
                                details
                            ))
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
                        let details = if msg.is_empty() {
                            String::new()
                        } else {
                            format!(": {}", msg)
                        };
                        InferenceError::Timeout(format!(
                            "Sarvam AI connection timed out. Check network connectivity{}",
                            details
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
}
