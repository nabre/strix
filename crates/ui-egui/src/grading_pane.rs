//! The Grading Scale pane (Teaching › Scores › Grading Scale › Properties, docs/exercise.md,
//! Grading scale): every property of the points-to-grade table — the country preset, the maximum
//! points (from the scores or fixed), the lowest, highest and pass grades, the pass threshold,
//! the curve, the rounding of grades and points, who sees it — with the scale it gives. Every
//! change is one `exercise.grading` (one undo step); numbers dragged apply when the drag ends.

use egui::{RichText, Ui};
use serde_json::{Value, json};

use crate::WordApp;
use crate::graph_pane::num_field;

/// The presets, as `exercise.grading` names them, with their labels.
const PRESETS: [(&str, &str); 6] = [
    ("it-10", "Italy (1–10)"),
    ("ch-6", "Switzerland (1–6)"),
    ("de-6", "Germany (6–1)"),
    ("fr-20", "France (0–20)"),
    ("percent", "Percentage (0–100%)"),
    ("custom", "Custom"),
];

const CURVES: [(&str, &str); 2] = [("linear", "Linear"), ("piecewiseLinear", "Piecewise (pass threshold)")];

const STEPS: [f64; 4] = [0.1, 0.25, 0.5, 1.0];

const VISIBILITY: [(&str, &str); 2] = [("teacher", "Teacher copy only"), ("both", "Student and teacher copies")];

fn set(app: &mut WordApp, params: Value) {
    if let Err(e) = app.run("exercise.grading", params) {
        app.status(e);
    }
}

/// A number shown plainly: whole numbers without decimals, else up to two.
fn shown(v: f64) -> String {
    let t = format!("{v:.2}");
    t.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// A combo of `(id, label)` choices; the id picked when it changes.
fn choice(ui: &mut Ui, salt: &str, current: &str, items: &[(&str, &str)]) -> Option<String> {
    let mut picked = None;
    let shown = items.iter().find(|(id, _)| *id == current).map_or_else(|| current.to_string(), |(_, l)| tl!(l).to_string());
    craft_ui::ComboBox::from_id_salt(salt).selected_text(shown).width(ui.available_width().min(220.0)).show_ui(ui, |ui| {
        for (id, label) in items {
            if ui.selectable_label(*id == current, tl!(label)).clicked() && *id != current {
                picked = Some((*id).to_string());
            }
        }
    });
    picked
}

pub fn pane(app: &mut WordApp, ui: &mut Ui) {
    let st = app.session.run("exercise.grading", &json!({})).ok().unwrap_or_default();
    egui::ScrollArea::vertical().show(ui, |ui| {
        if !st["on"].as_bool().unwrap_or(false) {
            crate::pane_kit::note(ui, tl!("The document has no grading scale. Insert one: it converts the points obtained into a grade."));
            ui.add_space(6.0);
            if ui.button(tl!("Insert Grading Scale")).clicked() {
                set(app, json!({"on": true}));
            }
            return;
        }
        properties(app, ui, &st);
        preview(ui, &st);
        ui.add_space(10.0);
        if ui
            .button(tl!("Remove Grading Scale"))
            .on_hover_text(tl!("Take the grading scale table out of the document (Undo brings it back)"))
            .clicked()
        {
            set(app, json!({"on": false}));
        }
    });
}

fn properties(app: &mut WordApp, ui: &mut Ui, st: &Value) {
    let num = |k: &str, d: f64| st[k].as_f64().unwrap_or(d);
    crate::pane_kit::section(ui, "01", tl!("Scale"));
    crate::rtl::grid(ui, egui::Grid::new("grading_scale_grid").num_columns(2).spacing([8.0, 6.0]), |ui| {
        ui.label(tl!("Preset"));
        if let Some(id) = choice(ui, "grading_preset", st["preset"].as_str().unwrap_or("it-10"), &PRESETS) {
            set(app, json!({"preset": id}));
        }
        ui.end_row();
        ui.label(tl!("Lowest grade"));
        if let Some(v) = num_field(ui, "grading_min", num("minGrade", 1.0), -1000.0..=1000.0, "", 2) {
            set(app, json!({"minGrade": v}));
        }
        ui.end_row();
        ui.label(tl!("Highest grade"));
        if let Some(v) = num_field(ui, "grading_max", num("maxGrade", 10.0), -1000.0..=1000.0, "", 2) {
            set(app, json!({"maxGrade": v}));
        }
        ui.end_row();
        ui.label(tl!("Grades rounded to"));
        let step = num("step", 0.5);
        let label = |v: f64| shown(v);
        craft_ui::ComboBox::from_id_salt("grading_step").selected_text(label(step)).show_ui(ui, |ui| {
            for v in STEPS {
                if ui.selectable_label((v - step).abs() < 1e-6, label(v)).clicked() && (v - step).abs() > 1e-6 {
                    set(app, json!({"step": v}));
                }
            }
        });
        ui.end_row();
    });

    crate::pane_kit::section(ui, "02", tl!("Pass Mark"));
    crate::rtl::grid(ui, egui::Grid::new("grading_pass_grid").num_columns(2).spacing([8.0, 6.0]), |ui| {
        ui.label(tl!("Curve"));
        if let Some(id) = choice(ui, "grading_curve", st["curve"].as_str().unwrap_or("linear"), &CURVES) {
            set(app, json!({"curve": id}));
        }
        ui.end_row();
        ui.label(tl!("Pass threshold"));
        if let Some(v) = num_field(ui, "grading_ratio", num("passRatio", 0.6) * 100.0, 5.0..=95.0, " %", 0) {
            set(app, json!({"passRatio": v / 100.0}));
        }
        ui.end_row();
        ui.label(tl!("Pass grade"));
        if let Some(v) = num_field(ui, "grading_pass", num("passGrade", 6.0), -1000.0..=1000.0, "", 2) {
            set(app, json!({"passGrade": v}));
        }
        ui.end_row();
    });
    crate::pane_kit::note(
        ui,
        tl!(
            "Piecewise: the pass threshold of the points gives the pass grade, and the grades rise evenly below and above it. Linear: the grade follows the points evenly."
        ),
    );

    crate::pane_kit::section(ui, "03", tl!("Points"));
    let auto = st["maxPoints"].is_null();
    let effective = num("effectiveMaxPoints", 10.0);
    let mut fixed = !auto;
    if ui
        .radio_value(&mut fixed, false, crate::i18n::fmt(tl!("From the document's points ({points})"), &[("points", &shown(effective))]))
        .on_hover_text(tl!("The scores table's grand total, else the questions' points added up"))
        .clicked()
        && !auto
    {
        set(app, json!({"maxPoints": null}));
    }
    ui.horizontal(|ui| {
        if ui.radio_value(&mut fixed, true, tl!("Fixed maximum")).clicked() && auto {
            set(app, json!({"maxPoints": effective}));
        }
        ui.add_enabled_ui(!auto, |ui| {
            if let Some(v) = num_field(ui, "grading_points", effective, 0.5..=10_000.0, " pt", 1) {
                set(app, json!({"maxPoints": v}));
            }
        });
    });
    ui.add_space(4.0);
    let points_step = st["pointsStep"].as_f64();
    let mut every = points_step.is_some();
    ui.horizontal(|ui| {
        if ui.checkbox(&mut every, tl!("A row every")).on_hover_text(tl!("Off: the table chooses the step so it stays short")).changed() {
            set(app, json!({"pointsStep": if every { json!(1.0) } else { Value::Null }}));
        }
        ui.add_enabled_ui(points_step.is_some(), |ui| {
            if let Some(v) = num_field(ui, "grading_points_step", points_step.unwrap_or(1.0), 0.1..=1000.0, " pt", 1) {
                set(app, json!({"pointsStep": v}));
            }
        });
    });

    crate::pane_kit::section(ui, "04", tl!("Shown In"));
    if let Some(id) = choice(ui, "grading_visibility", st["visibility"].as_str().unwrap_or("teacher"), &VISIBILITY) {
        set(app, json!({"visibility": id}));
    }
    if st["hidden"].as_bool().unwrap_or(false) {
        crate::pane_kit::note(ui, tl!("Hidden now: the solutions are hidden (student copy)."));
    }
}

/// The scale the settings give: points and grade, the pass mark marked.
fn preview(ui: &mut Ui, st: &Value) {
    let rows = st["scale"].as_array().cloned().unwrap_or_default();
    if rows.is_empty() {
        return;
    }
    crate::pane_kit::section(ui, "05", tl!("Preview"));
    let t = crate::theme::Tokens::get(ui.ctx());
    egui::Frame::new().stroke(egui::Stroke::new(1.0, t.border)).corner_radius(egui::CornerRadius::same(4)).inner_margin(6).show(ui, |ui| {
        ui.set_width(ui.available_width());
        // Two pairs of columns side by side when the scale is long.
        let pairs = if rows.len() > 12 { 2 } else { 1 };
        let per = rows.len().div_ceil(pairs);
        crate::rtl::grid(ui, egui::Grid::new("grading_preview").num_columns(pairs * 2).spacing([14.0, 2.0]).striped(true), |ui| {
            for _ in 0..pairs {
                ui.label(RichText::new(tl!("Points")).small().color(t.text_dim));
                ui.label(RichText::new(tl!("Grade")).small().color(t.text_dim));
            }
            ui.end_row();
            for r in 0..per {
                for c in 0..pairs {
                    match rows.get(c * per + r) {
                        Some(row) => {
                            let pass = row["pass"].as_bool().unwrap_or(false);
                            let pts = shown(row["points"].as_f64().unwrap_or(0.0));
                            let grade = shown(row["grade"].as_f64().unwrap_or(0.0));
                            ui.label(RichText::new(pts).monospace());
                            let g = RichText::new(grade).monospace();
                            ui.label(if pass { g.color(t.green) } else { g.color(t.red) });
                        }
                        None => {
                            ui.label("");
                            ui.label("");
                        }
                    }
                }
                ui.end_row();
            }
        });
    });
}
