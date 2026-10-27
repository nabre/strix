//! Drawing to primitives in paper millimetres.

use strix_calc::Worksheet;

use crate::geom::{Pt, arc_points_tol, bulge_points_tol};
use crate::model::{Align, Drawing, Entity, Kind, Mark, Orient};

/// Where a text sits on its point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    /// The baseline's left end.
    BaselineLeft,
    /// The baseline's middle and right end (centred and right-aligned texts).
    BaselineCenter,
    BaselineRight,
    /// The bottom centre (dimension text, just above its line).
    BottomCenter,
    /// The top centre (a dimension's text below its line).
    TopCenter,
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
        /// Filled in its colour (a closed shape: an arrowhead).
        fill: bool,
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

/// The largest gap between a drawn chord and its curve, paper mm.
const CHORD_MM: f64 = 0.01;

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
        // Curves are drawn with chords at most this far off them: 0.01 mm on paper.
        let chord = Some(CHORD_MM / k);
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
                fill: false,
            };
            match &e.kind {
                Kind::Line { a, b } => prims.push(path(vec![a.pt(), b.pt()], false)),
                Kind::Circle { c, r } => prims.push(path(arc_points_tol(c.pt(), r.value.abs(), 0.0, 360.0, chord), true)),
                Kind::Arc { c, r, start, end } => prims.push(path(arc_points_tol(c.pt(), r.value.abs(), start.value, end.value, chord), false)),
                Kind::Polyline { pts, bulges, closed } => {
                    let vs: Vec<Pt> = pts.iter().map(|p| p.pt()).collect();
                    let Some(&first) = vs.first() else { continue };
                    let mut out = vec![first];
                    let n = vs.len();
                    let segments = crate::kernel::segments(n, *closed);
                    for i in 0..segments {
                        let (Some(&a), Some(&b)) = (vs.get(i), vs.get((i + 1) % n)) else { continue };
                        out.extend(bulge_points_tol(a, b, bulges.get(i).copied().unwrap_or(0.0), chord));
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
                Kind::Text { p, value, height, angle, align } => {
                    let p = p.pt();
                    prims.push(Prim::Text {
                        at: (p.x * k, p.y * k),
                        // Texts take placeholders too (`{{L:both}}`).
                        text: crate::model::fill(value, ws),
                        height: *height,
                        angle: *angle,
                        anchor: match align {
                            Align::Left => Anchor::BaselineLeft,
                            Align::Center => Anchor::BaselineCenter,
                            Align::Right => Anchor::BaselineRight,
                        },
                        color,
                    });
                }
                Kind::Dim { .. } => self.dimension(e, k, color, width.min(0.25), ws, &mut prims),
                Kind::Angle { .. } => self.angular(e, k, color, width.min(0.25), ws, &mut prims),
            }
        }
        let mut r = place(prims, self.padding);
        r.scale = k;
        r
    }

    /// The end mark `mark` at `at` on a dimension line going along `dir` away from it (paper mm).
    fn mark(&self, mark: Mark, at: Pt, dir: Pt, color: [u8; 3], width: f64, out: &mut Vec<Prim>) {
        let size = self.dim_style.mark_size;
        let normal = dir.left();
        match mark {
            Mark::None => {}
            // Architectural tick: a 45° stroke across the end.
            Mark::Tick => {
                let t = dir.plus(normal).unit().scale(size / 2.0);
                out.push(Prim::Path {
                    pts: vec![(at.x - t.x, at.y - t.y), (at.x + t.x, at.y + t.y)],
                    closed: false,
                    color,
                    width: width * 2.0,
                    dash: Vec::new(),
                    fill: false,
                });
            }
            // A closed arrowhead pointing at the end, a third as wide as it is long.
            Mark::Arrow => {
                let (back, side) = (dir.scale(size), normal.scale(size / 6.0));
                let pts = [at, at.plus(back).plus(side), at.plus(back).minus(side)].iter().map(|p| (p.x, p.y)).collect();
                out.push(Prim::Path { pts, closed: true, color, width, dash: Vec::new(), fill: true });
            }
            Mark::Dot => out.push(Prim::Dot { at: (at.x, at.y), r: size / 5.0, color }),
        }
    }

    /// A dimension's text above (`up`) or below its line at `mid`, along `angle`.
    fn dim_text(&self, mid: Pt, up: Pt, angle: f64, label: String, below: bool, color: [u8; 3], out: &mut Vec<Prim>) {
        let st = self.dim_style;
        let (at, anchor) =
            if below { (mid.minus(up.scale(st.text_gap)), Anchor::TopCenter) } else { (mid.plus(up.scale(st.text_gap)), Anchor::BottomCenter) };
        out.push(Prim::Text { at: (at.x, at.y), text: label, height: st.text, angle, anchor, color });
    }

    /// A linear dimension: extension lines, the dimension line with its end marks, and its texts.
    fn dimension(&self, e: &Entity, k: f64, color: [u8; 3], width: f64, ws: Option<&Worksheet>, out: &mut Vec<Prim>) {
        let Kind::Dim { a, b, offset, orient, text, below, mark } = &e.kind else { return };
        let st = self.dim_style;
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
        let line =
            |pts: Vec<Pt>| Prim::Path { pts: pts.iter().map(|p| (p.x, p.y)).collect(), closed: false, color, width, dash: Vec::new(), fill: false };
        let side = if *offset >= 0.0 { 1.0 } else { -1.0 };
        for (p, f) in [(a, fa), (b, fb)] {
            let start = p.plus(normal.scale(st.ext_gap * side));
            if f.minus(p).len() > st.ext_gap {
                out.push(line(vec![start, f.plus(normal.scale(st.ext_over * side))]));
            }
        }
        out.push(line(vec![fa, fb]));
        // The end marks, each pointing at its end.
        let mark = mark.unwrap_or(st.mark);
        self.mark(mark, fa, dir, color, width, out);
        self.mark(mark, fb, dir.scale(-1.0), color, width, out);
        // Texts above and below the line's middle, kept readable (never upside down).
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
            let at = mid.plus(out_dir.scale(st.text_gap + st.ext_over));
            let anchor = if out_dir.x < 0.0 { Anchor::MiddleRight } else { Anchor::MiddleLeft };
            out.push(Prim::Text { at: (at.x, at.y), text: label, height: st.text, angle: 0.0, anchor, color });
        } else {
            self.dim_text(mid, up, angle, label, false, color, out);
            if let Some(b) = below.as_deref().filter(|b| !b.trim().is_empty()) {
                self.dim_text(mid, up, angle, self.label(Some(b), measured, ws), true, color, out);
            }
        }
    }

    /// An angular dimension: an arc between the two rays with its end marks, extension lines
    /// from the rays' ends to the arc when it is beyond them, and the angle just outside the
    /// arc's middle. A zero angle draws nothing (not a full circle).
    fn angular(&self, e: &Entity, k: f64, color: [u8; 3], width: f64, ws: Option<&Worksheet>, out: &mut Vec<Prim>) {
        let Kind::Angle { c, a, b, r, text, mark } = &e.kind else { return };
        let st = self.dim_style;
        let (c, a, b) = (c.pt().scale(k), a.pt().scale(k), b.pt().scale(k));
        let (ua, ub) = (a.minus(c).unit(), b.minus(c).unit());
        let r = r * k;
        let (start, sweep) = (crate::kernel::direction(ua), crate::kernel::angle_ccw(ua, ub));
        if ua.len() == 0.0 || ub.len() == 0.0 || r <= 0.0 || sweep == 0.0 {
            return;
        }
        let line =
            |pts: Vec<Pt>| Prim::Path { pts: pts.iter().map(|p| (p.x, p.y)).collect(), closed: false, color, width, dash: Vec::new(), fill: false };
        out.push(line(arc_points_tol(c, r, start.to_degrees(), (start + sweep).to_degrees(), Some(CHORD_MM))));
        // Extension lines along each ray, from past its end to just past the arc.
        for (u, end) in [(ua, a), (ub, b)] {
            let reach = end.minus(c).len();
            if r > reach + st.ext_gap {
                out.push(line(vec![c.plus(u.scale(reach + st.ext_gap)), c.plus(u.scale(r + st.ext_over))]));
            }
        }
        // The marks point along the arc, into each end.
        let (pa, pb) = (c.plus(ua.scale(r)), c.plus(ub.scale(r)));
        let mark = mark.unwrap_or(st.mark);
        self.mark(mark, pa, ua.left(), color, width, out);
        self.mark(mark, pb, ub.left().scale(-1.0), color, width, out);
        // The label reads level just outside the arc's middle, anchored on the side facing it.
        let mid_dir = Pt::new((start + sweep / 2.0).cos(), (start + sweep / 2.0).sin());
        let at = c.plus(mid_dir.scale(r + st.text_gap));
        let anchor = if mid_dir.x.abs() >= mid_dir.y.abs() {
            if mid_dir.x >= 0.0 { Anchor::MiddleLeft } else { Anchor::MiddleRight }
        } else if mid_dir.y >= 0.0 {
            Anchor::BottomCenter
        } else {
            Anchor::TopCenter
        };
        // The label: degrees, `<>` as for a linear dimension.
        let label = self.label(text.as_deref().or(Some("<>°")), crate::kernel::deg(sweep), ws);
        out.push(Prim::Text { at: (at.x, at.y), text: label, height: st.text, angle: 0.0, anchor, color });
    }
}

/// The standard scale denominators (ISO 5455's 1, 2, 5 series with 2.5, as drawings use): from
/// 10:1 to 1:1 000 000, ascending.
pub fn standard_scales() -> Vec<f64> {
    (-1..=6).flat_map(|e| [1.0, 2.0, 2.5, 5.0].map(|m| m * 10f64.powi(e))).filter(|n| *n <= 1e6).collect()
}

/// A scale as drawings write it: `1:50`, or `2:1` for an enlargement.
pub fn scale_text(n: f64) -> String {
    let num = |v: f64| {
        let s = format!("{:.3}", v);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    };
    if n >= 1.0 || n <= 0.0 || !n.is_finite() { format!("1:{}", num(n)) } else { format!("{}:1", num(1.0 / n)) }
}

impl Drawing {
    /// The standard scale at which the drawing, annotations and padding included, fits its
    /// `fit` space as large as it can: the smallest denominator that fits (the largest when none
    /// does). Annotations are paper sizes and do not shrink with the scale, so each candidate is
    /// rendered; a larger denominator never makes the drawing larger, so a binary search over
    /// the series takes a handful of renderings.
    pub fn fitted_scale(&self, ws: Option<&Worksheet>) -> Option<f64> {
        let fit = self.fit?;
        let scales = standard_scales();
        let fits = |n: f64| {
            let mut d = self.clone();
            d.scale = n;
            let (w, h) = d.render(ws).size;
            (fit.width <= 0.0 || w <= fit.width + 1e-9) && (fit.height <= 0.0 || h <= fit.height + 1e-9)
        };
        let (mut lo, mut hi) = (0usize, scales.len().saturating_sub(1));
        if !fits(*scales.get(hi)?) {
            return scales.get(hi).copied();
        }
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if fits(*scales.get(mid)?) {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        scales.get(lo).copied()
    }

    /// Sets the scale from `fit` (nothing changes without it).
    pub fn apply_fit(&mut self, ws: Option<&Worksheet>) {
        if let Some(n) = self.fitted_scale(ws) {
            self.scale = n;
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
                    Anchor::BottomCenter | Anchor::BaselineCenter => (-w / 2.0, 0.0),
                    Anchor::TopCenter => (-w / 2.0, -height),
                    Anchor::BaselineLeft => (0.0, 0.0),
                    Anchor::BaselineRight => (-w, 0.0),
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
