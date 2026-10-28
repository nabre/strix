//! Drawing to primitives in paper millimetres.

use strix_calc::Worksheet;

use crate::geom::{Pt, arc_points_tol, bulge_points_tol};
use crate::model::{Align, Drawing, Entity, Fit, Kind, Mark, Orient};

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
    /// Every anchor is in the text's own frame: a turned text turns about it.
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

/// How close to 90° an angular dimension is drawn as a right angle, radians (a millionth of a
/// degree: exact and parametric drawings, not "nearly square" ones).
pub(crate) const RIGHT_ANGLE_TOL: f64 = 1e-6 * std::f64::consts::PI / 180.0;

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
                Kind::Level { .. } => self.level(e, k, color, width.min(0.25), ws, &mut prims),
                // A hatch or a hatched strip: its lines, thin as hatches are; a solid fill's lines
                // close enough, and wide enough, to cover it (with its islands, on any output).
                Kind::Strip { pattern, .. } | Kind::Hatch { pattern, .. } => {
                    let paper = |p: Pt| (p.x * k, p.y * k);
                    let w = if *pattern == crate::model::Pattern::Solid { crate::hatch::SOLID_SPACING * 1.4 } else { width.min(0.18) };
                    let lines = if matches!(e.kind, Kind::Strip { .. }) { self.strip_lines(e) } else { self.hatch_lines(e) };
                    for (a, b) in lines {
                        prims.push(Prim::Path { pts: vec![paper(a), paper(b)], closed: false, color, width: w, dash: Vec::new(), fill: false });
                    }
                }
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
    /// Where a linear dimension is drawn, in paper mm (y up, before the drawing is placed on its
    /// paper): its measured points, the feet of its extension lines on the dimension line, the
    /// measuring direction and the line's normal. The rendering and the picking share it.
    pub(crate) fn dim_frame(&self, e: &Entity, k: f64) -> Option<DimFrame> {
        let Kind::Dim { a, b, offset, orient, .. } = &e.kind else { return None };
        let (a, b) = (a.pt().scale(k), b.pt().scale(k));
        // The measuring direction: along a→b, or the x or y axis.
        let dir = match orient {
            Orient::Aligned => b.minus(a).unit(),
            Orient::Horizontal => Pt::new(if b.x >= a.x { 1.0 } else { -1.0 }, 0.0),
            Orient::Vertical => Pt::new(0.0, if b.y >= a.y { 1.0 } else { -1.0 }),
        };
        if dir.len() == 0.0 {
            return None;
        }
        let normal = dir.left();
        // The dimension line passes `offset` beyond the farther point along the normal.
        let (na, nb) = (a.x * normal.x + a.y * normal.y, b.x * normal.x + b.y * normal.y);
        let level = if *offset >= 0.0 { na.max(nb) + offset } else { na.min(nb) + offset };
        let foot = |p: Pt| p.plus(normal.scale(level - (p.x * normal.x + p.y * normal.y)));
        Some(DimFrame { a, b, fa: foot(a), fb: foot(b), dir, normal, side: if *offset >= 0.0 { 1.0 } else { -1.0 } })
    }

    fn dimension(&self, e: &Entity, k: f64, color: [u8; 3], width: f64, ws: Option<&Worksheet>, out: &mut Vec<Prim>) {
        let Kind::Dim { text, below, mark, .. } = &e.kind else { return };
        let st = self.dim_style;
        let Some(DimFrame { a, b, fa, fb, dir, normal, side }) = self.dim_frame(e, k) else { return };
        let measured = fb.minus(fa).len() / k;
        let line =
            |pts: Vec<Pt>| Prim::Path { pts: pts.iter().map(|p| (p.x, p.y)).collect(), closed: false, color, width, dash: Vec::new(), fill: false };
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
        let (angle, flipped) = readable(dir.y.atan2(dir.x).to_degrees());
        let up = if flipped { normal.scale(-1.0) } else { normal };
        let label = self.label(text.as_deref(), measured, ws);
        if angle.abs() > 45.0 && !st.aligned {
            // A steep dimension's text reads level, beside the line on its outer side (unless
            // the style aligns texts with their lines: then it turns with the line, as below).
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
        // A right angle with no text of its own: the square of side `r` on the vertex, a dot in
        // it, instead of the arc and "90°".
        let right = text.is_none() && (sweep - std::f64::consts::FRAC_PI_2).abs() <= RIGHT_ANGLE_TOL;
        if right {
            out.push(line(vec![c.plus(ua.scale(r)), c.plus(ua.plus(ub).scale(r)), c.plus(ub.scale(r))]));
            let mid = c.plus(ua.plus(ub).scale(r / 2.0));
            out.push(Prim::Dot { at: (mid.x, mid.y), r: (width * 1.5).max(0.25).min(r / 4.0), color });
        } else {
            out.push(line(arc_points_tol(c, r, start.to_degrees(), (start + sweep).to_degrees(), Some(CHORD_MM))));
        }
        // Extension lines along each ray, from past its end to just past the arc.
        for (u, end) in [(ua, a), (ub, b)] {
            let reach = end.minus(c).len();
            if r > reach + st.ext_gap {
                out.push(line(vec![c.plus(u.scale(reach + st.ext_gap)), c.plus(u.scale(r + st.ext_over))]));
            }
        }
        if right {
            return;
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
        if st.aligned {
            // Along the arc (its tangent at the middle), readable: turned half a turn when it
            // would read upside down, then hanging outward from its point instead of standing.
            let (angle, flipped) = readable((start + sweep / 2.0).to_degrees() - 90.0);
            let anchor = if flipped { Anchor::TopCenter } else { Anchor::BottomCenter };
            out.push(Prim::Text { at: (at.x, at.y), text: label, height: st.text, angle, anchor, color });
            return;
        }
        out.push(Prim::Text { at: (at.x, at.y), text: label, height: st.text, angle: 0.0, anchor, color });
    }
}

impl Drawing {
    /// A level mark (paper mm, y up): a short line on the level through the tip, the triangle
    /// standing on its tip (hanging from it when flipped), a line along the triangle's top
    /// running out to the text side, and the elevation on that line (under it when flipped).
    fn level(&self, e: &Entity, k: f64, color: [u8; 3], width: f64, ws: Option<&Worksheet>, out: &mut Vec<Prim>) {
        let Kind::Level { p, datum, text, flip, left } = &e.kind else { return };
        let st = self.dim_style;
        let tip = p.pt().scale(k);
        if !tip.is_finite() {
            return;
        }
        let label = self.level_label(text.as_deref(), p.1.value - datum.value, ws);
        // The triangle: as tall as a text and a half, equilateral.
        let h = (st.text * 1.2).max(1.0);
        let half = h / 3f64.sqrt();
        let up = if *flip { -1.0 } else { 1.0 };
        let out_x = if *left { -1.0 } else { 1.0 };
        let top = tip.y + up * h;
        let path = |pts: Vec<(f64, f64)>, closed: bool, fill: bool| Prim::Path { pts, closed, color, width, dash: Vec::new(), fill };
        out.push(path(vec![(tip.x - h, tip.y), (tip.x + h, tip.y)], false, false));
        out.push(path(vec![(tip.x, tip.y), (tip.x - half, top), (tip.x + half, top)], true, false));
        // The half of the triangle on the text side filled, as level marks are drawn.
        out.push(path(vec![(tip.x, tip.y), (tip.x + out_x * half, top), (tip.x, top)], true, true));
        // The line under the text: past the triangle by the text's length (an estimate: about
        // 0.6 of the height per character).
        let reach = half + st.text_gap + label.chars().filter(|c| !crate::is_marker(*c)).count() as f64 * st.text * 0.6;
        out.push(path(vec![(tip.x - out_x * half, top), (tip.x + out_x * reach, top)], false, false));
        let at_x = tip.x + out_x * (half + st.text_gap);
        let at_y = if *flip { top - st.text_gap - st.text } else { top + st.text_gap };
        let anchor = if *left { Anchor::BaselineRight } else { Anchor::BaselineLeft };
        out.push(Prim::Text { at: (at_x, at_y), text: label, height: st.text, angle: 0.0, anchor, color });
    }
}

/// A text direction `deg` (degrees counter-clockwise) turned to read as drafting reads it, never
/// upside down: within (−90°, 90°], half a turn round when it pointed left or down (a vertical
/// text reads from the right, bottom to top). With whether it was turned.
pub fn readable(deg: f64) -> (f64, bool) {
    if !deg.is_finite() {
        return (0.0, false);
    }
    let mut a = deg.rem_euclid(360.0);
    if a > 180.0 {
        a -= 360.0;
    }
    // Now in (−180°, 180°].
    if a > 90.0 + 1e-9 || a <= -90.0 + 1e-9 {
        let turned = if a > 0.0 { a - 180.0 } else { a + 180.0 };
        (turned, true)
    } else {
        (a, false)
    }
}

/// Most dashes one path is cut into (a longer pattern run is drawn solid past it).
pub const MAX_DASHES: usize = 20_000;

impl Prim {
    /// A path's drawn pieces, its dash pattern applied (paper mm, from its first point; a
    /// closed path's closing side included): the runs where the pen is down, each a polyline.
    /// A solid path is one piece, a dot or a text none. For outputs that cannot dash a stroke
    /// themselves (the page); the SVG writes the pattern as `stroke-dasharray`.
    pub fn pieces(&self) -> Vec<Vec<(f64, f64)>> {
        let Prim::Path { pts, closed, dash, .. } = self else { return Vec::new() };
        let mut pts = pts.clone();
        if *closed && let Some(&first) = pts.first() {
            pts.push(first);
        }
        dash_runs(&pts, dash)
    }
}

/// `pts` cut by the on/off `pattern` (paper mm) into the runs where the pen is down. An empty or
/// degenerate pattern (no finite positive total) draws the polyline whole; an odd count of
/// lengths repeats twice over, as SVG's `stroke-dasharray` does.
pub fn dash_runs(pts: &[(f64, f64)], pattern: &[f64]) -> Vec<Vec<(f64, f64)>> {
    let finite = |p: &(f64, f64)| p.0.is_finite() && p.1.is_finite();
    let pts: Vec<(f64, f64)> = pts.iter().copied().filter(finite).collect();
    if pts.len() < 2 {
        return Vec::new();
    }
    let pattern: Vec<f64> = pattern.iter().map(|v| if v.is_finite() { v.max(0.0) } else { 0.0 }).collect();
    let total: f64 = pattern.iter().sum();
    let pattern: Vec<f64> = if pattern.len() % 2 == 1 { pattern.iter().chain(pattern.iter()).copied().collect() } else { pattern };
    if pattern.is_empty() || total <= 1e-6 {
        return vec![pts];
    }
    let mut out: Vec<Vec<(f64, f64)>> = Vec::new();
    // Where the pattern is: which entry and how much of it is left.
    let (mut k, mut left) = (0usize, pattern.first().copied().unwrap_or(0.0));
    let mut run: Vec<(f64, f64)> = vec![pts.first().copied().unwrap_or((0.0, 0.0))];
    let mut cuts = 0usize;
    for w in pts.windows(2) {
        let (Some(&a), Some(&b)) = (w.first(), w.get(1)) else { continue };
        let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        let mut t = 0.0;
        while len - t > left && cuts < MAX_DASHES {
            t += left;
            let f = t / len;
            let p = (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f);
            if k % 2 == 0 {
                // The pen lifts: the run ends here.
                run.push(p);
                out.push(std::mem::take(&mut run));
                cuts += 1;
            } else {
                run = vec![p];
            }
            k = (k + 1) % pattern.len();
            left = pattern.get(k).copied().unwrap_or(0.0);
        }
        if cuts >= MAX_DASHES && left.is_finite() {
            // Past the cap the rest is drawn solid, from where the pattern stopped.
            if k % 2 == 1 {
                let f = if len > 0.0 { (t / len).min(1.0) } else { 0.0 };
                run = vec![(a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f)];
                k = 0;
            }
            left = f64::INFINITY;
        }
        left -= len - t;
        if k % 2 == 0 {
            run.push(b);
        }
    }
    if k % 2 == 0 && run.len() >= 2 {
        out.push(run);
    }
    out
}

/// A linear dimension laid out ([`Drawing::dim_frame`]), paper mm.
pub(crate) struct DimFrame {
    pub a: Pt,
    pub b: Pt,
    pub fa: Pt,
    pub fb: Pt,
    pub dir: Pt,
    pub normal: Pt,
    /// 1 when the line is on the normal's side, −1 on the other.
    pub side: f64,
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
    /// Whether the drawing, annotations and padding included, fits its maximum size at `1:n`
    /// (always, without one).
    pub fn fits_at(&self, n: f64, ws: Option<&Worksheet>) -> bool {
        let Some(fit) = self.fit else { return true };
        let mut d = self.clone();
        d.scale = n;
        let (w, h) = d.render(ws).size;
        (fit.width <= 0.0 || w <= fit.width + 1e-9) && (fit.height <= 0.0 || h <= fit.height + 1e-9)
    }

    /// The smallest standard denominator at which the drawing fits its maximum size (the
    /// largest when none does). Annotations are paper sizes and do not shrink with the scale, so
    /// each candidate is rendered; a larger denominator never makes the drawing larger, so a
    /// binary search over the series takes a handful of renderings.
    fn smallest_fitting(&self, ws: Option<&Worksheet>) -> Option<f64> {
        self.fit?;
        let scales = standard_scales();
        let (mut lo, mut hi) = (0usize, scales.len().saturating_sub(1));
        if !self.fits_at(*scales.get(hi)?, ws) {
            return scales.get(hi).copied();
        }
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.fits_at(*scales.get(mid)?, ws) {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        scales.get(lo).copied()
    }

    /// The standard scale at which the drawing fits its maximum size as large as it can, when
    /// the scale is fitted (None without a maximum size or with a scale picked by hand).
    pub fn fitted_scale(&self, ws: Option<&Worksheet>) -> Option<f64> {
        if self.fit.is_some_and(|f| f.fixed) {
            return None;
        }
        self.smallest_fitting(ws)
    }

    /// The standard scales the drawing can be drawn at: those at which it fits its maximum size
    /// (every one without a maximum size), ascending denominators.
    pub fn fitting_scales(&self, ws: Option<&Worksheet>) -> Vec<f64> {
        let all = standard_scales();
        match self.smallest_fitting(ws) {
            Some(first) => all.into_iter().filter(|n| *n >= first).collect(),
            None => all,
        }
    }

    /// Sets the scale from the maximum size when it is fitted (nothing changes otherwise).
    pub fn apply_fit(&mut self, ws: Option<&Worksheet>) {
        if let Some(n) = self.fitted_scale(ws) {
            self.scale = n;
        }
    }

    /// Sets the maximum size, mm (None: no limit). A drawing that had none takes the fitted
    /// scale; one that had keeps how its scale is chosen, unless the scale picked by hand no
    /// longer fits (then it is fitted).
    pub fn set_max_size(&mut self, size: Option<(f64, f64)>, ws: Option<&Worksheet>) {
        let fixed = self.fit.is_some_and(|f| f.fixed);
        self.fit = size.map(|(width, height)| Fit { width, height, fixed }.sanitized());
        if fixed
            && !self.fits_at(self.scale, ws)
            && let Some(f) = self.fit.as_mut()
        {
            f.fixed = false;
        }
        self.apply_fit(ws);
    }

    /// The scale fitted to the maximum size again (an error without one).
    pub fn fit_scale(&mut self, ws: Option<&Worksheet>) -> Result<(), String> {
        let Some(f) = self.fit.as_mut() else { return Err("Set a maximum size first".into()) };
        f.fixed = false;
        self.apply_fit(ws);
        Ok(())
    }

    /// A scale `1:n` picked by hand: with a maximum size, only one at which the drawing fits.
    pub fn set_fixed_scale(&mut self, n: f64, ws: Option<&Worksheet>) -> Result<(), String> {
        if !(n.is_finite() && n > 0.0) {
            return Err("scale: the n of 1:n, more than 0".into());
        }
        if let Some(f) = self.fit {
            if !self.fits_at(n, ws) {
                return Err(format!("At {} the drawing is larger than its maximum size", scale_text(n)));
            }
            self.fit = Some(Fit { fixed: true, ..f });
        }
        self.scale = n;
        Ok(())
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
