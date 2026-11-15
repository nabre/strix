//! The CAD editor's Properties pane (docs/cad.md), beside the sheet like a CAD's palette:
//!
//! - **Considering** (AutoCAD's Quick Properties): with several objects selected, all of them or
//!   one family ("Line (3)"); ‹ › step through the objects one by one, the counter goes back to
//!   the family; Only These reduces the selection to them. The sheet shows them in orange.
//! - **General**: layer, colour, lineweight and linetype of the objects considered, each "ByLayer"
//!   (for annotations "ByStyle": their dimension style's, which may be ByLayer) or their own,
//!   with the value it comes to; "Varies" when they differ.
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
use crate::color_palette::{ButtonPick, Options, TopItem};
use wordcraft_doc::Rgb;

/// Lineweights offered, paper mm (ISO 128 series).
const WIDTHS: [f64; 9] = [0.13, 0.18, 0.25, 0.35, 0.5, 0.7, 1.0, 1.4, 2.0];
const DASHES: [(Dash, &str); 5] = [
    (Dash::Solid, "Continuous"),
    (Dash::Dashed, "Dashed"),
    (Dash::Dotted, "Dotted"),
    (Dash::DashDotted, "Dash-dot"),
    (Dash::DashDoubleDotted, "Dash-dot-dot"),
];
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

/// A sample of a line as it is drawn: its colour, its lineweight (paper mm, shown about three
/// times as many points) and its linetype's dashes, `w` points long. The same sample shows in
/// the linetype and lineweight menus and as each layer's preview.
fn line_sample(ui: &mut Ui, color: [u8; 3], width: f64, dash: Dash, w: f32) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(w, 14.0), egui::Sense::hover());
    // On the drawing's sheet: the line looks as it will on paper, in either theme.
    let edge = crate::theme::Tokens::get(ui.ctx()).border;
    ui.painter().rect_filled(r, 2.0, crate::cad_editor::SHEET);
    ui.painter().rect_stroke(r, 2.0, egui::Stroke::new(0.5, edge), egui::StrokeKind::Inside);
    let r = r.shrink2(vec2(3.0, 0.0));
    let stroke = egui::Stroke::new((width as f32 * 3.0).clamp(0.6, 6.0), Color32::from_rgb(color[0], color[1], color[2]));
    let y = r.center().y;
    let pattern: Vec<f32> = dash.pattern().iter().map(|&v| (v as f32 * 2.5).max(1.0)).collect();
    if pattern.is_empty() {
        ui.painter().line_segment([egui::pos2(r.left(), y), egui::pos2(r.right(), y)], stroke);
    } else {
        // On and off lengths in turn along the sample.
        let (mut x, mut k) = (r.left(), 0usize);
        while x < r.right() {
            let len = pattern.get(k % pattern.len()).copied().unwrap_or(1.0);
            if k % 2 == 0 {
                ui.painter().line_segment([egui::pos2(x, y), egui::pos2((x + len).min(r.right()), y)], stroke);
            }
            x += len;
            k += 1;
        }
    }
    resp
}

/// An icon that switches a layer state: `on` draws `icon`, off draws `off` dimmed. Returns
/// whether it was clicked.
fn state_toggle(ui: &mut Ui, on: bool, icon: &str, off: &str, tip: &str) -> bool {
    let t = crate::theme::Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(22.0, 20.0), egui::Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, 4.0, t.hover);
    }
    let (name, ink) = if on { (icon, t.icon) } else { (off, t.icon.gamma_multiply(0.45)) };
    crate::icons::paint(ui.painter(), egui::Rect::from_center_size(r.center(), vec2(16.0, 16.0)), name, ink, if on { t.accent } else { ink });
    resp.on_hover_text(tl!(tip)).clicked()
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

/// A colour combo: ByLayer (when offered) and the colour drop-down every colour uses (theme and
/// standard colours, Recent Colors, More Colors…). `Some(None)` is ByLayer.
fn color_combo(ui: &mut Ui, id: &str, shown: Option<Option<[u8; 3]>>, by_layer: bool) -> Option<Option<[u8; 3]>> {
    let text = match shown {
        None => tl!("Varies").to_string(),
        Some(None) => tl!("ByLayer").to_string(),
        Some(Some(c)) => color_name(c),
    };
    let current = shown.flatten().map(|c| Rgb(c[0], c[1], c[2]));
    let tops = if by_layer { vec![TopItem::none("ByLayer", serde_json::Value::Null)] } else { Vec::new() };
    let pick = crate::color_palette::color_button(ui, id, &wordcraft_doc::THEME_COLORS, current, &text, &Options::tops(tops))?;
    Some(match pick {
        ButtonPick::Color(c) => Some([c.0, c.1, c.2]),
        ButtonPick::Top(_) => None,
    })
}

/// `#rrggbb` (the `#` may be left out).
pub(crate) fn parse_hex(c: &str) -> Option<[u8; 3]> {
    let h = c.trim().trim_start_matches('#');
    if h.len() != 6 || !h.is_ascii() {
        return None;
    }
    let byte = |i: usize| h.get(i..i + 2).and_then(|x| u8::from_str_radix(x, 16).ok());
    Some([byte(0)?, byte(2)?, byte(4)?])
}

fn width_text(w: f64) -> String {
    format!("{w:.2} mm")
}

/// What a lineweight or linetype combo's "no value of its own" means: `label` (ByLayer, or
/// ByStyle for annotations, which take their dimension style's), and the value it comes to when
/// the objects agree (shown after the label).
pub(crate) struct Inherit<T> {
    pub label: &'static str,
    pub now: Option<T>,
}

/// A lineweight combo; `Some(None)` is the inherited value (`inherit`, when offered).
pub(crate) fn width_combo(ui: &mut Ui, id: &str, shown: Option<Option<f64>>, inherit: Option<Inherit<f64>>) -> Option<Option<f64>> {
    let mut out = None;
    let inherited = inherit.as_ref().map(|i| match i.now {
        Some(w) => format!("{} · {}", tl!(i.label), width_text(w)),
        None => tl!(i.label).to_string(),
    });
    let text = match shown {
        None => tl!("Varies").to_string(),
        Some(None) => inherited.clone().unwrap_or_default(),
        Some(Some(w)) => width_text(w),
    };
    craft_ui::ComboBox::from_id_salt(id).selected_text(text).width(140.0).show_ui(ui, |ui| {
        if let Some(t) = &inherited
            && ui.selectable_label(shown == Some(None), t.as_str()).clicked()
        {
            out = Some(None);
        }
        for w in WIDTHS {
            ui.horizontal(|ui| {
                line_sample(ui, [40, 40, 40], w, Dash::Solid, 40.0);
                if ui.selectable_label(shown == Some(Some(w)), width_text(w)).clicked() {
                    out = Some(Some(w));
                }
            });
        }
    });
    out
}

/// A linetype's name (English; translated by the caller).
fn dash_name(d: Dash) -> &'static str {
    DASHES.iter().find(|(x, _)| *x == d).map_or("", |(_, n)| n)
}

/// A linetype combo; `Some(None)` is the inherited value (`inherit`, when offered).
pub(crate) fn dash_combo(ui: &mut Ui, id: &str, shown: Option<Option<Dash>>, inherit: Option<Inherit<Dash>>) -> Option<Option<Dash>> {
    let mut out = None;
    let inherited = inherit.as_ref().map(|i| match i.now {
        Some(d) => format!("{} · {}", tl!(i.label), tl!(dash_name(d))),
        None => tl!(i.label).to_string(),
    });
    let text = match shown {
        None => tl!("Varies").to_string(),
        Some(None) => inherited.clone().unwrap_or_default(),
        Some(Some(d)) => tl!(dash_name(d)).to_string(),
    };
    craft_ui::ComboBox::from_id_salt(id).selected_text(text).width(140.0).show_ui(ui, |ui| {
        if let Some(t) = &inherited
            && ui.selectable_label(shown == Some(None), t.as_str()).clicked()
        {
            out = Some(None);
        }
        for (d, n) in DASHES {
            ui.horizontal(|ui| {
                line_sample(ui, [40, 40, 40], 0.35, d, 56.0);
                if ui.selectable_label(shown == Some(Some(d)), tl!(n)).clicked() {
                    out = Some(Some(d));
                }
            });
        }
    });
    out
}

/// What the General group's ByLayer means for these objects: ByStyle when they are all
/// annotations (their dimension style comes first), both names when some are.
fn inherit_label(ents: &[&strix_cad::Entity]) -> &'static str {
    match (ents.iter().all(|e| e.kind.is_annotation()), ents.iter().any(|e| e.kind.is_annotation())) {
        (true, _) => "ByStyle",
        (false, true) => "ByLayer / ByStyle",
        _ => "ByLayer",
    }
}

/// A scale picked in the pane.
#[derive(Clone, Copy)]
enum ScaleChange {
    /// `1:n`.
    Fixed(f64),
    /// Fitted to the maximum size.
    Fitted,
}

/// The pane; returns the drawing to store when something changed.
pub(crate) fn show(ed: &mut CadEditor, ui: &mut Ui) -> Option<Drawing> {
    let mut out = None;
    let mut typing = false;
    egui::ScrollArea::vertical().id_salt("cad_properties").auto_shrink([false, false]).show(ui, |ui| {
        if ed.selection.is_empty() {
            ui.label(RichText::new(tl!("No selection: click objects, or drag a window")).weak());
            let d = &ed.drawing;
            let mut input_unit: Option<&str> = None;
            let mut scale_to: Option<ScaleChange> = None;
            let error_key = egui::Id::unique("cad_drawing_info_error");
            let named = d.id.clone().filter(|id| !id.strip_prefix('d').is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())));
            let mut rename: Option<String> = None;
            crate::rtl::grid(ui, egui::Grid::new("cad_drawing_info").num_columns(2).spacing(vec2(8.0, 3.0)), |ui| {
                // The drawing's name, a slug (ground-floor) the underlay list shows.
                ui.label(tl!("Name")).on_hover_text(tl!("A short name for the drawing (letters, digits and hyphens), shown in the underlay list"));
                let shown = named.clone().unwrap_or_default();
                if let Some(t) = text_field(ui, ui.scope_id().with("cad_name_pane"), &shown, 150.0, &mut typing)
                    && !t.trim().is_empty()
                {
                    rename = Some(t);
                }
                ui.end_row();
                // The unit every length is shown and typed in (fields, properties, the command
                // line). The drawing's own unit is an internal convention, never shown.
                ui.label(tl!("Input unit")).on_hover_text(tl!("The unit lengths are shown and typed in"));
                let now = strix_cad::Units::of(d).input;
                craft_ui::ComboBox::from_id_salt("cad_input_unit").selected_text(now.symbol()).show_ui(ui, |ui| {
                    for u in [strix_cad::Unit::Mm, strix_cad::Unit::Cm, strix_cad::Unit::M] {
                        if ui.selectable_label(now == u, u.symbol()).clicked() {
                            input_unit = Some(u.symbol());
                        }
                    }
                });
                ui.end_row();
                // The scale: typed (1:75, 2:1, 75) or picked from the standard scales at which
                // the drawing fits (and Fitted to Frame with a maximum size).
                ui.label(tl!("Scale"));
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let fitted = d.fit.is_some_and(|f| !f.fixed);
                    let shown = strix_cad::scale_text(d.scale);
                    if let Some(t) = text_field(ui, ui.scope_id().with("cad_scale_pane"), &shown, 70.0, &mut typing) {
                        match strix_cad::parse_scale(&t) {
                            Some(n) => scale_to = Some(ScaleChange::Fixed(n)),
                            None if t.trim() != shown => {
                                ui.data_mut(|m| m.insert_temp(error_key, tl!("Type a scale such as 1:75 or 2:1").to_string()));
                            }
                            None => {}
                        }
                    }
                    let tip = if fitted { tl!("Fitted to frame") } else { tl!("Type a scale such as 1:75 or 2:1, or pick one") };
                    craft_ui::ComboBox::from_id_salt("cad_scale_pane_list")
                        .selected_text("")
                        .width(18.0)
                        .show_ui(ui, |ui| {
                            if d.fit.is_some() && ui.selectable_label(fitted, tl!("Fitted to Frame")).clicked() {
                                scale_to = Some(ScaleChange::Fitted);
                            }
                            for n in d.fitting_scales(None) {
                                if ui.selectable_label(!fitted && d.scale == n, strix_cad::scale_text(n)).clicked() {
                                    scale_to = Some(ScaleChange::Fixed(n));
                                }
                            }
                        })
                        .response
                        .on_hover_text(tip);
                });
                ui.end_row();
                ui.label(tl!("Objects"));
                ui.label(d.entities.len().to_string());
                ui.end_row();
            });
            if let Some(msg) = ui.data(|m| m.get_temp::<String>(error_key)) {
                ui.label(RichText::new(msg).color(ui.visuals().error_fg_color));
            }
            if rename.is_some() {
                ed.rename = rename;
            }
            if let Some(word) = input_unit {
                let mut d = ed.drawing.clone();
                d.input = strix_cad::Unit::parse(word);
                out = Some(d);
            }
            if let Some(change) = scale_to {
                let mut d = ed.drawing.clone();
                let done = match change {
                    ScaleChange::Fixed(n) => d.set_fixed_scale(n, None),
                    ScaleChange::Fitted => d.fit_scale(None),
                };
                match done {
                    Ok(()) => {
                        ui.data_mut(|m| m.remove::<String>(error_key));
                        out = Some(d);
                    }
                    Err(e) => {
                        ui.data_mut(|m| m.insert_temp(error_key, e));
                    }
                }
            }
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
            craft_ui::ComboBox::from_id_salt("cad_focus").selected_text(shown).width(150.0).show_ui(ui, |ui| {
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
    crate::rtl::grid(ui, egui::Grid::new("cad_general").num_columns(2).spacing(vec2(8.0, 4.0)), |ui| {
        ui.label(tl!("Layer"));
        let layer = common(ents.iter().map(|e| e.layer.clone()));
        let text = match &layer {
            Some(Some(l)) => l.clone(),
            _ => tl!("Varies").to_string(),
        };
        craft_ui::ComboBox::from_id_salt("cad_obj_layer").selected_text(text).width(150.0).show_ui(ui, |ui| {
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
        if let Some(c) = color_combo(ui, "cad_obj_color", common(ents.iter().map(|e| e.stroke.color)).flatten(), true) {
            d.set_stroke(&idx, StrokeEdit { color: Some(c), ..StrokeEdit::default() });
            changed = true;
        }
        ui.end_row();
        // What the objects with no width or linetype of their own come to (their layer's, an
        // annotation its style's first), shown beside ByLayer / ByStyle when they agree.
        let label = inherit_label(&ents);
        let effective: Vec<_> = idx.iter().filter_map(|&i| Some((ed.drawing.entities.get(i)?, ed.drawing.effective_stroke(i)?))).collect();
        let now_w = common(effective.iter().filter(|(e, _)| e.stroke.width.is_none()).map(|(_, s)| s.1)).flatten();
        let now_d = common(effective.iter().filter(|(e, _)| e.stroke.dash.is_none()).map(|(_, s)| s.2)).flatten();
        ui.label(tl!("Lineweight"));
        if let Some(w) = width_combo(ui, "cad_obj_width", common(ents.iter().map(|e| e.stroke.width)).flatten(), Some(Inherit { label, now: now_w }))
        {
            d.set_stroke(&idx, StrokeEdit { width: Some(w), ..StrokeEdit::default() });
            changed = true;
        }
        ui.end_row();
        ui.label(tl!("Linetype"));
        if let Some(k) = dash_combo(ui, "cad_obj_dash", common(ents.iter().map(|e| e.stroke.dash)).flatten(), Some(Inherit { label, now: now_d })) {
            d.set_stroke(&idx, StrokeEdit { dash: Some(k), ..StrokeEdit::default() });
            changed = true;
        }
        ui.end_row();
        if idx.len() > 1 {
            ui.label(tl!("Solution only"));
            let ans = common(ents.iter().map(|e| e.answer));
            let mut on = ans == Some(Some(true));
            let hint = if ans == Some(None) { tl!("Varies") } else { "" };
            if ui.checkbox(&mut on, hint).changed() {
                d.set_answer(&idx, on);
                changed = true;
            }
            ui.end_row();
        }
    });
    // Geometry of one object.
    if let [i] = idx.as_slice() {
        let i = *i;
        let unit = strix_cad::Units::of(&ed.drawing).input.symbol();
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
        let kind = ed.drawing.entities.get(i).map(|e| e.kind.clone());
        crate::rtl::grid(ui, egui::Grid::new("cad_geometry").num_columns(3).spacing(vec2(6.0, 4.0)), |ui| {
            for f in ed.drawing.fields_typed(i, ed.vertex) {
                // A dimension's scaled and annotative parts: marked in the sheet overlay's colours.
                match kind.as_ref().and_then(|k| crate::cad_dim_parts::field_part(k, &f.key)) {
                    Some((c, hint)) => {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            ui.label(RichText::new("■").color(c));
                            ui.label(tl!(f.label));
                        })
                        .response
                        .on_hover_text(tl!(hint));
                    }
                    None => {
                        ui.label(tl!(f.label));
                    }
                }
                let new = match f.kind {
                    FieldKind::Bool => {
                        let mut on = f.text == "true";
                        ui.checkbox(&mut on, "").changed().then(|| on.to_string())
                    }
                    // A mirroring: one press turns it over, lit while mirrored.
                    FieldKind::Flip => {
                        let on = f.text == "true";
                        ui.add(egui::Button::new(tl!("Flip")).selected(on)).clicked().then(|| (!on).to_string())
                    }
                    FieldKind::Choice(words) => {
                        let mut pick = None;
                        craft_ui::ComboBox::from_id_salt(("cad_choice", &id, &f.key)).selected_text(tl!(choice_label(&f.text))).show_ui(ui, |ui| {
                            for &w in words {
                                if ui.selectable_label(f.text == w, tl!(choice_label(w))).clicked() {
                                    pick = Some(w.to_string());
                                }
                            }
                        });
                        pick
                    }
                    FieldKind::Style => {
                        let names: Vec<String> = ed
                            .drawing
                            .entities
                            .get(i)
                            .map(|e| ed.drawing.style_names(e).into_iter().map(str::to_string).collect())
                            .unwrap_or_default();
                        let mut pick = None;
                        craft_ui::ComboBox::from_id_salt(("cad_style", &id)).selected_text(tl!(f.text.as_str())).show_ui(ui, |ui| {
                            for n in &names {
                                if ui.selectable_label(f.text == *n, tl!(n.as_str())).clicked() {
                                    pick = Some(n.clone());
                                }
                            }
                        });
                        pick
                    }
                    _ => text_field(ui, ui.scope_id().with(("cad_field", &id, &f.key, ed.vertex)), &f.text, 120.0, typing),
                };
                let suffix = match (f.kind, f.key.as_str()) {
                    _ if f.is_length() => unit,
                    (FieldKind::Angle, _) | (FieldKind::Derived, "angle" | "rotation") => "°",
                    (FieldKind::Paper, _) => "mm",
                    _ => "",
                };
                // A text that can show the calculations' parameters: a menu that writes them in.
                let annotation = matches!(f.kind, FieldKind::Text) && matches!(f.key.as_str(), "text" | "below" | "value");
                let added = if annotation {
                    let dim = f.key != "value";
                    parameter_menu(ui, &ed.names, dim).map(|piece| format!("{}{piece}", if dim && f.text.is_empty() { "" } else { f.text.as_str() }))
                } else {
                    ui.label(RichText::new(suffix).weak());
                    None
                };
                ui.end_row();
                if let Some(v) = new.or(added) {
                    match d.set_field_typed(i, ed.vertex, &f.key, &v) {
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
        ui.label(RichText::new(crate::i18n::fmt(tl!("Lengths in {unit} (or with their own unit: 2.5m, 30cm)"), &[("unit", unit)])).weak().small());
    }
    changed.then_some(d)
}

/// The button beside an annotation's text (a dimension's, level's, angle's or a text's) that
/// writes in what it can show (docs/cad.md): a parameter of the calculations defined before the
/// drawing as its value (`{{L}}`, a length written as the dimension style writes sizes), its
/// symbol alone (`{{L:symbol}}`, an equation's letter) or both (`{{L:both}}`, `L = 4.50 m`),
/// and for a dimension its measured size (`<>`). Returns the piece to add at the end.
fn parameter_menu(ui: &mut Ui, names: &[String], dim: bool) -> Option<String> {
    let mut piece = None;
    ui.menu_button("{x}", |ui| {
        if dim && ui.button(tl!("Measured size")).on_hover_text("<>").clicked() {
            piece = Some("<>".to_string());
            ui.close();
        }
        if names.is_empty() {
            ui.label(RichText::new(tl!("No parameters are defined before the drawing")).weak());
        }
        for n in names.iter().take(200) {
            ui.menu_button(n.as_str(), |ui| {
                for (label, form) in
                    [("Value", format!("{{{{{n}}}}}")), ("Symbol", format!("{{{{{n}:symbol}}}}")), ("Symbol = value", format!("{{{{{n}:both}}}}"))]
                {
                    if ui.button(tl!(label)).on_hover_text(form.as_str()).clicked() {
                        piece = Some(form);
                        ui.close();
                    }
                }
            });
        }
    })
    .response
    .on_hover_text(tl!("Insert a parameter of the calculations: its value, its symbol or both"));
    piece
}

/// A choice field's word as the pane shows it (English; translated by the caller).
pub(crate) fn choice_label(s: &str) -> &'static str {
    match s {
        "aligned" => "Aligned",
        "horizontal" => "Horizontal",
        "vertical" => "Vertical",
        "style" => "By style",
        "tick" => "Tick",
        "arrow" => "Arrow",
        "dot" => "Dot",
        "none" => "None",
        "left" => "Left",
        "center" => "Center",
        "right" => "Right",
        "lines" => "Lines",
        "cross" => "Cross",
        "steel" => "Steel",
        "dashed" => "Dashed",
        "solid" => "Solid",
        "along" => "Along the path",
        "slope" => "Slope ticks",
        "asStart" => "As the start",
        "start" => "At the start",
        "end" => "At the end",
        "both" => "At both ends",
        "roughFinished" => "Rough and finished level",
        "rough" => "Rough level",
        "finished" => "Finished level",
        _ => "",
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
    // The drawing's own layers first (`0` first), then those inherited from each other drawing,
    // nested under a row with its name.
    let mut order: Vec<usize> = (0..ed.drawing.layers.len()).collect();
    order.sort_by(|&a, &b| {
        let key = |i: usize| {
            let l = ed.drawing.layers.get(i);
            let from = l.and_then(|l| l.from.clone());
            let name = l.map(|l| l.name.to_lowercase()).unwrap_or_default();
            (from, name != "0", name)
        };
        key(a).cmp(&key(b))
    });
    egui::ScrollArea::vertical().id_salt("cad_layer_manager").auto_shrink([false, false]).show(ui, |ui| {
        crate::rtl::grid(ui, egui::Grid::new("cad_layer_grid").num_columns(12).striped(true).spacing(vec2(10.0, 3.0)), |ui| {
            for h in ["Current", "Name", "Preview", "On", "Lock", "Print", "Answer", "Colour", "Lineweight", "Linetype", "Objects", ""] {
                ui.label(RichText::new(tl!(h)).weak().small());
            }
            ui.end_row();
            let mut group: Option<String> = None;
            for li in order {
                let Some(l) = ed.drawing.layers.get(li).cloned() else { continue };
                let name = l.name.clone();
                // A drawing's layers under its name.
                if l.from.is_some() && l.from != group {
                    group.clone_from(&l.from);
                    ui.label("");
                    ui.label(RichText::new(format!("▾ {}", l.from.as_deref().unwrap_or_default())).strong())
                        .on_hover_text(tl!("Layers of another drawing (its underlay or imported): usable like the drawing's own"));
                    ui.end_row();
                }
                ui.push_id(("cad_layer", li), |ui| {
                    if ui.radio(ed.current_layer == name, "").on_hover_text(tl!("Current layer: new objects go on it")).clicked() {
                        ed.current_layer = name.clone();
                    }
                });
                if name == "0" {
                    ui.label("0");
                } else if let Some(from) = l.from.as_deref() {
                    // Nested: its own name, under its drawing's; never renamed.
                    let tail = name.strip_prefix(from).and_then(|t| t.strip_prefix('/')).unwrap_or(&name);
                    ui.label(format!("    {tail}")).on_hover_text(name.clone());
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
                // How the layer's lines look: colour, lineweight and linetype together.
                line_sample(ui, l.color, l.width, l.dash, 64.0).on_hover_text(format!("{} · {}", width_text(l.width), tl!(dash_name(l.dash))));
                let mut flag = |ui: &mut Ui, on: bool, icons: (&str, &str), tip: &str, set: &mut dyn FnMut(&mut strix_cad::Layer, bool)| {
                    if state_toggle(ui, on, icons.0, icons.1, tip)
                        && let Some(x) = d.layers.get_mut(li)
                    {
                        set(x, !on);
                        changed = true;
                    }
                };
                flag(ui, l.visible, ("cadEye", "cadEyeOff"), "Shown (hidden layers are neither drawn nor picked)", &mut |x, v| x.visible = v);
                flag(ui, l.locked, ("cadLock", "cadUnlock"), "Locked: shown and snapped to, but its objects can't be picked", &mut |x, v| {
                    x.locked = v
                });
                flag(ui, l.print, ("print", "cadNoPrint"), "Printed: off, the layer shows only in the editor", &mut |x, v| x.print = v);
                flag(ui, l.answer, ("check", "minus"), "Solution only: hidden when the exercise's solution is hidden", &mut |x, v| x.answer = v);
                if let Some(Some(c)) = color_combo(ui, &format!("cad_layer_color{li}"), Some(Some(l.color)), false)
                    && let Some(x) = d.layers.get_mut(li)
                {
                    x.color = c;
                    changed = true;
                }
                if let Some(Some(w)) = width_combo(ui, &format!("cad_layer_width{li}"), Some(Some(l.width)), None)
                    && let Some(x) = d.layers.get_mut(li)
                {
                    x.width = w;
                    changed = true;
                }
                if let Some(Some(k)) = dash_combo(ui, &format!("cad_layer_dash{li}"), Some(Some(l.dash)), None)
                    && let Some(x) = d.layers.get_mut(li)
                {
                    x.dash = k;
                    changed = true;
                }
                ui.label(ed.drawing.entities.iter().filter(|e| e.layer == name).count().to_string());
                let removable = ed.drawing.can_remove_layer(&name);
                let tip = if removable || l.from.is_none() {
                    tl!("Delete the layer (its objects go to layer 0)")
                } else {
                    tl!("A layer of another drawing can be deleted only when no object is on it")
                };
                if name != "0"
                    && ui.add_enabled(removable, egui::Button::new("×").small()).on_hover_text(tip).on_disabled_hover_text(tip).clicked()
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
