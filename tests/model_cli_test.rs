use clap::Parser;
use speechyyy::cli::{Cli, Command, ModelCommands};
use speechyyy::config::Config;
use speechyyy::model::{
    format_model_list_with_models_dir, handle_model_use_command_internal, LocalModelType,
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

    let cli_use = Cli::try_parse_from(["speechyyy", "model", "use", "parakeet-tdt-v3"])
        .expect("parse use");
    assert_eq!(
        cli_use.command,
        Command::Model {
            action: ModelCommands::Use {
                model: "parakeet-tdt-v3".to_string(),
            },
        }
    );
}

#[test]
fn test_model_list_table_format_and_hint() {
    let temp_dir = std::env::temp_dir().join(format!("test_model_list_table_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    let models_dir = temp_dir.join("models");
    let parakeet_dir = models_dir.join("parakeet-tdt-v3");
    fs::create_dir_all(&parakeet_dir).unwrap();
    for group in LocalModelType::ParakeetTdtV3.required_file_groups() {
        fs::write(parakeet_dir.join(group[0]), b"weights").unwrap();
    }

    let mut cfg = Config::default();
    cfg.inference.provider = "sarvam".to_string();
    cfg.inference.model = "saaras:v4".to_string();

    let list_output = format_model_list_with_models_dir(&cfg, Some(&models_dir));

    // Verify table columns
    assert!(list_output.contains("NAME"));
    assert!(list_output.contains("PROVIDER"));
    assert!(list_output.contains("STATUS"));
    assert!(list_output.contains("ACTIVE"));

    // Verify curated models
    assert!(list_output.contains("saaras:v4"));
    assert!(list_output.contains("parakeet-tdt-v3"));
    assert!(list_output.contains("whisper-small"));
    assert!(list_output.contains("whisper-large-v3-turbo"));
    assert!(list_output.contains("moonshine-base"));

    // Verify statuses
    assert!(list_output.contains("Available"));
    assert!(list_output.contains("Installed"));
    assert!(list_output.contains("Not Downloaded"));

    // Verify hint
    assert!(list_output.contains("Tip: For custom ONNX models, set provider = 'custom' in ~/.config/speechyyy/config.toml"));

    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn test_model_use_curated_presets() {
    let temp_dir = std::env::temp_dir().join(format!("test_model_use_presets_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    let config_file = temp_dir.join("config.toml");
    let models_dir = temp_dir.join("models");

    // 1. sarvam alias -> saaras:v4
    let res = handle_model_use_command_internal("sarvam", &config_file, Some(&models_dir))
        .expect("use sarvam");
    assert_eq!(res.provider, "sarvam");
    assert_eq!(res.model, "saaras:v4");
    assert_eq!(res.notice, None);
    assert_eq!(
        res.confirmation,
        "Active model set to 'saaras:v4' in config.toml. Restart daemon to apply."
    );

    // 2. saaras alias -> saaras:v4
    let res = handle_model_use_command_internal("saaras", &config_file, Some(&models_dir))
        .expect("use saaras");
    assert_eq!(res.provider, "sarvam");
    assert_eq!(res.model, "saaras:v4");

    // 3. parakeet-v3 alias -> parakeet-tdt-v3 with missing weights
    let res = handle_model_use_command_internal("parakeet-v3", &config_file, Some(&models_dir))
        .expect("use parakeet-v3");
    assert_eq!(res.provider, "local");
    assert_eq!(res.model, "parakeet-tdt-v3");
    assert_eq!(
        res.notice,
        Some("Weights not found. Run `speechyyy model download -m parakeet-tdt-v3`".into())
    );
    assert_eq!(
        res.confirmation,
        "Active model set to 'parakeet-tdt-v3' in config.toml. Restart daemon to apply."
    );

    // 4. whisper-small
    let res = handle_model_use_command_internal("whisper-small", &config_file, Some(&models_dir))
        .expect("use whisper-small");
    assert_eq!(res.provider, "local");
    assert_eq!(res.model, "whisper-small");
    assert_eq!(
        res.notice,
        Some("Weights not found. Run `speechyyy model download -m whisper-small`".into())
    );

    // 5. whisper-turbo alias -> whisper-large-v3-turbo
    let res = handle_model_use_command_internal("whisper-turbo", &config_file, Some(&models_dir))
        .expect("use whisper-turbo");
    assert_eq!(res.provider, "local");
    assert_eq!(res.model, "whisper-large-v3-turbo");
    assert_eq!(
        res.notice,
        Some("Weights not found. Run `speechyyy model download -m whisper-large-v3-turbo`".into())
    );

    // 6. moonshine-base
    let res = handle_model_use_command_internal("moonshine-base", &config_file, Some(&models_dir))
        .expect("use moonshine-base");
    assert_eq!(res.provider, "local");
    assert_eq!(res.model, "moonshine-base");
    assert_eq!(
        res.notice,
        Some("Weights not found. Run `speechyyy model download -m moonshine-base`".into())
    );

    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn test_model_use_unsupported_model() {
    let temp_dir = std::env::temp_dir().join(format!("test_model_use_unsupported_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    let config_file = temp_dir.join("config.toml");
    let models_dir = temp_dir.join("models");

    let res = handle_model_use_command_internal("whisper-tiny", &config_file, Some(&models_dir));
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(err.contains("Unsupported model 'whisper-tiny'"));
    assert!(err.contains("parakeet-tdt-v3"));

    let _ = fs::remove_dir_all(temp_dir);
}
