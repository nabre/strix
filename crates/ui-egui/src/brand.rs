//! The Strix brand inside the app: the symbol (the app icon) and the wordmark (STRIX, its I a
//! feather). The images are rendered from `assets/app-icon/strix.svg` and
//! `docs/brand/wordmark-{light,dark}.svg` by `packaging/icons.sh`; the rules for using them are in
//! `docs/brand/MARKETING.md`, section 7.

use egui::{Color32, Rect, Sense, TextureHandle, Ui, pos2, vec2};

const SYMBOL: &[u8] = include_bytes!("../../../assets/app-icon/hicolor/256x256/apps/io.github.nabre.strix.png");
const WORDMARK_LIGHT: &[u8] = include_bytes!("../../../assets/brand/wordmark-light.png");
const WORDMARK_DARK: &[u8] = include_bytes!("../../../assets/brand/wordmark-dark.png");

/// One of the brand's images.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Symbol,
    /// Deep Forest letters, for light backgrounds.
    WordmarkLight,
    /// Crest Mint letters, for dark backgrounds.
    WordmarkDark,
}

impl Mark {
    /// The wordmark that reads on a dark (`true`) or light background.
    pub fn wordmark(on_dark: bool) -> Self {
        if on_dark { Mark::WordmarkDark } else { Mark::WordmarkLight }
    }

    fn name(self) -> &'static str {
        match self {
            Mark::Symbol => "strix.brand.symbol",
            Mark::WordmarkLight => "strix.brand.wordmark-light",
            Mark::WordmarkDark => "strix.brand.wordmark-dark",
        }
    }

    fn png(self) -> &'static [u8] {
        match self {
            Mark::Symbol => SYMBOL,
            Mark::WordmarkLight => WORDMARK_LIGHT,
            Mark::WordmarkDark => WORDMARK_DARK,
        }
    }
}

fn decode(png: &[u8]) -> Option<egui::ColorImage> {
    let img = image::load_from_memory_with_format(png, image::ImageFormat::Png).ok()?.to_rgba8();
    Some(egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()))
}

/// The mark's texture, uploaded once per context; `None` (logged once) if its image doesn't decode.
fn texture(ctx: &egui::Context, mark: Mark) -> Option<TextureHandle> {
    let id = egui::Id::unique(mark.name());
    if let Some(done) = ctx.data(|d| d.get_temp::<Option<TextureHandle>>(id)) {
        return done;
    }
    // Mipmaps: the images are drawn well below their size, and plain linear filtering shimmers.
    let options = egui::TextureOptions::LINEAR.with_mipmap_mode(Some(egui::TextureFilter::Linear));
    let tex = decode(mark.png()).map(|img| ctx.load_texture(mark.name(), img, options));
    if tex.is_none() {
        log::error!("brand: {} does not decode", mark.name());
    }
    ctx.data_mut(|d| d.insert_temp(id, tex.clone()));
    tex
}

/// Draw `mark` `height` points tall with its left edge at `left` and its centre at `y`; returns the
/// rectangle drawn (empty if the image is missing).
pub fn paint(ui: &Ui, mark: Mark, left: f32, y: f32, height: f32) -> Rect {
    let Some(tex) = texture(ui.ctx(), mark) else {
        return Rect::NOTHING;
    };
    let [w, h] = tex.size();
    let width = if h == 0 { 0.0 } else { height * w as f32 / h as f32 };
    let rect = Rect::from_min_size(pos2(left, y - height / 2.0), vec2(width, height));
    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    ui.painter().image(tex.id(), rect, uv, Color32::WHITE);
    rect
}

/// The symbol and the wordmark side by side, as the About box shows them.
pub fn lockup(ui: &mut Ui, height: f32) {
    let on_dark = crate::theme::Tokens::get(ui.ctx()).dark;
    let word_h = height * 0.62;
    let (r, resp) = ui.allocate_exact_size(vec2(height + 14.0 + word_h * 3.3, height), Sense::hover());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, "Strix"));
    let sym = paint(ui, Mark::Symbol, r.min.x, r.center().y, height);
    paint(ui, Mark::wordmark(on_dark), sym.max.x.max(r.min.x) + 14.0, r.center().y, word_h);
}

/// The empty window's page area (no document open, #1129): the symbol over the wordmark, centred.
pub fn empty_window(ui: &mut Ui) {
    let on_dark = crate::theme::Tokens::get(ui.ctx()).dark;
    let area = ui.max_rect();
    let symbol = (area.height() * 0.22).clamp(48.0, 128.0);
    let word = symbol * 0.42;
    if area.height() < symbol + word + 40.0 || area.width() < symbol * 1.6 {
        return;
    }
    let top = area.center().y - (symbol + 18.0 + word) / 2.0;
    let sym_w = paint(ui, Mark::Symbol, area.center().x - symbol / 2.0, top + symbol / 2.0, symbol).width();
    let word_w = word * 3.3;
    paint(ui, Mark::wordmark(on_dark), area.center().x - word_w / 2.0, top + symbol + 18.0 + word / 2.0, word);
    if sym_w > 0.0 {
        ui.interact(area, ui.scope_id().with("strix.brand.empty"), Sense::hover())
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, "Strix"));
    }
}

/// The wordmark at the foot of the File page's side menu (Deep Forest, so the light-on-dark one),
/// when the menu has room for it.
pub fn rail(ui: &mut Ui) {
    const HEIGHT: f32 = 20.0;
    if ui.available_height() < HEIGHT + 28.0 {
        return;
    }
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), HEIGHT + 16.0), Sense::hover());
    paint(ui, Mark::WordmarkDark, r.min.x + 22.0, r.center().y, HEIGHT);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mark_decodes_at_its_aspect() {
        let symbol = decode(SYMBOL).unwrap();
        assert_eq!(symbol.size, [256, 256]);
        for png in [WORDMARK_LIGHT, WORDMARK_DARK] {
            let img = decode(png).unwrap();
            let [w, h] = img.size;
            assert_eq!(h, 192);
            // viewBox 290 x 88.
            assert!((w as f32 / h as f32 - 290.0 / 88.0).abs() < 0.02, "{w}x{h}");
        }
        assert_eq!(Mark::wordmark(true), Mark::WordmarkDark);
        assert_eq!(Mark::wordmark(false), Mark::WordmarkLight);
    }

    #[test]
    fn drawing_the_marks_uploads_each_texture_once() {
        let ctx = egui::Context::default();
        for theme in [egui::Theme::Light, egui::Theme::Dark, egui::Theme::Dark] {
            ctx.set_theme(theme);
            ctx.run_ui(egui::RawInput::default(), |ui| {
                lockup(ui, 56.0);
                empty_window(ui);
                rail(ui);
            })
            .drop_without_applying_deltas();
        }
        for mark in [Mark::Symbol, Mark::WordmarkLight, Mark::WordmarkDark] {
            assert!(ctx.data(|d| d.get_temp::<Option<TextureHandle>>(egui::Id::unique(mark.name()))).flatten().is_some(), "{mark:?}");
        }
    }
}
