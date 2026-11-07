# Compositor IPC Triggering

Global push-to-talk key interception on modern Wayland desktops (e.g. Hyprland, Sway, GNOME, River) historically required either reading raw `/dev/input/event*` devices or placing the user in the privileged `input` group. In v0.2.0, we transitioned push-to-talk trigger capture away from kernel evdev hardware polling to compositor-driven IPC commands (`speechyyy push-down`, `speechyyy push-up`, `speechyyy toggle`).

## Context & Motivation

1. **Security Isolation**: Reading `/dev/input/event*` requires blanket read access to all system input devices (a potential keylogger vector) or adding unprivileged users to the `input` group.
2. **Keyboard Locking Hazards**: Exclusive device grabs (`EVIOCGRAB`) carried risk of keyboard lockout if a daemon hang, panic, or ungrab failure occurred.
3. **Compositor Native Capabilities**: Modern Wayland compositors already provide native, unprivileged key-press and key-release bindings (such as `bind` and `bindr` in Hyprland, or `bindsym` and `bindsym --release` in Sway).

## Decision

We expose IPC commands over the daemon Unix Domain Socket:
- `speechyyy push-down`: Triggers `DictationEngine::start_recording()` upon hotkey press.
- `speechyyy push-up`: Triggers `DictationEngine::stop_and_transcribe()` and caret text injection upon hotkey release.
- `speechyyy toggle`: Toggles dictation state for desktop environments or window managers that lack separate key-release bindings.

The daemon runs an unprivileged event loop using `NoopInputSource`, completely bypassing `/dev/input/event*` iteration while retaining `/dev/uinput` solely for synthesized keystroke emission.

## Considered Options

- **Retaining Kernel evdev + EVIOCGRAB**: Rejected due to host security threat model (blanket `/dev/input/event*` access) and edge-case compositor focus/modifier collisions.
- **XDG GlobalShortcuts Portal**: Rejected because the portal specification lacks key-release events necessary for push-to-talk semantics.
