use speechyyy::daemon::stop_daemon;
use speechyyy::ipc::{IpcClient, IpcRequest, IpcResponse, IpcServer, PidFile, RuntimePaths};
use std::fs;
use std::sync::atomic::Ordering;
use std::time::Duration;

#[test]
fn test_daemon_full_lifecycle_on_temp_paths() {
    let temp_dir = std::env::temp_dir().join(format!("speechyyy_life_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    let paths = RuntimePaths::with_dir(temp_dir.clone());

    assert_eq!(PidFile::read_pid(&paths.pid_path), None);
    assert!(!paths.socket_path.exists());

    let server = IpcServer::new(paths.clone());
    let listener = server.start_listener().expect("bind listener");
    listener.set_nonblocking(true).unwrap();
    let running = server.running_flag();

    let server_running = running.clone();
    let server_thread = std::thread::spawn(move || {
        while server_running.load(Ordering::SeqCst) {
            if let Ok((mut stream, _)) = listener.accept() {
                let _ = IpcServer::handle_connection(
                    &mut stream,
                    &server_running,
                    "Idle",
                    "RightAlt",
                    3,
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });

    PidFile::acquire(&paths.pid_path).expect("acquire pid");

    let status_res = IpcClient::send_request(&paths.socket_path, &IpcRequest::Status);
    assert!(status_res.is_ok());
    match status_res.unwrap() {
        IpcResponse::Status {
            pid,
            state,
            hotkey,
            pacing_delay_ms,
        } => {
            assert_eq!(pid, std::process::id());
            assert_eq!(state, "Idle");
            assert_eq!(hotkey, "RightAlt");
            assert_eq!(pacing_delay_ms, 3);
        }
        other => panic!("Unexpected status response: {:?}", other),
    }

    let stop_result = stop_daemon(&paths);
    assert!(stop_result.is_ok());
    assert!(!running.load(Ordering::SeqCst));

    server_thread.join().unwrap();

    assert_eq!(PidFile::read_pid(&paths.pid_path), None);
    assert!(!paths.socket_path.exists());

    let _ = fs::remove_dir_all(temp_dir);
}

#[cfg(unix)]
#[test]
fn test_simulated_sigterm_cleans_up_socket() {
    let temp_dir = std::env::temp_dir().join(format!("speechyyy_sigterm_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    let paths = RuntimePaths::with_dir(temp_dir.clone());
    paths.ensure_dir().expect("ensure dir");

    let server = IpcServer::new(paths.clone());
    let _listener = server.start_listener().expect("bind listener");
    let running = server.running_flag();

    assert!(paths.socket_path.exists());

    // Register real termination signals handler from daemon module
    let signal_handle = speechyyy::daemon::register_termination_signals(std::sync::Arc::clone(&running))
        .expect("register termination signals");

    // Send SIGTERM to the current thread
    unsafe {
        libc::pthread_kill(libc::pthread_self(), libc::SIGTERM);
    }

    // Wait for the daemon signal thread to process SIGTERM and transition running to false
    let start = std::time::Instant::now();
    while running.load(Ordering::SeqCst) && start.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(10));
    }

    assert!(
        !running.load(Ordering::SeqCst),
        "Expected running flag to become false after SIGTERM"
    );

    signal_handle.close();

    // Verify cleanup unlinks socket file
    paths.cleanup();
    assert!(
        !paths.socket_path.exists(),
        "Socket file must be unlinked after shutdown"
    );

    let _ = fs::remove_dir_all(temp_dir);
}

#[cfg(unix)]
#[test]
fn test_secondary_daemon_startup_fails_with_addrinuse_and_preserves_active_socket_and_pid() {
    let temp_dir = std::env::temp_dir().join(format!("speechyyy_dup_start_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    let paths = RuntimePaths::with_dir(temp_dir.clone());
    paths.ensure_dir().expect("ensure dir");

    // 1. Start primary server
    let server1 = IpcServer::new(paths.clone());
    let listener1 = server1.start_listener().expect("bind primary listener");
    listener1.set_nonblocking(true).unwrap();
    let running1 = server1.running_flag();

    let server_running = running1.clone();
    let server_thread = std::thread::spawn(move || {
        while server_running.load(Ordering::SeqCst) {
            if let Ok((mut stream, _)) = listener1.accept() {
                let _ = IpcServer::handle_connection(
                    &mut stream,
                    &server_running,
                    "Idle",
                    "RightAlt",
                    3,
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });

    // Write primary PID
    PidFile::acquire(&paths.pid_path).expect("acquire primary pid");
    let primary_pid = PidFile::read_pid(&paths.pid_path).expect("read primary pid");
    assert!(paths.socket_path.exists());
    assert!(paths.pid_path.exists());

    // 2. Attempt to start secondary server on same runtime paths
    let server2 = IpcServer::new(paths.clone());
    let start_res = server2.start_listener();
    assert!(
        start_res.is_err(),
        "Expected secondary listener to fail because active daemon is listening"
    );
    let err = start_res.unwrap_err();
    assert_eq!(
        err.kind(),
        std::io::ErrorKind::AddrInUse,
        "Error kind must be AddrInUse"
    );

    // 3. Verify that failed secondary start did NOT unlink active daemon's socket or PID file
    assert!(
        paths.socket_path.exists(),
        "Active daemon socket must still exist after secondary start failure"
    );
    assert!(
        paths.pid_path.exists(),
        "Active daemon PID file must still exist after secondary start failure"
    );
    assert_eq!(
        PidFile::read_pid(&paths.pid_path),
        Some(primary_pid),
        "PID file must still contain the primary daemon's PID"
    );

    // 4. Verify primary daemon can still respond to Ping and Status
    let ping_res = IpcClient::send_request(&paths.socket_path, &IpcRequest::Ping);
    assert_eq!(ping_res, Ok(IpcResponse::Pong));

    // 5. Clean up primary daemon
    running1.store(false, Ordering::SeqCst);
    server_thread.join().unwrap();
    paths.cleanup();
    let _ = fs::remove_dir_all(temp_dir);
}

#[cfg(unix)]
#[test]
fn test_ipc_stream_read_timeout_prevents_daemon_block() {
    use std::os::unix::net::UnixStream;

    let temp_dir = std::env::temp_dir().join(format!("speechyyy_timeout_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    let paths = RuntimePaths::with_dir(temp_dir.clone());
    paths.ensure_dir().expect("ensure dir");

    let server = IpcServer::new(paths.clone());
    let listener = server.start_listener().expect("bind listener");
    let running = server.running_flag();

    // Connect a client that sends no data
    let _client_stream = UnixStream::connect(&paths.socket_path).expect("client connect");

    let (mut server_stream, _) = listener.accept().expect("server accept");

    let start = std::time::Instant::now();
    let res = IpcServer::handle_connection(
        &mut server_stream,
        &running,
        "Idle",
        "RightAlt",
        3,
    );
    let elapsed = start.elapsed();

    assert!(
        res.is_err(),
        "handle_connection must return an error on read timeout"
    );
    let err = res.unwrap_err();
    assert!(
        err.kind() == std::io::ErrorKind::WouldBlock || err.kind() == std::io::ErrorKind::TimedOut,
        "Error must be WouldBlock or TimedOut, got {:?}",
        err.kind()
    );
    assert!(
        elapsed >= Duration::from_millis(400) && elapsed < Duration::from_secs(2),
        "Expected timeout around 500ms, elapsed: {:?}",
        elapsed
    );

    paths.cleanup();
    let _ = fs::remove_dir_all(temp_dir);
}
