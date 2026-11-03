use evdev::KeyCode;
use speechyyy::audio::capture::MockAudioCapture;
use speechyyy::audio::resample::process_raw_audio;
use speechyyy::audio::vad::EnergyVad;
use speechyyy::audio::AudioChunk;
use speechyyy::daemon::run_session_loop;
use speechyyy::engine::{AudioDictationEngine, StateMachineController, TargetChord};
use speechyyy::inference::local::LocalSpeechInference;
use speechyyy::inference::mock::MockSpeechInference;
use speechyyy::inference::SpeechInference;
use speechyyy::injector::{RecordingInjector, TextInjector};
use speechyyy::input::{FakeInputSource, InputEventSource};
use speechyyy::ipc::{IpcClient, IpcRequest, IpcResponse, IpcServer, RuntimePaths};
use speechyyy::model::{LocalModelType, ModelManager};
use std::fs;
use std::path::Path;
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
fn test_audio_pipeline_e2e_valid_speech() {
    let temp_dir =
        std::env::temp_dir().join(format!("speechyyy_audio_e2e_speech_{}", std::process::id()));
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
    let inference = MockSpeechInference::with_transcript("Pipeline speech transcription");
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

    // 2. Release hotkey (triggers capture halt -> VAD -> emission)
    raw_source
        .lock()
        .unwrap()
        .push_event(KeyCode::KEY_RIGHTALT, false);
    std::thread::sleep(Duration::from_millis(50));

    // Verify injected output contains formatted speech summary
    let injected = raw_injector
        .lock()
        .unwrap()
        .injected_strings
        .lock()
        .unwrap()
        .clone();
    assert_eq!(injected.len(), 1);
    assert_eq!(injected[0], "Pipeline speech transcription");

    // 3. Stop daemon
    let _ = IpcClient::send_request(&paths.socket_path, &IpcRequest::Stop);
    let _ = loop_thread.join().unwrap();
    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn test_audio_pipeline_e2e_silence_rejected() {
    let temp_dir = std::env::temp_dir().join(format!(
        "speechyyy_audio_e2e_silence_{}",
        std::process::id()
    ));
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

    // 0.5s of low noise (RMS 0.001 < 0.005 threshold)
    let silence_chunk = AudioChunk::new(vec![0.001; 8000]);
    let capture = MockAudioCapture::with_queued_chunk(silence_chunk);
    let vad = EnergyVad::default();
    let inference = MockSpeechInference::with_transcript("Pipeline speech transcription");
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

    // Verify NOTHING was injected (silence rejected)
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
fn test_audio_pipeline_e2e_micro_tap_rejected() {
    let temp_dir = std::env::temp_dir().join(format!(
        "speechyyy_audio_e2e_microtap_{}",
        std::process::id()
    ));
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

    // 50ms of audio (800 samples < 3200 minimum duration samples)
    let micro_chunk = AudioChunk::new(vec![0.1; 800]);
    let capture = MockAudioCapture::with_queued_chunk(micro_chunk);
    let vad = EnergyVad::default();
    let inference = MockSpeechInference::with_transcript("Pipeline speech transcription");
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

    // Verify NOTHING was injected (micro-tap rejected)
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
fn test_audio_pipeline_differential_mic_and_dc_offset_repro() {
    let model_dir = ModelManager::model_dir(LocalModelType::MoonshineBase);
    let wav_path = Path::new("tests/test_data/0.wav");
    if !model_dir.exists() || !wav_path.exists() {
        eprintln!("Skipping repro test: model or test wav not found");
        return;
    }

    let (speech_samples, _) = sherpa_rs::read_audio_file(wav_path.to_str().unwrap()).unwrap();

    // 1. Symptom B: Constant DC offset (e.g. 0.2023) without speech must be rejected as silence by VAD
    let pure_dc_raw = vec![0.2023f32; 48000 * 2]; // 1 second of stereo DC 0.2023 at 48kHz
    let processed_dc = process_raw_audio(&pure_dc_raw, 48000, 2);
    let vad = EnergyVad::default();
    assert_eq!(
        vad.evaluate(&processed_dc),
        None,
        "Expected pure DC offset to be rejected by VAD after DC removal, but it was detected as speech"
    );

    // 2. Symptom A: Real speech on differential mic (stereo with phase inversion L = s, R = -s + DC bias 0.2023)
    // Interleaved stereo samples: [L0, R0, L1, R1, ...]
    let mut raw_stereo = Vec::with_capacity(speech_samples.len() * 2);
    for &s in &speech_samples {
        raw_stereo.push(s + 0.2023);
        raw_stereo.push(-s + 0.2023);
    }

    // Process raw 16kHz stereo audio through audio pipeline
    let processed_chunk = process_raw_audio(&raw_stereo, 16000, 2);

    // VAD must detect speech
    let metrics = vad
        .evaluate(&processed_chunk)
        .expect("VAD must detect speech in differential mic audio");
    assert!(metrics.rms > 0.01);

    // Decoder must transcribe valid speech rather than returning an empty string
    let mut local = LocalSpeechInference::new(LocalModelType::MoonshineBase, model_dir);
    let transcript = local
        .transcribe(&processed_chunk)
        .expect("Transcription must succeed");

    assert!(
        !transcript.trim().is_empty(),
        "Decoder yielded an empty string on differential mic audio"
    );
    let lower = transcript.to_lowercase();
    assert!(
        lower.contains("yellow lamps"),
        "Expected transcript to contain 'yellow lamps', got: {}",
        transcript
    );
}
