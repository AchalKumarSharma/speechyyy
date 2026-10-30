use clap::Parser;
use speechyyy::cli::{Cli, Command};
use speechyyy::daemon::{check_status, start_daemon, stop_daemon};
use speechyyy::doctor::{print_doctor_report, run_doctor, RealSysEnv};
use speechyyy::ipc::RuntimePaths;

fn main() {
    let cli = Cli::parse();
    let paths = RuntimePaths::new();

    match cli.command {
        Command::Doctor => {
            let env = RealSysEnv;
            let report = run_doctor(&env);
            print_doctor_report(&report);
            if !report.is_ready() {
                std::process::exit(1);
            }
        }
        Command::Start { foreground } => {
            if let Err(e) = start_daemon(foreground, &paths) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Command::Stop => {
            if let Err(e) = stop_daemon(&paths) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Command::Status => {
            if let Err(e) = check_status(&paths) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    }
}
