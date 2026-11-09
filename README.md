# Speechyyy

[![GitHub Release](https://img.shields.io/github/v/release/AchalKumarSharma/speechyyy?style=flat-square&color=blue)](https://github.com/AchalKumarSharma/speechyyy/releases)
[![Release Downloads](https://img.shields.io/github/downloads/AchalKumarSharma/speechyyy/total?style=flat-square&color=success)](https://github.com/AchalKumarSharma/speechyyy/releases)
[![GitHub Stars](https://img.shields.io/github/stars/AchalKumarSharma/speechyyy?style=flat-square&color=yellow)](https://github.com/AchalKumarSharma/speechyyy/stargazers)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg?style=flat-square)](LICENSE)

Speech-to-text dictation engine for Wayland compositors on Linux, featuring a headless daemon and an ambient companion dock.

---

## Highlights

- **Dual-Binary Architecture:** Decouples background audio capture and inference (`speechyyy`) from real-time visual feedback (`speechyyy-dock`).
- **Synchronous Rust:** Built entirely with synchronous blocking I/O (zero dependencies on Tokio or async-std), compiling down to compact native binaries (`speechyyy`: 3.6 MB, `speechyyy-dock`: 1.2 MB; 4.8 MB combined).
- **Local Offline Inference:** On-device recognition via sherpa-onnx supporting NVIDIA NeMo Parakeet-TDT v3, OpenAI Whisper Large-v3 Turbo (alias `whisper-turbo`), Useful Sensors Moonshine Base, and Whisper Small.
- **BYOK Cloud Providers:** Bring-your-own-key transcription with Sarvam AI, Groq, OpenAI, and custom OpenAI-compatible endpoints.
- **Text Delivery:** Instant clipboard paste (`wl-copy` + synthetic `Ctrl+V`) by default, with an opt-in typewriter mode (`/dev/uinput`).
- **Privacy:** Local models keep all audio capture and transcription strictly on your machine; cloud providers send audio directly to your configured provider endpoint.

---

## Requirements

- **Compositor:** A Wayland compositor supporting `wlr-layer-shell` (e.g. Hyprland, Sway) for `speechyyy-dock`. GNOME's Mutter does not implement layer-shell, so the dock will not display there, but daemon dictation functions normally.
- **Clipboard Utility:** `wl-clipboard` (`wl-copy`) for default clipboard paste mode.
- **Audio Subsystem:** PipeWire or ALSA for microphone capture.
- **Uinput Permissions:** `/dev/uinput` write access is required to emit the simulated `Ctrl+V` paste keystroke or typewriter characters into the active window.

Configure uinput access via udev (no root group membership required):

```bash
sudo modprobe uinput
echo 'uinput' | sudo tee /etc/modules-load.d/uinput.conf > /dev/null
echo 'KERNEL=="uinput", SUBSYSTEM=="misc", OPTIONS+="static_node=uinput", TAG+="uaccess"' | sudo tee /etc/udev/rules.d/99-uinput.rules > /dev/null
sudo udevadm control --reload-rules
sudo udevadm trigger /dev/uinput
```

> **Note:** If `/dev/uinput` is not writable in your current session, log out and log back in for systemd `uaccess` ACLs to take effect. Do not add your user to the `input` group.

---

## Installation

### Automated Installer

Run the automated installer script:

```bash
bash -c "$(curl -fsSL https://raw.githubusercontent.com/AchalKumarSharma/speechyyy/main/install.sh)"
```

> **Note:** If the command prints nothing, the download failed; check your network connectivity. The installer reads prompts from `/dev/tty`, so `curl -fsSL ... | bash` is equally supported.

Supported environment variables:
- `SPEECHYYY_DRY_RUN=1`: Preview all installation steps without executing changes.
- `SPEECHYYY_API_KEY="<key>"`: Provide an initial API key non-interactively.
- `SPEECHYYY_CONFIG_DIR`: Override target configuration directory (default: `~/.config/speechyyy`).
- `SPEECHYYY_CONFIG_FILE`: Override target configuration file path (default: `~/.config/speechyyy/config.toml`).

### Building from Source

```bash
git clone https://github.com/AchalKumarSharma/speechyyy.git
cd speechyyy
cargo build --release

mkdir -p ~/.local/bin
install -m 755 target/release/speechyyy ~/.local/bin/
install -m 755 target/release/speechyyy-dock ~/.local/bin/
```

#### Shared Libraries (`sherpa-rs`)
Local model inference dynamically links against `libsherpa-onnx-c-api.so` and `libonnxruntime.so`. Copy them to `~/.local/bin/` or ensure they reside in your dynamic linker search path (such as `/usr/local/lib` or `LD_LIBRARY_PATH`):

```bash
install -m 755 target/release/*.so ~/.local/bin/
```

Verify shared library resolution:

```bash
ldd ~/.local/bin/speechyyy
```

---

## Quick Start

1. **Check Available Models:**
   ```bash
   speechyyy models
   ```

2. **Activate an Engine:**
   - **Local Model (Offline):**
     Download Parakeet-TDT v3 (~487 MB; verified against pinned SHA-256 before extraction):
     ```bash
     speechyyy download parakeet-tdt-v3
     speechyyy use parakeet-tdt-v3
     ```
   - **Cloud Provider (BYOK):**
     Configure Groq, Sarvam, OpenAI, or an OpenAI-compatible endpoint:
     ```bash
     speechyyy use groq
     ```
     Prompts for your API key and saves it securely to `~/.config/speechyyy/config.toml` with `0600` permissions. Environment variables (`GROQ_API_KEY`, `SARVAM_API_KEY`, `OPENAI_API_KEY`, `OPENAI_COMPATIBLE_API_KEY`) work as fallbacks, but saved keys are recommended because compositor and systemd environments do not inherit shell exports.

3. **Start Speechyyy:**
   ```bash
   speechyyy start
   ```
   Launches both `speechyyy` daemon and `speechyyy-dock` detached in the background.

4. **Verify System Health:**
   ```bash
   speechyyy doctor --deep
   ```

---

## Compositor Keybindings

### Hyprland (`~/.config/hypr/hyprland.conf`)

```ini
# Start daemon and companion dock on compositor startup
exec-once = speechyyy start

# Push-to-Talk via RightAlt (Keycode 108)
bind = , code:108, exec, speechyyy push-down
bindr = , code:108, exec, speechyyy push-up

# Toggle mode alternative:
# bind = , code:108, exec, speechyyy toggle
```

### Sway (`~/.config/sway/config`)

```ini
# Start daemon on compositor startup
exec speechyyy start

# Push-to-Talk via RightAlt (Mod3)
bindsym Mod3 exec speechyyy push-down
bindsym --release Mod3 exec speechyyy push-up

# Toggle mode alternative:
# bindsym Mod3 exec speechyyy toggle
```

> **Note:** Toggle mode works with any launcher or desktop shortcut that fires on keypress. Push-to-talk requires a compositor that supports key-release bindings (`bindr` on Hyprland, `--release` on Sway).

---

## CLI Reference

### Lifecycle

| Command | Arguments / Flags | Description |
|---|---|---|
| `speechyyy start` | `-f`, `--foreground`<br>`--provider <PROVIDER>`<br>`--mode <MODE>`<br>`--injection <INJECTION>`<br>`--typing-delay <MS>`<br>`--no-dock`<br>`--with-dock` | Start Speechyyy in background (or foreground with `-f`). Launches `speechyyy-dock` unless `--no-dock` is specified. |
| `speechyyy stop` | — | Terminate running daemon and companion dock processes cleanly. |
| `speechyyy status` | — | Query active daemon process ID, model, and runtime state. |

### Models & Providers

| Command | Arguments / Flags | Description |
|---|---|---|
| `speechyyy models` | `list`, `download`, `use`, `remove` | Display the model roster table. (Alias: `model`). |
| `speechyyy download` | `[MODEL]`<br>`-m, --model <MODEL>`<br>`-f, --force` | Download, hash-verify, and unpack local model weights. |
| `speechyyy use` | `<MODEL>` | Switch active model and hot-reload the running daemon. |
| `speechyyy remove` | `<MODEL>` | Delete local model files to reclaim disk space. (Alias: `rm`). |

### Actions

| Command | Arguments / Flags | Description |
|---|---|---|
| `speechyyy toggle` | — | Toggle recording state on or off. Debounced to prevent keybounce. |
| `speechyyy push-down` | — | Trigger push-to-talk key press (starts recording). |
| `speechyyy push-up` | — | Trigger push-to-talk key release (stops recording and transcribes). |
| `speechyyy cancel` | — | Discard current recording or audio processing without emitting text. |
| `speechyyy learn` | `[WRONG] [CORRECT]` | Add a phonetic replacement rule to `dictionary.toml`. |
| `speechyyy fix` | `[CORRECTED_PHRASE]` | Compute diff against last utterance and save phonetic replacement. |
| `speechyyy unlearn` | `[KEY]` | Remove a replacement rule from `dictionary.toml`. |

### Diagnostics & Maintenance

| Command | Arguments / Flags | Description |
|---|---|---|
| `speechyyy doctor` | `--deep` | Audit audio inputs, `/dev/uinput`, dock binary, layer-shell, and model files. `--deep` verifies SHA-256 checksums. |
| `speechyyy events` | — | Stream real-time daemon events and 40 Hz RMS audio metering in NDJSON. |
| `speechyyy history` | `-l, --limit <LIMIT>` | View recent dictation transcripts (default: 10 records). |
| `speechyyy update` | `--force` | Self-update binaries in-place with staged swap and rollback. |
| `speechyyy uninstall` | `-y, --yes` | Cleanly stop processes and remove binaries, models, and service units. |

---

## Configuration

Configuration resides at `~/.config/speechyyy/config.toml` with restricted permissions (`0600`).

```toml
hotkey = "RightAlt"
pacing_delay_ms = 15
injection_mode = "clipboard"

[input]
hotkey = "RightAlt"
grab_key = true

[audio]
sample_rate = 16000
vad_threshold = 0.005
min_duration_ms = 200

[inference]
provider = "local"
api_key = ""
model = "parakeet-tdt-v3"
language_code = "unknown"
mode = "translit"

[injector]
method = "clipboard"
pacing_delay_ms = 15

[ui]
nickname = "babe"
```

### Configuration Options

| Key | Section | Type | Default | Description |
|---|---|---|---|---|
| `hotkey` | root | string | `"RightAlt"` | Trigger key name used in logs and doctor diagnostics. |
| `pacing_delay_ms` | root | integer | `15` | Default inter-key delay in milliseconds. |
| `injection_mode` | root | string | `"clipboard"` | Delivery mode: `"clipboard"` (`wl-copy` + `Ctrl+V`) or `"typewriter"` (`/dev/uinput`). |
| `hotkey` | `[input]` | string | `"RightAlt"` | Key mapping name. |
| `grab_key` | `[input]` | boolean | `true` | Legacy grab flag. |
| `sample_rate` | `[audio]` | integer | `16000` | Audio capture sample rate in Hz. |
| `vad_threshold` | `[audio]` | float | `0.005` | Voice activity detection RMS threshold. |
| `min_duration_ms` | `[audio]` | integer | `200` | Minimum recording duration to process. |
| `provider` | `[inference]` | string | `"local"` | Inference engine: `"local"`, `"sarvam"`, `"groq"`, `"openai"`, `"openai-compatible"`, `"custom"`. |
| `api_key` | `[inference]` | string | `""` | Sarvam AI API key. |
| `model` | `[inference]` | string | `"saaras:v4"` | Active model identifier or checkpoint. |
| `language_code` | `[inference]` | string | `"unknown"` | Language identifier for supported cloud engines. |
| `mode` | `[inference]` | string | `"translit"` | Sarvam operation mode (`"translit"`, `"transcribe"`, `"translate"`, `"codemix"`). |
| `groq_api_key` | `[inference]` | string | `null` | Groq API key (`GROQ_API_KEY` fallback). |
| `openai_api_key` | `[inference]` | string | `null` | OpenAI API key (`OPENAI_API_KEY` fallback). |
| `openai_compatible_endpoint` | `[inference]` | string | `null` | HTTP base URL for OpenAI-compatible transcription server. |
| `openai_compatible_model` | `[inference]` | string | `null` | Model name parameter for OpenAI-compatible endpoint. |
| `openai_compatible_api_key` | `[inference]` | string | `null` | Optional authorization key for OpenAI-compatible endpoint. |
| `language` | `[inference]` | string | `null` | ISO language parameter for Groq / OpenAI endpoints. |
| `request_timeout_secs` | `[inference]` | integer | `null` | Network timeout for cloud HTTP requests in seconds. |
| `method` | `[injector]` | string | `"clipboard"` | Injection method alias (`"clipboard"` or `"typewriter"`). |
| `pacing_delay_ms` | `[injector]` | integer | `15` | Typing delay for typewriter mode. |
| `nickname` | `[ui]` | string | `"babe"` | Personalized terminal greeting name. |

---

## Known Limitations

- **Layer-Shell Compositor Support:** `speechyyy-dock` relies on the Wayland `zwlr_layer_shell_v1` protocol. It operates on compositors like Hyprland and Sway, but will not display on GNOME Mutter (which lacks layer-shell). Dictation and CLI controls still function without the dock.
- **Terminal Pasting in Clipboard Mode:** Default clipboard mode pastes using synthetic `Ctrl+V`. Standard terminal emulators require `Ctrl+Shift+V` to paste and will ignore `Ctrl+V`. Set `injection_mode = "typewriter"` if you primarily dictate into raw terminals.
- **Typewriter Mode Keystrokes:** Typewriter mode emits character-by-character key events directly via `/dev/uinput`. If no text field is focused, keystrokes may be interpreted as application shortcuts (e.g. browser navigation keys).
- **Clipboard History:** Clipboard injection copies the transcription directly into the active clipboard via `wl-copy`. It does not stash or restore prior clipboard contents, avoiding restore-timing race conditions.
- **Cloud Audio Transmission:** Local models process speech completely offline on your device. Choosing a cloud provider (`sarvam`, `groq`, `openai`, `openai-compatible`) transmits audio data over HTTPS to external servers.
- **X11 Display Support:** Daemon recording and typewriter text injection function on X11 sessions, but `speechyyy-dock` is a Wayland layer-shell application and exits immediately if `$WAYLAND_DISPLAY` is not found.

---

## License & Third-Party Attributions

- **Code License:** [MIT License](LICENSE) &copy; 2026 Achal Kumar Sharma.
- **Model Checkpoints:** Local model weights (Parakeet-TDT, Whisper, Moonshine) are licensed under their respective authors' licenses (CC-BY-4.0, MIT). Detailed license URLs, hashes, and upstream citations are documented in [`THIRD_PARTY_MODELS.md`](THIRD_PARTY_MODELS.md).
