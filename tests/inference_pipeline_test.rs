use evdev::KeyCode;
use speechyyy::audio::capture::MockAudioCapture;
use speechyyy::audio::vad::EnergyVad;
use speechyyy::audio::AudioChunk;
use speechyyy::daemon::run_session_loop;
use speechyyy::engine::{AudioDictationEngine, StateMachineController, TargetChord};
use speechyyy::inference::mock::MockSpeechInference;
use speechyyy::inference::{InferenceError, SpeechInference};
use speechyyy::injector::{RecordingInjector, TextInjector};
use speechyyy::input::{FakeInputSource, InputEventSource};
use speechyyy::ipc::{IpcClient, IpcRequest, IpcResponse, IpcServer, RuntimePaths};
use std::fs;
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

    // Valid speech chunk passing VAD
    let speech_chunk = AudioChunk::new(vec![0.05; 8000]);
    let capture = MockAudioCapture::with_queued_chunk(speech_chunk);
    let vad = EnergyVad::default();
    let inference = MockSpeechInference::with_transcript("   "); // Empty / whitespace transcript
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

    // Verify NOTHING was injected for empty transcript
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
fn test_inference_pipeline_e2e_error_handled_gracefully() {
    struct FailingInference;
    impl SpeechInference for FailingInference {
        fn transcribe(&mut self, _chunk: &AudioChunk) -> Result<String, InferenceError> {
            Err(InferenceError::Http {
                status: 500,
                message: "Internal Server Error".into(),
            })
        }
        fn provider_name(&self) -> &'static str {
            "failing"
        }
    }

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
    assert_eq!(injected[1], "Second utterance");

    let _ = IpcClient::send_request(&paths.socket_path, &IpcRequest::Stop);
    let _ = loop_thread.join().unwrap();
    let _ = fs::remove_dir_all(temp_dir);
}
