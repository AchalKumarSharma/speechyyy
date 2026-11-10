use crate::engine::{Modifier, TargetChord};
use crate::inference::wire::Secret;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputKey {
    RightAlt,
    LeftAlt,
    CapsLock,
    ScrollLock,
    Pause,
    RightCtrl,
    LeftCtrl,
    RightShift,
    LeftShift,
    Space,
    Enter,
    Tab,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
}

impl InputKey {
    pub fn to_keycode(&self) -> KeyCode {
        match self {
            InputKey::Space => KeyCode::KEY_SPACE,
            InputKey::RightAlt => KeyCode::KEY_RIGHTALT,
            InputKey::LeftAlt => KeyCode::KEY_LEFTALT,
            InputKey::CapsLock => KeyCode::KEY_CAPSLOCK,
            InputKey::ScrollLock => KeyCode::KEY_SCROLLLOCK,
            InputKey::Pause => KeyCode::KEY_PAUSE,
            InputKey::RightCtrl => KeyCode::KEY_RIGHTCTRL,
            InputKey::LeftCtrl => KeyCode::KEY_LEFTCTRL,
            InputKey::RightShift => KeyCode::KEY_RIGHTSHIFT,
            InputKey::LeftShift => KeyCode::KEY_LEFTSHIFT,
            InputKey::Enter => KeyCode::KEY_ENTER,
            InputKey::Tab => KeyCode::KEY_TAB,
            InputKey::F1 => KeyCode::KEY_F1,
            InputKey::F2 => KeyCode::KEY_F2,
            InputKey::F3 => KeyCode::KEY_F3,
            InputKey::F4 => KeyCode::KEY_F4,
            InputKey::F5 => KeyCode::KEY_F5,
            InputKey::F6 => KeyCode::KEY_F6,
            InputKey::F7 => KeyCode::KEY_F7,
            InputKey::F8 => KeyCode::KEY_F8,
            InputKey::F9 => KeyCode::KEY_F9,
            InputKey::F10 => KeyCode::KEY_F10,
            InputKey::F11 => KeyCode::KEY_F11,
            InputKey::F12 => KeyCode::KEY_F12,
        }
    }
}

impl std::str::FromStr for InputKey {
    type Err = InputError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "space" => Ok(InputKey::Space),
            "rightalt" | "right_alt" | "alt_r" => Ok(InputKey::RightAlt),
            "leftalt" | "left_alt" | "alt_l" => Ok(InputKey::LeftAlt),
            "capslock" | "caps_lock" | "caps" => Ok(InputKey::CapsLock),
            "scrolllock" | "scroll_lock" => Ok(InputKey::ScrollLock),
            "pause" | "pausebreak" | "pause_break" => Ok(InputKey::Pause),
            "rightctrl" | "right_ctrl" | "ctrl_r" => Ok(InputKey::RightCtrl),
            "leftctrl" | "left_ctrl" | "ctrl_l" => Ok(InputKey::LeftCtrl),
            "rightshift" | "right_shift" | "shift_r" => Ok(InputKey::RightShift),
            "leftshift" | "left_shift" | "shift_l" => Ok(InputKey::LeftShift),
            "enter" | "return" => Ok(InputKey::Enter),
            "tab" => Ok(InputKey::Tab),
            "f1" => Ok(InputKey::F1),
            "f2" => Ok(InputKey::F2),
            "f3" => Ok(InputKey::F3),
            "f4" => Ok(InputKey::F4),
            "f5" => Ok(InputKey::F5),
            "f6" => Ok(InputKey::F6),
            "f7" => Ok(InputKey::F7),
            "f8" => Ok(InputKey::F8),
            "f9" => Ok(InputKey::F9),
            "f10" => Ok(InputKey::F10),
            "f11" => Ok(InputKey::F11),
            "f12" => Ok(InputKey::F12),
            _ => Err(InputError::UnknownKey(s.to_string())),
        }
    }
}

impl TryFrom<&str> for InputKey {
    type Error = InputError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

pub fn resolve_hotkey(name: &str) -> Result<KeyCode, InputError> {
    name.parse::<InputKey>().map(|k| k.to_keycode())
}

fn default_pacing_ms() -> u64 {
    15
}

/// Delivery mechanism for emitting dictation into the focused window/caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InjectionMode {
    /// Fast paste via `Ctrl+V`; leaves the transcription in the clipboard and is immune
    /// to browser hotkey hijacking. Does not work in most terminals (they need `Ctrl+Shift+V`).
    #[default]
    Clipboard,
    /// Character-by-character typing via `/dev/uinput`. Works in terminals, but triggers
    /// page shortcuts if no text field is focused.
    Typewriter,
}

impl std::fmt::Display for InjectionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InjectionMode::Clipboard => write!(f, "clipboard"),
            InjectionMode::Typewriter => write!(f, "typewriter"),
        }
    }
}

impl std::str::FromStr for InjectionMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "clipboard" | "paste" | "wayland" => Ok(InjectionMode::Clipboard),
            "typewriter" | "uinput" => Ok(InjectionMode::Typewriter),
            other => Err(format!(
                "Unknown injection mode '{}'. Supported: 'clipboard', 'typewriter'.",
                other
            )),
        }
    }
}

fn default_injector_method() -> String {
    "clipboard".to_string()
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

/// Single source of truth for default models across inference providers.
pub fn default_model_for(provider: &str) -> &'static str {
    match provider.trim().to_ascii_lowercase().as_str() {
        "local" => "parakeet-tdt-v3",
        "sarvam" => "saaras:v4",
        "groq" => crate::inference::DEFAULT_GROQ_MODEL,
        "openai" => crate::inference::DEFAULT_OPENAI_MODEL,
        "openai-compatible" | "custom" => "",
        _ => "",
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    Config,
    Env,
}

impl std::fmt::Display for KeySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeySource::Config => write!(f, "config"),
            KeySource::Env => write!(f, "environment"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InferenceConfig {
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default, alias = "sarvam_api_key")]
    pub api_key: Secret,
    #[serde(default, alias = "sarvam_model")]
    pub model: String,
    #[serde(default = "default_language_code")]
    pub language_code: String,
    #[serde(default = "default_mode", alias = "sarvam_mode")]
    pub mode: String,
    #[serde(default)]
    pub groq_api_key: Option<Secret>,
    #[serde(default)]
    pub openai_api_key: Option<Secret>,
    #[serde(default)]
    pub openai_compatible_endpoint: Option<String>,
    #[serde(default)]
    pub openai_compatible_model: Option<String>,
    #[serde(default)]
    pub openai_compatible_api_key: Option<Secret>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub request_timeout_secs: Option<u64>,
    #[serde(default)]
    pub custom: Option<CustomModelConfig>,
}

impl Default for InferenceConfig {
    fn default() -> Self {
        let provider = default_provider();
        let model = default_model_for(&provider).to_string();
        Self {
            provider,
            api_key: Secret::default(),
            model,
            language_code: default_language_code(),
            mode: default_mode(),
            groq_api_key: None,
            openai_api_key: None,
            openai_compatible_endpoint: None,
            openai_compatible_model: None,
            openai_compatible_api_key: None,
            language: None,
            request_timeout_secs: None,
            custom: None,
        }
    }
}

impl InferenceConfig {
    /// Pure key resolution function:
    /// - Returns Option<(Secret, KeySource)>
    /// - For sarvam: config api_key, env SARVAM_API_KEY
    /// - For groq: config groq_api_key, env GROQ_API_KEY
    /// - For openai: config openai_api_key, env OPENAI_API_KEY
    /// - For openai-compatible: config openai_compatible_api_key, env OPENAI_COMPATIBLE_API_KEY (optional: None if missing)
    /// - Strict isolation: a Sarvam key must never resolve for Groq or OpenAI, and vice versa.
    /// - Empty strings count as unset.
    pub fn resolve_key_for_provider(&self, provider: &str) -> Option<(Secret, KeySource)> {
        match provider.to_lowercase().as_str() {
            "sarvam" => {
                if !self.api_key.trim().is_empty() {
                    Some((self.api_key.clone(), KeySource::Config))
                } else {
                    std::env::var("SARVAM_API_KEY")
                        .ok()
                        .filter(|k| !k.trim().is_empty())
                        .map(|k| (Secret::new(k.trim()), KeySource::Env))
                }
            }
            "groq" => {
                if let Some(k) = self.groq_api_key.as_ref().filter(|k| !k.trim().is_empty()) {
                    Some((k.clone(), KeySource::Config))
                } else {
                    std::env::var("GROQ_API_KEY")
                        .ok()
                        .filter(|k| !k.trim().is_empty())
                        .map(|k| (Secret::new(k.trim()), KeySource::Env))
                }
            }
            "openai" => {
                if let Some(k) = self.openai_api_key.as_ref().filter(|k| !k.trim().is_empty()) {
                    Some((k.clone(), KeySource::Config))
                } else {
                    std::env::var("OPENAI_API_KEY")
                        .ok()
                        .filter(|k| !k.trim().is_empty())
                        .map(|k| (Secret::new(k.trim()), KeySource::Env))
                }
            }
            "openai-compatible" => {
                if let Some(k) = self.openai_compatible_api_key.as_ref().filter(|k| !k.trim().is_empty()) {
                    Some((k.clone(), KeySource::Config))
                } else {
                    std::env::var("OPENAI_COMPATIBLE_API_KEY")
                        .ok()
                        .filter(|k| !k.trim().is_empty())
                        .map(|k| (Secret::new(k.trim()), KeySource::Env))
                }
            }
            _ => None,
        }
    }

    pub fn resolve_provider_key(&self) -> Option<(Secret, KeySource)> {
        self.resolve_key_for_provider(&self.provider)
    }

    pub fn resolve_api_key(&self) -> Option<String> {
        self.resolve_key_for_provider("sarvam")
            .map(|(k, _)| k.expose().to_string())
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
    /// Active text injection mode (`clipboard` paste vs `typewriter` uinput typing).
    #[serde(default)]
    pub injection_mode: InjectionMode,
    pub input: InputConfig,
    pub audio: AudioConfig,
    pub inference: InferenceConfig,
    pub injector: InjectorConfig,
    pub ui: UiConfig,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RawInferenceConfig {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default, alias = "sarvam_api_key")]
    pub api_key: Option<Secret>,
    #[serde(default, alias = "sarvam_model")]
    pub model: Option<String>,
    #[serde(default = "default_language_code")]
    pub language_code: String,
    #[serde(default = "default_mode", alias = "sarvam_mode")]
    pub mode: String,
    #[serde(default)]
    pub groq_api_key: Option<Secret>,
    #[serde(default)]
    pub openai_api_key: Option<Secret>,
    #[serde(default)]
    pub openai_compatible_endpoint: Option<String>,
    #[serde(default)]
    pub openai_compatible_model: Option<String>,
    #[serde(default)]
    pub openai_compatible_api_key: Option<Secret>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub request_timeout_secs: Option<u64>,
    #[serde(default)]
    pub custom: Option<CustomModelConfig>,
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
    inference: RawInferenceConfig,
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
    injection_mode: Option<InjectionMode>,
}

impl From<RawConfig> for Config {
    fn from(raw: RawConfig) -> Self {
        let provider = raw
            .provider
            .or(raw.inference.provider)
            .unwrap_or_else(default_provider);

        let api_key = raw
            .sarvam_api_key
            .map(Secret::new)
            .or(raw.inference.api_key)
            .unwrap_or_default();

        let mode = raw
            .sarvam_mode
            .unwrap_or(raw.inference.mode);

        let model = raw
            .sarvam_model
            .or(raw.inference.model)
            .unwrap_or_else(|| default_model_for(&provider).to_string());

        let inference = InferenceConfig {
            provider,
            api_key,
            model,
            language_code: raw.inference.language_code,
            mode,
            groq_api_key: raw.inference.groq_api_key,
            openai_api_key: raw.inference.openai_api_key,
            openai_compatible_endpoint: raw.inference.openai_compatible_endpoint,
            openai_compatible_model: raw.inference.openai_compatible_model,
            openai_compatible_api_key: raw.inference.openai_compatible_api_key,
            language: raw.inference.language,
            request_timeout_secs: raw.inference.request_timeout_secs,
            custom: raw.inference.custom,
        };

        let injection_mode = raw
            .injection_mode
            .or_else(|| raw.injector.method.parse::<InjectionMode>().ok())
            .unwrap_or_default();

        let mut injector = raw.injector;
        if raw.injection_mode.is_some() {
            injector.method = injection_mode.to_string();
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
            injection_mode,
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
            injection_mode: InjectionMode::default(),
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

    fn mutate_inference_table<F: FnOnce(&mut toml::Table)>(
        path: &Path,
        cleanup_legacy: bool,
        f: F,
    ) -> Result<(), String> {
        let mut table = read_toml_table(path)?;

        let inf_val = table
            .entry("inference")
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));

        if let toml::Value::Table(ref mut inf_table) = inf_val {
            f(inf_table);
        } else {
            let mut inf_table = toml::Table::new();
            f(&mut inf_table);
            table.insert("inference".into(), toml::Value::Table(inf_table));
        }

        if cleanup_legacy {
            table.remove("provider");
            table.remove("sarvam_api_key");
            table.remove("sarvam_model");
        }

        write_secure_config(path, &table)
    }

    pub fn save_sarvam_credentials_in_file(path: &Path, api_key: &str) -> Result<(), String> {
        Self::mutate_inference_table(path, true, |inf| {
            inf.insert("api_key".into(), toml::Value::String(api_key.trim().to_string()));
        })
    }

    pub fn save_groq_credentials_in_file(path: &Path, api_key: &str) -> Result<(), String> {
        Self::mutate_inference_table(path, false, |inf| {
            inf.insert("groq_api_key".into(), toml::Value::String(api_key.trim().to_string()));
        })
    }

    pub fn save_openai_credentials_in_file(path: &Path, api_key: &str) -> Result<(), String> {
        Self::mutate_inference_table(path, false, |inf| {
            inf.insert("openai_api_key".into(), toml::Value::String(api_key.trim().to_string()));
        })
    }

    pub fn save_openai_compatible_config_in_file(
        path: &Path,
        endpoint: &str,
        model: Option<&str>,
        api_key: Option<&str>,
    ) -> Result<(), String> {
        Self::mutate_inference_table(path, false, |inf| {
            inf.insert(
                "openai_compatible_endpoint".into(),
                toml::Value::String(endpoint.trim().to_string()),
            );
            if let Some(m) = model {
                inf.insert(
                    "openai_compatible_model".into(),
                    toml::Value::String(m.trim().to_string()),
                );
            }
            if let Some(k) = api_key {
                let trimmed = k.trim();
                if !trimmed.is_empty() {
                    inf.insert(
                        "openai_compatible_api_key".into(),
                        toml::Value::String(trimmed.to_string()),
                    );
                }
            }
        })
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
            if let Ok(m) = inj.parse::<InjectionMode>() {
                self.injection_mode = m;
                self.injector.method = m.to_string();
            } else {
                self.injector.method = inj.to_string();
            }
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

    let parent_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let temp_path = parent_dir.join(format!(
        ".config.toml.tmp.{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));

    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temp_path)
            .map_err(|e| {
                format!(
                    "Failed to create secure temp config {}: {}",
                    temp_path.display(),
                    e
                )
            })?;

        file.write_all(serialized.as_bytes()).map_err(|e| {
            let _ = fs::remove_file(&temp_path);
            format!("Failed to write to {}: {}", temp_path.display(), e)
        })?;

        file.sync_all().map_err(|e| {
            let _ = fs::remove_file(&temp_path);
            format!("Failed to sync {}: {}", temp_path.display(), e)
        })?;
    }

    #[cfg(not(unix))]
    {
        fs::write(&temp_path, &serialized)
            .map_err(|e| format!("Failed to write to {}: {}", temp_path.display(), e))?;
    }

    fs::rename(&temp_path, path).map_err(|e| {
        let _ = fs::remove_file(&temp_path);
        format!(
            "Failed to atomically rename config to {}: {}",
            path.display(),
            e
        )
    })?;

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
        use std::str::FromStr;
        assert_eq!(InputKey::from_str("RightAlt").unwrap(), InputKey::RightAlt);
        assert_eq!(InputKey::try_from("CapsLock").unwrap(), InputKey::CapsLock);
        assert_eq!(InputKey::RightAlt.to_keycode(), KeyCode::KEY_RIGHTALT);

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
        assert_eq!(default_cfg.injection_mode, InjectionMode::Clipboard);
        assert_eq!(default_cfg.injector.method, "clipboard");
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
        assert_eq!(cfg.model, "parakeet-tdt-v3");
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
        assert_eq!(loaded.inference.model, "parakeet-tdt-v3");

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

    #[test]
    fn test_injection_mode_defaults_and_parsing() {
        // Missing from TOML defaults to Clipboard
        let toml_empty = "";
        let cfg: Config = toml::from_str(toml_empty).unwrap();
        assert_eq!(cfg.injection_mode, InjectionMode::Clipboard);
        assert_eq!(cfg.injector.method, "clipboard");

        // Parses explicit "clipboard"
        let toml_clip = "injection_mode = \"clipboard\"";
        let cfg_clip: Config = toml::from_str(toml_clip).unwrap();
        assert_eq!(cfg_clip.injection_mode, InjectionMode::Clipboard);
        assert_eq!(cfg_clip.injector.method, "clipboard");

        // Parses explicit "typewriter"
        let toml_type = "injection_mode = \"typewriter\"";
        let cfg_type: Config = toml::from_str(toml_type).unwrap();
        assert_eq!(cfg_type.injection_mode, InjectionMode::Typewriter);
        assert_eq!(cfg_type.injector.method, "typewriter");

        // Parses legacy [injector] method = "typewriter"
        let toml_legacy = "[injector]\nmethod = \"typewriter\"";
        let cfg_legacy: Config = toml::from_str(toml_legacy).unwrap();
        assert_eq!(cfg_legacy.injection_mode, InjectionMode::Typewriter);

        // String parsing
        assert_eq!("clipboard".parse::<InjectionMode>(), Ok(InjectionMode::Clipboard));
        assert_eq!("typewriter".parse::<InjectionMode>(), Ok(InjectionMode::Typewriter));
        assert!("invalid".parse::<InjectionMode>().is_err());
    }

    #[test]
    fn test_default_model_for_providers() {
        assert_eq!(default_model_for("local"), "parakeet-tdt-v3");
        assert_eq!(default_model_for("sarvam"), "saaras:v4");
        assert_eq!(default_model_for("groq"), "whisper-large-v3-turbo");
        assert_eq!(default_model_for("openai"), "whisper-1");
        assert_eq!(default_model_for("openai-compatible"), "");
        assert_eq!(default_model_for("custom"), "");
        assert_eq!(default_model_for("unknown"), "");
    }

    #[test]
    fn test_omitted_model_resolves_to_provider_default() {
        // 1. Fresh config (empty TOML) defaults to local + parakeet-tdt-v3
        let cfg_empty: Config = toml::from_str("").unwrap();
        assert_eq!(cfg_empty.inference.provider, "local");
        assert_eq!(cfg_empty.inference.model, "parakeet-tdt-v3");

        // 2. Existing config specifying provider = "sarvam" without model resolves to saaras:v4
        let cfg_sarvam: Config = toml::from_str("[inference]\nprovider = \"sarvam\"").unwrap();
        assert_eq!(cfg_sarvam.inference.provider, "sarvam");
        assert_eq!(cfg_sarvam.inference.model, "saaras:v4");

        // 3. Provider = "groq" without model resolves to whisper-large-v3-turbo
        let cfg_groq: Config = toml::from_str("[inference]\nprovider = \"groq\"").unwrap();
        assert_eq!(cfg_groq.inference.provider, "groq");
        assert_eq!(cfg_groq.inference.model, "whisper-large-v3-turbo");

        // 4. Provider = "openai" without model resolves to whisper-1
        let cfg_openai: Config = toml::from_str("[inference]\nprovider = \"openai\"").unwrap();
        assert_eq!(cfg_openai.inference.provider, "openai");
        assert_eq!(cfg_openai.inference.model, "whisper-1");

        // 5. Provider = "openai-compatible" without model resolves to empty string
        let cfg_compat: Config = toml::from_str("[inference]\nprovider = \"openai-compatible\"").unwrap();
        assert_eq!(cfg_compat.inference.provider, "openai-compatible");
        assert_eq!(cfg_compat.inference.model, "");
    }

    #[test]
    fn test_explicit_model_preserved_across_providers() {
        let cfg_local: Config = toml::from_str("[inference]\nprovider = \"local\"\nmodel = \"whisper-small\"").unwrap();
        assert_eq!(cfg_local.inference.provider, "local");
        assert_eq!(cfg_local.inference.model, "whisper-small");

        let cfg_sarvam: Config = toml::from_str("[inference]\nprovider = \"sarvam\"\nmodel = \"custom-sarvam-model\"").unwrap();
        assert_eq!(cfg_sarvam.inference.provider, "sarvam");
        assert_eq!(cfg_sarvam.inference.model, "custom-sarvam-model");
    }
}
