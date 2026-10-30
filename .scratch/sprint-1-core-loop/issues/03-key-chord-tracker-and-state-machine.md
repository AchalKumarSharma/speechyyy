# 03: Key Chord Tracker & Dictation State Machine

**What to build:** The in-memory Push-to-Talk state machine and chord detection engine. Evaluates sequences of input key events to detect single-key triggers (`KEY_RIGHTALT`) and compound chords (`Ctrl+Space`), transitioning the dictation state through `Idle -> Recording -> Processing -> Emitting`. On key release, completes the session and emits the mock payload `"Hello from Speechyyy"` through a decoupled engine trait.

**Blocked by:** None (can start immediately)

**Status:** resolved

- [x] `KeyChordTracker` maintaining active modifier bitmasks and tracking trigger key down/up transitions.
- [x] Configurable target chord supporting single keys (`RightAlt`) and combos (`Ctrl+Space`).
- [x] Four-state machine implementation (`Idle`, `Recording`, `Processing`, `Emitting`).
- [x] `DictationEngine` trait decoupling session transitions from output generation.
- [x] Mock engine adapter producing `"Hello from Speechyyy"` on transition to `Emitting`.
- [x] Pure unit tests exercising rapid key sequences, modifier interleaving, release transitions, and state boundaries.
