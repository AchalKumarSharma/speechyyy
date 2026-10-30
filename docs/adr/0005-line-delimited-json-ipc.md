# Line-Delimited JSON IPC

Inter-process communication between the CLI (`speechyyy stop`, `speechyyy status`) and the background daemon requires a structured protocol that remains lightweight and human-debuggable. We decided to use line-delimited JSON payloads over the Unix Domain Socket (e.g. `{"cmd":"stop"}\n`). This maintains strict adherence to our binary size target (< 10 MB) without complex RPC frameworks, while enabling structured telemetry and status responses for future CLI commands and GUI frontends.

## Considered Options

- **Binary serialization (bincode / postcard)**: Rejected because binary payloads prevent interactive debugging via `socat` or `nc -U` without offering meaningful performance gains for local CLI control.
- **Custom text command protocol (e.g. `STOP\n`)**: Rejected because plain string commands require bespoke parsing as soon as command arguments or structured status responses are introduced.

