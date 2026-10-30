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
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("speechyyy")
            .join("models")
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
}
