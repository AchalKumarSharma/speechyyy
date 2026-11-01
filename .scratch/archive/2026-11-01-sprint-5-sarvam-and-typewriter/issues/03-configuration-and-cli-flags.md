# 03: Configuration and CLI Flags

**What to build:**
Enable users to configure and dynamically override speech inference providers, Sarvam operating modes, text injection modes, and typing pacing speeds via `~/.config/speechyyy/config.toml`, environment variables, and CLI flags on `speechyyy start`.

**Blocked by:** 01 (In-Memory WAV & Sarvam STT), 02 (Typewriter Keystroke Injector)

**Status:** resolved

## Acceptance Criteria

- [x] Update `InferenceConfig` in `src/config.rs`:
  - `provider`: default `"local"`, accepts `"local"`, `"sarvam"`, or `"mock"`.
  - `mode`: default `"translit"`, accepts `"translit"`, `"transcribe"`, `"translate"`, `"codemix"`.
  - `api_key`: string, with `resolve_api_key()` checking `config.toml` first, then falling back to the `SARVAM_API_KEY` environment variable.
- [x] Update `InjectorConfig` in `src/config.rs`:
  - `mode`: default `InjectionMode::Typewriter { delay_ms: 15 }`, accepts `"typewriter"` or `"paste"`.
  - `pacing_delay_ms`: default `15`.
- [x] Extend `Command::Start` in `src/cli.rs`:
  - `--provider <local|sarvam>`: overrides `config.inference.provider`.
  - `--mode <translit|transcribe|translate|codemix>`: overrides `config.inference.mode`.
  - `--injection <typewriter|paste>`: overrides `config.injector.mode`.
  - `--typing-delay <ms>`: overrides `config.injector.pacing_delay_ms`.
- [x] Wire CLI flag overrides into the session initialization flow in `src/main.rs`.
- [x] Ensure backward compatibility with existing `config.toml` files missing new fields through serde defaults.
- [x] Unit tests in `src/cli.rs` and `src/config.rs`:
  - Test CLI argument parsing for `--provider`, `--mode`, `--injection`, and `--typing-delay`.
  - Test precedence: CLI flags override `config.toml`, which overrides built-in defaults.
  - Test `SARVAM_API_KEY` environment variable fallback when `api_key` is empty in `config.toml`.

## Testing Seams

- **Seam:** CLI argument parsing (`Cli::try_parse_from`) and configuration loader (`Config::load_from_path` / `Config::load`).
- **Method:** Unit tests verifying flag permutations, invalid option rejection, and configuration precedence rules.
