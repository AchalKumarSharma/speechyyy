use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, EventType, InputEvent, KeyCode};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub trait TextInjector: Send + Sync {
    fn inject(&mut self, text: &str) -> Result<(), String>;
}

pub struct KeyStroke {
    pub key: KeyCode,
    pub is_down: bool,
}

pub fn char_to_key(c: char) -> Option<(KeyCode, bool)> {
    match c {
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

        ' ' => Some((KeyCode::KEY_SPACE, false)),
        '\n' => Some((KeyCode::KEY_ENTER, false)),
        '\t' => Some((KeyCode::KEY_TAB, false)),
        '.' => Some((KeyCode::KEY_DOT, false)),
        ',' => Some((KeyCode::KEY_COMMA, false)),
        '-' => Some((KeyCode::KEY_MINUS, false)),
        '_' => Some((KeyCode::KEY_MINUS, true)),
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
        ':' => Some((KeyCode::KEY_SEMICOLON, true)),
        ';' => Some((KeyCode::KEY_SEMICOLON, false)),
        '/' => Some((KeyCode::KEY_SLASH, false)),
        '?' => Some((KeyCode::KEY_SLASH, true)),
        _ => None,
    }
}

pub struct RecordingInjector {
    pub injected_strings: Arc<Mutex<Vec<String>>>,
    pub emitted_events: Arc<Mutex<Vec<KeyStroke>>>,
    pub pacing_delay: Duration,
}

impl RecordingInjector {
    pub fn new(pacing_delay: Duration) -> Self {
        Self {
            injected_strings: Arc::new(Mutex::new(Vec::new())),
            emitted_events: Arc::new(Mutex::new(Vec::new())),
            pacing_delay,
        }
    }
}

impl TextInjector for RecordingInjector {
    fn inject(&mut self, text: &str) -> Result<(), String> {
        self.injected_strings.lock().unwrap().push(text.to_string());
        for c in text.chars() {
            if let Some((key, needs_shift)) = char_to_key(c) {
                if needs_shift {
                    self.emitted_events.lock().unwrap().push(KeyStroke {
                        key: KeyCode::KEY_LEFTSHIFT,
                        is_down: true,
                    });
                    if !self.pacing_delay.is_zero() {
                        thread::sleep(self.pacing_delay);
                    }
                }
                self.emitted_events.lock().unwrap().push(KeyStroke { key, is_down: true });
                if !self.pacing_delay.is_zero() {
                    thread::sleep(self.pacing_delay);
                }
                self.emitted_events.lock().unwrap().push(KeyStroke { key, is_down: false });
                if !self.pacing_delay.is_zero() {
                    thread::sleep(self.pacing_delay);
                }
                if needs_shift {
                    self.emitted_events.lock().unwrap().push(KeyStroke {
                        key: KeyCode::KEY_LEFTSHIFT,
                        is_down: false,
                    });
                    if !self.pacing_delay.is_zero() {
                        thread::sleep(self.pacing_delay);
                    }
                }
            }
        }
        Ok(())
    }
}

pub struct UinputInjector {
    device: VirtualDevice,
    pacing_delay: Duration,
}

impl UinputInjector {
    pub fn new(pacing_delay: Duration) -> Result<Self, String> {
        let mut keys = AttributeSet::<KeyCode>::new();
        for code in 1..=255 {
            keys.insert(KeyCode(code));
        }

        let device = VirtualDevice::builder()
            .map_err(|e| format!("Failed to create uinput builder: {}", e))?
            .name("Speechyyy Virtual Keyboard")
            .with_keys(&keys)
            .map_err(|e| format!("Failed to register keys: {}", e))?
            .build()
            .map_err(|e| format!("Failed to create uinput virtual device (is /dev/uinput writable?): {}", e))?;

        Ok(Self {
            device,
            pacing_delay,
        })
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
        for c in text.chars() {
            if let Some((key, needs_shift)) = char_to_key(c) {
                if needs_shift {
                    self.emit_key(KeyCode::KEY_LEFTSHIFT, true)?;
                }
                self.emit_key(key, true)?;
                self.emit_key(key, false)?;
                if needs_shift {
                    self.emit_key(KeyCode::KEY_LEFTSHIFT, false)?;
                }
            }
        }
        Ok(())
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
        assert_eq!(events[0].is_down, true);
        assert_eq!(events[1].key, KeyCode::KEY_H);
        assert_eq!(events[1].is_down, true);
        assert_eq!(events[2].key, KeyCode::KEY_H);
        assert_eq!(events[2].is_down, false);
        assert_eq!(events[3].key, KeyCode::KEY_LEFTSHIFT);
        assert_eq!(events[3].is_down, false);

        assert_eq!(events[4].key, KeyCode::KEY_I);
        assert_eq!(events[4].is_down, true);
        assert_eq!(events[5].key, KeyCode::KEY_I);
        assert_eq!(events[5].is_down, false);

        assert_eq!(events[6].key, KeyCode::KEY_LEFTSHIFT);
        assert_eq!(events[6].is_down, true);
        assert_eq!(events[7].key, KeyCode::KEY_1);
        assert_eq!(events[7].is_down, true);
        assert_eq!(events[8].key, KeyCode::KEY_1);
        assert_eq!(events[8].is_down, false);
        assert_eq!(events[9].key, KeyCode::KEY_LEFTSHIFT);
        assert_eq!(events[9].is_down, false);
    }

    #[test]
    fn test_recording_injector_pacing_delay() {
        let pacing = Duration::from_millis(2);
        let mut injector = RecordingInjector::new(pacing);
        let start = std::time::Instant::now();
        injector.inject("ab").unwrap();
        let elapsed = start.elapsed();
        // 2 characters, each has down and up (4 transitions * 2ms = ~8ms minimum)
        assert!(elapsed >= Duration::from_millis(7), "Expected pacing delay to be respected, got {:?}", elapsed);
    }
}
