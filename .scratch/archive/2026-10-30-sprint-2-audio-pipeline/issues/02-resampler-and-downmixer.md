# Issue 02: Resampler and Multi-Channel Downmixer

Status: resolved
Blocked by: 01
Type: task

## Description

Microphones often report 44,100 Hz, 48,000 Hz, or 96,000 Hz stereo or multi-channel audio. Standard speech-to-text models (Whisper, Sherpa-ONNX, Conformer) strictly require 16,000 Hz single-channel (mono) audio.

This issue implements self-contained, high-performance audio preprocessing:
1. Downmix multi-channel (e.g. stereo) interleaved `f32` samples to mono by averaging channels.
2. Resample mono `f32` PCM from arbitrary input sample rates (`from_rate`) to 16,000 Hz (`to_rate`) using linear interpolation / decimation without external heavy C libraries.

## Tasks

- [x] Implement `src/audio/resample.rs`:
  - `downmix_to_mono(interleaved: &[f32], channels: u16) -> Vec<f32>`:
    * For `channels == 1`, return a cloned vector.
    * For `channels > 1`, average each channel frame: `sum / channels`.
  - `resample_to_16k(mono_samples: &[f32], from_rate: u32) -> Vec<f32>`:
    * If `from_rate == 16000`, return unchanged.
    * If `from_rate == 48000`, perform integer decimation by 3.
    * For arbitrary rates (e.g., 44100, 96000), implement fractional linear interpolation:
      `sample = (1.0 - frac) * s0 + frac * s1`.
  - `process_raw_audio(raw: &[f32], from_rate: u32, channels: u16) -> AudioChunk`:
    * Combines downmixing and resampling, returning 16kHz mono `AudioChunk`.
- [x] Unit tests in `src/audio/resample.rs`:
  - Test stereo to mono downmix preserves amplitude and channel balance.
  - Test 48kHz to 16kHz decimation preserves frequency components without phase artifacts.
  - Test 44.1kHz to 16kHz fractional resampling produces expected sample count and signal fidelity.
  - Test boundary conditions (empty slice, 1 sample, unsupported channel count).

## Seams

- Seam: `process_raw_audio(&[f32], from_rate, channels) -> AudioChunk`.
  * Catches: Sample rate conversion math errors, channel order distortion, buffer length discrepancies.
  * Misses: Real-time latency issues caused by CPU contention.

## Answer

Implemented `downmix_to_mono` and `resample_to_16k` in `src/audio/resample.rs` using continuous-time area integration for anti-aliased downsampling (e.g. 48kHz, 44.1kHz -> 16kHz) and fractional linear interpolation.
Implemented `process_raw_audio` producing clean 16kHz single-channel `AudioChunk`.
Verified with 7 passing unit tests covering stereo downmix, empty arrays, passthrough, and 48k/44.1k resampling.

## Comments
