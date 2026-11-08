use crate::config::Config;
use crate::daemon::stop_all;
use crate::ipc::{PidFile, RuntimePaths};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub fn is_system_bin_dir(dir: &Path) -> bool {
    let s = dir.to_string_lossy();
    s.starts_with("/usr")
        || s.starts_with("/opt")
        || s == "/bin"
        || s.starts_with("/bin/")
        || s == "/sbin"
        || s.starts_with("/sbin/")
}

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
    // 1. Call stop_all(paths) before deleting anything.
    // Do not delete binaries while a process is still running unless reported as a survivor.
    let _ = stop_all(paths);

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

    if let Some(bin_path) = paths.binary_path.as_ref().or(paths.binary_path().as_ref()) {
        if let Some(parent) = bin_path.parent() {
            candidate_bin_dirs.push(parent.to_path_buf());
        }
    }
    candidate_bin_dirs.push(home.join(".local").join("bin"));
    candidate_bin_dirs.push(home.join(".cargo").join("bin"));

    // De-duplicate candidate dirs
    candidate_bin_dirs.dedup();

    // 2. Remove speechyyy and speechyyy-dock binaries from candidate dirs
    for dir in &candidate_bin_dirs {
        let dir_str = dir.to_string_lossy();
        if dir_str.contains("/target/debug/deps/") || dir_str.contains("/target/release/deps/") {
            continue;
        }

        if is_system_bin_dir(dir) {
            println!(
                "✦ Skipping system binary directory at {} (please use your package manager to remove system binaries).",
                dir.display()
            );
            continue;
        }

        for bin_name in &["speechyyy", "speechyyy-dock"] {
            let bin = dir.join(bin_name);
            if bin.exists() {
                let _ = fs::remove_file(&bin);
            }
        }

        // Clean up companion dynamic libraries (*.so)
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

    // 3. Remove systemd user units symmetrically
    let systemd_user_dir = home.join(".config").join("systemd").join("user");
    for unit in &["speechyyy.service", "speechyyy-dock.service"] {
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "disable", "--now", unit])
            .output();
        let unit_file = systemd_user_dir.join(unit);
        if unit_file.exists() {
            let _ = fs::remove_file(&unit_file);
        }
    }
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .output();

    // 4. Compositor configuration guidance: Never modify compositor config files directly
    let hypr_conf = home.join(".config").join("hypr").join("hyprland.conf");
    if hypr_conf.is_file() {
        if let Ok(content) = fs::read_to_string(&hypr_conf) {
            let matching: Vec<&str> = content
                .lines()
                .map(|l| l.trim())
                .filter(|l| {
                    l.starts_with("exec-once = speechyyy")
                        || l.starts_with("exec-once=speechyyy")
                        || (l.contains("speechyyy") && l.contains("toggle"))
                })
                .collect();
            if !matching.is_empty() {
                println!(
                    "[i] Note for Hyprland users: Please manually remove your Speechyyy autostarts/binds from {}:",
                    hypr_conf.display()
                );
                for line in matching {
                    println!("    {}", line);
                }
            }
        }
    }

    // 5. Recursively remove ~/.config/speechyyy and ~/.local/share/speechyyy
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

    // 6. Clean up active IPC sockets and PID files (including dock pid)
    paths.cleanup();
    PidFile::remove(&paths.dock_pid_path);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_system_bin_dir() {
        assert!(is_system_bin_dir(Path::new("/usr/bin")));
        assert!(is_system_bin_dir(Path::new("/usr/local/bin")));
        assert!(is_system_bin_dir(Path::new("/opt/speechyyy/bin")));
        assert!(is_system_bin_dir(Path::new("/bin")));
        assert!(is_system_bin_dir(Path::new("/sbin")));
        assert!(!is_system_bin_dir(Path::new("/home/user/.local/bin")));
        assert!(!is_system_bin_dir(Path::new("/home/user/.cargo/bin")));
    }

    #[test]
    fn test_uninstall_removes_binaries_units_and_preserves_hyprland_conf() {
        let temp_dir = std::env::temp_dir()
            .join(format!("test_uninstall_flow_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let fake_home = temp_dir.join("home");
        let local_bin = fake_home.join(".local").join("bin");
        let cargo_bin = fake_home.join(".cargo").join("bin");
        let systemd_dir = fake_home.join(".config").join("systemd").join("user");
        let hypr_dir = fake_home.join(".config").join("hypr");
        let app_config = fake_home.join(".config").join("speechyyy");
        let app_data = fake_home.join(".local").join("share").join("speechyyy");
        let runtime_dir = temp_dir.join("run");

        fs::create_dir_all(&local_bin).unwrap();
        fs::create_dir_all(&cargo_bin).unwrap();
        fs::create_dir_all(&systemd_dir).unwrap();
        fs::create_dir_all(&hypr_dir).unwrap();
        fs::create_dir_all(&app_config).unwrap();
        fs::create_dir_all(&app_data).unwrap();
        fs::create_dir_all(&runtime_dir).unwrap();

        // Binaries in .local/bin
        fs::write(local_bin.join("speechyyy"), b"bin1").unwrap();
        fs::write(local_bin.join("speechyyy-dock"), b"dock1").unwrap();

        // Binaries in .cargo/bin
        fs::write(cargo_bin.join("speechyyy"), b"bin2").unwrap();
        fs::write(cargo_bin.join("speechyyy-dock"), b"dock2").unwrap();

        // Systemd units
        let daemon_unit = systemd_dir.join("speechyyy.service");
        let dock_unit = systemd_dir.join("speechyyy-dock.service");
        fs::write(&daemon_unit, b"[Unit]\nDescription=speechyyy").unwrap();
        fs::write(&dock_unit, b"[Unit]\nDescription=speechyyy-dock").unwrap();

        // hyprland.conf with binds and autostart
        let hypr_conf = hypr_dir.join("hyprland.conf");
        let sample_hypr = "exec-once = waybar\nexec-once = speechyyy start\nbind = , RightAlt, exec, speechyyy toggle\n";
        fs::write(&hypr_conf, sample_hypr).unwrap();

        // Runtime paths
        let paths = RuntimePaths::with_dir(runtime_dir);
        fs::write(&paths.dock_pid_path, "12345").unwrap();

        // Run uninstall
        let res = handle_uninstall_command_internal(&paths, Some(&fake_home));
        assert!(res.is_ok());

        // Binaries removed
        assert!(!local_bin.join("speechyyy").exists());
        assert!(!local_bin.join("speechyyy-dock").exists());
        assert!(!cargo_bin.join("speechyyy").exists());
        assert!(!cargo_bin.join("speechyyy-dock").exists());

        // Systemd units removed
        assert!(!daemon_unit.exists());
        assert!(!dock_unit.exists());

        // Dock PID file removed
        assert!(!paths.dock_pid_path.exists());

        // Config & share removed
        assert!(!app_config.exists());
        assert!(!app_data.exists());

        // hyprland.conf MUST NOT be modified!
        assert!(hypr_conf.exists());
        assert_eq!(fs::read_to_string(&hypr_conf).unwrap(), sample_hypr);

        let _ = fs::remove_dir_all(temp_dir);
    }
}

