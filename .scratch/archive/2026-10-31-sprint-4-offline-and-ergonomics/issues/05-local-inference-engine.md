# Issue 05: Local Inference Engine Implementation

Status: resolved
Blocked by: 04
Type: task

## Description

Implement `LocalSpeechInference` in `src/inference/local.rs` using the on-disk model artifacts, handling clean fallback when models are not downloaded, and executing local inference without bloating the binary past 10 MB.

## Tasks

- [x] Update `src/inference/local.rs`:
  - `LocalSpeechInference`:
    * Holds model configuration and path (`~/.local/share/speechyyy/models/<model-name>/`).
    * Implements `SpeechInference` trait.
    * In `transcribe(&mut self, chunk: &AudioChunk)`:
      - Checks if model files exist on disk.
      - If missing: cleanly returns `InferenceError::NotConfigured("Local model weights not found at ~/.local/share/speechyyy/models/. Run 'speechyyy model download' to fetch them.")` without panicking.
      - If present: performs inference on the 16kHz audio samples.
      - Keeps idle RAM < 30 MB by dropping execution contexts when idle.
- [x] Update `src/config.rs`:
  - Wire `LocalModelType` in `create_inference_provider`.
- [x] Unit tests in `src/inference/local.rs`:
  - Test missing model files cleanly returns `InferenceError::NotConfigured`.
  - Test valid model directory discovery and verification.

## Seams

- Seam: `LocalSpeechInference::transcribe`.
  * Catches: Missing assets, uninitialized paths, process crashes on missing files.
  * Misses: Operating system CPU frequency throttling.

## Comments
Resolved with unit tests in src/inference/local.rs. Missing model files return a structured InferenceError::NotConfigured directing the user to 'speechyyy model download' without panicking.
