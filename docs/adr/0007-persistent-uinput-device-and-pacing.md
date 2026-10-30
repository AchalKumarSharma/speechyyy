# Persistent uinput Device and Pacing

Synthesized keystrokes emitted into target windows on Linux can cause input buffer drops in Wayland compositors, Electron apps, and terminals if emitted in zero-delay bursts, and creating device nodes on-demand introduces latency. We decided to create a persistent `/dev/uinput` virtual keyboard device once when the daemon starts, and enforce a 3ms pacing delay between synthesized keydown and keyup events. This keeps push-to-talk release latency near zero while ensuring target application input queues receive all characters reliably.

## Considered Options

- **Ephemeral virtual keyboard per injection**: Rejected because kernel device node creation takes 20–50ms and triggers unnecessary libinput device discovery cycles on every utterance.
- **Zero-delay instantaneous event bursts**: Rejected because fast-streaming bursts cause dropped or reordered characters in Chromium/Electron and terminal emulators.

