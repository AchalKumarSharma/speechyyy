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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: default_hotkey(),
            pacing_delay_ms: default_pacing_ms(),
            audio: AudioConfig::default(),
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
"#;
        let parsed: Config = toml::from_str(toml_data).unwrap();
        assert_eq!(parsed.hotkey, "Ctrl+Space");
        assert_eq!(parsed.pacing_delay_ms, 5);
        assert_eq!(parsed.to_target_chord(), TargetChord::ctrl_space());
        assert_eq!(parsed.audio.sample_rate, 16000);
        assert!((parsed.audio.vad_threshold - 0.008).abs() < 1e-6);
        assert_eq!(parsed.audio.min_duration_ms, 300);
    }
}
