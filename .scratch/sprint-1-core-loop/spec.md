# Sprint 1: Core Push-to-Talk Daemon Loop

## Problem Statement

Users on modern Linux desktop environments (Wayland and X11 on Arch, CachyOS, Omarchy) need an ultra-fast, local dictation workflow that inputs text directly into whatever application is currently focused. However, Wayland security isolation deliberately blocks global key-sniffing and synthetic input injection from unprivileged client applications. Before integrating audio capture or cloud/local inference backends, we need to prove that global push-to-talk key interception, daemon process lifecycle, and virtual caret text injection function reliably with zero latency overhead.

## Solution

Build a standalone, lightweight CLI daemon written in Rust that provides the complete push-to-talk trigger loop and virtual caret text injection. The daemon reads global input events directly from Linux kernel input devices via passive evdev monitoring, tracks the user's push-to-talk Key Chord, drives an explicit four-state session machine, and on key release injects a mock text payload ("Hello from Speechyyy") into the focused window using a persistent kernel virtual keyboard device with a 3ms pacing delay. The daemon is managed via line-delimited JSON IPC over a Unix Domain Socket, and includes a doctor command to guide system permissions.

## User Stories

1. As a user, I want to run `speechyyy start` in the terminal, so that the background daemon starts and begins listening for my push-to-talk hotkey without blocking my shell.
2. As a user, I want to pass a `--foreground` flag to `speechyyy start`, so that I can easily view debug logs or manage the process directly under systemd.
3. As a user, I want to run `speechyyy stop`, so that the running daemon receives a stop command over the local socket and cleanly shuts down.
4. As a user, I want to run `speechyyy status`, so that I can verify whether the daemon is actively running and inspect its PID and active configuration.
5. As a user, I want `speechyyy start` to enforce single-instance execution, so that attempting to start a second daemon while one is already active fails immediately with an informative message.
6. As a user, I want to press and hold Right Alt (default Key Chord), so that the daemon transitions from Idle to Recording without capturing or blocking system modifiers.
7. As a user, I want to release Right Alt, so that the daemon transitions from Recording through Processing to Emitting and injects "Hello from Speechyyy" at my current caret.
8. As a user, I want to configure an alternative Key Chord (such as Ctrl+Space) in `~/.config/speechyyy/config.toml`, so that I can use my preferred keyboard combination.
9. As a user, I want the daemon to maintain a persistent virtual keyboard device, so that text emission begins instantaneously on key release without device discovery delay.
10. As a user, I want synthesized keystrokes to be paced with a 3ms delay, so that fast applications, terminals, and Electron windows do not drop characters.
11. As a user, I want hotplugged or reconnected USB keyboards to be detected via an inotify watcher, so that dictation continues working after waking from sleep or replugging devices.
12. As a user, I want to run `speechyyy doctor`, so that I can verify my user's group permissions (`input`, `uinput`) and device access before launching the daemon.
13. As a user, I want `speechyyy doctor` to output the exact commands or udev rule (`/etc/udev/rules.d/99-speechyyy.rules`) needed to grant access, so that setup on Arch/CachyOS/Omarchy is frictionless.
14. As a developer, I want the dictation logic isolated behind a `DictationEngine` trait, so that Sprint 2 can swap mock text generation for audio capture and streaming inference without touching input or injection code.

## Implementation Decisions

- **Process Model & IPC**: The daemon runs as a detached background process (or foreground with `--foreground`). It binds a Unix Domain Socket and writes a PID file in `$XDG_RUNTIME_DIR/speechyyy/` (falling back to `/tmp/speechyyy-$UID/`). IPC commands and status telemetry use line-delimited JSON messages (`{"cmd":"stop"}\n`, `{"cmd":"status"}\n`).
- **Global Input Sniffing**: Uses passive kernel `evdev` event streaming from `/dev/input/event*` devices without exclusive grab (`EVIOCGRAB`). An `inotify` watcher monitors `/dev/input` for newly connected or reconnected keyboard devices.
- **Key Chord Tracking**: A stateful `KeyChordTracker` maintains modifier bitmasks and tracks key-down / key-up transitions, defaulting to `KEY_RIGHTALT` with support for compound chords like `Ctrl+Space`.
- **Dictation State Machine**: A four-state machine (`Idle -> Recording -> Processing -> Emitting`) driven by a `DictationEngine` trait decouples OS triggers from output generation. In Sprint 1, `MockDictationEngine` emits the static string `"Hello from Speechyyy"`.
- **Caret Text Injection**: A `TextInjector` trait manages a persistent kernel virtual keyboard via `/dev/uinput`, emitting scancodes with a 3ms inter-keystroke pacing delay. The interface provides for clipboard paste fallback (`wl-copy` + Ctrl+V) for complex Unicode characters.
- **Diagnostics & Setup**: A `Preflight` validator checks device node accessibility on startup. A `speechyyy doctor` CLI subcommand checks `/dev/input`, `/dev/uinput`, and user group memberships, displaying guided setup instructions.
- **Configuration**: Loads optional TOML configuration from `~/.config/speechyyy/config.toml` for hotkey binding and pacing delay, using built-in defaults if missing.

## Testing Decisions

- **What makes a good test**: Tests must verify observable behavior (state transitions, emitted text, and socket protocol contracts) rather than internal helper methods or mock expectations. Tests must run deterministically in unprivileged, headless environments.
- **Core Pipeline Seam**: The full Push-to-Talk loop will be tested through `FakeInputSource` and `RecordingInjector`, asserting that simulated key events produce expected state transitions and exact emitted text.
- **Key Chord Seam**: Unit tests feeding synthetic event sequences to `KeyChordTracker` covering single keys, compound chords, rapid press-release sequences, and modifier interleaving.
- **Daemon Socket Seam**: IPC integration tests verifying socket binding, single-instance rejection, command routing (`status`, `stop`), and graceful process termination over a temporary socket.
- **Prior Art**: Modern Rust CLI daemons using tokio/std UnixListener and evdev/uinput mocking patterns.

## Out of Scope

- Microphone audio capture, PCM ring buffering, and ALSA/PulseAudio/PipeWire integration (Sprint 2).
- Offline Sherpa-ONNX model inference and cloud WebSocket streaming (Sarvam AI / Groq) (Sprint 2).
- Multi-lingual text translation, phoneme mapping, and streaming token injection (Sprint 2).
- Floating desktop recording indicator, tray icon, or visual notifications (Sprint 3).

## Further Notes

- Target binary size remains strictly under 10 MB and idle RAM under 30 MB.
- Linux is the primary target for Sprint 1 (Arch/CachyOS/Omarchy on Wayland and X11), with code structured for subsequent macOS and Windows porting.

