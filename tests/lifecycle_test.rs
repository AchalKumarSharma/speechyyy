use speechyyy::daemon::stop_daemon;
use speechyyy::ipc::{IpcClient, IpcRequest, IpcResponse, IpcServer, PidFile, RuntimePaths};
use std::fs;
use std::sync::atomic::Ordering;
use std::time::Duration;

#[test]
fn test_daemon_full_lifecycle_on_temp_paths() {
    let temp_dir = std::env::temp_dir().join(format!("speechyyy_life_{}", std::process::id()));
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
                let _ = IpcServer::handle_connection(&mut stream, &server_running, "Idle", "RightAlt", 3);
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
    assert_eq!(running.load(Ordering::SeqCst), false);

    server_thread.join().unwrap();

    assert_eq!(PidFile::read_pid(&paths.pid_path), None);
    assert!(!paths.socket_path.exists());

    let _ = fs::remove_dir_all(temp_dir);
}
