# 04: Persistent uinput Virtual Keyboard Injector

**What to build:** Caret text injection through a persistent Linux `/dev/uinput` virtual keyboard device. Implements a `TextInjector` trait that translates characters to scancode key-presses and key-releases, enforcing a 3ms inter-keystroke pacing delay to prevent target application input buffers from dropping characters.

**Blocked by:** None (can start immediately)

**Status:** resolved

- [x] `TextInjector` trait definition for emitting text into the focused caret position.
- [x] Persistent Linux `/dev/uinput` virtual keyboard driver that initializes once and remains open.
- [x] Character-to-scancode translation supporting ASCII characters, uppercase shift modifiers, and space/punctuation.
- [x] Configurable pacing delay (default 3ms) between synthetic key-down and key-up events.
- [x] Test injector adapter verifying character order and pacing timing in unit/integration tests without requiring `/dev/uinput` device access.
