# Tasks: 03-streaming-and-cloud-inference

- [x] Abstract `SpeechInference` trait and `InferenceError` enum (`src/inference/mod.rs`)
- [x] Zero-dependency in-memory 16-bit 16kHz mono WAV container encoder (`src/inference/wav.rs`)
- [x] Deterministic `MockSpeechInference` provider with queued transcripts (`src/inference/mock.rs`)
- [x] Production `SarvamSpeechInference` cloud provider with RFC 7578 multipart upload (`src/inference/sarvam.rs`)
- [x] Offline `LocalSpeechInference` provider stub (`src/inference/local.rs`)
- [x] `AudioDictationEngine` wiring and state machine integration (`src/engine.rs`, `src/daemon.rs`)
- [x] `[inference]` configuration schema and `SARVAM_API_KEY` resolution (`src/config.rs`)
- [x] Inference diagnostics and API key validation in `speechyyy doctor` (`src/doctor.rs`)
- [x] End-to-end integration tests verifying transcription and error resilience (`tests/inference_pipeline_test.rs`)
- [x] Release binary size verification (strictly < 10MB target, reached 3.6MB stripped)
