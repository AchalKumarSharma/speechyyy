use indicatif::{ProgressBar, ProgressStyle};
use std::fs::{self};
use std::path::{Path, PathBuf};

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

    /// Downloads the model archive (.tar.bz2) and unpacks into `target_dir`.
    pub fn download_model(
        model_type: LocalModelType,
        target_dir: &Path,
    ) -> Result<ModelInfo, String> {
        fs::create_dir_all(target_dir).map_err(|e| {
            format!(
                "Failed to create target directory {}: {}",
                target_dir.display(),
                e
            )
        })?;

        let url = model_type.download_url();
        println!(
            "Downloading {} from {} into {}...",
            model_type.display_name(),
            url,
            target_dir.display()
        );

        let resp = ureq::get(url)
            .call()
            .map_err(|e| format!("Download request failed for {}: {}", url, e))?;

        let total_bytes = resp
            .header("content-length")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        let pb = ProgressBar::new(total_bytes);
        let style = ProgressStyle::with_template(
            "[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}",
        )
        .unwrap_or_else(|_| ProgressStyle::default_bar())
        .progress_chars("##-");
        pb.set_style(style);

        let temp_archive_path = target_dir.join(format!("{}.tar.bz2.part", model_type.folder_name()));
        if temp_archive_path.exists() {
            let _ = fs::remove_file(&temp_archive_path);
        }

        let mut reader = pb.wrap_read(resp.into_reader());
        let mut dest_file = fs::File::create(&temp_archive_path).map_err(|e| {
            format!(
                "Failed to create temp archive {}: {}",
                temp_archive_path.display(),
                e
            )
        })?;

        std::io::copy(&mut reader, &mut dest_file).map_err(|e| {
            let _ = fs::remove_file(&temp_archive_path);
            format!("Failed to download archive: {}", e)
        })?;
        drop(dest_file);

        pb.set_message("Extracting archive...");

        let archive_file = fs::File::open(&temp_archive_path).map_err(|e| {
            let _ = fs::remove_file(&temp_archive_path);
            format!(
                "Failed to open downloaded archive {}: {}",
                temp_archive_path.display(),
                e
            )
        })?;
        let decompressor = bzip2::read::BzDecoder::new(archive_file);
        let mut archive = tar::Archive::new(decompressor);

        for entry in archive
            .entries()
            .map_err(|e| {
                let _ = fs::remove_file(&temp_archive_path);
                format!("Failed to read tar archive: {}", e)
            })?
        {
            let mut entry = entry.map_err(|e| {
                let _ = fs::remove_file(&temp_archive_path);
                format!("Failed to read tar entry: {}", e)
            })?;
            let path = entry
                .path()
                .map_err(|e| {
                    let _ = fs::remove_file(&temp_archive_path);
                    format!("Invalid path in tar: {}", e)
                })?;
            let components: Vec<_> = path.components().collect();
            let relative_path: PathBuf = if components.len() > 1 {
                components[1..].iter().collect()
            } else {
                path.to_path_buf()
            };
            if relative_path.as_os_str().is_empty() {
                continue;
            }
            let dest = target_dir.join(relative_path);
            if entry.header().entry_type().is_dir() {
                let _ = fs::create_dir_all(&dest);
            } else {
                if let Some(parent) = dest.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                entry
                    .unpack(&dest)
                    .map_err(|e| {
                        let _ = fs::remove_file(&temp_archive_path);
                        format!("Failed to extract file {}: {}", dest.display(), e)
                    })?;
            }
        }

        let _ = fs::remove_file(&temp_archive_path);

        let info = Self::verify_model_files(model_type, target_dir)?;
        let completion_msg = format!(
            "[✓] Successfully installed {} to {}",
            model_type.folder_name(),
            target_dir.display()
        );
        pb.finish_with_message(completion_msg.clone());
        println!("{}", completion_msg);

        Ok(info)
    }
}

/// Checks if a model identifier refers to the Sarvam AI cloud provider.
pub fn is_sarvam_cloud_model(name: &str) -> bool {
    matches!(
        name.trim().to_lowercase().replace('_', "-").as_str(),
        "saaras:v4" | "sarvam:saaras-v4" | "sarvam" | "saaras"
    )
}

/// Generates a descriptive error message listing all supported local and cloud models.
pub fn unsupported_model_error(model: &str) -> String {
    #[cfg(target_os = "macos")]
    let supported = "Cloud: saaras:v4 (sarvam:saaras-v4). Local: parakeet-tdt-v3, whisper-small, whisper-large-v3-turbo, moonshine-base, apple-speech.";
    #[cfg(not(target_os = "macos"))]
    let supported = "Cloud: saaras:v4 (sarvam:saaras-v4). Local: parakeet-tdt-v3, whisper-small, whisper-large-v3-turbo, moonshine-base.";
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
    let result = handle_model_use_command_internal(
        model_arg,
        &config_path,
        None,
    )?;

    if let Some(notice) = result.notice {
        println!("{}", notice);
    }

    let config = crate::config::Config::load_from_path(&config_path);
    let nickname = config.nickname();

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
