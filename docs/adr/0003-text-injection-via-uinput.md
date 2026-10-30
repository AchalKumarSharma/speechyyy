# Text Injection via uinput

Caret text injection into the active focused window on Wayland cannot use legacy XTest or wlroots-specific tools like `wtype` (which are rejected by GNOME Mutter). We decided to abstract text emission behind a `TextInjector` trait with `/dev/uinput` as the primary virtual keyboard driver, while designing a clipboard paste fallback (`wl-copy` + Ctrl+V) for complex Unicode characters. This achieves universal compatibility across all Linux desktop environments and compositors while ensuring multi-lingual text can be emitted cleanly.

## Considered Options

- **`wtype` (`virtual-keyboard-v1`)**: Rejected as the primary driver because it only functions on wlroots-based compositors and fails on GNOME Mutter.
- **Pure clipboard paste (`wl-copy` + Ctrl+V)**: Rejected as the primary driver for all text because replacing the user's clipboard buffer on every utterance degrades UX.

