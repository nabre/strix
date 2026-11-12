//! The Questions section of the Exercise Properties pane for an open-answer or multiple-choice
//! exercise (docs/exercise.md, Kinds of exercise): each question's points, its answer space
//! (height in cm, squared or blank) or its correct options, through `exercise.questions`.

use egui::Ui;
use serde_json::{Value, json};

use crate::WordApp;
use crate::a11y_pane::draft_box;

/// A number typed in a box (`2`, `1,5`); `None` for anything else.
fn number(t: &str) -> Option<f64> {
    t.trim().replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite())
}

/// `v` written back for a box: whole numbers plain, else up to two decimals.
fn shown(v: f64) -> String {
    let t = format!("{v:.2}");
    t.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// The questions of quiz `ex` (1-based) with their settings; `st` is `exercise.questions`' reply.
pub(crate) fn questions(app: &mut WordApp, ui: &mut Ui, ex: u64, st: &Value) {
    let list = st["questions"].as_array().cloned().unwrap_or_default();
    let set = |app: &mut WordApp, q: usize, params: Value| {
        let mut p = json!({"exercise": ex, "question": q + 1});
        if let (Some(o), Some(extra)) = (p.as_object_mut(), params.as_object()) {
            for (k, v) in extra {
                o.insert(k.clone(), v.clone());
            }
        }
        let _ = app.run("exercise.questions", p);
    };
    for (k, q) in list.iter().enumerate() {
        let label = q["label"].as_str().unwrap_or_default();
        let text: String = q["text"].as_str().unwrap_or_default().chars().take(60).collect();
        let head = if text.is_empty() { tl!("Question").to_string() } else { text };
        ui.add_space(4.0);
        ui.label(egui::RichText::new(format!("{label} {head}").trim().to_string()).strong());
        crate::rtl::grid(ui, egui::Grid::new(("exercise_quiz", ex, k)).num_columns(2).spacing([6.0, 4.0]), |ui| {
            ui.label(tl!("Points"));
            let current = q["points"].as_f64().map(shown).unwrap_or_default();
            ui.scope(|ui| {
                ui.set_width(60.0);
                if let Some(t) = draft_box(ui, ("exercise_quiz_points", ex, k), &current, true, false) {
                    let v = if t.trim().is_empty() { Value::Null } else { number(&t).map_or(Value::Null, |n| json!(n)) };
                    set(app, k, json!({"points": v}));
                }
            });
            ui.end_row();
            if q["type"] == "choice" {
                ui.label(tl!("Correct"));
                let correct: Vec<String> =
                    q["correct"].as_array().map(|a| a.iter().filter_map(|c| c.as_str().map(str::to_string)).collect()).unwrap_or_default();
                let n = q["choices"].as_array().map_or(0, Vec::len);
                ui.horizontal_wrapped(|ui| {
                    for j in 0..n.min(26) {
                        let letter = char::from(b'A' + j as u8).to_string();
                        let on = correct.contains(&letter);
                        if ui.selectable_label(on, letter.as_str()).on_hover_text(tl!("Mark this option correct (more than one can be)")).clicked() {
                            let mut want = correct.clone();
                            if on {
                                want.retain(|c| *c != letter);
                            } else {
                                want.push(letter);
                            }
                            set(app, k, json!({"correct": want}));
                        }
                    }
                });
                ui.end_row();
            } else {
                ui.label(tl!("Space (cm)"));
                let current = q["height"].as_f64().map(shown).unwrap_or_default();
                ui.scope(|ui| {
                    ui.set_width(60.0);
                    if let Some(t) = draft_box(ui, ("exercise_quiz_height", ex, k), &current, true, false)
                        && let Some(h) = number(&t)
                    {
                        set(app, k, json!({"height": h}));
                    }
                });
                ui.end_row();
                ui.label("");
                let mut grid = q["grid"].as_bool().unwrap_or(true);
                if ui.checkbox(&mut grid, tl!("Squared")).on_hover_text(tl!("Graph paper in the answer space; off, the space is blank")).changed() {
                    set(app, k, json!({"grid": grid}));
                }
                ui.end_row();
                if grid {
                    ui.label(tl!("Square (mm)"));
                    let current = q["spacing"].as_f64().map(shown).unwrap_or_default();
                    ui.scope(|ui| {
                        ui.set_width(60.0);
                        if let Some(t) = draft_box(ui, ("exercise_quiz_spacing", ex, k), &current, true, false)
                            && let Some(v) = number(&t)
                        {
                            set(app, k, json!({"spacing": v}));
                        }
                    });
                    ui.end_row();
                }
            }
        });
    }
    ui.add_space(6.0);
    if let Some(total) = st["total"].as_f64() {
        ui.label(format!("{}: {}", tl!("Total"), shown(total)));
    }
    if ui.button(tl!("Add Question")).on_hover_text(tl!("Add a question like the last one")).clicked() {
        let _ = app.run("exercise.addQuestion", json!({}));
    }
    ui.add_space(6.0);
    crate::pane_kit::note(
        ui,
        tl!(
            "The answers show or hide with the Solution (Teaching › Exercise › Show): hidden, an answer space keeps its height and every box is empty."
        ),
    );
}
