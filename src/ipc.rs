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
}

impl RuntimePaths {
    pub fn new() -> Self {
        let base = std::env::var("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let uid = unsafe { libc::getuid() };
                PathBuf::from(format!("/tmp/speechyyy-{}", uid))
            });
        let dir = base.join("speechyyy");
        Self::with_dir(dir)
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
        }
    }

    pub fn ensure_dir(&self) -> std::io::Result<()> {
        fs::create_dir_all(&self.dir)
    }

    pub fn cleanup(&self) {
        PidFile::remove(&self.pid_path);
        let _ = fs::remove_file(&self.socket_path);
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
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory {}: {}", parent.display(), e))?;
        }

        if let Some(existing_pid) = Self::read_pid(path) {
            if Self::is_process_running(existing_pid) {
                return Err(format!(
                    "Daemon is already running with PID {}",
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
                "Failed to connect to daemon socket at {}: {}",
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

    pub fn start_listener(&self) -> Result<UnixListener, std::io::Result<()>> {
        if let Some(parent) = self.paths.socket_path.parent() {
            fs::create_dir_all(parent).map_err(Err)?;
        }
        let _ = fs::remove_file(&self.paths.socket_path);
        UnixListener::bind(&self.paths.socket_path).map_err(Err)
    }

    pub fn handle_connection(
        stream: &mut UnixStream,
        running: &AtomicBool,
        current_state: &str,
        hotkey: &str,
        pacing_delay_ms: u64,
    ) -> std::io::Result<()> {
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
                    message: "Daemon stopping".into(),
                }
            }
            Err(e) => IpcResponse::Error {
                message: format!("Malformed command: {}", e),
            },
        };

        let mut out = serde_json::to_string(&resp).unwrap();
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
}
