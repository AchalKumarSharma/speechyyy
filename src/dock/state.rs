use crate::dock::physics::{Spring, WaveformSprings};
use crate::events::{DaemonEvent, DictationState};
use std::time::{Duration, Instant};

pub const RESTING_BAR_HEIGHT: f32 = 4.0;
pub const MAX_BAR_HEIGHT: f32 = 26.0;
pub const PILL_HEIGHT: f32 = 38.0;

pub const ACTIVE_PILL_WIDTH: f32 = 180.0;
pub const PROCESSING_PILL_WIDTH: f32 = 140.0;
pub const STANDBY_PILL_WIDTH: f32 = 110.0;

#[derive(Debug)]
pub struct DockModel {
    pub state: DictationState,
    pub width_spring: Spring,
    pub opacity_spring: Spring,
    pub bar_springs: WaveformSprings,
    pub current_rms: f32,
    pub error_text: Option<String>,
    pub error_started_at: Option<Instant>,
    pub emitting_started_at: Option<Instant>,
    pub elapsed_time: f32,
}

impl Default for DockModel {
    fn default() -> Self {
        Self::new()
    }
}

impl DockModel {
    pub fn new() -> Self {
        let bar_springs = WaveformSprings::with_params(RESTING_BAR_HEIGHT, 300.0, 0.75);
        Self {
            state: DictationState::Idle,
            width_spring: Spring::new(STANDBY_PILL_WIDTH, 260.0, 0.75),
            opacity_spring: Spring::new(1.0, 280.0, 0.80),
            bar_springs,
            current_rms: 0.0,
            error_text: None,
            error_started_at: None,
            emitting_started_at: None,
            elapsed_time: 0.0,
        }
    }

    pub fn handle_event(&mut self, event: DaemonEvent) {
        match event {
            DaemonEvent::StateChanged { state, .. } => {
                self.transition_to(state);
            }
            DaemonEvent::AudioLevel { rms, .. } => {
                if self.state == DictationState::Recording {
                    self.current_rms = rms;
                    self.bar_springs.update_target(rms, self.elapsed_time);
                }
            }
            DaemonEvent::Error { code: _, message } => {
                let display_message = if message.len() > 30 {
                    format!("{}...", &message[..27])
                } else {
                    message
                };
                self.error_text = Some(display_message);
                self.error_started_at = Some(Instant::now());
                self.transition_to(DictationState::Error);
            }
            DaemonEvent::Canceled { .. } => {
                self.transition_to(DictationState::Idle);
            }
            DaemonEvent::TranscriptPreview { .. } => {}
        }
    }

    pub fn transition_to(&mut self, new_state: DictationState) {
        self.state = new_state;
        match new_state {
            DictationState::Idle => {
                self.width_spring.set_target(STANDBY_PILL_WIDTH);
                self.opacity_spring.set_target(1.0);
                self.current_rms = 0.0;
                self.bar_springs.set_target_all(RESTING_BAR_HEIGHT);
                self.error_text = None;
                self.error_started_at = None;
                self.emitting_started_at = None;
            }
            DictationState::Recording => {
                self.width_spring.set_target(ACTIVE_PILL_WIDTH);
                self.opacity_spring.set_target(1.0);
                self.error_text = None;
                self.error_started_at = None;
                self.emitting_started_at = None;
            }
            DictationState::Processing => {
                self.width_spring.set_target(PROCESSING_PILL_WIDTH);
                self.opacity_spring.set_target(1.0);
                self.current_rms = 0.0;
                self.bar_springs.set_target_all(RESTING_BAR_HEIGHT);
            }
            DictationState::Emitting => {
                self.width_spring.set_target(PROCESSING_PILL_WIDTH);
                self.opacity_spring.set_target(1.0);
                self.current_rms = 0.0;
                self.bar_springs.set_target_all(RESTING_BAR_HEIGHT);
                self.emitting_started_at = Some(Instant::now());
            }
            DictationState::Error => {
                let text_len = self.error_text.as_ref().map_or(12, |s| s.len());
                let target_w = (text_len as f32 * 9.0 + 60.0).clamp(180.0, 260.0);
                self.width_spring.set_target(target_w);
                self.opacity_spring.set_target(1.0);
                self.current_rms = 0.0;
                self.bar_springs.set_target_all(RESTING_BAR_HEIGHT);
                if self.error_started_at.is_none() {
                    self.error_started_at = Some(Instant::now());
                }
            }
        }
    }

    pub fn tick(&mut self, dt: f32) {
        self.elapsed_time += dt;
        self.width_spring.update(dt);
        self.opacity_spring.update(dt);

        if self.state == DictationState::Recording {
            self.bar_springs.update_target(self.current_rms, self.elapsed_time);
        }
        self.bar_springs.update(dt);

        // Auto-dismiss Error state after 2.0s
        if self.state == DictationState::Error {
            if let Some(started) = self.error_started_at {
                if started.elapsed() >= Duration::from_millis(2000) {
                    self.transition_to(DictationState::Idle);
                }
            }
        }

        // Auto-dismiss Emitting state after 250ms
        if self.state == DictationState::Emitting {
            if let Some(started) = self.emitting_started_at {
                if started.elapsed() >= Duration::from_millis(250) {
                    self.transition_to(DictationState::Idle);
                }
            }
        }
    }

    pub fn is_animating(&self) -> bool {
        if self.state != DictationState::Idle {
            return true;
        }
        // Still animating towards settled Standby capsule
        !self.width_spring.is_settled(0.5)
            || !self.opacity_spring.is_settled(0.01)
            || !self.bar_springs.is_settled(0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_recording_and_audio_level_pipeline() {
        let mut model = DockModel::new();
        assert_eq!(model.state, DictationState::Idle);
        assert_eq!(model.width_spring.target, STANDBY_PILL_WIDTH);

        model.handle_event(DaemonEvent::StateChanged {
            state: DictationState::Recording,
            previous: Some(DictationState::Idle),
        });

        assert_eq!(model.state, DictationState::Recording);
        assert_eq!(model.width_spring.target, ACTIVE_PILL_WIDTH);
        assert_eq!(model.opacity_spring.target, 1.0);

        model.handle_event(DaemonEvent::AudioLevel {
            rms: 0.8,
            peak: 0.95,
        });

        assert_eq!(model.current_rms, 0.8);
        assert!(model.bar_springs[4].target > 15.0);

        // Cancel resets to idle Standby capsule
        model.handle_event(DaemonEvent::Canceled {
            reason: "esc".into(),
        });
        assert_eq!(model.state, DictationState::Idle);
        assert_eq!(model.width_spring.target, STANDBY_PILL_WIDTH);
        assert_eq!(model.current_rms, 0.0);
    }

    #[test]
    fn test_model_standby_idle_ceases_animating() {
        let mut model = DockModel::new();
        // Initially settled in Standby
        assert!(!model.is_animating());

        // Transition to Recording starts animation
        model.transition_to(DictationState::Recording);
        assert!(model.is_animating());

        // Step forward in Recording to let width expand towards 180px
        for _ in 0..30 {
            model.tick(1.0 / 60.0);
        }
        assert!(model.width_spring.value > 150.0);

        // Transition back to Idle animates towards 110px
        model.transition_to(DictationState::Idle);
        assert!(model.is_animating());

        // Step 60 frames (1.0s) to settle
        for _ in 0..60 {
            model.tick(1.0 / 60.0);
        }

        // Now settled in Standby, is_animating must be false so frame callbacks cease
        assert!(!model.is_animating());
        assert!((model.width_spring.value - STANDBY_PILL_WIDTH).abs() < 0.5);
    }

    #[test]
    fn test_model_error_auto_dismiss_timer() {
        let mut model = DockModel::new();
        model.handle_event(DaemonEvent::Error {
            code: "rate_limited".into(),
            message: "Rate Limit Exceeded".into(),
        });

        assert_eq!(model.state, DictationState::Error);
        assert!(model.error_text.is_some());

        // Fast-forward past 2000ms
        model.error_started_at = Some(Instant::now() - Duration::from_millis(2100));
        model.tick(1.0 / 60.0);

        assert_eq!(model.state, DictationState::Idle);
        assert_eq!(model.width_spring.target, STANDBY_PILL_WIDTH);
    }
}

