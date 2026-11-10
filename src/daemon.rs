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
use std::fs;
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
    let mut is_restart = false;

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

        let mut had_activity = false;

        // 1. Process IPC socket connections
        match listener.accept() {
            Ok((mut stream, _)) => {
                had_activity = true;
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
                    Ok(Some(IpcRequest::Restart)) => {
                        is_restart = true;
                    }
                    Ok(Some(IpcRequest::Stop)) => {
                        is_restart = false;
                    }
                    Ok(Some(_)) | Ok(None) => {}
                    Err(ref e)
                        if e.kind() == std::io::ErrorKind::BrokenPipe
                            || e.raw_os_error() == Some(libc::EPIPE) =>
                    {
                        // Client disconnected before response could be flushed (e.g. timed out during inference); ignore silently
                    }
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

        // 2. Poll input events without hardware EVIOCGRAB (Duration::ZERO)
        if let Some((keycode, is_down)) = input_source.poll_event(Duration::ZERO) {
            had_activity = true;
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

        // 3. Sleep on the IPC listener socket until activity, a signal, or the next audio RMS tick
        if !had_activity && running.load(Ordering::SeqCst) {
            let wait_timeout = if controller.state == DictationState::Recording {
                Duration::from_millis(25).saturating_sub(last_rms_tick.elapsed())
            } else {
                Duration::from_millis(25)
            };
            if wait_timeout > Duration::ZERO {
                wait_for_listener_activity(listener, wait_timeout);
            }
        }
    }

    if let Some(b) = broadcaster {
        b.broadcast(DaemonEvent::Shutdown { restart: is_restart });
        std::thread::sleep(Duration::from_millis(25));
    }

    Ok(())
}

#[cfg(unix)]
fn wait_for_listener_activity(listener: &UnixListener, timeout: Duration) {
    use std::os::fd::AsRawFd;
    let mut pfd = libc::pollfd {
        fd: listener.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    let timeout_ms = timeout.as_millis().min(i32::MAX as u128) as libc::c_int;
    unsafe {
        libc::poll(&mut pfd, 1, timeout_ms);
    }
}

#[cfg(not(unix))]
fn wait_for_listener_activity(_listener: &UnixListener, timeout: Duration) {
    std::thread::sleep(timeout);
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DockLaunchDecision {
    Launch(PathBuf),
    SkipNoDock,
    SkipNoWayland,
    SkipAlreadyRunning,
    SkipBinaryNotFound,
}

impl DockLaunchDecision {
    pub fn is_launch(&self) -> bool {
        matches!(self, Self::Launch(_))
    }
}

/// Determines whether the companion dock should be launched based on CLI flags.
/// By default, the dock is launched in both background and foreground modes.
/// `--no-dock` suppresses dock launch and takes precedence if both `--no-dock` and `--with-dock` are supplied.
pub fn should_launch_dock_from_flags(no_dock: bool, with_dock: bool) -> bool {
    let _ = with_dock; // Accepted for backward compatibility
    !no_dock
}

pub fn should_launch_dock(
    no_dock: bool,
    wayland_display: Option<&str>,
    dock_running: bool,
    dock_binary: Option<PathBuf>,
) -> DockLaunchDecision {
    if no_dock {
        return DockLaunchDecision::SkipNoDock;
    }
    match wayland_display {
        Some(val) if !val.trim().is_empty() => {}
        _ => return DockLaunchDecision::SkipNoWayland,
    }
    if dock_running {
        return DockLaunchDecision::SkipAlreadyRunning;
    }
    match dock_binary {
        Some(path) if path.is_file() => DockLaunchDecision::Launch(path),
        _ => DockLaunchDecision::SkipBinaryNotFound,
    }
}

pub fn resolve_dock_binary(current_exe: Option<&Path>) -> Option<PathBuf> {
    if let Some(exe) = current_exe {
        if let Some(parent) = exe.parent() {
            let neighbor = parent.join("speechyyy-dock");
            if neighbor.is_file() {
                return Some(neighbor);
            }
        }
    }
    if let Ok(paths_val) = std::env::var("PATH") {
        for dir in std::env::split_paths(&paths_val) {
            let candidate = dir.join("speechyyy-dock");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

pub fn spawn_dock_process(bin_path: &Path) -> Option<u32> {
    let mut dock_cmd = ProcessCommand::new(bin_path);
    dock_cmd
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        dock_cmd.process_group(0);
    }
    match dock_cmd.spawn() {
        Ok(mut dock_child) => {
            let d_pid = dock_child.id();
            let check_start = Instant::now();
            while check_start.elapsed() < Duration::from_millis(300) {
                if let Ok(Some(status)) = dock_child.try_wait() {
                    if !status.success() {
                        eprintln!(
                            "Warning: speechyyy-dock exited immediately ({}). Run 'speechyyy doctor' to inspect Wayland compositor compatibility.",
                            status
                        );
                    }
                    return None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            if dock_child.try_wait().ok().flatten().is_none() {
                Some(d_pid)
            } else {
                None
            }
        }
        Err(e) => {
            eprintln!(
                "Warning: Failed to launch companion dock: {}. Run 'speechyyy doctor'.",
                e
            );
            None
        }
    }
}

pub fn find_running_dock_pid(paths: &RuntimePaths, proc_root: &Path) -> Option<u32> {
    let dock_pid_raw = PidFile::read_pid(&paths.dock_pid_path)?;
    #[cfg(unix)]
    let uid = unsafe { libc::geteuid() };
    #[cfg(not(unix))]
    let uid = 0;

    if crate::procs::pid_matches_uid(proc_root, dock_pid_raw, "speechyyy-dock", Some(uid)) {
        Some(dock_pid_raw)
    } else {
        // If the PID file is stale or was reused by another process, remove it and do not signal
        PidFile::remove(&paths.dock_pid_path);
        None
    }
}

pub fn is_dock_currently_running(paths: &RuntimePaths, proc_root: &Path) -> bool {
    find_running_dock_pid(paths, proc_root).is_some()
}

#[allow(clippy::too_many_arguments)]
pub fn start_daemon(
    foreground: bool,
    provider: Option<&str>,
    mode: Option<&str>,
    injection: Option<&str>,
    typing_delay: Option<u64>,
    no_dock: bool,
    with_dock: bool,
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

        let mut cmd = ProcessCommand::new(&current_exe);
        cmd.arg("start").arg("--foreground").arg("--no-dock");
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
            let wayland_display = std::env::var("WAYLAND_DISPLAY").ok();
            let dock_bin = resolve_dock_binary(Some(&current_exe));
            let proc_root = Path::new("/proc");
            let dock_running = is_dock_currently_running(paths, proc_root);

            let effective_no_dock = !should_launch_dock_from_flags(no_dock, with_dock);
            let decision = should_launch_dock(
                effective_no_dock,
                wayland_display.as_deref(),
                dock_running,
                dock_bin,
            );

            let mut active_dock_pid: Option<u32> = None;
            match decision {
                DockLaunchDecision::Launch(bin_path) => {
                    active_dock_pid = spawn_dock_process(&bin_path);
                }
                DockLaunchDecision::SkipNoDock => {
                    println!("✦ Companion dock disabled via --no-dock.");
                }
                DockLaunchDecision::SkipNoWayland => {
                    println!("✦ Wayland session not detected; companion dock skipped.");
                }
                DockLaunchDecision::SkipAlreadyRunning => {
                    println!("✦ Companion dock is already running; skipping duplicate launch.");
                    active_dock_pid = find_running_dock_pid(paths, proc_root);
                }
                DockLaunchDecision::SkipBinaryNotFound => {
                    println!("✦ Companion dock binary not found; speechyyy-dock skipped.");
                }
            }

            let config = Config::load();
            let nickname = config.nickname();
            if let Some(d_pid) = active_dock_pid {
                println!(
                    "[✓] I'm all ears, {}. Speechyyy is active and listening (PID: {}, dock PID: {}).",
                    nickname, child_pid, d_pid
                );
            } else {
                println!(
                    "[✓] I'm all ears, {}. Speechyyy is active and listening (PID: {}).",
                    nickname, child_pid
                );
            }
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
        let wayland_display = std::env::var("WAYLAND_DISPLAY").ok();
        let current_exe = std::env::current_exe().ok();
        let dock_bin = resolve_dock_binary(current_exe.as_deref());
        let proc_root = Path::new("/proc");
        let dock_running = is_dock_currently_running(paths, proc_root);

        let effective_no_dock = !should_launch_dock_from_flags(no_dock, with_dock);
        let decision = should_launch_dock(
            effective_no_dock,
            wayland_display.as_deref(),
            dock_running,
            dock_bin,
        );

        match decision {
            DockLaunchDecision::Launch(bin_path) => {
                if let Some(d_pid) = spawn_dock_process(&bin_path) {
                    println!("✦ Companion dock launched (PID: {}).", d_pid);
                }
            }
            DockLaunchDecision::SkipNoDock => {
                println!("✦ Companion dock disabled via --no-dock.");
            }
            DockLaunchDecision::SkipNoWayland => {
                println!("✦ Wayland session not detected; companion dock skipped.");
            }
            DockLaunchDecision::SkipAlreadyRunning => {
                println!("✦ Companion dock is already running; skipping duplicate launch.");
            }
            DockLaunchDecision::SkipBinaryNotFound => {
                println!("✦ Companion dock binary not found; speechyyy-dock skipped.");
            }
        }

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
        let proc_root = Path::new("/proc");
        if let Some(dock_pid) = find_running_dock_pid(paths, proc_root) {
            #[cfg(unix)]
            {
                let sink = crate::procs::RealSignalSink;
                let _ = crate::procs::terminate(&[dock_pid], Duration::from_millis(500), &sink);
            }
            PidFile::remove(&paths.dock_pid_path);
        }
        paths.cleanup();
        Ok(())
    }
}

pub fn stop_daemon(paths: &RuntimePaths) -> Result<(), String> {
    stop_all(paths)
}

/// Stops both Speechyyy and speechyyy-dock cleanly, sweeping any orphaned docks.
/// Used by `speechyyy stop`, `update`, and `uninstall`.
/// Gracefully terminates daemon and companion dock processes, PID files, and sockets.
///
/// Order of operations:
/// 1. Sends `IpcRequest::Stop` if control socket answers.
/// 2. Waits up to 1,000 ms for daemon and dock PID files to clear / processes to exit.
/// 3. Signals remaining tracked PIDs verified via `pid_matches` with SIGTERM (never SIGKILL).
/// 4. Sweeps untracked own-user `speechyyy-dock` processes with SIGTERM.
///    Note: This sweep also ends the user's dev-build docks (for example under `target/`), which is intended.
/// 5. Reports any surviving PIDs.
/// 6. Cleans PID and socket files only when processes are dead.
pub fn stop_all(paths: &RuntimePaths) -> Result<(), String> {
    #[cfg(unix)]
    let uid = unsafe { libc::geteuid() };
    #[cfg(not(unix))]
    let uid = 0;

    stop_all_with_sink_and_proc_root(
        paths,
        &crate::procs::RealSignalSink,
        Path::new("/proc"),
        uid,
    )
}

/// Inner implementation of `stop_all` with injectable `SignalSink` and `proc_root` for testing.
pub fn stop_all_with_sink_and_proc_root(
    paths: &RuntimePaths,
    sink: &dyn crate::procs::SignalSink,
    proc_root: &Path,
    uid: u32,
) -> Result<(), String> {
    let config = Config::load();
    let nickname = config.nickname();
    let current_pid = std::process::id();

    let daemon_pid = PidFile::read_pid(&paths.pid_path);
    let dock_pid = PidFile::read_pid(&paths.dock_pid_path);

    let daemon_live = daemon_pid.map(|p| sink.is_alive(p)).unwrap_or(false);
    let dock_live = dock_pid.map(|p| sink.is_alive(p)).unwrap_or(false);
    let socket_exists = paths.socket_path.exists();

    if !daemon_live
        && !dock_live
        && !socket_exists
        && !paths.pid_path.exists()
        && !paths.dock_pid_path.exists()
    {
        println!("[✓] Resting now, {}. Nothing to stop.", nickname);
        return Ok(());
    }

    // 1. If control socket is reachable, send IpcRequest::Stop
    if socket_exists {
        let _ = IpcClient::send_request(&paths.socket_path, &IpcRequest::Stop);
    }

    // 2. Wait up to 1,000 ms for the daemon and dock PID files to clear / processes to exit
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(1000) {
        let daemon_alive = daemon_pid
            .map(|p| p != current_pid && sink.is_alive(p))
            .unwrap_or(false);
        let dock_alive = dock_pid
            .map(|p| p != current_pid && sink.is_alive(p))
            .unwrap_or(false);
        let pid_files_cleared = !paths.pid_path.exists() && !paths.dock_pid_path.exists();
        if (!daemon_alive && !dock_alive) || pid_files_cleared {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    // 3. For any tracked PID still alive, verify with pid_matches and SIGTERM it.
    // If a tracked PID fails pid_matches, PID reuse occurred; remove the stale file, do NOT signal!
    let mut survivors: Vec<(u32, &'static str)> = Vec::new();

    if let Some(p) = daemon_pid {
        if p != current_pid && sink.is_alive(p) {
            if crate::procs::pid_matches_uid(proc_root, p, "speechyyy", Some(uid)) {
                let d_surv = crate::procs::terminate(&[p], Duration::from_millis(500), sink);
                survivors.extend(d_surv.into_iter().map(|pid| (pid, "Daemon")));
            } else {
                PidFile::remove(&paths.pid_path);
            }
        }
    }

    if let Some(p) = dock_pid {
        if p != current_pid && sink.is_alive(p) {
            if crate::procs::pid_matches_uid(proc_root, p, "speechyyy-dock", Some(uid)) {
                let dk_surv = crate::procs::terminate(&[p], Duration::from_millis(500), sink);
                survivors.extend(dk_surv.into_iter().map(|pid| (pid, "Dock")));
            } else {
                PidFile::remove(&paths.dock_pid_path);
            }
        }
    }

    // 4. Report survivors by PID instead of escalating to SIGKILL (never send SIGKILL)
    for (pid, label) in &survivors {
        eprintln!(
            "Warning: {} process (PID: {}) survived SIGTERM and is still running.",
            label, pid
        );
    }

    // 5. Remove PID and socket files only after owner is confirmed dead. Never remove a live process's PID file.
    let daemon_dead = !survivors.iter().any(|(_, label)| *label == "Daemon");
    let dock_dead = !survivors.iter().any(|(_, label)| *label == "Dock");

    if daemon_dead {
        PidFile::remove(&paths.pid_path);
        let _ = fs::remove_file(&paths.socket_path);
        let _ = fs::remove_file(&paths.events_socket_path);
    }
    if dock_dead {
        PidFile::remove(&paths.dock_pid_path);
    }

    println!("[✓] Resting now, {}. Speechyyy stopped.", nickname);
    Ok(())
}

pub fn restart_daemon_process(
    paths: &RuntimePaths,
    binary_override: Option<&Path>,
) -> Result<u32, String> {
    if let Some(pid) = PidFile::read_pid(&paths.pid_path) {
        if PidFile::is_process_running(pid) {
            if paths.socket_path.exists() {
                let _ = IpcClient::send_request(&paths.socket_path, &IpcRequest::Restart);
            }
            let start = Instant::now();
            while PidFile::is_process_running(pid) && start.elapsed() < Duration::from_secs(2) {
                std::thread::sleep(Duration::from_millis(50));
            }
            if PidFile::is_process_running(pid) {
                #[cfg(unix)]
                unsafe {
                    libc::kill(pid as libc::pid_t, libc::SIGTERM);
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            PidFile::remove_if_dead(&paths.pid_path);
            let _ = fs::remove_file(&paths.socket_path);
            let _ = fs::remove_file(&paths.events_socket_path);
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

    if ready {
        Ok(child_pid)
    } else {
        let _ = child.kill();
        Err("Speechyyy process timed out waiting for socket initialization".into())
    }
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
                println!("  Provider:     {}", config.inference.provider);
                println!("  Model:        {}", config.inference.model);
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
            println!("  Provider:     {}", config.inference.provider);
            println!("  Model:        {}", config.inference.model);
            return Ok(());
        }
    }

    println!(
        "[INACTIVE] Speechyyy is sleeping, {}. Run 'speechyyy start' to wake me up.",
        nickname
    );
    println!("  Provider:     {}", config.inference.provider);
    println!("  Model:        {}", config.inference.model);
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

    #[test]
    fn test_clipboard_write_failure_still_completes_the_cycle() {
        use crate::injector::{MockClipboardWriter, MockKeySink, WaylandClipboardInjector};

        let temp_dir = std::env::temp_dir().join(format!("test_clip_fail_cycle_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let paths = RuntimePaths::with_dir(temp_dir.clone());
        let broadcaster = EventBroadcaster::bind(&paths.events_socket_path).unwrap();

        let event_stream = UnixStream::connect(&paths.events_socket_path).unwrap();
        event_stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut event_reader = BufReader::new(event_stream);
        broadcaster.accept_subscribers();

        let sink = MockKeySink::new();
        let emitted_ref = Arc::clone(&sink.emitted);
        let mut injector = WaylandClipboardInjector::with_parts(
            Box::new(MockClipboardWriter::failing()),
            Box::new(sink),
            Duration::ZERO,
        );

        let engine = MockDictationEngine::with_payload("Text with failed clipboard");
        let mut controller = StateMachineController::new(TargetChord::right_alt(), engine);

        let t0 = Instant::now();
        // 1. Start recording
        handle_toggle(&mut controller, &mut injector, Some(&broadcaster), t0);
        assert_eq!(controller.state, DictationState::Recording);

        // 2. Stop recording and transcribe; clipboard fails
        let t1 = t0 + Duration::from_millis(200);
        let final_state = handle_toggle(&mut controller, &mut injector, Some(&broadcaster), t1);

        // State must return to Idle cleanly without panic
        assert_eq!(final_state, DictationState::Idle);
        assert_eq!(controller.state, DictationState::Idle);
        // Zero keys emitted
        assert_eq!(emitted_ref.lock().unwrap().len(), 0);

        // Events must still transition through Processing -> Emitting -> Idle
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

    #[test]
    fn test_run_session_loop_broken_pipe_ignored_silently() {
        use std::io::Write;

        let temp_dir = std::env::temp_dir().join(format!("test_broken_pipe_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let paths = RuntimePaths::with_dir(temp_dir.clone());
        let server = IpcServer::new(paths.clone());
        let listener = server.start_listener().unwrap();
        listener.set_nonblocking(true).unwrap();

        let running = server.running_flag();
        let running_clone = Arc::clone(&running);

        let socket_path = paths.socket_path.clone();
        let client = std::thread::spawn(move || {
            // Connect and send a request, but immediately close/shutdown the stream
            // so writing the response can encounter BrokenPipe
            if let Ok(mut stream) = UnixStream::connect(&socket_path) {
                let _ = stream.write_all(b"{\"cmd\":\"status\"}\n");
                let _ = stream.shutdown(std::net::Shutdown::Both);
                drop(stream);
            }
            std::thread::sleep(Duration::from_millis(50));
            running_clone.store(false, Ordering::SeqCst);
        });

        let mut input = NoopInputSource;
        let mut injector = RecordingInjector::new(Duration::ZERO);
        let mut controller = StateMachineController::new(
            TargetChord::right_alt(),
            MockDictationEngine::with_payload("test"),
        );

        let res = run_session_loop(
            &listener,
            &mut input,
            &mut injector,
            &mut controller,
            &running,
            "RightAlt",
            0,
        );

        client.join().unwrap();
        assert!(res.is_ok());
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_should_launch_dock_from_flags_all_combinations() {
        // 1. Defaults: neither flag passed -> dock is launched by default
        assert!(should_launch_dock_from_flags(false, false));
        // 2. Explicit with_dock -> dock is launched
        assert!(should_launch_dock_from_flags(false, true));
        // 3. Explicit no_dock -> dock is suppressed
        assert!(!should_launch_dock_from_flags(true, false));
        // 4. Both flags passed -> --no-dock wins
        assert!(!should_launch_dock_from_flags(true, true));
    }

    #[test]
    fn test_should_launch_dock_decisions() {
        let temp_dir = std::env::temp_dir().join(format!("test_dock_launch_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let dummy_bin = temp_dir.join("speechyyy-dock");
        std::fs::write(&dummy_bin, b"#!/bin/sh\nexit 0").unwrap();

        // 1. --no-dock flag skips regardless of anything else
        let res = should_launch_dock(
            true,
            Some("wayland-0"),
            false,
            Some(dummy_bin.clone()),
        );
        assert_eq!(res, DockLaunchDecision::SkipNoDock);
        assert!(!res.is_launch());

        // 2. Unset or empty WAYLAND_DISPLAY skips
        let res_no_wl = should_launch_dock(
            false,
            None,
            false,
            Some(dummy_bin.clone()),
        );
        assert_eq!(res_no_wl, DockLaunchDecision::SkipNoWayland);
        assert!(!res_no_wl.is_launch());

        let res_empty_wl = should_launch_dock(
            false,
            Some("   "),
            false,
            Some(dummy_bin.clone()),
        );
        assert_eq!(res_empty_wl, DockLaunchDecision::SkipNoWayland);

        // 3. Dock already running skips
        let res_running = should_launch_dock(
            false,
            Some("wayland-0"),
            true,
            Some(dummy_bin.clone()),
        );
        assert_eq!(res_running, DockLaunchDecision::SkipAlreadyRunning);
        assert!(!res_running.is_launch());

        // 4. Binary not found skips
        let res_no_bin = should_launch_dock(
            false,
            Some("wayland-0"),
            false,
            Some(temp_dir.join("nonexistent")),
        );
        assert_eq!(res_no_bin, DockLaunchDecision::SkipBinaryNotFound);
        assert!(!res_no_bin.is_launch());

        // 5. Valid launch
        let res_launch = should_launch_dock(
            false,
            Some("wayland-0"),
            false,
            Some(dummy_bin.clone()),
        );
        assert_eq!(res_launch, DockLaunchDecision::Launch(dummy_bin));
        assert!(res_launch.is_launch());

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_stop_daemon_four_states_and_live_pid_preservation() {
        let temp_dir = std::env::temp_dir().join(format!("test_stop_states_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let paths = RuntimePaths::with_dir(temp_dir.clone());
        paths.ensure_dir().unwrap();

        // State 4: Neither running (no socket, no pid files)
        let res = stop_daemon(&paths);
        assert!(res.is_ok());

        // State 3: Dock only (daemon socket/pid absent, dock pid exists but points to dead process)
        std::fs::write(&paths.dock_pid_path, "9999999").unwrap();
        let res = stop_daemon(&paths);
        assert!(res.is_ok());
        assert!(!paths.dock_pid_path.exists(), "Dead dock PID file must be cleaned");

        // State 2: Daemon only (daemon pid exists with dead process)
        std::fs::write(&paths.pid_path, "9999999").unwrap();
        let res = stop_daemon(&paths);
        assert!(res.is_ok());
        assert!(!paths.pid_path.exists(), "Dead daemon PID file must be cleaned");

        // State 1: Both dead PID files exist
        std::fs::write(&paths.pid_path, "9999999").unwrap();
        std::fs::write(&paths.dock_pid_path, "9999998").unwrap();
        let res = stop_daemon(&paths);
        assert!(res.is_ok());
        assert!(!paths.pid_path.exists());
        assert!(!paths.dock_pid_path.exists());

        // Live PID file preservation: if a PID file belongs to a live process, stop must not delete it if it cannot kill it
        let current_pid = std::process::id();
        std::fs::write(&paths.dock_pid_path, current_pid.to_string()).unwrap();
        PidFile::remove_if_dead(&paths.dock_pid_path);
        assert!(paths.dock_pid_path.exists(), "Live PID file must not be removed by remove_if_dead");

        let _ = std::fs::remove_file(&paths.dock_pid_path);
        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_restart_request_broadcasts_shutdown_with_restart_true() {
        let temp_dir = std::env::temp_dir()
            .join(format!("test_daemon_restart_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let paths = RuntimePaths::with_dir(temp_dir.clone());
        paths.ensure_dir().unwrap();

        let broadcaster = EventBroadcaster::bind(&paths.events_socket_path).unwrap();
        let event_stream = UnixStream::connect(&paths.events_socket_path).unwrap();
        event_stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut event_reader = BufReader::new(event_stream);
        broadcaster.accept_subscribers();

        let server = IpcServer::new(paths.clone());
        let listener = server.start_listener().unwrap();
        listener.set_nonblocking(true).unwrap();
        let running = server.running_flag();

        let mut injector = RecordingInjector::new(Duration::ZERO);
        let engine = MockDictationEngine::with_payload("hello");
        let mut controller = StateMachineController::new(TargetChord::right_alt(), engine);
        let mut input_source = NoopInputSource;

        let thread_running = Arc::clone(&running);
        let sock_path = paths.socket_path.clone();

        let server_thread = std::thread::spawn(move || {
            let _ = run_session_loop_with_broadcaster(
                &listener,
                &mut input_source,
                &mut injector,
                &mut controller,
                &thread_running,
                "RightAlt",
                10,
                Some(&broadcaster),
            );
        });

        // Send Restart request to daemon
        let resp = IpcClient::send_request(&sock_path, &IpcRequest::Restart);
        assert!(matches!(resp, Ok(IpcResponse::Ok { .. })));

        server_thread.join().expect("server thread joins cleanly");

        // Reader must receive Shutdown { restart: true }
        let mut line = String::new();
        event_reader.read_line(&mut line).unwrap();
        let event: DaemonEvent = serde_json::from_str(line.trim()).expect("parse event");
        assert_eq!(event, DaemonEvent::Shutdown { restart: true });

        // Dock decision for this event must be Reconnect (keeping the dock alive across model switch)
        assert_eq!(
            crate::dock::handle_daemon_event_action(&event),
            crate::dock::DockEventAction::Reconnect
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_stop_request_broadcasts_shutdown_with_restart_false() {
        let temp_dir = std::env::temp_dir()
            .join(format!("test_daemon_stop_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let paths = RuntimePaths::with_dir(temp_dir.clone());
        paths.ensure_dir().unwrap();

        let broadcaster = EventBroadcaster::bind(&paths.events_socket_path).unwrap();
        let event_stream = UnixStream::connect(&paths.events_socket_path).unwrap();
        event_stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut event_reader = BufReader::new(event_stream);
        broadcaster.accept_subscribers();

        let server = IpcServer::new(paths.clone());
        let listener = server.start_listener().unwrap();
        listener.set_nonblocking(true).unwrap();
        let running = server.running_flag();

        let mut injector = RecordingInjector::new(Duration::ZERO);
        let engine = MockDictationEngine::with_payload("hello");
        let mut controller = StateMachineController::new(TargetChord::right_alt(), engine);
        let mut input_source = NoopInputSource;

        let thread_running = Arc::clone(&running);
        let sock_path = paths.socket_path.clone();

        let server_thread = std::thread::spawn(move || {
            let _ = run_session_loop_with_broadcaster(
                &listener,
                &mut input_source,
                &mut injector,
                &mut controller,
                &thread_running,
                "RightAlt",
                10,
                Some(&broadcaster),
            );
        });

        // Send Stop request to daemon
        let resp = IpcClient::send_request(&sock_path, &IpcRequest::Stop);
        assert!(matches!(resp, Ok(IpcResponse::Ok { .. })));

        server_thread.join().expect("server thread joins cleanly");

        // Reader must receive Shutdown { restart: false }
        let mut line = String::new();
        event_reader.read_line(&mut line).unwrap();
        let event: DaemonEvent = serde_json::from_str(line.trim()).expect("parse event");
        assert_eq!(event, DaemonEvent::Shutdown { restart: false });

        // Dock decision for this event must be Exit (terminating dock without reconnect loop)
        assert_eq!(
            crate::dock::handle_daemon_event_action(&event),
            crate::dock::DockEventAction::Exit
        );

        let _ = std::fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_stop_all_with_fake_proc_and_recording_sink() {
        use crate::procs::SignalSink;
        use std::sync::Mutex;

        let base_temp = std::env::temp_dir()
            .join(format!("test_stop_all_fake_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base_temp);
        std::fs::create_dir_all(&base_temp).unwrap();

        #[cfg(unix)]
        let my_uid = unsafe { libc::geteuid() };
        #[cfg(not(unix))]
        let my_uid = 1000;

        // Struct for test sink
        struct TestSink {
            terms: Arc<Mutex<Vec<u32>>>,
            alive: Arc<Mutex<Vec<u32>>>,
        }
        impl SignalSink for TestSink {
            fn send_term(&self, pid: u32) -> Result<(), String> {
                self.terms.lock().unwrap().push(pid);
                self.alive.lock().unwrap().retain(|&p| p != pid);
                Ok(())
            }
            fn is_alive(&self, pid: u32) -> bool {
                self.alive.lock().unwrap().contains(&pid)
            }
        }

        let make_proc = |proc_dir: &Path, pid: u32, comm: &str| {
            let pdir = proc_dir.join(pid.to_string());
            std::fs::create_dir_all(&pdir).unwrap();
            std::fs::write(pdir.join("comm"), format!("{}\n", comm)).unwrap();
            std::fs::write(pdir.join("stat"), format!("{} ({}) S 1 1 0 0 0\n", pid, comm)).unwrap();
        };

        // Case 1: Both running
        {
            let tdir = base_temp.join("case1");
            let pdir = tdir.join("proc");
            let rdir = tdir.join("runtime");
            std::fs::create_dir_all(&pdir).unwrap();
            std::fs::create_dir_all(&rdir).unwrap();
            let paths = RuntimePaths::with_dir(rdir);

            make_proc(&pdir, 701, "speechyyy");
            make_proc(&pdir, 702, "speechyyy-dock");
            std::fs::write(&paths.pid_path, "701").unwrap();
            std::fs::write(&paths.dock_pid_path, "702").unwrap();

            let sink = TestSink {
                terms: Arc::new(Mutex::new(Vec::new())),
                alive: Arc::new(Mutex::new(vec![701, 702])),
            };
            let res = stop_all_with_sink_and_proc_root(&paths, &sink, &pdir, my_uid);
            assert!(res.is_ok());
            let terms = sink.terms.lock().unwrap().clone();
            assert!(terms.contains(&701), "Daemon must be signaled");
            assert!(terms.contains(&702), "Dock must be signaled");
            assert!(!paths.pid_path.exists());
            assert!(!paths.dock_pid_path.exists());
        }

        // Case 2: Daemon only
        {
            let tdir = base_temp.join("case2");
            let pdir = tdir.join("proc");
            let rdir = tdir.join("runtime");
            std::fs::create_dir_all(&pdir).unwrap();
            std::fs::create_dir_all(&rdir).unwrap();
            let paths = RuntimePaths::with_dir(rdir);

            make_proc(&pdir, 703, "speechyyy");
            std::fs::write(&paths.pid_path, "703").unwrap();

            let sink = TestSink {
                terms: Arc::new(Mutex::new(Vec::new())),
                alive: Arc::new(Mutex::new(vec![703])),
            };
            let res = stop_all_with_sink_and_proc_root(&paths, &sink, &pdir, my_uid);
            assert!(res.is_ok());
            assert_eq!(*sink.terms.lock().unwrap(), vec![703]);
            assert!(!paths.pid_path.exists());
        }

        // Case 3: Dock only
        {
            let tdir = base_temp.join("case3");
            let pdir = tdir.join("proc");
            let rdir = tdir.join("runtime");
            std::fs::create_dir_all(&pdir).unwrap();
            std::fs::create_dir_all(&rdir).unwrap();
            let paths = RuntimePaths::with_dir(rdir);

            make_proc(&pdir, 704, "speechyyy-dock");
            std::fs::write(&paths.dock_pid_path, "704").unwrap();

            let sink = TestSink {
                terms: Arc::new(Mutex::new(Vec::new())),
                alive: Arc::new(Mutex::new(vec![704])),
            };
            let res = stop_all_with_sink_and_proc_root(&paths, &sink, &pdir, my_uid);
            assert!(res.is_ok());
            assert_eq!(*sink.terms.lock().unwrap(), vec![704]);
            assert!(!paths.dock_pid_path.exists());
        }

        // Case 4: Neither running
        {
            let tdir = base_temp.join("case4");
            let pdir = tdir.join("proc");
            let rdir = tdir.join("runtime");
            std::fs::create_dir_all(&pdir).unwrap();
            std::fs::create_dir_all(&rdir).unwrap();
            let paths = RuntimePaths::with_dir(rdir);

            let sink = TestSink {
                terms: Arc::new(Mutex::new(Vec::new())),
                alive: Arc::new(Mutex::new(vec![])),
            };
            let res = stop_all_with_sink_and_proc_root(&paths, &sink, &pdir, my_uid);
            assert!(res.is_ok());
            assert!(sink.terms.lock().unwrap().is_empty());
        }

        // Case 5: PID reuse (PID file has live PID but comm is different app)
        {
            let tdir = base_temp.join("case5");
            let pdir = tdir.join("proc");
            let rdir = tdir.join("runtime");
            std::fs::create_dir_all(&pdir).unwrap();
            std::fs::create_dir_all(&rdir).unwrap();
            let paths = RuntimePaths::with_dir(rdir);

            make_proc(&pdir, 705, "other-daemon");
            std::fs::write(&paths.pid_path, "705").unwrap();

            let sink = TestSink {
                terms: Arc::new(Mutex::new(Vec::new())),
                alive: Arc::new(Mutex::new(vec![705])),
            };
            let res = stop_all_with_sink_and_proc_root(&paths, &sink, &pdir, my_uid);
            assert!(res.is_ok());
            // Must NOT signal PID 705!
            assert!(sink.terms.lock().unwrap().is_empty());
            // Must remove stale PID file
            assert!(!paths.pid_path.exists());
        }

        // Case 6: Dock PID file signaled, but unlinked second dock is NOT signaled
        {
            let tdir = base_temp.join("case6");
            let pdir = tdir.join("proc");
            let rdir = tdir.join("runtime");
            std::fs::create_dir_all(&pdir).unwrap();
            std::fs::create_dir_all(&rdir).unwrap();
            let paths = RuntimePaths::with_dir(rdir);

            make_proc(&pdir, 706, "speechyyy-dock");
            make_proc(&pdir, 799, "speechyyy-dock");
            std::fs::write(&paths.dock_pid_path, "706").unwrap();

            let sink = TestSink {
                terms: Arc::new(Mutex::new(Vec::new())),
                alive: Arc::new(Mutex::new(vec![706, 799])),
            };
            let res = stop_all_with_sink_and_proc_root(&paths, &sink, &pdir, my_uid);
            assert!(res.is_ok());
            let terms = sink.terms.lock().unwrap().clone();
            assert_eq!(terms, vec![706], "Only the PID-file dock must be signaled");
            assert!(!terms.contains(&799), "Untracked second dock must NOT be signaled");
            assert!(!paths.dock_pid_path.exists());
        }

        // Case 7: Stale dock PID file is removed without signaling
        {
            let tdir = base_temp.join("case7");
            let pdir = tdir.join("proc");
            let rdir = tdir.join("runtime");
            std::fs::create_dir_all(&pdir).unwrap();
            std::fs::create_dir_all(&rdir).unwrap();
            let paths = RuntimePaths::with_dir(rdir);

            std::fs::write(&paths.dock_pid_path, "707").unwrap();

            let sink = TestSink {
                terms: Arc::new(Mutex::new(Vec::new())),
                alive: Arc::new(Mutex::new(vec![])), // 707 is dead
            };
            let res = stop_all_with_sink_and_proc_root(&paths, &sink, &pdir, my_uid);
            assert!(res.is_ok());
            assert!(sink.terms.lock().unwrap().is_empty(), "Dead dock must not be signaled");
            assert!(!paths.dock_pid_path.exists(), "Stale dock PID file must be removed");
        }

        // Case 8: Dock PID reuse (alive, different comm) is not signaled
        {
            let tdir = base_temp.join("case8");
            let pdir = tdir.join("proc");
            let rdir = tdir.join("runtime");
            std::fs::create_dir_all(&pdir).unwrap();
            std::fs::create_dir_all(&rdir).unwrap();
            let paths = RuntimePaths::with_dir(rdir);

            make_proc(&pdir, 708, "firefox");
            std::fs::write(&paths.dock_pid_path, "708").unwrap();

            let sink = TestSink {
                terms: Arc::new(Mutex::new(Vec::new())),
                alive: Arc::new(Mutex::new(vec![708])),
            };
            let res = stop_all_with_sink_and_proc_root(&paths, &sink, &pdir, my_uid);
            assert!(res.is_ok());
            assert!(sink.terms.lock().unwrap().is_empty(), "Reused PID must not be signaled");
            assert!(!paths.dock_pid_path.exists(), "Mismatched dock PID file must be removed");
        }

        // Case 9: Dock owned by another UID is not signaled
        {
            let tdir = base_temp.join("case9");
            let pdir = tdir.join("proc");
            let rdir = tdir.join("runtime");
            std::fs::create_dir_all(&pdir).unwrap();
            std::fs::create_dir_all(&rdir).unwrap();
            let paths = RuntimePaths::with_dir(rdir);

            make_proc(&pdir, 709, "speechyyy-dock");
            std::fs::write(&paths.dock_pid_path, "709").unwrap();

            let sink = TestSink {
                terms: Arc::new(Mutex::new(Vec::new())),
                alive: Arc::new(Mutex::new(vec![709])),
            };
            // Call with other UID (my_uid + 9999)
            let res = stop_all_with_sink_and_proc_root(&paths, &sink, &pdir, my_uid + 9999);
            assert!(res.is_ok());
            assert!(sink.terms.lock().unwrap().is_empty(), "Dock of another UID must not be signaled");
            assert!(!paths.dock_pid_path.exists());
        }

        // Case 10: should_launch_dock ignores a dock not recorded in this instance's PID file
        {
            let tdir = base_temp.join("case10");
            let pdir = tdir.join("proc");
            let rdir = tdir.join("runtime");
            std::fs::create_dir_all(&pdir).unwrap();
            std::fs::create_dir_all(&rdir).unwrap();
            let paths = RuntimePaths::with_dir(rdir);

            // A foreign dock is running in the OS
            make_proc(&pdir, 710, "speechyyy-dock");
            // But this instance has no dock PID file
            assert_eq!(find_running_dock_pid(&paths, &pdir), None);
            assert!(!is_dock_currently_running(&paths, &pdir));

            let dummy_bin = tdir.join("speechyyy-dock");
            std::fs::write(&dummy_bin, b"#!/bin/sh\nexit 0").unwrap();

            let decision = should_launch_dock(
                false,
                Some("wayland-0"),
                is_dock_currently_running(&paths, &pdir),
                Some(dummy_bin.clone()),
            );
            assert_eq!(
                decision,
                DockLaunchDecision::Launch(dummy_bin),
                "Should launch dock despite unrecorded foreign dock process"
            );
        }

        let _ = std::fs::remove_dir_all(base_temp);
    }
}
