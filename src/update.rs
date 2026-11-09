use crate::ipc::{PidFile, RuntimePaths};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const GITHUB_RELEASES_URL: &str =
    "https://api.github.com/repos/achalkumarsharma/speechyyy/releases/latest";

pub const MAX_SHA256SUMS_BYTES: u64 = 256 * 1024;
pub const MAX_UPDATE_ARCHIVE_BYTES: u64 = 150 * 1024 * 1024;

/// Parses a SHA256SUMS file content for the expected target filename.
/// Enforces strict validation: 64-hex chars, matches target filename, rejects duplicates/malformed.
pub fn parse_sha256sums(content: &str, target_filename: &str) -> Result<String, String> {
    let mut matched_hash: Option<String> = None;
    let target_clean = target_filename.trim_start_matches("./");

    for (line_no, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let mut parts = trimmed.split_whitespace();
        let hash = parts.next().ok_or_else(|| {
            format!("Line {}: missing hash in SHA256SUMS", line_no + 1)
        })?;
        let filename = parts.next().ok_or_else(|| {
            format!("Line {}: missing filename in SHA256SUMS", line_no + 1)
        })?;

        if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(format!(
                "Line {}: invalid SHA-256 hash '{}' (expected 64 hexadecimal characters)",
                line_no + 1,
                hash
            ));
        }

        let clean_fname = filename.trim_start_matches('*').trim_start_matches("./");
        if clean_fname == target_clean {
            if matched_hash.is_some() {
                return Err(format!(
                    "Duplicate entry for '{}' found in SHA256SUMS on line {}",
                    target_filename,
                    line_no + 1
                ));
            }
            matched_hash = Some(hash.to_ascii_lowercase());
        }
    }

    matched_hash.ok_or_else(|| {
        format!(
            "Target file '{}' not found in SHA256SUMS",
            target_filename
        )
    })
}

pub fn download_and_verify_release<P>(
    archive_url: &str,
    sha256sums_url: &str,
    archive_filename: &str,
    staged_archive_path: &Path,
    staged_sha_path: &Path,
    policy: P,
) -> Result<String, String>
where
    P: Fn(&str) -> Result<(), String>,
{
    let fetcher = crate::model::UreqFetcher;

    // 1. Fetch SHA256SUMS
    let fetch_sha_res = fetcher.fetch_with_policy(
        sha256sums_url,
        staged_sha_path,
        MAX_SHA256SUMS_BYTES,
        None,
        &policy,
    );
    if let Err(e) = fetch_sha_res {
        let _ = fs::remove_file(staged_sha_path);
        return Err(format!("Failed to download SHA256SUMS: {}", e));
    }

    // 2. Parse SHA256SUMS for the target archive
    let sha_content = match fs::read_to_string(staged_sha_path) {
        Ok(c) => c,
        Err(e) => {
            let _ = fs::remove_file(staged_sha_path);
            return Err(format!("Failed to read staged SHA256SUMS file: {}", e));
        }
    };
    let expected_hash = match parse_sha256sums(&sha_content, archive_filename) {
        Ok(h) => h,
        Err(e) => {
            let _ = fs::remove_file(staged_sha_path);
            return Err(format!("Failed to parse SHA256SUMS: {}", e));
        }
    };

    // 3. Fetch archive tarball (fetch_with_policy hashes and enforces MAX_UPDATE_ARCHIVE_BYTES)
    let fetch_archive_res = fetcher.fetch_with_policy(
        archive_url,
        staged_archive_path,
        MAX_UPDATE_ARCHIVE_BYTES,
        None,
        &policy,
    );
    let actual_hash = match fetch_archive_res {
        Ok(h) => h,
        Err(e) => {
            let _ = fs::remove_file(staged_archive_path);
            let _ = fs::remove_file(staged_sha_path);
            return Err(format!("Failed to download release archive: {}", e));
        }
    };

    // 4. Verify checksum
    if !actual_hash.eq_ignore_ascii_case(&expected_hash) {
        let _ = fs::remove_file(staged_archive_path);
        let _ = fs::remove_file(staged_sha_path);
        return Err(format!(
            "SHA-256 verification failed for '{}': expected {}, computed {}",
            archive_filename, expected_hash, actual_hash
        ));
    }

    let _ = fs::remove_file(staged_sha_path);
    Ok(actual_hash)
}

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

pub fn find_companion_binary(dir: &Path, name: &str) -> Option<PathBuf> {
    let direct = dir.join(name);
    if direct.is_file() {
        return Some(direct);
    }
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.file_name().is_some_and(|f| f == name) {
                return Some(path);
            } else if path.is_dir() {
                if let Some(found) = find_companion_binary(&path, name) {
                    return Some(found);
                }
            }
        }
    }
    None
}

pub fn find_binary(dir: &Path) -> Option<PathBuf> {
    find_companion_binary(dir, "speechyyy")
}

fn copy_companion_libs(src_dir: &Path, dest_dir: &Path) {
    if let Ok(entries) = fs::read_dir(src_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with("lib") && name.contains(".so") {
                        let dest = dest_dir.join(name);
                        let tmp_dest = dest_dir.join(format!("{}.tmp", name));
                        let _ = fs::remove_file(&tmp_dest);
                        if fs::copy(&path, &tmp_dest).is_ok() {
                            #[cfg(unix)]
                            {
                                use std::os::unix::fs::PermissionsExt;
                                let _ = fs::set_permissions(
                                    &tmp_dest,
                                    fs::Permissions::from_mode(0o755),
                                );
                            }
                            let _ = fs::remove_file(&dest);
                            let _ = fs::rename(&tmp_dest, &dest);
                        }
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

fn stage_binary(src: &Path, tmp_path: &Path) -> Result<(), String> {
    if tmp_path.exists() {
        let _ = fs::remove_file(tmp_path);
    }

    #[cfg(unix)]
    let options = {
        use std::os::unix::fs::OpenOptionsExt;
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true).mode(0o755);
        opts
    };
    #[cfg(not(unix))]
    let mut options = {
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        opts
    };

    let mut reader = fs::File::open(src)
        .map_err(|e| format!("Failed to read source binary {}: {}", src.display(), e))?;
    let mut writer = options
        .open(tmp_path)
        .map_err(|e| format!("Failed to create staged binary {}: {}", tmp_path.display(), e))?;

    std::io::copy(&mut reader, &mut writer)
        .map_err(|e| format!("Failed to stage binary to {}: {}", tmp_path.display(), e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(tmp_path, fs::Permissions::from_mode(0o755));
    }

    writer
        .sync_all()
        .map_err(|e| format!("Failed to sync staged binary {}: {}", tmp_path.display(), e))?;

    Ok(())
}

fn validate_elf_magic(path: &Path) -> Result<(), String> {
    use std::io::Read;
    let mut file = fs::File::open(path)
        .map_err(|e| format!("Failed to open staged file {} for validation: {}", path.display(), e))?;
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)
        .map_err(|e| format!("Failed to read magic header from {}: {}", path.display(), e))?;
    if &magic != b"\x7fELF" {
        return Err(format!(
            "Binary at {} is not a valid ELF binary (expected \\x7fELF, found {:?})",
            path.display(),
            magic
        ));
    }
    Ok(())
}

fn validate_binary_execution(path: &Path) -> Result<(), String> {
    let status = std::process::Command::new(path)
        .arg("--version")
        .status()
        .map_err(|e| format!("Failed to execute staged binary {}: {}", path.display(), e))?;

    if !status.success() {
        return Err(format!(
            "Staged binary {} failed version check with status: {}",
            path.display(),
            status
        ));
    }
    Ok(())
}

fn backup_existing(target: &Path, bak: &Path) -> Result<bool, String> {
    if !target.is_file() {
        return Ok(false);
    }
    let _ = fs::remove_file(bak);
    if fs::hard_link(target, bak).is_err() {
        fs::copy(target, bak)
            .map_err(|e| format!("Failed to create backup {}: {}", bak.display(), e))?;
    }
    Ok(true)
}

fn rollback_binary(target: &Path, bak: &Path, had_existing: bool) {
    if had_existing && bak.exists() {
        let _ = fs::rename(bak, target);
    } else if !had_existing {
        let _ = fs::remove_file(target);
    }
}

pub fn install_from_extracted_dir(
    extracted_dir: &Path,
    target_dir: &Path,
    paths: Option<&RuntimePaths>,
    tag: &str,
) -> Result<(), String> {
    let speechyyy_src = find_companion_binary(extracted_dir, "speechyyy")
        .ok_or_else(|| "Could not locate speechyyy binary in release archive".to_string())?;
    let dock_src = find_companion_binary(extracted_dir, "speechyyy-dock");

    fs::create_dir_all(target_dir)
        .map_err(|e| format!("Failed to create target directory {}: {}", target_dir.display(), e))?;

    let tmp_speechyyy = target_dir.join("speechyyy.tmp");
    let tmp_dock = dock_src.as_ref().map(|_| target_dir.join("speechyyy-dock.tmp"));

    let cleanup_staged = || {
        let _ = fs::remove_file(&tmp_speechyyy);
        if let Some(ref d) = tmp_dock {
            let _ = fs::remove_file(d);
        }
    };

    // Stage speechyyy
    if let Err(e) = stage_binary(&speechyyy_src, &tmp_speechyyy) {
        cleanup_staged();
        return Err(e);
    }

    // Stage dock if present
    if let (Some(ref d_src), Some(ref d_tmp)) = (&dock_src, &tmp_dock) {
        if let Err(e) = stage_binary(d_src, d_tmp) {
            cleanup_staged();
            return Err(e);
        }
    }

    // Validate ELF magic
    if let Err(e) = validate_elf_magic(&tmp_speechyyy) {
        cleanup_staged();
        return Err(e);
    }
    if let Some(ref d_tmp) = tmp_dock {
        if let Err(e) = validate_elf_magic(d_tmp) {
            cleanup_staged();
            return Err(e);
        }
    }

    // Validate execution for speechyyy (do NOT execute dock)
    if let Err(e) = validate_binary_execution(&tmp_speechyyy) {
        cleanup_staged();
        return Err(e);
    }

    // Check if dock is missing from archive
    let target_dock = target_dir.join("speechyyy-dock");
    if dock_src.is_none() {
        if target_dock.exists() {
            println!(
                "[!] Companion dock not found in release archive; retaining existing dock. Note: version differences between speechyyy and speechyyy-dock may cause unexpected behavior."
            );
        } else {
            println!("[!] Companion dock not found in release archive; skipping dock installation.");
        }
    }

    // Check running state
    let daemon_running = paths.is_some_and(|p| {
        PidFile::read_pid(&p.pid_path).is_some_and(PidFile::is_process_running)
    });
    let dock_running = paths.is_some_and(|p| {
        PidFile::read_pid(&p.dock_pid_path).is_some_and(PidFile::is_process_running)
    });
    let was_running = daemon_running || dock_running;

    if was_running {
        if let Some(p) = paths {
            let _ = crate::daemon::stop_all(p);
        }
    }

    // Swap binaries with rollback protection (prevents ETXTBSY by staging to temporary files and atomically renaming)
    let target_speechyyy = target_dir.join("speechyyy");
    let speechyyy_bak = target_dir.join("speechyyy.bak");
    let dock_bak = target_dir.join("speechyyy-dock.bak");

    let speechyyy_had_existing = match backup_existing(&target_speechyyy, &speechyyy_bak) {
        Ok(v) => v,
        Err(e) => {
            cleanup_staged();
            let _ = fs::remove_file(&speechyyy_bak);
            return Err(e);
        }
    };
    let dock_had_existing = if tmp_dock.is_some() {
        match backup_existing(&target_dock, &dock_bak) {
            Ok(v) => v,
            Err(e) => {
                cleanup_staged();
                let _ = fs::remove_file(&speechyyy_bak);
                let _ = fs::remove_file(&dock_bak);
                return Err(e);
            }
        }
    } else {
        false
    };

    // Swap speechyyy
    if let Err(e) = fs::rename(&tmp_speechyyy, &target_speechyyy) {
        rollback_binary(&target_speechyyy, &speechyyy_bak, speechyyy_had_existing);
        cleanup_staged();
        let _ = fs::remove_file(&speechyyy_bak);
        let _ = fs::remove_file(&dock_bak);
        return Err(format!("Failed to install speechyyy binary: {}", e));
    }

    // Swap dock if staged
    if let Some(ref d_tmp) = tmp_dock {
        if let Err(e) = fs::rename(d_tmp, &target_dock) {
            rollback_binary(&target_speechyyy, &speechyyy_bak, speechyyy_had_existing);
            rollback_binary(&target_dock, &dock_bak, dock_had_existing);
            cleanup_staged();
            let _ = fs::remove_file(&speechyyy_bak);
            let _ = fs::remove_file(&dock_bak);
            return Err(format!("Failed to install speechyyy-dock binary: {}", e));
        }
    }

    // Both swaps succeeded: delete .bak files
    let _ = fs::remove_file(&speechyyy_bak);
    let _ = fs::remove_file(&dock_bak);

    // Copy companion dynamic libraries (*.so)
    copy_companion_libs(extracted_dir, target_dir);

    // Relaunch if previously running
    if was_running {
        if let Some(p) = paths {
            println!("Relaunching Speechyyy with updated binaries...");
            if let Err(e) =
                crate::daemon::start_daemon(false, None, None, None, None, false, false, p)
            {
                eprintln!(
                    "[!] Could not automatically relaunch Speechyyy: {}. Run 'speechyyy start' to launch it manually.",
                    e
                );
            } else {
                println!("Speechyyy successfully relaunched.");
            }
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
        let config = crate::config::Config::load();
        let nickname = config.nickname();
        let version_str = current_version.trim_start_matches('v');
        println!(
            "[✓] We're on the latest and greatest already, {} (v{}).",
            nickname, version_str
        );
        return Ok(());
    }

    if std::env::consts::ARCH != "x86_64" {
        return Err(format!(
            "Auto-update is currently only supported on x86_64 Linux (detected: {}).",
            std::env::consts::ARCH
        ));
    }

    // Determine download URL and SHA256SUMS URL
    let (archive_url, sha_url, archive_filename) = if let Some(assets) = release_info["assets"].as_array() {
        let mut arch_url = None;
        let mut arch_name = None;
        let mut s_url = None;

        for a in assets {
            if let Some(name) = a["name"].as_str() {
                if name == "SHA256SUMS" {
                    s_url = a["browser_download_url"].as_str().map(String::from);
                } else if name.ends_with("x86_64-unknown-linux-gnu.tar.gz") {
                    arch_name = Some(name.to_string());
                    arch_url = a["browser_download_url"].as_str().map(String::from);
                }
            }
        }
        (arch_url, s_url, arch_name)
    } else {
        (None, None, None)
    };

    let filename = archive_filename.unwrap_or_else(|| {
        format!("speechyyy-{}-x86_64-unknown-linux-gnu.tar.gz", tag_name)
    });

    let download_url = archive_url.unwrap_or_else(|| {
        format!(
            "https://github.com/achalkumarsharma/speechyyy/releases/download/{}/{}",
            tag_name, filename
        )
    });

    let sha256sums_url = sha_url.unwrap_or_else(|| {
        format!(
            "https://github.com/achalkumarsharma/speechyyy/releases/download/{}/SHA256SUMS",
            tag_name
        )
    });

    let temp_dir = std::env::temp_dir().join(format!("speechyyy_update_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir)
        .map_err(|e| format!("Failed to create temporary directory: {}", e))?;

    let tarball_path = temp_dir.join(&filename);
    let sha_path = temp_dir.join("SHA256SUMS");

    println!("==> Downloading pre-built release {} and SHA256SUMS...", tag_name);
    if let Err(e) = download_and_verify_release(
        &download_url,
        &sha256sums_url,
        &filename,
        &tarball_path,
        &sha_path,
        crate::model::validate_download_url,
    ) {
        let _ = fs::remove_dir_all(&temp_dir);
        return Err(e);
    }

    println!("==> Extracting verified release archive...");
    unpack_tarball(&tarball_path, &temp_dir)?;

    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let target_dir = PathBuf::from(home).join(".local").join("bin");
    let paths = RuntimePaths::new();

    let install_result =
        install_from_extracted_dir(&temp_dir, &target_dir, Some(&paths), tag_name);

    let _ = fs::remove_dir_all(&temp_dir);
    install_result?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find_mock_elf() -> PathBuf {
        for p in &["/usr/bin/true", "/bin/true"] {
            let path = PathBuf::from(p);
            if path.exists() {
                return path;
            }
        }
        panic!("Neither /usr/bin/true nor /bin/true found for testing");
    }

    #[test]
    fn test_update_both_binaries_staged_with_0755_and_stale_tmp_cleaned_up() {
        let temp_dir =
            std::env::temp_dir().join(format!("test_update_flow_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);

        let extracted_dir = temp_dir.join("extracted");
        let target_dir = temp_dir.join("target_bin");
        fs::create_dir_all(&extracted_dir).unwrap();
        fs::create_dir_all(&target_dir).unwrap();

        let mock_elf = find_mock_elf();

        // Create stale .tmp file in target_dir
        fs::write(target_dir.join("speechyyy.tmp"), b"stale tmp content").unwrap();

        // Extracted release has speechyyy, speechyyy-dock, and companion library
        fs::copy(&mock_elf, extracted_dir.join("speechyyy")).unwrap();
        fs::copy(&mock_elf, extracted_dir.join("speechyyy-dock")).unwrap();
        fs::write(extracted_dir.join("libsherpa-onnx-c-api.so"), b"dummy sherpa so").unwrap();

        // Target directory has an existing speechyyy binary
        let existing_bin = target_dir.join("speechyyy");
        fs::copy(&mock_elf, &existing_bin).unwrap();

        let res = install_from_extracted_dir(&extracted_dir, &target_dir, None, "v0.2.0");
        assert!(res.is_ok(), "Expected Ok, got: {:?}", res);

        let final_speechyyy = target_dir.join("speechyyy");
        let final_dock = target_dir.join("speechyyy-dock");
        assert!(final_speechyyy.exists());
        assert!(final_dock.exists());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let sp_mode = fs::metadata(&final_speechyyy).unwrap().permissions().mode();
            let dk_mode = fs::metadata(&final_dock).unwrap().permissions().mode();
            assert_eq!(sp_mode & 0o111, 0o111);
            assert_eq!(dk_mode & 0o111, 0o111);
        }

        assert!(target_dir.join("libsherpa-onnx-c-api.so").exists());
        assert!(!target_dir.join("speechyyy.tmp").exists());
        assert!(!target_dir.join("speechyyy-dock.tmp").exists());
        assert!(!target_dir.join("speechyyy.bak").exists());
        assert!(!target_dir.join("speechyyy-dock.bak").exists());

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_update_rejects_non_elf_binary_with_nothing_replaced() {
        let temp_dir =
            std::env::temp_dir().join(format!("test_update_non_elf_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);

        let extracted_dir = temp_dir.join("extracted");
        let target_dir = temp_dir.join("target_bin");
        fs::create_dir_all(&extracted_dir).unwrap();
        fs::create_dir_all(&target_dir).unwrap();

        let mock_elf = find_mock_elf();
        let target_bin = target_dir.join("speechyyy");
        fs::copy(&mock_elf, &target_bin).unwrap();
        let original_bytes = fs::read(&target_bin).unwrap();

        // Corrupt archive binary (non-ELF, e.g. HTML error page)
        fs::write(extracted_dir.join("speechyyy"), b"<html>502 Bad Gateway</html>").unwrap();

        let res = install_from_extracted_dir(&extracted_dir, &target_dir, None, "v0.2.0");
        assert!(res.is_err(), "Expected error for non-ELF binary");
        assert!(res.unwrap_err().contains("not a valid ELF binary"));

        // Verify target binary was not replaced
        assert_eq!(fs::read(&target_bin).unwrap(), original_bytes);
        assert!(!target_dir.join("speechyyy.tmp").exists());
        assert!(!target_dir.join("speechyyy.bak").exists());

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_update_forced_failure_on_second_swap_rolls_back_first() {
        let temp_dir =
            std::env::temp_dir().join(format!("test_update_rollback_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);

        let extracted_dir = temp_dir.join("extracted");
        let target_dir = temp_dir.join("target_bin");
        fs::create_dir_all(&extracted_dir).unwrap();
        fs::create_dir_all(&target_dir).unwrap();

        let mock_elf = find_mock_elf();
        let target_speechyyy = target_dir.join("speechyyy");
        fs::copy(&mock_elf, &target_speechyyy).unwrap();
        let original_speechyyy_bytes = fs::read(&target_speechyyy).unwrap();

        // Create target speechyyy-dock as a directory to force EISDIR on rename
        fs::create_dir_all(target_dir.join("speechyyy-dock")).unwrap();

        fs::copy(&mock_elf, extracted_dir.join("speechyyy")).unwrap();
        fs::copy(&mock_elf, extracted_dir.join("speechyyy-dock")).unwrap();

        let res = install_from_extracted_dir(&extracted_dir, &target_dir, None, "v0.2.0");
        assert!(res.is_err(), "Expected failure on second swap");

        // Target speechyyy must be rolled back to original
        assert_eq!(fs::read(&target_speechyyy).unwrap(), original_speechyyy_bytes);
        assert!(!target_dir.join("speechyyy.tmp").exists());
        assert!(!target_dir.join("speechyyy-dock.tmp").exists());
        assert!(!target_dir.join("speechyyy.bak").exists());

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_update_dockless_archive_updates_daemon_with_warning() {
        let temp_dir =
            std::env::temp_dir().join(format!("test_update_dockless_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);

        let extracted_dir = temp_dir.join("extracted");
        let target_dir = temp_dir.join("target_bin");
        fs::create_dir_all(&extracted_dir).unwrap();
        fs::create_dir_all(&target_dir).unwrap();

        let mock_elf = find_mock_elf();
        let target_speechyyy = target_dir.join("speechyyy");
        let target_dock = target_dir.join("speechyyy-dock");

        // Create existing files
        fs::copy(&mock_elf, &target_speechyyy).unwrap();
        fs::write(&target_dock, b"custom old dock").unwrap();

        // Extracted dir only has speechyyy
        fs::copy(&mock_elf, extracted_dir.join("speechyyy")).unwrap();

        let res = install_from_extracted_dir(&extracted_dir, &target_dir, None, "v0.2.0");
        assert!(res.is_ok(), "Expected success for dockless update");

        // speechyyy was updated, speechyyy-dock was preserved
        assert!(target_speechyyy.exists());
        assert_eq!(fs::read(&target_dock).unwrap(), b"custom old dock");

        let _ = fs::remove_dir_all(temp_dir);
    }
}
