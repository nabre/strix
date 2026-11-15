//! The body of every colour drop-down, laid out like Word's: a **High contrast only** switch, the
//! menu's own first choice (Automatic, No Color, ByLayer…) with its swatch, **Theme Colors** (the
//! ten theme colours, then five rows of their tints and shades in columns), **Standard Colors**,
//! Recent Colors and **More Colors…**. A menu that can also give a gradient adds its Gradient
//! submenu after it ([`text_gradient_menu`] for text).
//!
//! `colors.rs` keeps what a pick does (targets) and the Colors dialog; panes and dialogs that edit
//! a colour without the app use [`color_button`], whose More Colors… goes through the same dialog.
//! Everything is drawn in code.

use egui::{Color32, Id, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};
use wordcraft_doc::Rgb;

use crate::colors::Pick;
use crate::theme::{Tokens, c32};

/// A swatch's side, in points.
pub const SWATCH: f32 = 18.0;
/// The gap between swatch columns (and between the theme row and its tints).
const GAP: f32 = 5.0;
/// The grid's width: ten columns.
pub const GRID_W: f32 = SWATCH * 10.0 + GAP * 9.0;
/// Word's standard colours.
pub const STANDARD: [&str; 10] = ["C00000", "FF0000", "FFC000", "FFFF00", "92D050", "00B050", "00B0F0", "0070C0", "002060", "7030A0"];
/// The order theme colours are shown in: Light 1, Dark 1, Light 2, Dark 2, the accents.
const THEME_ORDER: [usize; 10] = [1, 0, 3, 2, 4, 5, 6, 7, 8, 9];
/// The contrast a colour needs to be shown with High contrast only (WCAG AA for text).
pub const HIGH_CONTRAST: f32 = 4.5;

/// The swatch beside a menu's first choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TopSwatch {
    Color(Rgb),
    /// No colour: an empty box crossed by a diagonal.
    Empty,
}

/// A menu's first choice above the theme colours: Automatic, No Color, ByLayer….
#[derive(Clone, Debug, PartialEq)]
pub struct TopItem {
    /// Already in the interface language.
    pub label: String,
    pub swatch: TopSwatch,
    /// What a command target gets at its colour's place (`"auto"`, `null`…).
    pub value: Value,
    /// How hover previews and the in-effect mark name it (`"auto"`, `"none"`…).
    pub mark: String,
    /// Params that replace the target's own (the choice is another kind of value, such as no
    /// outline at all rather than an outline without colour).
    pub params: Option<Value>,
}

impl TopItem {
    /// Automatic, with a black swatch: the command gets `"auto"`.
    pub fn automatic() -> TopItem {
        TopItem::new("Automatic", TopSwatch::Color(Rgb::BLACK), json!("auto"), "auto")
    }

    /// `label` (untranslated) with `swatch`; the command gets `value`, menus mark it as `mark`.
    pub fn new(label: &str, swatch: TopSwatch, value: Value, mark: &str) -> TopItem {
        TopItem { label: tl!(label).to_string(), swatch, value, mark: mark.into(), params: None }
    }

    /// The choice runs the target's command with `params` instead.
    pub fn with_params(mut self, params: Value) -> TopItem {
        self.params = Some(params);
        self
    }

    /// `label` (untranslated) with an empty swatch: the command gets `value`.
    pub fn none(label: &str, value: Value) -> TopItem {
        TopItem::new(label, TopSwatch::Empty, value, "none")
    }
}

/// How a colour drop-down looks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Options {
    /// The first choices, in order.
    pub top: Vec<TopItem>,
    /// The colour fills behind text (shading, page colour, shape fill): High contrast only keeps
    /// the colours black text reads on. Otherwise the colour is for text or lines on the page,
    /// and High contrast only keeps the colours that read on white.
    pub fill: bool,
}

impl Options {
    pub fn top(top: TopItem) -> Options {
        Options { top: vec![top], fill: false }
    }

    pub fn tops(top: Vec<TopItem>) -> Options {
        Options { top, fill: false }
    }

    pub fn fill(mut self) -> Options {
        self.fill = true;
        self
    }
}

// ---------------------------------------------------------------------------------------------
// Pure helpers

/// `c` made lighter by `k` (0–1, towards white) or, with a negative `k`, darker (towards black).
pub fn tint(c: Rgb, k: f32) -> Rgb {
    let k = if k.is_finite() { k.clamp(-1.0, 1.0) } else { 0.0 };
    let f = |v: u8| {
        let v = v as f32;
        let out = if k >= 0.0 { v + (255.0 - v) * k } else { v * (1.0 + k) };
        out.round().clamp(0.0, 255.0) as u8
    };
    Rgb(f(c.0), f(c.1), f(c.2))
}

/// The five tints and shades Word puts under a theme colour, as signed fractions for [`tint`]:
/// white gets darker steps, black lighter ones, very light and very dark colours longer ranges,
/// any other colour lighter 80/60/40 % and darker 25/50 %.
pub fn tint_steps(c: Rgb) -> [f32; 5] {
    let (_, _, l) = crate::colors::to_hsl(c);
    if l >= 0.99 {
        [-0.05, -0.15, -0.25, -0.35, -0.5]
    } else if l <= 0.01 {
        [0.5, 0.35, 0.25, 0.15, 0.05]
    } else if l >= 0.8 {
        [-0.1, -0.25, -0.5, -0.75, -0.9]
    } else if l <= 0.2 {
        [0.9, 0.75, 0.5, -0.25, -0.5]
    } else {
        [0.8, 0.6, 0.4, -0.25, -0.5]
    }
}

/// The theme colours as shown (ten, in Word's order); a short theme is padded with greys.
pub fn theme_row(theme: &[Rgb]) -> [Rgb; 10] {
    THEME_ORDER.map(|i| theme.get(i).copied().unwrap_or(Rgb(128, 128, 128)))
}

/// WCAG relative luminance of a colour.
pub fn luminance(c: Rgb) -> f32 {
    let lin = |v: u8| {
        let s = v as f32 / 255.0;
        if s <= 0.03928 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * lin(c.0) + 0.7152 * lin(c.1) + 0.0722 * lin(c.2)
}

/// WCAG contrast ratio of two colours (1–21).
pub fn contrast(a: Rgb, b: Rgb) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Whether High contrast only keeps `c`.
pub fn readable(c: Rgb, fill: bool) -> bool {
    let against = if fill { Rgb::BLACK } else { Rgb::WHITE };
    contrast(c, against) >= HIGH_CONTRAST
}

// ---------------------------------------------------------------------------------------------
// High contrast only: one switch for every menu, kept in the preferences.

fn high_contrast_id() -> Id {
    Id::unique("strix.colors.highContrast")
}

fn high_contrast_changed_id() -> Id {
    Id::unique("strix.colors.highContrastChanged")
}

/// Whether High contrast only is on.
pub fn high_contrast(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(high_contrast_id())).unwrap_or(false)
}

fn set_high_contrast(ctx: &egui::Context, on: bool) {
    ctx.data_mut(|d| {
        d.insert_temp(high_contrast_id(), on);
        d.insert_temp(high_contrast_changed_id(), true);
    });
}

/// Once a frame: a switch flipped in a menu goes to the preference, else the preference is what
/// the menus show.
pub fn sync_high_contrast(ctx: &egui::Context, pref: &mut bool) {
    let changed = ctx.data_mut(|d| d.remove_temp::<bool>(high_contrast_changed_id())).unwrap_or(false);
    if changed {
        *pref = high_contrast(ctx);
    } else {
        ctx.data_mut(|d| d.insert_temp(high_contrast_id(), *pref));
    }
}

// ---------------------------------------------------------------------------------------------
// Drawing

/// The whole drop-down: switch, first choice, theme and standard colours, Recent Colors, More
/// Colors….
pub fn body(ui: &mut Ui, theme: &[Rgb], opts: &Options) -> Option<Pick> {
    let mut picked = None;
    switch_row(ui);
    if !opts.top.is_empty() {
        ui.separator();
    }
    for (i, top) in opts.top.iter().enumerate() {
        if top_row(ui, top) {
            picked = Some(Pick::Top(i));
        }
    }
    ui.separator();
    if let Some(hex) = grid(ui, theme, opts.fill) {
        picked = Some(Pick::Color(hex));
    }
    let recent = crate::colors::recent(&ui.data(|d| d.get_temp::<Vec<String>>(crate::colors::recent_id())).unwrap_or_default());
    if !recent.is_empty() {
        ui.add_space(2.0);
        heading(ui, tl!("Recent Colors"));
        let hc = high_contrast(ui.ctx());
        row(ui, |ui| {
            for c in recent {
                if cell(ui, c, !hc || readable(c, opts.fill)) {
                    picked = Some(Pick::Color(c.hex()));
                }
            }
        });
    }
    ui.add_space(2.0);
    ui.separator();
    if more_item(ui).clicked() {
        picked = Some(Pick::More);
    }
    picked
}

/// Theme Colors (with their tints and shades) and Standard Colors. Returns a picked hex.
pub fn grid(ui: &mut Ui, theme: &[Rgb], fill: bool) -> Option<String> {
    let mut picked = None;
    let hc = high_contrast(ui.ctx());
    let shown = |c: Rgb| !hc || readable(c, fill);
    heading(ui, tl!("Theme Colors"));
    let base = theme_row(theme);
    row(ui, |ui| {
        for c in base {
            if cell(ui, c, shown(c)) {
                picked = Some(c.hex());
            }
        }
    });
    ui.add_space(GAP);
    let steps = base.map(tint_steps);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for k in 0..5 {
            row(ui, |ui| {
                for (c, s) in base.iter().zip(&steps) {
                    let t = tint(*c, s.get(k).copied().unwrap_or(0.0));
                    if cell(ui, t, shown(t)) {
                        picked = Some(t.hex());
                    }
                }
            });
        }
    });
    ui.add_space(2.0);
    ui.separator();
    heading(ui, tl!("Standard Colors"));
    row(ui, |ui| {
        for c in STANDARD.iter().filter_map(|h| Rgb::parse(h)) {
            if cell(ui, c, shown(c)) {
                picked = Some(c.hex());
            }
        }
    });
    picked
}

/// A row of swatches.
fn row(ui: &mut Ui, f: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = GAP;
        f(ui);
    });
}

/// A section's heading. Its accessible label is the text.
fn heading(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).strong());
}

/// One swatch; `shown` false leaves its place empty (High contrast only hid it). True when
/// clicked.
pub fn cell(ui: &mut Ui, c: Rgb, shown: bool) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(SWATCH, SWATCH), if shown { Sense::click() } else { Sense::hover() });
    if !shown {
        return false;
    }
    let t = Tokens::get(ui.ctx());
    let p = ui.painter();
    p.rect_filled(r, 0.0, c32(c));
    p.rect_stroke(r, 0.0, Stroke::new(0.5, t.border_strong), egui::StrokeKind::Inside);
    let current = crate::widgets::current_pick(ui.ctx()).is_some_and(|cur| cur.eq_ignore_ascii_case(&c.hex()));
    if resp.hovered() || current {
        // Word's hover: a dark frame with a light one inside, readable on any colour.
        p.rect_stroke(r.expand(1.0), 0.0, Stroke::new(2.0, if current { t.accent } else { t.text }), egui::StrokeKind::Outside);
        p.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::WHITE), egui::StrokeKind::Inside);
    }
    if resp.hovered() {
        crate::widgets::note_hover(ui.ctx(), c.hex());
    }
    let hex = format!("#{}", c.hex());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, hex.as_str()));
    resp.on_hover_text(format!("#{}", c.hex())).clicked()
}

/// The High contrast only switch, at the right of the menu's top row.
fn switch_row(ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let on = high_contrast(ui.ctx());
    let label = tl!("High contrast only");
    let (r, _) = ui.allocate_exact_size(vec2(GRID_W, 22.0), Sense::hover());
    let knob = Rect::from_min_size(pos2(r.max.x - 34.0, r.center().y - 8.0), vec2(34.0, 16.0));
    let font = egui::TextStyle::Button.resolve(ui.style());
    let galley = ui.painter().layout_no_wrap(label.to_string(), font, t.text);
    let text_pos = pos2(knob.min.x - 8.0 - galley.size().x, r.center().y - galley.size().y / 2.0);
    let hit = Rect::from_min_max(pos2(text_pos.x, r.min.y), r.max);
    let resp = ui.interact(hit, ui.scope_id().with("strix.colors.highContrastSwitch"), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Switch, true, on, label));
    let p = ui.painter();
    p.galley(text_pos, galley, t.text);
    let radius = knob.height() / 2.0;
    if on {
        p.rect_filled(knob, radius, t.accent);
        p.circle_filled(pos2(knob.max.x - radius, knob.center().y), radius - 3.0, t.on_accent);
    } else {
        p.rect_filled(knob, radius, if resp.hovered() { t.hover } else { t.input });
        p.rect_stroke(knob, radius, Stroke::new(1.0, t.text), egui::StrokeKind::Inside);
        p.circle_filled(pos2(knob.min.x + radius, knob.center().y), radius - 4.0, t.text);
    }
    if resp.clicked() {
        set_high_contrast(ui.ctx(), !on);
    }
}

/// A menu's first choice: its swatch and label across the menu. True when clicked.
fn top_row(ui: &mut Ui, top: &TopItem) -> bool {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(GRID_W, 24.0), Sense::click());
    let current = crate::widgets::current_pick(ui.ctx()).is_some_and(|cur| cur == top.mark);
    if resp.hovered() {
        ui.painter().rect_filled(r, 3.0, t.hover);
        crate::widgets::note_hover(ui.ctx(), top.mark.clone());
    } else if current {
        ui.painter().rect_filled(r, 3.0, t.checked);
    }
    let sw = Rect::from_center_size(pos2(r.min.x + 4.0 + 7.0, r.center().y), vec2(14.0, 14.0));
    match top.swatch {
        TopSwatch::Color(c) => {
            ui.painter().rect_filled(sw, 0.0, c32(c));
        }
        TopSwatch::Empty => {
            ui.painter().rect_filled(sw, 0.0, Color32::WHITE);
            ui.painter().line_segment([sw.left_bottom(), sw.right_top()], Stroke::new(1.0, Color32::from_rgb(0xC0, 0, 0)));
        }
    }
    ui.painter().rect_stroke(sw, 0.0, Stroke::new(0.5, t.border_strong), egui::StrokeKind::Inside);
    let font = egui::TextStyle::Button.resolve(ui.style());
    ui.painter().text(pos2(sw.max.x + 8.0, r.center().y), egui::Align2::LEFT_CENTER, &top.label, font, t.text);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, top.label.as_str()));
    resp.clicked()
}

/// The More Colors… row: a painter's palette and the label.
fn more_item(ui: &mut Ui) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(GRID_W, 24.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, 3.0, t.hover);
    }
    let c = pos2(r.min.x + 11.0, r.center().y);
    let p = ui.painter();
    p.circle_stroke(c, 7.0, Stroke::new(1.2, t.text));
    // The thumb hole, and four paints around it.
    p.circle_filled(c + vec2(2.5, 3.0), 1.6, t.text);
    for (k, col) in [
        (200.0f32, Color32::from_rgb(0xE8, 0x3E, 0x3E)),
        (260.0, Color32::from_rgb(0xF2, 0xB1, 0x1B)),
        (320.0, Color32::from_rgb(0x2E, 0x9E, 0x4F)),
        (20.0, Color32::from_rgb(0x2B, 0x6C, 0xD6)),
    ] {
        let a = k.to_radians();
        p.circle_filled(c + vec2(a.cos(), a.sin()) * 4.0, 1.5, col);
    }
    let label = tl!("More Colors…");
    let font = egui::TextStyle::Button.resolve(ui.style());
    p.text(pos2(r.min.x + 26.0, r.center().y), egui::Align2::LEFT_CENTER, label, font, t.text);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    resp
}

/// Font Color › Gradient: light and dark variations of `base` as gradient text fills
/// (`format.textEffects`), like Word's.
pub fn text_gradient_menu(ui: &mut Ui, app: &mut crate::WordApp, base: Rgb) {
    let mut picked = None;
    for (title, k) in [("Light Variations", 0.6f32), ("Dark Variations", -0.5)] {
        heading(ui, tl!(title));
        row(ui, |ui| {
            for angle in [90, 270, 0, 180] {
                let (a, b) = (tint(base, k), base);
                if gradient_cell(ui, a, b, angle) {
                    picked = Some(json!({"fill": {"gradient": [a.hex(), b.hex()], "angle": angle}}));
                }
            }
        });
    }
    ui.separator();
    if ui.button(tl!("No Gradient")).clicked() {
        picked = Some(json!({"fill": null}));
    }
    if let Some(p) = picked {
        crate::widgets::run_from_ribbon(app, "format.textEffects", p);
        ui.close();
    }
}

/// A gradient tile from `a` to `b` along `angle` (0 left to right, 90 top to bottom).
fn gradient_cell(ui: &mut Ui, a: Rgb, b: Rgb, angle: i32) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::click());
    let t = Tokens::get(ui.ctx());
    let n = 18;
    for k in 0..n {
        let f = k as f32 / (n - 1) as f32;
        let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * f).round().clamp(0.0, 255.0) as u8;
        let col = Color32::from_rgb(mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2));
        let (s0, s1) = (k as f32 / n as f32, (k + 1) as f32 / n as f32 + 0.02);
        let band = match angle {
            90 => Rect::from_min_max(pos2(r.min.x, r.min.y + r.height() * s0), pos2(r.max.x, r.min.y + r.height() * s1)),
            270 => Rect::from_min_max(pos2(r.min.x, r.max.y - r.height() * s1), pos2(r.max.x, r.max.y - r.height() * s0)),
            180 => Rect::from_min_max(pos2(r.max.x - r.width() * s1, r.min.y), pos2(r.max.x - r.width() * s0, r.max.y)),
            _ => Rect::from_min_max(pos2(r.min.x + r.width() * s0, r.min.y), pos2(r.min.x + r.width() * s1, r.max.y)),
        };
        ui.painter().rect_filled(band.intersect(r), 0.0, col);
    }
    let stroke = if resp.hovered() { Stroke::new(2.0, t.accent) } else { Stroke::new(0.5, t.border_strong) };
    ui.painter().rect_stroke(r, 0.0, stroke, egui::StrokeKind::Inside);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tl!("Gradient")));
    resp.on_hover_text(tl!("Gradient")).clicked()
}

// ---------------------------------------------------------------------------------------------
// A colour button for panes and dialogs

/// What [`color_button`] gives back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonPick {
    Color(Rgb),
    /// One of the first choices ([`Options::top`]), by index.
    Top(usize),
}

/// A swatch and `text` that open the colour drop-down; More Colors… opens the Colors dialog and
/// its colour comes back through here a frame later. `salt` must be the same each frame.
pub fn color_button(
    ui: &mut Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    theme: &[Rgb],
    current: Option<Rgb>,
    text: &str,
    opts: &Options,
) -> Option<ButtonPick> {
    let id = ui.scope_id().with(("strix.colorButton", salt));
    let mut out = None;
    let mut hex = String::new();
    crate::colors::take_field(ui.ctx(), id, &mut hex);
    if let Some(c) = crate::colors::parse(&hex) {
        out = Some(ButtonPick::Color(c));
    }
    ui.horizontal(|ui| {
        let t = Tokens::get(ui.ctx());
        let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
        match current {
            Some(c) => {
                ui.painter().rect_filled(r, 2.0, c32(c));
            }
            None => {
                ui.painter().line_segment([r.left_bottom(), r.right_top()], Stroke::new(1.0, t.text_dim));
            }
        }
        ui.painter().rect_stroke(r, 2.0, Stroke::new(1.0, t.border_strong), egui::StrokeKind::Inside);
        ui.menu_button(text, |ui| {
            let mut value = current.map(Rgb::hex).unwrap_or_default();
            if let Some(i) = crate::colors::field_menu_with(ui, theme, id, &mut value, opts) {
                out = Some(ButtonPick::Top(i));
            } else if let Some(c) = crate::colors::parse(&value).filter(|c| Some(*c) != current) {
                out = Some(ButtonPick::Color(c));
            }
        });
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tints_follow_words_steps() {
        assert_eq!(tint_steps(Rgb::WHITE), [-0.05, -0.15, -0.25, -0.35, -0.5]);
        assert_eq!(tint_steps(Rgb::BLACK), [0.5, 0.35, 0.25, 0.15, 0.05]);
        assert_eq!(tint_steps(Rgb(0x15, 0x60, 0x82)), [0.8, 0.6, 0.4, -0.25, -0.5]);
        assert_eq!(tint(Rgb(100, 100, 100), 0.5), Rgb(178, 178, 178));
        assert_eq!(tint(Rgb(100, 100, 100), -0.5), Rgb(50, 50, 50));
        assert_eq!(tint(Rgb(1, 2, 3), f32::NAN), Rgb(1, 2, 3), "junk changes nothing");
    }

    #[test]
    fn high_contrast_keeps_readable_colours() {
        assert!((contrast(Rgb::BLACK, Rgb::WHITE) - 21.0).abs() < 0.01);
        assert!(readable(Rgb::BLACK, false) && !readable(Rgb::WHITE, false));
        assert!(readable(Rgb::WHITE, true) && !readable(Rgb::BLACK, true));
        assert!(!readable(Rgb(0xFF, 0xFF, 0x00), false), "yellow text on white is hard to read");
    }

    #[test]
    fn a_short_theme_still_fills_the_row() {
        let row = theme_row(&[Rgb::BLACK]);
        assert_eq!(row[1], Rgb::BLACK);
        assert_eq!(row[0], Rgb(128, 128, 128));
    }
}
