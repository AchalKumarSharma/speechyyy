use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A secret string (such as an API token) whose `Debug` implementation is
/// redacted to prevent accidental disclosure in logs, traces, or diagnostics.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(val: impl Into<String>) -> Self {
        Self(val.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.trim().is_empty()
    }

    pub fn trim(&self) -> &str {
        self.0.trim()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[redacted]")
    }
}

impl From<String> for Secret {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for Secret {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl PartialEq<&str> for Secret {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<Secret> for &str {
    fn eq(&self, other: &Secret) -> bool {
        *self == other.0
    }
}

impl PartialEq<str> for Secret {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<Secret> for str {
    fn eq(&self, other: &Secret) -> bool {
        self == other.0
    }
}



impl PartialEq<String> for Secret {
    fn eq(&self, other: &String) -> bool {
        self.0 == *other
    }
}

impl PartialEq<Secret> for String {
    fn eq(&self, other: &Secret) -> bool {
        *self == other.0
    }
}

impl Serialize for Secret {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Secret)
    }
}

/// Encodes 16 kHz mono f32 samples into a canonical in-memory WAV byte buffer.
pub fn build_in_memory_wav(samples: &[f32]) -> Vec<u8> {
    crate::audio::wav::encode_wav_16k_mono(samples)
}

/// Constructs RFC 7578 multipart/form-data payloads.
#[derive(Debug, Clone)]
pub struct MultipartBuilder {
    boundary: String,
    body: Vec<u8>,
}

impl Default for MultipartBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl MultipartBuilder {
    pub fn new() -> Self {
        let boundary = format!(
            "----Speechyyy{:x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        Self {
            boundary,
            body: Vec::new(),
        }
    }

    pub fn with_boundary(boundary: impl Into<String>) -> Self {
        Self {
            boundary: boundary.into(),
            body: Vec::new(),
        }
    }

    pub fn boundary(&self) -> &str {
        &self.boundary
    }

    pub fn content_type(&self) -> String {
        format!("multipart/form-data; boundary={}", self.boundary)
    }

    pub fn add_file(
        mut self,
        field_name: &str,
        filename: &str,
        content_type: &str,
        data: &[u8],
    ) -> Self {
        self.body
            .extend_from_slice(format!("--{}\r\n", self.boundary).as_bytes());
        self.body.extend_from_slice(
            format!(
                "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
                field_name, filename
            )
            .as_bytes(),
        );
        self.body
            .extend_from_slice(format!("Content-Type: {}\r\n\r\n", content_type).as_bytes());
        self.body.extend_from_slice(data);
        self.body.extend_from_slice(b"\r\n");
        self
    }

    pub fn add_text(mut self, field_name: &str, value: &str) -> Self {
        self.body
            .extend_from_slice(format!("--{}\r\n", self.boundary).as_bytes());
        self.body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n", field_name).as_bytes(),
        );
        self.body.extend_from_slice(value.as_bytes());
        self.body.extend_from_slice(b"\r\n");
        self
    }

    pub fn finish(mut self) -> (String, Vec<u8>) {
        self.body
            .extend_from_slice(format!("--{}--\r\n", self.boundary).as_bytes());
        (self.boundary, self.body)
    }

    pub fn finish_body(mut self) -> Vec<u8> {
        self.body
            .extend_from_slice(format!("--{}--\r\n", self.boundary).as_bytes());
        self.body
    }
}

/// Constructs a valid RFC 7578 multipart/form-data payload with boundaries for Sarvam AI.
pub fn build_multipart_body(
    boundary: &str,
    wav_bytes: &[u8],
    model: &str,
    mode: &str,
    language_code: &str,
) -> Vec<u8> {
    let mut builder = MultipartBuilder::with_boundary(boundary)
        .add_file("file", "audio.wav", "audio/wav", wav_bytes)
        .add_text("model", model)
        .add_text("mode", mode);

    if !language_code.is_empty() && language_code != "unknown" {
        builder = builder.add_text("language_code", language_code);
    }

    builder.finish_body()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_debug_redaction() {
        let sec = Secret::new("sk_live_very_secret_token_12345");
        let debug_str = format!("{:?}", sec);
        assert_eq!(debug_str, "[redacted]");
        assert!(!debug_str.contains("secret"));
        assert!(!debug_str.contains("12345"));
        assert_eq!(sec.expose(), "sk_live_very_secret_token_12345");
    }

    #[test]
    fn test_secret_serde_transparent() {
        let original = Secret::new("secret_payload_abc");
        let serialized = serde_json::to_string(&original).unwrap();
        assert_eq!(serialized, "\"secret_payload_abc\"");

        let deserialized: Secret = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.expose(), "secret_payload_abc");
        assert_eq!(deserialized, "secret_payload_abc");
    }

    #[test]
    fn test_secret_comparisons_and_mutation() {
        let mut s = Secret::new(" hello ");
        assert_eq!(s.trim(), "hello");
        assert!(!s.is_empty());
        assert_eq!(s, " hello ");
        assert_eq!(s, String::from(" hello "));

        s.clear();
        assert!(s.is_empty());
    }

    #[test]
    fn test_build_in_memory_wav() {
        let samples = vec![0.0f32; 160];
        let bytes = build_in_memory_wav(&samples);
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[12..16], b"fmt ");
        assert_eq!(&bytes[36..40], b"data");
        // 44-byte header + 160 * 2 bytes = 364 bytes
        assert_eq!(bytes.len(), 44 + 320);
    }

    #[test]
    fn test_multipart_builder() {
        let (boundary, body) = MultipartBuilder::with_boundary("custom_boundary_999")
            .add_file("file", "test.wav", "audio/wav", b"RIFFTEST")
            .add_text("model", "whisper-large-v3-turbo")
            .add_text("response_format", "json")
            .finish();

        assert_eq!(boundary, "custom_boundary_999");
        let body_str = String::from_utf8_lossy(&body);
        assert!(body_str.contains("--custom_boundary_999\r\n"));
        assert!(body_str.contains("Content-Disposition: form-data; name=\"file\"; filename=\"test.wav\"\r\n"));
        assert!(body_str.contains("Content-Type: audio/wav\r\n\r\nRIFFTEST\r\n"));
        assert!(body_str.contains("name=\"model\"\r\n\r\nwhisper-large-v3-turbo\r\n"));
        assert!(body_str.contains("name=\"response_format\"\r\n\r\njson\r\n"));
        assert!(body_str.ends_with("--custom_boundary_999--\r\n"));
    }
}
