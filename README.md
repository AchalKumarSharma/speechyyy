# Speechyyy (`v0.2.0`)

[![GitHub Release](https://img.shields.io/github/v/release/AchalKumarSharma/speechyyy?style=flat-square&color=blue)](https://github.com/AchalKumarSharma/speechyyy/releases)
[![Release Downloads](https://img.shields.io/github/downloads/AchalKumarSharma/speechyyy/total?style=flat-square&color=success)](https://github.com/AchalKumarSharma/speechyyy/releases)
[![GitHub Stars](https://img.shields.io/github/stars/AchalKumarSharma/speechyyy?style=flat-square&color=yellow)](https://github.com/AchalKumarSharma/speechyyy/stargazers)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg?style=flat-square)](LICENSE)

> **Sub-300ms ambient dictation daemon & native Wayland companion dock for Linux in safe Rust.**

Speechyyy is an open-source, ultra-lightweight desktop dictation engine written in Rust. It streams transcribed speech directly into the active OS cursor in real time via global push-to-talk.

---

## Quick Install (One-Liner)

Install the pre-compiled binary, configure udev permissions, and set up your desktop integration automatically:

```bash
bash -c "$(curl -fsSL https://raw.githubusercontent.com/AchalKumarSharma/speechyyy/main/install.sh)"
```

> **Tip:** The `curl -fsSL https://raw.githubusercontent.com/AchalKumarSharma/speechyyy/main/install.sh | bash` pipe form also works identically because the installer reads prompts directly from `/dev/tty`.  
> *Note:* If the command prints nothing, the download likely failed due to network issues; check your internet connectivity.

### What the installer handles:
- **Binary Deployment:** Installs checksum-verified (`SHA256SUMS`) `speechyyy` daemon and `speechyyy-dock` to `~/.local/bin/` (`0755`).
- **Hardware Access:** Automatically sets up `/etc/udev/rules.d/99-uinput.rules` and kernel modules for non-root `/dev/uinput` keystroke injection.
- **Secure Configuration:** Generates `~/.config/speechyyy/config.toml` locked to user-only read/write (`chmod 600`).
- **Optional API Key:** Interactively prompts for your Sarvam AI API Key (hidden input). You can press <kbd>Enter</kbd> to skip and add it later in `~/.config/speechyyy/config.toml`.
- **Desktop Autostart:** Automatically configures Hyprland (`exec-once` & keybinding in `hyprland.conf`) and generates systemd user services (`speechyyy.service`, `speechyyy-dock.service`).
- **Dry-Run & Non-Interactive:** Inspect actions beforehand using `SPEECHYYY_DRY_RUN=1 bash install.sh`, or pass `SPEECHYYY_API_KEY="your-key"` for automated CI setups.

---

## Architecture & Internals

Speechyyy decouples background audio capture and inference from UI rendering via dual local UNIX domain sockets:

- **Control IPC (`speechyyy.sock`)**: Lightweight request-reply socket for hotkey press/release triggers (`push-down`, `push-up`, `toggle`, `cancel`) and status checks.
- **Telemetry Stream (`events.sock`)**: Non-blocking NDJSON event broadcaster emitting real-time dictation lifecycle changes (`Recording`, `Processing`, `Emitting`, `Error`) alongside **40 Hz atomic RMS audio level telemetry**.
- **Wayland Layer-Shell UI (`speechyyy-dock`)**: Standalone companion dock rendered using `smithay-client-toolkit` (`wlr-layer-shell`) and `tiny-skia` software rasterization.
- **Zero Telemetry & Sub-30 MB Footprint**: Standalone binary compiled in Rust (< 10 MB binary, < 30 MB idle RAM, 0.00% idle CPU draw).

```
[ Microphone ] ──> ( PipeWire / ALSA ) ──> [ speechyyy daemon ]
                                                 │       │
                       control (`speechyyy.sock`)│       │ telemetry (`events.sock`)
                                                 ▼       ▼
                                         [ Keybindings ] [ speechyyy-dock ] ──> ( wlr-layer-shell )
                                                 │
                                                 ▼
                                        [ /dev/uinput ] ──> Active Caret / Focused Window
```

---

## UX & Aesthetic Identity

- **Neon-Synth Palette**: High-contrast Apple HIG capsule geometry bordered in neon pink (`#f70776`) with deep magenta (`#c3195d`) reactive waveform bars and pulsing processing dots over a dark glassmorphism canvas (`rgba(18, 18, 20, 0.85)`).
- **Interactive Equalizer**: Multi-bar logarithmic Gaussian envelope with micro-phase harmonics dancing dynamically in real time to microphone volume.
- **Transparent Quiescent Surface**: When parked idle, the companion dock maintains its active Wayland configured surface while committing a single transparent frame, ceasing frame callbacks to preserve **0.00% idle CPU** and 100% click-through input transparency.
- **Instantaneous Collapse**: Snaps the exit spring immediately upon text emission (`DictationState::Emitting`), making text insertion feel instant.

---

## Quick Start

1. **Diagnose Your Setup**:
   ```bash
   speechyyy doctor
   ```

2. **Select & Configure a Model**:
   - **Local Offline Models** (Sherpa-ONNX):
     ```bash
     speechyyy download parakeet-tdt-v3
     speechyyy use parakeet-tdt-v3
     ```
   - **Cloud BYOK Models** (Sarvam AI 22+ Indic / Hinglish transliteration):
     ```bash
     speechyyy download saaras:v4
     speechyyy use saaras:v4
     ```

3. **Start the Daemon & Dock**:
   ```bash
   speechyyy daemon &
   speechyyy-dock &
   ```

4. **Dictate**:
   - Focus any text input in any application (editor, browser, terminal).
   - Hold <kbd>RightAlt</kbd>, speak, and release.
   - Text streams directly into your caret!

---

## CLI Command Reference

Speechyyy provides a fast CLI interface for both daily usage and window manager bindings:

| Command | Description |
|---|---|
| `speechyyy daemon [-f]` | Start the background daemon (alias: `start`). Pass `-f` / `--foreground` for live logging |
| `speechyyy toggle` | IPC trigger to toggle recording on/off (ideal for single-key shortcuts) |
| `speechyyy doctor` | Audit system permissions (`/dev/uinput`), audio inputs (PipeWire), Wayland session, and models |
| `speechyyy status` | Query active daemon status over `speechyyy.sock` |
| `speechyyy events` | Stream live NDJSON state changes and 40 Hz audio RMS metering over `events.sock` |
| `speechyyy stop` | Terminate the active Speechyyy daemon process |
| `speechyyy cancel` | Cancel current recording or processing without text insertion |
| `speechyyy push-down` | Start recording via IPC (bind to hotkey press event) |
| `speechyyy push-up` | Stop recording & transcribe via IPC (bind to hotkey release event) |
| `speechyyy model` | List all supported speech recognition models (alias: `models`) |
| `speechyyy download <MODEL>` | Fetch weights or configure credentials for a model preset |
| `speechyyy use <MODEL>` | Switch active model with automatic hot-reload |
| `speechyyy remove <MODEL>` | Delete local model weights to reclaim disk space (alias: `rm`) |
| `speechyyy learn <HEARD> <TYPED>` | Add a phonetic replacement rule to `dictionary.toml` |
| `speechyyy fix <EXPECTED>` | Calculate diff against last utterance and store replacement rule |
| `speechyyy unlearn [KEY]` | Remove a replacement rule from `dictionary.toml` |
| `speechyyy history` | View recent dictation transcripts |
| `speechyyy update [--force]` | Self-update to the newest GitHub release in-place |
| `speechyyy uninstall [-y]` | Autonomous complete removal of binaries, weights, and configs |

---

## Compositor Keybindings

Configure push-to-talk in your Wayland compositor:

### Hyprland (`~/.config/hypr/hyprland.conf`)
```ini
# Start daemon and dock on login
exec-once = speechyyy daemon
exec-once = speechyyy-dock

# Push-to-talk with RightAlt:
bind = , code:108, exec, speechyyy push-down
bindr = , code:108, exec, speechyyy push-up

# Or toggle with RightAlt:
# bind = , code:108, exec, speechyyy toggle
```

### Sway / i3 (`~/.config/sway/config`)
```ini
exec speechyyy daemon
exec speechyyy-dock

bindsym Mod3 exec speechyyy push-down
bindsym --release Mod3 exec speechyyy push-up
```

---

<details>
<summary><strong>Manual Setup & Build from Source</strong></summary>

### 1. Prerequisites
- Rust 2021 Edition (`cargo`, `rustc`)
- PipeWire / ALSA audio development libraries
- Wayland development headers

On Arch / CachyOS:
```bash
sudo pacman -S base-devel pipewire pipewire-pulse wayland
```

### 2. Build Release Binaries
```bash
git clone https://github.com/AchalKumarSharma/speechyyy.git
cd speechyyy
cargo build --release --bin speechyyy --bin speechyyy-dock

mkdir -p ~/.local/bin
install -m 755 target/release/speechyyy ~/.local/bin/
install -m 755 target/release/speechyyy-dock ~/.local/bin/
```

### 3. Configure `/dev/uinput` Permissions
Hardware keystroke injection requires non-root write access to `/dev/uinput`:

```bash
sudo modprobe uinput
echo 'uinput' | sudo tee /etc/modules-load.d/uinput.conf > /dev/null
echo 'KERNEL=="uinput", SUBSYSTEM=="misc", OPTIONS+="static_node=uinput", TAG+="uaccess"' | sudo tee /etc/udev/rules.d/99-uinput.rules > /dev/null
sudo udevadm control --reload-rules
sudo udevadm trigger /dev/uinput
```

*(Note: Log out and log back in for `uaccess` ACLs to take effect on your user session).*

### 4. Create Configuration File
Create `~/.config/speechyyy/config.toml` with restricted user-only permissions:

```bash
mkdir -p ~/.config/speechyyy
(
    umask 077
    cat > ~/.config/speechyyy/config.toml << 'EOF'
hotkey = "RightAlt"
pacing_delay_ms = 15

# Injection mode:
# - "clipboard" (default): fast paste via Ctrl+V; leaves transcription in clipboard and immune to browser hotkey hijacking. (Requires wl-clipboard on Wayland; does not paste in standard terminals).
# - "typewriter": character-by-character typing via /dev/uinput. Works in terminals, but triggers page shortcuts if no text field is focused.
injection_mode = "clipboard"

[input]
hotkey = "RightAlt"
grab_key = true

[audio]
sample_rate = 16000
vad_threshold = 0.005
min_duration_ms = 200

[inference]
provider = "sarvam"
api_key = ""
model = "saaras:v4"
language_code = "unknown"
mode = "translit"

[injector]
method = "clipboard"
pacing_delay_ms = 15

[ui]
nickname = "babe"
EOF
)
chmod 600 ~/.config/speechyyy/config.toml
```

### Injection Modes (`injection_mode`)

Speechyyy supports two delivery modes for inserting dictation into focused windows:

| Mode | Mechanism | Best For | Behavior & Known Limitations |
|---|---|---|---|
| `clipboard` *(default)* | `wl-copy` + `Ctrl+V` | Browsers, web apps, desktop editors, Electron apps | **Wispr Flow pattern:** Instant direct paste via simulated `Ctrl+V`. Immune to browser hotkey hijacking (e.g. YouTube playback toggling when no text field is focused). **Known limits:** Overwrites user's active clipboard item with transcript; does not paste into standard terminal emulators or Vim normal mode (which require `Ctrl+Shift+V` or `p`). |
| `typewriter` | `/dev/uinput` virtual keyboard | Terminal emulators, CLI tools, Vim/Neovim | Emits keystrokes sequentially character-by-character via kernel `/dev/uinput`. Works in terminals. **Known limits:** If no text field is focused, keystrokes may be interpreted as application hotkeys (e.g. page shortcuts). |

</details>

---

## License

MIT License. Crafted with safe Rust.
