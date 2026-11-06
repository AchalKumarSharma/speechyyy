use crate::audio::capture::CpalAudioCapture;
use crate::audio::vad::EnergyVad;
use crate::config::Config;
use crate::doctor::{run_doctor, RealSysEnv};
use crate::engine::{AudioDictationEngine, DictationEngine, StateMachineController};
use crate::history::HistoryLogger;
use crate::inference::create_inference_provider;
use crate::injector::{create_injector, TextInjector};
use crate::input::{EvdevListener, InputEventSource};
use crate::ipc::{IpcClient, IpcRequest, IpcResponse, IpcServer, PidFile, RuntimePaths};
use crate::text::{LearningDictionary, TextFormatter, TextPipeline};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(unix)]
pub fn register_termination_signals(
    running: Arc<AtomicBool>,
) -> Result<signal_hook::iterator::backend::Handle, std::io::Error> {
    let mut signals = signal_hook::iterator::Signals::new([
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGTERM,
    ])?;

    let handle = signals.handle();
    std::thread::spawn(move || {
        if let Some(sig) = signals.forever().next() {
            eprintln!("\nReceived termination signal ({}), shutting down daemon...", sig);
            running.store(false, Ordering::SeqCst);
        }
    });
    Ok(handle)
}

pub fn run_session_loop<I, J, E>(
    listener: &UnixListener,
    input_source: &mut I,
    injector: &mut J,
    controller: &mut StateMachineController<E>,
    running: &AtomicBool,
    hotkey: &str,
    pacing_delay_ms: u64,
) -> Result<(), String>
where
    I: InputEventSource,
    J: TextInjector,
    E: DictationEngine,
{
    while running.load(Ordering::SeqCst) {
        // 1. Process IPC socket connections
        match listener.accept() {
            Ok((mut stream, _)) => {
                let state_str = controller.state.as_str();
                let _ = IpcServer::handle_connection(
                    &mut stream,
                    running,
                    state_str,
                    hotkey,
                    pacing_delay_ms,
                );
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => {
                eprintln!("IPC socket accept error: {}", e);
            }
        }

        // 2. Poll input events (10ms tick)
        if let Some((keycode, is_down)) = input_source.poll_event(Duration::from_millis(10)) {
            match controller.handle_key_event(keycode, is_down) {
                Ok(Some(text)) => {
                    if let Err(e) = injector.inject(&text) {
                        eprintln!("Failed to inject text: {}", e);
                    }
                    controller.finish_emission();
                }
                Ok(None) => {}
                Err(e) => {
                    eprintln!("Engine error: {}", e);
                }
            }
        }
    }
    Ok(())
}

pub fn start_daemon(
    foreground: bool,
    provider: Option<&str>,
    mode: Option<&str>,
    injection: Option<&str>,
    typing_delay: Option<u64>,
    paths: &RuntimePaths,
) -> Result<(), String> {
    paths.ensure_dir().map_err(|e| {
        format!(
            "Failed to create runtime directory {}: {}",
            paths.dir.display(),
            e
        )
    })?;

    // Pre-flight check
    let mut check_config = Config::load();
    check_config.apply_cli_overrides(provider, mode, injection, typing_delay);
    if check_config.inference.provider != "mock"
        && check_config.inference.provider != "custom"
        && check_config.inference.model.trim().is_empty()
    {
        return Err(
            "No active model configured. Run 'speechyyy use <model>' to activate a model before starting Speechyyy.".into(),
        );
    }

    let env = RealSysEnv;
    let report = run_doctor(&env);
    if !report.is_ready() {
        eprintln!("[ERROR] Pre-flight system and configuration checks failed for Speechyyy.");
        eprintln!("Run 'speechyyy doctor' to inspect and fix permissions or configuration.");
        return Err("Pre-flight check failed".into());
    }

    if !foreground {
        if let Some(pid) = PidFile::read_pid(&paths.pid_path) {
            if PidFile::is_process_running(pid) {
                let config = Config::load();
                let nickname = config.nickname();
                return Err(format!(
                    "Speechyyy is already active and listening for you, {} (PID: {})",
                    nickname, pid
                ));
            }
        }

        let current_exe = std::env::current_exe()
            .map_err(|e| format!("Failed to determine binary path: {}", e))?;

        let mut cmd = ProcessCommand::new(current_exe);
        cmd.arg("start").arg("--foreground");
        if let Some(p) = provider {
            cmd.arg("--provider").arg(p);
        }
        if let Some(m) = mode {
            cmd.arg("--mode").arg(m);
        }
        if let Some(inj) = injection {
            cmd.arg("--injection").arg(inj);
        }
        if let Some(d) = typing_delay {
            cmd.arg("--typing-delay").arg(d.to_string());
        }

        let mut child = cmd
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to spawn Speechyyy process: {}", e))?;

        let child_pid = child.id();

        let start = Instant::now();
        let mut ready = false;
        while start.elapsed() < Duration::from_secs(2) {
            if paths.socket_path.exists() {
                if let Ok(IpcResponse::Pong) =
                    IpcClient::send_request(&paths.socket_path, &IpcRequest::Ping)
                {
                    ready = true;
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        if ready {
            let config = Config::load();
            let nickname = config.nickname();
            println!(
                "[✓] I'm all ears, {}. Speechyyy is active and listening (PID: {}).",
                nickname, child_pid
            );
            Ok(())
        } else {
            if let Ok(Some(status)) = child.try_wait() {
                Err(format!(
                    "Speechyyy process exited immediately with status: {}",
                    status
                ))
            } else {
                let _ = child.kill();
                Err("Speechyyy process timed out waiting for socket initialization".into())
            }
        }
    } else {
        PidFile::acquire(&paths.pid_path)?;

        let server = IpcServer::new(paths.clone());
        let listener = match server.start_listener() {
            Ok(l) => l,
            Err(e) => {
                paths.cleanup();
                return Err(format!(
                    "Failed to bind Speechyyy socket at {}: {:?}",
                    paths.socket_path.display(),
                    e
                ));
            }
        };

        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let running = server.running_flag();

        let mut config = Config::load();
        config.apply_cli_overrides(provider, mode, injection, typing_delay);
        let target_chord = config.to_target_chord();

        let mut injector = match create_injector(&config) {
            Ok(inj) => inj,
            Err(e) => {
                paths.cleanup();
                return Err(format!("Failed to initialize text injector: {}", e));
            }
        };

        let mut input_source = EvdevListener::new();
        let capture = match CpalAudioCapture::new() {
            Ok(c) => c,
            Err(e) => {
                paths.cleanup();
                return Err(format!(
                    "Failed to initialize audio capture: {}. Check 'speechyyy doctor'.",
                    e
                ));
            }
        };
        let vad = EnergyVad::from_config(&config.audio);
        let inference = match create_inference_provider(&config.inference) {
            Ok(p) => p,
            Err(e) => {
                paths.cleanup();
                return Err(format!("Failed to initialize speech inference: {}", e));
            }
        };
        let pipeline = TextPipeline::new(TextFormatter::default(), LearningDictionary::load());
        let history = Some(HistoryLogger::new());
        let engine = AudioDictationEngine::with_pipeline_and_history(
            capture, vad, inference, pipeline, history,
        );
        let mut controller = StateMachineController::new(target_chord, engine);

        println!(
            "Speechyyy running in foreground (PID: {}).",
            std::process::id()
        );
        println!("Push-to-Talk Hotkey: {:?}", config.hotkey);
        println!("Pacing Delay: {}ms", config.pacing_delay_ms);
        println!("Text Injector: {}", injector.name());
        println!("Inference Provider: {}", config.inference.provider);
        if config.inference.provider.to_lowercase() == "sarvam" {
            println!("Sarvam Model: {}", config.inference.model);
            println!("Sarvam Language: {}", config.inference.language_code);
        }

        #[cfg(unix)]
        let signal_handle = register_termination_signals(Arc::clone(&running))
            .map_err(|e| format!("Failed to register POSIX signal handlers: {}", e))?;

        let _ = run_session_loop(
            &listener,
            &mut input_source,
            &mut injector,
            &mut controller,
            &running,
            &config.hotkey,
            config.pacing_delay_ms,
        );

        #[cfg(unix)]
        signal_handle.close();

        println!("Shutting down Speechyyy...");
        paths.cleanup();
        Ok(())
    }
}

pub fn stop_daemon(paths: &RuntimePaths) -> Result<(), String> {
    let config = Config::load();
    let nickname = config.nickname();
    if !paths.socket_path.exists() && !paths.pid_path.exists() {
        println!("[✓] Resting now, {}. Speechyyy stopped.", nickname);
        return Ok(());
    }

    let existing_pid = PidFile::read_pid(&paths.pid_path);

    if paths.socket_path.exists() {
        if let Ok(IpcResponse::Ok { .. }) =
            IpcClient::send_request(&paths.socket_path, &IpcRequest::Stop)
        {
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(2) {
                if let Some(pid) = existing_pid {
                    if !PidFile::is_process_running(pid) {
                        break;
                    }
                } else if !paths.socket_path.exists() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            paths.cleanup();
            println!("[✓] Resting now, {}. Speechyyy stopped.", nickname);
            return Ok(());
        }
    }

    if let Some(pid) = existing_pid {
        let current_pid = std::process::id();
        if pid != current_pid && PidFile::is_process_running(pid) {
            println!("Stopping Speechyyy (PID: {})...", pid);
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGTERM);
            }
            std::thread::sleep(Duration::from_millis(100));
            paths.cleanup();
            println!("[✓] Resting now, {}. Speechyyy stopped.", nickname);
            return Ok(());
        }
    }

    paths.cleanup();
    println!("[✓] Resting now, {}. Speechyyy stopped.", nickname);
    Ok(())
}

pub fn restart_daemon_process(
    paths: &RuntimePaths,
    binary_override: Option<&Path>,
) -> Result<u32, String> {
    if let Some(pid) = PidFile::read_pid(&paths.pid_path) {
        if PidFile::is_process_running(pid) {
            let _ = stop_daemon(paths);
            let proc_path = PathBuf::from(format!("/proc/{}", pid));
            let start = Instant::now();
            while (proc_path.exists() || PidFile::is_process_running(pid))
                && start.elapsed() < Duration::from_secs(2)
            {
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }

    let binary_path = match binary_override {
        Some(p) => p.to_path_buf(),
        None => std::env::current_exe()
            .map_err(|e| format!("Failed to determine binary path: {}", e))?,
    };

    let mut cmd = ProcessCommand::new(binary_path);
    cmd.arg("start").arg("--foreground");

    let mut child = cmd
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to spawn Speechyyy process: {}", e))?;

    let child_pid = child.id();

    let start = Instant::now();
    let mut ready = false;
    while start.elapsed() < Duration::from_secs(2) {
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!(
                "Speechyyy process exited prematurely with status: {}",
                status
            ));
        }
        if paths.socket_path.exists() {
            if let Ok(IpcResponse::Pong) =
                IpcClient::send_request(&paths.socket_path, &IpcRequest::Ping)
            {
                ready = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    if !ready {
        let _ = child.kill();
        return Err("Timed out waiting for restarted Speechyyy to initialize.".into());
    }

    Ok(child_pid)
}

pub fn check_status(paths: &RuntimePaths) -> Result<(), String> {
    let config = Config::load();
    let nickname = config.nickname();
    if paths.socket_path.exists() {
        match IpcClient::send_request(&paths.socket_path, &IpcRequest::Status) {
            Ok(IpcResponse::Status {
                pid,
                state,
                hotkey,
                pacing_delay_ms,
            }) => {
                println!(
                    "[ACTIVE] Speechyyy is active and ready for you, {} (PID: {}).",
                    nickname, pid
                );
                println!("  PID:          {}", pid);
                println!("  State:        {}", state);
                println!("  Hotkey:       {}", hotkey);
                println!("  Pacing Delay: {}ms", pacing_delay_ms);
                println!("  Socket:       {}", paths.socket_path.display());
                return Ok(());
            }
            Ok(other) => {
                println!(
                    "[WARN] Speechyyy returned unexpected status response: {:?}",
                    other
                );
                return Ok(());
            }
            Err(_) => {}
        }
    }

    if let Some(pid) = PidFile::read_pid(&paths.pid_path) {
        if PidFile::is_process_running(pid) {
            println!(
                "[ACTIVE] Speechyyy is active and ready for you, {} (PID: {}).",
                nickname, pid
            );
            return Ok(());
        }
    }

    println!(
        "[INACTIVE] Speechyyy is sleeping, {}. Run 'speechyyy start' to wake me up.",
        nickname
    );
    Ok(())
}

pub use crate::uninstall::{handle_uninstall_command, handle_uninstall_command_internal};
