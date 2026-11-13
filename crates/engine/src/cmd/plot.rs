//! Mathematical and experimental Cartesian plots in the document (`docs/plot.md`).
//!
//! A `GraphicKind::Plot` graphic holds a [`strix_plot::Plot`] JSON source; its vector items and SVG
//! fallback are rendered in reading order against the active `strix_calc::Worksheet` (either the
//! main document sheet or an exercise's isolated variant sandbox), honouring `exercise.show` so
//! student copies (`Show = false`) print the blank/base Cartesian grid with identical dimensions
//! while teacher copies (`Show = true`) overlay `answer` curves, notable points and integrals.

use std::sync::Arc;

use serde_json::{Value, json};
use strix_plot::{DEFAULT_COLORS, Plot, PlotAnchor, PlotPrim, PlotRendered, RegressionKind, Series, SeriesKind};
use wordcraft_doc::InlineObject;
use wordcraft_doc::graphic::{Graphic, GraphicItem, GraphicKind, PathSeg, TextAlign};
use wordcraft_doc::para::Float;
use wordcraft_doc::props::Rgb;

use crate::{CmdError, CmdResult, CommandSpec, Selection, Session, p};

const PT_PER_MM: f32 = 72.0 / 25.4;
const MAX_ITEMS: usize = 20_000;
const CAP_TO_EM: f32 = 1.0 / 0.7;

pub fn specs() -> Vec<CommandSpec> {
    let selected = |s: &Session| if is_selected(s) { None } else { Some("select a plot first") };
    vec![
        CommandSpec::new("plot.insert", "Plot", "Math › Drawing", insert).params(
            r#"{"plot"?: object|string, "expr"?: string, "xMin"?: f64, "xMax"?: f64, "yMin"?: f64, "yMax"?: f64, "title"?: string, "xLabel"?: string, "yLabel"?: string, "xUnit"?: string, "yUnit"?: string, "grid"?: bool, "minorGrid"?: bool, "legend"?: bool, "answer"?: bool} → inserts a live mathematical/experimental plot in line at the caret"#,
        ),
        CommandSpec::new("plot.get", "Plot Source", "Math › Drawing", get)
            .pure()
            .when(selected)
            .params(r#"→ {"plot": object, "viewport": [x, y, w, h], "notables": [...], "integrals": [...], "regressions": [...], "errors": [...]}"#),
        CommandSpec::new("plot.set", "Set Plot", "Math › Drawing", set)
            .when(selected)
            .params(r#"{"plot"?: object|string, "width"?: mm, "height"?: mm, "xMin"?: f64, "xMax"?: f64, "yMin"?: f64, "yMax"?: f64, "title"?: string, "xLabel"?: string, "yLabel"?: string, "xUnit"?: string, "yUnit"?: string, "grid"?: bool, "minorGrid"?: bool, "legend"?: bool, "answer"?: bool, "series"?: [...]} → updates the selected plot and redraws it"#),
    ]
}

/// Whether the currently selected inline object is a Strix plot.
pub fn is_selected(s: &Session) -> bool {
    selected_source(s).is_some()
}

/// User-authored strings (titles, units, series labels and formulas) of the currently selected
/// plot, so interface localization audits treat them as document content.
pub fn selected_plot_texts(s: &Session) -> Vec<String> {
    let Some((_, src)) = selected_source(s) else {
        return Vec::new();
    };
    let Ok(p) = Plot::from_json(&src) else {
        return Vec::new();
    };
    let mut out = vec![p.title, p.x_label, p.y_label];
    out.extend(p.x_unit.into_iter().chain(p.y_unit));
    for s in p.series {
        out.push(s.label);
        match s.kind {
            SeriesKind::Function { expr, var, .. } => out.extend([expr, var]),
            SeriesKind::Parametric { x_expr, y_expr, param, .. } => out.extend([x_expr, y_expr, param]),
            SeriesKind::Data { x_col, y_col, dx_col, dy_col, .. } => {
                out.extend([x_col, y_col]);
                out.extend(dx_col.into_iter().chain(dy_col));
            }
        }
    }
    out.into_iter().filter(|t| !t.is_empty()).collect()
}

fn selected_source(s: &Session) -> Option<(wordcraft_doc::Pos, Arc<str>)> {
    let (pos, obj) = super::objects::selected(s)?;
    match obj {
        InlineObject::Graphic { graphic, .. } if graphic.kind == GraphicKind::Plot => {
            Some((pos, graphic.plot.clone().unwrap_or_else(|| Arc::from(""))))
        }
        _ => None,
    }
}

fn starter_plot() -> Plot {
    let mut plot = Plot::default();
    plot.series.push(Series {
        label: "f(x)".to_string(),
        color: DEFAULT_COLORS[0],
        width: 0.5,
        dashed: false,
        answer: false,
        kind: SeriesKind::Function {
            expr: "sin(x)".to_string(),
            var: "x".to_string(),
            domain: None,
            roots: false,
            extrema: false,
            tangent_at: None,
            integral: None,
        },
    });
    plot.sanitized()
}

fn num_f64(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(Value::as_f64).filter(|x| x.is_finite())
}

fn plot_param(v: &Value, key: &str) -> Result<Option<Plot>, CmdError> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Plot::from_json(text).map(Some).map_err(CmdError::Params),
        Some(other) => Plot::from_json(&other.to_string()).map(Some).map_err(CmdError::Params),
    }
}

fn apply_patch(plot: &mut Plot, v: &Value) -> Result<(), CmdError> {
    if let Some(w) = num_f64(v, "width") {
        plot.width = w;
    }
    if let Some(h) = num_f64(v, "height") {
        plot.height = h;
    }
    if let Some(x0) = num_f64(v, "xMin") {
        plot.x_min = x0;
    }
    if let Some(x1) = num_f64(v, "xMax") {
        plot.x_max = x1;
    }
    if let Some(y0) = num_f64(v, "yMin") {
        plot.y_min = y0;
    }
    if let Some(y1) = num_f64(v, "yMax") {
        plot.y_max = y1;
    }
    apply_patch_strings(plot, v);
    if let Some(g) = p::bool(v, "grid") {
        plot.grid = g;
    }
    if let Some(mg) = p::bool(v, "minorGrid") {
        plot.minor_grid = mg;
    }
    if let Some(leg) = p::bool(v, "legend") {
        plot.legend = leg;
    }
    if let Some(ans) = p::bool(v, "answer") {
        plot.answer = ans;
    }
    if let Some(series_val) = v.get("series") {
        let parsed: Vec<Series> = serde_json::from_value(series_val.clone()).map_err(|e| CmdError::Params(format!("series: {e}")))?;
        plot.series = parsed;
    } else if let Some(expr) = p::str(v, "expr") {
        plot.series = vec![Series {
            label: p::str(v, "label").unwrap_or("f(x)").to_string(),
            color: DEFAULT_COLORS[0],
            width: 0.5,
            dashed: false,
            answer: p::bool(v, "seriesAnswer").unwrap_or(false),
            kind: SeriesKind::Function {
                expr: expr.to_string(),
                var: "x".to_string(),
                domain: None,
                roots: p::bool(v, "roots").unwrap_or(false),
                extrema: p::bool(v, "extrema").unwrap_or(false),
                tangent_at: num_f64(v, "tangentAt"),
                integral: None,
            },
        }];
    }
    if let Some(reg) = p::str(v, "regression")
        && let Some(kind) = RegressionKind::parse(reg)
    {
        for s in &mut plot.series {
            if let SeriesKind::Data { regression, .. } = &mut s.kind {
                *regression = kind;
            }
        }
    }
    *plot = plot.clone().sanitized();
    Ok(())
}

fn apply_patch_strings(plot: &mut Plot, v: &Value) {
    if let Some(t) = p::str(v, "title") {
        plot.title = t.to_string();
    }
    if let Some(xl) = p::str(v, "xLabel") {
        plot.x_label = xl.to_string();
    }
    if let Some(yl) = p::str(v, "yLabel") {
        plot.y_label = yl.to_string();
    }
    if let Some(xu) = p::str(v, "xUnit") {
        plot.x_unit = (!xu.trim().is_empty()).then(|| xu.trim().to_string());
    }
    if let Some(yu) = p::str(v, "yUnit") {
        plot.y_unit = (!yu.trim().is_empty()).then(|| yu.trim().to_string());
    }
}

fn insert(s: &mut Session, v: &Value) -> CmdResult {
    let mut plot = match plot_param(v, "plot")? {
        Some(p) => p,
        None => starter_plot(),
    };
    apply_patch(&mut plot, v)?;
    let rendered = plot.render(None);
    let (w, h, items) = items_of(&rendered, &s.doc.settings.minor_font);
    let svg = Some(rendered.to_svg().into());
    let graphic = Graphic { kind: GraphicKind::Plot, items, w, h, plot: Some(plot.to_json().into()), svg, ..Default::default() };
    let obj = InlineObject::Graphic { w, h, alt: String::new(), float: Float::default(), graphic: Arc::new(graphic) };
    let props = s.typing_props();
    let at = super::delete_selection(s)?;
    let end = s.doc.insert_object(&at, obj, &props)?;
    s.sel = Selection { anchor: at, focus: end };
    super::calc::recalculate_now(s);
    s.touch();
    if p::bool(v, "openPane").unwrap_or(false) {
        s.view.plot_pane = true;
    }
    super::sel_result(s)
}

fn get(s: &mut Session, _: &Value) -> CmdResult {
    let (_, src) = selected_source(s).ok_or_else(|| CmdError::Disabled("select a plot first".into()))?;
    let plot = Plot::from_json(&src).map_err(|e| CmdError::Failed(format!("the plot can't be read: {e}")))?;
    let rendered = super::calc::resolved_plot(s, &src).unwrap_or_else(|| {
        let ws = strix_calc::Worksheet::new(super::calc::options_now(s));
        plot.render(Some(&ws))
    });
    Ok(json!({
        "plot": plot,
        "viewport": rendered.viewport,
        "notables": rendered.notables,
        "integrals": rendered.integrals,
        "regressions": rendered.regressions,
        "errors": rendered.errors,
    }))
}

fn set(s: &mut Session, v: &Value) -> CmdResult {
    let (pos, src) = selected_source(s).ok_or_else(|| CmdError::Disabled("select a plot first".into()))?;
    let mut plot = match plot_param(v, "plot")? {
        Some(p) => p,
        None => Plot::from_json(&src).map_err(|e| CmdError::Failed(format!("the plot can't be read: {e}")))?,
    };
    apply_patch(&mut plot, v)?;
    store(s, &pos, &plot)?;
    get(s, &json!({}))
}

fn store(s: &mut Session, pos: &wordcraft_doc::Pos, plot: &Plot) -> Result<(), CmdError> {
    let json_arc: Arc<str> = plot.to_json().into();
    {
        let para = s.doc.para_mut(pos.story, &pos.path).map_err(|e| CmdError::Failed(e.to_string()))?;
        {
            let Some(InlineObject::Graphic { graphic, .. }) = para.object_at_mut(pos.off) else {
                return Err(CmdError::Failed("no plot there".into()));
            };
            if graphic.kind != GraphicKind::Plot {
                return Err(CmdError::Failed("not a plot".into()));
            }
            let g = Arc::make_mut(graphic);
            g.plot = Some(json_arc);
        }
        para.touch();
    }
    let report = super::calc::recalculate_now(s);
    if report["changed"].as_u64().unwrap_or(0) == 0 {
        // Outside body or unchanged text: render directly against empty worksheet.
        let rendered = plot.render(None);
        let (dw, dh, items) = items_of(&rendered, &s.doc.settings.minor_font);
        let svg: Arc<str> = rendered.to_svg().into();
        if let Ok(p_mut) = s.doc.para_mut(pos.story, &pos.path) {
            let mut updated = false;
            if let Some(InlineObject::Graphic { w: ow, h: oh, graphic: gr, .. }) = p_mut.object_at_mut(pos.off) {
                let g = Arc::make_mut(gr);
                g.items = items;
                g.svg = Some(svg);
                (g.w, g.h) = (dw, dh);
                (*ow, *oh) = (dw, dh);
                updated = true;
            }
            if updated {
                p_mut.touch();
            }
        }
    }
    s.touch();
    Ok(())
}

fn measure(family: &str, bold: bool, text: &str, size: f32) -> f32 {
    let resolved = wordcraft_fonts::word::resolve(family, bold, false);
    let face = resolved.face.get();
    let k = size / face.upem.max(1.0) as f32;
    wordcraft_fonts::shape(face, text, &[], |c| c).iter().map(|g| g.x_advance as f32 * k).sum()
}

fn dash_polyline(pts: &[(f64, f64)], dash: &[f64]) -> Vec<Vec<(f64, f64)>> {
    let pat: Vec<f64> = dash.iter().copied().filter(|&d| d.is_finite() && d > 0.05).collect();
    if pat.is_empty() || pts.len() < 2 {
        return vec![pts.to_vec()];
    }
    let mut pieces: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut pat_idx = 0usize;
    let mut pat_rem = pat[0];
    let mut drawing = true;
    let mut cur: Vec<(f64, f64)> = Vec::new();
    for seg in pts.windows(2) {
        let (mut ax, mut ay) = seg[0];
        let (bx, by) = seg[1];
        let mut rem = (bx - ax).hypot(by - ay);
        if !rem.is_finite() || rem <= 1e-9 {
            continue;
        }
        let (ux, uy) = ((bx - ax) / rem, (by - ay) / rem);
        let mut guard = 0usize;
        while rem > 1e-9 && guard < 4_096 {
            guard += 1;
            let step = rem.min(pat_rem);
            let (nx, ny) = (ax + ux * step, ay + uy * step);
            if drawing {
                if cur.is_empty() {
                    cur.push((ax, ay));
                }
                cur.push((nx, ny));
            }
            ax = nx;
            ay = ny;
            rem -= step;
            pat_rem -= step;
            if pat_rem <= 1e-9 {
                if drawing && cur.len() >= 2 {
                    pieces.push(std::mem::take(&mut cur));
                } else {
                    cur.clear();
                }
                pat_idx = (pat_idx + 1) % pat.len();
                pat_rem = pat[pat_idx];
                drawing = !drawing;
            }
        }
    }
    if drawing && cur.len() >= 2 {
        pieces.push(cur);
    }
    pieces
}

/// Converts a [`PlotRendered`] into `wordcraft_doc` vector [`GraphicItem`]s (points from top-left).
pub(crate) fn items_of(r: &PlotRendered, body_font: &str) -> (f32, f32, Vec<GraphicItem>) {
    let pt = |v: f64| (v as f32) * PT_PER_MM;
    let rgb = |c: [u8; 3]| Rgb(c[0], c[1], c[2]);
    let mut items = Vec::with_capacity(r.prims.len().min(MAX_ITEMS));
    for prim in r.prims.iter().take(MAX_ITEMS) {
        match prim {
            PlotPrim::Rect { rect: [rx, ry, rw, rh], fill, stroke } => {
                let (x0, y0, x1, y1) = (pt(*rx), pt(*ry), pt(*rx + *rw), pt(*ry + *rh));
                let segs = vec![PathSeg::Move(x0, y0), PathSeg::Line(x1, y0), PathSeg::Line(x1, y1), PathSeg::Line(x0, y1), PathSeg::Close];
                items.push(GraphicItem::Path {
                    segs,
                    fill: fill.map(rgb),
                    stroke: stroke.map(|(c, _)| rgb(c)),
                    stroke_width: stroke.map_or(0.0, |(_, w)| pt(w).max(0.25)),
                });
            }
            PlotPrim::Path { pts, closed, stroke, fill, dash } => {
                let mut segs = Vec::with_capacity(pts.len() + 1);
                if dash.is_empty() || fill.is_some() {
                    for (k, &(x, y)) in pts.iter().enumerate() {
                        segs.push(if k == 0 { PathSeg::Move(pt(x), pt(y)) } else { PathSeg::Line(pt(x), pt(y)) });
                    }
                    if *closed {
                        segs.push(PathSeg::Close);
                    }
                } else {
                    for piece in dash_polyline(pts, dash) {
                        for (k, &(x, y)) in piece.iter().enumerate() {
                            segs.push(if k == 0 { PathSeg::Move(pt(x), pt(y)) } else { PathSeg::Line(pt(x), pt(y)) });
                        }
                    }
                }
                if !segs.is_empty() {
                    items.push(GraphicItem::Path {
                        segs,
                        fill: fill.map(rgb),
                        stroke: stroke.map(|(c, _)| rgb(c)),
                        stroke_width: stroke.map_or(0.0, |(_, w)| pt(w).max(0.25)),
                    });
                }
            }
            PlotPrim::Dot { at, r: rad, fill, stroke } => {
                let (cx, cy, rr) = (pt(at.0), pt(at.1), pt(*rad).max(0.5));
                let mut segs: Vec<PathSeg> = (0..12)
                    .map(|k| {
                        let a = std::f32::consts::TAU * k as f32 / 12.0;
                        let (x, y) = (cx + rr * a.cos(), cy + rr * a.sin());
                        if k == 0 { PathSeg::Move(x, y) } else { PathSeg::Line(x, y) }
                    })
                    .collect();
                segs.push(PathSeg::Close);
                items.push(GraphicItem::Path {
                    segs,
                    fill: Some(rgb(*fill)),
                    stroke: stroke.map(|(c, _)| rgb(c)),
                    stroke_width: stroke.map_or(0.0, |(_, w)| pt(w).max(0.25)),
                });
            }
            PlotPrim::Text { at, text, height, bold, anchor, color } => {
                let size = (pt(*height) * CAP_TO_EM).clamp(4.0, 72.0);
                let w = measure(body_font, *bold, text, size).max(size * 0.5);
                let h = size * 1.25;
                let (x, y) = (pt(at.0), pt(at.1));
                let (rect, align) = match anchor {
                    PlotAnchor::Left => ([x, y - h * 0.78, w + 2.0, h], TextAlign::Left),
                    PlotAnchor::Center => ([x - (w + 2.0) * 0.5, y - h * 0.78, w + 2.0, h], TextAlign::Center),
                    PlotAnchor::Right => ([x - (w + 2.0), y - h * 0.78, w + 2.0, h], TextAlign::Right),
                    PlotAnchor::TopCenter => ([x - (w + 2.0) * 0.5, y, w + 2.0, h], TextAlign::Center),
                    PlotAnchor::MiddleRight => ([x - (w + 2.0), y - h * 0.5, w + 2.0, h], TextAlign::Right),
                };
                items.push(GraphicItem::Text {
                    rect,
                    text: text.clone(),
                    size,
                    color: rgb(*color),
                    bold: *bold,
                    italic: false,
                    align,
                    font: None,
                    angle: 0.0,
                });
            }
        }
    }
    (pt(r.size.0).max(1.0), pt(r.size.1).max(1.0), items)
}
