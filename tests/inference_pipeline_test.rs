use evdev::KeyCode;
use speechyyy::audio::capture::MockAudioCapture;
use speechyyy::audio::vad::EnergyVad;
use speechyyy::audio::AudioChunk;
use speechyyy::daemon::run_session_loop;
use speechyyy::engine::{AudioDictationEngine, StateMachineController, TargetChord};
use speechyyy::history::HistoryLogger;
use speechyyy::inference::mock::MockSpeechInference;
use speechyyy::inference::sarvam::SarvamSpeechInference;
use speechyyy::inference::{InferenceError, SpeechInference};
use speechyyy::injector::{RecordingInjector, TextInjector};
use speechyyy::input::{FakeInputSource, InputEventSource};
use speechyyy::ipc::{IpcClient, IpcRequest, IpcResponse, IpcServer, RuntimePaths};
use speechyyy::text::dictionary::{DictionaryFile, LearningDictionary};
use speechyyy::text::format::TextFormatter;
use speechyyy::text::TextPipeline;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone)]
struct SharedFakeInput {
    source: Arc<Mutex<FakeInputSource>>,
}

impl InputEventSource for SharedFakeInput {
    fn poll_event(&mut self, timeout: Duration) -> Option<(KeyCode, bool)> {
        let ev = self.source.lock().unwrap().poll_event(Duration::ZERO);
        if ev.is_none() {
            std::thread::sleep(timeout);
        }
        ev
    }
}

#[derive(Clone)]
struct SharedRecordingInjector {
    injector: Arc<Mutex<RecordingInjector>>,
}

impl TextInjector for SharedRecordingInjector {
    fn inject(&mut self, text: &str) -> Result<(), String> {
        self.injector.lock().unwrap().inject(text)
    }
}

#[test]
fn test_inference_pipeline_e2e_successful_transcription() {
    let temp_dir =
        std::env::temp_dir().join(format!("speechyyy_inf_e2e_success_{}", std::process::id()));
    let paths = RuntimePaths::with_dir(temp_dir.clone());

    let server = IpcServer::new(paths.clone());
    let listener = server.start_listener().expect("bind listener");
    listener.set_nonblocking(true).unwrap();
    let running = server.running_flag();

    let raw_source = Arc::new(Mutex::new(FakeInputSource::new()));
    let mut input_source = SharedFakeInput {
        source: raw_source.clone(),
    };

    let raw_injector = Arc::new(Mutex::new(RecordingInjector::new(Duration::ZERO)));
    let mut injector = SharedRecordingInjector {
        injector: raw_injector.clone(),
    };

    // 0.5s of speech with RMS 0.05
    let speech_chunk = AudioChunk::new(vec![0.05; 8000]);
    let capture = MockAudioCapture::with_queued_chunk(speech_chunk);
    let vad = EnergyVad::default();
    let inference =
        MockSpeechInference::with_transcript("Streaming speech into cursor in real time");
    let engine = AudioDictationEngine::new(capture, vad, inference);
    let mut controller = StateMachineController::new(TargetChord::right_alt(), engine);

    let loop_running = running.clone();
    let loop_thread = std::thread::spawn(move || {
        run_session_loop(
            &listener,
            &mut input_source,
            &mut injector,
            &mut controller,
            &loop_running,
            "RightAlt",
            3,
        )
    });

    // 1. Press hotkey (PTT active)
    raw_source
        .lock()
        .unwrap()
        .push_event(KeyCode::KEY_RIGHTALT, true);
    std::thread::sleep(Duration::from_millis(50));

    // Verify daemon reports Recording state via IPC
    if let Ok(IpcResponse::Status { state, .. }) =
        IpcClient::send_request(&paths.socket_path, &IpcRequest::Status)
    {
        assert_eq!(state, "Recording");
    }

    // 2. Release hotkey (triggers capture halt -> VAD -> SpeechInference -> Caret injection)
    raw_source
        .lock()
        .unwrap()
        .push_event(KeyCode::KEY_RIGHTALT, false);
    std::thread::sleep(Duration::from_millis(50));

    // Verify injected output contains transcribed speech directly
    let injected = raw_injector
        .lock()
        .unwrap()
        .injected_strings
        .lock()
        .unwrap()
        .clone();
    assert_eq!(injected.len(), 1);
    assert_eq!(injected[0], "Streaming speech into cursor in real time");

    // Daemon returns to Idle
    if let Ok(IpcResponse::Status { state, .. }) =
        IpcClient::send_request(&paths.socket_path, &IpcRequest::Status)
    {
        assert_eq!(state, "Idle");
    }

    // 3. Stop daemon
    let _ = IpcClient::send_request(&paths.socket_path, &IpcRequest::Stop);
    let _ = loop_thread.join().unwrap();
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn test_inference_pipeline_e2e_empty_transcript_no_injection() {
    let temp_dir =
        std::env::temp_dir().join(format!("speechyyy_inf_e2e_empty_{}", std::process::id()));
    let paths = RuntimePaths::with_dir(temp_dir.clone());

    let server = IpcServer::new(paths.clone());
    let listener = server.start_listener().expect("bind listener");
    listener.set_nonblocking(true).unwrap();
    let running = server.running_flag();

    let raw_source = Arc::new(Mutex::new(FakeInputSource::new()));
    let mut input_source = SharedFakeInput {
        source: raw_source.clone(),
    };

    let raw_injector = Arc::new(Mutex::new(RecordingInjector::new(Duration::ZERO)));
    let mut injector = SharedRecordingInjector {
        injector: raw_injector.clone(),
    };

    let speech_chunk = AudioChunk::new(vec![0.05; 8000]);
    let capture = MockAudioCapture::with_queued_chunk(speech_chunk);
    let vad = EnergyVad::default();
    let inference = MockSpeechInference::with_transcript("");
    let engine = AudioDictationEngine::new(capture, vad, inference);
    let mut controller = StateMachineController::new(TargetChord::right_alt(), engine);

    let loop_running = running.clone();
    let loop_thread = std::thread::spawn(move || {
        run_session_loop(
            &listener,
            &mut input_source,
            &mut injector,
            &mut controller,
            &loop_running,
            "RightAlt",
            3,
        )
    });

    raw_source
        .lock()
        .unwrap()
        .push_event(KeyCode::KEY_RIGHTALT, true);
    std::thread::sleep(Duration::from_millis(50));

    raw_source
        .lock()
        .unwrap()
        .push_event(KeyCode::KEY_RIGHTALT, false);
    std::thread::sleep(Duration::from_millis(50));

    // Verify nothing injected into active caret on empty transcript
    let injected = raw_injector
        .lock()
        .unwrap()
        .injected_strings
        .lock()
        .unwrap()
        .clone();
    assert_eq!(injected.len(), 0);

    let _ = IpcClient::send_request(&paths.socket_path, &IpcRequest::Stop);
    let _ = loop_thread.join().unwrap();
    let _ = fs::remove_dir_all(temp_dir);
}

struct FailingInference;

impl SpeechInference for FailingInference {
    fn transcribe(&mut self, _chunk: &AudioChunk) -> Result<String, InferenceError> {
        Err(InferenceError::Http {
            status: 500,
            message: "Internal server error".into(),
        })
    }

    fn provider_name(&self) -> &'static str {
        "failing_mock"
    }
}

#[test]
fn test_inference_pipeline_e2e_error_handled_gracefully() {
    let temp_dir =
        std::env::temp_dir().join(format!("speechyyy_inf_e2e_err_{}", std::process::id()));
    let paths = RuntimePaths::with_dir(temp_dir.clone());

    let server = IpcServer::new(paths.clone());
    let listener = server.start_listener().expect("bind listener");
    listener.set_nonblocking(true).unwrap();
    let running = server.running_flag();

    let raw_source = Arc::new(Mutex::new(FakeInputSource::new()));
    let mut input_source = SharedFakeInput {
        source: raw_source.clone(),
    };

    let raw_injector = Arc::new(Mutex::new(RecordingInjector::new(Duration::ZERO)));
    let mut injector = SharedRecordingInjector {
        injector: raw_injector.clone(),
    };

    let speech_chunk = AudioChunk::new(vec![0.05; 8000]);
    let capture = MockAudioCapture::with_queued_chunk(speech_chunk);
    let vad = EnergyVad::default();
    let engine = AudioDictationEngine::new(capture, vad, FailingInference);
    let mut controller = StateMachineController::new(TargetChord::right_alt(), engine);

    let loop_running = running.clone();
    let loop_thread = std::thread::spawn(move || {
        run_session_loop(
            &listener,
            &mut input_source,
            &mut injector,
            &mut controller,
            &loop_running,
            "RightAlt",
            3,
        )
    });

    raw_source
        .lock()
        .unwrap()
        .push_event(KeyCode::KEY_RIGHTALT, true);
    std::thread::sleep(Duration::from_millis(50));

    raw_source
        .lock()
        .unwrap()
        .push_event(KeyCode::KEY_RIGHTALT, false);
    std::thread::sleep(Duration::from_millis(50));

    // Verify error was logged and nothing was injected into active caret
    let injected = raw_injector
        .lock()
        .unwrap()
        .injected_strings
        .lock()
        .unwrap()
        .clone();
    assert_eq!(injected.len(), 0);

    let _ = IpcClient::send_request(&paths.socket_path, &IpcRequest::Stop);
    let _ = loop_thread.join().unwrap();
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn test_inference_pipeline_e2e_multiple_utterances() {
    let temp_dir =
        std::env::temp_dir().join(format!("speechyyy_inf_e2e_multi_{}", std::process::id()));
    let paths = RuntimePaths::with_dir(temp_dir.clone());

    let server = IpcServer::new(paths.clone());
    let listener = server.start_listener().expect("bind listener");
    listener.set_nonblocking(true).unwrap();
    let running = server.running_flag();

    let raw_source = Arc::new(Mutex::new(FakeInputSource::new()));
    let mut input_source = SharedFakeInput {
        source: raw_source.clone(),
    };

    let raw_injector = Arc::new(Mutex::new(RecordingInjector::new(Duration::ZERO)));
    let mut injector = SharedRecordingInjector {
        injector: raw_injector.clone(),
    };

    let speech1 = AudioChunk::new(vec![0.05; 8000]);
    let speech2 = AudioChunk::new(vec![0.06; 8000]);
    let capture = MockAudioCapture::with_queued_chunks(vec![speech1, speech2]);
    let vad = EnergyVad::default();
    let inference = MockSpeechInference::with_queued_transcripts(vec![
        "First utterance".into(),
        "Second utterance".into(),
    ]);
    let engine = AudioDictationEngine::new(capture, vad, inference);
    let mut controller = StateMachineController::new(TargetChord::right_alt(), engine);

    let loop_running = running.clone();
    let loop_thread = std::thread::spawn(move || {
        run_session_loop(
            &listener,
            &mut input_source,
            &mut injector,
            &mut controller,
            &loop_running,
            "RightAlt",
            3,
        )
    });

    // Utterance 1
    raw_source
        .lock()
        .unwrap()
        .push_event(KeyCode::KEY_RIGHTALT, true);
    std::thread::sleep(Duration::from_millis(50));
    raw_source
        .lock()
        .unwrap()
        .push_event(KeyCode::KEY_RIGHTALT, false);
    std::thread::sleep(Duration::from_millis(50));

    // Utterance 2
    raw_source
        .lock()
        .unwrap()
        .push_event(KeyCode::KEY_RIGHTALT, true);
    std::thread::sleep(Duration::from_millis(50));
    raw_source
        .lock()
        .unwrap()
        .push_event(KeyCode::KEY_RIGHTALT, false);
    std::thread::sleep(Duration::from_millis(50));

    let injected = raw_injector
        .lock()
        .unwrap()
        .injected_strings
        .lock()
        .unwrap()
        .clone();
    assert_eq!(injected.len(), 2);
    assert_eq!(injected[0], "First utterance");
    // Smart spacing prepends a space between consecutive word utterances
    assert_eq!(injected[1], " Second utterance");

    let _ = IpcClient::send_request(&paths.socket_path, &IpcRequest::Stop);
    let _ = loop_thread.join().unwrap();
    let _ = fs::remove_dir_all(temp_dir);
}

fn spawn_mock_sarvam_server(response_json: &str) -> (String, Arc<Mutex<Vec<u8>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let endpoint = format!("http://127.0.0.1:{}/speech-to-text", port);
    let captured_req = Arc::new(Mutex::new(Vec::new()));
    let captured_clone = captured_req.clone();
    let body_str = response_json.to_string();

    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
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

            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
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
fn test_sarvam_translit_e2e_session_with_dictionary_formatting_and_history() {
    let temp_dir =
        std::env::temp_dir().join(format!("speechyyy_sarvam_e2e_{}", std::process::id()));
    let paths = RuntimePaths::with_dir(temp_dir.clone());

    let server = IpcServer::new(paths.clone());
    let listener = server.start_listener().expect("bind listener");
    listener.set_nonblocking(true).unwrap();
    let running = server.running_flag();

    let raw_source = Arc::new(Mutex::new(FakeInputSource::new()));
    let mut input_source = SharedFakeInput {
        source: raw_source.clone(),
    };

    let raw_injector = Arc::new(Mutex::new(RecordingInjector::new(Duration::from_millis(
        15,
    ))));
    let mut injector = SharedRecordingInjector {
        injector: raw_injector.clone(),
    };

    // 1. Mock Sarvam server returning transliterated Hinglish
    let mock_json = r#"{"transcript": "mera phone number hai", "language_code": "hi-IN"}"#;
    let (endpoint, captured_req) = spawn_mock_sarvam_server(mock_json);

    let sarvam_inference = SarvamSpeechInference::new(
        "sk_sarvam_test_key".into(),
        "saaras:v4".into(),
        "unknown".into(),
        "translit".into(),
    )
    .with_endpoint(&endpoint);

    // 2. Setup dictionary substitution: "phone" -> "Phone"
    let mut replacements = HashMap::new();
    replacements.insert("phone".into(), "Phone".into());
    let dictionary = LearningDictionary::from_file(DictionaryFile { replacements });
    let pipeline = TextPipeline::new(TextFormatter::default(), dictionary);

    // 3. Setup history logger
    let history_path = temp_dir.join("history.jsonl");
    let history_logger = HistoryLogger::with_path(history_path.clone());

    // 0.5s of audio samples passing VAD
    let speech_chunk = AudioChunk::new(vec![0.08; 8000]);
    let capture = MockAudioCapture::with_queued_chunk(speech_chunk);
    let vad = EnergyVad::default();

    let engine = AudioDictationEngine::with_pipeline_and_history(
        capture,
        vad,
        sarvam_inference,
        pipeline,
        Some(history_logger.clone()),
    );
    let mut controller = StateMachineController::new(TargetChord::right_alt(), engine);

    let loop_running = running.clone();
    let loop_thread = std::thread::spawn(move || {
        run_session_loop(
            &listener,
            &mut input_source,
            &mut injector,
            &mut controller,
            &loop_running,
            "RightAlt",
            3,
        )
    });

    // Press hotkey (PTT active)
    raw_source
        .lock()
        .unwrap()
        .push_event(KeyCode::KEY_RIGHTALT, true);
    std::thread::sleep(Duration::from_millis(50));

    // Release hotkey (capture stop -> VAD -> Sarvam translit -> Dictionary -> Formatting -> Injection)
    raw_source
        .lock()
        .unwrap()
        .push_event(KeyCode::KEY_RIGHTALT, false);
    std::thread::sleep(Duration::from_millis(150));

    // Assert request reached mock Sarvam server with model=saaras:v4 and mode=translit
    let req_bytes = captured_req.lock().unwrap().clone();
    let req_str = String::from_utf8_lossy(&req_bytes);
    assert!(req_str.contains("api-subscription-key: sk_sarvam_test_key"));
    assert!(req_str.contains("mode") && req_str.contains("translit"));

    // Assert injected text reflects transliteration + dictionary replacement + sentence capitalization
    let injected = raw_injector
        .lock()
        .unwrap()
        .injected_strings
        .lock()
        .unwrap()
        .clone();
    assert_eq!(injected.len(), 1);
    assert_eq!(injected[0], "Mera Phone number hai");

    // Assert history logger recorded the utterance with provider = "sarvam"
    let last_entry = history_logger
        .get_last_entry()
        .expect("read history")
        .expect("entry present");
    assert_eq!(last_entry.provider, "sarvam");
    assert_eq!(last_entry.raw_text, "mera phone number hai");
    assert_eq!(last_entry.final_text, "Mera Phone number hai");
    assert!(last_entry.duration_secs > 0.0);

    let _ = IpcClient::send_request(&paths.socket_path, &IpcRequest::Stop);
    let _ = loop_thread.join().unwrap();
    let _ = fs::remove_dir_all(temp_dir);
}
