pub mod physics;
pub mod render;
pub mod state;

pub use physics::{calculate_bar_targets, Spring, WaveformSprings};
pub use render::DockRenderer;
pub use state::{DockModel, HIDE_OPACITY_EPSILON, HIDE_WIDTH_EPSILON};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockEventAction {
    Continue,
    Exit,
    Reconnect,
}

pub fn handle_daemon_event_action(event: &crate::events::DaemonEvent) -> DockEventAction {
    match event {
        crate::events::DaemonEvent::Shutdown { restart: false } => DockEventAction::Exit,
        crate::events::DaemonEvent::Shutdown { restart: true } => DockEventAction::Reconnect,
        _ => DockEventAction::Continue,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{DaemonEvent, DictationState};

    #[test]
    fn test_dock_event_action_decision() {
        assert_eq!(
            handle_daemon_event_action(&DaemonEvent::Shutdown { restart: false }),
            DockEventAction::Exit
        );
        assert_eq!(
            handle_daemon_event_action(&DaemonEvent::Shutdown { restart: true }),
            DockEventAction::Reconnect
        );
        assert_eq!(
            handle_daemon_event_action(&DaemonEvent::StateChanged {
                state: DictationState::Recording,
                previous: None,
            }),
            DockEventAction::Continue
        );
        assert_eq!(
            handle_daemon_event_action(&DaemonEvent::audio_level(0.5, 0.8)),
            DockEventAction::Continue
        );
        assert_eq!(
            handle_daemon_event_action(&DaemonEvent::Canceled {
                reason: "test".into()
            }),
            DockEventAction::Continue
        );
    }
}

