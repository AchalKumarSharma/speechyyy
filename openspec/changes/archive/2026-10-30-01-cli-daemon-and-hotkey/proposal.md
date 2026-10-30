# Proposal: CLI Daemon & Push-to-Talk Loop (Sprint 1)

## Why

Wayland compositors intentionally block unprivileged applications from intercepting global keystrokes or synthesizing input into arbitrary windows. To provide real-time dictation without slow wait-and-paste delays, Speechyyy must prove that global Push-to-Talk and caret injection function across Arch/CachyOS/Omarchy on Wayland & X11 with sub-10MB binary footprint before audio capture or AI inference models are attached.

## What Changes

- CLI interface with `start [--foreground]`, `stop`, `status`, and `doctor` commands.
- Standalone background daemon with Unix Domain Socket IPC and PID file under `$XDG_RUNTIME_DIR/speechyyy/`.
- Direct kernel `evdev` event monitoring with passive sniffing (no `EVIOCGRAB`) and `inotify` reconnection.
- Stateful `KeyChordTracker` supporting single keys (`RightAlt`) and compound chords (`Ctrl+Space`).
- Persistent `/dev/uinput` virtual keyboard injector with a 3ms inter-keystroke pacing delay.
- Decoupled four-state machine (`Idle -> Recording -> Processing -> Emitting`) driven by `DictationEngine`.

