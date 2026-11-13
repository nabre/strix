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

/// Question modes offered in the per-question dropdown (`exercise.questionMode`).
const QUESTION_MODES: [(&str, &str); 9] = [
    ("open", "Open Answer"),
    ("choice", "Multiple Choice"),
    ("trueFalse", "True or False"),
    ("calc", "Calculation"),
    ("drawing", "CAD Drawing Question"),
    ("worked", "Worked Example"),
    ("cloze", "Inline Completion"),
    ("tableFill", "Table to Complete"),
    ("matching", "Matching"),
];

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
        let q_type = q["type"].as_str().unwrap_or("open");
        let cur_mode = if q_type == "choice" && q["trueFalse"].as_bool() == Some(true) { "trueFalse" } else { q_type };
        ui.add_space(4.0);
        ui.label(egui::RichText::new(format!("{label} {head}").trim().to_string()).strong());
        crate::rtl::grid(ui, egui::Grid::new(("exercise_quiz", ex, k)).num_columns(2).spacing([6.0, 4.0]), |ui| {
            ui.label(tl!("Mode"));
            let shown_mode = QUESTION_MODES.iter().find(|(id, _)| *id == cur_mode).map_or_else(|| cur_mode.to_string(), |(_, l)| tl!(l).to_string());
            craft_ui::ComboBox::from_id_salt(("exercise_quiz_mode", ex, k)).selected_text(shown_mode).show_ui(ui, |ui| {
                for (id, mode_label) in QUESTION_MODES {
                    if ui.selectable_label(id == cur_mode, tl!(mode_label)).clicked() && id != cur_mode {
                        let _ = app.run("exercise.questionMode", json!({"exercise": ex, "question": k + 1, "type": id}));
                    }
                }
            });
            ui.end_row();
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
            if q_type == "matching" {
                ui.label(tl!("Pairs (left = right)"));
                let current = q["pairs"]
                    .as_array()
                    .map(|pairs| {
                        pairs
                            .iter()
                            .map(|p| format!("{} = {}", p[0].as_str().unwrap_or_default(), p[1].as_str().unwrap_or_default()))
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();
                if let Some(text) = draft_box(ui, ("exercise_matching", ex, k), &current, true, true) {
                    let pairs: Vec<_> =
                        text.lines().take(26).filter_map(|line| line.split_once('=').map(|(a, b)| json!([a.trim(), b.trim()]))).collect();
                    set(app, k, json!({"pairs":pairs}));
                }
                ui.end_row();
            }
            if q_type == "choice" {
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
                ui.label("");
                let mut shuf = q["shuffleChoices"].as_bool().unwrap_or(false);
                if ui
                    .checkbox(&mut shuf, tl!("Shuffle options per variant"))
                    .on_hover_text(tl!("Keep variant A in canonical order and deterministically permute options in variants B, C…"))
                    .changed()
                {
                    set(app, k, json!({"shuffleChoices": shuf}));
                }
                ui.end_row();
                if n > 0 {
                    ui.label(tl!("Rationales"));
                    egui::CollapsingHeader::new(tl!("Teacher notes per option")).id_salt(("quiz_rationales", ex, k)).default_open(false).show(
                        ui,
                        |ui| {
                            let rats: Vec<Option<String>> =
                                q["rationales"].as_array().map(|a| a.iter().map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
                            for j in 0..n.min(26) {
                                let letter = char::from(b'A' + j as u8).to_string();
                                let cur_rat = rats.get(j).and_then(Option::as_deref).unwrap_or_default().to_string();
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(format!("{letter}:")).monospace());
                                    if let Some(t) = draft_box(ui, ("quiz_rat_box", ex, k, j), &cur_rat, true, false) {
                                        let mut next_rats: Vec<Value> =
                                            (0..n).map(|idx| rats.get(idx).and_then(Option::as_deref).map_or(Value::Null, |s| json!(s))).collect();
                                        next_rats[j] = if t.trim().is_empty() { Value::Null } else { json!(t.trim()) };
                                        set(app, k, json!({"rationales": next_rats}));
                                    }
                                });
                            }
                        },
                    );
                    ui.end_row();
                }
            } else {
                if q_type == "calc" {
                    ui.label(tl!("Result"));
                    ui.horizontal_wrapped(|ui| {
                        let preview = q["result"]
                            .as_str()
                            .filter(|s| !s.is_empty())
                            .or_else(|| q["line"].as_str().filter(|s| !s.is_empty() && !s.contains("____")))
                            .map(str::to_string)
                            .unwrap_or_else(|| tl!("No result equation yet").to_string());
                        let is_err = preview.starts_with('⚠');
                        let rt = if is_err {
                            egui::RichText::new(preview).color(egui::Color32::from_rgb(196, 62, 28))
                        } else if q["result"].is_null() {
                            egui::RichText::new(preview).weak()
                        } else {
                            egui::RichText::new(preview).monospace()
                        };
                        ui.label(rt);
                        if let Some(pos) = q.get("workingPos").filter(|p| p.is_object())
                            && ui
                                .small_button(tl!("Go to working"))
                                .on_hover_text(tl!("Put the cursor in this calculation's working space"))
                                .clicked()
                        {
                            let _ = app.run("caret.set", json!({"pos": pos}));
                            app.canvas.want_focus = true;
                        }
                    });
                    ui.end_row();
                    ui.label(tl!("Tolerance"));
                    let cur_tol = q["tolerance"].as_str().unwrap_or_default().to_string();
                    ui.scope(|ui| {
                        ui.set_width(90.0);
                        if let Some(t) = draft_box(ui, ("exercise_quiz_tol", ex, k), &cur_tol, true, false) {
                            let val = if t.trim().is_empty() { Value::Null } else { json!(t.trim()) };
                            set(app, k, json!({"tolerance": val}));
                        }
                    });
                    ui.end_row();
                } else if q_type == "drawing" {
                    ui.label(tl!("Scale"));
                    ui.horizontal_wrapped(|ui| {
                        let scale = q["scale"].as_str().unwrap_or("1:1");
                        ui.label(egui::RichText::new(scale).monospace());
                        if let Some(sel) = q.get("cadSelect").filter(|s| s.is_object())
                            && ui
                                .small_button(tl!("Open in CAD editor"))
                                .on_hover_text(tl!("Select this question's solution drawing and open it in the CAD editor"))
                                .clicked()
                        {
                            let _ = app.run("select.range", sel.clone());
                            let _ = app.run("ui.cadEdit", json!({}));
                        }
                    });
                    ui.end_row();
                }
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

/// Optional per-subquestion points (`exercise.questionPoints`) for a problem exercise (`a)`, `b)`…).
pub(crate) fn problem_questions(app: &mut WordApp, ui: &mut Ui, ex: u64, st: &Value) {
    let list = st["questions"].as_array().cloned().unwrap_or_default();
    if list.is_empty() {
        return;
    }
    crate::rtl::grid(ui, egui::Grid::new(("exercise_problem_pts", ex)).num_columns(2).spacing([6.0, 4.0]), |ui| {
        for (k, q) in list.iter().enumerate() {
            let label = q["label"].as_str().unwrap_or_default();
            let text: String = q["text"].as_str().unwrap_or_default().chars().take(48).collect();
            let head = if text.is_empty() { tl!("Question").to_string() } else { text };
            ui.label(format!("{label} {head}").trim().to_string());
            let current = q["points"].as_f64().map(shown).unwrap_or_default();
            ui.scope(|ui| {
                ui.set_width(60.0);
                if let Some(t) = draft_box(ui, ("exercise_problem_points", ex, k), &current, true, false) {
                    let v = if t.trim().is_empty() { Value::Null } else { number(&t).map_or(Value::Null, |n| json!(n)) };
                    let _ = app.run("exercise.questionPoints", json!({"exercise": ex, "question": k + 1, "points": v}));
                }
            });
            ui.end_row();
        }
    });
    if let Some(total) = st["total"].as_f64() {
        ui.add_space(4.0);
        ui.label(format!("{}: {}", tl!("Total"), shown(total)));
    }
}
