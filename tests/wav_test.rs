use speechyyy::audio::wav::encode_wav_16k_mono;

#[test]
fn test_wav_header_and_format() {
    let samples = vec![0.0f32; 16000]; // 1 second of 16kHz audio
    let bytes = encode_wav_16k_mono(&samples);

    // 44-byte header + 16000 samples * 2 bytes = 32044 bytes
    assert_eq!(bytes.len(), 44 + 32000);

    // 1. RIFF Chunk Descriptor
    assert_eq!(&bytes[0..4], b"RIFF");
    let file_size = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    assert_eq!(file_size, 36 + 32000);
    assert_eq!(&bytes[8..12], b"WAVE");

    // 2. fmt Subchunk
    assert_eq!(&bytes[12..16], b"fmt ");
    let fmt_size = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    assert_eq!(fmt_size, 16); // 16 bytes for PCM
    let audio_format = u16::from_le_bytes(bytes[20..22].try_into().unwrap());
    assert_eq!(audio_format, 1); // 1 = Linear PCM
    let num_channels = u16::from_le_bytes(bytes[22..24].try_into().unwrap());
    assert_eq!(num_channels, 1); // 1 = Mono
    let sample_rate = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
    assert_eq!(sample_rate, 16000); // 16,000 Hz
    let byte_rate = u32::from_le_bytes(bytes[28..32].try_into().unwrap());
    assert_eq!(byte_rate, 32000); // 16000 * 1 * 2 bytes
    let block_align = u16::from_le_bytes(bytes[32..34].try_into().unwrap());
    assert_eq!(block_align, 2); // 1 channel * 2 bytes
    let bits_per_sample = u16::from_le_bytes(bytes[34..36].try_into().unwrap());
    assert_eq!(bits_per_sample, 16); // 16 bits

    // 3. data Subchunk
    assert_eq!(&bytes[36..40], b"data");
    let data_size = u32::from_le_bytes(bytes[40..44].try_into().unwrap());
    assert_eq!(data_size, 32000);
}

#[test]
fn test_wav_sample_quantization_and_clamping() {
    let samples = [0.0, 1.0, -1.0, 0.5, -0.5, 2.0, -2.0];
    let bytes = encode_wav_16k_mono(&samples);

    assert_eq!(bytes.len(), 44 + 14);

    let read_pcm = |idx: usize| -> i16 {
        let start = 44 + idx * 2;
        i16::from_le_bytes(bytes[start..start + 2].try_into().unwrap())
    };

    assert_eq!(read_pcm(0), 0);
    assert_eq!(read_pcm(1), 32767);
    assert_eq!(read_pcm(2), -32767);
    assert_eq!(read_pcm(3), 16384);
    assert_eq!(read_pcm(4), -16384);
    assert_eq!(read_pcm(5), 32767); // Clamped > 1.0
    assert_eq!(read_pcm(6), -32767); // Clamped < -1.0
}

#[test]
fn test_wav_empty_samples() {
    let empty: [f32; 0] = [];
    let bytes = encode_wav_16k_mono(&empty);

    assert_eq!(bytes.len(), 44);
    let chunk_size = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    assert_eq!(chunk_size, 36);
    let data_size = u32::from_le_bytes(bytes[40..44].try_into().unwrap());
    assert_eq!(data_size, 0);
}
