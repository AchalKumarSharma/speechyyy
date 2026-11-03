use clap::Parser;
use speechyyy::cli::{Cli, Command, ModelCommands};
use speechyyy::config::Config;
use speechyyy::model::{
    format_model_list_with_models_dir, handle_model_use_command_internal,
};
use std::fs;

#[test]
fn test_cli_parsing_model_subcommands() {
    let cli_list = Cli::try_parse_from(["speechyyy", "model", "list"]).expect("parse list");
    assert_eq!(
        cli_list.command,
        Command::Model {
            action: ModelCommands::List,
        }
    );

    let cli_use = Cli::try_parse_from(["speechyyy", "model", "use", "moonshine-tiny"])
        .expect("parse use");
    assert_eq!(
        cli_use.command,
        Command::Model {
            action: ModelCommands::Use {
                model: "moonshine-tiny".to_string(),
            },
        }
    );
}

#[test]
fn test_model_list_displays_all_supported_models_with_active_and_weights_info() {
    let temp_dir = std::env::temp_dir().join(format!("test_model_list_e2e_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    let models_dir = temp_dir.join("models");
    fs::create_dir_all(models_dir.join("moonshine-tiny")).unwrap();

    let mut cfg = Config::default();
    cfg.inference.provider = "sarvam".to_string();
    cfg.inference.model = "saaras:v4".to_string();

    let list_output = format_model_list_with_models_dir(&cfg, Some(&models_dir));

    // Must display all 3 supported models with providers
    assert!(list_output.contains("saaras:v4 (Provider: Sarvam AI Cloud)"));
    assert!(list_output.contains("moonshine-tiny (Provider: Local ONNX)"));
    assert!(list_output.contains("whisper-tiny (Provider: Local ONNX)"));

    // Check active model indicator
    assert!(list_output.contains("saaras:v4 (Provider: Sarvam AI Cloud) [active]"));
    assert!(!list_output.contains("moonshine-tiny (Provider: Local ONNX) [active]"));

    // Check local weights indicators
    assert!(list_output.contains("moonshine-tiny (Provider: Local ONNX) [installed]"));
    assert!(list_output.contains("whisper-tiny (Provider: Local ONNX) [not downloaded]"));

    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn test_model_use_sarvam_and_saaras() {
    let temp_dir = std::env::temp_dir().join(format!("test_model_use_sarvam_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    let config_file = temp_dir.join("config.toml");
    let models_dir = temp_dir.join("models");

    // 1. Using "sarvam" alias
    let res = handle_model_use_command_internal("sarvam", &config_file, Some(&models_dir))
        .expect("use sarvam");
    assert_eq!(res.provider, "sarvam");
    assert_eq!(res.model, "saaras:v4");
    assert_eq!(res.notice, None);
    assert_eq!(
        res.confirmation,
        "Active model set to 'saaras:v4' in config.toml. Restart daemon to apply."
    );

    let loaded = Config::load_from_path(&config_file);
    assert_eq!(loaded.inference.provider, "sarvam");
    assert_eq!(loaded.inference.model, "saaras:v4");

    // 2. Using exact model identifier "saaras:v4"
    let res2 = handle_model_use_command_internal("saaras:v4", &config_file, Some(&models_dir))
        .expect("use saaras:v4");
    assert_eq!(res2.provider, "sarvam");
    assert_eq!(res2.model, "saaras:v4");
    assert_eq!(res2.notice, None);
    assert_eq!(
        res2.confirmation,
        "Active model set to 'saaras:v4' in config.toml. Restart daemon to apply."
    );

    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn test_model_use_moonshine_tiny_weights_missing_notice_and_persistence() {
    let temp_dir = std::env::temp_dir().join(format!("test_model_use_moonshine_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    let config_file = temp_dir.join("config.toml");
    let models_dir = temp_dir.join("models");

    let res = handle_model_use_command_internal("moonshine-tiny", &config_file, Some(&models_dir))
        .expect("use moonshine-tiny");
    assert_eq!(res.provider, "local");
    assert_eq!(res.model, "moonshine-tiny");
    assert_eq!(
        res.notice,
        Some("Model weights not found. Run `speechyyy model download -m moonshine-tiny`".into())
    );
    assert_eq!(
        res.confirmation,
        "Active model set to 'moonshine-tiny' in config.toml. Restart daemon to apply."
    );

    let loaded = Config::load_from_path(&config_file);
    assert_eq!(loaded.inference.provider, "local");
    assert_eq!(loaded.inference.model, "moonshine-tiny");

    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn test_model_use_whisper_tiny_weights_missing_notice_and_persistence() {
    let temp_dir = std::env::temp_dir().join(format!("test_model_use_whisper_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    let config_file = temp_dir.join("config.toml");
    let models_dir = temp_dir.join("models");

    let res = handle_model_use_command_internal("whisper-tiny", &config_file, Some(&models_dir))
        .expect("use whisper-tiny");
    assert_eq!(res.provider, "local");
    assert_eq!(res.model, "whisper-tiny");
    assert_eq!(
        res.notice,
        Some("Model weights not found. Run `speechyyy model download -m whisper-tiny`".into())
    );
    assert_eq!(
        res.confirmation,
        "Active model set to 'whisper-tiny' in config.toml. Restart daemon to apply."
    );

    let loaded = Config::load_from_path(&config_file);
    assert_eq!(loaded.inference.provider, "local");
    assert_eq!(loaded.inference.model, "whisper-tiny");

    // When weights directory exists:
    fs::create_dir_all(models_dir.join("whisper-tiny")).unwrap();
    let res_with_weights = handle_model_use_command_internal("whisper-tiny", &config_file, Some(&models_dir))
        .expect("use whisper-tiny with weights");
    assert_eq!(res_with_weights.notice, None);

    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn test_model_use_unsupported_model() {
    let temp_dir = std::env::temp_dir().join(format!("test_model_use_unsupported_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    let config_file = temp_dir.join("config.toml");
    let models_dir = temp_dir.join("models");

    let res = handle_model_use_command_internal("non-existent-model", &config_file, Some(&models_dir));
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(err.contains("Unsupported model 'non-existent-model'"));
    assert!(!config_file.exists());

    let _ = fs::remove_dir_all(temp_dir);
}
