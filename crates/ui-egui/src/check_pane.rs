//! The Assessment Check task pane (`Teaching › Check`, `view.checkPane`): runs `exercise.check`
//! across every variant (`A..Z`) and lists errors, warnings and info items with clickable links
//! to the exact position in the document, plus a shortcut to export the Assessment Pack.

use egui::{Color32, RichText, Stroke, Ui};
use serde_json::{Value, json};

use crate::WordApp;
use crate::theme::Tokens;

pub fn pane(app: &mut WordApp, ui: &mut Ui) {
    let report = app.session.run("exercise.check", &json!({})).unwrap_or_default();
    let errs = report["errors"].as_u64().unwrap_or(0);
    let warns = report["warnings"].as_u64().unwrap_or(0);
    let infos = report["infos"].as_u64().unwrap_or(0);
    let items = report["issues"].as_array().cloned().unwrap_or_default();

    ui.horizontal_wrapped(|ui| {
        if ui
            .button(tl!("Export Assessment Pack…"))
            .on_hover_text(tl!("Export student PDFs for every variant, DSA copies and the teacher key with synoptic table"))
            .clicked()
        {
            let _ = app.run("file.exportPack", json!({}));
        }
    });
    ui.add_space(6.0);

    let t = Tokens::get(ui.ctx());
    if items.is_empty() {
        let green = Color32::from_rgb(0x2E, 0x8B, 0x57);
        egui::Frame::NONE
            .fill(green.linear_multiply(0.10))
            .stroke(Stroke::new(1.0, green.linear_multiply(0.45)))
            .corner_radius(6)
            .inner_margin(8)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(tl!("All checks passed across every variant.")).color(green).strong());
            });
        return;
    }

    ui.horizontal_wrapped(|ui| {
        badge(ui, &format!("{errs} {}", tl!("Errors")), Color32::from_rgb(0xC4, 0x3E, 0x1C), errs > 0);
        badge(ui, &format!("{warns} {}", tl!("Warnings")), Color32::from_rgb(0xC8, 0x7A, 0x12), warns > 0);
        badge(ui, &format!("{infos} {}", tl!("Info")), t.accent, infos > 0);
    });
    ui.add_space(6.0);
    ui.separator();
    ui.add_space(4.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        for item in &items {
            render_item(app, ui, item, &t);
            ui.add_space(6.0);
        }
    });
}

fn badge(ui: &mut Ui, text: &str, color: Color32, active: bool) {
    let fg = if active { color } else { ui.visuals().weak_text_color() };
    let bg = if active { color.linear_multiply(0.14) } else { ui.visuals().faint_bg_color };
    egui::Frame::NONE.fill(bg).corner_radius(4).inner_margin(egui::Margin::symmetric(6, 2)).show(ui, |ui| {
        ui.label(RichText::new(text).small().color(fg).strong());
    });
}

fn render_item(app: &mut WordApp, ui: &mut Ui, item: &Value, t: &Tokens) {
    let sev = item["kind"].as_str().unwrap_or("info");
    let (sev_label, sev_col) = match sev {
        "error" => (tl!("Error"), Color32::from_rgb(0xC4, 0x3E, 0x1C)),
        "warning" => (tl!("Warning"), Color32::from_rgb(0xC8, 0x7A, 0x12)),
        _ => (tl!("Info"), t.accent),
    };
    let msg = item["message"].as_str().unwrap_or_default();
    let variant = item["variant"].as_str();
    let ex = item["exercise"].as_u64();

    egui::Frame::NONE.fill(t.input).stroke(Stroke::new(1.0, t.border)).corner_radius(6).inner_margin(8).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(sev_label).small().color(sev_col).strong());
            if let Some(ex) = ex {
                ui.label(RichText::new(format!("{} {ex}", tl!("Exercise"))).small().weak());
            }
            if let Some(var) = variant {
                ui.label(RichText::new(format!("[{var}]")).small().monospace());
            }
            if let Some(pos) = item.get("pos").filter(|p| p.is_object()) {
                ui.with_layout(crate::rtl::far_side(ui, egui::Align::Center), |ui| {
                    if ui.small_button(tl!("Go to")).clicked() {
                        let _ = app.run("select.range", json!({"anchor": pos, "focus": pos}));
                        app.canvas.want_focus = true;
                    }
                });
            }
        });
        ui.add_space(2.0);
        ui.label(RichText::new(msg).small());
    });
}
