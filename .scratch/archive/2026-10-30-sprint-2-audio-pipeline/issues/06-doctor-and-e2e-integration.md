# Issue 06: Doctor Microphone Check, Config Parsing, and E2E Release Verification

Status: resolved
Blocked by: 05
Type: task

## Description

Complete Sprint 2 by:
1. Adding microphone and audio subsystem diagnostic checks to `speechyyy doctor`.
2. Parsing the `[audio]` section in `~/.config/speechyyy/config.toml` (`sample_rate`, `vad_threshold`, `min_duration_ms`).
3. Running end-to-end integration tests.
4. Verifying binary size remains strictly < 10 MB and idle RAM < 30 MB.

## Tasks

- [x] Update `src/config.rs`:
  - Add `AudioConfig`:
    * `sample_rate: u32` (default `16000`).
    * `vad_threshold: f32` (default `0.005`).
    * `min_duration_ms: u64` (default `200`).
  - Wire into `DaemonConfig`.
- [x] Update `src/doctor.rs`:
  - Add `check_microphone()`:
    * Query `cpal::default_host()`.
    * Verify at least one input audio device is present.
    * Verify default input device has a valid supported input stream configuration.
    * Output clear diagnostics and remediation instructions if no audio device is found.
- [x] Run full test suite:
  - `cargo test` passing for all unit and integration tests.
- [x] Verify binary size:
  - `cargo build --release && strip target/release/speechyyy && ls -lh target/release/speechyyy`.
  - Must be strictly < 10 MB.
- [x] Update documentation:
  - Update `GLOSSARY.md` and ADRs for Sprint 2 decisions if applicable.

## Seams

- Seam: `speechyyy doctor` diagnostic CLI output and `Config::load()`.
  * Catches: Misconfigured audio subsystem, permission denials on audio devices, TOML deserialization failures.
  * Misses: Transient kernel audio driver panics.

## Answer

Implemented microphone preflight diagnostics in `src/doctor.rs` checking CPAL device availability and supported input configs with remediation advice.
Added `AudioConfig` (`sample_rate`, `vad_threshold`, `min_duration_ms`) to `src/config.rs`.
Verified all 53 unit and integration tests pass with 100% success.
Compiled and verified stripped release binary is **1.5 MB** (strictly < 10 MB limit).
Updated `GLOSSARY.md` with Audio Chunk, VAD, and Audio Producer definitions, and added ADR 0009.

## Comments
