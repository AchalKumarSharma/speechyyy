# Issue 06: Doctor Checks, End-to-End Integration, and Release Verification

Status: resolved
Blocked by: 01, 02, 03, 04, 05
Type: task

## Description

Wire the complete Sprint 4 pipeline into `AudioDictationEngine` and `daemon.rs`, add local model diagnostics to `speechyyy doctor`, build an end-to-end integration test suite, verify binary size remains strictly < 10 MB, and update documentation.

## Tasks

- [x] Wire `TextPipeline` and `HistoryLogger` into `AudioDictationEngine` (`src/engine.rs`) and `src/daemon.rs`:
  - Transcription -> Hallucination cleaning -> Dictionary substitution -> Smart formatting -> History logging -> Caret injection.
  - If hallucination cleaning returns empty text, cleanly return `Ok(None)` without triggering uinput or logging blank history.
- [x] Update `speechyyy doctor` in `src/doctor.rs`:
  - When `provider = "local"`, inspect `~/.local/share/speechyyy/models/`:
    * If present: reports `[PASS] Local model: Moonshine-Tiny (INT8, 5 files, 33.2 MB)`.
    * If missing: reports `[FAIL] Local model missing. Run 'speechyyy model download'`.
- [x] Create `tests/local_dictation_pipeline_test.rs`:
  - Test full Push-to-Talk cycle with text formatting, dictionary replacements, and history logging.
  - Test hallucination cleaning returns None without caret injection.
- [x] Full workspace verification:
  - `cargo test` passing 100% (103 tests passing).
  - `cargo clippy --all-targets` and `cargo fmt --check` with 0 warnings.
  - `cargo build --release && strip target/release/speechyyy && ls -lh target/release/speechyyy` (3.7 MB, strictly < 10 MB).
- [x] Update `GLOSSARY.md` and write ADR 0011 (`docs/adr/0011-offline-inference-and-text-ergonomics.md`).

## Seams

- Seam: Full push-to-talk loop in `StateMachineController` and `AudioDictationEngine`.
  * Catches: Regressions across formatting, history logging, and inference pipeline.
  * Misses: Out-of-memory errors on systems with < 256 MB RAM.

## Comments
Resolved and verified across all tests. Stripped binary is 3.7 MB. All 103 tests passing. ADR 0011 recorded.
