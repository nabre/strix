//! Strix's own drawing on the page while an equation is edited: the calculation hooks (leaving a
//! calculated equation recalculates; its Calculate button) and the selection inside an equation.
//! Kept apart from the upstream canvas so upstream merges don't touch it (docs/upstream.md).

use egui::{Color32, Painter, Rect, Stroke, pos2};
use serde_json::json;
use strix_layout::math::MathLayout;

use crate::WordApp;
use crate::theme::{Tokens, regular};

/// Whether the equation being edited is a calculation region. Leaving one (the caret left the
/// equation since the last frame) recalculates the document.
pub(super) fn track_calc_equation(app: &mut WordApp) -> bool {
    let in_calc = strix_engine::cmd::equation::edited_math(&app.session).is_some_and(|m| m.calc);
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
    if let Some((path, lo, hi)) = strix_engine::cmd::equation::math_selection(&app.session)
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
