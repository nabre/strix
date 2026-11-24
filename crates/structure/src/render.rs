//! Drawing: the structure, then its reactions and diagrams one under the other at the same
//! horizontal scale, as `strix-plot` primitives in paper millimetres (y down).

use std::f64::consts::FRAC_1_SQRT_2;

use strix_calc::{Options, Worksheet, format_number};
use strix_plot::{PlotAnchor, PlotPrim, PlotRendered};

use crate::diagram::{Action, NotableKind, notables};
use crate::model::{Diagram, Layout, Load, LoadAxes, Side, Structure, SupportKind, View};
use crate::solve::{Analysis, MemberResult, Status, analyse};

mod annotate;
mod arms;
mod conventions;
mod equilibrium;
mod section;
pub use annotate::{Chains, chains};

type P = (f64, f64);
type Rgb = [u8; 3];

const BLACK: Rgb = [0x1F, 0x1F, 0x1F];
const GREY: Rgb = [0x9A, 0x9A, 0x9A];
const LOAD: Rgb = [0xB0, 0x3A, 0x2E];
const REACTION: Rgb = [0x1B, 0x4F, 0x72];
const ERROR: Rgb = [0xC0, 0x1C, 0x1C];
/// N, V, M: deep blue, teal, plum (distinct in grey too: dark, middle, darker).
const COLOR_N: Rgb = [0x1B, 0x4F, 0x72];
const COLOR_V: Rgb = [0x11, 0x7A, 0x65];
const COLOR_M: Rgb = [0x7D, 0x3C, 0x98];
const TENSION: Rgb = [0xD9, 0x53, 0x2F];
const COMPRESSION: Rgb = [0x1B, 0x4F, 0x72];

const MEMBER_W: f64 = 0.6;
const AXIS_W: f64 = 0.35;
const TEXT: f64 = 2.3;
const ARROW: f64 = 11.0;
/// How far along a reaction's arrow from its near end its crossed-out positive way starts
/// (paper mm): the node's name stands by the node.
const CROSSED_FROM: f64 = 4.5;
const SPREAD: f64 = 8.0;
/// The room between a text and the line it names (paper mm).
const GAP: f64 = 1.0;
/// The room between a station's distance and the diagram's axis (paper mm).
const STATION_GAP: f64 = GAP + 2.6;
/// The room kept round the drawing, between panels and under a panel's title (paper mm).
const MARGIN: f64 = 1.0;
const PANEL_GAP: f64 = 5.0;
/// The row a diagram's title and sign convention take above it (paper mm).
const TITLE_ROW: f64 = 11.0;

/// A rendered structure: its drawing and its analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct StructureRendered {
    /// The drawing (`plot.errors` holds the expressions that failed and the analysis' problems).
    pub plot: PlotRendered,
    pub analysis: Analysis,
    /// The structure as drawn: its expressions evaluated.
    pub resolved: Structure,
}

/// World → paper.
#[derive(Clone, Copy)]
struct Frame {
    x0: f64,
    top: f64,
    xmin: f64,
    ymax: f64,
    k: f64,
}

impl Frame {
    fn p(&self, x: f64, y: f64) -> P {
        (self.x0 + (x - self.xmin) * self.k, self.top + (self.ymax - y) * self.k)
    }
}

/// A world direction on paper (y flipped).
fn pd(d: P) -> P {
    (d.0, -d.1)
}

fn add(a: P, b: P) -> P {
    (a.0 + b.0, a.1 + b.1)
}

fn mul(a: P, k: f64) -> P {
    (a.0 * k, a.1 * k)
}

fn unit(a: P) -> P {
    let l = a.0.hypot(a.1);
    if l > 0.0 { (a.0 / l, a.1 / l) } else { (1.0, 0.0) }
}

fn lighter(c: Rgb, t: f64) -> Rgb {
    let mix = |v: u8| (f64::from(v) + (255.0 - f64::from(v)) * t).round().clamp(0.0, 255.0) as u8;
    [mix(c[0]), mix(c[1]), mix(c[2])]
}

/// The largest distributed-load intensity on `st` (`0.0` when it has none).
fn max_spread(st: &Structure) -> f64 {
    st.loads
        .iter()
        .filter_map(|l| match l {
            Load::Distributed { q1, q2, .. } => Some(q1.value.abs().max(q2.as_ref().map_or(0.0, |q| q.value.abs()))),
            _ => None,
        })
        .fold(0.0_f64, f64::max)
}

#[derive(Default)]
struct Pen {
    prims: Vec<PlotPrim>,
}

impl Pen {
    fn line(&mut self, a: P, b: P, c: Rgb, w: f64) {
        self.prims.push(PlotPrim::Path { pts: vec![a, b], closed: false, stroke: Some((c, w)), fill: None, dash: Vec::new() });
    }

    fn dashed(&mut self, a: P, b: P, c: Rgb, w: f64) {
        self.prims.push(PlotPrim::Path { pts: vec![a, b], closed: false, stroke: Some((c, w)), fill: None, dash: vec![1.2, 0.8] });
    }

    fn poly(&mut self, pts: Vec<P>, c: Rgb, w: f64) {
        if pts.len() >= 2 {
            self.prims.push(PlotPrim::Path { pts, closed: false, stroke: Some((c, w)), fill: None, dash: Vec::new() });
        }
    }

    fn fill(&mut self, pts: Vec<P>, fill: Rgb, stroke: Option<(Rgb, f64)>) {
        if pts.len() >= 3 {
            self.prims.push(PlotPrim::Path { pts, closed: true, stroke, fill: Some(fill), dash: Vec::new() });
        }
    }

    fn dot(&mut self, at: P, r: f64, fill: Rgb, stroke: Option<(Rgb, f64)>) {
        self.prims.push(PlotPrim::Dot { at, r, fill, stroke });
    }

    /// Text centred on `at` (vertically too).
    fn label(&mut self, at: P, text: String, c: Rgb) {
        self.prims.push(PlotPrim::Text { at: (at.0, at.1 + TEXT * 0.5), text, height: TEXT, bold: false, anchor: PlotAnchor::Center, color: c });
    }

    /// Text beside `at` on the side `dir` (a paper direction) points to.
    fn beside(&mut self, at: P, dir: P, text: String, c: Rgb) {
        let (anchor, pos) = if dir.0.abs() >= 0.6 {
            if dir.0 > 0.0 { (PlotAnchor::Left, (at.0 + GAP, at.1 + TEXT * 0.4)) } else { (PlotAnchor::Right, (at.0 - GAP, at.1 + TEXT * 0.4)) }
        } else if dir.1 > 0.0 {
            (PlotAnchor::Center, (at.0, at.1 + TEXT * 0.85 + GAP))
        } else {
            (PlotAnchor::Center, (at.0, at.1 - GAP))
        };
        self.prims.push(PlotPrim::Text { at: pos, text, height: TEXT, bold: false, anchor, color: c });
    }

    fn text(&mut self, at: P, text: String, height: f64, bold: bool, anchor: PlotAnchor, c: Rgb) {
        self.prims.push(PlotPrim::Text { at, text, height, bold, anchor, color: c });
    }

    /// An arrow from `tail` to `tip` with a filled head.
    fn arrow(&mut self, tail: P, tip: P, c: Rgb, w: f64) {
        self.arrow_sized(tail, tip, c, w, 2.0, 0.36);
    }

    /// [`Pen::arrow`] with a head `head` mm long (at most 60 % of the arrow) and `half` of its
    /// length wide on each side.
    fn arrow_sized(&mut self, tail: P, tip: P, c: Rgb, w: f64, head: f64, half: f64) {
        let d = unit((tip.0 - tail.0, tip.1 - tail.1));
        let len = (tip.0 - tail.0).hypot(tip.1 - tail.1);
        let head = head.min(len * 0.6);
        let base = add(tip, mul(d, -head));
        self.line(tail, base, c, w);
        self.head(tip, d, head, half, c);
    }

    /// A filled arrowhead with its point at `tip`, pointing along the unit direction `d`, `head`
    /// mm long and `half` of its length wide on each side.
    fn head(&mut self, tip: P, d: P, head: f64, half: f64, c: Rgb) {
        let base = add(tip, mul(d, -head));
        let n = (-d.1, d.0);
        self.fill(vec![tip, add(base, mul(n, head * half)), add(base, mul(n, -head * half))], c, None);
    }

    /// A curved arrow round `c` (paper), counter-clockwise on the page when `ccw`.
    fn couple(&mut self, c: P, r: f64, ccw: bool, col: Rgb, w: f64) {
        self.couple_sized(c, r, ccw, col, w, (-120.0, 150.0), 3, (2.0, 0.36));
    }

    /// [`Pen::couple`] over the angles `span` (degrees, counter-clockwise from the right), its
    /// head taking the last `back` of the arc's 24 steps, `head` its length and half width.
    #[allow(clippy::too_many_arguments)]
    fn couple_sized(&mut self, c: P, r: f64, ccw: bool, col: Rgb, w: f64, span: (f64, f64), back: usize, head: (f64, f64)) {
        let (a0, a1) = (span.0.to_radians(), span.1.to_radians());
        let n = 24;
        let mut pts: Vec<P> = (0..=n)
            .map(|k| {
                let t = a0 + (a1 - a0) * f64::from(k) / f64::from(n);
                (c.0 + r * t.cos(), c.1 - r * t.sin())
            })
            .collect();
        if !ccw {
            pts.reverse();
        }
        let (Some(&tip), Some(&before)) = (pts.last(), pts.get(pts.len().saturating_sub(back + 1))) else { return };
        pts.truncate(pts.len().saturating_sub(back));
        self.poly(pts, col, w);
        self.arrow_sized(before, tip, col, w, head.0, head.1);
    }
}

struct Ctx<'a> {
    st: &'a Structure,
    an: &'a Analysis,
    opts: Options,
    pen: Pen,
    /// The largest diagram ordinate on paper (mm).
    amp_cap: f64,
    /// The boxes of the values written so far, `[x0, y0, x1, y1]`: later ones keep clear.
    placed: Vec<[f64; 4]>,
}

impl Ctx<'_> {
    fn num(&self, v: f64) -> String {
        format_number(v, &self.opts)
    }

    fn force(&self, v: f64) -> String {
        format!("{} {}", self.num(v), self.st.force_unit.symbol())
    }

    fn moment(&self, v: f64) -> String {
        format!("{} {}·{}", self.num(v), self.st.force_unit.symbol(), self.st.length_unit.symbol())
    }

    fn spread(&self, v: f64) -> String {
        format!("{} {}/{}", self.num(v), self.st.force_unit.symbol(), self.st.length_unit.symbol())
    }

    fn node_p(&self, f: &Frame, id: &str) -> Option<P> {
        self.st.node(id).map(|n| f.p(n.x.value, n.y.value))
    }

    /// The members as lines, hinges as circles.
    fn members(&mut self, f: &Frame, c: Rgb, w: f64, hinges: bool) {
        let st = self.st;
        for m in &st.members {
            let (Some(a), Some(b)) = (self.node_p(f, &m.a), self.node_p(f, &m.b)) else { continue };
            self.pen.line(a, b, c, w);
        }
        if !hinges {
            return;
        }
        for m in &st.members {
            let (Some(a), Some(b)) = (self.node_p(f, &m.a), self.node_p(f, &m.b)) else { continue };
            let d = unit((b.0 - a.0, b.1 - a.1));
            let node_hinged = |id: &str| st.node(id).is_some_and(|n| n.hinge);
            if m.hinge_a && !node_hinged(&m.a) {
                self.pen.dot(add(a, mul(d, 1.6)), 0.8, [255, 255, 255], Some((c, 0.3)));
            }
            if m.hinge_b && !node_hinged(&m.b) {
                self.pen.dot(add(b, mul(d, -1.6)), 0.8, [255, 255, 255], Some((c, 0.3)));
            }
        }
        for n in st.nodes.iter().filter(|n| n.hinge) {
            self.pen.dot(f.p(n.x.value, n.y.value), 0.9, [255, 255, 255], Some((c, 0.3)));
        }
    }

    fn node_names(&mut self, f: &Frame) {
        self.node_names_where(f, &|_| true);
    }

    /// [`Ctx::node_names`] of the nodes `at` picks.
    fn node_names_where(&mut self, f: &Frame, at: &dyn Fn(&str) -> bool) {
        self.node_names_styled(f, BLACK, true, at);
    }

    /// [`Ctx::node_names`] of the nodes `at` picks, in `color` and weight `bold`.
    fn node_names_styled(&mut self, f: &Frame, color: Rgb, bold: bool, at: &dyn Fn(&str) -> bool) {
        self.node_names_avoiding(f, color, bold, at, &|_| Vec::new());
    }

    /// [`Ctx::node_names_styled`], each name clear of the paper directions `taken` gives for its
    /// node too (the reactions' arrows).
    fn node_names_avoiding(&mut self, f: &Frame, color: Rgb, bold: bool, at: &dyn Fn(&str) -> bool, taken: &dyn Fn(&str) -> Vec<P>) {
        let st = self.st;
        for n in &st.nodes {
            if n.id.is_empty() || !at(&n.id) || !st.members.iter().any(|m| m.a == n.id || m.b == n.id) {
                continue;
            }
            let p = f.p(n.x.value, n.y.value);
            // Away from the node's members; off the side an arrow took only when it lies along it.
            let arrows = taken(&n.id);
            let mut away = self.free_side(&n.id);
            if arrows.iter().any(|a| a.0 * away.0 + a.1 * away.1 > 0.9) {
                away = self.free_side_avoiding(&n.id, &arrows);
            }
            let supported = st.supports.iter().any(|s| s.node == n.id);
            // Clear of a fixed end's couple, drawn round the node.
            let fixed = st.supports.iter().any(|s| s.node == n.id && matches!(s.kind, SupportKind::Fixed | SupportKind::Slider));
            let reach = match (fixed && !arrows.is_empty(), supported) {
                (true, _) => 8.5,
                (false, true) => 5.0,
                (false, false) => 3.6,
            };
            let at = add(p, mul(away, reach));
            self.pen.text((at.0, at.1 + 1.4), n.id.clone(), 2.8, bold, PlotAnchor::Center, color);
        }
    }

    /// The side of node `id` an arrow along paper axis `ap` stands on: the one clear of the
    /// members; between two clear ones the support's side (`below`), then below or left.
    fn arrow_side(&self, id: &str, ap: P, below: P) -> P {
        let st = self.st;
        let Some(n) = st.node(id) else { return ap };
        let dirs: Vec<P> = st
            .members
            .iter()
            .filter_map(|m| {
                let other = if m.a == id {
                    st.node(&m.b)
                } else if m.b == id {
                    st.node(&m.a)
                } else {
                    None
                }?;
                Some(unit(pd((other.x.value - n.x.value, other.y.value - n.y.value))))
            })
            .collect();
        let blocked = |s: P| dirs.iter().map(|d| d.0 * s.0 + d.1 * s.1).fold(-1.0_f64, f64::max);
        let (a, b) = (ap, mul(ap, -1.0));
        let (ba, bb) = (blocked(a), blocked(b));
        if (ba - bb).abs() > 0.3 {
            return if ba < bb { a } else { b };
        }
        let support = |s: P| s.0 * below.0 + s.1 * below.1;
        if (support(a) - support(b)).abs() > 0.3 {
            return if support(a) > support(b) { a } else { b };
        }
        let pref = |s: P| s.1 - s.0;
        if pref(a) >= pref(b) { a } else { b }
    }

    /// The paper direction at node `id` farthest from its members (and supports, below).
    fn free_side(&self, id: &str) -> P {
        self.free_side_avoiding(id, &[])
    }

    /// [`Ctx::free_side`] keeping clear of the paper directions `taken` too (arrows drawn there).
    fn free_side_avoiding(&self, id: &str, taken: &[P]) -> P {
        let st = self.st;
        let Some(n) = st.node(id) else { return (-FRAC_1_SQRT_2, -FRAC_1_SQRT_2) };
        let mut dirs: Vec<P> = Vec::new();
        for m in &st.members {
            let other = if m.a == id {
                st.node(&m.b)
            } else if m.b == id {
                st.node(&m.a)
            } else {
                None
            };
            if let Some(o) = other {
                dirs.push(unit(pd((o.x.value - n.x.value, o.y.value - n.y.value))));
            }
        }
        for s in st.supports.iter().filter(|s| s.node == id) {
            let phi = s.angle.value.to_radians();
            dirs.push((phi.sin(), phi.cos()));
            // A fixed end's plate runs along the ground both ways.
            if s.kind == SupportKind::Fixed {
                dirs.push((phi.cos(), -phi.sin()));
                dirs.push((-phi.cos(), phi.sin()));
            }
        }
        dirs.extend_from_slice(taken);
        let mut best = (-FRAC_1_SQRT_2, -FRAC_1_SQRT_2);
        let mut score = f64::MAX;
        for k in 0..16 {
            let t = std::f64::consts::TAU * f64::from(k) / 16.0;
            // Up-left first among equals.
            let cand = (-(t.cos()), -(t.sin()));
            let worst = dirs.iter().map(|d| d.0 * cand.0 + d.1 * cand.1).fold(-1.0_f64, f64::max);
            let s = worst - 0.01 * (-(cand.1) - cand.0 * 0.5);
            if s < score - 1e-9 {
                score = s;
                best = cand;
            }
        }
        best
    }

    fn supports(&mut self, f: &Frame, c: Rgb) {
        self.supports_where(f, c, &|_| true);
    }

    /// [`Ctx::supports`] at the nodes `at` picks.
    fn supports_where(&mut self, f: &Frame, c: Rgb, at: &dyn Fn(&str) -> bool) {
        let st = self.st;
        for s in st.supports.iter().filter(|s| at(&s.node)) {
            let Some(p) = self.node_p(f, &s.node) else { continue };
            let phi = s.angle.value.to_radians();
            let down = (phi.sin(), phi.cos());
            let along = (phi.cos(), -phi.sin());
            let q = |u: f64, w: f64| add(p, add(mul(along, u), mul(down, w)));
            let ground = |pen: &mut Pen, w: f64, half: f64| {
                pen.line(q(-half, w), q(half, w), c, 0.35);
                let mut u = -half + 0.6;
                while u <= half + 1e-9 {
                    pen.line(q(u, w), q(u - 1.1, w + 1.3), c, 0.2);
                    u += 1.4;
                }
            };
            match s.kind {
                SupportKind::Pin => {
                    self.pen.fill(vec![q(0.0, 0.0), q(-2.4, 3.8), q(2.4, 3.8)], [255, 255, 255], Some((c, 0.35)));
                    ground(&mut self.pen, 3.8, 3.8);
                }
                SupportKind::Roller => {
                    self.pen.fill(vec![q(0.0, 0.0), q(-2.4, 3.3), q(2.4, 3.3)], [255, 255, 255], Some((c, 0.35)));
                    self.pen.dot(q(-1.3, 4.0), 0.65, [255, 255, 255], Some((c, 0.3)));
                    self.pen.dot(q(1.3, 4.0), 0.65, [255, 255, 255], Some((c, 0.3)));
                    ground(&mut self.pen, 4.7, 3.8);
                }
                SupportKind::RollerCircle => {
                    self.pen.dot(q(0.0, 2.0), 2.0, [255, 255, 255], Some((c, 0.35)));
                    ground(&mut self.pen, 4.0, 3.8);
                }
                SupportKind::Link => {
                    self.pen.line(q(0.0, 0.0), q(0.0, 6.0), c, 0.45);
                    self.pen.dot(q(0.0, 6.0), 0.65, [255, 255, 255], Some((c, 0.3)));
                    self.pen.dot(q(0.0, 0.0), 0.65, [255, 255, 255], Some((c, 0.3)));
                    ground(&mut self.pen, 6.7, 2.8);
                }
                SupportKind::Slider => {
                    self.pen.line(q(-2.8, 0.0), q(2.8, 0.0), c, 0.55);
                    for u in [-1.8, 1.8] {
                        self.pen.line(q(u, 0.0), q(u, 5.0), c, 0.4);
                        self.pen.dot(q(u, 0.7), 0.55, [255, 255, 255], Some((c, 0.3)));
                        self.pen.dot(q(u, 5.0), 0.55, [255, 255, 255], Some((c, 0.3)));
                    }
                    ground(&mut self.pen, 5.6, 3.6);
                }
                SupportKind::Fixed => {
                    self.pen.line(q(-4.0, 0.0), q(4.0, 0.0), c, 0.6);
                    let mut u = -3.4;
                    while u <= 4.0 + 1e-9 {
                        self.pen.line(q(u, 0.0), q(u - 1.3, 1.6), c, 0.2);
                        u += 1.3;
                    }
                }
            }
        }
    }

    fn loads(&mut self, f: &Frame) {
        self.loads_as(f, LOAD, true, &|_, _| true);
    }

    /// The loads `which` picks, in colour `col`, with their names and values when `labelled`.
    fn loads_as(&mut self, f: &Frame, col: Rgb, labelled: bool, which: &dyn Fn(usize, &Load) -> bool) {
        self.loads_with_max(f, col, labelled, max_spread(self.st), which);
    }

    /// [`Ctx::loads_as`] scaled against `qmax` (so a distributed load split across a section cut
    /// keeps one scale on both sides).
    fn loads_with_max(&mut self, f: &Frame, col: Rgb, labelled: bool, qmax: f64, which: &dyn Fn(usize, &Load) -> bool) {
        self.loads_named(f, col, labelled, qmax, which, &crate::names::load_names(self.st));
    }

    /// [`Ctx::loads_with_max`] with the loads' `names` given (a section's loads in pieces take
    /// those of the loads they come from).
    fn loads_named(&mut self, f: &Frame, col: Rgb, labelled: bool, qmax: f64, which: &dyn Fn(usize, &Load) -> bool, names: &[Option<crate::names::LoadName>]) {
        let st = self.st;
        // `F₁ = 20 kN`: the load's name before its value.
        let named = |k: usize, end: u8, value: String| match names.get(k).copied().flatten() {
            Some(n) => format!("{} = {value}", n.pretty_end(end)),
            None => value,
        };
        for (k, l) in st.loads.iter().enumerate() {
            if !which(k, l) {
                continue;
            }
            match l {
                Load::Force { node, member, at, value, angle } => {
                    let Some(p) = self.point_of(f, node.as_deref(), member.as_deref(), at.value) else { continue };
                    if value.value == 0.0 {
                        continue;
                    }
                    let a = angle.value.to_radians();
                    let d = mul(pd((a.cos(), a.sin())), value.value.signum());
                    let tip = add(p, mul(d, -0.5));
                    let tail = add(p, mul(d, -ARROW));
                    self.pen.arrow(tail, tip, col, 0.35);
                    if labelled {
                        let label = named(k, 1, self.force(value.value.abs()));
                        self.pen.beside(add(tail, mul(d, -0.8)), mul(d, -1.0), label, col);
                    }
                }
                Load::Moment { node, member, at, value } => {
                    let Some(p) = self.point_of(f, node.as_deref(), member.as_deref(), at.value) else { continue };
                    if value.value == 0.0 {
                        continue;
                    }
                    self.pen.couple(p, 4.0, value.value > 0.0, col, 0.35);
                    if labelled {
                        let label = named(k, 1, self.moment(value.value.abs()));
                        self.pen.label(add(p, (5.5, -5.5)), label, col);
                    }
                }
                Load::Distributed { member, then, q1, q2, from, to, angle, axes, .. } => {
                    // Along its member, or the aligned members it goes on over.
                    let Some(line) = st.spread_line(member, then) else { continue };
                    let (len, (c, s)) = (line.len, line.dir);
                    let (x0, y0) = line.start;
                    let a0 = from.value.clamp(0.0, len);
                    let a1 = to.as_ref().map_or(len, |t| t.value).clamp(0.0, len);
                    let (a0, a1) = if a0 <= a1 { (a0, a1) } else { (a1, a0) };
                    let (q1, q2) = (q1.value, q2.as_ref().map_or(q1.value, |q| q.value));
                    if qmax <= 0.0 || a1 - a0 <= 0.0 {
                        continue;
                    }
                    let th = angle.value.to_radians();
                    let g = match axes {
                        LoadAxes::Global => (th.cos(), th.sin()),
                        LoadAxes::Local => (c * th.cos() - s * th.sin(), s * th.cos() + c * th.sin()),
                    };
                    let gp = pd(g);
                    let world = |x: f64| f.p(x0 + c * x, y0 + s * x);
                    let span_mm = (a1 - a0) * f.k;
                    let count = ((span_mm / 4.5).round() as usize).clamp(2, 60);
                    let q_at = |x: f64| q1 + (q2 - q1) * (x - a0) / (a1 - a0);
                    let mut tails = Vec::new();
                    for k in 0..=count {
                        let x = a0 + (a1 - a0) * k as f64 / count as f64;
                        let q = q_at(x);
                        let len = SPREAD * q.abs() / qmax;
                        let d = mul(gp, q.signum());
                        let tip = add(world(x), mul(d, -0.4));
                        // Tails on the side the load comes from: q's sign may flip in a stretch.
                        let tail = add(world(x), mul(gp, -SPREAD * q / qmax));
                        if len >= 1.2 {
                            self.pen.arrow(tail, tip, col, 0.25);
                        }
                        tails.push(tail);
                    }
                    self.pen.poly(tails.clone(), col, 0.3);
                    let (Some(&first), Some(&last)) = (tails.first(), tails.last()) else { continue };
                    if !labelled {
                        continue;
                    }
                    let out = mul(gp, -1.0);
                    if (q1 - q2).abs() <= 1e-12 * q1.abs().max(q2.abs()) {
                        let mid = tails.get(tails.len() / 2).copied().unwrap_or(first);
                        let label = named(k, 1, self.spread(q1.abs()));
                        self.pen.beside(add(mid, mul(out, 0.8)), out, label, col);
                    } else {
                        for (end, q, at) in [(1, q1, first), (2, q2, last)] {
                            if q != 0.0 {
                                let label = named(k, end, self.spread(q.abs()));
                                self.pen.beside(add(at, mul(out, 0.8)), out, label, col);
                            }
                        }
                    }
                }
            }
        }
    }

    fn point_of(&self, f: &Frame, node: Option<&str>, member: Option<&str>, at: f64) -> Option<P> {
        if let Some(id) = member {
            let m = self.st.member(id)?;
            let (na, nb) = (self.st.node(&m.a)?, self.st.node(&m.b)?);
            let len = self.st.length(m)?;
            let t = if len > 0.0 { (at / len).clamp(0.0, 1.0) } else { 0.0 };
            return Some(f.p(na.x.value + (nb.x.value - na.x.value) * t, na.y.value + (nb.y.value - na.y.value) * t));
        }
        self.node_p(f, node?)
    }

    /// The reactions with their values; returns, for each support's node, the paper directions
    /// its arrows and its couple's value took.
    fn reactions(&mut self, f: &Frame) -> Vec<(String, Vec<P>)> {
        let mut sides = Vec::new();
        let an = self.an;
        let pos = self.st.positive;
        let (sx, sy, sr) = (pos.sx(), pos.sy(), pos.sr());
        for (k, r) in an.reactions.iter().enumerate() {
            let Some(p) = self.node_p(f, &r.node) else { continue };
            // Named as the free body and the working name it (`H_A = 10 kN`).
            let s = equilibrium::node_sub(&r.node, k + 1);
            let phi = r.angle.to_radians();
            let below = (phi.sin(), phi.cos());
            let mut comps: Vec<(P, f64, &str)> = Vec::new();
            match r.kind {
                SupportKind::Fixed | SupportKind::Pin => {
                    // Each along the way it counts positive: crossed out when it came out negative.
                    comps.push(((sx, 0.0), r.fx * sx, "H"));
                    comps.push(((0.0, sy), r.fy * sy, "V"));
                }
                SupportKind::Roller | SupportKind::RollerCircle | SupportKind::Link | SupportKind::Slider => {
                    // Across the roller; along an axis, the way that axis counts positive.
                    let mut n = (-phi.sin(), phi.cos());
                    if (n.0.abs() < 1e-9 && n.1 * sy < 0.0) || (n.1.abs() < 1e-9 && n.0 * sx < 0.0) {
                        n = (-n.0, -n.1);
                    }
                    let letter = if n.0.abs() < 1e-9 {
                        "V"
                    } else if n.1.abs() < 1e-9 {
                        "H"
                    } else {
                        "R"
                    };
                    comps.push((n, r.fx * n.0 + r.fy * n.1, letter));
                }
            }
            let mut taken: Vec<P> = Vec::new();
            // The directions of the crossed-out arrows from the node: the couple's value keeps clear.
            let mut crossed: Vec<P> = Vec::new();
            for (axis, v, letter) in comps {
                if v == 0.0 {
                    continue;
                }
                let ap = pd(axis);
                // The side of the node the arrow stands on: the support's side for the axis
                // across it, else away from the members.
                let side = self.arrow_side(&r.node, ap, below);
                taken.push(side);
                let far = add(p, mul(side, ARROW + 2.0));
                let near = add(p, mul(side, 1.0));
                // The force on the paper.
                let fd = mul(ap, v.signum());
                let toward_node = fd.0 * side.0 + fd.1 * side.1 < 0.0;
                if toward_node {
                    self.pen.arrow(far, near, REACTION, 0.4);
                } else {
                    self.pen.arrow(near, far, REACTION, 0.4);
                }
                if v < 0.0 {
                    // Taken positive along the axis, it came out negative: that way crossed out
                    // beside it, on the side away from the structure; along the arrow's outer
                    // part only, clear of the node's name next to the node.
                    let across = self.outwards(&r.node, (-side.1, side.0));
                    let (a, b) = (add(near, mul(across, 4.0)), add(far, mul(across, 4.0)));
                    let a = add(a, mul(side, CROSSED_FROM));
                    if toward_node {
                        self.pen.crossed_arrow(a, b, REACTION)
                    } else {
                        self.pen.crossed_arrow(b, a, REACTION)
                    }
                    crossed.push(unit((((a.0 + b.0) / 2.0) - p.0, ((a.1 + b.1) / 2.0) - p.1)));
                }
                let label = format!(" = {}", self.force(v.abs()));
                self.pen.subscripted_then(add(far, mul(side, 0.8)), side, letter, &s, &label, REACTION);
            }
            if r.m != 0.0 {
                self.pen.couple(p, 5.0, r.m > 0.0, REACTION, 0.4);
                let mut reach = 6.5;
                if r.m * sr < 0.0 {
                    // Taken positive the way a rotation counts positive, it came out negative:
                    // crossed out round it.
                    self.pen.crossed_couple(p, 7.5, sr > 0.0, REACTION);
                    reach = 9.5;
                }
                let label = format!(" = {}", self.moment(r.m.abs()));
                // Clear of the node's name too, which takes the node's free side.
                let mut avoid = taken.clone();
                avoid.push(self.free_side(&r.node));
                avoid.extend_from_slice(&crossed);
                let free = self.free_side_avoiding(&r.node, &avoid);
                self.pen.subscripted_then(add(p, mul(free, reach)), free, "M", &s, &label, REACTION);
                taken.push(free);
            }
            sides.push((r.node.clone(), taken));
        }
        sides
    }

    /// One diagram of `action` on every member.
    fn diagram(&mut self, f: &Frame, action: Action, d: Diagram, color: Rgb) {
        let an = self.an;
        let (hatch, signs, values) = (self.st.diagrams.hatch, self.st.diagrams.signs, self.st.diagrams.values);
        let samples: Vec<Vec<Vec<(f64, f64)>>> = an.members.iter().map(|m| sample(m, action)).collect();
        let vmax = samples.iter().flatten().flatten().map(|(_, v)| v.abs()).fold(0.0_f64, f64::max);
        let side = if d.positive == Side::Up { 1.0 } else { -1.0 };
        // The curves are obstacles: no value or station is written over them.
        for (m, segs) in an.members.iter().zip(samples.iter()) {
            let dir = m.dir();
            let normal = pd((-dir.1, dir.0));
            for seg in segs {
                let pts: Vec<P> = seg
                    .iter()
                    .map(|&(x, v)| {
                        let o = if vmax > 0.0 { side * v / vmax * d.amplitude } else { 0.0 };
                        add(f.p(m.a.0 + dir.0 * x, m.a.1 + dir.1 * x), mul(normal, o))
                    })
                    .collect();
                // Every 0.8 mm along it, so a narrow text can't slip between two samples.
                for w in pts.windows(2) {
                    let (a, b) = (w[0], w[1]);
                    let n = ((b.0 - a.0).hypot(b.1 - a.1) / 0.8).ceil().clamp(1.0, 200.0) as usize;
                    for k in 0..=n {
                        let t = k as f64 / n as f64;
                        let c = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
                        self.placed.push([c.0 - 0.4, c.1 - 0.4, c.0 + 0.4, c.1 + 0.4]);
                    }
                }
            }
        }
        // Every stretch's ends on the paper (axis, curve): a stretch's end closed down to the
        // axis is a jump of the diagram unless another stretch (of this member or of one meeting
        // it there) goes on from the same point of the curve.
        let ends: Vec<(P, P)> = an
            .members
            .iter()
            .zip(samples.iter())
            .flat_map(|(m, segs)| {
                let dir = m.dir();
                let normal = pd((-dir.1, dir.0));
                segs.iter().flat_map(move |seg| [seg.first(), seg.last()]).flatten().map(move |&(x, v)| {
                    let o = if vmax > 0.0 { side * v / vmax * d.amplitude } else { 0.0 };
                    let base = f.p(m.a.0 + dir.0 * x, m.a.1 + dir.1 * x);
                    (base, add(base, mul(normal, o)))
                })
            })
            .collect();
        let near = |a: P, b: P| (a.0 - b.0).hypot(a.1 - b.1) <= 0.05;
        let jump = |base: P, top: P| ends.iter().filter(|(b, t)| near(*b, base) && near(*t, top)).count() < 2;
        // Each value with the places it may go, best first.
        let mut labels: Vec<(Vec<Place>, String, P)> = Vec::new();
        for (m, segs) in an.members.iter().zip(samples.iter()) {
            let dir = m.dir();
            let normal = pd((-dir.1, dir.0));
            let axis = |x: f64| f.p(m.a.0 + dir.0 * x, m.a.1 + dir.1 * x);
            let ord = |v: f64| if vmax > 0.0 { side * v / vmax * d.amplitude } else { 0.0 };
            let curve = |x: f64, v: f64| add(axis(x), mul(normal, ord(v)));
            for seg in segs {
                let (Some(&(x0, _)), Some(&(x1, _))) = (seg.first(), seg.last()) else { continue };
                if hatch {
                    let len_mm = (x1 - x0) * f.k;
                    let count = (len_mm / 1.4).floor() as usize;
                    for k in 1..count {
                        let x = x0 + (x1 - x0) * k as f64 / count as f64;
                        let v = interp(seg, x);
                        if ord(v).abs() > 0.3 {
                            self.pen.line(axis(x), curve(x, v), lighter(color, 0.45), 0.15);
                        }
                    }
                }
                // The curve bold; its ends closed down to the axis bold too where the diagram
                // jumps there (a concentrated load, a support, a member's end), as thin as the
                // hatching where it goes on (a stretch's bound only).
                let pts: Vec<P> = seg.iter().map(|&(x, v)| curve(x, v)).collect();
                for (x, v) in [seg.first(), seg.last()].into_iter().flatten() {
                    if ord(*v).abs() > 1e-9 {
                        let (base, top) = (axis(*x), curve(*x, *v));
                        if jump(base, top) {
                            self.pen.line(base, top, color, 0.4);
                        } else {
                            self.pen.line(base, top, lighter(color, 0.3), 0.15);
                        }
                    }
                }
                self.pen.poly(pts, color, 0.4);
                if signs {
                    self.signs(seg, &curve, &axis, &ord, normal, color, f.k);
                }
            }
            if self.st.diagrams.stations {
                self.stations(m, action, segs, &axis, &curve, &ord, normal, color);
            }
            if values {
                for (n, constant) in shown_values(m, action) {
                    if n.kind == NotableKind::Zero || n.value == 0.0 {
                        continue;
                    }
                    let o = ord(n.value);
                    let out = mul(normal, if o >= 0.0 { 1.0 } else { -1.0 });
                    // At a member's end, a little inside it: clear of the members meeting there.
                    let inset = 2.5 / f.k;
                    let x = if n.x <= 1e-9 {
                        n.x + inset.min(m.length * 0.25)
                    } else if n.x >= m.length - 1e-9 {
                        n.x - inset.min(m.length * 0.25)
                    } else {
                        n.x
                    };
                    let point = |x: f64, gap: f64| add(curve(x, interp_member(segs, x).unwrap_or(n.value)), mul(out, gap));
                    let text = self.num(n.value);
                    let at = point(x, GAP + 0.4);
                    // One label per place: a value shared by two members at a node, once.
                    if labels.iter().any(|(_, t, a)| *t == text && (a.0 - at.0).hypot(a.1 - at.1) < 8.0) {
                        continue;
                    }
                    let places = if constant {
                        // A stretch of constant value: written level, at its middle or sliding along it.
                        let mut places = vec![Place::Beside(at, out), Place::Beside(point(x, 3.8), out)];
                        if let Some(seg) = segs.iter().find(|s| s.first().is_some_and(|p| p.0 < x) && s.last().is_some_and(|p| p.0 > x)) {
                            let (Some(&(x0, _)), Some(&(x1, _))) = (seg.first(), seg.last()) else { continue };
                            for t in [0.3, 0.7, 0.15, 0.85] {
                                places.push(Place::Beside(point(x0 + (x1 - x0) * t, GAP + 0.4), out));
                            }
                        }
                        places
                    } else {
                        // A value at a point: written across the axis, standing on the curve there.
                        let half = text.chars().count() as f64 * TEXT * 0.55 / 2.0;
                        let angle = annotate::reading_angle(out);
                        // Then a little along the axis, either way (clear of an ordinate there).
                        let step = TEXT * 0.7 / f.k;
                        let mut places: Vec<Place> = [1.6, 4.0].iter().map(|g| Place::Turned(point(x, g + half), angle)).collect();
                        for dx in [step, -step, 2.0 * step, -2.0 * step] {
                            let xs = (x + dx).clamp(0.0, m.length);
                            places.push(Place::Turned(point(xs, 1.6 + half), angle));
                        }
                        places
                    };
                    labels.push((places, text, at));
                }
            }
        }
        for (places, text, _) in labels {
            let boxes: Vec<[f64; 4]> = places.iter().map(|p| p.bounds(&text)).collect();
            let k = self.clearest(&boxes);
            if let (Some(place), Some(b)) = (places.get(k), boxes.get(k)) {
                self.placed.push(*b);
                match *place {
                    Place::Beside(at, dir) => self.pen.beside(at, dir, text, color),
                    Place::Turned(at, angle) => self.pen.turned(at, text, TEXT, angle, color),
                }
            }
        }
    }

    /// The stations of a member's diagram: where its notable values are (a load's position, a
    /// maximum, a zero), each a tick on the axis with its distance from the member's start
    /// written across the axis on the side away from the diagram, and a dotted ordinate.
    #[allow(clippy::too_many_arguments)]
    fn stations(
        &mut self,
        m: &MemberResult,
        action: Action,
        segs: &[Vec<(f64, f64)>],
        axis: &dyn Fn(f64) -> P,
        curve: &dyn Fn(f64, f64) -> P,
        ord: &dyn Fn(f64) -> f64,
        normal: P,
        color: Rgb,
    ) {
        let lt = 1e-6 * (1.0 + m.length);
        let mut xs: Vec<f64> = notables(m, action).iter().map(|n| n.x).filter(|x| *x > lt && *x < m.length - lt).collect();
        xs.sort_by(f64::total_cmp);
        xs.dedup_by(|a, b| (*a - *b).abs() <= lt);
        // A diagram that is nought all along has nothing to locate.
        if segs.iter().flatten().all(|&(_, v)| ord(v).abs() <= 1e-9) {
            return;
        }
        let lu = self.st.length_unit.symbol();
        let height = TEXT * 0.9;
        for x in xs {
            let (vl, vr) = sides(segs, x);
            let (ol, or) = (ord(vl), ord(vr));
            let (base, top) = (axis(x), curve(x, vl));
            if ol.abs() > 0.8 {
                self.pen.dashed(base, top, lighter(color, 0.2), 0.15);
            }
            self.pen.line(add(base, mul(normal, -0.8)), add(base, mul(normal, 0.8)), color, 0.25);
            let text = format!("{} {lu}", self.num(x));
            let half = text.chars().count() as f64 * height * 0.55 / 2.0;
            // Away from the diagram (both sides of the tick counted), below the axis at a zero.
            let away = mul(normal, if ol + or > 0.0 { -1.0 } else { 1.0 });
            let angle = annotate::reading_angle(away);
            let along = (away.1, -away.0);
            let side_step = height * 0.75 + 0.3;
            let centre = add(base, mul(away, STATION_GAP + half));
            let opposite = add(base, mul(away, -(STATION_GAP + half)));
            // Across the axis on the tick; else on the tick across the other side where the
            // diagram is thin there (a zero, a station close to another); else beside the tick.
            let thin = ol.abs().max(or.abs()) < 1.5;
            let mut places = vec![centre];
            if thin {
                places.push(opposite);
            }
            places.extend([add(centre, mul(along, side_step)), add(centre, mul(along, -side_step))]);
            let boxes: Vec<[f64; 4]> = places.iter().map(|p| annotate::turned_box(*p, &text, height, angle)).collect();
            let (k, hits) = self.clearest_hits(&boxes);
            // Nowhere clear: the tick only, rather than one text over another.
            if hits > 0 {
                continue;
            }
            let (Some(&at), Some(&b)) = (places.get(k), boxes.get(k)) else { continue };
            self.placed.push(b);
            self.pen.turned(at, text, height, angle, GREY);
        }
    }

    /// ⊕/⊖ in each stretch of one sign.
    #[allow(clippy::too_many_arguments)]
    fn signs(
        &mut self,
        seg: &[(f64, f64)],
        curve: &dyn Fn(f64, f64) -> P,
        axis: &dyn Fn(f64) -> P,
        ord: &dyn Fn(f64) -> f64,
        normal: P,
        color: Rgb,
        k: f64,
    ) {
        let mut start = 0;
        let n = seg.len();
        let mut k_i = 0;
        while k_i <= n {
            let end_run = k_i == n || seg.get(k_i).zip(seg.get(start)).is_some_and(|(a, b)| a.1.signum() != b.1.signum() && a.1 != 0.0 && b.1 != 0.0);
            if end_run {
                let run = seg.get(start..k_i).unwrap_or(&[]);
                if let (Some(first), Some(last)) = (run.first(), run.last())
                    && (last.0 - first.0) * k >= 4.0
                    && let Some(&(xm, _)) = run.iter().max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
                    && let x = xm.clamp(first.0 + 0.25 * (last.0 - first.0), last.0 - 0.25 * (last.0 - first.0))
                    && let v = interp(run, x)
                    && v != 0.0
                {
                    let o = ord(v);
                    let at = if o.abs() >= 4.0 { add(axis(x), mul(normal, o * 0.5)) } else { add(curve(x, v), mul(normal, o.signum() * 2.6)) };
                    self.pen.dot(at, 1.25, [255, 255, 255], Some((color, 0.25)));
                    self.placed.push([at.0 - 1.5, at.1 - 1.5, at.0 + 1.5, at.1 + 1.5]);
                    self.pen.line(add(at, (-0.75, 0.0)), add(at, (0.75, 0.0)), color, 0.25);
                    if v > 0.0 {
                        self.pen.line(add(at, (0.0, -0.75)), add(at, (0.0, 0.75)), color, 0.25);
                    }
                }
                start = k_i;
            }
            k_i += 1;
        }
    }

    /// The first of `boxes` clear of what is written, else the one over the fewest things.
    fn clearest(&self, boxes: &[[f64; 4]]) -> usize {
        self.clearest_hits(boxes).0
    }

    /// [`Self::clearest`] with how many written things its box is over.
    fn clearest_hits(&self, boxes: &[[f64; 4]]) -> (usize, usize) {
        let mut best = (0, usize::MAX);
        for (k, b) in boxes.iter().enumerate() {
            let hits = self.placed.iter().filter(|o| overlaps(b, o)).count();
            if hits == 0 {
                return (k, 0);
            }
            if hits < best.1 {
                best = (k, hits);
            }
        }
        best
    }

    /// Truss: every member in its colour with its axial force.
    fn truss(&mut self, f: &Frame) {
        let an = self.an;
        let nmax = an.members.iter().map(|m| m.forces(0.0, true).n.abs()).fold(0.0_f64, f64::max);
        for m in &an.members {
            let n = m.forces(m.length * 0.5, true).n;
            let (a, b) = (f.p(m.a.0, m.a.1), f.p(m.b.0, m.b.1));
            let zero = n.abs() <= 1e-9 * nmax.max(1e-12);
            if zero {
                self.pen.dashed(a, b, GREY, 0.5);
            } else {
                self.pen.line(a, b, if n > 0.0 { TENSION } else { COMPRESSION }, 0.9);
            }
            let dir = m.dir();
            let normal = pd((-dir.1, dir.0));
            let mid = add(mul(add(a, b), 0.5), mul(normal, 1.2));
            let text = if zero { "0".to_string() } else { format!("{}{}", if n > 0.0 { "+" } else { "" }, self.num(n)) };
            self.pen.beside(
                mid,
                normal,
                text,
                if zero {
                    GREY
                } else if n > 0.0 {
                    TENSION
                } else {
                    COMPRESSION
                },
            );
        }
    }
}

/// The values written on a diagram: the notable ones, a stretch of constant value once at its
/// middle instead of at both its ends.
fn shown_values(m: &MemberResult, action: Action) -> Vec<(crate::diagram::Notable, bool)> {
    let bps = m.breakpoints_of(action);
    let val = |x: f64, after: bool| action.of(m.forces(x, after));
    let mut constant: Vec<(f64, f64, f64)> = Vec::new();
    for w in bps.windows(2) {
        let (x0, x1) = (w[0], w[1]);
        let (a, b, c) = (val(x0, true), val(x1, false), val(0.5 * (x0 + x1), true));
        let tol = 1e-9 * a.abs().max(b.abs()).max(1e-12);
        if (a - b).abs() <= tol && (a - c).abs() <= tol {
            constant.push((x0, x1, a));
        }
    }
    let lt = 1e-9 * (1.0 + m.length);
    let mut out: Vec<(crate::diagram::Notable, bool)> = notables(m, action)
        .into_iter()
        .filter(|n| {
            n.kind != NotableKind::End
                || !constant
                    .iter()
                    .any(|&(x0, x1, v)| ((n.x - x0).abs() <= lt || (n.x - x1).abs() <= lt) && (n.value - v).abs() <= 1e-9 * v.abs().max(1e-12))
        })
        .map(|n| (n, false))
        .collect();
    out.extend(constant.iter().map(|&(x0, x1, v)| (crate::diagram::Notable { x: 0.5 * (x0 + x1), value: v, kind: NotableKind::End }, true)));
    out.sort_by(|a, b| a.0.x.total_cmp(&b.0.x));
    out
}

/// The diagram's points along a member, a list per stretch (the ends of a stretch take the
/// values just inside it, so jumps show).
fn sample(m: &MemberResult, action: Action) -> Vec<Vec<(f64, f64)>> {
    let bps = m.breakpoints_of(action);
    let mut out = Vec::new();
    for w in bps.windows(2) {
        let (x0, x1) = (w[0], w[1]);
        if x1 - x0 <= 1e-12 * (1.0 + m.length) {
            continue;
        }
        let n = 24;
        let pts: Vec<(f64, f64)> = (0..=n)
            .map(|k| {
                let x = x0 + (x1 - x0) * f64::from(k) / f64::from(n);
                (x, action.of(m.forces(x, k == 0)))
            })
            .collect();
        out.push(pts);
    }
    out
}

/// The box `Pen::beside` writes `text` in (an estimate: glyphs ~0.55 of the height wide).
fn text_box(at: P, dir: P, text: &str) -> [f64; 4] {
    let w = text.chars().count() as f64 * TEXT * 0.55;
    let h = TEXT * 1.1;
    if dir.0.abs() >= 0.6 {
        let y0 = at.1 - h * 0.5;
        if dir.0 > 0.0 { [at.0, y0, at.0 + w, y0 + h] } else { [at.0 - w, y0, at.0, y0 + h] }
    } else if dir.1 > 0.0 {
        [at.0 - w / 2.0, at.1, at.0 + w / 2.0, at.1 + h]
    } else {
        [at.0 - w / 2.0, at.1 - h, at.0 + w / 2.0, at.1]
    }
}

fn overlaps(a: &[f64; 4], b: &[f64; 4]) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}

/// The sampled values just before and just after `x` along a member (they differ at a jump).
fn sides(segs: &[Vec<(f64, f64)>], x: f64) -> (f64, f64) {
    let near = |a: f64| (a - x).abs() <= 1e-9 * (1.0 + x.abs());
    let left = segs.iter().find(|s| s.last().is_some_and(|p| near(p.0))).and_then(|s| s.last()).map(|p| p.1);
    let right = segs.iter().find(|s| s.first().is_some_and(|p| near(p.0))).and_then(|s| s.first()).map(|p| p.1);
    let inside = interp_member(segs, x).unwrap_or(0.0);
    (left.unwrap_or(inside), right.unwrap_or(inside))
}

/// The sampled value at `x` along a member, from the stretch holding it.
fn interp_member(segs: &[Vec<(f64, f64)>], x: f64) -> Option<f64> {
    segs.iter().find(|s| s.first().is_some_and(|p| p.0 <= x) && s.last().is_some_and(|p| p.0 >= x)).map(|s| interp(s, x))
}

fn interp(seg: &[(f64, f64)], x: f64) -> f64 {
    for w in seg.windows(2) {
        let ((x0, v0), (x1, v1)) = (w[0], w[1]);
        if x >= x0 && x <= x1 {
            return if x1 > x0 { v0 + (v1 - v0) * (x - x0) / (x1 - x0) } else { v0 };
        }
    }
    seg.last().map_or(0.0, |p| p.1)
}

impl Structure {
    /// The structure alone — members, hinges, supports, loads, node names and, when
    /// [`dimensions`](crate::Diagrams::dimensions) is on, its dimensions, slopes and angles — for an editor:
    /// `k` paper mm per unit, the world point `origin` at paper (0, 0), y down. Its numbers as
    /// they are (resolve them first).
    pub fn schema_prims(&self, k: f64, origin: (f64, f64), opts: Options) -> Vec<PlotPrim> {
        let an = Analysis::failed(Status::Empty, Vec::new());
        let k = if k.is_finite() && k > 0.0 { k } else { 1.0 };
        let f = Frame { x0: 0.0, top: 0.0, xmin: origin.0, ymax: origin.1, k };
        let mut cx = Ctx { st: self, an: &an, opts, pen: Pen::default(), amp_cap: f64::MAX, placed: Vec::new() };
        cx.members(&f, BLACK, MEMBER_W, true);
        cx.supports(&f, BLACK);
        cx.loads(&f);
        cx.force_angles(&f);
        cx.slopes(&f);
        cx.dimensions(&f);
        cx.node_names(&f);
        cx.pen.prims
    }

    /// Renders the teacher's copy (reactions and diagrams shown).
    pub fn render(&self, ws: Option<&Worksheet>) -> StructureRendered {
        self.render_for(ws, true)
    }

    /// Renders the structure with its reactions and diagrams; with `show_answers` false and the
    /// structure an [`answer`](Structure::answer), the student's copy: the same panels, empty.
    pub fn render_for(&self, ws: Option<&Worksheet>, show_answers: bool) -> StructureRendered {
        let mut st = self.clone().sanitized();
        let mut errors: Vec<String> = Vec::new();
        if let Some(ws) = ws {
            for (expr, why) in st.resolve(ws) {
                errors.push(format!("{expr}: {why}"));
            }
        }
        // However its members were drawn, they run along the structure's axes.
        let st = st.oriented();
        let an = analyse(&st);
        errors.extend(an.warnings.iter().cloned());
        let show = show_answers || !st.answer;
        let opts = ws.map(|w| *w.options()).unwrap_or_default();
        let width = st.width;

        // The structure's extent.
        let used: Vec<(f64, f64)> =
            st.nodes.iter().filter(|n| st.members.iter().any(|m| m.a == n.id || m.b == n.id)).map(|n| (n.x.value, n.y.value)).collect();
        let mut pen = Pen::default();
        if used.is_empty() {
            pen.text((width / 2.0, 12.0), "—".to_string(), 3.0, false, PlotAnchor::Center, GREY);
            let plot = PlotRendered { size: (width, 20.0), viewport: [0.0, 0.0, width, 20.0], prims: pen.prims, errors, ..PlotRendered::default() };
            return StructureRendered { plot, analysis: an, resolved: st };
        }
        let (xmin, xmax) = used.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
        let (ymin, ymax) = used.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
        let (bw, bh) = (xmax - xmin, ymax - ymin);
        let panels = plan(&st, &an);
        // Two columns for a frame (a tall structure), one for a beam.
        let grid = match st.layout {
            Layout::Column => false,
            Layout::Grid => true,
            Layout::Auto => bh > 0.25 * bw && panels.len() > 2,
        };
        let cols: usize = if grid { 2 } else { 1 };
        let cell = width / cols as f64;
        let avail = (cell - 2.0 * MARGIN).max(10.0);
        // The scale: the structure as wide as the cell leaves once its texts and arrows are in,
        // found by drawing it and narrowing it until everything is inside (a few rounds at most).
        let mut k = if bw > 0.0 { (avail - 30.0).max(10.0) / bw } else { f64::MAX };
        if bh > 0.0 {
            k = k.min(if grid { 60.0 } else { 70.0 } / bh);
        }
        if !k.is_finite() || k == f64::MAX {
            k = 10.0;
        }
        let mut cx = Ctx { st: &st, an: &an, opts, pen: Pen::default(), amp_cap: f64::MAX, placed: Vec::new() };
        let mut drawn = Vec::new();
        for _ in 0..6 {
            drawn = cx.draw_panels(&panels, k, (xmin, ymax, bw, bh), true);
            let over = (0..cols).map(|c| column_box(&drawn, c, cols).map_or(0.0, |b| b[2] - b[0] - avail)).fold(0.0_f64, f64::max);
            let sw = bw * k;
            if over <= 0.05 || bw <= 0.0 || sw <= 10.0 {
                break;
            }
            k = (sw - over - 0.3).max(10.0) / bw;
        }
        // The columns, each centred in its cell.
        let shifts: Vec<f64> =
            (0..cols).map(|c| column_box(&drawn, c, cols).map_or(0.0, |b| cell * c as f64 + (cell - (b[2] - b[0])) / 2.0 - b[0])).collect();
        // The student's copy: drawn without the answers, laid out as the teacher's.
        let student = if show { None } else { Some(cx.draw_panels(&panels, k, (xmin, ymax, bw, bh), false)) };

        let mut pen = Pen::default();
        let mut y = MARGIN;
        if !st.title.is_empty() {
            pen.text((width / 2.0, y + 3.5), st.title.clone(), 3.5, true, PlotAnchor::Center, BLACK);
            y += 7.0;
        }
        let mut first = true;
        for (r, row) in drawn.chunks(cols).enumerate() {
            if !first {
                y += PANEL_GAP;
            }
            first = false;
            // The free body's row shows the senses its equations count positive.
            let titled = row.iter().any(|(p, _, _)| cx.panel_title(*p).is_some() || matches!(p, Panel::Equilibrium | Panel::Arms));
            let top = y + if titled { TITLE_ROW } else { 0.0 };
            let height = row.iter().filter_map(|(_, _, b)| b.map(|b| b[3] - b[1])).fold(0.0_f64, f64::max);
            for (c, (panel, prims, bbox)) in row.iter().enumerate() {
                let i = r * cols + c;
                if let Some((text, color)) = cx.panel_title(*panel) {
                    let after = cell * c as f64 + 2.0 + text.chars().count() as f64 * 3.0 * 0.7 + conventions::AFTER_TITLE;
                    pen.text((cell * c as f64 + 2.0, y + 4.0), text, 3.0, true, PlotAnchor::Left, color);
                    let action = match panel {
                        Panel::Diagram(a) => Some(*a),
                        Panel::Truss => Some(Action::Axial),
                        _ => None,
                    };
                    if let Some(a) = action {
                        pen.convention((after, y + 3.0), a, color);
                    }
                }
                if matches!(panel, Panel::Equilibrium | Panel::Arms) {
                    pen.positive((cell * c as f64 + 2.0, y), st.positive);
                }
                let Some(b) = bbox else { continue };
                let prims = student.as_ref().and_then(|s| s.get(i)).map_or(prims, |s| &s.1);
                let (dx, dy) = (shifts.get(c).copied().unwrap_or(0.0), top - b[1]);
                pen.prims.extend(prims.iter().cloned().map(|p| moved(p, dx, dy)));
            }
            y = top + height;
        }
        let solved = an.status.is_solved();
        if !solved && st.view != View::Structure {
            let msg = match &an.status {
                Status::Empty => String::new(),
                other => other.describe(),
            };
            if !msg.is_empty() {
                pen.text((width / 2.0, y + 5.0), msg, 3.0, true, PlotAnchor::Center, ERROR);
                y += 7.0;
            }
        }
        let height = y + MARGIN;
        let plot = PlotRendered { size: (width, height), viewport: [0.0, 0.0, width, height], prims: pen.prims, errors, ..PlotRendered::default() };
        StructureRendered { plot, analysis: an, resolved: st }
    }
}

/// A drawn panel: what it is, its primitives (the structure's left at x 0, its top at y 0) and
/// their box.
type Drawn = (Panel, Vec<PlotPrim>, Option<[f64; 4]>);

/// The box round the panels of column `c` (of `cols`).
fn column_box(drawn: &[Drawn], c: usize, cols: usize) -> Option<[f64; 4]> {
    drawn.iter().enumerate().filter(|(i, _)| i % cols == c).filter_map(|(_, d)| d.2).reduce(union)
}

fn union(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]
}

/// The box round what a primitive draws (texts estimated: glyphs ~0.6 of the height wide, a
/// little wider than most fonts' so a value at the drawing's edge, `7.071 kN` beside a horizontal
/// reaction, stays inside it whatever font draws it).
fn prim_box(p: &PlotPrim) -> Option<[f64; 4]> {
    let b = match p {
        PlotPrim::Rect { rect, .. } => [rect[0], rect[1], rect[0] + rect[2], rect[1] + rect[3]],
        PlotPrim::Path { pts, stroke, .. } => {
            let w = stroke.map_or(0.0, |s| s.1 / 2.0);
            let b = pts.iter().fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, p| [b[0].min(p.0), b[1].min(p.1), b[2].max(p.0), b[3].max(p.1)]);
            [b[0] - w, b[1] - w, b[2] + w, b[3] + w]
        }
        PlotPrim::Dot { at, r, .. } => [at.0 - r, at.1 - r, at.0 + r, at.1 + r],
        PlotPrim::Text { at, text, height, bold, anchor, .. } => {
            let w = text.chars().count() as f64 * height * if *bold { 0.65 } else { 0.6 };
            let (x0, y0) = match anchor {
                PlotAnchor::Left => (at.0, at.1 - height * 0.85),
                PlotAnchor::Center => (at.0 - w / 2.0, at.1 - height * 0.85),
                PlotAnchor::Right => (at.0 - w, at.1 - height * 0.85),
                PlotAnchor::TopCenter => (at.0 - w / 2.0, at.1),
                PlotAnchor::MiddleRight => (at.0 - w, at.1 - height * 0.55),
            };
            [x0, y0, x0 + w, y0 + height * 1.1]
        }
        PlotPrim::TextAt { at, text, height, angle, .. } => annotate::turned_box(*at, text, *height, *angle),
    };
    b.iter().all(|v| v.is_finite()).then_some(b)
}

/// A primitive moved by `(dx, dy)`.
fn moved(p: PlotPrim, dx: f64, dy: f64) -> PlotPrim {
    let m = |q: (f64, f64)| (q.0 + dx, q.1 + dy);
    match p {
        PlotPrim::Rect { rect, fill, stroke } => PlotPrim::Rect { rect: [rect[0] + dx, rect[1] + dy, rect[2], rect[3]], fill, stroke },
        PlotPrim::Path { pts, closed, stroke, fill, dash } => PlotPrim::Path { pts: pts.into_iter().map(m).collect(), closed, stroke, fill, dash },
        PlotPrim::Dot { at, r, fill, stroke } => PlotPrim::Dot { at: m(at), r, fill, stroke },
        PlotPrim::Text { at, text, height, bold, anchor, color } => PlotPrim::Text { at: m(at), text, height, bold, anchor, color },
        PlotPrim::TextAt { at, text, height, angle, color } => PlotPrim::TextAt { at: m(at), text, height, angle, color },
    }
}

impl Ctx<'_> {
    /// Every panel drawn at scale `k` (the structure's `(xmin, ymax, width, height)` in world
    /// units), each on its own, with its box.
    fn draw_panels(&mut self, panels: &[Panel], k: f64, (xmin, ymax, bw, bh): (f64, f64, f64, f64), show: bool) -> Vec<Drawn> {
        let sh = bh * k;
        // A frame's diagrams in proportion to it: at most a fifth of its size on paper.
        self.amp_cap = if bh > 0.0 { 0.2 * (bw * k).max(sh) } else { f64::MAX };
        let f = Frame { x0: 0.0, top: 0.0, xmin, ymax, k };
        panels
            .iter()
            .map(|&panel| {
                self.pen = Pen::default();
                self.placed.clear();
                self.panel(panel, &f, show);
                let prims = std::mem::take(&mut self.pen.prims);
                let bbox = prims.iter().filter_map(prim_box).reduce(union);
                (panel, prims, bbox)
            })
            .collect()
    }

    /// A panel's title and its colour (the diagrams' panels have one).
    fn panel_title(&self, panel: Panel) -> Option<(String, Rgb)> {
        let st = self.st;
        let fu = st.force_unit.symbol();
        let lu = st.length_unit.symbol();
        match panel {
            Panel::Schema | Panel::Reactions | Panel::Equilibrium | Panel::Section | Panel::Arms => None,
            // The name given to the diagram (`Sforzo normale`), else its symbol (`N`).
            Panel::Truss | Panel::Diagram(Action::Axial) => Some((format!("{} [{fu}]", named(&st.names.n, &st.titles.n)), COLOR_N)),
            Panel::Diagram(Action::Shear) => Some((format!("{} [{fu}]", named(&st.names.v, &st.titles.v)), COLOR_V)),
            Panel::Diagram(Action::Moment) => Some((format!("{} [{fu}·{lu}]", named(&st.names.m, &st.titles.m)), COLOR_M)),
        }
    }

    /// Draws one panel in frame `f`.
    fn panel(&mut self, panel: Panel, f: &Frame, show: bool) {
        let d = self.st.diagrams;
        match panel {
            Panel::Schema => {
                self.members(f, BLACK, MEMBER_W, true);
                self.supports(f, BLACK);
                self.loads(f);
                self.force_angles(f);
                self.slopes(f);
                self.dimensions(f);
                self.node_names(f);
            }
            Panel::Reactions => {
                self.members(f, if show { GREY } else { BLACK }, MEMBER_W, true);
                self.supports(f, if show { GREY } else { BLACK });
                // On its own (an exercise's question) it carries the schema's indications: the
                // loads with their names and values, the dimensions (after the reactions, so they
                // keep clear of them) and the nodes' names (clear of the reactions' arrows).
                let alone = self.st.view == View::Reactions;
                if alone {
                    self.loads(f);
                    self.force_angles(f);
                    self.slopes(f);
                }
                let taken = if show { self.reactions(f) } else { Vec::new() };
                if alone {
                    self.dimensions(f);
                    let sides = |id: &str| taken.iter().filter(|(n, _)| n == id).flat_map(|(_, s)| s.iter().copied()).collect();
                    self.node_names_avoiding(f, BLACK, true, &|_| true, &sides);
                }
            }
            // Names only, no answers: the same in the student's copy.
            Panel::Equilibrium => self.equilibrium(f),
            Panel::Section => self.section(f),
            Panel::Arms => self.arms(f),
            Panel::Truss => {
                if show {
                    self.truss(f);
                } else {
                    self.members(f, GREY, AXIS_W, false);
                }
            }
            Panel::Diagram(action) => {
                let (dg, color) = match action {
                    Action::Axial => (d.n, COLOR_N),
                    Action::Shear => (d.v, COLOR_V),
                    Action::Moment => (d.m, COLOR_M),
                };
                self.members(f, GREY, AXIS_W, false);
                if show {
                    let dg = Diagram { amplitude: dg.amplitude.min(self.amp_cap), ..dg };
                    self.diagram(f, action, dg, color);
                }
            }
        }
    }
}

/// The panels the structure's view shows, top to bottom.
fn plan(st: &Structure, an: &Analysis) -> Vec<Panel> {
    let d = st.diagrams;
    let mut panels = Vec::new();
    if matches!(st.view, View::All | View::Structure) {
        panels.push(Panel::Schema);
    }
    if !an.status.is_solved() || st.view == View::Structure {
        return panels;
    }
    if st.view == View::Equilibrium {
        panels.push(Panel::Equilibrium);
    }
    if st.view == View::Section {
        panels.push(Panel::Section);
    }
    if st.view == View::Arms {
        panels.push(Panel::Arms);
    }
    if (d.reactions && st.view == View::All) || st.view == View::Reactions {
        panels.push(Panel::Reactions);
    }
    if matches!(st.view, View::All | View::Diagrams) {
        if an.truss {
            if d.n.show {
                panels.push(Panel::Truss);
            }
        } else {
            for (action, dg) in [(Action::Axial, d.n), (Action::Shear, d.v), (Action::Moment, d.m)] {
                if dg.show {
                    panels.push(Panel::Diagram(action));
                }
            }
        }
    }
    // One diagram alone, whatever its flag: the drawing asked for it.
    match (st.view, an.truss) {
        (View::Axial, true) => panels.push(Panel::Truss),
        (View::Axial, false) => panels.push(Panel::Diagram(Action::Axial)),
        (View::Shear, false) => panels.push(Panel::Diagram(Action::Shear)),
        (View::Moment, false) => panels.push(Panel::Diagram(Action::Moment)),
        _ => {}
    }
    panels
}

/// Where a diagram's value may be written.
#[derive(Clone, Copy)]
enum Place {
    /// Level, beside the point on the side the direction points to.
    Beside(P, P),
    /// Centred on the point, turned this many degrees.
    Turned(P, f64),
}

impl Place {
    fn bounds(&self, text: &str) -> [f64; 4] {
        match *self {
            Place::Beside(at, dir) => text_box(at, dir, text),
            Place::Turned(at, angle) => annotate::turned_box(at, text, TEXT, angle),
        }
    }
}

/// A panel of the drawing.
#[derive(Clone, Copy)]
enum Panel {
    Schema,
    Reactions,
    /// The free body of the reactions' working ([`View::Equilibrium`]).
    Equilibrium,
    /// One stretch of a written working, cut ([`View::Section`]).
    Section,
    /// The lever arms of one moment equation of the reactions' working ([`View::Arms`]).
    Arms,
    Truss,
    Diagram(Action),
}

/// A diagram's panel name: the name given to it, else its symbol.
fn named<'a>(name: &'a str, symbol: &'a str) -> &'a str {
    if name.trim().is_empty() { symbol } else { name }
}

#[cfg(test)]
mod layout_tests {
    use super::{prim_box, union};
    use crate::model::{Load, LoadAxes, LoadPer, Member, Node, Num, Structure, Support, SupportKind, View};

    /// The beam of the owner's example: 8 m, 10 kN/m over its first half, 10 kN at 45° at 6 m.
    fn beam() -> Structure {
        let mut st = Structure {
            nodes: vec![
                Node { id: "A".into(), x: 0.0.into(), y: 0.0.into(), hinge: false },
                Node { id: "B".into(), x: 8.0.into(), y: 0.0.into(), hinge: false },
            ],
            members: vec![Member { id: "1".into(), a: "A".into(), b: "B".into(), ..Member::default() }],
            supports: vec![
                Support { node: "A".into(), kind: SupportKind::Pin, angle: 0.0.into() },
                Support { node: "B".into(), kind: SupportKind::Roller, angle: 0.0.into() },
            ],
            loads: vec![
                Load::Distributed {
                    then: Vec::new(),
                    member: "1".into(),
                    q1: 10.0.into(),
                    q2: None,
                    from: Num::default(),
                    to: Some(4.0.into()),
                    angle: Num::new(-90.0),
                    axes: LoadAxes::Global,
                    per: LoadPer::Length,
                },
                Load::Force { node: None, member: Some("1".into()), at: 6.0.into(), value: 10.0.into(), angle: Num::new(-135.0) },
            ],
            ..Structure::default()
        };
        st.diagrams.dimensions = true;
        st.diagrams.stations = true;
        st
    }

    fn frame() -> Structure {
        let mut st = beam();
        st.nodes = vec![
            Node { id: "A".into(), x: 0.0.into(), y: 0.0.into(), hinge: false },
            Node { id: "B".into(), x: 0.0.into(), y: 4.0.into(), hinge: false },
            Node { id: "C".into(), x: 6.0.into(), y: 4.0.into(), hinge: false },
            Node { id: "D".into(), x: 6.0.into(), y: 0.0.into(), hinge: false },
        ];
        st.members = vec![
            Member { id: "1".into(), a: "A".into(), b: "B".into(), ..Member::default() },
            Member { id: "2".into(), a: "B".into(), b: "C".into(), ..Member::default() },
            Member { id: "3".into(), a: "C".into(), b: "D".into(), ..Member::default() },
        ];
        st.supports[1].node = "D".into();
        st.loads = vec![Load::Force { node: Some("B".into()), member: None, at: Num::default(), value: 10.0.into(), angle: Num::new(0.0) }];
        st
    }

    #[test]
    fn everything_drawn_stays_inside_the_drawing() {
        for st in [beam(), frame()] {
            let r = st.render(None);
            let (w, h) = r.plot.size;
            let b = r.plot.prims.iter().filter_map(prim_box).reduce(union).unwrap();
            assert!(b[0] >= -0.01 && b[1] >= -0.01 && b[2] <= w + 0.01 && b[3] <= h + 0.01, "{b:?} in {w}×{h}");
        }
    }

    #[test]
    fn panels_have_no_lines_between_them() {
        let r = beam().render(None);
        let w = r.plot.size.0;
        // No line across (nearly) the whole width: the old separators.
        assert!(!r.plot.prims.iter().any(|p| matches!(p, strix_plot::PlotPrim::Path { pts, .. }
            if pts.len() == 2 && (pts[0].1 - pts[1].1).abs() < 1e-9 && (pts[0].0 - pts[1].0).abs() > w - 6.0)));
    }

    #[test]
    fn the_reactions_on_their_own_keep_the_names_and_dimensions_clear_of_their_arrows() {
        let mut st = Structure { view: View::Reactions, ..frame() };
        st.loads.push(Load::Distributed {
            then: Vec::new(),
            member: "2".into(),
            q1: 10.0.into(),
            q2: None,
            from: Num::default(),
            to: None,
            angle: Num::new(-90.0),
            axes: LoadAxes::Global,
            per: LoadPer::Length,
        });
        let prims = st.render(None).plot.prims;
        let reaction = [0x1B, 0x4F, 0x72];
        let arrows: Vec<[f64; 4]> = prims
            .iter()
            .filter(|p| matches!(p, strix_plot::PlotPrim::Path { stroke: Some((c, _)), .. } if *c == reaction))
            .filter_map(prim_box)
            .collect();
        let values: Vec<[f64; 4]> = prims
            .iter()
            .filter(|p| matches!(p, strix_plot::PlotPrim::Text { text, color, .. } if *color == reaction && text.ends_with("kN")))
            .filter_map(prim_box)
            .collect();
        let hits = |a: &[f64; 4], b: &[f64; 4]| a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3];
        let a = prims.iter().find(|p| matches!(p, strix_plot::PlotPrim::Text { text, .. } if text == "A")).and_then(prim_box).expect("A is named");
        assert!(!arrows.iter().any(|b| hits(&a, b)), "A clear of its reactions' arrows");
        // The dimensions below the reactions' values: the span's text under them.
        let span =
            prims.iter().find(|p| matches!(p, strix_plot::PlotPrim::TextAt { text, .. } if text == "6 m")).and_then(prim_box).expect("the span");
        assert!(values.len() >= 3 && values.iter().all(|v| v[3] < span[1]), "{values:?} {span:?}");
    }

    #[test]
    fn a_view_draws_only_its_panels() {
        let all = beam().render(None).plot;
        let schema = Structure { view: View::Structure, ..beam() }.render(None).plot;
        let reactions = Structure { view: View::Reactions, ..beam() }.render(None).plot;
        let diagrams = Structure { view: View::Diagrams, ..beam() }.render(None).plot;
        let texts = |p: &strix_plot::PlotRendered| -> Vec<String> {
            p.prims
                .iter()
                .filter_map(|q| match q {
                    strix_plot::PlotPrim::Text { text, .. } | strix_plot::PlotPrim::TextAt { text, .. } => Some(text.clone()),
                    _ => None,
                })
                .collect()
        };
        let has = |p: &strix_plot::PlotRendered, t: &str| texts(p).iter().any(|x| x.contains(t));
        assert!(has(&schema, "8 m") && !has(&schema, "N [") && !has(&schema, "31.768"));
        // On its own, the reactions' panel carries the schema's dimensions, loads and names.
        assert!(has(&reactions, "31.768 kN") && has(&reactions, "8 m") && has(&reactions, "10 kN/m") && !has(&reactions, "M ["));
        assert!(texts(&reactions).iter().any(|t| t == "A") && texts(&reactions).iter().any(|t| t == "B"));
        assert!(has(&diagrams, "M [") && has(&diagrams, "50.46") && !has(&diagrams, "31.768 kN"));
        for p in [&schema, &reactions, &diagrams] {
            assert!(p.size.1 < all.size.1);
        }
        // One diagram alone, even with its flag off in the whole drawing.
        let one = |view: View| {
            let mut st = Structure { view, ..beam() };
            st.diagrams.m.show = false;
            st.render(None).plot
        };
        let (axial, shear, moment) = (one(View::Axial), one(View::Shear), one(View::Moment));
        assert!(has(&axial, "N [") && !has(&axial, "M [") && !has(&axial, "8 m"));
        assert!(has(&shear, "T [") && !has(&shear, "N [") && !has(&shear, "M ["));
        assert!(has(&moment, "M [") && has(&moment, "50.46") && !has(&moment, "T ["));
        assert!(moment.size.1 < diagrams.size.1);
        // The view is written only when it is not the whole drawing.
        assert!(!beam().to_json().contains("view"));
        let json = Structure { view: View::Diagrams, group: "structure-1".into(), ..beam() }.to_json();
        let back = Structure::from_json(&json).unwrap();
        assert_eq!((back.view, back.group.as_str()), (View::Diagrams, "structure-1"));
    }
}
