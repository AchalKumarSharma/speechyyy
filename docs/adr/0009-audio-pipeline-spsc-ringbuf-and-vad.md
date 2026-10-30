# Audio Pipeline, Lock-Free Ring Buffer, and Energy VAD

Speechyyy must capture real-time microphone audio across Linux audio backends (PipeWire and ALSA) and supply clean, standardized 16,000 Hz single-channel PCM audio to downstream speech recognition models. Audio callbacks run on high-priority OS audio threads where mutex contention, blocking locks, or heap allocations cause audio dropouts, clicks, and buffer underruns. We decided to decouple audio capture using a lock-free Single-Producer Single-Consumer (SPSC) ring buffer (`ringbuf`), process raw interleaved streams through an anti-aliased downmixer and continuous-time area resampler, and evaluate an energy-based Voice Activity Detection (RMS threshold and 200ms minimum duration filter).

## Considered Options

- **Direct Mutex Locking in CPAL Callback**: Rejected because blocking locks in real-time audio threads trigger priority inversion and sound glitches under CPU load.
- **Heavy Third-Party Resampling/FFT Libraries (e.g. libsamplerate, rubato)**: Rejected to preserve the strict < 10 MB binary target and zero-dependency compilation footprint.
- **Relying Exclusively on Inference Engine VAD**: Rejected because passing silence and accidental micro-taps to heavy neural models wastes CPU cycles, induces hallucinated punctuation, and degrades battery life.

