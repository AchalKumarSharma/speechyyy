# Dictation State Machine Seam

Sprint 1 must validate the push-to-talk event loop and caret text injection using mock output before audio streaming or AI inference models are introduced. We decided to decouple the OS input hooks from transcription logic using an explicit four-state machine (`Idle -> Recording -> Processing -> Emitting`) driven by a `DictationEngine` trait. This allows Sprint 1 to be fully verified with deterministic unit tests while ensuring audio capture and inference engines in Sprint 2 plug directly into existing state transitions without altering the event loop.

## Considered Options

- **Directly coupling hotkey callbacks to injection**: Rejected because embedding mock typing directly into key-release handlers would require refactoring the entire input loop when audio streaming is added.

