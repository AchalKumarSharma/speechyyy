use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum LocalModelType {
    #[value(name = "parakeet-tdt-v3", alias = "parakeet-v3")]
    ParakeetTdtV3,
    #[value(name = "whisper-small")]
    WhisperSmall,
    #[value(name = "whisper-large-v3-turbo", alias = "whisper-turbo")]
    WhisperLargeV3Turbo,
    #[value(name = "moonshine-base")]
    MoonshineBase,
}

impl LocalModelType {
    pub fn from_str_name(name: &str) -> Option<Self> {
        match name.to_lowercase().replace('_', "-").as_str() {
            "parakeet-tdt-v3" | "parakeet-v3" | "parakeet" => Some(Self::ParakeetTdtV3),
            "whisper-small" | "small" => Some(Self::WhisperSmall),
            "whisper-large-v3-turbo" | "whisper-turbo" | "turbo" => Some(Self::WhisperLargeV3Turbo),
            "moonshine-base" | "moonshine" => Some(Self::MoonshineBase),
            _ => None,
        }
    }

    pub fn folder_name(&self) -> &'static str {
        match self {
            Self::ParakeetTdtV3 => "parakeet-tdt-v3",
            Self::WhisperSmall => "whisper-small",
            Self::WhisperLargeV3Turbo => "whisper-large-v3-turbo",
            Self::MoonshineBase => "moonshine-base",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::ParakeetTdtV3 => "Parakeet-TDT-v3 (0.6B INT8)",
            Self::WhisperSmall => "Whisper-Small (INT8)",
            Self::WhisperLargeV3Turbo => "Whisper-Large-v3-Turbo (INT8)",
            Self::MoonshineBase => "Moonshine-Base (INT8)",
        }
    }

    pub fn required_file_groups(&self) -> &'static [&'static [&'static str]] {
        match self {
            Self::ParakeetTdtV3 => &[
                &["encoder.int8.onnx"],
                &["decoder.int8.onnx"],
                &["joiner.int8.onnx"],
                &["tokens.txt"],
            ],
            Self::WhisperSmall => &[
                &["small-encoder.int8.onnx", "encoder.int8.onnx"],
                &["small-decoder.int8.onnx", "decoder.int8.onnx"],
                &["small-tokens.txt", "tokens.txt"],
            ],
            Self::WhisperLargeV3Turbo => &[
                &["turbo-encoder.int8.onnx", "encoder.int8.onnx"],
                &["turbo-decoder.int8.onnx", "decoder.int8.onnx"],
                &["turbo-tokens.txt", "tokens.txt"],
            ],
            Self::MoonshineBase => &[
                &["preprocess.onnx"],
                &["encode.int8.onnx"],
                &["uncache_decode.int8.onnx", "uncached_decode.int8.onnx"],
                &["cached_decode.int8.onnx"],
                &["tokens.txt"],
            ],
        }
    }

    pub fn download_url(&self) -> &'static str {
        match self {
            Self::ParakeetTdtV3 => {
                "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2"
            }
            Self::WhisperSmall => {
                "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-whisper-small.tar.bz2"
            }
            Self::WhisperLargeV3Turbo => {
                "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-whisper-turbo.tar.bz2"
            }
            Self::MoonshineBase => {
                "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-moonshine-base-en-int8.tar.bz2"
            }
        }
    }
}

pub fn find_file(dir: &Path, candidates: &[&str]) -> Option<PathBuf> {
    for &cand in candidates {
        let p = dir.join(cand);
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                for &cand in candidates {
                    let sub_p = path.join(cand);
                    if sub_p.exists() {
                        return Some(sub_p);
                    }
                }
            }
        }
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelInfo {
    pub model_type: LocalModelType,
    pub path: PathBuf,
    pub file_count: usize,
    pub total_bytes: u64,
}

impl ModelInfo {
    pub fn total_mb(&self) -> f64 {
        self.total_bytes as f64 / (1024.0 * 1024.0)
    }
}

pub struct ModelManager;

impl ModelManager {
    pub fn default_models_dir() -> PathBuf {
        if let Ok(dir) = std::env::var("SPEECHYYY_MODELS_DIR") {
            return PathBuf::from(dir);
        }
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        let standard_dir = PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("speechyyy")
            .join("models");

        if !standard_dir.exists() && Path::new("/tmp/speechyyy_models").exists() {
            return PathBuf::from("/tmp/speechyyy_models");
        }

        standard_dir
    }

    pub fn model_dir(model_type: LocalModelType) -> PathBuf {
        Self::default_models_dir().join(model_type.folder_name())
    }

    /// Verifies that all expected ONNX and token assets for `model_type` exist in `dir`.
    pub fn verify_model_files(model_type: LocalModelType, dir: &Path) -> Result<ModelInfo, String> {
        if !dir.exists() {
            return Err(format!("Model directory {} does not exist", dir.display()));
        }

        let groups = model_type.required_file_groups();
        let mut total_bytes = 0u64;

        for group in groups {
            let found = find_file(dir, group);
            match found {
                Some(p) => {
                    let meta = fs::metadata(&p)
                        .map_err(|e| format!("Failed to inspect {}: {}", p.display(), e))?;
                    if meta.len() == 0 {
                        return Err(format!("Model file {} is 0 bytes (corrupted)", p.display()));
                    }
                    total_bytes += meta.len();
                }
                None => {
                    return Err(format!(
                        "Missing required model file (expected one of {:?}) in {}",
                        group,
                        dir.display()
                    ));
                }
            }
        }

        Ok(ModelInfo {
            model_type,
            path: dir.to_path_buf(),
            file_count: groups.len(),
            total_bytes,
        })
    }

    /// Downloads the model archive (.tar.bz2), verifies its SHA-256 and size against the pinned manifest,
    /// safely extracts it into staging with traversal guards, and atomically promotes it into `target_dir`.
    pub fn download_model(
        model_type: LocalModelType,
        target_dir: &Path,
    ) -> Result<ModelInfo, String> {
        let fetcher = UreqFetcher;
        download_and_install_model(model_type, target_dir, &fetcher, None)
    }

    /// Downloads and installs model with a custom fetcher and optional manifest override (for hermetic testing).
    pub fn download_model_with_fetcher<F: Fetcher>(
        model_type: LocalModelType,
        target_dir: &Path,
        fetcher: &F,
        manifest: Option<&ModelsManifest>,
    ) -> Result<ModelInfo, String> {
        download_and_install_model(model_type, target_dir, fetcher, manifest)
    }
}

pub const EMBEDDED_MANIFEST: &str = include_str!("../assets/models.toml");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelManifestEntry {
    pub id: String,
    pub url: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub max_extracted_bytes: u64,
    pub max_files: usize,
    pub expected_files: Vec<String>,
    pub license: String,
    pub license_url: String,
    pub attribution: String,
}

impl ModelManifestEntry {
    pub fn validate_for_download(&self) -> Result<(), String> {
        let hash = self.sha256.trim();
        if hash.is_empty()
            || hash == "unverified"
            || hash == "placeholder"
            || hash == "TODO"
            || hash.len() != 64
        {
            return Err(format!(
                "error: model '{}' does not have a pinned checksum in this build.\nhint: run 'scripts/update-model-manifest.sh' or set up a verified build.",
                self.id
            ));
        }
        if self.size_bytes == 0 {
            return Err(format!(
                "error: model '{}' has no pinned archive size in this build.\nhint: run 'scripts/update-model-manifest.sh' or set up a verified build.",
                self.id
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelsManifest {
    #[serde(default)]
    pub models: HashMap<String, ModelManifestEntry>,
}

impl ModelsManifest {
    pub fn parse(content: &str) -> Result<Self, String> {
        toml::from_str(content).map_err(|e| format!("Failed to parse models.toml: {}", e))
    }

    pub fn embedded() -> Result<Self, String> {
        Self::parse(EMBEDDED_MANIFEST)
    }

    pub fn get(&self, model_id: &str) -> Option<&ModelManifestEntry> {
        self.models.get(model_id)
    }
}

pub fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(s, "{:02x}", b);
    }
    s
}

pub fn compute_file_sha256(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| format!("Failed to open {}: {}", path.display(), e))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(to_hex(&hasher.finalize()))
}

/// Pure function validating that a model download URL conforms strictly to the URL policy:
/// - Must begin with 'https://'
/// - Host up to '/', '?', '#', or ':' must be exactly 'github.com' or end with '.githubusercontent.com' on a dot boundary
/// - Must not contain userinfo ('@') before authority ends
pub fn validate_download_url(url: &str) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("URL scheme must be exactly 'https://'".into());
    }
    let after_scheme = &url["https://".len()..];

    let authority_end = after_scheme
        .find(['/', '?', '#', ':'])
        .unwrap_or(after_scheme.len());
    let authority = &after_scheme[..authority_end];

    if authority.contains('@') {
        return Err("Userinfo ('@') is not allowed in model download URL".into());
    }

    let host = authority.to_ascii_lowercase();
    if host.is_empty() {
        return Err("Host cannot be empty".into());
    }

    let is_allowed = host == "github.com"
        || (host.ends_with(".githubusercontent.com")
            && host.len() > ".githubusercontent.com".len()
            && !host[..host.len() - ".githubusercontent.com".len()].is_empty()
            && !host[..host.len() - ".githubusercontent.com".len()].ends_with('.'));

    if !is_allowed {
        return Err(format!(
            "Host '{}' is not an allowlisted GitHub download domain",
            host
        ));
    }

    Ok(())
}

pub fn compute_available_disk_space(bavail: u64, frsize: u64) -> u64 {
    bavail.saturating_mul(frsize)
}

pub fn compute_required_disk_space(archive_size: u64, max_extracted_bytes: u64) -> u64 {
    let archive_part = (archive_size as f64 * 2.5) as u64;
    archive_part.saturating_add(max_extracted_bytes)
}

pub fn check_disk_space(target_dir: &Path, required_bytes: u64) -> Result<u64, String> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let mut check_dir = target_dir.to_path_buf();
    while !check_dir.exists() {
        if let Some(parent) = check_dir.parent() {
            check_dir = parent.to_path_buf();
        } else {
            break;
        }
    }

    let c_path = CString::new(check_dir.as_os_str().as_bytes())
        .map_err(|e| format!("Invalid path for statvfs: {}", e))?;

    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let res = unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) };
    if res != 0 {
        return Err(format!(
            "statvfs failed on {}: {}",
            check_dir.display(),
            std::io::Error::last_os_error()
        ));
    }

    let available = compute_available_disk_space(stat.f_bavail as u64, stat.f_frsize as u64);
    if available < required_bytes {
        let req_mb = required_bytes as f64 / (1024.0 * 1024.0);
        let avail_mb = available as f64 / (1024.0 * 1024.0);
        return Err(format!(
            "Insufficient disk space in {}: required {:.1} MB, available {:.1} MB",
            check_dir.display(),
            req_mb,
            avail_mb
        ));
    }

    Ok(available)
}

pub trait Fetcher {
    fn fetch(
        &self,
        url: &str,
        dest: &Path,
        max_bytes: u64,
        progress_cb: Option<&mut dyn FnMut(u64)>,
    ) -> Result<String, String>;
}

pub struct UreqFetcher;

impl Fetcher for UreqFetcher {
    fn fetch(
        &self,
        initial_url: &str,
        dest: &Path,
        max_bytes: u64,
        mut progress_cb: Option<&mut dyn FnMut(u64)>,
    ) -> Result<String, String> {
        let mut current_url = initial_url.to_string();
        let mut redirect_count = 0;

        let agent = ureq::AgentBuilder::new()
            .redirects(0)
            .timeout_connect(Duration::from_secs(10))
            .timeout_read(Duration::from_secs(30))
            .build();

        loop {
            validate_download_url(&current_url)?;

            let mut attempts = 0;
            let mut last_err = String::new();
            let mut response = None;

            while attempts < 3 {
                attempts += 1;
                match agent.get(&current_url).call() {
                    Ok(resp) => {
                        response = Some(resp);
                        break;
                    }
                    Err(ureq::Error::Status(code, resp)) => {
                        if (301..=308).contains(&code) {
                            if let Some(loc) = resp.header("location") {
                                redirect_count += 1;
                                if redirect_count > 5 {
                                    return Err(format!(
                                        "Too many redirects (exceeded 5) while fetching {}",
                                        initial_url
                                    ));
                                }
                                let next_url = if loc.starts_with("http://") || loc.starts_with("https://") {
                                    loc.to_string()
                                } else if loc.starts_with('/') {
                                    let authority_end = current_url["https://".len()..]
                                        .find('/')
                                        .map(|idx| idx + "https://".len())
                                        .unwrap_or(current_url.len());
                                    format!("{}{}", &current_url[..authority_end], loc)
                                } else {
                                    format!("{}/{}", current_url.trim_end_matches('/'), loc)
                                };
                                current_url = next_url;
                                response = None;
                                break;
                            }
                        }
                        return Err(format!(
                            "HTTP error {} fetching {}: {}",
                            code, current_url, resp.status_text()
                        ));
                    }
                    Err(ureq::Error::Transport(e)) => {
                        last_err = format!("Network failure on {}: {}", current_url, e);
                        if attempts < 3 {
                            let backoff = Duration::from_millis(500 * (1 << (attempts - 1)));
                            std::thread::sleep(backoff);
                        }
                    }
                }
            }

            if let Some(resp) = response {
                if dest.exists() {
                    let _ = fs::remove_file(dest);
                }
                if let Some(p) = dest.parent() {
                    fs::create_dir_all(p)
                        .map_err(|e| format!("Failed to create staging dir {}: {}", p.display(), e))?;
                }

                let mut dest_file = File::create(dest)
                    .map_err(|e| format!("Failed to create staging file {}: {}", dest.display(), e))?;

                let mut reader = resp.into_reader();
                let mut hasher = Sha256::new();
                let mut buf = [0u8; 65536];
                let mut total_bytes = 0u64;

                loop {
                    let n = reader
                        .read(&mut buf)
                        .map_err(|e| format!("Failed reading stream from {}: {}", current_url, e))?;
                    if n == 0 {
                        break;
                    }
                    total_bytes += n as u64;
                    if total_bytes > max_bytes {
                        drop(dest_file);
                        let _ = fs::remove_file(dest);
                        return Err(format!(
                            "Download aborted: received {} bytes, exceeding allowed maximum of {} bytes",
                            total_bytes, max_bytes
                        ));
                    }
                    hasher.update(&buf[..n]);
                    dest_file
                        .write_all(&buf[..n])
                        .map_err(|e| format!("Failed to write to {}: {}", dest.display(), e))?;

                    if let Some(ref mut cb) = progress_cb {
                        cb(total_bytes);
                    }
                }

                dest_file
                    .flush()
                    .map_err(|e| format!("Failed to flush {}: {}", dest.display(), e))?;
                let computed_hash = to_hex(&hasher.finalize());
                return Ok(computed_hash);
            }

            if attempts >= 3 && !last_err.is_empty() {
                return Err(format!("Download failed after 3 attempts: {}", last_err));
            }
        }
    }
}

pub fn clean_stale_staging_dirs(staging_root: &Path) {
    if !staging_root.exists() {
        return;
    }
    let entries = match fs::read_dir(staging_root) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let dir_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };

        let pid_opt = dir_name.rsplit('-').next().and_then(|p| p.parse::<libc::pid_t>().ok());
        let mut should_clean = false;

        if let Some(pid) = pid_opt {
            let res = unsafe { libc::kill(pid, 0) };
            if res != 0 {
                let err = std::io::Error::last_os_error();
                if err.raw_os_error() == Some(libc::ESRCH) {
                    should_clean = true;
                }
            }
        }

        if let Ok(meta) = fs::metadata(&path) {
            if let Ok(modified) = meta.modified() {
                if let Ok(elapsed) = SystemTime::now().duration_since(modified) {
                    if elapsed >= Duration::from_secs(86400) {
                        should_clean = true;
                    }
                }
            }
        }

        if should_clean {
            let _ = fs::remove_dir_all(&path);
        }
    }
}

pub fn safe_extract_tar_bz2(
    archive_path: &Path,
    dest_dir: &Path,
    max_files: usize,
    max_extracted_bytes: u64,
) -> Result<usize, String> {
    if dest_dir.exists() {
        let _ = fs::remove_dir_all(dest_dir);
    }
    fs::create_dir_all(dest_dir)
        .map_err(|e| format!("Failed to create extraction directory {}: {}", dest_dir.display(), e))?;

    let file = File::open(archive_path)
        .map_err(|e| format!("Failed to open archive {}: {}", archive_path.display(), e))?;
    let decompressor = bzip2::read::BzDecoder::new(file);
    let mut archive = tar::Archive::new(decompressor);

    let mut file_count = 0usize;
    let mut total_bytes_written = 0u64;

    let entries = archive
        .entries()
        .map_err(|e| format!("Corrupted tar archive {}: {}", archive_path.display(), e))?;

    for entry_res in entries {
        let mut entry = entry_res.map_err(|e| format!("Failed to read archive entry: {}", e))?;
        let entry_type = entry.header().entry_type();

        if !entry_type.is_file() && !entry_type.is_dir() {
            return Err(format!(
                "Archive contains forbidden entry type '{:?}'",
                entry_type
            ));
        }

        let raw_path = entry
            .path()
            .map_err(|e| format!("Invalid path in archive: {}", e))?
            .to_path_buf();

        let mut normalized_parts = Vec::new();
        for comp in raw_path.components() {
            match comp {
                Component::CurDir => continue,
                Component::RootDir | Component::Prefix(_) => {
                    return Err(format!(
                        "Archive entry '{}' contains illegal absolute path component",
                        raw_path.display()
                    ));
                }
                Component::ParentDir => {
                    return Err(format!(
                        "Archive entry '{}' contains illegal path traversal component '..'",
                        raw_path.display()
                    ));
                }
                Component::Normal(c) => {
                    normalized_parts.push(PathBuf::from(c));
                }
            }
        }

        if normalized_parts.is_empty() {
            continue;
        }

        let relative_path: PathBuf = if normalized_parts.len() > 1 {
            normalized_parts[1..].iter().collect()
        } else {
            if entry_type.is_dir() {
                continue;
            }
            normalized_parts[0].clone()
        };

        let target_path = dest_dir.join(&relative_path);

        if entry_type.is_dir() {
            fs::create_dir_all(&target_path)
                .map_err(|e| format!("Failed to create directory {}: {}", target_path.display(), e))?;
            let _ = fs::set_permissions(&target_path, fs::Permissions::from_mode(0o755));
        } else if entry_type.is_file() {
            file_count += 1;
            if file_count > max_files {
                return Err(format!(
                    "Decompression rejected: archive exceeded file count cap of {}",
                    max_files
                ));
            }

            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    format!("Failed to create parent dir {}: {}", parent.display(), e)
                })?;
                let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o755));
            }

            let mut out_file = File::create(&target_path)
                .map_err(|e| format!("Failed to create file {}: {}", target_path.display(), e))?;
            let _ = fs::set_permissions(&target_path, fs::Permissions::from_mode(0o644));

            let mut buf = [0u8; 65536];
            loop {
                let n = entry
                    .read(&mut buf)
                    .map_err(|e| format!("Decompression read error on {}: {}", relative_path.display(), e))?;
                if n == 0 {
                    break;
                }
                total_bytes_written += n as u64;
                if total_bytes_written > max_extracted_bytes {
                    drop(out_file);
                    let _ = fs::remove_file(&target_path);
                    return Err(format!(
                        "Decompression bomb aborted: extracted bytes exceeded cap of {} bytes",
                        max_extracted_bytes
                    ));
                }
                out_file
                    .write_all(&buf[..n])
                    .map_err(|e| format!("Failed writing {}: {}", target_path.display(), e))?;
            }
            out_file
                .flush()
                .map_err(|e| format!("Failed to flush {}: {}", target_path.display(), e))?;
        }
    }

    Ok(file_count)
}

pub fn recover_interrupted_install(model_dir: &Path, model_id: &str) -> Result<(), String> {
    let old_dir = PathBuf::from(format!("{}.old", model_dir.display()));
    if old_dir.exists() && !model_dir.exists() {
        fs::rename(&old_dir, model_dir).map_err(|e| {
            format!(
                "Failed to restore interrupted model backup from {}: {}",
                old_dir.display(),
                e
            )
        })?;
        println!(
            "[✓] Restored previous model '{}' from .old backup after an interrupted install.",
            model_id
        );
    } else if old_dir.exists() && model_dir.exists() {
        let _ = fs::remove_dir_all(&old_dir);
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileChecksumInfo {
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstalledModelInfo {
    pub model_id: String,
    pub source_url: String,
    pub archive_sha256: String,
    pub files: HashMap<String, FileChecksumInfo>,
    pub license: String,
    pub license_url: String,
    pub attribution: String,
    pub installed_at: String,
}

impl InstalledModelInfo {
    pub fn load(model_dir: &Path) -> Result<Self, String> {
        let path = model_dir.join("MODEL_INFO.json");
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("Corrupt {}: {}", path.display(), e))
    }
}

pub fn generate_and_save_model_info(
    manifest_entry: &ModelManifestEntry,
    archive_sha256: &str,
    model_dir: &Path,
) -> Result<(), String> {
    let mut files_map = HashMap::new();
    let entries = fs::read_dir(model_dir)
        .map_err(|e| format!("Failed to read {}: {}", model_dir.display(), e))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            if name == "MODEL_INFO.json" {
                continue;
            }
            let size = match fs::metadata(&path) {
                Ok(m) => m.len(),
                Err(e) => return Err(format!("Failed to stat {}: {}", path.display(), e)),
            };
            let hash = compute_file_sha256(&path)?;
            files_map.insert(
                name,
                FileChecksumInfo {
                    sha256: hash,
                    size_bytes: size,
                },
            );
        }
    }

    let now_ts = match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
        Ok(dur) => dur.as_secs().to_string(),
        Err(_) => "0".to_string(),
    };

    let info = InstalledModelInfo {
        model_id: manifest_entry.id.clone(),
        source_url: manifest_entry.url.clone(),
        archive_sha256: archive_sha256.to_string(),
        files: files_map,
        license: manifest_entry.license.clone(),
        license_url: manifest_entry.license_url.clone(),
        attribution: manifest_entry.attribution.clone(),
        installed_at: now_ts,
    };

    let json_content = serde_json::to_string_pretty(&info)
        .map_err(|e| format!("Failed to serialize MODEL_INFO.json: {}", e))?;
    let info_path = model_dir.join("MODEL_INFO.json");
    fs::write(&info_path, json_content)
        .map_err(|e| format!("Failed to write {}: {}", info_path.display(), e))?;

    Ok(())
}

pub fn download_and_install_model<F: Fetcher>(
    model_type: LocalModelType,
    target_dir: &Path,
    fetcher: &F,
    manifest: Option<&ModelsManifest>,
) -> Result<ModelInfo, String> {
    let embedded = ModelsManifest::embedded()?;
    let manifest = manifest.unwrap_or(&embedded);
    let entry = manifest.get(model_type.folder_name()).ok_or_else(|| {
        format!(
            "error: model '{}' does not have a pinned checksum in this build.\nhint: run 'scripts/update-model-manifest.sh' or set up a verified build.",
            model_type.folder_name()
        )
    })?;

    entry.validate_for_download()?;

    println!("Model: {}", model_type.display_name());
    println!("License: {} ({})", entry.license, entry.license_url);
    println!("Attribution: {}", entry.attribution);

    let models_root = target_dir
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let staging_root = models_root.join(".staging");
    fs::create_dir_all(&staging_root).map_err(|e| {
        format!("Failed to create staging directory {}: {}", staging_root.display(), e)
    })?;

    clean_stale_staging_dirs(&staging_root);

    let required_bytes = compute_required_disk_space(entry.size_bytes, entry.max_extracted_bytes);
    check_disk_space(target_dir, required_bytes)?;

    recover_interrupted_install(target_dir, model_type.folder_name())?;

    let pid = std::process::id();
    let attempt_staging = staging_root.join(format!("{}-{}", model_type.folder_name(), pid));
    if attempt_staging.exists() {
        let _ = fs::remove_dir_all(&attempt_staging);
    }
    fs::create_dir_all(&attempt_staging).map_err(|e| {
        format!("Failed to create staging attempt directory {}: {}", attempt_staging.display(), e)
    })?;

    let archive_path = attempt_staging.join("archive.tar.bz2");
    let extracted_dir = attempt_staging.join("extracted");

    let pb = ProgressBar::new(entry.size_bytes);
    let style = ProgressStyle::with_template(
        "[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}",
    )
    .unwrap_or_else(|_| ProgressStyle::default_bar())
    .progress_chars("##-");
    pb.set_style(style);
    pb.set_message("Downloading verified model archive...");

    let mut on_progress = |bytes: u64| {
        pb.set_position(bytes);
    };

    let fetch_res = fetcher.fetch(
        &entry.url,
        &archive_path,
        entry.size_bytes,
        Some(&mut on_progress),
    );

    let actual_sha256 = match fetch_res {
        Ok(h) => h,
        Err(e) => {
            let _ = fs::remove_dir_all(&attempt_staging);
            return Err(e);
        }
    };
    pb.finish_with_message("Download complete. Verifying SHA-256 integrity...");

    if !actual_sha256.eq_ignore_ascii_case(&entry.sha256) {
        let _ = fs::remove_dir_all(&attempt_staging);
        return Err(format!(
            "SHA-256 checksum mismatch for '{}': expected {}, got {}. Nothing was extracted.",
            entry.id, entry.sha256, actual_sha256
        ));
    }

    let archive_size = match fs::metadata(&archive_path) {
        Ok(m) => m.len(),
        Err(e) => {
            let _ = fs::remove_dir_all(&attempt_staging);
            return Err(format!("Failed to inspect staging archive {}: {}", archive_path.display(), e));
        }
    };
    if archive_size != entry.size_bytes {
        let _ = fs::remove_dir_all(&attempt_staging);
        return Err(format!(
            "Archive size mismatch for '{}': expected {} bytes, got {}. Nothing was extracted.",
            entry.id, entry.size_bytes, archive_size
        ));
    }

    println!("[✓] Checksum verified: {}", actual_sha256);
    println!("Extracting archive safely...");

    if let Err(e) = safe_extract_tar_bz2(
        &archive_path,
        &extracted_dir,
        entry.max_files,
        entry.max_extracted_bytes,
    ) {
        let _ = fs::remove_dir_all(&attempt_staging);
        return Err(format!("Safe extraction failed: {}", e));
    }

    let verified_info = match ModelManager::verify_model_files(model_type, &extracted_dir) {
        Ok(info) => info,
        Err(e) => {
            let _ = fs::remove_dir_all(&attempt_staging);
            return Err(format!("Extracted assets failed file verification: {}", e));
        }
    };

    let old_dir = PathBuf::from(format!("{}.old", target_dir.display()));
    if old_dir.exists() {
        let _ = fs::remove_dir_all(&old_dir);
    }

    let had_previous = target_dir.exists();
    if had_previous {
        fs::rename(target_dir, &old_dir).map_err(|e| {
            let _ = fs::remove_dir_all(&attempt_staging);
            format!("Failed to backup current model to {}: {}", old_dir.display(), e)
        })?;
    }

    if let Err(e) = fs::rename(&extracted_dir, target_dir) {
        if had_previous && old_dir.exists() {
            let _ = fs::rename(&old_dir, target_dir);
        }
        let _ = fs::remove_dir_all(&attempt_staging);
        return Err(format!("Failed to promote model directory: {}", e));
    }

    if old_dir.exists() {
        let _ = fs::remove_dir_all(&old_dir);
    }

    if let Err(e) = generate_and_save_model_info(entry, &actual_sha256, target_dir) {
        eprintln!("Warning: Failed to write MODEL_INFO.json: {}", e);
    }

    let _ = fs::remove_dir_all(&attempt_staging);

    let completion_msg = format!(
        "[✓] Successfully installed {} to {}",
        model_type.folder_name(),
        target_dir.display()
    );
    println!("{}", completion_msg);

    Ok(verified_info)
}

/// Checks if a model identifier refers to the Sarvam AI cloud provider.
pub fn is_sarvam_cloud_model(name: &str) -> bool {
    matches!(
        name.trim().to_lowercase().replace('_', "-").as_str(),
        "saaras:v4" | "sarvam:saaras-v4" | "sarvam" | "saaras"
    )
}

/// Checks if a model identifier refers to another cloud provider (Groq or OpenAI).
pub fn is_cloud_provider(name: &str) -> bool {
    matches!(
        name.trim().to_lowercase().replace('_', "-").as_str(),
        "groq" | "openai"
    )
}

/// Generates a descriptive error message listing all supported local and cloud models.
pub fn unsupported_model_error(model: &str) -> String {
    #[cfg(target_os = "macos")]
    let supported = "Cloud: groq, openai, openai-compatible, saaras:v4 (sarvam:saaras-v4). Local: parakeet-tdt-v3, whisper-small, whisper-large-v3-turbo, moonshine-base, apple-speech.";
    #[cfg(not(target_os = "macos"))]
    let supported = "Cloud: groq, openai, openai-compatible, saaras:v4 (sarvam:saaras-v4). Local: parakeet-tdt-v3, whisper-small, whisper-large-v3-turbo, moonshine-base.";
    format!(
        "Unsupported model '{}'. Supported models: {}",
        model, supported
    )
}

/// Handler for the CLI `model download` command.
pub fn handle_model_download_command(model_name: &str, force: bool) -> Result<(), String> {
    handle_model_download_internal(
        model_name,
        force,
        &crate::config::Config::config_path(),
        None,
        &mut || {
            use std::io::{stdin, stdout, IsTerminal, Write};
            if !stdin().is_terminal() {
                return Ok(false);
            }
            print!("Sarvam API key is already configured. Replace it? [y/N] ");
            let _ = stdout().flush();
            let mut line = String::new();
            if stdin().read_line(&mut line).is_err() {
                return Ok(false);
            }
            let trimmed = line.trim().to_lowercase();
            Ok(trimmed == "y" || trimmed == "yes")
        },
        &mut || {
            rpassword::prompt_password("Enter your Sarvam API Key: ")
                .map_err(|e| format!("Failed to read API key: {}", e))
        },
    )
}

/// Internal handler for model downloading with dependency injection for prompting and paths.
pub fn handle_model_download_internal<C, F>(
    model_name: &str,
    force: bool,
    config_path: &Path,
    models_dir: Option<&Path>,
    confirm_replace_fn: &mut C,
    prompt_key_fn: &mut F,
) -> Result<(), String>
where
    C: FnMut() -> Result<bool, String>,
    F: FnMut() -> Result<String, String>,
{
    let trimmed = model_name.trim();
    let cfg = crate::config::Config::load_from_path(config_path);
    let nickname = cfg.nickname();
    if is_sarvam_cloud_model(trimmed) {
        if let Some(key) = cfg.inference.resolve_api_key() {
            if !key.trim().is_empty() {
                let should_replace = if force {
                    true
                } else {
                    confirm_replace_fn()?
                };

                if !should_replace {
                    println!("[✓] Sarvam API key is already configured. saaras:v4 is ready to use.");
                    println!(
                        "[✓] Model 'saaras:v4' is all yours, {}! Run 'speechyyy use saaras:v4' whenever you want me to speak it.",
                        nickname
                    );
                    return Ok(());
                }
            }
        }

        let raw_key = prompt_key_fn()?;
        let key = raw_key.trim().trim_matches('"');
        if key.is_empty() {
            return Err("Sarvam API key cannot be empty.".to_string());
        }

        crate::config::Config::save_sarvam_credentials_in_file(config_path, key)?;
        println!("[✓] Sarvam API key configured successfully. saaras:v4 is ready to use.");
        println!(
            "[✓] Model 'saaras:v4' is all yours, {}! Run 'speechyyy use saaras:v4' whenever you want me to speak it.",
            nickname
        );
        Ok(())
    } else if is_cloud_provider(trimmed) {
        let (prov_name, env_var) = if trimmed.eq_ignore_ascii_case("groq") {
            ("Groq", "GROQ_API_KEY")
        } else {
            ("OpenAI", "OPENAI_API_KEY")
        };
        let key_opt = cfg.inference.resolve_key_for_provider(trimmed);
        println!(
            "[i] {} is a cloud provider — there are no weights to download onto your machine.",
            prov_name
        );
        match key_opt {
            Some((_, source)) => {
                println!("[✓] {} API key is configured (source: {}).", prov_name, source);
                println!(
                    "[✓] Run 'speechyyy use {}' whenever you want me to speak it, {}!",
                    trimmed.to_lowercase(),
                    nickname
                );
            }
            None => {
                println!("[!] No {} API key is configured.", prov_name);
                use std::io::{stdin, IsTerminal};
                if stdin().is_terminal() {
                    let raw_key = prompt_key_fn()?;
                    let key = raw_key.trim().trim_matches('"');
                    if !key.is_empty() {
                        if trimmed.eq_ignore_ascii_case("groq") {
                            crate::config::Config::save_groq_credentials_in_file(config_path, key)?;
                        } else {
                            crate::config::Config::save_openai_credentials_in_file(config_path, key)?;
                        }
                        println!("[✓] {} API key configured successfully.", prov_name);
                        println!(
                            "[✓] Run 'speechyyy use {}' whenever you want me to speak it, {}!",
                            trimmed.to_lowercase(),
                            nickname
                        );
                    } else {
                        println!("[!] Blank key entered. Configuration unchanged.");
                    }
                } else {
                    println!(
                        "[i] To configure, set {} in your environment or run 'speechyyy use {}' in an interactive terminal.",
                        env_var,
                        trimmed.to_lowercase()
                    );
                }
            }
        }
        Ok(())
    } else if let Some(local_type) = LocalModelType::from_str_name(trimmed) {
        let target_dir = match models_dir {
            Some(d) => d.join(local_type.folder_name()),
            None => ModelManager::model_dir(local_type),
        };
        ModelManager::download_model(local_type, &target_dir)?;
        println!(
            "[✓] Model '{}' is all yours, {}! Run 'speechyyy use {}' whenever you want me to speak it.",
            local_type.folder_name(),
            nickname,
            local_type.folder_name()
        );
        Ok(())
    } else {
        Err(unsupported_model_error(trimmed))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupportedModel {
    pub id: &'static str,
    pub provider_name: &'static str,
    pub provider_type: &'static str,
    pub is_local: bool,
    pub local_type: Option<LocalModelType>,
}

pub const SUPPORTED_MODELS: &[SupportedModel] = &[
    SupportedModel {
        id: "saaras:v4",
        provider_name: "Sarvam AI Cloud",
        provider_type: "sarvam",
        is_local: false,
        local_type: None,
    },
    SupportedModel {
        id: "parakeet-tdt-v3",
        provider_name: "Local Transducer",
        provider_type: "local",
        is_local: true,
        local_type: Some(LocalModelType::ParakeetTdtV3),
    },
    SupportedModel {
        id: "whisper-small",
        provider_name: "Local Whisper",
        provider_type: "local",
        is_local: true,
        local_type: Some(LocalModelType::WhisperSmall),
    },
    SupportedModel {
        id: "whisper-large-v3-turbo",
        provider_name: "Local Whisper",
        provider_type: "local",
        is_local: true,
        local_type: Some(LocalModelType::WhisperLargeV3Turbo),
    },
    SupportedModel {
        id: "moonshine-base",
        provider_name: "Local Moonshine",
        provider_type: "local",
        is_local: true,
        local_type: Some(LocalModelType::MoonshineBase),
    },
    #[cfg(target_os = "macos")]
    SupportedModel {
        id: "apple-speech",
        provider_name: "Apple Native",
        provider_type: "apple",
        is_local: false,
        local_type: None,
    },
];

pub fn check_local_model_exists(model_type: LocalModelType, base_dir: Option<&Path>) -> bool {
    let base = match base_dir {
        Some(d) => d.to_path_buf(),
        None => ModelManager::default_models_dir(),
    };
    let dir = base.join(model_type.folder_name());
    if !dir.exists() {
        return false;
    }
    ModelManager::verify_model_files(model_type, &dir).is_ok()
}

pub fn is_model_active(cfg: &crate::config::Config, model_id: &str) -> bool {
    if cfg.inference.model.trim().is_empty() {
        return false;
    }
    match model_id {
        "saaras:v4" => {
            cfg.inference.provider.eq_ignore_ascii_case("sarvam")
                && (cfg.inference.model.eq_ignore_ascii_case("saaras:v4")
                    || cfg.inference.model.eq_ignore_ascii_case("sarvam:saaras-v4"))
        }
        #[cfg(target_os = "macos")]
        "apple-speech" => cfg.inference.provider.eq_ignore_ascii_case("apple"),
        preset => {
            if !cfg.inference.provider.eq_ignore_ascii_case("local") {
                return false;
            }
            if let Some(target_type) = LocalModelType::from_str_name(preset) {
                if let Some(active_type) = LocalModelType::from_str_name(&cfg.inference.model) {
                    return target_type == active_type;
                }
            }
            cfg.inference.model.eq_ignore_ascii_case(preset)
        }
    }
}

pub fn format_model_list_with_models_dir(
    cfg: &crate::config::Config,
    models_dir: Option<&Path>,
) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "{:<24} {:<18} {:<16} {}",
        "NAME", "PROVIDER", "STATUS", "ACTIVE"
    ));

    for model in SUPPORTED_MODELS {
        let active_str = if is_model_active(cfg, model.id) {
            "yes"
        } else {
            "no"
        };

        let status_str = if model.is_local {
            if let Some(lt) = model.local_type {
                if check_local_model_exists(lt, models_dir) {
                    "Installed"
                } else {
                    "Not Downloaded"
                }
            } else {
                "Unknown"
            }
        } else if model.provider_type == "sarvam" {
            let has_key = !cfg.inference.api_key.trim().is_empty()
                || std::env::var("SARVAM_API_KEY").is_ok_and(|k| !k.trim().is_empty());
            if has_key {
                "Configured"
            } else {
                "API Key Required"
            }
        } else {
            "Available"
        };

        lines.push(format!(
            "{:<24} {:<18} {:<16} {}",
            model.id, model.provider_name, status_str, active_str
        ));
    }

    lines.push(String::new());
    lines.push("Tip: For custom ONNX models, set provider = 'custom' in ~/.config/speechyyy/config.toml".to_string());

    lines.join("\n")
}

pub fn format_model_list(cfg: &crate::config::Config) -> String {
    format_model_list_with_models_dir(cfg, None)
}

pub fn handle_model_list_command() -> Result<(), String> {
    let cfg = crate::config::Config::load();
    let output = format_model_list(&cfg);
    println!("{}", output);
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelUseResult {
    pub provider: String,
    pub model: String,
    pub notice: Option<String>,
    pub confirmation: String,
}

pub fn resolve_model_use(
    model_arg: &str,
    models_dir: Option<&Path>,
    config_path: Option<&Path>,
) -> Result<(String, String, Option<String>), String> {
    let lower = model_arg.trim().to_lowercase().replace('_', "-");
    match lower.as_str() {
        "groq" => {
            let cfg = match config_path {
                Some(p) => crate::config::Config::load_from_path(p),
                None => crate::config::Config::load(),
            };
            let has_key = cfg.inference.resolve_key_for_provider("groq").is_some();
            let notice = if !has_key {
                Some("[!] No Groq API key found. Set GROQ_API_KEY in config.toml or environment.".to_string())
            } else {
                None
            };
            Ok((
                "groq".to_string(),
                crate::inference::DEFAULT_GROQ_MODEL.to_string(),
                notice,
            ))
        }
        "openai" => {
            let cfg = match config_path {
                Some(p) => crate::config::Config::load_from_path(p),
                None => crate::config::Config::load(),
            };
            let has_key = cfg.inference.resolve_key_for_provider("openai").is_some();
            let notice = if !has_key {
                Some("[!] No OpenAI API key found. Set OPENAI_API_KEY in config.toml or environment.".to_string())
            } else {
                None
            };
            Ok((
                "openai".to_string(),
                crate::inference::DEFAULT_OPENAI_MODEL.to_string(),
                notice,
            ))
        }
        "openai-compatible" => {
            let cfg = match config_path {
                Some(p) => crate::config::Config::load_from_path(p),
                None => crate::config::Config::load(),
            };
            let has_endpoint = cfg
                .inference
                .openai_compatible_endpoint
                .as_ref()
                .is_some_and(|e| !e.trim().is_empty());
            let notice = if !has_endpoint {
                Some("[!] No OpenAI-compatible endpoint configured. Set 'openai_compatible_endpoint' in config.toml.".to_string())
            } else {
                None
            };
            let model = cfg
                .inference
                .openai_compatible_model
                .unwrap_or_else(|| crate::inference::DEFAULT_OPENAI_COMPATIBLE_MODEL.to_string());
            Ok(("openai-compatible".to_string(), model, notice))
        }
        s if is_sarvam_cloud_model(s) => {
            let cfg = match config_path {
                Some(p) => crate::config::Config::load_from_path(p),
                None => crate::config::Config::load(),
            };
            let has_key = cfg
                .inference
                .resolve_api_key()
                .map(|k| !k.trim().is_empty())
                .unwrap_or(false);

            let notice = if !has_key {
                Some("[!] No API key found. Run 'speechyyy download saaras:v4' first to configure.".to_string())
            } else {
                None
            };
            Ok(("sarvam".to_string(), "saaras:v4".to_string(), notice))
        }
        "parakeet-tdt-v3" | "parakeet-v3" => {
            let lt = LocalModelType::ParakeetTdtV3;
            let exists = check_local_model_exists(lt, models_dir);
            let notice = if !exists {
                Some("Weights not found. Run `speechyyy model download -m parakeet-tdt-v3`".to_string())
            } else {
                None
            };
            Ok(("local".to_string(), "parakeet-tdt-v3".to_string(), notice))
        }
        "whisper-small" => {
            let lt = LocalModelType::WhisperSmall;
            let exists = check_local_model_exists(lt, models_dir);
            let notice = if !exists {
                Some("Weights not found. Run `speechyyy model download -m whisper-small`".to_string())
            } else {
                None
            };
            Ok(("local".to_string(), "whisper-small".to_string(), notice))
        }
        "whisper-large-v3-turbo" | "whisper-turbo" => {
            let lt = LocalModelType::WhisperLargeV3Turbo;
            let exists = check_local_model_exists(lt, models_dir);
            let notice = if !exists {
                Some("Weights not found. Run `speechyyy model download -m whisper-large-v3-turbo`".to_string())
            } else {
                None
            };
            Ok((
                "local".to_string(),
                "whisper-large-v3-turbo".to_string(),
                notice,
            ))
        }
        "moonshine-base" => {
            let lt = LocalModelType::MoonshineBase;
            let exists = check_local_model_exists(lt, models_dir);
            let notice = if !exists {
                Some("Weights not found. Run `speechyyy model download -m moonshine-base`".to_string())
            } else {
                None
            };
            Ok(("local".to_string(), "moonshine-base".to_string(), notice))
        }
        #[cfg(target_os = "macos")]
        "apple-speech" => Ok(("apple".to_string(), "apple-speech".to_string(), None)),
        _ => Err(unsupported_model_error(model_arg)),
    }
}

pub fn handle_model_use_command_internal(
    model_arg: &str,
    config_path: &Path,
    models_dir: Option<&Path>,
) -> Result<ModelUseResult, String> {
    let (provider, target_model, notice) =
        resolve_model_use(model_arg, models_dir, Some(config_path))?;

    crate::config::Config::update_active_model_in_file(config_path, &provider, &target_model)?;

    let confirmation = format!(
        "Active model set to '{}' in config.toml. Restart Speechyyy to apply.",
        target_model
    );

    Ok(ModelUseResult {
        provider,
        model: target_model,
        notice,
        confirmation,
    })
}

pub fn handle_model_use_command(model_arg: &str) -> Result<(), String> {
    let config_path = crate::config::Config::config_path();
    let lower = model_arg.trim().to_lowercase().replace('_', "-");

    // 1. If key / endpoint missing for cloud providers, prompt interactively
    if lower == "groq" {
        let cfg = crate::config::Config::load_from_path(&config_path);
        if cfg.inference.resolve_key_for_provider("groq").is_none() {
            use std::io::{stdin, IsTerminal};
            if !stdin().is_terminal() {
                return Err("GROQ_API_KEY is not configured. Set GROQ_API_KEY in environment or run 'speechyyy use groq' in an interactive terminal.".to_string());
            }
            let key = rpassword::prompt_password("✦ Enter your Groq API Key: ")
                .map_err(|e| format!("Failed to read API key: {}", e))?;
            let trimmed = key.trim().trim_matches('"');
            if trimmed.is_empty() {
                return Err("Blank entry. Cancelled — configuration unchanged.".to_string());
            }
            crate::config::Config::save_groq_credentials_in_file(&config_path, trimmed)?;
        }
    } else if lower == "openai" {
        let cfg = crate::config::Config::load_from_path(&config_path);
        if cfg.inference.resolve_key_for_provider("openai").is_none() {
            use std::io::{stdin, IsTerminal};
            if !stdin().is_terminal() {
                return Err("OPENAI_API_KEY is not configured. Set OPENAI_API_KEY in environment or run 'speechyyy use openai' in an interactive terminal.".to_string());
            }
            let key = rpassword::prompt_password("✦ Enter your OpenAI API Key: ")
                .map_err(|e| format!("Failed to read API key: {}", e))?;
            let trimmed = key.trim().trim_matches('"');
            if trimmed.is_empty() {
                return Err("Blank entry. Cancelled — configuration unchanged.".to_string());
            }
            crate::config::Config::save_openai_credentials_in_file(&config_path, trimmed)?;
        }
    } else if lower == "openai-compatible" {
        let cfg = crate::config::Config::load_from_path(&config_path);
        if cfg
            .inference
            .openai_compatible_endpoint
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
        {
            use std::io::{stdin, stdout, IsTerminal, Write};
            if !stdin().is_terminal() {
                return Err("OpenAI-compatible endpoint is not configured. Set 'openai_compatible_endpoint' in config.toml or run 'speechyyy use openai-compatible' in an interactive terminal.".to_string());
            }
            print!("✦ Enter your OpenAI-compatible endpoint URL (e.g. http://localhost:11434/v1): ");
            let _ = stdout().flush();
            let mut line = String::new();
            stdin()
                .read_line(&mut line)
                .map_err(|e| format!("Failed to read endpoint: {}", e))?;
            let endpoint = line.trim();
            if endpoint.is_empty() {
                return Err("Blank entry. Cancelled — configuration unchanged.".to_string());
            }
            crate::inference::validate_endpoint(endpoint)
                .map_err(|e| format!("Invalid endpoint: {}", e))?;

            let key_input = rpassword::prompt_password(
                "✦ Enter an optional API Key (press Enter to skip): ",
            )
            .unwrap_or_default();
            let trimmed_key = key_input.trim().trim_matches('"');
            let key_opt = if trimmed_key.is_empty() {
                None
            } else {
                Some(trimmed_key)
            };

            crate::config::Config::save_openai_compatible_config_in_file(
                &config_path,
                endpoint,
                Some("whisper-1"),
                key_opt,
            )?;
        }
    }

    let result = handle_model_use_command_internal(
        model_arg,
        &config_path,
        None,
    )?;

    if let Some(notice) = result.notice {
        println!("{}", notice);
    }

    // Privacy notice
    let cfg = crate::config::Config::load_from_path(&config_path);
    match result.provider.as_str() {
        "groq" => {
            println!("[i] Privacy notice: Audio will be streamed to api.groq.com for transcription.");
        }
        "openai" => {
            println!("[i] Privacy notice: Audio will be streamed to api.openai.com for transcription.");
        }
        "sarvam" => {
            println!("[i] Privacy notice: Audio will be streamed to api.sarvam.ai for transcription.");
        }
        "openai-compatible" => {
            if let Some(endpoint) = cfg.inference.openai_compatible_endpoint.as_deref() {
                let host = crate::inference::extract_endpoint_host(endpoint)
                    .unwrap_or_else(|| "endpoint".to_string());
                if crate::inference::is_loopback_host(&host) {
                    println!(
                        "[i] Privacy notice: Audio stays on this machine ({}) — no external network calls.",
                        host
                    );
                } else {
                    println!(
                        "[i] Privacy notice: Audio will be streamed to {} for transcription.",
                        host
                    );
                }
            }
        }
        _ => {}
    }

    let nickname = cfg.nickname();
    let paths = crate::ipc::RuntimePaths::new();
    let is_running = crate::ipc::PidFile::read_pid(&paths.pid_path)
        .is_some_and(crate::ipc::PidFile::is_process_running);

    if is_running {
        let new_pid = crate::daemon::restart_daemon_process(&paths, None)?;
        println!(
            "[✓] Switched to '{}' for you, {}. Speechyyy reloaded (PID: {}).",
            result.model, nickname, new_pid
        );
    } else {
        println!(
            "[✓] Switched to '{}' for you, {}. Run 'speechyyy start' to wake me up.",
            result.model, nickname
        );
    }

    Ok(())
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                total += dir_size(&p);
            } else if let Ok(meta) = p.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

pub fn handle_model_remove_internal(
    model_name: &str,
    models_dir: Option<&Path>,
    config_path: Option<&Path>,
) -> Result<(), String> {
    handle_model_remove_full(model_name, models_dir, config_path, None)
}

pub fn handle_model_remove_full(
    model_name: &str,
    models_dir: Option<&Path>,
    config_path: Option<&Path>,
    paths_override: Option<&crate::ipc::RuntimePaths>,
) -> Result<(), String> {
    let trimmed = model_name.trim();
    if trimmed.is_empty() {
        return Err("Model name cannot be empty".to_string());
    }

    let base_dir = models_dir
        .map(PathBuf::from)
        .unwrap_or_else(ModelManager::default_models_dir);

    let target_dir = if let Some(lt) = LocalModelType::from_str_name(trimmed) {
        let preset_dir = base_dir.join(lt.folder_name());
        if preset_dir.exists() {
            preset_dir
        } else {
            base_dir.join(trimmed)
        }
    } else {
        base_dir.join(trimmed)
    };

    if !target_dir.exists() {
        println!("[!] Model '{}' is not installed locally.", trimmed);
        return Ok(());
    }

    let size_bytes = dir_size(&target_dir);
    let size_mb = size_bytes as f64 / (1024.0 * 1024.0);

    let cfg = match config_path {
        Some(cp) => crate::config::Config::load_from_path(cp),
        None => crate::config::Config::load(),
    };
    let nickname = cfg.nickname();

    if is_model_active(&cfg, trimmed) {
        let paths = paths_override.cloned().unwrap_or_else(crate::ipc::RuntimePaths::new);
        let is_running = crate::ipc::PidFile::read_pid(&paths.pid_path)
            .is_some_and(crate::ipc::PidFile::is_process_running)
            || paths.socket_path.exists();
        if is_running {
            let _ = crate::daemon::stop_daemon(&paths);
        }

        let target_config_path = config_path
            .map(PathBuf::from)
            .unwrap_or_else(crate::config::Config::config_path);
        let _ = crate::config::Config::deactivate_model_in_file(&target_config_path);

        println!(
            "[!] Notice: '{}' was your active model, {}. Speechyyy has been stopped and deactivated. Pick another model with 'speechyyy use <model>' whenever you're ready.",
            trimmed, nickname
        );
    }

    fs::remove_dir_all(&target_dir)
        .map_err(|e| format!("Failed to remove model directory {}: {}", target_dir.display(), e))?;

    let size_str = format!("{:.1} MB", size_mb);
    println!(
        "[✓] Removed '{}', {}. Reclaimed {} for you.",
        trimmed, nickname, size_str
    );

    Ok(())
}

pub fn handle_model_remove(model_name: &str) -> Result<(), String> {
    handle_model_remove_internal(model_name, None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_type_parsing() {
        assert_eq!(
            LocalModelType::from_str_name("parakeet-tdt-v3"),
            Some(LocalModelType::ParakeetTdtV3)
        );
        assert_eq!(
            LocalModelType::from_str_name("parakeet-v3"),
            Some(LocalModelType::ParakeetTdtV3)
        );
        assert_eq!(
            LocalModelType::from_str_name("whisper-small"),
            Some(LocalModelType::WhisperSmall)
        );
        assert_eq!(
            LocalModelType::from_str_name("whisper-large-v3-turbo"),
            Some(LocalModelType::WhisperLargeV3Turbo)
        );
        assert_eq!(
            LocalModelType::from_str_name("whisper-turbo"),
            Some(LocalModelType::WhisperLargeV3Turbo)
        );
        assert_eq!(
            LocalModelType::from_str_name("moonshine-base"),
            Some(LocalModelType::MoonshineBase)
        );
        assert_eq!(LocalModelType::from_str_name("unknown-model"), None);
    }

    #[test]
    fn test_verify_model_files_success() {
        let temp_dir =
            std::env::temp_dir().join(format!("model_verify_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        // Create mock files for Parakeet
        for group in LocalModelType::ParakeetTdtV3.required_file_groups() {
            fs::write(temp_dir.join(group[0]), b"fake onnx weights").unwrap();
        }

        let info =
            ModelManager::verify_model_files(LocalModelType::ParakeetTdtV3, &temp_dir).unwrap();
        assert_eq!(info.model_type, LocalModelType::ParakeetTdtV3);
        assert_eq!(info.file_count, 4);
        assert!(info.total_bytes > 0);

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_verify_model_files_missing_file_fails() {
        let temp_dir =
            std::env::temp_dir().join(format!("model_verify_fail_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        // Only create 1 file
        fs::write(temp_dir.join("encoder.int8.onnx"), b"fake data").unwrap();

        let err =
            ModelManager::verify_model_files(LocalModelType::ParakeetTdtV3, &temp_dir).unwrap_err();
        assert!(err.contains("Missing required model file"));

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_model_list_formatting_and_weights_detection() {
        let temp_dir = std::env::temp_dir().join(format!("model_list_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        let models_dir = temp_dir.join("models");
        let parakeet_dir = models_dir.join("parakeet-tdt-v3");
        fs::create_dir_all(&parakeet_dir).unwrap();
        for group in LocalModelType::ParakeetTdtV3.required_file_groups() {
            fs::write(parakeet_dir.join(group[0]), b"weights").unwrap();
        }

        let mut cfg = crate::config::Config::default();
        cfg.inference.provider = "sarvam".into();
        cfg.inference.model = "saaras:v4".into();

        let output_sarvam = format_model_list_with_models_dir(&cfg, Some(&models_dir));
        assert!(output_sarvam.contains("NAME                     PROVIDER           STATUS           ACTIVE".split_whitespace().next().unwrap()));
        assert!(output_sarvam.contains("saaras:v4"));
        assert!(output_sarvam.contains("Sarvam AI Cloud"));
        assert!(output_sarvam.contains("parakeet-tdt-v3"));
        assert!(output_sarvam.contains("Installed"));
        assert!(output_sarvam.contains("Not Downloaded"));
        assert!(output_sarvam.contains("API Key Required") || output_sarvam.contains("Configured"));
        assert!(output_sarvam.contains("Tip: For custom ONNX models, set provider = 'custom' in ~/.config/speechyyy/config.toml"));

        cfg.inference.api_key = "test_key".into();
        let output_configured = format_model_list_with_models_dir(&cfg, Some(&models_dir));
        assert!(output_configured.contains("Configured"));

        // Switch active config to local parakeet-tdt-v3
        cfg.inference.provider = "local".into();
        cfg.inference.model = "parakeet-tdt-v3".into();
        let output_parakeet = format_model_list_with_models_dir(&cfg, Some(&models_dir));
        let parakeet_row = output_parakeet
            .lines()
            .find(|l| l.contains("parakeet-tdt-v3"))
            .expect("parakeet row");
        assert!(parakeet_row.contains("Local Transducer"));
        assert!(parakeet_row.contains("Installed"));
        assert!(parakeet_row.contains("yes"));

        // When deactivated, no model is active ("no" for all models)
        cfg.inference.model = "".into();
        let output_inactive = format_model_list_with_models_dir(&cfg, Some(&models_dir));
        for line in output_inactive.lines().skip(1) {
            if line.contains("Sarvam") || line.contains("Local") {
                assert!(line.ends_with("no"), "Row should have ACTIVE: no, got: {}", line);
            }
        }

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_handle_model_use_command_flow() {
        let temp_dir = std::env::temp_dir().join(format!("model_use_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        let config_file = temp_dir.join("config.toml");
        let models_dir = temp_dir.join("models");

        // 1. Unknown model returns error
        let err = handle_model_use_command_internal("unknown-model", &config_file, Some(&models_dir));
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("Unsupported model"));

        // 2. saaras:v4 or sarvam -> provider sarvam, model saaras:v4, warns if no API key
        let res_sarvam = handle_model_use_command_internal("sarvam", &config_file, Some(&models_dir))
            .expect("use sarvam");
        assert_eq!(res_sarvam.provider, "sarvam");
        assert_eq!(res_sarvam.model, "saaras:v4");
        assert_eq!(
            res_sarvam.notice,
            Some("[!] No API key found. Run 'speechyyy download saaras:v4' first to configure.".to_string())
        );
        assert_eq!(
            res_sarvam.confirmation,
            "Active model set to 'saaras:v4' in config.toml. Restart Speechyyy to apply."
        );
        let cfg = crate::config::Config::load_from_path(&config_file);
        assert_eq!(cfg.inference.provider, "sarvam");
        assert_eq!(cfg.inference.model, "saaras:v4");

        // 2b. saaras:v4 with configured API key -> no notice
        crate::config::Config::save_sarvam_credentials_in_file(&config_file, "key_abc123").unwrap();
        let res_sarvam_keyed = handle_model_use_command_internal("saaras:v4", &config_file, Some(&models_dir))
            .expect("use saaras:v4 with key");
        assert_eq!(res_sarvam_keyed.provider, "sarvam");
        assert_eq!(res_sarvam_keyed.model, "saaras:v4");
        assert_eq!(res_sarvam_keyed.notice, None);

        // 3. parakeet-tdt-v3 when weights don't exist -> notice printed, config updated
        let res_parakeet = handle_model_use_command_internal("parakeet-tdt-v3", &config_file, Some(&models_dir))
            .expect("use parakeet-tdt-v3");
        assert_eq!(res_parakeet.provider, "local");
        assert_eq!(res_parakeet.model, "parakeet-tdt-v3");
        assert_eq!(
            res_parakeet.notice,
            Some("Weights not found. Run `speechyyy model download -m parakeet-tdt-v3`".into())
        );
        assert_eq!(
            res_parakeet.confirmation,
            "Active model set to 'parakeet-tdt-v3' in config.toml. Restart Speechyyy to apply."
        );
        let cfg = crate::config::Config::load_from_path(&config_file);
        assert_eq!(cfg.inference.provider, "local");
        assert_eq!(cfg.inference.model, "parakeet-tdt-v3");

        // 4. whisper-small when weights exist -> no notice, config updated
        let whisper_dir = models_dir.join("whisper-small");
        fs::create_dir_all(&whisper_dir).unwrap();
        for group in LocalModelType::WhisperSmall.required_file_groups() {
            fs::write(whisper_dir.join(group[0]), b"weights").unwrap();
        }
        let res_whisper = handle_model_use_command_internal("whisper-small", &config_file, Some(&models_dir))
            .expect("use whisper-small");
        assert_eq!(res_whisper.provider, "local");
        assert_eq!(res_whisper.model, "whisper-small");
        assert_eq!(res_whisper.notice, None);
        assert_eq!(
            res_whisper.confirmation,
            "Active model set to 'whisper-small' in config.toml. Restart Speechyyy to apply."
        );
        let cfg = crate::config::Config::load_from_path(&config_file);
        assert_eq!(cfg.inference.provider, "local");
        assert_eq!(cfg.inference.model, "whisper-small");

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_download_progress_bar_style_and_transitions() {
        let template =
            "[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}";
        let style = ProgressStyle::with_template(template).expect("valid template");
        let pb = ProgressBar::new(1024);
        pb.set_style(style);
        pb.set_message("Downloading...");
        pb.inc(512);

        // Transition to extraction
        pb.set_message("Extracting archive...");
        assert_eq!(pb.message(), "Extracting archive...");

        // Transition to completion
        let completion = format!(
            "[✓] Successfully installed {} to /tmp/path",
            LocalModelType::ParakeetTdtV3.folder_name()
        );
        pb.finish_with_message(completion.clone());
        assert_eq!(pb.message(), completion);
    }

    #[test]
    fn test_archive_extraction_and_cleanup() {
        let temp_dir =
            std::env::temp_dir().join(format!("model_archive_extract_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        // Build an in-memory tar.bz2 containing the Parakeet files
        let mut tar_builder = tar::Builder::new(Vec::new());
        for filename in &["encoder.int8.onnx", "decoder.int8.onnx", "joiner.int8.onnx", "tokens.txt"] {
            let data = b"dummy onnx weights or tokens";
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar_builder
                .append_data(&mut header, format!("nested_root/{}", filename), &data[..])
                .unwrap();
        }
        let tar_bytes = tar_builder.into_inner().unwrap();
        let mut bz2_encoder = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
        std::io::Write::write_all(&mut bz2_encoder, &tar_bytes).unwrap();
        let compressed_bytes = bz2_encoder.finish().unwrap();

        // Write as .tar.bz2.part
        let part_file = temp_dir.join(format!("{}.tar.bz2.part", LocalModelType::ParakeetTdtV3.folder_name()));
        fs::write(&part_file, &compressed_bytes).unwrap();
        assert!(part_file.exists());

        // Perform decompression & unpacking loop like in download_model
        let archive_file = fs::File::open(&part_file).unwrap();
        let decompressor = bzip2::read::BzDecoder::new(archive_file);
        let mut archive = tar::Archive::new(decompressor);

        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            let path = entry.path().unwrap();
            let components: Vec<_> = path.components().collect();
            let relative_path: PathBuf = if components.len() > 1 {
                components[1..].iter().collect()
            } else {
                path.to_path_buf()
            };
            if relative_path.as_os_str().is_empty() {
                continue;
            }
            let dest = temp_dir.join(relative_path);
            if entry.header().entry_type().is_dir() {
                let _ = fs::create_dir_all(&dest);
            } else {
                if let Some(parent) = dest.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                entry.unpack(&dest).unwrap();
            }
        }

        let _ = fs::remove_file(&part_file);
        assert!(!part_file.exists());

        let info = ModelManager::verify_model_files(LocalModelType::ParakeetTdtV3, &temp_dir).unwrap();
        assert_eq!(info.file_count, 4);

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_handle_model_download_flow() {
        let temp_dir = std::env::temp_dir().join(format!("model_download_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        let config_file = temp_dir.join("config.toml");
        let models_dir = temp_dir.join("models");

        // 1. saaras:v4 when no key is configured -> prompts for key, writes config
        let mut prompt_called = false;
        let mut prompter = || {
            prompt_called = true;
            Ok("my_entered_sarvam_key_999".to_string())
        };
        let mut confirm_noop = || Ok(false);
        let res = handle_model_download_internal(
            "saaras:v4",
            false,
            &config_file,
            Some(&models_dir),
            &mut confirm_noop,
            &mut prompter,
        );
        assert!(res.is_ok());
        assert!(prompt_called);
        let cfg = crate::config::Config::load_from_path(&config_file);
        assert_eq!(cfg.inference.provider, "local");
        assert_eq!(cfg.inference.api_key, "my_entered_sarvam_key_999");

        // 2a. sarvam:saaras-v4 when key is already configured and confirm is false -> does not prompt
        let mut prompt_called2 = false;
        let mut prompter2 = || {
            prompt_called2 = true;
            Ok("should_not_be_called".to_string())
        };
        let mut confirm_decline = || Ok(false);
        let res2 = handle_model_download_internal(
            "sarvam:saaras-v4",
            false,
            &config_file,
            Some(&models_dir),
            &mut confirm_decline,
            &mut prompter2,
        );
        assert!(res2.is_ok());
        assert!(!prompt_called2);

        // 2b. saaras:v4 with confirm = true -> prompts and overwrites key
        let mut prompt_called_confirm = false;
        let mut prompter_confirm = || {
            prompt_called_confirm = true;
            Ok("  \"overwritten_key_with_quotes\"  ".to_string())
        };
        let mut confirm_accept = || Ok(true);
        let res_confirm = handle_model_download_internal(
            "saaras:v4",
            false,
            &config_file,
            Some(&models_dir),
            &mut confirm_accept,
            &mut prompter_confirm,
        );
        assert!(res_confirm.is_ok());
        assert!(prompt_called_confirm);
        let cfg_updated = crate::config::Config::load_from_path(&config_file);
        assert_eq!(cfg_updated.inference.api_key, "overwritten_key_with_quotes");

        // 2c. saaras:v4 with force = true -> prompts and overwrites key even if confirm is false
        let mut prompt_called_force = false;
        let mut prompter_force = || {
            prompt_called_force = true;
            Ok("forced_key_123".to_string())
        };
        let mut confirm_false = || Ok(false);
        let res_force = handle_model_download_internal(
            "saaras:v4",
            true,
            &config_file,
            Some(&models_dir),
            &mut confirm_false,
            &mut prompter_force,
        );
        assert!(res_force.is_ok());
        assert!(prompt_called_force);
        let cfg_forced = crate::config::Config::load_from_path(&config_file);
        assert_eq!(cfg_forced.inference.api_key, "forced_key_123");

        // 3. saaras:v4 when user enters empty key -> returns error
        let empty_cfg_file = temp_dir.join("empty_config.toml");
        let mut empty_prompter = || Ok("   ".to_string());
        let res_empty = handle_model_download_internal(
            "saaras:v4",
            false,
            &empty_cfg_file,
            Some(&models_dir),
            &mut confirm_noop,
            &mut empty_prompter,
        );
        assert!(res_empty.is_err());
        assert_eq!(
            res_empty.unwrap_err(),
            "Sarvam API key cannot be empty."
        );

        // 4. unsupported model -> returns clear error listing models
        let mut dummy_prompter = || Ok("dummy".to_string());
        let res_unsupported = handle_model_download_internal(
            "nonexistent-model",
            false,
            &config_file,
            Some(&models_dir),
            &mut confirm_noop,
            &mut dummy_prompter,
        );
        assert!(res_unsupported.is_err());
        let err_msg = res_unsupported.unwrap_err();
        assert!(err_msg.contains("Unsupported model 'nonexistent-model'"));
        assert!(err_msg.contains("saaras:v4"));
        assert!(err_msg.contains("parakeet-tdt-v3"));

        let _ = fs::remove_dir_all(temp_dir);
    }
}
