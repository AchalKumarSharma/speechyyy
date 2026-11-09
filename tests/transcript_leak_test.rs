use speechyyy::inference::local::format_decode_log;

#[test]
fn test_format_decode_log_never_contains_transcript() {
    let sentinel = "SENTINEL_TRANSCRIPT_SECRET_12345";
    let formatted = format_decode_log("parakeet-tdt-v3", 2.34, 37440);

    assert!(
        !formatted.contains(sentinel),
        "Formatted decode log must never contain transcript"
    );
    assert_eq!(
        formatted,
        "Local INT8 decoder (parakeet-tdt-v3) decoded 2.34s chunk (37440 samples)"
    );
}

#[test]
fn test_source_code_has_no_transcript_logging() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let local_rs_path = std::path::Path::new(manifest_dir).join("src/inference/local.rs");
    let content = std::fs::read_to_string(&local_rs_path).expect("read local.rs");

    // Build the banned interpolation needle dynamically so the test itself doesn't match
    let needle_chunk = "decoded {:.2}s chunk:";
    let needle_quote = " \"{}\"";
    let banned_pattern = format!("{}{}", needle_chunk, needle_quote);

    assert!(
        !content.contains(&banned_pattern),
        "src/inference/local.rs must not contain transcript-interpolating log pattern"
    );
}

