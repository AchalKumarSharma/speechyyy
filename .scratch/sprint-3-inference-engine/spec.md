# Sprint 3: Streaming & Cloud Inference Engine

## Problem Statement

Sprint 2 built an allocation-free audio pipeline with CPAL capture, ring buffering, anti-aliased resampling, and energy VAD. When the user releases the push-to-talk hotkey, the system currently emits a temporary diagnostic tag (`[Speech: 0.50s, 16kHz, RMS 0.050]`).
In Sprint 3, Speechyyy must deliver true speech-to-text dictation by dispatching the 16kHz single-channel PCM `AudioChunk` to an inference provider, transcribing spoken speech, and streaming the transcribed text directly to the focused window caret via `/dev/uinput`.

## Solution

1. Define an abstract `SpeechInference` trait returning `Result<String, InferenceError>` from an `AudioChunk`.
2. Connect `SpeechInference` to `AudioDictationEngine` so that when `RightAlt` is released and audio passes VAD, the transcribed words are typed directly at the active caret.
3. Build a multi-provider inference seam:
   - **`MockSpeechInference`**: Fast, deterministic offline provider for automated testing and CI.
   - **`SarvamSpeechInference`**: Production cloud provider for Sarvam AI (`https://api.sarvam.ai/speech-to-text`) using the `saaras:v4` model supporting 22+ Indic languages and English (including Hinglish/codemix).
   - **`LocalSpeechInference`**: Extensible offline stub prepared for local quantized ONNX / Sherpa runtimes.
4. Implement an in-memory 16-bit 16kHz mono WAV container encoder with zero external dependencies in `src/inference/wav.rs`.
5. Add `[inference]` configuration schema to `~/.config/speechyyy/config.toml` supporting environment variable fallback (`SARVAM_API_KEY`).
6. Add inference diagnostics to `speechyyy doctor` to inspect API keys.
7. Maintain project constraints: strictly < 10 MB binary size, < 30 MB idle RAM, 100% test pass rate.

## User Stories

1. As a user, I want words I speak while holding push-to-talk to be transcribed into text and typed into my active application caret.
2. As a user, I want to use Sarvam AI's `saaras:v4` speech-to-text model by providing my `SARVAM_API_KEY`.
3. As a user, I want language auto-detection (`language_code = "unknown"`) and support for transcription modes (`transcribe`, `codemix`, `translate`).
4. As a developer, I want an abstract `SpeechInference` trait with a `MockSpeechInference` provider so tests can run offline and deterministically in CI.
5. As a user, I want empty transcripts to return cleanly without typing ghost characters at my caret.
6. As a user, I want inference errors to be logged to stderr without dumping error text into my document or editor.
7. As a user, I want `speechyyy doctor` to verify my inference configuration and API key status.

## Implementation Decisions

- **HTTP Client**: Use `ureq` v2 with `tls` and `json` features (blocking, lightweight, minimal footprint, keeps binary < 2 MB).
- **In-Memory WAV Packaging**: Construct exact 44-byte RIFF/WAVE header and 16-bit signed PCM payload in `src/inference/wav.rs`.
- **Multipart Form-Data Serialization**: Implement RFC 7578 multipart body builder with dynamic boundary in `src/inference/sarvam.rs`.
- **Seam**: `SpeechInference` trait:
  ```rust
  pub trait SpeechInference: Send {
      fn transcribe(&mut self, chunk: &AudioChunk) -> Result<String, InferenceError>;
      fn provider_name(&self) -> &'static str;
  }
  ```
- **Dictation Engine Wiring**: `AudioDictationEngine<C: AudioCapture, I: SpeechInference>` in `src/engine.rs`.
- **Pre-flight Check**: `speechyyy doctor` checks `SARVAM_API_KEY` when `provider = "sarvam"`.

