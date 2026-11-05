use crate::ipc::{PidFile, RuntimePaths};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const GITHUB_RELEASES_URL: &str =
    "https://api.github.com/repos/achalkumarsharma/speechyyy/releases/latest";

/// Fetches latest release metadata from GitHub
pub fn fetch_latest_release(url: &str) -> Result<serde_json::Value, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .build();

    let user_agent = format!("speechyyy-updater/{}", env!("CARGO_PKG_VERSION"));
    let resp = agent
        .get(url)
        .set("User-Agent", &user_agent)
        .call()
        .map_err(|e| format!("Failed to check for updates: {}", e))?;

    resp.into_json::<serde_json::Value>()
        .map_err(|e| format!("Failed to parse release metadata: {}", e))
}

fn find_binary(dir: &Path) -> Option<PathBuf> {
    let direct = dir.join("speechyyy");
    if direct.is_file() {
        return Some(direct);
    }
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.file_name().is_some_and(|f| f == "speechyyy") {
                return Some(path);
            } else if path.is_dir() {
                if let Some(found) = find_binary(&path) {
                    return Some(found);
                }
            }
        }
    }
    None
}

fn copy_companion_libs(src_dir: &Path, dest_dir: &Path) {
    if let Ok(entries) = fs::read_dir(src_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with("lib") && name.contains(".so") {
                        let dest = dest_dir.join(name);
                        let _ = fs::copy(&path, &dest);
                    }
                }
            } else if path.is_dir() {
                copy_companion_libs(&path, dest_dir);
            }
        }
    }
}

pub fn unpack_tarball(tarball_path: &Path, dest_dir: &Path) -> Result<(), String> {
    let file = fs::File::open(tarball_path)
        .map_err(|e| format!("Failed to open downloaded archive: {}", e))?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    archive
        .unpack(dest_dir)
        .map_err(|e| format!("Failed to unpack release archive: {}", e))
}

pub fn install_from_extracted_dir(
    extracted_dir: &Path,
    target_dir: &Path,
    paths: Option<&RuntimePaths>,
    tag: &str,
) -> Result<(), String> {
    let binary_src = find_binary(extracted_dir)
        .ok_or_else(|| "Could not locate speechyyy binary in release archive".to_string())?;

    fs::create_dir_all(target_dir)
        .map_err(|e| format!("Failed to create target directory {}: {}", target_dir.display(), e))?;

    // Prevent ETXTBSY by writing to a temporary file and atomically renaming
    let tmp_bin = target_dir.join("speechyyy.tmp");
    fs::copy(&binary_src, &tmp_bin)
        .map_err(|e| format!("Failed to stage binary to {}: {}", tmp_bin.display(), e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp_bin, fs::Permissions::from_mode(0o755));
    }

    let final_bin = target_dir.join("speechyyy");
    fs::rename(&tmp_bin, &final_bin)
        .map_err(|e| format!("Failed to install binary to {}: {}", final_bin.display(), e))?;

    // Copy companion dynamic libraries (*.so)
    copy_companion_libs(extracted_dir, target_dir);

    // If daemon was running prior to update, restart it
    if let Some(p) = paths {
        let was_running = PidFile::read_pid(&p.pid_path)
            .is_some_and(PidFile::is_process_running);
        if was_running {
            println!("Restarting Speechyyy daemon with updated binary...");
            let new_pid = crate::daemon::restart_daemon_process(p, Some(&final_bin))?;
            println!("Speechyyy daemon restarted (PID: {}).", new_pid);
        }
    }

    println!("[✓] Successfully updated Speechyyy to {}.", tag);
    Ok(())
}

pub fn handle_update_command(force: bool) -> Result<(), String> {
    let current_version = env!("CARGO_PKG_VERSION");
    let release_info = fetch_latest_release(GITHUB_RELEASES_URL)?;

    let tag_name = release_info["tag_name"]
        .as_str()
        .ok_or_else(|| "Missing tag_name in release metadata".to_string())?;

    let latest_version = tag_name.trim_start_matches('v');

    if latest_version == current_version && !force {
        println!(
            "Speechyyy is already up to date ({}, release {}).",
            current_version, tag_name
        );
        return Ok(());
    }

    if std::env::consts::ARCH != "x86_64" {
        return Err(format!(
            "Auto-update is currently only supported on x86_64 Linux (detected: {}).",
            std::env::consts::ARCH
        ));
    }

    // Determine download URL
    let asset_url = release_info["assets"].as_array().and_then(|assets| {
        assets.iter().find_map(|a| {
            let name = a["name"].as_str()?;
            if name.contains("x86_64-unknown-linux-gnu.tar.gz") {
                a["browser_download_url"].as_str().map(|s| s.to_string())
            } else {
                None
            }
        })
    });

    let download_url = asset_url.unwrap_or_else(|| {
        format!(
            "https://github.com/achalkumarsharma/speechyyy/releases/download/{}/speechyyy-{}-x86_64-unknown-linux-gnu.tar.gz",
            tag_name, tag_name
        )
    });

    let temp_dir = std::env::temp_dir().join(format!("speechyyy_update_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir)
        .map_err(|e| format!("Failed to create temporary directory: {}", e))?;

    let tarball_path = temp_dir.join("speechyyy.tar.gz");
    println!("==> Downloading pre-built release {}...", tag_name);

    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(60))
        .build();

    let user_agent = format!("speechyyy-updater/{}", current_version);
    let mut reader = agent
        .get(&download_url)
        .set("User-Agent", &user_agent)
        .call()
        .map_err(|e| format!("Failed to download release from {}: {}", download_url, e))?
        .into_reader();

    let mut out_file = fs::File::create(&tarball_path)
        .map_err(|e| format!("Failed to create download file: {}", e))?;
    std::io::copy(&mut reader, &mut out_file)
        .map_err(|e| format!("Failed to save release download: {}", e))?;

    println!("==> Extracting release archive...");
    unpack_tarball(&tarball_path, &temp_dir)?;

    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let target_dir = PathBuf::from(home).join(".local").join("bin");
    let paths = RuntimePaths::new();

    let install_result =
        install_from_extracted_dir(&temp_dir, &target_dir, Some(&paths), tag_name);

    let _ = fs::remove_dir_all(&temp_dir);
    install_result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_etxtbsy_safe_binary_installation_and_shared_libs() {
        let temp_dir =
            std::env::temp_dir().join(format!("test_update_flow_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);

        let extracted_dir = temp_dir.join("extracted");
        let target_dir = temp_dir.join("target_bin");
        fs::create_dir_all(&extracted_dir).unwrap();
        fs::create_dir_all(&target_dir).unwrap();

        // Create mock extracted files
        fs::write(extracted_dir.join("speechyyy"), b"#!/bin/sh\necho updated\n").unwrap();
        fs::write(extracted_dir.join("libsherpa-onnx-c-api.so"), b"dummy sherpa so").unwrap();
        fs::write(extracted_dir.join("libonnxruntime.so"), b"dummy onnx so").unwrap();
        fs::write(extracted_dir.join("README.md"), b"docs").unwrap();

        // Target directory has an existing "speechyyy" binary
        let existing_bin = target_dir.join("speechyyy");
        fs::write(&existing_bin, b"#!/bin/sh\necho old\n").unwrap();

        let res = install_from_extracted_dir(&extracted_dir, &target_dir, None, "v0.1.3");
        assert!(res.is_ok());

        assert!(target_dir.join("speechyyy").exists());
        assert_eq!(
            fs::read_to_string(target_dir.join("speechyyy")).unwrap(),
            "#!/bin/sh\necho updated\n"
        );
        assert!(target_dir.join("libsherpa-onnx-c-api.so").exists());
        assert!(target_dir.join("libonnxruntime.so").exists());
        assert!(!target_dir.join("README.md").exists());
        assert!(!target_dir.join("speechyyy.tmp").exists());

        let _ = fs::remove_dir_all(temp_dir);
    }
}
