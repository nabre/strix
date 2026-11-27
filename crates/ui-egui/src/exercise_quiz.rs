//! The Question Settings pane (Exercise › Questions › Question Settings, `view.questionsPane`;
//! docs/exercise.md, Kinds of exercise):
//! the questions of the exercise at the cursor, one row each with its own settings on it — the
//! label, the start of its text and its points at the far end (in a column down the pane), a quiz
//! question's type always on the line under its text — so every question and sub-question is set
//! where it is listed; sub-questions are indented under their question. Under its row a question
//! has what its type uses and nothing else (`sections`): first a line saying what the student
//! does and what the corrected copy shows, then headed sections (the result and its format, the
//! answer space, the correct options or answer, the drawing, the pairs) whose labels and boxes
//! have the same widths everywhere, so they line up. Shown or hidden with a click on the row and
//! opened for the question the cursor moves into; through `exercise.questions`,
//! `exercise.questionMode` and `exercise.questionPoints`. It follows the cursor: outside the
//! exercises it says so.

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
pub(crate) const QUESTION_MODES: [(&str, &str); 8] = [
    ("open", "Open Answer"),
    ("choice", "Multiple Choice"),
    ("trueFalse", "True or False"),
    ("calc", "Calculation"),
    ("drawing", "CAD Drawing Question"),
    ("worked", "Worked Example"),
    ("cloze", "Inline Completion"),
    ("matching", "Matching"),
];

/// The type a question reports, a true or false question told from a multiple choice.
pub(crate) fn mode_of(q: &Value) -> &str {
    let t = q["type"].as_str().unwrap_or("open");
    if t == "choice" && q["trueFalse"].as_bool() == Some(true) { "trueFalse" } else { t }
}

fn mode_name(id: &str) -> String {
    QUESTION_MODES.iter().find(|(m, _)| *m == id).map_or_else(|| id.to_string(), |(_, l)| tl!(l).to_string())
}

/// How deep a question sits under the exercise's own questions, from its label: `a)` 0,
/// `a.1)` 1… (at most 4, whatever the label says).
fn depth(label: &str) -> usize {
    label.matches('.').count().min(4)
}

/// Where a question's "details shown" flag lives (quiz questions; 0-based).
fn open_id(ex: u64, k: usize) -> egui::Id {
    egui::Id::unique(("exercise_quiz_open", ex, k))
}

/// The question the cursor is in (0-based), its details opened when the cursor moves into it.
fn at_cursor(ui: &Ui, ex: u64, st: &Value) -> Option<usize> {
    let seen = egui::Id::unique(("exercise_quiz_seen", ex));
    let current = st["current"].as_u64().map(|c| c.saturating_sub(1) as usize);
    let last_seen = ui.data(|d| d.get_temp::<Option<usize>>(seen)).flatten();
    if current != last_seen {
        ui.data_mut(|d| d.insert_temp(seen, current));
        if let Some(c) = current {
            ui.data_mut(|d| d.insert_temp(open_id(ex, c), true));
        }
    }
    current
}

/// The Question Settings pane: the questions of the exercise at the cursor, a note outside them.
pub(crate) fn pane(app: &mut WordApp, ui: &mut Ui) {
    let Ok(info) = app.session.run("exercise.data", &json!({})) else {
        crate::pane_kit::note(ui, tl!("Put the cursor in an exercise to see its questions' settings."));
        return;
    };
    let ex = info["exercise"].as_u64().unwrap_or(0);
    let st = app.session.run("exercise.questions", &json!({"exercise": ex})).ok().unwrap_or_default();
    let quiz = st["kind"].as_str().unwrap_or("problem") != "problem";
    // Both ways: in a pane narrower than a question's settings, what does not fit is scrolled to.
    egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| questions(app, ui, ex, &st, quiz));
}

/// The questions of exercise `ex` (1-based), each with its own settings; `st` is
/// `exercise.questions`' reply. `quiz`: an exercise answered in its own spaces.
pub(crate) fn questions(app: &mut WordApp, ui: &mut Ui, ex: u64, st: &Value, quiz: bool) {
    let list = st["questions"].as_array().cloned().unwrap_or_default();
    if list.is_empty() {
        crate::pane_kit::note(ui, tl!("No question yet: write one in the exercise, or add one."));
    } else {
        let current = at_cursor(ui, ex, st);
        for (k, q) in list.iter().enumerate() {
            question(app, ui, ex, k, q, quiz, current == Some(k));
        }
        if !quiz {
            ui.add_space(4.0);
            crate::pane_kit::note(ui, tl!("A problem's questions are answered in its solution."));
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

/// Change settings of question `k` (0-based) of a quiz through `exercise.questions`.
fn set_question(app: &mut WordApp, ex: u64, k: usize, params: Value) {
    let mut p = json!({"exercise": ex, "question": k + 1});
    if let (Some(o), Some(extra)) = (p.as_object_mut(), params.as_object()) {
        for (key, v) in extra {
            o.insert(key.clone(), v.clone());
        }
    }
    if let Err(e) = app.run("exercise.questions", p) {
        app.status(e);
    }
}

/// The width of the labels' column in a question's settings: the boxes start at the same place
/// in every section and every question.
const LABEL_W: f32 = 118.0;
/// The width of a box or list in a question's settings: the same for every setting.
const FIELD_W: f32 = 128.0;
/// The width of the question type's list on a quiz question's row.
const TYPE_W: f32 = 172.0;

/// The narrowest a label's column and a box in a question's settings get in a narrow pane.
const MIN_LABEL_W: f32 = 48.0;
const MIN_FIELD_W: f32 = 56.0;

/// The widths of the labels' column and of the boxes in a form `avail` points wide (pure,
/// tested): [`LABEL_W`] and [`FIELD_W`] when they fit, narrower in a narrow pane, so a box is not
/// cut at the pane's edge.
fn form_widths(avail: f32) -> (f32, f32) {
    let avail = if avail.is_finite() { avail.max(0.0) } else { LABEL_W + FIELD_W + 8.0 };
    let label = LABEL_W.min(avail * 0.45).max(MIN_LABEL_W);
    let field = (avail - label - 8.0).clamp(MIN_FIELD_W, FIELD_W);
    (label, field)
}

/// Where the current form's box width is kept while its rows are added.
fn field_w_id() -> egui::Id {
    egui::Id::new("exercise_quiz_field_w")
}

/// The width of a box in the form being added.
fn field_w(ui: &Ui) -> f32 {
    ui.data(|d| d.get_temp::<f32>(field_w_id())).unwrap_or(FIELD_W)
}

/// How far a question's type and settings sit in from the row's start: under its text, `text_at`,
/// unless the pane is too narrow for that, when they move left to keep `room` points (pure,
/// tested).
fn inset(text_at: f32, avail: f32, room: f32) -> f32 {
    text_at.min((avail - room).max(0.0))
}

/// What the student does and what the corrected copy shows for a question of type `mode`
/// ([`mode_of`]'s, or `problem`), written at the top of its settings; `squared`: its answer space
/// is squared (a calculation's and a drawing's say so only then).
fn mode_help(mode: &str, squared: bool) -> &'static str {
    match mode {
        "problem" => "Answered in the problem's solution; here the format of its result.",
        "choice" => {
            "The student ticks the options' boxes; the corrected copy ticks the correct ones. Write the options in the exercise, one per line under the question."
        }
        "trueFalse" => "The student ticks True or False on the statement's line; the corrected copy ticks the right one.",
        "calc" if !squared => {
            "The student works it out in the blank answer space and writes the result on the answer line; the corrected copy shows your working and its result, calculated with the exercise's data."
        }
        "drawing" if !squared => "The student draws in the blank answer space; the corrected copy shows your CAD drawing in it.",
        "calc" => {
            "The student works it out in the squared space and writes the result on the answer line; the corrected copy shows your working and its result, calculated with the exercise's data."
        }
        "drawing" => "The student draws in the squared space; the corrected copy shows your CAD drawing in it.",
        "worked" => "A solved example: its working is always shown, also in the students' copy, and it gives no points.",
        "cloze" => {
            "The student writes the hidden words on the lines in the text: type [[word]] in the question, or select words and use Exercise › Hide Words."
        }
        "matching" => "The student matches each item on the left with one on the right; the corrected copy shows the pairs.",
        _ => "The student writes the answer in the space under the question; the corrected copy shows the answer you write there.",
    }
}

/// One question: its row — the label, the start of its text and, at the far end, its points (in
/// a column down the pane); under them, in a quiz, its type — then, opened with a click on the
/// row, what its type has (the answer space, the correct options, the result…). Sub-questions are
/// indented under their question. `current`: the cursor is in it.
fn question(app: &mut WordApp, ui: &mut Ui, ex: u64, k: usize, q: &Value, quiz: bool, current: bool) {
    let t = crate::theme::Tokens::get(ui.ctx());
    let label = q["label"].as_str().unwrap_or_default();
    let text: String = q["text"].as_str().unwrap_or_default().chars().take(60).collect();
    let (text, written) = if text.is_empty() { (tl!("(question not written yet)").to_string(), false) } else { (text, true) };
    let open = ui.data(|d| d.get_temp::<bool>(open_id(ex, k))).unwrap_or(false);
    let indent = 14.0 * depth(label) as f32;
    let far = crate::rtl::far_side(ui, egui::Align::Center);
    let near = crate::rtl::row_layout(ui, egui::Align::Center);
    let frame = egui::Frame::new()
        .fill(if current { t.accent.gamma_multiply(0.18) } else { egui::Color32::TRANSPARENT })
        .stroke(egui::Stroke::new(1.0, if open { t.border } else { egui::Color32::TRANSPARENT }))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(6, 4));
    // A click on the row shows or hides its details; one among the details (a label there, the
    // space between two boxes) leaves them open. The row takes its clicks where it was drawn
    // last, before what is on it is added: the points box and the type, added after, are above
    // it and keep their own clicks (taken after them, the row had every click on them too).
    let row_id = ui.scope_id().with(("exercise_quiz_row", ex, k));
    if let Some(row) = ui.data(|d| d.get_temp::<egui::Rect>(row_id)) {
        let r = ui.interact(row, row_id, egui::Sense::click());
        if r.on_hover_text(tl!("Show this question's settings")).clicked() {
            ui.data_mut(|d| d.insert_temp(open_id(ex, k), !open));
        }
    }
    let shown_frame = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        let row_h = ui.spacing().interact_size.y;
        // Where the text starts: the type goes under it.
        let text_at = indent + 12.0 + ui.spacing().item_spacing.x;
        // The points first, from the far end, so the text gets what is left and is cut there.
        ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), row_h), far, |ui| {
            points_box(app, ui, ex, k, q, quiz);
            ui.with_layout(near, |ui| {
                ui.add_space(indent);
                {
                    let (rect, mut icon) = ui.allocate_exact_size(egui::vec2(12.0, row_h), egui::Sense::hover());
                    icon.rect = rect.shrink2(egui::vec2(1.0, (row_h - 10.0).max(0.0) / 2.0));
                    egui::collapsing_header::paint_default_icon(ui, if open { 1.0 } else { 0.0 }, &icon);
                }
                if !label.is_empty() {
                    ui.label(RichText::new(label).monospace().strong());
                }
                let rt = if written { RichText::new(text.as_str()) } else { RichText::new(text.as_str()).color(t.text_dim) };
                ui.add(egui::Label::new(rt).truncate().selectable(false));
            });
        });
        // A quiz question's type, always on a line of its own under the text.
        if quiz {
            ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), row_h), near, |ui| {
                ui.add_space(inset(text_at, ui.available_width(), TYPE_W));
                type_combo(app, ui, ex, k, q);
            });
        }
        // Where the row itself ends: what is under it is not the row.
        let row_bottom = ui.min_rect().bottom();
        if open {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(inset(text_at, ui.available_width(), LABEL_W + FIELD_W + 8.0));
                ui.vertical(|ui| {
                    ui.set_max_width(LABEL_W + FIELD_W + 12.0);
                    let mode = if quiz { mode_of(q) } else { "problem" };
                    crate::pane_kit::note(ui, tl!(mode_help(mode, q["grid"].as_bool().unwrap_or(true))));
                    details(app, ui, ex, k, q);
                });
            });
            ui.add_space(4.0);
        }
        row_bottom
    });
    {
        // Where the row is, for its clicks in the next frame: not what is under it.
        let mut row = shown_frame.response.rect;
        row.max.y = row.max.y.min(shown_frame.inner + 4.0);
        ui.data_mut(|d| d.insert_temp(row_id, row));
    }
    ui.add_space(2.0);
}

/// A question's points on its row, at the far end: a box and "pt"; a worked example says it gives
/// none.
fn points_box(app: &mut WordApp, ui: &mut Ui, ex: u64, k: usize, q: &Value, quiz: bool) {
    let t = crate::theme::Tokens::get(ui.ctx());
    let row_h = ui.spacing().interact_size.y;
    if quiz && mode_of(q) == "worked" {
        ui.label(RichText::new(tl!("No points")).color(t.text_dim)).on_hover_text(tl!("A worked example gives no points"));
        return;
    }
    // The box is named by its "pt" for screen readers.
    let pt = ui.label(RichText::new("pt").color(t.text_dim));
    let now = q["points"].as_f64().map(shown).unwrap_or_default();
    let points = ui.allocate_ui_with_layout(egui::vec2(46.0, row_h), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        crate::ribbon::named_draft_box(ui, ("exercise_quiz_points", ex, k), &now, pt.id)
    });
    points.response.on_hover_text(tl!("The most points this question gives; empty: not scored"));
    if let Some(typed) = points.inner {
        // Only an empty box takes the points away: text that is no number leaves them as they
        // were and says so.
        let v = match number(&typed) {
            Some(n) => json!(n),
            None if typed.trim().is_empty() => Value::Null,
            None => {
                app.status(tl!("Points: a number, or empty for a question that is not scored").to_string());
                return;
            }
        };
        if quiz {
            set_question(app, ex, k, json!({"points": v}));
        } else if let Err(e) = app.run("exercise.questionPoints", json!({"exercise": ex, "question": k + 1, "points": v})) {
            app.status(e);
        }
    }
}

/// A quiz question's type, chosen in a list under its text.
fn type_combo(app: &mut WordApp, ui: &mut Ui, ex: u64, k: usize, q: &Value) {
    let mode = mode_of(q).to_string();
    let combo = craft_ui::ComboBox::from_id_salt(("exercise_quiz_mode", ex, k))
        .width(TYPE_W.min(ui.available_width()).max(MIN_FIELD_W))
        .selected_text(mode_name(&mode))
        .show_ui(ui, |ui| {
            for (id, name) in QUESTION_MODES {
                let r = ui.selectable_label(id == mode, tl!(name)).on_hover_text(tl!(mode_help(id, true)));
                if r.clicked() && id != mode {
                    let _ = app.run("exercise.questionMode", json!({"exercise": ex, "question": k + 1, "type": id}));
                }
            }
        });
    // Named for screen readers: nothing is written beside it on the row.
    let named = mode_name(&mode);
    combo
        .response
        .on_hover_text(tl!("The question's type: how the student answers it"))
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, format!("{}: {named}", tl!("Question Type"))));
}

/// The sign between the two sides of a pair when a side has an `=` of its own (`F = m·a`).
const PAIR_SIGN: char = '↔';

/// A matching pair as its line in the Pairs box: `left = right`, or `left ↔ right` when a side
/// holds an `=` (read back at the first `=`, the pair would come out cut elsewhere).
fn pair_line(left: &str, right: &str) -> String {
    if left.contains('=') || right.contains('=') { format!("{left} {PAIR_SIGN} {right}") } else { format!("{left} = {right}") }
}

/// The pair a line of the Pairs box holds: its two sides round `↔`, else round the first `=`.
fn pair_of(line: &str) -> Option<(&str, &str)> {
    line.split_once(PAIR_SIGN).or_else(|| line.split_once('=')).map(|(a, b)| (a.trim(), b.trim()))
}

/// The sections a question of type `mode` ([`mode_of`]'s, or `problem`) has under its row, in
/// order (pure, tested): only what that type uses.
fn sections(mode: &str) -> &'static [&'static str] {
    match mode {
        "problem" => &["Result"],
        "calc" => &["Result", "Answer Space"],
        "choice" => &["Correct Options"],
        "trueFalse" => &["Correct Answer"],
        "drawing" => &["Drawing", "Answer Space"],
        "matching" => &["Pairs"],
        "cloze" => &[],
        // An open answer, a worked example.
        _ => &["Answer Space"],
    }
}

/// What a question's type has, under its row.
fn details(app: &mut WordApp, ui: &mut Ui, ex: u64, k: usize, q: &Value) {
    let set = |app: &mut WordApp, params: Value| set_question(app, ex, k, params);
    let mode = if q["type"] == "problem" { "problem" } else { mode_of(q) };
    for section in sections(mode) {
        match *section {
            "Result" => calc_settings(app, ui, ex, k, q, &set),
            "Answer Space" => space_settings(app, ui, ex, k, q, &set),
            "Correct Options" => choice_settings(app, ui, ex, k, q, &set),
            "Correct Answer" => true_false_settings(app, ui, q, &set),
            "Drawing" => drawing_settings(app, ui, q),
            "Pairs" => {
                let pairs_label = sub(ui, tl!("Pairs"));
                crate::pane_kit::note(ui, tl!("One pair per line: left = right"));
                let current = q["pairs"]
                    .as_array()
                    .map(|pairs| {
                        pairs
                            .iter()
                            .map(|p| pair_line(p[0].as_str().unwrap_or_default(), p[1].as_str().unwrap_or_default()))
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();
                if let Some(text) = draft_box(ui, ("exercise_matching", ex, k), pairs_label, &current, true, true) {
                    let pairs: Vec<_> = text.lines().take(26).filter_map(|line| pair_of(line).map(|(a, b)| json!([a, b]))).collect();
                    set(app, json!({"pairs": pairs}));
                }
            }
            _ => {}
        }
    }
}

/// A small heading inside the settings card.
fn sub(ui: &mut Ui, text: &str) -> egui::Id {
    let t = crate::theme::Tokens::get(ui.ctx());
    ui.add_space(8.0);
    let id = ui.label(RichText::new(text.to_uppercase()).font(crate::pane_kit::mono(10.0)).color(t.text_dim)).id;
    ui.add_space(2.0);
    id
}

/// The settings of a section in two columns — the labels, then the boxes — of the same widths in
/// every section, so the boxes line up down the pane; narrower in a narrow pane ([`form_widths`]).
fn form(ui: &mut Ui, id: impl std::hash::Hash, add: impl FnOnce(&mut Ui)) {
    let (label, field) = form_widths(ui.available_width());
    ui.data_mut(|d| d.insert_temp(field_w_id(), field));
    ui.data_mut(|d| d.insert_temp(label_w_id(), label));
    crate::rtl::grid(ui, egui::Grid::new(id).num_columns(2).spacing([8.0, 6.0]).min_col_width(label).max_col_width(label.max(field)), add);
}

/// Where the current form's label width is kept while its rows are added.
fn label_w_id() -> egui::Id {
    egui::Id::new("exercise_quiz_label_w")
}

/// A label in a [`form`]'s first column, cut to the column's width (whole in its tooltip); its id
/// names the box beside it for screen readers.
fn form_label(ui: &mut Ui, text: &str) -> egui::Id {
    let w = ui.data(|d| d.get_temp::<f32>(label_w_id())).unwrap_or(LABEL_W);
    let h = ui.spacing().interact_size.y;
    ui.allocate_ui_with_layout(egui::vec2(w, h), crate::rtl::row_layout(ui, egui::Align::Center), |ui| {
        ui.set_max_width(w);
        ui.add(egui::Label::new(text).truncate()).on_hover_text(text).id
    })
    .inner
}

/// A text box of the [`form`]'s width, what is typed given back once it is left.
fn form_box(ui: &mut Ui, key: impl std::hash::Hash + std::fmt::Debug, label: egui::Id, current: &str) -> Option<String> {
    ui.scope(|ui| {
        ui.set_width(field_w(ui));
        draft_box(ui, key, label, current, true, false)
    })
    .inner
}

/// The answer space: its height, squared or blank, the side of a square, their lines.
fn space_settings(app: &mut WordApp, ui: &mut Ui, ex: u64, k: usize, q: &Value, set: &dyn Fn(&mut WordApp, Value)) {
    sub(ui, tl!("Answer Space"));
    form(ui, ("exercise_quiz_space", ex, k), |ui| {
        form_label(ui, tl!("Height"));
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
            form_label(ui, tl!("Square size"));
            let sp = q["spacing"].as_f64().unwrap_or(5.0);
            if let Some(v) = num_field(ui, &format!("quiz_spacing_{ex}_{k}"), sp, 2.0..=20.0, " mm", 1) {
                set(app, json!({"spacing": v}));
            }
            ui.end_row();
            form_label(ui, tl!("Style"));
            if let Some(st) = crate::graph_pane::style_combo(ui, &format!("quiz_{ex}_{k}"), q["gridStyle"].as_str().unwrap_or("solid")) {
                set(app, json!({"gridStyle": st}));
            }
            ui.end_row();
            form_label(ui, tl!("Colour"));
            let theme = app.session.doc.settings.theme_colors.clone();
            let now = q["gridColor"].as_str().and_then(wordcraft_doc::Rgb::parse).map_or([0x9F, 0xC5, 0xE8], |c| [c.0, c.1, c.2]);
            if let Some(c) = crate::graph_pane::color_combo(ui, &format!("quiz_{ex}_{k}"), &theme, now) {
                set(app, json!({"gridColor": format!("{:02X}{:02X}{:02X}", c[0], c[1], c[2])}));
            }
            ui.end_row();
        }
    });
}

/// A calculation, or a problem's question: the result its working gives, then how it is written
/// (unit, decimals, rounding) and, for a calculation, the tolerance accepted.
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
    result_format(app, ui, ex, k, q, set);
}

/// How the result is written: its unit, decimal places, rounding direction and step, a
/// calculation's tolerance; and going back to the document's format.
fn result_format(app: &mut WordApp, ui: &mut Ui, ex: u64, k: usize, q: &Value, set: &dyn Fn(&mut WordApp, Value)) {
    sub(ui, tl!("Result Format"));
    let format = &q["resultFormat"];
    let change = |app: &mut WordApp, key: &str, value: Value| {
        let mut fields = format.as_object().cloned().unwrap_or_default();
        fields.insert(key.to_string(), value);
        set(app, json!({"resultFormat": fields}));
    };
    form(ui, ("question_result_form", ex, k), |ui| {
        let unit = form_label(ui, tl!("Unit"));
        if let Some(text) = form_box(ui, ("question_result", ex, k, "unit"), unit, format["unit"].as_str().unwrap_or_default()) {
            change(app, "unit", if text.trim().is_empty() { Value::Null } else { json!(text.trim()) });
        }
        ui.end_row();

        let decimals = form_label(ui, tl!("Decimal Places"));
        let current = format["decimals"].as_u64();
        let combo = craft_ui::ComboBox::from_id_salt(("question_decimals", ex, k))
            .width(field_w(ui))
            .selected_text(current.map_or_else(|| tl!("Automatic").to_string(), |n| n.to_string()))
            .show_ui(ui, |ui| {
                if ui.selectable_label(current.is_none(), tl!("Automatic")).clicked() {
                    change(app, "decimals", Value::Null);
                }
                for n in 0..=15 {
                    if ui.selectable_label(current == Some(n), n.to_string()).clicked() {
                        change(app, "decimals", json!(n));
                    }
                }
            });
        combo.response.labelled_by(decimals);
        ui.end_row();

        let rounding = form_label(ui, tl!("Rounding"));
        let modes =
            [(None, tl!("Automatic")), (Some("nearest"), tl!("Round to Nearest")), (Some("up"), tl!("Round Up")), (Some("down"), tl!("Round Down"))];
        let mode = format["stepMode"].as_str();
        let combo = craft_ui::ComboBox::from_id_salt(("question_rounding", ex, k))
            .width(field_w(ui))
            .selected_text(modes.iter().find(|(key, _)| *key == mode).map_or_else(|| tl!("Automatic"), |(_, label)| *label))
            .show_ui(ui, |ui| {
                for (key, label) in modes {
                    if ui.selectable_label(key == mode, label).clicked() {
                        change(app, "stepMode", json!(key));
                    }
                }
            });
        combo.response.labelled_by(rounding);
        ui.end_row();

        let step = form_label(ui, tl!("Rounding Step"));
        let r = ui.scope(|ui| form_box(ui, ("question_result", ex, k, "step"), step, format["step"].as_str().unwrap_or_default()));
        if let Some(text) = r.inner {
            change(app, "step", if text.trim().is_empty() { Value::Null } else { json!(text.trim()) });
        }
        r.response.on_hover_text(tl!("Round the result to a multiple of this: 0.05, 0.5 cm; empty: by the decimal places"));
        ui.end_row();

        if q["type"] == "calc" {
            let tol = form_label(ui, tl!("Tolerance"));
            let cur = q["tolerance"].as_str().unwrap_or_default().to_string();
            let r = ui.scope(|ui| form_box(ui, ("exercise_quiz_tol", ex, k), tol, &cur));
            if let Some(t) = r.inner {
                let val = if t.trim().is_empty() { Value::Null } else { json!(t.trim()) };
                set(app, json!({"tolerance": val}));
            }
            r.response.on_hover_text(tl!("How far an answer may be from the result and still be right: 2% or 0.1 cm"));
            ui.end_row();
        }
    });
    if !format.is_null() && ui.small_button(tl!("Rounding as the Document")).clicked() {
        set(app, json!({"resultFormat": null}));
    }
}

/// A CAD drawing: its scale, and the drawing opened in the CAD editor.
fn drawing_settings(app: &mut WordApp, ui: &mut Ui, q: &Value) {
    sub(ui, tl!("Drawing"));
    form(ui, ("exercise_quiz_drawing", q["label"].as_str().unwrap_or_default()), |ui| {
        form_label(ui, tl!("Scale"));
        ui.label(RichText::new(q["scale"].as_str().unwrap_or("1:1")).monospace());
        ui.end_row();
    });
    if let Some(sel) = q.get("cadSelect").filter(|s| s.is_object())
        && ui
            .small_button(tl!("Open in CAD editor"))
            .on_hover_text(tl!("Select this question's solution drawing and open it in the CAD editor"))
            .clicked()
    {
        let _ = app.run("select.range", sel.clone());
        let _ = app.run("ui.cadEdit", json!({}));
    }
}

/// A true or false question: which of its two answers is the right one (only one can be).
fn true_false_settings(app: &mut WordApp, ui: &mut Ui, q: &Value, set: &dyn Fn(&mut WordApp, Value)) {
    sub(ui, tl!("Correct Answer"));
    let names: Vec<String> =
        q["choices"].as_array().map(|a| a.iter().map(|c| c.as_str().unwrap_or_default().to_string()).collect()).unwrap_or_default();
    let correct: Vec<&str> = q["correct"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    ui.horizontal_wrapped(|ui| {
        for (j, fallback) in [tl!("True"), tl!("False")].into_iter().enumerate() {
            let letter = if j == 0 { "A" } else { "B" };
            let name = names.get(j).filter(|n| !n.trim().is_empty()).map_or(fallback, String::as_str);
            let on = correct.len() == 1 && correct.first() == Some(&letter);
            if ui.radio(on, name).clicked() && !on {
                set(app, json!({"correct": letter}));
            }
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
    sub(ui, tl!("Options"));
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
                let letter_label = ui.label(RichText::new(format!("{letter})")).monospace());
                if let Some(t) = draft_box(ui, ("quiz_rat_box", ex, k, j), letter_label.id, &cur, true, false) {
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

#[cfg(test)]
mod tests {
    use egui_kittest::kittest::Queryable;
    use serde_json::{Value, json};
    use wordcraft_engine::Session;

    use crate::WordApp;

    /// Found in review: a pair with an `=` in its left side (`F = m·a`) came back cut at that
    /// `=` whenever any line of the Pairs box was edited.
    #[test]
    fn a_pair_with_an_equals_sign_reads_back_as_it_was() {
        for (left, right) in [("Roma", "Italia"), ("F = m·a", "Seconda legge"), ("y", "x = 2"), ("a=b", "c=d")] {
            assert_eq!(super::pair_of(&super::pair_line(left, right)), Some((left, right)));
        }
        assert_eq!(super::pair_line("Roma", "Italia"), "Roma = Italia");
        assert_eq!(super::pair_of("  Parigi=Francia "), Some(("Parigi", "Francia")));
        assert_eq!(super::pair_of("no pair here"), None);
    }

    /// A quiz question's row: a click on its text shows and hides what is under it; its own
    /// settings on the row (the points, the type) are set there, and using them neither goes to
    /// the row nor is lost to it.
    #[test]
    fn a_row_opens_with_a_click_and_its_settings_are_set_on_it() {
        let mut app = WordApp::new(Session::new(wordcraft_doc::Document::from_text("Intro")), crate::Services::default());
        app.run("exercise.insert", json!({"kind": "open"})).unwrap();
        let state = |app: &mut WordApp| app.session.run("exercise.questions", &json!({})).unwrap();
        if state(&mut app)["questions"].as_array().is_none_or(Vec::is_empty) {
            app.run("exercise.addQuestion", json!({})).unwrap();
        }
        let ex = state(&mut app)["exercise"].as_u64().unwrap();
        let mut h =
            egui_kittest::Harness::builder().with_size(egui::vec2(600.0, 900.0)).build_ui_state(|ui, app: &mut WordApp| super::pane(app, ui), app);
        h.run_steps(3);
        let open = |h: &egui_kittest::Harness<'_, WordApp>| h.ctx.data(|d| d.get_temp::<bool>(super::open_id(ex, 0))).unwrap_or(false);
        let question = |h: &mut egui_kittest::Harness<'_, WordApp>| -> Value { state(h.state_mut())["questions"][0].clone() };
        // A click on the row's text shows the details, another hides them.
        let was = open(&h);
        let text: String = question(&mut h)["text"].as_str().unwrap_or_default().chars().take(60).collect();
        let text = if text.is_empty() { tl!("(question not written yet)").to_string() } else { text };
        h.get_by_label(&text).click();
        h.run_steps(3);
        assert_eq!(open(&h), !was, "the row's text toggles the details");
        if open(&h) {
            h.get_by_label(&text).click();
            h.run_steps(3);
        }
        assert!(!open(&h));
        // Shown, a click among the details (a label there) leaves them shown.
        h.get_by_label(&text).click();
        h.run_steps(3);
        assert!(open(&h));
        h.get_by_label(tl!("Height")).click();
        h.run_steps(3);
        assert!(open(&h), "a click among the details is not a click on the row");
        h.get_by_label(&text).click();
        h.run_steps(3);
        assert!(!open(&h));
        // The points, typed on the row: the row stays as it is.
        let points = h.query_all_by_role(egui::accesskit::Role::TextInput).next().expect("the points box");
        points.click();
        h.run_steps(3);
        assert!(!open(&h), "a click in the points box is not a click on the row");
        h.query_all_by_role(egui::accesskit::Role::TextInput).next().unwrap().type_text("3");
        h.run_steps(2);
        h.key_press(egui::Key::Tab);
        h.run_steps(3);
        assert_eq!(question(&mut h)["points"].as_f64(), Some(3.0), "{}", question(&mut h));
        // The type, chosen on the row.
        h.get_by_label(&format!("{}: {}", tl!("Question Type"), super::mode_name("open"))).click();
        h.run_steps(3);
        assert!(!open(&h), "a click on the type is not a click on the row");
        h.get_by_label(&super::mode_name("calc")).click();
        h.run_steps(3);
        assert_eq!(super::mode_of(&question(&mut h)), "calc", "{}", question(&mut h));
    }

    /// Card 3 (2026-11-27): under its row a question has only what its type uses — a true or
    /// false question no shuffling (ignored for it), an inline completion no answer space (it has
    /// none), a problem's question only its result — and every type says what happens.
    #[test]
    fn each_type_has_only_its_own_sections() {
        assert_eq!(super::sections("open"), ["Answer Space"]);
        assert_eq!(super::sections("worked"), ["Answer Space"]);
        assert_eq!(super::sections("calc"), ["Result", "Answer Space"]);
        assert_eq!(super::sections("drawing"), ["Drawing", "Answer Space"]);
        assert_eq!(super::sections("choice"), ["Correct Options"]);
        assert_eq!(super::sections("trueFalse"), ["Correct Answer"]);
        assert_eq!(super::sections("matching"), ["Pairs"]);
        assert!(super::sections("cloze").is_empty());
        assert_eq!(super::sections("problem"), ["Result"]);
        let helps: Vec<&str> = super::QUESTION_MODES.iter().map(|(id, _)| super::mode_help(id, true)).collect();
        for (k, h) in helps.iter().enumerate() {
            assert!(!helps[..k].contains(h), "{h}: every type explains itself");
        }
        assert_ne!(super::mode_help("problem", true), super::mode_help("open", true));
        // Found in review: a calculation or drawing with a blank space said it was squared.
        for mode in ["calc", "drawing"] {
            assert!(super::mode_help(mode, true).contains("squared"));
            assert!(!super::mode_help(mode, false).contains("squared"), "{mode}");
        }
    }

    /// Found in review: the type list and the settings' boxes had fixed widths on top of a
    /// sub-question's indent, so a narrow pane cut them off out of reach. They now shrink, and
    /// move in less, to fit.
    #[test]
    fn settings_fit_a_narrow_pane() {
        assert_eq!(super::form_widths(400.0), (super::LABEL_W, super::FIELD_W));
        for avail in [140.0_f32, 180.0, 220.0, 254.0] {
            let (label, field) = super::form_widths(avail);
            assert!(label + 8.0 + field <= avail + 0.5, "{avail}: {label} + {field}");
            assert!(label >= super::MIN_LABEL_W && field >= super::MIN_FIELD_W);
        }
        assert_eq!(super::form_widths(f32::NAN), (super::LABEL_W, super::FIELD_W));
        assert_eq!(super::inset(40.0, 600.0, 200.0), 40.0);
        assert_eq!(super::inset(80.0, 230.0, 200.0), 30.0);
        assert_eq!(super::inset(80.0, 150.0, 200.0), 0.0);
    }

    /// A worked example gives no points: its row says so instead of offering a box that is
    /// ignored; a true or false question's right answer is one of two, chosen with a click.
    #[test]
    fn worked_examples_have_no_points_box_and_true_false_picks_one_answer() {
        let mut app = WordApp::new(Session::new(wordcraft_doc::Document::from_text("Intro")), crate::Services::default());
        app.run(
            "exercise.insert",
            json!({"kind": "trueFalse", "questions": [{"text": "Il sole è una stella", "type": "trueFalse"}, {"text": "Esempio", "type": "worked"}]}),
        )
        .unwrap();
        let state = |app: &mut WordApp| app.session.run("exercise.questions", &json!({})).unwrap();
        let st = state(&mut app);
        assert_eq!(super::mode_of(&st["questions"][0]), "trueFalse", "{st}");
        assert_eq!(super::mode_of(&st["questions"][1]), "worked", "{st}");
        let ex = st["exercise"].as_u64().unwrap();
        let mut h =
            egui_kittest::Harness::builder().with_size(egui::vec2(600.0, 900.0)).build_ui_state(|ui, app: &mut WordApp| super::pane(app, ui), app);
        h.run_steps(3);
        assert_eq!(h.query_all_by_role(egui::accesskit::Role::TextInput).count(), 1, "one points box: the worked example has none");
        assert!(h.query_by_label(tl!("No points")).is_some());
        h.ctx.data_mut(|d| d.insert_temp(super::open_id(ex, 0), true));
        h.run_steps(3);
        assert!(h.query_by_label(tl!("Shuffle options per variant")).is_none(), "true or false is never shuffled");
        let pick = |h: &mut egui_kittest::Harness<'_, WordApp>, name: &str| {
            h.get_by_label(name).click();
            h.run_steps(3);
            state(h.state_mut())["questions"][0]["correct"].clone()
        };
        let names: Vec<String> =
            state(h.state_mut())["questions"][0]["choices"].as_array().unwrap().iter().map(|c| c.as_str().unwrap().to_string()).collect();
        assert_eq!(pick(&mut h, &names[1]), json!(["B"]));
        assert_eq!(pick(&mut h, &names[0]), json!(["A"]), "the other answer is no longer right");
    }

    #[test]
    fn sub_questions_sit_deeper_than_their_question() {
        assert_eq!(super::depth("a)"), 0);
        assert_eq!(super::depth("a.2)"), 1);
        assert_eq!(super::depth("a.2.1)"), 2);
        assert_eq!(super::depth(""), 0);
        assert_eq!(super::depth("1.1.1.1.1.1.1)"), 4);
    }
}
