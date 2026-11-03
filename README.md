# Speechyyy

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

### Automated Installer (Linux)

Clone the repository and run the automated installer:

```bash
git clone https://github.com/achalsharma669/speechyyy.git
cd speechyyy
./install.sh
```

The installer builds the release binary, copies it to `~/.local/bin/speechyyy`, configures the uinput kernel module, installs the udev rule (`/etc/udev/rules.d/99-speechyyy-uinput.rules`), and runs diagnostics.

### Build from Source

Ensure you have Rust and system dependencies installed:

```bash
# Ubuntu/Debian
sudo apt-get install -y libasound2-dev pkg-config cmake

# Build release binary
cargo build --release
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

2. **Manage Models**:
   ```bash
   # List available presets
   speechyyy models

   # Download local weights (e.g. Parakeet TDT v3)
   speechyyy download parakeet-tdt-v3

   # Activate the model
   speechyyy use parakeet-tdt-v3
   ```

3. **Start the Daemon**:
   ```bash
   speechyyy start
   ```

4. **Dictate**:
   - Focus any text box or application.
   - Hold **RightAlt**, speak your thoughts, and release.
   - Speechyyy transcribes and types the text directly at your caret.

## CLI Reference

| Command | Description |
|---|---|
| `speechyyy start [-f]` | Start the background daemon (`-f` for foreground logs) |
| `speechyyy stop` | Stop the running daemon |
| `speechyyy status` | Inspect daemon process status |
| `speechyyy doctor` | Inspect system permissions, audio inputs, and model weights |
| `speechyyy models` | List available models and active status (alias: `model`) |
| `speechyyy download <MODEL>` | Download local model weights |
| `speechyyy use <MODEL>` | Switch active model (`saaras:v4`, `parakeet-tdt-v3`, etc.) |
| `speechyyy remove <MODEL>` | Delete local model weights to free disk space (alias: `rm`) |
| `speechyyy uninstall [--purge]`| Stop daemon and remove local configuration/data |
| `speechyyy learn <HEARD> <TYPED>`| Add a phonetic replacement rule to `dictionary.toml` |
| `speechyyy fix <EXPECTED>` | Fix the last transcription and store correction rule |
| `speechyyy unlearn [KEY]` | Remove a replacement rule from `dictionary.toml` |
| `speechyyy history` | Inspect recent dictation utterances |

## Configuration

Configuration is stored in `~/.config/speechyyy/config.toml`:

```toml
hotkey = "RightAlt"
pacing_delay_ms = 15

[audio]
sample_rate = 16000
vad_threshold = 0.005
min_duration_ms = 200

[inference]
provider = "local" # "local", "sarvam", or "custom"
model = "parakeet-tdt-v3"
# api_key = "..." # or set SARVAM_API_KEY environment variable

[injector]
method = "typewriter" # "typewriter" or "wayland"
pacing_delay_ms = 15
```

## License

MIT License.
