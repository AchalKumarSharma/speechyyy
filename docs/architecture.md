What Speechyyy Is
Speechyyy is an open-source, ultra-lightweight desktop dictation engine that streams spoken words directly into the active cursor in real time. It operates as a native system daemon across macOS, Linux, and Windows, combining offline local-first privacy with a hybrid bring-your-own-key (BYOK) architecture.

Unlike proprietary tools that force users into paid subscriptions, cloud lock-in, or slow wait-and-paste delays, Speechyyy provides zero-account local transcription by default, supports regional Indic and Hinglish code-switching via low-latency streaming endpoints, and keeps its core memory and disk footprint under 10MB.

Specs:

Live Streaming Cursor Insertion: Streams recognized tokens into the focused text caret word-by-word while speaking, eliminating post-recording delays.

Sub-10MB Native Footprint: Built in native systems code without Chromium or Electron runtimes, keeping disk usage under 10MB and idle RAM under 30MB.

Zero Account & Local Privacy: Requires no accounts, logins, telemetry, or remote logging; all keys, history, and configs remain on the local machine.

Hybrid Engine & BYOK Freedom: Runs completely offline via embedded local models, with modular adapters for user-provided API keys (Sarvam AI, Groq, Ollama).

Mid-Sentence Hinglish Code-Switching: Transcribes fluid language switching between English and Indic languages, with an instant toggle between Latin and Devanagari script.

App-Aware Contextual Tone: Inspects the active foreground process to match style automatically—formatting casually in Slack, formally in Docs, and syntactically in terminals.

Tiered Filler Cleanup: Offers selectable cleanup thresholds, removing "ums" and false starts via regex rules, with an optional local micro-LLM pass for sentence polish.

Soft-Speech Whisper Boost: An automatic digital gain filter amplifies low-decibel, quiet input so users can dictate late at night without raising their voice.

Phonetic Custom Vocabulary: Dynamic replacement dictionary that maps spoken technical phrases to exact syntax (e.g., "cube cuddle" $\rightarrow$ kubectl, "post gress" $\rightarrow$ Postgres).

"Use Original" Rollback & Clipboard Guard: Stores raw transcripts alongside polished text for instant rollbacks, and falls back to clipboard copying if direct pasting is blocked.

================================================================================
                    SPEECHYYY - TECHNICAL ARCHITECTURE & STACK
================================================================================

1. PRODUCT OVERVIEW
--------------------------------------------------------------------------------
Name: Speechyyy
Category: Ultra-lightweight, local-first streaming dictation engine
Target Size: < 10 MB standalone binary
Target Memory: < 30 MB idle RAM
Key Value: Word-by-word streaming cursor insertion, zero telemetry/accounts, 
           hybrid local offline models + BYOK cloud (Sarvam Indic/Hinglish & Groq).


2. HIGH-LEVEL PIPELINE FLOWCHART
--------------------------------------------------------------------------------
[Hardware: Push-to-Talk Hotkey]
       |
       v
[Audio Capture Stream] (16kHz, 16-bit Mono PCM)
       |
       v
[Pre-Processing Node]
   |--> Soft-Speech Whisper Boost (Dynamic Gain Control)
   |--> Silero VAD (Noise Gate / Silence Filtering via ONNX)
       |
       +------------------------------------+
       |                                    |
       v                                    v
[Engine A: Local Offline]            [Engine B: Cloud Streaming (BYOK)]
  * Sherpa-ONNX / WhisperKit           * Sarvam AI Saaras (Indic/Hinglish WS)
  * Quantized Zipformer / Whisper      * Groq Cloud (Whisper-large-v3 WS)
  * 100% Private & Air-gapped          * Sub-200ms TTFT
       |                                    |
       +-----------------+------------------+
                         |
                         v (Live Token Chunks)
[Speculative Cursor Ingestion]
  * Simulates OS keystrokes into active focused caret
  * Tracks character offsets in session buffer
                         |
                 [Hotkey Released]
                         |
                         v
[Contextual Reconciliation & Cleanup Pass]
  * Read Active Foreground Process (Bundle ID / Window Class)
  * Phonetic Custom Vocabulary Match (Slang/Jargon -> Proper Syntax)
  * Tiered Filler Cleanup:
      - None: Retain verbatim transcript
      - Light: Regex removal of "ums", "uhs", repeated words
      - Medium: Fast micro-LLM rewrite (Ollama / Groq) tuned to app tone
                         |
                         v
[Atomic Diff & Patch Commit]
  * Sends backspaces for modified tokens -> Pastes final polished text
  * Clipboard Guard: If direct paste/injection is blocked, copies to clipboard


3. DETAILED TECH STACK
--------------------------------------------------------------------------------
A. Core System Language
   * Language: Rust (2021 Edition)
   * Why: Compiles to a single zero-dependency static binary under 10 MB, provides
     cross-platform portability (macOS, Linux, Windows), and avoids Electron/Python bloat.

B. Audio Capture & Pre-Processing
   * Audio Capture: `cpal` (cross-platform audio abstraction layer)
       - macOS: CoreAudio backend
       - Windows: WASAPI backend
       - Linux: PipeWire / ALSA backend
   * Resampling: `rubato` (Real-time sample rate conversion to 16kHz mono PCM)
   * Voice Activity Detection (VAD): `silero-vad` via lightweight ONNX Runtime
   * Whisper Boost Filter: Custom RMS calculation with adaptive gain (6 dB to 18 dB)

C. Inference Engines (Hybrid Provider Layer)
   * Local Engine: `sherpa-rs` / `sherpa-onnx`
       - Model: Quantized streaming Zipformer or Whisper-tiny/base INT8 (< 50MB)
   * Cloud WebSocket Client: `tokio-tungstenite`
       - Sarvam AI: Real-time Saaras WebSocket endpoint (`codemix`, `translit`, `transcribe`)
       - Groq / OpenAI: Real-time transcription endpoints

D. OS Hooks, Input Synthesis & Window Inspection
   * Global Hotkey Listener: `rdev` or `global-hotkey`
       - Push-to-talk default: Right Option (macOS), Right Alt (Windows/Linux)
   * Keystroke Emulation: `enigo`
       - macOS: `CGEvent` & `AXUIElement`
       - Windows: Win32 `SendInput` API
       - Linux: `wtype` (Wayland) / `xdotool` (X11)
   * Foreground Context Inspector: `active-win-pos-rs`
       - Inspects process name (e.g., Slack, VS Code, Chrome, Terminal)

E. Storage & Configuration
   * Local Config: `serde` + `serde_json`
   * Path Management: `directories-next`
   * Location:
       - Linux: `~/.config/speechyyy/config.json`
       - macOS: `~/Library/Application Support/speechyyy/config.json`
       - Windows: `%APPDATA%\speechyyy\config.json`
   * Cloud Keys: Plaintext in local config or stored via OS Keychain (`keyring-rs`)


4. CONFIGURATION SCHEMA (config.json)
--------------------------------------------------------------------------------
{
  "active_engine": "sarvam",
  "hotkey": "RightOption",
  "whisper_boost": true,
  "filler_cleanup": "light",
  "engines": {
    "local": {
      "model_type": "sherpa_zipformer",
      "model_path": "models/sherpa-zipformer-en-int8.onnx",
      "threads": 2
    },
    "sarvam": {
      "api_key": "YOUR_SARVAM_API_KEY",
      "language_code": "hi-IN",
      "mode": "codemix"
    },
    "groq": {
      "api_key": "YOUR_GROQ_API_KEY",
      "model": "whisper-large-v3"
    }
  },
  "vocabulary": {
    "post gress": "Postgres",
    "cube cuddle": "kubectl",
    "next js": "Next.js",
    "pi torch": "PyTorch"
  },
  "app_profiles": {
    "Slack": { "tone": "casual", "strip_final_period": true },
    "Terminal": { "tone": "syntax", "code_case": true },
    "Google Docs": { "tone": "formal" }
  }
}


5. PROJECT DIRECTORY STRUCTURE
--------------------------------------------------------------------------------
speechyyy/
├── Cargo.toml
├── build.rs
├── install.sh                  # One-line curl installer for Linux/macOS
├── install.ps1                 # One-line PowerShell installer for Windows
├── assets/
│   └── icon.png
└── src/
    ├── main.rs                 # CLI setup, daemon launcher & event loop
    ├── audio/
    │   ├── mod.rs
    │   ├── capture.rs          # cpal microphone stream handler
    │   ├── gain.rs             # Soft-speech whisper boost (AGC)
    │   └── vad.rs              # Silero VAD frame filtering
    ├── engine/
    │   ├── mod.rs              # SpeechEngine trait interface
    │   ├── local_sherpa.rs     # Sherpa-ONNX streaming implementation
    │   ├── cloud_sarvam.rs     # Sarvam AI streaming WebSocket client
    │   └── cloud_groq.rs       # Groq cloud transcription client
    ├── system/
    │   ├── mod.rs
    │   ├── hotkey.rs           # Global push-to-talk key listener
    │   ├── keyboard.rs         # Direct cursor injection via enigo
    │   └── window.rs           # Active window & bundle ID detector
    ├── processing/
    │   ├── mod.rs
    │   ├── vocabulary.rs       # Phonetic dictionary replacements
    │   ├── cleanup.rs          # Regex filler word cleanup ("ums", "uhs")
    │   └── context_rules.rs    # App-specific tone & capitalization rules
    └── config/
        ├── mod.rs
        └── settings.rs         # JSON serialization & path resolution