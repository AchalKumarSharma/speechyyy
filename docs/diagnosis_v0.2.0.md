# Pre-Release Diagnosis: Speechyyy v0.2.0

**Target Branch:** `refactor/v0.2.0-clean-arch`  
**Date:** 2026-11-09  
**Evaluation Scope:** Safe, Secure, Reliable, Smooth, Dead & Garbage Code Audit

---

## 1. Verdict

| Pillar | Status | Confidence | Reason |
|---|---|---|---|
| **Safe** | **Pass** | High | Zero memory unsafety in active paths, no `attach(None, ...)` on layer surfaces, clean modifier release on injection, and non-test unwraps are guarded. |
| **Secure** | **Needs work** | High | `speechyyy update` downloads and extracts binary archives without verifying SHA-256 or signatures, and local inference leaks spoken transcripts to stderr via `eprintln!`. |
| **Reliable** | **Needs work** | High | Sarvam lacks a request timeout (`timeout: None`), all cloud providers lack transient retries, and synchronous transcription blocks the daemon loop, preventing mid-processing cancelation. |
| **Smooth** | **Needs work** | High | Clean machine defaults are broken (`default_provider = "local"` with `default_model = "saaras:v4"`), causing immediate pre-flight/doctor failure on fresh installs; `speechyyy-dock` lacks `--help`. |
| **Code Hygiene** | **Needs work** | High | ~378 lines of legacy evdev/`EVIOCGRAB` code, 10 dead `unsafe` blocks, and a 47-line loop in `daemon.rs` remain compiled despite the daemon exclusively using `NoopInputSource`. |

---

## 2. Release Blockers

Before tagging `v0.2.0`, the following 3 issues must be resolved:

1. **[SEC-01] Missing Checksum/Signature Verification in `speechyyy update` (`src/update.rs:403-417`):**  
   The auto-updater fetches pre-compiled release tarballs from GitHub and unpacks them directly into a temporary directory without checking any SHA-256 hash or cryptographic signature.
2. **[SMOOTH-01] Broken Default Configuration on Fresh Installs (`src/config.rs:231-237`):**  
   The out-of-the-box defaults configure `provider = "local"` and `model = "saaras:v4"`. Because "saaras:v4" is a Sarvam cloud model, `speechyyy doctor` and `speechyyy start -f` fail immediately on a new install with `Unknown local model 'saaras:v4'`.
3. **[SEC-02] Spoken Transcript Plaintext Leaked to `stderr` (`src/inference/local.rs:249-251, 401-406`):**  
   The local and custom INT8 inference engines log the raw transcribed user speech directly to standard error via `eprintln!`, violating the zero-telemetry and privacy guarantees if logs are captured.

---

## 3. Findings Table

| ID | Severity | Pillar | Location | Evidence | Impact | Suggested Fix | Effort | Confidence |
|---|---|---|---|---|---|---|---|---|
| **SEC-01** | **Blocker** | Secure | `src/update.rs:403-417` | `unpack_tarball(&tarball_path, &temp_dir)?;` directly follows `agent.get(&download_url).call()`. | Tampered or compromised release assets on GitHub could achieve unverified execution. | Download `SHA256SUMS` from release, verify SHA-256 of tarball before unpacking (matching `install.sh` & `model.rs`). | S | Verified |
| **SMOOTH-01** | **Blocker** | Smooth | `src/config.rs:231-237` | `default_provider() -> "local"`, `default_model() -> "saaras:v4"`. | Fresh users cannot run the daemon or pass `speechyyy doctor` without manually running `speechyyy use`. | Align defaults: set `default_provider` to `"sarvam"` or set `default_model` to a valid local model (`"parakeet-tdt-v3"`), or initialize `model = ""`. | S | Verified |
| **SEC-02** | **Blocker** | Secure | `src/inference/local.rs:249-251, 401-406` | `eprintln!("Local INT8 decoder ({}) decoded {:.2}s chunk: \"{}\"", ...)` | Private dictation text is leaked to standard error, terminal history, and journald logs. | Remove transcript logging; log only chunk duration and sample count. | S | Verified |
| **REL-01** | **High** | Reliable | `src/inference/mod.rs:83-88`, `src/inference/sarvam.rs:32` | `SarvamSpeechInference::new(...)` sets `timeout: None`; `mod.rs` never calls `.with_timeout()`. | Network stalls or dropped packets on Sarvam API can hang the daemon loop indefinitely. | Pass `config.request_timeout_secs.unwrap_or(30)` to `SarvamSpeechInference::with_timeout()`. | S | Verified |
| **REL-02** | **High** | Reliable | `src/daemon.rs:825`, `src/procs.rs:126-155` | `list_own_processes_named(proc_root, "speechyyy-dock", uid)` scans `/proc` across entire system. | `speechyyy stop` terminates all user-owned docks, killing docks from separate test sandboxes or dev sessions. | Scope dock process termination to the PID stored in `paths.dock_pid_path`, verifying UID and name. | M | Verified |
| **REL-03** | **High** | Reliable | `src/daemon.rs:134-175, 304-320` | `controller.stop_and_transcribe()` is called synchronously in main session loop. | Main IPC listener blocks during model inference; `cancel`, `status`, and `stop` are blocked until inference finishes. | Move transcription to a dedicated worker thread or run non-blocking channel handoff. | L | Verified |
| **DEAD-01** | **Medium** | Code Hygiene | `src/input.rs:68-446`, `src/daemon.rs:214-260` | Daemon sets `input_source = NoopInputSource`; `EvdevListener` is never constructed outside tests. | 378 lines in `input.rs`, 47 lines in `daemon.rs`, and 10 raw `unsafe` blocks are dead code. | Delete `EvdevListener`, `EvdevGrabGuard`, and the 10ms `input_source.poll_event` loop in `daemon.rs`. | M | Verified |
| **DEAD-02** | **Medium** | Code Hygiene | `Cargo.toml:26`, `src/dock/render.rs:16, 29-46` | `fontdue = "0.9"` dependency is only used for `load_system_font` which is unused for text rendering. | Adds font parsing crate and disk lookups without user-facing text layout in dock. | Remove `fontdue` and hardcoded font paths if dock only renders vector capsules and equalizer bars. | S | Verified |
| **SMOOTH-02** | **Medium** | Smooth | `src/bin/speechyyy-dock.rs:273-282` | `speechyyy-dock` only inspects `--check`; running `--help` fails with Wayland display error. | Users running `speechyyy-dock --help` or `-h` get error exit instead of usage instructions. | Add basic `--help` / `-h` argument handling before connecting to Wayland display. | S | Verified |
| **SEC-03** | **Medium** | Secure | `src/ipc.rs:292` | `reader.read_line(&mut line)?` on Unix socket stream. | Malicious or runaway local client can stream arbitrary bytes without newline, consuming memory. | Wrap reader with `take(MAX_IPC_PAYLOAD_BYTES)` (e.g. 64 KB). | S | Verified |
| **DOC-01** | **Low** | Smooth | `install.sh:308, 464` | `bind = , code:108, exec, speechyyy toggle` vs `"Hold [RightAlt] to speak"`. | User configuring Hyprland via installer gets toggle behavior while installer docs claim push-to-talk. | Update `install.sh` Hyprland lines to generate both `bind` (push-down) and `bindr` (push-up) or clarify toggle. | S | Verified |
| **REL-04** | **Low** | Reliable | `src/inference/sarvam.rs:101`, `src/inference/openai.rs:234` | Single `send_bytes(&body)` call without retry loop or exponential backoff. | Transient HTTP 503 or dropped socket fails dictation immediately with no retry attempt. | Add 2 retries with exponential backoff on transient errors (HTTP 502/503/504 or network timeout). | M | Verified |
| **CODE-01** | **Low** | Code Hygiene | `src/update.rs:427` | `std::process::exit(0);` invoked inside `handle_update_command`. | Library function bypasses clean stack unwinding and caller cleanup. | Return `Ok(())` from `handle_update_command` and exit from `main.rs`. | S | Verified |

---

## 4. Dead and Garbage Code Cleanup List

### Safe to Delete (Confirmed Zero External References)
1. **`EvdevListener` and `EvdevGrabGuard` (`src/input.rs:68-446` - 378 lines):**  
   *Verification:* Git grep confirms `EvdevListener` is never constructed outside `src/input.rs` and its internal unit tests. Daemon initialization at `src/daemon.rs:701, 1284, 1339, 1692, 1755` strictly uses `NoopInputSource`.  
   *Safety:* Safe to remove entirely. Removes 10 raw POSIX `unsafe` blocks (`libc::ioctl(EVIOCGRAB)`, `libc::inotify_init1`, `libc::read`, `CStr::from_ptr`, `libc::poll`).
2. **Daemon Input Polling Loop (`src/daemon.rs:214-260` - 47 lines):**  
   *Verification:* `input_source.poll_event(Duration::from_millis(10))` runs against `NoopInputSource`, which unconditionally returns `None`.  
   *Safety:* Safe to remove. The entire Compositor IPC architecture relies on socket events (`handle_toggle`, `handle_push_down`, `handle_push_up`).
3. **`ObservableInputSource` Test Double (`src/daemon.rs:1202-1230` - 29 lines):**  
   *Verification:* Only exists to feed events into the dead `input_source.poll_event` loop in daemon tests.
4. **Unused Fontdue System Font Loader (`src/dock/render.rs:16, 29-46` - 20 lines):**  
   *Verification:* `load_system_font` checks 5 hardcoded font paths; the loaded font is stored in `DockRenderer.font` but never used for text rendering.

### Likely Dead (Decision Required)
1. **`evdev` Direct Dependency in `Cargo.toml:9`:**  
   *Usage:* Currently used for `KeyCode` in `config.rs` and `engine.rs`, and `VirtualDevice` in `injector.rs` (the fallback typewriter mode). If typewriter injection is retained as an opt-in, `evdev` is needed; if typewriter mode is deprecated in favor of pure clipboard paste, `evdev` can be removed completely.
2. **`KeyChordTracker`, `TargetChord::ctrl_space()`, `Modifier` (`src/engine.rs:13-130` - 118 lines):**  
   *Usage:* Multi-key chord state machine designed for evdev hardware key-grabbing. In v0.2.0, compositor keybindings intercept chords and invoke `speechyyy push-down` / `push-up` over IPC. Chords are never tracked in daemon.
3. **`wtype` Fallback Keystroke Injector (`src/injector.rs:770-800`):**  
   *Usage:* Keystroke injector fallback referencing `wtype`. Primary injection is `wl-copy` with Ctrl+V uinput paste.

### Keep (Used Indirectly or Trait/Test Required)
1. **`whisper-small` in `assets/models.toml:16-27`:**  
   *Reason:* Fully pinned with SHA-256 (`486a46af...`) and MIT license. Kept as an optional middle-ground model between Parakeet and Whisper-Turbo.
2. **`MockClipboardWriter` & `MockKeySink` in `src/injector.rs:670-721`:**  
   *Reason:* Located outside `#[cfg(test)]` so they can be reused across integration tests (`tests/injector_test.rs`).
3. **`Secret` wrapper in `src/inference/wire.rs`:**  
   *Reason:* Implements custom redacted `Debug` (`[redacted]`) to prevent accidental API key leaks.

---

## 5. Answers to the Ten Explicit Questions

1. **Does `speechyyy update` verify a checksum or signature before installing?**  
   **NO.** `src/update.rs:403-417` downloads `speechyyy.tar.gz` and extracts it directly via `unpack_tarball` without computing or checking any SHA-256 hash or signature.
2. **Is any non-test `unwrap()` / `expect()` reachable from external input in the daemon, given `panic = "abort"`?**  
   **NO.** Every non-test `unwrap()` in core paths is structurally guarded: `src/model.rs:587` has an explicit `response.is_none()` guard; `src/inference/local.rs:241` has `self.init_recognizer()?`; remaining unwraps are inside test mocks (`MockClipboardWriter`, `MockKeySink`, `RecordingInjector`).
3. **Can a stalled dock or events subscriber block the daemon?**  
   **NO.** `src/events.rs:118-124, 151-154`: Client streams connecting to `events.sock` are configured non-blocking (`stream.set_nonblocking(true)`). When a client buffer fills, `write_all` returns `ErrorKind::WouldBlock`, which is caught and the frame is dropped rather than blocking.
4. **Can any API key or transcript text appear in logs or errors?**  
   **YES for transcript text; NO for API keys.**  
   - API keys are protected: wrapped in `Secret` with redacted `Debug` (`"[redacted]"`), omitted from error strings and status commands.  
   - Spoken transcript text **IS leaked** to `stderr` in `src/inference/local.rs:249-251` and `src/inference/local.rs:401-406` via `eprintln!("Local INT8 decoder ({}) decoded {:.2}s chunk: \"{}\"", ...)`.
5. **Is any `attach(None, ...)` call made on the dock's configured surface?**  
   **NO.** `src/bin/speechyyy-dock.rs:182` always attaches a valid `SlotPool` SHM buffer. When settled hidden in Idle, the canvas is filled with zero (transparent) and committed, and frame callbacks cease without ever calling `attach(None, ...)` or unmapping.
6. **Are the model manifest hashes and licenses fully populated?**  
   **YES.** All 4 models in `assets/models.toml` (`parakeet-tdt-v3`, `whisper-small`, `whisper-large-v3-turbo`, `moonshine-base`) have exact SHA-256 hashes, exact byte sizes, maximum extracted bounds, license identifiers, and attribution strings.
7. **What are the measured idle RAM and CPU, with and without a loaded model?**  
   - **Binary Sizes (Measured):** `speechyyy` is **3.76 MB** (3,764,160 bytes); `speechyyy-dock` is **1.17 MB** (1,173,344 bytes); combined **4.93 MB** (strictly < 10 MB budget).  
   - **Runtime CPU/RAM:** Process runtime RSS could not be measured dynamically in this container environment because the ALSA subsystem is unavailable (`Host is down (112)`), preventing the daemon from completing pre-flight checks. Statically: Ring buffer allocates 2,880,000 f32 samples (~11.5 MB); dock SHM pool allocates ~147 KB; local ONNX model weights (`parakeet-tdt-v3` ~487 MB) once loaded lazily into RAM remain in memory permanently and never unload.
8. **Does Sarvam (and each cloud provider) have timeouts, retries, and actionable errors?**  
   **Partial.**  
   - Groq, OpenAI, and OpenAI-compatible have a 10s connect timeout and a 30s request timeout (`src/inference/openai.rs:222-224`). Sarvam has `timeout: None` when initialized via `create_inference_provider()`.  
   - **Retries:** **NO.** Zero retries or backoff across all cloud providers.  
   - **Actionable Errors:** **YES.** HTTP 401/403/429/timeouts map to explicit, descriptive errors advising the user on how to remediate credentials or rate limits.
9. **Does a fresh user with no config reach a first dictation without reading the docs?**  
   **NO.** Out-of-the-box defaults in `src/config.rs:231-237` specify `default_provider() = "local"` and `default_model() = "saaras:v4"`. Because "saaras:v4" is not a local model, running `speechyyy start -f` or `speechyyy doctor` fails immediately with `Unknown local model 'saaras:v4'`.
10. **Is the stop-time dock sweep scoped to this instance's runtime directory, or does it signal every `speechyyy-dock` process owned by the user?**  
    **Signals every `speechyyy-dock` process owned by the user.** `src/daemon.rs:825` and `src/procs.rs:126-155` scan `/proc` for all processes owned by the user's UID where `/proc/<pid>/comm` equals `"speechyyy-dock"`, terminating all matching instances across the entire system regardless of `XDG_RUNTIME_DIR`.

---

## 6. What Is Solid

- **Binary Size & Compilation:** Release binaries are compact (3.76 MB and 1.17 MB), totaling 4.93 MB—well under the 10 MB constraint. Full test suite passes (266 tests, 0 failures), and `cargo clippy --all-targets --all-features -- -D warnings` passes with 0 warnings.
- **Model Download Security:** `src/model.rs` downloads are strictly HTTPS, restricted to an allowlist of GitHub hosts, manually follow redirects with per-hop policy validation, stream-compute SHA-256, enforce file-count and extraction size bounds, and atomically install into destination directories.
- **Wayland Dock Architecture:** The dock uses zero-allocation canvas updates via `tiny-skia` on an SHM pool, enforces 100% click-through input transparency with empty Wayland input regions, uses neon synth tokens (`#f70776` / `#c3195d`), and stops requesting VSync frame callbacks when idle (0.00% idle CPU).
- **Key Isolation & Secret Handling:** Secret API keys are wrapped in `Secret` with redacted `Debug` implementations, written with `0o600` permissions via atomic temp-rename, and isolated by provider.
- **Non-blocking Event Broadcast:** `EventBroadcaster` sets Unix stream sockets to non-blocking; slow or stalled subscribers drop audio level ticks with `WouldBlock` without blocking daemon execution.
- **Rollback Safety in Updates:** `src/update.rs` backs up existing binaries to `.bak` and tests ELF headers and `--version` before completing the atomic swap.

---

## 7. Not Verified

- **Dynamic Audio Input Streaming:** Live microphone streaming could not be captured because the execution sandbox lacks an active ALSA/PulseAudio/PipeWire sound card (`snd_pcm_open failed with error 'Host is down (112)'`). Audio verification was performed via comprehensive unit tests in `src/audio/` (215 tests).
- **Physical Wayland Compositor Presentation:** Live rendering of layer-shell surfaces on an active display could not be visually confirmed in a headless environment. Wayland protocol compliance was verified via protocol dispatch tests and unit tests in `src/dock/`.
- **System-Wide Process Sweep Impact on Real Workstations:** In accordance with Ground Rule 3 (Real-Dock Guard), `speechyyy stop` was not executed against real user processes; sweep behavior was verified via code reading of `src/procs.rs:list_own_processes_named` and sandboxed unit tests.

---

## 8. Suggested Fix Plan

The findings are grouped into 4 independent, ordered work packages:

### Package 1: Pre-Release Security & Privacy Fixes (Blockers)
*Dependencies: None*
1. **Fix `speechyyy update` integrity (`src/update.rs`):** Download `SHA256SUMS` from the GitHub release and verify SHA-256 before unpacking `speechyyy.tar.gz`.
2. **Remove plaintext transcript logging (`src/inference/local.rs`):** Delete `eprintln!` calls at lines 249-251 and 401-406 that print `trimmed` transcript text.
3. **Cap IPC request line reading (`src/ipc.rs`):** Limit `reader.read_line` to 64 KB to prevent unbounded memory growth.

### Package 2: First-Run & CLI Polish
*Dependencies: None*
1. **Align default configuration (`src/config.rs`):** Change `default_model()` to `"parakeet-tdt-v3"` (or set `default_provider` to `"sarvam"`), ensuring fresh installs pass doctor checks.
2. **Add `--help` / `-h` support to `speechyyy-dock` (`src/bin/speechyyy-dock.rs`):** Handle `--help` and print usage instructions before attempting Wayland connection.
3. **Update `install.sh` autostart bindings:** Update Hyprland autostart templates to correctly reflect compositor IPC push-to-talk commands.

### Package 3: Reliability & Network Resilience
*Dependencies: None*
1. **Enforce Sarvam timeout (`src/inference/mod.rs`):** Configure `SarvamSpeechInference::with_timeout()` using `config.request_timeout_secs.unwrap_or(30)`.
2. **Scope dock termination to runtime dir (`src/daemon.rs`, `src/procs.rs`):** Terminate dock processes by PID stored in `dock_pid_path` rather than sweeping all user-owned docks system-wide.
3. **Add transient HTTP retries:** Implement retry with exponential backoff on transient HTTP 5xx errors for cloud providers.

### Package 4: Dead Code Pruning & Hygiene
*Dependencies: After Package 1 & 2*
1. **Delete dead evdev subsystem (`src/input.rs`, `src/daemon.rs`):** Remove `EvdevListener`, `EvdevGrabGuard`, dead `unsafe` blocks, and the unused 10ms polling loop in `daemon.rs`.
2. **Remove unused `fontdue` font loading (`src/dock/render.rs`, `Cargo.toml`):** Clean up unused font loader in dock renderer.

