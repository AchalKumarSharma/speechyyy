# Speechyyy

[![GitHub Release](https://img.shields.io/github/v/release/AchalKumarSharma/speechyyy?style=flat-square&color=blue)](https://github.com/AchalKumarSharma/speechyyy/releases)
[![Release Downloads](https://img.shields.io/github/downloads/AchalKumarSharma/speechyyy/total?style=flat-square&color=success)](https://github.com/AchalKumarSharma/speechyyy/releases)
[![GitHub Stars](https://img.shields.io/github/stars/AchalKumarSharma/speechyyy?style=flat-square&color=yellow)](https://github.com/AchalKumarSharma/speechyyy/stargazers)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg?style=flat-square)](LICENSE)

> You can type faster with your mouth now.

Speechyyy is an open-source, ultra-lightweight desktop dictation engine written in Rust. It streams transcribed speech directly into the active OS cursor in real time via global push-to-talk.

## Features

- **Global Push-to-Talk**: Hold `RightAlt` to speak, release to type directly into any focused window (Wayland & X11).
- **Ultra-Lightweight & Fast**: Compiled Rust binary (< 10 MB binary, < 30 MB idle RAM).
- **Hybrid Inference**:
  - **Local Offline**: Sherpa-ONNX with quantized models (`parakeet-tdt-v3`, `whisper-small`, `whisper-large-v3-turbo`, `moonshine-base`).
  - **Cloud BYOK**: Sarvam AI (`saaras:v4` with 22+ Indic languages and Hinglish transliteration).
- **Direct Keystroke Injection**: Hardware-level virtual input via `/dev/uinput` with automatic Wayland clipboard fallback.
- **Dynamic Vocabulary & History**: Real-time phonetic replacement rules (`speechyyy learn`, `speechyyy fix`, `speechyyy unlearn`) and history inspection.
- **Zero Telemetry**: No tracking, no external account required.

## Installation

### One-Line Install (Recommended)

Install the pre-compiled binary and set up system permissions automatically:

```bash
bash -c "$(curl -fsSL https://raw.githubusercontent.com/AchalKumarSharma/speechyyy/main/install.sh)"
```

### Build from Source

```bash
git clone https://github.com/achalkumarsharma/speechyyy.git
cd speechyyy
./install.sh
```

To configure `/dev/uinput` permissions manually:
```bash
sudo cp extra/99-speechyyy-uinput.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger
```

## Quick Start

1. **Verify System Setup**:
   ```bash
   speechyyy doctor
   ```

2. **Initial Setup Flow**:
   - **Step 1: Download or Configure a Model**:
     Fetch local model weights or configure cloud BYOK credentials without premature activation:
     ```bash
     # Download a fast local offline model (e.g. Parakeet TDT v3)
     speechyyy download parakeet-tdt-v3

     # Or configure cloud models (interactively prompts for Sarvam API key if not set)
     speechyyy download saaras:v4
     ```

   - **Step 2: Activate the Model**:
     Explicitly set your active model:
     ```bash
     speechyyy use parakeet-tdt-v3  # or: speechyyy use saaras:v4
     ```

   - **Step 3: Wake Up Speechyyy**:
     Start the background daemon:
     ```bash
     speechyyy start
     ```

3. **Dictate**:
   - Focus any text box or application.
   - Hold **RightAlt**, speak your thoughts, and release.
   - Speechyyy transcribes and types the text directly at your caret.

## CLI Reference

| Command | Description |
|---|---|
| `speechyyy start [-f]` | Start Speechyyy in background (`-f` / `--foreground` for foreground logs) |
| `speechyyy stop` | Stop the running Speechyyy process |
| `speechyyy status` | Check whether Speechyyy is active and listening |
| `speechyyy doctor` | Inspect system permissions, audio inputs, and active model configuration |
| `speechyyy model` | List all supported models with installation status and active indicator (alias: `models`) |
| `speechyyy download <MODEL>` | Fetch model weights or configure credentials without premature activation |
| `speechyyy use <MODEL>` | Switch active model, automatically stopping and reloading active processes |
| `speechyyy remove <MODEL>` | Reclaim disk space by deleting weights; deactivates and stops Speechyyy if active (alias: `rm`) |
| `speechyyy update [--force]` | In-place atomic self-updater to the latest GitHub release |
| `speechyyy uninstall [-y]` | Autonomous complete uninstallation of binary, companion libraries, models, and configs (`-y` to skip prompt) |
| `speechyyy learn <HEARD> <TYPED>` | Add a phonetic replacement rule to `dictionary.toml` |
| `speechyyy fix <EXPECTED>` | Fix the last transcription and store correction rule |
| `speechyyy unlearn [KEY]` | Remove a replacement rule from `dictionary.toml` |
| `speechyyy history` | Inspect recent dictation utterances |

## Configuration & Personality

Speechyyy comes with a warm, flirty personality engine that addresses you by nickname across all commands and notifications.

Configuration is stored in `~/.config/speechyyy/config.toml`:

```toml
hotkey = "RightAlt"
pacing_delay_ms = 15

[ui]
nickname = "babe" # Customize how Speechyyy affectionately addresses you (default: "babe")

[audio]
sample_rate = 16000
vad_threshold = 0.005
min_duration_ms = 200

[inference]
provider = "local" # "local", "sarvam", or "custom"
model = "parakeet-tdt-v3"
# api_key = "..."  # or set SARVAM_API_KEY environment variable

[injector]
method = "typewriter" # "typewriter" (direct uinput keystrokes) or "wayland" (clipboard fallback)
pacing_delay_ms = 15
```

## License

MIT License.

