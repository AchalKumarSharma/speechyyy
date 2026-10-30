# Daemon Lifecycle and IPC

Speechyyy must run as an independent user daemon controllable via `speechyyy start` and `speechyyy stop` without requiring systemd as a hard dependency. We decided to run a self-contained daemon process that binds a Unix Domain Socket and writes a PID file in `$XDG_RUNTIME_DIR/speechyyy/`, while providing a `--foreground` flag for debugging and service managers. This gives us zero-dependency IPC, reliable single-instance enforcement, and an extensible channel for future CLI status and runtime controls.

## Considered Options

- **systemd user service**: Rejected as a mandatory requirement to keep the CLI self-contained and portable across systems without systemd.
- **PID file with POSIX signals**: Rejected because signals cannot carry structured status payloads or bi-directional control messages.

