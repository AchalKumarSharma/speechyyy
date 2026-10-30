use evdev::{Device, EventType, KeyCode};
use std::collections::VecDeque;
use std::ffi::{CStr, CString};
use std::fs;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub trait InputEventSource {
    fn poll_event(&mut self, timeout: Duration) -> Option<(KeyCode, bool)>;
}

pub struct FakeInputSource {
    events: VecDeque<(KeyCode, bool)>,
}

impl FakeInputSource {
    pub fn new() -> Self {
        Self {
            events: VecDeque::new(),
        }
    }

    pub fn push_event(&mut self, key: KeyCode, is_down: bool) {
        self.events.push_back((key, is_down));
    }
}

impl InputEventSource for FakeInputSource {
    fn poll_event(&mut self, _timeout: Duration) -> Option<(KeyCode, bool)> {
        self.events.pop_front()
    }
}

pub fn is_keyboard_device(device: &Device) -> bool {
    // Exclude our own virtual keyboard
    if let Some(name) = device.name() {
        if name.contains("Speechyyy") {
            return false;
        }
    }

    if let Some(keys) = device.supported_keys() {
        keys.contains(KeyCode::KEY_A)
            && keys.contains(KeyCode::KEY_SPACE)
            && keys.contains(KeyCode::KEY_ENTER)
    } else {
        false
    }
}

pub struct EvdevListener {
    input_dir: PathBuf,
    devices: Vec<Device>,
    inotify_fd: Option<i32>,
    pending_events: VecDeque<(KeyCode, bool)>,
}

impl EvdevListener {
    pub fn new() -> Self {
        Self::with_dir(PathBuf::from("/dev/input"))
    }

    pub fn with_dir(dir: PathBuf) -> Self {
        let mut listener = Self {
            input_dir: dir.clone(),
            devices: Vec::new(),
            inotify_fd: None,
            pending_events: VecDeque::new(),
        };

        listener.init_inotify(&dir);
        listener.scan_devices();
        listener
    }

    fn init_inotify(&mut self, dir: &Path) {
        let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        if fd >= 0 {
            if let Ok(c_path) = CString::new(dir.to_string_lossy().as_bytes()) {
                let wd = unsafe { libc::inotify_add_watch(fd, c_path.as_ptr(), libc::IN_CREATE) };
                if wd >= 0 {
                    self.inotify_fd = Some(fd);
                    return;
                }
            }
            unsafe {
                libc::close(fd);
            }
        }
    }

    pub fn scan_devices(&mut self) {
        self.devices.clear();
        if let Ok(entries) = fs::read_dir(&self.input_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                    if file_name.starts_with("event") {
                        if let Ok(device) = Device::open(&path) {
                            if is_keyboard_device(&device) {
                                self.devices.push(device);
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn check_inotify_new_devices(&mut self) {
        if let Some(ifd) = self.inotify_fd {
            let mut buf = [0u8; 1024];
            let n = unsafe { libc::read(ifd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
            if n > 0 {
                let mut offset = 0;
                while offset < n as usize {
                    let event = unsafe { &*(buf.as_ptr().add(offset) as *const libc::inotify_event) };
                    let name_len = event.len as usize;
                    if name_len > 0 {
                        let name_ptr = unsafe { buf.as_ptr().add(offset + std::mem::size_of::<libc::inotify_event>()) };
                        let name = unsafe { CStr::from_ptr(name_ptr as *const libc::c_char) }
                            .to_string_lossy();
                        if name.starts_with("event") {
                            let path = self.input_dir.join(name.as_ref());
                            std::thread::sleep(Duration::from_millis(50));
                            if let Ok(device) = Device::open(&path) {
                                if is_keyboard_device(&device) {
                                    self.devices.push(device);
                                }
                            }
                        }
                    }
                    offset += std::mem::size_of::<libc::inotify_event>() + name_len;
                }
            }
        }
    }
}

impl Drop for EvdevListener {
    fn drop(&mut self) {
        if let Some(fd) = self.inotify_fd {
            unsafe {
                libc::close(fd);
            }
        }
    }
}

impl InputEventSource for EvdevListener {
    fn poll_event(&mut self, timeout: Duration) -> Option<(KeyCode, bool)> {
        if let Some(ev) = self.pending_events.pop_front() {
            return Some(ev);
        }

        let mut pollfds = Vec::new();
        if let Some(ifd) = self.inotify_fd {
            pollfds.push(libc::pollfd {
                fd: ifd,
                events: libc::POLLIN,
                revents: 0,
            });
        }

        for dev in &self.devices {
            pollfds.push(libc::pollfd {
                fd: dev.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            });
        }

        if pollfds.is_empty() {
            std::thread::sleep(timeout);
            return None;
        }

        let timeout_ms = timeout.as_millis() as libc::c_int;
        let poll_res = unsafe {
            libc::poll(pollfds.as_mut_ptr(), pollfds.len() as libc::nfds_t, timeout_ms)
        };

        if poll_res <= 0 {
            return None;
        }

        let mut poll_idx = 0;
        if self.inotify_fd.is_some() {
            if (pollfds[0].revents & libc::POLLIN) != 0 {
                self.check_inotify_new_devices();
            }
            poll_idx += 1;
        }

        let mut to_remove = Vec::new();
        for (i, dev) in self.devices.iter_mut().enumerate() {
            let pfd = &pollfds[poll_idx + i];
            if (pfd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL)) != 0 {
                to_remove.push(i);
                continue;
            }

            if (pfd.revents & libc::POLLIN) != 0 {
                match dev.fetch_events() {
                    Ok(events) => {
                        for ev in events {
                            if ev.event_type() == EventType::KEY {
                                let is_down = ev.value() == 1;
                                if ev.value() == 1 || ev.value() == 0 {
                                    self.pending_events.push_back((KeyCode(ev.code()), is_down));
                                }
                            }
                        }
                    }
                    Err(e) if e.raw_os_error() == Some(libc::ENODEV) => {
                        to_remove.push(i);
                    }
                    Err(_) => {}
                }
            }
        }

        for idx in to_remove.into_iter().rev() {
            self.devices.remove(idx);
        }

        self.pending_events.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fake_input_source_fifo() {
        let mut source = FakeInputSource::new();
        source.push_event(KeyCode::KEY_RIGHTALT, true);
        source.push_event(KeyCode::KEY_RIGHTALT, false);

        assert_eq!(source.poll_event(Duration::ZERO), Some((KeyCode::KEY_RIGHTALT, true)));
        assert_eq!(source.poll_event(Duration::ZERO), Some((KeyCode::KEY_RIGHTALT, false)));
        assert_eq!(source.poll_event(Duration::ZERO), None);
    }

    #[test]
    fn test_evdev_listener_with_empty_temp_dir() {
        let temp_dir = std::env::temp_dir().join(format!("speechyyy_evdev_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);

        let mut listener = EvdevListener::with_dir(temp_dir.clone());
        assert_eq!(listener.devices.len(), 0);
        assert_eq!(listener.poll_event(Duration::from_millis(5)), None);

        let _ = fs::remove_dir_all(temp_dir);
    }
}
