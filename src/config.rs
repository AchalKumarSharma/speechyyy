use crate::engine::{Modifier, TargetChord};
use evdev::KeyCode;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

fn default_hotkey() -> String {
    "RightAlt".to_string()
}

fn default_grab_key() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InputConfig {
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    #[serde(default = "default_grab_key")]
    pub grab_key: bool,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            hotkey: default_hotkey(),
            grab_key: default_grab_key(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputError {
    UnknownKey(String),
}

impl std::fmt::Display for InputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InputError::UnknownKey(k) => write!(f, "Unknown or unsupported key '{}'", k),
        }
    }
}

impl std::error::Error for InputError {}

pub fn resolve_hotkey(name: &str) -> Result<KeyCode, InputError> {
    match name.to_lowercase().as_str() {
        "space" => Ok(KeyCode::KEY_SPACE),
        "rightalt" | "right_alt" | "alt_r" => Ok(KeyCode::KEY_RIGHTALT),
        "leftalt" | "left_alt" | "alt_l" => Ok(KeyCode::KEY_LEFTALT),
        "capslock" | "caps_lock" | "caps" => Ok(KeyCode::KEY_CAPSLOCK),
        "scrolllock" | "scroll_lock" => Ok(KeyCode::KEY_SCROLLLOCK),
        "pause" | "pausebreak" | "pause_break" => Ok(KeyCode::KEY_PAUSE),
        "rightctrl" | "right_ctrl" | "ctrl_r" => Ok(KeyCode::KEY_RIGHTCTRL),
        "leftctrl" | "left_ctrl" | "ctrl_l" => Ok(KeyCode::KEY_LEFTCTRL),
        "rightshift" | "right_shift" | "shift_r" => Ok(KeyCode::KEY_RIGHTSHIFT),
        "leftshift" | "left_shift" | "shift_l" => Ok(KeyCode::KEY_LEFTSHIFT),
        "enter" | "return" => Ok(KeyCode::KEY_ENTER),
        "tab" => Ok(KeyCode::KEY_TAB),
        "f1" => Ok(KeyCode::KEY_F1),
        "f2" => Ok(KeyCode::KEY_F2),
        "f3" => Ok(KeyCode::KEY_F3),
        "f4" => Ok(KeyCode::KEY_F4),
        "f5" => Ok(KeyCode::KEY_F5),
        "f6" => Ok(KeyCode::KEY_F6),
        "f7" => Ok(KeyCode::KEY_F7),
        "f8" => Ok(KeyCode::KEY_F8),
        "f9" => Ok(KeyCode::KEY_F9),
        "f10" => Ok(KeyCode::KEY_F10),
        "f11" => Ok(KeyCode::KEY_F11),
        "f12" => Ok(KeyCode::KEY_F12),
        _ => Err(InputError::UnknownKey(name.to_string())),
    }
}

fn default_pacing_ms() -> u64 {
    15
}

fn default_injector_method() -> String {
    "typewriter".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InjectorConfig {
    #[serde(
        default = "default_injector_method",
        alias = "injection_mode",
        alias = "mode"
    )]
    pub method: String,
    #[serde(default = "default_pacing_ms", alias = "typing_delay_ms")]
    pub pacing_delay_ms: u64,
}

impl Default for InjectorConfig {
    fn default() -> Self {
        Self {
            method: default_injector_method(),
            pacing_delay_ms: default_pacing_ms(),
        }
    }
}

fn default_sample_rate() -> u32 {
    16000
}

fn default_vad_threshold() -> f32 {
    0.005
}

fn default_min_duration_ms() -> u64 {
    200
}

fn default_provider() -> String {
    "local".to_string()
}

fn default_model() -> String {
    "saaras:v4".to_string()
}

fn default_language_code() -> String {
    "unknown".to_string()
}

fn default_mode() -> String {
    "translit".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomModelConfig {
    pub r#type: String, // "whisper", "transducer", "moonshine", "sensevoice"
    pub encoder: PathBuf,
    pub decoder: PathBuf,
    pub joiner: Option<PathBuf>,
    pub tokens: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InferenceConfig {
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default, alias = "sarvam_api_key")]
    pub api_key: String,
    #[serde(default = "default_model", alias = "sarvam_model")]
    pub model: String,
    #[serde(default = "default_language_code")]
    pub language_code: String,
    #[serde(default = "default_mode", alias = "sarvam_mode")]
    pub mode: String,
    #[serde(default)]
    pub custom: Option<CustomModelConfig>,
}

impl Default for InferenceConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            api_key: String::new(),
            model: default_model(),
            language_code: default_language_code(),
            mode: default_mode(),
            custom: None,
        }
    }
}

impl InferenceConfig {
    pub fn resolve_api_key(&self) -> Option<String> {
        if !self.api_key.trim().is_empty() {
            Some(self.api_key.trim().to_string())
        } else {
            std::env::var("SARVAM_API_KEY")
                .ok()
                .filter(|k| !k.trim().is_empty())
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioConfig {
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,
    #[serde(default = "default_vad_threshold")]
    pub vad_threshold: f32,
    #[serde(default = "default_min_duration_ms")]
    pub min_duration_ms: u64,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: default_sample_rate(),
            vad_threshold: default_vad_threshold(),
            min_duration_ms: default_min_duration_ms(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct UiConfig {
    #[serde(default = "default_nickname")]
    pub nickname: String,
}

fn default_nickname() -> String {
    "babe".to_string()
}

fn default_ui_config() -> UiConfig {
    UiConfig {
        nickname: default_nickname(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(from = "RawConfig")]
pub struct Config {
    pub hotkey: String,
    pub pacing_delay_ms: u64,
    pub input: InputConfig,
    pub audio: AudioConfig,
    pub inference: InferenceConfig,
    pub injector: InjectorConfig,
    pub ui: UiConfig,
}

#[derive(Debug, Clone, Deserialize)]
struct RawConfig {
    #[serde(default)]
    input: Option<InputConfig>,
    #[serde(default)]
    hotkey: Option<String>,
    #[serde(default)]
    pacing_delay_ms: Option<u64>,
    #[serde(default)]
    typing_delay_ms: Option<u64>,
    #[serde(default)]
    audio: AudioConfig,
    #[serde(default)]
    inference: InferenceConfig,
    #[serde(default)]
    injector: InjectorConfig,
    #[serde(default = "default_ui_config")]
    ui: UiConfig,

    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    sarvam_api_key: Option<String>,
    #[serde(default)]
    sarvam_mode: Option<String>,
    #[serde(default)]
    sarvam_model: Option<String>,
    #[serde(default)]
    injection_mode: Option<String>,
}

impl From<RawConfig> for Config {
    fn from(raw: RawConfig) -> Self {
        let mut inference = raw.inference;
        if let Some(p) = raw.provider {
            inference.provider = p;
        }
        if let Some(k) = raw.sarvam_api_key {
            inference.api_key = k;
        }
        if let Some(m) = raw.sarvam_mode {
            inference.mode = m;
        }
        if let Some(mdl) = raw.sarvam_model {
            inference.model = mdl;
        }

        let mut injector = raw.injector;
        if let Some(inj) = raw.injection_mode {
            injector.method = inj;
        }

        let delay = raw
            .typing_delay_ms
            .or_else(|| {
                if injector.pacing_delay_ms != default_pacing_ms() {
                    Some(injector.pacing_delay_ms)
                } else {
                    raw.pacing_delay_ms
                }
            })
            .or(raw.pacing_delay_ms)
            .unwrap_or_else(default_pacing_ms);

        injector.pacing_delay_ms = delay;

        let mut ui = raw.ui;
        if ui.nickname.trim().is_empty() {
            ui.nickname = default_nickname();
        }

        let mut input = raw.input.unwrap_or_default();
        if let Some(h) = raw.hotkey {
            input.hotkey = h;
        }
        let hotkey = input.hotkey.clone();

        Config {
            hotkey,
            pacing_delay_ms: delay,
            input,
            audio: raw.audio,
            inference,
            injector,
            ui,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: default_hotkey(),
            pacing_delay_ms: default_pacing_ms(),
            input: InputConfig::default(),
            audio: AudioConfig::default(),
            inference: InferenceConfig::default(),
            injector: InjectorConfig::default(),
            ui: UiConfig {
                nickname: default_nickname(),
            },
        }
    }
}

impl Config {
    pub fn nickname(&self) -> &str {
        let n = self.ui.nickname.trim();
        if n.is_empty() {
            "babe"
        } else {
            n
        }
    }

    pub fn config_path() -> PathBuf {
        let base = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
                PathBuf::from(home).join(".config")
            });
        base.join("speechyyy").join("config.toml")
    }

    pub fn load_from_path(path: &Path) -> Self {
        if path.exists() {
            if let Ok(content) = fs::read_to_string(path) {
                if let Ok(config) = toml::from_str(&content) {
                    return config;
                }
            }
        }
        Self::default()
    }

    pub fn load() -> Self {
        Self::load_from_path(&Self::config_path())
    }

    pub fn update_active_model_in_file(
        path: &Path,
        provider: &str,
        model: &str,
    ) -> Result<(), String> {
        let mut table = read_toml_table(path)?;

        let inf_val = table
            .entry("inference")
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));

        if let toml::Value::Table(ref mut inf_table) = inf_val {
            inf_table.insert("provider".into(), toml::Value::String(provider.to_string()));
            inf_table.insert("model".into(), toml::Value::String(model.to_string()));
        } else {
            let mut inf_table = toml::Table::new();
            inf_table.insert("provider".into(), toml::Value::String(provider.to_string()));
            inf_table.insert("model".into(), toml::Value::String(model.to_string()));
            table.insert("inference".into(), toml::Value::Table(inf_table));
        }

        table.remove("provider");
        table.remove("sarvam_model");

        write_secure_config(path, &table)
    }

    pub fn deactivate_model_in_file(path: &Path) -> Result<(), String> {
        let mut table = read_toml_table(path)?;

        let inf_val = table
            .entry("inference")
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));

        if let toml::Value::Table(ref mut inf_table) = inf_val {
            inf_table.insert("model".into(), toml::Value::String("".to_string()));
        } else {
            let mut inf_table = toml::Table::new();
            inf_table.insert("model".into(), toml::Value::String("".to_string()));
            table.insert("inference".into(), toml::Value::Table(inf_table));
        }

        table.remove("sarvam_model");

        write_secure_config(path, &table)
    }

    pub fn save_sarvam_credentials_in_file(path: &Path, api_key: &str) -> Result<(), String> {
        let mut table = read_toml_table(path)?;

        let inf_val = table
            .entry("inference")
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));

        if let toml::Value::Table(ref mut inf_table) = inf_val {
            inf_table.insert("api_key".into(), toml::Value::String(api_key.trim().to_string()));
        } else {
            let mut inf_table = toml::Table::new();
            inf_table.insert("api_key".into(), toml::Value::String(api_key.trim().to_string()));
            table.insert("inference".into(), toml::Value::Table(inf_table));
        }

        table.remove("provider");
        table.remove("sarvam_api_key");
        table.remove("sarvam_model");

        write_secure_config(path, &table)
    }

    pub fn to_target_chord(&self) -> TargetChord {
        parse_hotkey(&self.input.hotkey)
            .or_else(|| parse_hotkey(&self.hotkey))
            .unwrap_or_else(TargetChord::right_alt)
    }

    pub fn apply_cli_overrides(
        &mut self,
        provider: Option<&str>,
        mode: Option<&str>,
        injection: Option<&str>,
        typing_delay: Option<u64>,
    ) {
        if let Some(p) = provider {
            self.inference.provider = p.to_string();
        }
        if let Some(m) = mode {
            self.inference.mode = m.to_string();
        }
        if let Some(inj) = injection {
            self.injector.method = inj.to_string();
        }
        if let Some(d) = typing_delay {
            self.pacing_delay_ms = d;
            self.injector.pacing_delay_ms = d;
        }
    }
}

fn read_toml_table(path: &Path) -> Result<toml::Table, String> {
    if path.exists() {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
        match toml::from_str::<toml::Value>(&content) {
            Ok(toml::Value::Table(t)) => Ok(t),
            _ => Ok(toml::Table::new()),
        }
    } else {
        Ok(toml::Table::new())
    }
}

fn write_secure_config(path: &Path, table: &toml::Table) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            format!(
                "Failed to create config directory {}: {}",
                parent.display(),
                e
            )
        })?;
    }

    let serialized = toml::to_string_pretty(&toml::Value::Table(table.clone()))
        .map_err(|e| format!("Failed to serialize TOML: {}", e))?;

    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

        let mut file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| format!("Failed to open {}: {}", path.display(), e))?;

        file.write_all(serialized.as_bytes())
            .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;

        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("Failed to set permissions on {}: {}", path.display(), e))?;
    }

    #[cfg(not(unix))]
    {
        fs::write(path, serialized)
            .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;
    }

    Ok(())
}

fn parse_trigger_key(name: &str) -> Option<KeyCode> {
    resolve_hotkey(name).ok()
}

pub fn parse_hotkey(s: &str) -> Option<TargetChord> {
    let lower = s.to_lowercase();
    if lower == "rightalt" || lower == "right_alt" || lower == "alt_r" {
        return Some(TargetChord::right_alt());
    }

    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
    if parts.len() == 1 {
        parse_trigger_key(parts[0]).map(TargetChord::SingleKey)
    } else {
        let mut modifiers = Vec::new();
        let trigger_key = parse_trigger_key(parts.last()?)?;

        for &mod_str in &parts[..parts.len() - 1] {
            match mod_str.to_lowercase().as_str() {
                "ctrl" | "control" => modifiers.push(Modifier::Ctrl),
                "alt" => modifiers.push(Modifier::Alt),
                "shift" => modifiers.push(Modifier::Shift),
                "super" | "win" | "meta" => modifiers.push(Modifier::Super),
                _ => return None,
            }
        }

        Some(TargetChord::Compound {
            modifiers,
            trigger_key,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hotkey_strings() {
        assert_eq!(parse_hotkey("RightAlt"), Some(TargetChord::right_alt()));
        assert_eq!(parse_hotkey("right_alt"), Some(TargetChord::right_alt()));
        assert_eq!(parse_hotkey("Ctrl+Space"), Some(TargetChord::ctrl_space()));
        assert_eq!(
            parse_hotkey("Alt+Space"),
            Some(TargetChord::Compound {
                modifiers: vec![Modifier::Alt],
                trigger_key: KeyCode::KEY_SPACE
            })
        );
        assert_eq!(
            parse_hotkey("CapsLock"),
            Some(TargetChord::SingleKey(KeyCode::KEY_CAPSLOCK))
        );
        assert_eq!(
            parse_hotkey("F8"),
            Some(TargetChord::SingleKey(KeyCode::KEY_F8))
        );
    }

    #[test]
    fn test_hotkey_resolver_mappings() {
        assert_eq!(resolve_hotkey("RightAlt").unwrap(), KeyCode::KEY_RIGHTALT);
        assert_eq!(resolve_hotkey("LeftAlt").unwrap(), KeyCode::KEY_LEFTALT);
        assert_eq!(resolve_hotkey("CapsLock").unwrap(), KeyCode::KEY_CAPSLOCK);
        assert_eq!(resolve_hotkey("ScrollLock").unwrap(), KeyCode::KEY_SCROLLLOCK);
        assert_eq!(resolve_hotkey("Pause").unwrap(), KeyCode::KEY_PAUSE);
        assert_eq!(resolve_hotkey("Space").unwrap(), KeyCode::KEY_SPACE);
        assert_eq!(resolve_hotkey("F8").unwrap(), KeyCode::KEY_F8);
        assert_eq!(resolve_hotkey("F12").unwrap(), KeyCode::KEY_F12);
        assert!(matches!(
            resolve_hotkey("InvalidKey123"),
            Err(InputError::UnknownKey(_))
        ));
    }

    #[test]
    fn test_input_config_deserialization() {
        let toml_data = r#"
[input]
hotkey = "CapsLock"
grab_key = false
"#;
        let parsed: Config = toml::from_str(toml_data).unwrap();
        assert_eq!(parsed.input.hotkey, "CapsLock");
        assert!(!parsed.input.grab_key);
        assert_eq!(parsed.hotkey, "CapsLock");
        assert_eq!(
            parsed.to_target_chord(),
            TargetChord::SingleKey(KeyCode::KEY_CAPSLOCK)
        );
    }

    #[test]
    fn test_config_defaults_and_parsing() {
        let default_cfg = Config::default();
        assert_eq!(default_cfg.hotkey, "RightAlt");
        assert_eq!(default_cfg.pacing_delay_ms, 15);
        assert_eq!(default_cfg.injector.method, "typewriter");
        assert_eq!(default_cfg.to_target_chord(), TargetChord::right_alt());

        let toml_data = r#"
hotkey = "Ctrl+Space"
pacing_delay_ms = 5

[audio]
sample_rate = 16000
vad_threshold = 0.008
min_duration_ms = 300

[inference]
provider = "sarvam"
api_key = "test_sarvam_key_123"
model = "saaras:v4"
language_code = "hi-IN"
mode = "codemix"
"#;
        let parsed: Config = toml::from_str(toml_data).unwrap();
        assert_eq!(parsed.hotkey, "Ctrl+Space");
        assert_eq!(parsed.pacing_delay_ms, 5);
        assert_eq!(parsed.to_target_chord(), TargetChord::ctrl_space());
        assert_eq!(parsed.audio.sample_rate, 16000);
        assert!((parsed.audio.vad_threshold - 0.008).abs() < 1e-6);
        assert_eq!(parsed.audio.min_duration_ms, 300);
        assert_eq!(parsed.inference.provider, "sarvam");
        assert_eq!(
            parsed.inference.resolve_api_key(),
            Some("test_sarvam_key_123".into())
        );
        assert_eq!(parsed.inference.model, "saaras:v4");
        assert_eq!(parsed.inference.language_code, "hi-IN");
        assert_eq!(parsed.inference.mode, "codemix");
    }

    #[test]
    fn test_inference_config_api_key_env_var_fallback() {
        let mut cfg = InferenceConfig::default();
        assert_eq!(cfg.provider, "local");
        assert_eq!(cfg.model, "saaras:v4");
        assert_eq!(cfg.language_code, "unknown");

        std::env::remove_var("SARVAM_API_KEY");
        assert_eq!(cfg.resolve_api_key(), None);

        std::env::set_var("SARVAM_API_KEY", "env_key_secret_999");
        assert_eq!(cfg.resolve_api_key(), Some("env_key_secret_999".into()));

        // Explicit config field takes precedence over env var
        cfg.api_key = "explicit_config_key".into();
        assert_eq!(cfg.resolve_api_key(), Some("explicit_config_key".into()));

        std::env::remove_var("SARVAM_API_KEY");
    }

    #[test]
    fn test_custom_model_config_deserialization() {
        let toml_data = r#"
[inference]
provider = "custom"

[inference.custom]
type = "transducer"
encoder = "/path/to/encoder.onnx"
decoder = "/path/to/decoder.onnx"
joiner = "/path/to/joiner.onnx"
tokens = "/path/to/tokens.txt"
"#;
        let cfg: Config = toml::from_str(toml_data).expect("parse custom config");
        assert_eq!(cfg.inference.provider, "custom");
        let custom = cfg.inference.custom.expect("custom config present");
        assert_eq!(custom.r#type, "transducer");
        assert_eq!(custom.encoder, PathBuf::from("/path/to/encoder.onnx"));
        assert_eq!(custom.decoder, PathBuf::from("/path/to/decoder.onnx"));
        assert_eq!(custom.joiner, Some(PathBuf::from("/path/to/joiner.onnx")));
        assert_eq!(custom.tokens, PathBuf::from("/path/to/tokens.txt"));
    }

    #[test]
    fn test_update_active_model_in_config_file() {
        let temp_dir = std::env::temp_dir().join(format!("speechyyy_cfg_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        let config_file = temp_dir.join("speechyyy").join("config.toml");

        // 1. File does not exist yet -> creates file with target model and provider
        Config::update_active_model_in_file(&config_file, "sarvam", "saaras:v4")
            .expect("create config file with active model");
        assert!(config_file.exists());
        let loaded = Config::load_from_path(&config_file);
        assert_eq!(loaded.inference.provider, "sarvam");
        assert_eq!(loaded.inference.model, "saaras:v4");
        assert_eq!(loaded.hotkey, "RightAlt"); // defaults preserved

        // 2. Existing file with custom configuration -> updates model/provider and preserves others
        let custom_toml = r#"
hotkey = "Ctrl+Space"
pacing_delay_ms = 42

[audio]
sample_rate = 48000

[inference]
provider = "sarvam"
api_key = "my_secret_token"
model = "saaras:v4"
"#;
        fs::write(&config_file, custom_toml).unwrap();

        Config::update_active_model_in_file(&config_file, "local", "parakeet-tdt-v3")
            .expect("update existing config file");
        let updated = Config::load_from_path(&config_file);
        assert_eq!(updated.inference.provider, "local");
        assert_eq!(updated.inference.model, "parakeet-tdt-v3");
        assert_eq!(updated.hotkey, "Ctrl+Space");
        assert_eq!(updated.pacing_delay_ms, 42);
        assert_eq!(updated.audio.sample_rate, 48000);
        assert_eq!(updated.inference.api_key, "my_secret_token");

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_save_sarvam_credentials_in_config_file() {
        let temp_dir = std::env::temp_dir().join(format!("speechyyy_sarvam_cfg_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        let config_file = temp_dir.join("speechyyy").join("config.toml");

        // 1. File does not exist -> creates config with api_key without setting provider
        Config::save_sarvam_credentials_in_file(&config_file, "sarvam_test_token_abc")
            .expect("save credentials to new config");
        assert!(config_file.exists());
        let loaded = Config::load_from_path(&config_file);
        assert_eq!(loaded.inference.provider, "local");
        assert_eq!(loaded.inference.api_key, "sarvam_test_token_abc");
        assert_eq!(loaded.inference.model, "saaras:v4");

        // 2. Existing file -> updates api_key, preserves existing provider, model and other configs
        let custom_toml = r#"
hotkey = "LeftAlt"
[inference]
provider = "local"
model = "whisper-small"
"#;
        fs::write(&config_file, custom_toml).unwrap();

        Config::save_sarvam_credentials_in_file(&config_file, "updated_token_xyz")
            .expect("update credentials in existing config");
        let reloaded = Config::load_from_path(&config_file);
        assert_eq!(reloaded.inference.provider, "local");
        assert_eq!(reloaded.inference.api_key, "updated_token_xyz");
        assert_eq!(reloaded.inference.model, "whisper-small");
        assert_eq!(reloaded.hotkey, "LeftAlt");

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_deactivate_model_in_config_file() {
        let temp_dir = std::env::temp_dir().join(format!("speechyyy_deactivate_cfg_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        let config_file = temp_dir.join("speechyyy").join("config.toml");

        let custom_toml = r#"
hotkey = "RightAlt"
[inference]
provider = "local"
model = "parakeet-tdt-v3"
"#;
        fs::create_dir_all(config_file.parent().unwrap()).unwrap();
        fs::write(&config_file, custom_toml).unwrap();

        let loaded = Config::load_from_path(&config_file);
        assert_eq!(loaded.inference.model, "parakeet-tdt-v3");

        Config::deactivate_model_in_file(&config_file).expect("deactivate model in file");

        let reloaded = Config::load_from_path(&config_file);
        assert_eq!(reloaded.inference.provider, "local");
        assert_eq!(reloaded.inference.model, "");

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[cfg(unix)]
    #[test]
    fn test_save_sarvam_credentials_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let temp_dir = std::env::temp_dir().join(format!("speechyyy_cfg_perm_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        let config_file = temp_dir.join("speechyyy").join("config.toml");

        Config::save_sarvam_credentials_in_file(&config_file, "secret_key_123")
            .expect("save credentials");
        let meta = fs::metadata(&config_file).expect("config file metadata");
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "Config file permissions must be 0600");

        let _ = fs::remove_dir_all(temp_dir);
    }
}
