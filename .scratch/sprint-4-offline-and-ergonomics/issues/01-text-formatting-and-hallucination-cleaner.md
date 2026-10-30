# Issue 01: Text Formatting and Hallucination Cleaner

Status: resolved
Blocked by: none
Type: task

## Description

Create `src/text/mod.rs` and `src/text/format.rs` providing smart spacing, sentence casing, and hallucination cleaning for transcribed speech.

## Tasks

- [x] Create `src/text/mod.rs` and `src/text/format.rs`:
  - `clean_hallucinations(text: &str) -> Option<String>`:
    * Strips bracketed tokens (`[...]`, `(...)`, `*...*`) such as `[BLANK_AUDIO]`, `(music)`, `*applause*`, `[silence]`.
    * Trims leftover whitespace; returns `None` if the text becomes empty.
  - `capitalize_sentence(text: &str) -> String`:
    * Capitalizes the first ASCII alphabetic character if lowercase; leaves acronyms and symbols intact.
  - `TextFormatter` stateful struct for smart spacing:
    * Tracks `last_char: Option<char>`.
    * `format(&mut self, text: &str) -> Option<String>`:
      - Cleans hallucinations (returns `None` if blank).
      - Applies sentence casing.
      - If `last_char` was alphanumeric and new text starts with an alphanumeric character, prepends a single space `' '`.
      - If new text starts with punctuation (`,`, `.`, `?`, `!`, `;`, `:`), avoids prepending a space.
      - Updates `last_char` to the end of the formatted text.
      - `reset()` method to clear spacing context on pause or enter.
- [x] Unit tests in `src/text/format.rs`:
  - Test `[BLANK_AUDIO]` and `(music)` stripped to `None`.
  - Test mixed text `"[BLANK_AUDIO] Hello world"` cleaned to `"Hello world"`.
  - Test sentence casing on lowercase start.
  - Test smart spacing across consecutive dictations.
  - Test punctuation attachment without leading spaces.

## Seams

- Seam: `TextFormatter::format(&mut self, text: &str) -> Option<String>`.
  * Catches: Colliding words between consecutive Push-to-Talk sessions, ghost characters from audio hallucinations.
  * Misses: Multilingual uppercase rules beyond standard Unicode.

## Comments
Resolved with 7 comprehensive unit tests in src/text/format.rs covering hallucination stripping, sentence casing, smart spacing, and punctuation handling.
