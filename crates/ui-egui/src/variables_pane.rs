//! The Variables pane (Math › Calculation › Variables, `docs/calc.md`): inspects every live
//! calculation parameter in the document's main worksheet and in each exercise's isolated sandbox,
//! jumps to the defining region on click, and converts displayed quantities across compatible units
//! on the fly.

use std::collections::BTreeMap;

use egui::{RichText, Ui};
use serde_json::{Value, json};

use crate::WordApp;
use crate::pane_kit::{self, Status};
use crate::theme::Tokens;

#[derive(Clone, Default)]
struct PaneUiState {
    filter: String,
    units: BTreeMap<String, String>,
}

fn load_state(ui: &Ui, id: egui::Id) -> PaneUiState {
    ui.ctx().data(|d| d.get_temp::<PaneUiState>(id)).unwrap_or_default()
}

fn save_state(ui: &Ui, id: egui::Id, state: PaneUiState) {
    ui.ctx().data_mut(|d| d.insert_temp(id, state));
}

pub fn pane(app: &mut WordApp, ui: &mut Ui) {
    let state_id = app.window_id("variables_pane");
    let mut state = load_state(ui, state_id);
    ui.horizontal(|ui| {
        let has_filter = !state.filter.is_empty();
        ui.add(
            egui::TextEdit::singleline(&mut state.filter)
                .hint_text(tl!("Filter variables…"))
                .desired_width((ui.available_width() - if has_filter { 52.0 } else { 0.0 }).max(80.0)),
        );
        if !state.filter.is_empty() && ui.small_button(tl!("Clear")).clicked() {
            state.filter.clear();
        }
    });
    let unit_json: serde_json::Map<String, Value> = state.units.iter().map(|(k, v)| (k.clone(), Value::String(v.clone()))).collect();
    let res = app.session.run("calc.variables", &json!({"units": unit_json})).unwrap_or_default();
    let doc_vars = res["variables"].as_array().cloned().unwrap_or_default();
    let exercises = res["exercises"].as_array().cloned().unwrap_or_default();
    let needle = state.filter.trim().to_lowercase();
    egui::ScrollArea::vertical().show(ui, |ui| {
        if doc_vars.is_empty() && exercises.is_empty() {
            pane_kit::section(ui, "01", tl!("Document"));
            pane_kit::note(ui, tl!("No variables defined yet. Insert a live calculation (such as L = 4.5 m) or exercise data to inspect them here."));
            return;
        }
        if !doc_vars.is_empty() {
            pane_kit::section(ui, "01", tl!("Document"));
            render_variable_list(app, ui, "document", &doc_vars, &needle, &mut state);
        }
        for (idx, ex) in exercises.iter().enumerate() {
            let ex_num = ex["exercise"].as_u64().unwrap_or(1);
            let vars = ex["variables"].as_array().map(Vec::as_slice).unwrap_or_default();
            let filtered: Vec<&Value> = vars.iter().filter(|v| matches_filter(v, &needle)).collect();
            if filtered.is_empty() && !needle.is_empty() {
                continue;
            }
            let sec_num = format!("{:02}", idx + if doc_vars.is_empty() { 1 } else { 2 });
            let title = ex["title"].as_str().unwrap_or_default();
            let heading = if title.is_empty() {
                crate::i18n::fmt(tl!("Exercise {n}"), &[("n", &ex_num.to_string())])
            } else {
                format!("{} — {title}", crate::i18n::fmt(tl!("Exercise {n}"), &[("n", &ex_num.to_string())]))
            };
            pane_kit::section(ui, &sec_num, &heading);
            if let Some(variant) = ex["variant"].as_str().filter(|s| !s.is_empty()) {
                ui.horizontal(|ui| {
                    pane_kit::badge(ui, &crate::i18n::fmt(tl!("Variant {v}"), &[("v", variant)]), Status::Changed);
                });
                ui.add_space(4.0);
            }
            let scope_key = format!("exercise:{ex_num}");
            render_variable_list(app, ui, &scope_key, vars, &needle, &mut state);
        }
    });
    save_state(ui, state_id, state);
}

fn matches_filter(var: &Value, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let name = var["name"].as_str().unwrap_or_default().to_lowercase();
    let conv = var["converted"].as_str().unwrap_or_default().to_lowercase();
    let unit = var["unit"].as_str().unwrap_or_default().to_lowercase();
    name.contains(needle) || conv.contains(needle) || unit.contains(needle)
}

fn render_variable_list(app: &mut WordApp, ui: &mut Ui, scope_key: &str, vars: &[Value], needle: &str, state: &mut PaneUiState) {
    let mut any = false;
    for var in vars {
        if !matches_filter(var, needle) {
            continue;
        }
        any = true;
        variable_card(app, ui, scope_key, var, state);
        ui.add_space(5.0);
    }
    if !any {
        pane_kit::note(ui, tl!("No matching variables."));
    }
}

fn variable_card(app: &mut WordApp, ui: &mut Ui, scope_key: &str, var: &Value, state: &mut PaneUiState) {
    let t = Tokens::get(ui.ctx());
    let name = var["name"].as_str().unwrap_or_default();
    let full_key = format!("{scope_key}:{name}");
    let default_unit = var["unit"].as_str().unwrap_or_default();
    let selected_unit = var["selectedUnit"].as_str().unwrap_or(default_unit);
    let conv_text = var["convertedText"].as_str().unwrap_or_else(|| var["text"].as_str().unwrap_or_default());
    let shown = var["shown"].as_str().unwrap_or_default();
    let compat: Vec<&str> = var["compatibleUnits"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    pane_kit::card(ui, |ui| {
        ui.horizontal(|ui| {
            let jump =
                ui.add(egui::Button::new(RichText::new(name).font(pane_kit::mono(12.5)).strong().color(t.accent_text)).frame(false).truncate());
            if jump.on_hover_text(tl!("Go to definition")).clicked()
                && let Some(pos) = var.get("pos").filter(|p| p.is_object())
            {
                let _ = app.run("select.range", json!({"anchor": pos, "focus": pos}));
                app.canvas.want_focus = true;
            }
            ui.label(RichText::new("=").font(pane_kit::mono(11.5)).color(t.text_dim));
            if compat.len() > 1 {
                let val_label = ui.label(RichText::new(conv_text).font(pane_kit::mono(12.0)).color(t.text));
                let display_u = if selected_unit.is_empty() { "—" } else { selected_unit };
                let combo = craft_ui::ComboBox::from_id_salt(("var_unit", &full_key)).selected_text(display_u).width(78.0).show_ui(ui, |ui| {
                    if state.units.contains_key(&full_key) && ui.selectable_label(false, tl!("Auto")).clicked() {
                        state.units.remove(&full_key);
                    }
                    for &u in &compat {
                        let item_label = if u.is_empty() { "—" } else { u };
                        if ui.selectable_label(u == selected_unit, item_label).clicked() {
                            if u == default_unit {
                                state.units.remove(&full_key);
                            } else {
                                state.units.insert(full_key.clone(), u.to_string());
                            }
                        }
                    }
                });
                combo.response.labelled_by(val_label.id);
            } else {
                let text = var["converted"].as_str().unwrap_or(shown);
                ui.label(RichText::new(text).font(pane_kit::mono(12.0)).color(t.text));
            }
        });
        if selected_unit != default_unit && !shown.is_empty() {
            ui.label(RichText::new(format!("{} {shown}", tl!("Defined:"))).font(pane_kit::mono(10.0)).color(t.text_dim));
        }
    });
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use egui_kittest::kittest::Queryable;
    use serde_json::json;
    use wordcraft_engine::Session;

    use crate::{Services, WordApp};

    #[test]
    fn variables_pane_shows_document_and_sandbox_quantities_and_jumps_on_click() {
        let mut session = Session::new(wordcraft_doc::Document::from_text(""));
        session.run("calc.insert", &json!({"text": "L = 2.5 m"})).unwrap();
        session.run("text.newParagraph", &json!({})).unwrap();
        session.run("exercise.insert", &json!({"title": "Beam", "questions": ["Moment"]})).unwrap();
        session
            .run(
                "exercise.setData",
                &json!({"exercise": 1, "data": {"items": [{"symbol": "q", "unit": "kN/m", "values": ["12"]}], "variants": ["A"], "active": 0}}),
            )
            .unwrap();
        let app = WordApp::new(session, Services::default());
        let mut h =
            egui_kittest::Harness::builder().with_size(egui::vec2(340.0, 600.0)).build_ui_state(|ui, app: &mut WordApp| super::pane(app, ui), app);
        h.run_steps(4);
        assert!(h.query_by_label("L").is_some());
        assert!(h.query_by_label("q").is_some());
        h.get_by_label("L").click();
        h.run_steps(2);
        assert_eq!(h.state().session.sel.focus.path.0, vec![0]);
    }
}
