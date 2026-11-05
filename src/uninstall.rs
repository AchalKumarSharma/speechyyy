use crate::config::Config;
use crate::daemon::stop_daemon;
use crate::ipc::{PidFile, RuntimePaths};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub fn handle_uninstall_command(yes: bool) -> Result<(), String> {
    if !yes {
        print!("Are you sure you want to completely uninstall Speechyyy? (y/N): ");
        io::stdout().flush().map_err(|e| e.to_string())?;
        let mut input = String::new();
        io::stdin().read_line(&mut input).map_err(|e| e.to_string())?;
        let trimmed = input.trim();
        if !trimmed.eq_ignore_ascii_case("y") {
            println!("Uninstallation aborted.");
            return Ok(());
        }
    }

    let paths = RuntimePaths::new();
    handle_uninstall_command_internal(&paths, None)?;
    std::process::exit(0);
}

pub fn handle_uninstall_command_internal(
    paths: &RuntimePaths,
    home_dir: Option<&Path>,
) -> Result<(), String> {
    let pid = PidFile::read_pid(&paths.pid_path);
    let is_running = paths.socket_path.exists() || pid.is_some_and(PidFile::is_process_running);
    if is_running {
        println!("Stopping running Speechyyy...");
        let _ = stop_daemon(paths);
    }

    let home = match home_dir {
        Some(h) => h.to_path_buf(),
        None => {
            let h = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(h)
        }
    };

    // Load user nickname before directory deletion
    let config_dir = home.join(".config").join("speechyyy");
    let nickname = Config::load_from_path(&config_dir.join("config.toml"))
        .nickname()
        .to_string();

    let mut candidate_bin_dirs = Vec::new();

    // 1. Remove installed binary
    if let Some(bin_path) = paths.binary_path.as_ref().or(paths.binary_path().as_ref()) {
        let bin_str = bin_path.to_string_lossy();
        if !bin_str.contains("/target/debug/deps/") && !bin_str.contains("/target/release/deps/") {
            if bin_path.exists() {
                let _ = fs::remove_file(bin_path);
            }
            if let Some(parent) = bin_path.parent() {
                candidate_bin_dirs.push(parent.to_path_buf());
            }
        }
    }

    let cargo_bin = home.join(".cargo").join("bin").join("speechyyy");
    if cargo_bin.exists() {
        let _ = fs::remove_file(&cargo_bin);
    }
    candidate_bin_dirs.push(home.join(".cargo").join("bin"));

    let local_bin = home.join(".local").join("bin").join("speechyyy");
    if local_bin.exists() {
        let _ = fs::remove_file(&local_bin);
    }
    candidate_bin_dirs.push(home.join(".local").join("bin"));

    // 2. Remove companion runtime libraries (libsherpa-onnx*, libonnxruntime*) in binary directories
    for dir in &candidate_bin_dirs {
        if dir.exists() {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                            if file_name.starts_with("libsherpa-onnx")
                                || file_name.starts_with("libonnxruntime")
                            {
                                let _ = fs::remove_file(&path);
                            }
                        }
                    }
                }
            }
        }
    }

    // 3. Recursively remove ~/.config/speechyyy and ~/.local/share/speechyyy
    if config_dir.exists() {
        let _ = fs::remove_dir_all(&config_dir);
    }

    let share_dir = home.join(".local").join("share").join("speechyyy");
    if share_dir.exists() {
        let _ = fs::remove_dir_all(&share_dir);
    }

    if home_dir.is_none() {
        if let Ok(xdg_config) = std::env::var("XDG_CONFIG_HOME") {
            let xdg_config_path = PathBuf::from(xdg_config).join("speechyyy");
            if xdg_config_path.exists() {
                let _ = fs::remove_dir_all(&xdg_config_path);
            }
        }
        if let Ok(xdg_data) = std::env::var("XDG_DATA_HOME") {
            let xdg_data_path = PathBuf::from(xdg_data).join("speechyyy");
            if xdg_data_path.exists() {
                let _ = fs::remove_dir_all(&xdg_data_path);
            }
        }
    }

    // 4. Clean up any active IPC socket file at /run/user/<uid>/speechyyy/speechyyy.sock
    paths.cleanup();
    if paths.dir.exists() {
        let _ = fs::remove_dir(&paths.dir);
    }

    #[cfg(unix)]
    if home_dir.is_none() {
        let uid = unsafe { libc::getuid() };
        let run_sock = PathBuf::from(format!("/run/user/{}/speechyyy/speechyyy.sock", uid));
        if run_sock.exists() {
            let _ = fs::remove_file(&run_sock);
        }
    }

    println!(
        "[✓] Cleaned up every last file for you, {}. Going to miss you!",
        nickname
    );
    Ok(())
}

