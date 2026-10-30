# Kernel evdev Push-to-Talk

Global push-to-talk hotkey detection on Wayland cannot rely on traditional X11 hotkey hooks or standard windowing protocols due to compositor security isolation. We decided to read Linux input events directly from `/dev/input/event*` devices using passive monitoring (without `EVIOCGRAB`), combined with an `inotify` watcher on `/dev/input` for device reconnection and sleep/resume recovery. This provides compositor-agnostic global keypress and keyrelease detection without blocking regular modifier events or locking out the keyboard.

## Considered Options

- **XDG Desktop Portal (`GlobalShortcuts`)**: Rejected because portal implementations vary significantly across compositors and typically do not expose key-release events required for push-to-talk.
- **Exclusive Grab (`EVIOCGRAB`)**: Rejected because grabbing locks the entire keyboard, swallowing regular typing and risking an input lockup if the process hangs.

