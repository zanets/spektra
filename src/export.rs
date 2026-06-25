use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use image::{imageops, RgbaImage, Rgba};

use crate::audio::AudioInfo;
use crate::palette;
use crate::pipeline::BANDS;

// Canvas layout (pixels)
const W: u32 = 1400;
const H: u32 = 800;
const LPAD: u32 = 72;
const RPAD: u32 = 110;
const TPAD: u32 = 62;
const BPAD: u32 = 44;
const GAP: u32 = 10;
const PAL_W: u32 = 14;

pub fn save(info: &AudioInfo, src_pixels: &[u8], src_cols: usize, ceil_db: f32, floor_db: f32) -> Result<String, String> {
    let font = FontRef::try_from_slice(epaint_default_fonts::UBUNTU_LIGHT)
        .map_err(|e| e.to_string())?;

    let mut img = RgbaImage::from_pixel(W, H, Rgba([0, 0, 0, 255]));

    let spec_x = LPAD;
    let spec_y = TPAD;
    let spec_w = W - LPAD - RPAD;
    let spec_h = H - TPAD - BPAD;

    // --- Blit spectrogram (scale to fit) ---
    if src_cols > 0 && !src_pixels.is_empty() {
        let src = RgbaImage::from_raw(src_cols as u32, BANDS as u32, src_pixels.to_vec())
            .ok_or("bad pixel buffer")?;
        let scaled = imageops::resize(&src, spec_w, spec_h, imageops::FilterType::Triangle);
        imageops::overlay(&mut img, &scaled, spec_x as i64, spec_y as i64);
    }

    // --- Border ---
    draw_rect_outline(&mut img, spec_x, spec_y, spec_w, spec_h, gray(80));

    // --- Frequency axis (left) — every 5 kHz from 0 to nyquist ---
    let nyquist = info.sample_rate as f32 / 2.0;
    let mut last_label_y: Option<u32> = None;
    let mut hz = 0u32;
    while hz as f32 <= nyquist {
        let t = hz as f32 / nyquist;
        let y = spec_y + spec_h - (t * spec_h as f32) as u32;
        hline(&mut img, spec_x.saturating_sub(6), spec_x, y, gray(150));
        let label = if hz == 0 { "0".to_string() } else { format!("{} kHz", hz / 1000) };
        // Only draw label if there's enough room from the previous one
        if last_label_y.map_or(true, |prev| prev.saturating_sub(y) >= 14) {
            let tw = text_width(&font, &label, 11.0);
            draw_text(&mut img, &font, &label, spec_x.saturating_sub(8 + tw), y.saturating_sub(6), 11.0, gray(210));
            last_label_y = Some(y);
        }
        hz += 5000;
    }

    // --- Time axis (bottom) ---
    let duration = info.duration as f32;
    if duration > 0.0 {
        let step = best_time_step(duration);
        let mut t = step;
        while t < duration {
            let x = spec_x + ((t / duration) * spec_w as f32) as u32;
            vline(&mut img, spec_y + spec_h, spec_y + spec_h + 6, x, gray(150));
            let label = format!("{}:{:02}", t as u32 / 60, t as u32 % 60);
            let tw = text_width(&font, &label, 11.0);
            draw_text(&mut img, &font, &label, x.saturating_sub(tw / 2), spec_y + spec_h + 8, 11.0, gray(210));
            t += step;
        }
    }

    // --- Palette bar ---
    let pal_x = spec_x + spec_w + GAP;
    let steps = spec_h;
    for i in 0..steps {
        let level = i as f64 / steps as f64;
        let rgba = palette::sox(level);
        // level 0 = bottom (LRANGE), level 1 = top (URANGE) → y goes top-to-bottom
        let y = spec_y + steps - 1 - i;
        for x in pal_x..pal_x + PAL_W {
            img.put_pixel(x, y, Rgba([rgba[0], rgba[1], rgba[2], 255]));
        }
    }

    // --- dB axis (right of palette) — dynamic range ---
    let db_step = best_db_step(ceil_db - floor_db);
    let first_db = (ceil_db / db_step).floor() * db_step;
    let mut db = first_db;
    let mut last_label_y: Option<u32> = None;
    while db >= floor_db - 0.5 {
        let level = ((db - floor_db) / (ceil_db - floor_db)).clamp(0.0, 1.0);
        let y = spec_y + spec_h - (level * spec_h as f32) as u32;
        if last_label_y.map_or(true, |prev| prev.saturating_sub(y) >= 14 || y.saturating_sub(prev) >= 14) {
            draw_text(&mut img, &font, &format!("{:.0} dB", db), pal_x + PAL_W + 4, y.saturating_sub(6), 11.0, gray(210));
            last_label_y = Some(y);
        }
        db -= db_step;
    }

    // --- Header ---
    let filename = std::path::Path::new(&info.path)
        .file_name().unwrap_or_default().to_string_lossy().to_string();
    draw_text(&mut img, &font, &filename, spec_x, 8, 14.0, gray(255));
    draw_text(&mut img, &font, &info.desc(), spec_x, 28, 11.0, gray(190));

    // --- Save ---
    let stem = std::path::Path::new(&info.path)
        .file_stem().unwrap_or_default().to_string_lossy().to_string();
    let out = std::env::current_dir()
        .unwrap_or_default()
        .join(format!("{}.png", stem));
    img.save(&out).map_err(|e| e.to_string())?;
    Ok(out.to_string_lossy().to_string())
}

// --- Drawing helpers ---

fn gray(v: u8) -> Rgba<u8> { Rgba([v, v, v, 255]) }

fn draw_rect_outline(img: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, c: Rgba<u8>) {
    hline(img, x, x + w, y, c);
    hline(img, x, x + w, y + h, c);
    vline(img, y, y + h, x, c);
    vline(img, y, y + h, x + w, c);
}

fn hline(img: &mut RgbaImage, x0: u32, x1: u32, y: u32, c: Rgba<u8>) {
    if y >= img.height() { return; }
    for x in x0..x1.min(img.width()) { img.put_pixel(x, y, c); }
}

fn vline(img: &mut RgbaImage, y0: u32, y1: u32, x: u32, c: Rgba<u8>) {
    if x >= img.width() { return; }
    for y in y0..y1.min(img.height()) { img.put_pixel(x, y, c); }
}

fn text_width(font: &FontRef, text: &str, size: f32) -> u32 {
    let scaled = font.as_scaled(PxScale::from(size));
    text.chars().map(|c| scaled.h_advance(scaled.glyph_id(c))).sum::<f32>() as u32
}

fn draw_text(img: &mut RgbaImage, font: &FontRef, text: &str, x: u32, y: u32, size: f32, color: Rgba<u8>) {
    let scaled = font.as_scaled(PxScale::from(size));
    let mut cursor = x as f32;
    for ch in text.chars() {
        let glyph = scaled.scaled_glyph(ch);
        if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, cov| {
                let px = (cursor + bounds.min.x + gx as f32) as i32;
                let py = (y as f32 + bounds.min.y + gy as f32) as i32;
                if px >= 0 && py >= 0 {
                    let px = px as u32;
                    let py = py as u32;
                    if px < img.width() && py < img.height() {
                        let bg = img.get_pixel(px, py);
                        let r = (color[0] as f32 * cov + bg[0] as f32 * (1.0 - cov)) as u8;
                        let g = (color[1] as f32 * cov + bg[1] as f32 * (1.0 - cov)) as u8;
                        let b = (color[2] as f32 * cov + bg[2] as f32 * (1.0 - cov)) as u8;
                        img.put_pixel(px, py, Rgba([r, g, b, 255]));
                    }
                }
            });
        }
        cursor += scaled.h_advance(scaled.glyph_id(ch));
    }
}

fn best_db_step(range: f32) -> f32 {
    for &s in &[2.0_f32, 5.0, 10.0, 20.0, 40.0] {
        if range / s <= 8.0 { return s; }
    }
    40.0
}

fn best_time_step(duration: f32) -> f32 {
    for &s in &[1.0_f32, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0] {
        if duration / s <= 12.0 { return s; }
    }
    600.0
}
