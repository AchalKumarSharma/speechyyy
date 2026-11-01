use clap::Parser;
use speechyyy::cli::{Cli, Command};
use speechyyy::config::Config;
use speechyyy::inference::create_inference_provider;

#[test]
fn test_config_defaults() {
    let cfg = Config::default();

    // Default provider must be "local" (Sherpa-ONNX offline Moonshine)
    assert_eq!(
        cfg.inference.provider, "local",
        "Default speech inference provider must be 'local'"
    );

    // Default Sarvam mode must be "translit" (Romanized Hinglish transliteration)
    assert_eq!(
        cfg.inference.mode, "translit",
        "Default Sarvam mode must be 'translit'"
    );

    assert_eq!(cfg.inference.model, "saaras:v4");

    // Default text injection mode must be "typewriter"
    assert_eq!(
        cfg.injector.method, "typewriter",
        "Default injector method must be 'typewriter'"
    );

    // Default typing delay must be 15ms
    assert_eq!(
        cfg.pacing_delay_ms, 15,
        "Default typing pacing delay must be 15ms"
    );
}

#[test]
fn test_config_deserialization_from_toml() {
    let toml_data = r#"
hotkey = "RightAlt"
pacing_delay_ms = 15

[audio]
sample_rate = 16000
vad_threshold = 0.005
min_duration_ms = 200

[inference]
provider = "sarvam"
sarvam_api_key = "sk_test_sarvam_file_key_123"
sarvam_mode = "codemix"
sarvam_model = "saaras:v4"

[injector]
injection_mode = "paste"
typing_delay_ms = 20
"#;

    let cfg: Config = toml::from_str(toml_data).expect("deserialize config from TOML");

    assert_eq!(cfg.inference.provider, "sarvam");
    assert_eq!(
        cfg.inference.resolve_api_key(),
        Some("sk_test_sarvam_file_key_123".into())
    );
    assert_eq!(cfg.inference.mode, "codemix");
    assert_eq!(cfg.inference.model, "saaras:v4");
    assert_eq!(cfg.injector.method, "paste");
    assert_eq!(cfg.pacing_delay_ms, 20);
}

#[test]
fn test_sarvam_api_key_env_var_fallback_and_precedence() {
    let toml_without_key = r#"
[inference]
provider = "sarvam"
mode = "translit"
"#;
    let cfg: Config = toml::from_str(toml_without_key).expect("parse toml without key");

    // 1. Neither config nor env var -> None
    std::env::remove_var("SARVAM_API_KEY");
    assert_eq!(cfg.inference.resolve_api_key(), None);

    // 2. Env var fallback
    std::env::set_var("SARVAM_API_KEY", "env_secret_key_456");
    assert_eq!(
        cfg.inference.resolve_api_key(),
        Some("env_secret_key_456".into())
    );

    // 3. File key overrides env var
    let toml_with_key = r#"
[inference]
provider = "sarvam"
api_key = "explicit_file_key_789"
"#;
    let cfg_with_key: Config = toml::from_str(toml_with_key).expect("parse toml with key");
    assert_eq!(
        cfg_with_key.inference.resolve_api_key(),
        Some("explicit_file_key_789".into())
    );

    std::env::remove_var("SARVAM_API_KEY");
}

#[test]
fn test_cli_flags_parsing_on_start() {
    let cli = Cli::try_parse_from([
        "speechyyy",
        "start",
        "--provider",
        "sarvam",
        "--mode",
        "codemix",
        "--injection",
        "paste",
        "--typing-delay",
        "25",
    ])
    .expect("parse start with all flags");

    match cli.command {
        Command::Start {
            foreground,
            provider,
            mode,
            injection,
            typing_delay,
        } => {
            assert!(!foreground);
            assert_eq!(provider, Some("sarvam".into()));
            assert_eq!(mode, Some("codemix".into()));
            assert_eq!(injection, Some("paste".into()));
            assert_eq!(typing_delay, Some(25));
        }
        other => panic!("Expected Command::Start, got {:?}", other),
    }
}

#[test]
fn test_cli_flags_override_config_toml_precedence() {
    let toml_data = r#"
[inference]
provider = "local"
mode = "transcribe"

[injector]
method = "paste"
"#;
    let mut cfg: Config = toml::from_str(toml_data).expect("parse initial config");
    assert_eq!(cfg.inference.provider, "local");
    assert_eq!(cfg.inference.mode, "transcribe");
    assert_eq!(cfg.injector.method, "paste");

    // CLI overrides must take precedence over config.toml
    cfg.apply_cli_overrides(
        Some("sarvam"),
        Some("translit"),
        Some("typewriter"),
        Some(35),
    );

    assert_eq!(
        cfg.inference.provider, "sarvam",
        "CLI --provider must override config.toml"
    );
    assert_eq!(
        cfg.inference.mode, "translit",
        "CLI --mode must override config.toml"
    );
    assert_eq!(
        cfg.injector.method, "typewriter",
        "CLI --injection must override config.toml"
    );
    assert_eq!(
        cfg.pacing_delay_ms, 35,
        "CLI --typing-delay must override config.toml"
    );
}

#[test]
fn test_missing_sarvam_api_key_returns_explicit_actionable_error() {
    std::env::remove_var("SARVAM_API_KEY");

    let toml_data = r#"
[inference]
provider = "sarvam"
api_key = ""
"#;
    let cfg: Config = toml::from_str(toml_data).expect("parse config");

    let result = create_inference_provider(&cfg.inference);
    assert!(
        result.is_err(),
        "Expected missing API key to fail provider creation"
    );

    let err_msg = result.err().unwrap();
    assert!(
        err_msg.contains("SARVAM_API_KEY"),
        "Error message must mention SARVAM_API_KEY, got: {}",
        err_msg
    );
    assert!(
        err_msg.contains("speechyyy doctor"),
        "Error message must provide actionable doctor command, got: {}",
        err_msg
    );
}
