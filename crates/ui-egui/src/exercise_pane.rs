//! The Exercise Properties pane (Teaching › Exercise › Properties, docs/exercise.md): the initial data
//! and the variants of the exercise at the caret. Every change goes through `exercise.setData` or
//! `exercise.variant`, so it is undoable and recalculates like any edit.

use egui::Ui;
use serde_json::{Value, json};
use wordcraft_doc::control::ControlRange;

use crate::WordApp;
use crate::a11y_pane::draft_box;

/// Most variants offered (A…Z).
const MAX_VARIANTS: usize = 26;

/// The exercise control around the caret, when the caret is in one of its parts (`current`, the
/// innermost control there, is not the exercise itself): its frame and tab show too, and a click
/// on the tab selects the whole exercise.
pub(crate) fn enclosing_exercise(app: &WordApp, current: Option<&ControlRange>) -> Option<ControlRange> {
    let focus = &app.session.sel.focus;
    if focus.story != wordcraft_doc::StoryRef::Body {
        return None;
    }
    app.session
        .doc
        .control_ranges(focus.story)
        .into_iter()
        .filter(|r| r.control.tag == wordcraft_engine::cmd::exercise::TAG_EXERCISE && r.contains(focus))
        .find(|r| current.is_none_or(|c| c.start != r.start))
}

/// The pane's two tabs in a series: this exercise's own settings, and the series' (every
/// exercise's). A lone exercise has no Series tab: what of it applies follows in Exercise.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Exercise,
    Series,
}

pub fn pane(app: &mut WordApp, ui: &mut Ui) {
    let info = app.session.run("exercise.data", &json!({})).ok();
    let exercises = app.session.run("exercise.info", &json!({})).ok().and_then(|i| i["exercises"].as_u64()).unwrap_or(0);
    let hint = |ui: &mut Ui| {
        ui.label(egui::RichText::new(tl!("Put the cursor in an exercise to see its data and variants.")).weak());
    };
    if exercises < 2 {
        match info {
            Some(info) => {
                egui::ScrollArea::vertical().show(ui, |ui| exercise_tab(app, ui, &info, true));
            }
            None => hint(ui),
        }
        return;
    }
    // The tab chosen stays chosen; until one is, Exercise with the cursor in one, else Series.
    let key = egui::Id::unique("exercise_pane_tab");
    let chosen = ui.data(|d| d.get_temp::<bool>(key));
    let mut tab = match chosen {
        Some(true) => Tab::Series,
        Some(false) => Tab::Exercise,
        None if info.is_some() => Tab::Exercise,
        None => Tab::Series,
    };
    let names = [tl!("Exercise").to_string(), tl!("Series").to_string()];
    if let Some(k) = crate::widgets::dialog_tabs(ui, &names, usize::from(tab == Tab::Series)) {
        tab = if k == 1 { Tab::Series } else { Tab::Exercise };
        ui.data_mut(|d| d.insert_temp(key, tab == Tab::Series));
    }
    ui.add_space(6.0);
    egui::ScrollArea::vertical().show(ui, |ui| match (tab, info) {
        (Tab::Exercise, Some(info)) => exercise_tab(app, ui, &info, false),
        (Tab::Exercise, None) => hint(ui),
        (Tab::Series, _) => series_tab(app, ui),
    });
}

/// The series' settings, the same for every exercise: numbering, parts shown, page breaks and
/// the frontispiece. They show wherever the cursor is.
fn series_tab(app: &mut WordApp, ui: &mut Ui) {
    crate::pane_kit::note(
        ui,
        tl!(
            "Every exercise of the series has the same settings, the first exercise's: a change applies to all of them, and a new exercise takes them."
        ),
    );
    crate::pane_kit::section(ui, "01", tl!("Numbering"));
    numbering(app, ui);
    crate::pane_kit::section(ui, "02", tl!("Parts Shown"));
    parts(app, ui, 0);
    crate::pane_kit::section(ui, "03", tl!("Page Breaks"));
    series_breaks(app, ui);
    crate::pane_kit::section(ui, "04", tl!("Frontispiece"));
    frontispiece(app, ui);
}

/// The exercise at the caret (`info`, from `exercise.data`): its variants and data.
/// `lone`: the document's only exercise, so the settings a series would rule (page breaks, parts
/// shown, frontispiece) follow here.
fn exercise_tab(app: &mut WordApp, ui: &mut Ui, info: &Value, lone: bool) {
    let ex = info["exercise"].as_u64().unwrap_or(0);
    let mut data = info["data"].clone();
    let errors = info["errors"].as_array().cloned().unwrap_or_default();
    let variants: Vec<String> =
        data["variants"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
    let active = data["active"].as_u64().unwrap_or(0) as usize;
    let save = |app: &mut WordApp, data: &Value| {
        let _ = app.run("exercise.setData", json!({"exercise": ex, "data": data}));
    };

    {
        // A lone exercise has no number.
        let title = if lone { tl!("Exercise").to_string() } else { format!("{} {ex}", tl!("Exercise")) };
        ui.label(egui::RichText::new(title).size(15.0).strong());
        if let Some(st) = app.session.run("exercise.questions", &json!({"exercise": ex})).ok().filter(|st| st["kind"] != "problem") {
            crate::pane_kit::section(ui, "01", tl!("Questions"));
            crate::exercise_quiz::questions(app, ui, ex, &st);
            let needs_data = st["questions"].as_array().is_some_and(|qs| qs.iter().any(|q| matches!(q["type"].as_str(), Some("calc" | "drawing"))))
                || data["items"].as_array().is_some_and(|a| !a.is_empty())
                || variants.len() > 1;
            ui.add_space(6.0);
            egui::CollapsingHeader::new(tl!("Variants & Data")).id_salt(("quiz_variants_data", ex, needs_data)).default_open(needs_data).show(
                ui,
                |ui| {
                    variants_and_data(app, ui, ex, &mut data, &variants, active, &errors, "02", "03", &save);
                },
            );
            if lone {
                crate::pane_kit::section(ui, "04", tl!("Page Breaks"));
                page_breaks(app, ui);
                crate::pane_kit::section(ui, "05", tl!("Parts Shown"));
                parts(app, ui, ex);
                crate::pane_kit::section(ui, "06", tl!("Frontispiece"));
                frontispiece(app, ui);
            } else {
                ui.add_space(6.0);
                crate::pane_kit::note(ui, tl!("Page breaks, parts shown and graph paper are the series': see the Series tab."));
            }
            return;
        }
        variants_and_data(app, ui, ex, &mut data, &variants, active, &errors, "01", "02", &save);
        let q_st = app.session.run("exercise.questions", &json!({"exercise": ex})).ok();
        let has_q = q_st.as_ref().and_then(|st| st["questions"].as_array()).is_some_and(|a| !a.is_empty());
        if let Some(st) = q_st.as_ref().filter(|_| has_q) {
            crate::pane_kit::section(ui, "03", tl!("Questions"));
            crate::exercise_quiz::problem_questions(app, ui, ex, st);
        }
        if lone {
            let (s1, s2, s3) = if has_q { ("04", "05", "06") } else { ("03", "04", "05") };
            crate::pane_kit::section(ui, s1, tl!("Page Breaks"));
            page_breaks(app, ui);
            crate::pane_kit::section(ui, s2, tl!("Parts Shown"));
            parts(app, ui, ex);
            crate::pane_kit::section(ui, s3, tl!("Frontispiece"));
            frontispiece(app, ui);
        } else {
            ui.add_space(6.0);
            crate::pane_kit::note(ui, tl!("Page breaks, parts shown and graph paper are the series': see the Series tab."));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn variants_and_data(
    app: &mut WordApp,
    ui: &mut Ui,
    ex: u64,
    data: &mut Value,
    variants: &[String],
    active: usize,
    errors: &[Value],
    sec_var: &str,
    sec_data: &str,
    save: &dyn Fn(&mut WordApp, &Value),
) {
    // Variants: the active one is the one the exercise is calculated with.
    crate::pane_kit::section(ui, sec_var, tl!("Variants"));
    ui.horizontal_wrapped(|ui| {
        for (k, name) in variants.iter().enumerate() {
            if ui.selectable_label(k == active, name.as_str()).on_hover_text(tl!("Calculate the exercise with this variant")).clicked() && k != active
            {
                let _ = app.run("exercise.variant", json!({"exercise": ex, "active": k}));
            }
        }
        let can_add = variants.len() < MAX_VARIANTS;
        if ui.add_enabled(can_add, egui::Button::new("+")).on_hover_text(tl!("Add a variant: a copy of the active one's values")).clicked() {
            // The first letter no variant has yet (after A, B removed: A again, not B twice).
            let taken = |c: &String| variants.iter().any(|v| v == c) || (variants.is_empty() && c == "A");
            let next = (b'A'..=b'Z').map(|b| char::from(b).to_string()).find(|c| !taken(c)).unwrap_or_else(|| format!("{}", variants.len() + 1));
            if let Some(names) = data["variants"].as_array_mut() {
                names.push(json!(next));
            } else {
                data["variants"] = json!(["A", next]);
            }
            if let Some(items) = data["items"].as_array_mut() {
                for item in items {
                    let copy = item["values"].get(active).cloned().unwrap_or(json!(""));
                    if let Some(values) = item["values"].as_array_mut() {
                        values.push(copy);
                    }
                }
            }
            data["active"] = json!(variants.len().max(1));
            save(app, data);
        }
        if ui.add_enabled(variants.len() > 1, egui::Button::new("−")).on_hover_text(tl!("Remove the active variant")).clicked() {
            if let Some(names) = data["variants"].as_array_mut()
                && active < names.len()
            {
                names.remove(active);
            }
            if let Some(items) = data["items"].as_array_mut() {
                for item in items {
                    if let Some(values) = item["values"].as_array_mut()
                        && active < values.len()
                    {
                        values.remove(active);
                    }
                }
            }
            data["active"] = json!(active.saturating_sub(1));
            save(app, data);
        }
    });
    // Data: symbol, description, value in the active variant, unit.
    crate::pane_kit::section(ui, sec_data, tl!("Data"));
    let mut show_data = data["showData"].as_bool().unwrap_or(true);
    if ui
        .checkbox(&mut show_data, tl!("Show data in exercise"))
        .on_hover_text(tl!("Print the Data block in the exercise; off, the values stay in the calculation sandbox only"))
        .changed()
    {
        data["showData"] = json!(show_data);
        save(app, data);
    }
    let rows = data["items"].as_array().map_or(0, Vec::len);
    let mut remove = None;
    crate::rtl::grid(ui, egui::Grid::new(("exercise_data", ex)).num_columns(5).spacing([4.0, 4.0]).striped(true), |ui| {
        for head in ["Symbol", "Description", "Value", "Unit", ""] {
            ui.label(egui::RichText::new(if head.is_empty() { String::new() } else { tl!(head).to_string() }).small().weak());
        }
        ui.end_row();
        for row in 0..rows {
            let field = |data: &Value, key: &str| data["items"][row][key].as_str().unwrap_or_default().to_string();
            let value = data["items"][row]["values"][active].as_str().unwrap_or_default().to_string();
            let mut changed: Option<(&str, String)> = None;
            for (key, width, current) in [("symbol", 40.0, field(data, "symbol")), ("description", 90.0, field(data, "description"))] {
                ui.scope(|ui| {
                    ui.set_width(width);
                    if let Some(t) = draft_box(ui, ("exercise_data", ex, row, key), &current, true, false) {
                        changed = Some((key, t));
                    }
                });
            }
            ui.scope(|ui| {
                ui.set_width(56.0);
                if let Some(t) = draft_box(ui, ("exercise_data", ex, row, "value", active), &value, true, false) {
                    changed = Some(("value", t));
                }
            });
            ui.scope(|ui| {
                ui.set_width(58.0);
                if let Some(t) = draft_box(ui, ("exercise_data", ex, row, "unit"), &field(data, "unit"), true, false) {
                    changed = Some(("unit", t));
                }
            });
            if ui.small_button("×").on_hover_text(tl!("Remove this datum")).clicked() {
                remove = Some(row);
            }
            ui.end_row();
            if let Some(error) = errors.get(row).and_then(Value::as_str) {
                ui.label("");
                ui.colored_label(ui.visuals().error_fg_color, egui::RichText::new(error).small());
                ui.end_row();
            }
            if let Some((key, text)) = changed {
                if key == "value" {
                    data["items"][row]["values"][active] = json!(text);
                } else {
                    data["items"][row][key] = json!(text);
                }
                save(app, data);
            }
        }
    });
    if let Some(row) = remove
        && let Some(items) = data["items"].as_array_mut()
        && row < items.len()
    {
        items.remove(row);
        save(app, data);
    }
    ui.add_space(6.0);
    if ui.button(tl!("Add Datum")).on_hover_text(tl!("Add an initial datum: a symbol with its value in every variant")).clicked() {
        let n = variants.len().max(1);
        let item = json!({"symbol": "", "description": "", "unit": "", "values": vec![""; n]});
        match data["items"].as_array_mut() {
            Some(items) => items.push(item),
            None => data["items"] = json!([item]),
        }
        if data["variants"].as_array().is_none_or(Vec::is_empty) {
            data["variants"] = json!(["A"]);
        }
        save(app, data);
    }
    ui.add_space(6.0);
    crate::pane_kit::note(
        ui,
        tl!(
            "The data are the first parameters of the exercise's calculations: the solution uses them by symbol. Changing the variant recalculates the exercise."
        ),
    );
}

/// The parts shown and the graph paper (`exercise.show`, `exercise.grid`): of this exercise, or
/// of every one in a series (the series rules). `ex` 0: the series, with the cursor anywhere.
fn parts(app: &mut WordApp, ui: &mut Ui, ex: u64) {
    let target = if ex > 0 { json!({"exercise": ex}) } else { json!({}) };
    let Ok(st) = app.session.run("exercise.show", &target) else { return };
    let with = |mut v: Value, k: &str, on: bool| {
        if ex > 0 {
            v["exercise"] = json!(ex);
        }
        v[k] = json!(on);
        v
    };
    let run = |app: &mut WordApp, id: &str, params: Value| {
        if let Err(e) = app.run(id, params) {
            app.status(e);
        }
    };
    let mut solution = st["solution"].as_bool().unwrap_or(true);
    if ui.checkbox(&mut solution, tl!("Solution")).on_hover_text(tl!("The working under each question; hidden, it stays in the document")).changed() {
        run(app, "exercise.show", with(json!({}), "solution", solution));
    }
    let mut results = st["results"].as_bool().unwrap_or(true);
    if ui.checkbox(&mut results, tl!("Results")).on_hover_text(tl!("The exercise's lines in the results summary")).changed() {
        run(app, "exercise.show", with(json!({}), "results", results));
    }
    let grid = st["grid"].clone();
    let mut ruled = !grid.is_null();
    if ui.checkbox(&mut ruled, tl!("Graph paper")).on_hover_text(tl!("Graph paper after the questions, before the solution")).changed() {
        run(app, "exercise.grid", with(json!({}), "on", ruled));
    }
    if !grid.is_null() {
        ui.indent("exercise_grid", |ui| {
            if let Some(mut change) = crate::graph_pane::editor(ui, &grid, false) {
                if ex > 0 {
                    change["exercise"] = json!(ex);
                }
                run(app, "exercise.grid", change);
            }
        });
    }
    // In a series the Series tab says already that these are every exercise's.
    crate::pane_kit::note(ui, tl!("Hidden parts stay in the document as hidden text."));
}

/// The frontispiece of the series (`exercise.frontispiece`).
fn frontispiece(app: &mut WordApp, ui: &mut Ui) {
    let Ok(f) = app.session.run("exercise.frontispiece", &json!({})) else { return };
    let mut on = f["on"].as_bool().unwrap_or(false);
    if ui
        .checkbox(&mut on, tl!("Frontispiece"))
        .on_hover_text(tl!("Pages of your own before the exercises: title, class, date, instructions"))
        .changed()
        && let Err(e) = app.run("exercise.frontispiece", json!({"on": on}))
    {
        app.status(e);
    }
    crate::pane_kit::note(
        ui,
        tl!("Everything before the Frontispiece Break is the frontispiece: write there, as many pages as you need. Show/Hide ¶ shows the break."),
    );
}

/// The lone exercise's page breaks (`exercise.pageBreaks`): before its solution and before the
/// results summary (no exercise comes before it).
fn page_breaks(app: &mut WordApp, ui: &mut Ui) {
    let Ok(b) = app.session.run("exercise.pageBreaks", &json!({})) else { return };
    let mut solution = b["beforeSolution"].as_bool().unwrap_or(false);
    if ui.checkbox(&mut solution, tl!("New page before the solution")).changed() {
        set_breaks(app, json!({"beforeSolution": solution}));
    }
    let mut results = b["beforeResults"].as_bool().unwrap_or(false);
    if ui.checkbox(&mut results, tl!("New page before the results summary")).changed() {
        set_breaks(app, json!({"beforeResults": results}));
    }
}

/// The series' page breaks (`exercise.pageBreaks`; the series rules, so every exercise has the
/// first one's): before every exercise but the first, before every solution, before the results
/// summary.
fn series_breaks(app: &mut WordApp, ui: &mut Ui) {
    let Ok(b) = app.session.run("exercise.pageBreaks", &json!({})) else { return };
    for (key, label, hover) in [
        ("beforeExercise", tl!("New page before each exercise"), tl!("Every exercise but the first starts a new page")),
        ("beforeSolution", tl!("New page before each solution"), tl!("Each exercise's solution starts a new page")),
        ("beforeResults", tl!("New page before the results summary"), ""),
    ] {
        let mut on = b[key].as_bool().unwrap_or(false);
        let r = ui.checkbox(&mut on, label);
        let r = if hover.is_empty() { r } else { r.on_hover_text(hover) };
        if r.changed() {
            set_breaks(app, json!({key: on}));
        }
    }
}

fn set_breaks(app: &mut WordApp, params: Value) {
    if let Err(e) = app.run("exercise.pageBreaks", params) {
        app.status(e);
    }
}

/// The number formats offered for exercises: the format's id and how it counts.
const FORMATS: [(&str, &str); 5] =
    [("decimal", "1, 2, 3"), ("upperLetter", "A, B, C"), ("lowerLetter", "a, b, c"), ("upperRoman", "I, II, III"), ("lowerRoman", "i, ii, iii")];

/// The separators offered after an exercise's number (`exercise.numbering`'s ids) and their names.
const SEPARATORS: [(&str, &str); 5] =
    [(".", "Period (1.)"), (":", "Colon (1:)"), ("-", "Dash (1 -)"), (")", "Parenthesis (1))"), (" ", "Space (1 Title)")];

/// Exercise numbering, a document setting (`exercise.numbering`): the prefix, number format,
/// separator and first number of every exercise's label, in the titles and the results summary.
fn numbering(app: &mut WordApp, ui: &mut Ui) {
    let Ok(n) = app.session.run("exercise.numbering", &json!({})) else { return };
    let prefix = n["prefix"].as_str().unwrap_or_default().to_string();
    let format = n["format"].as_str().unwrap_or("decimal").to_string();
    let start = n["start"].as_u64().unwrap_or(1);
    let separator = n["separator"].as_str().unwrap_or(".").to_string();
    let set = |app: &mut WordApp, params: Value| {
        if let Err(e) = app.run("exercise.numbering", params) {
            app.status(e);
        }
    };
    crate::pane_kit::note(ui, tl!("For every exercise of the document: the label of each title and of the results summary."));
    ui.add_space(4.0);
    crate::rtl::grid(ui, egui::Grid::new("exercise_numbering").num_columns(2).spacing([6.0, 4.0]), |ui| {
        ui.label(tl!("Prefix"));
        ui.horizontal(|ui| {
            ui.scope(|ui| {
                ui.set_width(110.0);
                if let Some(t) = draft_box(ui, "exercise_numbering_prefix", &prefix, true, false) {
                    set(app, json!({"prefix": t}));
                }
            });
            let default = n["default"].as_bool().unwrap_or(false);
            if ui
                .add_enabled(!default, egui::Button::new(tl!("Default")).small())
                .on_hover_text(tl!("Exercise, in the document's language"))
                .clicked()
            {
                set(app, json!({"prefix": null}));
            }
        });
        ui.end_row();
        ui.label(tl!("Number format"));
        let shown = FORMATS.iter().find(|(id, _)| *id == format).map_or("1, 2, 3", |(_, l)| l);
        craft_ui::ComboBox::from_id_salt("exercise_numbering_format").selected_text(shown).show_ui(ui, |ui| {
            for (id, label) in FORMATS {
                if ui.selectable_label(id == format, label).clicked() && id != format {
                    set(app, json!({"format": id}));
                }
            }
        });
        ui.end_row();
        ui.label(tl!("Separator"));
        let shown = SEPARATORS.iter().find(|(id, _)| *id == separator).map_or(separator.clone(), |(_, l)| tl!(l).to_string());
        craft_ui::ComboBox::from_id_salt("exercise_numbering_separator").selected_text(shown).show_ui(ui, |ui| {
            for (id, label) in SEPARATORS {
                if ui.selectable_label(id == separator, tl!(label)).clicked() && id != separator {
                    set(app, json!({"separator": id}));
                }
            }
        });
        ui.end_row();
        ui.label(tl!("Start at"));
        let mut value = start;
        if ui.add(egui::DragValue::new(&mut value).range(1..=32767)).changed() && value != start {
            set(app, json!({"start": value}));
        }
        ui.end_row();
    });
    let example = n["example"].as_str().unwrap_or_default();
    ui.label(egui::RichText::new(crate::i18n::fmt(tl!("First exercise: {label}"), &[("label", example)])).small().weak());
}
