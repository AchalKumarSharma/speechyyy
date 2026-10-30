# 02: Daemon Lifecycle & Line-Delimited JSON IPC

**What to build:** Background daemon lifecycle management with Unix Domain Socket IPC. Allows running `speechyyy start --foreground` to run the daemon process, binds a socket and writes a PID file under the runtime directory, prevents running concurrent instances, and allows `speechyyy status` and `speechyyy stop` to communicate with the daemon via line-delimited JSON commands to inspect state and trigger clean shutdown.

**Blocked by:** 01: CLI Scaffold & System Doctor Command

**Status:** resolved

- [x] Runtime directory resolution (`$XDG_RUNTIME_DIR/speechyyy/` with `/tmp/speechyyy-$UID/` fallback).
- [x] Atomic PID file creation, validation, and single-instance enforcement on `start`.
- [x] Unix Domain Socket listener accepting line-delimited JSON messages (`stop`, `status`, ping).
- [x] `speechyyy status` queries socket, printing whether daemon is running, its PID, and uptime/state.
- [x] `speechyyy stop` sends stop command, waits for clean daemon exit, and cleans up socket and PID files.
- [x] Integration tests verifying socket commands and clean shutdown lifecycle across temporary test sockets.
