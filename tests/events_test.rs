use speechyyy::audio::calculate_block_rms;
use speechyyy::events::{DaemonEvent, DictationState, EventBroadcaster};
use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[test]
fn test_daemon_event_ndjson_serialization_and_deserialization() {
    // 1. StateChanged
    let state_ev = DaemonEvent::StateChanged {
        state: DictationState::Recording,
        previous: Some(DictationState::Idle),
    };
    let ndjson = state_ev.to_ndjson().expect("Serialization failed");
    assert!(
        ndjson.ends_with('\n'),
        "NDJSON must end with a single newline character"
    );
    assert_eq!(
        ndjson.matches('\n').count(),
        1,
        "NDJSON must contain exactly one newline character"
    );
    let parsed: DaemonEvent =
        serde_json::from_str(ndjson.trim()).expect("Failed to deserialize StateChanged");
    assert_eq!(parsed, state_ev);

    // 2. AudioLevel clamped between 0.0 and 1.0
    let audio_ev = DaemonEvent::audio_level(0.0425, 0.118);
    let ndjson = audio_ev.to_ndjson().expect("Serialization failed");
    assert!(ndjson.ends_with('\n'));
    let parsed: DaemonEvent =
        serde_json::from_str(ndjson.trim()).expect("Failed to deserialize AudioLevel");
    assert_eq!(parsed, audio_ev);

    // AudioLevel clamping
    let clamped_ev = DaemonEvent::audio_level(-0.5, 2.5);
    if let DaemonEvent::AudioLevel { rms, peak } = clamped_ev {
        assert_eq!(rms, 0.0);
        assert_eq!(peak, 1.0);
    } else {
        panic!("Expected AudioLevel variant");
    }

    // 3. Error
    let error_ev = DaemonEvent::Error {
        code: "rate_limited".to_string(),
        message: "Sarvam AI rate limit exceeded".to_string(),
    };
    let ndjson = error_ev.to_ndjson().expect("Serialization failed");
    let parsed: DaemonEvent =
        serde_json::from_str(ndjson.trim()).expect("Failed to deserialize Error");
    assert_eq!(parsed, error_ev);

    // 4. Canceled
    let cancel_ev = DaemonEvent::Canceled {
        reason: "shortcut_cancel".to_string(),
    };
    let ndjson = cancel_ev.to_ndjson().expect("Serialization failed");
    let parsed: DaemonEvent =
        serde_json::from_str(ndjson.trim()).expect("Failed to deserialize Canceled");
    assert_eq!(parsed, cancel_ev);
}

#[test]
fn test_audio_rms_metering_math_and_atomic_bit_casting() {
    // 1. Silence
    let silence = [0.0f32; 512];
    assert_eq!(calculate_block_rms(&silence), 0.0);

    // 2. DC constant signal 0.5
    let dc_signal = [0.5f32; 512];
    let dc_rms = calculate_block_rms(&dc_signal);
    assert!((dc_rms - 0.5).abs() < 1e-6);

    // 3. Clipped signal (1.0 or higher clamped to 1.0)
    let clipped = [1.5f32; 256];
    let clipped_rms = calculate_block_rms(&clipped);
    assert_eq!(clipped_rms, 1.0);

    // 4. IEEE 754 bit round-tripping via AtomicU32
    let atomic_rms = Arc::new(AtomicU32::new(0));
    let original_rms = 0.0425f32;
    atomic_rms.store(original_rms.to_bits(), Ordering::Relaxed);

    let retrieved_bits = atomic_rms.load(Ordering::Relaxed);
    let retrieved_rms = f32::from_bits(retrieved_bits);
    assert_eq!(retrieved_rms, original_rms);
}

#[test]
fn test_broadcaster_fanout_and_subscriber_pruning() {
    let temp_dir = std::env::temp_dir().join(format!("broadcaster_test_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let sock_path = temp_dir.join("events.sock");

    let broadcaster = EventBroadcaster::bind(&sock_path).expect("Failed to bind broadcaster");

    // Client 1 connects
    let client1 = UnixStream::connect(&sock_path).expect("Client 1 failed to connect");
    client1
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();

    // Client 2 connects and is closed immediately
    let client2 = UnixStream::connect(&sock_path).expect("Client 2 failed to connect");
    drop(client2); // abrupt disconnect -> BrokenPipe / EPIPE on write

    // Accept incoming subscriber connections
    broadcaster.accept_subscribers();
    assert_eq!(broadcaster.subscriber_count(), 2);

    // Broadcast critical state change event
    broadcaster.broadcast(DaemonEvent::StateChanged {
        state: DictationState::Recording,
        previous: Some(DictationState::Idle),
    });

    // Client 1 should read the event
    let mut reader = BufReader::new(client1);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .expect("Failed to read line from client 1");
    assert!(line.contains("\"event\":\"state_changed\""));

    // Broadcaster should have pruned disconnected client 2
    assert_eq!(broadcaster.subscriber_count(), 1);

    let _ = std::fs::remove_file(&sock_path);
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_broadcaster_drops_audio_level_on_would_block() {
    let temp_dir =
        std::env::temp_dir().join(format!("broadcaster_block_test_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let sock_path = temp_dir.join("events.sock");

    let broadcaster = EventBroadcaster::bind(&sock_path).expect("Failed to bind broadcaster");

    let _client = UnixStream::connect(&sock_path).expect("Client failed to connect");
    // Never read from client to saturate socket buffer
    broadcaster.accept_subscribers();
    assert_eq!(broadcaster.subscriber_count(), 1);

    // Send high-frequency AudioLevel frames; broadcaster must NEVER hang or block
    for _ in 0..5000 {
        broadcaster.broadcast(DaemonEvent::audio_level(0.5, 0.8));
    }

    // Broadcaster remains alive and subscriber is not unlinked unless an unrecoverable error occurs
    assert_eq!(broadcaster.subscriber_count(), 1);

    let _ = std::fs::remove_file(&sock_path);
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_daemon_events_fanout_end_to_end() {
    use speechyyy::daemon::run_session_loop_with_broadcaster;
    use speechyyy::engine::{MockDictationEngine, StateMachineController, TargetChord};
    use speechyyy::injector::RecordingInjector;
    use speechyyy::input::FakeInputSource;
    use speechyyy::ipc::{IpcClient, IpcRequest, IpcServer, RuntimePaths};

    let temp_dir = std::env::temp_dir().join(format!("daemon_events_e2e_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let paths = RuntimePaths::with_dir(temp_dir.clone());

    let server = IpcServer::new(paths.clone());
    let listener = server.start_listener().expect("bind listener");
    listener.set_nonblocking(true).unwrap();
    let running = server.running_flag();

    let broadcaster = EventBroadcaster::bind(&paths.events_socket_path).expect("bind broadcaster");

    // Client connects to events.sock
    let event_stream =
        UnixStream::connect(&paths.events_socket_path).expect("Connect to events.sock");
    event_stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut event_reader = BufReader::new(event_stream);

    let mut input_source = FakeInputSource::new();
    let mut injector = RecordingInjector::new(Duration::ZERO);
    let engine = MockDictationEngine::with_payload("event stream text");
    let mut controller = StateMachineController::new(TargetChord::right_alt(), engine);

    let loop_running = running.clone();
    let loop_paths = paths.clone();
    let loop_thread = std::thread::spawn(move || {
        let _ = run_session_loop_with_broadcaster(
            &listener,
            &mut input_source,
            &mut injector,
            &mut controller,
            &loop_running,
            "RightAlt",
            10,
            Some(&broadcaster),
        );
    });

    std::thread::sleep(Duration::from_millis(50));

    // 1. Send PushDown -> Expect StateChanged (Recording)
    let _ = IpcClient::send_request(&loop_paths.socket_path, &IpcRequest::PushDown).unwrap();
    let mut line = String::new();
    event_reader.read_line(&mut line).unwrap();
    assert!(
        line.contains("\"state\":\"recording\""),
        "Expected recording event, got: {}",
        line
    );

    // 2. Send Cancel -> Expect Canceled event and StateChanged (Idle)
    let _ = IpcClient::send_request(&loop_paths.socket_path, &IpcRequest::Cancel).unwrap();
    line.clear();
    event_reader.read_line(&mut line).unwrap();
    assert!(
        line.contains("\"event\":\"canceled\""),
        "Expected canceled event, got: {}",
        line
    );
    line.clear();
    event_reader.read_line(&mut line).unwrap();
    assert!(
        line.contains("\"state\":\"idle\""),
        "Expected idle event, got: {}",
        line
    );

    // 3. Send PushDown followed by PushUp -> Expect Recording -> Processing -> Emitting -> Idle
    let _ = IpcClient::send_request(&loop_paths.socket_path, &IpcRequest::PushDown).unwrap();
    line.clear();
    event_reader.read_line(&mut line).unwrap();
    assert!(line.contains("\"state\":\"recording\""));

    let _ = IpcClient::send_request(&loop_paths.socket_path, &IpcRequest::PushUp).unwrap();
    line.clear();
    event_reader.read_line(&mut line).unwrap();
    assert!(
        line.contains("\"state\":\"processing\""),
        "Expected processing event, got: {}",
        line
    );

    line.clear();
    event_reader.read_line(&mut line).unwrap();
    assert!(
        line.contains("\"state\":\"emitting\""),
        "Expected emitting event, got: {}",
        line
    );

    line.clear();
    event_reader.read_line(&mut line).unwrap();
    assert!(
        line.contains("\"state\":\"idle\""),
        "Expected idle event, got: {}",
        line
    );

    // Stop daemon
    let _ = IpcClient::send_request(&loop_paths.socket_path, &IpcRequest::Stop).unwrap();
    let _ = loop_thread.join();
    paths.cleanup();
    let _ = std::fs::remove_dir_all(temp_dir);
}
