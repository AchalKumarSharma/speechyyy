use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerShellCheckResult {
    Supported,
    Unsupported,
    CannotConnect,
    Timeout,
    ExecutionFailed,
}

pub trait SysEnv {
    fn current_username(&self) -> String;
    fn user_groups(&self) -> Vec<String>;
    fn can_write_path(&self, path: &Path) -> bool;
    fn check_audio_input(&self) -> Result<String, String>;
    fn check_inference(&self) -> Result<String, String>;

    fn is_wayland(&self) -> bool {
        false
    }

    fn has_command(&self, _cmd: &str) -> bool {
        false
    }

    fn find_dock_binary(&self) -> Option<std::path::PathBuf> {
        None
    }

    fn is_executable(&self, _path: &Path) -> bool {
        false
    }

    fn check_layer_shell(&self, _dock_bin: &Path) -> Result<LayerShellCheckResult, String> {
        Ok(LayerShellCheckResult::Supported)
    }

    fn current_desktop(&self) -> String {
        std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "current desktop".into())
    }
}

pub struct RealSysEnv;

impl SysEnv for RealSysEnv {
    fn current_username(&self) -> String {
        std::env::var("USER")
            .or_else(|_| std::env::var("LOGNAME"))
            .unwrap_or_else(|_| "current_user".to_string())
    }

    fn user_groups(&self) -> Vec<String> {
        let username = self.current_username();
        let mut groups = Vec::new();

        let mut gid_to_name: HashMap<u32, String> = HashMap::new();
        let mut user_in_group_names = Vec::new();

        if let Ok(content) = fs::read_to_string("/etc/group") {
            for line in content.lines() {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 4 {
                    let group_name = parts[0].to_string();
                    if let Ok(gid) = parts[2].parse::<u32>() {
                        gid_to_name.insert(gid, group_name.clone());
                    }
                    let members: Vec<&str> =
                        parts[3].split(',').filter(|s| !s.is_empty()).collect();
                    if members.contains(&username.as_str()) {
                        user_in_group_names.push(group_name);
                    }
                }
            }
        }

        if let Ok(status) = fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if let Some(rest) = line.strip_prefix("Groups:") {
                    for gid_str in rest.split_whitespace() {
                        if let Ok(gid) = gid_str.parse::<u32>() {
                            if let Some(name) = gid_to_name.get(&gid) {
                                if !groups.contains(name) {
                                    groups.push(name.clone());
                                }
                            }
                        }
                    }
                }
            }
        }

        for name in user_in_group_names {
            if !groups.contains(&name) {
                groups.push(name);
            }
        }

        groups
    }

    fn can_write_path(&self, path: &Path) -> bool {
        OpenOptions::new().write(true).open(path).is_ok()
    }

    fn check_audio_input(&self) -> Result<String, String> {
        use cpal::traits::{DeviceTrait, HostTrait};
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or_else(|| {
            "No default audio input device found (check PipeWire/ALSA)".to_string()
        })?;

        let name = device
            .description()
            .map(|d| d.to_string())
            .unwrap_or_else(|_| "Default Audio Device".to_string());
        let _ = device
            .default_input_config()
            .map_err(|e| format!("Default input config error: {}", e))?;

        Ok(name)
    }

    fn check_inference(&self) -> Result<String, String> {
        let config = crate::config::Config::load();
        check_inference_for_config(&config)
    }

    fn is_wayland(&self) -> bool {
        std::env::var_os("WAYLAND_DISPLAY").is_some()
    }

    fn has_command(&self, cmd: &str) -> bool {
        if let Ok(path) = std::env::var("PATH") {
            for dir in std::env::split_paths(&path) {
                let candidate = dir.join(cmd);
                if candidate.is_file() {
                    return true;
                }
            }
        }
        false
    }

    fn find_dock_binary(&self) -> Option<std::path::PathBuf> {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                let candidate = parent.join("speechyyy-dock");
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        if let Ok(path) = std::env::var("PATH") {
            for dir in std::env::split_paths(&path) {
                let candidate = dir.join("speechyyy-dock");
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        None
    }

    fn is_executable(&self, path: &Path) -> bool {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = fs::metadata(path) {
                return (meta.permissions().mode() & 0o100) != 0;
            }
            false
        }
        #[cfg(not(unix))]
        {
            path.is_file()
        }
    }

    fn check_layer_shell(&self, dock_bin: &Path) -> Result<LayerShellCheckResult, String> {
        let mut child = match std::process::Command::new(dock_bin)
            .arg("--check")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => return Err(format!("Failed to spawn {}: {}", dock_bin.display(), e)),
        };

        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(2);
        let poll_interval = std::time::Duration::from_millis(50);

        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    return Ok(match status.code() {
                        Some(0) => LayerShellCheckResult::Supported,
                        Some(1) => LayerShellCheckResult::Unsupported,
                        Some(2) => LayerShellCheckResult::CannotConnect,
                        _ => LayerShellCheckResult::ExecutionFailed,
                    });
                }
                Ok(None) => {
                    if start.elapsed() >= timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Ok(LayerShellCheckResult::Timeout);
                    }
                    std::thread::sleep(poll_interval);
                }
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("Error polling child process: {}", e));
                }
            }
        }
    }
}

pub fn mask_credential(key: &str) -> String {
    let char_count = key.chars().count();
    if char_count > 8 {
        let prefix: String = key.chars().take(4).collect();
        let suffix: String = key.chars().skip(char_count.saturating_sub(4)).collect();
        format!("{}...{}", prefix, suffix)
    } else {
        "********".to_string()
    }
}

pub fn check_inference_for_config(config: &crate::config::Config) -> Result<String, String> {
    let prov = config.inference.provider.to_lowercase();
    if prov != "mock"
        && prov != "custom"
        && prov != "groq"
        && prov != "openai"
        && prov != "openai-compatible"
        && config.inference.model.trim().is_empty()
    {
        return Err(
            "No active model configured. Run 'speechyyy use <model>' to choose one.".to_string(),
        );
    }
    match prov.as_str() {
        "sarvam" => {
            let key_info = config.inference.resolve_key_for_provider("sarvam");
            match key_info {
                Some((_, source)) => {
                    let host = "api.sarvam.ai";
                    Ok(format!(
                        "Sarvam: key configured (source: {}), host: {} (policy: valid HTTPS)",
                        source, host
                    ))
                }
                None => Err(
                    "Sarvam API Key missing. Set SARVAM_API_KEY in config.toml or environment."
                        .into(),
                ),
            }
        }
        "groq" => {
            let key_info = config.inference.resolve_key_for_provider("groq");
            match key_info {
                Some((_, source)) => {
                    let endpoint = crate::inference::DEFAULT_GROQ_ENDPOINT;
                    let policy_ok = crate::inference::validate_endpoint(endpoint).is_ok();
                    let host = crate::inference::extract_endpoint_host(endpoint)
                        .unwrap_or_else(|| "api.groq.com".into());
                    Ok(format!(
                        "Groq: key configured (source: {}), host: {} (policy: {})",
                        source,
                        host,
                        if policy_ok { "valid HTTPS" } else { "invalid" }
                    ))
                }
                None => Err(
                    "Groq API key missing. Set GROQ_API_KEY in config.toml or environment. Run 'speechyyy use groq' to set it.".into(),
                ),
            }
        }
        "openai" => {
            let key_info = config.inference.resolve_key_for_provider("openai");
            match key_info {
                Some((_, source)) => {
                    let endpoint = crate::inference::DEFAULT_OPENAI_ENDPOINT;
                    let policy_ok = crate::inference::validate_endpoint(endpoint).is_ok();
                    let host = crate::inference::extract_endpoint_host(endpoint)
                        .unwrap_or_else(|| "api.openai.com".into());
                    Ok(format!(
                        "OpenAI: key configured (source: {}), host: {} (policy: {})",
                        source,
                        host,
                        if policy_ok { "valid HTTPS" } else { "invalid" }
                    ))
                }
                None => Err(
                    "OpenAI API key missing. Set OPENAI_API_KEY in config.toml or environment. Run 'speechyyy use openai' to set it.".into(),
                ),
            }
        }
        "openai-compatible" => {
            let endpoint = match config
                .inference
                .openai_compatible_endpoint
                .as_deref()
                .filter(|e| !e.trim().is_empty())
            {
                Some(e) => e,
                None => {
                    return Err(
                        "OpenAI-compatible endpoint missing. Set 'openai_compatible_endpoint' in config.toml or run 'speechyyy use openai-compatible'.".into(),
                    )
                }
            };
            let policy_result = match crate::inference::validate_endpoint(endpoint) {
                Ok(_) => {
                    let host = crate::inference::extract_endpoint_host(endpoint)
                        .unwrap_or_else(|| "endpoint".into());
                    let is_loopback = crate::inference::is_loopback_host(&host);
                    if is_loopback {
                        "valid loopback HTTP"
                    } else {
                        "valid HTTPS"
                    }
                }
                Err(err) => return Err(format!("Invalid OpenAI-compatible endpoint: {}", err)),
            };
            let host = crate::inference::extract_endpoint_host(endpoint)
                .unwrap_or_else(|| "endpoint".into());
            let key_status = match config.inference.resolve_key_for_provider("openai-compatible") {
                Some((_, source)) => format!("key configured (source: {})", source),
                None => "no key (keyless)".to_string(),
            };
            Ok(format!(
                "OpenAI-compatible: {}, host: {} (policy: {})",
                key_status, host, policy_result
            ))
        }
        "mock" => Ok("Mock provider (mock)".into()),
        "local" => {
            let model_type = match crate::model::LocalModelType::from_str_name(&config.inference.model) {
                Some(m) => m,
                None => {
                    return Err(format!(
                        "Unknown local model '{}'. Supported models: parakeet-tdt-v3, whisper-small, whisper-large-v3-turbo, moonshine-base",
                        config.inference.model
                    ));
                }
            };
            let model_dir = crate::model::ModelManager::model_dir(model_type);
            match crate::model::ModelManager::verify_model_files(model_type, &model_dir) {
                Ok(info) => Ok(format!(
                    "Local model: {} ({:.1} MB, {} files)",
                    model_type.display_name(),
                    info.total_mb(),
                    info.file_count
                )),
                Err(err) => Err(format!(
                    "Local model missing at {}: {}. Run 'speechyyy model download -m {}' to fetch weights.",
                    model_dir.display(),
                    err,
                    model_type.folder_name()
                )),
            }
        }
        "custom" => {
            let custom_cfg = match &config.inference.custom {
                Some(c) => c,
                None => {
                    return Err(
                        "Inference provider is 'custom' but [inference.custom] is missing in config.toml".into()
                    );
                }
            };

            let valid_types = [
                "whisper",
                "transducer",
                "nemo_transducer",
                "moonshine",
                "sensevoice",
            ];
            if !valid_types.contains(&custom_cfg.r#type.to_lowercase().as_str()) {
                return Err(format!(
                    "Unsupported custom model type '{}'. Supported types: whisper, transducer, nemo_transducer, moonshine, sensevoice",
                    custom_cfg.r#type
                ));
            }

            if !custom_cfg.encoder.exists() {
                return Err(format!(
                    "Custom model encoder file not found: {}",
                    custom_cfg.encoder.display()
                ));
            }
            if fs::File::open(&custom_cfg.encoder).is_err() {
                return Err(format!(
                    "Custom model encoder file unreadable: {}",
                    custom_cfg.encoder.display()
                ));
            }

            if !custom_cfg.decoder.exists() {
                return Err(format!(
                    "Custom model decoder file not found: {}",
                    custom_cfg.decoder.display()
                ));
            }
            if fs::File::open(&custom_cfg.decoder).is_err() {
                return Err(format!(
                    "Custom model decoder file unreadable: {}",
                    custom_cfg.decoder.display()
                ));
            }

            if !custom_cfg.tokens.exists() {
                return Err(format!(
                    "Custom model tokens file not found: {}",
                    custom_cfg.tokens.display()
                ));
            }
            if fs::File::open(&custom_cfg.tokens).is_err() {
                return Err(format!(
                    "Custom model tokens file unreadable: {}",
                    custom_cfg.tokens.display()
                ));
            }

            if let Some(ref joiner) = custom_cfg.joiner {
                if !joiner.exists() {
                    return Err(format!(
                        "Custom model joiner file not found: {}",
                        joiner.display()
                    ));
                }
                if fs::File::open(joiner).is_err() {
                    return Err(format!(
                        "Custom model joiner file unreadable: {}",
                        joiner.display()
                    ));
                }
            } else if custom_cfg.r#type.to_lowercase() == "transducer"
                || custom_cfg.r#type.to_lowercase() == "nemo_transducer"
            {
                return Err("Custom transducer model requires 'joiner' in [inference.custom]".into());
            }

            Ok(format!(
                "Custom model: type='{}' (all model files verified)",
                custom_cfg.r#type
            ))
        }
        #[cfg(target_os = "macos")]
        "apple" => Ok("Apple Speech Native (apple-speech)".into()),
        #[cfg(not(target_os = "macos"))]
        "apple" => Err("Apple Speech provider is only supported on macOS".into()),
        other => Err(format!(
            "Unknown inference provider '{}'. Supported providers are 'mock', 'sarvam', 'groq', 'openai', 'openai-compatible', 'local', 'custom'{}.",
            other,
            if cfg!(target_os = "macos") { ", 'apple'" } else { "" }
        )),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckStatus {
    Pass,
    Warn(String),
    Fail(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticItem {
    pub name: String,
    pub status: CheckStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorReport {
    pub items: Vec<DiagnosticItem>,
    pub fix_instructions: Vec<String>,
    pub udev_rule_content: String,
}

impl DoctorReport {
    pub fn is_ready(&self) -> bool {
        self.items
            .iter()
            .all(|i| !matches!(i.status, CheckStatus::Fail(_)))
    }
}

pub fn run_doctor<E: SysEnv>(env: &E) -> DoctorReport {
    let config = crate::config::Config::load();
    run_doctor_opts(env, &config, false, None)
}

pub fn run_doctor_with_config<E: SysEnv>(env: &E, config: &crate::config::Config) -> DoctorReport {
    run_doctor_opts(env, config, false, None)
}

pub fn run_doctor_opts<E: SysEnv>(
    env: &E,
    config: &crate::config::Config,
    deep: bool,
    models_dir: Option<&Path>,
) -> DoctorReport {
    let mut items = Vec::new();
    let mut fix_instructions = Vec::new();

    // 1. /dev/uinput write access or Wayland clipboard fallback
    let uinput_path = Path::new("/dev/uinput");
    let uinput_writable = env.can_write_path(uinput_path);
    let has_wl_copy = env.has_command("wl-copy");

    if uinput_writable {
        items.push(DiagnosticItem {
            name: "Virtual keyboard permissions (/dev/uinput)".into(),
            status: CheckStatus::Pass,
        });
    } else if env.is_wayland() && has_wl_copy {
        items.push(DiagnosticItem {
            name: "Virtual keyboard permissions (/dev/uinput)".into(),
            status: CheckStatus::Warn("Cannot write to /dev/uinput (using Wayland clipboard fallback)".into()),
        });
        items.push(DiagnosticItem {
            name: "Wayland clipboard fallback (wl-copy)".into(),
            status: CheckStatus::Pass,
        });
    } else {
        items.push(DiagnosticItem {
            name: "Virtual keyboard permissions (/dev/uinput)".into(),
            status: CheckStatus::Fail("Cannot write to /dev/uinput (permission denied)".into()),
        });
        fix_instructions.push("sudo cp /etc/udev/rules.d/99-speechyyy-uinput.rules (or generate below) && sudo udevadm control --reload-rules && sudo udevadm trigger".into());
    }

    // Active injection mode & tools check
    let is_typewriter = config.injection_mode == crate::config::InjectionMode::Typewriter
        || config.injector.method.eq_ignore_ascii_case("typewriter");

    if is_typewriter {
        items.push(DiagnosticItem {
            name: "Active injection mode: typewriter (/dev/uinput)".into(),
            status: CheckStatus::Pass,
        });
        if uinput_writable {
            items.push(DiagnosticItem {
                name: "Typewriter Injection: /dev/uinput writable".into(),
                status: CheckStatus::Pass,
            });
        } else {
            items.push(DiagnosticItem {
                name:
                    "/dev/uinput not writable: typewriter mode will fall back to clipboard paste."
                        .into(),
                status: CheckStatus::Warn(String::new()),
            });
            fix_instructions.push(
                "For hardware typewriter keystrokes, grant /dev/uinput write permissions (e.g. via udev rules). Currently falling back to clipboard paste.".into(),
            );
        }
    } else {
        // Clipboard mode (default)
        if has_wl_copy {
            items.push(DiagnosticItem {
                name: "Active injection mode: clipboard (wl-copy installed)".into(),
                status: CheckStatus::Pass,
            });
        } else {
            items.push(DiagnosticItem {
                name: "Active injection mode: clipboard (wl-copy missing)".into(),
                status: CheckStatus::Warn(
                    "wl-copy is missing; daemon will fall back to typewriter (/dev/uinput)".into(),
                ),
            });
            fix_instructions.push(
                "Install 'wl-clipboard' for direct clipboard paste (e.g. 'sudo pacman -S wl-clipboard' or 'sudo apt install wl-clipboard'), or set injection_mode = 'typewriter' in config.toml.".into(),
            );
        }
    }

    // 4. Microphone audio input device check
    match env.check_audio_input() {
        Ok(device_name) => {
            items.push(DiagnosticItem {
                name: format!("Microphone input device ({})", device_name),
                status: CheckStatus::Pass,
            });
        }
        Err(err) => {
            items.push(DiagnosticItem {
                name: "Microphone input device".into(),
                status: CheckStatus::Fail(err),
            });
            fix_instructions.push(
                "Verify microphone is connected and PipeWire/PulseAudio/ALSA is running (e.g. 'wpctl status' or 'pactl info').".into(),
            );
        }
    }

    // 5. Speech inference provider readiness check
    match env.check_inference() {
        Ok(info) => {
            let name = if info.starts_with("Sarvam API Key") || info.starts_with("Sarvam:") {
                info
            } else {
                format!("Speech inference provider: {}", info)
            };
            items.push(DiagnosticItem {
                name,
                status: CheckStatus::Pass,
            });
        }
        Err(err) => {
            let (name, status_err) = if err.starts_with("Sarvam API Key") {
                (err.clone(), String::new())
            } else {
                ("Speech inference provider".to_string(), err)
            };
            items.push(DiagnosticItem {
                name,
                status: CheckStatus::Fail(status_err),
            });
            fix_instructions.push(
                "Export SARVAM_API_KEY (e.g. 'export SARVAM_API_KEY=your_key') or run 'speechyyy model download' for offline local inference.".into(),
            );
        }
    }

    // 6. Text injection pipeline verification
    if uinput_writable || (env.is_wayland() && env.has_command("wl-copy")) {
        let mode_desc = if is_typewriter {
            if uinput_writable {
                "Typewriter (uinput sequential)"
            } else {
                "Typewriter fallback (Wayland clipboard)"
            }
        } else if has_wl_copy {
            "Clipboard paste (wl-copy + Ctrl+V)"
        } else if uinput_writable {
            "Clipboard fallback to Typewriter (uinput sequential)"
        } else {
            "Text injector"
        };
        items.push(DiagnosticItem {
            name: format!("Text injector pipeline ({})", mode_desc),
            status: CheckStatus::Pass,
        });
    } else {
        match crate::injector::create_injector(config) {
            Ok(inj) => {
                items.push(DiagnosticItem {
                    name: format!("Text injector pipeline ({})", inj.name()),
                    status: CheckStatus::Pass,
                });
            }
            Err(err) => {
                items.push(DiagnosticItem {
                    name: "Text injector pipeline".into(),
                    status: CheckStatus::Fail(err),
                });
                fix_instructions.push(
                    "Ensure either /dev/uinput is writable or 'wl-copy' is installed for Wayland clipboard typing.".into(),
                );
            }
        }
    }

    // 7. Installed models integrity check
    let models_dir_path = match models_dir {
        Some(d) => d.to_path_buf(),
        None => crate::model::ModelManager::default_models_dir(),
    };
    check_installed_models_integrity(&models_dir_path, deep, &mut items, &mut fix_instructions);

    // 8. Companion dock binary check
    let dock_bin_opt = env.find_dock_binary();
    let dock_executable = match &dock_bin_opt {
        Some(bin) => {
            if env.is_executable(bin) {
                items.push(DiagnosticItem {
                    name: format!("Companion dock binary ({})", bin.display()),
                    status: CheckStatus::Pass,
                });
                true
            } else {
                items.push(DiagnosticItem {
                    name: format!("Companion dock binary ({})", bin.display()),
                    status: CheckStatus::Warn("File exists but executable bit is not set".into()),
                });
                fix_instructions.push(format!("chmod +x {}", bin.display()));
                false
            }
        }
        None => {
            items.push(DiagnosticItem {
                name: "Companion dock binary (speechyyy-dock)".into(),
                status: CheckStatus::Warn(
                    "Not found in PATH or next to speechyyy; companion dock will not appear on 'start'"
                        .into(),
                ),
            });
            fix_instructions.push(
                "Install 'speechyyy-dock' into ~/.local/bin or alongside speechyyy.".into(),
            );
            false
        }
    };

    // 9. Wayland session & Layer-Shell protocol support
    if !env.is_wayland() {
        items.push(DiagnosticItem {
            name: "Wayland session (WAYLAND_DISPLAY unset)".into(),
            status: CheckStatus::Pass,
        });
    } else {
        items.push(DiagnosticItem {
            name: "Wayland session detected".into(),
            status: CheckStatus::Pass,
        });

        if dock_executable {
            if let Some(ref dock_bin) = dock_bin_opt {
                match env.check_layer_shell(dock_bin) {
                    Ok(LayerShellCheckResult::Supported) => {
                        items.push(DiagnosticItem {
                            name: "Wayland layer-shell protocol (zwlr_layer_shell_v1)".into(),
                            status: CheckStatus::Pass,
                        });
                    }
                    Ok(LayerShellCheckResult::Unsupported) => {
                        let desktop = env.current_desktop();
                        items.push(DiagnosticItem {
                            name: "Wayland layer-shell protocol (zwlr_layer_shell_v1)".into(),
                            status: CheckStatus::Warn(format!(
                                "Compositor ({}) does not advertise zwlr_layer_shell_v1; companion dock will not appear, but dictation works normally.",
                                desktop
                            )),
                        });
                    }
                    Ok(LayerShellCheckResult::CannotConnect) => {
                        items.push(DiagnosticItem {
                            name: "Wayland layer-shell protocol check".into(),
                            status: CheckStatus::Warn(
                                "Cannot connect to Wayland display to query layer-shell".into(),
                            ),
                        });
                    }
                    Ok(LayerShellCheckResult::Timeout) => {
                        items.push(DiagnosticItem {
                            name: "Wayland layer-shell protocol check".into(),
                            status: CheckStatus::Warn("Check timed out after 2 seconds".into()),
                        });
                    }
                    Ok(LayerShellCheckResult::ExecutionFailed) | Err(_) => {
                        items.push(DiagnosticItem {
                            name: "Wayland layer-shell protocol check".into(),
                            status: CheckStatus::Warn(
                                "Failed to execute speechyyy-dock --check".into(),
                            ),
                        });
                    }
                }
            }
        }
    }

    let udev_rule_content =
        r#"# Speechyyy uinput rule for unprivileged virtual keyboard caret injection
KERNEL=="uinput", SUBSYSTEM=="misc", TAG+="uaccess", OPTIONS+="static_node=uinput"
"#
        .trim_start()
        .to_string();

    DoctorReport {
        items,
        fix_instructions,
        udev_rule_content,
    }
}

pub fn check_installed_models_integrity(
    models_dir: &Path,
    deep: bool,
    items: &mut Vec<DiagnosticItem>,
    fix_instructions: &mut Vec<String>,
) {
    if !models_dir.exists() {
        return;
    }

    let entries = match fs::read_dir(models_dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    let mut paths: Vec<std::path::PathBuf> = Vec::new();
    for entry in entries.flatten() {
        paths.push(entry.path());
    }
    paths.sort();

    for path in paths {
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };

        if file_name.starts_with('.') {
            continue;
        }

        if file_name.ends_with(".old") && path.is_dir() {
            let model_name = file_name.trim_end_matches(".old");
            items.push(DiagnosticItem {
                name: format!("Dangling backup: {}", file_name),
                status: CheckStatus::Warn(format!(
                    "Dangling backup directory detected. Run 'speechyyy download {}' to recover or clean.",
                    model_name
                )),
            });
            fix_instructions.push(format!(
                "Re-run 'speechyyy download {}' to resolve the dangling backup directory.",
                model_name
            ));
            continue;
        }

        if !path.is_dir() {
            continue;
        }

        let model_id = file_name;
        let model_info_path = path.join("MODEL_INFO.json");
        if !model_info_path.exists() {
            items.push(DiagnosticItem {
                name: format!("Installed model '{}'", model_id),
                status: CheckStatus::Warn(format!(
                    "unverified legacy install, re-run 'speechyyy download {} -f' to verify",
                    model_id
                )),
            });
            continue;
        }

        let content = match fs::read_to_string(&model_info_path) {
            Ok(c) => c,
            Err(e) => {
                items.push(DiagnosticItem {
                    name: format!("Installed model '{}' (MODEL_INFO.json)", model_id),
                    status: CheckStatus::Fail(format!("Failed to read MODEL_INFO.json: {}", e)),
                });
                fix_instructions.push(format!(
                    "Re-run 'speechyyy download {} -f' to reinstall and verify model metadata.",
                    model_id
                ));
                continue;
            }
        };

        let info: crate::model::InstalledModelInfo = match serde_json::from_str(&content) {
            Ok(i) => i,
            Err(e) => {
                items.push(DiagnosticItem {
                    name: format!("Installed model '{}' (MODEL_INFO.json)", model_id),
                    status: CheckStatus::Fail(format!("Corrupt MODEL_INFO.json: {}", e)),
                });
                fix_instructions.push(format!(
                    "Re-run 'speechyyy download {} -f' to reinstall and verify model metadata.",
                    model_id
                ));
                continue;
            }
        };

        let mut model_failed = false;
        let mut model_fail_msg = String::new();

        for (rel_name, file_meta) in &info.files {
            let file_path = path.join(rel_name);
            if !file_path.exists() {
                model_failed = true;
                model_fail_msg = format!("Missing file '{}'", rel_name);
                break;
            }

            let disk_meta = match fs::metadata(&file_path) {
                Ok(m) => m,
                Err(e) => {
                    model_failed = true;
                    model_fail_msg = format!("Failed to stat file '{}': {}", rel_name, e);
                    break;
                }
            };

            if disk_meta.len() != file_meta.size_bytes {
                model_failed = true;
                model_fail_msg = format!(
                    "File '{}' size mismatch: expected {} bytes, found {}",
                    rel_name, file_meta.size_bytes, disk_meta.len()
                );
                break;
            }

            if deep {
                match crate::model::compute_file_sha256(&file_path) {
                    Ok(hash) => {
                        if !hash.eq_ignore_ascii_case(&file_meta.sha256) {
                            model_failed = true;
                            model_fail_msg = format!(
                                "File '{}' SHA-256 mismatch (corrupted): expected {}, found {}",
                                rel_name, file_meta.sha256, hash
                            );
                            break;
                        }
                    }
                    Err(e) => {
                        model_failed = true;
                        model_fail_msg = format!("Failed to hash file '{}': {}", rel_name, e);
                        break;
                    }
                }
            }
        }

        if model_failed {
            items.push(DiagnosticItem {
                name: format!("Installed model '{}'", model_id),
                status: CheckStatus::Fail(model_fail_msg),
            });
            fix_instructions.push(format!(
                "Re-run 'speechyyy download {} -f' to repair corrupted model files.",
                model_id
            ));
        } else {
            let check_type = if deep { "verified (deep)" } else { "verified" };
            items.push(DiagnosticItem {
                name: format!("Installed model '{}' ({})", model_id, check_type),
                status: CheckStatus::Pass,
            });
        }
    }
}

pub fn print_doctor_report(report: &DoctorReport) {
    println!("\n=== Speechyyy Doctor Diagnostics ===\n");
    for item in &report.items {
        match &item.status {
            CheckStatus::Pass => println!("  [PASS] {}", item.name),
            CheckStatus::Warn(reason) => {
                if reason.is_empty() {
                    println!("  [WARN] {}", item.name);
                } else {
                    println!("  [WARN] {}: {}", item.name, reason);
                }
            }
            CheckStatus::Fail(reason) => {
                if reason.is_empty() {
                    println!("  [FAIL] {}", item.name);
                } else {
                    println!("  [FAIL] {}: {}", item.name, reason);
                }
            }
        }
    }

    if report.is_ready() {
        let config = crate::config::Config::load();
        let nickname = config.nickname();
        println!(
            "\n[✓] Everything looks gorgeous, {}. Audio and permissions are golden.\n",
            nickname
        );
    } else {
        println!("\nRecommended Remediation Steps:\n");
        for (idx, step) in report.fix_instructions.iter().enumerate() {
            println!("  {}. {}", idx + 1, step);
        }
        println!("\nProposed udev rule (/etc/udev/rules.d/99-speechyyy.rules):\n");
        println!("{}", report.udev_rule_content);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct MockEnv {
        user: String,
        groups: Vec<String>,
        writable_paths: Vec<PathBuf>,
        audio_input: Result<String, String>,
        inference_result: Result<String, String>,
    }

    impl SysEnv for MockEnv {
        fn current_username(&self) -> String {
            self.user.clone()
        }
        fn user_groups(&self) -> Vec<String> {
            self.groups.clone()
        }
        fn can_write_path(&self, path: &Path) -> bool {
            self.writable_paths.iter().any(|p| p == path)
        }
        fn check_audio_input(&self) -> Result<String, String> {
            self.audio_input.clone()
        }
        fn check_inference(&self) -> Result<String, String> {
            self.inference_result.clone()
        }
        fn has_command(&self, cmd: &str) -> bool {
            cmd == "wl-copy"
        }
        fn find_dock_binary(&self) -> Option<PathBuf> {
            Some(PathBuf::from("/usr/local/bin/speechyyy-dock"))
        }
        fn is_executable(&self, _path: &Path) -> bool {
            true
        }
    }

    struct MockMissingWlCopyEnv {
        user: String,
        groups: Vec<String>,
        writable_paths: Vec<PathBuf>,
        audio_input: Result<String, String>,
        inference_result: Result<String, String>,
    }

    impl SysEnv for MockMissingWlCopyEnv {
        fn current_username(&self) -> String {
            self.user.clone()
        }
        fn user_groups(&self) -> Vec<String> {
            self.groups.clone()
        }
        fn can_write_path(&self, path: &Path) -> bool {
            self.writable_paths.iter().any(|p| p == path)
        }
        fn check_audio_input(&self) -> Result<String, String> {
            self.audio_input.clone()
        }
        fn check_inference(&self) -> Result<String, String> {
            self.inference_result.clone()
        }
        fn has_command(&self, _cmd: &str) -> bool {
            false
        }
        fn find_dock_binary(&self) -> Option<PathBuf> {
            Some(PathBuf::from("/usr/local/bin/speechyyy-dock"))
        }
        fn is_executable(&self, _path: &Path) -> bool {
            true
        }
    }

    #[test]
    fn test_doctor_all_passing() {
        let env = MockEnv {
            user: "alfred".into(),
            groups: vec!["alfred".into(), "input".into(), "uinput".into()],
            writable_paths: vec![PathBuf::from("/dev/uinput")],
            audio_input: Ok("PipeWire Default Source".into()),
            inference_result: Ok("Mock provider (mock)".into()),
        };

        let report = run_doctor(&env);
        assert!(report.is_ready(), "Expected all checks to pass");
        assert!(report.fix_instructions.is_empty());
        assert!(report
            .items
            .iter()
            .any(|i| i.name.contains("PipeWire Default Source")));
        assert!(report
            .items
            .iter()
            .any(|i| i.name.contains("Speech inference provider")));
    }

    #[test]
    fn test_doctor_uinput_unwritable_fails() {
        let env = MockEnv {
            user: "alfred".into(),
            groups: vec!["alfred".into(), "nobody".into()],
            writable_paths: vec![],
            audio_input: Ok("Builtin Mic".into()),
            inference_result: Ok("Mock provider (mock)".into()),
        };

        let report = run_doctor(&env);
        assert!(!report.is_ready(), "Expected checks to fail when /dev/uinput is unwritable");
        assert!(!report.fix_instructions.is_empty());
        assert!(report
            .fix_instructions
            .iter()
            .any(|cmd| cmd.contains("99-speechyyy-uinput.rules")));
        assert!(!report
            .fix_instructions
            .iter()
            .any(|cmd| cmd.contains("usermod -aG input")));
        assert!(report.udev_rule_content.contains("KERNEL==\"uinput\""));
        assert!(!report.udev_rule_content.contains("KERNEL==\"event*\""));
    }

    #[test]
    fn test_doctor_uinput_writable_passes_without_input_group() {
        let env = MockEnv {
            user: "alfred".into(),
            groups: vec!["alfred".into(), "nobody".into()],
            writable_paths: vec![PathBuf::from("/dev/uinput")],
            audio_input: Ok("Builtin Mic".into()),
            inference_result: Ok("Mock provider (mock)".into()),
        };

        let report = run_doctor(&env);
        assert!(report.is_ready(), "Expected checks to pass via /dev/uinput write access");
        assert!(report.fix_instructions.is_empty());
    }

    #[test]
    fn test_doctor_no_audio_input_device() {
        let env = MockEnv {
            user: "alfred".into(),
            groups: vec!["input".into()],
            writable_paths: vec![PathBuf::from("/dev/uinput")],
            audio_input: Err("No default audio input device found".into()),
            inference_result: Ok("Mock provider (mock)".into()),
        };

        let report = run_doctor(&env);
        assert!(!report.is_ready());
        assert!(report
            .items
            .iter()
            .any(|i| i.name.contains("Microphone input device")
                && matches!(i.status, CheckStatus::Fail(_))));
        assert!(report
            .fix_instructions
            .iter()
            .any(|cmd| cmd.contains("PipeWire")));
    }

    #[test]
    fn test_doctor_missing_sarvam_api_key() {
        let env = MockEnv {
            user: "alfred".into(),
            groups: vec!["input".into()],
            writable_paths: vec![PathBuf::from("/dev/uinput")],
            audio_input: Ok("PipeWire Default Source".into()),
            inference_result: Err("SARVAM_API_KEY is not set".into()),
        };

        let report = run_doctor(&env);
        assert!(!report.is_ready());
        assert!(report
            .items
            .iter()
            .any(|i| i.name.contains("Speech inference provider")
                && matches!(i.status, CheckStatus::Fail(_))));
        assert!(report
            .fix_instructions
            .iter()
            .any(|cmd| cmd.contains("SARVAM_API_KEY")));
    }

    #[test]
    fn test_doctor_invalid_inference_provider() {
        let env = MockEnv {
            user: "alfred".into(),
            groups: vec!["input".into(), "uinput".into()],
            writable_paths: vec![PathBuf::from("/dev/uinput")],
            audio_input: Ok("PipeWire Default Source".into()),
            inference_result: Err("Unknown inference provider 'foo'".into()),
        };

        let report = run_doctor(&env);
        assert!(!report.is_ready());
        assert!(report
            .items
            .iter()
            .any(|i| i.name.contains("Speech inference provider")
                && matches!(i.status, CheckStatus::Fail(_))));
    }

    struct MockWaylandEnv {
        user: String,
        groups: Vec<String>,
        audio_input: Result<String, String>,
        inference_result: Result<String, String>,
    }

    impl SysEnv for MockWaylandEnv {
        fn current_username(&self) -> String {
            self.user.clone()
        }
        fn user_groups(&self) -> Vec<String> {
            self.groups.clone()
        }
        fn can_write_path(&self, _path: &Path) -> bool {
            false
        }
        fn check_audio_input(&self) -> Result<String, String> {
            self.audio_input.clone()
        }
        fn check_inference(&self) -> Result<String, String> {
            self.inference_result.clone()
        }
        fn is_wayland(&self) -> bool {
            true
        }
        fn has_command(&self, cmd: &str) -> bool {
            cmd == "wl-copy"
        }
    }

    #[test]
    fn test_doctor_wayland_fallback_passes_when_uinput_unwritable() {
        let env = MockWaylandEnv {
            user: "alfred".into(),
            groups: vec!["alfred".into(), "input".into()],
            audio_input: Ok("PipeWire Source".into()),
            inference_result: Ok("Mock provider (mock)".into()),
        };

        let report = run_doctor(&env);
        assert!(
            report.is_ready(),
            "Wayland fallback should pass preflight even without /dev/uinput write access"
        );
        assert!(report
            .items
            .iter()
            .any(|i| i.name.contains("Virtual keyboard permissions")
                && matches!(i.status, CheckStatus::Warn(_))));
        assert!(report
            .items
            .iter()
            .any(|i| i.name.contains("Wayland clipboard fallback")
                && matches!(i.status, CheckStatus::Pass)));
    }

    #[test]
    fn test_doctor_sarvam_api_key_configured_passes() {
        let env = MockEnv {
            user: "alfred".into(),
            groups: vec!["input".into(), "uinput".into()],
            writable_paths: vec![PathBuf::from("/dev/uinput")],
            audio_input: Ok("PipeWire Source".into()),
            inference_result: Ok("Sarvam API Key: sk_...1234 (configured)".into()),
        };

        let report = run_doctor(&env);
        assert!(report.is_ready());
        assert!(report
            .items
            .iter()
            .any(|i| i.name == "Sarvam API Key: sk_...1234 (configured)"
                && matches!(i.status, CheckStatus::Pass)));
    }

    #[test]
    fn test_doctor_sarvam_api_key_missing_fails() {
        let env = MockEnv {
            user: "alfred".into(),
            groups: vec!["input".into(), "uinput".into()],
            writable_paths: vec![PathBuf::from("/dev/uinput")],
            audio_input: Ok("PipeWire Source".into()),
            inference_result: Err(
                "Sarvam API Key missing. Set SARVAM_API_KEY in config.toml or environment.".into(),
            ),
        };

        let report = run_doctor(&env);
        assert!(!report.is_ready());
        assert!(report.items.iter().any(|i| i.name
            == "Sarvam API Key missing. Set SARVAM_API_KEY in config.toml or environment."
            && matches!(i.status, CheckStatus::Fail(_))));
        assert!(report
            .fix_instructions
            .iter()
            .any(|f| f.contains("SARVAM_API_KEY")));
    }

    #[test]
    fn test_doctor_typewriter_uinput_writable_passes() {
        let env = MockEnv {
            user: "alfred".into(),
            groups: vec!["input".into(), "uinput".into()],
            writable_paths: vec![PathBuf::from("/dev/uinput")],
            audio_input: Ok("PipeWire Source".into()),
            inference_result: Ok("Mock provider (mock)".into()),
        };

        let mut config = crate::config::Config::default();
        config.injector.method = "typewriter".into();

        let report = run_doctor_with_config(&env, &config);
        assert!(report.is_ready());
        assert!(report
            .items
            .iter()
            .any(|i| i.name == "Typewriter Injection: /dev/uinput writable"
                && matches!(i.status, CheckStatus::Pass)));
    }

    #[test]
    fn test_doctor_typewriter_uinput_unwritable_warns_and_stays_ready() {
        let env = MockWaylandEnv {
            user: "alfred".into(),
            groups: vec!["input".into()],
            audio_input: Ok("PipeWire Source".into()),
            inference_result: Ok("Mock provider (mock)".into()),
        };

        let mut config = crate::config::Config::default();
        config.injector.method = "typewriter".into();

        let report = run_doctor_with_config(&env, &config);
        assert!(
            report.is_ready(),
            "Doctor should stay ready because Wayland clipboard fallback handles text delivery"
        );
        assert!(report.items.iter().any(|i| i.name
            == "/dev/uinput not writable: typewriter mode will fall back to clipboard paste."
            && matches!(i.status, CheckStatus::Warn(_))));
    }

    #[test]
    fn test_doctor_clipboard_mode_with_wl_copy_passes() {
        let env = MockWaylandEnv {
            user: "alfred".into(),
            groups: vec!["input".into(), "uinput".into()],
            audio_input: Ok("PipeWire Source".into()),
            inference_result: Ok("Mock provider (mock)".into()),
        };

        let config = crate::config::Config::default(); // defaults to InjectionMode::Clipboard
        let report = run_doctor_with_config(&env, &config);

        assert!(report.is_ready());
        assert!(report.items.iter().any(|i| i.name
            == "Active injection mode: clipboard (wl-copy installed)"
            && matches!(i.status, CheckStatus::Pass)));
    }

    #[test]
    fn test_doctor_clipboard_mode_without_wl_copy_warns_and_explains_fallback() {
        let env = MockMissingWlCopyEnv {
            user: "alfred".into(),
            groups: vec!["input".into(), "uinput".into()],
            writable_paths: vec![PathBuf::from("/dev/uinput")],
            audio_input: Ok("PipeWire Source".into()),
            inference_result: Ok("Mock provider (mock)".into()),
        };

        let config = crate::config::Config::default(); // defaults to InjectionMode::Clipboard, but MockMissingWlCopyEnv does not have wl-copy
        let report = run_doctor_with_config(&env, &config);

        assert!(report.is_ready());
        assert!(report.items.iter().any(|i| i.name
            == "Active injection mode: clipboard (wl-copy missing)"
            && matches!(i.status, CheckStatus::Warn(ref msg) if msg.contains("typewriter"))));
        assert!(report
            .fix_instructions
            .iter()
            .any(|f| f.contains("wl-clipboard")));
    }

    #[test]
    fn test_check_inference_for_config_sarvam_and_mock() {
        let mut config = crate::config::Config::default();
        config.inference.provider = "sarvam".into();
        config.inference.api_key = "sk_test_1234567890abcdef".into();

        let res = check_inference_for_config(&config).expect("should succeed with api key");
        assert!(res.contains("Sarvam: key configured (source: config), host: api.sarvam.ai"));

        config.inference.api_key.clear();
        std::env::remove_var("SARVAM_API_KEY");

        let err = check_inference_for_config(&config).expect_err("should fail without api key");
        assert_eq!(
            err,
            "Sarvam API Key missing. Set SARVAM_API_KEY in config.toml or environment."
        );

        config.inference.provider = "mock".into();
        let mock_res = check_inference_for_config(&config).expect("mock should succeed");
        assert_eq!(mock_res, "Mock provider (mock)");
    }

    #[test]
    fn test_doctor_unicode_key_masking() {
        // Multi-byte Unicode tokens (emojis & accents) must not panic on char boundary slicing
        let unicode_key = "🦀🔑🔐💎_secret_token_✨🚀🌟🎉";
        let masked = mask_credential(unicode_key);
        assert_eq!(masked, "🦀🔑🔐💎...✨🚀🌟🎉");

        let accented_key = "café_münchen_straße";
        let masked_accented = mask_credential(accented_key);
        assert_eq!(masked_accented, "café...raße");

        let short_key = "short";
        assert_eq!(mask_credential(short_key), "********");
    }

    #[test]
    fn test_check_inference_for_config_custom_valid_and_missing_paths() {
        let temp_dir = std::env::temp_dir().join(format!("doctor_custom_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let mut config = crate::config::Config::default();
        config.inference.provider = "custom".into();

        // 1. Missing [inference.custom]
        let err1 = check_inference_for_config(&config).unwrap_err();
        assert!(err1.contains("[inference.custom] is missing"));

        // 2. Paths missing on disk
        let enc_path = temp_dir.join("encoder.onnx");
        let dec_path = temp_dir.join("decoder.onnx");
        let tok_path = temp_dir.join("tokens.txt");
        let join_path = temp_dir.join("joiner.onnx");

        config.inference.custom = Some(crate::config::CustomModelConfig {
            r#type: "transducer".into(),
            encoder: enc_path.clone(),
            decoder: dec_path.clone(),
            joiner: Some(join_path.clone()),
            tokens: tok_path.clone(),
        });

        let err2 = check_inference_for_config(&config).unwrap_err();
        assert!(err2.contains("encoder file not found"));

        // 3. Create all files -> passes
        fs::write(&enc_path, b"model data").unwrap();
        fs::write(&dec_path, b"model data").unwrap();
        fs::write(&tok_path, b"model data").unwrap();
        fs::write(&join_path, b"model data").unwrap();

        let ok_res = check_inference_for_config(&config).unwrap();
        assert!(ok_res.contains("Custom model: type='transducer' (all model files verified)"));

        // 4. Test nemo_transducer type
        config.inference.custom = Some(crate::config::CustomModelConfig {
            r#type: "nemo_transducer".into(),
            encoder: enc_path.clone(),
            decoder: dec_path.clone(),
            joiner: Some(join_path.clone()),
            tokens: tok_path.clone(),
        });
        let ok_nemo = check_inference_for_config(&config).unwrap();
        assert!(ok_nemo.contains("Custom model: type='nemo_transducer' (all model files verified)"));

        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn test_check_inference_for_config_local_curated_models() {
        let mut config = crate::config::Config::default();
        config.inference.provider = "local".into();
        config.inference.model = "non-existent-model".into();

        let err = check_inference_for_config(&config).unwrap_err();
        assert!(err.contains("Unknown local model 'non-existent-model'"));
        assert!(err.contains("parakeet-tdt-v3"));

        config.inference.model = "parakeet-tdt-v3".into();
        match check_inference_for_config(&config) {
            Ok(msg) => {
                assert!(msg.contains("Parakeet") || msg.contains("parakeet"));
            }
            Err(err) => {
                let err_str = err.to_string();
                assert!(
                    err_str.contains("parakeet")
                        || err_str.contains("weights")
                        || err_str.contains("speechyyy model download -m parakeet-tdt-v3")
                );
            }
        }

        // Empty active model returns clean actionable error
        config.inference.model = "".into();
        let err_empty = check_inference_for_config(&config).unwrap_err();
        assert!(err_empty.contains("speechyyy use <model>"));
    }

    #[derive(Clone)]
    struct MockDoctorEnv {
        user: String,
        groups: Vec<String>,
        writable_paths: Vec<PathBuf>,
        audio_input: Result<String, String>,
        inference_result: Result<String, String>,
        is_wayland: bool,
        commands: Vec<String>,
        dock_binary: Option<PathBuf>,
        dock_executable: bool,
        layer_shell_result: Result<LayerShellCheckResult, String>,
    }

    impl SysEnv for MockDoctorEnv {
        fn current_username(&self) -> String {
            self.user.clone()
        }
        fn user_groups(&self) -> Vec<String> {
            self.groups.clone()
        }
        fn can_write_path(&self, path: &Path) -> bool {
            self.writable_paths.contains(&path.to_path_buf())
        }
        fn check_audio_input(&self) -> Result<String, String> {
            self.audio_input.clone()
        }
        fn check_inference(&self) -> Result<String, String> {
            self.inference_result.clone()
        }
        fn is_wayland(&self) -> bool {
            self.is_wayland
        }
        fn has_command(&self, cmd: &str) -> bool {
            self.commands.iter().any(|c| c == cmd)
        }
        fn find_dock_binary(&self) -> Option<PathBuf> {
            self.dock_binary.clone()
        }
        fn is_executable(&self, _path: &Path) -> bool {
            self.dock_executable
        }
        fn check_layer_shell(&self, _dock_bin: &Path) -> Result<LayerShellCheckResult, String> {
            self.layer_shell_result.clone()
        }
    }

    #[test]
    fn test_doctor_dock_and_wayland_layer_shell_scenarios() {
        let mut config = crate::config::Config::default();
        config.inference.provider = "mock".into();

        // Scenario 1: Dock present, executable, Wayland with LayerShell::Supported (exit 0)
        let env_ok = MockDoctorEnv {
            user: "testuser".into(),
            groups: vec!["input".into()],
            writable_paths: vec![PathBuf::from("/dev/uinput")],
            audio_input: Ok("Mock Mic".into()),
            inference_result: Ok("Mock Provider".into()),
            is_wayland: true,
            commands: vec!["wl-copy".into()],
            dock_binary: Some(PathBuf::from("/usr/bin/speechyyy-dock")),
            dock_executable: true,
            layer_shell_result: Ok(LayerShellCheckResult::Supported),
        };
        let rep_ok = run_doctor_with_config(&env_ok, &config);
        assert!(rep_ok.items.iter().any(|i| i.name.contains("Companion dock binary") && i.status == CheckStatus::Pass));
        assert!(rep_ok.items.iter().any(|i| i.name.contains("zwlr_layer_shell_v1") && i.status == CheckStatus::Pass));

        // Scenario 2: Dock missing
        let env_no_dock = MockDoctorEnv {
            dock_binary: None,
            ..env_ok.clone()
        };
        let rep_no_dock = run_doctor_with_config(&env_no_dock, &config);
        assert!(rep_no_dock.items.iter().any(|i| i.name.contains("Companion dock binary") && matches!(i.status, CheckStatus::Warn(_))));

        // Scenario 3: Dock not executable
        let env_not_exec = MockDoctorEnv {
            dock_executable: false,
            ..env_ok.clone()
        };
        let rep_not_exec = run_doctor_with_config(&env_not_exec, &config);
        assert!(rep_not_exec.items.iter().any(|i| i.name.contains("Companion dock binary") && matches!(i.status, CheckStatus::Warn(ref w) if w.contains("executable bit"))));

        // Scenario 4: Layer shell unsupported (exit 1)
        let env_unsupported = MockDoctorEnv {
            layer_shell_result: Ok(LayerShellCheckResult::Unsupported),
            ..env_ok.clone()
        };
        let rep_unsupported = run_doctor_with_config(&env_unsupported, &config);
        assert!(rep_unsupported.items.iter().any(|i| i.name.contains("zwlr_layer_shell_v1") && matches!(i.status, CheckStatus::Warn(ref w) if w.contains("does not advertise"))));

        // Scenario 5: Cannot connect (exit 2)
        let env_cannot_connect = MockDoctorEnv {
            layer_shell_result: Ok(LayerShellCheckResult::CannotConnect),
            ..env_ok.clone()
        };
        let rep_cannot_connect = run_doctor_with_config(&env_cannot_connect, &config);
        assert!(rep_cannot_connect.items.iter().any(|i| i.name.contains("layer-shell protocol check") && matches!(i.status, CheckStatus::Warn(ref w) if w.contains("Cannot connect"))));

        // Scenario 6: Timeout (exceeded 2s)
        let env_timeout = MockDoctorEnv {
            layer_shell_result: Ok(LayerShellCheckResult::Timeout),
            ..env_ok.clone()
        };
        let rep_timeout = run_doctor_with_config(&env_timeout, &config);
        assert!(rep_timeout.items.iter().any(|i| i.name.contains("layer-shell protocol check") && matches!(i.status, CheckStatus::Warn(ref w) if w.contains("timed out"))));

        // Scenario 7: Non-Wayland session
        let env_x11 = MockDoctorEnv {
            is_wayland: false,
            ..env_ok.clone()
        };
        let rep_x11 = run_doctor_with_config(&env_x11, &config);
        assert!(rep_x11.items.iter().any(|i| i.name.contains("WAYLAND_DISPLAY unset") && i.status == CheckStatus::Pass));
        assert!(!rep_x11.items.iter().any(|i| i.name.contains("zwlr_layer_shell_v1")));
    }
}
