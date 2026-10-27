//! The CAD editor's Properties pane (docs/cad.md), beside the sheet like a CAD's palette:
//!
//! - **Considering** (AutoCAD's Quick Properties): with several objects selected, all of them or
//!   one family ("Line (3)"); ‹ › step through the objects one by one, the counter goes back to
//!   the family; Only These reduces the selection to them. The sheet shows them in orange.
//! - **General**: layer, colour, lineweight and linetype of the objects considered, each "ByLayer"
//!   or their own; "Varies" when they differ.
//! - **Geometry** of one object: coordinates, radii and angles take a number or an expression of
//!   the calculations (`L/2`), which stays linked; a line's length and angle and a circle's
//!   diameter recompute it; a polyline shows one vertex at a time.
//!
//! The **Layer Manager** ([`layer_manager`], docked under the sheet): the current layer (new
//! objects go on it), on, locked, printed, colour, lineweight, linetype, rename, delete, new.
//!
//! Every change is one `cad.set` (one undo step), made by the editor.

use egui::{Color32, RichText, Ui, vec2};
use strix_cad::{Dash, Drawing, FieldKind, Focus, StrokeEdit, families};

use crate::cad_editor::CadEditor;

/// Lineweights offered, paper mm (ISO 128 series).
const WIDTHS: [f64; 9] = [0.13, 0.18, 0.25, 0.35, 0.5, 0.7, 1.0, 1.4, 2.0];
const DASHES: [(Dash, &str); 4] = [(Dash::Solid, "Continuous"), (Dash::Dashed, "Dashed"), (Dash::Dotted, "Dotted"), (Dash::DashDotted, "Dash-dot")];
/// The pens offered (our own picks): fifteen colours, then a grey scale from black to white;
/// any other colour is set by its RGB values or typed `#rrggbb`.
const COLORS: [([u8; 3], &str); 23] = [
    ([220, 38, 38], "Red"),
    ([234, 88, 12], "Orange"),
    ([217, 119, 6], "Amber"),
    ([234, 179, 8], "Yellow"),
    ([101, 163, 13], "Lime"),
    ([22, 163, 74], "Green"),
    ([13, 148, 136], "Teal"),
    ([8, 145, 178], "Cyan"),
    ([2, 132, 199], "Sky blue"),
    ([37, 99, 235], "Blue"),
    ([79, 70, 229], "Indigo"),
    ([124, 58, 237], "Violet"),
    ([192, 38, 211], "Magenta"),
    ([219, 39, 119], "Pink"),
    ([146, 64, 14], "Brown"),
    ([0, 0, 0], "Black"),
    ([51, 51, 51], "Grey 80%"),
    ([89, 89, 89], "Grey 65%"),
    ([128, 128, 128], "Grey 50%"),
    ([166, 166, 166], "Grey 35%"),
    ([204, 204, 204], "Grey 20%"),
    ([230, 230, 230], "Grey 10%"),
    ([255, 255, 255], "White"),
];
/// Pens per row in the colour menu: the colours fill three rows, the greys the last one.
const PENS_PER_ROW: usize = 8;

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn color_name(c: [u8; 3]) -> String {
    COLORS.iter().find(|(x, _)| *x == c).map_or_else(|| hex(c), |(_, n)| tl!(n).to_string())
}

fn swatch(ui: &mut Ui, c: [u8; 3]) {
    let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), egui::Sense::hover());
    ui.painter().rect_filled(r, 2.0, Color32::from_rgb(c[0], c[1], c[2]));
    ui.painter().rect_stroke(r, 2.0, egui::Stroke::new(0.5, Color32::from_gray(140)), egui::StrokeKind::Inside);
}

/// A pen in the colour menu: a swatch to click, ringed when it is the one shown.
fn pen_swatch(ui: &mut Ui, c: [u8; 3], on: bool) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(18.0, 18.0), egui::Sense::click());
    ui.painter().rect_filled(r.shrink(1.0), 2.0, Color32::from_rgb(c[0], c[1], c[2]));
    let edge = if on {
        egui::Stroke::new(2.0, ui.visuals().selection.stroke.color)
    } else if resp.hovered() {
        egui::Stroke::new(1.5, Color32::from_gray(90))
    } else {
        egui::Stroke::new(0.5, Color32::from_gray(140))
    };
    ui.painter().rect_stroke(r, 2.0, edge, egui::StrokeKind::Inside);
    resp
}

/// What a combo of the General section shows: one value, or "Varies".
fn common<T: PartialEq + Clone>(values: impl Iterator<Item = T>) -> Option<Option<T>> {
    let mut out: Option<T> = None;
    for v in values {
        match &out {
            None => out = Some(v),
            Some(o) if *o == v => {}
            Some(_) => return Some(None),
        }
    }
    out.map(Some)
}

/// A text field that applies on Enter or when it loses the keyboard (Esc puts it back);
/// returns the new text when it changed. `typing` is set while it has the keyboard.
fn text_field(ui: &mut Ui, id: egui::Id, shown: &str, width: f32, typing: &mut bool) -> Option<String> {
    let key = id.with("buffer");
    let mut buf = ui.data(|m| m.get_temp::<String>(key)).unwrap_or_else(|| shown.to_string());
    let r = ui.add(egui::TextEdit::singleline(&mut buf).id(id).desired_width(width).min_size(vec2(width, 0.0)));
    if r.has_focus() {
        *typing = true;
        ui.data_mut(|m| m.insert_temp(key, buf));
        return None;
    }
    ui.data_mut(|m| m.remove::<String>(key));
    let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
    (r.lost_focus() && !esc && buf != shown).then_some(buf)
}

/// A colour combo: ByLayer (when offered), the palette, and a typed `#rrggbb`. `Some(None)` is
/// ByLayer.
fn color_combo(ui: &mut Ui, id: &str, shown: Option<Option<[u8; 3]>>, by_layer: bool, typing: &mut bool) -> Option<Option<[u8; 3]>> {
    let mut out = None;
    let text = match shown {
        None => tl!("Varies").to_string(),
        Some(None) => tl!("ByLayer").to_string(),
        Some(Some(c)) => color_name(c),
    };
    ui.horizontal(|ui| {
        if let Some(Some(c)) = shown {
            swatch(ui, c);
        }
        // Clicks inside keep it open (the RGB field takes typing); a pen or Enter applies.
        egui::ComboBox::from_id_salt(id).selected_text(text).width(120.0).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show_ui(
            ui,
            |ui| {
                if by_layer && ui.selectable_label(shown == Some(None), tl!("ByLayer")).clicked() {
                    out = Some(None);
                }
                // The pens as a grid of swatches: the colours, then the greys on a row of their own.
                ui.label(RichText::new(tl!("Pens")).weak().small());
                let (colours, greys) = COLORS.split_at(15);
                for row in colours.chunks(PENS_PER_ROW).chain(std::iter::once(greys)) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
                        for &(c, name) in row {
                            if pen_swatch(ui, c, shown == Some(Some(c))).on_hover_text(tl!(name)).clicked() {
                                out = Some(Some(c));
                            }
                        }
                    });
                }
                ui.separator();
                // Any colour by its RGB values (applied with Enter: one undo step).
                let rgb = match shown {
                    Some(Some(c)) => format!("{}, {}, {}", c[0], c[1], c[2]),
                    _ => String::new(),
                };
                ui.horizontal(|ui| {
                    ui.label("RGB");
                    if let Some(t) = text_field(ui, ui.scope_id().with((id, "rgb")), &rgb, 110.0, typing)
                        && let Some(c) = parse_rgb(&t)
                    {
                        out = Some(Some(c));
                    }
                });
            },
        );
        let typed = match shown {
            Some(Some(c)) => hex(c),
            _ => String::new(),
        };
        if let Some(t) = text_field(ui, ui.scope_id().with((id, "hex")), &typed, 64.0, typing)
            && let Some(c) = parse_hex(&t)
        {
            out = Some(Some(c));
        }
    });
    out
}

fn parse_hex(c: &str) -> Option<[u8; 3]> {
    let h = c.trim().trim_start_matches('#');
    if h.len() != 6 || !h.is_ascii() {
        return None;
    }
    let byte = |i: usize| h.get(i..i + 2).and_then(|x| u8::from_str_radix(x, 16).ok());
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// `r, g, b` (0–255 each, commas or spaces), or `#rrggbb`.
fn parse_rgb(t: &str) -> Option<[u8; 3]> {
    if t.trim().starts_with('#') {
        return parse_hex(t);
    }
    let parts: Vec<u8> = t.split([',', ' ', ';']).filter(|s| !s.is_empty()).map(|s| s.trim().parse::<u8>().ok()).collect::<Option<_>>()?;
    match parts.as_slice() {
        [r, g, b] => Some([*r, *g, *b]),
        _ => None,
    }
}

fn width_text(w: f64) -> String {
    format!("{w:.2} mm")
}

/// A lineweight combo; `Some(None)` is ByLayer.
fn width_combo(ui: &mut Ui, id: &str, shown: Option<Option<f64>>, by_layer: bool) -> Option<Option<f64>> {
    let mut out = None;
    let text = match shown {
        None => tl!("Varies").to_string(),
        Some(None) => tl!("ByLayer").to_string(),
        Some(Some(w)) => width_text(w),
    };
    egui::ComboBox::from_id_salt(id).selected_text(text).width(120.0).show_ui(ui, |ui| {
        if by_layer && ui.selectable_label(shown == Some(None), tl!("ByLayer")).clicked() {
            out = Some(None);
        }
        for w in WIDTHS {
            if ui.selectable_label(shown == Some(Some(w)), width_text(w)).clicked() {
                out = Some(Some(w));
            }
        }
    });
    out
}

/// A linetype combo; `Some(None)` is ByLayer.
fn dash_combo(ui: &mut Ui, id: &str, shown: Option<Option<Dash>>, by_layer: bool) -> Option<Option<Dash>> {
    let mut out = None;
    let name = |d: Dash| DASHES.iter().find(|(x, _)| *x == d).map_or("", |(_, n)| n);
    let text = match shown {
        None => tl!("Varies").to_string(),
        Some(None) => tl!("ByLayer").to_string(),
        Some(Some(d)) => tl!(name(d)).to_string(),
    };
    egui::ComboBox::from_id_salt(id).selected_text(text).width(120.0).show_ui(ui, |ui| {
        if by_layer && ui.selectable_label(shown == Some(None), tl!("ByLayer")).clicked() {
            out = Some(None);
        }
        for (d, n) in DASHES {
            if ui.selectable_label(shown == Some(Some(d)), tl!(n)).clicked() {
                out = Some(Some(d));
            }
        }
    });
    out
}

/// The pane; returns the drawing to store when something changed.
pub(crate) fn show(ed: &mut CadEditor, ui: &mut Ui) -> Option<Drawing> {
    let mut out = None;
    let mut typing = false;
    egui::ScrollArea::vertical().id_salt("cad_properties").auto_shrink([false, false]).show(ui, |ui| {
        ui.label(RichText::new(tl!("Properties")).strong().size(14.0));
        ui.add_space(2.0);
        if ed.selection.is_empty() {
            ui.label(RichText::new(tl!("No selection: click objects, or drag a window")).weak());
            let d = &ed.drawing;
            egui::Grid::new("cad_drawing_info").num_columns(2).spacing(vec2(8.0, 3.0)).show(ui, |ui| {
                ui.label(tl!("Unit"));
                ui.label(d.unit.symbol());
                ui.end_row();
                ui.label(tl!("Scale"));
                ui.label(format!("1:{}", strix_cad::num_text(&strix_cad::Num::new(d.scale))));
                ui.end_row();
                ui.label(tl!("Objects"));
                ui.label(d.entities.len().to_string());
                ui.end_row();
            });
        } else {
            out = objects(ed, ui, &mut typing);
        }
    });
    ed.panel_typing = typing;
    out
}

/// The selection's properties.
fn objects(ed: &mut CadEditor, ui: &mut Ui, typing: &mut bool) -> Option<Drawing> {
    let sel = ed.selection.clone();
    let (scope, idx) = ed.focus.scope(&ed.drawing, &sel);
    ed.focus = scope;
    let fams = families(&ed.drawing, &sel);
    if sel.len() > 1 {
        // Considering: all, a family, or one object stepping with ‹ ›.
        let whole = scope.family.is_none() || scope.across;
        let all_label = format!("{} ({})", tl!("All"), sel.len());
        let shown = match scope.family.and_then(|f| fams.iter().find(|(g, _)| *g == f)) {
            Some((f, v)) if !scope.across => format!("{} ({})", tl!(f.name()), v.len()),
            _ => all_label.clone(),
        };
        ui.horizontal(|ui| {
            ui.label(tl!("Considering"));
            egui::ComboBox::from_id_salt("cad_focus").selected_text(shown).width(150.0).show_ui(ui, |ui| {
                if ui.selectable_label(whole, &all_label).clicked() {
                    ed.focus = Focus::default();
                }
                for (f, v) in &fams {
                    if ui.selectable_label(scope.family == Some(*f) && !scope.across, format!("{} ({})", tl!(f.name()), v.len())).clicked() {
                        ed.focus = Focus { family: Some(*f), object: None, across: false };
                    }
                }
            });
        });
        let order: Vec<usize> = if whole {
            fams.iter().flat_map(|(_, v)| v.iter().copied()).collect()
        } else {
            fams.iter().find(|(f, _)| Some(*f) == scope.family).map(|(_, v)| v.clone()).unwrap_or_default()
        };
        ui.horizontal(|ui| {
            if ui.button("‹").on_hover_text(tl!("Previous object")).clicked() {
                ed.focus = scope.step(&ed.drawing, &sel, false);
            }
            let mid = match scope.object.and_then(|i| order.iter().position(|&x| x == i)) {
                Some(k) => format!("{} {} {}", k + 1, tl!("of"), order.len()),
                None => tl!("All").to_string(),
            };
            let back = if scope.across || scope.family.is_none() { tl!("All the selection") } else { tl!("The whole family") };
            if ui.button(mid).on_hover_text(back).clicked() {
                ed.focus = if whole { Focus::default() } else { Focus { family: scope.family, object: None, across: false } };
            }
            if ui.button("›").on_hover_text(tl!("Next object")).clicked() {
                ed.focus = scope.step(&ed.drawing, &sel, true);
            }
            let only = ui.add_enabled(idx.len() < sel.len(), egui::Button::new(tl!("Only these")));
            if only.on_hover_text(tl!("Reduce the selection to the objects considered")).clicked() {
                ed.replace_selection(idx.clone());
            }
        });
        ui.add_space(4.0);
    }
    let d0 = &ed.drawing;
    let title = match idx.as_slice() {
        [i] => d0.entities.get(*i).map(|e| format!("{} · {}", tl!(strix_cad::Family::of(&e.kind).name()), e.id)).unwrap_or_default(),
        _ => format!("{} {}", idx.len(), tl!("objects")),
    };
    ui.label(RichText::new(title).strong());
    let mut d = ed.drawing.clone();
    let mut changed = false;
    // General.
    let ents: Vec<&strix_cad::Entity> = idx.iter().filter_map(|&i| ed.drawing.entities.get(i)).collect();
    egui::Grid::new("cad_general").num_columns(2).spacing(vec2(8.0, 4.0)).show(ui, |ui| {
        ui.label(tl!("Layer"));
        let layer = common(ents.iter().map(|e| e.layer.clone()));
        let text = match &layer {
            Some(Some(l)) => l.clone(),
            _ => tl!("Varies").to_string(),
        };
        egui::ComboBox::from_id_salt("cad_obj_layer").selected_text(text).width(150.0).show_ui(ui, |ui| {
            for l in &ed.drawing.layers {
                ui.horizontal(|ui| {
                    swatch(ui, l.color);
                    if ui.selectable_label(layer.as_ref() == Some(&Some(l.name.clone())), &l.name).clicked() && d.set_layer(&idx, &l.name).is_ok() {
                        changed = true;
                    }
                });
            }
        });
        ui.end_row();
        ui.label(tl!("Colour"));
        if let Some(c) = color_combo(ui, "cad_obj_color", common(ents.iter().map(|e| e.stroke.color)).flatten(), true, typing) {
            d.set_stroke(&idx, StrokeEdit { color: Some(c), ..StrokeEdit::default() });
            changed = true;
        }
        ui.end_row();
        ui.label(tl!("Lineweight"));
        if let Some(w) = width_combo(ui, "cad_obj_width", common(ents.iter().map(|e| e.stroke.width)).flatten(), true) {
            d.set_stroke(&idx, StrokeEdit { width: Some(w), ..StrokeEdit::default() });
            changed = true;
        }
        ui.end_row();
        ui.label(tl!("Linetype"));
        if let Some(k) = dash_combo(ui, "cad_obj_dash", common(ents.iter().map(|e| e.stroke.dash)).flatten(), true) {
            d.set_stroke(&idx, StrokeEdit { dash: Some(k), ..StrokeEdit::default() });
            changed = true;
        }
        ui.end_row();
    });
    // Geometry of one object.
    if let [i] = idx.as_slice() {
        let i = *i;
        let unit = ed.drawing.unit.symbol();
        ui.add_space(4.0);
        ui.label(RichText::new(tl!("Geometry")).strong());
        if let Some(strix_cad::Kind::Polyline { pts, .. }) = ed.drawing.entities.get(i).map(|e| &e.kind) {
            let n = pts.len();
            ed.vertex = ed.vertex.min(n.saturating_sub(1));
            ui.horizontal(|ui| {
                if ui.add_enabled(ed.vertex > 0, egui::Button::new("‹")).clicked() {
                    ed.vertex -= 1;
                }
                ui.label(format!("{} {} {} {n}", tl!("Vertex"), ed.vertex + 1, tl!("of")));
                if ui.add_enabled(ed.vertex + 1 < n, egui::Button::new("›")).clicked() {
                    ed.vertex += 1;
                }
            });
        }
        let id = ed.drawing.entities.get(i).map(|e| e.id.clone()).unwrap_or_default();
        egui::Grid::new("cad_geometry").num_columns(3).spacing(vec2(6.0, 4.0)).show(ui, |ui| {
            for f in ed.drawing.fields(i, ed.vertex) {
                ui.label(tl!(f.label));
                let new = match f.kind {
                    FieldKind::Bool => {
                        let mut on = f.text == "true";
                        ui.checkbox(&mut on, "").changed().then(|| on.to_string())
                    }
                    FieldKind::Orient => {
                        let mut pick = None;
                        egui::ComboBox::from_id_salt(("cad_orient", &id)).selected_text(tl!(capital(&f.text))).show_ui(ui, |ui| {
                            for o in ["aligned", "horizontal", "vertical"] {
                                if ui.selectable_label(f.text == o, tl!(capital(o))).clicked() {
                                    pick = Some(o.to_string());
                                }
                            }
                        });
                        pick
                    }
                    _ => text_field(ui, ui.scope_id().with(("cad_field", &id, &f.key, ed.vertex)), &f.text, 120.0, typing),
                };
                let suffix = match (f.kind, f.key.as_str()) {
                    (FieldKind::Angle, _) | (FieldKind::Derived, "angle" | "rotation") => "°",
                    (FieldKind::Length | FieldKind::Derived, _) => unit,
                    (FieldKind::Paper, _) => "mm",
                    _ => "",
                };
                ui.label(RichText::new(suffix).weak());
                ui.end_row();
                if let Some(v) = new {
                    match d.set_field(i, ed.vertex, &f.key, &v) {
                        Ok(()) => {
                            changed = true;
                            ed.message.clear();
                        }
                        Err(e) => ed.message = e,
                    }
                }
            }
        });
        ui.label(RichText::new(tl!("Coordinates take a number or an expression of the calculations (L/2)")).weak().small());
    }
    changed.then_some(d)
}

fn capital(s: &str) -> &'static str {
    match s {
        "aligned" => "Aligned",
        "horizontal" => "Horizontal",
        _ => "Vertical",
    }
}

/// The Layer Manager (docked under the sheet, like a CAD's layer palette): one row per layer,
/// sorted by name with `0` first: current, name, on, locked, printed, colour, lineweight,
/// linetype, objects, delete; New Layer and Set Current above. Returns the drawing to store when
/// something changed.
pub(crate) fn layer_manager(ed: &mut CadEditor, ui: &mut Ui) -> Option<Drawing> {
    let mut d = ed.drawing.clone();
    let mut changed = false;
    let mut typing = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(tl!("Layer Manager")).strong());
        ui.add_space(12.0);
        if ui.button(tl!("New layer")).on_hover_text(tl!("A new layer, made current")).clicked() {
            let name = d.new_layer_name();
            match d.ensure_layer(&name) {
                Ok(()) => {
                    ed.current_layer = name;
                    changed = true;
                }
                Err(e) => ed.message = e,
            }
        }
        // The selection's layer becomes the current one (AutoCAD's "Make Object's Layer Current").
        let picked = common(ed.selection.iter().filter_map(|&i| ed.drawing.entities.get(i)).map(|e| e.layer.clone())).flatten();
        if ui
            .add_enabled(picked.is_some(), egui::Button::new(tl!("Make object's layer current")))
            .on_hover_text(tl!("The layer of the selected objects becomes the current layer"))
            .clicked()
            && let Some(name) = picked
        {
            ed.current_layer = name;
        }
        ui.label(RichText::new(format!("{}: {}", tl!("Current layer"), ed.current_layer)).weak());
    });
    ui.add_space(2.0);
    let mut order: Vec<usize> = (0..ed.drawing.layers.len()).collect();
    order.sort_by(|&a, &b| {
        let name = |i: usize| ed.drawing.layers.get(i).map(|l| l.name.to_lowercase()).unwrap_or_default();
        (name(a) != "0").cmp(&(name(b) != "0")).then_with(|| name(a).cmp(&name(b)))
    });
    egui::ScrollArea::vertical().id_salt("cad_layer_manager").auto_shrink([false, false]).show(ui, |ui| {
        egui::Grid::new("cad_layer_grid").num_columns(10).striped(true).spacing(vec2(10.0, 3.0)).show(ui, |ui| {
            for h in ["Current", "Name", "On", "Lock", "Print", "Colour", "Lineweight", "Linetype", "Objects", ""] {
                ui.label(RichText::new(tl!(h)).weak().small());
            }
            ui.end_row();
            for li in order {
                let Some(l) = ed.drawing.layers.get(li).cloned() else { continue };
                let name = l.name.clone();
                ui.push_id(("cad_layer", li), |ui| {
                    if ui.radio(ed.current_layer == name, "").on_hover_text(tl!("Current layer: new objects go on it")).clicked() {
                        ed.current_layer = name.clone();
                    }
                });
                if name == "0" {
                    ui.label("0");
                } else if let Some(to) = text_field(ui, ui.scope_id().with(("cad_layer_name", li)), &name, 130.0, &mut typing) {
                    match d.rename_layer(&name, &to) {
                        Ok(()) => {
                            if ed.current_layer == name {
                                ed.current_layer = to.trim().to_string();
                            }
                            changed = true;
                        }
                        Err(e) => ed.message = e,
                    }
                }
                let mut flag = |ui: &mut Ui, on: bool, tip: &str, set: &mut dyn FnMut(&mut strix_cad::Layer, bool)| {
                    let mut v = on;
                    if ui.checkbox(&mut v, "").on_hover_text(tl!(tip)).changed()
                        && let Some(x) = d.layers.get_mut(li)
                    {
                        set(x, v);
                        changed = true;
                    }
                };
                flag(ui, l.visible, "Shown (hidden layers are neither drawn nor picked)", &mut |x, v| x.visible = v);
                flag(ui, l.locked, "Locked: shown and snapped to, but its objects can't be picked", &mut |x, v| x.locked = v);
                flag(ui, l.print, "Printed: off, the layer shows only in the editor", &mut |x, v| x.print = v);
                if let Some(Some(c)) = color_combo(ui, &format!("cad_layer_color{li}"), Some(Some(l.color)), false, &mut typing)
                    && let Some(x) = d.layers.get_mut(li)
                {
                    x.color = c;
                    changed = true;
                }
                if let Some(Some(w)) = width_combo(ui, &format!("cad_layer_width{li}"), Some(Some(l.width)), false)
                    && let Some(x) = d.layers.get_mut(li)
                {
                    x.width = w;
                    changed = true;
                }
                if let Some(Some(k)) = dash_combo(ui, &format!("cad_layer_dash{li}"), Some(Some(l.dash)), false)
                    && let Some(x) = d.layers.get_mut(li)
                {
                    x.dash = k;
                    changed = true;
                }
                ui.label(ed.drawing.entities.iter().filter(|e| e.layer == name).count().to_string());
                if name != "0"
                    && ui.small_button("×").on_hover_text(tl!("Delete the layer (its objects go to layer 0)")).clicked()
                    && d.remove_layer(&name).is_ok()
                {
                    if ed.current_layer == name {
                        ed.current_layer = "0".into();
                    }
                    changed = true;
                } else if name == "0" {
                    ui.label("");
                }
                ui.end_row();
            }
        });
    });
    ed.panel_typing |= typing;
    changed.then_some(d)
}
