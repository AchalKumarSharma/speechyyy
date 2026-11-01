# Sarvam AI Speech-to-Text Provider and Typewriter Keystroke Injection

Speechyyy must support multilingual dictation—particularly code-mixed Indian English and conversational Hinglish—with Romanized text delivery, while offering natural, human-paced text injection across terminal emulators, code editors, and GUI applications. Existing local offline models (Moonshine-Tiny, Whisper-Tiny) struggle with Hinglish acoustic nuances and vocabulary, requiring a specialized cloud provider. Furthermore, instant clipboard paste (`Ctrl+V`) can feel abrupt and causes occasional input collisions in certain applications, necessitating a typewriter-style keystroke injector that renders text character-by-character with realistic pacing.

We decided to implement:
1. **Pluggable Sarvam AI Speech-to-Text Provider**:
   - Implemented `SarvamSpeechInference` under the unified `SpeechInference` trait targeting Sarvam AI's REST endpoint (`POST https://api.sarvam.ai/speech-to-text`) with model `saaras:v4`.
   - Set the default mode to `"translit"` (Romanized Hinglish transliteration), while exposing `"transcribe"`, `"translate"`, and `"codemix"` via `config.toml` and CLI `--mode`.
   - Selected synchronous, minimalist HTTP client `ureq` to execute RFC 7578 multipart/form-data requests without introducing heavy `tokio` or async runtime dependencies, keeping the release binary strictly under 10 MB (4.9 MB actual) and idle RAM under 30 MB (~13 MB actual).
2. **In-Memory RIFF/WAVE Audio Serialization**:
   - Built canonical 44-byte RIFF/WAVE header and 16-bit little-endian PCM sample encoder in `src/audio/wav.rs`.
   - Serialized 16kHz mono audio directly into RAM byte buffers, preventing temporary file creation, disk wear, and potential audio leaks in `/tmp`.
3. **Typewriter Keystroke Injection**:
   - Implemented `InjectionMode::Typewriter { delay_ms: u64 }` with a default inter-key pacing delay of 15ms in `src/injector.rs`.
   - Added comprehensive modifier defusing to settle all 8 hardware modifiers (`LeftAlt`, `RightAlt`, `LeftCtrl`, `RightCtrl`, `LeftShift`, `RightShift`, `LeftMeta`, `RightMeta`) before keystrokes begin, preventing push-to-talk key releases from forming accidental window shortcuts.
   - Synthesized accurate virtual `KEY_LEFTSHIFT` keychords for uppercase characters (`A`..`Z`) and shifted punctuation (`!`, `@`, `#`, `"`, etc.), while transparently falling back to clipboard paste for unmapped Unicode glyphs.
4. **Resilient Doctor Diagnostics and Precedence Overrides**:
   - Enhanced `speechyyy doctor` to inspect Sarvam API keys (with masked formatting) and test `/dev/uinput` write permissions, reporting non-fatal `[WARN]` fallback when running under unprivileged Wayland sessions.
   - Enforced strict configuration precedence: CLI flags (`speechyyy start --provider --mode --injection --typing-delay`) > Environment variables (`SARVAM_API_KEY`) > `config.toml` > Built-in defaults.

## Considered Options

- **Async Tokio / Reqwest Client**: Rejected because pulling in Tokio, Reqwest, and hyper adds over 3–5 MB of binary overhead and increases memory footprint, violating the hard < 10 MB binary and < 30 MB idle RAM constraints. Synchronous `ureq` with OS socket timeouts satisfies all requirements with minimal footprint.
- **Disk-Backed Temp WAV Files (`/tmp/audio.wav`)**: Rejected for security, privacy, and performance reasons. Writing audio to `/tmp` leaves persistent audio remnants on disk and incurs unnecessary filesystem I/O latency.
- **Pure Clipboard-Only Injection**: Rejected because pasting triggers clipboard manager history pollution and lacks the visual streaming feedback that typewriter typing provides.
- **Dropping Unmapped Unicode in Typewriter Mode**: Rejected because multilingual dictation often contains non-ASCII characters or emojis. Segmenting text into ASCII evdev keystrokes and falling back to clipboard paste for unmapped glyphs ensures zero dropped characters.

