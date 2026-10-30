# Proposal: Audio Pipeline & Capture Engine (Sprint 2)

## Why

Speech-to-text dictation requires streaming real-time microphone audio from the user's audio subsystem (PipeWire/ALSA on Linux), buffering it safely without memory allocation or blocking locks on the real-time audio thread, downmixing and resampling arbitrary input sample rates to 16,000 Hz single-channel PCM, and filtering out silence or accidental micro-taps before passing audio frames to inference models.

## What Changes

- Abstract `AudioCapture` trait enabling both production CPAL capture and deterministic mock testing in CI.
- Lock-free Single-Producer Single-Consumer (SPSC) ring buffer (`ringbuf::HeapRb<f32>`) with 576,000 sample capacity (> 6s stereo / 12s mono at 48kHz).
- Stream lifecycle management with idle callback gating and pre-trigger buffer clearing to eliminate buffer overflows during idle periods.
- High-performance multi-channel to mono downmixer and anti-aliased 16,000 Hz resampler with fast 3:1 integer decimation for 48kHz.
- Energy-based Voice Activity Detection (RMS threshold 0.005 and 200ms minimum duration filter).
- `AudioDictationEngine` wired to Push-to-Talk state machine, emitting tactile diagnostic summary tags on valid speech while cleanly discarding silence and micro-taps.
- Microphone diagnostics in `speechyyy doctor` checking input devices and stream configs.
- `[audio]` configuration block in `~/.config/speechyyy/config.toml`.
