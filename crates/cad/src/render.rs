//! Drawing to primitives in paper millimetres.

use strix_calc::Worksheet;

use crate::geom::{Pt, arc_points, bulge_points};
use crate::model::{Drawing, Entity, Kind, Orient};

/// Where a text sits on its point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    /// The baseline's left end.
    BaselineLeft,
    /// The bottom centre (dimension text, just above its line).
    BottomCenter,
    /// The middle of its left or right side (a steep dimension's text, set level beside it).
    MiddleLeft,
    MiddleRight,
}

/// A primitive in paper mm, y down, origin at the top-left of [`Rendered::size`].
#[derive(Debug, Clone, PartialEq)]
pub enum Prim {
    /// A polyline.
    Path {
        pts: Vec<(f64, f64)>,
        closed: bool,
        color: [u8; 3],
        /// Paper mm.
        width: f64,
        /// On/off lengths in paper mm; empty for solid.
        dash: Vec<f64>,
    },
    /// A filled dot (a point entity).
    Dot { at: (f64, f64), r: f64, color: [u8; 3] },
    Text {
        at: (f64, f64),
        text: String,
        /// Paper mm.
        height: f64,
        /// Degrees counter-clockwise as seen on paper.
        angle: f64,
        anchor: Anchor,
        color: [u8; 3],
    },
}

/// A rendered drawing: its primitives and the paper size they fit in.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Rendered {
    pub prims: Vec<Prim>,
    /// Width and height in paper mm (at least 10 × 10).
    pub size: (f64, f64),
    /// Where the drawing sits on the paper: a point at drawing coordinates (x, y) is at paper
    /// (x·scale − origin.0, origin.1 − y·scale) mm.
    pub origin: (f64, f64),
    /// Paper mm per drawing unit (`unit_mm / N` at `1:N`).
    pub scale: f64,
}

/// Gap between a dimension's text and its line, and the ticks' half length, paper mm.
const DIM_GAP: f64 = 1.0;
const TICK: f64 = 1.25;
/// Extension lines start this far from the measured point and pass the dimension line by this.
const EXT_GAP: f64 = 1.0;
const EXT_OVER: f64 = 1.5;
/// Dimension text height, paper mm.
const DIM_TEXT: f64 = 2.5;

impl Drawing {
    /// The drawing as primitives in paper mm, as printed (the page, the PDF): layers that are
    /// hidden or not printed are left out. Dimension labels read parameters from `ws`.
    pub fn render(&self, ws: Option<&Worksheet>) -> Rendered {
        self.render_layers(ws, false)
    }

    /// Whether some shown layer is not printed: the editor's sheet then differs from the page.
    pub fn has_unprinted(&self) -> bool {
        self.layers.iter().any(|l| l.visible && !l.print)
    }

    /// As [`Drawing::render`]; with `screen`, the layers not printed are drawn too (the editor).
    pub fn render_layers(&self, ws: Option<&Worksheet>, screen: bool) -> Rendered {
        // Drawing units to paper mm, y up (flipped and shifted once the bounds are known).
        let k = self.unit.mm() / self.scale.max(1e-9);
        let mut prims = Vec::new();
        for e in &self.entities {
            let layer = self.layer(&e.layer);
            if !layer.visible || !(layer.print || screen) {
                continue;
            }
            let color = e.stroke.color.unwrap_or(layer.color);
            let width = e.stroke.width.unwrap_or(layer.width);
            let dash = e.stroke.dash.unwrap_or(layer.dash).pattern().to_vec();
            let path = |pts: Vec<Pt>, closed: bool| Prim::Path {
                pts: pts.iter().map(|p| (p.x * k, p.y * k)).collect(),
                closed,
                color,
                width,
                dash: dash.clone(),
            };
            match &e.kind {
                Kind::Line { a, b } => prims.push(path(vec![a.pt(), b.pt()], false)),
                Kind::Circle { c, r } => prims.push(path(arc_points(c.pt(), r.value.abs(), 0.0, 360.0), true)),
                Kind::Arc { c, r, start, end } => prims.push(path(arc_points(c.pt(), r.value.abs(), start.value, end.value), false)),
                Kind::Polyline { pts, bulges, closed } => {
                    let vs: Vec<Pt> = pts.iter().map(|p| p.pt()).collect();
                    let Some(&first) = vs.first() else { continue };
                    let mut out = vec![first];
                    let n = vs.len();
                    let segments = if *closed { n } else { n.saturating_sub(1) };
                    for i in 0..segments {
                        let (Some(&a), Some(&b)) = (vs.get(i), vs.get((i + 1) % n)) else { continue };
                        out.extend(bulge_points(a, b, bulges.get(i).copied().unwrap_or(0.0)));
                    }
                    if *closed {
                        out.pop();
                    }
                    prims.push(path(out, *closed));
                }
                Kind::Point { p } => {
                    let p = p.pt();
                    prims.push(Prim::Dot { at: (p.x * k, p.y * k), r: (width * 1.5).max(0.3), color });
                }
                Kind::Text { p, value, height, angle } => {
                    let p = p.pt();
                    prims.push(Prim::Text {
                        at: (p.x * k, p.y * k),
                        // Texts take placeholders too (`{{L:both}}`).
                        text: crate::model::fill(value, ws),
                        height: *height,
                        angle: *angle,
                        anchor: Anchor::BaselineLeft,
                        color,
                    });
                }
                Kind::Dim { .. } => self.dimension(e, k, color, width.min(0.25), ws, &mut prims),
            }
        }
        let mut r = place(prims, self.padding);
        r.scale = k;
        r
    }

    /// A linear dimension: extension lines, the dimension line with ticks, and its text.
    fn dimension(&self, e: &Entity, k: f64, color: [u8; 3], width: f64, ws: Option<&Worksheet>, out: &mut Vec<Prim>) {
        let Kind::Dim { a, b, offset, orient, text } = &e.kind else { return };
        let (a, b) = (a.pt().scale(k), b.pt().scale(k));
        // The measuring direction: along a→b, or the x or y axis.
        let dir = match orient {
            Orient::Aligned => b.minus(a).unit(),
            Orient::Horizontal => Pt::new(if b.x >= a.x { 1.0 } else { -1.0 }, 0.0),
            Orient::Vertical => Pt::new(0.0, if b.y >= a.y { 1.0 } else { -1.0 }),
        };
        if dir.len() == 0.0 {
            return;
        }
        let normal = dir.left();
        // The dimension line passes `offset` beyond the farther point along the normal.
        let (na, nb) = (a.x * normal.x + a.y * normal.y, b.x * normal.x + b.y * normal.y);
        let level = if *offset >= 0.0 { na.max(nb) + offset } else { na.min(nb) + offset };
        let foot = |p: Pt| p.plus(normal.scale(level - (p.x * normal.x + p.y * normal.y)));
        let (fa, fb) = (foot(a), foot(b));
        let measured = fb.minus(fa).len() / k;
        let line = |pts: Vec<Pt>| Prim::Path { pts: pts.iter().map(|p| (p.x, p.y)).collect(), closed: false, color, width, dash: Vec::new() };
        let side = if *offset >= 0.0 { 1.0 } else { -1.0 };
        for (p, f) in [(a, fa), (b, fb)] {
            let start = p.plus(normal.scale(EXT_GAP * side));
            if f.minus(p).len() > EXT_GAP {
                out.push(line(vec![start, f.plus(normal.scale(EXT_OVER * side))]));
            }
        }
        out.push(line(vec![fa, fb]));
        // Architectural ticks: 45° strokes across each end.
        let tick = dir.plus(normal).unit().scale(TICK);
        for f in [fa, fb] {
            out.push(Prim::Path {
                pts: vec![(f.x - tick.x, f.y - tick.y), (f.x + tick.x, f.y + tick.y)],
                closed: false,
                color,
                width: width * 2.0,
                dash: Vec::new(),
            });
        }
        // Text above the line's middle, kept readable (never upside down).
        let mid = fa.plus(fb).scale(0.5);
        let mut angle = dir.y.atan2(dir.x).to_degrees();
        let mut up = normal;
        if angle > 90.0 || angle <= -90.0 {
            angle -= 180.0f64.copysign(angle);
            up = up.scale(-1.0);
        }
        let label = self.label(text.as_deref(), measured, ws);
        if angle.abs() > 45.0 {
            // A steep dimension's text reads level, beside the line on its outer side.
            let out_dir = normal.scale(side);
            let at = mid.plus(out_dir.scale(DIM_GAP + EXT_OVER));
            let anchor = if out_dir.x < 0.0 { Anchor::MiddleRight } else { Anchor::MiddleLeft };
            out.push(Prim::Text { at: (at.x, at.y), text: label, height: DIM_TEXT, angle: 0.0, anchor, color });
        } else {
            let at = mid.plus(up.scale(DIM_GAP));
            out.push(Prim::Text { at: (at.x, at.y), text: label, height: DIM_TEXT, angle, anchor: Anchor::BottomCenter, color });
        }
    }
}

/// Flips the y-up primitives to paper (y down) and moves them inside the padding.
fn place(mut prims: Vec<Prim>, pad: crate::model::Padding) -> Rendered {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut grow = |x: f64, y: f64| {
        if x.is_finite() && y.is_finite() {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    };
    for p in &prims {
        match p {
            Prim::Path { pts, .. } => pts.iter().for_each(|&(x, y)| grow(x, y)),
            Prim::Dot { at, r, .. } => {
                grow(at.0 - r, at.1 - r);
                grow(at.0 + r, at.1 + r);
            }
            // A text's extent is estimated: 0.6 em per character along its angle, one em up.
            Prim::Text { at, text, height, angle, anchor, .. } => {
                let w = 0.8 * height * text.chars().filter(|&c| !crate::is_marker(c)).count() as f64;
                let (c, s) = (angle.to_radians().cos(), angle.to_radians().sin());
                let (x_start, y_start) = match anchor {
                    Anchor::BottomCenter => (-w / 2.0, 0.0),
                    Anchor::BaselineLeft => (0.0, 0.0),
                    Anchor::MiddleLeft => (0.0, -height / 2.0),
                    Anchor::MiddleRight => (-w, -height / 2.0),
                };
                for (dx, dy) in [(x_start, y_start), (x_start + w, y_start), (x_start, y_start + height), (x_start + w, y_start + height)] {
                    grow(at.0 + dx * c - dy * s, at.1 + dx * s + dy * c);
                }
            }
        }
    }
    if !x0.is_finite() {
        return Rendered { prims, size: (10.0, 10.0), origin: (0.0, 10.0), scale: 1.0 };
    }
    let (ox, oy) = (x0 - pad.left, y1 + pad.top);
    let map = |(x, y): (f64, f64)| (x - ox, oy - y);
    for p in &mut prims {
        match p {
            Prim::Path { pts, .. } => pts.iter_mut().for_each(|q| *q = map(*q)),
            Prim::Dot { at, .. } => *at = map(*at),
            Prim::Text { at, .. } => *at = map(*at),
        }
    }
    let size = ((x1 - x0 + pad.left + pad.right).clamp(1.0, 5000.0), (y1 - y0 + pad.top + pad.bottom).clamp(1.0, 5000.0));
    Rendered { prims, size, origin: (ox, oy), scale: 1.0 }
}
