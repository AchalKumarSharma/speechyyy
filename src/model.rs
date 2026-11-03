use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum LocalModelType {
    #[value(name = "moonshine-tiny", alias = "moonshine")]
    MoonshineTiny,
    #[value(name = "whisper-tiny", alias = "whisper")]
    WhisperTiny,
}

impl LocalModelType {
    pub fn from_str_name(name: &str) -> Option<Self> {
        match name.to_lowercase().replace('_', "-").as_str() {
            "moonshine" | "moonshine-tiny" | "moonshine-tiny-en-int8" => Some(Self::MoonshineTiny),
            "whisper" | "whisper-tiny" | "whisper-tiny-en-int8" | "tiny.en" => {
                Some(Self::WhisperTiny)
            }
            _ => None,
        }
    }

    pub fn folder_name(&self) -> &'static str {
        match self {
            Self::MoonshineTiny => "moonshine-tiny-en-int8",
            Self::WhisperTiny => "whisper-tiny-en-int8",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::MoonshineTiny => "Moonshine-Tiny (INT8)",
            Self::WhisperTiny => "Whisper-Tiny (INT8)",
        }
    }

    pub fn required_files(&self) -> &'static [&'static str] {
        match self {
            Self::MoonshineTiny => &[
                "preprocess.onnx",
                "encode.int8.onnx",
                "uncached_decode.int8.onnx",
                "cached_decode.int8.onnx",
                "tokens.txt",
            ],
            Self::WhisperTiny => &[
                "tiny.en-encoder.int8.onnx",
                "tiny.en-decoder.int8.onnx",
                "tiny.en-tokens.txt",
            ],
        }
    }

    pub fn download_base_url(&self) -> &'static str {
        match self {
            Self::MoonshineTiny => {
                "https://huggingface.co/csukuangfj/sherpa-onnx-moonshine-tiny-en-int8/resolve/main"
            }
            Self::WhisperTiny => {
                "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-tiny.en/resolve/main"
            }
        }
    }
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

        let required = model_type.required_files();
        let mut total_bytes = 0u64;

        for &file_name in required {
            let file_path = dir.join(file_name);
            if !file_path.exists() {
                return Err(format!(
                    "Missing required model file: {}",
                    file_path.display()
                ));
            }

            let meta = fs::metadata(&file_path)
                .map_err(|e| format!("Failed to inspect {}: {}", file_path.display(), e))?;

            if meta.len() == 0 {
                return Err(format!(
                    "Model file {} is 0 bytes (corrupted)",
                    file_path.display()
                ));
            }

            total_bytes += meta.len();
        }

        Ok(ModelInfo {
            model_type,
            path: dir.to_path_buf(),
            file_count: required.len(),
            total_bytes,
        })
    }

    /// Downloads the model artifacts from HuggingFace mirror into `target_dir`.
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

        println!(
            "Downloading {} into {}...",
            model_type.display_name(),
            target_dir.display()
        );

        let base_url = model_type.download_base_url();
        let required = model_type.required_files();

        for (idx, &file_name) in required.iter().enumerate() {
            let dest_path = target_dir.join(file_name);
            let part_path = target_dir.join(format!("{}.part", file_name));

            let url = format!("{}/{}", base_url, file_name);
            println!(
                "[{}/{}] Downloading {}...",
                idx + 1,
                required.len(),
                file_name
            );

            let resp = ureq::get(&url)
                .call()
                .map_err(|e| format!("Download request failed for {}: {}", url, e))?;

            let mut reader = resp.into_reader();
            let mut file = File::create(&part_path).map_err(|e| {
                format!("Failed to create part file {}: {}", part_path.display(), e)
            })?;

            std::io::copy(&mut reader, &mut file)
                .map_err(|e| format!("Failed to stream data to {}: {}", part_path.display(), e))?;

            file.flush().map_err(|e| e.to_string())?;
            drop(file);

            // Rename .part to final
            fs::rename(&part_path, &dest_path).map_err(|e| {
                format!(
                    "Failed to rename {} to {}: {}",
                    part_path.display(),
                    dest_path.display(),
                    e
                )
            })?;
        }

        let info = Self::verify_model_files(model_type, target_dir)?;
        println!(
            "Successfully verified {} ({:.1} MB, {} files).",
            model_type.display_name(),
            info.total_mb(),
            info.file_count
        );

        Ok(info)
    }
}

/// Handler for the CLI `model download` command.
pub fn handle_model_download_command(model: LocalModelType) -> Result<(), String> {
    let target_dir = ModelManager::model_dir(model);
    ModelManager::download_model(model, &target_dir).map(|_| ())
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
        id: "moonshine-tiny",
        provider_name: "Local ONNX",
        provider_type: "local",
        is_local: true,
        local_type: Some(LocalModelType::MoonshineTiny),
    },
    SupportedModel {
        id: "whisper-tiny",
        provider_name: "Local ONNX",
        provider_type: "local",
        is_local: true,
        local_type: Some(LocalModelType::WhisperTiny),
    },
];

pub fn check_local_model_exists(model_type: LocalModelType, base_dir: Option<&Path>) -> bool {
    let base = match base_dir {
        Some(d) => d.to_path_buf(),
        None => ModelManager::default_models_dir(),
    };
    let short_name = match model_type {
        LocalModelType::MoonshineTiny => "moonshine-tiny",
        LocalModelType::WhisperTiny => "whisper-tiny",
    };
    let folder_name = model_type.folder_name();
    base.join(short_name).exists() || base.join(folder_name).exists()
}

pub fn is_model_active(cfg: &crate::config::Config, model_id: &str) -> bool {
    match model_id {
        "saaras:v4" => cfg.inference.provider.eq_ignore_ascii_case("sarvam"),
        "moonshine-tiny" => {
            cfg.inference.provider.eq_ignore_ascii_case("local")
                && (cfg.inference.model.eq_ignore_ascii_case("moonshine-tiny")
                    || cfg.inference.model.eq_ignore_ascii_case("moonshine")
                    || (!cfg.inference.model.eq_ignore_ascii_case("whisper-tiny")
                        && !cfg.inference.model.eq_ignore_ascii_case("whisper")))
        }
        "whisper-tiny" => {
            cfg.inference.provider.eq_ignore_ascii_case("local")
                && (cfg.inference.model.eq_ignore_ascii_case("whisper-tiny")
                    || cfg.inference.model.eq_ignore_ascii_case("whisper"))
        }
        _ => false,
    }
}

pub fn format_model_list_with_models_dir(
    cfg: &crate::config::Config,
    models_dir: Option<&Path>,
) -> String {
    let mut lines = Vec::new();
    lines.push("Supported models:".to_string());

    for model in SUPPORTED_MODELS {
        let active = is_model_active(cfg, model.id);
        let active_tag = if active { " [active]" } else { "" };

        let weights_tag = if model.is_local {
            if let Some(lt) = model.local_type {
                if check_local_model_exists(lt, models_dir) {
                    " [installed]"
                } else {
                    " [not downloaded]"
                }
            } else {
                ""
            }
        } else {
            ""
        };

        lines.push(format!(
            "  * {} (Provider: {}){}{}",
            model.id, model.provider_name, active_tag, weights_tag
        ));
    }

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
) -> Result<(String, String, Option<String>), String> {
    let lower = model_arg.trim().to_lowercase();
    match lower.as_str() {
        "saaras:v4" | "sarvam" => Ok(("sarvam".to_string(), "saaras:v4".to_string(), None)),
        "moonshine-tiny" => {
            let exists = check_local_model_exists(LocalModelType::MoonshineTiny, models_dir);
            let notice = if !exists {
                Some("Model weights not found. Run `speechyyy model download -m moonshine-tiny`".to_string())
            } else {
                None
            };
            Ok(("local".to_string(), "moonshine-tiny".to_string(), notice))
        }
        "whisper-tiny" => {
            let exists = check_local_model_exists(LocalModelType::WhisperTiny, models_dir);
            let notice = if !exists {
                Some("Model weights not found. Run `speechyyy model download -m whisper-tiny`".to_string())
            } else {
                None
            };
            Ok(("local".to_string(), "whisper-tiny".to_string(), notice))
        }
        _ => Err(format!(
            "Unsupported model '{}'. Supported models: saaras:v4, moonshine-tiny, whisper-tiny",
            model_arg
        )),
    }
}

pub fn handle_model_use_command_internal(
    model_arg: &str,
    config_path: &Path,
    models_dir: Option<&Path>,
) -> Result<ModelUseResult, String> {
    let (provider, target_model, notice) = resolve_model_use(model_arg, models_dir)?;

    crate::config::Config::update_active_model_in_file(config_path, &provider, &target_model)?;

    let confirmation = format!(
        "Active model set to '{}' in config.toml. Restart daemon to apply.",
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
    let result = handle_model_use_command_internal(
        model_arg,
        &crate::config::Config::config_path(),
        None,
    )?;

    if let Some(notice) = result.notice {
        println!("{}", notice);
    }
    println!("{}", result.confirmation);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_type_parsing() {
        assert_eq!(
            LocalModelType::from_str_name("moonshine-tiny"),
            Some(LocalModelType::MoonshineTiny)
        );
        assert_eq!(
            LocalModelType::from_str_name("whisper-tiny"),
            Some(LocalModelType::WhisperTiny)
        );
        assert_eq!(LocalModelType::from_str_name("unknown-model"), None);
    }

    #[test]
    fn test_verify_model_files_success() {
        let temp_dir =
            std::env::temp_dir().join(format!("model_verify_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        // Create mock files for Moonshine
        for &name in LocalModelType::MoonshineTiny.required_files() {
            fs::write(temp_dir.join(name), b"fake onnx weights").unwrap();
        }

        let info =
            ModelManager::verify_model_files(LocalModelType::MoonshineTiny, &temp_dir).unwrap();
        assert_eq!(info.model_type, LocalModelType::MoonshineTiny);
        assert_eq!(info.file_count, 5);
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
        fs::write(temp_dir.join("preprocess.onnx"), b"fake data").unwrap();

        let err =
            ModelManager::verify_model_files(LocalModelType::MoonshineTiny, &temp_dir).unwrap_err();
        assert!(err.contains("Missing required model file"));

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_model_list_formatting_and_weights_detection() {
        let temp_dir = std::env::temp_dir().join(format!("model_list_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        let models_dir = temp_dir.join("models");
        fs::create_dir_all(models_dir.join("moonshine-tiny")).unwrap();

        let mut cfg = crate::config::Config::default();
        cfg.inference.provider = "sarvam".into();
        cfg.inference.model = "saaras:v4".into();

        let output_sarvam = format_model_list_with_models_dir(&cfg, Some(&models_dir));
        assert!(output_sarvam.contains("* saaras:v4 (Provider: Sarvam AI Cloud) [active]"));
        assert!(output_sarvam.contains("* moonshine-tiny (Provider: Local ONNX) [installed]"));
        assert!(output_sarvam.contains("* whisper-tiny (Provider: Local ONNX) [not downloaded]"));
        assert!(!output_sarvam.contains("moonshine-tiny (Provider: Local ONNX) [active]"));

        // Switch active config to local moonshine-tiny
        cfg.inference.provider = "local".into();
        cfg.inference.model = "moonshine-tiny".into();
        let output_moonshine = format_model_list_with_models_dir(&cfg, Some(&models_dir));
        assert!(output_moonshine.contains("* moonshine-tiny (Provider: Local ONNX) [active] [installed]"));
        assert!(!output_moonshine.contains("saaras:v4 (Provider: Sarvam AI Cloud) [active]"));

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

        // 2. saaras:v4 or sarvam -> provider sarvam, model saaras:v4, no notice
        let res_sarvam = handle_model_use_command_internal("sarvam", &config_file, Some(&models_dir))
            .expect("use sarvam");
        assert_eq!(res_sarvam.provider, "sarvam");
        assert_eq!(res_sarvam.model, "saaras:v4");
        assert_eq!(res_sarvam.notice, None);
        assert_eq!(
            res_sarvam.confirmation,
            "Active model set to 'saaras:v4' in config.toml. Restart daemon to apply."
        );
        let cfg = crate::config::Config::load_from_path(&config_file);
        assert_eq!(cfg.inference.provider, "sarvam");
        assert_eq!(cfg.inference.model, "saaras:v4");

        // 3. moonshine-tiny when weights don't exist -> notice printed, config updated
        let res_moonshine = handle_model_use_command_internal("moonshine-tiny", &config_file, Some(&models_dir))
            .expect("use moonshine-tiny");
        assert_eq!(res_moonshine.provider, "local");
        assert_eq!(res_moonshine.model, "moonshine-tiny");
        assert_eq!(
            res_moonshine.notice,
            Some("Model weights not found. Run `speechyyy model download -m moonshine-tiny`".into())
        );
        assert_eq!(
            res_moonshine.confirmation,
            "Active model set to 'moonshine-tiny' in config.toml. Restart daemon to apply."
        );
        let cfg = crate::config::Config::load_from_path(&config_file);
        assert_eq!(cfg.inference.provider, "local");
        assert_eq!(cfg.inference.model, "moonshine-tiny");

        // 4. whisper-tiny when weights exist -> no notice, config updated
        fs::create_dir_all(models_dir.join("whisper-tiny")).unwrap();
        let res_whisper = handle_model_use_command_internal("whisper-tiny", &config_file, Some(&models_dir))
            .expect("use whisper-tiny");
        assert_eq!(res_whisper.provider, "local");
        assert_eq!(res_whisper.model, "whisper-tiny");
        assert_eq!(res_whisper.notice, None);
        assert_eq!(
            res_whisper.confirmation,
            "Active model set to 'whisper-tiny' in config.toml. Restart daemon to apply."
        );
        let cfg = crate::config::Config::load_from_path(&config_file);
        assert_eq!(cfg.inference.provider, "local");
        assert_eq!(cfg.inference.model, "whisper-tiny");

        let _ = fs::remove_dir_all(temp_dir);
    }
}
