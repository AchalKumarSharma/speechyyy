# Tasks: 02-audio-pipeline-and-vad

- [x] Lock-free SPSC ring buffer with `AudioProducer` and `AudioConsumer` (`src/audio/ring_buffer.rs`)
- [x] Audio domain types `AudioChunk`, `AudioError`, and `AudioCapture` trait (`src/audio/mod.rs`)
- [x] Multi-channel to mono downmixing and anti-aliased 16kHz resampling with 48kHz 3:1 decimation (`src/audio/resample.rs`)
- [x] Energy-based Voice Activity Detection with RMS calculation and 200ms micro-tap rejection (`src/audio/vad.rs`)
- [x] Production `CpalAudioCapture` with idle callback gating and `MockAudioCapture` for headless CI (`src/audio/capture.rs`)
- [x] Dictation engine integration wiring capture and VAD to push-to-talk loop (`src/engine.rs`, `src/daemon.rs`)
- [x] Microphone diagnostic checks in `speechyyy doctor` (`src/doctor.rs`)
- [x] `[audio]` configuration block parsing in `~/.config/speechyyy/config.toml` (`src/config.rs`)
- [x] End-to-end integration tests verifying speech emission and silence/micro-tap rejection (`tests/audio_pipeline_test.rs`)
- [x] Release binary size verification (strictly < 10MB target, reached 1.5MB stripped)
