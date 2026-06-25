mod audio;
mod export;
mod palette;
mod pipeline;

use audio::AudioInfo;
use pipeline::{Channel, Msg, BANDS};

use eframe::egui::{self, Color32, ColorImage, Pos2, Rect, TextureHandle, TextureOptions};
use std::sync::mpsc;

const NUM_COLS: usize = 800;
const LOG_F_MIN: f32 = 20.0;

const LPAD: f32 = 58.0;
const RPAD: f32 = 82.0;
const TPAD: f32 = 76.0;
const BPAD: f32 = 36.0;

fn spec_rect(avail: Rect) -> Rect {
    Rect::from_min_max(
        Pos2::new(avail.min.x + LPAD, avail.min.y + TPAD),
        Pos2::new(avail.max.x - RPAD, avail.max.y - BPAD),
    )
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1100.0, 640.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Spektra",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            // Fallback font for CJK filenames (Arial Unicode covers the full BMP)
            if let Ok(data) = std::fs::read("/Library/Fonts/Arial Unicode.ttf") {
                let mut fonts = egui::FontDefinitions::default();
                fonts.font_data.insert(
                    "arial_unicode".to_owned(),
                    egui::FontData::from_owned(data).into(),
                );
                for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                    fonts.families.entry(family).or_default().push("arial_unicode".to_owned());
                }
                cc.egui_ctx.set_fonts(fonts);
            }
            Ok(Box::new(App::default()))
        }),
    )
}

// --- App ---

struct App {
    texture: Option<TextureHandle>,
    texture_r: Option<TextureHandle>,
    pixels: Vec<u8>,
    pixels_r: Vec<u8>,
    db_grid: Vec<f32>,
    db_grid_r: Vec<f32>,
    info: Option<AudioInfo>,
    rx: Option<mpsc::Receiver<Msg>>,
    cols_ready: usize,
    cols_ready_r: usize,
    status: String,
    log_freq: bool,
    ceil_db: f32,
    floor_db: f32,
    overlap: f32,
    channel: Channel,
}

impl Default for App {
    fn default() -> Self {
        Self {
            texture: None,
            texture_r: None,
            pixels: Vec::new(),
            pixels_r: Vec::new(),
            db_grid: Vec::new(),
            db_grid_r: Vec::new(),
            info: None,
            rx: None,
            cols_ready: 0,
            cols_ready_r: 0,
            status: String::new(),
            log_freq: false,
            ceil_db: 0.0,
            floor_db: -120.0,
            overlap: 0.5,
            channel: Channel::Mix,
        }
    }
}

struct HoverInfo {
    pos: Pos2,
    hover_spec: Rect,
    time_sec: f32,
    freq_hz: f32,
    db: f32,
    ch_tag: &'static str,
}

impl App {
    fn open(&mut self, path: String) {
        self.texture = None;
        self.texture_r = None;
        self.pixels = vec![0u8; NUM_COLS * BANDS * 4];
        self.pixels_r = vec![0u8; NUM_COLS * BANDS * 4];
        self.db_grid = vec![-120.0; NUM_COLS * BANDS];
        self.db_grid_r = vec![-120.0; NUM_COLS * BANDS];
        self.info = None;
        self.cols_ready = 0;
        self.cols_ready_r = 0;
        self.status = format!(
            "Loading {}…",
            std::path::Path::new(&path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        );
        let (tx, rx) = mpsc::channel::<Msg>();
        self.rx = Some(rx);
        let overlap = self.overlap;
        let channel = self.channel;
        std::thread::spawn(move || match audio::decode(&path) {
            Err(e) => { tx.send(Msg::Error(e)).ok(); }
            Ok((info, left, right)) => { pipeline::start(left, right, info, NUM_COLS, overlap, channel, tx); }
        });
    }

    fn do_save(&mut self) {
        if let Some(info) = &self.info {
            self.status = "Saving…".into();
            match export::save(info, &self.pixels, NUM_COLS, self.ceil_db, self.floor_db) {
                Ok(path) => self.status = format!("Saved → {}", path),
                Err(e) => self.status = format!("Save failed: {}", e),
            }
        }
    }

    fn reopen(&mut self) {
        if let Some(path) = self.info.as_ref().map(|i| i.path.clone()) {
            self.open(path);
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Drag-and-drop
        let dropped: Vec<_> = ui.ctx().input(|i| i.raw.dropped_files.clone());
        if let Some(f) = dropped.into_iter().find(|f| f.path.is_some()) {
            self.open(f.path.unwrap().to_string_lossy().to_string());
        }

        // Drain pipeline messages
        let mut dirty = false;
        if let Some(rx) = &self.rx {
            loop {
                match rx.try_recv() {
                    Ok(Msg::Info(info)) => { self.status.clear(); self.info = Some(info); }
                    Ok(Msg::Column(col, bands)) => {
                        write_column(&mut self.db_grid, col, &bands);
                        self.cols_ready = self.cols_ready.max(col + 1);
                        dirty = true;
                    }
                    Ok(Msg::ColumnR(col, bands)) => {
                        write_column(&mut self.db_grid_r, col, &bands);
                        self.cols_ready_r = self.cols_ready_r.max(col + 1);
                        dirty = true;
                    }
                    Ok(Msg::Done) => { self.rx = None; dirty = true; break; }
                    Ok(Msg::Error(e)) => { self.status = format!("Error: {}", e); self.rx = None; break; }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => { self.rx = None; break; }
                }
            }
        }

        if self.rx.is_some() {
            ui.ctx().request_repaint();
        }

        if ui.ctx().input(|i| i.key_pressed(egui::Key::S) && i.modifiers.command) && self.info.is_some() {
            self.do_save();
        }

        let avail = ui.max_rect();
        let spec = spec_rect(avail);

        // Background first (must precede all widgets so they render on top)
        ui.painter().rect_filled(avail, 0.0, Color32::BLACK);

        // --- Toolbar buttons ---
        // Toolbar: all gaps = 8 px, right margin = 8 px
        // right edges (from right): max.x-8, -71, -134, -206, -278
        let btn_rect = Rect::from_min_size(Pos2::new(avail.max.x - 63.0, avail.min.y + 50.0), egui::vec2(55.0, 22.0));
        let btn = ui.interact(btn_rect, egui::Id::new("open_btn"), egui::Sense::click());
        if btn.clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Audio", &["mp3", "wav", "flac", "ogg", "m4a", "aac", "opus", "wma", "aiff", "au"])
                .pick_file()
            {
                self.open(path.to_string_lossy().to_string());
            }
        }
        let save_rect = Rect::from_min_size(Pos2::new(avail.max.x - 134.0, avail.min.y + 50.0), egui::vec2(55.0, 22.0));
        let save_btn = ui.interact(save_rect, egui::Id::new("save_btn"), egui::Sense::click());
        if save_btn.clicked() && self.info.is_some() {
            self.do_save();
        }
        let log_rect = Rect::from_min_size(Pos2::new(avail.max.x - 214.0, avail.min.y + 50.0), egui::vec2(64.0, 22.0));
        let log_btn = ui.interact(log_rect, egui::Id::new("log_btn"), egui::Sense::click());
        if log_btn.clicked() {
            self.log_freq = !self.log_freq;
            dirty = true;
        }
        let ovlp_rect = Rect::from_min_size(Pos2::new(avail.max.x - 294.0, avail.min.y + 50.0), egui::vec2(64.0, 22.0));
        let prev_overlap = self.overlap;
        ui.scope_builder(egui::UiBuilder::new().max_rect(ovlp_rect), |ui| {
            combo_dark_style(ui);
            let label = match (self.overlap * 100.0).round() as u32 {
                0  => "OL: 0%",
                50 => "OL: 50%",
                _  => "OL: 75%",
            };
            egui::ComboBox::from_id_salt("ovlp_combo")
                .selected_text(label)
                .width(ovlp_rect.width() - 6.0)
                .show_ui(ui, |ui| {
                    ui.visuals_mut().override_text_color = Some(Color32::from_gray(180));
                    ui.selectable_value(&mut self.overlap, 0.0_f32,  "OL: 0%");
                    ui.selectable_value(&mut self.overlap, 0.5_f32,  "OL: 50%");
                    ui.selectable_value(&mut self.overlap, 0.75_f32, "OL: 75%");
                });
        });
        if (self.overlap - prev_overlap).abs() > 1e-6 {
            self.reopen();
        }

        let num_ch = self.info.as_ref().map(|i| i.channels).unwrap_or(0);
        let ch_rect = Rect::from_min_size(Pos2::new(avail.max.x - 362.0, avail.min.y + 50.0), egui::vec2(52.0, 22.0));
        let prev_channel = self.channel;
        ui.scope_builder(egui::UiBuilder::new().max_rect(ch_rect), |ui| {
            combo_dark_style(ui);
            let ch_label = if num_ch < 2 { "Mono" } else {
                match self.channel {
                    Channel::Mix   => "Mix",
                    Channel::Split => "L+R",
                    Channel::Left  => "L",
                    Channel::Right => "R",
                }
            };
            egui::ComboBox::from_id_salt("ch_combo")
                .selected_text(ch_label)
                .width(ch_rect.width() - 6.0)
                .show_ui(ui, |ui| {
                    ui.visuals_mut().override_text_color = Some(Color32::from_gray(180));
                    if num_ch >= 2 {
                        ui.selectable_value(&mut self.channel, Channel::Mix,   "Mix");
                        ui.selectable_value(&mut self.channel, Channel::Split, "L+R");
                        ui.selectable_value(&mut self.channel, Channel::Left,  "L");
                        ui.selectable_value(&mut self.channel, Channel::Right, "R");
                    }
                });
        });
        if self.channel != prev_channel {
            self.reopen();
        }

        // --- dB range sliders ---
        let prev_ceil = self.ceil_db;
        let prev_floor = self.floor_db;
        let slider_y = avail.min.y + 50.0;
        let ceil_rect  = Rect::from_min_size(Pos2::new(spec.min.x,          slider_y), egui::vec2(248.0, 22.0));
        let floor_rect = Rect::from_min_size(Pos2::new(spec.min.x + 256.0,  slider_y), egui::vec2(248.0, 22.0));
        ui.put(ceil_rect,
            egui::Slider::new(&mut self.ceil_db, -60.0..=0.0)
                .suffix(" dB").text("Ceil").step_by(1.0),
        );
        ui.put(floor_rect,
            egui::Slider::new(&mut self.floor_db, -160.0..=-10.0)
                .suffix(" dB").text("Floor").step_by(1.0),
        );
        if self.ceil_db - self.floor_db < 10.0 {
            if self.ceil_db != prev_ceil {
                self.floor_db = (self.ceil_db - 10.0).max(-160.0);
            } else {
                self.ceil_db = (self.floor_db + 10.0).min(0.0);
            }
        }
        if self.ceil_db != prev_ceil || self.floor_db != prev_floor {
            dirty = true;
        }

        let reset_rect = Rect::from_min_size(
            Pos2::new(spec.min.x + 512.0, slider_y),
            egui::vec2(36.0, 22.0),
        );
        let reset_btn = ui.interact(reset_rect, egui::Id::new("db_reset_btn"), egui::Sense::click());
        if reset_btn.clicked() {
            self.ceil_db = 0.0;
            self.floor_db = -120.0;
            dirty = true;
        }
        {
            let p = ui.painter();
            let fill = if reset_btn.hovered() { Color32::from_gray(70) } else { Color32::from_gray(40) };
            p.rect_filled(reset_rect, 4.0, fill);
            p.rect_stroke(reset_rect, 4.0, egui::Stroke::new(1.0, Color32::from_gray(130)), egui::StrokeKind::Middle);
            p.text(reset_rect.center(), egui::Align2::CENTER_CENTER, "↺",
                egui::FontId::proportional(14.0), Color32::from_gray(230));
        }

        // --- Rebuild textures ---
        let is_split = self.channel == Channel::Split;
        if dirty && self.info.is_some() {
            let nyquist = self.info.as_ref().unwrap().sample_rate as f32 / 2.0;

            self.pixels = build_pixels(&self.db_grid, self.log_freq, nyquist, self.ceil_db, self.floor_db);
            let image = ColorImage::from_rgba_unmultiplied([NUM_COLS, BANDS], &self.pixels);
            set_texture(&mut self.texture, image, ui.ctx(), "spektra_l");

            if is_split {
                self.pixels_r = build_pixels(&self.db_grid_r, self.log_freq, nyquist, self.ceil_db, self.floor_db);
                let image_r = ColorImage::from_rgba_unmultiplied([NUM_COLS, BANDS], &self.pixels_r);
                set_texture(&mut self.texture_r, image_r, ui.ctx(), "spektra_r");
            }
        }

        // --- Split sub-rects ---
        const SPLIT_GAP: f32 = 4.0;
        let (spec_l, spec_r) = if is_split {
            let half_h = (spec.height() - SPLIT_GAP) / 2.0;
            (
                Rect::from_min_size(spec.min, egui::vec2(spec.width(), half_h)),
                Rect::from_min_max(Pos2::new(spec.min.x, spec.max.y - half_h), spec.max),
            )
        } else {
            (spec, spec)
        };

        // --- Hover ---
        let nyquist = self.info.as_ref().map(|i| i.sample_rate as f32 / 2.0).unwrap_or(22050.0);
        let log_freq = self.log_freq;
        let floor_db = self.floor_db;

        let hover = self.info.as_ref().and_then(|info| {
            ui.ctx().input(|i| i.pointer.hover_pos()).and_then(|pos| {
                let (hover_spec, ch_tag): (Rect, &'static str) = if is_split {
                    if spec_l.contains(pos) { (spec_l, "L") }
                    else if spec_r.contains(pos) { (spec_r, "R") }
                    else { return None; }
                } else {
                    if !spec.contains(pos) { return None; }
                    (spec, "")
                };

                let tx = (pos.x - hover_spec.min.x) / hover_spec.width();
                let ty = (hover_spec.max.y - pos.y) / hover_spec.height();
                let freq_hz = if log_freq {
                    (LOG_F_MIN * (nyquist / LOG_F_MIN).powf(ty)).min(nyquist)
                } else {
                    ty * nyquist
                };
                let time_sec = (tx * info.duration as f32).max(0.0);
                let col = ((tx * NUM_COLS as f32) as usize).min(NUM_COLS - 1);
                let band = ((freq_hz / nyquist * BANDS as f32) as usize).min(BANDS - 1);
                let grid = if ch_tag == "R" { &self.db_grid_r } else { &self.db_grid };
                let db = grid.get(col * BANDS + band).copied().unwrap_or(floor_db);
                Some(HoverInfo { pos, hover_spec, time_sec, freq_hz, db, ch_tag })
            })
        });

        // --- Draw ---
        let p = ui.painter();
        draw(p, avail, spec, spec_l, spec_r,
             &self.texture, &self.texture_r,
             self.info.as_ref(), &self.status,
             self.cols_ready, self.cols_ready_r,
             &btn, &save_btn, &log_btn,
             log_freq,
             self.ceil_db, self.floor_db, hover.as_ref());
        ui.allocate_rect(avail, egui::Sense::hover());
    }
}

// --- Drawing ---

#[allow(clippy::too_many_arguments)]
fn draw(
    p: &egui::Painter,
    avail: Rect,
    spec: Rect,
    spec_l: Rect,
    spec_r: Rect,
    texture: &Option<TextureHandle>,
    texture_r: &Option<TextureHandle>,
    info: Option<&AudioInfo>,
    status: &str,
    cols_ready: usize,
    cols_ready_r: usize,
    btn: &egui::Response,
    save_btn: &egui::Response,
    log_btn: &egui::Response,
    log_freq: bool,
    ceil_db: f32,
    floor_db: f32,
    hover: Option<&HoverInfo>,
) {
    let is_split = spec_l != spec_r;
    let gap = 8.0_f32;
    let pal_w = 12.0_f32;

    // Header
    let title = info.map(|i| std::path::Path::new(&i.path)
        .file_name().unwrap_or_default().to_string_lossy().to_string())
        .unwrap_or_else(|| "Spektra".into());
    p.text(Pos2::new(spec.min.x, avail.min.y + 8.0), egui::Align2::LEFT_TOP,
        &title, egui::FontId::proportional(15.0), Color32::WHITE);

    let subtitle = if !status.is_empty() { status.to_string() }
        else { info.map(|i| i.desc()).unwrap_or_default() };
    p.text(Pos2::new(spec.min.x, avail.min.y + 28.0), egui::Align2::LEFT_TOP,
        &subtitle, egui::FontId::proportional(11.0), Color32::from_gray(180));

    // Button drawing helper
    let draw_btn = |rect: Rect, label: &str, resp: &egui::Response, active: bool| {
        let fill = if resp.hovered() {
            Color32::from_gray(70)
        } else if active {
            Color32::from_rgb(25, 55, 95)
        } else {
            Color32::from_gray(40)
        };
        p.rect_filled(rect, 4.0, fill);
        p.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, Color32::from_gray(130)), egui::StrokeKind::Middle);
        p.text(rect.center(), egui::Align2::CENTER_CENTER, label,
            egui::FontId::proportional(12.0), Color32::from_gray(230));
    };

    draw_btn(btn.rect, "Open", btn, false);
    if info.is_some() { draw_btn(save_btn.rect, "Save PNG", save_btn, false); }

    let log_label = if log_freq { "Freq: Log" } else { "Freq: Lin" };
    draw_btn(log_btn.rect, log_label, log_btn, log_freq);

    // Spectrogram(s)
    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    if is_split {
        // L half (top)
        if let Some(tex) = texture {
            p.image(tex.id(), spec_l, uv, Color32::WHITE);
        }
        // R half (bottom)
        if let Some(tex) = texture_r {
            p.image(tex.id(), spec_r, uv, Color32::WHITE);
        }
        // No texture yet
        if texture.is_none() {
            p.text(spec.center(), egui::Align2::CENTER_CENTER,
                "Drop an audio file here, or click Open",
                egui::FontId::proportional(16.0), Color32::from_gray(55));
        }
        // "L" / "R" channel labels
        let lbl_font = egui::FontId::proportional(11.0);
        let lbl_color = Color32::from_gray(160);
        p.text(Pos2::new(spec_l.min.x - 14.0, spec_l.center().y),
            egui::Align2::CENTER_CENTER, "L", lbl_font.clone(), lbl_color);
        p.text(Pos2::new(spec_r.min.x - 14.0, spec_r.center().y),
            egui::Align2::CENTER_CENTER, "R", lbl_font, lbl_color);
        // Borders
        p.rect_stroke(spec_l, 0.0, egui::Stroke::new(1.0, Color32::from_gray(70)), egui::StrokeKind::Middle);
        p.rect_stroke(spec_r, 0.0, egui::Stroke::new(1.0, Color32::from_gray(70)), egui::StrokeKind::Middle);
        // Progress lines
        for (cr, sr) in [(cols_ready, spec_l), (cols_ready_r, spec_r)] {
            if cr > 0 && cr < NUM_COLS {
                let cx = sr.min.x + (cr as f32 / NUM_COLS as f32) * sr.width();
                p.line_segment([Pos2::new(cx, sr.min.y), Pos2::new(cx, sr.max.y)],
                    egui::Stroke::new(1.5, Color32::from_rgba_unmultiplied(255, 255, 255, 90)));
            }
        }
    } else {
        if let Some(tex) = texture {
            p.image(tex.id(), spec, uv, Color32::WHITE);
            if cols_ready > 0 && cols_ready < NUM_COLS {
                let cx = spec.min.x + (cols_ready as f32 / NUM_COLS as f32) * spec.width();
                p.line_segment([Pos2::new(cx, spec.min.y), Pos2::new(cx, spec.max.y)],
                    egui::Stroke::new(1.5, Color32::from_rgba_unmultiplied(255, 255, 255, 90)));
            }
        } else {
            p.text(spec.center(), egui::Align2::CENTER_CENTER,
                "Drop an audio file here, or click Open",
                egui::FontId::proportional(16.0), Color32::from_gray(55));
        }
        p.rect_stroke(spec, 0.0, egui::Stroke::new(1.0, Color32::from_gray(70)), egui::StrokeKind::Middle);
    }

    // Frequency axis — drawn against spec_l in split mode, spec otherwise
    let freq_spec = spec_l;
    if let Some(info) = info {
        let nyquist = info.sample_rate as f32 / 2.0;

        if log_freq {
            const LOG_TICKS: &[f32] = &[
                20.0, 50.0, 100.0, 200.0, 500.0,
                1_000.0, 2_000.0, 5_000.0, 10_000.0, 20_000.0, 48_000.0, 96_000.0,
            ];
            let mut last_y: Option<f32> = None;
            for &hz in LOG_TICKS {
                if hz > nyquist { break; }
                let t = ((hz / LOG_F_MIN).ln() / (nyquist / LOG_F_MIN).ln()).clamp(0.0, 1.0);
                let y = freq_spec.max.y - t * freq_spec.height();
                p.line_segment([Pos2::new(freq_spec.min.x - 5.0, y), Pos2::new(freq_spec.min.x, y)],
                    egui::Stroke::new(1.0, Color32::from_gray(110)));
                let label = if hz < 1000.0 {
                    format!("{} Hz", hz as u32)
                } else {
                    format!("{} kHz", hz as u32 / 1000)
                };
                if last_y.map_or(true, |prev| prev - y >= 14.0) {
                    p.text(Pos2::new(freq_spec.min.x - 7.0, y), egui::Align2::RIGHT_CENTER,
                        label, egui::FontId::proportional(10.0), Color32::from_gray(200));
                    last_y = Some(y);
                }
            }
        } else {
            let mut last_y: Option<f32> = None;
            let mut hz = 0u32;
            while hz as f32 <= nyquist {
                let t = hz as f32 / nyquist;
                let y = freq_spec.max.y - t * freq_spec.height();
                p.line_segment([Pos2::new(freq_spec.min.x - 5.0, y), Pos2::new(freq_spec.min.x, y)],
                    egui::Stroke::new(1.0, Color32::from_gray(110)));
                let label = if hz == 0 { "0".to_string() } else { format!("{} kHz", hz / 1000) };
                if last_y.map_or(true, |prev| prev - y >= 14.0) {
                    p.text(Pos2::new(freq_spec.min.x - 7.0, y), egui::Align2::RIGHT_CENTER,
                        label, egui::FontId::proportional(10.0), Color32::from_gray(200));
                    last_y = Some(y);
                }
                hz += 5000;
            }
        }

        // Time axis — always along bottom of spec (= spec_r.max.y in split)
        let duration = info.duration as f32;
        if duration > 0.0 {
            let step = best_time_step(duration);
            let mut t = step;
            while t < duration {
                let x = spec.min.x + (t / duration) * spec.width();
                p.line_segment([Pos2::new(x, spec.max.y), Pos2::new(x, spec.max.y + 5.0)],
                    egui::Stroke::new(1.0, Color32::from_gray(110)));
                p.text(Pos2::new(x, spec.max.y + 7.0), egui::Align2::CENTER_TOP,
                    format!("{}:{:02}", t as u32 / 60, t as u32 % 60),
                    egui::FontId::proportional(10.0), Color32::from_gray(200));
                t += step;
            }
        }
    }

    // Palette bar (spans full spec height regardless of split)
    let pal = Rect::from_min_max(
        Pos2::new(spec.max.x + gap, spec.min.y),
        Pos2::new(spec.max.x + gap + pal_w, spec.max.y));
    for i in 0..200u32 {
        let level = i as f64 / 200.0;
        let rgba = palette::sox(level);
        let y_bot = pal.max.y - i as f32 / 200.0 * pal.height();
        let y_top = pal.max.y - (i + 1) as f32 / 200.0 * pal.height();
        p.rect_filled(Rect::from_min_max(Pos2::new(pal.min.x, y_top), Pos2::new(pal.max.x, y_bot)),
            0.0, Color32::from_rgb(rgba[0], rgba[1], rgba[2]));
    }

    let db_step = best_db_step(ceil_db - floor_db);
    let first_db = (ceil_db / db_step).floor() * db_step;
    let mut db = first_db;
    let mut last_label_y: Option<f32> = None;
    while db >= floor_db - 0.5 {
        let level = ((db - floor_db) / (ceil_db - floor_db)).clamp(0.0, 1.0);
        let y = spec.max.y - level * spec.height();
        if last_label_y.map_or(true, |prev| (y - prev).abs() >= 14.0) {
            p.text(Pos2::new(pal.max.x + 4.0, y), egui::Align2::LEFT_CENTER,
                format!("{:.0} dB", db), egui::FontId::proportional(10.0), Color32::from_gray(200));
            last_label_y = Some(y);
        }
        db -= db_step;
    }

    // Hover crosshair + info box
    if let Some(h) = hover {
        let hs = h.hover_spec;
        let stroke = egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 55));
        p.line_segment([Pos2::new(hs.min.x, h.pos.y), Pos2::new(hs.max.x, h.pos.y)], stroke);
        p.line_segment([Pos2::new(h.pos.x, hs.min.y), Pos2::new(h.pos.x, hs.max.y)], stroke);

        let freq_str = if h.freq_hz >= 1000.0 {
            format!("{:.2} kHz", h.freq_hz / 1000.0)
        } else {
            format!("{:.0} Hz", h.freq_hz)
        };
        let note = freq_to_note(h.freq_hz);
        let mins = h.time_sec as u32 / 60;
        let secs = h.time_sec % 60.0;
        let ch_prefix = if h.ch_tag.is_empty() { String::new() } else { format!("[{}] ", h.ch_tag) };
        let info_line = format!("{}{}:{:05.2}  {}  {}  {:.1} dB",
            ch_prefix, mins, secs, freq_str, note, h.db.max(floor_db));

        let font = egui::FontId::monospace(11.0);
        let text_size = p.layout_no_wrap(info_line.clone(), font.clone(), Color32::WHITE).size();
        let pad = egui::vec2(6.0, 4.0);
        let box_size = text_size + pad * 2.0;

        let offset = egui::vec2(12.0, -box_size.y - 8.0);
        let mut box_min = h.pos + offset;
        if box_min.x + box_size.x > spec.max.x { box_min.x = h.pos.x - box_size.x - 8.0; }
        if box_min.y < spec.min.y { box_min.y = h.pos.y + 8.0; }
        let box_rect = Rect::from_min_size(box_min, box_size);

        p.rect_filled(box_rect, 4.0, Color32::from_rgba_unmultiplied(0, 0, 0, 200));
        p.rect_stroke(box_rect, 4.0, egui::Stroke::new(1.0, Color32::from_gray(80)), egui::StrokeKind::Middle);
        p.text(box_min + pad, egui::Align2::LEFT_TOP, &info_line, font, Color32::WHITE);
    }
}

// --- Helpers ---

fn combo_dark_style(ui: &mut egui::Ui) {
    // Match the 22px toolbar button height
    ui.spacing_mut().interact_size.y = 22.0;
    ui.spacing_mut().button_padding  = egui::vec2(4.0, 1.0);

    // Local UI visuals — controls the button face (trigger)
    {
        let v = ui.visuals_mut();
        let stroke = egui::Stroke::new(1.0, Color32::from_gray(130));
        for wv in [
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            wv.bg_stroke  = stroke;
            wv.expansion  = 0.0; // no bleed outside rect — keeps 8px gaps intact
        }
        v.widgets.inactive.weak_bg_fill = Color32::from_gray(40);
        v.widgets.hovered.weak_bg_fill  = Color32::from_gray(70);
        v.widgets.active.weak_bg_fill   = Color32::from_gray(55);
        v.widgets.open.weak_bg_fill     = Color32::from_gray(55);
    }
    // Global context visuals — the popup Area reads ctx.style(), not ui.style()
    ui.ctx().global_style_mut(|s| {
        s.visuals.window_fill = Color32::from_gray(32);
        s.visuals.panel_fill  = Color32::from_gray(32);
        s.visuals.widgets.inactive.weak_bg_fill = Color32::from_gray(40);
        s.visuals.widgets.hovered.weak_bg_fill  = Color32::from_gray(60);
        s.visuals.widgets.active.weak_bg_fill   = Color32::from_gray(50);
    });
}

fn set_texture(tex: &mut Option<TextureHandle>, image: ColorImage, ctx: &egui::Context, name: &str) {
    if let Some(t) = tex {
        t.set(image, TextureOptions::LINEAR);
    } else {
        *tex = Some(ctx.load_texture(name, image, TextureOptions::LINEAR));
    }
}

fn build_pixels(db_grid: &[f32], log_freq: bool, nyquist: f32, ceil_db: f32, floor_db: f32) -> Vec<u8> {
    let range = ceil_db - floor_db;
    let mut pixels = vec![0u8; NUM_COLS * BANDS * 4];
    for col in 0..NUM_COLS {
        for row in 0..BANDS {
            let band = if log_freq {
                let t = (BANDS - 1 - row) as f32 / (BANDS - 1) as f32;
                let freq = LOG_F_MIN * (nyquist / LOG_F_MIN).powf(t);
                ((freq / nyquist) * (BANDS - 1) as f32).round() as usize
            } else {
                BANDS - 1 - row
            };
            let band = band.clamp(0, BANDS - 1);
            let db = db_grid.get(col * BANDS + band).copied().unwrap_or(floor_db);
            let level = ((db - floor_db) / range).clamp(0.0, 1.0) as f64;
            let rgba = palette::sox(level);
            let idx = (row * NUM_COLS + col) * 4;
            pixels[idx..idx + 4].copy_from_slice(&rgba);
        }
    }
    pixels
}

fn write_column(db_grid: &mut Vec<f32>, col: usize, bands: &[f32]) {
    for (band, &db) in bands.iter().enumerate() {
        let gi = col * BANDS + band;
        if gi < db_grid.len() {
            db_grid[gi] = db;
        }
    }
}

fn freq_to_note(hz: f32) -> String {
    if hz < 16.0 { return "—".into(); }
    const NAMES: [&str; 12] = ["C","C#","D","D#","E","F","F#","G","G#","A","A#","B"];
    let midi = 69.0 + 12.0 * (hz / 440.0).log2();
    let midi_i = midi.round() as i32;
    let note = NAMES[((midi_i % 12 + 12) as usize) % 12];
    let octave = midi_i / 12 - 1;
    let cents = (midi - midi_i as f32) * 100.0;
    if cents.abs() > 3.0 {
        format!("{}{} ({:+.0}¢)", note, octave, cents)
    } else {
        format!("{}{}", note, octave)
    }
}

fn best_time_step(duration: f32) -> f32 {
    for &s in &[1.0_f32, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0] {
        if duration / s <= 12.0 { return s; }
    }
    600.0
}

fn best_db_step(range: f32) -> f32 {
    for &s in &[2.0_f32, 5.0, 10.0, 20.0, 40.0] {
        if range / s <= 8.0 { return s; }
    }
    40.0
}
