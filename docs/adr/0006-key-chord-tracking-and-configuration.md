# Key Chord Tracking and Configuration

Push-to-Talk activation must support both single key triggers (e.g. Right Alt) and compound modifier combinations (e.g. Ctrl+Space) configured by the user. We decided to implement a stateful `KeyChordTracker` that tracks modifier bitmasks across `evdev` events, defaulting to `KEY_RIGHTALT` and loading overrides from `~/.config/speechyyy/config.toml` (falling back to built-in defaults if absent). This handles modifier state transitions uniformly without requiring external shortcut daemons.

## Considered Options

- **Single-key only trigger**: Rejected because power users on different keyboards and laptop layouts require modifier combinations like `Ctrl+Space` or `Super+Space`.
- **System shortcut daemon delegation (e.g. Hyprland/Sway bindsym)**: Rejected because relying on the window manager's keybindings breaks cross-desktop portability and cannot guarantee accurate key-release detection for push-to-talk.

