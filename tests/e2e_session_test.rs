use evdev::KeyCode;
use speechyyy::daemon::run_session_loop;
use speechyyy::engine::{MockDictationEngine, StateMachineController, TargetChord};
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
fn test_end_to_end_push_to_talk_mock_session() {
    let temp_dir = std::env::temp_dir().join(format!("speechyyy_e2e_{}", std::process::id()));
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

    let engine = MockDictationEngine::with_payload("Hello from Speechyyy");
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

    // 1. Simulate Push-to-Talk: KeyDown RightAlt
    raw_source
        .lock()
        .unwrap()
        .push_event(KeyCode::KEY_RIGHTALT, true);
    std::thread::sleep(Duration::from_millis(50));

    // 2. Query status while key is held down
    let status_res = IpcClient::send_request(&paths.socket_path, &IpcRequest::Status);
    assert!(status_res.is_ok());
    match status_res.unwrap() {
        IpcResponse::Status {
            state,
            hotkey,
            pacing_delay_ms,
            ..
        } => {
            assert_eq!(state, "Recording");
            assert_eq!(hotkey, "RightAlt");
            assert_eq!(pacing_delay_ms, 3);
        }
        other => panic!("Expected status Recording, got {:?}", other),
    }

    // 3. Simulate KeyUp RightAlt -> triggers Mock emission
    raw_source
        .lock()
        .unwrap()
        .push_event(KeyCode::KEY_RIGHTALT, false);
    std::thread::sleep(Duration::from_millis(50));

    // Verify injected output
    let strings = raw_injector
        .lock()
        .unwrap()
        .injected_strings
        .lock()
        .unwrap()
        .clone();
    assert_eq!(strings, vec!["Hello from Speechyyy".to_string()]);

    // 4. Send Stop command via IPC
    let stop_res = IpcClient::send_request(&paths.socket_path, &IpcRequest::Stop);
    assert!(stop_res.is_ok());

    let loop_result = loop_thread.join().unwrap();
    assert!(loop_result.is_ok());

    let _ = fs::remove_dir_all(temp_dir);
}
