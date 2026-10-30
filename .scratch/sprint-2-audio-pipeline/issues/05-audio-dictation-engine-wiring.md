# Issue 05: Audio Dictation Engine Wiring to Push-to-Talk Loop

Status: resolved
Blocked by: 01, 02, 03, 04
Type: task

## Description

Sprint 1 wired key press and release to a mock dictation engine that immediately typed a fixed string `"Hello from Speechyyy!"`.

In Sprint 2, the push-to-talk event loop must be wired to real audio capture:
1. When the hotkey is pressed (`on_press`):
   - Transition state from `Idle` to `Recording`.
   - Start audio capture via `AudioCaptureService::start_capture()`.
2. When the hotkey is released (`on_release`):
   - Transition state from `Recording` to `Processing`.
   - Call `AudioCaptureService::stop_capture()` to retrieve the raw buffered 16kHz audio chunk.
   - Run VAD evaluation (`EnergyVad::evaluate(&chunk)`).
   - If VAD fails (silence or < 200ms tap): discard audio chunk, log diagnostic, transition directly back to `Idle` (no text typed).
   - If VAD passes: format summary string `[Speech: {:.2}s, 16kHz, RMS {:.3}]`, transition to `Emitting`, and send to text injector for typing.

## Tasks

- [x] Update `src/engine.rs`:
  - Refactor `DictationEngine` trait to handle state machine transitions with real audio.
  - Implement `AudioDictationEngine<C: AudioCaptureService>`:
    * Holds capture service, VAD evaluator, and current engine state (`Idle`, `Recording`, `Processing`, `Emitting`).
    * `on_press(&mut self) -> Result<(), EngineError>` starts capture stream.
    * `on_release(&mut self) -> Result<Option<String>, EngineError>` stops capture, applies VAD, returns summary string or None.
- [x] Update `src/daemon.rs`:
  - Instantiate `AudioDictationEngine` with `CpalAudioCaptureService` (or `MockAudioCaptureService` in tests).
  - Inject speech summary tag into active window caret via `UinputKeyInjector` when `on_release` returns `Some(text)`.
- [x] Integration tests in `tests/audio_pipeline_test.rs`:
  - Simulate hotkey press -> feed mock speech audio -> simulate hotkey release -> verify injector receives expected summary text.
  - Simulate hotkey press -> feed silence -> release -> verify injector receives nothing.
  - Simulate hotkey tap (< 200ms) -> release -> verify injector receives nothing.

## Seams

- Seam: `AudioDictationEngine::on_press` / `on_release`.
  * Catches: Race conditions between hotkey release and buffer draining, state transition drops, improper VAD filtering.
  * Misses: Evdev hardware driver timing quirks.

## Answer

Implemented `AudioDictationEngine<C: AudioCaptureService>` in `src/engine.rs` wired to `StateMachineController`.
Wired `start_daemon` in `src/daemon.rs` to initialize `CpalAudioCaptureService` and `EnergyVad`.
Added 3 integration tests in `tests/audio_pipeline_test.rs` validating that speech emits the summary tag `[Speech: 0.50s, 16kHz, RMS 0.050]`, while silence and micro-taps are cleanly discarded without emitting any keystrokes.

## Comments
