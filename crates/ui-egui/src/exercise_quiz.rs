//! The Questions section of the Exercise Properties pane (docs/exercise.md, Kinds of exercise):
//! a compact list of the questions (label, text, type, points) and, under it, the settings of the
//! one chosen — the question the cursor is in, until another is clicked: its type and points,
//! then what its type has (the answer space, the correct options, the tolerance…), through
//! `exercise.questions` and `exercise.questionMode`. One question's settings at a time keeps the
//! pane readable however many questions the exercise has.

use egui::{RichText, Ui};
use serde_json::{Value, json};

use crate::WordApp;
use crate::a11y_pane::draft_box;
use crate::graph_pane::num_field;

/// A number typed in a box (`2`, `1,5`); `None` for anything else.
fn number(t: &str) -> Option<f64> {
    t.trim().replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite())
}

/// `v` written back for a box: whole numbers plain, else up to two decimals.
fn shown(v: f64) -> String {
    let t = format!("{v:.2}");
    t.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Question types offered (`exercise.questionMode`), with their names.
pub(crate) const QUESTION_MODES: [(&str, &str); 9] = [
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

/// The type a question reports, a true or false question told from a multiple choice.
fn mode_of(q: &Value) -> &str {
    let t = q["type"].as_str().unwrap_or("open");
    if t == "choice" && q["trueFalse"].as_bool() == Some(true) { "trueFalse" } else { t }
}

fn mode_name(id: &str) -> String {
    QUESTION_MODES.iter().find(|(m, _)| *m == id).map_or_else(|| id.to_string(), |(_, l)| tl!(l).to_string())
}

/// The question whose settings show (0-based): the one the cursor moved into, else the one
/// clicked last, else the first.
fn chosen(ui: &Ui, ex: u64, st: &Value, n: usize) -> usize {
    let pick = egui::Id::unique(("exercise_quiz_pick", ex));
    let seen = egui::Id::unique(("exercise_quiz_seen", ex));
    let current = st["current"].as_u64().map(|c| c.saturating_sub(1) as usize);
    let last_seen = ui.data(|d| d.get_temp::<Option<usize>>(seen)).flatten();
    if current != last_seen {
        ui.data_mut(|d| d.insert_temp(seen, current));
        if let Some(c) = current {
            ui.data_mut(|d| d.insert_temp(pick, c));
        }
    }
    ui.data(|d| d.get_temp::<usize>(pick)).unwrap_or(0).min(n.saturating_sub(1))
}

/// The questions of exercise `ex` (1-based) with the chosen one's settings; `st` is
/// `exercise.questions`' reply. `quiz`: an exercise answered in its own spaces (else a problem,
/// whose questions have only points).
pub(crate) fn questions(app: &mut WordApp, ui: &mut Ui, ex: u64, st: &Value, quiz: bool) {
    let list = st["questions"].as_array().cloned().unwrap_or_default();
    if list.is_empty() {
        crate::pane_kit::note(ui, tl!("No question yet: write one in the exercise, or add one."));
    } else {
        let k = chosen(ui, ex, st, list.len());
        list_rows(ui, ex, &list, k, quiz);
        if let Some(q) = list.get(k) {
            ui.add_space(6.0);
            crate::pane_kit::card(ui, |ui| {
                if quiz {
                    settings(app, ui, ex, k, q);
                } else {
                    problem_settings(app, ui, ex, k, q);
                }
            });
        }
    }
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        if let Some(total) = st["total"].as_f64() {
            ui.label(RichText::new(format!("{}: {} pt", tl!("Total"), shown(total))).strong());
        }
        if quiz {
            add_question_menu(app, ui);
        } else if ui.button(tl!("Add Question")).on_hover_text(tl!("Add a question at the end of the exercise")).clicked() {
            let _ = app.run("exercise.addQuestion", json!({}));
        }
    });
}

/// One row per question: label and the start of its text, its type and its points. A click
/// shows that question's settings.
fn list_rows(ui: &mut Ui, ex: u64, list: &[Value], chosen: usize, quiz: bool) {
    let t = crate::theme::Tokens::get(ui.ctx());
    let pick = egui::Id::unique(("exercise_quiz_pick", ex));
    for (k, q) in list.iter().enumerate() {
        let label = q["label"].as_str().unwrap_or_default();
        let text: String = q["text"].as_str().unwrap_or_default().chars().take(40).collect();
        let text = if text.is_empty() { tl!("(question not written yet)").to_string() } else { text };
        let pts = q["points"].as_f64().map_or_else(|| "—".to_string(), |p| format!("{} pt", shown(p)));
        let selected = k == chosen;
        let frame = egui::Frame::new()
            .fill(if selected { t.accent.gamma_multiply(0.18) } else { egui::Color32::TRANSPARENT })
            .corner_radius(egui::CornerRadius::same(4))
            .inner_margin(egui::Margin::symmetric(6, 3));
        let r = frame
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    if !label.is_empty() {
                        ui.label(RichText::new(label).monospace().strong());
                    }
                    ui.add(egui::Label::new(RichText::new(text)).truncate().selectable(false));
                    ui.with_layout(crate::rtl::far_side(ui, egui::Align::Center), |ui| {
                        ui.label(RichText::new(pts).monospace().color(t.text_dim));
                        if quiz {
                            ui.label(RichText::new(mode_name(mode_of(q))).small().color(t.text_dim));
                        }
                    });
                });
            })
            .response
            .interact(egui::Sense::click());
        if r.on_hover_text(tl!("Show this question's settings")).clicked() {
            ui.data_mut(|d| d.insert_temp(pick, k));
        }
    }
}

/// A quiz question's settings: its type and points, then what its type has.
fn settings(app: &mut WordApp, ui: &mut Ui, ex: u64, k: usize, q: &Value) {
    let set = |app: &mut WordApp, params: Value| {
        let mut p = json!({"exercise": ex, "question": k + 1});
        if let (Some(o), Some(extra)) = (p.as_object_mut(), params.as_object()) {
            for (key, v) in extra {
                o.insert(key.clone(), v.clone());
            }
        }
        if let Err(e) = app.run("exercise.questions", p) {
            app.status(e);
        }
    };
    let label = q["label"].as_str().unwrap_or_default();
    let head = if label.is_empty() { tl!("Question").to_string() } else { format!("{} {label}", tl!("Question")) };
    ui.label(RichText::new(head).strong());
    ui.add_space(4.0);
    let mode = mode_of(q).to_string();
    let q_type = q["type"].as_str().unwrap_or("open").to_string();
    crate::rtl::grid(ui, egui::Grid::new(("exercise_quiz", ex, k)).num_columns(2).spacing([8.0, 6.0]), |ui| {
        ui.label(tl!("Type"));
        craft_ui::ComboBox::from_id_salt(("exercise_quiz_mode", ex, k)).selected_text(mode_name(&mode)).show_ui(ui, |ui| {
            for (id, name) in QUESTION_MODES {
                if ui.selectable_label(id == mode, tl!(name)).clicked() && id != mode {
                    let _ = app.run("exercise.questionMode", json!({"exercise": ex, "question": k + 1, "type": id}));
                }
            }
        });
        ui.end_row();
        ui.label(tl!("Points"));
        ui.horizontal(|ui| {
            let current = q["points"].as_f64().map(shown).unwrap_or_default();
            ui.scope(|ui| {
                ui.set_width(56.0);
                if let Some(t) = draft_box(ui, ("exercise_quiz_points", ex, k), &current, true, false) {
                    let v = if t.trim().is_empty() { Value::Null } else { number(&t).map_or(Value::Null, |n| json!(n)) };
                    set(app, json!({"points": v}));
                }
            });
            ui.label("pt");
        })
        .response
        .on_hover_text(tl!("The most points this question gives; empty: not scored"));
        ui.end_row();
    });
    match q_type.as_str() {
        "choice" => choice_settings(app, ui, ex, k, q, &set),
        "matching" => {
            sub(ui, tl!("Pairs"));
            crate::pane_kit::note(ui, tl!("One pair per line: left = right"));
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
                let pairs: Vec<_> = text.lines().take(26).filter_map(|line| line.split_once('=').map(|(a, b)| json!([a.trim(), b.trim()]))).collect();
                set(app, json!({"pairs": pairs}));
            }
        }
        _ => {
            if q_type == "calc" {
                calc_settings(app, ui, ex, k, q, &set);
            } else if q_type == "drawing" {
                drawing_settings(app, ui, q);
            }
            space_settings(app, ui, ex, k, q, &set);
        }
    }
}

/// A small heading inside the settings card.
fn sub(ui: &mut Ui, text: &str) {
    let t = crate::theme::Tokens::get(ui.ctx());
    ui.add_space(8.0);
    ui.label(RichText::new(text.to_uppercase()).font(crate::pane_kit::mono(10.0)).color(t.text_dim));
    ui.add_space(2.0);
}

/// The answer space: its height, squared or blank, the side of a square.
fn space_settings(app: &mut WordApp, ui: &mut Ui, ex: u64, k: usize, q: &Value, set: &dyn Fn(&mut WordApp, Value)) {
    sub(ui, tl!("Answer Space"));
    crate::rtl::grid(ui, egui::Grid::new(("exercise_quiz_space", ex, k)).num_columns(2).spacing([8.0, 6.0]), |ui| {
        ui.label(tl!("Height"));
        let h = q["height"].as_f64().unwrap_or(4.0);
        if let Some(v) = num_field(ui, &format!("quiz_height_{ex}_{k}"), h, 0.5..=25.0, " cm", 1) {
            set(app, json!({"height": v}));
        }
        ui.end_row();
        ui.label("");
        let mut grid = q["grid"].as_bool().unwrap_or(true);
        if ui.checkbox(&mut grid, tl!("Squared")).on_hover_text(tl!("Graph paper in the answer space; off, the space is blank")).changed() {
            set(app, json!({"grid": grid}));
        }
        ui.end_row();
        if grid {
            ui.label(tl!("Square"));
            let sp = q["spacing"].as_f64().unwrap_or(5.0);
            if let Some(v) = num_field(ui, &format!("quiz_spacing_{ex}_{k}"), sp, 2.0..=20.0, " mm", 1) {
                set(app, json!({"spacing": v}));
            }
            ui.end_row();
        }
    });
}

/// A calculation: the result its working gives, and the tolerance accepted.
fn calc_settings(app: &mut WordApp, ui: &mut Ui, ex: u64, k: usize, q: &Value, set: &dyn Fn(&mut WordApp, Value)) {
    sub(ui, tl!("Result"));
    let t = crate::theme::Tokens::get(ui.ctx());
    let preview = q["result"]
        .as_str()
        .filter(|s| !s.is_empty())
        .or_else(|| q["line"].as_str().filter(|s| !s.is_empty() && !s.contains("____")))
        .map(str::to_string)
        .unwrap_or_else(|| tl!("No result equation yet").to_string());
    let rt = if preview.starts_with('⚠') {
        RichText::new(preview).color(t.red)
    } else if q["result"].is_null() {
        RichText::new(preview).color(t.text_dim)
    } else {
        RichText::new(preview).monospace()
    };
    ui.add(egui::Label::new(rt).wrap());
    if let Some(pos) = q.get("workingPos").filter(|p| p.is_object())
        && ui.small_button(tl!("Go to working")).on_hover_text(tl!("Put the cursor in this calculation's working space")).clicked()
    {
        let _ = app.run("caret.set", json!({"pos": pos}));
        app.canvas.want_focus = true;
    }
    ui.horizontal(|ui| {
        ui.label(tl!("Tolerance"));
        let cur = q["tolerance"].as_str().unwrap_or_default().to_string();
        ui.scope(|ui| {
            ui.set_width(90.0);
            if let Some(t) = draft_box(ui, ("exercise_quiz_tol", ex, k), &cur, true, false) {
                let val = if t.trim().is_empty() { Value::Null } else { json!(t.trim()) };
                set(app, json!({"tolerance": val}));
            }
        });
    })
    .response
    .on_hover_text(tl!("How far an answer may be from the result and still be right: 2% or 0.1 cm"));
}

/// A CAD drawing: its scale, and the drawing opened in the CAD editor.
fn drawing_settings(app: &mut WordApp, ui: &mut Ui, q: &Value) {
    sub(ui, tl!("Drawing"));
    ui.horizontal_wrapped(|ui| {
        ui.label(tl!("Scale"));
        ui.label(RichText::new(q["scale"].as_str().unwrap_or("1:1")).monospace());
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
}

/// A multiple choice: the correct options, their order per variant, the notes on each.
fn choice_settings(app: &mut WordApp, ui: &mut Ui, ex: u64, k: usize, q: &Value, set: &dyn Fn(&mut WordApp, Value)) {
    let choices: Vec<String> =
        q["choices"].as_array().map(|a| a.iter().map(|c| c.as_str().unwrap_or_default().to_string()).collect()).unwrap_or_default();
    let correct: Vec<String> = q["correct"].as_array().map(|a| a.iter().filter_map(|c| c.as_str().map(str::to_string)).collect()).unwrap_or_default();
    sub(ui, tl!("Correct Options"));
    if choices.is_empty() {
        crate::pane_kit::note(ui, tl!("Write the options in the exercise: each line under the question is one."));
    }
    for (j, text) in choices.iter().enumerate().take(26) {
        let letter = char::from(b'A' + j as u8).to_string();
        let mut on = correct.contains(&letter);
        let label = if text.is_empty() { format!("{letter})") } else { format!("{letter}) {}", text.chars().take(40).collect::<String>()) };
        if ui.checkbox(&mut on, label).on_hover_text(tl!("Mark this option correct (more than one can be)")).changed() {
            let mut want = correct.clone();
            if on {
                want.push(letter);
            } else {
                want.retain(|c| *c != letter);
            }
            set(app, json!({"correct": want}));
        }
    }
    ui.add_space(4.0);
    let mut shuf = q["shuffleChoices"].as_bool().unwrap_or(false);
    if ui
        .checkbox(&mut shuf, tl!("Shuffle options per variant"))
        .on_hover_text(tl!("Keep variant A in canonical order and deterministically permute options in variants B, C…"))
        .changed()
    {
        set(app, json!({"shuffleChoices": shuf}));
    }
    if choices.is_empty() {
        return;
    }
    egui::CollapsingHeader::new(tl!("Teacher notes per option")).id_salt(("quiz_rationales", ex, k)).default_open(false).show(ui, |ui| {
        let n = choices.len();
        let rats: Vec<Option<String>> =
            q["rationales"].as_array().map(|a| a.iter().map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
        for j in 0..n.min(26) {
            let letter = char::from(b'A' + j as u8).to_string();
            let cur = rats.get(j).and_then(Option::as_deref).unwrap_or_default().to_string();
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{letter}:")).monospace());
                if let Some(t) = draft_box(ui, ("quiz_rat_box", ex, k, j), &cur, true, false) {
                    let mut next: Vec<Value> = (0..n).map(|idx| rats.get(idx).and_then(Option::as_deref).map_or(Value::Null, |s| json!(s))).collect();
                    if let Some(slot) = next.get_mut(j) {
                        *slot = if t.trim().is_empty() { Value::Null } else { json!(t.trim()) };
                    }
                    set(app, json!({"rationales": next}));
                }
            });
        }
    });
}

/// A problem's question: only its points (it is answered in the solution).
fn problem_settings(app: &mut WordApp, ui: &mut Ui, ex: u64, k: usize, q: &Value) {
    let label = q["label"].as_str().unwrap_or_default();
    let head = if label.is_empty() { tl!("Question").to_string() } else { format!("{} {label}", tl!("Question")) };
    ui.label(RichText::new(head).strong());
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(tl!("Points"));
        let current = q["points"].as_f64().map(shown).unwrap_or_default();
        ui.scope(|ui| {
            ui.set_width(56.0);
            if let Some(t) = draft_box(ui, ("exercise_problem_points", ex, k), &current, true, false) {
                let v = if t.trim().is_empty() { Value::Null } else { number(&t).map_or(Value::Null, |n| json!(n)) };
                if let Err(e) = app.run("exercise.questionPoints", json!({"exercise": ex, "question": k + 1, "points": v})) {
                    app.status(e);
                }
            }
        });
        ui.label("pt");
    });
    crate::pane_kit::note(ui, tl!("A problem's questions are answered in its solution."));
}

/// Add Question, with the type to add.
fn add_question_menu(app: &mut WordApp, ui: &mut Ui) {
    let r = ui.menu_button(format!("+ {}", tl!("Add Question")), |ui| {
        if ui.button(tl!("Like the last one")).clicked() {
            let _ = app.run("exercise.addQuestion", json!({}));
            ui.close();
        }
        ui.separator();
        for (id, name) in QUESTION_MODES {
            if ui.button(tl!(name)).clicked() {
                let _ = app.run("exercise.addQuestion", json!({"type": id}));
                ui.close();
            }
        }
    });
    r.response.on_hover_text(tl!("Add a question at the end of the exercise"));
}
