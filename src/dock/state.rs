use crate::dock::physics::{Spring, WaveformSprings};
use crate::events::{DaemonEvent, DictationState};
use std::time::{Duration, Instant};

pub const RESTING_BAR_HEIGHT: f32 = 4.0;
pub const MAX_BAR_HEIGHT: f32 = 26.0;
pub const PILL_HEIGHT: f32 = 38.0;

pub const ACTIVE_PILL_WIDTH: f32 = 180.0;
pub const PROCESSING_PILL_WIDTH: f32 = 140.0;
pub const ERROR_PILL_WIDTH: f32 = 220.0;

pub const HIDE_OPACITY_EPSILON: f32 = 0.01;
pub const HIDE_WIDTH_EPSILON: f32 = 4.0;

#[derive(Debug)]
pub struct DockModel {
    pub state: DictationState,
    pub width_spring: Spring,
    pub opacity_spring: Spring,
    pub target_width: f32,
    pub current_width: f32,
    pub target_opacity: f32,
    pub current_opacity: f32,
    pub bar_springs: WaveformSprings,
    pub current_rms: f32,
    pub error_text: Option<String>,
    pub error_started_at: Option<Instant>,
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
            width_spring: Spring::new(0.0, 260.0, 0.75),
            opacity_spring: Spring::new(0.0, 280.0, 0.80),
            target_width: 0.0,
            current_width: 0.0,
            target_opacity: 0.0,
            current_opacity: 0.0,
            bar_springs,
            current_rms: 0.0,
            error_text: None,
            error_started_at: None,
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
            DictationState::Recording => {
                self.target_width = ACTIVE_PILL_WIDTH;
                self.target_opacity = 1.0;
                self.width_spring.set_target(ACTIVE_PILL_WIDTH);
                self.opacity_spring.set_target(1.0);
                self.error_text = None;
                self.error_started_at = None;
            }
            DictationState::Processing => {
                self.target_width = PROCESSING_PILL_WIDTH;
                self.target_opacity = 1.0;
                self.width_spring.set_target(PROCESSING_PILL_WIDTH);
                self.opacity_spring.set_target(1.0);
                self.current_rms = 0.0;
                self.bar_springs.set_target_all(RESTING_BAR_HEIGHT);
                self.error_started_at = None;
            }
            // Snap exit spring immediately upon emission or return to idle
            DictationState::Emitting | DictationState::Idle => {
                self.target_width = 0.0;
                self.target_opacity = 0.0;
                self.width_spring.set_target(0.0);
                self.opacity_spring.set_target(0.0);
                self.current_rms = 0.0;
                self.bar_springs.set_target_all(RESTING_BAR_HEIGHT);
                self.error_text = None;
                self.error_started_at = None;
            }
            DictationState::Error => {
                let target_w = if let Some(ref text) = self.error_text {
                    (text.len() as f32 * 9.0 + 60.0).clamp(180.0, 260.0)
                } else {
                    ERROR_PILL_WIDTH
                };
                self.target_width = target_w;
                self.target_opacity = 1.0;
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
        let dt = dt.clamp(0.0, 0.05);
        self.elapsed_time += dt;
        self.width_spring.update(dt);
        self.opacity_spring.update(dt);
        self.current_width = self.width_spring.value;
        self.current_opacity = self.opacity_spring.value;

        if self.state == DictationState::Recording {
            self.bar_springs.update_target(self.current_rms, self.elapsed_time);
        }
        self.bar_springs.update(dt);

        // Auto-dismiss Error state after 2.0s exclusively
        if self.state == DictationState::Error {
            if let Some(started) = self.error_started_at {
                if started.elapsed() >= Duration::from_millis(2000) {
                    self.transition_to(DictationState::Idle);
                }
            }
        }
    }

    /// Settle check called after each spring step.
    /// When target_width is 0.0 and current values fall below the hide epsilons,
    /// current values are snapped directly to 0.0 and velocities zeroed.
    pub fn settle(&mut self) {
        // Exact float equality on target_width == 0.0 is intentional because the value
        // is assigned directly from the literal 0.0 in Emitting and Idle states,
        // so it must not be converted to an epsilon check.
        if self.target_width == 0.0
            && self.current_width < HIDE_WIDTH_EPSILON
            && self.current_opacity < HIDE_OPACITY_EPSILON
        {
            self.current_width = 0.0;
            self.current_opacity = 0.0;
            self.width_spring.value = 0.0;
            self.width_spring.velocity = 0.0;
            self.opacity_spring.value = 0.0;
            self.opacity_spring.velocity = 0.0;
        }
    }

    /// Helper returning true when the model is at rest in its completely hidden state.
    pub fn is_settled_hidden(&self) -> bool {
        // Exact float equality on target_width == 0.0, current_width == 0.0,
        // and current_opacity == 0.0 is intentional because settled hidden state
        // snaps directly to the literal 0.0.
        self.target_width == 0.0 && self.current_width == 0.0 && self.current_opacity == 0.0
    }

    /// Pure query returning whether the dock is currently animating.
    /// Returns false once the model is settled at its hidden state (or otherwise at rest on its target).
    pub fn is_animating(&self) -> bool {
        if self.is_settled_hidden() {
            return false;
        }
        if self.state == DictationState::Idle || self.state == DictationState::Emitting {
            // Animating towards collapsed hidden state
            return !self.is_settled_hidden();
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_recording_and_audio_level_pipeline() {
        let mut model = DockModel::new();
        assert_eq!(model.state, DictationState::Idle);
        assert_eq!(model.target_width, 0.0);
        assert_eq!(model.target_opacity, 0.0);
        assert!(model.is_settled_hidden());
        assert!(!model.is_animating());

        model.handle_event(DaemonEvent::StateChanged {
            state: DictationState::Recording,
            previous: Some(DictationState::Idle),
        });

        assert_eq!(model.state, DictationState::Recording);
        assert_eq!(model.target_width, ACTIVE_PILL_WIDTH);
        assert_eq!(model.target_opacity, 1.0);
        assert!(model.is_animating());

        model.handle_event(DaemonEvent::AudioLevel {
            rms: 0.8,
            peak: 0.95,
        });

        assert_eq!(model.current_rms, 0.8);
        assert!(model.bar_springs[4].target > 15.0);

        // Cancel resets to idle
        model.handle_event(DaemonEvent::Canceled {
            reason: "esc".into(),
        });
        assert_eq!(model.state, DictationState::Idle);
        assert_eq!(model.target_width, 0.0);
        assert_eq!(model.target_opacity, 0.0);
        assert_eq!(model.current_rms, 0.0);
    }

    #[test]
    fn test_model_standby_idle_ceases_animating() {
        let mut model = DockModel::new();
        // Initially settled in hidden state
        assert!(!model.is_animating());
        assert!(model.is_settled_hidden());

        // Transition to Recording starts animation
        model.transition_to(DictationState::Recording);
        assert!(model.is_animating());

        // Step forward in Recording to let width expand towards 180px
        for _ in 0..30 {
            model.tick(1.0 / 60.0);
            model.settle();
        }
        assert!(model.current_width > 150.0);

        // Transition back to Idle animates towards 0.0
        model.transition_to(DictationState::Idle);
        assert!(model.is_animating());

        // Step frames with settle() each step to settle into hidden state
        for _ in 0..120 {
            model.tick(1.0 / 60.0);
            model.settle();
        }

        // Now settled hidden, is_animating must be false so frame callbacks cease
        assert!(!model.is_animating());
        assert!(model.is_settled_hidden());
        assert_eq!(model.current_width, 0.0);
        assert_eq!(model.current_opacity, 0.0);
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
        assert!(model.error_started_at.is_some());

        // Fast-forward past 2000ms
        model.error_started_at = Some(Instant::now() - Duration::from_millis(2100));
        model.tick(1.0 / 60.0);

        assert_eq!(model.state, DictationState::Idle);
        assert_eq!(model.target_width, 0.0);
    }

    #[test]
    fn test_tick_large_dt_is_clamped_and_prevents_instant_overshoot() {
        let mut model = DockModel::new();
        model.transition_to(DictationState::Recording);
        assert_eq!(model.target_width, ACTIVE_PILL_WIDTH);
        assert_eq!(model.current_width, 0.0);

        // Simulate a huge delta time spike after sitting idle (e.g. 60 seconds)
        model.tick(60.0);
        model.settle();

        // The timestep must be clamped to 50ms, so width should only have advanced a 50ms step (117px),
        // definitely NOT jumping to millions or overshooting/settling hidden.
        assert!(model.current_width > 0.0);
        assert!(
            model.current_width <= 120.0,
            "current_width was {}",
            model.current_width
        );
        assert!(model.is_animating());
        assert!(!model.is_settled_hidden());
    }
}

