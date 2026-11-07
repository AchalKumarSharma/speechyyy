//! Spring dynamics and Gaussian waveform distribution for speechyyy-dock.

/// Critically damped spring physics solver:
/// acceleration = -k * (value - target) - 2 * zeta * sqrt(k) * velocity
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spring {
    pub value: f32,
    pub target: f32,
    pub velocity: f32,
    pub stiffness: f32, // k
    pub damping: f32,   // zeta (0.75 for near-critical damping)
}

impl Spring {
    pub fn new(initial: f32, stiffness: f32, damping: f32) -> Self {
        Self {
            value: initial,
            target: initial,
            velocity: 0.0,
            stiffness,
            damping,
        }
    }

    pub fn set_target(&mut self, target: f32) {
        self.target = target;
    }

    pub fn update(&mut self, dt: f32) {
        let dt = dt.clamp(0.001, 0.05);
        let diff = self.value - self.target;
        let c = 2.0 * self.damping * self.stiffness.sqrt();
        let acceleration = -self.stiffness * diff - c * self.velocity;
        self.velocity += acceleration * dt;
        self.value += self.velocity * dt;
    }

    pub fn is_settled(&self, epsilon: f32) -> bool {
        (self.value - self.target).abs() < epsilon && self.velocity.abs() < epsilon
    }
}

/// Multi-bar waveform springs manager with Gaussian distribution and harmonic jitter.
#[derive(Debug, Clone, PartialEq)]
pub struct WaveformSprings {
    pub bars: [Spring; 9],
}

impl Default for WaveformSprings {
    fn default() -> Self {
        Self::new()
    }
}

impl WaveformSprings {
    pub fn new() -> Self {
        Self {
            bars: std::array::from_fn(|_| Spring::new(4.0, 300.0, 0.75)),
        }
    }

    pub fn with_params(initial: f32, stiffness: f32, damping: f32) -> Self {
        Self {
            bars: std::array::from_fn(|_| Spring::new(initial, stiffness, damping)),
        }
    }

    pub fn update_target(&mut self, raw_rms: f32, elapsed_secs: f32) {
        // 1. Logarithmic sensitivity curve: boosts quiet voice while preventing hard clipping
        let amplified = (raw_rms * 8.5).clamp(0.0, 1.0).powf(0.7);

        for (i, bar) in self.bars.iter_mut().enumerate() {
            let offset = (i as f32) - 4.0;
            let gaussian = (- (offset * offset) / 5.2).exp();

            // 2. Organic harmonic phase jitter so bars ripple independently
            let jitter = 1.0 + 0.16 * ((elapsed_secs * 15.0) + (i as f32 * 1.15)).sin();
            let target_height = 4.0 + (amplified * 22.0 * gaussian * jitter);

            // Strict clamp to maintain pill capsule containment
            bar.set_target(target_height.clamp(4.0, 26.0));
        }
    }

    pub fn update(&mut self, dt: f32) {
        for bar in &mut self.bars {
            bar.update(dt);
        }
    }

    pub fn set_target_all(&mut self, target: f32) {
        for bar in &mut self.bars {
            bar.set_target(target);
        }
    }

    pub fn is_settled(&self, epsilon: f32) -> bool {
        self.bars.iter().all(|b| b.is_settled(epsilon))
    }
}

impl std::ops::Deref for WaveformSprings {
    type Target = [Spring; 9];
    fn deref(&self) -> &Self::Target {
        &self.bars
    }
}

impl std::ops::DerefMut for WaveformSprings {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.bars
    }
}

/// Gaussian envelope for 9 vertical waveform bars:
/// height_i = clamp(4.0 + rms * 32.0 * exp(-((i - 4)^2) / 6.0), 4.0, 22.0)
pub fn calculate_bar_targets(rms: f32) -> [f32; 9] {
    let mut targets = [4.0f32; 9];
    let rms = rms.clamp(0.0, 1.0);
    for (i, target) in targets.iter_mut().enumerate() {
        let diff = (i as f32) - 4.0;
        let gaussian = (- (diff * diff) / 6.0).exp();
        let h = 4.0 + rms * 32.0 * gaussian;
        *target = h.clamp(4.0, 22.0);
    }
    targets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spring_convergence_to_target() {
        let mut spring = Spring::new(0.0, 300.0, 0.75);
        spring.set_target(180.0);

        // Step 60 frames at 60 Hz (1 second)
        for _ in 0..60 {
            spring.update(1.0 / 60.0);
        }

        assert!(spring.is_settled(0.5));
        assert!((spring.value - 180.0).abs() < 0.5);
    }

    #[test]
    fn test_gaussian_envelope_silence() {
        let targets = calculate_bar_targets(0.0);
        for &h in &targets {
            assert_eq!(h, 4.0);
        }
    }

    #[test]
    fn test_gaussian_envelope_symmetry_and_clamping() {
        let targets = calculate_bar_targets(1.0);
        // Center peak clamped to 22.0
        assert_eq!(targets[4], 22.0);
        // Symmetric bars
        for i in 0..4 {
            assert!((targets[i] - targets[8 - i]).abs() < 1e-4);
            assert!(targets[i] <= targets[i + 1]);
        }
        assert!(targets[0] < targets[1]);
        assert!(targets[1] < targets[2]);
        assert!(targets[2] < targets[3]);
    }

    #[test]
    fn test_waveform_springs_initialization() {
        let springs = WaveformSprings::new();
        assert_eq!(springs.bars.len(), 9);
        for bar in &springs.bars {
            assert_eq!(bar.value, 4.0);
            assert_eq!(bar.target, 4.0);
        }
        assert!(springs.is_settled(0.01));
    }

    #[test]
    fn test_waveform_springs_silence_maintains_resting_floor() {
        let mut springs = WaveformSprings::new();
        springs.update_target(0.0, 1.23);
        for bar in &springs.bars {
            assert_eq!(bar.target, 4.0);
        }
    }

    #[test]
    fn test_waveform_springs_amplifies_quiet_voice() {
        let mut springs = WaveformSprings::new();
        // Quiet whisper: 0.03 raw RMS
        springs.update_target(0.03, 0.0);
        // Center bar (index 4) should be significantly elevated above 4.0
        assert!(springs.bars[4].target > 10.0);
        // All bars stay within capsule containment (4.0 to 26.0)
        for bar in &springs.bars {
            assert!(bar.target >= 4.0 && bar.target <= 26.0);
        }
    }

    #[test]
    fn test_waveform_springs_peak_clamping() {
        let mut springs = WaveformSprings::new();
        // Loud input: 1.0 raw RMS
        springs.update_target(1.0, 0.5);
        for bar in &springs.bars {
            assert!(bar.target >= 4.0);
            assert!(bar.target <= 26.0);
        }
        // Peak center bar clamped at or near 26.0
        assert!(springs.bars[4].target >= 22.0);
    }

    #[test]
    fn test_waveform_springs_organic_phase_jitter() {
        let mut springs_t0 = WaveformSprings::new();
        let mut springs_t1 = WaveformSprings::new();

        springs_t0.update_target(0.2, 0.0);
        springs_t1.update_target(0.2, 0.1);

        // Micro-phase jitter causes time-dependent dynamic variations
        let diff: f32 = springs_t0
            .bars
            .iter()
            .zip(springs_t1.bars.iter())
            .map(|(a, b)| (a.target - b.target).abs())
            .sum();

        assert!(diff > 0.1);
    }
}
