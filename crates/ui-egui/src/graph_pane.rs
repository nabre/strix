//! The Graph Paper pane (Insert › Pages › Graph Paper › Properties, docs/graph-paper.md): the
//! graph paper at the caret — where it goes, its squares, its lines and its heavier lines —
//! with a preview on the paper's white. Every change is one `graph.set` (one undo step); numbers
//! dragged are applied when the drag ends.

use egui::{Color32, RichText, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};

use crate::WordApp;

/// The placements, as `graph.set` names them, with their labels.
const PLACEMENTS: [(&str, &str); 6] = [
    ("pageEnd", "To the end of the page"),
    ("background", "Page background"),
    ("untilOdd", "Until the next odd page"),
    ("untilEven", "Until the next even page"),
    ("pages", "For a number of pages"),
    ("height", "For a height"),
];

const STYLES: [(&str, &str); 3] = [("solid", "Continuous"), ("dashed", "Dashed"), ("dotted", "Dotted")];

/// Colours offered for the lines (our own picks, the usual inks of graph paper); any other is
/// typed as `#rrggbb`.
const COLORS: [([u8; 3], &str); 8] = [
    ([0x9F, 0xC5, 0xE8], "Light blue"),
    ([0x6A, 0x9E, 0xD0], "Blue"),
    ([0xA8, 0xD5, 0xB0], "Light green"),
    ([0x4F, 0x9D, 0x69], "Green"),
    ([0xF2, 0xB8, 0x8C], "Light orange"),
    ([0xE0, 0x7B, 0x8B], "Pink"),
    ([0xC8, 0xC8, 0xC8], "Light grey"),
    ([0x80, 0x80, 0x80], "Grey"),
];

fn rgb(hex: &str) -> [u8; 3] {
    crate::cad_panel::parse_hex(hex).unwrap_or([0x9F, 0xC5, 0xE8])
}

fn color32(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// A number field applied when it is let go: a drag shows its value as it goes and is applied
/// when it ends; typing is applied with Enter or on leaving the field.
fn num_field(ui: &mut Ui, key: &str, current: f64, range: std::ops::RangeInclusive<f64>, suffix: &str, decimals: usize) -> Option<f64> {
    let id = ui.scope_id().with(("graph_num", key));
    let mut v = ui.data(|d| d.get_temp::<f64>(id)).unwrap_or(current);
    let speed = if decimals == 0 { 0.1 } else { 0.05 };
    let r = ui.add(egui::DragValue::new(&mut v).range(range).suffix(suffix).speed(speed).fixed_decimals(decimals).update_while_editing(false));
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

/// A colour combo: the swatches, and any colour typed as `#rrggbb`.
fn color_combo(ui: &mut Ui, key: &str, current: [u8; 3]) -> Option<[u8; 3]> {
    let mut out = None;
    let name = COLORS
        .iter()
        .find(|(c, _)| *c == current)
        .map_or_else(|| format!("#{:02x}{:02x}{:02x}", current[0], current[1], current[2]), |(_, n)| tl!(n).to_string());
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), egui::Sense::hover());
        ui.painter().rect_filled(r, 2.0, color32(current));
        ui.painter().rect_stroke(r, 2.0, Stroke::new(0.5, Color32::from_gray(140)), egui::StrokeKind::Inside);
        craft_ui::ComboBox::from_id_salt(("graph_color", key)).selected_text(name).width(130.0).show_ui(ui, |ui| {
            for &(c, n) in &COLORS {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), egui::Sense::hover());
                    ui.painter().rect_filled(r, 2.0, color32(c));
                    if ui.selectable_label(c == current, tl!(n)).clicked() {
                        out = Some(c);
                    }
                });
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("#");
                let shown = format!("{:02x}{:02x}{:02x}", current[0], current[1], current[2]);
                if let Some(t) = crate::a11y_pane::draft_box(ui, ("graph_hex", key), &shown, true, false)
                    && let Some(c) = crate::cad_panel::parse_hex(&t)
                {
                    out = Some(c);
                }
            });
        });
    });
    out
}

fn style_combo(ui: &mut Ui, key: &str, current: &str) -> Option<&'static str> {
    let mut out = None;
    let label = STYLES.iter().find(|(k, _)| *k == current).map_or("Continuous", |(_, l)| l);
    craft_ui::ComboBox::from_id_salt(("graph_style", key)).selected_text(tl!(label)).width(130.0).show_ui(ui, |ui| {
        for (k, l) in STYLES {
            if ui.selectable_label(k == current, tl!(l)).clicked() {
                out = Some(k);
            }
        }
    });
    out
}

/// A preview of the grid on the paper's white: 3 px a millimetre, so 5 mm squares look about as
/// they do at 100 %; the line widths and dashes as on the page.
fn preview(ui: &mut Ui, g: &Value) {
    let w = ui.available_width().clamp(120.0, 280.0);
    let (r, _) = ui.allocate_exact_size(vec2(w, 96.0), egui::Sense::hover());
    let painter = ui.painter_at(r);
    painter.rect_filled(r, 2.0, crate::cad_editor::SHEET);
    let px_mm = 3.0_f32;
    let spacing = (g["spacing"].as_f64().unwrap_or(5.0) as f32 * px_mm).max(2.0);
    let every = g["majorEvery"].as_u64().unwrap_or(0);
    let line = |style: &str, color: &str, width: f64| (style.to_string(), color32(rgb(color)), (width as f32 * px_mm * 25.4 / 72.0).clamp(0.5, 6.0));
    let minor = line(g["style"].as_str().unwrap_or("solid"), g["color"].as_str().unwrap_or(""), g["width"].as_f64().unwrap_or(0.4));
    let major = line(g["majorStyle"].as_str().unwrap_or("solid"), g["majorColor"].as_str().unwrap_or(""), g["majorWidth"].as_f64().unwrap_or(0.9));
    let seg = |a: egui::Pos2, b: egui::Pos2, (style, color, width): &(String, Color32, f32)| {
        let stroke = Stroke::new(*width, *color);
        let (on, off) = match style.as_str() {
            "dashed" => ((width * 6.0).max(4.0), (width * 4.0).max(3.0)),
            "dotted" => (width.max(1.0), (width * 3.0).max(2.5)),
            _ => (f32::INFINITY, 0.0),
        };
        if on.is_infinite() {
            painter.line_segment([a, b], stroke);
            return;
        }
        let len = a.distance(b);
        let dir = (b - a) / len.max(0.001);
        let mut t = 0.0;
        while t < len {
            painter.line_segment([a + dir * t, a + dir * (t + on).min(len)], stroke);
            t += on + off;
        }
    };
    // Whole squares only, as on the page, a few pixels in from the preview's edge.
    let inner = r.shrink(4.0);
    let (nx, ny) = ((inner.width() / spacing).floor().min(400.0), (inner.height() / spacing).floor().min(400.0));
    let area = egui::Rect::from_min_size(inner.min, vec2(nx * spacing, ny * spacing));
    let framed = g["frameCustom"].as_bool().unwrap_or(false);
    // The ordinary lines first, the heavier ones over them, from the top-left corner; a frame of
    // its own weight replaces the edge lines.
    for pass_major in [false, true] {
        let mut k = 0u64;
        loop {
            let at = k as f32 * spacing;
            if at > area.width().max(area.height()) + 0.01 || k > 400 {
                break;
            }
            let is_major = every > 0 && k.is_multiple_of(every);
            if is_major == pass_major {
                let l = if is_major { &major } else { &minor };
                let x_edge = framed && (k == 0 || (k as f32 - nx).abs() < 0.5);
                let y_edge = framed && (k == 0 || (k as f32 - ny).abs() < 0.5);
                if at <= area.width() + 0.01 && !x_edge {
                    seg(pos2(area.left() + at, area.top()), pos2(area.left() + at, area.bottom()), l);
                }
                if at <= area.height() + 0.01 && !y_edge {
                    seg(pos2(area.left(), area.top() + at), pos2(area.right(), area.top() + at), l);
                }
            }
            k += 1;
        }
    }
    if framed {
        let fw = (g["frameWidth"].as_f64().unwrap_or(0.4) as f32 * px_mm * 25.4 / 72.0).clamp(0.5, 8.0);
        let frame = (minor.0.clone(), minor.1, fw);
        let [a, b, c, d] = [area.left_top(), area.right_top(), area.right_bottom(), area.left_bottom()];
        for (p, q) in [(a, b), (b, c), (c, d), (d, a)] {
            seg(p, q, &frame);
        }
    }
    let edge = crate::theme::Tokens::get(ui.ctx()).border;
    ui.painter().rect_stroke(r, 2.0, Stroke::new(0.5, edge), egui::StrokeKind::Inside);
}

pub fn pane(app: &mut WordApp, ui: &mut Ui) {
    let grid = app.session.run("graph.get", &json!({})).ok().map(|v| v["grid"].clone()).filter(|g| !g.is_null());
    let Some(g) = grid else {
        ui.label(RichText::new(tl!("Put the cursor in graph paper (or on a page with a graph paper background) to see its properties.")).weak());
        ui.add_space(8.0);
        if ui.button(tl!("Insert Graph Paper")).on_hover_text(tl!("A grid from the cursor to the end of the page")).clicked() {
            let _ = app.run("graph.insert", json!({}));
        }
        return;
    };
    let mut change: Option<Value> = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        preview(ui, &g);
        ui.add_space(8.0);
        egui::Grid::new("graph_props").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label(tl!("Placement"));
            let current = g["placement"].as_str().unwrap_or("pageEnd");
            let label = PLACEMENTS.iter().find(|(k, _)| *k == current).map_or("", |(_, l)| l);
            craft_ui::ComboBox::from_id_salt("graph_placement").selected_text(tl!(label)).width(170.0).show_ui(ui, |ui| {
                for (k, l) in PLACEMENTS {
                    if ui.selectable_label(k == current, tl!(l)).clicked() && k != current {
                        change = Some(json!({"placement": k}));
                    }
                }
            });
            ui.end_row();
            if current == "pages" {
                ui.label(tl!("Pages"));
                if let Some(v) = num_field(ui, "pages", g["pages"].as_f64().unwrap_or(2.0), 1.0..=100.0, "", 0) {
                    change = Some(json!({"pages": v}));
                }
                ui.end_row();
            }
            if current == "height" {
                ui.label(tl!("Height"));
                if let Some(v) = num_field(ui, "height", g["height"].as_f64().unwrap_or(50.0), 1.0..=2000.0, " mm", 0) {
                    change = Some(json!({"height": v}));
                }
                ui.end_row();
            }
            ui.label(tl!("Square size"));
            if let Some(v) = num_field(ui, "spacing", g["spacing"].as_f64().unwrap_or(5.0), 1.0..=100.0, " mm", 1) {
                change = Some(json!({"spacing": v}));
            }
            ui.end_row();
            ui.label(tl!("Columns"));
            let mut follow = g["followColumns"].as_bool().unwrap_or(false);
            if ui
                .checkbox(&mut follow, tl!("Follow columns"))
                .on_hover_text(tl!(
                    "In a section with more than one column, each column is ruled on its own and the space between them is left clear"
                ))
                .changed()
            {
                change = Some(json!({"followColumns": follow}));
            }
            ui.end_row();
        });
        ui.add_space(10.0);
        ui.label(RichText::new(tl!("Grid lines")).strong());
        egui::Grid::new("graph_lines").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label(tl!("Style"));
            if let Some(s) = style_combo(ui, "line", g["style"].as_str().unwrap_or("solid")) {
                change = Some(json!({"style": s}));
            }
            ui.end_row();
            ui.label(tl!("Colour"));
            if let Some(c) = color_combo(ui, "line", rgb(g["color"].as_str().unwrap_or(""))) {
                change = Some(json!({"color": format!("{:02X}{:02X}{:02X}", c[0], c[1], c[2])}));
            }
            ui.end_row();
            ui.label(tl!("Weight"));
            if let Some(v) = num_field(ui, "width", g["width"].as_f64().unwrap_or(0.4), 0.1..=6.0, " pt", 2) {
                change = Some(json!({"width": v}));
            }
            ui.end_row();
        });
        ui.add_space(10.0);
        ui.label(RichText::new(tl!("Outer frame")).strong());
        egui::Grid::new("graph_frame").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label(tl!("Weight"));
            ui.horizontal(|ui| {
                if let Some(v) = num_field(ui, "frameWidth", g["frameWidth"].as_f64().unwrap_or(0.4), 0.1..=6.0, " pt", 2) {
                    change = Some(json!({"frameWidth": v}));
                }
                let mut same = !g["frameCustom"].as_bool().unwrap_or(false);
                if ui.checkbox(&mut same, tl!("As the grid lines")).changed() {
                    change = Some(if same { json!({"frameWidth": null}) } else { json!({"frameWidth": g["width"].as_f64().unwrap_or(0.4)}) });
                }
            });
            ui.end_row();
        });
        ui.add_space(10.0);
        ui.label(RichText::new(tl!("Heavier lines")).strong());
        egui::Grid::new("graph_major").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label(tl!("Every"));
            ui.horizontal(|ui| {
                if let Some(v) = num_field(ui, "every", g["majorEvery"].as_f64().unwrap_or(0.0), 0.0..=100.0, "", 0) {
                    change = Some(json!({"majorEvery": v}));
                }
                ui.label(tl!("squares (0: none)"));
            });
            ui.end_row();
            if g["majorEvery"].as_u64().unwrap_or(0) > 0 {
                ui.label(tl!("Style"));
                if let Some(s) = style_combo(ui, "major", g["majorStyle"].as_str().unwrap_or("solid")) {
                    change = Some(json!({"majorStyle": s}));
                }
                ui.end_row();
                ui.label(tl!("Colour"));
                if let Some(c) = color_combo(ui, "major", rgb(g["majorColor"].as_str().unwrap_or(""))) {
                    change = Some(json!({"majorColor": format!("{:02X}{:02X}{:02X}", c[0], c[1], c[2])}));
                }
                ui.end_row();
                ui.label(tl!("Weight"));
                if let Some(v) = num_field(ui, "majorWidth", g["majorWidth"].as_f64().unwrap_or(0.9), 0.1..=6.0, " pt", 2) {
                    change = Some(json!({"majorWidth": v}));
                }
                ui.end_row();
            }
        });
        ui.add_space(12.0);
        if ui.button(tl!("Remove Graph Paper")).clicked() {
            let _ = app.run("graph.remove", json!({}));
        }
        ui.add_space(8.0);
        let note = if g["target"] == "section" {
            "Behind the text of every page of this section."
        } else {
            "A block in the text: write on it, or press Enter at its end to go on below it."
        };
        ui.label(RichText::new(tl!(note)).small().weak());
    });
    if let Some(c) = change {
        let _ = app.run("graph.set", c);
    }
}
