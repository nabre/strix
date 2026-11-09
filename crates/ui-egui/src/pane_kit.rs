//! Building blocks for task panes in Strix's sober, technical look (the Atollo palette in
//! `theme::Tokens`): section kickers in small monospace capitals over a hairline rule, cards with a
//! one-pixel border, status badges and dots. Only colours from the tokens.

use egui::{Color32, CornerRadius, FontFamily, FontId, Frame, Margin, RichText, Stroke, Ui};

use crate::theme::Tokens;

/// A section heading: an index (`01`) in the accent colour and the title in small monospace
/// capitals, over a hairline rule.
pub fn section(ui: &mut Ui, index: &str, title: &str) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        if !index.is_empty() {
            ui.label(RichText::new(index).font(mono(10.5)).color(t.accent_text));
        }
        ui.label(RichText::new(title.to_uppercase()).font(mono(10.5)).color(t.text_dim));
    });
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, Stroke::new(1.0, t.border));
    ui.add_space(6.0);
}

/// A card: the widget surface with a one-pixel border and small corners.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let t = Tokens::get(ui.ctx());
    Frame::new()
        .fill(t.menu)
        .stroke(Stroke::new(1.0, t.border))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// The kinds of state a badge shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Ok,
    Changed,
    Warning,
    Error,
}

impl Status {
    pub fn color(self, t: &Tokens) -> Color32 {
        match self {
            Status::Ok => t.green,
            Status::Changed => t.accent_text,
            Status::Warning => t.orange,
            Status::Error => t.red,
        }
    }
}

/// A small pill: the status colour as a thin outline and text, on the panel.
pub fn badge(ui: &mut Ui, text: &str, status: Status) {
    let t = Tokens::get(ui.ctx());
    let c = status.color(&t);
    Frame::new().stroke(Stroke::new(1.0, c)).corner_radius(CornerRadius::same(8)).inner_margin(Margin::symmetric(6, 1)).show(ui, |ui| {
        ui.label(RichText::new(text).font(mono(10.0)).color(c));
    });
}

/// Secondary text: dim and small.
pub fn note(ui: &mut Ui, text: &str) {
    let t = Tokens::get(ui.ctx());
    ui.label(RichText::new(text).size(11.0).color(t.text_dim));
}

/// The monospace interface face.
pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}
