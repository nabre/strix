//! The Plot Properties task pane (`Math › Drawing`, `view.plotPane`, `docs/plot.md`):
//! configures the selected parametric Cartesian plot (`plot.get` / `plot.set`), its axes,
//! student blank-grid mode, mathematical/parametric/experimental series, and displays live
//! computed roots, extrema, tangents, definite integrals and least-squares regressions.

use egui::{Color32, RichText, Stroke, Ui, vec2};
use serde_json::{Value, json};

use crate::WordApp;
use crate::theme::Tokens;

const SERIES_COLORS: [([u8; 3], &str); 6] = [
    ([0x25, 0x63, 0xEB], "Blue"),
    ([0xDC, 0x26, 0x26], "Red"),
    ([0x16, 0xA3, 0x4A], "Green"),
    ([0xD9, 0x77, 0x06], "Amber"),
    ([0x7C, 0x3A, 0xED], "Violet"),
    ([0x08, 0x91, 0xB2], "Cyan"),
];

pub(crate) fn pane(app: &mut WordApp, ui: &mut Ui) {
    let Ok(info) = app.session.run("plot.get", &json!({})) else {
        empty_state(app, ui);
        return;
    };
    let mut plot = info["plot"].clone();
    let mut changed = false;
    let t = Tokens::get(ui.ctx());

    egui::ScrollArea::vertical().id_salt("plot_pane_scroll").show(ui, |ui| {
        changed |= axes_section(ui, &mut plot);
        ui.add_space(6.0);
        ui.separator();
        ui.add_space(6.0);
        changed |= series_section(ui, &mut plot, &t);
        ui.add_space(6.0);
        analysis_section(ui, &info, &t);
    });

    if changed {
        let _ = app.run("plot.set", json!({"plot": plot}));
    }
}

fn empty_state(app: &mut WordApp, ui: &mut Ui) {
    ui.label(RichText::new(tl!("Select a plot in the document to edit its axes, functions and experimental data.")).weak());
    ui.add_space(8.0);
    if ui.button(tl!("Insert Plot")).clicked() {
        let _ = app.run("plot.insert", json!({"openPane": true}));
    }
}

fn num_drag(ui: &mut Ui, key: (&str, usize), current: f64, range: std::ops::RangeInclusive<f64>, suffix: &str) -> Option<f64> {
    let id = ui.scope_id().with(("plot_num", key));
    let mut v = ui.data(|d| d.get_temp::<f64>(id)).unwrap_or(current);
    let r = ui.add(egui::DragValue::new(&mut v).range(range).suffix(suffix).speed(0.2).max_decimals(3).update_while_editing(false));
    if r.dragged() {
        ui.data_mut(|d| d.insert_temp(id, v));
        return None;
    }
    let was_dragging = ui.data(|d| d.get_temp::<f64>(id)).is_some();
    if was_dragging {
        ui.data_mut(|d| d.remove::<f64>(id));
    }
    ((r.changed() || was_dragging) && (v - current).abs() > 1e-9).then_some(v)
}

fn text_box(ui: &mut Ui, key: (&str, usize), current: &str) -> Option<String> {
    crate::a11y_pane::draft_box(ui, ("plot_txt", key.0, key.1), current, true, false)
}

fn axes_section(ui: &mut Ui, plot: &mut Value) -> bool {
    let mut changed = false;
    ui.label(RichText::new(tl!("Axes & Frame")).strong());
    ui.add_space(4.0);
    crate::rtl::grid(ui, egui::Grid::new("plot_axes_grid").num_columns(2).spacing(vec2(8.0, 4.0)), |ui| {
        ui.label(tl!("Title"));
        let title = plot["title"].as_str().unwrap_or_default().to_string();
        if let Some(v) = text_box(ui, ("title", 0), &title) {
            plot["title"] = json!(v);
            changed = true;
        }
        ui.end_row();

        for (lbl, k, idx, def) in [("Width", "width", 1usize, 120.0), ("Height", "height", 2, 80.0)] {
            ui.label(tl!(lbl));
            let cur = plot[k].as_f64().unwrap_or(def);
            if let Some(v) = num_drag(ui, (k, idx), cur, 30.0..=260.0, " mm") {
                plot[k] = json!(v);
                changed = true;
            }
            ui.end_row();
        }

        for (lbl, k0, k1, base) in [("X range", "xMin", "xMax", 10usize), ("Y range", "yMin", "yMax", 20)] {
            ui.label(tl!(lbl));
            ui.horizontal(|ui| {
                let a = plot[k0].as_f64().unwrap_or(-5.0);
                let b = plot[k1].as_f64().unwrap_or(5.0);
                if let Some(va) = num_drag(ui, (k0, base), a, -1e6..=1e6, "") {
                    plot[k0] = json!(va);
                    changed = true;
                }
                ui.label("..");
                if let Some(vb) = num_drag(ui, (k1, base + 1), b, -1e6..=1e6, "") {
                    plot[k1] = json!(vb);
                    changed = true;
                }
            });
            ui.end_row();
        }

        for (lbl, lk, uk, idx) in [("X axis", "xLabel", "xUnit", 30usize), ("Y axis", "yLabel", "yUnit", 40)] {
            ui.label(tl!(lbl));
            ui.horizontal(|ui| {
                let lcur = plot[lk].as_str().unwrap_or_default().to_string();
                let ucur = plot[uk].as_str().unwrap_or_default().to_string();
                ui.set_max_width(170.0);
                if let Some(vl) = text_box(ui, (lk, idx), &lcur) {
                    plot[lk] = json!(vl);
                    changed = true;
                }
                ui.label("[");
                if let Some(vu) = text_box(ui, (uk, idx + 1), &ucur) {
                    if vu.trim().is_empty() {
                        if let Some(obj) = plot.as_object_mut() {
                            obj.remove(uk);
                        }
                    } else {
                        plot[uk] = json!(vu.trim());
                    }
                    changed = true;
                }
                ui.label("]");
            });
            ui.end_row();
        }
    });

    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        for (k, lbl) in [("grid", "Grid"), ("minorGrid", "Minor grid"), ("legend", "Legend")] {
            let mut on = plot[k].as_bool().unwrap_or(k != "minorGrid");
            if ui.checkbox(&mut on, tl!(lbl)).changed() {
                plot[k] = json!(on);
                changed = true;
            }
        }
    });
    let mut ans = plot["answer"].as_bool().unwrap_or(false);
    if ui
        .checkbox(&mut ans, tl!("Student blank grid (solution only)"))
        .on_hover_text(tl!("On the student copy, draw only the Cartesian grid and axes so students can sketch the curve by hand"))
        .changed()
    {
        plot["answer"] = json!(ans);
        changed = true;
    }
    changed
}

fn series_section(ui: &mut Ui, plot: &mut Value, t: &Tokens) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(tl!("Series")).strong());
        let next_idx = plot["series"].as_array().map_or(0, Vec::len);
        let col = SERIES_COLORS[next_idx % SERIES_COLORS.len()].0;
        if ui.small_button(format!("+ {}", tl!("Function"))).clicked() {
            push_series(plot, json!({"type": "function", "label": "f(x)", "expr": "sin(x)", "color": col, "width": 0.5}));
            changed = true;
        }
        if ui.small_button(format!("+ {}", tl!("Parametric"))).clicked() {
            push_series(
                plot,
                json!({"type": "parametric", "label": "γ(t)", "xExpr": "cos(t)", "yExpr": "sin(t)", "tMin": 0.0, "tMax": std::f64::consts::TAU, "color": col, "width": 0.5}),
            );
            changed = true;
        }
        if ui.small_button(format!("+ {}", tl!("Data"))).clicked() {
            push_series(plot, json!({"type": "data", "label": "Data", "xCol": "t", "yCol": "x", "regression": "linear", "color": col, "width": 0.5}));
            changed = true;
        }
    });
    ui.add_space(4.0);

    let mut remove_idx = None;
    if let Some(arr) = plot["series"].as_array_mut() {
        for (i, s) in arr.iter_mut().enumerate() {
            egui::Frame::NONE.fill(t.input).stroke(Stroke::new(1.0, t.border)).corner_radius(6).inner_margin(8).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let (c, rm) = one_series_card(ui, i, s);
                changed |= c;
                if rm {
                    remove_idx = Some(i);
                }
            });
            ui.add_space(4.0);
        }
        if let Some(idx) = remove_idx {
            arr.remove(idx);
            changed = true;
        }
    }
    changed
}

fn push_series(plot: &mut Value, item: Value) {
    if !plot["series"].is_array() {
        plot["series"] = json!([]);
    }
    if let Some(arr) = plot["series"].as_array_mut() {
        arr.push(item);
    }
}

fn one_series_card(ui: &mut Ui, i: usize, s: &mut Value) -> (bool, bool) {
    let mut changed = false;
    let mut remove = false;
    let kind = s["type"].as_str().unwrap_or("function").to_string();
    ui.horizontal(|ui| {
        if let Some(c) = color_picker(ui, i, current_rgb(&s["color"])) {
            s["color"] = json!(c);
            changed = true;
        }
        let lbl = s["label"].as_str().unwrap_or_default().to_string();
        if let Some(v) = text_box(ui, ("s_lbl", i), &lbl) {
            s["label"] = json!(v);
            changed = true;
        }
        if ui.small_button("×").on_hover_text(tl!("Remove series")).clicked() {
            remove = true;
        }
    });
    changed |= match kind.as_str() {
        "parametric" => parametric_fields(ui, i, s),
        "data" => data_fields(ui, i, s),
        _ => function_fields(ui, i, s),
    };
    ui.horizontal_wrapped(|ui| {
        let mut dashed = s["dashed"].as_bool().unwrap_or(false);
        if ui.checkbox(&mut dashed, tl!("Dashed")).changed() {
            s["dashed"] = json!(dashed);
            changed = true;
        }
        let mut ans = s["answer"].as_bool().unwrap_or(false);
        if ui.checkbox(&mut ans, tl!("Solution only")).changed() {
            s["answer"] = json!(ans);
            changed = true;
        }
    });
    (changed, remove)
}

fn current_rgb(v: &Value) -> [u8; 3] {
    let Some(a) = v.as_array() else { return SERIES_COLORS[0].0 };
    if a.len() < 3 {
        return SERIES_COLORS[0].0;
    }
    [a[0].as_u64().unwrap_or(0x25) as u8, a[1].as_u64().unwrap_or(0x63) as u8, a[2].as_u64().unwrap_or(0xEB) as u8]
}

fn color_picker(ui: &mut Ui, idx: usize, cur: [u8; 3]) -> Option<[u8; 3]> {
    let mut out = None;
    let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), egui::Sense::hover());
    ui.painter().rect_filled(r, 2.0, Color32::from_rgb(cur[0], cur[1], cur[2]));
    craft_ui::ComboBox::from_id_salt(("plot_scol", idx)).selected_text("").width(22.0).show_ui(ui, |ui| {
        for (c, name) in SERIES_COLORS {
            if ui.selectable_label(c == cur, tl!(name)).clicked() {
                out = Some(c);
            }
        }
    });
    out
}

fn function_fields(ui: &mut Ui, i: usize, s: &mut Value) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("y =");
        let expr = s["expr"].as_str().unwrap_or("x").to_string();
        if let Some(v) = text_box(ui, ("f_expr", i), &expr) {
            s["expr"] = json!(v);
            changed = true;
        }
    });
    ui.horizontal_wrapped(|ui| {
        let mut roots = s["roots"].as_bool().unwrap_or(false);
        if ui.checkbox(&mut roots, tl!("Roots")).changed() {
            s["roots"] = json!(roots);
            changed = true;
        }
        let mut ext = s["extrema"].as_bool().unwrap_or(false);
        if ui.checkbox(&mut ext, tl!("Extrema")).changed() {
            s["extrema"] = json!(ext);
            changed = true;
        }
    });
    changed |= tangent_and_integral_fields(ui, i, s);
    changed
}

fn tangent_and_integral_fields(ui: &mut Ui, i: usize, s: &mut Value) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let mut has_tan = s.get("tangentAt").is_some_and(|v| !v.is_null());
        let cur_x0 = s["tangentAt"].as_f64().unwrap_or(0.0);
        if ui.checkbox(&mut has_tan, tl!("Tangent at")).changed() {
            if has_tan {
                s["tangentAt"] = json!(0.0);
            } else if let Some(obj) = s.as_object_mut() {
                obj.remove("tangentAt");
            }
            changed = true;
        }
        if has_tan && let Some(v) = num_drag(ui, ("tan", i), cur_x0, -1e6..=1e6, "") {
            s["tangentAt"] = json!(v);
            changed = true;
        }
    });
    ui.horizontal(|ui| {
        let cur = s["integral"].as_array().filter(|a| a.len() == 2).map(|a| [a[0].as_f64().unwrap_or(0.0), a[1].as_f64().unwrap_or(1.0)]);
        let mut has_int = cur.is_some();
        if ui.checkbox(&mut has_int, tl!("Integral")).changed() {
            if has_int {
                s["integral"] = json!([0.0, 1.0]);
            } else if let Some(obj) = s.as_object_mut() {
                obj.remove("integral");
            }
            changed = true;
        }
        if let Some([a, b]) = cur {
            if let Some(va) = num_drag(ui, ("int_a", i), a, -1e6..=1e6, "") {
                s["integral"] = json!([va, b]);
                changed = true;
            }
            ui.label("..");
            if let Some(vb) = num_drag(ui, ("int_b", i), b, -1e6..=1e6, "") {
                s["integral"] = json!([a, vb]);
                changed = true;
            }
        }
    });
    changed
}

fn parametric_fields(ui: &mut Ui, i: usize, s: &mut Value) -> bool {
    let mut changed = false;
    for (lbl, k, tag) in [("x(t) =", "xExpr", "px"), ("y(t) =", "yExpr", "py")] {
        ui.horizontal(|ui| {
            ui.label(lbl);
            let cur = s[k].as_str().unwrap_or("t").to_string();
            if let Some(v) = text_box(ui, (tag, i), &cur) {
                s[k] = json!(v);
                changed = true;
            }
        });
    }
    ui.horizontal(|ui| {
        // ASCII: the interface font (Inter) has no "∈" glyph.
        ui.label("t =");
        let t0 = s["tMin"].as_f64().unwrap_or(0.0);
        let t1 = s["tMax"].as_f64().unwrap_or(std::f64::consts::TAU);
        if let Some(v) = num_drag(ui, ("t0", i), t0, -1e6..=1e6, "") {
            s["tMin"] = json!(v);
            changed = true;
        }
        ui.label("..");
        if let Some(v) = num_drag(ui, ("t1", i), t1, -1e6..=1e6, "") {
            s["tMax"] = json!(v);
            changed = true;
        }
    });
    changed
}

fn data_fields(ui: &mut Ui, i: usize, s: &mut Value) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("X:");
        let xc = s["xCol"].as_str().unwrap_or("t").to_string();
        if let Some(v) = text_box(ui, ("xcol", i), &xc) {
            s["xCol"] = json!(v);
            changed = true;
        }
        ui.label("Y:");
        let yc = s["yCol"].as_str().unwrap_or("x").to_string();
        if let Some(v) = text_box(ui, ("ycol", i), &yc) {
            s["yCol"] = json!(v);
            changed = true;
        }
    });
    ui.horizontal(|ui| {
        ui.label(tl!("Fit"));
        let reg = s["regression"].as_str().unwrap_or("none").to_string();
        craft_ui::ComboBox::from_id_salt(("plot_reg", i)).selected_text(tl!(reg_label(&reg))).width(110.0).show_ui(ui, |ui| {
            for id in ["none", "linear", "quadratic"] {
                if ui.selectable_label(reg == id, tl!(reg_label(id))).clicked() {
                    s["regression"] = json!(id);
                    changed = true;
                }
            }
        });
    });
    changed
}

fn reg_label(id: &str) -> &'static str {
    match id {
        "linear" => "Linear",
        "quadratic" => "Quadratic",
        _ => "None",
    }
}

fn analysis_section(ui: &mut Ui, info: &Value, t: &Tokens) {
    let errs = info["errors"].as_array().cloned().unwrap_or_default();
    let notables = info["notables"].as_array().cloned().unwrap_or_default();
    let integrals = info["integrals"].as_array().cloned().unwrap_or_default();
    let regressions = info["regressions"].as_array().cloned().unwrap_or_default();
    if errs.is_empty() && notables.is_empty() && integrals.is_empty() && regressions.is_empty() {
        return;
    }
    ui.separator();
    ui.add_space(4.0);
    ui.label(RichText::new(tl!("Analysis")).strong());
    for e in &errs {
        if let Some(msg) = e.as_str() {
            ui.label(RichText::new(msg).small().color(Color32::from_rgb(0xC4, 0x3E, 0x1C)));
        }
    }
    for n in &notables {
        let kind = n["kind"].as_str().unwrap_or("point");
        let x = n["x"].as_f64().unwrap_or(0.0);
        let y = n["y"].as_f64().unwrap_or(0.0);
        let text = match n["slope"].as_f64() {
            Some(m) => format!("{kind}: ({x:.4}, {y:.4}), m = {m:.4}"),
            None => format!("{kind}: ({x:.4}, {y:.4})"),
        };
        ui.label(RichText::new(text).small().monospace());
    }
    for ig in &integrals {
        let a = ig["a"].as_f64().unwrap_or(0.0);
        let b = ig["b"].as_f64().unwrap_or(0.0);
        let v = ig["value"].as_f64().unwrap_or(0.0);
        ui.label(RichText::new(format!("∫ [{a:.3}, {b:.3}] = {v:.5}")).small().monospace().color(t.accent));
    }
    for rg in &regressions {
        let eq = rg["equation"].as_str().unwrap_or_default();
        let r2 = rg["r2"].as_f64().unwrap_or(0.0);
        ui.label(RichText::new(format!("{eq}  (R² = {r2:.4})")).small().monospace());
    }
}
