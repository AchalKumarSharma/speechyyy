use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, EventType, InputEvent, KeyCode};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InjectionMode {
    Typewriter { delay_ms: u64 },
    Paste,
}

impl Default for InjectionMode {
    fn default() -> Self {
        Self::Typewriter { delay_ms: 15 }
    }
}

pub trait TextInjector: Send + Sync {
    fn inject(&mut self, text: &str) -> Result<(), String>;

    fn send_text(&mut self, text: &str) -> Result<(), String> {
        self.inject(text)
    }

    fn name(&self) -> &'static str {
        "TextInjector"
    }
}

impl TextInjector for Box<dyn TextInjector> {
    fn inject(&mut self, text: &str) -> Result<(), String> {
        (**self).inject(text)
    }

    fn send_text(&mut self, text: &str) -> Result<(), String> {
        (**self).send_text(text)
    }

    fn name(&self) -> &'static str {
        (**self).name()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyStroke {
    pub key: KeyCode,
    pub is_down: bool,
}

pub fn char_to_key(c: char) -> Option<(KeyCode, bool)> {
    match c {
        // Lowercase
        'a' => Some((KeyCode::KEY_A, false)),
        'b' => Some((KeyCode::KEY_B, false)),
        'c' => Some((KeyCode::KEY_C, false)),
        'd' => Some((KeyCode::KEY_D, false)),
        'e' => Some((KeyCode::KEY_E, false)),
        'f' => Some((KeyCode::KEY_F, false)),
        'g' => Some((KeyCode::KEY_G, false)),
        'h' => Some((KeyCode::KEY_H, false)),
        'i' => Some((KeyCode::KEY_I, false)),
        'j' => Some((KeyCode::KEY_J, false)),
        'k' => Some((KeyCode::KEY_K, false)),
        'l' => Some((KeyCode::KEY_L, false)),
        'm' => Some((KeyCode::KEY_M, false)),
        'n' => Some((KeyCode::KEY_N, false)),
        'o' => Some((KeyCode::KEY_O, false)),
        'p' => Some((KeyCode::KEY_P, false)),
        'q' => Some((KeyCode::KEY_Q, false)),
        'r' => Some((KeyCode::KEY_R, false)),
        's' => Some((KeyCode::KEY_S, false)),
        't' => Some((KeyCode::KEY_T, false)),
        'u' => Some((KeyCode::KEY_U, false)),
        'v' => Some((KeyCode::KEY_V, false)),
        'w' => Some((KeyCode::KEY_W, false)),
        'x' => Some((KeyCode::KEY_X, false)),
        'y' => Some((KeyCode::KEY_Y, false)),
        'z' => Some((KeyCode::KEY_Z, false)),

        // Uppercase
        'A' => Some((KeyCode::KEY_A, true)),
        'B' => Some((KeyCode::KEY_B, true)),
        'C' => Some((KeyCode::KEY_C, true)),
        'D' => Some((KeyCode::KEY_D, true)),
        'E' => Some((KeyCode::KEY_E, true)),
        'F' => Some((KeyCode::KEY_F, true)),
        'G' => Some((KeyCode::KEY_G, true)),
        'H' => Some((KeyCode::KEY_H, true)),
        'I' => Some((KeyCode::KEY_I, true)),
        'J' => Some((KeyCode::KEY_J, true)),
        'K' => Some((KeyCode::KEY_K, true)),
        'L' => Some((KeyCode::KEY_L, true)),
        'M' => Some((KeyCode::KEY_M, true)),
        'N' => Some((KeyCode::KEY_N, true)),
        'O' => Some((KeyCode::KEY_O, true)),
        'P' => Some((KeyCode::KEY_P, true)),
        'Q' => Some((KeyCode::KEY_Q, true)),
        'R' => Some((KeyCode::KEY_R, true)),
        'S' => Some((KeyCode::KEY_S, true)),
        'T' => Some((KeyCode::KEY_T, true)),
        'U' => Some((KeyCode::KEY_U, true)),
        'V' => Some((KeyCode::KEY_V, true)),
        'W' => Some((KeyCode::KEY_W, true)),
        'X' => Some((KeyCode::KEY_X, true)),
        'Y' => Some((KeyCode::KEY_Y, true)),
        'Z' => Some((KeyCode::KEY_Z, true)),

        // Digits
        '0' => Some((KeyCode::KEY_0, false)),
        '1' => Some((KeyCode::KEY_1, false)),
        '2' => Some((KeyCode::KEY_2, false)),
        '3' => Some((KeyCode::KEY_3, false)),
        '4' => Some((KeyCode::KEY_4, false)),
        '5' => Some((KeyCode::KEY_5, false)),
        '6' => Some((KeyCode::KEY_6, false)),
        '7' => Some((KeyCode::KEY_7, false)),
        '8' => Some((KeyCode::KEY_8, false)),
        '9' => Some((KeyCode::KEY_9, false)),

        // Whitespace
        ' ' => Some((KeyCode::KEY_SPACE, false)),
        '\n' => Some((KeyCode::KEY_ENTER, false)),
        '\t' => Some((KeyCode::KEY_TAB, false)),

        // Symbols and Punctuation (Full Printable ASCII)
        '.' => Some((KeyCode::KEY_DOT, false)),
        '>' => Some((KeyCode::KEY_DOT, true)),
        ',' => Some((KeyCode::KEY_COMMA, false)),
        '<' => Some((KeyCode::KEY_COMMA, true)),
        '-' => Some((KeyCode::KEY_MINUS, false)),
        '_' => Some((KeyCode::KEY_MINUS, true)),
        '=' => Some((KeyCode::KEY_EQUAL, false)),
        '+' => Some((KeyCode::KEY_EQUAL, true)),
        ';' => Some((KeyCode::KEY_SEMICOLON, false)),
        ':' => Some((KeyCode::KEY_SEMICOLON, true)),
        '/' => Some((KeyCode::KEY_SLASH, false)),
        '?' => Some((KeyCode::KEY_SLASH, true)),
        '\'' => Some((KeyCode::KEY_APOSTROPHE, false)),
        '"' => Some((KeyCode::KEY_APOSTROPHE, true)),
        '[' => Some((KeyCode::KEY_LEFTBRACE, false)),
        '{' => Some((KeyCode::KEY_LEFTBRACE, true)),
        ']' => Some((KeyCode::KEY_RIGHTBRACE, false)),
        '}' => Some((KeyCode::KEY_RIGHTBRACE, true)),
        '\\' => Some((KeyCode::KEY_BACKSLASH, false)),
        '|' => Some((KeyCode::KEY_BACKSLASH, true)),
        '`' => Some((KeyCode::KEY_GRAVE, false)),
        '~' => Some((KeyCode::KEY_GRAVE, true)),

        // Shifted number row symbols
        '!' => Some((KeyCode::KEY_1, true)),
        '@' => Some((KeyCode::KEY_2, true)),
        '#' => Some((KeyCode::KEY_3, true)),
        '$' => Some((KeyCode::KEY_4, true)),
        '%' => Some((KeyCode::KEY_5, true)),
        '^' => Some((KeyCode::KEY_6, true)),
        '&' => Some((KeyCode::KEY_7, true)),
        '*' => Some((KeyCode::KEY_8, true)),
        '(' => Some((KeyCode::KEY_9, true)),
        ')' => Some((KeyCode::KEY_0, true)),

        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextSegment {
    Mapped(char, KeyCode, bool),
    Unmapped(String),
}

pub fn segment_text(text: &str) -> Vec<TextSegment> {
    let mut segments: Vec<TextSegment> = Vec::new();
    let mut current_unmapped = String::new();

    for c in text.chars() {
        if let Some((key, needs_shift)) = char_to_key(c) {
            if !current_unmapped.is_empty() {
                segments.push(TextSegment::Unmapped(std::mem::take(&mut current_unmapped)));
            }
            segments.push(TextSegment::Mapped(c, key, needs_shift));
        } else {
            current_unmapped.push(c);
        }
    }
    if !current_unmapped.is_empty() {
        segments.push(TextSegment::Unmapped(current_unmapped));
    }

    segments
}

pub struct RecordingInjector {
    pub injected_strings: Arc<Mutex<Vec<String>>>,
    pub emitted_events: Arc<Mutex<Vec<KeyStroke>>>,
    pub fallback_pastes: Arc<Mutex<Vec<String>>>,
    pub recorded_delays: Arc<Mutex<Vec<Duration>>>,
    pub pacing_delay: Duration,
    pub settle_modifiers: bool,
    pub mode: InjectionMode,
}

impl RecordingInjector {
    pub fn new(pacing_delay: Duration) -> Self {
        Self::with_mode(
            InjectionMode::Typewriter {
                delay_ms: pacing_delay.as_millis() as u64,
            },
            false,
        )
    }

    pub fn with_settle(pacing_delay: Duration, settle_modifiers: bool) -> Self {
        Self::with_mode(
            InjectionMode::Typewriter {
                delay_ms: pacing_delay.as_millis() as u64,
            },
            settle_modifiers,
        )
    }

    pub fn with_mode(mode: InjectionMode, settle_modifiers: bool) -> Self {
        let pacing_delay = match mode {
            InjectionMode::Typewriter { delay_ms } => Duration::from_millis(delay_ms),
            InjectionMode::Paste => Duration::ZERO,
        };
        Self {
            injected_strings: Arc::new(Mutex::new(Vec::new())),
            emitted_events: Arc::new(Mutex::new(Vec::new())),
            fallback_pastes: Arc::new(Mutex::new(Vec::new())),
            recorded_delays: Arc::new(Mutex::new(Vec::new())),
            pacing_delay,
            settle_modifiers,
            mode,
        }
    }

    pub fn mode(&self) -> InjectionMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: InjectionMode) {
        self.mode = mode;
        self.pacing_delay = match mode {
            InjectionMode::Typewriter { delay_ms } => Duration::from_millis(delay_ms),
            InjectionMode::Paste => Duration::ZERO,
        };
    }

    pub fn reset_modifiers(&mut self) {
        let modifiers = [
            KeyCode::KEY_LEFTALT,
            KeyCode::KEY_RIGHTALT,
            KeyCode::KEY_LEFTSHIFT,
            KeyCode::KEY_RIGHTSHIFT,
            KeyCode::KEY_LEFTCTRL,
            KeyCode::KEY_RIGHTCTRL,
            KeyCode::KEY_LEFTMETA,
            KeyCode::KEY_RIGHTMETA,
        ];
        for key in modifiers {
            self.emitted_events.lock().unwrap().push(KeyStroke {
                key,
                is_down: false,
            });
        }
    }

    fn emit_keystroke(&self, key: KeyCode, is_down: bool) {
        self.emitted_events
            .lock()
            .unwrap()
            .push(KeyStroke { key, is_down });
        if !self.pacing_delay.is_zero() {
            self.recorded_delays.lock().unwrap().push(self.pacing_delay);
            thread::sleep(self.pacing_delay);
        }
    }
}

impl TextInjector for RecordingInjector {
    fn inject(&mut self, text: &str) -> Result<(), String> {
        self.injected_strings.lock().unwrap().push(text.to_string());

        if self.settle_modifiers {
            self.reset_modifiers();
        }

        match self.mode {
            InjectionMode::Paste => {
                self.fallback_pastes.lock().unwrap().push(text.to_string());
            }
            InjectionMode::Typewriter { .. } => {
                let segments = segment_text(text);
                for segment in segments {
                    match segment {
                        TextSegment::Mapped(_c, key, needs_shift) => {
                            if needs_shift {
                                self.emit_keystroke(KeyCode::KEY_LEFTSHIFT, true);
                            }
                            self.emit_keystroke(key, true);
                            self.emit_keystroke(key, false);
                            if needs_shift {
                                self.emit_keystroke(KeyCode::KEY_LEFTSHIFT, false);
                            }
                        }
                        TextSegment::Unmapped(fallback_text) => {
                            self.fallback_pastes.lock().unwrap().push(fallback_text);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "RecordingInjector"
    }
}

pub fn create_virtual_device() -> Result<VirtualDevice, String> {
    let mut keys = AttributeSet::<KeyCode>::new();
    for code in 1..=255 {
        keys.insert(KeyCode(code));
    }

    VirtualDevice::builder()
        .map_err(|e| format!("Failed to create uinput builder: {}", e))?
        .name("Speechyyy Virtual Keyboard")
        .with_keys(&keys)
        .map_err(|e| format!("Failed to register keys: {}", e))?
        .build()
        .map_err(|e| {
            format!(
                "Failed to create uinput virtual device (is /dev/uinput writable?): {}",
                e
            )
        })
}

pub fn emit_defuse_and_paste<F, S>(mut emit_batch: F, mut sleep_fn: S) -> Result<(), String>
where
    F: FnMut(&[InputEvent]) -> Result<(), String>,
    S: FnMut(Duration),
{
    // Synthesize paste chord: KEY_LEFTCTRL down -> KEY_V down -> SYN_REPORT -> 20ms sleep -> KEY_V up -> KEY_LEFTCTRL up -> SYN_REPORT
    let ctrl_v_down = [
        InputEvent::new(EventType::KEY.0, KeyCode::KEY_LEFTCTRL.0, 1),
        InputEvent::new(EventType::KEY.0, KeyCode::KEY_V.0, 1),
    ];
    emit_batch(&ctrl_v_down)?;

    sleep_fn(WaylandClipboardInjector::PASTE_HOLD_DELAY);

    let ctrl_v_up = [
        InputEvent::new(EventType::KEY.0, KeyCode::KEY_V.0, 0),
        InputEvent::new(EventType::KEY.0, KeyCode::KEY_LEFTCTRL.0, 0),
    ];
    emit_batch(&ctrl_v_up)?;

    Ok(())
}

pub struct UinputInjector {
    device: VirtualDevice,
    pacing_delay: Duration,
    mode: InjectionMode,
}

impl UinputInjector {
    pub const MODIFIER_SETTLE_DELAY: Duration = Duration::from_millis(50);

    pub fn new(pacing_delay: Duration) -> Result<Self, String> {
        let device = create_virtual_device()?;
        let mode = InjectionMode::Typewriter {
            delay_ms: pacing_delay.as_millis() as u64,
        };
        Ok(Self {
            device,
            pacing_delay,
            mode,
        })
    }

    pub fn with_mode(mut self, mode: InjectionMode) -> Self {
        self.set_mode(mode);
        self
    }

    pub fn mode(&self) -> InjectionMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: InjectionMode) {
        self.mode = mode;
        self.pacing_delay = match mode {
            InjectionMode::Typewriter { delay_ms } => Duration::from_millis(delay_ms),
            InjectionMode::Paste => Duration::ZERO,
        };
    }

    pub fn reset_modifiers(&mut self) -> Result<(), String> {
        let modifiers = [
            KeyCode::KEY_LEFTALT,
            KeyCode::KEY_RIGHTALT,
            KeyCode::KEY_LEFTSHIFT,
            KeyCode::KEY_RIGHTSHIFT,
            KeyCode::KEY_LEFTCTRL,
            KeyCode::KEY_RIGHTCTRL,
            KeyCode::KEY_LEFTMETA,
            KeyCode::KEY_RIGHTMETA,
        ];
        let events: Vec<InputEvent> = modifiers
            .iter()
            .map(|k| InputEvent::new(EventType::KEY.0, k.0, 0))
            .collect();
        self.device
            .emit(&events)
            .map_err(|e| format!("Failed to emit modifier reset: {}", e))?;

        thread::sleep(Self::MODIFIER_SETTLE_DELAY);
        Ok(())
    }

    pub fn settle_modifiers(&mut self) -> Result<(), String> {
        self.reset_modifiers()
    }

    fn emit_key(&mut self, key: KeyCode, is_down: bool) -> Result<(), String> {
        let val = if is_down { 1 } else { 0 };
        let ev = InputEvent::new(EventType::KEY.0, key.0, val);
        self.device
            .emit(&[ev])
            .map_err(|e| format!("Failed to emit uinput event: {}", e))?;

        if !self.pacing_delay.is_zero() {
            thread::sleep(self.pacing_delay);
        }
        Ok(())
    }
}

impl TextInjector for UinputInjector {
    fn inject(&mut self, text: &str) -> Result<(), String> {
        self.reset_modifiers()?;

        let res = match self.mode {
            InjectionMode::Paste => {
                WaylandClipboardInjector::copy_to_clipboard(text)?;
                thread::sleep(WaylandClipboardInjector::SETTLE_DELAY);
                WaylandClipboardInjector::trigger_uinput_paste(&mut self.device)
            }
            InjectionMode::Typewriter { .. } => {
                let segments = segment_text(text);
                let mut type_res = Ok(());
                for segment in segments {
                    match segment {
                        TextSegment::Mapped(_c, key, needs_shift) => {
                            if needs_shift {
                                if let Err(e) = self.emit_key(KeyCode::KEY_LEFTSHIFT, true) {
                                    type_res = Err(e);
                                    break;
                                }
                            }
                            if let Err(e) = self.emit_key(key, true) {
                                type_res = Err(e);
                                break;
                            }
                            if let Err(e) = self.emit_key(key, false) {
                                type_res = Err(e);
                                break;
                            }
                            if needs_shift {
                                if let Err(e) = self.emit_key(KeyCode::KEY_LEFTSHIFT, false) {
                                    type_res = Err(e);
                                    break;
                                }
                            }
                        }
                        TextSegment::Unmapped(fallback_text) => {
                            if let Err(e) =
                                WaylandClipboardInjector::copy_to_clipboard(&fallback_text)
                            {
                                type_res = Err(e);
                                break;
                            }
                            thread::sleep(WaylandClipboardInjector::SETTLE_DELAY);
                            if let Err(e) =
                                WaylandClipboardInjector::trigger_uinput_paste(&mut self.device)
                            {
                                type_res = Err(e);
                                break;
                            }
                        }
                    }
                }
                type_res
            }
        };

        if res.is_err() {
            let _ = self.reset_modifiers();
        }
        res
    }

    fn name(&self) -> &'static str {
        match self.mode {
            InjectionMode::Typewriter { .. } => "uinput virtual keyboard (typewriter)",
            InjectionMode::Paste => "uinput clipboard paste",
        }
    }
}

pub struct WaylandClipboardInjector {
    device: Option<VirtualDevice>,
    settle_delay: Duration,
}

impl WaylandClipboardInjector {
    pub const SETTLE_DELAY: Duration = Duration::from_millis(80);
    pub const PASTE_HOLD_DELAY: Duration = Duration::from_millis(20);

    pub fn new(_pacing_delay: Duration) -> Self {
        let device = create_virtual_device().ok();
        Self {
            device,
            settle_delay: Self::SETTLE_DELAY,
        }
    }

    pub fn with_device(device: Option<VirtualDevice>, settle_delay: Duration) -> Self {
        Self {
            device,
            settle_delay,
        }
    }

    pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
        let mut child = std::process::Command::new("wl-copy")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn wl-copy: {}", e))?;

        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            stdin
                .write_all(text.as_bytes())
                .map_err(|e| format!("Failed to write to wl-copy stdin: {}", e))?;
        }

        let status = child
            .wait()
            .map_err(|e| format!("Failed to wait for wl-copy: {}", e))?;
        if !status.success() {
            return Err(format!("wl-copy exited with error status: {}", status));
        }
        Ok(())
    }

    pub fn trigger_uinput_paste(device: &mut VirtualDevice) -> Result<(), String> {
        emit_defuse_and_paste(
            |batch| {
                device
                    .emit(batch)
                    .map_err(|e| format!("Failed to emit paste event batch: {}", e))
            },
            thread::sleep,
        )
    }

    pub fn fallback_wtype_paste() -> Result<(), String> {
        // Synthetic Ctrl+V paste via wtype
        let status = std::process::Command::new("wtype")
            .args(["-M", "ctrl", "-k", "v", "-m", "ctrl"])
            .status()
            .map_err(|e| format!("Failed to run wtype paste shortcut: {}", e))?;

        if !status.success() {
            return Err(format!("wtype exited with status: {}", status));
        }
        Ok(())
    }
}

impl TextInjector for WaylandClipboardInjector {
    fn inject(&mut self, text: &str) -> Result<(), String> {
        Self::copy_to_clipboard(text)?;
        thread::sleep(self.settle_delay);

        if self.device.is_none() {
            self.device = create_virtual_device().ok();
        }

        if let Some(ref mut device) = self.device {
            Self::trigger_uinput_paste(device)?;
        } else if let Err(e) = Self::fallback_wtype_paste() {
            eprintln!(
                "Wayland paste warning: could not emit uinput paste chord and wtype failed: {}. Transcript is in clipboard.",
                e
            );
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "Wayland clipboard (wl-copy + uinput paste)"
    }
}

pub fn create_injector(config: &crate::config::Config) -> Result<Box<dyn TextInjector>, String> {
    let pacing = Duration::from_millis(config.pacing_delay_ms);
    let method = config.injector.method.to_lowercase();

    match method.as_str() {
        "clipboard" | "wayland" | "paste" => Ok(Box::new(WaylandClipboardInjector::new(pacing))),
        "uinput" => {
            let inj = UinputInjector::new(pacing)?;
            Ok(Box::new(inj))
        }
        "typewriter" => match UinputInjector::new(pacing) {
            Ok(inj) => Ok(Box::new(inj.with_mode(InjectionMode::Typewriter {
                delay_ms: config.pacing_delay_ms,
            }))),
            Err(e) => {
                eprintln!(
                    "UinputInjector initialization failed ({}); typewriter mode falling back to clipboard paste",
                    e
                );
                Ok(Box::new(WaylandClipboardInjector::new(pacing)))
            }
        },
        "auto" => {
            let is_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
            if is_wayland {
                Ok(Box::new(WaylandClipboardInjector::new(pacing)))
            } else {
                match UinputInjector::new(pacing) {
                    Ok(inj) => Ok(Box::new(inj)),
                    Err(e) => {
                        eprintln!(
                            "UinputInjector initialization failed ({}); falling back to Wayland clipboard injector",
                            e
                        );
                        Ok(Box::new(WaylandClipboardInjector::new(pacing)))
                    }
                }
            }
        }
        other => Err(format!(
            "Unknown injector method '{}'. Supported: 'auto', 'typewriter', 'uinput', 'clipboard', 'paste'.",
            other
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_char_to_key_mappings() {
        assert_eq!(char_to_key('a'), Some((KeyCode::KEY_A, false)));
        assert_eq!(char_to_key('H'), Some((KeyCode::KEY_H, true)));
        assert_eq!(char_to_key(' '), Some((KeyCode::KEY_SPACE, false)));
        assert_eq!(char_to_key('!'), Some((KeyCode::KEY_1, true)));
        assert_eq!(char_to_key('"'), Some((KeyCode::KEY_APOSTROPHE, true)));
        assert_eq!(char_to_key('['), Some((KeyCode::KEY_LEFTBRACE, false)));
    }

    #[test]
    fn test_recording_injector_captures_keystrokes_and_shifts() {
        let mut injector = RecordingInjector::new(Duration::ZERO);
        injector.inject("Hi!").expect("inject string");

        let strings = injector.injected_strings.lock().unwrap();
        assert_eq!(*strings, vec!["Hi!".to_string()]);

        let events = injector.emitted_events.lock().unwrap();
        // 'H' -> Shift down, H down, H up, Shift up
        // 'i' -> i down, i up
        // '!' -> Shift down, 1 down, 1 up, Shift up
        assert_eq!(events[0].key, KeyCode::KEY_LEFTSHIFT);
        assert!(events[0].is_down);
        assert_eq!(events[1].key, KeyCode::KEY_H);
        assert!(events[1].is_down);
        assert_eq!(events[2].key, KeyCode::KEY_H);
        assert!(!events[2].is_down);
        assert_eq!(events[3].key, KeyCode::KEY_LEFTSHIFT);
        assert!(!events[3].is_down);

        assert_eq!(events[4].key, KeyCode::KEY_I);
        assert!(events[4].is_down);
        assert_eq!(events[5].key, KeyCode::KEY_I);
        assert!(!events[5].is_down);

        assert_eq!(events[6].key, KeyCode::KEY_LEFTSHIFT);
        assert!(events[6].is_down);
        assert_eq!(events[7].key, KeyCode::KEY_1);
        assert!(events[7].is_down);
        assert_eq!(events[8].key, KeyCode::KEY_1);
        assert!(!events[8].is_down);
        assert_eq!(events[9].key, KeyCode::KEY_LEFTSHIFT);
        assert!(!events[9].is_down);
    }

    #[test]
    fn test_recording_injector_pacing_delay() {
        let pacing = Duration::from_millis(2);
        let mut injector = RecordingInjector::new(pacing);
        let start = std::time::Instant::now();
        injector.inject("ab").unwrap();
        let elapsed = start.elapsed();
        // 2 characters, each has down and up (4 transitions * 2ms = ~8ms minimum)
        assert!(
            elapsed >= Duration::from_millis(7),
            "Expected pacing delay to be respected, got {:?}",
            elapsed
        );
    }

    #[test]
    fn test_recording_injector_modifier_reset_sequence() {
        let mut injector = RecordingInjector::with_settle(Duration::ZERO, true);
        injector.inject("a").unwrap();

        let events = injector.emitted_events.lock().unwrap();
        // The first 8 events must be modifier KeyUp releases
        let expected_modifiers = [
            KeyCode::KEY_LEFTALT,
            KeyCode::KEY_RIGHTALT,
            KeyCode::KEY_LEFTSHIFT,
            KeyCode::KEY_RIGHTSHIFT,
            KeyCode::KEY_LEFTCTRL,
            KeyCode::KEY_RIGHTCTRL,
            KeyCode::KEY_LEFTMETA,
            KeyCode::KEY_RIGHTMETA,
        ];

        assert!(events.len() >= 10);
        for (i, &expected_key) in expected_modifiers.iter().enumerate() {
            assert_eq!(events[i].key, expected_key);
            assert!(!events[i].is_down, "Modifier event must be KeyUp");
        }

        // Following events should be 'a' down and 'a' up
        assert_eq!(events[8].key, KeyCode::KEY_A);
        assert!(events[8].is_down);
        assert_eq!(events[9].key, KeyCode::KEY_A);
        assert!(!events[9].is_down);
    }
}
