use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DictationState {
    Idle,
    Recording,
    Processing,
    Emitting,
    Error,
}

impl DictationState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Recording => "Recording",
            Self::Processing => "Processing",
            Self::Emitting => "Emitting",
            Self::Error => "Error",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum DaemonEvent {
    StateChanged {
        state: DictationState,
        #[serde(skip_serializing_if = "Option::is_none")]
        previous: Option<DictationState>,
    },
    AudioLevel {
        rms: f32,
        peak: f32,
    },
    TranscriptPreview {
        text: String,
        is_final: bool,
    },
    Error {
        code: String,
        message: String,
    },
    Canceled {
        reason: String,
    },
}

impl DaemonEvent {
    pub fn audio_level(rms: f32, peak: f32) -> Self {
        Self::AudioLevel {
            rms: rms.clamp(0.0, 1.0),
            peak: peak.clamp(0.0, 1.0),
        }
    }

    pub fn to_ndjson(&self) -> Result<String, serde_json::Error> {
        let mut json = serde_json::to_string(self)?;
        json.push('\n');
        Ok(json)
    }
}

pub struct EventBroadcaster {
    listener: UnixListener,
    subscribers: Arc<Mutex<Vec<UnixStream>>>,
    tx: Sender<DaemonEvent>,
    _worker_handle: Option<JoinHandle<()>>,
}

impl EventBroadcaster {
    pub fn bind(path: &Path) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            crate::ipc::create_secure_dir(parent)?;
        }
        if path.exists() {
            let _ = fs::remove_file(path);
        }

        let listener = UnixListener::bind(path)?;
        listener.set_nonblocking(true)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }

        let subscribers = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = channel::<DaemonEvent>();

        let subs_clone = Arc::clone(&subscribers);
        let worker_handle = thread::spawn(move || {
            Self::worker_loop(rx, subs_clone);
        });

        Ok(Self {
            listener,
            subscribers,
            tx,
            _worker_handle: Some(worker_handle),
        })
    }

    pub fn accept_subscribers(&self) {
        while let Ok((stream, _)) = self.listener.accept() {
            let _ = stream.set_nonblocking(true);
            if let Ok(mut subs) = self.subscribers.lock() {
                subs.push(stream);
            }
        }
    }

    pub fn subscriber_count(&self) -> usize {
        self.subscribers.lock().map(|s| s.len()).unwrap_or(0)
    }

    pub fn sender(&self) -> Sender<DaemonEvent> {
        self.tx.clone()
    }

    /// Broadcasts an event immediately to all subscribers synchronously,
    /// dropping AudioLevel on WouldBlock and pruning dead/closed sockets.
    pub fn broadcast(&self, event: DaemonEvent) {
        let ndjson = match event.to_ndjson() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to serialize DaemonEvent: {}", e);
                return;
            }
        };

        let bytes = ndjson.as_bytes();

        if let Ok(mut subs) = self.subscribers.lock() {
            subs.retain_mut(|stream| {
                match stream.write_all(bytes) {
                    Ok(_) => true,
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        // Frame is dropped because buffer is full; keep subscriber connected
                        true
                    }
                    Err(ref e)
                        if e.kind() == std::io::ErrorKind::BrokenPipe
                            || e.kind() == std::io::ErrorKind::ConnectionReset
                            || e.raw_os_error() == Some(libc::EPIPE) =>
                    {
                        // Abrupt disconnect: prune subscriber
                        false
                    }
                    Err(_) => false,
                }
            });
        }
    }

    fn worker_loop(rx: Receiver<DaemonEvent>, subscribers: Arc<Mutex<Vec<UnixStream>>>) {
        while let Ok(event) = rx.recv() {
            let ndjson = match event.to_ndjson() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Worker failed to serialize DaemonEvent: {}", e);
                    continue;
                }
            };

            let bytes = ndjson.as_bytes();

            if let Ok(mut subs) = subscribers.lock() {
                subs.retain_mut(|stream| {
                    match stream.write_all(bytes) {
                        Ok(_) => true,
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            // Frame is dropped because buffer is full; keep subscriber connected
                            true
                        }
                        Err(ref e)
                            if e.kind() == std::io::ErrorKind::BrokenPipe
                                || e.kind() == std::io::ErrorKind::ConnectionReset
                                || e.raw_os_error() == Some(libc::EPIPE) =>
                        {
                            false
                        }
                        Err(_) => false,
                    }
                });
            }
        }
    }
}
