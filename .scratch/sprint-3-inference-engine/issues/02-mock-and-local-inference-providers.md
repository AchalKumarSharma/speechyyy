# Issue 02: Mock and Local Inference Providers

Status: resolved
Blocked by: 01
Type: task

## Description

Implement `MockSpeechInference` for deterministic offline testing and `LocalSpeechInference` as an extensible stub for future offline quantized ONNX / Sherpa runtimes.

`MockSpeechInference` allows tests to queue specific transcripts or verify that audio chunks were received without making real HTTP requests or requiring cloud API keys.

## Tasks

- [x] Implement `src/inference/mock.rs`:
  - `MockSpeechInference` struct with:
    * `queued_transcripts: Vec<String>`
    * `transcribed_chunks: Vec<AudioChunk>`
    * `call_count: usize`
  - Methods:
    * `new() -> Self`
    * `with_transcript(text: &str) -> Self`
    * `with_queued_transcripts(transcripts: Vec<String>) -> Self`
  - Implement `SpeechInference` for `MockSpeechInference`.
- [x] Implement `src/inference/local.rs`:
  - `LocalSpeechInference` struct implementing `SpeechInference`.
  - Returns `Err(InferenceError::NotConfigured(...))` stating that local offline model weights are reserved for Sprint 4.
- [x] Unit tests:
  - Test `MockSpeechInference` with default transcript, queued transcripts, and chunk recording.
  - Test `LocalSpeechInference` returns expected `NotConfigured` error.

## Seams

- Seam: `SpeechInference::transcribe(&AudioChunk)`.
  * Catches: Inconsistent provider trait signatures, mutable state recording bugs.
  * Misses: Real network errors.

## Answer

Implemented `MockSpeechInference` in `src/inference/mock.rs` with configurable queued transcripts and call metrics.
Implemented `LocalSpeechInference` stub in `src/inference/local.rs` returning `NotConfigured` until Sprint 4.
Verified with unit tests for both providers.

## Comments
