use crate::engine::{Modifier, TargetChord};
use evdev::KeyCode;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

fn default_hotkey() -> String {
    "RightAlt".to_string()
}

fn default_pacing_ms() -> u64 {
    3
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
    "mock".to_string()
}

fn default_model() -> String {
    "saaras:v4".to_string()
}

fn default_language_code() -> String {
    "unknown".to_string()
}

fn default_mode() -> String {
    "transcribe".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InferenceConfig {
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_language_code")]
    pub language_code: String,
    #[serde(default = "default_mode")]
    pub mode: String,
}

impl Default for InferenceConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            api_key: String::new(),
            model: default_model(),
            language_code: default_language_code(),
            mode: default_mode(),
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    #[serde(default = "default_pacing_ms")]
    pub pacing_delay_ms: u64,
    #[serde(default)]
    pub audio: AudioConfig,
    #[serde(default)]
    pub inference: InferenceConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: default_hotkey(),
            pacing_delay_ms: default_pacing_ms(),
            audio: AudioConfig::default(),
            inference: InferenceConfig::default(),
        }
    }
}

impl Config {
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

    pub fn to_target_chord(&self) -> TargetChord {
        parse_hotkey(&self.hotkey).unwrap_or_else(TargetChord::right_alt)
    }
}

fn parse_trigger_key(name: &str) -> Option<KeyCode> {
    match name.to_lowercase().as_str() {
        "space" => Some(KeyCode::KEY_SPACE),
        "rightalt" | "right_alt" | "alt_r" => Some(KeyCode::KEY_RIGHTALT),
        "leftalt" | "left_alt" | "alt_l" => Some(KeyCode::KEY_LEFTALT),
        "rightctrl" | "right_ctrl" | "ctrl_r" => Some(KeyCode::KEY_RIGHTCTRL),
        "leftctrl" | "left_ctrl" | "ctrl_l" => Some(KeyCode::KEY_LEFTCTRL),
        "enter" => Some(KeyCode::KEY_ENTER),
        "tab" => Some(KeyCode::KEY_TAB),
        _ => None,
    }
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
    }

    #[test]
    fn test_config_defaults_and_parsing() {
        let default_cfg = Config::default();
        assert_eq!(default_cfg.hotkey, "RightAlt");
        assert_eq!(default_cfg.pacing_delay_ms, 3);
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
        assert_eq!(cfg.provider, "mock");
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
}
