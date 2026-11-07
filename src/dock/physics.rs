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
}
