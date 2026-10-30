# core-push-to-talk Specification

## Purpose

The `core-push-to-talk` capability provides an ultra-lightweight desktop Push-to-Talk loop and virtual caret text injection on Linux (Wayland and X11). It bypasses Wayland's security isolation by reading raw kernel input events directly via passive `evdev` monitoring, manages the daemon lifecycle through line-delimited JSON IPC over a local Unix Domain Socket, and simulates low-latency keystroke injection into focused windows using a persistent `/dev/uinput` virtual keyboard device.

## Requirements

### Requirement: Daemon Lifecycle and IPC Management

The system MUST provide standalone background process management controllable via `start`, `stop`, and `status` CLI commands communicating over a local Unix Domain Socket.

#### Scenario: Starting the daemon in background
- **Given** no existing daemon process is running
- **When** the user executes `speechyyy start`
- **Then** a background process is detached
- **And** a PID file is written at `$XDG_RUNTIME_DIR/speechyyy/speechyyy.pid`
- **And** a Unix Domain Socket is bound at `$XDG_RUNTIME_DIR/speechyyy/speechyyy.sock`
- **And** parent directories are created automatically if missing.

#### Scenario: Running in foreground mode
- **Given** the user provides the `--foreground` flag
- **When** the user executes `speechyyy start --foreground`
- **Then** the daemon executes in the foreground without detaching, logging events to standard output until terminated.

#### Scenario: Stopping the daemon gracefully
- **Given** an active daemon is running
- **When** the user executes `speechyyy stop`
- **Then** a stop command is delivered over the Unix Domain Socket
- **And** the daemon performs clean shutdown, removing the PID file and socket node.

#### Scenario: Querying runtime status
- **Given** an active daemon is running
- **When** the user executes `speechyyy status`
- **Then** the daemon replies with its current PID, state (`Idle`, `Recording`, `Processing`, `Emitting`), active hotkey, and pacing delay in milliseconds.

#### Scenario: Enforcing single-instance execution
- **Given** an active daemon is already running
- **When** the user attempts to execute `speechyyy start` a second time
- **Then** the command exits with code 1 and displays an informative message stating the daemon is already running.

---

### Requirement: System Permissions and Pre-flight Inspection

The system MUST verify that required input group memberships and device node permissions are present before starting, providing self-guided diagnosis.

#### Scenario: Inspecting permissions via doctor
- **When** the user executes `speechyyy doctor`
- **Then** the system inspects `/dev/input/event*` read access, `/dev/uinput` write access, and `input`/`uinput` group memberships
- **And** outputs clear Pass/Fail statuses alongside exact remediation commands and proposed udev rules.

#### Scenario: Fast-failing at daemon launch
- **Given** user permissions for `/dev/input/event*` or `/dev/uinput` are absent
- **When** the user executes `speechyyy start`
- **Then** the daemon fails fast with an informative error prompting the user to run `speechyyy doctor`.

---

### Requirement: Global Push-to-Talk Key Chord Sniffing

The system MUST passively monitor Linux input devices to detect push-to-talk press and release transitions without grabbing or blocking regular keyboard keys.

#### Scenario: Single-key Right Alt Push-to-Talk
- **Given** the default configuration (`hotkey = "RightAlt"`)
- **When** the user presses and holds Right Alt
- **Then** the session transitions from `Idle` to `Recording`
- **When** the user releases Right Alt
- **Then** the session transitions through `Processing` and `Emitting`, then returns to `Idle`.

#### Scenario: Compound key chord tracking
- **Given** a compound chord is configured (such as `Ctrl+Space`)
- **When** the user holds `Ctrl` and presses `Space`
- **Then** the session transitions to `Recording`
- **When** either the trigger key or modifier key is released
- **Then** the session completes and fires key release.

#### Scenario: Hotplug and device reconnect
- **Given** the daemon is actively running
- **When** a USB keyboard is plugged in or a device resumes from sleep
- **Then** an `inotify` watcher on `/dev/input/` detects the new device node and adds it to the active listener pool.

---

### Requirement: Caret Keystroke Injection with Pacing Delay

The system MUST emit synthesized keystrokes into the active focused window using a persistent kernel virtual keyboard device.

#### Scenario: Injecting text on key release
- **Given** a completed dictation session
- **When** the session reaches the `Emitting` state
- **Then** characters are translated into scancodes and dispatched through `/dev/uinput` into the focused window.

#### Scenario: Inter-keystroke pacing delay
- **Given** a configured pacing delay (default: 3ms)
- **When** keystroke pairs (keydown/keyup) are emitted
- **Then** a 3ms pause is enforced between transitions to prevent target application input buffer drops.

#### Scenario: Excluding virtual device from input sniffing
- **When** the daemon initializes its virtual keyboard device
- **Then** `EvdevListener` filters out any input node containing `"Speechyyy"` so the daemon never sniffs its own emitted keystrokes.
