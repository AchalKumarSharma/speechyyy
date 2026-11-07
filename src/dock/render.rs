use crate::dock::state::{DockModel, PILL_HEIGHT};
use crate::events::DictationState;
use tiny_skia::{FillRule, Paint, Path, PathBuilder, PixmapMut, Stroke, Transform};

/// Neon Synth / Dark Glass design tokens
pub const COLOR_BORDER_NEON_PINK: (u8, u8, u8) = (0xf7, 0x07, 0x76); // #f70776
pub const COLOR_CONTENT_MAGENTA: (u8, u8, u8) = (0xc3, 0x19, 0x5d);   // #c3195d
pub const COLOR_STANDBY_PLUM: (u8, u8, u8) = (0x68, 0x07, 0x47);      // #680747
pub const COLOR_BG_GLASS_RGB: (u8, u8, u8) = (18, 18, 20);            // rgba(18, 18, 20, 0.85)
pub const BG_GLASS_BASE_ALPHA: f32 = 0.85;

pub struct DockRenderer {
    pub width: u32,
    pub height: u32,
    font: Option<fontdue::Font>,
}

impl DockRenderer {
    pub fn new(width: u32, height: u32) -> Self {
        let font = Self::load_system_font();
        Self {
            width,
            height,
            font,
        }
    }

    fn load_system_font() -> Option<fontdue::Font> {
        let paths = [
            "/usr/share/fonts/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/Adwaita/AdwaitaSans-Regular.ttf",
            "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        ];
        for path in &paths {
            if let Ok(bytes) = std::fs::read(path) {
                if let Ok(font) = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default())
                {
                    return Some(font);
                }
            }
        }
        None
    }

    pub fn render(&self, model: &DockModel, canvas: &mut [u8]) {
        // Zero-fill / clear canvas
        canvas.fill(0);

        let pill_width = model.width_spring.value;
        let opacity = model.opacity_spring.value.clamp(0.0, 1.0);

        if pill_width < 4.0 || opacity <= 0.005 {
            return;
        }

        let mut pixmap =
            match PixmapMut::from_bytes(canvas, self.width, self.height) {
                Some(p) => p,
                None => return,
            };

        let center_x = self.width as f32 / 2.0;
        let center_y = self.height as f32 / 2.0;

        let pill_x = center_x - pill_width / 2.0;
        let pill_y = center_y - PILL_HEIGHT / 2.0;
        let corner_radius = (PILL_HEIGHT / 2.0).min(pill_width / 2.0);

        // 1. Ambient Drop Shadow (subtle elevation behind pill)
        if let Some(shadow_path) = Self::rounded_rect_path(
            pill_x - 1.5,
            pill_y + 2.0,
            pill_width + 3.0,
            PILL_HEIGHT + 2.0,
            corner_radius + 1.0,
        ) {
            let mut paint = Paint {
                anti_alias: true,
                ..Default::default()
            };
            paint.set_color_rgba8(0, 0, 0, (45.0 * opacity) as u8);
            pixmap.fill_path(
                &shadow_path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }

        // 2. Pill Capsule Background
        if let Some(pill_path) = Self::rounded_rect_path(
            pill_x,
            pill_y,
            pill_width,
            PILL_HEIGHT,
            corner_radius,
        ) {
            let mut bg_paint = Paint {
                anti_alias: true,
                ..Default::default()
            };

            match model.state {
                DictationState::Error => {
                    // Apple Red glass: rgba(255, 69, 58, 0.22)
                    bg_paint.set_color_rgba8(
                        255,
                        69,
                        58,
                        (56.0 * opacity) as u8,
                    );
                }
                DictationState::Emitting => {
                    // Apple Mint glass tint: rgba(28, 48, 34, 0.88)
                    bg_paint.set_color_rgba8(
                        28,
                        48,
                        34,
                        (224.0 * opacity) as u8,
                    );
                }
                _ => {
                    // Deep dark glassmorphism: rgba(18, 18, 20, 0.85)
                    let alpha = (BG_GLASS_BASE_ALPHA * 255.0 * opacity).clamp(0.0, 255.0) as u8;
                    bg_paint.set_color_rgba8(
                        COLOR_BG_GLASS_RGB.0,
                        COLOR_BG_GLASS_RGB.1,
                        COLOR_BG_GLASS_RGB.2,
                        alpha,
                    );
                }
            }

            pixmap.fill_path(
                &pill_path,
                &bg_paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );

            // 3. Perimeter Border Stroke (#f70776, 1.5px stroke, rounded capsule geometry)
            let mut stroke_paint = Paint {
                anti_alias: true,
                ..Default::default()
            };
            let stroke = Stroke {
                width: 1.5,
                ..Default::default()
            };

            match model.state {
                DictationState::Recording => {
                    // Modulate border opacity slightly with incoming RMS (0.6 resting up to 1.0 on peak voice)
                    let rms_norm = model.current_rms.clamp(0.0, 1.0);
                    let border_opacity_factor = 0.6 + 0.4 * rms_norm;
                    let border_alpha =
                        (border_opacity_factor * 255.0 * opacity).clamp(0.0, 255.0) as u8;
                    stroke_paint.set_color_rgba8(
                        COLOR_BORDER_NEON_PINK.0,
                        COLOR_BORDER_NEON_PINK.1,
                        COLOR_BORDER_NEON_PINK.2,
                        border_alpha,
                    );
                }
                DictationState::Processing | DictationState::Idle => {
                    let border_alpha = (255.0 * opacity).clamp(0.0, 255.0) as u8;
                    stroke_paint.set_color_rgba8(
                        COLOR_BORDER_NEON_PINK.0,
                        COLOR_BORDER_NEON_PINK.1,
                        COLOR_BORDER_NEON_PINK.2,
                        border_alpha,
                    );
                }
                DictationState::Emitting => {
                    stroke_paint.set_color_rgba8(
                        COLOR_BORDER_NEON_PINK.0,
                        COLOR_BORDER_NEON_PINK.1,
                        COLOR_BORDER_NEON_PINK.2,
                        (220.0 * opacity) as u8,
                    );
                }
                DictationState::Error => {
                    // #FF453A error alert stroke
                    stroke_paint.set_color_rgba8(
                        255,
                        69,
                        58,
                        (230.0 * opacity) as u8,
                    );
                }
            }

            pixmap.stroke_path(
                &pill_path,
                &stroke_paint,
                &stroke,
                Transform::identity(),
                None,
            );
        }

        // 4. Interior Visualizer Elements
        match model.state {
            DictationState::Recording => {
                self.render_waveform_bars(&mut pixmap, model, center_x, center_y, opacity);
            }
            DictationState::Processing => {
                self.render_processing_dots(&mut pixmap, model, center_x, center_y, opacity);
            }
            DictationState::Idle => {
                self.render_standby_dot(&mut pixmap, center_x, center_y, opacity);
            }
            DictationState::Emitting => {
                self.render_emitting_indicator(&mut pixmap, center_x, center_y, opacity);
            }
            DictationState::Error => {
                self.render_error_text(&mut pixmap, model, center_x, center_y, opacity);
            }
        }

        // 5. Wayland SHM Format Translation (RGBA -> ARGB8888 on little-endian)
        #[allow(clippy::chunks_exact_to_as_chunks)]
        for pixel in canvas.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
    }

    fn render_waveform_bars(
        &self,
        pixmap: &mut PixmapMut,
        model: &DockModel,
        center_x: f32,
        center_y: f32,
        opacity: f32,
    ) {
        // 9 bars: 3px width, 4px gap. Total = 9 * 3 + 8 * 4 = 59px.
        let total_w = 59.0;
        let start_x = center_x - total_w / 2.0;

        for i in 0..9 {
            let bar_h = model.bar_springs[i].value.clamp(4.0, 26.0);
            let bar_center_x = start_x + (i as f32 * 7.0) + 1.5;
            let bar_x = bar_center_x - 1.5;
            let bar_y = center_y - bar_h / 2.0;

            if let Some(bar_path) = Self::rounded_rect_path(bar_x, bar_y, 3.0, bar_h, 1.5) {
                let mut bar_paint = Paint {
                    anti_alias: true,
                    ..Default::default()
                };
                bar_paint.set_color_rgba8(
                    COLOR_CONTENT_MAGENTA.0,
                    COLOR_CONTENT_MAGENTA.1,
                    COLOR_CONTENT_MAGENTA.2,
                    (255.0 * opacity) as u8,
                );

                pixmap.fill_path(
                    &bar_path,
                    &bar_paint,
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }
    }

    fn render_processing_dots(
        &self,
        pixmap: &mut PixmapMut,
        model: &DockModel,
        center_x: f32,
        center_y: f32,
        opacity: f32,
    ) {
        // 3 pulsing dots spaced 12px apart in #c3195d with phase-shifted sinusoidal breathing
        for j in 0..3 {
            let dot_x = center_x + (j as f32 - 1.0) * 12.0;
            let dot_y = center_y;
            let phase = model.elapsed_time * 6.0 - (j as f32) * 0.8;
            let wave = 0.5 + 0.5 * phase.sin();
            let dot_alpha = ((0.30 + 0.65 * wave) * 255.0 * opacity).clamp(0.0, 255.0) as u8;

            if let Some(dot_path) =
                Self::rounded_rect_path(dot_x - 2.5, dot_y - 2.5, 5.0, 5.0, 2.5)
            {
                let mut paint = Paint {
                    anti_alias: true,
                    ..Default::default()
                };
                paint.set_color_rgba8(
                    COLOR_CONTENT_MAGENTA.0,
                    COLOR_CONTENT_MAGENTA.1,
                    COLOR_CONTENT_MAGENTA.2,
                    dot_alpha,
                );
                pixmap.fill_path(
                    &dot_path,
                    &paint,
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }
    }

    fn render_standby_dot(
        &self,
        pixmap: &mut PixmapMut,
        center_x: f32,
        center_y: f32,
        opacity: f32,
    ) {
        // Single status dot at exact center in #680747 (radius: 2px)
        if let Some(dot_path) =
            Self::rounded_rect_path(center_x - 2.0, center_y - 2.0, 4.0, 4.0, 2.0)
        {
            let mut paint = Paint {
                anti_alias: true,
                ..Default::default()
            };
            paint.set_color_rgba8(
                COLOR_STANDBY_PLUM.0,
                COLOR_STANDBY_PLUM.1,
                COLOR_STANDBY_PLUM.2,
                (255.0 * opacity) as u8,
            );
            pixmap.fill_path(
                &dot_path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }

    fn render_emitting_indicator(
        &self,
        pixmap: &mut PixmapMut,
        center_x: f32,
        center_y: f32,
        opacity: f32,
    ) {
        // Apple Mint #30D158 checkmark / glowing center circle
        if let Some(dot_path) =
            Self::rounded_rect_path(center_x - 4.0, center_y - 4.0, 8.0, 8.0, 4.0)
        {
            let mut paint = Paint {
                anti_alias: true,
                ..Default::default()
            };
            paint.set_color_rgba8(48, 209, 88, (240.0 * opacity) as u8);
            pixmap.fill_path(
                &dot_path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
    }

    fn render_error_text(
        &self,
        pixmap: &mut PixmapMut,
        model: &DockModel,
        center_x: f32,
        center_y: f32,
        opacity: f32,
    ) {
        let msg = model
            .error_text
            .as_deref()
            .unwrap_or("Dictation Error");

        if let Some(font) = &self.font {
            let font_size = 12.0f32;
            let total_text_w: f32 = msg
                .chars()
                .map(|c| font.metrics(c, font_size).advance_width)
                .sum();

            let mut pen_x = center_x - total_text_w / 2.0;
            // Baseline centered vertically in 38px pill
            let baseline_y = center_y + 4.0;

            for ch in msg.chars() {
                let (metrics, bitmap) = font.rasterize(ch, font_size);
                let x0 = pen_x + metrics.xmin as f32;
                let y0 = baseline_y - metrics.height as f32 - metrics.ymin as f32;

                for row in 0..metrics.height {
                    for col in 0..metrics.width {
                        let alpha = bitmap[row * metrics.width + col];
                        if alpha == 0 {
                            continue;
                        }

                        let px = (x0 + col as f32) as i32;
                        let py = (y0 + row as f32) as i32;

                        if px >= 0 && px < self.width as i32 && py >= 0 && py < self.height as i32 {
                            let text_alpha =
                                (alpha as f32 / 255.0) * opacity;
                            Self::blend_pixel(
                                pixmap,
                                px as u32,
                                py as u32,
                                255,
                                213,
                                210, // Light coral text (#FFD5D2)
                                text_alpha,
                            );
                        }
                    }
                }
                pen_x += metrics.advance_width;
            }
        }
    }

    fn blend_pixel(pixmap: &mut PixmapMut, x: u32, y: u32, r: u8, g: u8, b: u8, alpha: f32) {
        let idx = (y * pixmap.width() + x) as usize * 4;
        let data = pixmap.data_mut();
        let bg_r = data[idx] as f32 / 255.0;
        let bg_g = data[idx + 1] as f32 / 255.0;
        let bg_b = data[idx + 2] as f32 / 255.0;
        let bg_a = data[idx + 3] as f32 / 255.0;

        let fg_r = r as f32 / 255.0;
        let fg_g = g as f32 / 255.0;
        let fg_b = b as f32 / 255.0;
        let fg_a = alpha.clamp(0.0, 1.0);

        let out_a = fg_a + bg_a * (1.0 - fg_a);
        if out_a > 0.0 {
            let out_r = (fg_r * fg_a + bg_r * bg_a * (1.0 - fg_a)) / out_a;
            let out_g = (fg_g * fg_a + bg_g * bg_a * (1.0 - fg_a)) / out_a;
            let out_b = (fg_b * fg_a + bg_b * bg_a * (1.0 - fg_a)) / out_a;

            data[idx] = (out_r * 255.0).clamp(0.0, 255.0) as u8;
            data[idx + 1] = (out_g * 255.0).clamp(0.0, 255.0) as u8;
            data[idx + 2] = (out_b * 255.0).clamp(0.0, 255.0) as u8;
            data[idx + 3] = (out_a * 255.0).clamp(0.0, 255.0) as u8;
        }
    }

    pub fn rounded_rect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
        let r = r.min(w / 2.0).min(h / 2.0);
        let c = 0.552_284_8 * r;
        let mut pb = PathBuilder::new();
        pb.move_to(x + r, y);
        pb.line_to(x + w - r, y);
        pb.cubic_to(x + w - r + c, y, x + w, y + r - c, x + w, y + r);
        pb.line_to(x + w, y + h - r);
        pb.cubic_to(x + w, y + h - r + c, x + w - r + c, y + h, x + w - r, y + h);
        pb.line_to(x + r, y + h);
        pb.cubic_to(x + r - c, y + h, x, y + h - r + c, x, y + h - r);
        pb.line_to(x, y + r);
        pb.cubic_to(x, y + r - c, x + r - c, y, x + r, y);
        pb.close();
        pb.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_renderer_initialization_and_rendering() {
        let renderer = DockRenderer::new(280, 48);
        let mut model = DockModel::new();
        let mut canvas = vec![0u8; 280 * 48 * 4];

        // Zero opacity renders transparent buffer
        model.opacity_spring.value = 0.0;
        renderer.render(&model, &mut canvas);
        assert!(canvas.iter().all(|&b| b == 0));

        // Ready / Standby state: renders compact 110px capsule with #680747 center dot
        model.opacity_spring.value = 1.0;
        model.width_spring.value = 110.0;
        renderer.render(&model, &mut canvas);
        assert!(canvas.iter().any(|&b| b != 0));

        // Active Listening state: renders 180px capsule with 9 waveform bars and #f70776 border
        model.transition_to(DictationState::Recording);
        model.width_spring.value = 180.0;
        model.opacity_spring.value = 1.0;
        model.current_rms = 0.5;
        let mut rec_canvas = vec![0u8; 280 * 48 * 4];
        renderer.render(&model, &mut rec_canvas);
        assert!(rec_canvas.iter().any(|&b| b != 0));

        // Processing state: renders 140px capsule with 3 pulse dots
        model.transition_to(DictationState::Processing);
        model.width_spring.value = 140.0;
        let mut proc_canvas = vec![0u8; 280 * 48 * 4];
        renderer.render(&model, &mut proc_canvas);
        assert!(proc_canvas.iter().any(|&b| b != 0));
    }
}
