# Issue 02: Dynamic Learning Dictionary and Replacement Engine

Status: resolved
Blocked by: 01
Type: task

## Description

Create `src/text/dictionary.rs` to load, match, and update custom word substitutions from `~/.config/speechyyy/dictionary.toml` using strict case-insensitive word boundary matching.

## Tasks

- [x] Create `src/text/dictionary.rs`:
  - `LearningDictionary` struct holding sorted substitution pairs `(pattern, replacement)`.
  - Storage location: `~/.config/speechyyy/dictionary.toml`.
  - Format:
    ```toml
    [replacements]
    "post gress" = "PostgreSQL"
    "wispr flow" = "wisprflow"
    "java script" = "JavaScript"
    ```
  - `apply(&self, text: &str) -> String`:
    * Case-insensitive matching of `pattern`.
    * Strict word-boundary check (before and after match must not be alphanumeric or underscore).
    * Prevents substring mangling (e.g., "js" does not replace inside "json").
    * Descending length order so longer phrases take precedence over substrings.
  - `add_rule(&mut self, wrong: &str, correct: &str)`:
    * Adds or updates the rule in memory.
  - `load_from_path(path: &Path) -> Result<Self, String>` and `save_to_path(&self, path: &Path) -> Result<(), String>`.
- [x] Implement CLI command `speechyyy learn <wrong> <correct>` in `src/cli.rs` and `src/main.rs`.
- [x] Unit tests in `src/text/dictionary.rs`:
  - Test word boundary replacement `"post gress"` -> `"PostgreSQL"`.
  - Test substring protection: `"js"` inside `"json"` is NOT replaced.
  - Test case-insensitivity: `"POST GRESS"` -> `"PostgreSQL"`.
  - Test longer phrase precedence.
  - Test save and load roundtrip to TOML file.

## Seams

- Seam: `LearningDictionary::apply(&self, text: &str) -> String`.
  * Catches: Mangled words from substring collisions, file parsing corruption.
  * Misses: Context-sensitive grammar inflection.

## Comments
Resolved with 5 unit tests in src/text/dictionary.rs and CLI parser tests in src/cli.rs. Substring protection guarantees that 'js' does not alter 'json' or 'nodejs'.
