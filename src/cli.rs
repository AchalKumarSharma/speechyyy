use crate::model::LocalModelType;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug, PartialEq, Eq)]
#[command(
    name = "speechyyy",
    about = "Ultra-lightweight Linux desktop push-to-talk dictation daemon",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug, PartialEq, Eq, Clone)]
pub enum Command {
    /// Start the Speechyyy background daemon
    Start {
        /// Run in foreground instead of detaching as a daemon
        #[arg(short, long)]
        foreground: bool,
        /// Speech-to-text inference provider ('local' or 'sarvam')
        #[arg(long)]
        provider: Option<String>,
        /// Sarvam operating mode ('translit', 'transcribe', 'translate', 'codemix')
        #[arg(long)]
        mode: Option<String>,
        /// Text injection mode ('typewriter' or 'paste')
        #[arg(long)]
        injection: Option<String>,
        /// Keystroke typing delay in milliseconds for typewriter mode
        #[arg(long)]
        typing_delay: Option<u64>,
    },
    /// Stop a running Speechyyy daemon
    Stop,
    /// Inspect the status of the Speechyyy daemon
    Status,
    /// Inspect system permissions and input device accessibility
    Doctor,
    /// Add or update a dynamic vocabulary replacement rule in dictionary.toml
    Learn {
        /// The incorrect phrase heard by the speech model
        wrong: String,
        /// The correct replacement text
        correct: String,
    },
    /// Diff against the most recent utterance and save the correction rule
    Fix {
        /// The fully corrected phrase
        corrected_phrase: String,
    },
    /// Inspect recent dictation history
    History {
        /// Number of recent records to display
        #[arg(short, long, default_value_t = 10)]
        limit: usize,
    },
    /// Manage local speech recognition models
    Model {
        #[command(subcommand)]
        action: ModelCommands,
    },
}

#[derive(Subcommand, Debug, PartialEq, Eq, Clone)]
pub enum ModelCommands {
    /// Download pre-converted INT8 quantized ONNX model weights
    Download {
        /// Target model architecture ('moonshine-tiny' or 'whisper-tiny')
        #[arg(short, long, default_value = "moonshine-tiny")]
        model: LocalModelType,
    },
    /// List supported speech models
    List,
    /// Set the active speech model
    Use {
        /// Name of the model ('saaras:v4', 'moonshine-tiny', 'whisper-tiny')
        model: String,
    },
}

pub type ModelAction = ModelCommands;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_start_default() {
        let cli = Cli::try_parse_from(["speechyyy", "start"]).expect("parse start");
        assert_eq!(
            cli.command,
            Command::Start {
                foreground: false,
                provider: None,
                mode: None,
                injection: None,
                typing_delay: None,
            }
        );
    }

    #[test]
    fn test_parse_start_foreground() {
        let cli = Cli::try_parse_from(["speechyyy", "start", "--foreground"])
            .expect("parse start foreground");
        assert_eq!(
            cli.command,
            Command::Start {
                foreground: true,
                provider: None,
                mode: None,
                injection: None,
                typing_delay: None,
            }
        );
    }

    #[test]
    fn test_parse_stop() {
        let cli = Cli::try_parse_from(["speechyyy", "stop"]).expect("parse stop");
        assert_eq!(cli.command, Command::Stop);
    }

    #[test]
    fn test_parse_status() {
        let cli = Cli::try_parse_from(["speechyyy", "status"]).expect("parse status");
        assert_eq!(cli.command, Command::Status);
    }

    #[test]
    fn test_parse_doctor() {
        let cli = Cli::try_parse_from(["speechyyy", "doctor"]).expect("parse doctor");
        assert_eq!(cli.command, Command::Doctor);
    }

    #[test]
    fn test_parse_learn() {
        let cli = Cli::try_parse_from(["speechyyy", "learn", "post gress", "PostgreSQL"])
            .expect("parse learn");
        assert_eq!(
            cli.command,
            Command::Learn {
                wrong: "post gress".into(),
                correct: "PostgreSQL".into()
            }
        );
    }

    #[test]
    fn test_parse_fix() {
        let cli =
            Cli::try_parse_from(["speechyyy", "fix", "I love PostgreSQL"]).expect("parse fix");
        assert_eq!(
            cli.command,
            Command::Fix {
                corrected_phrase: "I love PostgreSQL".into()
            }
        );
    }

    #[test]
    fn test_parse_history() {
        let cli =
            Cli::try_parse_from(["speechyyy", "history", "--limit", "25"]).expect("parse history");
        assert_eq!(cli.command, Command::History { limit: 25 });
    }

    #[test]
    fn test_parse_model_download() {
        let cli =
            Cli::try_parse_from(["speechyyy", "model", "download", "--model", "whisper-tiny"])
                .expect("parse model download");
        assert_eq!(
            cli.command,
            Command::Model {
                action: ModelAction::Download {
                    model: LocalModelType::WhisperTiny,
                }
            }
        );
    }

    #[test]
    fn test_parse_model_list() {
        let cli = Cli::try_parse_from(["speechyyy", "model", "list"]).expect("parse model list");
        assert_eq!(
            cli.command,
            Command::Model {
                action: ModelCommands::List,
            }
        );
    }

    #[test]
    fn test_parse_model_use() {
        let cli = Cli::try_parse_from(["speechyyy", "model", "use", "moonshine-tiny"])
            .expect("parse model use");
        assert_eq!(
            cli.command,
            Command::Model {
                action: ModelCommands::Use {
                    model: "moonshine-tiny".to_string(),
                }
            }
        );
    }
}
