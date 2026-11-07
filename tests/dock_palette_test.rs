use speechyyy::dock::render::{
    COLOR_BG_GLASS_RGB, COLOR_BORDER_NEON, COLOR_BORDER_NEON_PINK, COLOR_CONTENT_MAGENTA,
    COLOR_STANDBY_PLUM,
};

#[test]
fn no_green_in_palette() {
    // 1. Neon Pink Border must equal #f70776
    assert_eq!(COLOR_BORDER_NEON, (0xf7, 0x07, 0x76));
    assert_eq!(COLOR_BORDER_NEON_PINK, (0xf7, 0x07, 0x76));

    // 2. Ensure no exported palette constant in render.rs matches the legacy Apple Mint green (48, 209, 88)
    let forbidden_rgb = (0x30u8, 0xd1u8, 0x58u8);
    let all_palette_constants = [
        COLOR_BORDER_NEON,
        COLOR_BORDER_NEON_PINK,
        COLOR_CONTENT_MAGENTA,
        COLOR_STANDBY_PLUM,
        COLOR_BG_GLASS_RGB,
    ];
    for &color in &all_palette_constants {
        assert_ne!(
            color, forbidden_rgb,
            "Found legacy green color in exported palette constant: {:?}",
            color
        );
    }

    // 3. Inspect non-test source of src/dock/render.rs to ensure zero legacy green color codes
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let render_path = format!("{}/src/dock/render.rs", manifest_dir);
    let source = std::fs::read_to_string(&render_path).expect("Failed to read render.rs");

    // Separate non-test source from unit test section
    let non_test_source = if let Some(idx) = source.find("#[cfg(test)]") {
        &source[..idx]
    } else {
        &source[..]
    };

    // Dynamically construct search needles to prevent self-referential string matches
    let hex_upper = format!("{:02X}{:02X}{:02X}", 0x30, 0xd1, 0x58);
    let hex_lower = format!("{:02x}{:02x}{:02x}", 0x30, 0xd1, 0x58);
    let rgb_needle = format!("{}, {}, {}", 48, 209, 88);

    assert!(
        !non_test_source.contains(&hex_upper),
        "render.rs contains legacy green hex needle: {}",
        hex_upper
    );
    assert!(
        !non_test_source.contains(&hex_lower),
        "render.rs contains legacy green hex needle: {}",
        hex_lower
    );
    assert!(
        !non_test_source.contains(&rgb_needle),
        "render.rs contains legacy green RGB needle: {}",
        rgb_needle
    );
}
