use crate::audio::AudioChunk;

pub const DEFAULT_VAD_THRESHOLD: f32 = 0.005;
pub const DEFAULT_MIN_DURATION_MS: u64 = 200;

#[derive(Debug, Clone, PartialEq)]
pub struct VadMetrics {
    pub rms: f32,
    pub duration_secs: f32,
    pub sample_count: usize,
}

#[derive(Debug, Clone)]
pub struct EnergyVad {
    pub threshold: f32,
    pub min_duration_samples: usize,
}

impl Default for EnergyVad {
    fn default() -> Self {
        Self::new(DEFAULT_VAD_THRESHOLD, DEFAULT_MIN_DURATION_MS)
    }
}

impl EnergyVad {
    pub fn new(threshold: f32, min_duration_ms: u64) -> Self {
        let min_duration_samples = ((min_duration_ms as f64 / 1000.0)
            * (crate::audio::TARGET_SAMPLE_RATE as f64))
            .round() as usize;
        Self {
            threshold,
            min_duration_samples,
        }
    }

    pub fn from_config(config: &crate::config::AudioConfig) -> Self {
        Self::new(config.vad_threshold, config.min_duration_ms)
    }

    /// Evaluates whether an AudioChunk contains valid speech activity.
    ///
    /// Returns `None` if:
    /// - The chunk duration is shorter than `min_duration_samples` (e.g. < 200ms micro-tap).
    /// - The chunk RMS energy is below `threshold` (ambient room silence or keyboard tap).
    ///
    /// Returns `Some(VadMetrics)` if valid speech activity is detected.
    pub fn evaluate(&self, chunk: &AudioChunk) -> Option<VadMetrics> {
        if chunk.samples.len() < self.min_duration_samples {
            return None;
        }

        let rms = compute_rms(&chunk.samples);
        if rms < self.threshold {
            return None;
        }

        Some(VadMetrics {
            rms,
            duration_secs: chunk.duration_secs(),
            sample_count: chunk.samples.len(),
        })
    }
}

/// Computes Root Mean Square (RMS) energy:
/// sqrt( 1/N * sum(x_i^2) )
pub fn compute_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }

    let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum_sq / samples.len() as f64).sqrt() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_rms_silence() {
        let silence = vec![0.0; 1600];
        assert_eq!(compute_rms(&silence), 0.0);
    }

    #[test]
    fn test_compute_rms_dc_signal() {
        let signal = vec![0.5; 1000];
        assert!((compute_rms(&signal) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_compute_rms_sine_wave() {
        // RMS of a sine wave with amplitude A is A / sqrt(2) ≈ 0.7071 * A
        let sample_rate = 16_000;
        let freq = 440.0;
        let amplitude = 0.1;
        let mut samples = Vec::with_capacity(sample_rate);
        for i in 0..sample_rate {
            let t = i as f32 / sample_rate as f32;
            let sample = amplitude * (2.0 * std::f32::consts::PI * freq * t).sin();
            samples.push(sample);
        }
        let rms = compute_rms(&samples);
        let expected = amplitude / std::f32::consts::SQRT_2;
        assert!((rms - expected).abs() < 1e-3);
    }

    #[test]
    fn test_vad_rejects_micro_tap() {
        let vad = EnergyVad::default(); // 200ms = 3200 samples at 16kHz
                                        // 100ms chunk with high energy
        let chunk = AudioChunk::new(vec![0.5; 1600]);
        assert_eq!(vad.evaluate(&chunk), None);
    }

    #[test]
    fn test_vad_rejects_silence() {
        let vad = EnergyVad::default();
        // 500ms chunk with near-zero noise (RMS 0.001 < 0.005)
        let chunk = AudioChunk::new(vec![0.001; 8000]);
        assert_eq!(vad.evaluate(&chunk), None);
    }

    #[test]
    fn test_vad_accepts_valid_speech() {
        let vad = EnergyVad::default();
        // 500ms chunk with RMS ~0.035 (well above 0.005 threshold)
        let chunk = AudioChunk::new(vec![0.05; 8000]);
        let metrics = vad.evaluate(&chunk).expect("Should detect valid speech");
        assert!((metrics.rms - 0.05).abs() < 1e-5);
        assert_eq!(metrics.duration_secs, 0.5);
        assert_eq!(metrics.sample_count, 8000);
    }
}
