use evdev::KeyCode;
use speechyyy::audio::capture::MockAudioCapture;
use speechyyy::audio::vad::EnergyVad;
use speechyyy::audio::AudioChunk;
use speechyyy::daemon::run_session_loop;
use speechyyy::engine::{AudioDictationEngine, StateMachineController, TargetChord};
use speechyyy::history::HistoryLogger;
use speechyyy::inference::local::LocalSpeechInference;
use speechyyy::inference::mock::MockSpeechInference;
use speechyyy::injector::{RecordingInjector, TextInjector};
use speechyyy::input::{FakeInputSource, InputEventSource};
use speechyyy::ipc::{IpcClient, IpcRequest, IpcServer, RuntimePaths};
use speechyyy::model::LocalModelType;
use speechyyy::text::{LearningDictionary, TextFormatter, TextPipeline};
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
fn test_local_dictation_pipeline_e2e_with_dictionary_and_history() {
    let temp_dir = std::env::temp_dir().join(format!("speechyyy_local_e2e_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let paths = RuntimePaths::with_dir(temp_dir.join("runtime"));
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

    // 1. Point to real Moonshine model assets
    let model_dir = speechyyy::model::ModelManager::model_dir(LocalModelType::MoonshineTiny);
    if !model_dir.exists() {
        eprintln!(
            "Skipping e2e local test: Moonshine model not found at {}",
            model_dir.display()
        );
        return;
    }

    // 2. Setup dictionary and formatting pipeline
    let mut dict = LearningDictionary::new();
    dict.add_rule("yellow lamps", "golden lanterns");

    let formatter = TextFormatter::default();
    let pipeline = TextPipeline::new(formatter, dict);

    // 3. Setup history logger
    let hist_path = temp_dir.join("history.jsonl");
    let history = HistoryLogger::with_path(hist_path.clone());

    // 4. Setup AudioDictationEngine with LocalSpeechInference using real speech audio
    let (samples, _) = sherpa_rs::read_audio_file("tests/test_data/0.wav").expect("read 0.wav");
    let speech_chunk = AudioChunk::new(samples);
    let capture = MockAudioCapture::with_queued_chunk(speech_chunk);
    let vad = EnergyVad::default();
    let inference = LocalSpeechInference::new(LocalModelType::MoonshineTiny, model_dir);

    let engine = AudioDictationEngine::with_pipeline_and_history(
        capture,
        vad,
        inference,
        pipeline,
        Some(history.clone()),
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

    // Press PTT
    raw_source
        .lock()
        .unwrap()
        .push_event(KeyCode::KEY_RIGHTALT, true);
    std::thread::sleep(Duration::from_millis(50));

    // Release PTT: triggers capture halt -> VAD -> LocalInference -> Hallucination clean -> Dictionary replace -> Formatting -> History log -> Caret injection
    raw_source
        .lock()
        .unwrap()
        .push_event(KeyCode::KEY_RIGHTALT, false);

    // Wait for the background session loop to complete ONNX model initialization and inference
    let mut injected = Vec::new();
    for _ in 0..200 {
        injected = raw_injector
            .lock()
            .unwrap()
            .injected_strings
            .lock()
            .unwrap()
            .clone();
        if !injected.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    let _ = IpcClient::send_request(&paths.socket_path, &IpcRequest::Stop);
    let _ = loop_thread.join().unwrap();

    // Verify injected text:
    // Raw: "...the yellow lamps would light up..."
    // Replaced: "...the golden lanterns would light up..."
    assert_eq!(injected.len(), 1);
    let injected_text = &injected[0];
    assert!(
        injected_text.contains("golden lanterns"),
        "Expected injected text to contain 'golden lanterns', got: {}",
        injected_text
    );

    // Verify history log was created
    let last = history.get_last_entry().unwrap().unwrap();
    assert!(last.raw_text.contains("yellow lamps"));
    assert!(last.final_text.contains("golden lanterns"));
    assert_eq!(last.provider, "local");

    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn test_hallucination_only_discards_without_injection_or_history() {
    let temp_dir = std::env::temp_dir().join(format!(
        "speechyyy_hallucination_e2e_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let paths = RuntimePaths::with_dir(temp_dir.join("runtime"));
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

    let hist_path = temp_dir.join("history.jsonl");
    let history = HistoryLogger::with_path(hist_path.clone());

    let pipeline = TextPipeline::default();

    let speech_chunk = AudioChunk::new(vec![0.05; 8000]);
    let capture = MockAudioCapture::with_queued_chunk(speech_chunk);
    let vad = EnergyVad::default();
    let inference = MockSpeechInference::with_transcript("[BLANK_AUDIO] (music)");

    let engine = AudioDictationEngine::with_pipeline_and_history(
        capture,
        vad,
        inference,
        pipeline,
        Some(history.clone()),
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

    // Verify NOTHING was injected (Guardrail #5)
    let injected = raw_injector
        .lock()
        .unwrap()
        .injected_strings
        .lock()
        .unwrap()
        .clone();
    assert_eq!(injected.len(), 0);

    // Verify history remains empty (Guardrail #5)
    assert_eq!(history.get_last_entry().unwrap(), None);

    let _ = IpcClient::send_request(&paths.socket_path, &IpcRequest::Stop);
    let _ = loop_thread.join().unwrap();
    let _ = fs::remove_dir_all(temp_dir);
}
