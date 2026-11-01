# 05: End-to-End Integration, Constraints, and Release Verification

**What to build:**
Verify the complete Sprint 5 system end-to-end: push-to-talk hotkey press -> microphone audio capture -> VAD speech evaluation -> Sarvam Saaras v4 transliteration -> text formatting & hallucination cleaning -> dictionary replacement -> typewriter keystroke injection into the focused application window. Ensure all project constraints are strictly upheld: release binary < 10 MB, idle RAM < 30 MB, 100% test pass rate, 0 Clippy warnings, and documentation updated.

**Blocked by:** 01 (In-Memory WAV & Sarvam STT), 02 (Typewriter Keystroke Injector), 03 (Configuration & CLI Flags), 04 (Doctor Diagnostics & Error Handling)

**Status:** resolved

## Acceptance Criteria

- [x] End-to-end integration test in `tests/inference_pipeline_test.rs`:
  - Simulates full Push-to-Talk cycle using Sarvam AI provider in `mode = "translit"`.
  - Asserts that Hinglish transliteration output (e.g. *"mera phone number hai"*) flows through text formatting, dictionary substitutions, and into `RecordingInjector`.
  - Confirms history logger records the utterance in `history.jsonl` with `provider = "sarvam"`.
- [x] Typewriter keystroke tests in `tests/injector_test.rs`:
  - Verifies that `InjectionMode::Typewriter { delay_ms: 15 }` emits key events with 15ms spacing.
  - Verifies that uppercase characters and punctuation marks emit correct virtual Shift keychords.
  - Verifies that unmapped characters fall back cleanly to clipboard paste.
- [x] Strict project constraints verified:
  - `cargo test` passing 100% across all unit and integration tests (149 passed).
  - `cargo clippy --all-targets` passing with 0 warnings.
  - `cargo fmt -- --check` passing with clean formatting.
  - Stripped release binary size verified strictly < 10 MB (4.9 MB actual).
  - Idle RAM consumption verified < 30 MB (13.08 MB peak RSS).
- [x] Architectural documentation:
  - Record ADR 0013 (`docs/adr/0013-sarvam-ai-provider-and-typewriter-injection.md`) documenting synchronous `ureq` client choice, in-memory WAV serialization, and typewriter injection ergonomics.
  - Update `GLOSSARY.md` with definitions for `Transliteration Mode`, `Typewriter Mode`, and `In-Memory WAV`.

## Testing Seams

- **Seam:** Full Push-to-Talk session loop (`run_session_loop`) combining `MockAudioCapture`, `FakeInputSource`, `AudioDictationEngine`, and `RecordingInjector`.
- **Method:** Integration test verifying that holding and releasing the hotkey drives the complete end-to-end path from speech capture through cloud transliteration to typewriter keystroke emission.
