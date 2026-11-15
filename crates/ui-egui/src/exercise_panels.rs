//! What the Exercise and Series ribbon tabs (`ribbon/exercise.rs`, docs/exercise.md) show in
//! their drop-down panels: the exercise's data in the active variant, the variant generator, and
//! the variants added and removed. Every change goes through `exercise.setData` or
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

/// The exercise's data (`info`, from `exercise.data`): symbol, description, value in the active
/// variant and unit, whether the Data block prints, then the variant generator.
pub(crate) fn data_panel(app: &mut WordApp, ui: &mut Ui, info: &Value) {
    let ex = info["exercise"].as_u64().unwrap_or(0);
    let mut data = info["data"].clone();
    let errors = info["errors"].as_array().cloned().unwrap_or_default();
    let variants = data["variants"].as_array().map_or(0, Vec::len);
    let active = data["active"].as_u64().unwrap_or(0) as usize;
    let data = &mut data;
    let save = |app: &mut WordApp, data: &Value| {
        let _ = app.run("exercise.setData", json!({"exercise": ex, "data": data}));
    };
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
            "The data are the first parameters of the exercise's calculations: the solution uses them by symbol. Changing the variant recalculates the exercise."
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
            ui.label(tl!("Variants to generate"));
            ui.scope(|ui| {
                ui.set_width(52.0);
                let cur = cfg.count.to_string();
                if let Some(t) = draft_box(ui, ("gen_v_cnt", ex), &cur, true, false)
                    && let Ok(n) = t.trim().parse::<u64>()
                {
                    cfg.count = n.clamp(1, MAX_VARIANTS as u64);
                }
            });
            ui.end_row();
            ui.label(tl!("Max result decimals"));
            ui.scope(|ui| {
                ui.set_width(52.0);
                if let Some(t) = draft_box(ui, ("gen_v_dec", ex), &cfg.max_decimals, true, false) {
                    cfg.max_decimals = t;
                }
            });
            ui.end_row();
            ui.label(tl!("Extra constraints"));
            ui.scope(|ui| {
                ui.set_width(140.0);
                if let Some(t) = draft_box(ui, ("gen_v_ext", ex), &cfg.extra, true, false) {
                    cfg.extra = t;
                }
            });
            ui.end_row();
        });
        ui.checkbox(&mut cfg.positive, tl!("Positive final results"));
        ui.checkbox(&mut cfg.distinct, tl!("Distinct results across variants"));
        ui.add_space(4.0);
        crate::rtl::grid(ui, egui::Grid::new(("gen_domains_grid", ex)).num_columns(4).spacing([4.0, 3.0]).striped(true), |ui| {
            for head in ["Symbol", "Min", "Max", "Step"] {
                ui.label(egui::RichText::new(tl!(head)).small().weak());
            }
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
                    if let Some(t) = draft_box(ui, ("gen_dom_min", ex, idx), &mn, true, false) {
                        mn = t;
                    }
                });
                ui.scope(|ui| {
                    ui.set_width(44.0);
                    if let Some(t) = draft_box(ui, ("gen_dom_max", ex, idx), &mx, true, false) {
                        mx = t;
                    }
                });
                ui.scope(|ui| {
                    ui.set_width(44.0);
                    if let Some(t) = draft_box(ui, ("gen_dom_stp", ex, idx), &stp, true, false) {
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
            let _ = app.run(
                "exercise.generateVariants",
                json!({
                    "exercise": ex,
                    "count": cfg.count,
                    "domains": dom_arr,
                    "constraints": constraints,
                }),
            );
        }
    }
}
