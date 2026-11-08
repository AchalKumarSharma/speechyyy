# OpenSpec Exploration Report: v0.2.0 Floating Pill Dock & Real-Time IPC Event Stream

**Branch:** `refactor/v0.2.0-clean-arch`  
**Date:** 2026-11-07  
**Status:** Architectural Exploration & Integration Mapping  

```
+-------------------------------------------------------------------------------------------------------+
|                                    PROPOSED EVENT & DOCK ARCHITECTURE                                  |
+-------------------------------------------------------------------------------------------------------+
|                                                                                                       |
|   +-------------------+      spawn       +-------------------------+      push       +------------+   |
|   |   CPAL / ALSA     | ---------------> | Audio Callback (Thread) | --------------> | RingBuffer |   |
|   |  (Capture Stream) |                  +-------------------------+                 +------------+   |
|   +-------------------+                               |                                     |         |
|                                                       | atomic RMS                          | drain   |
|                                                       v                                     v         |
|   +-------------------+      IPC Cmd     +-------------------------+   transcribe    +------------+   |
|   |    CLI Trigger    | ---------------> |      Daemon Loop        | --------------> |  Inference |   |
|   | (push-up/down/etc)| (speechyyy.sock) |   (StateMachine / Evt)  |                 | (Local/BYOK|   |
|   +-------------------+                  +-------------------------+                 +------------+   |
|                                                       |                                               |
|                                                       | pub/sub line-JSON                             |
|                                                       v                                               |
|                                          +-------------------------+                                  |
|                                          |  events.sock (0600)     |                                  |
|                                          +-------------------------+                                  |
|                                                       |                                               |
|                                                       v stream                                        |
|                                          +-------------------------+                                  |
|                                          | Floating Pill Dock (UI) |                                  |
|                                          | (Layer-Shell: No Focus) |                                  |
|                                          +-------------------------+                                  |
|                                                                                                       |
+-------------------------------------------------------------------------------------------------------+
```

---

## 1. Audio Capture Pipeline & Threading

### Capture Location
- **Device Setup & Stream Lifecycle:** `src/audio/capture.rs#L20-L106` (`CpalAudioCapture::new`).
  - Queries system default input device via `cpal::default_host()` and `device.default_input_config()`.
  - Builds an input stream on the driver's native thread with callback closures for `SampleFormat::F32` (`src/audio/capture.rs#L45-L62`) and `SampleFormat::I16` (`src/audio/capture.rs#L63-L88`).

### Buffer Callback Timing & Chunk Sizes
- **Callback Periodicity:** Under PipeWire/ALSA, CPAL invokes the callback with chunks typically ranging between **256 and 1024 frames** (approx. 5.3ms to 21.3ms at 48 kHz).
- **Buffer Allocation:** The single-producer single-consumer lock-free ring buffer (`src/audio/ring_buffer.rs`) has a capacity of **2,880,000 samples** (~30s stereo or 60s mono at 48kHz, consuming ~11.5 MB RAM).
- **Ingestion Mode:** Samples currently accumulate passively during active recording without being drained until key release / `stop_capture()` (`src/audio/capture.rs#L128-L141`).

### Real-Time RMS Insertion (Zero-Latency Strategy)
- The current `compute_rms()` function (`src/audio/vad.rs#L67-L74`) runs in $O(N)$ arithmetic time over slices.
- **Optimal Hook Location:** Inside the CPAL callback closure in `src/audio/capture.rs#L48-L58`, immediately following the recording gating check (`if !is_recording_cb.load(Ordering::Relaxed)`).
- **Mechanism:** Compute local RMS or absolute peak for the 256–1024 sample block ($< 1\,\mu\text{s}$ CPU overhead) and store it in an `Arc<AtomicU32>` (holding `f32::to_bits()`).
- **Why this avoids latency:** It requires zero heap allocations, zero mutex contention, and leaves the ring buffer producer unhindered. An event ticker in the daemon can sample this atomic value at 30–60 Hz to broadcast volume levels to the UI dock.

---

## 2. Existing IPC & Command Handling

### `speechyyy toggle` Communication Path
- **CLI Client:** `src/daemon.rs#L530-L543` (`send_toggle`) invokes `IpcClient::send_request` (`src/ipc.rs#L158-L185`) with `IpcRequest::Toggle`.
- **Protocol:** Synchronous request-response over a local Unix domain socket.
  - Client connects, sets a 2-second timeout, sends newline-delimited JSON (`{"cmd":"toggle"}\n`), flushes, and reads a single newline-delimited JSON response (`{"type":"ok","message":"..."}\n`).
- **Daemon Handling:** `src/daemon.rs#L83-L96` accepts the connection, routes to `controller.toggle()`, and responds within a 500ms read/write deadline (`src/ipc.rs#L258-L259`).

### Socket Path, Binding & Permissions
- **Socket Path:** Configured in `src/ipc.rs#L49-L79` via `RuntimePaths`.
  - Primary: `$XDG_RUNTIME_DIR/speechyyy/speechyyy.sock` (e.g. `/run/user/1000/speechyyy/speechyyy.sock`).
  - Fallback: `/tmp/speechyyy-<uid>/speechyyy.sock`.
- **Directory Mode:** Created strictly with `0700` POSIX permissions (`src/ipc.rs#L102-L112`).
- **Socket Mode:** Explicitly secured via `fs::set_permissions(&path, Permissions::from_mode(0o600))` (`src/ipc.rs#L246`).

### Separating Command Ingestion from Async Event Broadcast
- **Architectural Seam:** Because `speechyyy.sock` is built for synchronous short-lived request-response interactions, keeping long-running subscriber streams on it would block incoming commands (`toggle`, `cancel`, `push-up`).
- **Clean Separation:**
  1. `speechyyy.sock`: Dedicated exclusively to transient commands (`push-down`, `push-up`, `toggle`, `cancel`, `status`, `stop`).
  2. `events.sock`: Dedicated Unix domain socket in `$XDG_RUNTIME_DIR/speechyyy/events.sock` (mode `0600`) where clients (such as the Floating Pill Dock) connect and remain connected.
  3. The daemon broadcasts newline-delimited JSON events over `events.sock` (e.g. `{"event":"state_changed","state":"recording"}` and `{"event":"audio_level","rms":0.042}`). Dead or closed client streams are pruned on write error (`EPIPE` / `BrokenPipe`).

---

## 3. Concurrency & State Management

### Central Daemon State Machine
- **State Enum:** `src/engine.rs#L173-L190` (`DictationState`):
  ```
  [Idle] ---- start_recording() ---> [Recording]
    ^                                     |
    |                               stop_and_transcribe()
    |                                     v
  [Idle] <---- finish_emission() <-- [Processing]
    ^                                     |
    |-------------- (if no text) ---------+
  ```
- **Controller Implementation:** `src/engine.rs#L327-L404` (`StateMachineController`).
- **Main Session Loop:** `src/daemon.rs#L38-L126` (`run_session_loop`):
  - Single-threaded, synchronous loop polling `listener.accept()` (non-blocking) and ticking `input_source.poll_event()` with a 10ms timeout.

### Multi-Client Channel & Pub/Sub Audit
- **Current Channel Infrastructure:**
  - `tokio::sync::broadcast`: **Not present**.
  - No `crossbeam`, `async`, or thread-safe pub/sub channels exist anywhere in `src/`.
  - All existing synchronization relies on `std::sync::Arc`, `std::sync::Mutex`, and `std::sync::atomic::*` flags.
- **Distribution Pattern for Event Stream:**
  - Standard library solution: A thread-safe subscriber list `Arc<Mutex<Vec<UnixStream>>>` managed by a dedicated non-blocking broadcaster, or a standard `std::sync::mpsc` channel sending events from the daemon loop to an `EventBroadcaster` thread.

---

## 4. Dependency Audit (`Cargo.toml`)

| Dependency | Status | Purpose / Impact |
|---|---|---|
| `serde` & `serde_json` | **Present** (v1.0.229 / v1.0.154) | Ready for serialization of streaming event payloads. |
| `tokio` / async runtimes | **Absent** | The daemon is 100% synchronous/thread-based. Introducing Tokio would increase binary size and compile times. Standard library threads/sockets remain preferred to protect the `< 10 MB` binary constraint. |
| Wayland / UI Crates (`gtk4-layer-shell`, `smithay-client-toolkit`) | **Absent** | No GUI or Wayland client crates currently exist in `Cargo.toml`. |

### Architectural Implication for Floating Pill Dock
- In accordance with our hard constraints (`< 10 MB` binary, `< 30 MB` idle RAM), embedding a heavy GTK4 runtime directly into the daemon binary risks memory and dependency bloat.
- **Recommended Architectural Approach:**
  - **Option 1 (Standalone Companion Binary):** Keep the core daemon lightweight and headless; provide a companion binary (`speechyyy-dock`) or script using Wayland layer-shell (e.g. via `gtk4-layer-shell` or lightweight C/Rust layer-shell client) that subscribes to `$XDG_RUNTIME_DIR/speechyyy/events.sock`.
  - **Option 2 (Feature-Gated Minimal Native Surface):** Introduce an optional Cargo feature (e.g., `--features dock`) using minimal `smithay-client-toolkit` with tiny software-rendered shm buffers.

---

## 5. Critical Integration Anchors & Risk Flags

### Anchor Points in Codebase

```
+---------------------------------------------------------------------------------------------------------+
| ANCHOR SUMMARY                                                                                          |
+---------------------------------------------------------------------------------------------------------+
| Event / Lifecycle Hook          | File & Line Reference                 | Action Needed for Dock Stream |
|---------------------------------|---------------------------------------|-------------------------------|
| Audio Frame Push                | src/audio/capture.rs:52, 77           | Compute atomic RMS level      |
| Audio Ingestion Complete        | src/audio/capture.rs:138-140          | AudioChunk ready for VAD      |
| State: Idle -> Recording        | src/engine.rs:344                     | Emit "state_changed": "rec"   |
| State: Recording -> Processing  | src/engine.rs:352                     | Emit "state_changed": "proc"  |
| State: Processing -> Emitting   | src/engine.rs:355                     | Emit "state_changed": "emit"  |
| State: Return to Idle           | src/engine.rs:357, 393                | Emit "state_changed": "idle"  |
| API Error: 401 Unauthorized     | src/inference/sarvam.rs:155           | Emit "error": "auth_failed"   |
| API Error: 429 Rate Limited     | src/inference/sarvam.rs:166           | Emit "error": "rate_limited"  |
| API Error: Transport Timeout    | src/inference/sarvam.rs:194           | Emit "error": "timeout"       |
| API Error Catch Point           | src/engine.rs:313                     | Intercept before Ok(None)     |
+---------------------------------------------------------------------------------------------------------+
```

### Risk Flags & Mitigations

1. **Synchronous Network Blocking in Daemon Thread:**
   - **Risk:** In `src/engine.rs#L286-L317`, `self.inference.transcribe(&chunk)` executes blocking network I/O (`ureq::post`) on the daemon main thread. During transcription (200ms–2500ms), `run_session_loop` cannot accept incoming IPC commands or process a `cancel` trigger.
   - **Mitigation:** Execute inference on a background worker thread, allowing the main daemon loop to handle `cancel` commands and emit real-time status pulses.

2. **Wayland Focus Stealing on Hyprland:**
   - **Risk:** Creating a standard window or an improperly configured layer surface on Wayland/Hyprland will seize keyboard focus from active text fields, breaking cursor injection.
   - **Mitigation:** The dock window surface must strictly request `wlr_layer_shell` with `keyboard_interactivity = NONE`. It must render purely as an overlay widget without input focus.

3. **Silent Error Swallowing:**
   - **Risk:** When `inference.transcribe()` returns an error (`src/engine.rs#L312-L316`), it logs to stderr and returns `Ok(None)`. The controller transitions directly from `Processing` to `Idle` without broadcasting an error event.
   - **Mitigation:** Extend `DictationState` or event broadcasting with an explicit `Error { code, message }` payload before resetting to `Idle`, allowing the pill dock to display clear visual feedback (e.g. red state indicator, "Invalid API Key" tooltip).

4. **Slow Subscriber Backpressure on `events.sock`:**
   - **Risk:** If a GUI dock freezes or lags while receiving high-frequency audio RMS frames (30–60 Hz), socket send buffers could fill up and block the daemon.
   - **Mitigation:** Set subscriber `UnixStream` sockets to non-blocking mode (`set_nonblocking(true)`). If `write_all` returns `WouldBlock`, drop volume meter frames rather than stalling the daemon.

---

## Recommended Next Steps

1. **Protocol Specification:** Define JSON event schemas (`StateChanged`, `AudioLevel`, `TranscriptPreview`, `Error`, `Canceled`) in an OpenSpec proposal.
2. **Channel & Socket Scaffolding:** Add `events.sock` binding to `RuntimePaths` (`src/ipc.rs#L42`) with non-blocking subscriber fanout.
3. **Decouple Long-Lived Inference:** Wrap `inference.transcribe()` in an asynchronous or worker-thread dispatch so the session loop can process `cancel` requests mid-flight.

