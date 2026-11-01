# Differential Microphone Phase Correction and Text Injection Ergonomics

Speechyyy must reliably capture human speech across diverse Linux hardware configurations and inject transcribed text into modern GUI environments (including Wayland compositors, nested Electron webviews, and terminal emulators). In real-world deployment on laptop hardware (e.g. Realtek ALC257), two primary hardware issues arose: differential dual-mic arrays with 180° out-of-phase stereo channels that completely cancel speech to silence when averaged, and uncorrected hardware DC bias that falsely trips energy VAD on silence. Additionally, fast push-to-talk bursts require expanded buffer headroom, and automated text injection in Electron webviews requires precise hardware uinput paste chords.

We decided to implement:
1. **Pearson Correlation-Aware Stereo Downmixing**: Detect anti-correlated stereo channels ($r < -0.2$ and $\text{energy}_{diff} > \text{energy}_{sum}$) and compute $\frac{L - R}{2}$ instead of arithmetic averaging, eliminating destructive phase cancellation.
2. **DC Bias Removal**: Subtract sample mean in `process_raw_audio` so pure DC baseline offset is zeroed to RMS 0.0, allowing VAD to cleanly reject silence.
3. **Frame-Aligned Ring Buffer Headroom**: Increase ring buffer capacity to 2,880,000 samples (30 seconds of 48kHz stereo, consuming ~11.5 MB RAM well within the < 30 MB idle limit) and enforce `push_frames` so buffer pushes are strictly aligned to channel multiples, preventing channel desynchronization.
4. **Kernel-Level uinput Paste with Wayland Settle Delay**: For clipboard-based text injection, synchronize via `wl-copy`, wait an 80ms compositor settle delay, and emit a direct hardware `Ctrl+V` key chord via `/dev/uinput` with a 20ms hold delay, without injecting disruptive `Esc` keystrokes.

## Considered Options

- **Static Single-Channel Capture**: Rejected because ALSA/PipeWire hardware configurations vary between mono and multi-channel; querying the system default config and dynamically detecting phase relationships in software guarantees portable operation across both conventional stereo mics and differential arrays.
- **Dynamic Array Reallocation during Overflow**: Rejected to maintain real-time safety inside the audio callback; a fixed pre-allocated lock-free SPSC buffer of 30 seconds eliminates allocation latency while adhering to RAM constraints.
- **Escape Defuse Keystroke for Electron Menus**: Rejected because emitting synthetic `KEY_ESC` before pasting deselects active text inputs and cancels input focus across Chromium and Electron webviews. A clean 80ms settle delay and pure `Ctrl+V` uinput chord provides reliable paste delivery without focus loss.

