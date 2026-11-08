use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum IpcRequest {
    Status,
    Stop,
    Restart,
    Ping,
    PushDown,
    PushUp,
    Toggle,
    Cancel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcResponse {
    Ok {
        message: String,
    },
    Status {
        pid: u32,
        state: String,
        hotkey: String,
        pacing_delay_ms: u64,
    },
    Pong,
    PushDown,
    PushUp,
    Error {
        message: String,
    },
}

#[derive(Debug, Clone)]
pub struct RuntimePaths {
    pub dir: PathBuf,
    pub socket_path: PathBuf,
    pub events_socket_path: PathBuf,
    pub pid_path: PathBuf,
    pub dock_pid_path: PathBuf,
    pub binary_path: Option<PathBuf>,
}

impl RuntimePaths {
    pub fn new() -> Self {
        let dir = std::env::var("XDG_RUNTIME_DIR")
            .map(|xdg| PathBuf::from(xdg).join("speechyyy"))
            .unwrap_or_else(|_| {
                let uid = unsafe { libc::getuid() };
                PathBuf::from(format!("/tmp/speechyyy-{}", uid))
            });
        let mut paths = Self::with_dir(dir);
        paths.binary_path = std::env::current_exe().ok();
        paths
    }
}

impl Default for RuntimePaths {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimePaths {
    pub fn with_dir(dir: PathBuf) -> Self {
        let socket_path = dir.join("speechyyy.sock");
        let events_socket_path = dir.join("events.sock");
        let pid_path = dir.join("speechyyy.pid");
        let dock_pid_path = dir.join("speechyyy-dock.pid");
        Self {
            dir,
            socket_path,
            events_socket_path,
            pid_path,
            dock_pid_path,
            binary_path: None,
        }
    }

    pub fn binary_path(&self) -> Option<PathBuf> {
        self.binary_path
            .clone()
            .or_else(|| std::env::current_exe().ok())
    }

    pub fn with_binary_path(mut self, path: PathBuf) -> Self {
        self.binary_path = Some(path);
        self
    }

    pub fn ensure_dir(&self) -> std::io::Result<()> {
        create_secure_dir(&self.dir)
    }

    pub fn cleanup(&self) {
        if let Some(pid) = PidFile::read_pid(&self.pid_path) {
            if pid == std::process::id() || !PidFile::is_process_running(pid) {
                PidFile::remove(&self.pid_path);
            }
        } else {
            PidFile::remove(&self.pid_path);
        }
        let _ = fs::remove_file(&self.socket_path);
        let _ = fs::remove_file(&self.events_socket_path);
    }
}

pub fn create_secure_dir(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        builder.mode(0o700);
        builder.create(path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(path)
    }
}

pub struct PidFile;

impl PidFile {
    pub fn is_process_running(pid: u32) -> bool {
        unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
    }

    pub fn read_pid(path: &Path) -> Option<u32> {
        fs::read_to_string(path).ok()?.trim().parse::<u32>().ok()
    }

    pub fn acquire(path: &Path) -> Result<(), String> {
        Self::acquire_for_service(path, "Speechyyy")
    }

    pub fn acquire_for_service(path: &Path, service_name: &str) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            create_secure_dir(parent)
                .map_err(|e| format!("Failed to create directory {}: {}", parent.display(), e))?;
        }

        if let Some(existing_pid) = Self::read_pid(path) {
            if Self::is_process_running(existing_pid) {
                return Err(format!(
                    "{} is already running with PID {}",
                    service_name, existing_pid
                ));
            }
            let _ = fs::remove_file(path);
        }

        let pid = std::process::id();
        fs::write(path, pid.to_string())
            .map_err(|e| format!("Failed to write PID file {}: {}", path.display(), e))
    }

    pub fn remove(path: &Path) {
        let _ = fs::remove_file(path);
    }

    pub fn remove_if_dead(path: &Path) {
        if let Some(pid) = Self::read_pid(path) {
            if !Self::is_process_running(pid) {
                let _ = fs::remove_file(path);
            }
        } else {
            let _ = fs::remove_file(path);
        }
    }
}

pub struct IpcClient;

impl IpcClient {
    pub fn send_request(socket_path: &Path, req: &IpcRequest) -> Result<IpcResponse, String> {
        let mut stream = UnixStream::connect(socket_path).map_err(|e| {
            format!(
                "Failed to connect to Speechyyy socket at {}: {}",
                socket_path.display(),
                e
            )
        })?;

        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .map_err(|e| e.to_string())?;

        let mut payload = serde_json::to_string(req).map_err(|e| e.to_string())?;
        payload.push('\n');
        stream
            .write_all(payload.as_bytes())
            .map_err(|e| e.to_string())?;
        stream.flush().map_err(|e| e.to_string())?;

        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| e.to_string())?;

        serde_json::from_str(line.trim()).map_err(|e| format!("Invalid JSON response: {}", e))
    }
}

pub struct IpcServer {
    paths: RuntimePaths,
    running: Arc<AtomicBool>,
}

impl IpcServer {
    pub fn new(paths: RuntimePaths) -> Self {
        Self {
            paths,
            running: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn running_flag(&self) -> Arc<AtomicBool> {
        self.running.clone()
    }

    pub fn start_listener(&self) -> Result<UnixListener, std::io::Error> {
        if let Some(parent) = self.paths.socket_path.parent() {
            create_secure_dir(parent)?;
        }
        if self.paths.socket_path.exists() {
            match IpcClient::send_request(&self.paths.socket_path, &IpcRequest::Ping) {
                Ok(IpcResponse::Pong) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::AddrInUse,
                        format!(
                            "Active Speechyyy daemon is already listening on {}",
                            self.paths.socket_path.display()
                        ),
                    ));
                }
                _ => {
                    match UnixStream::connect(&self.paths.socket_path) {
                        Err(ref e) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
                            eprintln!(
                                "Detected stale socket at {}, unlinking...",
                                self.paths.socket_path.display()
                            );
                            let _ = fs::remove_file(&self.paths.socket_path);
                        }
                        Ok(_) => {
                            eprintln!(
                                "Detected non-responsive socket at {}, unlinking...",
                                self.paths.socket_path.display()
                            );
                            let _ = fs::remove_file(&self.paths.socket_path);
                        }
                        Err(e) => {
                            return Err(e);
                        }
                    }
                }
            }
        }
        let listener = UnixListener::bind(&self.paths.socket_path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.paths.socket_path, fs::Permissions::from_mode(0o600))?;
        }
        Ok(listener)
    }

    pub fn handle_connection(
        stream: &mut UnixStream,
        running: &AtomicBool,
        current_state: &str,
        hotkey: &str,
        pacing_delay_ms: u64,
    ) -> std::io::Result<Option<IpcRequest>> {
        stream.set_read_timeout(Some(Duration::from_millis(500)))?;
        stream.set_write_timeout(Some(Duration::from_millis(500)))?;

        let mut reader = BufReader::new(stream.try_clone()?);
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }

        let (resp, action) = match serde_json::from_str::<IpcRequest>(line.trim()) {
            Ok(IpcRequest::Ping) => (IpcResponse::Pong, None),
            Ok(IpcRequest::Status) => (
                IpcResponse::Status {
                    pid: std::process::id(),
                    state: current_state.to_string(),
                    hotkey: hotkey.to_string(),
                    pacing_delay_ms,
                },
                None,
            ),
            Ok(IpcRequest::Stop) => {
                running.store(false, Ordering::SeqCst);
                (
                    IpcResponse::Ok {
                        message: "Speechyyy stopping".into(),
                    },
                    Some(IpcRequest::Stop),
                )
            }
            Ok(IpcRequest::Restart) => {
                running.store(false, Ordering::SeqCst);
                (
                    IpcResponse::Ok {
                        message: "Speechyyy restarting".into(),
                    },
                    Some(IpcRequest::Restart),
                )
            }
            Ok(IpcRequest::PushDown) => (IpcResponse::PushDown, Some(IpcRequest::PushDown)),
            Ok(IpcRequest::PushUp) => (IpcResponse::PushUp, Some(IpcRequest::PushUp)),
            Ok(IpcRequest::Toggle) => (
                IpcResponse::Ok {
                    message: "Toggling recording state".into(),
                },
                Some(IpcRequest::Toggle),
            ),
            Ok(IpcRequest::Cancel) => (
                IpcResponse::Ok {
                    message: "Dictation canceled".into(),
                },
                Some(IpcRequest::Cancel),
            ),
            Err(e) => (
                IpcResponse::Error {
                    message: format!("Malformed command: {}", e),
                },
                None,
            ),
        };

        let mut out = serde_json::to_string(&resp)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        out.push('\n');
        stream.write_all(out.as_bytes())?;
        stream.flush()?;
        Ok(action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_roundtrip_ping_and_status() {
        let temp_dir = std::env::temp_dir().join(format!("speechyyy_test_{}", std::process::id()));
        let paths = RuntimePaths::with_dir(temp_dir.clone());
        let server = IpcServer::new(paths.clone());
        let listener = server.start_listener().expect("listener should bind");
        listener.set_nonblocking(true).unwrap();

        let running = server.running_flag();

        let socket_path = paths.socket_path.clone();
        let client_thread = std::thread::spawn(move || {
            let res = IpcClient::send_request(&socket_path, &IpcRequest::Ping);
            res.expect("ping request should succeed")
        });

        let mut stream = loop {
            if let Ok((s, _)) = listener.accept() {
                break s;
            }
            std::thread::sleep(Duration::from_millis(5));
        };

        IpcServer::handle_connection(&mut stream, &running, "Idle", "RightAlt", 3)
            .expect("handle connection");
        let resp = client_thread.join().unwrap();
        assert_eq!(resp, IpcResponse::Pong);

        let socket_path = paths.socket_path.clone();
        let client_thread = std::thread::spawn(move || {
            IpcClient::send_request(&socket_path, &IpcRequest::Status).expect("status request")
        });

        let mut stream = loop {
            if let Ok((s, _)) = listener.accept() {
                break s;
            }
            std::thread::sleep(Duration::from_millis(5));
        };

        IpcServer::handle_connection(&mut stream, &running, "Recording", "RightAlt", 3)
            .expect("handle connection");
        let resp = client_thread.join().unwrap();
        match resp {
            IpcResponse::Status {
                pid,
                state,
                hotkey,
                pacing_delay_ms,
            } => {
                assert_eq!(pid, std::process::id());
                assert_eq!(state, "Recording");
                assert_eq!(hotkey, "RightAlt");
                assert_eq!(pacing_delay_ms, 3);
            }
            other => panic!("Unexpected response: {:?}", other),
        }

        paths.cleanup();
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_ipc_push_down_up_and_toggle() {
        let temp_dir = std::env::temp_dir().join(format!("speechyyy_push_test_{}", std::process::id()));
        let paths = RuntimePaths::with_dir(temp_dir.clone());
        let server = IpcServer::new(paths.clone());
        let listener = server.start_listener().expect("listener should bind");
        listener.set_nonblocking(true).unwrap();

        let running = server.running_flag();

        // 1. PushDown request
        let socket_path = paths.socket_path.clone();
        let client_thread = std::thread::spawn(move || {
            IpcClient::send_request(&socket_path, &IpcRequest::PushDown).expect("push down request")
        });

        let mut stream = loop {
            if let Ok((s, _)) = listener.accept() {
                break s;
            }
            std::thread::sleep(Duration::from_millis(5));
        };

        let action = IpcServer::handle_connection(&mut stream, &running, "Idle", "RightAlt", 3)
            .expect("handle connection");
        assert_eq!(action, Some(IpcRequest::PushDown));
        let resp = client_thread.join().unwrap();
        assert_eq!(resp, IpcResponse::PushDown);

        // 2. PushUp request
        let socket_path = paths.socket_path.clone();
        let client_thread = std::thread::spawn(move || {
            IpcClient::send_request(&socket_path, &IpcRequest::PushUp).expect("push up request")
        });

        let mut stream = loop {
            if let Ok((s, _)) = listener.accept() {
                break s;
            }
            std::thread::sleep(Duration::from_millis(5));
        };

        let action = IpcServer::handle_connection(&mut stream, &running, "Recording", "RightAlt", 3)
            .expect("handle connection");
        assert_eq!(action, Some(IpcRequest::PushUp));
        let resp = client_thread.join().unwrap();
        assert_eq!(resp, IpcResponse::PushUp);

        // 3. Toggle request
        let socket_path = paths.socket_path.clone();
        let client_thread = std::thread::spawn(move || {
            IpcClient::send_request(&socket_path, &IpcRequest::Toggle).expect("toggle request")
        });

        let mut stream = loop {
            if let Ok((s, _)) = listener.accept() {
                break s;
            }
            std::thread::sleep(Duration::from_millis(5));
        };

        let action = IpcServer::handle_connection(&mut stream, &running, "Idle", "RightAlt", 3)
            .expect("handle connection");
        assert_eq!(action, Some(IpcRequest::Toggle));
        let resp = client_thread.join().unwrap();
        match resp {
            IpcResponse::Ok { message } => assert!(message.contains("Toggling")),
            other => panic!("Unexpected response for toggle: {:?}", other),
        }

        paths.cleanup();
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_pid_file_lifecycle() {
        let temp_dir =
            std::env::temp_dir().join(format!("speechyyy_pid_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);
        let pid_path = temp_dir.join("test.pid");

        assert!(PidFile::acquire(&pid_path).is_ok());
        let read = PidFile::read_pid(&pid_path);
        assert_eq!(read, Some(std::process::id()));

        assert!(PidFile::acquire(&pid_path).is_err());

        PidFile::remove(&pid_path);
        assert_eq!(PidFile::read_pid(&pid_path), None);

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_creates_parent_directory_automatically() {
        let temp_dir = std::env::temp_dir()
            .join(format!("speechyyy_nested_{}", std::process::id()))
            .join("sub_dir")
            .join("nested");
        let paths = RuntimePaths::with_dir(temp_dir.clone());

        assert!(!temp_dir.exists());

        // Acquire PID creates parent directories automatically
        assert!(PidFile::acquire(&paths.pid_path).is_ok());
        assert!(temp_dir.exists());
        assert!(paths.pid_path.exists());

        // Start listener also ensures parent directories exist
        let server = IpcServer::new(paths.clone());
        let listener = server.start_listener();
        assert!(listener.is_ok());
        assert!(paths.socket_path.exists());

        paths.cleanup();
        let root_created = temp_dir.parent().unwrap().parent().unwrap();
        let _ = fs::remove_dir_all(root_created);
    }

    #[cfg(unix)]
    #[test]
    fn test_ipc_directory_and_socket_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let temp_dir = std::env::temp_dir()
            .join(format!("speechyyy_perm_test_{}", std::process::id()))
            .join("sock_dir");
        let paths = RuntimePaths::with_dir(temp_dir.clone());
        paths.ensure_dir().expect("ensure_dir should succeed");

        let dir_meta = fs::metadata(&temp_dir).expect("dir metadata");
        let dir_mode = dir_meta.permissions().mode() & 0o777;
        assert_eq!(dir_mode, 0o700, "Runtime directory mode must be 0700");

        let server = IpcServer::new(paths.clone());
        let _listener = server.start_listener().expect("listener should bind");
        let sock_meta = fs::metadata(&paths.socket_path).expect("socket metadata");
        let sock_mode = sock_meta.permissions().mode() & 0o777;
        assert_eq!(sock_mode, 0o600, "Socket file mode must be 0600");

        paths.cleanup();
        let _ = fs::remove_dir_all(temp_dir.parent().unwrap());
    }

    #[test]
    fn test_stale_socket_recovery() {
        let temp_dir = std::env::temp_dir().join(format!("speechyyy_stale_sock_{}", std::process::id()));
        let paths = RuntimePaths::with_dir(temp_dir.clone());
        paths.ensure_dir().expect("ensure dir");

        // 1. Create a socket listener and immediately drop it without unlinking the file
        {
            let server = IpcServer::new(paths.clone());
            let listener = server.start_listener().expect("bind initial listener");
            assert!(paths.socket_path.exists());
            drop(listener);
        }

        // The socket file still exists on disk, but no process is listening (ECONNREFUSED)
        assert!(paths.socket_path.exists());

        // 2. A new server should detect the stale socket, unlink it, and bind successfully
        let server2 = IpcServer::new(paths.clone());
        let listener2 = server2.start_listener();
        assert!(listener2.is_ok(), "Expected stale socket to be unlinked and rebound");
        assert!(paths.socket_path.exists());

        paths.cleanup();
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_ipc_request_parsing_restart_and_existing() {
        let restart_json = "{\"cmd\":\"restart\"}";
        let parsed_restart: IpcRequest =
            serde_json::from_str(restart_json).expect("parse restart");
        assert_eq!(parsed_restart, IpcRequest::Restart);

        let stop_json = "{\"cmd\":\"stop\"}";
        let parsed_stop: IpcRequest = serde_json::from_str(stop_json).expect("parse stop");
        assert_eq!(parsed_stop, IpcRequest::Stop);

        let ping_json = "{\"cmd\":\"ping\"}";
        let parsed_ping: IpcRequest = serde_json::from_str(ping_json).expect("parse ping");
        assert_eq!(parsed_ping, IpcRequest::Ping);

        let status_json = "{\"cmd\":\"status\"}";
        let parsed_status: IpcRequest = serde_json::from_str(status_json).expect("parse status");
        assert_eq!(parsed_status, IpcRequest::Status);

        let push_down_json = "{\"cmd\":\"push_down\"}";
        let parsed_pd: IpcRequest = serde_json::from_str(push_down_json).expect("parse push_down");
        assert_eq!(parsed_pd, IpcRequest::PushDown);

        let push_up_json = "{\"cmd\":\"push_up\"}";
        let parsed_pu: IpcRequest = serde_json::from_str(push_up_json).expect("parse push_up");
        assert_eq!(parsed_pu, IpcRequest::PushUp);

        let toggle_json = "{\"cmd\":\"toggle\"}";
        let parsed_toggle: IpcRequest = serde_json::from_str(toggle_json).expect("parse toggle");
        assert_eq!(parsed_toggle, IpcRequest::Toggle);

        let cancel_json = "{\"cmd\":\"cancel\"}";
        let parsed_cancel: IpcRequest = serde_json::from_str(cancel_json).expect("parse cancel");
        assert_eq!(parsed_cancel, IpcRequest::Cancel);
    }

    #[test]
    fn test_dock_pid_file_lifecycle() {
        let temp_dir = std::env::temp_dir()
            .join(format!("speechyyy_dock_pid_test_{}", std::process::id()));
        let paths = RuntimePaths::with_dir(temp_dir.clone());
        paths.ensure_dir().expect("ensure dir");

        let pid_path = paths.dock_pid_path.clone();

        // 1. Initial acquire should succeed and write current PID
        let res = PidFile::acquire_for_service(&pid_path, "speechyyy-dock");
        assert!(res.is_ok(), "First acquire should succeed: {:?}", res);
        assert_eq!(PidFile::read_pid(&pid_path), Some(std::process::id()));

        // 2. Second acquire while current PID is running should fail with refusal
        let res2 = PidFile::acquire_for_service(&pid_path, "speechyyy-dock");
        assert!(res2.is_err(), "Acquire while process is live must fail");
        assert!(res2.unwrap_err().contains("already running"));

        // 3. Stale PID file (with dead PID 9999999) should be overwritten cleanly
        fs::write(&pid_path, "9999999").expect("write stale pid");
        assert!(!PidFile::is_process_running(9999999));
        let res3 = PidFile::acquire_for_service(&pid_path, "speechyyy-dock");
        assert!(res3.is_ok(), "Stale PID should be overwritten");
        assert_eq!(PidFile::read_pid(&pid_path), Some(std::process::id()));

        // 4. Removal on clean exit
        PidFile::remove(&pid_path);
        assert!(!pid_path.exists());

        // 5. remove_if_dead should not delete if process is live, but deletes if dead
        fs::write(&pid_path, std::process::id().to_string()).expect("write live pid");
        PidFile::remove_if_dead(&pid_path);
        assert!(
            pid_path.exists(),
            "remove_if_dead must leave live process PID file alone"
        );

        fs::write(&pid_path, "9999999").expect("write dead pid");
        PidFile::remove_if_dead(&pid_path);
        assert!(
            !pid_path.exists(),
            "remove_if_dead must delete dead process PID file"
        );

        let _ = fs::remove_dir_all(temp_dir);
    }
}
