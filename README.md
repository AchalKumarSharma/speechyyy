# Speechyyy

[![Latest Release](https://img.shields.io/github/v/release/AchalKumarSharma/speechyyy?color=f70776&style=flat-square)](https://github.com/AchalKumarSharma/speechyyy/releases/latest)
[![Total Downloads](https://img.shields.io/github/downloads/AchalKumarSharma/speechyyy/total?color=c3195d&style=flat-square)](https://github.com/AchalKumarSharma/speechyyy/releases)
[![GitHub Stars](https://img.shields.io/github/stars/AchalKumarSharma/speechyyy?color=yellow&style=flat-square)](https://github.com/AchalKumarSharma/speechyyy/stargazers)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square)](https://github.com/AchalKumarSharma/speechyyy/blob/main/LICENSE)

Speech-to-text dictation for Linux on Wayland.
Hold or toggle a shortcut key, speak, and transcribed text appears directly at your cursor.

---

## Before you start

- **Wayland compositor:** Needs `wlr-layer-shell` support for the dock overlay. Tested on Hyprland. Other wlroots-based compositors should work, untested. (GNOME Mutter and X11 lack layer-shell; the dock will not display there).
- **Clipboard tool:** `wl-copy` (from `wl-clipboard`) for default clipboard paste mode.
- **Input access:** Write permission to `/dev/uinput` to emit simulated typing keystrokes. Run `speechyyy doctor` to check your setup.
- **Microphone:** A working audio input device.

---

## Install

Run the automated installer:

```bash
bash -c "$(curl -fsSL https://raw.githubusercontent.com/AchalKumarSharma/speechyyy/main/install.sh)"
```

This installs `speechyyy` and `speechyyy-dock` into `~/.local/bin/` and creates `~/.config/speechyyy/config.toml`. The command also works piped (`curl -fsSL ... | bash`). To preview every step without making changes, set `SPEECHYYY_DRY_RUN=1`.

---

## First run

1. Check your environment and permissions:
   ```bash
   speechyyy doctor
   ```
2. Pick how you want to transcribe (more options below):

   **Offline, on your device:** Parakeet. It's a big download, so give it a minute.

   ```bash
   speechyyy download parakeet-tdt-v3
   speechyyy use parakeet-tdt-v3
   ```

   **Cloud, bring your own key:** Sarvam. This is the one to pick for Indian languages. Your audio is sent to Sarvam to be transcribed.

   ```bash
   speechyyy use sarvam
   ```

3. Start the engine:
   ```bash
   speechyyy start
   ```
   The companion dock appears automatically. Pass `--no-dock` if you want to run headless.
4. Focus any text field, hold your shortcut key, and speak.

---

## Daily use

### Compositor bindings

Add these lines to your Hyprland configuration (`~/.config/hypr/hyprland.conf`):

```ini
# Start daemon and companion dock on login
exec-once = speechyyy start

# Push-to-talk via Right Alt (Keycode 108)
bind = , code:108, exec, speechyyy push-down
bindr = , code:108, exec, speechyyy push-up

# Toggle mode alternative:
# bind = , code:108, exec, speechyyy toggle
```

Push-to-talk requires a shortcut that triggers on key release (`bindr`). If your desktop only supports keypress shortcuts, use `speechyyy toggle`.

### Where text goes

Speechyyy pastes via clipboard by default (`wl-copy` followed by a synthetic `Ctrl+V`). Standard terminal emulators ignore `Ctrl+V`; if you dictate into terminals, set `injection_mode = "typewriter"` in `config.toml` or start with `speechyyy start --injection typewriter`.

---

## Choose how it transcribes

### Cloud (bring your own key)

Audio is sent over HTTPS to your chosen provider for transcription. Sarvam is the pick for Indian languages.

| Provider  | Setup command                     | Environment key fallback    |
| --------- | --------------------------------- | --------------------------- |
| Sarvam AI | `speechyyy use sarvam`            | `SARVAM_API_KEY`            |
| Groq      | `speechyyy use groq`              | `GROQ_API_KEY`              |
| OpenAI    | `speechyyy use openai`            | `OPENAI_API_KEY`            |
| Custom    | `speechyyy use openai-compatible` | `OPENAI_COMPATIBLE_API_KEY` |

Enter your API key when prompted; it is saved with `0600` permissions to `config.toml`. You can also provide `SPEECHYYY_API_KEY="<key>"` during installation.

### Local (offline)

Runs completely on your machine with no network connection during dictation.

| Model                                            | Size                     | Setup command                                                         |
| ------------------------------------------------ | ------------------------ | --------------------------------------------------------------------- |
| `parakeet-tdt-v3`                                | Large download (~487 MB) | `speechyyy download parakeet-tdt-v3 && speechyyy use parakeet-tdt-v3` |
| `whisper-large-v3-turbo` (alias `whisper-turbo`) | Large download           | `speechyyy download whisper-turbo && speechyyy use whisper-turbo`     |
| `moonshine-base`                                 | Medium download          | `speechyyy download moonshine-base && speechyyy use moonshine-base`   |

View installed models with `speechyyy models`, and delete local model files with `speechyyy remove <model>`.

---

## Commands cheat-sheet

| Command                      | What it does                                                                          |
| ---------------------------- | ------------------------------------------------------------------------------------- |
| `speechyyy start`            | Starts daemon and dock. Use `-f` for foreground logs, `--no-dock` to skip dock.       |
| `speechyyy stop`             | Stops the running daemon and dock cleanly via SIGTERM.                                |
| `speechyyy status`           | Shows daemon PID, current state, and active model.                                    |
| `speechyyy doctor`           | Audits microphone, `/dev/uinput`, dock, and models. Add `--deep` to verify checksums. |
| `speechyyy push-down`        | Starts recording (press shortcut).                                                    |
| `speechyyy push-up`          | Stops recording and transcribes (release shortcut).                                   |
| `speechyyy toggle`           | Toggles recording state on or off.                                                    |
| `speechyyy cancel`           | Discards current audio without emitting text.                                         |
| `speechyyy events`           | Streams live daemon state and audio RMS levels as NDJSON.                             |
| `speechyyy history`          | Shows recent dictation transcripts (default: last 10).                                |
| `speechyyy models`           | Lists available and installed speech models.                                          |
| `speechyyy download <model>` | Downloads and verifies model weights.                                                 |
| `speechyyy use <model>`      | Switches active model and reloads the daemon.                                         |
| `speechyyy remove <model>`   | Deletes local model files to free disk space.                                         |
| `speechyyy update`           | Updates binaries in-place to the newest release.                                      |
| `speechyyy uninstall`        | Stops services and removes binaries, models, and config.                              |

Run `speechyyy --help` for flag details on any command.

- **Dock flag priority:** The dock launches by default in both background and foreground modes. `--no-dock` turns it off. `--with-dock` is accepted; if both are passed, `--no-dock` wins.
- **Timeouts:** Control requests (`status`, `stop`, `ping`) time out after 2 seconds so stuck states fail fast; commands waiting on transcription (`push-up`, `toggle`) wait up to 15 seconds.

---

## Update and uninstall

```bash
# Update to latest stable release
speechyyy update

# Completely remove Speechyyy
speechyyy uninstall
```

**Pre-releases:** GitHub's standard release feed skips pre-releases. To install or update to a release candidate (such as `v0.2.0-rc1`), pass the tag explicitly:

```bash
SPEECHYYY_VERSION=v0.2.0-rc1 bash -c "$(curl -fsSL https://raw.githubusercontent.com/AchalKumarSharma/speechyyy/main/install.sh)"
# Or update an existing install:
speechyyy update --tag v0.2.0-rc1
```

_Upgrade note:_ If you started a dock under a pre-v0.2.0 version before PID tracking existed, close it manually once (`killall speechyyy-dock`).

---

## Troubleshooting

- **The dock does not appear:** Verify you are running a Wayland compositor with `wlr-layer-shell` support. GNOME Mutter and X11 cannot display the dock. Run `speechyyy doctor` to inspect compositor compatibility.
- **No text is pasted:** In terminal emulators, `Ctrl+V` paste does not work; set `injection_mode = "typewriter"` in `config.toml` or paste manually with `Ctrl+Shift+V`. Outside terminals, verify `/dev/uinput` write access with `speechyyy doctor`.
- **Cloud provider errors:** Verify your API key in `~/.config/speechyyy/config.toml` or reconfigure it with `speechyyy use <provider>`.
- **`speechyyy stop` seems to pause:** If speech inference is currently running on your CPU, `stop` waits until decoding completes before terminating.

---

## Known limits

- The companion dock requires `wlr-layer-shell`; GNOME has none, and X11 is unsupported.
- Transcription is synchronous: during multi-second CPU decoding, `stop` and `cancel` wait until decoding completes.
- Clipboard mode overwrites your active clipboard and does not paste in most terminal emulators.
- Speechyyy cannot detect whether your cursor is in an active text input field.
- Cloud providers transmit audio over the network to external servers.
- Idle CPU usage is low but not zero (measured at approximately 0.8% to 1.1% on one tested system).

---

## Privacy

API keys and user options are stored in `~/.config/speechyyy/config.toml` with restricted `0600` permissions. Recent transcript history is stored locally in `~/.local/share/speechyyy/history.jsonl` (capped at 500 records, `0600` permissions) and is never sent to external servers. Terminal logs and standard output never print your speech transcripts.

---

## Under the hood

Speechyyy consists of two standalone binaries: `speechyyy` (daemon and CLI) and `speechyyy-dock` (ambient HUD overlay). Both are written in synchronous Rust with no async runtime and communicate over local Unix domain sockets in `$XDG_RUNTIME_DIR/speechyyy/`.
