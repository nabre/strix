//! Strix's own drawing on the page while an equation is edited: the calculation hooks (leaving a
//! calculated equation recalculates; its Calculate button) and the selection inside an equation.
//! Kept apart from the upstream canvas so upstream merges don't touch it (docs/upstream.md).

use egui::{Color32, Painter, Rect, Stroke, pos2};
use serde_json::json;
use wordcraft_layout::math::MathLayout;

use crate::WordApp;
use crate::theme::{Tokens, regular};

/// Whether the equation being edited is a calculation region. Leaving one (the caret left the
/// equation since the last frame) recalculates the document.
pub(super) fn track_calc_equation(app: &mut WordApp) -> bool {
    let in_calc = wordcraft_engine::cmd::equation::edited_math(&app.session).is_some_and(|m| m.calc);
    if std::mem::replace(&mut app.canvas.in_calc_equation, in_calc) && app.session.math.is_none() {
        let _ = app.run("calc.recalculate", json!({}));
    }
    in_calc
}

/// Where an equation is drawn on its page: the page's rectangle, the equation's left edge and
/// baseline in page points, and the zoom.
pub(super) struct EquationAt {
    pub page: Rect,
    pub x: f32,
    pub base: f32,
    pub scale: f32,
}

/// The selection inside the edited equation (Shift with the arrows): from its start to the
/// caret, as tall as the box it is in.
pub(super) fn paint_math_selection(app: &WordApp, painter: &Painter, ml: &MathLayout, at: &EquationAt, color: Color32) {
    let EquationAt { page: pr, x: ex, base, scale: s } = *at;
    if let Some((path, lo, hi)) = wordcraft_engine::cmd::equation::math_selection(&app.session)
        && let (Some(a), Some(b)) = (ml.slot(&path, lo), ml.slot(&path, hi))
    {
        let (up, down) = ml.arg_extent(&path).map_or((ml.ascent, ml.descent), |(_, _, up, down)| (up, down));
        let r = Rect::from_min_max(
            pos2(pr.min.x + (ex + a.x.min(b.x)) * s, pr.min.y + (base - up) * s),
            pos2(pr.min.x + (ex + a.x.max(b.x)) * s, pr.min.y + (base + down) * s),
        );
        painter.rect_filled(r, 0.0, color);
    }
}

/// The Calculate button beside the equation frame: filled while the equation is a calculation
/// region.
pub(super) fn paint_calc_button(painter: &Painter, button: Rect, in_calc: bool, t: &Tokens) {
    if in_calc {
        painter.rect_filled(button, 2.0, t.accent);
    } else {
        painter.rect_filled(button, 2.0, Color32::WHITE);
        painter.rect_stroke(button, 2.0, Stroke::new(1.0, t.equation_box), egui::StrokeKind::Inside);
    }
    let ink = if in_calc { t.on_accent } else { t.equation_box };
    painter.text(button.center(), egui::Align2::CENTER_CENTER, "=", regular(12.0), ink);
}

/// Right-click on an exercise's inline value (`{{R}}` in its text): the expression it shows
/// (Enter applies), how it is shown, and Delete Value (docs/exercise.md, Inline values).
pub(super) fn inline_value_menu(app: &mut WordApp, ui: &mut egui::Ui) {
    let Some(info) = wordcraft_engine::cmd::exercise::inline_value_at(&app.session) else { return };
    ui.separator();
    ui.label(egui::RichText::new(tl!("Inline Value")).small().weak());
    let expr = info["expr"].as_str().unwrap_or_default().to_string();
    // The draft is kept while the menu is open, for the value it was started on.
    let id = ui.scope_id().with("inline_value_expr");
    let mut draft = ui.data(|d| d.get_temp::<(String, String)>(id)).filter(|(of, _)| *of == expr).map_or_else(|| expr.clone(), |(_, d)| d);
    let edit = ui.add(egui::TextEdit::singleline(&mut draft).desired_width(200.0).hint_text(tl!("Expression")));
    let edit = edit.on_hover_text(tl!("What the value shows: a datum (R), a calculation (2*R) or a conversion (R -> cm). Enter applies it."));
    ui.data_mut(|d| d.insert_temp(id, (expr.clone(), draft.clone())));
    let wanted = draft.trim().to_string();
    if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !wanted.is_empty() && wanted != expr {
        if let Err(e) = app.run("exercise.inlineValue", json!({"expr": wanted})) {
            app.status(e);
        }
        ui.close();
    }
    if !info["blank"].as_bool().unwrap_or(false) {
        let show = info["show"].as_str().unwrap_or("value");
        for (mode, label) in [("value", "Value and Unit"), ("both", "Symbol = Value"), ("number", "Number Only")] {
            if ui.selectable_label(show == mode, tl!(label)).clicked() {
                if mode != show
                    && let Err(e) = app.run("exercise.inlineValue", json!({"show": mode}))
                {
                    app.status(e);
                }
                ui.close();
            }
        }
    }
    if ui.button(tl!("Delete Value")).clicked() {
        if let Err(e) = app.run("exercise.inlineValue", json!({"remove": true})) {
            app.status(e);
        }
        ui.close();
    }
}
