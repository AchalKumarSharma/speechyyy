# 01: In-Memory WAV Serialization and Sarvam STT Provider

**What to build:**
Enable bilingual and Indic speech-to-text transcription via Sarvam AI's Saaras v4 cloud engine. Audio samples captured from the microphone pipeline are formatted directly in memory as standard 16,000 Hz 16-bit mono PCM RIFF/WAVE bytes with zero disk writes, and dispatched via an RFC 7578 multipart HTTP request to `https://api.sarvam.ai/speech-to-text`. The provider defaults to `mode = "translit"` so that spoken Hindi is transcribed into Romanized Hinglish (e.g. *"mera phone number note kar lo"*), while also supporting `"transcribe"`, `"translate"`, and `"codemix"`.

**Blocked by:** None (can start immediately)

**Status:** resolved

## Acceptance Criteria

- [x] In-memory canonical RIFF/WAVE encoder in `src/audio/wav.rs` (and re-exported in `src/inference/wav.rs`) formats `&[f32]` samples into 44-byte header + 16-bit signed little-endian PCM bytes with zero disk I/O.
- [x] `SarvamSpeechInference` implements the unified `SpeechInference` trait (`transcribe(&mut self, chunk: &AudioChunk) -> Result<String, InferenceError>`).
- [x] Sarvam client defaults to `model = "saaras:v4"` and `mode = "translit"`.
- [x] Exposes configurable support for all four Sarvam operation modes: `"translit"`, `"transcribe"`, `"translate"`, and `"codemix"`.
- [x] RFC 7578 multipart payload includes required boundary headers, `file` (as `audio.wav`), `model`, `mode`, and optional `language_code` fields.
- [x] Deserializes valid Sarvam responses (`{ "transcript": "...", "language_code": "..." }`) into trimmed transcript strings.
- [x] Maps transport errors, network timeouts, and HTTP status codes (400, 401, 429, 500) into typed `InferenceError` variants without panicking.
- [x] Uses synchronous `ureq` client to avoid pulling in heavy async runtimes, preserving binary footprint < 10 MB.
- [x] Comprehensive unit tests verify WAV byte structure, multipart boundary layout, and HTTP mock response handling.

## Testing Seams

- **Seam:** `SpeechInference::transcribe(&mut self, chunk: &AudioChunk) -> Result<String, InferenceError>`.
- **Method:** Tested against an isolated mock HTTP server via `SarvamSpeechInference::with_endpoint(&str)` to assert multipart payload formatting, headers (`api-subscription-key`), mode handling, and error response mapping without external network dependency.
