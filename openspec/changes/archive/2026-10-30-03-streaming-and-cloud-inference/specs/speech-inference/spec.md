# Speech Inference Engine

## Purpose & Overview

Speechyyy converts spoken speech captured during global Push-to-Talk sessions into transcribed text and types it directly into the focused window at the active OS caret. The Speech Inference Engine abstracts downstream speech recognition providers, encodes 16,000 Hz single-channel PCM audio into a standard in-memory WAV container, communicates with cloud speech services via multipart/form-data HTTP or local runtime backends, and handles responses and errors safely without polluting the user's cursor or documents.

## Architecture & Data Flow

```
                      +---------------------------------------+
                      | Audio Pipeline (16kHz AudioChunk PCM) |
                      +---------------------------------------+
                                          |
                              Evaluated & Passed VAD
                                          v
                      +---------------------------------------+
                      |         AudioDictationEngine          |
                      +---------------------------------------+
                                          |
                                   transcribe(&chunk)
                                          v
                      +---------------------------------------+
                      |        SpeechInference Trait          |
                      +---------------------------------------+
                           /              |              \
           "mock" provider/    "sarvam" provider \         \ "local" provider
                         v                v               v
            +----------------+   +-------------------+  +------------------+
            |  Mock Provider |   |  Sarvam AI Cloud  |  |  Local Provider  |
            | (Deterministic |   | (saaras:v4 REST)  |  | (Sherpa-ONNX stub|
            |  Offline Test) |   | (22+ Indic/Eng)   |  |   Sprint 4)      |
            +----------------+   +-------------------+  +------------------+
                                          |
                               44-byte WAV + Multipart
                                          |
                                    HTTP POST (ureq)
                                          v
                                 Transcript String
                                          v
                      +---------------------------------------+
                      |    Non-empty Caret Injection (uinput) |
                      |    (Empty or error cleanly discarded) |
                      +---------------------------------------+
```

## Requirements & Invariants

### 1. Abstract Speech Inference Trait
- Defined in `src/inference/mod.rs`:
  ```rust
  pub trait SpeechInference: Send {
      fn transcribe(&mut self, chunk: &AudioChunk) -> Result<String, InferenceError>;
      fn provider_name(&self) -> &'static str;
  }
  ```
- Implemented for `Box<dyn SpeechInference>` to allow dynamic provider dispatch based on configuration.
- Standardized error handling via `InferenceError` (`MissingApiKey`, `Network`, `Http`, `Serialization`, `NotConfigured`).

### 2. Zero-Dependency WAV Encoding
- Exact 44-byte standard RIFF/WAVE header construction in `src/inference/wav.rs`.
- Encodes 16,000 Hz, 1 channel (mono), 16 bits per sample little-endian signed PCM.
- Quantizes `[-1.0, 1.0]` floating-point samples to `[-32767, 32767]` `i16` integers with hard clipping.

### 3. Cloud Provider (Sarvam AI)
- Target REST API: `POST https://api.sarvam.ai/speech-to-text`.
- Authentication: `api-subscription-key` HTTP header.
- Multipart form fields:
  * `file`: in-memory WAV container named `audio.wav`.
  * `model`: `saaras:v4` (default).
  * `mode`: `transcribe` (default), `codemix`, or `translate`.
  * `language_code`: omitted when set to `"unknown"` (triggering automatic server-side language detection).
- Low-footprint synchronous HTTP client using `ureq` v2 with `tls` and `json`.

### 4. Mock & Local Inference Providers
- `MockSpeechInference` providing configurable default or queued transcripts for hermetic offline test execution in headless CI environments.
- `LocalSpeechInference` stub prepared for local quantized ONNX and Sherpa runtimes planned for Sprint 4.

### 5. Dictation Engine Wiring & Caret Injection
- `AudioDictationEngine<C: AudioCapture, I: SpeechInference>` coordinates audio capture cessation, VAD evaluation, speech inference transcription, and caret emission.
- Valid non-empty transcripts are emitted into the OS typing pipeline via `/dev/uinput`.
- Empty or whitespace-only transcripts are dropped without injecting ghost characters.
- Upstream inference errors (network, auth, HTTP) are logged to stderr only; no error strings are typed into the user's active window.

### 6. Diagnostics & Configuration
- `[inference]` section in `~/.config/speechyyy/config.toml`:
  * `provider`: `"mock"` | `"sarvam"` | `"local"`.
  * `api_key`: string (optional, falls back to `SARVAM_API_KEY` environment variable).
  * `model`: default `"saaras:v4"`.
  * `language_code`: default `"unknown"`.
  * `mode`: default `"transcribe"`.
- `speechyyy doctor` validates inference configuration, masking API keys and warning on missing credentials or invalid provider strings.
