# Issue 05: AudioDictationEngine Wiring to SpeechInference

Status: resolved
Blocked by: 01, 02, 03, 04
Type: task

## Description

Wire `SpeechInference` into `AudioDictationEngine` so that releasing the push-to-talk hotkey after speaking transcribes the audio and types the actual words directly at the active caret.

This replaces the temporary Sprint 2 telemetry tag (`[Speech: 0.50s, 16kHz, RMS 0.050]`) with real transcribed text.

## Tasks

- [x] Update `src/engine.rs`:
  - `AudioDictationEngine<C: AudioCapture, I: SpeechInference>`:
    * Holds `capture: C`, `vad: EnergyVad`, and `inference: I`.
    * `on_press(&mut self) -> Result<(), String>` starts capture.
    * `on_release(&mut self) -> Result<Option<String>, String>`:
      - Stops capture and drains 16kHz `AudioChunk`.
      - Evaluates VAD:
        * If fails (silence / micro-tap): logs diagnostic to stderr, returns `Ok(None)`.
        * If passes: calls `self.inference.transcribe(&chunk)`.
          - If transcript is non-empty: returns `Ok(Some(transcript))`.
          - If transcript is empty: logs to stderr, returns `Ok(None)`.
          - If inference returns error: logs error to stderr, returns `Ok(None)` (does not inject error text into user's window).
- [x] Update `src/daemon.rs`:
  - Factory pattern creating the configured `SpeechInference` provider based on `config.inference.provider`:
    * `"mock"` -> `MockSpeechInference`.
    * `"sarvam"` -> `SarvamSpeechInference::from_config(&config.inference)`.
    * `"local"` -> `LocalSpeechInference`.
  - Pass the provider into `AudioDictationEngine`.
- [x] Unit tests in `src/engine.rs`:
  - Test valid speech with `MockSpeechInference` returns transcribed text.
  - Test empty transcript returns `None`.
  - Test inference error returns `None` without crashing.

## Seams

- Seam: `AudioDictationEngine::on_release()`.
  * Catches: Drop in state machine emission, uncaught errors during transcription, empty text injection.
  * Misses: Operating system keyboard buffer overflow.

## Comments
Resolved and verified. AudioDictationEngine is generic over SpeechInference, properly delegates transcription to the inference provider, discards empty transcripts, logs errors to stderr without caret injection, and is integrated with factory dispatch in src/daemon.rs. All unit and integration tests passing.
