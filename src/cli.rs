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
    },
    /// Stop a running Speechyyy daemon
    Stop,
    /// Inspect the status of the Speechyyy daemon
    Status,
    /// Inspect system permissions and input device accessibility
    Doctor,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_start_default() {
        let cli = Cli::try_parse_from(["speechyyy", "start"]).expect("parse start");
        assert_eq!(cli.command, Command::Start { foreground: false });
    }

    #[test]
    fn test_parse_start_foreground() {
        let cli = Cli::try_parse_from(["speechyyy", "start", "--foreground"])
            .expect("parse start foreground");
        assert_eq!(cli.command, Command::Start { foreground: true });
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
}
