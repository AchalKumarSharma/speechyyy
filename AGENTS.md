# Speechyyy

Speechyyy is an open-source, ultra-lightweight desktop dictation engine written in Rust.
It streams speech directly into the active OS cursor in real time via global push-to-talk.

## Hard Constraints
- Architecture: Standalone CLI daemon compiled in Rust 2021 Edition.
- Binary Size Target: Strictly < 10 MB. Idle RAM: < 30 MB.
- Platform Priority: Linux first (Arch/CachyOS/Omarchy on Wayland & X11), structured for cross-platform portability to macOS and Windows.
- Core Workflow: Daemon started via `speechyyy start`. User holds a global hotkey (Push-to-Talk), talks, and words stream live into the focused window/caret.
- Audio & Inference: Hybrid model (Offline local Sherpa-ONNX + BYOK Cloud WebSocket for Sarvam AI 22+ Indic/Hinglish & Groq). Zero account/telemetry required.

## Agent skills

### Issue tracker

Local markdown files in `.scratch/<feature>/`. See `docs/agents/issue-tracker.md`.

### Domain docs

Single-context repo (`GLOSSARY.md` and `docs/adr/` at repo root). See `docs/agents/domain.md`.

## Agent skills

### Issue tracker

Local markdown files in `.scratch/<feature>/`. See `docs/agents/issue-tracker.md`.

### Domain docs

Single-context repo (`GLOSSARY.md` and `docs/adr/` at repo root). See `docs/agents/domain.md`.
