# Issue 04: Model Downloader CLI and Asset Management

Status: resolved
Blocked by: none
Type: task

## Description

Create `src/model.rs` providing `speechyyy model download [--model <name>]` to download quantized INT8 ONNX models and runtime assets into `~/.local/share/speechyyy/models/`.

## Tasks

- [x] Create `src/model.rs`:
  - `ModelMetadata` definition supporting:
    * `moonshine-tiny` (default): `preprocess.onnx`, `encode.int8.onnx`, `uncached_decode.int8.onnx`, `cached_decode.int8.onnx`, `tokens.txt` (~33 MB).
    * `whisper-tiny`: `tiny.en-encoder.int8.onnx`, `tiny.en-decoder.int8.onnx`, `tiny.en-tokens.txt` (~41 MB).
  - Storage paths: `~/.local/share/speechyyy/models/<model-name>/`.
  - `download_model(model_name: &str, target_dir: &Path) -> Result<(), String>`:
    * Uses `ureq` to stream files from HuggingFace (`csukuangfj/sherpa-onnx-...`).
    * Prints download progress per file.
    * Validates non-empty file creation.
  - `verify_model_files(model_name: &str, dir: &Path) -> Result<ModelInfo, String>`:
    * Confirms all expected files exist with non-zero size.
    * Computes total size in megabytes.
- [x] Add CLI command `speechyyy model download [--model <name>]` in `src/cli.rs` and `src/main.rs`.
- [x] Unit tests in `src/model.rs`:
  - Test model manifest definitions and expected filenames.
  - Test `verify_model_files` on temporary directory with mock files.
  - Test missing file detection.

## Seams

- Seam: `verify_model_files` and download pipeline.
  * Catches: Partial downloads, missing tokenizer or weight files.
  * Misses: Network timeouts on upstream HuggingFace mirrors.

## Comments
Resolved with 3 unit tests in src/model.rs and CLI parsing test in src/cli.rs. Model downloader creates .part staging files before atomic rename to guarantee asset integrity.
