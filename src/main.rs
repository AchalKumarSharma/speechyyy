use clap::Parser;
use speechyyy::cli::{Cli, Command};
use speechyyy::daemon::{check_status, start_daemon, stop_daemon};
use speechyyy::doctor::{print_doctor_report, run_doctor, RealSysEnv};
use speechyyy::history::{handle_fix_command, handle_history_command};
use speechyyy::ipc::RuntimePaths;
use speechyyy::model::{
    handle_model_download_command, handle_model_list_command, handle_model_use_command,
};
use speechyyy::text::handle_learn_command;

fn main() {
    let cli = Cli::parse();
    let paths = RuntimePaths::new();

    let result = match cli.command {
        Command::Learn { wrong, correct } => handle_learn_command(&wrong, &correct),
        Command::Fix { corrected_phrase } => handle_fix_command(&corrected_phrase),
        Command::History { limit } => handle_history_command(limit),
        Command::Doctor => {
            let env = RealSysEnv;
            let report = run_doctor(&env);
            print_doctor_report(&report);
            if !report.is_ready() {
                std::process::exit(1);
            }
            Ok(())
        }
        Command::Model { action } => match action {
            speechyyy::cli::ModelCommands::Download { model } => {
                handle_model_download_command(model)
            }
            speechyyy::cli::ModelCommands::List => handle_model_list_command(),
            speechyyy::cli::ModelCommands::Use { model } => handle_model_use_command(&model),
        },
        Command::Start {
            foreground,
            provider,
            mode,
            injection,
            typing_delay,
        } => start_daemon(
            foreground,
            provider.as_deref(),
            mode.as_deref(),
            injection.as_deref(),
            typing_delay,
            &paths,
        ),
        Command::Stop => stop_daemon(&paths),
        Command::Status => check_status(&paths),
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
