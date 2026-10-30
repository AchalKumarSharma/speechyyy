# Issue 03: Energy-Based Voice Activity Detection (VAD)

Status: resolved
Blocked by: 01, 02
Type: task

## Description

Users often tap hotkeys accidentally, or release them without speaking. Passing empty or silent audio chunks to speech recognition engines wastes CPU/network and triggers false transcriptions or hallucinated punctuation.

This issue implements an energy-based Voice Activity Detection (VAD) module using Root Mean Square (RMS) energy calculation and minimum duration filtering.

## Tasks

- [x] Implement `src/audio/vad.rs`:
  - `compute_rms(samples: &[f32]) -> f32`:
    * Calculate $\sqrt{\frac{1}{N} \sum_{i=1}^{N} x_i^2}$.
  - `EnergyVad` struct with:
    * `threshold: f32` (default `0.005`).
    * `min_duration_samples: usize` (default `3200` samples = 200ms at 16kHz).
  - `EnergyVad::evaluate(&self, chunk: &AudioChunk) -> Option<VadMetrics>`:
    * Returns `None` if `chunk.samples.len() < self.min_duration_samples`.
    * Calculates RMS energy across the chunk (or sliding windows).
    * Returns `None` if RMS is below `self.threshold`.
    * Returns `Some(VadMetrics { rms, duration_secs, sample_count })` if valid voice activity is detected.
- [x] Unit tests in `src/audio/vad.rs`:
  - Verify pure zero silence fails VAD (`None`).
  - Verify low ambient noise (RMS = 0.001) fails VAD (`None`).
  - Verify micro-taps (< 200ms / < 3,200 samples) fail VAD even with high amplitude (`None`).
  - Verify genuine speech waveform (sine burst > 200ms with RMS > 0.005) passes VAD and returns accurate metrics.

## Seams

- Seam: `EnergyVad::evaluate(&AudioChunk) -> Option<VadMetrics>`.
  * Catches: Silence misclassification, micro-tap edge cases, RMS calculation precision.
  * Misses: Advanced semantic speech vs non-speech noise distinction (e.g. coughing or dog barking).

## Answer

Implemented `compute_rms`, `VadMetrics`, and `EnergyVad` in `src/audio/vad.rs` with configurable RMS threshold (default 0.005) and minimum duration (default 200ms / 3200 samples at 16kHz).
Verified with 6 passing unit tests covering pure zero silence, DC signals, sine waves, micro-taps (<200ms rejection), ambient noise rejection (<0.005), and valid voice activity detection.

## Comments
