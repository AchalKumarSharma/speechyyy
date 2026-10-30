use crate::audio::capture::CpalAudioCapture;
use crate::audio::vad::EnergyVad;
use crate::config::Config;
use crate::doctor::{run_doctor, RealSysEnv};
use crate::engine::{AudioDictationEngine, DictationEngine, StateMachineController};
use crate::injector::{TextInjector, UinputInjector};
use crate::input::{EvdevListener, InputEventSource};
use crate::ipc::{IpcClient, IpcRequest, IpcResponse, IpcServer, PidFile, RuntimePaths};
use std::os::unix::net::UnixListener;
use std::process::Command as ProcessCommand;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

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

pub fn start_daemon(foreground: bool, paths: &RuntimePaths) -> Result<(), String> {
    if let Some(parent) = paths.pid_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            format!(
                "Failed to create runtime directory {}: {}",
                parent.display(),
                e
            )
        })?;
    }
    if let Some(parent) = paths.socket_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            format!(
                "Failed to create runtime directory {}: {}",
                parent.display(),
                e
            )
        })?;
    }

    // Pre-flight check
    let env = RealSysEnv;
    let report = run_doctor(&env);
    if !report.is_ready() {
        eprintln!("[ERROR] System permissions missing for Speechyyy.");
        eprintln!("Run 'speechyyy doctor' to inspect and fix permissions.");
        return Err("Pre-flight permission check failed".into());
    }

    if !foreground {
        if let Some(pid) = PidFile::read_pid(&paths.pid_path) {
            if PidFile::is_process_running(pid) {
                return Err(format!(
                    "Speechyyy daemon is already running with PID {}",
                    pid
                ));
            }
        }

        let current_exe = std::env::current_exe()
            .map_err(|e| format!("Failed to determine binary path: {}", e))?;

        let mut child = ProcessCommand::new(current_exe)
            .arg("start")
            .arg("--foreground")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to spawn daemon process: {}", e))?;

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
            println!(
                "Speechyyy daemon started successfully (PID: {}).",
                child_pid
            );
            Ok(())
        } else {
            if let Ok(Some(status)) = child.try_wait() {
                Err(format!(
                    "Daemon process exited immediately with status: {}",
                    status
                ))
            } else {
                let _ = child.kill();
                Err("Daemon process timed out waiting for socket initialization".into())
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
                    "Failed to bind daemon socket at {}: {:?}",
                    paths.socket_path.display(),
                    e
                ));
            }
        };

        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let running = server.running_flag();

        let config = Config::load();
        let target_chord = config.to_target_chord();
        let pacing = Duration::from_millis(config.pacing_delay_ms);

        let mut injector = match UinputInjector::new(pacing) {
            Ok(inj) => inj,
            Err(e) => {
                paths.cleanup();
                return Err(e);
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
        let engine = AudioDictationEngine::new(capture, vad);
        let mut controller = StateMachineController::new(target_chord, engine);

        println!(
            "Speechyyy daemon running in foreground (PID: {}).",
            std::process::id()
        );
        println!("Push-to-Talk Hotkey: {:?}", config.hotkey);
        println!("Pacing Delay: {}ms", config.pacing_delay_ms);

        let _ = run_session_loop(
            &listener,
            &mut input_source,
            &mut injector,
            &mut controller,
            &running,
            &config.hotkey,
            config.pacing_delay_ms,
        );

        println!("Shutting down Speechyyy daemon...");
        paths.cleanup();
        Ok(())
    }
}

pub fn stop_daemon(paths: &RuntimePaths) -> Result<(), String> {
    if !paths.socket_path.exists() && !paths.pid_path.exists() {
        println!("Speechyyy daemon is not running.");
        return Ok(());
    }

    let existing_pid = PidFile::read_pid(&paths.pid_path);

    if paths.socket_path.exists() {
        if let Ok(IpcResponse::Ok { message }) =
            IpcClient::send_request(&paths.socket_path, &IpcRequest::Stop)
        {
            println!("{}", message);
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
            println!("Speechyyy daemon stopped.");
            return Ok(());
        }
    }

    if let Some(pid) = existing_pid {
        let current_pid = std::process::id();
        if pid != current_pid && PidFile::is_process_running(pid) {
            println!("Sending SIGTERM to PID {}...", pid);
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGTERM);
            }
            std::thread::sleep(Duration::from_millis(100));
            paths.cleanup();
            println!("Speechyyy daemon stopped.");
            return Ok(());
        }
    }

    paths.cleanup();
    println!("Cleaned up stale daemon files. Daemon was not running.");
    Ok(())
}

pub fn check_status(paths: &RuntimePaths) -> Result<(), String> {
    if paths.socket_path.exists() {
        match IpcClient::send_request(&paths.socket_path, &IpcRequest::Status) {
            Ok(IpcResponse::Status {
                pid,
                state,
                hotkey,
                pacing_delay_ms,
            }) => {
                println!("[ACTIVE] Speechyyy daemon is running.");
                println!("  PID:          {}", pid);
                println!("  State:        {}", state);
                println!("  Hotkey:       {}", hotkey);
                println!("  Pacing Delay: {}ms", pacing_delay_ms);
                println!("  Socket:       {}", paths.socket_path.display());
                return Ok(());
            }
            Ok(other) => {
                println!(
                    "[WARN] Daemon returned unexpected status response: {:?}",
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
                "[ACTIVE] Daemon process found (PID: {}), but socket is unresponsive.",
                pid
            );
            return Ok(());
        }
    }

    println!("[INACTIVE] Speechyyy daemon is not running.");
    Ok(())
}
