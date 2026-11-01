# Sprint 5: Pluggable Sarvam AI Speech-to-Text Provider & Typewriter Keystroke Injection

## Problem Statement

Users of Speechyyy on Linux desktop environments face two key limitations:
1. **Language & Transliteration Gap**: While local Moonshine-Tiny and Whisper-Tiny models perform well on standard English, they struggle with bilingual conversational speech, Indian English, and Indic languages. Specifically, developers and multilingual professionals frequently speak in Hinglish (code-mixed Hindi and English) and require Romanized text output (e.g. speaking *"mera phone number note kar lo"* and expecting *"mera phone number note kar lo"*, not Devanagari or garbled phonetics). Local models lack the acoustic vocabulary for this, necessitating a cloud provider like Sarvam AI (Saaras v4) with native support for `translit`, `transcribe`, `translate`, and `codemix`.
2. **Abrupt Text Delivery vs. Natural Feedback**: Existing text injection either relies on clipboard pasting (`Ctrl+V`), which can cause race conditions or feel unnatural and jarring in certain text editors, or basic virtual typing without configurable pacing. Users desire a "Typewriter" injection mode that types out words character-by-character with realistic pacing (e.g. 10–20ms delay), creating a visible streaming typewriter effect while preserving full support for symbols, punctuation, and uppercase characters.

## Solution

Speechyyy will introduce:
1. **Pluggable Sarvam AI Speech-to-Text Provider**:
   - Implements the unified `SpeechInference` trait alongside `LocalSpeechInference`.
   - Targets Sarvam's `POST https://api.sarvam.ai/speech-to-text` endpoint with model `saaras:v4`.
   - Defaults to `mode = "translit"` for automatic Romanized Hinglish transliteration, while allowing configurable `transcribe`, `translate`, and `codemix` modes.
   - Emits in-memory 16kHz mono WAV bytes directly via `src/audio/wav.rs` without disk I/O.
   - Uses lightweight synchronous `ureq` HTTP client to keep binary size strictly under 10 MB (avoiding heavy async runtime bloat).
2. **Typewriter Keystroke Injection**:
   - Introduces `InjectionMode::Typewriter { delay_ms: u64 }` alongside `InjectionMode::Paste`.
   - Emits sequential `/dev/uinput` evdev keystrokes with configurable inter-key pacing delays to produce a smooth, human-like typewriter visual effect.
   - Handles uppercase characters and punctuation through virtual `KEY_LEFTSHIFT` chords, with clean UTF-8 fallback to clipboard paste for unmapped Unicode characters.
3. **Ergonomic CLI and Configuration**:
   - Configuration options in `config.toml` and environment variables (`SARVAM_API_KEY`, `sarvam_mode`, `injection_mode`, `typing_delay_ms`).
   - CLI flags on `speechyyy start`: `--provider <local|sarvam>`, `--mode <translit|transcribe|translate|codemix>`, `--injection <typewriter|paste>`, `--typing-delay <ms>`.
   - Enhanced `speechyyy doctor` diagnostics for cloud credentials and uinput device permissions.

## User Stories

1. As a bilingual developer speaking Hinglish, I want my spoken Hindi phrases to be transliterated into Romanized script ("translit" mode), so that I can dictate commit messages, code comments, and chat messages without manually switching keyboards.
2. As a user, I want Sarvam AI to be selectable as the active speech inference provider via `config.toml` or `--provider sarvam`, so that I can choose high-accuracy cloud transcription when connected to the internet.
3. As a user, I want Moonshine-Tiny to remain available as the local inference provider via `--provider local`, so that I can work completely offline when internet access is unavailable.
4. As a developer, I want to configure the Sarvam operation mode (`translit`, `transcribe`, `translate`, `codemix`) via `config.toml` and CLI `--mode`, so that I can tailor the transcription to my target language and format.
5. As a user, I want `speechyyy start` to accept a `--provider` flag that overrides the default provider in `config.toml`, so that I can switch providers on the fly for specific sessions.
6. As a user, I want `speechyyy start` to accept an `--injection` flag (`typewriter` or `paste`), so that I can toggle between smooth sequential typing and instant clipboard pasting.
7. As a user, I want the typewriter injection mode to support a configurable pacing delay (`--typing-delay <ms>` or `typing_delay_ms = 15`), so that the typing speed feels natural and responsive.
8. As a writer, I want capital letters and punctuation marks (e.g. `!`, `@`, `#`, `"`, `?`) to be accurately typed with virtual Shift chords in typewriter mode, so that formatted speech requires no manual cleanup.
9. As a user dictating special characters or emojis not mapped in ASCII evdev tables, I want typewriter mode to cleanly fall back to clipboard paste for unmapped characters, so that no characters are silently dropped.
10. As a user without an API key configured in `config.toml`, I want Speechyyy to check the `SARVAM_API_KEY` environment variable, so that I can keep secrets out of persistent dotfiles.
11. As a user running `speechyyy doctor`, I want it to check my Sarvam API key configuration and report its presence and masked value, so that I can verify cloud readiness before starting dictation.
12. As a user experiencing a network disconnection while using Sarvam AI, I want Speechyyy to report a structured network error and log a friendly failure message rather than crashing the background daemon.
13. As a user receiving an HTTP 401 Unauthorized or HTTP 429 Rate Limited response from Sarvam AI, I want clear diagnostic error output in `speechyyy doctor` and daemon logs, so that I know whether to replenish credits or fix my key.
14. As a user, I want audio chunks to be converted to WAV format purely in RAM, so that no temporary audio files are written to `/tmp` or disk during cloud inference.
15. As a privacy-conscious user, I want all transcription history to remain logged locally in `~/.local/share/speechyyy/history.jsonl` regardless of whether the provider is local or Sarvam AI, so that I have a persistent offline audit trail.
16. As a user with custom technical terms in `dictionary.toml`, I want my learning dictionary substitution rules to apply seamlessly to Sarvam AI transcripts before typewriter injection, so that learned words (e.g. "PostgreSQL") are typed correctly.
17. As an embedded Linux user, I want the compiled binary with Sarvam AI support and typewriter injection to stay strictly under 10 MB, so that Speechyyy remains ultra-lightweight.
18. As a user running the daemon in the background, I want idle RAM consumption to stay under 30 MB even with the Sarvam HTTP client loaded, so that Speechyyy has zero perceptible system impact.
19. As a user in a Wayland session, I want typewriter mode to emit synthetic hardware uinput key events that work seamlessly across terminal emulators, native GUI apps, and Electron webviews, so that I can dictate anywhere.
20. As a user, I want hotkey release modifiers (e.g. `RightAlt`) to be settled before typewriter keystrokes begin, so that the first letter is never combined with Alt into an accidental application shortcut.

## Implementation Decisions

1. **Synchronous HTTP Client with `ureq`**:
   We will continue using `ureq` (with TLS and JSON features) rather than introducing `tokio` and `reqwest`. A synchronous HTTP call on the worker thread satisfies Speechyyy's execution model, introduces zero async runtime overhead, and keeps the release binary well under 5 MB.
2. **Unified `SpeechInference` Trait**:
   Maintain the synchronous trait interface in `src/inference/mod.rs`:
   ```rust
   pub trait SpeechInference: Send {
       fn transcribe(&mut self, chunk: &AudioChunk) -> Result<String, InferenceError>;
       fn provider_name(&self) -> &'static str;
   }
   ```
   Both `LocalSpeechInference` and `SarvamSpeechInference` implement this trait cleanly.
3. **In-Memory WAV Serialization (`src/audio/wav.rs`)**:
   Relocate and standardize canonical RIFF/WAVE 16kHz mono 16-bit PCM serialization into `src/audio/wav.rs` (`encode_wav_16k_mono`), re-exported by `src/inference/wav.rs` for backwards compatibility. Audio samples are serialized entirely in memory into a 44-byte header buffer followed by clamped i16 PCM samples.
4. **Sarvam AI STT Client (`src/inference/sarvam.rs`)**:
   - Endpoint: `https://api.sarvam.ai/speech-to-text`.
   - Headers: `api-subscription-key: <key>`, `Content-Type: multipart/form-data; boundary=<boundary>`.
   - Multipart fields:
     - `file`: in-memory WAV payload named `audio.wav`.
     - `model`: default `"saaras:v4"`.
     - `mode`: default `"translit"`; supported values: `"translit"`, `"transcribe"`, `"translate"`, `"codemix"`.
     - `language_code`: default `"unknown"`, omitted or set to specific code (e.g. `"hi-IN"`).
   - Response deserialization: parses `{ "transcript": "...", "language_code": "..." }`.
5. **Typewriter Keystroke Injection Mode (`src/injector.rs`)**:
   - Define `InjectionMode`:
     ```rust
     #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
     #[serde(rename_all = "lowercase")]
     pub enum InjectionMode {
         Typewriter { delay_ms: u64 },
         Paste,
     }
     ```
   - In `Typewriter` mode, `UinputInjector`:
     * Settles held hotkey modifiers first (50ms settle delay).
     * For each character in the formatted transcript, resolves `char_to_key(c)`:
       - If shifted: emits `KEY_LEFTSHIFT` down -> sleep -> key down -> sleep -> key up -> sleep -> `KEY_LEFTSHIFT` up -> sleep.
       - If unshifted: emits key down -> sleep -> key up -> sleep.
     * If a character is unmapped in evdev ASCII table: emits the remaining text or character via clipboard fallback so no text is lost.
   - In `Paste` mode, `WaylandClipboardInjector` performs `wl-copy` followed by an 80ms settle delay and a hardware `Ctrl+V` chord with 20ms hold.
6. **Configuration & CLI Architecture (`src/config.rs`, `src/cli.rs`)**:
   - Add fields to `InferenceConfig`:
     - `mode: String` (defaults to `"translit"`).
   - Add fields to `InjectorConfig`:
     - `mode: InjectionMode` (defaults to `InjectionMode::Typewriter { delay_ms: 15 }`).
     - `pacing_delay_ms: u64` (defaults to `15`).
   - Add CLI options to `speechyyy start`:
     - `--provider <local|sarvam>`
     - `--mode <translit|transcribe|translate|codemix>`
     - `--injection <typewriter|paste>`
     - `--typing-delay <ms>`
7. **Diagnostics (`src/doctor.rs`)**:
   - Check `SARVAM_API_KEY` when `provider = "sarvam"`:
     * Masked key display (e.g. `sk_...1234`).
     * Optional lightweight TLS handshake check to `api.sarvam.ai`.
   - Check `/dev/uinput` write permissions for typewriter mode.

## Testing Decisions

1. **Seam 1: Inference Provider Seam (`src/inference/sarvam.rs`)**:
   - Tested against an isolated mock HTTP server with custom endpoint override (`with_endpoint(...)`).
   - Asserts RFC 7578 multipart body formatting, `mode = "translit"` parameter, `api-subscription-key` header, and error handling for HTTP 400, 401, 429, 500, and transport network drops.
2. **Seam 2: Typewriter Injection Seam (`src/injector.rs`)**:
   - Tested using `RecordingInjector` asserting the exact sequence of emitted key events:
     * Uppercase character triggers `KEY_LEFTSHIFT` down -> key down -> key up -> shift up.
     * Punctuation marks trigger correct key codes.
     * Pacing delay duration is enforced between keystroke events.
3. **Seam 3: End-to-End Pipeline Seam (`tests/inference_pipeline_test.rs`)**:
   - Push-to-Talk cycle with mock audio -> Sarvam provider -> text formatting -> dictionary replacement -> typewriter injection.
   - Validates that transliterated output flows cleanly into the injector.
4. **Seam 4: CLI Parsing Tests (`src/cli.rs`)**:
   - Unit tests verifying that `--provider`, `--mode`, `--injection`, and `--typing-delay` parse correctly and override configuration defaults.

## Out of Scope

- Real-time streaming WebSocket ASR (Sprint 5 focuses on REST multipart inference with Saaras v4).
- Direct audio recording into compressed MP3/FLAC formats (16kHz PCM WAV is standard and zero-overhead).
- macOS and Windows injection drivers (Linux `/dev/uinput` and `wl-copy` priority).
- Graphical configuration utility.

## Further Notes

- The default mode for Sarvam AI is intentionally set to `"translit"` rather than `"transcribe"` because bilingual Indian users expect English alphabet output (Romanized Hinglish) for code, terminals, and chat, avoiding unintended Devanagari characters in source code.
- In-memory WAV generation eliminates disk wear on SSDs and avoids file cleanup issues.
- The binary size remains strictly under 10 MB (currently 4.9 MB).

