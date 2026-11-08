use crate::audio::capture::CpalAudioCapture;
use crate::audio::vad::EnergyVad;
use crate::config::Config;
use crate::doctor::{run_doctor, RealSysEnv};
use crate::engine::{AudioDictationEngine, DictationEngine, StateMachineController};
use crate::events::{DaemonEvent, DictationState, EventBroadcaster};
use crate::history::HistoryLogger;
use crate::inference::create_inference_provider;
use crate::injector::{create_injector, TextInjector};
use crate::input::{InputEventSource, NoopInputSource};
use crate::ipc::{IpcClient, IpcRequest, IpcResponse, IpcServer, PidFile, RuntimePaths};
use crate::text::{LearningDictionary, TextFormatter, TextPipeline};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
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
    run_session_loop_with_broadcaster(
        listener,
        input_source,
        injector,
        controller,
        running,
        hotkey,
        pacing_delay_ms,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn run_session_loop_with_broadcaster<I, J, E>(
    listener: &UnixListener,
    input_source: &mut I,
    injector: &mut J,
    controller: &mut StateMachineController<E>,
    running: &AtomicBool,
    hotkey: &str,
    pacing_delay_ms: u64,
    broadcaster: Option<&EventBroadcaster>,
) -> Result<(), String>
where
    I: InputEventSource,
    J: TextInjector,
    E: DictationEngine,
{
    let mut last_rms_tick = Instant::now();

    while running.load(Ordering::SeqCst) {
        // Broadcast audio level tick if recording
        if let Some(b) = broadcaster {
            b.accept_subscribers();
            if controller.state == DictationState::Recording
                && last_rms_tick.elapsed() >= Duration::from_millis(25)
            {
                last_rms_tick = Instant::now();
                if let Some(active_rms) = controller.active_rms() {
                    let bits = active_rms.load(Ordering::Relaxed);
                    let rms = f32::from_bits(bits);
                    b.broadcast(DaemonEvent::audio_level(rms, rms * 1.5));
                }
            }
        }

        // 1. Process IPC socket connections
        match listener.accept() {
            Ok((mut stream, _)) => {
                let state_str = controller.state.as_str();
                match IpcServer::handle_connection(
                    &mut stream,
                    running,
                    state_str,
                    hotkey,
                    pacing_delay_ms,
                ) {
                    Ok(Some(IpcRequest::PushDown)) => {
                        let prev = controller.state;
                        if let Err(e) = controller.start_recording() {
                            eprintln!("Failed to start recording on PushDown: {}", e);
                        } else if controller.state == DictationState::Recording && prev != DictationState::Recording {
                            if let Some(b) = broadcaster {
                                b.broadcast(DaemonEvent::StateChanged {
                                    state: DictationState::Recording,
                                    previous: Some(prev),
                                });
                            }
                        }
                    }
                    Ok(Some(IpcRequest::PushUp)) => {
                        if controller.state == DictationState::Recording {
                            if let Some(b) = broadcaster {
                                b.broadcast(DaemonEvent::StateChanged {
                                    state: DictationState::Processing,
                                    previous: Some(DictationState::Recording),
                                });
                            }
                            match controller.stop_and_transcribe() {
                                Ok(Some(text)) => {
                                    if let Some(b) = broadcaster {
                                        b.broadcast(DaemonEvent::StateChanged {
                                            state: DictationState::Emitting,
                                            previous: Some(DictationState::Processing),
                                        });
                                    }
                                    if let Err(e) = injector.inject(&text) {
                                        eprintln!("Failed to inject text: {}", e);
                                    }
                                    controller.finish_emission();
                                    if let Some(b) = broadcaster {
                                        b.broadcast(DaemonEvent::StateChanged {
                                            state: DictationState::Idle,
                                            previous: Some(DictationState::Emitting),
                                        });
                                    }
                                }
                                Ok(None) => {
                                    if let Some(b) = broadcaster {
                                        b.broadcast(DaemonEvent::StateChanged {
                                            state: DictationState::Idle,
                                            previous: Some(DictationState::Processing),
                                        });
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Engine error on PushUp: {}", e);
                                    if let Some(b) = broadcaster {
                                        b.broadcast(DaemonEvent::Error {
                                            code: "inference_failed".into(),
                                            message: e,
                                        });
                                        b.broadcast(DaemonEvent::StateChanged {
                                            state: DictationState::Idle,
                                            previous: Some(DictationState::Error),
                                        });
                                    }
                                    controller.reset_to_idle();
                                }
                            }
                        }
                    }
                    Ok(Some(IpcRequest::Toggle)) => {
                        handle_toggle(controller, injector, broadcaster, Instant::now());
                    }
                    Ok(Some(IpcRequest::Cancel)) => {
                        if controller.state == DictationState::Recording || controller.state == DictationState::Processing {
                            let prev = controller.state;
                            controller.cancel();
                            if let Some(b) = broadcaster {
                                b.broadcast(DaemonEvent::Canceled {
                                    reason: "ipc_cancel".into(),
                                });
                                b.broadcast(DaemonEvent::StateChanged {
                                    state: DictationState::Idle,
                                    previous: Some(prev),
                                });
                            }
                        }
                    }
                    Ok(Some(_)) | Ok(None) => {}
                    Err(e) => {
                        eprintln!("IPC connection error: {}", e);
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => {
                eprintln!("IPC socket accept error: {}", e);
            }
        }

        // 2. Poll input events (10ms tick) without hardware EVIOCGRAB
        if let Some((keycode, is_down)) = input_source.poll_event(Duration::from_millis(10)) {
            let prev = controller.state;
            match controller.handle_key_event(keycode, is_down) {
                Ok(Some(text)) => {
                    if let Some(b) = broadcaster {
                        b.broadcast(DaemonEvent::StateChanged {
                            state: DictationState::Emitting,
                            previous: Some(DictationState::Processing),
                        });
                    }
                    if let Err(e) = injector.inject(&text) {
                        eprintln!("Failed to inject text: {}", e);
                    }
                    controller.finish_emission();
                    if let Some(b) = broadcaster {
                        b.broadcast(DaemonEvent::StateChanged {
                            state: DictationState::Idle,
                            previous: Some(DictationState::Emitting),
                        });
                    }
                }
                Ok(None) => {
                    if controller.state != prev {
                        if let Some(b) = broadcaster {
                            b.broadcast(DaemonEvent::StateChanged {
                                state: controller.state,
                                previous: Some(prev),
                            });
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Engine error: {}", e);
                    if let Some(b) = broadcaster {
                        b.broadcast(DaemonEvent::Error {
                            code: "inference_failed".into(),
                            message: e,
                        });
                        b.broadcast(DaemonEvent::StateChanged {
                            state: DictationState::Idle,
                            previous: Some(DictationState::Error),
                        });
                    }
                    controller.reset_to_idle();
                }
            }
        }
    }
    Ok(())
}

/// Handles a `Command::Toggle` / `IpcRequest::Toggle` with debouncing against rapid double-toggles.
/// Returns the resulting DictationState.
pub fn handle_toggle<J: TextInjector, E: DictationEngine>(
    controller: &mut StateMachineController<E>,
    injector: &mut J,
    broadcaster: Option<&EventBroadcaster>,
    now: Instant,
) -> DictationState {
    if controller.should_debounce_toggle(now) {
        return controller.state;
    }
    controller.record_toggle(now);

    let prev = controller.state;
    match prev {
        DictationState::Idle => {
            if let Err(e) = controller.start_recording() {
                eprintln!("Failed to start recording on Toggle: {}", e);
            } else if let Some(b) = broadcaster {
                b.broadcast(DaemonEvent::StateChanged {
                    state: DictationState::Recording,
                    previous: Some(DictationState::Idle),
                });
            }
        }
        DictationState::Recording => {
            if let Some(b) = broadcaster {
                b.broadcast(DaemonEvent::StateChanged {
                    state: DictationState::Processing,
                    previous: Some(DictationState::Recording),
                });
            }
            match controller.stop_and_transcribe() {
                Ok(Some(text)) => {
                    if let Some(b) = broadcaster {
                        b.broadcast(DaemonEvent::StateChanged {
                            state: DictationState::Emitting,
                            previous: Some(DictationState::Processing),
                        });
                    }
                    if let Err(e) = injector.inject(&text) {
                        eprintln!("Failed to inject text: {}", e);
                    }
                    controller.finish_emission();
                    if let Some(b) = broadcaster {
                        b.broadcast(DaemonEvent::StateChanged {
                            state: DictationState::Idle,
                            previous: Some(DictationState::Emitting),
                        });
                    }
                }
                Ok(None) => {
                    if let Some(b) = broadcaster {
                        b.broadcast(DaemonEvent::StateChanged {
                            state: DictationState::Idle,
                            previous: Some(DictationState::Processing),
                        });
                    }
                }
                Err(e) => {
                    eprintln!("Engine error on Toggle: {}", e);
                    if let Some(b) = broadcaster {
                        b.broadcast(DaemonEvent::Error {
                            code: "inference_failed".into(),
                            message: e,
                        });
                        b.broadcast(DaemonEvent::StateChanged {
                            state: DictationState::Idle,
                            previous: Some(DictationState::Error),
                        });
                    }
                    controller.reset_to_idle();
                }
            }
        }
        _ => {}
    }
    controller.state
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
        let server = IpcServer::new(paths.clone());
        let listener = match server.start_listener() {
            Ok(l) => l,
            Err(ref e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                return Err(format!(
                    "Active Speechyyy daemon is already running (listening on {})",
                    paths.socket_path.display()
                ));
            }
            Err(e) => {
                return Err(format!(
                    "Failed to bind Speechyyy socket at {}: {}",
                    paths.socket_path.display(),
                    e
                ));
            }
        };

        if let Err(e) = PidFile::acquire(&paths.pid_path) {
            paths.cleanup();
            return Err(e);
        }

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

        let mut input_source = NoopInputSource;
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
        println!("Trigger Mode: Compositor IPC (speechyyy push-down / push-up / toggle)");
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

        let broadcaster = match EventBroadcaster::bind(&paths.events_socket_path) {
            Ok(b) => Some(b),
            Err(e) => {
                eprintln!(
                    "Warning: Failed to bind events socket at {}: {}",
                    paths.events_socket_path.display(),
                    e
                );
                None
            }
        };

        let _ = run_session_loop_with_broadcaster(
            &listener,
            &mut input_source,
            &mut injector,
            &mut controller,
            &running,
            &config.hotkey,
            config.pacing_delay_ms,
            broadcaster.as_ref(),
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

pub fn send_push_down(paths: &RuntimePaths) -> Result<(), String> {
    if !paths.socket_path.exists() {
        return Err("Speechyyy is not running. Start it first with 'speechyyy start'.".into());
    }
    match IpcClient::send_request(&paths.socket_path, &IpcRequest::PushDown) {
        Ok(IpcResponse::PushDown) | Ok(IpcResponse::Ok { .. }) => {
            println!("[✓] Recording started.");
            Ok(())
        }
        Ok(IpcResponse::Error { message }) => Err(message),
        Ok(other) => Err(format!("Unexpected response: {:?}", other)),
        Err(e) => Err(format!("Failed to communicate with Speechyyy daemon: {}", e)),
    }
}

pub fn send_push_up(paths: &RuntimePaths) -> Result<(), String> {
    if !paths.socket_path.exists() {
        return Err("Speechyyy is not running. Start it first with 'speechyyy start'.".into());
    }
    match IpcClient::send_request(&paths.socket_path, &IpcRequest::PushUp) {
        Ok(IpcResponse::PushUp) | Ok(IpcResponse::Ok { .. }) => {
            println!("[✓] Recording stopped, transcribing...");
            Ok(())
        }
        Ok(IpcResponse::Error { message }) => Err(message),
        Ok(other) => Err(format!("Unexpected response: {:?}", other)),
        Err(e) => Err(format!("Failed to communicate with Speechyyy daemon: {}", e)),
    }
}

pub fn send_toggle(paths: &RuntimePaths) -> Result<(), String> {
    if !paths.socket_path.exists() {
        return Err("Speechyyy is not running. Start it first with 'speechyyy start'.".into());
    }
    match IpcClient::send_request(&paths.socket_path, &IpcRequest::Toggle) {
        Ok(IpcResponse::Ok { message }) => {
            println!("[✓] {}", message);
            Ok(())
        }
        Ok(IpcResponse::Error { message }) => Err(message),
        Ok(other) => Err(format!("Unexpected response: {:?}", other)),
        Err(e) => Err(format!("Failed to communicate with Speechyyy daemon: {}", e)),
    }
}

pub fn send_cancel(paths: &RuntimePaths) -> Result<(), String> {
    if !paths.socket_path.exists() {
        return Err("Speechyyy is not running. Start it first with 'speechyyy start'.".into());
    }
    match IpcClient::send_request(&paths.socket_path, &IpcRequest::Cancel) {
        Ok(IpcResponse::Ok { message }) => {
            println!("[✓] {}", message);
            Ok(())
        }
        Ok(IpcResponse::Error { message }) => Err(message),
        Ok(other) => Err(format!("Unexpected response: {:?}", other)),
        Err(e) => Err(format!("Failed to communicate with Speechyyy daemon: {}", e)),
    }
}

pub fn stream_events(paths: &RuntimePaths) -> Result<(), String> {
    if !paths.events_socket_path.exists() {
        return Err(format!(
            "Speechyyy daemon is not running (events socket not found at {}). Start it first with 'speechyyy start'.",
            paths.events_socket_path.display()
        ));
    }
    let stream = UnixStream::connect(&paths.events_socket_path).map_err(|e| {
        format!(
            "Failed to connect to events socket at {}: {}",
            paths.events_socket_path.display(),
            e
        )
    })?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    while let Ok(bytes) = reader.read_line(&mut line) {
        if bytes == 0 {
            break;
        }
        print!("{}", line);
        let _ = std::io::stdout().flush();
        line.clear();
    }
    Ok(())
}

pub use crate::uninstall::{handle_uninstall_command, handle_uninstall_command_internal};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{MockDictationEngine, TargetChord};
    use crate::injector::RecordingInjector;
    use crate::input::FakeInputSource;
    use evdev::KeyCode;

    #[test]
    fn test_run_session_loop_grab_and_release() {
        let temp_dir = std::env::temp_dir().join(format!("daemon_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let sock_path = temp_dir.join("test.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();
        listener.set_nonblocking(true).unwrap();

        let mut input_source = FakeInputSource::new();
        input_source.push_event(KeyCode::KEY_RIGHTALT, true);
        input_source.push_event(KeyCode::KEY_RIGHTALT, false);

        let mut injector = RecordingInjector::new(Duration::ZERO);
        let engine = MockDictationEngine::with_payload("test transcription");
        let mut controller = StateMachineController::new(TargetChord::right_alt(), engine);
        let running = Arc::new(AtomicBool::new(true));

        let running_clone = Arc::clone(&running);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            running_clone.store(false, Ordering::SeqCst);
        });

        let _ = run_session_loop(
            &listener,
            &mut input_source,
            &mut injector,
            &mut controller,
            &running,
            "RightAlt",
            15,
        );

        assert!(!input_source.grabbed);
        assert_eq!(
            injector.injected_strings.lock().unwrap().as_slice(),
            &["test transcription".to_string()]
        );

        let _ = std::fs::remove_file(sock_path);
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_immediate_ungrab_on_keyup_before_transcription() {
        let temp_dir = std::env::temp_dir().join(format!("daemon_ungrab_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let sock_path = temp_dir.join("test.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();
        listener.set_nonblocking(true).unwrap();

        struct ObservableInputSource {
            events: std::collections::VecDeque<(KeyCode, bool)>,
            grabbed: Arc<AtomicBool>,
        }
        impl InputEventSource for ObservableInputSource {
            fn poll_event(&mut self, _timeout: Duration) -> Option<(KeyCode, bool)> {
                self.events.pop_front()
            }
            fn grab(&mut self) -> Result<(), String> {
                self.grabbed.store(true, Ordering::SeqCst);
                Ok(())
            }
            fn ungrab(&mut self) -> Result<(), String> {
                self.grabbed.store(false, Ordering::SeqCst);
                Ok(())
            }
        }

        let grabbed_flag = Arc::new(AtomicBool::new(false));
        let mut input_source = ObservableInputSource {
            events: std::collections::VecDeque::from(vec![
                (KeyCode::KEY_RIGHTALT, true),
                (KeyCode::KEY_RIGHTALT, false),
            ]),
            grabbed: Arc::clone(&grabbed_flag),
        };

        struct InspectingEngine {
            grabbed: Arc<AtomicBool>,
            was_ungrabbed_during_transcription: Arc<AtomicBool>,
        }
        impl DictationEngine for InspectingEngine {
            fn on_press(&mut self) -> Result<(), String> {
                Ok(())
            }
            fn on_release(&mut self) -> Result<Option<String>, String> {
                if !self.grabbed.load(Ordering::SeqCst) {
                    self.was_ungrabbed_during_transcription.store(true, Ordering::SeqCst);
                }
                Ok(Some("immediate ungrab verified".to_string()))
            }
        }

        let was_ungrabbed = Arc::new(AtomicBool::new(false));
        let engine = InspectingEngine {
            grabbed: Arc::clone(&grabbed_flag),
            was_ungrabbed_during_transcription: Arc::clone(&was_ungrabbed),
        };
        let mut controller = StateMachineController::new(TargetChord::right_alt(), engine);
        let mut injector = RecordingInjector::new(Duration::ZERO);
        let running = Arc::new(AtomicBool::new(true));

        let running_clone = Arc::clone(&running);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            running_clone.store(false, Ordering::SeqCst);
        });

        let _ = run_session_loop(
            &listener,
            &mut input_source,
            &mut injector,
            &mut controller,
            &running,
            "RightAlt",
            15,
        );

        assert!(
            was_ungrabbed.load(Ordering::SeqCst),
            "Keyboard MUST be ungrabbed before on_release/transcription begins!"
        );
        assert!(!grabbed_flag.load(Ordering::SeqCst));

        let _ = std::fs::remove_file(sock_path);
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_run_session_loop_ipc_push_down_and_up() {
        let temp_dir = std::env::temp_dir().join(format!("daemon_ipc_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let sock_path = temp_dir.join("test.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();
        listener.set_nonblocking(true).unwrap();

        let mut input_source = NoopInputSource;
        let mut injector = RecordingInjector::new(Duration::ZERO);
        let engine = MockDictationEngine::with_payload("IPC Transcribed Output");
        let mut controller = StateMachineController::new(TargetChord::right_alt(), engine);
        let running = Arc::new(AtomicBool::new(true));

        let loop_running = Arc::clone(&running);
        let client_sock = sock_path.clone();
        let loop_thread = std::thread::spawn(move || {
            let _ = run_session_loop(
                &listener,
                &mut input_source,
                &mut injector,
                &mut controller,
                &loop_running,
                "RightAlt",
                15,
            );
            injector.injected_strings.lock().unwrap().clone()
        });

        // 1. Send PushDown via IPC
        let down_res = IpcClient::send_request(&client_sock, &IpcRequest::PushDown);
        assert_eq!(down_res, Ok(IpcResponse::PushDown));

        // 2. Query Status via IPC to verify Recording state
        let status_res = IpcClient::send_request(&client_sock, &IpcRequest::Status);
        match status_res {
            Ok(IpcResponse::Status { state, .. }) => assert_eq!(state, "Recording"),
            other => panic!("Expected status Recording, got {:?}", other),
        }

        // 3. Send PushUp via IPC
        let up_res = IpcClient::send_request(&client_sock, &IpcRequest::PushUp);
        assert_eq!(up_res, Ok(IpcResponse::PushUp));

        // 4. Send Stop
        let stop_res = IpcClient::send_request(&client_sock, &IpcRequest::Stop);
        assert!(matches!(stop_res, Ok(IpcResponse::Ok { .. })));

        let injected = loop_thread.join().unwrap();
        assert_eq!(injected, vec!["IPC Transcribed Output".to_string()]);

        let _ = std::fs::remove_file(sock_path);
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_run_session_loop_ipc_toggle() {
        let temp_dir = std::env::temp_dir().join(format!("daemon_toggle_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let sock_path = temp_dir.join("test.sock");
        let listener = UnixListener::bind(&sock_path).unwrap();
        listener.set_nonblocking(true).unwrap();

        let mut input_source = NoopInputSource;
        let mut injector = RecordingInjector::new(Duration::ZERO);
        let engine = MockDictationEngine::with_payload("Toggled Transcription");
        let mut controller = StateMachineController::new(TargetChord::right_alt(), engine);
        let running = Arc::new(AtomicBool::new(true));

        let loop_running = Arc::clone(&running);
        let client_sock = sock_path.clone();
        let loop_thread = std::thread::spawn(move || {
            let _ = run_session_loop(
                &listener,
                &mut input_source,
                &mut injector,
                &mut controller,
                &loop_running,
                "RightAlt",
                15,
            );
            injector.injected_strings.lock().unwrap().clone()
        });

        // 1. First toggle starts recording
        let toggle1 = IpcClient::send_request(&client_sock, &IpcRequest::Toggle);
        assert!(matches!(toggle1, Ok(IpcResponse::Ok { .. })));

        let status1 = IpcClient::send_request(&client_sock, &IpcRequest::Status);
        match status1 {
            Ok(IpcResponse::Status { state, .. }) => assert_eq!(state, "Recording"),
            other => panic!("Expected Recording, got {:?}", other),
        }

        // 2. Second toggle stops recording and injects (spaced past 150ms debounce threshold)
        std::thread::sleep(Duration::from_millis(160));
        let toggle2 = IpcClient::send_request(&client_sock, &IpcRequest::Toggle);
        assert!(matches!(toggle2, Ok(IpcResponse::Ok { .. })));

        // 3. Stop daemon
        let _ = IpcClient::send_request(&client_sock, &IpcRequest::Stop);

        let injected = loop_thread.join().unwrap();
        assert_eq!(injected, vec!["Toggled Transcription".to_string()]);

        let _ = std::fs::remove_file(sock_path);
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_toggle_debounce_ignores_rapid_second_toggle() {
        let mut injector = RecordingInjector::new(Duration::ZERO);
        let engine = MockDictationEngine::with_payload("Debounced Payload");
        let mut controller = StateMachineController::new(TargetChord::right_alt(), engine);

        let t0 = Instant::now();
        // 1. First toggle at t0: starts recording
        let state1 = handle_toggle(&mut controller, &mut injector, None, t0);
        assert_eq!(state1, DictationState::Recording);
        assert_eq!(controller.state, DictationState::Recording);

        // 2. Rapid second toggle 50ms later (< 150ms): MUST be ignored
        let t1 = t0 + Duration::from_millis(50);
        let state2 = handle_toggle(&mut controller, &mut injector, None, t1);
        assert_eq!(state2, DictationState::Recording);
        assert_eq!(controller.state, DictationState::Recording);
        assert_eq!(injector.injected_strings.lock().unwrap().len(), 0);
    }

    #[test]
    fn test_toggle_debounce_allows_spaced_toggles() {
        let mut injector = RecordingInjector::new(Duration::ZERO);
        let engine = MockDictationEngine::with_payload("Spaced Payload");
        let mut controller = StateMachineController::new(TargetChord::right_alt(), engine);

        let t0 = Instant::now();
        // 1. First toggle at t0: Idle -> Recording
        let state1 = handle_toggle(&mut controller, &mut injector, None, t0);
        assert_eq!(state1, DictationState::Recording);

        // 2. Second toggle 200ms later (> 150ms): Recording -> Idle (via Processing & Emitting)
        let t1 = t0 + Duration::from_millis(200);
        let state2 = handle_toggle(&mut controller, &mut injector, None, t1);
        assert_eq!(state2, DictationState::Idle);
        assert_eq!(controller.state, DictationState::Idle);
        assert_eq!(injector.injected_strings.lock().unwrap().as_slice(), &["Spaced Payload"]);
    }

    #[test]
    fn test_debounce_is_toggle_only() {
        let engine = MockDictationEngine::with_payload("Non Toggle Payload");
        let mut controller = StateMachineController::new(TargetChord::right_alt(), engine);

        let t0 = Instant::now();
        // 1. Toggle into recording at t0
        controller.toggle_at(t0).unwrap();
        assert_eq!(controller.state, DictationState::Recording);

        // 2. Non-toggle command (stop_and_transcribe) immediately 10ms later is NOT debounced
        let text = controller.stop_and_transcribe().unwrap();
        assert_eq!(text, Some("Non Toggle Payload".to_string()));
        assert_eq!(controller.state, DictationState::Emitting);
    }

    #[test]
    fn test_skip_path_still_completes_the_cycle() {
        let temp_dir = std::env::temp_dir().join(format!("test_empty_cycle_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let paths = RuntimePaths::with_dir(temp_dir.clone());
        let broadcaster = EventBroadcaster::bind(&paths.events_socket_path).unwrap();

        let event_stream = UnixStream::connect(&paths.events_socket_path).unwrap();
        event_stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut event_reader = BufReader::new(event_stream);
        broadcaster.accept_subscribers();

        let mut injector = RecordingInjector::new(Duration::ZERO);
        // Engine returns whitespace-only payload
        let engine = MockDictationEngine::with_payload("   ");
        let mut controller = StateMachineController::new(TargetChord::right_alt(), engine);

        let t0 = Instant::now();
        // 1. Start recording
        handle_toggle(&mut controller, &mut injector, Some(&broadcaster), t0);
        assert_eq!(controller.state, DictationState::Recording);

        // 2. Stop recording and transcribe with whitespace output
        let t1 = t0 + Duration::from_millis(200);
        handle_toggle(&mut controller, &mut injector, Some(&broadcaster), t1);

        // State must return to Idle
        assert_eq!(controller.state, DictationState::Idle);
        // Zero keycodes emitted!
        assert_eq!(injector.emitted_events.lock().unwrap().len(), 0);

        // Post-emission events must be published: Recording, Processing, Emitting, Idle
        let mut received_states = Vec::new();
        for _ in 0..4 {
            let mut line = String::new();
            if event_reader.read_line(&mut line).is_ok() && !line.is_empty() {
                if let Ok(DaemonEvent::StateChanged { state, .. }) = serde_json::from_str(line.trim()) {
                    received_states.push(state);
                }
            }
        }

        assert_eq!(
            received_states,
            vec![
                DictationState::Recording,
                DictationState::Processing,
                DictationState::Emitting,
                DictationState::Idle,
            ]
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
