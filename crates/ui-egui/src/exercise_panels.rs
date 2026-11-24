//! The Exercise Data pane (Exercise › Calculation › Data, `view.dataPane`; docs/exercise.md): the
//! data of the exercise at the cursor in the active variant, with the variants added and removed;
//! and the variant generator the Exercise ribbon tab (`ribbon/exercise.rs`) shows in a drop-down
//! panel. Every change goes through `exercise.setData` or
//! `exercise.variant`, so it is undoable and recalculates like any edit.

use egui::Ui;
use serde_json::{Value, json};
use wordcraft_doc::control::ControlRange;

use crate::WordApp;
use crate::a11y_pane::draft_box;

/// Most variants offered (A…Z).
pub(crate) const MAX_VARIANTS: usize = 26;

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

/// Adds a variant to the exercise's `data`: a copy of the active one's values, under the first
/// letter no variant has yet (after A, B removed: A again, not B twice). It becomes the active one.
pub(crate) fn add_variant(data: &mut Value) {
    let variants: Vec<String> =
        data["variants"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
    if variants.len() >= MAX_VARIANTS {
        return;
    }
    let active = data["active"].as_u64().unwrap_or(0) as usize;
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
}

/// Removes the active variant from the exercise's `data` (never the last one).
pub(crate) fn remove_variant(data: &mut Value) {
    let count = data["variants"].as_array().map_or(0, Vec::len);
    if count < 2 {
        return;
    }
    let active = data["active"].as_u64().unwrap_or(0) as usize;
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
}

/// The name an exercise's tab shows on the page (`ctl`, the exercise control): "Exercise", and
/// the active variant while it has several, which the printed exercise never shows.
pub(crate) fn tab_title(ctl: &wordcraft_doc::control::ContentControl) -> String {
    match wordcraft_engine::cmd::exercise::active_variant(ctl) {
        Some(name) => format!("{} · {} {name}", tl!("Exercise"), tl!("Variant")),
        None => tl!("Exercise").to_string(),
    }
}

/// The variants of the exercise's data as a row of buttons: the active one is selected, a click
/// calculates the exercise with another one; then add (a copy of the active one) and remove.
fn variants_row(app: &mut WordApp, ui: &mut Ui, ex: u64, data: &Value, errors: &Value) {
    let variants: Vec<String> =
        data["variants"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
    let active = data["active"].as_u64().unwrap_or(0) as usize;
    let has_items = data["items"].as_array().is_some_and(|a| !a.is_empty());
    crate::widgets::row(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        ui.label(egui::RichText::new(tl!("Variants")).small().weak());
        let names = if variants.is_empty() { vec!["A".to_string()] } else { variants.clone() };
        for (k, name) in names.iter().enumerate() {
            let errs: Vec<&str> =
                errors.get(name.as_str()).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
            let text = if errs.is_empty() { name.clone() } else { format!("{name} ⚠") };
            let r = ui.selectable_label(k == active, egui::RichText::new(text).strong());
            let r = if errs.is_empty() {
                r.on_hover_text(tl!("Calculate the exercise with this variant"))
            } else {
                r.on_hover_text(format!("⚠ {}", errs.join("\n⚠ ")))
            };
            if r.clicked() && k != active && variants.len() > 1 {
                let _ = app.run("exercise.variant", json!({"exercise": ex, "active": k}));
            }
        }
        ui.add_space(4.0);
        let can_add = has_items && variants.len() < MAX_VARIANTS;
        if ui
            .add_enabled(can_add, egui::Button::new("+").small())
            .on_hover_text(tl!("Add a variant: a copy of the active one's values (add data first)"))
            .clicked()
        {
            let mut added = data.clone();
            add_variant(&mut added);
            let _ = app.run("exercise.setData", json!({"exercise": ex, "data": added}));
        }
        if ui.add_enabled(variants.len() > 1, egui::Button::new("−").small()).on_hover_text(tl!("Remove the active variant")).clicked() {
            let mut removed = data.clone();
            remove_variant(&mut removed);
            let _ = app.run("exercise.setData", json!({"exercise": ex, "data": removed}));
        }
    });
}

/// A datum's equation box: the text as typed while it has the focus; otherwise drawn like the
/// page sets it, the symbol in italics and the unit in the unit colour.
fn equation_box(ui: &mut Ui, key: impl std::hash::Hash + std::fmt::Debug, current: &str) -> Option<String> {
    let id = egui::Id::unique(key);
    let mut text = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| current.to_string());
    let t = crate::theme::Tokens::get(ui.ctx());
    let font = egui::TextStyle::Body.resolve(ui.style());
    let (ink, unit_ink) = (ui.visuals().text_color(), t.accent_text);
    let mut layouter = |ui: &Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
        let s = buf.as_str();
        let mut job = egui::text::LayoutJob::default();
        let plain = egui::TextFormat::simple(font.clone(), ink);
        let parts = s.split_once('=').map(|(lhs, rhs)| (lhs, rhs, wordcraft_engine::cmd::exercise::data_line::split_value_unit(rhs).1));
        match parts {
            Some((lhs, rhs, unit)) if !unit.is_empty() && rhs.trim_end().ends_with(unit.as_str()) => {
                let cut = rhs.trim_end().len() - unit.len();
                job.append(lhs, 0.0, egui::TextFormat { italics: true, ..plain.clone() });
                job.append("=", 0.0, plain.clone());
                job.append(rhs.get(..cut).unwrap_or_default(), 0.0, plain.clone());
                job.append(rhs.get(cut..).unwrap_or_default(), 0.0, egui::TextFormat::simple(font.clone(), unit_ink));
            }
            Some((lhs, rhs, _)) => {
                job.append(lhs, 0.0, egui::TextFormat { italics: true, ..plain.clone() });
                job.append("=", 0.0, plain.clone());
                job.append(rhs, 0.0, plain.clone());
            }
            None => job.append(s, 0.0, plain.clone()),
        }
        job.wrap.max_width = wrap;
        ui.fonts_mut(|f| f.layout_job(job))
    };
    let r = ui.add(egui::TextEdit::singleline(&mut text).hint_text("L = 4.5 m").desired_width(f32::INFINITY).layouter(&mut layouter));
    if r.changed() {
        ui.data_mut(|d| d.insert_temp(id, text.clone()));
    }
    if r.lost_focus() {
        ui.data_mut(|d| d.remove::<String>(id));
        return (text != current).then_some(text);
    }
    if !r.has_focus() {
        ui.data_mut(|d| d.remove::<String>(id));
    }
    None
}

/// The Exercise Data pane: the data and variants of the exercise at the cursor, a note outside
/// the exercises.
pub(crate) fn pane(app: &mut WordApp, ui: &mut Ui) {
    let Ok(info) = app.session.run("exercise.data", &json!({})) else {
        crate::pane_kit::note(ui, tl!("Put the cursor in an exercise to see its data and variants."));
        return;
    };
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| data_panel(app, ui, &info));
}

/// The exercise's data (`info`, from `exercise.data`): its variants, then one row per datum, its
/// description and then the datum written whole as an equation in the active variant (`L = 4.5 m`).
/// The data part on the page lists what is not already shown elsewhere in the exercise, and can
/// be edited there too.
fn data_panel(app: &mut WordApp, ui: &mut Ui, info: &Value) {
    let ex = info["exercise"].as_u64().unwrap_or(0);
    let mut data = info["data"].clone();
    let errors = info["errors"].as_array().cloned().unwrap_or_default();
    let variants = data["variants"].as_array().map_or(0, Vec::len);
    let active = data["active"].as_u64().unwrap_or(0) as usize;
    let names: Vec<String> =
        data["variants"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
    let data = &mut data;
    let save = |app: &mut WordApp, data: &Value| {
        let _ = app.run("exercise.setData", json!({"exercise": ex, "data": data}));
    };
    variants_row(app, ui, ex, data, &info["variantErrors"]);
    ui.add_space(4.0);
    let rows = data["items"].as_array().map_or(0, Vec::len);
    let mut remove = None;
    crate::rtl::grid(ui, egui::Grid::new(("exercise_data", ex)).num_columns(3).spacing([4.0, 4.0]).striped(true), |ui| {
        let description = ["Description", "Equation", ""]
            .map(|head| ui.label(egui::RichText::new(if head.is_empty() { String::new() } else { tl!(head).to_string() }).small().weak()).id)[0];
        ui.end_row();
        for row in 0..rows {
            let item = &data["items"][row];
            let text = |v: &Value| v.as_str().unwrap_or_default().to_string();
            let (symbol, unit) = (text(&item["symbol"]), text(&item["unit"]));
            let value = text(&item["values"][active]);
            let eq = if symbol.is_empty() && value.is_empty() && unit.is_empty() {
                String::new()
            } else {
                format!("{symbol} = {value} {unit}").split_whitespace().collect::<Vec<_>>().join(" ")
            };
            let mut changed: Option<(&str, String)> = None;
            ui.scope(|ui| {
                ui.set_width(130.0);
                if let Some(t) = draft_box(ui, ("exercise_data", ex, row, "description"), description, &text(&item["description"]), true, false) {
                    changed = Some(("description", t));
                }
            });
            let r = ui.scope(|ui| {
                ui.set_width(150.0);
                if let Some(t) = equation_box(ui, ("exercise_data", ex, row, "equation", active), &eq) {
                    changed = Some(("equation", t));
                }
            });
            // The values in the other variants, on hover.
            if names.len() > 1 {
                let all: Vec<String> = names
                    .iter()
                    .enumerate()
                    .map(|(k, n)| format!("{n}: {} {unit}", data["items"][row]["values"][k].as_str().unwrap_or("—")).trim().to_string())
                    .collect();
                r.response.on_hover_text(all.join("\n"));
            }
            if ui.small_button("×").on_hover_text(tl!("Remove this datum")).clicked() {
                remove = Some(row);
            }
            ui.end_row();
            if let Some(error) = errors.get(row).and_then(Value::as_str) {
                ui.colored_label(ui.visuals().error_fg_color, egui::RichText::new(error).small());
                ui.end_row();
            }
            if let Some((key, text)) = changed {
                data["items"][row][key] = json!(text);
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
    if ui.button(tl!("Add Datum")).on_hover_text(tl!("Add a datum: an equation such as L = 4.5 m, with its value in every variant")).clicked() {
        let n = variants.max(1);
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
            "Each datum is an equation (L = 4.5 m): the solution uses it by its symbol. The exercise lists the data it doesn't already show, and they can be edited there too. Changing the variant recalculates the exercise."
        ),
    );
}

#[derive(Clone)]
struct GenConfig {
    count: u64,
    max_decimals: String,
    positive: bool,
    distinct: bool,
    extra: String,
    domains: std::collections::BTreeMap<String, (String, String, String)>,
}

impl Default for GenConfig {
    fn default() -> Self {
        Self { count: 4, max_decimals: "2".into(), positive: true, distinct: true, extra: String::new(), domains: std::collections::BTreeMap::new() }
    }
}

/// The variant generator (`exercise.generateVariants`): how many variants, the result's
/// constraints and each datum's domain. Nothing without data.
pub(crate) fn variant_generator_ui(app: &mut WordApp, ui: &mut Ui, ex: u64, data: &Value) {
    let items = data["items"].as_array().cloned().unwrap_or_default();
    if items.is_empty() {
        return;
    }
    {
        let state_id = egui::Id::unique(("gen_variants_cfg", ex));
        let mut cfg = ui.data(|d| d.get_temp::<GenConfig>(state_id)).unwrap_or_default();
        crate::rtl::grid(ui, egui::Grid::new(("gen_variants_grid", ex)).num_columns(2).spacing([6.0, 4.0]), |ui| {
            let count_label = ui.label(tl!("Variants to generate")).id;
            ui.scope(|ui| {
                ui.set_width(52.0);
                let cur = cfg.count.to_string();
                if let Some(t) = draft_box(ui, ("gen_v_cnt", ex), count_label, &cur, true, false)
                    && let Ok(n) = t.trim().parse::<u64>()
                {
                    cfg.count = n.clamp(1, MAX_VARIANTS as u64);
                }
            });
            ui.end_row();
            let decimals_label = ui.label(tl!("Max result decimals")).id;
            ui.scope(|ui| {
                ui.set_width(52.0);
                if let Some(t) = draft_box(ui, ("gen_v_dec", ex), decimals_label, &cfg.max_decimals, true, false) {
                    cfg.max_decimals = t;
                }
            });
            ui.end_row();
            let extra_label = ui.label(tl!("Extra constraints")).id;
            ui.scope(|ui| {
                ui.set_width(140.0);
                if let Some(t) = draft_box(ui, ("gen_v_ext", ex), extra_label, &cfg.extra, true, false) {
                    cfg.extra = t;
                }
            });
            ui.end_row();
        });
        ui.checkbox(&mut cfg.positive, tl!("Positive final results"));
        ui.checkbox(&mut cfg.distinct, tl!("Distinct results across variants"));
        ui.add_space(4.0);
        crate::rtl::grid(ui, egui::Grid::new(("gen_domains_grid", ex)).num_columns(4).spacing([4.0, 3.0]).striped(true), |ui| {
            let [_, min_label, max_label, step_label] =
                ["Symbol", "Min", "Max", "Step"].map(|head| ui.label(egui::RichText::new(tl!(head)).small().weak()).id);
            ui.end_row();
            for (idx, it) in items.iter().enumerate() {
                let sym = it["symbol"].as_str().unwrap_or_default().trim().to_string();
                if sym.is_empty() {
                    continue;
                }
                let entry = cfg.domains.entry(sym.clone()).or_insert_with(|| ("1".into(), "10".into(), "1".into())).clone();
                let (mut mn, mut mx, mut stp) = entry;
                ui.label(egui::RichText::new(&sym).monospace());
                ui.scope(|ui| {
                    ui.set_width(44.0);
                    if let Some(t) = draft_box(ui, ("gen_dom_min", ex, idx), min_label, &mn, true, false) {
                        mn = t;
                    }
                });
                ui.scope(|ui| {
                    ui.set_width(44.0);
                    if let Some(t) = draft_box(ui, ("gen_dom_max", ex, idx), max_label, &mx, true, false) {
                        mx = t;
                    }
                });
                ui.scope(|ui| {
                    ui.set_width(44.0);
                    if let Some(t) = draft_box(ui, ("gen_dom_stp", ex, idx), step_label, &stp, true, false) {
                        stp = t;
                    }
                });
                cfg.domains.insert(sym, (mn, mx, stp));
                ui.end_row();
            }
        });
        ui.add_space(4.0);
        let clicked = ui
            .button(tl!("Generate Variants"))
            .on_hover_text(tl!("Deterministically generate clean, constraint-checked variants across the specified domains"))
            .clicked();
        ui.data_mut(|d| d.insert_temp(state_id, cfg.clone()));
        if clicked {
            let mut dom_arr = Vec::new();
            for it in &items {
                let sym = it["symbol"].as_str().unwrap_or_default().trim();
                if sym.is_empty() {
                    continue;
                }
                if let Some((mn, mx, stp)) = cfg.domains.get(sym) {
                    let parse_num = |s: &str| s.trim().replace(',', ".").parse::<f64>().ok();
                    if let (Some(a), Some(b)) = (parse_num(mn), parse_num(mx)) {
                        let s_val = parse_num(stp).unwrap_or(1.0);
                        dom_arr.push(json!({
                            "symbol": sym,
                            "unit": it["unit"].as_str().unwrap_or_default(),
                            "description": it["description"].as_str().unwrap_or_default(),
                            "min": a,
                            "max": b,
                            "step": s_val,
                        }));
                    }
                }
            }
            let extra_list: Vec<String> = cfg.extra.split([',', ';']).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect();
            let mut constraints = json!({
                "positiveResults": cfg.positive,
                "distinctFinalResults": cfg.distinct,
                "extra": extra_list,
            });
            if let Ok(md) = cfg.max_decimals.trim().parse::<u8>() {
                constraints["maxDecimals"] = json!(md.min(9));
            }
            // A generation that fails (too few values for the variants asked, a minimum above
            // its maximum) says why in the status bar.
            if let Err(e) = app.run(
                "exercise.generateVariants",
                json!({
                    "exercise": ex,
                    "count": cfg.count,
                    "domains": dom_arr,
                    "constraints": constraints,
                }),
            ) {
                app.status(e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable;
    use wordcraft_engine::Session;

    /// The owner: the data are in a pane beside the document, not a drop-down panel under the
    /// button. `view.dataPane` shows and hides it; it edits the data of the exercise at the
    /// cursor and says so outside the exercises.
    #[test]
    fn exercise_data_is_a_pane() {
        let mut app = WordApp::new(Session::new(wordcraft_doc::Document::from_text("Intro")), crate::Services::default());
        app.run("exercise.insert", json!({"kind": "problem"})).unwrap();
        assert!(!crate::panes::open(&app).contains(&crate::panes::Pane::Data));
        app.run("view.dataPane", json!({})).unwrap();
        assert!(crate::panes::open(&app).contains(&crate::panes::Pane::Data));
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(360.0, 600.0)).build_ui_state(|ui, app: &mut WordApp| pane(app, ui), app);
        h.run_steps(2);
        h.get_by_label(tl!("Add Datum")).click();
        h.run_steps(2);
        let data = h.state_mut().session.run("exercise.data", &json!({})).unwrap();
        assert_eq!(data["data"]["items"].as_array().map(Vec::len), Some(1), "{data}");
        // Outside the exercises: a note, nothing to edit.
        h.state_mut().run("caret.set", json!({"pos": wordcraft_doc::Pos::body(0, 0)})).unwrap();
        h.run_steps(2);
        assert!(h.query_by_label(tl!("Add Datum")).is_none());
        h.get_by_label(tl!("Put the cursor in an exercise to see its data and variants."));
        h.state_mut().run("view.dataPane", json!({})).unwrap();
        assert!(!crate::panes::open(h.state()).contains(&crate::panes::Pane::Data));
    }
}
