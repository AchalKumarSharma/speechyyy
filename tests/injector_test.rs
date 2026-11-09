use evdev::KeyCode;
use speechyyy::config::Config;
use speechyyy::injector::{
    char_to_key, is_whitespace_or_empty, KeyStroke, MockClipboardWriter, MockKeySink,
    RecordingInjector, TextInjector, WaylandClipboardInjector,
};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[test]
fn test_default_config_pacing_delay_is_at_least_8ms() {
    let config = Config::default();
    assert!(
        config.pacing_delay_ms >= 8 && config.pacing_delay_ms <= 20,
        "Default pacing delay must be 8-20ms for Wayland stability, got {}ms",
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
    match speechyyy::injector::create_injector(&config) {
        Ok(injector) => {
            assert_eq!(
                injector.name(),
                "Wayland clipboard (wl-copy + uinput paste)"
            );
        }
        Err(e)
            if e.contains("uinput")
                || e.contains("Permission denied")
                || e.contains("permission")
                || e.contains("accessible")
                || e.contains("No such file") =>
        {
            eprintln!(
                "Skipping test_create_injector_respects_config_method: /dev/uinput not accessible in this environment ({})",
                e
            );
        }
        Err(e) => panic!("create clipboard injector unexpected error: {}", e),
    }
}

#[test]
#[ignore = "touches real /dev/uinput device and injects keystrokes"]
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

#[test]
fn test_injection_mode_configuration_and_serialization() {
    use speechyyy::injector::InjectionMode;

    let default_mode = InjectionMode::default();
    assert_eq!(default_mode, InjectionMode::Typewriter { delay_ms: 15 });

    let paste_mode = InjectionMode::Paste;
    let serialized_paste = serde_json::to_string(&paste_mode).unwrap();
    assert_eq!(serialized_paste, r#""paste""#);

    let typewriter_mode = InjectionMode::Typewriter { delay_ms: 20 };
    let serialized_tw = serde_json::to_string(&typewriter_mode).unwrap();
    assert!(serialized_tw.contains(r#""typewriter""#) && serialized_tw.contains("20"));

    let deserialized: InjectionMode = serde_json::from_str(&serialized_tw).unwrap();
    assert_eq!(deserialized, typewriter_mode);

    let mut injector = RecordingInjector::with_mode(typewriter_mode, false);
    assert_eq!(injector.mode(), typewriter_mode);
    assert_eq!(injector.pacing_delay, Duration::from_millis(20));

    injector.set_mode(InjectionMode::Typewriter { delay_ms: 35 });
    assert_eq!(injector.mode(), InjectionMode::Typewriter { delay_ms: 35 });
    assert_eq!(injector.pacing_delay, Duration::from_millis(35));
}

#[test]
fn test_typewriter_pacing_verification() {
    use speechyyy::injector::InjectionMode;

    let mut injector =
        RecordingInjector::with_mode(InjectionMode::Typewriter { delay_ms: 12 }, false);
    injector.inject("hello").expect("inject string");

    let events = injector.emitted_events.lock().unwrap().clone();
    // 5 letters ('h', 'e', 'l', 'l', 'o') -> 5 down, 5 up
    assert_eq!(
        events.len(),
        10,
        "Expected 10 events for 5 lowercase letters"
    );

    for i in 0..5 {
        assert!(events[i * 2].is_down, "Event {} must be key down", i * 2);
        assert!(
            !events[i * 2 + 1].is_down,
            "Event {} must be key up",
            i * 2 + 1
        );
        assert_eq!(events[i * 2].key, events[i * 2 + 1].key);
    }

    // Verify pacing delay recording: each transition had 12ms delay recorded
    let delays = injector.recorded_delays.lock().unwrap().clone();
    assert_eq!(
        delays.len(),
        10,
        "Expected 10 recorded pacing delays for 10 transitions"
    );
    for d in delays {
        assert_eq!(d, Duration::from_millis(12));
    }
}

#[test]
fn test_typewriter_shift_chord_verification() {
    use speechyyy::injector::InjectionMode;

    // 1. All uppercase letters 'A'..='Z'
    for c in 'A'..='Z' {
        let mut injector =
            RecordingInjector::with_mode(InjectionMode::Typewriter { delay_ms: 0 }, false);
        let s = c.to_string();
        injector.inject(&s).expect("inject uppercase letter");

        let events = injector.emitted_events.lock().unwrap().clone();
        assert_eq!(
            events.len(),
            4,
            "Shift chord for '{}' must emit exactly 4 events (Shift down, Key down, Key up, Shift up)",
            c
        );
        assert_eq!(events[0].key, KeyCode::KEY_LEFTSHIFT);
        assert!(events[0].is_down, "Shift must be down");
        assert!(events[1].is_down, "Character key must be down");
        assert_ne!(events[1].key, KeyCode::KEY_LEFTSHIFT);
        assert!(!events[2].is_down, "Character key must be up");
        assert_eq!(events[1].key, events[2].key);
        assert_eq!(events[3].key, KeyCode::KEY_LEFTSHIFT);
        assert!(!events[3].is_down, "Shift must be up");
    }

    // 2. Shifted punctuation: !, @, #, $, %, ^, &, *, (, ), _, +, {, }, |, :, ", <, >, ?
    let shifted_symbols = [
        '!', '@', '#', '$', '%', '^', '&', '*', '(', ')', '_', '+', '{', '}', '|', ':', '"', '<',
        '>', '?',
    ];
    for &sym in &shifted_symbols {
        let mut injector =
            RecordingInjector::with_mode(InjectionMode::Typewriter { delay_ms: 0 }, false);
        let s = sym.to_string();
        injector.inject(&s).expect("inject shifted punctuation");

        let events = injector.emitted_events.lock().unwrap().clone();
        assert_eq!(
            events.len(),
            4,
            "Shift chord for symbol '{}' must emit exactly 4 events (Shift down, Key down, Key up, Shift up)",
            sym
        );
        assert_eq!(events[0].key, KeyCode::KEY_LEFTSHIFT);
        assert!(events[0].is_down);
        assert!(events[1].is_down);
        assert_ne!(events[1].key, KeyCode::KEY_LEFTSHIFT);
        assert!(!events[2].is_down);
        assert_eq!(events[1].key, events[2].key);
        assert_eq!(events[3].key, KeyCode::KEY_LEFTSHIFT);
        assert!(!events[3].is_down);
    }
}

#[test]
fn test_typewriter_whitespace_and_digits() {
    use speechyyy::injector::InjectionMode;

    let test_digits = [
        ('0', KeyCode::KEY_0),
        ('1', KeyCode::KEY_1),
        ('2', KeyCode::KEY_2),
        ('3', KeyCode::KEY_3),
        ('4', KeyCode::KEY_4),
        ('5', KeyCode::KEY_5),
        ('6', KeyCode::KEY_6),
        ('7', KeyCode::KEY_7),
        ('8', KeyCode::KEY_8),
        ('9', KeyCode::KEY_9),
    ];

    for (c, expected_key) in test_digits {
        let mut injector =
            RecordingInjector::with_mode(InjectionMode::Typewriter { delay_ms: 0 }, false);
        let s = c.to_string();
        injector.inject(&s).expect("inject digit");

        let events = injector.emitted_events.lock().unwrap().clone();
        assert_eq!(
            events.len(),
            2,
            "Character '{}' must emit exactly 2 events without Shift",
            c
        );
        assert_eq!(events[0].key, expected_key);
        assert!(events[0].is_down);
        assert_eq!(events[1].key, expected_key);
        assert!(!events[1].is_down);

        // Assert Shift is NOT emitted
        for ev in &events {
            assert_ne!(
                ev.key,
                KeyCode::KEY_LEFTSHIFT,
                "Shift must not be emitted for '{}'",
                c
            );
        }
    }

    let test_whitespace = [
        (' ', KeyCode::KEY_SPACE),
        ('\t', KeyCode::KEY_TAB),
        ('\n', KeyCode::KEY_ENTER),
    ];

    for (c, expected_key) in test_whitespace {
        let mut injector =
            RecordingInjector::with_mode(InjectionMode::Typewriter { delay_ms: 0 }, false);
        // Non-empty string with leading digit to ensure whitespace character is emitted
        let s = format!("1{}", c);
        injector.inject(&s).expect("inject whitespace with prefix");

        let events = injector.emitted_events.lock().unwrap().clone();
        assert_eq!(
            events.len(),
            4,
            "Character '{}' with digit prefix must emit exactly 4 events without Shift",
            c
        );
        assert_eq!(events[2].key, expected_key);
        assert!(events[2].is_down);
        assert_eq!(events[3].key, expected_key);
        assert!(!events[3].is_down);

        // Assert Shift is NOT emitted
        for ev in &events {
            assert_ne!(
                ev.key,
                KeyCode::KEY_LEFTSHIFT,
                "Shift must not be emitted for '{}'",
                c
            );
        }
    }
}

#[test]
fn test_typewriter_utf8_fallback_unmapped_characters() {
    use speechyyy::injector::InjectionMode;

    let mut injector =
        RecordingInjector::with_mode(InjectionMode::Typewriter { delay_ms: 0 }, false);
    injector
        .inject("hello 🚀 world")
        .expect("inject text with emoji");

    let fallback = injector.fallback_pastes.lock().unwrap().clone();
    assert_eq!(
        fallback,
        vec!["🚀".to_string()],
        "Unmapped emoji '🚀' must fall back to clipboard paste"
    );

    let events = injector.emitted_events.lock().unwrap().clone();
    // 'hello ' = 6 chars = 12 events; ' world' = 6 chars = 12 events -> total 24 key events
    assert_eq!(
        events.len(),
        24,
        "Mapped characters before and after fallback must still type normally"
    );

    // Multi-byte script fallback
    let mut script_injector =
        RecordingInjector::with_mode(InjectionMode::Typewriter { delay_ms: 0 }, false);
    script_injector
        .inject("namaste नमस्ते")
        .expect("inject text with Devanagari");

    let script_fallback = script_injector.fallback_pastes.lock().unwrap().clone();
    assert_eq!(
        script_fallback,
        vec!["नमस्ते".to_string()],
        "Devanagari script must fall back to clipboard paste intact"
    );
}

#[test]
fn test_typewriter_modifier_settle_releases_all_held_modifiers() {
    use speechyyy::injector::InjectionMode;

    let mut injector = RecordingInjector::with_mode(
        InjectionMode::Typewriter { delay_ms: 0 },
        true, // settle_modifiers enabled
    );
    injector.inject("A").expect("inject with settle");

    let events = injector.emitted_events.lock().unwrap().clone();
    assert!(
        events.len() >= 8,
        "Must emit releases for all held modifiers"
    );

    let required_modifiers = [
        KeyCode::KEY_RIGHTALT,
        KeyCode::KEY_LEFTALT,
        KeyCode::KEY_LEFTCTRL,
        KeyCode::KEY_RIGHTCTRL,
        KeyCode::KEY_LEFTSHIFT,
        KeyCode::KEY_RIGHTSHIFT,
        KeyCode::KEY_LEFTMETA,
        KeyCode::KEY_RIGHTMETA,
    ];

    for &mod_key in &required_modifiers {
        let release_pos = events
            .iter()
            .position(|ev| ev.key == mod_key && !ev.is_down);
        assert!(
            release_pos.is_some(),
            "Expected release event for modifier {:?}",
            mod_key
        );

        let char_down_pos = events
            .iter()
            .position(|ev| ev.key == KeyCode::KEY_A && ev.is_down)
            .expect("KEY_A down must be present");

        assert!(
            release_pos.unwrap() < char_down_pos,
            "Modifier {:?} release must precede character emission",
            mod_key
        );
    }
}

#[test]
#[ignore = "touches real /dev/uinput device and injects keystrokes"]
fn test_create_injector_supports_typewriter_method() {
    let mut config = Config::default();
    config.injector.method = "typewriter".into();
    match speechyyy::injector::create_injector(&config) {
        Ok(injector) => {
            assert!(
                injector.name().contains("typewriter") || injector.name().contains("uinput"),
                "Injector should be typewriter-capable uinput injector, got: {}",
                injector.name()
            );
        }
        Err(e) => {
            // Expected in unprivileged test environments where /dev/uinput is unwritable
            assert!(
                e.contains("uinput") || e.contains("Permission denied") || e.contains("writable"),
                "Expected descriptive permission error, got: {}",
                e
            );
        }
    }
}

#[test]
fn test_empty_text_skips_emission() {
    let mut injector = RecordingInjector::new(Duration::ZERO);

    assert!(is_whitespace_or_empty(""));
    assert!(is_whitespace_or_empty("   "));
    assert!(is_whitespace_or_empty("\n\t "));

    assert!(injector.inject("").is_ok());
    assert_eq!(injector.emitted_events.lock().unwrap().len(), 0);
    assert_eq!(injector.injected_strings.lock().unwrap().len(), 0);

    assert!(injector.inject("   ").is_ok());
    assert_eq!(injector.emitted_events.lock().unwrap().len(), 0);
    assert_eq!(injector.injected_strings.lock().unwrap().len(), 0);

    assert!(injector.inject("\n\t ").is_ok());
    assert_eq!(injector.emitted_events.lock().unwrap().len(), 0);
    assert_eq!(injector.injected_strings.lock().unwrap().len(), 0);
}

#[test]
fn test_non_empty_text_is_unchanged() {
    let mut injector = RecordingInjector::new(Duration::ZERO);

    assert!(!is_whitespace_or_empty("hello"));
    assert!(!is_whitespace_or_empty(" hello "));

    injector.inject("hello").unwrap();
    let events_hello = injector.emitted_events.lock().unwrap().clone();
    assert!(!events_hello.is_empty());
    // 'h', 'e', 'l', 'l', 'o' -> 5 characters, each has down and up = 10 events
    assert_eq!(events_hello.len(), 10);

    let mut injector2 = RecordingInjector::new(Duration::ZERO);
    injector2.inject(" hello ").unwrap();
    let events_spaced = injector2.emitted_events.lock().unwrap().clone();
    // space, 'h', 'e', 'l', 'l', 'o', space -> 7 characters = 14 events
    assert_eq!(events_spaced.len(), 14);
    // First event is space down, last is space up
    assert_eq!(events_spaced[0].key, KeyCode::KEY_SPACE);
    assert!(events_spaced[0].is_down);
    assert_eq!(events_spaced[13].key, KeyCode::KEY_SPACE);
    assert!(!events_spaced[13].is_down);
}

#[test]
fn test_clipboard_injector_mocks_sequence_and_empty_guard() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let writer = MockClipboardWriter::with_order(Arc::clone(&order));
    let written_ref = Arc::clone(&writer.written);
    let sink = MockKeySink::with_order(Arc::clone(&order));
    let emitted_ref = Arc::clone(&sink.emitted);

    let mut injector = WaylandClipboardInjector::with_parts(
        Box::new(writer),
        Box::new(sink),
        Duration::ZERO,
    );

    // 1. Empty / whitespace inputs do neither clipboard write nor key emission
    for empty_input in ["", "   ", "\n\t "] {
        assert!(injector.inject(empty_input).is_ok());
        assert!(written_ref.lock().unwrap().is_empty());
        assert!(emitted_ref.lock().unwrap().is_empty());
        assert!(order.lock().unwrap().is_empty());
    }

    // 2. Non-empty text writes clipboard first, then emits LEFTCTRL↓, V↓, V↑, LEFTCTRL↑
    let text = "Hello Wispr Flow";
    assert!(injector.inject(text).is_ok());

    let written = written_ref.lock().unwrap().clone();
    assert_eq!(written, vec![text.to_string()]);

    let emitted = emitted_ref.lock().unwrap().clone();
    assert_eq!(
        emitted,
        vec![
            (KeyCode::KEY_LEFTCTRL, true),
            (KeyCode::KEY_V, true),
            (KeyCode::KEY_V, false),
            (KeyCode::KEY_LEFTCTRL, false),
        ],
        "Key sequence must be exactly LEFTCTRL down, V down, V up, LEFTCTRL up"
    );

    // Verify ordering: clipboard write occurred BEFORE any key events
    let ops = order.lock().unwrap().clone();
    assert_eq!(ops.len(), 5);
    assert_eq!(ops[0], format!("clipboard:{}", text));
    assert_eq!(ops[1], "key:KEY_LEFTCTRL:down");
    assert_eq!(ops[2], "key:KEY_V:down");
    assert_eq!(ops[3], "key:KEY_V:up");
    assert_eq!(ops[4], "key:KEY_LEFTCTRL:up");
}

#[test]
fn test_clipboard_write_failure_emits_no_keys_and_does_not_panic() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let writer = MockClipboardWriter::failing();
    let sink = MockKeySink::with_order(Arc::clone(&order));
    let emitted_ref = Arc::clone(&sink.emitted);

    let mut injector = WaylandClipboardInjector::with_parts(
        Box::new(writer),
        Box::new(sink),
        Duration::ZERO,
    );

    let result = injector.inject("Failed paste");
    assert!(result.is_err(), "Must return Err on clipboard write failure");
    assert!(
        emitted_ref.lock().unwrap().is_empty(),
        "Must emit zero keys if clipboard writing fails"
    );
    assert!(
        order.lock().unwrap().is_empty(),
        "No keys should be recorded in order"
    );
}

#[test]
fn test_clipboard_injector_paste_hold_delay_timing() {
    use std::time::Instant;

    let order = Arc::new(Mutex::new(Vec::new()));
    let writer = MockClipboardWriter::with_order(Arc::clone(&order));
    let sink = MockKeySink::with_order(Arc::clone(&order));

    let mut injector = WaylandClipboardInjector::with_parts(
        Box::new(writer),
        Box::new(sink),
        Duration::ZERO,
    );

    let start = Instant::now();
    assert!(injector.inject("Test hold duration").is_ok());
    let elapsed = start.elapsed();

    assert!(
        elapsed >= WaylandClipboardInjector::PASTE_HOLD_DELAY,
        "inject() must hold the Ctrl+V chord for at least PASTE_HOLD_DELAY (20ms), took {:?}",
        elapsed
    );
}
