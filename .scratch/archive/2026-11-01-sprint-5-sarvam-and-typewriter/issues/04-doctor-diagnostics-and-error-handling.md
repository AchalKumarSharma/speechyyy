# 04: Doctor Diagnostics and Error Handling

**What to build:**
Enhance `speechyyy doctor` to inspect Sarvam AI credentials, endpoint connectivity, and `/dev/uinput` permissions required for typewriter injection, and ensure transient cloud errors (network timeout, HTTP 401, HTTP 429) produce clear, user-friendly messages rather than crashing the background daemon.

**Blocked by:** 01 (In-Memory WAV & Sarvam STT), 03 (Configuration & CLI Flags)

**Status:** resolved

## Acceptance Criteria

- [x] Update `src/doctor.rs`:
  - When `provider = "sarvam"`:
    * Checks if a Sarvam API key is resolved from `config.toml` or `SARVAM_API_KEY`.
    * If present: reports `[PASS] Sarvam API Key: sk_...1234 (configured)`.
    * If missing: reports `[FAIL] Sarvam API Key missing. Set SARVAM_API_KEY in config.toml or environment.`.
  - When `injection_mode = "typewriter"`:
    * Verifies `/dev/uinput` is writable for sequential keystroke synthesis.
    * If writable: reports `[PASS] Typewriter Injection: /dev/uinput writable`.
    * If not writable: reports `[WARN] /dev/uinput not writable: typewriter mode will fall back to clipboard paste.`.
- [x] Structured error messages in `SarvamSpeechInference`:
  - HTTP 401: `"Invalid or expired Sarvam API key. Check 'speechyyy doctor' or replenish credentials at https://dashboard.sarvam.ai"`.
  - HTTP 429: `"Sarvam AI rate limit exceeded or credit balance exhausted"`.
  - Network timeout: `"Sarvam AI connection timed out. Check network connectivity"`.
- [x] `AudioDictationEngine` handles inference errors gracefully, logging the failure to standard error without crashing the daemon session loop.
- [x] Unit tests in `src/doctor.rs` verify diagnostic output when credentials and permissions are present, missing, or misconfigured.

## Testing Seams

- **Seam:** `run_doctor(&SysEnv) -> Vec<DoctorCheck>`.
- **Method:** Unit tests with mock `SysEnv` simulating present/missing API keys, valid/unwritable `/dev/uinput` permissions, and verifying exact doctor check pass/fail verdicts.
