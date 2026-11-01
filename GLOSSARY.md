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

**Audio Chunk**:
A discrete contiguous vector of 16,000 Hz single-channel PCM audio samples produced by downmixing and resampling.
_Avoid_: Sound buffer, audio segment, recording slice

**Voice Activity Detection (VAD)**:
Energy-based evaluation assessing Root Mean Square (RMS) signal level and minimum duration to discard silence and accidental micro-taps before inference.
_Avoid_: Sound filter, noise gate, speech detector

**Audio Producer**:
The lock-free, allocation-free producer handle writing real-time microphone frames into the SPSC ring buffer inside the OS audio callback.
_Avoid_: Sound writer, audio pump, capture thread

**Speech Inference**:
The conversion of a processed 16,000 Hz single-channel PCM Audio Chunk into transcribed natural language text via an abstract provider.
_Avoid_: Model processing, AI recognition, STT run

**Inference Provider**:
An implementation of the `SpeechInference` trait (e.g., `MockSpeechInference`, `SarvamSpeechInference`, `LocalSpeechInference`) providing transcription capability.
_Avoid_: Speech backend, AI driver, STT adapter

**WAV Container**:
An in-memory 44-byte standard RIFF/WAVE header framing 16,000 Hz 16-bit signed PCM audio for HTTP multipart/form-data ingestion by cloud models.
_Avoid_: Audio file wrapper, sound file, WAV payload

**Learning Dictionary**:
The user-customizable substitution table in `~/.config/speechyyy/dictionary.toml` performing case-insensitive, word-boundary replacements on recognized speech before typing.
_Avoid_: Autocorrect, spellchecker, word map

**Transcription History**:
An append-only record of spoken utterances logged to `~/.local/share/speechyyy/history.jsonl` tracking timestamps, raw model output, finalized text, and audio duration.
_Avoid_: Speech cache, audit log, dictation records

**Smart Spacing**:
The inter-utterance context evaluation that prepends a space before alphanumeric words when following a word while attaching punctuation directly to the previous character.
_Avoid_: Space insertion, auto-space, typing pad

**Local Model Manager**:
The subsystem that manages INT8 quantized ONNX speech recognition weights (Moonshine-Tiny or Whisper-Tiny) stored in `~/.local/share/speechyyy/models/`.
_Avoid_: Model store, AI loader, weight cache

**Transliteration Mode**:
Spoken Indic and bilingual speech (e.g. Hinglish) converted into phonetically faithful Romanized English script (e.g. speaking *"mera phone number hai"* and transcribing *"mera phone number hai"*), powered by Sarvam Saaras v4 `mode="translit"`.
_Avoid_: Romanizer, phonetic converter, Hinglish mode

**Typewriter Mode**:
Sequential character-by-character caret injection pacing evdev key events with virtual Shift keychords and configurable inter-key delays (default 15ms) to produce a natural, human-like typewriter visual effect.
_Avoid_: Slow typing, key streamer, character typer

**In-Memory WAV**:
Purely in-RAM serialization of phase-corrected, normalized 16kHz mono f32 PCM audio slices into a 44-byte RIFF/WAVE container without creating temporary files or performing disk I/O.
_Avoid_: Temp audio file, disk WAV, ramdisk audio

