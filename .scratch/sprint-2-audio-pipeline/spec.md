# Sprint 2: Audio Pipeline & Capture Engine

## Problem Statement

Sprint 1 validated global push-to-talk key interception, daemon process lifecycle, and caret text injection with a mock string. However, speech-to-text dictation requires streaming real-time microphone audio from the user's audio subsystem (PipeWire/ALSA on Linux), buffering it safely without memory allocation or blocking locks on the real-time audio thread, downmixing and resampling arbitrary input sample rates to 16,000 Hz single-channel PCM, and filtering out silence or accidental micro-taps before passing audio frames to inference models.

## Solution

Build an audio capture and processing pipeline for Speechyyy using `cpal` and a lock-free Single-Producer Single-Consumer (SPSC) ring buffer (`ringbuf`). The pipeline captures audio from the default input device without locking or allocation in the callback, downmixes multi-channel input to mono, resamples to 16,000 Hz PCM, and evaluates an energy-based Voice Activity Detection (RMS threshold and 200ms minimum duration check). The pipeline is abstracted behind an `AudioCaptureService` trait and wired to the Push-to-Talk state machine so pressing the hotkey starts recording, and releasing evaluates VAD and produces a clean 16kHz `AudioChunk`.

## User Stories

1. As a user, I want the daemon to record audio from my default microphone when holding the push-to-talk hotkey, so that my voice is captured for dictation.
2. As a user, I want the audio callback to be allocation- and lock-free via an SPSC ring buffer, so that the audio stream never crackles, glitches, or suffers buffer underruns.
3. As a developer, I want the capture service abstracted behind an `AudioCaptureService` trait, so that the audio pipeline can be tested deterministically with mock buffers in headless environments.
4. As a user, I want my microphone audio (e.g. 44.1kHz or 48kHz stereo) downmixed and resampled to 16,000 Hz mono PCM, so that it matches standard ASR model input requirements.
5. As a user, I want accidental key taps (< 200ms) to be automatically discarded, so that unintended hotkey presses do not trigger transcription or caret typing.
6. As a user, I want quiet ambient room noise and keyboard clicks (< 0.005 RMS threshold) to be discarded as silence, so that empty recordings do not emit text.
7. As a user, I want holding push-to-talk, speaking, and releasing to emit a formatted summary tag at the caret in Sprint 2 (e.g. `[Speech: 1.25s, 16kHz, RMS 0.038]`), so that I have immediate tactile confirmation that audio capture works before AI models are plugged in.
8. As a user, I want to configure VAD thresholds and sample rate in `~/.config/speechyyy/config.toml` under `[audio]`, so that I can tune sensitivity for my microphone.
9. As a user, I want `speechyyy doctor` to inspect microphone availability and permissions alongside input devices, so that audio setup issues are diagnosed proactively.

## Implementation Decisions

- **Dependencies**: Use `cpal` for cross-platform audio input and `ringbuf` for lock-free SPSC ring buffering.
- **Lock-Free Capture Boundary**: An SPSC ring buffer (`ringbuf::HeapRb<f32>`) connects the real-time audio thread to the processing worker. The callback performs only non-blocking pushes with zero allocation.
- **Resampling & Downmixing**: `resample.rs` averages multi-channel inputs to single-channel mono and resamples to 16,000 Hz using decimation and band-limited interpolation.
- **Voice Activity Detection**: `vad.rs` computes RMS energy and validates minimum duration (200ms). Chunks failing VAD return `None`.
- **Dictation Engine Wiring**: `AudioDictationEngine` adapts `AudioCaptureService` to the `DictationEngine` trait. On key release, valid speech transitions to `Emitting` with audio summary text, while silent/short chunks return `None`.
- **Doctor Check**: `src/doctor.rs` inspects audio device enumeration via `cpal::default_host()`.
- **Configuration**: `[audio]` section in `config.toml` configuring `sample_rate`, `vad_threshold`, and `min_duration_ms`.

## Testing Decisions

- **Ring Buffer Tests**: Multithreaded producer-consumer tests verifying zero drop and lock-free throughput.
- **Resampler Tests**: Precise signal verification converting 44.1kHz and 48kHz sine waves to 16kHz.
- **VAD Tests**: Edge-case tests verifying rejection of micro-taps (< 200ms), rejection of pure silence, and acceptance of voice bursts.
- **End-to-End Engine Tests**: Testing `AudioDictationEngine` with `MockAudioCaptureService` exercising press -> capture -> release -> VAD -> caret emission.

## Out of Scope

- Offline Sherpa-ONNX model inference execution (Sprint 3).
- Sarvam AI / Groq WebSocket streaming (Sprint 3).
- Microphone selection CLI or GUI picker (Sprint 4).

