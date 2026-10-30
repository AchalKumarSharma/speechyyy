# 05: evdev Input Listener & inotify Reconnect Watcher

**What to build:** Passive kernel input capture across all connected keyboard devices. Scans `/dev/input/event*`, identifies keyboard device nodes, streams raw input events without exclusive grab (`EVIOCGRAB`), and uses an `inotify` watcher on `/dev/input` to dynamically add newly connected or resumed keyboards to the listener pool.

**Blocked by:** 03: Key Chord Tracker & Dictation State Machine

**Status:** resolved

- [x] Linux `/dev/input/event*` discovery and keyboard capability detection via `evdev`.
- [x] Passive event reading from all active keyboards without exclusive grabbing.
- [x] `inotify` directory monitor on `/dev/input` detecting new device nodes (`IN_CREATE`) and adding them to the event stream.
- [x] Graceful handling of disconnected devices (`ENODEV`) without dropping the listener thread.
- [x] Automated tests for device discovery, capability filtering, and simulated disconnect/reconnect scenarios.
