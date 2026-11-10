# Root-Cause Analysis: Dock Auto-Launch Failure & IPC Broken Pipe

**Date:** 2026-11-09  
**Branch:** `refactor/v0.2.0-clean-arch`  
**Status:** Complete  

---

## 1. Executive Summary & Observed Symptoms

During manual testing of speechyyy v0.2.0 (`refactor/v0.2.0-clean-arch`) on Linux / Wayland:
1. Running `./target/release/speechyyy start -f` started the daemon and captured microphone audio, but `speechyyy-dock` never appeared on the Wayland display. However, running `./target/release/speechyyy-dock` directly in a separate terminal worked properly.
2. After speaking for 6.90 seconds, the local engine logged:
   ```text
   Local INT8 decoder (Parakeet-TDT-v3 (0.6B INT8)) decoded 6.90s chunk (110421 samples)
   ```
   Immediately followed by four repeated errors:
   ```text
   IPC connection error: Broken pipe (os error 32)
   IPC connection error: Broken pipe (os error 32)
   IPC connection error: Broken pipe (os error 32)
   IPC connection error: Broken pipe (os error 32)
   ```
   No text was injected into the active window.

This document details the root causes and outlines the minimal targeted fixes.

---

## 2. Issue 1: Dock Auto-Launch Path

### Why the Dock Was Not Launched
The companion dock was skipped because running in foreground mode (`-f` or `--foreground`) currently makes launching the dock **opt-in** via `--with-dock`, defaulting to `false`.

In [`src/daemon.rs`](file:///home/alfred/Projects/speechyyy/src/daemon.rs):
- **Background Mode ([`src/daemon.rs:599-631`](file:///home/alfred/Projects/speechyyy/src/daemon.rs#L599-L631)):** Dock auto-launch is enabled by default. It executes `should_launch_dock(no_dock, ...)` directly.
- **Foreground Mode ([`src/daemon.rs:658-691`](file:///home/alfred/Projects/speechyyy/src/daemon.rs#L658-L691)):** The entire dock decision and spawn logic is wrapped inside:
  ```rust
  if with_dock {
      let wayland_display = std::env::var("WAYLAND_DISPLAY").ok();
      let current_exe = std::env::current_exe().ok();
      let dock_bin = resolve_dock_binary(current_exe.as_deref());
      let proc_root = Path::new("/proc");
      let dock_running = is_dock_currently_running(paths, proc_root);

      let decision = should_launch_dock(
          no_dock,
          wayland_display.as_deref(),
          dock_running,
          dock_bin,
      );
      ...
  }
  ```

In [`src/cli.rs:38-40`](file:///home/alfred/Projects/speechyyy/src/cli.rs#L38-L40), `--with-dock` is defined as:
```rust
/// Launch companion dock even when running in foreground mode
#[arg(long)]
with_dock: bool,
```
When running `./target/release/speechyyy start -f`, `with_dock` was `false`. Lines 658–691 were completely bypassed. Because there is no `else` block or fallback log for `!with_dock`, the daemon silently skipped launching the dock without printing any message.

### Executable Resolution
The daemon **does** find `./target/release/speechyyy-dock` adjacent to `./target/release/speechyyy` even if `target/release` is not on `$PATH`.
In [`src/daemon.rs:424-442`](file:///home/alfred/Projects/speechyyy/src/daemon.rs#L424-L442):
```rust
pub fn resolve_dock_binary(current_exe: Option<&Path>) -> Option<PathBuf> {
    if let Some(exe) = current_exe {
        if let Some(parent) = exe.parent() {
            let neighbor = parent.join("speechyyy-dock");
            if neighbor.is_file() {
                return Some(neighbor);
            }
        }
    }
    which::which("speechyyy-dock").ok()
}
```
It checks `exe.parent().join("speechyyy-dock")` first before falling back to `which::which("speechyyy-dock")`.

### Spawn Error Handling
In [`src/daemon.rs:477-484`](file:///home/alfred/Projects/speechyyy/src/daemon.rs#L477-L484):
```rust
Err(e) => {
    eprintln!(
        "Warning: Failed to launch companion dock: {}. Run 'speechyyy doctor' to verify GUI dependencies.",
        e
    );
    None
}
```
If `Command::spawn()` fails, it is not silently ignored; it logs a warning to `stderr`. In this incident, no error was logged because `spawn_dock_process` was never called.

---

## 3. Issue 2: IPC Broken Pipe (`os error 32`)

### Where `IPC connection error: Broken pipe` Is Emitted
This error is emitted strictly by the command socket listener in [`src/daemon.rs:207`](file:///home/alfred/Projects/speechyyy/src/daemon.rs#L207):
```rust
Err(e) => {
    eprintln!("IPC connection error: {}", e);
}
```
It is **not** emitted by `events.sock`. In [`src/events.rs:155-162`](file:///home/alfred/Projects/speechyyy/src/events.rs#L155-L162), `EventBroadcaster` catches `BrokenPipe`, `ConnectionReset`, and `EPIPE` and silently prunes disconnected subscribers without logging.

### Root Cause & Sequence of Events
The broken pipe errors occurred due to **client read timeouts during synchronous blocking inference on the daemon main thread**:

```
[User/Compositor]                      [Daemon Main Thread]                    [Linux Kernel Socket Backlog]
       |                                       |                                             |
       |--- PushUp --------------------------->| (Calls stop_and_transcribe())               |
       |                                       | [Inference Running on CPU for 6.90s chunk]  |
       |                                       | (Thread blocked for ~2.5 - 4.0s)            |
       |                                       |                                             |
       |--- PushDown / Up / Status (Retry 1) ->|                                             |---> Enqueued in backlog
       |--- PushDown / Up / Status (Retry 2) ->|                                             |---> Enqueued in backlog
       |--- PushDown / Up / Status (Retry 3) ->|                                             |---> Enqueued in backlog
       |--- PushDown / Up / Status (Retry 4) ->|                                             |---> Enqueued in backlog
       |                                       |                                             |
       | [2.0s Client Timeout Expires]         |                                             |
       | (Clients close socket & exit) ------->|                                             |---> Socket half-closed (FIN)
       |                                       |                                             |
       |                                       | [Inference Completes!]                      |
       |                                       | Local INT8 decoder decoded 6.90s chunk      |
       |                                       |                                             |
       |                                       | accepts connection 1 from backlog           |
       |                                       | writes response -> fails with EPIPE (os 32) |
       |                                       | accepts connection 2 from backlog           |
       |                                       | writes response -> fails with EPIPE (os 32) |
       |                                       | accepts connection 3 from backlog           |
       |                                       | writes response -> fails with EPIPE (os 32) |
       |                                       | accepts connection 4 from backlog           |
       |                                       | writes response -> fails with EPIPE (os 32) |
```

1. **Synchronous Inference Freeze ([`src/daemon.rs:137`](file:///home/alfred/Projects/speechyyy/src/daemon.rs#L137)):**
   When the user released the hotkey, the compositor sent `push-up`. In [`src/daemon.rs:137`](file:///home/alfred/Projects/speechyyy/src/daemon.rs#L137), the daemon's single thread called `controller.stop_and_transcribe()`.
   Decoding a 6.90s audio chunk with `Parakeet-TDT-v3 (0.6B INT8)` on CPU took **> 2.0 seconds**.
   While `stop_and_transcribe()` was executing, the daemon main loop was blocked and could **not** call `listener.accept()`.

2. **Client Connections Enqueued in Socket Backlog:**
   Because the dock was not running, the user received no visual feedback and pressed the hotkey again or ran commands (e.g. `speechyyy push-down`, `speechyyy push-up`, `speechyyy toggle`, or `speechyyy status`).
   Each CLI command invoked [`IpcClient::send_request`](file:///home/alfred/Projects/speechyyy/src/ipc.rs#L188-L213). The Linux kernel accepted the Unix domain socket connections into the socket listen backlog queue.

3. **Client Read Timeout ([`src/ipc.rs:198`](file:///home/alfred/Projects/speechyyy/src/ipc.rs#L198)):**
   [`IpcClient::send_request`](file:///home/alfred/Projects/speechyyy/src/ipc.rs#L198) sets a strict 2-second timeout:
   ```rust
   stream.set_read_timeout(Some(Duration::from_secs(2)))
   ```
   Because the daemon was blocked on CPU inference for >2 seconds, all queued client commands timed out waiting for a response. The CLI client processes exited, causing the kernel to close their sockets (sending `FIN`/`RST`).

4. **Daemon Resumes and Writes to Dead Sockets ([`src/ipc.rs:358-359`](file:///home/alfred/Projects/speechyyy/src/ipc.rs#L358-L359)):**
   As soon as `stop_and_transcribe()` finished, the daemon resumed its event loop and accepted the four queued connections one by one.
   In [`src/ipc.rs:358-359`](file:///home/alfred/Projects/speechyyy/src/ipc.rs#L358-L359):
   ```rust
   writer.write_all(out.as_bytes())?;
   writer.flush()?;
   ```
   Writing to sockets whose client processes had already closed resulted in `std::io::ErrorKind::BrokenPipe` (`os error 32`).
   `handle_connection` returned `Err(e)`, and [`src/daemon.rs:207`](file:///home/alfred/Projects/speechyyy/src/daemon.rs#L207) printed:
   `IPC connection error: Broken pipe (os error 32)` four times.

---

## 4. Issue 3: Text Injection Execution

### Why No Text Appeared in the Active Window
Following `stop_and_transcribe()`, text injection proceeded via [`src/daemon.rs:145-147`](file:///home/alfred/Projects/speechyyy/src/daemon.rs#L145-L147):
```rust
if let Err(e) = injector.inject(&text) {
    eprintln!("Failed to inject text: {}", e);
}
```
No error was printed to stderr, meaning `injector.inject(&text)` returned `Ok(())`.

By default, the daemon configures [`WaylandClipboardInjector`](file:///home/alfred/Projects/speechyyy/src/injector.rs#L790-L835) (`config.injection_mode = Clipboard`):
1. **Clipboard Write ([`src/injector.rs:798`](file:///home/alfred/Projects/speechyyy/src/injector.rs#L798)):** Invokes `copy_to_wl_clipboard(text)`, which spawned `wl-copy --sensitive` and wrote the text to the Wayland clipboard. This completed successfully.
2. **Settle Delay ([`src/injector.rs:807-809`](file:///home/alfred/Projects/speechyyy/src/injector.rs#L807-L809)):** Sleeps 15ms.
3. **Synthetic Paste Keystrokes ([`src/injector.rs:811-832`](file:///home/alfred/Projects/speechyyy/src/injector.rs#L811-L832)):** Emits `KEY_LEFTCTRL` press, `KEY_V` press, `KEY_V` release, `KEY_LEFTCTRL` release via `/dev/uinput`.

The text did not appear for two primary reasons:
1. **Terminal Target Mismatch:** In Linux terminal emulators (Kitty, Alacritty, Foot, GNOME Terminal), `Ctrl+V` is `lnext` (verbatim escape input) and does **not** paste clipboard content (terminals require `Ctrl+Shift+V` or bracketed paste). If testing was performed into a terminal window or the terminal running the daemon had focus, `Ctrl+V` was ignored or swallowed.
2. **Zero Hold Time on Keystroke Chord:** Unlike [`emit_defuse_and_paste`](file:///home/alfred/Projects/speechyyy/src/injector.rs#L354-L375) (which holds the chord for 20ms via `PASTE_HOLD_DELAY`), [`WaylandClipboardInjector::inject`](file:///home/alfred/Projects/speechyyy/src/injector.rs#L811-L832) emits the press and release of `KEY_LEFTCTRL` and `KEY_V` with **0ms delay**. On fast hardware, Wayland compositors / libinput can miss the modifier chord if down/up events occur within microseconds.

---

## 5. Minimal Targeted Fixes

### Fix 1: Dock Launch in Foreground Mode
In [`src/daemon.rs:658`](file:///home/alfred/Projects/speechyyy/src/daemon.rs#L658), make foreground mode launch the dock by default unless `--no-dock` is explicitly passed (matching background mode behavior):

```diff
--- a/src/daemon.rs
+++ b/src/daemon.rs
@@ -655,7 +655,7 @@ pub fn start_daemon(
             }
         }
     } else {
-        if with_dock {
+        if !no_dock {
             let wayland_display = std::env::var("WAYLAND_DISPLAY").ok();
             let current_exe = std::env::current_exe().ok();
             let dock_bin = resolve_dock_binary(current_exe.as_deref());
```

### Fix 2: IPC Resilience During Inference
1. **Increase Client Timeout ([`src/ipc.rs:198`](file:///home/alfred/Projects/speechyyy/src/ipc.rs#L198)):** Increase `stream.set_read_timeout` from `2s` to `15s` to accommodate local CPU inference on multi-second audio chunks:
   ```diff
   --- a/src/ipc.rs
   +++ b/src/ipc.rs
   @@ -195,7 +195,7 @@ impl IpcClient {
            })?;

            stream
   -            .set_read_timeout(Some(Duration::from_secs(2)))
   +            .set_read_timeout(Some(Duration::from_secs(15)))
                .map_err(|e| e.to_string())?;
   ```
2. **Handle Client Disconnects Gracefully ([`src/daemon.rs:206-209`](file:///home/alfred/Projects/speechyyy/src/daemon.rs#L206-L209)):** Do not log an alarming error when a client disconnects before its response is flushed:
   ```diff
   --- a/src/daemon.rs
   +++ b/src/daemon.rs
   @@ -204,6 +204,8 @@ pub fn run_event_loop<E: DictationEngine, I: TextInjector, IN: InputSource>(
                            is_restart = false;
                        }
                        Ok(Some(_)) | Ok(None) => {}
   +                    Err(ref e) if e.kind() == std::io::ErrorKind::BrokenPipe => {}
                        Err(e) => {
                            eprintln!("IPC connection error: {}", e);
                        }
   ```

### Fix 3: Wayland Clipboard Paste Keystroke
In [`src/injector.rs:790-835`](file:///home/alfred/Projects/speechyyy/src/injector.rs#L790-L835), use [`emit_defuse_and_paste`](file:///home/alfred/Projects/speechyyy/src/injector.rs#L354-L375) (or apply `PASTE_HOLD_DELAY`) to guarantee that compositors register the `Ctrl+V` chord reliably:
```diff
--- a/src/injector.rs
+++ b/src/injector.rs
@@ -808,27 +808,12 @@ impl TextInjector for WaylandClipboardInjector {
             thread::sleep(self.settle_delay);
         }

-        // 3. Emit KEY_LEFTCTRL press -> KEY_V press -> KEY_V release -> KEY_LEFTCTRL release
-        if let Err(e) = self.key_sink.emit_key(KeyCode::KEY_LEFTCTRL, true) {
-            eprintln!("Clipboard paste warning: failed to emit KEY_LEFTCTRL press: {}", e);
-            return Err(e);
-        }
-
-        if let Err(e) = self.key_sink.emit_key(KeyCode::KEY_V, true) {
-            eprintln!("Clipboard paste warning: failed to emit KEY_V press: {}", e);
-            let _ = self.key_sink.emit_key(KeyCode::KEY_LEFTCTRL, false);
-            return Err(e);
-        }
-
-        if let Err(e) = self.key_sink.emit_key(KeyCode::KEY_V, false) {
-            eprintln!("Clipboard paste warning: failed to emit KEY_V release: {}", e);
-            let _ = self.key_sink.emit_key(KeyCode::KEY_LEFTCTRL, false);
-            return Err(e);
-        }
-
-        if let Err(e) = self.key_sink.emit_key(KeyCode::KEY_LEFTCTRL, false) {
-            eprintln!("Clipboard paste warning: failed to emit KEY_LEFTCTRL release: {}", e);
-            return Err(e);
-        }
+        // 3. Emit paste chord with hold delay
+        self.key_sink.emit_key(KeyCode::KEY_LEFTCTRL, true)?;
+        self.key_sink.emit_key(KeyCode::KEY_V, true)?;
+        thread::sleep(Self::PASTE_HOLD_DELAY);
+        self.key_sink.emit_key(KeyCode::KEY_V, false)?;
+        self.key_sink.emit_key(KeyCode::KEY_LEFTCTRL, false)?;

         Ok(())
     }
```
*Note for testing in terminal emulators:* Remind users testing speechyyy inside terminal windows to use typewriter injection mode (`--injection typewriter`) or paste manually (`Ctrl+Shift+V`), as standard Linux terminals do not accept `Ctrl+V` as clipboard paste.

