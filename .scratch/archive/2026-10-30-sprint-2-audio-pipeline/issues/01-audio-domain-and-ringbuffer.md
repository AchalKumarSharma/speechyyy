# Issue 01: Audio Domain Types and Lock-Free SPSC Ring Buffer

Status: resolved
Blocked by: none
Type: task

## Description

Define core audio domain models and implement a lock-free Single-Producer Single-Consumer (SPSC) ring buffer wrapper around `ringbuf`.

The real-time audio callback in `cpal` runs on a high-priority OS audio thread and must never allocate on the heap or block on mutex locks. This issue establishes the domain types (`AudioChunk`, `AudioError`, `AudioCaptureService` trait) and creates an allocation-free audio ring buffer abstraction that allows the audio callback to safely enqueue `f32` samples into consumer storage.

## Tasks

- [x] Create `src/audio/mod.rs` defining:
  - `AudioChunk`: struct holding `samples: Vec<f32>`, `sample_rate: u32`, `channels: u16`.
  - `AudioError`: enum covering device errors, stream errors, and format mismatches.
  - `AudioCaptureService`: trait with `start_capture(&mut self) -> Result<(), AudioError>`, `stop_capture(&mut self) -> Result<AudioChunk, AudioError>`, and `is_capturing(&self) -> bool`.
- [x] Implement `src/audio/ring_buffer.rs`:
  - `AudioRingBuffer`: wraps `ringbuf::traits::RingBuffer` with configurable capacity (default 10 seconds of audio at 48kHz = 480,000 samples).
  - Split into a lock-free producer (`AudioProducer`) and consumer (`AudioConsumer`).
  - Producer provides `push_slice(&mut self, samples: &[f32]) -> usize` with zero heap allocations.
  - Consumer provides `drain_all(&mut self) -> Vec<f32>` and `clear(&mut self)`.
- [x] Unit tests in `src/audio/ring_buffer.rs`:
  - Test push and drain under single-thread and cross-thread producer-consumer patterns.
  - Test buffer overflow behavior (drops oldest or logs warning without panicking or blocking).

## Seams

- Seam: `AudioRingBuffer::new(capacity)` -> `(AudioProducer, AudioConsumer)`.
  * Catches: Thread synchronization bugs, buffer capacity exhaustion, data corruption.
  * Misses: Hardware audio device failures.

## Answer

Implemented `AudioChunk`, `AudioError`, and `AudioCaptureService` trait in `src/audio/mod.rs`.
Implemented zero-allocation lock-free `AudioRingBuffer` (`AudioProducer` and `AudioConsumer`) in `src/audio/ring_buffer.rs` using `ringbuf::HeapRb`.
Verified with 6 passing unit tests covering single-thread push/drain, buffer limits, and multi-threaded SPSC transfer.

## Comments
