use speechyyy::audio::AudioChunk;
use speechyyy::inference::sarvam::{
    SarvamSpeechInference, DEFAULT_SARVAM_ENDPOINT, DEFAULT_SARVAM_MODE, DEFAULT_SARVAM_MODEL,
};
use speechyyy::inference::{InferenceError, SpeechInference};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

fn spawn_mock_server(
    status_code: u16,
    response_body: &str,
    delay: Option<Duration>,
) -> (String, Arc<Mutex<Vec<u8>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let endpoint = format!("http://127.0.0.1:{}/speech-to-text", port);
    let captured_req = Arc::new(Mutex::new(Vec::new()));
    let captured_clone = captured_req.clone();
    let body_str = response_body.to_string();

    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            if let Some(d) = delay {
                thread::sleep(d);
            }
            let mut buf = [0u8; 4096];
            let mut total_req = Vec::new();
            while let Ok(n) = stream.read(&mut buf) {
                if n == 0 {
                    break;
                }
                total_req.extend_from_slice(&buf[..n]);
                if let Some(pos) = total_req.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&total_req[..pos]);
                    let mut cl = 0;
                    for line in headers.lines() {
                        if line.to_lowercase().starts_with("content-length:") {
                            if let Some(val) = line.split(':').nth(1) {
                                cl = val.trim().parse::<usize>().unwrap_or(0);
                            }
                        }
                    }
                    if total_req.len() >= pos + 4 + cl {
                        break;
                    }
                }
            }
            *captured_clone.lock().unwrap() = total_req;

            let reason = match status_code {
                200 => "OK",
                401 => "Unauthorized",
                429 => "Too Many Requests",
                _ => "Error",
            };
            let resp = format!(
                "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                status_code,
                reason,
                body_str.len(),
                body_str
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
        }
    });

    (endpoint, captured_req)
}

#[test]
fn test_sarvam_defaults_to_translit_mode_and_saaras_v4() {
    assert_eq!(
        DEFAULT_SARVAM_ENDPOINT,
        "https://api.sarvam.ai/speech-to-text"
    );
    assert_eq!(DEFAULT_SARVAM_MODEL, "saaras:v4");
    assert_eq!(
        DEFAULT_SARVAM_MODE, "translit",
        "Sarvam default mode must be 'translit' for Romanized Hinglish transliteration"
    );

    let sarvam = SarvamSpeechInference::new(
        "sk_test_key".into(),
        DEFAULT_SARVAM_MODEL.into(),
        "unknown".into(),
        DEFAULT_SARVAM_MODE.into(),
    );
    assert_eq!(sarvam.mode, "translit");
    assert_eq!(sarvam.model, "saaras:v4");
}

#[test]
fn test_sarvam_request_construction_targets_endpoint_and_multipart() {
    let response_json =
        r#"{"transcript": "mera phone number note kar lo", "language_code": "hi-IN"}"#;
    let (endpoint, captured_req) = spawn_mock_server(200, response_json, None);

    let mut sarvam = SarvamSpeechInference::new(
        "test_secret_key_999".into(),
        "saaras:v4".into(),
        "unknown".into(),
        "translit".into(),
    )
    .with_endpoint(&endpoint);

    let chunk = AudioChunk::new(vec![0.1f32; 16000]);
    let transcript = sarvam
        .transcribe(&chunk)
        .expect("transcribe should succeed");

    assert_eq!(transcript, "mera phone number note kar lo");

    let req_bytes = captured_req.lock().unwrap().clone();
    let req_str = String::from_utf8_lossy(&req_bytes);

    // 1. Target POST endpoint
    assert!(
        req_str.starts_with("POST /speech-to-text HTTP/1.1"),
        "Expected POST /speech-to-text, got request: {}",
        &req_str[..req_str.len().min(100)]
    );

    // 2. Authentication header
    assert!(
        req_str.contains("api-subscription-key: test_secret_key_999"),
        "Request missing api-subscription-key header"
    );

    // 3. Multipart content type
    assert!(
        req_str.contains("Content-Type: multipart/form-data; boundary="),
        "Request missing multipart boundary"
    );

    // 4. Multipart fields: model, mode, file
    assert!(
        req_str.contains(r#"name="model""#) && req_str.contains("saaras:v4"),
        "Multipart missing model=saaras:v4 field"
    );
    assert!(
        req_str.contains(r#"name="mode""#) && req_str.contains("translit"),
        "Multipart missing mode=translit field"
    );
    assert!(
        req_str.contains(r#"name="file"; filename="audio.wav""#),
        "Multipart missing audio.wav file attachment"
    );
}

#[test]
fn test_sarvam_http_401_maps_to_explicit_authentication_error() {
    let error_json = r#"{"error": "Unauthorized: invalid subscription key"}"#;
    let (endpoint, _) = spawn_mock_server(401, error_json, None);

    let mut sarvam = SarvamSpeechInference::new(
        "invalid_key".into(),
        "saaras:v4".into(),
        "unknown".into(),
        "translit".into(),
    )
    .with_endpoint(&endpoint);

    let chunk = AudioChunk::new(vec![0.1f32; 1600]);
    let err = sarvam.transcribe(&chunk).unwrap_err();

    match err {
        InferenceError::Authentication(msg) => {
            assert!(
                msg.contains("Invalid or expired Sarvam API key. Check 'speechyyy doctor' or replenish credentials at https://dashboard.sarvam.ai"),
                "Missing structured 401 error message in: {}",
                msg
            );
        }
        other => panic!(
            "Expected InferenceError::Authentication for HTTP 401, got: {:?}",
            other
        ),
    }
}

#[test]
fn test_sarvam_http_429_maps_to_explicit_rate_limited_error() {
    let error_json = r#"{"error": "Rate limit exceeded: quota exhausted"}"#;
    let (endpoint, _) = spawn_mock_server(429, error_json, None);

    let mut sarvam = SarvamSpeechInference::new(
        "rate_limited_key".into(),
        "saaras:v4".into(),
        "unknown".into(),
        "translit".into(),
    )
    .with_endpoint(&endpoint);

    let chunk = AudioChunk::new(vec![0.1f32; 1600]);
    let err = sarvam.transcribe(&chunk).unwrap_err();

    match err {
        InferenceError::RateLimited(msg) => {
            assert!(
                msg.contains("Rate limit") || msg.contains("quota"),
                "Expected rate limit message, got: {}",
                msg
            );
            assert!(
                msg.contains("Sarvam AI rate limit exceeded or credit balance exhausted"),
                "Missing structured 429 error message in: {}",
                msg
            );
        }
        other => panic!(
            "Expected InferenceError::RateLimited for HTTP 429, got: {:?}",
            other
        ),
    }
}

#[test]
fn test_sarvam_resolve_request_timeout() {
    use speechyyy::inference::sarvam::resolve_request_timeout;
    assert_eq!(resolve_request_timeout(None), Duration::from_secs(30));
    assert_eq!(resolve_request_timeout(Some(0)), Duration::from_secs(30));
    assert_eq!(resolve_request_timeout(Some(15)), Duration::from_secs(15));
}

#[test]
fn test_sarvam_network_timeout_maps_to_explicit_timeout_error() {
    // Mock server connects but sleeps longer than client timeout
    let (endpoint, _) = spawn_mock_server(200, "{}", Some(Duration::from_millis(500)));

    let mut sarvam = SarvamSpeechInference::new(
        "timeout_key".into(),
        "saaras:v4".into(),
        "unknown".into(),
        "translit".into(),
    )
    .with_endpoint(&endpoint)
    .with_timeout(Duration::from_millis(50));

    let chunk = AudioChunk::new(vec![0.1f32; 1600]);
    let err = sarvam.transcribe(&chunk).unwrap_err();

    match err {
        InferenceError::Timeout(msg) => {
            assert!(!msg.is_empty());
            assert!(
                msg.contains("Sarvam AI connection timed out after"),
                "Missing structured timeout error message in: {}",
                msg
            );
            assert!(
                msg.contains("raise 'request_timeout_secs' in config.toml"),
                "Missing actionable config guidance in: {}",
                msg
            );
        }
        other => panic!(
            "Expected InferenceError::Timeout for network timeout, got: {:?}",
            other
        ),
    }
}

#[test]
fn test_sarvam_timeout_with_server_not_responding() {
    let (endpoint, _) = spawn_mock_server(200, "{}", Some(Duration::from_secs(10)));
    let mut sarvam = SarvamSpeechInference::new(
        "timeout_key".into(),
        "saaras:v4".into(),
        "unknown".into(),
        "translit".into(),
    )
    .with_endpoint(&endpoint)
    .with_timeout(Duration::from_secs(1));

    let start = std::time::Instant::now();
    let chunk = AudioChunk::new(vec![0.1f32; 1600]);
    let err = sarvam.transcribe(&chunk).unwrap_err();
    let elapsed = start.elapsed();

    assert!(elapsed < Duration::from_secs(5), "Request took too long: {:?}", elapsed);
    match err {
        InferenceError::Timeout(msg) => {
            assert!(msg.contains("Sarvam AI connection timed out"));
        }
        other => panic!("Expected InferenceError::Timeout, got {:?}", other),
    }
}
