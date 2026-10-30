# Speechyyy

Desktop speech-to-text dictation daemon that streams speech directly into the active OS cursor.

## Language

**Push-to-Talk**:
The physical holding of a global hotkey to capture speech, ending on key release.
_Avoid_: Voice trigger, hotkey toggle, keybind

**Daemon**:
The standalone background process that orchestrates global input monitoring, transcription, and text delivery.
_Avoid_: Service, agent, worker process

**Daemon Socket**:
The local Unix Domain Socket used for inter-process control and telemetry between the CLI and the daemon.
_Avoid_: Control pipe, IPC port, message bus

**Key Chord**:
A single key or compound key combination that initiates and sustains a Push-to-Talk session.
_Avoid_: Key combo, shortcut, key sequence

**Caret Injection**:
The delivery of transcribed text directly into the focused window at the active OS cursor position.
_Avoid_: Keystroke emulation, typing simulation, pasting

**Pacing Delay**:
The microsecond-scale pause between synthesized keystrokes that prevents application input buffer drops.
_Avoid_: Keystroke sleep, typing throttle, input lag

**Dictation Session**:
A single user interaction cycle transitioning through idle, recording, processing, and emission states.
_Avoid_: Transcription run, recording event

**Pre-flight Check**:
The startup verification of required OS permissions and device node access before the daemon initializes.
_Avoid_: Permission probe, initialization check, sanity test
