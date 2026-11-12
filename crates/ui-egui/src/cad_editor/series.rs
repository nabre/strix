//! The rectangular array (Serie) in the editor: a base point and an end point, spaced to fit
//! the end point, at an exact spacing or spread within one ([`Spacing`]); the items made one
//! object (docs/cad.md).

use egui::Ui;
use serde_json::json;
use strix_cad::{Drawing, Pt, Spacing};

use super::{CadEditor, Tool};
use crate::WordApp;

/// The ways to space a series as the menu and the command line name them: the spacing, its
/// label, the letter typed for it.
pub(super) const SPACINGS: [(Spacing, &str, &str); 3] = [
    (Spacing::Fit, "Fit to the end point (the items typed, the last on the end point)", "F"),
    (Spacing::Pitch, "Exact spacing (as many as fit, the rest left over)", "I"),
    (Spacing::Spread, "Spread evenly (within the spacing, the last on the end point)", "S"),
];

/// A length of the drawing (drawing units) in its input unit, with the unit: `25 cm`.
fn length(ed: &CadEditor, v: f64) -> String {
    let u = strix_cad::Units::of(&ed.drawing);
    let k = u.drawing.mm() / u.input.mm();
    let x = (v * k * 1000.0).round() / 1000.0;
    format!("{x} {}", u.input.symbol())
}

/// What the command line asks for while a rectangular array runs.
pub(super) fn prompt(ed: &CadEditor) -> String {
    let pitch = if ed.array_pitch > 0.0 { length(ed, ed.array_pitch) } else { tl!("not set").to_string() };
    let way = match ed.array_spacing {
        Spacing::Fit => format!("{} {} × {}", tl!("fit to the end point,"), ed.array_cols, ed.array_rows),
        Spacing::Pitch => format!("{} {pitch}", tl!("exact spacing")),
        Spacing::Spread => format!("{} {pitch}", tl!("spread within")),
    };
    if ed.points.is_empty() {
        format!(
            "{} ({way}) · {}",
            tl!("Array: base point"),
            tl!("F fit · I exact spacing · S spread · a number: the spacing · 4x3: columns x rows (1xN: along a line)")
        )
    } else if ed.array_cols == 1 || ed.array_rows == 1 {
        format!("{} ({way})", tl!("Array: end point (the items run along the line to it)"))
    } else {
        format!("{} ({way})", tl!("Array: end point (the far corner of the grid)"))
    }
}

/// `F`, `I` or `S` (or `fit`, `pitch`, `spread`) typed at the array's base point: how it is
/// spaced. True when it was one.
pub(super) fn typed_option(ed: &mut CadEditor, typed: &str) -> bool {
    if ed.tool != Tool::ArrayRect || !ed.points.is_empty() || ed.picking {
        return false;
    }
    let found = SPACINGS.iter().find(|(s, _, letter)| typed.eq_ignore_ascii_case(letter) || typed.eq_ignore_ascii_case(s.name())).map(|(s, _, _)| *s);
    let Some(spacing) = found else { return false };
    set_spacing(ed, spacing);
    true
}

fn set_spacing(ed: &mut CadEditor, spacing: Spacing) {
    ed.array_spacing = spacing;
    ed.message.clear();
    if spacing != Spacing::Fit && ed.array_pitch <= 0.0 {
        ed.message = tl!("Type the spacing (a length), then the base point").to_string();
    }
}

/// A length typed at the array's base point: the spacing (and Fit becomes Exact spacing, as
/// giving one means to keep it). True when it was one.
pub(super) fn typed_pitch(ed: &mut CadEditor, typed: &str) -> bool {
    if ed.tool != Tool::ArrayRect || !ed.points.is_empty() || ed.picking {
        return false;
    }
    let Some(v) = strix_cad::parse_length(typed, strix_cad::Units::of(&ed.drawing)) else { return false };
    if !(v.is_finite() && v > 0.0 && v <= strix_cad::MAX_COORD) {
        ed.message = tl!("A spacing is more than 0").to_string();
        return true;
    }
    ed.array_pitch = v;
    if ed.array_spacing == Spacing::Fit {
        ed.array_spacing = Spacing::Pitch;
    }
    ed.message.clear();
    true
}

/// The series of the selection from `from` to `to`, as the editor's settings say; the message
/// says what it came to.
pub(super) fn make(ed: &mut CadEditor, from: Pt, to: Pt) -> Option<Drawing> {
    let (cols, rows, spacing, pitch, sel) = (ed.array_cols, ed.array_rows, ed.array_spacing, ed.array_pitch, ed.selection.clone());
    let mut d = ed.drawing.clone();
    match d.array_series(&sel, from, to, (cols, rows), spacing, pitch) {
        Ok((_, s)) => {
            ed.selection.clear();
            let steps = if s.rows > 1 {
                format!("{} × {} · {} {} × {}", s.cols, s.rows, tl!("spacing"), length(ed, s.col_step.len()), length(ed, s.row_step.len()))
            } else {
                format!("{} {} · {} {}", s.cols, tl!("items"), tl!("spacing"), length(ed, s.col_step.len()))
            };
            ed.message = format!("{} {steps}", tl!("Array made, one object:"));
            Some(d)
        }
        Err(e) => {
            ed.message = tl!(&e).to_string();
            None
        }
    }
}

/// `ui.cadArrayMode`: `spacing` fit, pitch or spread; `pitch` the spacing (drawing units).
pub(super) fn command(app: &mut WordApp, p: &serde_json::Value) -> Result<serde_json::Value, String> {
    let ed = app.cad.as_mut().ok_or("no CAD drawing is open")?;
    if let Some(s) = p.get("spacing") {
        let spacing = s.as_str().and_then(Spacing::parse).ok_or("spacing: fit, pitch, spread")?;
        set_spacing(ed, spacing);
    }
    if let Some(v) = p.get("pitch") {
        let v = v.as_f64().filter(|v| v.is_finite() && *v > 0.0 && *v <= strix_cad::MAX_COORD).ok_or("pitch: a length more than 0")?;
        ed.array_pitch = v;
        ed.message.clear();
    }
    Ok(json!({"spacing": ed.array_spacing.name(), "pitch": ed.array_pitch}))
}

/// The array button's menu: how the series is spaced.
pub(super) fn menu_items(ui: &mut Ui, app: &mut WordApp) {
    let now = app.cad.as_ref().map_or(app.cad_settings.array_spacing, |e| e.array_spacing);
    ui.separator();
    ui.label(tl!("Spacing"));
    for (spacing, label, _) in SPACINGS {
        if ui.selectable_label(now == spacing, tl!(label)).clicked() {
            let _ = app.run("ui.cadArrayMode", json!({"spacing": spacing.name()}));
            ui.close();
        }
    }
}
