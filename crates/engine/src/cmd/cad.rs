//! CAD drawings in the document (docs/cad.md): a `GraphicKind::Cad` graphic holds a `strix-cad`
//! drawing's source; its items are drawn from it, in reading order with the calculations, so a
//! coordinate written as a parameter (`"L"`) follows the value defined before the drawing.

use std::sync::Arc;

use serde_json::{Value, json};
use strix_cad::{Anchor, Drawing, Entity, MISSING_END, MISSING_START, Prim, Rendered, UNIT_END, UNIT_START};
use strix_doc::InlineObject;
use strix_doc::graphic::{Graphic, GraphicItem, GraphicKind, PathSeg, TextAlign};
use strix_doc::para::Float;
use strix_doc::props::Rgb;

use crate::{CmdError, CmdResult, CommandSpec, Selection, Session};

/// Points in a millimetre.
const PT_PER_MM: f32 = 72.0 / 25.4;
/// Most items one drawing makes (each segment of a tessellated arc counts once).
const MAX_ITEMS: usize = 20_000;
/// Font size of a text whose capital letters are `h` mm high (cap height ≈ 0.7 em).
const CAP_TO_EM: f32 = 1.0 / 0.7;

pub fn specs() -> Vec<CommandSpec> {
    let selected = |s: &Session| if selected_drawing(s).is_some() { None } else { Some("select a CAD drawing first") };
    vec![
        CommandSpec::new("insert.cad", "CAD Drawing", "Insert › Illustrations", insert).params(
            r#"{"drawing"?: object|string (a strix-cad drawing: {"unit", "scale", "layers", "entities": [{"type": "line", "a": [0,0], "b": ["L",0]} …]}; default: a starter drawing)} → inserts a CAD drawing in line at the caret; coordinates may be expressions of the calculations' parameters"#,
        ),
        CommandSpec::new("cad.get", "CAD Drawing Source", "Insert › Illustrations", get).pure().when(selected).params(r#"→ {"drawing": object, "errors": [[expression, why]]}: the selected drawing"#),
        CommandSpec::new("cad.set", "Set CAD Drawing", "Insert › Illustrations", set).when(selected).params(r#"{"drawing": object|string} → replaces the selected drawing's source and redraws it"#),
        CommandSpec::new("cad.add", "Add to CAD Drawing", "Insert › Illustrations", add)
            .when(selected)
            .params(r#"{"entity": object ({"type": "line|circle|arc|polyline|point|text|dim", …})} → adds an entity to the selected drawing; returns its id"#),
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
    let drawing = match drawing_param(v, "drawing")? {
        Some(d) => d,
        None => Drawing::from_json(STARTER).map_err(CmdError::Failed)?,
    };
    let (w, h, items) = items_of(&drawing.render(None), super::calc::unit_color(s));
    let graphic = Graphic { kind: GraphicKind::Cad, items, w, h, cad: Some(drawing.to_json().into()), ..Default::default() };
    let obj = InlineObject::Graphic { w, h, alt: String::new(), float: Float::default(), graphic: Arc::new(graphic) };
    let props = s.typing_props();
    let at = super::delete_selection(s)?;
    let end = s.doc.insert_object(&at, obj, &props)?;
    s.sel = Selection { anchor: at, focus: end };
    // Its parameters come from the calculations before it.
    super::calc::recalculate_now(s);
    super::sel_result(s)
}

/// The selected CAD drawing for the editor: where it is, the drawing with its coordinates'
/// values worked out where it sits, and how it renders (with its paper origin and scale).
pub fn edited(s: &Session) -> Option<(strix_doc::Pos, Drawing, Rendered, Vec<String>)> {
    let (pos, obj) = super::objects::selected(s)?;
    let InlineObject::Graphic { graphic, .. } = obj else { return None };
    if graphic.kind != GraphicKind::Cad {
        return None;
    }
    let source = graphic.cad.clone()?;
    let (d, r, names) = super::calc::resolved_cad(s, &source)?;
    Some((pos, d, r, names))
}

/// The selected CAD drawing: where it is and its source.
fn selected_drawing(s: &Session) -> Option<(strix_doc::Pos, Drawing)> {
    let (pos, obj) = super::objects::selected(s)?;
    match obj {
        InlineObject::Graphic { graphic, .. } if graphic.kind == GraphicKind::Cad => {
            let d = Drawing::from_json(graphic.cad.as_deref().unwrap_or_default()).unwrap_or_default();
            Some((pos, d))
        }
        _ => None,
    }
}

fn get(s: &mut Session, _: &Value) -> CmdResult {
    let (_, mut d) = selected_drawing(s).ok_or_else(|| CmdError::Disabled("select a CAD drawing first".into()))?;
    let opts = super::calc::options_now(s);
    let errors = d.resolve(&strix_calc::Worksheet::new(opts));
    let drawing: Value = serde_json::from_str(&d.to_json()).unwrap_or(Value::Null);
    Ok(json!({"drawing": drawing, "errors": errors}))
}

/// Stores `d` as the selected drawing's source, then redraws.
fn store(s: &mut Session, pos: &strix_doc::Pos, d: &Drawing) -> Result<(), CmdError> {
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

fn add(s: &mut Session, v: &Value) -> CmdResult {
    let (pos, mut d) = selected_drawing(s).ok_or_else(|| CmdError::Disabled("select a CAD drawing first".into()))?;
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

/// What a piece of a drawing's text is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Piece {
    Plain,
    Unit,
    Missing,
}

/// The graphic items of a rendered drawing (points from its top-left corner) and its size.
pub(crate) fn items_of(r: &Rendered, unit_color: Rgb) -> (f32, f32, Vec<GraphicItem>) {
    let pt = |v: f64| (v as f32) * PT_PER_MM;
    let rgb = |c: [u8; 3]| Rgb(c[0], c[1], c[2]);
    let mut items = Vec::new();
    for prim in &r.prims {
        if items.len() >= MAX_ITEMS {
            break;
        }
        match prim {
            Prim::Path { pts, closed, color, width, .. } => {
                let mut segs: Vec<PathSeg> = Vec::with_capacity(pts.len() + 1);
                for (k, &(x, y)) in pts.iter().enumerate() {
                    segs.push(if k == 0 { PathSeg::Move(pt(x), pt(y)) } else { PathSeg::Line(pt(x), pt(y)) });
                }
                if *closed {
                    segs.push(PathSeg::Close);
                }
                items.push(GraphicItem::Path { segs, fill: None, stroke: Some(rgb(*color)), stroke_width: pt(*width).max(0.25) });
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
            // Texts are set horizontally for now (a rotated dimension's text reads level).
            Prim::Text { at, text, height, anchor, color, .. } => {
                let size = (pt(*height) * CAP_TO_EM).clamp(2.0, 400.0);
                // Plain, unit and missing pieces: a unit is set in the Unit style's colour, a
                // placeholder whose parameter is not defined in red.
                let mut pieces: Vec<(Piece, String)> = vec![(Piece::Plain, String::new())];
                for c in text.chars() {
                    match c {
                        UNIT_START => pieces.push((Piece::Unit, String::new())),
                        MISSING_START => pieces.push((Piece::Missing, String::new())),
                        UNIT_END | MISSING_END => pieces.push((Piece::Plain, String::new())),
                        c => {
                            if let Some((_, t)) = pieces.last_mut() {
                                t.push(c);
                            }
                        }
                    }
                }
                pieces.retain(|(_, t)| !t.is_empty());
                let visible: String = pieces.iter().map(|(_, t)| t.as_str()).collect();
                let char_w = 0.6 * size;
                let w = char_w * visible.chars().count() as f32;
                let (x, y) = (pt(at.0), pt(at.1));
                let (rect, align) = match anchor {
                    Anchor::MiddleRight => ([x - w, y - size * 0.55, w, size * 1.1], TextAlign::Right),
                    Anchor::MiddleLeft => ([x, y - size * 0.55, w, size * 1.1], TextAlign::Left),
                    Anchor::BottomCenter => ([x - w / 2.0, y - size * 1.1, w, size * 1.1], TextAlign::Center),
                    Anchor::BaselineLeft => ([x, y - size * 0.9, w, size * 1.1], TextAlign::Left),
                };
                if pieces.len() <= 1 {
                    items.push(GraphicItem::Text { rect, text: visible, size, color: rgb(*color), bold: false, align, font: None });
                } else {
                    // Pieces one after the other, from where the whole text starts.
                    let [x0, y0, _, h0] = rect;
                    let mut cursor = match align {
                        TextAlign::Left => x0,
                        TextAlign::Center | TextAlign::Right => x0 + (rect[2] - w) * if align == TextAlign::Center { 0.5 } else { 1.0 },
                    };
                    // Widths are estimates: each piece but the last ends flush on its boundary and
                    // the last starts on it, so neighbours touch whatever the glyphs' real widths.
                    let n = pieces.len();
                    for (k, (unit, piece)) in pieces.into_iter().enumerate() {
                        let pw = char_w * piece.chars().count() as f32;
                        let c = match unit {
                            Piece::Unit => unit_color,
                            Piece::Missing => Rgb(0xC0, 0x1C, 0x1C),
                            Piece::Plain => rgb(*color),
                        };
                        let align = if k + 1 < n { TextAlign::Right } else { TextAlign::Left };
                        items.push(GraphicItem::Text { rect: [cursor, y0, pw, h0], text: piece, size, color: c, bold: false, align, font: None });
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
    use strix_doc::{Block, Document};

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
        s.sel = Selection::caret(strix_doc::Pos { off: 0, ..s.sel.focus.clone() });
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
}
