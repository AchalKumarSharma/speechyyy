# Kernel evdev Push-to-Talk (Superseded by ADR-0014)

> **Status: Superseded & Removed in v0.2.0**
> Direct `/dev/input/event*` hardware monitoring and device grabbing were replaced by compositor-level IPC keybindings (`speechyyy push-down`, `speechyyy push-up`, `speechyyy toggle`) under ADR-0014. The legacy `EvdevListener` and `EvdevGrabGuard` subsystem has been completely removed from `src/input.rs`.

Global push-to-talk hotkey detection on Wayland cannot rely on traditional X11 hotkey hooks or standard windowing protocols due to compositor security isolation. We decided to read Linux input events directly from `/dev/input/event*` devices using passive monitoring (without `EVIOCGRAB`), combined with an `inotify` watcher on `/dev/input` for device reconnection and sleep/resume recovery. This provides compositor-agnostic global keypress and keyrelease detection without blocking regular modifier events or locking out the keyboard.

## Considered Options

- **XDG Desktop Portal (`GlobalShortcuts`)**: Rejected because portal implementations vary significantly across compositors and typically do not expose key-release events required for push-to-talk.
- **Exclusive Grab (`EVIOCGRAB`)**: Originally rejected in v0.1 due to risk of process hang locking out keyboards. Re-evaluated and adopted under scoped safeguards in v0.2.0 (see Amendment below).

## Amendment (v0.2.0): Scoped EVIOCGRAB with RAII Guard & Watchdog

In v0.2.0, purely passive monitoring was found to leak push-to-talk chords (such as `RightAlt`, `Alt+Space`) to window managers and focused applications, triggering desktop collisions (e.g., KRunner, Alt-drag, application menu accelerators).

To isolate desktop input without risking keyboard lockups:
1. `EVIOCGRAB` is engaged exclusively during active push-to-talk chords (`DictationState::Recording`) and released immediately upon key release or text injection.
2. The grab lifecycle is managed via an RAII `EvdevGrabGuard`. If the daemon, recording loop, or thread exits unexpectedly or panics, `Drop` guarantees immediate device ungrab.
3. A safety watchdog automatically drops the grab if continuous recording exceeds 60 seconds.
4. Desktop environments or users who prefer passive monitoring can toggle `grab_key = false` under `[input]` in `config.toml`.

