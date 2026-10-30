# Issue 03: Sarvam AI Cloud Inference Provider

Status: resolved
Blocked by: 01, 02
Type: task

## Description

Implement `SarvamSpeechInference` using `ureq` to query Sarvam AI's speech-to-text REST endpoint (`https://api.sarvam.ai/speech-to-text`).

The provider takes a 16kHz mono `AudioChunk`, packages it into an in-memory 16-bit WAV file, constructs a `multipart/form-data` payload containing the file, model (`saaras:v4`), mode (`transcribe` / `codemix` / `translate`), and optional `language_code`, and posts to Sarvam AI with the `api-subscription-key` header.

## Tasks

- [x] Implement `src/inference/sarvam.rs`:
  - `SarvamSpeechInference` holding `api_key`, `model`, `language_code`, and `mode`.
  - `build_multipart_body(boundary: &str, wav_bytes: &[u8], model: &str, mode: &str, language_code: &str) -> Vec<u8>`.
  - `SarvamResponse` struct deriving `Deserialize` (`request_id`, `transcript`, `language_code`).
  - Implement `SpeechInference for SarvamSpeechInference`:
    * Encodes WAV bytes from `chunk.samples`.
    * Dispatches HTTP POST to `https://api.sarvam.ai/speech-to-text`.
    * Maps HTTP errors (401 Unauthorized, 400 Bad Request, 5xx) to `InferenceError`.
    * Returns trimmed transcript string.
- [x] Unit tests in `src/inference/sarvam.rs`:
  - Test multipart payload formatting (headers, boundary markers, field names, carriage returns).
  - Test `SarvamResponse` JSON deserialization with real Sarvam API fixture responses.
  - Test error mapping from HTTP status codes.

## Seams

- Seam: `build_multipart_body` and `SarvamSpeechInference::transcribe`.
  * Catches: Multipart delimiter corruption, missing headers, JSON parsing failures.
  * Misses: Live Sarvam API outages or upstream rate limits.

## Answer

Implemented `SarvamSpeechInference` in `src/inference/sarvam.rs` with in-memory WAV serialization, RFC 7578 multipart body construction, `api-subscription-key` header dispatch, and JSON response parsing.
Verified with 4 unit tests covering multipart boundaries, unknown language filtering, JSON deserialization, and missing API key error classification.

## Comments
