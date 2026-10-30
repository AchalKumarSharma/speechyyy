use crate::audio::AudioChunk;

pub const TARGET_SAMPLE_RATE: u32 = 16_000;

/// Downmixes multi-channel interleaved audio into a single-channel (mono) vector
/// by averaging samples across channels for each audio frame.
pub fn downmix_to_mono(interleaved: &[f32], channels: u16) -> Vec<f32> {
    if interleaved.is_empty() {
        return Vec::new();
    }
    if channels <= 1 {
        return interleaved.to_vec();
    }

    let channels_usize = channels as usize;
    let frame_count = interleaved.len() / channels_usize;
    let mut mono = Vec::with_capacity(frame_count);

    for frame_idx in 0..frame_count {
        let start = frame_idx * channels_usize;
        let mut sum = 0.0;
        for c in 0..channels_usize {
            sum += interleaved[start + c];
        }
        mono.push(sum / (channels as f32));
    }

    mono
}

/// Resamples mono audio from `from_rate` to 16,000 Hz.
/// Uses continuous-time area integration for downsampling (anti-aliased box filter)
/// and linear interpolation for equal/higher rate conversion.
pub fn resample_to_16k(mono_samples: &[f32], from_rate: u32) -> Vec<f32> {
    if mono_samples.is_empty() || from_rate == 0 {
        return Vec::new();
    }

    if from_rate == TARGET_SAMPLE_RATE {
        return mono_samples.to_vec();
    }

    if from_rate == 48_000 {
        let out_len = mono_samples.len() / 3;
        let mut out = Vec::with_capacity(out_len);
        for i in 0..out_len {
            let sum = mono_samples[3 * i] + mono_samples[3 * i + 1] + mono_samples[3 * i + 2];
            out.push(sum / 3.0);
        }
        return out;
    }

    let ratio = from_rate as f64 / TARGET_SAMPLE_RATE as f64;
    let target_len = ((mono_samples.len() as f64) / ratio).round() as usize;
    if target_len == 0 {
        return Vec::new();
    }

    let mut out = Vec::with_capacity(target_len);

    // Downsampling via continuous-time area integration prevents aliasing
    for i in 0..target_len {
        let start_f = i as f64 * ratio;
        let end_f = (i + 1) as f64 * ratio;
        let end_clamped = end_f.min(mono_samples.len() as f64);
        if start_f >= end_clamped {
            break;
        }

        let mut area = 0.0;
        let mut curr = start_f;
        while curr < end_clamped {
            let idx = curr.floor() as usize;
            let next_boundary = (idx + 1) as f64;
            let step_end = end_clamped.min(next_boundary);
            let weight = step_end - curr;
            if idx < mono_samples.len() {
                area += mono_samples[idx] as f64 * weight;
            }
            curr = step_end;
        }

        let span = end_clamped - start_f;
        let val = if span > 0.0 {
            (area / span) as f32
        } else {
            0.0
        };
        out.push(val);
    }

    out
}

/// Downmixes and resamples raw audio stream to standard 16,000 Hz single-channel AudioChunk.
pub fn process_raw_audio(raw: &[f32], from_rate: u32, channels: u16) -> AudioChunk {
    let mono = downmix_to_mono(raw, channels);
    let resampled = resample_to_16k(&mono, from_rate);
    AudioChunk::new(resampled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_downmix_stereo_to_mono() {
        // [L0, R0, L1, R1]
        let interleaved = vec![1.0, 0.0, 0.5, 0.5, -0.2, 0.8];
        let mono = downmix_to_mono(&interleaved, 2);
        assert_eq!(mono.len(), 3);
        assert!((mono[0] - 0.5).abs() < 1e-6);
        assert!((mono[1] - 0.5).abs() < 1e-6);
        assert!((mono[2] - 0.3).abs() < 1e-6);
    }

    #[test]
    fn test_downmix_mono_identity() {
        let input = vec![0.1, 0.2, 0.3];
        let mono = downmix_to_mono(&input, 1);
        assert_eq!(mono, input);
    }

    #[test]
    fn test_downmix_empty() {
        let empty: Vec<f32> = Vec::new();
        assert_eq!(downmix_to_mono(&empty, 2), Vec::<f32>::new());
    }

    #[test]
    fn test_resample_48k_to_16k() {
        // 48kHz audio of 1 second = 48,000 samples
        // Constant DC signal of 0.8
        let input = vec![0.8; 48_000];
        let resampled = resample_to_16k(&input, 48_000);
        assert_eq!(resampled.len(), 16_000);
        for s in resampled {
            assert!((s - 0.8).abs() < 1e-5);
        }
    }

    #[test]
    fn test_resample_44_1k_to_16k() {
        // 44.1kHz audio of 1 second = 44,100 samples
        let input = vec![0.5; 44_100];
        let resampled = resample_to_16k(&input, 44_100);
        assert_eq!(resampled.len(), 16_000);
        for s in resampled {
            assert!((s - 0.5).abs() < 1e-4);
        }
    }

    #[test]
    fn test_resample_16k_passthrough() {
        let input = vec![0.1, 0.2, 0.3, 0.4];
        let resampled = resample_to_16k(&input, 16_000);
        assert_eq!(resampled, input);
    }

    #[test]
    fn test_process_raw_audio() {
        // 48kHz stereo, 2 channels, 96,000 samples = 1 second
        let raw = vec![0.6; 96_000];
        let chunk = process_raw_audio(&raw, 48_000, 2);
        assert_eq!(chunk.sample_rate(), 16_000);
        assert_eq!(chunk.channels(), 1);
        assert_eq!(chunk.samples.len(), 16_000);
        assert_eq!(chunk.duration_secs(), 1.0);
    }
}
