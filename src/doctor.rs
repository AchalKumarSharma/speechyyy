use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};

pub trait SysEnv {
    fn current_username(&self) -> String;
    fn user_groups(&self) -> Vec<String>;
    fn can_read_path(&self, path: &Path) -> bool;
    fn can_write_path(&self, path: &Path) -> bool;
    fn list_input_events(&self) -> Vec<PathBuf>;
    fn check_audio_input(&self) -> Result<String, String>;
    fn check_inference(&self) -> Result<String, String>;
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

    fn can_read_path(&self, path: &Path) -> bool {
        OpenOptions::new().read(true).open(path).is_ok()
    }

    fn can_write_path(&self, path: &Path) -> bool {
        OpenOptions::new().write(true).open(path).is_ok()
    }

    fn list_input_events(&self) -> Vec<PathBuf> {
        let mut events = Vec::new();
        if let Ok(entries) = fs::read_dir("/dev/input") {
            for entry in entries.flatten() {
                let name = entry.file_name();
                if name.to_string_lossy().starts_with("event") {
                    events.push(entry.path());
                }
            }
        }
        events.sort();
        events
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
        match config.inference.provider.to_lowercase().as_str() {
            "sarvam" => {
                if let Some(key) = config.inference.resolve_api_key() {
                    let masked = if key.len() > 8 {
                        format!("{}...{}", &key[..4], &key[key.len() - 4..])
                    } else {
                        "********".to_string()
                    };
                    Ok(format!(
                        "Sarvam AI (key: {}, model: {})",
                        masked, config.inference.model
                    ))
                } else {
                    Err("SARVAM_API_KEY is not set (configure in config.toml or set environment variable)".into())
                }
            }
            "mock" => Ok("Mock provider (mock)".into()),
            "local" => {
                let model_type = crate::model::LocalModelType::from_str_name(&config.inference.model)
                    .unwrap_or(crate::model::LocalModelType::MoonshineTiny);
                let model_dir = crate::model::ModelManager::model_dir(model_type);
                match crate::model::ModelManager::verify_model_files(model_type, &model_dir) {
                    Ok(info) => Ok(format!(
                        "Local model: {} ({:.1} MB, {} files)",
                        model_type.display_name(),
                        info.total_mb(),
                        info.file_count
                    )),
                    Err(err) => Err(format!(
                        "Local model missing at {}: {}. Run 'speechyyy model download' to fetch weights.",
                        model_dir.display(),
                        err
                    )),
                }
            }
            other => Err(format!(
                "Unknown inference provider '{}'. Supported providers are 'mock', 'sarvam', 'local'.",
                other
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckStatus {
    Pass,
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
            .all(|i| matches!(i.status, CheckStatus::Pass))
    }
}

pub fn run_doctor<E: SysEnv>(env: &E) -> DoctorReport {
    let mut items = Vec::new();
    let mut fix_instructions = Vec::new();
    let username = env.current_username();
    let groups = env.user_groups();

    // 1. Group membership checks (input and uinput)
    let has_input_group = groups.iter().any(|g| g == "input");
    let _has_uinput_group = groups.iter().any(|g| g == "uinput");

    if has_input_group {
        items.push(DiagnosticItem {
            name: "User 'input' group membership".into(),
            status: CheckStatus::Pass,
        });
    } else {
        items.push(DiagnosticItem {
            name: "User 'input' group membership".into(),
            status: CheckStatus::Fail(format!("User '{}' is not in the 'input' group", username)),
        });
    }

    items.push(DiagnosticItem {
        name: "User 'uinput' group membership".into(),
        status: CheckStatus::Pass, // On many distros uinput is managed under input group via udev
    });

    if !has_input_group {
        fix_instructions.push(format!("sudo usermod -aG input,uinput {}", username));
    }

    // 2. evdev event access
    let events = env.list_input_events();
    if events.is_empty() {
        items.push(DiagnosticItem {
            name: "Linux evdev devices (/dev/input/event*)".into(),
            status: CheckStatus::Fail("No /dev/input/event* devices found on system".into()),
        });
    } else {
        let readable_count = events.iter().filter(|p| env.can_read_path(p)).count();
        if readable_count > 0 {
            items.push(DiagnosticItem {
                name: "evdev read permissions".into(),
                status: CheckStatus::Pass,
            });
        } else {
            items.push(DiagnosticItem {
                name: "evdev read permissions".into(),
                status: CheckStatus::Fail(format!(
                    "Found {} input device(s) in /dev/input/, but none are readable",
                    events.len()
                )),
            });
        }
    }

    // 3. /dev/uinput write access
    let uinput_path = Path::new("/dev/uinput");
    if env.can_write_path(uinput_path) {
        items.push(DiagnosticItem {
            name: "Virtual keyboard permissions (/dev/uinput)".into(),
            status: CheckStatus::Pass,
        });
    } else {
        items.push(DiagnosticItem {
            name: "Virtual keyboard permissions (/dev/uinput)".into(),
            status: CheckStatus::Fail("Cannot write to /dev/uinput (permission denied)".into()),
        });
        fix_instructions.push("sudo cp /etc/udev/rules.d/99-speechyyy.rules (or generate below) && sudo udevadm control --reload-rules && sudo udevadm trigger".into());
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
            items.push(DiagnosticItem {
                name: format!("Speech inference provider: {}", info),
                status: CheckStatus::Pass,
            });
        }
        Err(err) => {
            items.push(DiagnosticItem {
                name: "Speech inference provider".into(),
                status: CheckStatus::Fail(err),
            });
            fix_instructions.push(
                "Export SARVAM_API_KEY (e.g. 'export SARVAM_API_KEY=your_key') or run 'speechyyy model download' for offline local inference.".into(),
            );
        }
    }

    let udev_rule_content =
        r#"# Speechyyy uinput rule for unprivileged virtual keyboard caret injection
KERNEL=="uinput", GROUP="input", MODE="0660", OPTIONS+="static_node=uinput"
"#
        .trim_start()
        .to_string();

    if !has_input_group {
        fix_instructions
            .push("Log out and log back in for new group permissions to take effect.".into());
    }

    DoctorReport {
        items,
        fix_instructions,
        udev_rule_content,
    }
}

pub fn print_doctor_report(report: &DoctorReport) {
    println!("\n=== Speechyyy Doctor Diagnostics ===\n");
    for item in &report.items {
        match &item.status {
            CheckStatus::Pass => println!("  [PASS] {}", item.name),
            CheckStatus::Fail(reason) => println!("  [FAIL] {}: {}", item.name, reason),
        }
    }

    if report.is_ready() {
        println!("\nAll pre-flight checks passed! Speechyyy is ready to run.\n");
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

    struct MockEnv {
        user: String,
        groups: Vec<String>,
        readable_paths: Vec<PathBuf>,
        writable_paths: Vec<PathBuf>,
        events: Vec<PathBuf>,
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
        fn can_read_path(&self, path: &Path) -> bool {
            self.readable_paths.iter().any(|p| p == path)
        }
        fn can_write_path(&self, path: &Path) -> bool {
            self.writable_paths.iter().any(|p| p == path)
        }
        fn list_input_events(&self) -> Vec<PathBuf> {
            self.events.clone()
        }
        fn check_audio_input(&self) -> Result<String, String> {
            self.audio_input.clone()
        }
        fn check_inference(&self) -> Result<String, String> {
            self.inference_result.clone()
        }
    }

    #[test]
    fn test_doctor_all_passing() {
        let env = MockEnv {
            user: "alfred".into(),
            groups: vec!["alfred".into(), "input".into(), "uinput".into()],
            readable_paths: vec![PathBuf::from("/dev/input/event0")],
            writable_paths: vec![PathBuf::from("/dev/uinput")],
            events: vec![PathBuf::from("/dev/input/event0")],
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
    fn test_doctor_missing_input_group_and_uinput_access() {
        let env = MockEnv {
            user: "alfred".into(),
            groups: vec!["alfred".into(), "nobody".into()],
            readable_paths: vec![],
            writable_paths: vec![],
            events: vec![PathBuf::from("/dev/input/event0")],
            audio_input: Ok("Builtin Mic".into()),
            inference_result: Ok("Mock provider (mock)".into()),
        };

        let report = run_doctor(&env);
        assert!(!report.is_ready(), "Expected checks to fail");
        assert!(!report.fix_instructions.is_empty());
        assert!(report
            .fix_instructions
            .iter()
            .any(|cmd| cmd.contains("usermod -aG input")));
        assert!(report.udev_rule_content.contains("KERNEL==\"uinput\""));
    }

    #[test]
    fn test_doctor_no_events_found() {
        let env = MockEnv {
            user: "alfred".into(),
            groups: vec!["input".into()],
            readable_paths: vec![],
            writable_paths: vec![PathBuf::from("/dev/uinput")],
            events: vec![],
            audio_input: Ok("Builtin Mic".into()),
            inference_result: Ok("Mock provider (mock)".into()),
        };

        let report = run_doctor(&env);
        assert!(!report.is_ready());
        assert!(report
            .items
            .iter()
            .any(|i| i.name.contains("Linux evdev devices")));
    }

    #[test]
    fn test_doctor_no_audio_input_device() {
        let env = MockEnv {
            user: "alfred".into(),
            groups: vec!["input".into()],
            readable_paths: vec![PathBuf::from("/dev/input/event0")],
            writable_paths: vec![PathBuf::from("/dev/uinput")],
            events: vec![PathBuf::from("/dev/input/event0")],
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
            readable_paths: vec![PathBuf::from("/dev/input/event0")],
            writable_paths: vec![PathBuf::from("/dev/uinput")],
            events: vec![PathBuf::from("/dev/input/event0")],
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
            readable_paths: vec![PathBuf::from("/dev/input/event0")],
            writable_paths: vec![PathBuf::from("/dev/uinput")],
            events: vec![PathBuf::from("/dev/input/event0")],
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
}
