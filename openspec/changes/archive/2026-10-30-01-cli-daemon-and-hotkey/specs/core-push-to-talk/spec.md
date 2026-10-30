# core-push-to-talk Specification Delta

## ADDED Requirements

### Requirement: Daemon Lifecycle and IPC Management
Provides standalone background process management controllable via `start`, `stop`, and `status` CLI commands communicating over a local Unix Domain Socket with line-delimited JSON messages.

### Requirement: System Permissions and Pre-flight Inspection
Verifies required input group memberships and device node permissions before startup, providing actionable fix instructions via `speechyyy doctor`.

### Requirement: Global Push-to-Talk Key Chord Sniffing
Passively monitors Linux input devices to detect push-to-talk press and release transitions without grabbing or blocking regular keyboard keys, dynamically monitoring new devices via `inotify`.

### Requirement: Caret Keystroke Injection with Pacing Delay
Emits synthesized keystrokes into the active focused window using a persistent kernel virtual keyboard device with a 3ms inter-keystroke pacing delay.
