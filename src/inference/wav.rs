/// Encodes 16kHz mono f32 samples into a canonical 44-byte RIFF/WAVE container
/// with 16-bit signed little-endian PCM samples.
pub fn encode_wav_16k_mono(samples: &[f32]) -> Vec<u8> {
    let data_len = samples.len() * 2;
    let file_len = 36 + data_len;
    let mut out = Vec::with_capacity(44 + data_len);

    // 1. RIFF Header
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(file_len as u32).to_le_bytes());
    out.extend_from_slice(b"WAVE");

    // 2. Format Chunk ("fmt ")
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // Subchunk1Size (16 bytes for PCM)
    out.extend_from_slice(&1u16.to_le_bytes()); // AudioFormat (1 = PCM)
    out.extend_from_slice(&1u16.to_le_bytes()); // NumChannels (1 = Mono)
    out.extend_from_slice(&16000u32.to_le_bytes()); // SampleRate (16,000 Hz)
    out.extend_from_slice(&32000u32.to_le_bytes()); // ByteRate (16000 * 1 * 2 bytes = 32000)
    out.extend_from_slice(&2u16.to_le_bytes()); // BlockAlign (1 channel * 2 bytes = 2)
    out.extend_from_slice(&16u16.to_le_bytes()); // BitsPerSample (16)

    // 3. Data Chunk ("data")
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data_len as u32).to_le_bytes());

    // 4. PCM Samples (clamp -1.0..=1.0 to i16)
    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let pcm = (clamped * 32767.0).round() as i16;
        out.extend_from_slice(&pcm.to_le_bytes());
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wav_header_structure() {
        let samples = vec![0.0f32; 100];
        let bytes = encode_wav_16k_mono(&samples);

        assert_eq!(bytes.len(), 44 + 200);

        // RIFF header
        assert_eq!(&bytes[0..4], b"RIFF");
        let chunk_size = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        assert_eq!(chunk_size, 36 + 200);
        assert_eq!(&bytes[8..12], b"WAVE");

        // "fmt " subchunk
        assert_eq!(&bytes[12..16], b"fmt ");
        assert_eq!(u32::from_le_bytes(bytes[16..20].try_into().unwrap()), 16); // Subchunk1Size
        assert_eq!(u16::from_le_bytes(bytes[20..22].try_into().unwrap()), 1); // PCM format
        assert_eq!(u16::from_le_bytes(bytes[22..24].try_into().unwrap()), 1); // Mono (1 channel)
        assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), 16000); // Sample rate
        assert_eq!(u32::from_le_bytes(bytes[28..32].try_into().unwrap()), 32000); // Byte rate
        assert_eq!(u16::from_le_bytes(bytes[32..34].try_into().unwrap()), 2); // Block align
        assert_eq!(u16::from_le_bytes(bytes[34..36].try_into().unwrap()), 16); // Bits per sample

        // "data" subchunk
        assert_eq!(&bytes[36..40], b"data");
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 200); // Subchunk2Size
    }

    #[test]
    fn test_wav_empty_samples() {
        let empty: [f32; 0] = [];
        let bytes = encode_wav_16k_mono(&empty);
        assert_eq!(bytes.len(), 44);
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 36);
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 0);
    }

    #[test]
    fn test_wav_sample_quantization() {
        let samples = [0.0, 1.0, -1.0, 0.5, -0.5, 2.0, -2.0];
        let bytes = encode_wav_16k_mono(&samples);

        let data = &bytes[44..];
        assert_eq!(data.len(), 14);

        let read_i16 = |idx: usize| -> i16 {
            let start = idx * 2;
            i16::from_le_bytes(data[start..start + 2].try_into().unwrap())
        };

        assert_eq!(read_i16(0), 0);
        assert_eq!(read_i16(1), 32767);
        assert_eq!(read_i16(2), -32767);
        assert_eq!(read_i16(3), 16384);
        assert_eq!(read_i16(4), -16384);
        assert_eq!(read_i16(5), 32767); // Clamped
        assert_eq!(read_i16(6), -32767); // Clamped
    }
}
