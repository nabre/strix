//! CAD drawings in the document (docs/cad.md): a `GraphicKind::Cad` graphic holds a `strix-cad`
//! drawing's source; its items are drawn from it, in reading order with the calculations, so a
//! coordinate written as a parameter (`"L"`) follows the value defined before the drawing.

use std::sync::Arc;

use serde_json::{Value, json};
use strix_cad::{Anchor, Drawing, Entity, Prim, Rendered};
use wordcraft_doc::InlineObject;
use wordcraft_doc::graphic::{Graphic, GraphicItem, GraphicKind, PathSeg, TextAlign};
use wordcraft_doc::para::Float;
use wordcraft_doc::props::Rgb;

use crate::{CmdError, CmdResult, CommandSpec, Selection, Session};

mod underlay;
pub use underlay::{CadEntry, entries, underlay_view};

/// Points in a millimetre.
const PT_PER_MM: f32 = 72.0 / 25.4;
/// Most items one drawing makes (each segment of a tessellated arc counts once).
const MAX_ITEMS: usize = 20_000;
/// Most pieces in one item made of a run of strokes (see `items_of`).
const MAX_RUN_SEGS: usize = 20_000;
/// Font size of a text whose capital letters are `h` mm high (cap height ≈ 0.7 em).
const CAP_TO_EM: f32 = 1.0 / 0.7;

pub fn specs() -> Vec<CommandSpec> {
    let selected = |s: &Session| if is_selected(s) { None } else { Some("select a CAD drawing first") };
    vec![
        CommandSpec::new("insert.cad", "CAD Drawing", "Insert › Illustrations", insert).params(
            r#"{"drawing"?: object|string (a strix-cad drawing: {"unit", "scale", "layers", "entities": [{"type": "line", "a": [0,0], "b": ["L",0]} …]}; default: a starter drawing, its maximum size the space where it goes), "underlay"?: n | id (another drawing as its underlay, its layers and styles inherited), "inherit"?: n | id (only its layers and styles)} → inserts a CAD drawing in line at the caret; coordinates may be expressions of the calculations' parameters"#,
        ),
        CommandSpec::new("cad.get", "CAD Drawing Source", "Insert › Illustrations", get).pure().when(selected).params(r#"→ {"drawing": object, "errors": [[expression, why]]}: the selected drawing"#),
        CommandSpec::new("cad.set", "Set CAD Drawing", "Insert › Illustrations", set).when(selected).params(r#"{"drawing": object|string} → replaces the selected drawing's source and redraws it"#),
        CommandSpec::new("cad.add", "Add to CAD Drawing", "Insert › Illustrations", add)
            .when(selected)
            .params(r#"{"entity": object ({"type": "line|circle|arc|polyline|point|text|dim|angle|level", …})} → adds an entity to the selected drawing; returns its id"#),
        CommandSpec::new("cad.space", "Drawing Space", "CAD Picture Format › Scale", space).pure().when(selected).params(
            r#"→ {"width", "height"} mm: the space where the selected drawing sits (its column's width, the height from its top down to the page's bottom margin), a guide for its size, not a limit; null when it is not laid out"#,
        ),
        CommandSpec::new("cad.drawings", "Drawings of the Document", "CAD Picture Format › Underlay", underlay::drawings).pure().params(
            r#"→ [{"index", "id", "name", "page", "current", "underlay"}]: the document's CAD drawings (body paragraphs, reading order), to pick an underlay from"#,
        ),
        CommandSpec::new("cad.underlay", "Drawing Underlay", "CAD Picture Format › Underlay", underlay::underlay).when(selected).params(
            r#"{"source"?: n | id (another drawing: its number in cad.drawings or its id; taken by id, never copied), "inherit"?: true (default: its layers and styles merged in), "dx"?, "dy"? (drawing units), "fade"? (0–0.95), "show"?, "print"? (also on the page and in the PDF; default: only in the editor), "remove"?: true} → {"underlay", "inherited"}; refuses the drawing itself and two drawings taking each other; one level deep"#,
        ),
        CommandSpec::new("cad.inherit", "Inherit Layers and Styles", "CAD Picture Format › Underlay", underlay::inherit).when(selected).params(
            r#"{"source": n | id} → {"inherited": {"layers", "dimStyles", "textStyles"}}: another drawing's layers and styles merged into the selected one (names it has keep its definition), no underlay"#,
        ),
        CommandSpec::new("cad.padding", "Drawing Padding", "CAD Picture Format › Picture", padding)
            .when(selected)
            .params(r#"{"value": mm | [top, right, bottom, left]} → the space around the selected drawing in its picture (0–100 mm)"#),
        CommandSpec::new("cad.scale", "Drawing Scale", "CAD Picture Format › Scale", scale).when(selected).params(
            r#"{"fit"?: true (the space where it sits: its column's width and the height down to the page's bottom margin; else the page's text width and two thirds of its height) | false | {"width"?: mm, "height"?: mm} (a maximum size, the scale fitted to it), "max"?: true | false | {"width"?, "height"?} (the maximum size alone; how the scale is chosen stays), "auto"?: true (the scale fitted to the maximum size again), "scale"?: n | "1:75" | "2:1" (a fixed scale, any; with a maximum size, one at which the drawing fits)} → {"scale": n, "fit": {"width", "height", "fixed"}|null, "scales": [n…] (the standard scales that fit)}"#,
        ),
    ]
}

/// The drawing a new CAD drawing starts with: a 120 × 60 mm rectangle and its two sizes.
const STARTER: &str = r#"{"unit":"mm","scale":1,"entities":[
    {"type":"polyline","pts":[[0,0],[120,0],[120,60],[0,60]],"closed":true},
    {"type":"dim","a":[0,60],"b":[120,60],"offset":8},
    {"type":"dim","a":[0,0],"b":[0,60],"offset":8,"orient":"vertical"}
]}"#;

/// A drawing from a JSON object or string parameter.
fn drawing_param(v: &Value, key: &str) -> Result<Option<Drawing>, CmdError> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Drawing::from_json(text).map(Some).map_err(CmdError::Params),
        Some(other) => Drawing::from_json(&other.to_string()).map(Some).map_err(CmdError::Params),
    }
}

fn insert(s: &mut Session, v: &Value) -> CmdResult {
    let given = drawing_param(v, "drawing")?;
    // The starter drawing is given the space where it goes once laid out (below).
    let is_starter = given.is_none();
    let drawing = match given {
        Some(d) => d,
        None => Drawing::from_json(STARTER).map_err(CmdError::Failed)?,
    };
    let starter = is_starter.then(|| drawing.clone());
    let rendered = drawing.render(None);
    let unit = super::calc::unit_color(s);
    let (w, h, items) = items_of(&rendered, unit, &s.doc.settings.minor_font);
    let svg = Some(rendered.to_svg([unit.0, unit.1, unit.2]).into());
    let graphic = Graphic { kind: GraphicKind::Cad, items, w, h, cad: Some(drawing.to_json().into()), svg, ..Default::default() };
    let obj = InlineObject::Graphic { w, h, alt: String::new(), float: Float::default(), graphic: Arc::new(graphic) };
    let props = s.typing_props();
    let at = super::delete_selection(s)?;
    let end = s.doc.insert_object(&at, obj, &props)?;
    s.sel = Selection { anchor: at.clone(), focus: end };
    // Its parameters come from the calculations before it.
    super::calc::recalculate_now(s);
    // A new drawing knows the space where it went (its column's width, down to the page's
    // bottom): its maximum size, the scale fitted to it. Not binding: it may be made larger.
    // (The document changed: laid out again, not read from the layout before the insert.)
    s.touch();
    if let Some(mut d) = starter
        && let Some(space) = space_at(s, &at)
    {
        d.fit = Some(space.sanitized());
        store(s, &at, &d)?;
    }
    // Started from another drawing: its layers and styles, and as an underlay.
    if let Some(src) = v.get("underlay") {
        underlay::underlay(s, &json!({"source": src}))?;
    } else if let Some(src) = v.get("inherit") {
        underlay::inherit(s, &json!({"source": src}))?;
    }
    super::sel_result(s)
}

/// The selected CAD drawing for the editor: where it is, the drawing with its coordinates'
/// values worked out where it sits, and how it renders (with its paper origin and scale).
pub fn edited(s: &Session) -> Option<(wordcraft_doc::Pos, Drawing, Rendered, Vec<String>)> {
    let (pos, obj) = super::objects::selected(s)?;
    let InlineObject::Graphic { graphic, .. } = obj else { return None };
    if graphic.kind != GraphicKind::Cad {
        return None;
    }
    let source = graphic.cad.clone()?;
    let resolved = super::calc::resolved_cad(s, &source)?;
    let (d, r, names) = &*resolved;
    Some((pos, d.clone(), r.clone(), names.clone()))
}

/// The selected CAD drawing as [`edited`] works it out, without copying it: whether it can be
/// edited, and its drawing (for the ribbon, which asks every frame).
fn edited_ref(s: &Session) -> Option<super::calc::ResolvedCad> {
    let (_, obj) = super::objects::selected(s)?;
    let InlineObject::Graphic { graphic, .. } = obj else { return None };
    if graphic.kind != GraphicKind::Cad {
        return None;
    }
    super::calc::resolved_cad(s, graphic.cad.as_ref()?)
}

/// Whether the selected object is a CAD drawing the editor can open.
pub fn editable(s: &Session) -> bool {
    edited_ref(s).is_some()
}

/// The selected CAD drawing with its sizes worked out where it sits (no rendering copied).
pub fn edited_drawing(s: &Session) -> Option<Drawing> {
    edited_ref(s).map(|r| r.0.clone())
}

/// The selected object, when it is a CAD drawing: where it is and its source.
fn selected_source(s: &Session) -> Option<(wordcraft_doc::Pos, Arc<str>)> {
    let (pos, obj) = super::objects::selected(s)?;
    match obj {
        InlineObject::Graphic { graphic, .. } if graphic.kind == GraphicKind::Cad => Some((pos, graphic.cad.clone().unwrap_or_else(|| "".into()))),
        _ => None,
    }
}

/// The colour drawing texts show units in: the Unit style's, as on the page.
pub fn unit_rgb(s: &Session) -> [u8; 3] {
    let c = super::calc::unit_color_of(s);
    [c.0, c.1, c.2]
}

/// Whether the selected object is a CAD drawing.
pub fn is_selected(s: &Session) -> bool {
    selected_source(s).is_some()
}

/// The selected CAD drawing: where it is and its source, read.
pub fn selected_drawing(s: &Session) -> Option<(wordcraft_doc::Pos, Drawing)> {
    selected_source(s).map(|(pos, src)| (pos, Drawing::from_json(&src).unwrap_or_default()))
}

/// The selected CAD drawing to change: an error, not an empty drawing, when its source can't be
/// read (changing it would replace the drawing with a blank one).
fn drawing_to_change(s: &Session) -> Result<(wordcraft_doc::Pos, Drawing), CmdError> {
    let (pos, src) = selected_source(s).ok_or_else(|| CmdError::Disabled("select a CAD drawing first".into()))?;
    let d = Drawing::from_json(&src).map_err(|e| CmdError::Failed(format!("the drawing can't be read: {e}")))?;
    Ok((pos, d))
}

fn get(s: &mut Session, _: &Value) -> CmdResult {
    let (_, src) = selected_source(s).ok_or_else(|| CmdError::Disabled("select a CAD drawing first".into()))?;
    let mut d = Drawing::from_json(&src).map_err(|e| CmdError::Failed(format!("the drawing can't be read: {e}")))?;
    // The errors where the drawing sits (its parameters come from the calculations before it);
    // outside the body, against no calculations.
    let errors = match (super::calc::cad_errors(s, &src), super::calc::resolved_cad(s, &src)) {
        (Some(errors), Some(resolved)) => {
            d = resolved.0.clone();
            errors
        }
        _ => d.resolve(&strix_calc::Worksheet::new(super::calc::options_now(s))),
    };
    let drawing: Value = serde_json::from_str(&d.to_json()).unwrap_or(Value::Null);
    Ok(json!({"drawing": drawing, "errors": errors}))
}

/// Stores `d` as the selected drawing's source, then redraws.
fn store(s: &mut Session, pos: &wordcraft_doc::Pos, d: &Drawing) -> Result<(), CmdError> {
    let para = s.doc.para_mut(pos.story, &pos.path)?;
    let Some(InlineObject::Graphic { graphic, .. }) = para.object_at_mut(pos.off) else {
        return Err(CmdError::Failed("the drawing vanished".into()));
    };
    Arc::make_mut(graphic).cad = Some(d.to_json().into());
    para.touch();
    s.touch();
    super::calc::recalculate_now(s);
    Ok(())
}

fn set(s: &mut Session, v: &Value) -> CmdResult {
    let (pos, _) = selected_drawing(s).ok_or_else(|| CmdError::Disabled("select a CAD drawing first".into()))?;
    let d = drawing_param(v, "drawing")?.ok_or_else(|| CmdError::Params("drawing: a strix-cad drawing".into()))?;
    store(s, &pos, &d)?;
    super::sel_result(s)
}

/// The space a drawing fits by default: the narrowest text width of the document and two
/// thirds of its text height, mm.
pub fn page_space(s: &Session) -> strix_cad::Fit {
    let mm = |pt: f32| f64::from(pt / PT_PER_MM);
    let height = s.doc.sections().iter().map(|(_, sp)| sp.text_height()).fold(f32::INFINITY, f32::min).clamp(36.0, 4000.0);
    strix_cad::Fit { width: mm(crate::io::text_width(&s.doc)), height: mm(height) * 2.0 / 3.0, fixed: false }
}

/// The space where the selected drawing sits, mm: the width of the column it is in, and the
/// height from its top to the bottom of the page's text area (in the layout of the view shown).
/// A guide, not a limit: the drawing may be larger. None when it is not laid out.
pub fn space_here(s: &mut Session) -> Option<strix_cad::Fit> {
    let (pos, _) = selected_source(s)?;
    space_at(s, &pos)
}

/// [`space_here`] for the drawing at `pos` (its U+FFFC).
fn space_at(s: &mut Session, pos: &wordcraft_doc::Pos) -> Option<strix_cad::Fit> {
    let pos = pos.clone();
    let layout = s.layout();
    let page_i = layout.page_of(&pos)?;
    let hit = layout.object(&pos, page_i)?;
    let page = layout.pages.get(page_i)?;
    let mm = |pt: f32| f64::from(pt / PT_PER_MM);
    let sections = s.doc.sections();
    let columns = sections.get(page.section).or_else(|| sections.last()).map(|(_, sp)| sp.columns.clone()).unwrap_or_default();
    let body = page.body;
    let gaps = columns.space.max(0.0) * columns.count.saturating_sub(1) as f32;
    let width = if columns.count <= 1 {
        body.w
    } else if columns.widths.is_empty() {
        (body.w - gaps) / columns.count.max(1) as f32
    } else {
        // Unequal columns: the one the drawing starts in.
        let mut x = body.x;
        let mut found = columns.widths.first().map_or(body.w, |w| w.0);
        for (w, after) in columns.widths.iter().take(64) {
            if hit.rect.x < x + w + after {
                found = *w;
                break;
            }
            x += w + after;
        }
        found
    };
    let height = body.y + body.h - hit.rect.y;
    let (w, h) = (mm(width), mm(height));
    (w.is_finite() && h.is_finite()).then(|| strix_cad::Fit { width: w.clamp(10.0, 5000.0), height: h.clamp(10.0, 5000.0), fixed: false })
}

fn space(s: &mut Session, _: &Value) -> CmdResult {
    selected_source(s).ok_or_else(|| CmdError::Disabled("select a CAD drawing first".into()))?;
    Ok(space_here(s).map_or(Value::Null, |f| json!({"width": f.width, "height": f.height})))
}

/// [`space_here`], else the page's ([`page_space`]).
pub fn space_or_page(s: &mut Session) -> strix_cad::Fit {
    space_here(s).unwrap_or_else(|| page_space(s))
}

/// The scale parameter of `cad.scale` and the editor's: a number `n` (1:n) or a scale as written
/// (`"1:75"`, `"2:1"`, `"75"`).
pub fn scale_param(v: &Value) -> Result<f64, String> {
    let n = match v {
        Value::String(t) => strix_cad::parse_scale(t),
        other => other.as_f64().filter(|n| n.is_finite() && *n > 0.0).map(|n| n.clamp(strix_cad::SCALE_RANGE.0, strix_cad::SCALE_RANGE.1)),
    };
    n.ok_or_else(|| "scale: the n of 1:n (more than 0), or a scale such as \"1:75\" or \"2:1\"".to_string())
}

/// A padding parameter: one size for all sides, or `[top, right, bottom, left]`, mm.
pub fn padding_param(v: &Value) -> Result<strix_cad::Padding, String> {
    match v.get("value") {
        Some(p @ Value::Number(_)) => serde_json::from_value(p.clone()).map_err(|e| e.to_string()),
        // Four sides, each a number: anything else would read as the default padding.
        Some(p @ Value::Array(a)) if a.len() == 4 && a.iter().all(Value::is_number) => serde_json::from_value(p.clone()).map_err(|e| e.to_string()),
        _ => Err("value: the padding in mm, or [top, right, bottom, left]".into()),
    }
}

fn padding(s: &mut Session, v: &Value) -> CmdResult {
    let (pos, mut d) = drawing_to_change(s)?;
    d.padding = padding_param(v).map_err(CmdError::Params)?;
    store(s, &pos, &d)?;
    Ok(serde_json::to_value(d.padding).unwrap_or(Value::Null))
}

fn scale(s: &mut Session, v: &Value) -> CmdResult {
    let (pos, mut d) = drawing_to_change(s)?;
    // The drawing with its sizes worked out by the calculations: what fits is judged on it.
    let resolved = edited(s).map_or_else(|| d.clone(), |(_, r, ..)| r);
    let probe = |fit: Option<strix_cad::Fit>| {
        let mut r = resolved.clone();
        r.fit = fit;
        r
    };
    let space = |o: &Value| -> Result<strix_cad::Fit, CmdError> {
        let num = |k: &str| o.get(k).map(|x| x.as_f64().ok_or_else(|| CmdError::Params(format!("{k}: mm")))).transpose();
        Ok(strix_cad::Fit { width: num("width")?.unwrap_or(0.0), height: num("height")?.unwrap_or(0.0), fixed: false })
    };
    match v.get("fit") {
        None | Some(Value::Null) => {}
        Some(Value::Bool(true)) => d.fit = Some(space_or_page(s)),
        Some(Value::Bool(false)) => d.fit = None,
        Some(o @ Value::Object(_)) => d.fit = Some(space(o)?),
        Some(_) => return Err(CmdError::Params("fit: true, false or {\"width\", \"height\"} in mm".into())),
    }
    // The maximum size alone: how the scale is chosen stays (a scale picked by hand that no
    // longer fits gives way to the fitted one).
    match v.get("max") {
        None => {}
        Some(Value::Null | Value::Bool(false)) => d.fit = None,
        Some(m) => {
            let fixed = d.fit.is_some_and(|f| f.fixed);
            let mut f = match m {
                Value::Bool(true) => space_or_page(s),
                Value::Object(_) => space(m)?,
                _ => return Err(CmdError::Params("max: true (the page), false or {\"width\", \"height\"} in mm".into())),
            };
            f.fixed = fixed && probe(Some(f)).fits_at(d.scale, None);
            d.fit = Some(f.sanitized());
        }
    }
    if v.get("auto").and_then(Value::as_bool) == Some(true) {
        let f = match d.fit {
            Some(f) => f,
            None => space_or_page(s),
        };
        d.fit = Some(strix_cad::Fit { fixed: false, ..f });
    }
    if let Some(n) = v.get("scale") {
        let n = scale_param(n).map_err(CmdError::Params)?;
        let mut p = probe(d.fit);
        p.set_fixed_scale(n, None).map_err(CmdError::Params)?;
        d.fit = p.fit;
        d.scale = n;
    }
    d.sanitize();
    store(s, &pos, &d)?;
    // The scale the drawing is drawn at now (fitted where it sits, with the calculations), and
    // the standard scales it can be drawn at.
    let (shown, scales) = edited(s).map_or((d.scale, Vec::new()), |(_, fitted, ..)| (fitted.scale, fitted.fitting_scales(None)));
    Ok(json!({
        "scale": shown,
        "fit": d.fit.map(|f| json!({"width": f.width, "height": f.height, "fixed": f.fixed})),
        "scales": scales,
    }))
}

fn add(s: &mut Session, v: &Value) -> CmdResult {
    let (pos, mut d) = drawing_to_change(s)?;
    let entity: Entity = v
        .get("entity")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| CmdError::Params(format!("entity: {e}")))?
        .ok_or_else(|| CmdError::Params("entity: an object".into()))?;
    let id = d.add(entity).map_err(CmdError::Failed)?;
    store(s, &pos, &d)?;
    Ok(json!({"id": id}))
}

/// The font parameters are set in, as the equations are (resolved to an open math font).
const MATH_FONT: &str = "Cambria Math";
/// A subscript's size, as a share of the text's.
const SUB_SCALE: f32 = 0.7;

/// How wide `text` is at `size` points in `family` (italic or not), shaped as the page draws it.
fn measure(family: &str, bold: bool, italic: bool, text: &str, size: f32) -> f32 {
    let resolved = wordcraft_fonts::word::resolve(family, bold, italic);
    let face = resolved.face.get();
    let k = size / face.upem.max(1.0) as f32;
    wordcraft_fonts::shape(face, text, &[], |c| c).iter().map(|g| g.x_advance as f32 * k).sum()
}

/// The graphic items of a rendered drawing (points from its top-left corner) and its size. A
/// text in pieces (units, parameters) is measured in its fonts, `body_font` where it has none
/// (the document's, as the page draws it).
pub(crate) fn items_of(r: &Rendered, unit_color: Rgb, body_font: &str) -> (f32, f32, Vec<GraphicItem>) {
    // A drawing of millions of points is saved with fewer (`Rendered::lightened`).
    let light = r.lightened();
    let r: &Rendered = &light;
    let pt = |v: f64| (v as f32) * PT_PER_MM;
    let rgb = |c: [u8; 3]| Rgb(c[0], c[1], c[2]);
    let mut items = Vec::new();
    // The style of the last item when it is a run of single solid strokes (a hatch's lines),
    // which the next such stroke in that style joins as one more piece: a fine hatch is one
    // item, not thousands, and leaves room under `MAX_ITEMS` for what is drawn over it.
    let mut run: Option<([u8; 3], f64)> = None;
    for prim in &r.prims {
        if let Prim::Path { pts, closed: false, color, width, fill: false, dash } = prim
            && let [a, b] = pts.as_slice()
            && dash.is_empty()
        {
            if run == Some((*color, *width))
                && let Some(GraphicItem::Path { segs, .. }) = items.last_mut()
                && segs.len() < MAX_RUN_SEGS
            {
                segs.extend([PathSeg::Move(pt(a.0), pt(a.1)), PathSeg::Line(pt(b.0), pt(b.1))]);
                continue;
            }
            run = Some((*color, *width));
        } else {
            run = None;
        }
        if items.len() >= MAX_ITEMS {
            break;
        }
        match prim {
            Prim::Path { pts, closed, color, width, fill, dash } => {
                let mut segs: Vec<PathSeg> = Vec::with_capacity(pts.len() + 1);
                if dash.is_empty() || *fill {
                    for (k, &(x, y)) in pts.iter().enumerate() {
                        segs.push(if k == 0 { PathSeg::Move(pt(x), pt(y)) } else { PathSeg::Line(pt(x), pt(y)) });
                    }
                    if *closed {
                        segs.push(PathSeg::Close);
                    }
                } else {
                    // A dashed line: its dashes, each a move and its lines (the page strokes them).
                    for piece in prim.pieces() {
                        for (k, &(x, y)) in piece.iter().enumerate() {
                            segs.push(if k == 0 { PathSeg::Move(pt(x), pt(y)) } else { PathSeg::Line(pt(x), pt(y)) });
                        }
                    }
                }
                items.push(GraphicItem::Path {
                    segs,
                    fill: fill.then(|| rgb(*color)),
                    stroke: Some(rgb(*color)),
                    stroke_width: pt(*width).max(0.25),
                });
            }
            Prim::Dot { at, r, color } => {
                let (cx, cy, rr) = (pt(at.0), pt(at.1), pt(*r));
                let mut segs: Vec<PathSeg> = (0..12)
                    .map(|k| {
                        let a = std::f32::consts::TAU * k as f32 / 12.0;
                        let (x, y) = (cx + rr * a.cos(), cy + rr * a.sin());
                        if k == 0 { PathSeg::Move(x, y) } else { PathSeg::Line(x, y) }
                    })
                    .collect();
                segs.push(PathSeg::Close);
                items.push(GraphicItem::Path { segs, fill: Some(rgb(*color)), stroke: None, stroke_width: 0.0 });
            }
            // A text is laid out level from its anchor, then turned about it by its angle: each
            // piece's rectangle turns about its own centre, its centre about the anchor.
            Prim::Text { at, text, height, angle, anchor, color, look } => {
                // The text style's font (else the document's), bold and italic.
                let text_font = look.font.as_deref().unwrap_or(body_font);
                let angle = if angle.is_finite() { *angle as f32 } else { 0.0 };
                let (sin, cos) = angle.to_radians().sin_cos();
                let (ax, ay) = (pt(at.0), pt(at.1));
                // Counter-clockwise on paper, whose y points down.
                let turn = |rect: [f32; 4]| {
                    if angle == 0.0 {
                        return rect;
                    }
                    let (dx, dy) = (rect[0] + rect[2] / 2.0 - ax, rect[1] + rect[3] / 2.0 - ay);
                    let (cx, cy) = (ax + dx * cos + dy * sin, ay - dx * sin + dy * cos);
                    [cx - rect[2] / 2.0, cy - rect[3] / 2.0, rect[2], rect[3]]
                };
                let size = (pt(*height) * CAP_TO_EM).clamp(2.0, 400.0);
                // Plain, unit and missing pieces (a unit in the Unit style's colour, a placeholder
                // whose parameter is not defined in red), and parameters set as the equations
                // set them: the math font, the symbol in math italics, its subscript smaller and
                // lowered.
                let pieces = strix_cad::spans(text);
                let piece_size = |p: &strix_cad::Span| if p.sub { size * SUB_SCALE } else { size };
                let piece_w = |p: &strix_cad::Span| {
                    measure(if p.math { MATH_FONT } else { text_font }, look.bold && !p.math, p.symbol || look.italic, &p.text, piece_size(p))
                };
                let widths: Vec<f32> = pieces.iter().map(piece_w).collect();
                let w: f32 = widths.iter().sum();
                let (x, y) = (pt(at.0), pt(at.1));
                let (rect, align) = match anchor {
                    Anchor::MiddleRight => ([x - w, y - size * 0.55, w, size * 1.1], TextAlign::Right),
                    Anchor::MiddleLeft => ([x, y - size * 0.55, w, size * 1.1], TextAlign::Left),
                    Anchor::BottomCenter => ([x - w / 2.0, y - size * 1.1, w, size * 1.1], TextAlign::Center),
                    Anchor::TopCenter => ([x - w / 2.0, y, w, size * 1.1], TextAlign::Center),
                    Anchor::BaselineLeft => ([x, y - size * 0.9, w, size * 1.1], TextAlign::Left),
                    Anchor::BaselineCenter => ([x - w / 2.0, y - size * 0.9, w, size * 1.1], TextAlign::Center),
                    Anchor::BaselineRight => ([x - w, y - size * 0.9, w, size * 1.1], TextAlign::Right),
                };
                let plain = |p: &strix_cad::Span| p.kind == strix_cad::SpanKind::Plain && !p.math && !p.sub;
                if pieces.len() <= 1 && pieces.iter().all(plain) {
                    items.push(GraphicItem::Text {
                        rect: turn(rect),
                        text: pieces.into_iter().map(|p| p.text).collect(),
                        size,
                        color: rgb(*color),
                        bold: look.bold,
                        italic: look.italic,
                        align,
                        font: look.font.clone(),
                        angle,
                    });
                } else {
                    // Pieces one after the other, from where the whole text starts, each as wide as
                    // its glyphs.
                    let [x0, y0, _, h0] = rect;
                    let mut cursor = x0;
                    for (piece, pw) in pieces.into_iter().zip(widths) {
                        let ps = piece_size(&piece);
                        let c = match piece.kind {
                            strix_cad::SpanKind::Unit => unit_color,
                            strix_cad::SpanKind::Missing => {
                                Rgb(strix_cad::MISSING_COLOR[0], strix_cad::MISSING_COLOR[1], strix_cad::MISSING_COLOR[2])
                            }
                            strix_cad::SpanKind::Plain => rgb(*color),
                        };
                        let align = TextAlign::Left;
                        // A subscript sits lower: its box's bottom a quarter of the size below.
                        let top = if piece.sub { y0 + h0 - ps * 1.1 + size * 0.25 } else { y0 };
                        let height = if piece.sub { ps * 1.1 } else { h0 };
                        items.push(GraphicItem::Text {
                            rect: turn([cursor, top, pw, height]),
                            text: piece.text,
                            size: ps,
                            color: c,
                            bold: look.bold && !piece.math,
                            italic: piece.symbol || look.italic,
                            align,
                            font: if piece.math { Some(MATH_FONT.to_string()) } else { look.font.clone() },
                            angle,
                        });
                        cursor += pw;
                    }
                }
            }
        }
    }
    (pt(r.size.0), pt(r.size.1), items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wordcraft_doc::{Block, Document};

    fn session() -> Session {
        let mut s = Session::new(Document::new());
        s.run("document.setText", &json!({"text": ""})).unwrap();
        s
    }

    /// The first CAD graphic in the body.
    fn drawing(s: &Session) -> Option<(f32, f32, Arc<Graphic>)> {
        s.doc.body.iter().find_map(|b| match &**b {
            Block::Para(p) => p.objects.iter().find_map(|o| match o {
                InlineObject::Graphic { w, h, graphic, .. } if graphic.kind == GraphicKind::Cad => Some((*w, *h, graphic.clone())),
                _ => None,
            }),
            _ => None,
        })
    }

    /// The drawing's text, its pieces joined (a unit is a piece of its own).
    fn texts(g: &Graphic) -> Vec<String> {
        vec![g.items.iter().filter_map(|i| if let GraphicItem::Text { text, .. } = i { Some(text.as_str()) } else { None }).collect::<String>()]
    }

    /// A parameter in a drawing's text is set as the equations set it: the math font, the
    /// symbol in italics, its subscript smaller and lower; the rest stays plain.
    #[test]
    fn parameters_are_set_as_equations_on_the_page() {
        let d = Drawing::from_json(r#"{"unit":"m","entities":[{"type":"text","p":[0,0],"value":"Load {{F_k:both}}","height":2.5}]}"#).unwrap();
        let mut ws = strix_calc::Worksheet::new(strix_calc::Options::default());
        ws.eval_statement("F_k = 12 kN");
        let (_, _, items) = items_of(&d.render(Some(&ws)), Rgb(0, 0x70, 0xC0), "Calibri");
        let texts: Vec<(&str, f32, Option<&str>, f32)> = items
            .iter()
            .filter_map(|i| match i {
                GraphicItem::Text { text, size, font, rect, .. } => Some((text.as_str(), *size, font.as_deref(), rect[1] + rect[3])),
                _ => None,
            })
            .collect();
        let names: Vec<&str> = texts.iter().map(|t| t.0).collect();
        assert_eq!(names, ["Load ", "F", "k", " = 12 ", "kN"]);
        let italic: Vec<bool> = items.iter().filter_map(|i| if let GraphicItem::Text { italic, .. } = i { Some(*italic) } else { None }).collect();
        assert_eq!(italic, [false, true, true, false, false], "the symbol in italics");
        assert_eq!(texts[0].2, None, "plain text in the drawing's font");
        assert!(texts[1..].iter().all(|t| t.2 == Some(MATH_FONT)), "{texts:?}");
        assert!(texts[2].1 < texts[1].1 && texts[2].3 > texts[1].3, "the subscript smaller and lower: {texts:?}");
    }

    #[test]
    fn a_drawing_follows_the_parameters_before_it() {
        let mut s = session();
        s.run("calc.insert", &json!({"text": "L = 4.5 m"})).unwrap();
        s.run("text.insert", &json!({"text": " "})).unwrap();
        let beam = json!({"unit": "m", "scale": 50, "entities": [
            {"type": "line", "a": [0, 0], "b": ["L", 0]},
            {"type": "dim", "a": [0, 0], "b": ["L", 0], "offset": -8, "text": "L = {{L}}"}
        ]});
        s.run("insert.cad", &json!({"drawing": beam})).unwrap();
        let (w, _, g) = drawing(&s).expect("a drawing");
        assert_eq!(texts(&g), ["L = 4.5 m"]);
        // The unit is in the Unit style's colour, like the calculations' units.
        let unit = super::super::calc::unit_color(&mut s);
        assert!(g.items.iter().any(|i| matches!(i, GraphicItem::Text { text, color, .. } if text == "m" && *color == unit)));
        // 4.5 m at 1:50 is 90 mm of paper, plus margins.
        assert!((94.0..100.0).contains(&(w / PT_PER_MM)), "{w}");
        // Changing L redraws it.
        s.sel = Selection::caret(wordcraft_doc::Pos { off: 0, ..s.sel.focus.clone() });
        s.run("equation.set", &json!({"linear": "L = 6 m"})).unwrap();
        s.run("equation.exit", &json!({})).ok();
        let (w, _, g) = drawing(&s).unwrap();
        assert_eq!(texts(&g), ["L = 6 m"]);
        assert!((124.0..130.0).contains(&(w / PT_PER_MM)), "{w}");
        // It survives saving.
        let bytes = crate::io::save_bytes("d.xmath", &s.doc).unwrap();
        let back = Session::new(crate::io::open_bytes("d.xmath", &bytes).unwrap());
        let (_, _, g) = drawing(&back).expect("read back");
        assert!(g.cad.as_deref().is_some_and(|c| c.contains("\"L\"")));
        assert_eq!(texts(&g), ["L = 6 m"], "drawn again on open");
    }

    /// The editor's view of the selected drawing is kept between frames, and follows the
    /// parameters before it when they change.
    #[test]
    fn the_edited_drawing_follows_its_parameters() {
        let mut s = session();
        s.run("calc.insert", &json!({"text": "L = 4.5 m"})).unwrap();
        s.run("text.insert", &json!({"text": " "})).unwrap();
        let beam = json!({"unit": "m", "entities": [{"type": "line", "a": [0, 0], "b": ["L", 0]}]});
        s.run("insert.cad", &json!({"drawing": beam})).unwrap();
        let on_drawing = s.sel.clone();
        let end = |s: &Session| match edited_drawing(s).map(|d| d.entities[0].kind.clone()) {
            Some(strix_cad::Kind::Line { b, .. }) => b.0.value,
            other => panic!("{other:?}"),
        };
        assert!(editable(&s));
        assert_eq!(end(&s), 4.5);
        assert_eq!(end(&s), 4.5, "asked again");
        s.sel = Selection::caret(wordcraft_doc::Pos { off: 0, ..s.sel.focus.clone() });
        assert!(!editable(&s), "the caret is not on a drawing");
        s.run("equation.set", &json!({"linear": "L = 6 m"})).unwrap();
        s.run("equation.exit", &json!({})).ok();
        s.sel = on_drawing;
        assert_eq!(end(&s), 6.0);
        assert_eq!(edited(&s).map(|(_, d, ..)| d.entities.len()), Some(1));
    }

    /// Drawing Scale: a fixed 1:n, or the largest standard scale that fits the space (by
    /// default the page's text width), which follows a parameter that grows the drawing.
    #[test]
    fn the_scale_is_fixed_or_fits_the_page() {
        let mut s = session();
        s.run("calc.insert", &json!({"text": "L = 4.5 m"})).unwrap();
        s.run("text.insert", &json!({"text": " "})).unwrap();
        let beam = json!({"unit": "m", "scale": 1, "entities": [{"type": "line", "a": [0, 0], "b": ["L", 0]}]});
        s.run("insert.cad", &json!({"drawing": beam})).unwrap();
        let r = s.run("cad.scale", &json!({"fit": true})).unwrap();
        // A4 with 2.5 cm margins: 160 mm of text; 4.5 m fits at 1:50 (90 mm), not at 1:25.
        assert_eq!(r["scale"], json!(50.0), "{r}");
        let (w, ..) = drawing(&s).unwrap();
        assert!(w / PT_PER_MM <= 160.0 + 1e-3);
        // L = 9 m: the next scale that fits.
        let sel = s.sel.clone();
        s.sel = Selection::caret(wordcraft_doc::Pos { off: 0, ..s.sel.focus.clone() });
        s.run("equation.set", &json!({"linear": "L = 9 m"})).unwrap();
        s.run("equation.exit", &json!({})).ok();
        s.sel = sel;
        let (w, ..) = drawing(&s).unwrap();
        assert!((90.0..100.0).contains(&(w / PT_PER_MM)), "1:100: {w}");
        // The scales that fit: from 1:100 on. A scale picked among them keeps the maximum size;
        // one that does not fit is refused; fitted again on request.
        let r = s.run("cad.scale", &json!({})).unwrap();
        assert_eq!(r["scales"][0], json!(100.0), "{r}");
        assert!(s.run("cad.scale", &json!({"scale": 20})).is_err());
        let r = s.run("cad.scale", &json!({"scale": 200})).unwrap();
        assert_eq!((r["scale"].clone(), r["fit"]["fixed"].clone()), (json!(200.0), json!(true)));
        let r = s.run("cad.scale", &json!({"auto": true})).unwrap();
        assert_eq!((r["scale"].clone(), r["fit"]["fixed"].clone()), (json!(100.0), json!(false)));
        // A maximum size alone, then none: any scale.
        let r = s.run("cad.scale", &json!({"max": {"width": 50}})).unwrap();
        assert_eq!(r["scale"], json!(200.0), "{r}");
        s.run("cad.scale", &json!({"max": false})).unwrap();
        let r = s.run("cad.scale", &json!({"scale": 20})).unwrap();
        assert_eq!((r["scale"].clone(), r["fit"].clone()), (json!(20.0), Value::Null));
        assert!(s.run("cad.scale", &json!({"max": "page"})).is_err());
        assert!(s.run("cad.scale", &json!({"scale": -1})).is_err());
        assert!(s.run("cad.scale", &json!({"fit": "page"})).is_err());
        let r = s.run("cad.scale", &json!({"fit": {"width": 50}})).unwrap();
        // 9 m in 50 mm: 1:200 is 45 mm plus the 2 mm padding each side; 1:100 would be 90.
        assert_eq!(r["scale"], json!(200.0), "{r}");
        // The fitted scale is kept in the source; the drawing can be wrapped like a picture,
        // and stays so once saved.
        assert_eq!(s.run("cad.get", &json!({})).unwrap()["drawing"]["scale"], json!(200.0));
        s.run("arrange.wrap", &json!({"wrap": "square"})).unwrap();
        let bytes = crate::io::save_bytes("d.docx", &s.doc).unwrap();
        let back = crate::io::open_bytes("d.docx", &bytes).unwrap();
        let wrapped = back.body.iter().any(|b| {
            matches!(&**b, wordcraft_doc::Block::Para(p) if p.objects.iter().any(|o| matches!(o, InlineObject::Graphic { float, graphic, .. }
                if graphic.kind == GraphicKind::Cad && float.wrap == wordcraft_doc::para::Wrap::Square)))
        });
        assert!(wrapped, "the wrap survives saving");
    }

    /// A drawing knows the space where it sits: its column's width and the height down to the
    /// page's bottom margin; a new drawing takes it as its maximum size (not binding: a larger
    /// fixed scale stays possible once the maximum size is off). Scales may be typed as written.
    #[test]
    fn a_drawing_knows_the_space_where_it_sits() {
        let mut s = session();
        s.run("insert.cad", &json!({})).unwrap();
        let top = s.run("cad.space", &json!({})).unwrap();
        // The text width, and the text height from the top of the text area (the drawing is
        // the first thing on the page).
        let (text_w, text_h) = s.doc.sections().first().map(|(_, sp)| (sp.text_width(), sp.text_height())).unwrap();
        let mm = |pt: f32| f64::from(pt / PT_PER_MM);
        assert!((top["width"].as_f64().unwrap() - mm(text_w)).abs() < 0.5, "{top}");
        assert!((top["height"].as_f64().unwrap() - mm(text_h)).abs() < 1.0, "{top}");
        let fit = s.run("cad.get", &json!({})).unwrap()["drawing"]["fit"].clone();
        assert!((fit["width"].as_f64().unwrap() - mm(text_w)).abs() < 0.5, "a new drawing fits the space: {fit}");
        // In a document with text and calculations, at the caret where it opens.
        let mut sample = Session::new(crate::sample::sample_document());
        // Laid out before (as the window has it): the insert lays it out again.
        let _ = sample.layout();
        sample.run("insert.cad", &json!({})).unwrap();
        let src = sample.run("cad.get", &json!({})).unwrap();
        assert!(src["drawing"]["fit"].is_object(), "{}", sample.run("cad.space", &json!({})).map(|v| v.to_string()).unwrap_or_default());
        // Lower on the page, less height; two columns, half the width less the gap.
        let mut s = session();
        s.run("text.insert", &json!({"text": "One\nTwo\nThree\nFour\n"})).unwrap();
        s.run("layout.columns", &json!({"count": 2})).unwrap();
        s.run("insert.cad", &json!({})).unwrap();
        let low = s.run("cad.space", &json!({})).unwrap();
        assert!(low["height"].as_f64().unwrap() < top["height"].as_f64().unwrap() - 10.0, "{low}");
        assert!(low["width"].as_f64().unwrap() < mm(text_w) / 2.0, "{low}");
        // Typed scales.
        s.run("cad.scale", &json!({"max": false})).unwrap();
        assert_eq!(s.run("cad.scale", &json!({"scale": "1:75"})).unwrap()["scale"], json!(75.0));
        assert_eq!(s.run("cad.scale", &json!({"scale": "2:1"})).unwrap()["scale"], json!(0.5));
        for junk in [json!("1:0"), json!("abc"), json!(0), json!(-3), json!(null), json!([1]), json!({"n": 2})] {
            assert!(s.run("cad.scale", &json!({"scale": junk})).is_err(), "{junk}");
        }
        assert_eq!(s.run("cad.scale", &json!({"scale": 1e300})).unwrap()["scale"], json!(strix_cad::SCALE_RANGE.1));
        assert_eq!(scale_param(&json!("1:x")), Err(scale_param(&json!(-1)).unwrap_err()));
    }

    #[test]
    fn the_selected_drawing_can_be_read_and_extended() {
        let mut s = session();
        s.run("insert.cad", &json!({})).unwrap();
        let got = s.run("cad.get", &json!({})).unwrap();
        assert_eq!(got["drawing"]["entities"].as_array().map(Vec::len), Some(3));
        let r = s.run("cad.add", &json!({"entity": {"type": "circle", "c": [60, 30], "r": 10}})).unwrap();
        assert_eq!(r["id"], "e4");
        assert_eq!(s.run("cad.get", &json!({})).unwrap()["drawing"]["entities"].as_array().map(Vec::len), Some(4));
        assert!(s.run("cad.add", &json!({"entity": {"type": "nope"}})).is_err());
        assert!(s.run("cad.set", &json!({"drawing": "{"})).is_err());
    }

    #[test]
    fn hostile_params_never_panic() {
        let mut s = session();
        for v in
            [json!({"drawing": 5}), json!({"drawing": "x"}), json!({"drawing": {"entities": [{"type": "circle", "c": ["1/0", 0], "r": "-1e308"}]}})]
        {
            let _ = s.run("insert.cad", &v);
        }
        let _ = s.run("cad.get", &json!({}));
        let _ = s.run("cad.add", &json!({"entity": null}));
    }

    /// `cad.get` reports the errors where the drawing sits: a parameter defined before it is no
    /// error; a missing one is.
    #[test]
    fn cad_get_reports_errors_where_the_drawing_sits() {
        let mut s = session();
        s.run("calc.insert", &json!({"text": "L = 4.5 m"})).unwrap();
        s.run("text.insert", &json!({"text": " "})).unwrap();
        let beam = json!({"unit": "m", "entities": [{"type": "line", "a": [0, 0], "b": ["L", 0]}, {"type": "line", "a": [0, 0], "b": ["Z", 0]}]});
        s.run("insert.cad", &json!({"drawing": beam})).unwrap();
        let got = s.run("cad.get", &json!({})).unwrap();
        let errors = got["errors"].as_array().cloned().unwrap_or_default();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].to_string().contains('Z'), "{errors:?}");
        // The drawing comes back resolved: L is 4.5 there.
        assert_eq!(got["drawing"]["entities"][0]["b"][0], json!("L"));
    }

    /// A padding of the wrong shape is refused, not read as the default.
    #[test]
    fn a_padding_of_the_wrong_shape_is_refused() {
        let mut s = session();
        s.run("insert.cad", &json!({})).unwrap();
        assert!(s.run("cad.padding", &json!({"value": [1, 2]})).is_err());
        assert!(s.run("cad.padding", &json!({"value": [1, 2, "x", 4]})).is_err());
        assert!(s.run("cad.padding", &json!({"value": [1, 2, 3, 4]})).is_ok());
        assert!(s.run("cad.padding", &json!({"value": 3})).is_ok());
    }

    /// A drawing whose source can't be read is never replaced by a blank one.
    #[test]
    fn an_unreadable_drawing_is_never_overwritten() {
        let mut s = session();
        s.run("insert.cad", &json!({})).unwrap();
        let (pos, _) = selected_source(&s).unwrap();
        let para = s.doc.para_mut(pos.story, &pos.path).unwrap();
        let Some(InlineObject::Graphic { graphic, .. }) = para.object_at_mut(pos.off) else { panic!() };
        Arc::make_mut(graphic).cad = Some("{not json".into());
        for (id, v) in [
            ("cad.padding", json!({"value": 3})),
            ("cad.scale", json!({"scale": 10})),
            ("cad.add", json!({"entity": {"type": "point", "p": [0, 0]}})),
        ] {
            assert!(s.run(id, &v).is_err(), "{id}");
        }
        assert_eq!(selected_source(&s).unwrap().1.as_ref(), "{not json");
    }

    /// Deleting the calculation a drawing reads redraws it with the parameter missing.
    #[test]
    fn a_drawing_shows_a_deleted_parameter_as_missing() {
        let mut s = session();
        s.run("calc.insert", &json!({"text": "L = 4.5 m"})).unwrap();
        s.run("text.insert", &json!({"text": " "})).unwrap();
        let beam = json!({"unit": "m", "entities": [{"type": "dim", "a": [0, 0], "b": [5, 0], "text": "{{L}}"}]});
        s.run("insert.cad", &json!({"drawing": beam})).unwrap();
        assert_eq!(texts(&drawing(&s).unwrap().2), ["4.5 m"]);
        // The equation goes (the first paragraph's text and objects).
        let Block::Para(p) = Arc::make_mut(&mut s.doc.body[0]) else { panic!() };
        let at = p.text.char_indices().map(|(i, _)| i).find(|&i| matches!(p.object_at(i), Some(InlineObject::Equation { .. }))).unwrap();
        p.delete(at, at + wordcraft_doc::para::OBJ.len_utf8()).unwrap();
        p.touch();
        s.touch();
        crate::cmd::calc::auto_recalculate(&mut s);
        let shown = texts(&drawing(&s).unwrap().2);
        assert!(shown[0].contains("L?"), "{shown:?}");
    }

    /// Recalculating a document with no calculation leaves its styles alone.
    #[test]
    fn recalculating_without_calculations_adds_no_style() {
        let mut s = session();
        s.run("text.insert", &json!({"text": "Plain text"})).unwrap();
        let before = s.doc.styles.styles.len();
        s.run("calc.recalculate", &json!({})).unwrap();
        assert_eq!(s.doc.styles.styles.len(), before);
        assert!(s.doc.styles.get(super::super::calc::UNIT_STYLE).is_none());
    }

    /// Fine hatches drawn first (they go behind everything) no longer use up the page's item
    /// budget: their lines are pieces of one item each, and the dimension drawn over them
    /// still reaches the page and the PDF.
    #[test]
    fn hatch_lines_leave_room_for_what_is_drawn_over_them() {
        let mut entities = Vec::new();
        for i in 0..30 {
            let x = i * 300;
            entities.push(format!(r#"{{"type":"hatch","boundary":["b{i}"],"pattern":"cross","spacing":0.5}}"#));
            entities.push(format!(r#"{{"id":"b{i}","type":"polyline","closed":true,"pts":[[{x},0],[{},0],[{},250],[{x},250]]}}"#, x + 250, x + 250));
        }
        entities.push(r#"{"type":"dim","a":[0,0],"b":[250,0],"text":"last"}"#.to_string());
        let d = Drawing::from_json(&format!(r#"{{"entities":[{}]}}"#, entities.join(","))).unwrap();
        let rendered = d.render(None);
        assert!(rendered.prims.len() > MAX_ITEMS, "{}", rendered.prims.len());
        let (_, _, items) = items_of(&rendered, Rgb(0, 0, 0), "Calibri");
        assert!(items.len() < MAX_ITEMS, "{}", items.len());
        assert!(items.iter().any(|i| matches!(i, GraphicItem::Text { text, .. } if text == "last")), "the dimension is drawn");
        // Every hatch line is still drawn, as a move and a line.
        let moves: usize = items
            .iter()
            .map(|i| if let GraphicItem::Path { segs, .. } = i { segs.iter().filter(|s| matches!(s, PathSeg::Move(..))).count() } else { 0 })
            .sum();
        let strokes = rendered.prims.iter().filter(|p| matches!(p, Prim::Path { pts, .. } if pts.len() == 2)).count();
        assert!(moves >= strokes, "{moves} of {strokes}");
    }

    /// On the page a dashed line is drawn as its dashes and a turned text at its angle, its
    /// rectangle's centre turned about the text's anchor.
    #[test]
    fn page_items_draw_dashes_and_turned_texts() {
        let d = Drawing::from_json(
            r#"{"layers":[{"name":"0","dash":"dashed"}],"entities":[{"type":"line","a":[0,0],"b":[20,0]},
                {"type":"text","p":[0,10],"value":"AB","height":5,"angle":90}]}"#,
        )
        .unwrap();
        let rendered = d.render(None);
        let (_, _, items) = items_of(&rendered, Rgb(0, 0, 0), "Calibri");
        let segs = items.iter().find_map(|i| if let GraphicItem::Path { segs, .. } = i { Some(segs) } else { None }).unwrap();
        // 20 mm dashed 4 on / 2 off: four dashes, each a move.
        assert_eq!(segs.iter().filter(|s| matches!(s, PathSeg::Move(..))).count(), 4, "{segs:?}");
        let (rect, angle) =
            items.iter().find_map(|i| if let GraphicItem::Text { rect, angle, .. } = i { Some((*rect, *angle)) } else { None }).unwrap();
        assert_eq!(angle, 90.0);
        // On its baseline's left end, turned a quarter left: the text runs up from its anchor,
        // so its centre is above the anchor by about half its length and a little left of it.
        let at = rendered.prims.iter().find_map(|p| if let Prim::Text { at, .. } = p { Some(*at) } else { None }).unwrap();
        let (ax, ay) = ((at.0 as f32) * PT_PER_MM, (at.1 as f32) * PT_PER_MM);
        let (cx, cy) = (rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0);
        assert!(cx < ax && (cy - (ay - rect[2] / 2.0)).abs() < 1.0, "{rect:?} from {ax},{ay}");
        // Saved without an angle when level.
        let level = GraphicItem::Text {
            rect: [0.0; 4],
            text: "x".into(),
            size: 1.0,
            color: Rgb(0, 0, 0),
            bold: false,
            italic: false,
            align: TextAlign::Left,
            font: None,
            angle: 0.0,
        };
        assert!(!serde_json::to_string(&level).unwrap().contains("angle"));
    }
}
