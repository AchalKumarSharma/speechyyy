# Issue 01: In-Memory WAV Container Encoder and SpeechInference Trait

Status: resolved
Blocked by: none
Type: task

## Description

Define the core inference domain abstractions and an in-memory 16-bit 16kHz mono WAV container encoder.

Speech recognition REST APIs (including Sarvam AI) expect standard audio containers like WAV rather than raw naked byte buffers. This issue implements a zero-dependency in-memory WAV encoder (`encode_wav_16k_mono`) that writes a valid 44-byte RIFF/WAVE header and quantizes `f32` samples into 16-bit little-endian PCM bytes. It also establishes the `SpeechInference` trait and `InferenceError` enum.

## Tasks

- [x] Create `src/inference/mod.rs` defining:
  - `InferenceError`: enum covering `MissingApiKey(String)`, `Network(String)`, `Http { status: u16, message: String }`, `Serialization(String)`, `NotConfigured(String)`.
  - `SpeechInference`: trait with `fn transcribe(&mut self, chunk: &AudioChunk) -> Result<String, InferenceError>` and `fn provider_name(&self) -> &'static str`.
- [x] Implement `src/inference/wav.rs`:
  - `encode_wav_16k_mono(samples: &[f32]) -> Vec<u8>`:
    * 44-byte RIFF/WAVE header (RIFF, fmt, 16kHz, 1 channel, 16 bits/sample, data).
    * Quantize each `f32` sample (`clamp(-1.0, 1.0) * 32767.0`) to `i16` little-endian.
- [x] Unit tests in `src/inference/wav.rs` and `src/inference/mod.rs`:
  - Verify header fields: "RIFF", "WAVE", "fmt ", sample rate = 16000, channels = 1, bits per sample = 16, "data".
  - Verify sample count and byte length match expected $44 + 2 \times N$.
  - Test boundary values (empty slice, silence, clipping at $\pm 1.0$).
  - Test `InferenceError` display formatting.

## Seams

- Seam: `encode_wav_16k_mono(&[f32]) -> Vec<u8>`.
  * Catches: Header corruption, endianness bugs, clipping distortion.
  * Misses: Network transmission issues.

## Answer

Implemented `SpeechInference` trait and `InferenceError` in `src/inference/mod.rs`.
Implemented zero-dependency 44-byte RIFF/WAVE container encoder in `src/inference/wav.rs`.
Verified with 4 unit tests covering 44-byte header byte alignments, empty sample slices, PCM quantization, and clamping behavior.

## Comments
