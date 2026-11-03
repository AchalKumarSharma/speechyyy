use crate::model::LocalModelType;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug, PartialEq, Eq)]
#[command(
    name = "speechyyy",
    about = "You can type faster with your mouth now.",
    after_help = "Hotkey:\n  Hold [RightAlt] to speak. Release to type anywhere.\n",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug, PartialEq, Eq, Clone)]
pub enum Command {
    /// Start the Speechyyy daemon (-f for foreground logs)
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
    /// Stop the running daemon
    Stop,
    /// Inspect daemon process status
    Status,
    /// Inspect system permissions, audio inputs, and model weights
    Doctor,
    /// List available speech recognition models (alias: model)
    #[command(alias = "model")]
    Models {
        #[command(subcommand)]
        action: Option<ModelCommands>,
    },
    /// Download model weights (e.g. speechyyy download parakeet-tdt-v3)
    Download {
        /// Target model architecture ('parakeet-tdt-v3', 'whisper-small', 'whisper-large-v3-turbo', 'moonshine-base')
        #[arg(value_name = "MODEL")]
        model: Option<LocalModelType>,
        /// Target model architecture flag (-m, --model)
        #[arg(short = 'm', long = "model", value_name = "MODEL")]
        model_flag: Option<LocalModelType>,
    },
    /// Switch active model (e.g. speechyyy use parakeet-tdt-v3)
    Use {
        /// Name of the model preset ('saaras:v4', 'parakeet-tdt-v3', 'whisper-small', 'whisper-large-v3-turbo', 'moonshine-base')
        model: String,
    },
    /// Delete local model weights to free disk space (alias: rm)
    #[command(alias = "rm", alias = "delete")]
    Remove {
        /// Name of the model to remove ('parakeet-tdt-v3', 'whisper-small', 'whisper-large-v3-turbo', 'moonshine-base')
        model: String,
    },
    /// Stop daemon and clean up local data/configuration
    Uninstall {
        /// Purge all models, configs, dictionaries, and user-space binaries
        #[arg(long)]
        purge: bool,
    },
    /// Add a phonetic replacement rule to dictionary.toml
    Learn {
        /// The incorrect phrase heard by the speech model
        wrong: Option<String>,
        /// The correct replacement text
        correct: Option<String>,
    },
    /// Diff against the most recent utterance and save correction rule
    Fix {
        /// The fully corrected phrase
        corrected_phrase: Option<String>,
    },
    /// Remove a replacement rule from dictionary.toml
    Unlearn {
        /// The replacement rule key to remove
        key: Option<String>,
    },
    /// Inspect recent dictation history
    History {
        /// Number of recent records to display
        #[arg(short, long, default_value_t = 10)]
        limit: usize,
    },
}

#[derive(Subcommand, Debug, PartialEq, Eq, Clone)]
pub enum ModelCommands {
    /// Download model weights (e.g. speechyyy download parakeet-tdt-v3)
    Download {
        /// Target model architecture ('parakeet-tdt-v3', 'whisper-small', 'whisper-large-v3-turbo', 'moonshine-base')
        #[arg(value_name = "MODEL")]
        model: Option<LocalModelType>,
        /// Target model architecture flag (-m, --model)
        #[arg(short = 'm', long = "model", value_name = "MODEL")]
        model_flag: Option<LocalModelType>,
    },
    /// List supported speech models
    List,
    /// Set the active speech model
    Use {
        /// Name of the model preset ('saaras:v4', 'parakeet-tdt-v3', 'whisper-small', 'whisper-large-v3-turbo', 'moonshine-base')
        model: String,
    },
    /// Delete local model weights to free disk space (alias: rm)
    #[command(alias = "rm", alias = "delete")]
    Remove {
        /// Name of the model to remove ('parakeet-tdt-v3', 'whisper-small', 'whisper-large-v3-turbo', 'moonshine-base')
        model: String,
    },
}

impl ModelCommands {
    pub fn resolved_download_model(
        model: Option<LocalModelType>,
        model_flag: Option<LocalModelType>,
    ) -> LocalModelType {
        model.or(model_flag).unwrap_or(LocalModelType::ParakeetTdtV3)
    }
}

pub type Commands = Command;
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
                wrong: Some("post gress".into()),
                correct: Some("PostgreSQL".into())
            }
        );
    }

    #[test]
    fn test_parse_learn_bare() {
        let cli = Cli::try_parse_from(["speechyyy", "learn"]).expect("parse bare learn");
        assert_eq!(
            cli.command,
            Command::Learn {
                wrong: None,
                correct: None
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
                corrected_phrase: Some("I love PostgreSQL".into())
            }
        );
    }

    #[test]
    fn test_parse_fix_bare() {
        let cli = Cli::try_parse_from(["speechyyy", "fix"]).expect("parse bare fix");
        assert_eq!(
            cli.command,
            Command::Fix {
                corrected_phrase: None
            }
        );
    }

    #[test]
    fn test_parse_unlearn() {
        let cli = Cli::try_parse_from(["speechyyy", "unlearn", "Speechy kya tum mujhe sun sakte ho"])
            .expect("parse unlearn");
        assert_eq!(
            cli.command,
            Command::Unlearn {
                key: Some("Speechy kya tum mujhe sun sakte ho".into())
            }
        );
    }

    #[test]
    fn test_parse_unlearn_bare() {
        let cli = Cli::try_parse_from(["speechyyy", "unlearn"]).expect("parse bare unlearn");
        assert_eq!(cli.command, Command::Unlearn { key: None });
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
            Cli::try_parse_from(["speechyyy", "model", "download", "--model", "parakeet-tdt-v3"])
                .expect("parse model download");
        assert_eq!(
            cli.command,
            Command::Models {
                action: Some(ModelAction::Download {
                    model: None,
                    model_flag: Some(LocalModelType::ParakeetTdtV3),
                })
            }
        );
    }

    #[test]
    fn test_parse_model_list() {
        let cli = Cli::try_parse_from(["speechyyy", "model", "list"]).expect("parse model list");
        assert_eq!(
            cli.command,
            Command::Models {
                action: Some(ModelCommands::List),
            }
        );
    }

    #[test]
    fn test_parse_model_use() {
        let cli = Cli::try_parse_from(["speechyyy", "model", "use", "parakeet-tdt-v3"])
            .expect("parse model use");
        assert_eq!(
            cli.command,
            Command::Models {
                action: Some(ModelCommands::Use {
                    model: "parakeet-tdt-v3".to_string(),
                })
            }
        );
    }

    #[test]
    fn test_parse_top_level_models_and_bare_model() {
        let cli_models =
            Cli::try_parse_from(["speechyyy", "models"]).expect("parse models");
        assert_eq!(
            cli_models.command,
            Command::Models { action: None }
        );

        let cli_model_bare =
            Cli::try_parse_from(["speechyyy", "model"]).expect("parse bare model");
        assert_eq!(
            cli_model_bare.command,
            Command::Models { action: None }
        );
    }

    #[test]
    fn test_parse_top_level_download() {
        let cli_pos = Cli::try_parse_from(["speechyyy", "download", "parakeet-tdt-v3"])
            .expect("parse download positional");
        assert_eq!(
            cli_pos.command,
            Command::Download {
                model: Some(LocalModelType::ParakeetTdtV3),
                model_flag: None,
            }
        );

        let cli_flag = Cli::try_parse_from(["speechyyy", "download", "-m", "whisper-small"])
            .expect("parse download flag");
        assert_eq!(
            cli_flag.command,
            Command::Download {
                model: None,
                model_flag: Some(LocalModelType::WhisperSmall),
            }
        );
    }

    #[test]
    fn test_parse_top_level_use() {
        let cli = Cli::try_parse_from(["speechyyy", "use", "parakeet-tdt-v3"])
            .expect("parse use");
        assert_eq!(
            cli.command,
            Command::Use {
                model: "parakeet-tdt-v3".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_remove_and_aliases() {
        let cli_remove =
            Cli::try_parse_from(["speechyyy", "remove", "parakeet-tdt-v3"]).expect("parse remove");
        assert_eq!(
            cli_remove.command,
            Command::Remove {
                model: "parakeet-tdt-v3".to_string(),
            }
        );

        let cli_rm =
            Cli::try_parse_from(["speechyyy", "rm", "whisper-small"]).expect("parse rm");
        assert_eq!(
            cli_rm.command,
            Command::Remove {
                model: "whisper-small".to_string(),
            }
        );

        let cli_delete =
            Cli::try_parse_from(["speechyyy", "delete", "moonshine-base"]).expect("parse delete");
        assert_eq!(
            cli_delete.command,
            Command::Remove {
                model: "moonshine-base".to_string(),
            }
        );

        let cli_model_rm =
            Cli::try_parse_from(["speechyyy", "model", "remove", "parakeet-tdt-v3"])
                .expect("parse model remove");
        assert_eq!(
            cli_model_rm.command,
            Command::Models {
                action: Some(ModelCommands::Remove {
                    model: "parakeet-tdt-v3".to_string(),
                }),
            }
        );
    }

    #[test]
    fn test_parse_uninstall() {
        let cli = Cli::try_parse_from(["speechyyy", "uninstall"]).expect("parse uninstall");
        assert_eq!(cli.command, Command::Uninstall { purge: false });

        let cli_purge =
            Cli::try_parse_from(["speechyyy", "uninstall", "--purge"]).expect("parse uninstall purge");
        assert_eq!(cli_purge.command, Command::Uninstall { purge: true });
    }

    #[test]
    fn test_help_metadata_tagline_and_hotkey_guidance() {
        use clap::CommandFactory;
        let mut cmd = Cli::command();
        let mut buf = Vec::new();
        cmd.write_help(&mut buf).unwrap();
        let help = String::from_utf8(buf).unwrap();

        assert!(help.contains("You can type faster with your mouth now."));
        assert!(help.contains("Hotkey:\n  Hold [RightAlt] to speak. Release to type anywhere."));
        assert!(help.contains("models"));
        assert!(help.contains("download"));
        assert!(help.contains("use"));
        assert!(help.contains("remove"));
        assert!(help.contains("uninstall"));
        assert!(help.contains("unlearn"));
    }
}
