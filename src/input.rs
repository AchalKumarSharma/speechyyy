use evdev::KeyCode;
use std::collections::VecDeque;
use std::time::Duration;

pub trait InputEventSource {
    fn poll_event(&mut self, timeout: Duration) -> Option<(KeyCode, bool)>;

    fn grab(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn ungrab(&mut self) -> Result<(), String> {
        Ok(())
    }
}

pub struct FakeInputSource {
    events: VecDeque<(KeyCode, bool)>,
    pub grabbed: bool,
}

impl FakeInputSource {
    pub fn new() -> Self {
        Self {
            events: VecDeque::new(),
            grabbed: false,
        }
    }

    pub fn push_event(&mut self, key: KeyCode, is_down: bool) {
        self.events.push_back((key, is_down));
    }
}

impl Default for FakeInputSource {
    fn default() -> Self {
        Self::new()
    }
}

impl InputEventSource for FakeInputSource {
    fn poll_event(&mut self, _timeout: Duration) -> Option<(KeyCode, bool)> {
        self.events.pop_front()
    }

    fn grab(&mut self) -> Result<(), String> {
        self.grabbed = true;
        Ok(())
    }

    fn ungrab(&mut self) -> Result<(), String> {
        self.grabbed = false;
        Ok(())
    }
}

/// No-op input source for compositor IPC mode where the daemon does not iterate /dev/input/event*
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopInputSource;

impl InputEventSource for NoopInputSource {
    fn poll_event(&mut self, _timeout: Duration) -> Option<(KeyCode, bool)> {
        None
    }

    fn grab(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn ungrab(&mut self) -> Result<(), String> {
        Ok(())
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

        assert_eq!(
            source.poll_event(Duration::ZERO),
            Some((KeyCode::KEY_RIGHTALT, true))
        );
        assert_eq!(
            source.poll_event(Duration::ZERO),
            Some((KeyCode::KEY_RIGHTALT, false))
        );
        assert_eq!(source.poll_event(Duration::ZERO), None);
    }

    #[test]
    fn test_fake_input_source_grab() {
        let mut source = FakeInputSource::new();
        assert!(!source.grabbed);
        assert!(source.grab().is_ok());
        assert!(source.grabbed);
        assert!(source.ungrab().is_ok());
        assert!(!source.grabbed);
    }

    #[test]
    fn test_noop_input_source() {
        let mut source = NoopInputSource;
        assert_eq!(source.poll_event(Duration::from_millis(10)), None);
        assert!(source.grab().is_ok());
        assert!(source.ungrab().is_ok());
    }
}
