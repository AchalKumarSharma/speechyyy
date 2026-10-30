# Streaming and Cloud Speech Inference Architecture

Speechyyy requires an extensible, low-latency speech inference pipeline that receives 16,000 Hz single-channel PCM `AudioChunk` records from the audio engine, converts them into valid WAV format, and transcribes them into natural language text to be injected directly at the active OS caret. The engine must support deterministic offline mocking for hermetic testing, a high-accuracy multilingual cloud provider (Sarvam AI's `saaras:v4` model supporting 22+ Indic and Hinglish languages), and a future-proof offline local inference stub (Sherpa-ONNX). We decided to introduce an abstract `SpeechInference` trait returning `Result<String, InferenceError>`, a zero-dependency 44-byte in-memory 16-bit 16kHz mono RIFF/WAVE encoder, and a synchronous HTTP client (`ureq` v2 with `rustls`) sending RFC 7578 multipart/form-data POST requests.

## Considered Options

- **Full Async / Tokio Runtime with reqwest**: Rejected because pulling in Tokio and reqwest increases binary size by multiple megabytes, consumes unnecessary thread pools and background memory, and conflicts with Speechyyy's lightweight standalone synchronous daemon architecture (< 10 MB binary, < 30 MB idle RAM).
- **External WAV Crates (e.g. hound)**: Rejected because a 16kHz 1-channel 16-bit PCM WAV container is a fixed 44-byte standard RIFF header; writing it directly eliminates crate dependencies, reduces build times, and maintains zero heap allocations outside the audio buffer.
- **Injecting Raw Error Text at User Caret**: Rejected because spraying HTTP network errors or missing API key messages into arbitrary focused user applications (editors, terminals, chats) causes destructive keystroke side-effects; errors are isolated to stderr and diagnostics commands (`speechyyy doctor`).
- **Monolithic In-Engine HTTP Code**: Rejected because hardcoding network calls inside the audio state machine violates testability and prevents offline development or pluggable local models.

