//! The bar under the ribbon for Strix objects changed in another program (docs/word-round-trip.md):
//! a Word document Strix wrote came back with calculations, tables or drawings whose content was
//! changed there. They stay static until the person picks **Recalculate from Strix** (live again,
//! the change replaced) or **Keep Changes** (the change stays, the Strix source goes), through
//! `file.externalEdits`. Closing the bar leaves them waiting, for that document, until it opens
//! again.

use egui::{RichText, Ui};
use serde_json::json;

use crate::WordApp;
use crate::theme::Tokens;

fn dismissed_id(document: u64) -> egui::Id {
    egui::Id::unique(("strix_external_edits_dismissed", document))
}

pub(crate) fn bar(app: &mut WordApp, ui: &mut Ui) {
    if !app.session.doc.passthrough.contains_key(wordcraft_engine::word_envelope::PENDING) {
        return;
    }
    let n = wordcraft_engine::word_envelope::pending(&app.session.doc).len();
    let document = app.session.document_id();
    if n == 0 || ui.ctx().data(|d| d.get_temp::<bool>(dismissed_id(document))).unwrap_or(false) {
        return;
    }
    let t = Tokens::get(ui.ctx());
    egui::Panel::top("strix_external_edits_bar")
        .exact_size(36.0)
        .show_separator_line(false)
        .frame(egui::Frame::NONE.fill(t.panel).inner_margin(egui::Margin::symmetric(12, 0)))
        .show(ui, |ui| {
            let full = ui.max_rect();
            ui.painter().hline(full.x_range(), full.bottom() - 0.5, egui::Stroke::new(1.0, t.border));
            ui.horizontal_centered(|ui| {
                let text = crate::i18n::tn(
                    n as u64,
                    "{n} Strix object was changed in another program and is static for now.",
                    "{n} Strix objects were changed in another program and are static for now.",
                );
                ui.add(egui::Label::new(RichText::new(text).color(t.text)).truncate());
                let recalc = ui
                    .add(crate::widgets::primary_button(ui, tl!("Recalculate from Strix").to_string()))
                    .on_hover_text(tl!("Make them live again from their Strix source: the changes made in the other program are replaced"));
                if recalc.clicked() {
                    let _ = app.run("file.externalEdits", json!({"action": "recalculate"}));
                }
                let keep = ui
                    .button(tl!("Keep Changes"))
                    .on_hover_text(tl!("Keep what the other program shows: they stay static and lose their Strix source"));
                if keep.clicked() {
                    let _ = app.run("file.externalEdits", json!({"action": "keep"}));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if crate::widgets::icon_button(ui, "close", tl!("Decide Later")).clicked() {
                        ui.ctx().data_mut(|d| d.insert_temp(dismissed_id(document), true));
                    }
                });
            });
        });
}
