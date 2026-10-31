use evdev::KeyCode;
use speechyyy::config::Config;
use speechyyy::injector::{char_to_key, KeyStroke, RecordingInjector, TextInjector};
use std::time::Duration;

#[test]
fn test_default_config_pacing_delay_is_at_least_8ms() {
    let config = Config::default();
    assert!(
        config.pacing_delay_ms >= 8 && config.pacing_delay_ms <= 10,
        "Default pacing delay must be 8-10ms for Wayland stability, got {}ms",
        config.pacing_delay_ms
    );
}

#[test]
fn test_char_to_key_covers_all_printable_ascii() {
    // All printable ASCII characters from 32 (' ') to 126 ('~') must map to a KeyCode
    for b in 32u8..=126u8 {
        let c = b as char;
        assert!(
            char_to_key(c).is_some(),
            "Printable ASCII character '{}' (0x{:02x}) must be mapped by char_to_key",
            c,
            b
        );
    }

    // Explicit checks for symbols that were previously missing
    assert_eq!(char_to_key('\''), Some((KeyCode::KEY_APOSTROPHE, false)));
    assert_eq!(char_to_key('"'), Some((KeyCode::KEY_APOSTROPHE, true)));
    assert_eq!(char_to_key('['), Some((KeyCode::KEY_LEFTBRACE, false)));
    assert_eq!(char_to_key(']'), Some((KeyCode::KEY_RIGHTBRACE, false)));
    assert_eq!(char_to_key('{'), Some((KeyCode::KEY_LEFTBRACE, true)));
    assert_eq!(char_to_key('}'), Some((KeyCode::KEY_RIGHTBRACE, true)));
    assert_eq!(char_to_key('='), Some((KeyCode::KEY_EQUAL, false)));
    assert_eq!(char_to_key('+'), Some((KeyCode::KEY_EQUAL, true)));
    assert_eq!(char_to_key('\\'), Some((KeyCode::KEY_BACKSLASH, false)));
    assert_eq!(char_to_key('|'), Some((KeyCode::KEY_BACKSLASH, true)));
    assert_eq!(char_to_key('`'), Some((KeyCode::KEY_GRAVE, false)));
    assert_eq!(char_to_key('~'), Some((KeyCode::KEY_GRAVE, true)));
    assert_eq!(char_to_key('<'), Some((KeyCode::KEY_COMMA, true)));
    assert_eq!(char_to_key('>'), Some((KeyCode::KEY_DOT, true)));
}

#[test]
fn test_recording_injector_modifier_settle_releases_right_alt_before_characters() {
    // Injector with settle enabled
    let mut injector = RecordingInjector::with_settle(Duration::ZERO, true);
    injector.inject("hi").expect("inject string");

    let events: Vec<KeyStroke> = injector.emitted_events.lock().unwrap().clone();
    assert!(
        !events.is_empty(),
        "Events should not be empty after injection"
    );

    // The first event must be a modifier release (specifically KeyCode::KEY_RIGHTALT is_down: false)
    let first_right_alt_idx = events
        .iter()
        .position(|ev| ev.key == KeyCode::KEY_RIGHTALT && !ev.is_down);
    assert!(
        first_right_alt_idx.is_some(),
        "Expected KEY_RIGHTALT release before character emission"
    );

    let h_key_idx = events
        .iter()
        .position(|ev| ev.key == KeyCode::KEY_H && ev.is_down)
        .expect("KEY_H down must be emitted");

    assert!(
        first_right_alt_idx.unwrap() < h_key_idx,
        "KEY_RIGHTALT release must precede KEY_H down event"
    );
}

#[test]
fn test_injector_send_text_alias_trait_method() {
    let mut injector = RecordingInjector::new(Duration::ZERO);
    injector.send_text("Speechyyy").expect("send_text alias");

    let strings = injector.injected_strings.lock().unwrap();
    assert_eq!(*strings, vec!["Speechyyy".to_string()]);
}

#[test]
fn test_wayland_clipboard_injector_trait_implementation() {
    let injector = speechyyy::injector::WaylandClipboardInjector::new(Duration::from_millis(10));
    assert_eq!(
        injector.name(),
        "Wayland clipboard (wl-copy + uinput paste)"
    );
}

#[test]
fn test_wayland_clipboard_injector_delays() {
    assert_eq!(
        speechyyy::injector::WaylandClipboardInjector::SETTLE_DELAY,
        Duration::from_millis(80)
    );
    assert_eq!(
        speechyyy::injector::WaylandClipboardInjector::PASTE_HOLD_DELAY,
        Duration::from_millis(20)
    );
}

#[test]
fn test_emit_defuse_and_paste_sequence() {
    use evdev::InputEvent;
    use speechyyy::injector::emit_defuse_and_paste;

    let mut emitted_batches: Vec<Vec<InputEvent>> = Vec::new();
    let mut sleeps: Vec<Duration> = Vec::new();

    let res = emit_defuse_and_paste(
        |batch| {
            emitted_batches.push(batch.to_vec());
            Ok(())
        },
        |duration| {
            sleeps.push(duration);
        },
    );
    assert!(res.is_ok());

    // Clean Ctrl+V chord: exactly 2 batches (no KEY_ESC defuse)
    assert_eq!(emitted_batches.len(), 2);
    assert_eq!(sleeps.len(), 1);

    // Batch 1: KEY_LEFTCTRL down, KEY_V down
    assert_eq!(emitted_batches[0].len(), 2);
    assert_eq!(emitted_batches[0][0].code(), KeyCode::KEY_LEFTCTRL.0);
    assert_eq!(emitted_batches[0][0].value(), 1);
    assert_eq!(emitted_batches[0][1].code(), KeyCode::KEY_V.0);
    assert_eq!(emitted_batches[0][1].value(), 1);

    // Sleep: 20ms paste hold
    assert_eq!(sleeps[0], Duration::from_millis(20));

    // Batch 2: KEY_V up, KEY_LEFTCTRL up
    assert_eq!(emitted_batches[1].len(), 2);
    assert_eq!(emitted_batches[1][0].code(), KeyCode::KEY_V.0);
    assert_eq!(emitted_batches[1][0].value(), 0);
    assert_eq!(emitted_batches[1][1].code(), KeyCode::KEY_LEFTCTRL.0);
    assert_eq!(emitted_batches[1][1].value(), 0);

    // Verify KEY_ESC is not emitted anywhere
    for batch in &emitted_batches {
        for ev in batch {
            assert_ne!(
                ev.code(),
                KeyCode::KEY_ESC.0,
                "KEY_ESC must not be emitted as it blurs webview inputs"
            );
        }
    }
}

#[test]
fn test_uinput_injector_settle_delay_constant() {
    assert_eq!(
        speechyyy::injector::UinputInjector::MODIFIER_SETTLE_DELAY,
        Duration::from_millis(50)
    );
}

#[test]
fn test_create_injector_respects_config_method() {
    let mut config = Config::default();
    config.injector.method = "clipboard".into();
    let injector =
        speechyyy::injector::create_injector(&config).expect("create clipboard injector");
    assert_eq!(
        injector.name(),
        "Wayland clipboard (wl-copy + uinput paste)"
    );
}

#[test]
fn test_uinput_isolated_send_text_or_permission_error() {
    let pacing = Duration::from_millis(10);
    match speechyyy::injector::UinputInjector::new(pacing) {
        Ok(mut injector) => {
            // If /dev/uinput is writable on this machine, test emission directly
            let res = injector.send_text("test");
            assert!(res.is_ok(), "uinput send_text failed: {:?}", res);
        }
        Err(e) => {
            // Expected in unprivileged/sandboxed test environments
            assert!(
                e.contains("uinput") || e.contains("permission") || e.contains("writable"),
                "Expected descriptive permission error, got: {}",
                e
            );
        }
    }
}
