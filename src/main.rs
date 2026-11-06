use clap::Parser;
use speechyyy::cli::{Cli, Command, ModelCommands};
use speechyyy::daemon::{check_status, handle_uninstall_command, start_daemon, stop_daemon};
use speechyyy::doctor::{print_doctor_report, run_doctor, RealSysEnv};
use speechyyy::history::{handle_fix_command_opts, handle_history_command};
use speechyyy::ipc::RuntimePaths;
use speechyyy::model::{
    handle_model_download_command, handle_model_list_command, handle_model_remove,
    handle_model_use_command,
};
use speechyyy::text::{handle_learn_command_opts, handle_unlearn_command};

fn main() {
    let cli = Cli::parse();
    let paths = RuntimePaths::new();

    let result = match cli.command {
        Command::Learn { wrong, correct } => {
            handle_learn_command_opts(wrong.as_deref(), correct.as_deref())
        }
        Command::Fix { corrected_phrase } => {
            handle_fix_command_opts(corrected_phrase.as_deref())
        }
        Command::Unlearn { key } => handle_unlearn_command(key.as_deref()),
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
        Command::Models { action } => match action {
            Some(ModelCommands::Download {
                model,
                model_flag,
                force,
            }) => {
                let target = ModelCommands::resolved_download_model(model, model_flag);
                handle_model_download_command(&target, force)
            }
            Some(ModelCommands::List) | None => handle_model_list_command(),
            Some(ModelCommands::Use { model }) => handle_model_use_command(&model),
            Some(ModelCommands::Remove { model }) => handle_model_remove(&model),
        },
        Command::Download {
            model,
            model_flag,
            force,
        } => {
            let target = ModelCommands::resolved_download_model(model, model_flag);
            handle_model_download_command(&target, force)
        }
        Command::Use { model } => handle_model_use_command(&model),
        Command::Remove { model } => handle_model_remove(&model),
        Command::Uninstall { yes } => handle_uninstall_command(yes),
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
        Command::Update { force } => speechyyy::update::handle_update_command(force),
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
