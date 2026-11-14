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
/// the frontispiece; first, every exercise with a way to go to it or delete it. They show
/// wherever the cursor is.
fn series_tab(app: &mut WordApp, ui: &mut Ui) {
    crate::pane_kit::section(ui, "01", tl!("Exercises"));
    exercise_list(app, ui);
    crate::pane_kit::section(ui, "02", tl!("Numbering"));
    numbering(app, ui);
    egui::CollapsingHeader::new(tl!("Layout")).id_salt("series_layout").default_open(false).show(ui, |ui| {
        crate::pane_kit::note(
            ui,
            tl!(
                "Every exercise of the series has the same settings, the first exercise's: a change applies to all of them, and a new exercise takes them."
            ),
        );
        sub_heading(ui, tl!("Parts Shown"));
        parts(app, ui, 0);
        sub_heading(ui, tl!("Page Breaks"));
        series_breaks(app, ui);
        sub_heading(ui, tl!("Frontispiece"));
        frontispiece(app, ui);
    });
}

/// A heading inside a collapsed group.
fn sub_heading(ui: &mut Ui, text: &str) {
    let t = crate::theme::Tokens::get(ui.ctx());
    ui.add_space(8.0);
    ui.label(egui::RichText::new(text.to_uppercase()).font(crate::pane_kit::mono(10.0)).color(t.text_dim));
    ui.add_space(2.0);
}

/// Every exercise of the document: its label and title, Go To and Delete (one at a time, whole,
/// Undo brings it back).
fn exercise_list(app: &mut WordApp, ui: &mut Ui) {
    let info = app.session.run("exercise.info", &json!({})).ok().unwrap_or_default();
    let here = info["exercise"].as_u64();
    let list = info["list"].as_array().cloned().unwrap_or_default();
    let t = crate::theme::Tokens::get(ui.ctx());
    let mut delete = None;
    for (k, e) in list.iter().enumerate() {
        let n = k as u64 + 1;
        let label = e["label"].as_str().unwrap_or_default();
        let title: String = e["title"].as_str().unwrap_or_default().chars().take(40).collect();
        let fill = if here == Some(n) { t.accent.gamma_multiply(0.18) } else { egui::Color32::TRANSPARENT };
        egui::Frame::new().fill(fill).corner_radius(egui::CornerRadius::same(4)).inner_margin(egui::Margin::symmetric(6, 3)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(label).strong());
                ui.add(egui::Label::new(title).truncate());
                ui.with_layout(crate::rtl::far_side(ui, egui::Align::Center), |ui| {
                    if ui
                        .small_button("🗑")
                        .on_hover_text(tl!("Delete this exercise whole: its statement, questions and solution (Undo brings it back)"))
                        .clicked()
                    {
                        delete = Some(n);
                    }
                    if ui.small_button(tl!("Go to")).on_hover_text(tl!("Put the cursor in this exercise's title")).clicked() {
                        let _ = app.run("caret.set", json!({"pos": e["pos"]}));
                        app.canvas.want_focus = true;
                    }
                });
            });
        });
    }
    if let Some(n) = delete
        && let Err(e) = app.run("exercise.delete", json!({"exercise": n}))
    {
        app.status(e);
    }
}

/// The kind of an exercise, as its badge names it.
fn kind_name(kind: &str) -> &'static str {
    match kind {
        "open" => "Open Answer",
        "choice" => "Multiple Choice",
        _ => "Problem",
    }
}

/// The exercise at the caret (`info`, from `exercise.data`): what its first line holds, its
/// duration, its questions, its variants and data, and (`lone`: the document's only exercise,
/// so what a series would rule follows here) its layout.
fn exercise_tab(app: &mut WordApp, ui: &mut Ui, info: &Value, lone: bool) {
    let ex = info["exercise"].as_u64().unwrap_or(0);
    let mut data = info["data"].clone();
    let errors = info["errors"].as_array().cloned().unwrap_or_default();
    let variant_errors = &info["variantErrors"];
    let variants: Vec<String> =
        data["variants"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
    let active = data["active"].as_u64().unwrap_or(0) as usize;
    let save = |app: &mut WordApp, data: &Value| {
        let _ = app.run("exercise.setData", json!({"exercise": ex, "data": data}));
    };
    let q_st = app.session.run("exercise.questions", &json!({"exercise": ex})).ok();
    let kind = q_st.as_ref().and_then(|st| st["kind"].as_str()).unwrap_or("problem").to_string();
    let quiz = kind != "problem";

    header(app, ui, ex, lone, &kind);
    crate::pane_kit::section(ui, "01", tl!("General"));
    general(app, ui, ex, &mut data, &save);

    crate::pane_kit::section(ui, "02", tl!("Questions"));
    if let Some(st) = q_st.as_ref() {
        crate::exercise_quiz::questions(app, ui, ex, st, quiz);
    }

    ui.add_space(8.0);
    let needs_data = !quiz
        || q_st
            .as_ref()
            .and_then(|st| st["questions"].as_array())
            .is_some_and(|qs| qs.iter().any(|q| matches!(q["type"].as_str(), Some("calc" | "drawing"))))
        || data["items"].as_array().is_some_and(|a| !a.is_empty())
        || variants.len() > 1;
    egui::CollapsingHeader::new(tl!("Variants & Data")).id_salt(("exercise_variants_data", ex, needs_data)).default_open(needs_data).show(ui, |ui| {
        variants_and_data(app, ui, ex, &mut data, &variants, active, &errors, variant_errors, "", "", &save);
    });
    if lone {
        egui::CollapsingHeader::new(tl!("Layout")).id_salt(("exercise_layout", ex)).default_open(false).show(ui, |ui| {
            sub_heading(ui, tl!("Parts Shown"));
            parts(app, ui, ex);
            sub_heading(ui, tl!("Page Breaks"));
            page_breaks(app, ui);
            sub_heading(ui, tl!("Frontispiece"));
            frontispiece(app, ui);
        });
    } else {
        ui.add_space(6.0);
        crate::pane_kit::note(ui, tl!("Page breaks, parts shown and graph paper are the series': see the Series tab."));
    }
}

/// The exercise's name and kind, with Delete.
fn header(app: &mut WordApp, ui: &mut Ui, ex: u64, lone: bool, kind: &str) {
    // A lone exercise has no number.
    let title = if lone { tl!("Exercise").to_string() } else { format!("{} {ex}", tl!("Exercise")) };
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).size(15.0).strong());
        crate::pane_kit::badge(ui, tl!(kind_name(kind)), crate::pane_kit::Status::Changed);
        ui.with_layout(crate::rtl::far_side(ui, egui::Align::Center), |ui| {
            if ui
                .button(tl!("Delete"))
                .on_hover_text(tl!("Delete this exercise whole: its statement, questions and solution (Undo brings it back)"))
                .clicked()
                && let Err(e) = app.run("exercise.delete", json!({"exercise": ex}))
            {
                app.status(e);
            }
        });
    });
}

/// What the first line holds after the number, and the duration.
fn general(app: &mut WordApp, ui: &mut Ui, ex: u64, data: &mut Value, save: &dyn Fn(&mut WordApp, &Value)) {
    let line = app.session.run("exercise.titleLine", &json!({"exercise": ex})).ok().and_then(|v| v["line"].as_str().map(str::to_string));
    crate::rtl::grid(ui, egui::Grid::new(("exercise_general", ex)).num_columns(2).spacing([8.0, 6.0]), |ui| {
        ui.label(tl!("First line"));
        let line = line.unwrap_or_else(|| "title".into());
        let names = [("title", tl!("Title")), ("statement", tl!("Text next to the number (no title)"))];
        let shown = names.iter().find(|(id, _)| *id == line).map_or("", |(_, l)| l).to_string();
        craft_ui::ComboBox::from_id_salt(("exercise_title_line", ex)).selected_text(shown).width(ui.available_width().min(220.0)).show_ui(ui, |ui| {
            for (id, label) in names {
                if ui.selectable_label(id == line, label).clicked()
                    && id != line
                    && let Err(e) = app.run("exercise.titleLine", json!({"exercise": ex, "line": id}))
                {
                    app.status(e);
                }
            }
        });
        ui.end_row();
        ui.label(tl!("Duration"));
        ui.horizontal(|ui| {
            let dur = data["duration"].as_u64().map(|d| d.to_string()).unwrap_or_default();
            ui.scope(|ui| {
                ui.set_width(48.0);
                if let Some(t) = draft_box(ui, ("exercise_duration", ex), &dur, true, false) {
                    let parsed = t.trim().parse::<u32>().ok().filter(|&n| (1..=600).contains(&n));
                    data["duration"] = parsed.map_or(Value::Null, |n| json!(n));
                    save(app, data);
                }
            });
            ui.label("min");
        });
        ui.end_row();
    });
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
    variant_errors: &Value,
    sec_var: &str,
    sec_data: &str,
    save: &dyn Fn(&mut WordApp, &Value),
) {
    // Variants: the active one is the one the exercise is calculated with.
    let _ = (sec_var, sec_data);
    sub_heading(ui, tl!("Variants"));
    ui.horizontal_wrapped(|ui| {
        for (k, name) in variants.iter().enumerate() {
            let v_errs: Vec<&str> =
                variant_errors.get(name.as_str()).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
            let has_err = !v_errs.is_empty();
            let btn_text = if has_err {
                egui::RichText::new(format!("{name} ⚠")).color(ui.visuals().error_fg_color)
            } else {
                egui::RichText::new(name.as_str())
            };
            let hover = if has_err {
                format!("{}\n\n⚠ {}", tl!("Calculate the exercise with this variant"), v_errs.join("\n⚠ "))
            } else {
                tl!("Calculate the exercise with this variant").to_string()
            };
            if ui.selectable_label(k == active, btn_text).on_hover_text(hover).clicked() && k != active {
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
    sub_heading(ui, tl!("Data"));
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
    variant_generator_ui(app, ui, ex, data);
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

fn variant_generator_ui(app: &mut WordApp, ui: &mut Ui, ex: u64, data: &Value) {
    let items = data["items"].as_array().cloned().unwrap_or_default();
    if items.is_empty() {
        return;
    }
    egui::CollapsingHeader::new(tl!("Generate Variants…")).id_salt(("gen_variants_hdr", ex)).default_open(false).show(ui, |ui| {
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
    });
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
