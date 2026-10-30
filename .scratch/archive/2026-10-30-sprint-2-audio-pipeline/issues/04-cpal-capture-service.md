# Issue 04: CPAL Audio Capture Service and Mock Implementation

Status: resolved
Blocked by: 01, 02
Type: task

## Description

Implement the concrete `CpalAudioCaptureService` using `cpal` to capture microphone audio on Linux (PipeWire / ALSA) and push samples into the lock-free SPSC ring buffer. Implement `MockAudioCaptureService` so tests and headless CI can inject deterministic audio vectors without needing physical soundcards or running sound daemons.

## Tasks

- [x] Implement `src/audio/capture.rs`:
  - `MockAudioCaptureService`:
    * Holds pre-canned audio buffers or generates synthetic sine waves.
    * Implements `AudioCaptureService` cleanly for unit and integration testing.
  - `CpalAudioCaptureService`:
    * Initializes default audio input host (`cpal::default_host()`).
    * Finds default input device (`host.default_input_device()`).
    * Inspects supported input configs and selects optimal stream format (`f32` or `i16`).
    * Builds input stream (`device.build_input_stream(...)`):
      - Audio callback pushes raw interleaved frames into `AudioProducer` via lock-free `push_slice`.
      - Error callback logs stream errors without panicking.
    * `start_capture(&mut self)`: Clears buffer, unpauses/plays stream.
    * `stop_capture(&mut self)`: Pauses stream, drains ring buffer via `AudioConsumer`, downmixes and resamples to 16,000 Hz mono `AudioChunk`.
- [x] Unit & Integration tests in `src/audio/capture.rs`:
  - Test `MockAudioCaptureService` lifecycle (`start_capture` -> push -> `stop_capture`).
  - Test `CpalAudioCaptureService` graceful handling when no input device is present (returns `AudioError::NoDevice` instead of crashing).

## Seams

- Seam: `AudioCaptureService::start_capture()` and `stop_capture()`.
  * Catches: Stream state inconsistencies, buffer drain synchronization, sample format mismatches.
  * Misses: PipeWire daemon crashes or OS-level device disconnects during recording.

## Answer

Implemented `CpalAudioCaptureService` with support for `f32`, `i16`, and `u16` stream sample formats using allocation-free callback pushing to `AudioProducer`, paired with `AudioConsumer` draining and downmix/resampling on capture halt.
Implemented `MockAudioCaptureService` for headless testing.
Verified with 3 passing unit tests verifying mock lifecycle, queued buffers, and crash-free CPAL initialization.

## Comments
