# Issue 04: Inference Configuration and Doctor Checks

Status: resolved
Blocked by: 01, 02, 03
Type: task

## Description

Add the `[inference]` configuration schema to `~/.config/speechyyy/config.toml` and extend `speechyyy doctor` to inspect API key readiness.

## Tasks

- [x] Update `src/config.rs`:
  - `InferenceConfig` struct:
    * `provider`: String (default `"mock"`).
    * `api_key`: String (default `""`).
    * `model`: String (default `"saaras:v4"`).
    * `language_code`: String (default `"unknown"`).
    * `mode`: String (default `"transcribe"`).
  - Helper method `resolve_api_key(&self) -> Option<String>`:
    * Checks `self.api_key` non-empty, falls back to `SARVAM_API_KEY` environment variable.
  - Add `pub inference: InferenceConfig` to `Config`.
- [x] Update `src/doctor.rs`:
  - Add `check_inference()` check to `SysEnv` and `run_doctor()`:
    * If provider is `mock`: reports Pass.
    * If provider is `sarvam`: verifies that `resolve_api_key()` yields a key. If missing, reports Fail with remediation (`export SARVAM_API_KEY="..."`).
    * If provider is `local`: reports Pass / Warning for local stub.
- [x] Unit tests:
  - Test TOML deserialization of `[inference]` block.
  - Test `resolve_api_key` config and environment variable precedence.
  - Test doctor diagnostics with present and missing API keys.

## Seams

- Seam: `InferenceConfig::resolve_api_key()` and `run_doctor()`.
  * Catches: Missing environment variables, config parsing errors, invalid provider strings.
  * Misses: Expired or revoked API keys at the upstream server.

## Comments
Resolved and verified with unit tests for config deserialization, environment variable fallback, and doctor diagnostics.
