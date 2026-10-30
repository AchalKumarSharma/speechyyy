# Issue 03: Transcription History Logger and Fix Command

Status: resolved
Blocked by: 02
Type: task

## Description

Create `src/history.rs` to maintain an append-only JSONL log of utterances at `~/.local/share/speechyyy/history.jsonl`, implement the `speechyyy history` command, and implement `speechyyy fix <corrected_phrase>` with automatic diff extraction.

## Tasks

- [x] Create `src/history.rs`:
  - `HistoryRecord` struct:
    * `timestamp`: ISO 8601 string.
    * `raw_text`: raw transcript from inference model.
    * `final_text`: final transcript delivered to caret.
    * `provider`: string.
    * `duration_secs`: f32.
  - `HistoryLogger` with helper methods:
    * `append(&self, record: &HistoryRecord) -> Result<(), std::io::Error>`.
    * `get_last_entry(&self) -> Result<Option<HistoryRecord>, std::io::Error>`.
    * `get_recent_entries(&self, limit: usize) -> Result<Vec<HistoryRecord>, std::io::Error>`.
- [x] Word-level diff algorithm:
  - `extract_correction(original: &str, corrected: &str) -> Option<(String, String)>`:
    * Finds common leading and trailing words.
    * Extracts differing middle segment as `(wrong, correct)`.
    * Trims punctuation and whitespace.
- [x] Implement CLI subcommands in `src/cli.rs` and `src/main.rs`:
  - `speechyyy history [--limit N]`: prints formatted recent utterances.
  - `speechyyy fix <corrected_phrase>`: reads last history record, runs `extract_correction`, updates `dictionary.toml`, and prints learned rule.
- [x] Unit tests in `src/history.rs`:
  - Test append and read recent entries.
  - Test single-word correction extraction.
  - Test multi-word correction extraction.
  - Test identical string returns None.

## Seams

- Seam: `extract_correction` and `HistoryLogger::append`.
  * Catches: Corrupted JSON lines, inaccurate word boundary diffs.
  * Misses: Completely rephrased sentences with zero overlap.

## Comments
Resolved with 4 unit tests in src/history.rs and CLI tests in src/cli.rs. Diff extraction cleanly computes word-level differences and commits learned pairs to dictionary.toml.
