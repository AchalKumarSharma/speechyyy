use clap::Parser;
use speechyyy::cli::{Cli, Command, ModelCommands};
use speechyyy::config::Config;
use speechyyy::daemon::handle_uninstall_command_internal;
use speechyyy::ipc::RuntimePaths;
use speechyyy::model::{
    format_model_list_with_models_dir, handle_model_remove_internal,
    handle_model_use_command_internal, LocalModelType,
};
use std::fs;

#[test]
fn test_cli_parsing_model_subcommands() {
    let cli_list = Cli::try_parse_from(["speechyyy", "model", "list"]).expect("parse list");
    assert_eq!(
        cli_list.command,
        Command::Models {
            action: Some(ModelCommands::List),
        }
    );

    let cli_use = Cli::try_parse_from(["speechyyy", "model", "use", "parakeet-tdt-v3"])
        .expect("parse use");
    assert_eq!(
        cli_use.command,
        Command::Models {
            action: Some(ModelCommands::Use {
                model: "parakeet-tdt-v3".to_string(),
            }),
        }
    );

    let cli_models_bare = Cli::try_parse_from(["speechyyy", "models"]).expect("parse models");
    assert_eq!(cli_models_bare.command, Command::Models { action: None });

    let cli_model_bare = Cli::try_parse_from(["speechyyy", "model"]).expect("parse bare model");
    assert_eq!(cli_model_bare.command, Command::Models { action: None });

    let cli_download_pos =
        Cli::try_parse_from(["speechyyy", "download", "parakeet-tdt-v3"]).expect("parse download");
    assert_eq!(
        cli_download_pos.command,
        Command::Download {
            model: Some(LocalModelType::ParakeetTdtV3),
            model_flag: None,
        }
    );

    let cli_download_flag =
        Cli::try_parse_from(["speechyyy", "download", "-m", "whisper-small"]).expect("parse download -m");
    assert_eq!(
        cli_download_flag.command,
        Command::Download {
            model: None,
            model_flag: Some(LocalModelType::WhisperSmall),
        }
    );

    let cli_use_top =
        Cli::try_parse_from(["speechyyy", "use", "parakeet-tdt-v3"]).expect("parse use top");
    assert_eq!(
        cli_use_top.command,
        Command::Use {
            model: "parakeet-tdt-v3".to_string(),
        }
    );

    let cli_unlearn_key =
        Cli::try_parse_from(["speechyyy", "unlearn", "Speechy"]).expect("parse unlearn key");
    assert_eq!(
        cli_unlearn_key.command,
        Command::Unlearn {
            key: Some("Speechy".to_string()),
        }
    );

    let cli_unlearn_bare =
        Cli::try_parse_from(["speechyyy", "unlearn"]).expect("parse unlearn bare");
    assert_eq!(cli_unlearn_bare.command, Command::Unlearn { key: None });
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

#[test]
fn test_unlearn_command_lifecycle() {
    use speechyyy::text::{handle_unlearn_command_internal, LearningDictionary};

    let temp_dir = std::env::temp_dir().join(format!("test_unlearn_lifecycle_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    let dict_path = temp_dir.join("dictionary.toml");

    let mut dict = LearningDictionary::load_from_path(&dict_path).unwrap();
    dict.add_rule_and_save("n8n", "N8N").unwrap();
    assert_eq!(dict.len(), 1);

    // 1. Remove existing rule
    let res = handle_unlearn_command_internal(Some("n8n"), Some(&dict_path));
    assert!(res.is_ok());

    let reloaded = LearningDictionary::load_from_path(&dict_path).unwrap();
    assert_eq!(reloaded.len(), 0);

    // 2. Remove rule that does not exist
    let res_missing = handle_unlearn_command_internal(Some("n8n"), Some(&dict_path));
    assert!(res_missing.is_ok());

    // 3. Remove with missing key argument
    let res_none = handle_unlearn_command_internal(None, Some(&dict_path));
    assert!(res_none.is_ok());

    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn test_model_remove_lifecycle() {
    let temp_dir =
        std::env::temp_dir().join(format!("test_model_remove_lifecycle_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    let models_dir = temp_dir.join("models");
    let config_path = temp_dir.join("config.toml");

    let parakeet_dir = models_dir.join("parakeet-tdt-v3");
    fs::create_dir_all(&parakeet_dir).unwrap();
    fs::write(parakeet_dir.join("encoder.int8.onnx"), vec![0u8; 1024 * 1024]).unwrap();

    Config::update_active_model_in_file(&config_path, "local", "parakeet-tdt-v3").unwrap();

    // 1. Remove active model
    let res = handle_model_remove_internal("parakeet-tdt-v3", Some(&models_dir), Some(&config_path));
    assert!(res.is_ok());
    assert!(!parakeet_dir.exists());

    // 2. Remove already removed / non-existent model
    let res_missing =
        handle_model_remove_internal("parakeet-tdt-v3", Some(&models_dir), Some(&config_path));
    assert!(res_missing.is_ok());

    // 3. Remove using alias
    let turbo_dir = models_dir.join("whisper-large-v3-turbo");
    fs::create_dir_all(&turbo_dir).unwrap();
    fs::write(turbo_dir.join("turbo-encoder.int8.onnx"), b"turbo").unwrap();
    assert!(turbo_dir.exists());

    let res_alias =
        handle_model_remove_internal("whisper-turbo", Some(&models_dir), Some(&config_path));
    assert!(res_alias.is_ok());
    assert!(!turbo_dir.exists());

    let _ = fs::remove_dir_all(temp_dir);
}

#[test]
fn test_uninstall_command_lifecycle() {
    let temp_dir =
        std::env::temp_dir().join(format!("test_uninstall_lifecycle_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);

    let home_dir = temp_dir.join("mock_home");
    let config_dir = home_dir.join(".config").join("speechyyy");
    let share_dir = home_dir.join(".local").join("share").join("speechyyy");
    let cargo_bin = home_dir.join(".cargo").join("bin").join("speechyyy");
    let local_bin = home_dir.join(".local").join("bin").join("speechyyy");

    fs::create_dir_all(&config_dir).unwrap();
    fs::write(config_dir.join("config.toml"), b"# config").unwrap();
    fs::create_dir_all(&share_dir).unwrap();
    fs::write(share_dir.join("dummy.dat"), b"data").unwrap();
    fs::create_dir_all(cargo_bin.parent().unwrap()).unwrap();
    fs::write(&cargo_bin, b"binary").unwrap();
    fs::create_dir_all(local_bin.parent().unwrap()).unwrap();
    fs::write(&local_bin, b"binary").unwrap();

    let paths = RuntimePaths::with_dir(temp_dir.join("runtime"));

    // 1. Uninstall without purge: nothing is deleted
    let res_no_purge = handle_uninstall_command_internal(false, &paths, Some(&home_dir));
    assert!(res_no_purge.is_ok());
    assert!(config_dir.exists());
    assert!(share_dir.exists());
    assert!(cargo_bin.exists());
    assert!(local_bin.exists());

    // 2. Uninstall with purge: everything is purged
    let res_purge = handle_uninstall_command_internal(true, &paths, Some(&home_dir));
    assert!(res_purge.is_ok());
    assert!(!config_dir.exists());
    assert!(!share_dir.exists());
    assert!(!cargo_bin.exists());
    assert!(!local_bin.exists());

    let _ = fs::remove_dir_all(temp_dir);
}
