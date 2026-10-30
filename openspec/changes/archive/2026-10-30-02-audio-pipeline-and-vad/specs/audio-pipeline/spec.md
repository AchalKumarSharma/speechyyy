# Audio Pipeline & Capture Engine

## Purpose & Overview

Speechyyy streams speech directly into the active OS caret in real time. The Audio Pipeline captures microphone audio from the user's audio subsystem (PipeWire / ALSA on Linux), buffers it safely across thread boundaries without memory allocation or blocking locks, downmixes and resamples arbitrary input formats to 16,000 Hz single-channel PCM, and evaluates an energy-based Voice Activity Detection (VAD) filter before handing frames to downstream inference engines.

## Architecture & Data Flow

```
                      +---------------------------------------+
                      |       High-Priority OS Audio Thread   |
                      |   (PipeWire / ALSA via cpal stream)   |
                      +---------------------------------------+
                                          |
                              Gated by is_recording flag
                                          v
                      +---------------------------------------+
                      | AudioProducer (Allocation-free SPSC)  |
                      +---------------------------------------+
                                          |  Lock-free ring buffer
                                          v  (576,000 f32 capacity)
                      +---------------------------------------+
                      | AudioConsumer (Worker / Main Thread)  |
                      +---------------------------------------+
                                          |
                         drain_all() on hotkey release
                                          v
                      +---------------------------------------+
                      |  Multi-Channel Downmixing (to mono)   |
                      |    & Resampling (to 16,000 Hz PCM)    |
                      +---------------------------------------+
                                          |
                                      AudioChunk
                                          v
                      +---------------------------------------+
                      |  Energy VAD (RMS & Min Duration 200ms)|
                      +---------------------------------------+
                                   /             \
                        Fails VAD /               \ Passes VAD
                                 v                 v
                         Discard Chunk       AudioDictationEngine
                        (return to Idle)     (transition to Emitting)
```

## Requirements & Specifications

### 1. CPAL Audio Capture (`AudioCapture`)
- Captures microphone audio using the host's default audio input device.
- Supports `f32` (preferred) and `i16` input sample formats.
- Manages stream lifecycle: paused on daemon startup and while idle; unpaused when hotkey is held.
- **Callback Gating**: The audio callback inspects `is_recording` (`AtomicBool`). While idle, incoming samples are dropped immediately at zero allocation cost, preventing ring buffer accumulation or overflow spam.
- **Buffer Flushing**: Transitioning to `Recording` explicitly flushes any stale pre-trigger frames via `AudioConsumer::clear()`.

### 2. Lock-Free SPSC Ring Buffer (`AudioRingBuffer`)
- Built on `ringbuf::HeapRb<f32>` with a pre-allocated capacity of 576,000 samples (6s of 48kHz stereo or 12s of 48kHz mono).
- Split into `AudioProducer` (held by real-time callback) and `AudioConsumer` (held by `CpalAudioCapture`).
- Producer calls `push_slice(&[f32])` with zero heap allocation and zero mutex locks.
- Warns if ring buffer overflows during recording without panicking or dropping audio thread priority.

### 3. Downmixing & Resampling (`resample.rs`)
- Multi-channel input is averaged across channels into single-channel mono.
- Resampling to 16,000 Hz:
  * 16,000 Hz input passes through unchanged.
  * 48,000 Hz input performs fast 3:1 integer decimation.
  * Arbitrary sample rates (e.g. 44,100 Hz, 96,000 Hz) apply continuous-time area integration to eliminate high-frequency aliasing.
- Output is a standardized `AudioChunk` holding 16,000 Hz single-channel PCM `f32` samples.

### 4. Voice Activity Detection (`vad.rs`)
- Evaluates Root Mean Square (RMS) energy: $\sqrt{\frac{1}{N} \sum_{i=1}^{N} x_i^2}$.
- Discards accidental micro-taps (< 200ms / < 3,200 samples at 16kHz) without transcription or typing.
- Discards ambient room noise and keyboard clicks below the RMS threshold (default `0.005`).
- Valid speech returns `VadMetrics { rms, duration_secs, sample_count }`.

### 5. Dictation Engine Integration (`AudioDictationEngine`)
- Wired to the Push-to-Talk state machine:
  * Key Press (`on_press`): Transitions to `Recording`, clears buffer, unpauses stream.
  * Key Release (`on_release`): Gates callback, pauses stream, drains buffer, processes audio, evaluates VAD.
  * Valid speech transitions to `Emitting`, outputting a diagnostic summary tag (e.g. `[Speech: 1.25s, 16kHz, RMS 0.038]`).
  * Silence / micro-taps transition directly back to `Idle` with no text typed.

### 6. Diagnostics & Configuration
- `speechyyy doctor` checks default audio input device availability and valid stream configuration, printing remediation advice if PipeWire/ALSA is down.
- `~/.config/speechyyy/config.toml` supports `[audio]` section (`sample_rate`, `vad_threshold`, `min_duration_ms`).
