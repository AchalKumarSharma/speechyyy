# 06: End-to-End Push-to-Talk Daemon Assembly

**What to build:** Complete assembly of the Push-to-Talk daemon. Wires the evdev event stream, inotify watcher, key chord tracker, dictation state machine, mock engine, and persistent uinput injector into the background daemon loop. Loads user configuration from `~/.config/speechyyy/config.toml` (with built-in defaults) and delivers the end-to-end Sprint 1 experience: running `speechyyy start`, holding Right Alt, releasing it, and seeing "Hello from Speechyyy" injected at the active cursor position.

**Blocked by:** 02: Daemon Lifecycle & Line-Delimited JSON IPC, 04: Persistent uinput Virtual Keyboard Injector, 05: evdev Input Listener & inotify Reconnect Watcher

**Status:** resolved

- [x] TOML configuration loader reading `~/.config/speechyyy/config.toml` with default fallback for hotkey and pacing delay.
- [x] Daemon event loop coordinating input events through `KeyChordTracker` -> `DictationState` -> `TextInjector`.
- [x] Detached background process spawning when running `speechyyy start` without `--foreground`.
- [x] End-to-end automated test utilizing `FakeInputSource` and `RecordingInjector` exercising the complete daemon loop.
- [x] Manual smoke test verification on Wayland (Arch/CachyOS/Omarchy) holding Right Alt and observing caret injection.
- [x] Binary size check verifying release target remains well under 10 MB.
