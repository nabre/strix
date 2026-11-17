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
    // A new abscissa name goes as `xLabel` beside the plot, so the engine renames the variable
    // of the functions written in the old one (`Plot::set_x_label`).
    let mut x_label = None;
    let t = Tokens::get(ui.ctx());

    egui::ScrollArea::vertical().id_salt("plot_pane_scroll").show(ui, |ui| {
        changed |= axes_section(ui, &mut plot, &info["range"], &mut x_label);
        ui.add_space(6.0);
        changed |= reference_section(ui, &mut plot);
        ui.add_space(6.0);
        ui.separator();
        ui.add_space(6.0);
        changed |= series_section(app, ui, &mut plot, &t);
        ui.add_space(6.0);
        analysis_section(ui, &info, &t);
        insert_analysis_button(app, ui, &plot);
    });

    if let Some(label) = x_label {
        let _ = app.run("plot.set", json!({"plot": plot, "xLabel": label}));
    } else if changed {
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
    sized_box(ui, key, current, 0.0)
}

/// A text box committed when it loses focus (like `a11y_pane::draft_box`), as wide as the row's
/// remaining width less `reserve` (room for what follows it), so a card's row never runs past
/// the pane's edge.
fn sized_box(ui: &mut Ui, key: (&str, usize), current: &str, reserve: f32) -> Option<String> {
    let id = egui::Id::unique(("plot_txt", key.0, key.1));
    let mut text = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| current.to_string());
    let w = (ui.available_width() - reserve).max(32.0);
    let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(w));
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

fn axes_section(ui: &mut Ui, plot: &mut Value, fitted: &Value, x_label: &mut Option<String>) -> bool {
    let auto = plot["autoRange"].as_bool().unwrap_or(false);
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

        ui.label(tl!("Ranges"));
        let mut on = auto;
        if ui
            .checkbox(&mut on, tl!("Automatic"))
            .on_hover_text(tl!("Fit the ranges to what the plot shows: roots, extrema, tangent points, integral bounds, data and curves"))
            .changed()
        {
            // Turning it off keeps the view: the fitted ranges become the ones set.
            if !on && let Some(r) = fitted.as_array().filter(|r| r.len() == 4) {
                for (k, v) in ["xMin", "xMax", "yMin", "yMax"].iter().zip(r) {
                    plot[*k] = v.clone();
                }
            }
            plot["autoRange"] = json!(on);
            changed = true;
        }
        ui.end_row();

        for (lbl, k0, k1, base, ri) in [("X range", "xMin", "xMax", 10usize, 0usize), ("Y range", "yMin", "yMax", 20, 2)] {
            ui.label(tl!(lbl));
            if auto {
                // Fitted: shown, not edited.
                let a = fitted[ri].as_f64().unwrap_or(-5.0);
                let b = fitted[ri + 1].as_f64().unwrap_or(5.0);
                ui.label(RichText::new(format!("{a} .. {b}")).weak());
                ui.end_row();
                continue;
            }
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

        for (lbl, lk, uk, idx) in [("Abscissa", "xLabel", "xUnit", 30usize), ("Ordinate", "yLabel", "yUnit", 40)] {
            ui.label(tl!(lbl)).on_hover_text(tl!(
                "The axis name, written at the tip of its arrow; when it is a name (x, E, t), the formulas use it as their variable"
            ));
            ui.horizontal(|ui| {
                let lcur = plot[lk].as_str().unwrap_or_default().to_string();
                let ucur = plot[uk].as_str().unwrap_or_default().to_string();
                // Name, then its unit in brackets: the unit box keeps 56 px and the brackets.
                if let Some(vl) = sized_box(ui, (lk, idx), &lcur, 80.0) {
                    if lk == "xLabel" {
                        *x_label = Some(vl);
                    } else {
                        plot[lk] = json!(vl);
                        changed = true;
                    }
                }
                ui.label("[");
                if let Some(vu) = sized_box(ui, (uk, idx + 1), &ucur, 12.0) {
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
            // As the model defaults: grid on, minor grid and legend off.
            let mut on = plot[k].as_bool().unwrap_or(k == "grid");
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

/// Where oriented angles start and which way they turn, their unit, and whether the plot shows
/// the positive direction.
fn reference_section(ui: &mut Ui, plot: &mut Value) -> bool {
    let mut changed = false;
    ui.label(RichText::new(tl!("Reference System")).strong());
    ui.add_space(4.0);
    crate::rtl::grid(ui, egui::Grid::new("plot_ref_grid").num_columns(2).spacing(vec2(8.0, 4.0)), |ui| {
        ui.label(tl!("Angles"));
        let cur = plot["angles"].as_str().unwrap_or("math").to_string();
        craft_ui::ComboBox::from_id_salt("plot_angles").selected_text(tl!(angles_label(&cur))).width(190.0).show_ui(ui, |ui| {
            for id in ["math", "survey"] {
                if ui.selectable_label(cur == id, tl!(angles_label(id))).on_hover_text(tl!(angles_hint(id))).clicked() && cur != id {
                    plot["angles"] = json!(id);
                    // Surveyors measure in gon: offer it with their convention.
                    if id == "survey" && plot["angleUnit"].as_str().unwrap_or("rad") == "rad" {
                        plot["angleUnit"] = json!("gon");
                    }
                    changed = true;
                }
            }
        });
        ui.end_row();

        ui.label(tl!("Angle unit"));
        let unit = plot["angleUnit"].as_str().unwrap_or("rad").to_string();
        craft_ui::ComboBox::from_id_salt("plot_angle_unit").selected_text(tl!(unit_label(&unit))).width(190.0).show_ui(ui, |ui| {
            for id in ["rad", "deg", "gon"] {
                if ui.selectable_label(unit == id, tl!(unit_label(id))).clicked() && unit != id {
                    plot["angleUnit"] = json!(id);
                    changed = true;
                }
            }
        });
        ui.end_row();
    });
    let mut mark = plot["angleMark"].as_bool().unwrap_or(false);
    if ui
        .checkbox(&mut mark, tl!("Show angle direction"))
        .on_hover_text(tl!("Draw an arc with an arrow from the reference axis in the direction angles grow"))
        .changed()
    {
        plot["angleMark"] = json!(mark);
        changed = true;
    }
    changed
}

fn angles_label(id: &str) -> &'static str {
    match id {
        "survey" => "Surveying (clockwise)",
        _ => "Mathematical (counter-clockwise)",
    }
}

fn angles_hint(id: &str) -> &'static str {
    match id {
        "survey" => "Angles start from the positive ordinate and turn clockwise, like a bearing from north",
        _ => "Angles start from the positive abscissa and turn counter-clockwise",
    }
}

fn unit_label(id: &str) -> &'static str {
    match id {
        "deg" => "Degrees (°)",
        "gon" => "Gradians (gon)",
        _ => "Radians (rad)",
    }
}

/// The axes' names as the formulas use them (`Plot::x_name`, `Plot::y_name`).
fn axis_names(plot: &Value) -> (String, String) {
    let name = |k: &str, fallback: &str| {
        let s = plot[k].as_str().unwrap_or_default().trim().to_string();
        let ok = s.chars().count() <= 24 && s.chars().next().is_some_and(char::is_alphabetic) && s.chars().all(|c| c.is_alphanumeric() || c == '_');
        if ok { s } else { fallback.to_string() }
    };
    (name("xLabel", "x"), name("yLabel", "y"))
}

fn series_section(app: &mut WordApp, ui: &mut Ui, plot: &mut Value, t: &Tokens) -> bool {
    let mut changed = false;
    let names = axis_names(plot);
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(tl!("Series")).strong());
        let next_idx = plot["series"].as_array().map_or(0, Vec::len);
        let col = SERIES_COLORS[next_idx % SERIES_COLORS.len()].0;
        if ui.small_button(format!("+ {}", tl!("Function"))).clicked() {
            let x = &names.0;
            push_series(plot, json!({"type": "function", "label": format!("f({x})"), "expr": format!("sin({x})"), "var": x, "color": col, "width": 0.5}));
            changed = true;
        }
        if ui.small_button(format!("+ {}", tl!("Parametric"))).clicked() {
            push_series(
                plot,
                json!({"type": "parametric", "label": "γ(t)", "xExpr": "cos(t)", "yExpr": "sin(t)", "tMin": 0.0, "tMax": std::f64::consts::TAU, "color": col, "width": 0.5}),
            );
            changed = true;
        }
        if ui.small_button(format!("+ {}", tl!("Polar"))).clicked() {
            push_series(plot, json!({"type": "polar", "label": "ρ(θ)", "expr": "1 + cos(θ)", "color": col, "width": 0.5}));
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
                let (c, rm) = one_series_card(app, ui, i, s, &names);
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

fn one_series_card(app: &mut WordApp, ui: &mut Ui, i: usize, s: &mut Value, names: &(String, String)) -> (bool, bool) {
    let mut changed = false;
    let mut remove = false;
    let kind = s["type"].as_str().unwrap_or("function").to_string();
    let theme = app.session.doc.settings.theme_colors.clone();
    ui.horizontal(|ui| {
        if let Some(c) = color_picker(ui, i, &theme, current_rgb(&s["color"])) {
            s["color"] = json!(c);
            changed = true;
        }
        let lbl = s["label"].as_str().unwrap_or_default().to_string();
        if let Some(v) = sized_box(ui, ("s_lbl", i), &lbl, 26.0) {
            s["label"] = json!(v);
            changed = true;
        }
        if ui.small_button("×").on_hover_text(tl!("Remove series")).clicked() {
            remove = true;
        }
    });
    changed |= match kind.as_str() {
        "parametric" => parametric_fields(app, ui, i, s, names),
        "polar" => polar_fields(app, ui, i, s),
        "data" => data_fields(ui, i, s),
        _ => function_fields(app, ui, i, s, names),
    };
    changed |= line_fields(ui, i, s);
    ui.horizontal_wrapped(|ui| {
        let mut ans = s["answer"].as_bool().unwrap_or(false);
        if ui.checkbox(&mut ans, tl!("Solution only")).changed() {
            s["answer"] = json!(ans);
            changed = true;
        }
    });
    (changed, remove)
}

/// The line's style, as four samples drawn in the series' colour (solid, dashed, dotted,
/// dash-dot; the chosen one framed), and its thickness in millimetres.
fn line_fields(ui: &mut Ui, i: usize, s: &mut Value) -> bool {
    const STYLES: [(&str, &str, &[f32]); 4] = [
        ("solid", "Solid line", &[]),
        ("dashed", "Dashed line", &[7.0, 4.0]),
        ("dotted", "Dotted line", &[2.0, 3.0]),
        ("dashDot", "Dash-dot line", &[7.0, 3.0, 2.0, 3.0]),
    ];
    let mut changed = false;
    // Older plots say `dashed`; the engine reads it as the dashed style.
    let cur = match s["style"].as_str() {
        Some(st) => st.to_string(),
        None if s["dashed"].as_bool().unwrap_or(false) => "dashed".to_string(),
        None => "solid".to_string(),
    };
    let rgb = current_rgb(&s["color"]);
    let ink = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(tl!("Line"));
        for (id, name, pattern) in STYLES {
            let (r, resp) = ui.allocate_exact_size(vec2(34.0, 18.0), egui::Sense::click());
            let selected = cur == id;
            if selected {
                ui.painter().rect(r, 3.0, t.hover, Stroke::new(1.0, t.accent), egui::StrokeKind::Inside);
            } else if resp.hovered() {
                ui.painter().rect_filled(r, 3.0, t.hover);
            }
            let (x0, x1, y) = (r.min.x + 5.0, r.max.x - 5.0, r.center().y);
            let stroke = Stroke::new(2.0, ink);
            if pattern.is_empty() {
                ui.painter().line_segment([egui::pos2(x0, y), egui::pos2(x1, y)], stroke);
            } else {
                let mut x = x0;
                let mut k = 0;
                while x < x1 {
                    let len = pattern.get(k % pattern.len()).copied().unwrap_or(4.0);
                    if k % 2 == 0 {
                        ui.painter().line_segment([egui::pos2(x, y), egui::pos2((x + len).min(x1), y)], stroke);
                    }
                    x += len;
                    k += 1;
                }
            }
            if resp.on_hover_text(tl!(name)).clicked() && !selected {
                s["style"] = json!(id);
                if let Some(obj) = s.as_object_mut() {
                    obj.remove("dashed");
                }
                changed = true;
            }
        }
        ui.add_space(4.0);
        let w = s["width"].as_f64().unwrap_or(0.45);
        if let Some(v) = num_drag(ui, ("lw", i), w, 0.1..=4.0, " mm") {
            s["width"] = json!(v);
            changed = true;
        }
    });
    changed
}

fn current_rgb(v: &Value) -> [u8; 3] {
    let Some(a) = v.as_array() else { return SERIES_COLORS[0].0 };
    if a.len() < 3 {
        return SERIES_COLORS[0].0;
    }
    [a[0].as_u64().unwrap_or(0x25) as u8, a[1].as_u64().unwrap_or(0x63) as u8, a[2].as_u64().unwrap_or(0xEB) as u8]
}

/// A series' colour: the colour drop-down every colour uses (theme and standard colours, Recent
/// Colors, More Colors…).
fn color_picker(ui: &mut Ui, idx: usize, theme: &[wordcraft_doc::Rgb], cur: [u8; 3]) -> Option<[u8; 3]> {
    let now = wordcraft_doc::Rgb(cur[0], cur[1], cur[2]);
    match crate::color_palette::color_button(ui, ("plot_scol", idx), theme, Some(now), "", &Default::default())? {
        crate::color_palette::ButtonPick::Color(c) => Some([c.0, c.1, c.2]),
        crate::color_palette::ButtonPick::Top(_) => None,
    }
}

/// A formula field: the formula in the Equation's linear format (`x^2`, `√x`, `(a)/(b)`,
/// `sin⁡x`), as calc reads equations, and under it the formula built up by the math engine that
/// sets the document's equations, `lhs = formula`.
fn formula_field(app: &mut WordApp, ui: &mut Ui, key: (&str, usize), lhs: &str, current: &str) -> Option<String> {
    let mut out = None;
    ui.horizontal(|ui| {
        ui.label(format!("{lhs} ="));
        out = text_box(ui, key, current);
    });
    let shown = out.as_deref().unwrap_or(current);
    if !shown.trim().is_empty() {
        let linear = format!("{lhs}={shown}");
        let w = ui.available_width().clamp(40.0, 400.0);
        let (r, _) = ui.allocate_exact_size(vec2(w, 26.0), egui::Sense::hover());
        let nodes = wordcraft_doc::math::parse_linear(&linear);
        let cache_key = format!("plot:{}:{}:{linear}", key.0, key.1);
        match crate::previews::math_texture(app, ui.ctx(), &cache_key, &nodes, false, 12.0, r.size()) {
            Some((tex, uv)) => {
                ui.painter().image(tex, r, uv, Tokens::get(ui.ctx()).text);
            }
            None => {
                let t = Tokens::get(ui.ctx());
                ui.painter().with_clip_rect(r).text(r.center(), egui::Align2::CENTER_CENTER, &linear, crate::theme::regular(11.0), t.text_dim);
            }
        }
    }
    out
}

fn function_fields(app: &mut WordApp, ui: &mut Ui, i: usize, s: &mut Value, names: &(String, String)) -> bool {
    let mut changed = false;
    let expr = s["expr"].as_str().unwrap_or("x").to_string();
    if let Some(v) = formula_field(app, ui, ("f_expr", i), &names.1, &expr) {
        s["expr"] = json!(v);
        changed = true;
    }
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

fn parametric_fields(app: &mut WordApp, ui: &mut Ui, i: usize, s: &mut Value, names: &(String, String)) -> bool {
    let mut changed = false;
    for (name, k, tag) in [(&names.0, "xExpr", "px"), (&names.1, "yExpr", "py")] {
        let cur = s[k].as_str().unwrap_or("t").to_string();
        if let Some(v) = formula_field(app, ui, (tag, i), &format!("{name}(t)"), &cur) {
            s[k] = json!(v);
            changed = true;
        }
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

fn polar_fields(app: &mut WordApp, ui: &mut Ui, i: usize, s: &mut Value) -> bool {
    let mut changed = false;
    let cur = s["expr"].as_str().unwrap_or("1").to_string();
    if let Some(v) = formula_field(app, ui, ("pol", i), "ρ", &cur) {
        s["expr"] = json!(v);
        changed = true;
    }
    ui.horizontal(|ui| {
        let mut whole = s["theta"].as_array().is_none_or(|a| a.len() != 2);
        if ui.checkbox(&mut whole, tl!("Full turn")).changed() {
            if whole {
                if let Some(obj) = s.as_object_mut() {
                    obj.remove("theta");
                }
            } else {
                s["theta"] = json!([0.0, 1.0]);
            }
            changed = true;
        }
        if let Some([a, b]) = s["theta"].as_array().filter(|a| a.len() == 2).map(|a| [a[0].as_f64().unwrap_or(0.0), a[1].as_f64().unwrap_or(1.0)]) {
            ui.label("θ =");
            if let Some(v) = num_drag(ui, ("th0", i), a, -1e6..=1e6, "") {
                s["theta"] = json!([v, b]);
                changed = true;
            }
            ui.label("..");
            if let Some(v) = num_drag(ui, ("th1", i), b, -1e6..=1e6, "") {
                s["theta"] = json!([a, v]);
                changed = true;
            }
        }
    });
    changed
}

fn data_fields(ui: &mut Ui, i: usize, s: &mut Value) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        // Two equal boxes: the first leaves half the row (and the second label) to the other.
        let half = ((ui.available_width() - 34.0) * 0.5).max(32.0);
        ui.label("X:");
        let xc = s["xCol"].as_str().unwrap_or("t").to_string();
        if let Some(v) = sized_box(ui, ("xcol", i), &xc, half + 18.0) {
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
        ui.label(crate::i18n::tc("plot", "Fit"));
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

/// Writes the analysis after the plot as live calculations (`plot.analysis`), in the interface's
/// language.
fn insert_analysis_button(app: &mut WordApp, ui: &mut Ui, plot: &Value) {
    let functions = plot["series"].as_array().is_some_and(|all| all.iter().any(|s| s["type"] == "function"));
    if !functions {
        return;
    }
    ui.add_space(4.0);
    let button = ui
        .button(tl!("Insert Analysis in Document"))
        .on_hover_text(tl!("Writes after the plot each function's definition and live calculations of its zeros, extrema, tangent and integral"));
    if button.clicked() {
        let labels = json!({
            "zeros": tl!("Zeros"),
            "minimum": tl!("Minimum"),
            "maximum": tl!("Maximum"),
            "tangent": tl!("Tangent"),
            "integral": tl!("Area"),
        });
        let _ = app.run("plot.analysis", json!({ "labels": labels }));
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
