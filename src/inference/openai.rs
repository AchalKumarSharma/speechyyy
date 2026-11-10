use std::time::Duration;

use crate::audio::AudioChunk;
use crate::inference::wire::{build_in_memory_wav, MultipartBuilder, Secret};
use crate::inference::{InferenceError, SpeechInference};
use serde::Deserialize;

pub const DEFAULT_GROQ_ENDPOINT: &str = "https://api.groq.com/openai/v1/audio/transcriptions";
pub const DEFAULT_GROQ_MODEL: &str = "whisper-large-v3-turbo";
pub const GROQ_ENV_VAR: &str = "GROQ_API_KEY";

pub const DEFAULT_OPENAI_ENDPOINT: &str = "https://api.openai.com/v1/audio/transcriptions";
pub const DEFAULT_OPENAI_MODEL: &str = "whisper-1";
pub const OPENAI_ENV_VAR: &str = "OPENAI_API_KEY";

pub const DEFAULT_OPENAI_COMPATIBLE_MODEL: &str = "whisper-1";
pub const OPENAI_COMPATIBLE_ENV_VAR: &str = "OPENAI_COMPATIBLE_API_KEY";

pub const MAX_AUDIO_BYTES: usize = 24 * 1024 * 1024; // 24 MB size guard

/// Returns true if the host is a local loopback address (localhost, 127.0.0.1, [::1], ::1).
pub fn is_loopback_host(host: &str) -> bool {
    let host_lower = host.trim().to_ascii_lowercase();
    host_lower == "localhost"
        || host_lower == "127.0.0.1"
        || host_lower == "::1"
        || host_lower == "[::1]"
}

/// Validates that an endpoint URL conforms to the security policy:
/// - `https://` is always allowed.
/// - `http://` is allowed ONLY when the host is loopback (`localhost`, `127.0.0.1`, `[::1]`).
/// - Reject any other scheme and any URL containing userinfo (`user@host`).
pub fn validate_endpoint(url: &str) -> Result<String, String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("Endpoint URL cannot be empty".to_string());
    }

    let (scheme, rest) = if let Some(r) = trimmed.strip_prefix("https://") {
        ("https", r)
    } else if let Some(r) = trimmed.strip_prefix("http://") {
        ("http", r)
    } else {
        return Err(format!(
            "Unsupported endpoint scheme in '{}'. Only 'https://' and local 'http://' loopback are supported.",
            trimmed
        ));
    };

    // Authority is everything up to the first '/', '?', or '#'
    let authority = match rest.find(['/', '?', '#']) {
        Some(idx) => &rest[..idx],
        None => rest,
    };

    if authority.is_empty() {
        return Err("Endpoint URL contains an empty host".to_string());
    }

    // Reject userinfo (e.g. user@host or user:pass@host)
    if authority.contains('@') {
        return Err("Endpoint URL must not contain credentials or userinfo (e.g. 'user@host')".to_string());
    }

    // Parse host and optional port
    let host = if authority.starts_with('[') {
        let close_idx = authority.find(']').ok_or_else(|| {
            "Endpoint URL contains malformed IPv6 host (missing closing ']')".to_string()
        })?;
        let ipv6 = &authority[1..close_idx];
        let after_bracket = &authority[close_idx + 1..];
        if !after_bracket.is_empty() {
            if !after_bracket.starts_with(':') {
                return Err("Malformed port after IPv6 host".to_string());
            }
            let port_str = &after_bracket[1..];
            if port_str.is_empty() || port_str.parse::<u16>().is_err() {
                return Err(format!("Invalid port '{}' in endpoint", port_str));
            }
        }
        ipv6
    } else {
        match authority.rfind(':') {
            Some(colon_idx) => {
                let port_str = &authority[colon_idx + 1..];
                if port_str.is_empty() || port_str.parse::<u16>().is_err() {
                    return Err(format!("Invalid port '{}' in endpoint", port_str));
                }
                &authority[..colon_idx]
            }
            None => authority,
        }
    };

    let host_lower = host.to_ascii_lowercase();
    if host_lower.is_empty() {
        return Err("Endpoint host cannot be empty".to_string());
    }

    if scheme == "http" && !is_loopback_host(&host_lower) {
        return Err(format!(
            "Insecure HTTP endpoint '{}' rejected. Plain HTTP is only allowed for local loopback (localhost, 127.0.0.1, [::1]). Use HTTPS for remote endpoints.",
            trimmed
        ));
    }

    Ok(trimmed.to_string())
}

/// Extracts the host string from an endpoint URL for diagnostic reporting.
pub fn extract_endpoint_host(url: &str) -> Option<String> {
    let trimmed = url.trim();
    let rest = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))?;
    let authority = match rest.find(['/', '?', '#']) {
        Some(idx) => &rest[..idx],
        None => rest,
    };
    let authority = match authority.rfind('@') {
        Some(idx) => &authority[idx + 1..],
        None => authority,
    };
    if authority.starts_with('[') {
        let close_idx = authority.find(']')?;
        Some(authority[1..close_idx].to_string())
    } else {
        match authority.rfind(':') {
            Some(idx) => Some(authority[..idx].to_string()),
            None => Some(authority.to_string()),
        }
    }
}

/// Speech inference engine compatible with OpenAI Audio Transcriptions API format
/// (`/v1/audio/transcriptions`), used for Groq, OpenAI, and self-hosted endpoints.
#[derive(Debug)]
pub struct OpenAiSpeechInference {
    pub provider: String,
    pub api_key: Option<Secret>,
    pub endpoint: String,
    pub model: String,
    pub language: Option<String>,
    pub timeout: Duration,
}

impl OpenAiSpeechInference {
    pub fn new(
        provider: impl Into<String>,
        api_key: Option<Secret>,
        endpoint: impl Into<String>,
        model: impl Into<String>,
        language: Option<String>,
        timeout: Duration,
    ) -> Result<Self, String> {
        let endpoint_str = endpoint.into();
        let validated_endpoint = validate_endpoint(&endpoint_str)?;
        let provider_str = provider.into();
        let sanitized_key = api_key.and_then(|k| {
            let trimmed = k.expose().trim().trim_matches('"');
            if trimmed.is_empty() {
                None
            } else {
                Some(Secret::new(trimmed))
            }
        });

        Ok(Self {
            provider: provider_str,
            api_key: sanitized_key,
            endpoint: validated_endpoint,
            model: model.into(),
            language,
            timeout,
        })
    }
}

#[derive(Deserialize)]
struct OpenAiResponse {
    text: Option<String>,
}

impl SpeechInference for OpenAiSpeechInference {
    fn transcribe(&mut self, chunk: &AudioChunk) -> Result<String, InferenceError> {
        // Fast pre-allocation size check (16-bit mono PCM = 2 bytes/sample + 44 bytes header)
        let estimated_wav_len = chunk.samples.len() * 2 + 44;
        if estimated_wav_len > MAX_AUDIO_BYTES {
            return Err(InferenceError::Serialization(format!(
                "Audio payload size ({} bytes) exceeds the 24 MB limit ({} bytes)",
                estimated_wav_len,
                MAX_AUDIO_BYTES
            )));
        }

        let wav_bytes = build_in_memory_wav(&chunk.samples);
        if wav_bytes.len() > MAX_AUDIO_BYTES {
            return Err(InferenceError::Serialization(format!(
                "Audio payload size ({} bytes) exceeds the 24 MB limit ({} bytes)",
                wav_bytes.len(),
                MAX_AUDIO_BYTES
            )));
        }

        let mut builder = MultipartBuilder::new()
            .add_file("file", "audio.wav", "audio/wav", &wav_bytes)
            .add_text("model", &self.model)
            .add_text("response_format", "json");

        if let Some(ref lang) = self.language {
            let trimmed = lang.trim();
            if !trimmed.is_empty() && trimmed != "unknown" {
                builder = builder.add_text("language", trimmed);
            }
        }

        let (boundary, body) = builder.finish();

        let agent = ureq::builder()
            .redirects(0)
            .timeout_connect(Duration::from_secs(10))
            .timeout(self.timeout)
            .build();

        let mut req = agent
            .post(&self.endpoint)
            .set("Content-Type", &format!("multipart/form-data; boundary={}", boundary));

        if let Some(ref key) = self.api_key {
            req = req.set("Authorization", &format!("Bearer {}", key.expose()));
        }

        let response = req.send_bytes(&body).map_err(|err| match err {
            ureq::Error::Status(status, resp) => {
                let retry_after = resp.header("Retry-After").map(String::from);
                let msg = resp
                    .into_string()
                    .unwrap_or_else(|_| "Unknown HTTP error".into());
                match status {
                    401 | 403 => InferenceError::Authentication(format!(
                        "API key rejected by {}. Run 'speechyyy use {}' to update your key.",
                        self.provider, self.provider
                    )),
                    429 => {
                        let retry_msg = match retry_after {
                            Some(ra) => format!(" (Retry-After: {})", ra),
                            None => String::new(),
                        };
                        InferenceError::RateLimited(format!(
                            "{} rate limit exceeded{}",
                            self.provider, retry_msg
                        ))
                    }
                    300..=399 => InferenceError::Http {
                        status,
                        message: format!(
                            "Redirect HTTP {} received from {} but redirects are disabled for security",
                            status, self.provider
                        ),
                    },
                    500..=599 => InferenceError::Http {
                        status,
                        message: format!("{} provider error (HTTP {})", self.provider, status),
                    },
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
                        "{} request timed out after {:?}. Check your network connection.",
                        self.provider, self.timeout
                    ))
                } else {
                    InferenceError::Network(format!(
                        "Failed to connect to {}: {}",
                        self.provider, msg
                    ))
                }
            }
        })?;

        let status = response.status();
        if (300..=399).contains(&status) {
            return Err(InferenceError::Http {
                status,
                message: format!(
                    "Redirect HTTP {} received from {} but redirects are disabled for security",
                    status, self.provider
                ),
            });
        }

        let parsed: OpenAiResponse = response
            .into_json()
            .map_err(|e| InferenceError::Serialization(format!("Failed to parse response JSON: {}", e)))?;

        match parsed.text {
            Some(text) => Ok(text.trim().to_string()),
            None => Err(InferenceError::Serialization(
                "Missing 'text' field in transcription response".to_string(),
            )),
        }
    }

    fn provider_name(&self) -> &'static str {
        match self.provider.as_str() {
            "groq" => "groq",
            "openai" => "openai",
            "openai-compatible" => "openai-compatible",
            _ => "openai-compatible",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_endpoint_valid_https() {
        assert!(validate_endpoint("https://api.groq.com/openai/v1/audio/transcriptions").is_ok());
        assert!(validate_endpoint("https://api.openai.com/v1/audio/transcriptions").is_ok());
        assert!(validate_endpoint("https://my-custom-host.internal/v1").is_ok());
    }

    #[test]
    fn test_validate_endpoint_valid_http_loopback() {
        assert!(validate_endpoint("http://localhost:11434/v1/audio/transcriptions").is_ok());
        assert!(validate_endpoint("http://127.0.0.1:8080/v1").is_ok());
        assert!(validate_endpoint("http://[::1]:8000/v1").is_ok());
    }

    #[test]
    fn test_validate_endpoint_rejects_non_loopback_http() {
        assert!(validate_endpoint("http://api.groq.com/v1").is_err());
        assert!(validate_endpoint("http://evil.com/v1").is_err());
        assert!(validate_endpoint("http://localhost.evil.com:8080/v1").is_err());
        assert!(validate_endpoint("http://127.0.0.1.evil.com/v1").is_err());
    }

    #[test]
    fn test_validate_endpoint_rejects_unsupported_scheme() {
        assert!(validate_endpoint("ftp://localhost:8080/v1").is_err());
        assert!(validate_endpoint("ws://localhost:8080/v1").is_err());
        assert!(validate_endpoint("localhost:8080/v1").is_err());
    }

    #[test]
    fn test_validate_endpoint_rejects_userinfo() {
        assert!(validate_endpoint("https://user@api.groq.com/v1").is_err());
        assert!(validate_endpoint("http://user:pass@localhost:8080/v1").is_err());
    }

    #[test]
    fn test_extract_endpoint_host() {
        assert_eq!(
            extract_endpoint_host("https://api.groq.com/openai/v1/audio/transcriptions"),
            Some("api.groq.com".into())
        );
        assert_eq!(
            extract_endpoint_host("http://localhost:11434/v1"),
            Some("localhost".into())
        );
        assert_eq!(
            extract_endpoint_host("http://127.0.0.1:8080/v1"),
            Some("127.0.0.1".into())
        );
        assert_eq!(
            extract_endpoint_host("http://[::1]:8000/v1"),
            Some("::1".into())
        );
    }
}
