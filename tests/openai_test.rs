use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use speechyyy::audio::AudioChunk;
use speechyyy::config::{Config, InferenceConfig, KeySource};
use speechyyy::inference::openai::{
    validate_endpoint, OpenAiSpeechInference, DEFAULT_GROQ_MODEL,
    DEFAULT_OPENAI_COMPATIBLE_MODEL, DEFAULT_OPENAI_MODEL,
};
use speechyyy::inference::wire::{build_in_memory_wav, Secret};
use speechyyy::inference::{create_inference_provider, InferenceError, SpeechInference};
use speechyyy::model::resolve_model_use;

struct MockServer {
    addr: String,
    shutdown_tx: Option<Sender<()>>,
    request_received: Arc<AtomicBool>,
}

impl MockServer {
    fn spawn<F>(handler: F) -> Self
    where
        F: Fn(&str, &[u8]) -> (u16, Vec<(&'static str, &'static str)>, Vec<u8>) + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let local_addr = listener.local_addr().expect("local addr");
        let (shutdown_tx, shutdown_rx) = channel();
        let request_received = Arc::new(AtomicBool::new(false));
        let req_rec_clone = Arc::clone(&request_received);

        thread::spawn(move || {
            listener.set_nonblocking(true).unwrap();
            loop {
                if shutdown_rx.try_recv().is_ok() {
                    break;
                }
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        req_rec_clone.store(true, Ordering::SeqCst);
                        let mut buffer = Vec::new();
                        let mut temp = [0u8; 4096];
                        stream
                            .set_read_timeout(Some(Duration::from_millis(500)))
                            .ok();

                        let mut header_end = None;
                        let mut content_length = 0;
                        while let Ok(n) = stream.read(&mut temp) {
                            if n == 0 {
                                break;
                            }
                            buffer.extend_from_slice(&temp[..n]);
                            if let Some(pos) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
                                header_end = Some(pos + 4);
                                let header_str = String::from_utf8_lossy(&buffer[..pos]);
                                for line in header_str.lines() {
                                    if line.to_lowercase().starts_with("content-length:") {
                                        if let Ok(cl) = line[15..].trim().parse::<usize>() {
                                            content_length = cl;
                                        }
                                    }
                                }
                                break;
                            }
                        }

                        if let Some(hdr_end) = header_end {
                            while buffer.len() < hdr_end + content_length {
                                match stream.read(&mut temp) {
                                    Ok(n) if n > 0 => buffer.extend_from_slice(&temp[..n]),
                                    _ => break,
                                }
                            }
                            let headers_str =
                                String::from_utf8_lossy(&buffer[..hdr_end]).to_string();
                            let body = if buffer.len() >= hdr_end {
                                &buffer[hdr_end..]
                            } else {
                                &[][..]
                            };
                            let (status, resp_headers, resp_body) = handler(&headers_str, body);
                            let status_text = match status {
                                200 => "OK",
                                302 => "Found",
                                401 => "Unauthorized",
                                403 => "Forbidden",
                                429 => "Too Many Requests",
                                500 => "Internal Server Error",
                                _ => "Unknown",
                            };
                            let mut resp = format!(
                                "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n",
                                status,
                                status_text,
                                resp_body.len()
                            );
                            for (k, v) in resp_headers {
                                resp.push_str(&format!("{}: {}\r\n", k, v));
                            }
                            resp.push_str("\r\n");
                            let _ = stream.write_all(resp.as_bytes());
                            let _ = stream.write_all(&resp_body);
                            let _ = stream.flush();
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            addr: format!("http://127.0.0.1:{}", local_addr.port()),
            shutdown_tx: Some(shutdown_tx),
            request_received,
        }
    }

    fn endpoint(&self) -> String {
        format!("{}/v1/audio/transcriptions", self.addr)
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

// 1. Shared WAV builder: header fields, sizes, alignment
#[test]
fn test_1_shared_wav_builder_header_fields_sizes_alignment() {
    let samples = vec![0.0f32; 16000]; // 1 second of audio at 16kHz
    let wav = build_in_memory_wav(&samples);

    assert_eq!(&wav[0..4], b"RIFF");
    let file_len = u32::from_le_bytes(wav[4..8].try_into().unwrap());
    assert_eq!(file_len, 36 + 32000);
    assert_eq!(&wav[8..12], b"WAVE");

    assert_eq!(&wav[12..16], b"fmt ");
    let subchunk1_size = u32::from_le_bytes(wav[16..20].try_into().unwrap());
    assert_eq!(subchunk1_size, 16);
    let audio_format = u16::from_le_bytes(wav[20..22].try_into().unwrap());
    assert_eq!(audio_format, 1); // PCM
    let num_channels = u16::from_le_bytes(wav[22..24].try_into().unwrap());
    assert_eq!(num_channels, 1); // Mono
    let sample_rate = u32::from_le_bytes(wav[24..28].try_into().unwrap());
    assert_eq!(sample_rate, 16000);
    let byte_rate = u32::from_le_bytes(wav[28..32].try_into().unwrap());
    assert_eq!(byte_rate, 32000);
    let block_align = u16::from_le_bytes(wav[32..34].try_into().unwrap());
    assert_eq!(block_align, 2);
    let bits_per_sample = u16::from_le_bytes(wav[34..36].try_into().unwrap());
    assert_eq!(bits_per_sample, 16);

    assert_eq!(&wav[36..40], b"data");
    let data_len = u32::from_le_bytes(wav[40..44].try_into().unwrap());
    assert_eq!(data_len, 32000);
    assert_eq!(wav.len(), 44 + 32000);
}

// 2. In-test mock HTTP server: 200 JSON parse, missing text, malformed JSON, 401, 429, 500, 302, timeout
#[test]
fn test_2_mock_server_happy_path_and_header_assertions() {
    let server = MockServer::spawn(|headers, body| {
        assert!(headers.starts_with("POST /v1/audio/transcriptions HTTP/1.1"));
        assert!(headers
            .to_lowercase()
            .contains("authorization: bearer test_secret_key_12345"));
        assert!(headers
            .to_lowercase()
            .contains("content-type: multipart/form-data; boundary="));

        let body_str = String::from_utf8_lossy(body);
        assert!(body_str.contains("name=\"file\"; filename=\"audio.wav\""));
        assert!(body_str.contains("Content-Type: audio/wav"));
        assert!(body_str.contains("name=\"model\"\r\n\r\nwhisper-large-v3-turbo"));
        assert!(body_str.contains("name=\"response_format\"\r\n\r\njson"));

        let json = r#"{"text": "Hello from Groq Whisper!"}"#;
        (
            200,
            vec![("Content-Type", "application/json")],
            json.as_bytes().to_vec(),
        )
    });

    let mut engine = OpenAiSpeechInference::new(
        "groq",
        Some(Secret::new("test_secret_key_12345")),
        server.endpoint(),
        DEFAULT_GROQ_MODEL,
        None,
        Duration::from_secs(5),
    )
    .unwrap();

    let chunk = AudioChunk::new(vec![0.1; 1600]);
    let transcript = engine.transcribe(&chunk).unwrap();
    assert_eq!(transcript, "Hello from Groq Whisper!");
}

#[test]
fn test_2_mock_server_language_configured_and_keyless_mode() {
    let server = MockServer::spawn(|headers, body| {
        // Assert Authorization is absent when keyless
        assert!(!headers.to_lowercase().contains("authorization:"));
        let body_str = String::from_utf8_lossy(body);
        assert!(body_str.contains("name=\"language\"\r\n\r\nhi"));
        let json = r#"{"text": "नमस्ते"}"#;
        (
            200,
            vec![("Content-Type", "application/json")],
            json.as_bytes().to_vec(),
        )
    });

    let mut engine = OpenAiSpeechInference::new(
        "openai-compatible",
        None,
        server.endpoint(),
        DEFAULT_OPENAI_COMPATIBLE_MODEL,
        Some("hi".to_string()),
        Duration::from_secs(5),
    )
    .unwrap();

    let chunk = AudioChunk::new(vec![0.1; 1600]);
    let transcript = engine.transcribe(&chunk).unwrap();
    assert_eq!(transcript, "नमस्ते");
}

#[test]
fn test_2_mock_server_missing_text_and_malformed_json() {
    let server_missing = MockServer::spawn(|_, _| {
        (
            200,
            vec![("Content-Type", "application/json")],
            br#"{"duration": 1.23}"#.to_vec(),
        )
    });
    let mut engine_missing = OpenAiSpeechInference::new(
        "groq",
        Some(Secret::new("key")),
        server_missing.endpoint(),
        DEFAULT_GROQ_MODEL,
        None,
        Duration::from_secs(5),
    )
    .unwrap();
    let err = engine_missing
        .transcribe(&AudioChunk::new(vec![0.0; 800]))
        .unwrap_err();
    assert!(matches!(err, InferenceError::Serialization(ref msg) if msg.contains("Missing 'text'")));

    let server_malformed = MockServer::spawn(|_, _| {
        (
            200,
            vec![("Content-Type", "text/plain")],
            b"Not JSON at all".to_vec(),
        )
    });
    let mut engine_malformed = OpenAiSpeechInference::new(
        "openai",
        Some(Secret::new("key")),
        server_malformed.endpoint(),
        DEFAULT_OPENAI_MODEL,
        None,
        Duration::from_secs(5),
    )
    .unwrap();
    let err = engine_malformed
        .transcribe(&AudioChunk::new(vec![0.0; 800]))
        .unwrap_err();
    assert!(matches!(err, InferenceError::Serialization(_)));
}

#[test]
fn test_2_mock_server_401_429_500_errors() {
    let server_401 = MockServer::spawn(|_, _| {
        (
            401,
            vec![("Content-Type", "application/json")],
            br#"{"error": "Unauthorized"}"#.to_vec(),
        )
    });
    let mut engine_401 = OpenAiSpeechInference::new(
        "groq",
        Some(Secret::new("invalid_key")),
        server_401.endpoint(),
        DEFAULT_GROQ_MODEL,
        None,
        Duration::from_secs(5),
    )
    .unwrap();
    let err = engine_401
        .transcribe(&AudioChunk::new(vec![0.0; 800]))
        .unwrap_err();
    assert_eq!(
        err,
        InferenceError::Authentication(
            "API key rejected by groq. Run 'speechyyy use groq' to update your key.".to_string()
        )
    );

    let server_429 = MockServer::spawn(|_, _| {
        (
            429,
            vec![
                ("Content-Type", "application/json"),
                ("Retry-After", "12"),
            ],
            br#"{"error": "Rate limit exceeded"}"#.to_vec(),
        )
    });
    let mut engine_429 = OpenAiSpeechInference::new(
        "openai",
        Some(Secret::new("valid_key")),
        server_429.endpoint(),
        DEFAULT_OPENAI_MODEL,
        None,
        Duration::from_secs(5),
    )
    .unwrap();
    let err = engine_429
        .transcribe(&AudioChunk::new(vec![0.0; 800]))
        .unwrap_err();
    assert_eq!(
        err,
        InferenceError::RateLimited("openai rate limit exceeded (Retry-After: 12)".to_string())
    );

    let server_500 = MockServer::spawn(|_, _| {
        (
            500,
            vec![("Content-Type", "text/plain")],
            b"Internal Server Error".to_vec(),
        )
    });
    let mut engine_500 = OpenAiSpeechInference::new(
        "groq",
        Some(Secret::new("key")),
        server_500.endpoint(),
        DEFAULT_GROQ_MODEL,
        None,
        Duration::from_secs(5),
    )
    .unwrap();
    let err = engine_500
        .transcribe(&AudioChunk::new(vec![0.0; 800]))
        .unwrap_err();
    assert_eq!(
        err,
        InferenceError::Http {
            status: 500,
            message: "groq provider error (HTTP 500)".to_string(),
        }
    );
}

#[test]
fn test_2_mock_server_302_redirect_not_followed() {
    let server_302 = MockServer::spawn(|_, _| {
        (
            302,
            vec![("Location", "http://127.0.0.1:1234/other")],
            b"".to_vec(),
        )
    });
    let mut engine_302 = OpenAiSpeechInference::new(
        "groq",
        Some(Secret::new("key")),
        server_302.endpoint(),
        DEFAULT_GROQ_MODEL,
        None,
        Duration::from_secs(5),
    )
    .unwrap();
    let err = engine_302
        .transcribe(&AudioChunk::new(vec![0.0; 800]))
        .unwrap_err();
    assert!(
        matches!(err, InferenceError::Http { status: 302, ref message } if message.contains("redirects are disabled"))
    );
}

#[test]
fn test_2_mock_server_timeout_triggers_error() {
    let server_slow = MockServer::spawn(|_, _| {
        thread::sleep(Duration::from_millis(300));
        (
            200,
            vec![("Content-Type", "application/json")],
            br#"{"text": "done"}"#.to_vec(),
        )
    });
    let mut engine_slow = OpenAiSpeechInference::new(
        "groq",
        Some(Secret::new("key")),
        server_slow.endpoint(),
        DEFAULT_GROQ_MODEL,
        None,
        Duration::from_millis(50),
    )
    .unwrap();
    let err = engine_slow
        .transcribe(&AudioChunk::new(vec![0.0; 800]))
        .unwrap_err();
    assert!(matches!(err, InferenceError::Timeout(ref msg) if msg.contains("timed out")));
}

// 3. Key never leaks: using sentinel key in config & engine structs and error strings
#[test]
fn test_3_key_never_leaks() {
    let sentinel = "SENTINEL_VERY_SECRET_KEY_999888777";
    let mut config = Config::default();
    config.inference.provider = "groq".into();
    config.inference.groq_api_key = Some(Secret::new(sentinel));
    config.inference.api_key = Secret::new(sentinel);

    let config_debug = format!("{:?}", config);
    assert!(
        !config_debug.contains(sentinel),
        "Config must not leak key!"
    );

    let engine = OpenAiSpeechInference::new(
        "groq",
        Some(Secret::new(sentinel)),
        "https://api.groq.com/openai/v1/audio/transcriptions",
        DEFAULT_GROQ_MODEL,
        None,
        Duration::from_secs(10),
    )
    .unwrap();

    let engine_debug = format!("{:?}", engine);
    assert!(
        !engine_debug.contains(sentinel),
        "Engine must not leak key!"
    );

    let sarvam_engine = speechyyy::inference::sarvam::SarvamSpeechInference::new(
        Secret::new(sentinel),
        "saaras:v4".into(),
        "unknown".into(),
        "translit".into(),
    );
    let sarvam_debug = format!("{:?}", sarvam_engine);
    assert!(
        !sarvam_debug.contains(sentinel),
        "Sarvam engine must not leak key!"
    );

    let server_err = MockServer::spawn(|_, _| {
        (
            401,
            vec![("Content-Type", "application/json")],
            br#"{"error": "forbidden"}"#.to_vec(),
        )
    });
    let mut live_engine = OpenAiSpeechInference::new(
        "groq",
        Some(Secret::new(sentinel)),
        server_err.endpoint(),
        DEFAULT_GROQ_MODEL,
        None,
        Duration::from_secs(5),
    )
    .unwrap();
    let err = live_engine
        .transcribe(&AudioChunk::new(vec![0.0; 800]))
        .unwrap_err();
    let err_str = err.to_string();
    assert!(
        !err_str.contains(sentinel),
        "Error string must not leak key!"
    );
}

// 4. Key isolation: Sarvam key never resolves for groq/openai/openai-compatible; env fallback; empty = unset
#[test]
fn test_4_key_isolation_matrix() {
    let mut inf = InferenceConfig {
        api_key: Secret::new("sarvam_sentinel_key"),
        ..Default::default()
    };

    // Strict isolation: Sarvam key does NOT resolve for groq, openai, or openai-compatible
    assert_eq!(inf.resolve_key_for_provider("groq"), None);
    assert_eq!(inf.resolve_key_for_provider("openai"), None);
    assert_eq!(inf.resolve_key_for_provider("openai-compatible"), None);
    assert_eq!(
        inf.resolve_key_for_provider("sarvam"),
        Some((Secret::new("sarvam_sentinel_key"), KeySource::Config))
    );

    // Empty string in config counts as unset
    inf.groq_api_key = Some(Secret::new("   "));
    assert_eq!(inf.resolve_key_for_provider("groq"), None);

    // Env fallback works
    std::env::set_var("GROQ_API_KEY", "groq_env_token_456");
    assert_eq!(
        inf.resolve_key_for_provider("groq"),
        Some((Secret::new("groq_env_token_456"), KeySource::Env))
    );

    // Config overrides env
    inf.groq_api_key = Some(Secret::new("groq_config_token_123"));
    assert_eq!(
        inf.resolve_key_for_provider("groq"),
        Some((Secret::new("groq_config_token_123"), KeySource::Config))
    );

    std::env::remove_var("GROQ_API_KEY");
}

// 5. validate_endpoint table: accepts https and local http loopback; rejects others and userinfo
#[test]
fn test_5_validate_endpoint_table() {
    // Accepts
    assert!(validate_endpoint("https://api.groq.com/openai/v1/audio/transcriptions").is_ok());
    assert!(validate_endpoint("http://localhost:11434/v1/audio/transcriptions").is_ok());
    assert!(validate_endpoint("http://127.0.0.1:8080/v1").is_ok());
    assert!(validate_endpoint("http://[::1]:8000/v1").is_ok());

    // Rejects non-loopback http
    assert!(validate_endpoint("http://api.groq.com/v1").is_err());
    assert!(validate_endpoint("http://evil.com/v1").is_err());
    assert!(validate_endpoint("http://localhost.evil.com:8080/v1").is_err());
    assert!(validate_endpoint("http://127.0.0.1.evil.com/v1").is_err());

    // Rejects invalid schemes
    assert!(validate_endpoint("ftp://localhost:8080/v1").is_err());
    assert!(validate_endpoint("ws://localhost:8080/v1").is_err());

    // Rejects userinfo
    assert!(validate_endpoint("https://user@api.groq.com/v1").is_err());
    assert!(validate_endpoint("http://user:pass@localhost:8080/v1").is_err());
}

// 6. Config: old config with none of new fields loads; each new provider parses; provider="custom" routes to ONNX
#[test]
fn test_6_config_backward_compatibility_and_custom() {
    let old_toml = r#"
hotkey = "RightAlt"
pacing_delay_ms = 15

[inference]
provider = "sarvam"
api_key = "old_sarvam_key"
model = "saaras:v4"
"#;

    let cfg: Config = toml::from_str(old_toml).unwrap();
    assert_eq!(cfg.inference.provider, "sarvam");
    assert_eq!(cfg.inference.api_key, "old_sarvam_key");
    assert_eq!(cfg.inference.groq_api_key, None);
    assert_eq!(cfg.inference.openai_api_key, None);
    assert_eq!(cfg.inference.openai_compatible_endpoint, None);

    let groq_toml = r#"
[inference]
provider = "groq"
groq_api_key = "gsk_groq123"
model = "whisper-large-v3-turbo"
"#;
    let groq_cfg: Config = toml::from_str(groq_toml).unwrap();
    assert_eq!(groq_cfg.inference.provider, "groq");
    assert_eq!(
        groq_cfg.inference.groq_api_key.as_ref().unwrap().expose(),
        "gsk_groq123"
    );

    let custom_toml = r#"
[inference]
provider = "custom"
model = "my-custom"

[inference.custom]
type = "whisper"
encoder = "/tmp/enc.onnx"
decoder = "/tmp/dec.onnx"
tokens = "/tmp/tokens.txt"
"#;
    let custom_cfg: Config = toml::from_str(custom_toml).unwrap();
    let engine = create_inference_provider(&custom_cfg.inference).unwrap();
    assert_eq!(engine.provider_name(), "custom");
}

// 7. Factory: missing key gives styled error for groq and openai; openai-compatible without key succeeds
#[test]
fn test_7_factory_missing_key_and_openai_compatible() {
    let groq_cfg = InferenceConfig {
        provider: "groq".into(),
        model: DEFAULT_GROQ_MODEL.into(),
        ..Default::default()
    };
    std::env::remove_var("GROQ_API_KEY");

    let err_groq = match create_inference_provider(&groq_cfg) {
        Err(e) => e,
        Ok(_) => panic!("Expected error for groq without key"),
    };
    assert!(err_groq.contains("GROQ_API_KEY is not configured"));
    assert!(err_groq.contains("speechyyy use groq"));

    let openai_cfg = InferenceConfig {
        provider: "openai".into(),
        model: DEFAULT_OPENAI_MODEL.into(),
        ..Default::default()
    };
    std::env::remove_var("OPENAI_API_KEY");

    let err_openai = match create_inference_provider(&openai_cfg) {
        Err(e) => e,
        Ok(_) => panic!("Expected error for openai without key"),
    };
    assert!(err_openai.contains("OPENAI_API_KEY is not configured"));
    assert!(err_openai.contains("speechyyy use openai"));

    let compat_cfg = InferenceConfig {
        provider: "openai-compatible".into(),
        openai_compatible_endpoint: Some("http://localhost:11434/v1".into()),
        ..Default::default()
    };

    let compat_engine = create_inference_provider(&compat_cfg).unwrap();
    assert_eq!(compat_engine.provider_name(), "openai-compatible");
}

// 8. use routing: use groq sets provider and default model without running local validation; use parakeet-tdt-v3 restores provider=local
#[test]
fn test_8_use_routing() {
    let (provider, model, _) = resolve_model_use("groq", None, None).unwrap();
    assert_eq!(provider, "groq");
    assert_eq!(model, DEFAULT_GROQ_MODEL);

    let (provider_openai, model_openai, _) = resolve_model_use("openai", None, None).unwrap();
    assert_eq!(provider_openai, "openai");
    assert_eq!(model_openai, DEFAULT_OPENAI_MODEL);

    let (provider_local, model_local, _) =
        resolve_model_use("parakeet-tdt-v3", None, None).unwrap();
    assert_eq!(provider_local, "local");
    assert_eq!(model_local, "parakeet-tdt-v3");
}

// 9. Config write: file is created with mode 0600 in a temp dir
#[test]
fn test_9_config_write_mode_0600() {
    let temp_dir = std::env::temp_dir().join(format!("speechyyy_cfg_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();
    let config_path = temp_dir.join("config.toml");

    Config::save_groq_credentials_in_file(&config_path, "gsk_atomic_key_test_123").unwrap();
    assert!(config_path.exists());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::metadata(&config_path).unwrap().permissions();
        assert_eq!(
            perms.mode() & 0o777,
            0o600,
            "Config file must be created with mode 0600"
        );
    }

    let loaded = Config::load_from_path(&config_path);
    assert_eq!(
        loaded.inference.groq_api_key.as_ref().unwrap().expose(),
        "gsk_atomic_key_test_123"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

// 10. Size guard rejects an oversized WAV without sending
#[test]
fn test_10_size_guard_rejects_oversized_audio_without_sending() {
    let server = MockServer::spawn(|_, _| {
        panic!("Server should NEVER be contacted when payload exceeds size limit!");
    });

    let mut engine = OpenAiSpeechInference::new(
        "groq",
        Some(Secret::new("key")),
        server.endpoint(),
        DEFAULT_GROQ_MODEL,
        None,
        Duration::from_secs(5),
    )
    .unwrap();

    // 13,000,000 f32 samples * 2 bytes = 26 MB > 24 MB limit
    let large_samples = vec![0.0f32; 13_000_000];
    let chunk = AudioChunk::new(large_samples);

    let err = engine.transcribe(&chunk).unwrap_err();
    assert!(matches!(err, InferenceError::Serialization(ref msg) if msg.contains("exceeds the 24 MB limit")));
    assert!(
        !server.request_received.load(Ordering::SeqCst),
        "Server must not receive any request!"
    );
}
