use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

/// Abstraction over sending signals and checking process liveness.
pub trait SignalSink: Send + Sync {
    /// Send SIGTERM to the process.
    fn send_term(&self, pid: u32) -> Result<(), String>;
    /// Check whether the process is currently alive.
    fn is_alive(&self, pid: u32) -> bool;
}

/// Production implementation of `SignalSink` using standard `libc::kill`.
pub struct RealSignalSink;

impl SignalSink for RealSignalSink {
    fn send_term(&self, pid: u32) -> Result<(), String> {
        #[cfg(unix)]
        {
            let res = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
            if res == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error().to_string())
            }
        }
        #[cfg(not(unix))]
        {
            Err("Non-unix OS not supported for signal sending".to_string())
        }
    }

    fn is_alive(&self, pid: u32) -> bool {
        #[cfg(unix)]
        {
            let res = unsafe { libc::kill(pid as libc::pid_t, 0) };
            if res == 0 {
                return true;
            }
            let err = std::io::Error::last_os_error().raw_os_error();
            err == Some(libc::EPERM)
        }
        #[cfg(not(unix))]
        {
            false
        }
    }
}

/// Parses the process state character from `/proc/<pid>/stat` content.
///
/// Linux format: `pid (comm) state ...`
/// Because `comm` may contain spaces and parentheses, the state character
/// is the first non-whitespace character after the *last* `)` in the stat content.
pub fn parse_stat_state(stat_content: &str) -> Option<char> {
    let last_paren = stat_content.rfind(')')?;
    let rest = stat_content[last_paren + 1..].trim_start();
    rest.chars().next()
}

/// Returns true if the `/proc/<pid>/stat` content represents a zombie process (`'Z'`).
pub fn is_stat_zombie(stat_content: &str) -> bool {
    matches!(parse_stat_state(stat_content), Some('Z'))
}

/// Returns true if `/proc/<pid>/comm` under `proc_root` matches `comm` exactly,
/// the process directory is owned by `uid` (if specified), and the process is not a zombie.
pub fn pid_matches_uid(proc_root: &Path, pid: u32, comm: &str, uid: Option<u32>) -> bool {
    let pid_dir = proc_root.join(pid.to_string());
    if !pid_dir.is_dir() {
        return false;
    }

    #[cfg(unix)]
    if let Some(target_uid) = uid {
        if let Ok(meta) = fs::metadata(&pid_dir) {
            if meta.uid() != target_uid {
                return false;
            }
        } else {
            return false;
        }
    }

    // 1. Exact match on comm
    let comm_path = pid_dir.join("comm");
    let actual_comm = match fs::read_to_string(&comm_path) {
        Ok(c) => c.trim().to_string(),
        Err(_) => return false,
    };
    if actual_comm != comm {
        return false;
    }

    // 2. Skip zombies
    let stat_path = pid_dir.join("stat");
    if let Ok(stat_content) = fs::read_to_string(&stat_path) {
        if is_stat_zombie(&stat_content) {
            return false;
        }
    }

    true
}

/// Checks if a PID read from a PID file matches `comm` exactly, belongs to the current user,
/// and is not a zombie.
pub fn pid_matches(proc_root: &Path, pid: u32, comm: &str) -> bool {
    #[cfg(unix)]
    let euid = unsafe { libc::geteuid() };
    #[cfg(not(unix))]
    let euid = 0;

    pid_matches_uid(proc_root, pid, comm, Some(euid))
}

/// Sends `SIGTERM` to the given PIDs, polls up to `grace` duration for exit,
/// and returns any surviving PIDs. **Never sends SIGKILL.**
pub fn terminate(pids: &[u32], grace: Duration, sink: &dyn SignalSink) -> Vec<u32> {
    if pids.is_empty() {
        return Vec::new();
    }

    for &pid in pids {
        let _ = sink.send_term(pid);
    }

    let start = Instant::now();
    let poll_interval = Duration::from_millis(50);

    while start.elapsed() < grace {
        let any_alive = pids.iter().any(|&pid| sink.is_alive(pid));
        if !any_alive {
            return Vec::new();
        }
        let remaining = grace.saturating_sub(start.elapsed());
        std::thread::sleep(poll_interval.min(remaining));
    }

    pids.iter().copied().filter(|&pid| sink.is_alive(pid)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    pub struct RecordingSignalSink {
        pub term_signals: Arc<Mutex<Vec<u32>>>,
        pub alive_pids: Arc<Mutex<Vec<u32>>>,
        pub poll_count: Arc<AtomicUsize>,
    }

    impl RecordingSignalSink {
        pub fn new(initial_alive: Vec<u32>) -> Self {
            Self {
                term_signals: Arc::new(Mutex::new(Vec::new())),
                alive_pids: Arc::new(Mutex::new(initial_alive)),
                poll_count: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    impl SignalSink for RecordingSignalSink {
        fn send_term(&self, pid: u32) -> Result<(), String> {
            self.term_signals.lock().unwrap().push(pid);
            Ok(())
        }

        fn is_alive(&self, pid: u32) -> bool {
            self.poll_count.fetch_add(1, Ordering::SeqCst);
            self.alive_pids.lock().unwrap().contains(&pid)
        }
    }

    fn create_fake_proc_entry(
        proc_dir: &Path,
        pid: u32,
        comm: &str,
        stat_state: char,
    ) {
        let pid_dir = proc_dir.join(pid.to_string());
        fs::create_dir_all(&pid_dir).unwrap();
        fs::write(pid_dir.join("comm"), format!("{}\n", comm)).unwrap();
        fs::write(
            pid_dir.join("stat"),
            format!("{} ({}) {} 1 1 0 0 0\n", pid, comm, stat_state),
        )
        .unwrap();
    }

    #[test]
    fn test_parse_stat_state_with_spaces_and_parentheses() {
        // Comm containing parentheses and spaces
        let stat = "1234 (my (app) ) S 1 1 0 0 0 0";
        assert_eq!(parse_stat_state(stat), Some('S'));
        assert!(!is_stat_zombie(stat));

        let zombie_stat = "5678 (python3 (worker)) Z 1 1 0 0 0 0";
        assert_eq!(parse_stat_state(zombie_stat), Some('Z'));
        assert!(is_stat_zombie(zombie_stat));
    }

    #[test]
    fn test_pid_matches_logic() {
        let temp_dir = std::env::temp_dir().join(format!("test_pid_matches_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        #[cfg(unix)]
        let my_uid = unsafe { libc::geteuid() };
        #[cfg(not(unix))]
        let my_uid = 1000;

        create_fake_proc_entry(&temp_dir, 500, "speechyyy", 'S');
        create_fake_proc_entry(&temp_dir, 501, "speechyyy-dock", 'S');
        create_fake_proc_entry(&temp_dir, 502, "bash", 'S');

        assert!(pid_matches_uid(&temp_dir, 500, "speechyyy", Some(my_uid)));
        assert!(!pid_matches_uid(&temp_dir, 500, "speechyyy-dock", Some(my_uid)));
        assert!(pid_matches_uid(&temp_dir, 501, "speechyyy-dock", Some(my_uid)));
        assert!(!pid_matches_uid(&temp_dir, 502, "speechyyy", Some(my_uid)));

        // Non-existent PID
        assert!(!pid_matches_uid(&temp_dir, 999, "speechyyy", Some(my_uid)));

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_terminate_sends_only_sigterm_and_returns_survivors() {
        let sink = RecordingSignalSink::new(vec![200, 201]);

        // Terminate 200 and 201 with 100ms grace, but 201 survives
        let pids = vec![200, 201];
        // After 20ms, simulate 200 exiting
        let sink_alive_ref = Arc::clone(&sink.alive_pids);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            sink_alive_ref.lock().unwrap().retain(|&p| p != 200);
        });

        let survivors = terminate(&pids, Duration::from_millis(150), &sink);

        // 200 exited, 201 survived
        assert_eq!(survivors, vec![201]);

        // Exactly SIGTERM was sent to 200 and 201
        let terms = sink.term_signals.lock().unwrap().clone();
        assert_eq!(terms, vec![200, 201]);
    }
}

