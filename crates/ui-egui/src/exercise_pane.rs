//! The Exercise Properties pane (Math › Exercise › Properties, docs/exercise.md): the initial data
//! and the variants of the exercise at the caret. Every change goes through `exercise.setData` or
//! `exercise.variant`, so it is undoable and recalculates like any edit.

use egui::Ui;
use serde_json::{Value, json};
use strix_doc::control::ControlRange;

use crate::WordApp;
use crate::a11y_pane::draft_box;

/// Most variants offered (A…Z).
const MAX_VARIANTS: usize = 26;

/// The exercise control around the caret, when the caret is in one of its parts (`current`, the
/// innermost control there, is not the exercise itself): its frame and tab show too, and a click
/// on the tab selects the whole exercise.
pub(crate) fn enclosing_exercise(app: &WordApp, current: Option<&ControlRange>) -> Option<ControlRange> {
    let focus = &app.session.sel.focus;
    if focus.story != strix_doc::StoryRef::Body {
        return None;
    }
    app.session
        .doc
        .control_ranges(focus.story)
        .into_iter()
        .filter(|r| r.control.tag == strix_engine::cmd::exercise::TAG_EXERCISE && r.contains(focus))
        .find(|r| current.is_none_or(|c| c.start != r.start))
}

pub fn pane(app: &mut WordApp, ui: &mut Ui) {
    let Ok(info) = app.session.run("exercise.data", &json!({})) else {
        ui.label(egui::RichText::new(tl!("Put the cursor in an exercise to see its data and variants.")).weak());
        // The numbering is the document's: it shows wherever the cursor is, once there are exercises.
        let exercises = app.session.run("exercise.info", &json!({})).ok().and_then(|i| i["exercises"].as_u64()).unwrap_or(0);
        if exercises > 0 {
            ui.add_space(10.0);
            numbering(app, ui);
        }
        return;
    };
    let ex = info["exercise"].as_u64().unwrap_or(0);
    let mut data = info["data"].clone();
    let errors = info["errors"].as_array().cloned().unwrap_or_default();
    let variants: Vec<String> =
        data["variants"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
    let active = data["active"].as_u64().unwrap_or(0) as usize;
    let save = |app: &mut WordApp, data: &Value| {
        let _ = app.run("exercise.setData", json!({"exercise": ex, "data": data}));
    };

    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.label(egui::RichText::new(format!("{} {ex}", tl!("Exercise"))).strong());
        ui.add_space(6.0);
        // Variants: the active one is the one the exercise is calculated with.
        ui.label(tl!("Variants"));
        ui.horizontal_wrapped(|ui| {
            for (k, name) in variants.iter().enumerate() {
                if ui.selectable_label(k == active, name.as_str()).on_hover_text(tl!("Calculate the exercise with this variant")).clicked() && k != active {
                    let _ = app.run("exercise.variant", json!({"exercise": ex, "active": k}));
                }
            }
            let can_add = variants.len() < MAX_VARIANTS;
            if ui.add_enabled(can_add, egui::Button::new("+")).on_hover_text(tl!("Add a variant: a copy of the active one's values")).clicked() {
                let next = char::from(b'A' + (variants.len() % MAX_VARIANTS) as u8).to_string();
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
                save(app, &data);
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
                save(app, &data);
            }
        });
        ui.add_space(10.0);
        // Data: symbol, description, value in the active variant, unit.
        ui.label(tl!("Data"));
        let rows = data["items"].as_array().map_or(0, Vec::len);
        let mut remove = None;
        egui::Grid::new(("exercise_data", ex)).num_columns(5).spacing([4.0, 4.0]).striped(true).show(ui, |ui| {
            for head in ["Symbol", "Description", "Value", "Unit", ""] {
                ui.label(egui::RichText::new(if head.is_empty() { String::new() } else { tl!(head).to_string() }).small().weak());
            }
            ui.end_row();
            for row in 0..rows {
                let field = |data: &Value, key: &str| data["items"][row][key].as_str().unwrap_or_default().to_string();
                let value = data["items"][row]["values"][active].as_str().unwrap_or_default().to_string();
                let mut changed: Option<(&str, String)> = None;
                for (key, width, current) in [("symbol", 40.0, field(&data, "symbol")), ("description", 90.0, field(&data, "description"))] {
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
                    if let Some(t) = draft_box(ui, ("exercise_data", ex, row, "unit"), &field(&data, "unit"), true, false) {
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
                    save(app, &data);
                }
            }
        });
        if let Some(row) = remove
            && let Some(items) = data["items"].as_array_mut()
            && row < items.len()
        {
            items.remove(row);
            save(app, &data);
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
            save(app, &data);
        }
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(tl!("The data are the first parameters of the exercise's calculations: the solution uses them by symbol. Changing the variant recalculates the exercise."))
                .small()
                .weak(),
        );
        ui.add_space(10.0);
        ui.separator();
        numbering(app, ui);
    });
}

/// The number formats offered for exercises: the format's id and how it counts.
const FORMATS: [(&str, &str); 5] =
    [("decimal", "1, 2, 3"), ("upperLetter", "A, B, C"), ("lowerLetter", "a, b, c"), ("upperRoman", "I, II, III"), ("lowerRoman", "i, ii, iii")];

/// Exercise numbering, a document setting (`exercise.numbering`): the prefix, number format and
/// first number of every exercise's label, in the titles and the results summary.
fn numbering(app: &mut WordApp, ui: &mut Ui) {
    let Ok(n) = app.session.run("exercise.numbering", &json!({})) else { return };
    let prefix = n["prefix"].as_str().unwrap_or_default().to_string();
    let format = n["format"].as_str().unwrap_or("decimal").to_string();
    let start = n["start"].as_u64().unwrap_or(1);
    let set = |app: &mut WordApp, params: Value| {
        if let Err(e) = app.run("exercise.numbering", params) {
            app.status(e);
        }
    };
    ui.label(egui::RichText::new(tl!("Numbering")).strong());
    ui.label(egui::RichText::new(tl!("For every exercise of the document: the label of each title and of the results summary.")).small().weak());
    ui.add_space(4.0);
    egui::Grid::new("exercise_numbering").num_columns(2).spacing([6.0, 4.0]).show(ui, |ui| {
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
