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
    Ping,
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
    Error {
        message: String,
    },
}

#[derive(Debug, Clone)]
pub struct RuntimePaths {
    pub dir: PathBuf,
    pub socket_path: PathBuf,
    pub pid_path: PathBuf,
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
        let pid_path = dir.join("speechyyy.pid");
        Self {
            dir,
            socket_path,
            pid_path,
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
        PidFile::remove(&self.pid_path);
        let _ = fs::remove_file(&self.socket_path);
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
        if let Some(parent) = path.parent() {
            create_secure_dir(parent)
                .map_err(|e| format!("Failed to create directory {}: {}", parent.display(), e))?;
        }

        if let Some(existing_pid) = Self::read_pid(path) {
            if Self::is_process_running(existing_pid) {
                return Err(format!(
                    "Speechyyy is already running with PID {}",
                    existing_pid
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
    ) -> std::io::Result<()> {
        stream.set_read_timeout(Some(Duration::from_millis(500)))?;
        stream.set_write_timeout(Some(Duration::from_millis(500)))?;

        let mut reader = BufReader::new(stream.try_clone()?);
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }

        let resp = match serde_json::from_str::<IpcRequest>(line.trim()) {
            Ok(IpcRequest::Ping) => IpcResponse::Pong,
            Ok(IpcRequest::Status) => IpcResponse::Status {
                pid: std::process::id(),
                state: current_state.to_string(),
                hotkey: hotkey.to_string(),
                pacing_delay_ms,
            },
            Ok(IpcRequest::Stop) => {
                running.store(false, Ordering::SeqCst);
                IpcResponse::Ok {
                    message: "Speechyyy stopping".into(),
                }
            }
            Err(e) => IpcResponse::Error {
                message: format!("Malformed command: {}", e),
            },
        };

        let mut out = serde_json::to_string(&resp)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        out.push('\n');
        stream.write_all(out.as_bytes())?;
        stream.flush()
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
}
