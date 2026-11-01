# 02: Typewriter Keystroke Injection Mode

**What to build:**
Provide a character-by-character "typewriter" injection mode that types out transcribed text sequentially into the active window via `/dev/uinput` with configurable pacing delays (e.g. 10–20ms), creating a smooth visual typing effect. Uppercase letters and symbols are typed using virtual Shift keychords, held modifiers (like `RightAlt`) are settled before typing begins, and unmapped Unicode characters fall back cleanly to clipboard paste so that all characters reach the screen intact.

**Blocked by:** None (can start immediately)

**Status:** resolved

## Acceptance Criteria

- [x] Define `InjectionMode` enum in `src/injector.rs`:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(rename_all = "lowercase")]
  pub enum InjectionMode {
      Typewriter { delay_ms: u64 },
      Paste,
  }
  ```
- [x] Implement `Typewriter` mode in `UinputInjector`:
  - Settles held hotkey modifiers (`RightAlt`, `LeftAlt`, `Ctrl`, `Meta`) with explicit key-up and 50ms settle delay before character typing begins.
  - Iterates over transcript characters, resolving evdev keycode and shift requirement via `char_to_key(c)`.
  - For shifted characters (uppercase `A-Z`, punctuation symbols like `!`, `@`, `#`, `$`, `%`, `^`, `&`, `*`, `(`, `)`, `"`, `?`, `:`, `{`, `}`, `|`, `~`, `<`, `>`):
    * Emits `KEY_LEFTSHIFT` down -> pacing sleep -> key down -> pacing sleep -> key up -> pacing sleep -> `KEY_LEFTSHIFT` up -> pacing sleep.
  - For unshifted characters (lowercase `a-z`, digits `0-9`, space, enter, tab, `.`, `,`, `-`, `=`, `;`, `/`, `'`, `[`, `]`, `\`, `` ` ``):
    * Emits key down -> pacing sleep -> key up -> pacing sleep.
  - For unmapped Unicode characters (e.g. emojis or extended scripts): falls back to clipboard paste (`wl-copy` + uinput paste chord) for the unmapped segment so that no characters are lost.
- [x] Retain `InjectionMode::Paste` using `WaylandClipboardInjector` (80ms settle delay + `Ctrl+V` uinput chord with 20ms hold).
- [x] Update `RecordingInjector` to capture the exact sequence of `KeyStroke` events and pacing durations.
- [x] Unit tests verify Shift chords for uppercase and punctuation, inter-key pacing delays, modifier settlement, and clipboard fallback.

## Testing Seams

- **Seam:** `TextInjector::inject(&mut self, text: &str) -> Result<(), String>`.
- **Method:** Exercised via `RecordingInjector` and `UinputInjector` headless unit tests to assert that key down/up events, shift state transitions, pacing delays, and fallback paths fire in the correct sequence.
