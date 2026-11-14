//! Drawing to primitives in paper millimetres.

use strix_calc::Worksheet;

use crate::geom::{Pt, arc_points_tol, bulge_points_tol};
use crate::model::{Align, DimStyle, Drawing, Entity, Fit, Kind, Mark, Orient, TextLook};

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
        /// Its font, bold and italic (from its text style).
        look: TextLook,
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
    /// The free texts as drawn: each text entity's id and its text with its placeholders filled
    /// (what an editor measures to grip it at its real size, [`crate::TextWidths`]).
    pub texts: Vec<(String, String)>,
}

/// The largest gap between a drawn chord and its curve, paper mm.
const CHORD_MM: f64 = 0.01;

/// Most points one drawing draws, all its primitives together: a file with thousands of huge
/// arcs or fine hatches would otherwise take gigabytes. Past it curves keep a few chords each
/// (`MIN_THINNED` points) and hatches are left out.
pub const MAX_POINTS: usize = 2_000_000;
/// Most hatch and strip segments one drawing draws, all its hatches and strips together.
pub const MAX_HATCH_TOTAL: usize = 200_000;
/// Most boundary points all of a drawing's hatches and strips are worked out from.
pub(crate) const MAX_HATCH_WORK_POINTS: usize = 2_000_000;
/// The fewest points a curve is thinned to once the drawing's budget is spent.
const MIN_THINNED: usize = 16;

/// Most points the outputs written into the document carry, all paths together: the SVG
/// picture saved with the drawing and the page's items. Past it [`Rendered::lightened`] thins
/// every curve alike, so a drawing near [`MAX_POINTS`] saves a few megabytes, not tens.
pub const MAX_OUTPUT_POINTS: usize = 500_000;

/// `pts`, every so many dropped so that at most `left` remain (never fewer than `MIN_THINNED`);
/// the first and last points are kept.
fn thinned<T: Copy + PartialEq>(pts: &[T], left: usize) -> std::borrow::Cow<'_, [T]> {
    let keep = left.max(MIN_THINNED);
    if pts.len() <= keep {
        return std::borrow::Cow::Borrowed(pts);
    }
    let stride = pts.len().div_ceil(keep - 1).max(1);
    let mut out: Vec<T> = pts.iter().step_by(stride).copied().collect();
    if let Some(&last) = pts.last()
        && out.last() != Some(&last)
    {
        out.push(last);
    }
    std::borrow::Cow::Owned(out)
}

/// How close to 90° an angular dimension is drawn as a right angle, radians (a millionth of a
/// degree: exact and parametric drawings, not "nearly square" ones).
pub(crate) const RIGHT_ANGLE_TOL: f64 = 1e-6 * std::f64::consts::PI / 180.0;

impl Drawing {
    /// The drawing as primitives in paper mm, as printed (the page, the PDF): layers that are
    /// hidden or not printed are left out. Dimension labels read parameters from `ws`.
    pub fn render(&self, ws: Option<&Worksheet>) -> Rendered {
        self.render_for(ws, false, true)
    }

    /// Whether some shown layer is not printed: the editor's sheet then differs from the page.
    pub fn has_unprinted(&self) -> bool {
        self.layers.iter().any(|l| l.visible && !l.print)
    }

    /// As [`Drawing::render`]; with `screen`, the layers not printed are drawn too (the editor).
    pub fn render_layers(&self, ws: Option<&Worksheet>, screen: bool) -> Rendered {
        self.render_for(ws, screen, true)
    }

    /// As [`Drawing::render_layers`]; with `!show_answers` (the students' copy when `Show` hides
    /// solutions, or Student Preview in the CAD editor), entities and layers marked `answer` are
    /// omitted from the drawn primitives while still counting in the drawing's bounds, `origin`
    /// and `size` so pagination and base geometry never shift.
    pub fn render_for(&self, ws: Option<&Worksheet>, screen: bool, show_answers: bool) -> Rendered {
        // Annotative blocks drawn for another scale (the scale just set, or one being tried to
        // fit): drawn as they are at this one.
        if self.annotative_stale() {
            let mut d = self.clone();
            d.sync_annotative();
            return d.render_for(ws, screen, show_answers);
        }
        // Drawing units to paper mm, y up (flipped and shifted once the bounds are known).
        let k = self.unit.mm() / self.scale.max(1e-9);
        // Curves are drawn with chords at most this far off them: 0.01 mm on paper.
        let chord = Some(CHORD_MM / k);
        let mut prims = Vec::new();
        let mut keep_prims = Vec::new();
        // What the drawing may still draw, all its entities together (see `MAX_POINTS`).
        let mut points_left = MAX_POINTS;
        let mut hatch_left = MAX_HATCH_TOTAL;
        // The boundary points hatches and strips may still work on, drawn or not: an `along`
        // hatch, or one too fine for its budget, costs its boundary without drawing much.
        let mut work_left = MAX_HATCH_WORK_POINTS;
        let mut texts = Vec::new();
        let mut keep_texts = Vec::new();
        for e in &self.entities {
            let layer = self.layer(&e.layer);
            if !layer.visible || !(layer.print || screen) {
                continue;
            }
            let keep = show_answers || !(e.answer || layer.answer);
            let drawn = prims.len();
            let (color, width, dash) = self.stroke_of(e);
            let dash = dash.pattern().to_vec();
            // A curve's chords, thinned once the budget is spent (`thin`); a polyline's own
            // vertices are never dropped (at most `MAX_VERTICES`): only its arcs lose chords.
            let stroke = |pts: Vec<Pt>, closed: bool, thin: bool| Prim::Path {
                pts: if thin { thinned(&pts, points_left) } else { std::borrow::Cow::Borrowed(&pts[..]) }
                    .iter()
                    .map(|p| (p.x * k, p.y * k))
                    .collect(),
                closed,
                color,
                width,
                dash: dash.clone(),
                fill: false,
            };
            let path = |pts: Vec<Pt>, closed: bool| stroke(pts, closed, true);
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
                        // Past the drawing's budget the rest of the outline is its chords.
                        if out.len() < points_left {
                            out.extend(bulge_points_tol(a, b, bulges.get(i).copied().unwrap_or(0.0), chord));
                        } else {
                            out.push(b);
                        }
                    }
                    if *closed {
                        out.pop();
                    }
                    prims.push(stroke(out, *closed, false));
                }
                Kind::Point { p } => {
                    let p = p.pt();
                    prims.push(Prim::Dot { at: (p.x * k, p.y * k), r: (width * 1.5).max(0.3), color });
                }
                Kind::Text { p, value, height, angle, align } => {
                    let p = p.pt();
                    // Texts take placeholders too (`{{L:both}}`).
                    let text = crate::model::fill(value, ws);
                    texts.push((e.id.clone(), text.clone()));
                    prims.push(Prim::Text {
                        at: (p.x * k, p.y * k),
                        text,
                        height: *height,
                        angle: *angle,
                        anchor: match align {
                            Align::Left => Anchor::BaselineLeft,
                            Align::Center => Anchor::BaselineCenter,
                            Align::Right => Anchor::BaselineRight,
                        },
                        color,
                        look: self.text_look(e.style.as_deref()),
                    });
                }
                // Annotations are thin, their style's line width, unless they set their own.
                Kind::Dim { .. } => self.dimension(e, k, color, e.stroke.width.unwrap_or(self.dim_style_of(e).line_width), ws, &mut prims),
                Kind::Angle { .. } => self.angular(e, k, color, e.stroke.width.unwrap_or(self.dim_style_of(e).line_width), ws, &mut prims),
                Kind::Level { .. } => self.level(e, k, color, e.stroke.width.unwrap_or(self.dim_style_of(e).line_width), ws, &mut prims),
                // A hatch or a hatched strip: its lines, thin as hatches are; a solid fill's lines
                // close enough, and wide enough, to cover it (with its islands, on any output).
                Kind::Strip { pattern, .. } | Kind::Hatch { pattern, .. } => {
                    let paper = |p: Pt| (p.x * k, p.y * k);
                    let w = if *pattern == crate::model::Pattern::Solid { crate::hatch::SOLID_SPACING * 1.4 } else { width.min(0.18) };
                    // Past the drawing's budget hatches are left out (not even worked out).
                    let (lines, work) = match (hatch_left.min(points_left / 2).min(work_left), &e.kind) {
                        (0, _) => (Vec::new(), 0),
                        (_, Kind::Strip { .. }) => self.strip_lines_and_work(e),
                        _ => self.hatch_lines_and_work(e),
                    };
                    work_left = work_left.saturating_sub(work);
                    let take = hatch_left.min(points_left / 2);
                    hatch_left = hatch_left.saturating_sub(lines.len());
                    for (a, b) in lines.into_iter().take(take) {
                        prims.push(Prim::Path { pts: vec![paper(a), paper(b)], closed: false, color, width: w, dash: Vec::new(), fill: false });
                    }
                }
            }
            if e.arrows != crate::model::Ends::None
                && matches!(e.kind, Kind::Line { .. } | Kind::Arc { .. } | Kind::Polyline { closed: false, .. })
                && let Some(Prim::Path { pts, .. }) = prims.get(drawn)
            {
                let heads = self.arrowheads(pts, e.arrows, color, width);
                prims.extend(heads);
            }
            let spent: usize = prims.get(drawn..).unwrap_or_default().iter().map(Prim::point_count).sum();
            points_left = points_left.saturating_sub(spent);
            if !show_answers {
                keep_prims.resize(prims.len(), keep);
                keep_texts.resize(texts.len(), keep);
            }
        }
        let mut r = place(prims, (!show_answers).then_some(&keep_prims), self.padding);
        r.scale = k;
        r.texts = if show_answers {
            texts
        } else {
            texts.into_iter().enumerate().filter_map(|(i, t)| keep_texts.get(i).copied().unwrap_or(true).then_some(t)).collect()
        };
        r
    }

    /// The arrowheads `ends` asks for on an open path drawn through `pts` (paper mm, y up): each
    /// points out of the path along its last chord (or the first chord at least an arrowhead
    /// long, so a curve's arrow follows it rather than its last tiny chord).
    fn arrowheads(&self, pts: &[(f64, f64)], ends: crate::model::Ends, color: [u8; 3], width: f64) -> Vec<Prim> {
        let mut out = Vec::new();
        let size = self.dim_style.mark_size.max(0.5);
        let tip_dir = |walk: &mut dyn Iterator<Item = &(f64, f64)>| -> Option<(Pt, Pt)> {
            let tip = walk.next().map(|&(x, y)| Pt::new(x, y))?;
            let mut fallback = None;
            for &(x, y) in walk {
                let d = Pt::new(x, y).minus(tip);
                if d.len() >= size {
                    return Some((tip, d.unit()));
                }
                if d.len() > 1e-9 {
                    fallback = Some(d.unit());
                }
            }
            fallback.map(|d| (tip, d))
        };
        if ends.start()
            && let Some((tip, dir)) = tip_dir(&mut pts.iter())
        {
            self.mark(&self.dim_style, Mark::Arrow, tip, dir, color, width, &mut out);
        }
        if ends.end()
            && let Some((tip, dir)) = tip_dir(&mut pts.iter().rev())
        {
            self.mark(&self.dim_style, Mark::Arrow, tip, dir, color, width, &mut out);
        }
        out
    }

    /// The end mark `mark` at `at` on a dimension line going along `dir` away from it (paper mm),
    /// sized as style `st` says.
    #[allow(clippy::too_many_arguments)]
    fn mark(&self, st: &DimStyle, mark: Mark, at: Pt, dir: Pt, color: [u8; 3], width: f64, out: &mut Vec<Prim>) {
        let size = st.mark_size;
        // The style's mark width, else automatic: a tick heavier than the line, an arrow as thin.
        let own = (st.mark_width > 0.0).then_some(st.mark_width);
        let normal = dir.left();
        match mark {
            Mark::None => {}
            // Architectural tick: a 45° stroke across the end, heavier than the line.
            Mark::Tick => {
                let t = dir.plus(normal).unit().scale(size / 2.0);
                out.push(Prim::Path {
                    pts: vec![(at.x - t.x, at.y - t.y), (at.x + t.x, at.y + t.y)],
                    closed: false,
                    color,
                    width: own.unwrap_or_else(|| (width * 2.8).clamp(0.35, 1.0).max(width)),
                    dash: Vec::new(),
                    fill: false,
                });
            }
            // A closed filled arrowhead pointing at the end, 30° across its tip (ISO 129-1).
            Mark::Arrow => {
                let (back, side) = (dir.scale(size), normal.scale(size * (15f64).to_radians().tan()));
                let pts = [at, at.plus(back).plus(side), at.plus(back).minus(side)].iter().map(|p| (p.x, p.y)).collect();
                out.push(Prim::Path { pts, closed: true, color, width: own.unwrap_or(width), dash: Vec::new(), fill: true });
            }
            Mark::Dot => out.push(Prim::Dot { at: (at.x, at.y), r: size / 5.0, color }),
        }
    }

    /// A dimension's text above (`up`) or below its line at `mid`, along `angle`.
    #[allow(clippy::too_many_arguments)]
    fn dim_text(&self, st: &DimStyle, (mid, up): (Pt, Pt), angle: f64, label: String, below: bool, color: [u8; 3], out: &mut Vec<Prim>) {
        let (at, anchor) =
            if below { (mid.minus(up.scale(st.text_gap)), Anchor::TopCenter) } else { (mid.plus(up.scale(st.text_gap)), Anchor::BottomCenter) };
        out.push(Prim::Text { at: (at.x, at.y), text: label, height: st.text, angle, anchor, color, look: self.text_look(st.text_style.as_deref()) });
    }

    /// A linear dimension: extension lines, the dimension line with its end marks, and its texts.
    /// Where a linear dimension is drawn, in paper mm (y up, before the drawing is placed on its
    /// paper): its measured points, the feet of its extension lines on the dimension line, the
    /// measuring direction and the line's normal. The rendering and the picking share it.
    pub(crate) fn dim_frame(&self, e: &Entity, k: f64) -> Option<DimFrame> {
        let Kind::Dim { a, b, offset, offset_units, orient, .. } = &e.kind else { return None };
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
        // The dimension line passes its distance beyond the farther point along the normal: the
        // paper part plus the scaled part on paper.
        let offset = offset + offset_units * k;
        let (na, nb) = (a.x * normal.x + a.y * normal.y, b.x * normal.x + b.y * normal.y);
        let level = if offset >= 0.0 { na.max(nb) + offset } else { na.min(nb) + offset };
        let foot = |p: Pt| p.plus(normal.scale(level - (p.x * normal.x + p.y * normal.y)));
        Some(DimFrame { a, b, fa: foot(a), fb: foot(b), dir, normal, side: if offset >= 0.0 { 1.0 } else { -1.0 } })
    }

    fn dimension(&self, e: &Entity, k: f64, color: [u8; 3], width: f64, ws: Option<&Worksheet>, out: &mut Vec<Prim>) {
        let Kind::Dim { text, below, mark, mark_end, .. } = &e.kind else { return };
        let st = self.dim_style_of(e);
        let look = self.text_look(st.text_style.as_deref());
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
        // The end marks, each pointing at its end: the style's start and end marks, or the
        // dimension's own at both. Past a tick or a dot the line runs on beyond the extension line.
        let (m1, m2) = crate::model::dim_marks(st, *mark, *mark_end);
        let over = |m: Mark| if matches!(m, Mark::Tick | Mark::Dot) { st.line_over } else { 0.0 };
        out.push(line(vec![fa.minus(dir.scale(over(m1))), fb.plus(dir.scale(over(m2)))]));
        self.mark(st, m1, fa, dir, color, width, out);
        self.mark(st, m2, fb, dir.scale(-1.0), color, width, out);
        // Texts above and below the line's middle, kept readable (never upside down); moved by
        // the entity's text shift (in scale), with a leader back to the line when off it.
        let texts_from = out.len();
        let mid = fa.plus(fb).scale(0.5);
        let (angle, flipped) = readable(dir.y.atan2(dir.x).to_degrees());
        let up = if flipped { normal.scale(-1.0) } else { normal };
        let label = self.dim_label(st, text.as_deref(), measured, ws);
        if angle.abs() > 45.0 && !st.aligned {
            // With the style's aligned texts off, a steep dimension's text reads level, beside the
            // line on its outer side (by default it turns with the line, as below).
            let out_dir = normal.scale(side);
            let at = mid.plus(out_dir.scale(st.text_gap + st.ext_over));
            let anchor = if out_dir.x < 0.0 { Anchor::MiddleRight } else { Anchor::MiddleLeft };
            // A second text goes under the first, the pair centred on the line's middle.
            let second = below.as_deref().filter(|b| !b.trim().is_empty()).map(|b| self.dim_label(st, Some(b), measured, ws));
            let half = if second.is_some() { st.text * 0.7 } else { 0.0 };
            out.push(Prim::Text { at: (at.x, at.y + half), text: label, height: st.text, angle: 0.0, anchor, color, look: look.clone() });
            if let Some(b) = second {
                out.push(Prim::Text { at: (at.x, at.y - half), text: b, height: st.text, angle: 0.0, anchor, color, look });
            }
        } else {
            self.dim_text(st, (mid, up), angle, label, false, color, out);
            if let Some(b) = below.as_deref().filter(|b| !b.trim().is_empty()) {
                self.dim_text(st, (mid, up), angle, self.dim_label(st, Some(b), measured, ws), true, color, out);
            }
        }
        if let Some(leader) = self.text_home(e, k).and_then(|home| shift_texts(e, k, out, texts_from, home, mid, st.text, line)) {
            out.push(leader);
        }
    }

    /// Where a dimension's (first) text or a level mark's text is anchored without a text shift,
    /// paper mm (as the rendering places it, before the drawing is placed on its paper).
    pub(crate) fn text_home(&self, e: &Entity, k: f64) -> Option<Pt> {
        let st = self.dim_style_of(e);
        match &e.kind {
            Kind::Dim { .. } => {
                let DimFrame { fa, fb, dir, normal, side, .. } = self.dim_frame(e, k)?;
                let mid = fa.plus(fb).scale(0.5);
                let (angle, flipped) = readable(dir.y.atan2(dir.x).to_degrees());
                if angle.abs() > 45.0 && !st.aligned {
                    return Some(mid.plus(normal.scale(side).scale(st.text_gap + st.ext_over)));
                }
                let up = if flipped { normal.scale(-1.0) } else { normal };
                Some(mid.plus(up.scale(st.text_gap)))
            }
            Kind::Level { p, flip, left, .. } => {
                let tip = p.pt().scale(k);
                let (h, text_h) = (st.level_mark_height(), st.level_text_height());
                let half = h / 3f64.sqrt();
                let top = tip.y + if *flip { -h } else { h };
                let out_x = if *left { -1.0 } else { 1.0 };
                let at_y = if *flip { top - st.text_gap - text_h } else { top + st.text_gap };
                Some(Pt::new(tip.x + out_x * (half + st.text_gap), at_y)).filter(|p| p.is_finite())
            }
            _ => None,
        }
    }

    /// Where the text grip of a dimension or level mark is, drawing units: its text's anchor,
    /// moved by its text shift.
    pub fn text_grip(&self, e: &Entity) -> Option<Pt> {
        let k = crate::edit::paper_per_unit(self);
        let home = self.text_home(e, k)?.scale(1.0 / k);
        let [dx, dy] = e.text_shift.unwrap_or_default();
        Some(home.plus(Pt::new(dx, dy))).filter(|p| p.is_finite())
    }

    /// An angular dimension: an arc between the two rays with its end marks, extension lines
    /// from the rays' ends to the arc when it is beyond them, and the angle just outside the
    /// arc's middle. A zero angle draws nothing (not a full circle).
    fn angular(&self, e: &Entity, k: f64, color: [u8; 3], width: f64, ws: Option<&Worksheet>, out: &mut Vec<Prim>) {
        let Kind::Angle { c, a, b, r, r_paper, text, mark, mark_end } = &e.kind else { return };
        let st = self.dim_style_of(e);
        let look = self.text_look(st.text_style.as_deref());
        let (c, a, b) = (c.pt().scale(k), a.pt().scale(k), b.pt().scale(k));
        let (ua, ub) = (a.minus(c).unit(), b.minus(c).unit());
        // The scaled part of the radius on paper, plus its paper part.
        let r = r * k + r_paper;
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
        let (m1, m2) = crate::model::dim_marks(st, *mark, *mark_end);
        self.mark(st, m1, pa, ua.left(), color, width, out);
        self.mark(st, m2, pb, ub.left().scale(-1.0), color, width, out);
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
            out.push(Prim::Text { at: (at.x, at.y), text: label, height: st.text, angle, anchor, color, look });
            return;
        }
        out.push(Prim::Text { at: (at.x, at.y), text: label, height: st.text, angle: 0.0, anchor, color, look });
    }
}

impl Drawing {
    /// A level mark (paper mm, y up): a short line on the level through the tip, the triangle
    /// standing on its tip (hanging from it when flipped), a line along the triangle's top
    /// running out to the text side, and the elevation on that line (under it when flipped).
    fn level(&self, e: &Entity, k: f64, color: [u8; 3], width: f64, ws: Option<&Worksheet>, out: &mut Vec<Prim>) {
        let Kind::Level { p, datum, text, flip, left } = &e.kind else { return };
        let st = self.dim_style_of(e);
        let tip = p.pt().scale(k);
        if !tip.is_finite() {
            return;
        }
        let label = self.level_label(text.as_deref(), p.1.value - datum.value, st.level_unit, ws);
        // The level marks' own text height (else the dimension text's).
        let text_h = st.level_text_height();
        // The triangle: the style's height (automatic: a text and a fifth), equilateral.
        let h = st.level_mark_height();
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
        // 0.8 of the height per character, as the drawing's bounds estimate texts).
        let texts_from = out.len();
        let reach = half + st.text_gap + label.chars().filter(|c| !crate::is_marker(*c)).count() as f64 * text_h * 0.8;
        out.push(path(vec![(tip.x - out_x * half, top), (tip.x + out_x * reach, top)], false, false));
        let at_x = tip.x + out_x * (half + st.text_gap);
        let at_y = if *flip { top - st.text_gap - text_h } else { top + st.text_gap };
        let anchor = if *left { Anchor::BaselineRight } else { Anchor::BaselineLeft };
        out.push(Prim::Text {
            at: (at_x, at_y),
            text: label,
            height: text_h,
            angle: 0.0,
            anchor,
            color,
            look: self.text_look(st.text_style.as_deref()),
        });
        let line =
            |pts: Vec<Pt>| Prim::Path { pts: pts.iter().map(|p| (p.x, p.y)).collect(), closed: false, color, width, dash: Vec::new(), fill: false };
        let anchor = Pt::new(tip.x + out_x * half, top);
        if let Some(leader) = self.text_home(e, k).and_then(|home| shift_texts(e, k, out, texts_from, home, anchor, text_h, line)) {
            out.push(leader);
        }
    }
}

/// Moves the prims from `from` on (a dimension's or level mark's texts, and a level's line under
/// its text) by entity `e`'s text shift (drawing units, `k` paper mm each); a text moved more
/// than a text `height` from its `home` gets a leader from `anchor` (on the dimension line, the
/// level mark's top) to it.
#[allow(clippy::too_many_arguments)]
fn shift_texts(e: &Entity, k: f64, out: &mut [Prim], from: usize, home: Pt, anchor: Pt, height: f64, line: impl Fn(Vec<Pt>) -> Prim) -> Option<Prim> {
    let [dx, dy] = e.text_shift?;
    let (sx, sy) = (dx * k, dy * k);
    if !(sx.is_finite() && sy.is_finite()) || (sx == 0.0 && sy == 0.0) {
        return None;
    }
    for p in out.get_mut(from..).unwrap_or_default() {
        match p {
            Prim::Text { at, .. } => *at = (at.0 + sx, at.1 + sy),
            Prim::Path { pts, .. } => pts.iter_mut().for_each(|q| *q = (q.0 + sx, q.1 + sy)),
            _ => {}
        }
    }
    let to = home.plus(Pt::new(sx, sy));
    (to.minus(home).len() > height).then(|| line(vec![anchor, to]))
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

impl Rendered {
    /// The rendering as the outputs saved in the document carry it: itself, or past
    /// [`MAX_OUTPUT_POINTS`] every path of more than a few points thinned by the same share
    /// (its first and last points kept). Texts, dots and two-point strokes stay.
    pub fn lightened(&self) -> std::borrow::Cow<'_, Rendered> {
        let total: usize = self.prims.iter().map(Prim::point_count).sum();
        if total <= MAX_OUTPUT_POINTS {
            return std::borrow::Cow::Borrowed(self);
        }
        let mut light = self.clone();
        for prim in &mut light.prims {
            if let Prim::Path { pts, .. } = prim
                && pts.len() > MIN_THINNED
            {
                // This path's share of the output, as its share of the drawing.
                let keep = pts.len().saturating_mul(MAX_OUTPUT_POINTS) / total.max(1);
                if let std::borrow::Cow::Owned(fewer) = thinned(pts, keep) {
                    *pts = fewer;
                }
            }
        }
        std::borrow::Cow::Owned(light)
    }
}

impl Prim {
    /// How many points it draws (a text or a dot: one).
    pub(crate) fn point_count(&self) -> usize {
        match self {
            Prim::Path { pts, .. } => pts.len(),
            Prim::Dot { .. } | Prim::Text { .. } => 1,
        }
    }

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

/// The smallest and largest `n` of a scale `1:n` (1000:1 to 1:1 000 000), as a saved drawing
/// keeps them.
pub const SCALE_RANGE: (f64, f64) = (1e-3, 1e6);

/// A scale typed by hand: `1:75`, `1/75`, `75` (1:75), `2:1` (an enlargement), with spaces or a
/// decimal comma; the `n` of `1:n`, within [`SCALE_RANGE`]. None for anything else (zero,
/// negative, not a number).
pub fn parse_scale(text: &str) -> Option<f64> {
    let t: String = text.trim().chars().filter(|c| !c.is_whitespace()).take(64).collect::<String>().replace(',', ".");
    let num = |s: &str| s.parse::<f64>().ok().filter(|v| v.is_finite() && *v > 0.0);
    let n = match t.split_once([':', '/']) {
        Some((a, b)) => num(b)? / num(a)?,
        None => num(&t)?,
    };
    (n.is_finite() && n > 0.0).then(|| n.clamp(SCALE_RANGE.0, SCALE_RANGE.1))
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
/// When `keep` is given, bounds and `origin` are still computed across all `prims` so the
/// remaining primitives keep identical coordinates and paper size.
fn place(prims: Vec<Prim>, keep: Option<&[bool]>, pad: crate::model::Padding) -> Rendered {
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
    let mut prims: Vec<Prim> = match keep {
        None => prims,
        Some(mask) => prims.into_iter().enumerate().filter_map(|(i, p)| mask.get(i).copied().unwrap_or(true).then_some(p)).collect(),
    };
    if !x0.is_finite() {
        return Rendered { prims, size: (10.0, 10.0), origin: (0.0, 10.0), scale: 1.0, texts: Vec::new() };
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
    Rendered { prims, size, origin: (ox, oy), scale: 1.0, texts: Vec::new() }
}
