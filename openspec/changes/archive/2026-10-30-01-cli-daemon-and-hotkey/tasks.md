# Tasks: 01-cli-daemon-and-hotkey

- [x] CLI argument parsing for `start`, `stop`, `status`, and `doctor` (`src/cli.rs`)
- [x] Pre-flight system permission inspector and udev generator (`src/doctor.rs`)
- [x] Background daemon lifecycle, atomic PID management, and line-delimited JSON IPC (`src/ipc.rs`, `src/daemon.rs`)
- [x] Key Chord tracker supporting single keys and modifier chords (`src/engine.rs`)
- [x] Decoupled four-state dictation machine with Mock engine (`src/engine.rs`)
- [x] Persistent `/dev/uinput` virtual keyboard injector with 3ms pacing delay (`src/injector.rs`)
- [x] Passive `evdev` event scanner with `inotify` reconnect watcher and self-device exclusion (`src/input.rs`)
- [x] Configuration file loader for `~/.config/speechyyy/config.toml` (`src/config.rs`)
- [x] End-to-end integration test suite verifying full push-to-talk loop (`tests/`)
- [x] Release binary size verification (< 10MB target, reached 1.3MB stripped)
