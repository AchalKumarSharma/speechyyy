# Sprint 4: Offline Local Inference, Dynamic Word Learning & Text Ergonomics

## Problem Statement

Sprint 3 enabled cloud speech inference via Sarvam AI REST API and mock offline inference for testing. However:
1. Speechyyy cannot yet transcribe speech offline without an internet connection or cloud API key.
2. Technical terminology, custom jargon, and phonetic transcription mistakes (e.g., "post gress" instead of "PostgreSQL") require manual correction every time.
3. Rapid push-to-talk bursts can collide without smart spacing between consecutive dictations.
4. Whisper/Moonshine models can hallucinate bracketed artifacts (e.g., `[BLANK_AUDIO]`, `(music)`).

In Sprint 4, Speechyyy introduces:
1. On-device local inference with `sherpa-onnx` INT8 quantized models (`moonshine-tiny` and `whisper-tiny`), downloaded on demand via `speechyyy model download`.
2. Dynamic learning dictionary stored in `~/.config/speechyyy/dictionary.toml` with `speechyyy learn` and diff-based `speechyyy fix`.
3. Transcription history logger recording utterances to `~/.local/share/speechyyy/history.jsonl` with `speechyyy history`.
4. Text formatting & ergonomics layer providing smart spacing, sentence casing, and hallucination cleaning.
5. Strict constraints: binary size < 10 MB, idle RAM < 30 MB, peak decoding RAM <= 250 MB, 100% test pass rate.

## Key Requirements & Invariants

### 1. Text Formatting & Ergonomics (`src/text/format.rs`)
- Hallucination cleaner strips bracketed artifacts (`[...]`, `(...)`, `*...*`) such as `[BLANK_AUDIO]`, `(music)`, `*applause*`.
- If cleaned text is empty or whitespace, it returns `None`, suppressing caret injection and history logging.
- Sentence casing capitalizes the leading ASCII letter of new sentences without altering intentional inner capitalization.
- Smart spacing tracks typing state across dictations to prepend a space before alphanumeric words when following a word, while preserving tight punctuation (`,`, `.`, `?`, `!`, `;`, `:`).

### 2. Dynamic Learning Dictionary (`src/text/dictionary.rs`)
- Stored independently at `~/.config/speechyyy/dictionary.toml`.
- Case-insensitive whole-word boundary substitution pass applied before caret injection.
- Prevents substring mangling (e.g. "js" will not match inside "json").
- `speechyyy learn <wrong> <correct>` adds or updates replacement rules.
- `speechyyy fix <corrected_phrase>` diffs against the most recent history item to extract the replacement pair automatically.

### 3. Transcription History Logger (`src/history.rs`)
- Append-only JSONL log at `~/.local/share/speechyyy/history.jsonl`.
- Schema: `{ timestamp, raw_text, final_text, provider, duration_secs }`.
- `speechyyy history [--limit N]` displays recent utterances.

### 4. Local Model Downloader & Storage (`src/model.rs`)
- Directory: `~/.local/share/speechyyy/models/<model_name>/`.
- Pre-converted INT8 quantized weights (Moonshine-Tiny ~33 MB, Whisper-Tiny ~41 MB) downloaded via `ureq` from Hugging Face with progress telemetry.
- CLI subcommand: `speechyyy model download [--model <name>]`.

### 5. Local Inference Engine (`src/inference/local.rs`)
- Replaces stub with functional `LocalSpeechInference`.
- Validates model assets on disk; returns `InferenceError::NotConfigured` cleanly with instructions to run `speechyyy model download` if missing or unconfigured.
- Low memory footprint: allocates decoding context on demand, keeping idle daemon RAM < 30 MB.

### 6. Diagnostics & Verification
- `speechyyy doctor` inspects local model assets on disk when `provider = "local"`.
- Stripped release binary strictly < 10 MB.
- 100% test pass rate and 0 clippy warnings.

