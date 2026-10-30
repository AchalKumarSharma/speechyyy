# Issue 06: Integration Tests and Release Size Verification

Status: resolved
Blocked by: 05
Type: task

## Description

Create end-to-end integration tests for the inference pipeline, verify 100% test pass rate across the workspace, and ensure the release binary size remains strictly < 10 MB.

## Tasks

- [x] Create `tests/inference_pipeline_test.rs`:
  - Full end-to-end test simulating push-to-talk press -> audio capture -> release -> VAD -> `MockSpeechInference` -> caret text injection.
  - Verify `RecordingInjector` receives the exact transcribed string.
  - Verify silence audio does not trigger inference or caret injection.
- [x] Run full test suite:
  - `cargo test` passing across all unit and integration tests.
- [x] Verify formatting and linter:
  - `cargo clippy --all-targets` and `cargo fmt --check` with 0 warnings.
- [x] Verify binary size:
  - `cargo build --release && strip target/release/speechyyy && ls -lh target/release/speechyyy`.
  - Must remain strictly < 10 MB. (Achieved: 3.6 MB).
- [x] Update `GLOSSARY.md` and write ADR 0010 (`docs/adr/0010-streaming-and-cloud-inference-architecture.md`).

## Seams

- Seam: Full binary execution and integration test runner.
  * Catches: System integration regressions, release compiler failures, binary bloat.
  * Misses: Outbound firewall packet drops on user networks.

## Comments
Resolved. Created tests/inference_pipeline_test.rs with 4 E2E scenarios. 73 total tests passing (64 unit + 9 integration tests across 4 test suites). Clippy and rustfmt pass with 0 warnings. Stripped release binary is 3.6 MB (< 10 MB requirement). ADR 0010 recorded and GLOSSARY.md updated.

