# Technical Specification: Floating Overlay Dock & Real-Time Event Stream

**Document ID:** `SPEC-v0.2.0-OVERLAY-DOCK`  
**Status:** Approved for Implementation  
**Target Release:** `v0.2.0`  
**Target Branch:** `refactor/v0.2.0-clean-arch`  
**Author:** Antigravity Architect & Apple Design Specialist  
**Grounding Reference:** [`docs/floating_pill_dock_exploration.md`](file:///home/alfred/Projects/speechyyy/docs/floating_pill_dock_exploration.md)

---

## 1. Architectural Invariants

### 1.1 Hard System Constraints
- **Binary Footprint:** Strictly `< 10 MB` compiled release binary (via `panic = "abort"`, `codegen-units = 1`, `lto = true`, and `strip = true`).
- **Memory Footprint:** Strictly `< 30 MB` idle RAM.
- **Zero Tokio Runtime Dependency:** The daemon architecture remains 100% synchronous and standard-library threaded. All concurrency uses `std::thread`, `std::sync::mpsc`, and non-blocking `std::os::unix::net::UnixStream`. No async runtimes (Tokio, async-std) may be introduced into the daemon core.
- **Wayland Focus Isolation:** The UI dock runs as a Wayland layer-shell surface with `keyboard_interactivity = NONE`, ensuring it can never steal keyboard focus or interrupt virtual caret keystroke injection.

---

### 1.2 Dual-Socket IPC Architecture

The daemon exposes two distinct Unix Domain Sockets in the user's secure runtime directory (`$XDG_RUNTIME_DIR/speechyyy/`, default permissions `0700`):

```
+---------------------------------------------------------------------------------------------------+
|                                     DUAL-SOCKET TOPOLOGY                                          |
+---------------------------------------------------------------------------------------------------+
|                                                                                                   |
|  +--------------------+                     +-------------------------+                           |
|  | CLI Trigger Client |                     |   Speechyyy Daemon      |                           |
|  |  (speechyyy toggle)|                     |     (Main Thread)       |                           |
|  +--------------------+                     +-------------------------+                           |
|            |                                             |                                        |
|            | 1. Request-Response (short-lived)           |                                        |
|            +-------------------------------------------->| speechyyy.sock (mode 0600)             |
|            |    {"cmd":"toggle"}                         | (accepts commands, replies, closes)    |
|            |<--------------------------------------------+                                        |
|            |    {"type":"ok","message":"..."}            |                                        |
|                                                          |                                        |
|  +--------------------+                                  |                                        |
|  | Floating Pill Dock |                                  |                                        |
|  |   (Overlay UI)     |                                  v                                        |
|  +--------------------+                     +-------------------------+                           |
|            ^                                | Event Broadcaster Thread|                           |
|            | 2. NDJSON Stream (long-lived)  +-------------------------+                           |
|            +--------------------------------| events.sock (mode 0600) |                           |
|                 {"event":"state_changed"}   | (fanout to subscribers) |                           |
|                 {"event":"audio_level"}     +-------------------------+                           |
|                                                                                                   |
+---------------------------------------------------------------------------------------------------+
```

#### Socket Roles & Contracts

1. **`speechyyy.sock` (Command Ingestion Socket):**
   - **File Permissions:** `0600` (`rw-------`).
   - **Pattern:** Synchronous Request-Response (short-lived connection).
   - **Timeout:** 500ms read/write timeout per connection.
   - **Supported Commands:** `PushDown`, `PushUp`, `Toggle`, `Cancel`, `Status`, `Stop`, `Ping`.
   - **Guarantee:** Never holds connections open; immediately processes, replies, and closes.

2. **`events.sock` (Asynchronous Event Broadcast Socket):**
   - **File Permissions:** `0600` (`rw-------`).
   - **Pattern:** Asynchronous One-Way Publish-Subscribe stream.
   - **Payload Format:** Newline-delimited JSON (NDJSON).
   - **Connection Model:** Clients connect and remain connected indefinitely.
   - **Backpressure Handling:** Non-blocking writes (`set_nonblocking(true)`). High-frequency `audio_level` frames are discarded on `WouldBlock`. Broken or terminated client streams (`BrokenPipe`, `EPIPE`) are immediately pruned without blocking the daemon.

---

### 1.3 Audio Pipeline: Lock-Free Atomic RMS Calculation

To stream real-time audio volume without locking or allocating inside the audio driver thread:

1. **Capture Hook Point:** Inside [`src/audio/capture.rs`](file:///home/alfred/Projects/speechyyy/src/audio/capture.rs#L48-L85) within the CPAL buffer callback:
   ```rust
   // Invoked every ~5ms - 21ms (256 - 1024 frames)
   if is_recording_cb.load(Ordering::Relaxed) {
       let block_rms = calculate_block_rms(data);
       active_rms.store(block_rms.to_bits(), Ordering::Relaxed);
       producer.push_frames(data, channels_usize);
   }
   ```
2. **Data Structure:** `Arc<AtomicU32>` holding IEEE 754 float bits (`f32::to_bits()` / `f32::from_bits()`).
3. **Sampling Ticker:** The event broadcaster thread samples this atomic value at **30–60 Hz** (every 16–33ms) and broadcasts an `AudioLevel` NDJSON frame if the daemon state is `Recording`.
4. **Latency Guarantee:** Zero locks, zero heap allocations, and zero impact on the audio ring buffer push operation.

---

### 1.4 Worker-Thread Inference & Cancellation Support

Currently, [`inference.transcribe(&chunk)`](file:///home/alfred/Projects/speechyyy/src/engine.rs#L286) executes synchronously on the daemon session loop, freezing socket processing for 200ms–2500ms during network requests.

#### Specification:
1. **Worker Offload:** On key release or `PushUp`, the recorded audio chunk is transferred to a dedicated worker thread via `std::sync::mpsc::channel`.
2. **State Transition:** The controller immediately transitions from `Recording` to `Processing` and returns to the event loop.
3. **Cancellation Path:** If a `Cancel` command (`speechyyy cancel`) arrives while in `Processing`:
   - An atomic cancellation token (`Arc<AtomicBool>`) is flagged.
   - The pending worker result is discarded upon completion.
   - The daemon immediately resets to `Idle` and emits a `StateChanged { state: DictationState::Idle }` event.
4. **Result Ingestion:** When the worker completes successfully and the cancellation token is clear:
   - The controller receives the transcript, injects text via `/dev/uinput` (`Emitting`), and transitions back to `Idle`.

---

### 1.5 Zero Silent Failures

Silent swallows of errors into `Ok(None)` are strictly prohibited.

#### Error Flow:
1. Any network failure, authorization failure, or quota exhaustion from [`src/inference/sarvam.rs`](file:///home/alfred/Projects/speechyyy/src/inference/sarvam.rs#L143-L202) produces an explicit `InferenceError`.
2. In [`src/engine.rs`](file:///home/alfred/Projects/speechyyy/src/engine.rs#L312-L316), errors are intercepted:
   - The daemon transitions to `DictationState::Error`.
   - An `Error` event is broadcast across `events.sock`:
     ```json
     {"event":"error","code":"auth_failed","message":"Invalid or expired Sarvam API key"}
     ```
   - After emitting the error event, the controller pauses for **2000ms** (or the UI auto-dismisses) and resets to `DictationState::Idle`.

---

## 2. IPC NDJSON Protocol Schema

All events broadcast across `events.sock` are single-line JSON objects terminated by `\n`.

### 2.1 Rust Data Definitions

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DictationState {
    Idle,
    Recording,
    Processing,
    Emitting,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum DaemonEvent {
    /// Broadcast whenever the dictation state changes
    StateChanged {
        state: DictationState,
        #[serde(skip_serializing_if = "Option::is_none")]
        previous: Option<DictationState>,
    },
    /// Broadcast at 30-60 Hz during active recording
    AudioLevel {
        /// Normalized RMS value clamped strictly between 0.0 and 1.0
        rms: f32,
        /// Peak amplitude (0.0 to 1.0)
        peak: f32,
    },
    /// Live transcript preview / speculative token chunk
    TranscriptPreview {
        text: String,
        is_final: bool,
    },
    /// Explicit error notification
    Error {
        code: String,
        message: String,
    },
    /// Recording or transcription canceled by user
    Canceled {
        reason: String,
    },
}
```

### 2.2 Wire Payloads (Examples)

#### 1. State Transition to Recording
```json
{"event":"state_changed","state":"recording","previous":"idle"}
```

#### 2. Real-Time Audio Level (30–60 Hz)
```json
{"event":"audio_level","rms":0.0425,"peak":0.1180}
```

#### 3. State Transition to Processing
```json
{"event":"state_changed","state":"processing","previous":"recording"}
```

#### 4. API Error Event
```json
{"event":"error","code":"rate_limited","message":"Sarvam AI rate limit exceeded or credit balance exhausted"}
```

#### 5. User Cancellation
```json
{"event":"canceled","reason":"shortcut_cancel"}
```

---

### 2.3 Non-Blocking Backpressure Rules

To ensure slow UI clients never degrade the daemon:
1. All client subscriber streams are placed in non-blocking mode: `stream.set_nonblocking(true)`.
2. When serializing and writing `DaemonEvent::AudioLevel`:
   - If `stream.write_all()` returns `std::io::ErrorKind::WouldBlock`, the frame is dropped immediately. Audio levels are ephemeral; missing a 16ms frame causes zero visual stutter due to client-side spring smoothing.
3. When writing critical events (`StateChanged`, `Error`, `Canceled`):
   - A bounded FIFO buffer (up to 16 events) is maintained per subscriber.
4. If a subscriber socket returns `BrokenPipe`, `ConnectionReset`, or `EPIPE`, the subscriber is removed from the active broadcast list and dropped.

---

## 3. Apple HIG & Wayland Layer-Shell Design Tokens

The overlay dock is designed in accordance with **Apple Human Interface Guidelines (Materials, Layout, Typography, Motion, Craft & Delight)** and native Wayland protocols.

```
+---------------------------------------------------------------------------------------------------+
|                                  FLOATING CAPSULE PILL SPECIFICATION                              |
+---------------------------------------------------------------------------------------------------+
|                                                                                                   |
|                       <-------------------- 180px -------------------->                           |
|                      +-------------------------------------------------+     ^                    |
|                      |   |||||   (Live Waveform / Morphing Icon)   |||| |     | 38px               |
|                      +-------------------------------------------------+     v                    |
|                                                                                                   |
|                                              | 36px bottom margin                                 |
|                                              v                                                    |
|                                 +-------------------------+                                       |
|                                 |   Screen Bottom Edge    |                                       |
|                                 +-------------------------+                                       |
|                                                                                                   |
+---------------------------------------------------------------------------------------------------+
```

### 3.1 Wayland Layer-Shell Integration
- **Surface Protocol:** `zwlr_layer_shell_v1`.
- **Target Layer:** `ZWLR_LAYER_SHELL_V1_LAYER_OVERLAY`.
- **Anchor:** `ZWLR_LAYER_SHELL_V1_ANCHOR_BOTTOM`.
- **Margin:** Bottom margin = **`36px`** (avoids dock/taskbar collision on Omarchy/Hyprland/Sway).
- **Exclusive Zone:** `0` (floats purely above workspace windows without pushing application tiles).
- **Focus Safety Invariant:** `ZWLR_LAYER_SHELL_V1_KEYBOARD_INTERACTIVITY_NONE` (`keyboard_mode = 0`). The surface **never** accepts keyboard input, preventing window focus theft.

---

### 3.2 Visual Tokens & Dimensions

| Token | Value | Design Rationale & HIG Reference |
|---|---|---|
| **Height** | `38px` | Compact glanceable utility; unobtrusive during active typing. |
| **Width (Resting / Idle)** | `0px` (Hidden) or `14px` (Micro-dot) | Zero visual clutter when idle. |
| **Width (Recording)** | `180px` (Dynamic: `140px`–`220px`) | Expands fluidly based on speech waveform width. |
| **Border Radius** | `9999px` (`border-radius: 19px`) | Perfect continuous pill/capsule curve (`layout.md`). |
| **Surface Background** | `rgba(28, 28, 30, 0.78)` | Apple Dark Neutral Surface (`materials.md` Liquid Glass). |
| **Backdrop Blur** | `backdrop-filter: blur(24px) saturate(180%)` | True frosted glass material over arbitrary wallpapers. |
| **Border Stroke** | `1px solid rgba(255, 255, 255, 0.12)` | Subtle hairline edge highlight for depth and separation. |
| **Drop Shadow** | `0 8px 24px rgba(0, 0, 0, 0.35)` | Elevation hierarchy without harsh drop edges. |

---

### 3.3 Visual Physics & Waveform Bars

During active `Recording`, the dock displays **9 vertical rounded bars**:

```
           Bar Index:   0   1   2   3   4   5   6   7   8
                      +---+---+---+---+---+---+---+---+---+
                      |   |   |   | # | # | # |   |   |   |
                      |   | # | # | # | # | # | # | # |   |
                      | # | # | # | # | # | # | # | # | # |
                      +---+---+---+---+---+---+---+---+---+
          Weights:    0.3 0.6 0.8 1.0 1.2 1.0 0.8 0.6 0.3
```

- **Bar Width:** `3px`.
- **Bar Gap:** `4px`.
- **Bar Radius:** `1.5px` (fully rounded caps).
- **Resting Height:** `4px`.
- **Maximum Height:** `22px`.
- **Color:** `#F5F5F7` (Apple Light Gray 1) with a soft electric purple/blue tint (`#8E8CF8`) at the central peaks.
- **Spring Dynamics:** Heights are interpolated using a critically damped spring model:
  - **Stiffness ($k$):** `300.0`
  - **Damping ratio ($\zeta$):** `0.75`
  - **Frequency:** Updates run at 60 Hz on the client render loop.
  - **Gaussian Envelope:** Bar heights are scaled by a symmetric Gaussian weight:
    $$\text{height}_i = \text{clamp}\left(4.0 + \text{rms} \times 32.0 \times e^{-\frac{(i-4)^2}{6.0}},\, 4.0,\, 22.0\right)$$

---

### 3.4 Morphing State Micro-Interactions

```
[Idle] 
  (Hidden / Opacity: 0.0, Scale: 0.85)
    |
    | Hotkey Down (PushDown)
    v
[Recording] 
  (Capsule expands to 180px, 9 animated waveform bars bounce with live RMS)
    |
    | Hotkey Up (PushUp)
    v
[Processing] 
  (Width shrinks to 140px, subtle horizontal glowing pulse passes left-to-right)
    |
    +------------------------------------------+
    | Success                                  | Error (401, 429, Timeout)
    v                                          v
[Emitting]                                 [Error]
  (Brief green confirmation pulse,           (Capsule tint turns to #FF453A,
   text injects into caret,                   displays concise error string,
   fades to Idle in 200ms)                    auto-dismisses after 2.0s)
```

1. **State: Idle**
   - Visibility: `opacity: 0.0`, `transform: scale(0.88) translateY(12px)`.
   - Interaction: Completely unmapped or zero-size surface.

2. **State: Recording**
   - Transition: Spring expand (250ms, response `0.35s`, damping `0.72`).
   - Content: 9 vertical spring-loaded waveform bars responsive to `AudioLevel` packets.

3. **State: Processing**
   - Transition: Smooth morph to 140px width.
   - Content: Three pulsing dots or a subtle horizontal shimmer gradient across the capsule.

4. **State: Emitting**
   - Transition: Accent dot flashes Apple Mint/Green (`#30D158`).
   - Keystroke typewriter injection streams into the active window.
   - Transitions to `Idle` with a 200ms ease-out fade.

5. **State: Error**
   - Visual Treatment: Pill background shifts to semitransparent Apple Red (`rgba(255, 69, 58, 0.22)`), border stroke `#FF453A`.
   - Typography: San Francisco Pro Display / Inter, 12pt Medium, `#FFD5D2`.
   - Behavior: Displays error string (e.g., "Key Expired", "Rate Limited", "Offline"), holds for **2.0 seconds**, then fades smoothly back to `Idle`.

---

## 4. Implementation Phase Breakdown

### Phase 1: Daemon Sockets & Event Fanout (`speechyyy` daemon core)
1. Add `events.sock` path resolution to `RuntimePaths` in [`src/ipc.rs`](file:///home/alfred/Projects/speechyyy/src/ipc.rs).
2. Implement `EventBroadcaster` using `std::sync::mpsc` and non-blocking `UnixStream` client registry.
3. Add `Arc<AtomicU32>` RMS metric to [`src/audio/capture.rs`](file:///home/alfred/Projects/speechyyy/src/audio/capture.rs).
4. Offload inference to a worker thread and route errors to explicit `DaemonEvent::Error`.

### Phase 2: CLI Event Stream Command
1. Expose `speechyyy events` CLI command that connects to `$XDG_RUNTIME_DIR/speechyyy/events.sock` and streams NDJSON directly to stdout (useful for debugging, scripts, and Waybar/Eww widgets).

### Phase 3: Layer-Shell Overlay Dock
1. Implement the floating capsule pill client targeting `wlr-layer-shell`.
2. Connect to `events.sock` and drive the 9-bar spring waveform.

---

## 5. Verification Checklist

- [ ] `cargo check --all-targets` compiles cleanly.
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` completes with 0 warnings.
- [ ] Release binary size remains strictly `< 10 MB` (`target/release/speechyyy`).
- [ ] Daemon idle memory remains `< 30 MB` RSS.
- [ ] Layer-shell surface sets `keyboard_mode = NONE`; focused terminal/editor retains 100% active caret typing focus.
- [ ] Disconnecting an event subscriber (`Ctrl+C` on `speechyyy events`) never panics or stalls the active daemon.
- [ ] Mock and real Sarvam HTTP 401/429 errors produce explicit `DaemonEvent::Error` without hanging in `Processing`.

