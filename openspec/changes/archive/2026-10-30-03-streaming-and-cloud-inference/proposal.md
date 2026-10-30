# Proposal: Streaming & Cloud Inference Engine (Sprint 3)

## Why

Sprint 2 built an allocation-free audio pipeline with CPAL capture, ring buffering, anti-aliased resampling, and energy VAD. When the user releases the push-to-talk hotkey, the system emitted a temporary diagnostic tag (`[Speech: 0.50s, 16kHz, RMS 0.050]`).
In Sprint 3, Speechyyy delivers true speech-to-text dictation by dispatching the 16kHz single-channel PCM `AudioChunk` to an inference provider, transcribing spoken speech, and typing the transcribed text directly into the focused window caret via `/dev/uinput`.

## What Changes

- Abstract `SpeechInference` trait returning `Result<String, InferenceError>` from an `AudioChunk`.
- Zero-dependency in-memory 16-bit 16kHz mono WAV container encoder (`src/inference/wav.rs`).
- Deterministic offline `MockSpeechInference` provider for unit and integration testing.
- Production `SarvamSpeechInference` cloud provider targeting Sarvam AI's REST API (`saaras:v4` model with auto language detection).
- Offline `LocalSpeechInference` provider stub prepared for local quantized ONNX / Sherpa runtimes.
- `AudioDictationEngine` wired to `SpeechInference`, typing real words into the active cursor position on hotkey release.
- Graceful handling of empty transcripts and error isolation to stderr (preventing ghost keystrokes).
- `[inference]` configuration block in `~/.config/speechyyy/config.toml` with `SARVAM_API_KEY` environment variable fallback.
- Diagnostics in `speechyyy doctor` checking inference provider readiness and API key presence.
