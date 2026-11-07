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
    /// Start Speechyyy (-f for foreground logs)
    Start {
        /// Run in foreground instead of detaching in background
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
    /// Stop running Speechyyy
    Stop,
    /// Inspect Speechyyy status
    Status,
    /// Trigger push-to-talk press (start recording via compositor IPC)
    #[command(name = "push-down", alias = "down")]
    PushDown,
    /// Trigger push-to-talk release (stop recording & transcribe via compositor IPC)
    #[command(name = "push-up", alias = "up")]
    PushUp,
    /// Toggle push-to-talk recording state
    Toggle,
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
        /// Target model architecture ('parakeet-tdt-v3', 'whisper-small', 'whisper-large-v3-turbo', 'moonshine-base', 'saaras:v4', 'sarvam:saaras-v4')
        #[arg(value_name = "MODEL")]
        model: Option<String>,
        /// Target model architecture flag (-m, --model)
        #[arg(short = 'm', long = "model", value_name = "MODEL")]
        model_flag: Option<String>,
        /// Force re-download or overwrite existing credentials
        #[arg(short = 'f', long = "force")]
        force: bool,
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
    /// Completely uninstall Speechyyy
    Uninstall {
        /// Automatic yes to prompts; run non-interactively
        #[arg(short = 'y', long = "yes")]
        yes: bool,
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
    /// Update Speechyyy to the newest release
    Update {
        /// Force re-installation even if already on the latest version
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand, Debug, PartialEq, Eq, Clone)]
pub enum ModelCommands {
    /// Download model weights (e.g. speechyyy download parakeet-tdt-v3)
    Download {
        /// Target model architecture ('parakeet-tdt-v3', 'whisper-small', 'whisper-large-v3-turbo', 'moonshine-base', 'saaras:v4', 'sarvam:saaras-v4')
        #[arg(value_name = "MODEL")]
        model: Option<String>,
        /// Target model architecture flag (-m, --model)
        #[arg(short = 'm', long = "model", value_name = "MODEL")]
        model_flag: Option<String>,
        /// Force re-download or overwrite existing credentials
        #[arg(short = 'f', long = "force")]
        force: bool,
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
        model: Option<String>,
        model_flag: Option<String>,
    ) -> String {
        model.or(model_flag).unwrap_or_else(|| "parakeet-tdt-v3".to_string())
    }
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
    fn test_parse_push_down_and_up() {
        let cli_down = Cli::try_parse_from(["speechyyy", "push-down"]).expect("parse push-down");
        assert_eq!(cli_down.command, Command::PushDown);

        let cli_alias_down = Cli::try_parse_from(["speechyyy", "down"]).expect("parse down alias");
        assert_eq!(cli_alias_down.command, Command::PushDown);

        let cli_up = Cli::try_parse_from(["speechyyy", "push-up"]).expect("parse push-up");
        assert_eq!(cli_up.command, Command::PushUp);

        let cli_alias_up = Cli::try_parse_from(["speechyyy", "up"]).expect("parse up alias");
        assert_eq!(cli_alias_up.command, Command::PushUp);
    }

    #[test]
    fn test_parse_toggle() {
        let cli_toggle = Cli::try_parse_from(["speechyyy", "toggle"]).expect("parse toggle");
        assert_eq!(cli_toggle.command, Command::Toggle);
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
                action: Some(ModelCommands::Download {
                    model: None,
                    model_flag: Some("parakeet-tdt-v3".to_string()),
                    force: false,
                })
            }
        );

        let cli_force =
            Cli::try_parse_from(["speechyyy", "model", "download", "saaras:v4", "-f"])
                .expect("parse model download force");
        assert_eq!(
            cli_force.command,
            Command::Models {
                action: Some(ModelCommands::Download {
                    model: Some("saaras:v4".to_string()),
                    model_flag: None,
                    force: true,
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
                model: Some("parakeet-tdt-v3".to_string()),
                model_flag: None,
                force: false,
            }
        );

        let cli_flag = Cli::try_parse_from(["speechyyy", "download", "-m", "whisper-small"])
            .expect("parse download flag");
        assert_eq!(
            cli_flag.command,
            Command::Download {
                model: None,
                model_flag: Some("whisper-small".to_string()),
                force: false,
            }
        );

        let cli_cloud = Cli::try_parse_from(["speechyyy", "download", "saaras:v4"])
            .expect("parse download cloud saaras");
        assert_eq!(
            cli_cloud.command,
            Command::Download {
                model: Some("saaras:v4".to_string()),
                model_flag: None,
                force: false,
            }
        );

        let cli_cloud_sarvam = Cli::try_parse_from(["speechyyy", "download", "sarvam:saaras-v4"])
            .expect("parse download cloud sarvam");
        assert_eq!(
            cli_cloud_sarvam.command,
            Command::Download {
                model: Some("sarvam:saaras-v4".to_string()),
                model_flag: None,
                force: false,
            }
        );

        let cli_force = Cli::try_parse_from(["speechyyy", "download", "saaras:v4", "--force"])
            .expect("parse download cloud saaras with --force");
        assert_eq!(
            cli_force.command,
            Command::Download {
                model: Some("saaras:v4".to_string()),
                model_flag: None,
                force: true,
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
        assert_eq!(
            cli.command,
            Command::Uninstall {
                yes: false
            }
        );

        let cli_yes =
            Cli::try_parse_from(["speechyyy", "uninstall", "-y"]).expect("parse uninstall yes");
        assert_eq!(
            cli_yes.command,
            Command::Uninstall {
                yes: true
            }
        );

        let cli_yes_long =
            Cli::try_parse_from(["speechyyy", "uninstall", "--yes"]).expect("parse uninstall yes long");
        assert_eq!(
            cli_yes_long.command,
            Command::Uninstall {
                yes: true
            }
        );
    }

    #[test]
    fn test_parse_update() {
        let cli = Cli::try_parse_from(["speechyyy", "update"]).expect("parse update");
        assert_eq!(cli.command, Command::Update { force: false });

        let cli_force =
            Cli::try_parse_from(["speechyyy", "update", "--force"]).expect("parse update force");
        assert_eq!(cli_force.command, Command::Update { force: true });
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
