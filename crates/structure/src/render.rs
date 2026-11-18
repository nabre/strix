//! Drawing: the structure, then its reactions and diagrams one under the other at the same
//! horizontal scale, as `strix-plot` primitives in paper millimetres (y down).

use std::f64::consts::FRAC_1_SQRT_2;

use strix_calc::{Options, Worksheet, format_number};
use strix_plot::{PlotAnchor, PlotPrim, PlotRendered};

use crate::diagram::{Action, NotableKind, notables};
use crate::model::{Diagram, Layout, Load, LoadAxes, Side, Structure, SupportKind};
use crate::solve::{Analysis, MemberResult, Status, analyse};

mod annotate;

type P = (f64, f64);
type Rgb = [u8; 3];

const BLACK: Rgb = [0x1F, 0x1F, 0x1F];
const GREY: Rgb = [0x9A, 0x9A, 0x9A];
const LIGHT: Rgb = [0xD5, 0xD5, 0xD5];
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
const SPREAD: f64 = 8.0;

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
            if dir.0 > 0.0 { (PlotAnchor::Left, (at.0 + 0.5, at.1 + TEXT * 0.5)) } else { (PlotAnchor::Right, (at.0 - 0.5, at.1 + TEXT * 0.5)) }
        } else if dir.1 > 0.0 {
            (PlotAnchor::Center, (at.0, at.1 + TEXT + 0.3))
        } else {
            (PlotAnchor::Center, (at.0, at.1 - 0.5))
        };
        self.prims.push(PlotPrim::Text { at: pos, text, height: TEXT, bold: false, anchor, color: c });
    }

    fn text(&mut self, at: P, text: String, height: f64, bold: bool, anchor: PlotAnchor, c: Rgb) {
        self.prims.push(PlotPrim::Text { at, text, height, bold, anchor, color: c });
    }

    /// An arrow from `tail` to `tip` with a filled head.
    fn arrow(&mut self, tail: P, tip: P, c: Rgb, w: f64) {
        let d = unit((tip.0 - tail.0, tip.1 - tail.1));
        let len = (tip.0 - tail.0).hypot(tip.1 - tail.1);
        let head = 2.0_f64.min(len * 0.6);
        let base = add(tip, mul(d, -head));
        self.line(tail, base, c, w);
        let n = (-d.1, d.0);
        self.fill(vec![tip, add(base, mul(n, head * 0.36)), add(base, mul(n, -head * 0.36))], c, None);
    }

    /// A curved arrow round `c` (paper), counter-clockwise on the page when `ccw`.
    fn couple(&mut self, c: P, r: f64, ccw: bool, col: Rgb, w: f64) {
        let (a0, a1) = (-120.0_f64.to_radians(), 150.0_f64.to_radians());
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
        let (Some(&tip), Some(&before)) = (pts.last(), pts.get(pts.len().saturating_sub(4))) else { return };
        pts.truncate(pts.len().saturating_sub(3));
        self.poly(pts, col, w);
        self.arrow(before, tip, col, w);
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
        let st = self.st;
        for n in &st.nodes {
            if n.id.is_empty() || !st.members.iter().any(|m| m.a == n.id || m.b == n.id) {
                continue;
            }
            let p = f.p(n.x.value, n.y.value);
            // Away from the node's members.
            let away = self.free_side(&n.id);
            let supported = st.supports.iter().any(|s| s.node == n.id);
            let at = add(p, mul(away, if supported { 5.0 } else { 3.6 }));
            self.pen.text((at.0, at.1 + 1.4), n.id.clone(), 2.8, true, PlotAnchor::Center, BLACK);
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
        let st = self.st;
        for s in &st.supports {
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
        let st = self.st;
        let qmax = st
            .loads
            .iter()
            .filter_map(|l| match l {
                Load::Distributed { q1, q2, .. } => Some(q1.value.abs().max(q2.as_ref().map_or(0.0, |q| q.value.abs()))),
                _ => None,
            })
            .fold(0.0_f64, f64::max);
        for l in &st.loads {
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
                    self.pen.arrow(tail, tip, LOAD, 0.35);
                    let label = self.force(value.value.abs());
                    self.pen.beside(add(tail, mul(d, -0.8)), mul(d, -1.0), label, LOAD);
                }
                Load::Moment { node, member, at, value } => {
                    let Some(p) = self.point_of(f, node.as_deref(), member.as_deref(), at.value) else { continue };
                    if value.value == 0.0 {
                        continue;
                    }
                    self.pen.couple(p, 4.0, value.value > 0.0, LOAD, 0.35);
                    let label = self.moment(value.value.abs());
                    self.pen.label(add(p, (5.5, -5.5)), label, LOAD);
                }
                Load::Distributed { member, q1, q2, from, to, angle, axes, .. } => {
                    let Some(m) = st.member(member) else { continue };
                    let (Some(na), Some(nb)) = (st.node(&m.a), st.node(&m.b)) else { continue };
                    let Some(len) = st.length(m).filter(|l| *l > 0.0) else { continue };
                    let (c, s) = ((nb.x.value - na.x.value) / len, (nb.y.value - na.y.value) / len);
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
                    let world = |x: f64| f.p(na.x.value + c * x, na.y.value + s * x);
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
                            self.pen.arrow(tail, tip, LOAD, 0.25);
                        }
                        tails.push(tail);
                    }
                    self.pen.poly(tails.clone(), LOAD, 0.3);
                    let (Some(&first), Some(&last)) = (tails.first(), tails.last()) else { continue };
                    let out = mul(gp, -1.0);
                    if (q1 - q2).abs() <= 1e-12 * q1.abs().max(q2.abs()) {
                        let mid = tails.get(tails.len() / 2).copied().unwrap_or(first);
                        let label = self.spread(q1.abs());
                        self.pen.beside(add(mid, mul(out, 0.8)), out, label, LOAD);
                    } else {
                        for (q, at) in [(q1, first), (q2, last)] {
                            if q != 0.0 {
                                let label = self.spread(q.abs());
                                self.pen.beside(add(at, mul(out, 0.8)), out, label, LOAD);
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

    fn reactions(&mut self, f: &Frame) {
        let an = self.an;
        for r in &an.reactions {
            let Some(p) = self.node_p(f, &r.node) else { continue };
            let phi = r.angle.to_radians();
            let below = (phi.sin(), phi.cos());
            let mut comps: Vec<(P, f64)> = Vec::new();
            match r.kind {
                SupportKind::Fixed | SupportKind::Pin => {
                    comps.push(((1.0, 0.0), r.fx));
                    comps.push(((0.0, 1.0), r.fy));
                }
                SupportKind::Roller | SupportKind::Link | SupportKind::Slider => {
                    let n = (-phi.sin(), phi.cos());
                    comps.push((n, r.fx * n.0 + r.fy * n.1));
                }
            }
            let mut taken: Vec<P> = Vec::new();
            for (axis, v) in comps {
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
                let label = self.force(v.abs());
                self.pen.beside(add(far, mul(side, 0.8)), side, label, REACTION);
            }
            if r.m != 0.0 {
                self.pen.couple(p, 5.0, r.m > 0.0, REACTION, 0.4);
                let label = self.moment(r.m.abs());
                let free = self.free_side_avoiding(&r.node, &taken);
                self.pen.beside(add(p, mul(free, 6.5)), free, label, REACTION);
            }
        }
    }

    /// One diagram of `action` on every member.
    fn diagram(&mut self, f: &Frame, action: Action, d: Diagram, color: Rgb) {
        let an = self.an;
        let (hatch, signs, values) = (self.st.diagrams.hatch, self.st.diagrams.signs, self.st.diagrams.values);
        let samples: Vec<Vec<Vec<(f64, f64)>>> = an.members.iter().map(|m| sample(m, action)).collect();
        let vmax = samples.iter().flatten().flatten().map(|(_, v)| v.abs()).fold(0.0_f64, f64::max);
        let side = if d.positive == Side::Up { 1.0 } else { -1.0 };
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
                let mut pts = vec![axis(x0)];
                pts.extend(seg.iter().map(|&(x, v)| curve(x, v)));
                pts.push(axis(x1));
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
                    let at = point(x, 1.2);
                    // One label per place: a value shared by two members at a node, once.
                    if labels.iter().any(|(_, t, a)| *t == text && (a.0 - at.0).hypot(a.1 - at.1) < 8.0) {
                        continue;
                    }
                    let places = if constant {
                        // A stretch of constant value: written level, at its middle or sliding along it.
                        let mut places = vec![Place::Beside(at, out), Place::Beside(point(x, 3.6), out)];
                        if let Some(seg) = segs.iter().find(|s| s.first().is_some_and(|p| p.0 < x) && s.last().is_some_and(|p| p.0 > x)) {
                            let (Some(&(x0, _)), Some(&(x1, _))) = (seg.first(), seg.last()) else { continue };
                            for t in [0.3, 0.7, 0.15, 0.85] {
                                places.push(Place::Beside(point(x0 + (x1 - x0) * t, 0.8), out));
                            }
                        }
                        places
                    } else {
                        // A value at a point: written across the axis, standing on the curve there.
                        let half = text.chars().count() as f64 * TEXT * 0.55 / 2.0;
                        let angle = annotate::reading_angle(out);
                        vec![Place::Turned(point(x, 1.0 + half), angle), Place::Turned(point(x, 3.5 + half), angle)]
                    };
                    labels.push((places, text, at));
                }
            }
        }
        for (places, text, _) in labels {
            let boxes: Vec<[f64; 4]> = places.iter().map(|p| p.bounds(&text)).collect();
            let free = boxes.iter().position(|b| !self.placed.iter().any(|o| overlaps(b, o)));
            let k = free.unwrap_or(0);
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
        let lu = self.st.length_unit.symbol();
        let height = TEXT * 0.9;
        let mut last: Option<P> = None;
        for x in xs {
            let v = interp_member(segs, x).unwrap_or(0.0);
            let o = ord(v);
            let (base, top) = (axis(x), curve(x, v));
            if o.abs() > 0.8 {
                self.pen.dashed(base, top, lighter(color, 0.2), 0.15);
            }
            self.pen.line(add(base, mul(normal, -0.8)), add(base, mul(normal, 0.8)), color, 0.25);
            // Too close to the last one on paper: the tick only.
            if last.is_some_and(|p| (p.0 - base.0).hypot(p.1 - base.1) < height * 1.2) {
                continue;
            }
            let text = format!("{} {lu}", self.num(x));
            let half = text.chars().count() as f64 * height * 0.55 / 2.0;
            let away = mul(
                normal,
                if o > 0.0 {
                    -1.0
                } else if o < 0.0 {
                    1.0
                } else {
                    -1.0
                },
            );
            let angle = annotate::reading_angle(away);
            let at = add(base, mul(away, 1.4 + half));
            self.placed.push(annotate::turned_box(at, &text, height, angle));
            self.pen.turned(at, text, height, angle, GREY);
            last = Some(base);
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
    let bps = m.breakpoints();
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
    let bps = m.breakpoints();
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
    /// The structure alone — members, hinges, supports, loads and node names — for an editor:
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
        let d = st.diagrams;

        // The panels, top to bottom: (what, room above the structure, below it).
        let solved = an.status.is_solved();
        let mut panels = plan(&st, &an);
        // Two columns for a frame (a tall structure), one for a beam.
        let grid = match st.layout {
            Layout::Column => false,
            Layout::Grid => true,
            Layout::Auto => bh > 0.25 * bw && panels.len() > 2,
        };
        let cols = if grid { 2.0 } else { 1.0 };
        let cell = width / cols;
        let amp = [d.n, d.v, d.m].iter().filter(|x| x.show).map(|x| x.amplitude).fold(0.0_f64, f64::max);
        // A grid's cells keep room at the left for the reactions' arrows and their values, and at
        // the right for the chain of heights when there is one.
        let (side, side_r) = if grid {
            (22.0, if annotate::has_y_chain(&st) { 14.0 + annotate::Y_CHAIN_ROOM } else { 14.0 })
        } else {
            ((8.0 + amp).max(16.0), (8.0 + amp).max(16.0))
        };
        let usable = (cell - side - side_r).max(10.0);
        let mut k = if bw > 0.0 { usable / bw } else { f64::MAX };
        if bh > 0.0 {
            k = k.min(if grid { 60.0 } else { 70.0 } / bh);
        }
        if !k.is_finite() || k == f64::MAX {
            k = 10.0;
        }
        let sh = bh * k;
        // A frame's diagrams in proportion to it: at most a fifth of its size on paper.
        let amp_cap = if bh > 0.0 { 0.2 * (bw * k).max(sh) } else { f64::MAX };
        for (panel, above, below) in &mut panels {
            if let Panel::Diagram(action) = panel {
                let dg = match action {
                    Action::Axial => d.n,
                    Action::Shear => d.v,
                    Action::Moment => d.m,
                };
                let pad = (dg.amplitude.min(amp_cap) + 9.0).max(12.0);
                (*above, *below) = (pad, pad);
            }
        }

        let mut cx = Ctx { st: &st, an: &an, opts, pen, amp_cap, placed: Vec::new() };
        let mut y = 2.0;
        if !st.title.is_empty() {
            cx.pen.text((width / 2.0, y + 4.0), st.title.clone(), 3.5, true, PlotAnchor::Center, BLACK);
            y += 7.0;
        }
        let rows: Vec<&[(Panel, f64, f64)]> = if grid { panels.chunks(2).collect() } else { panels.chunks(1).collect() };
        for (r, row) in rows.iter().enumerate() {
            let above = row.iter().map(|p| p.1).fold(0.0_f64, f64::max);
            let below = row.iter().map(|p| p.2).fold(0.0_f64, f64::max);
            if r > 0 {
                cx.pen.line((2.0, y), (width - 2.0, y), LIGHT, 0.2);
            }
            if grid && row.len() == 2 {
                cx.pen.line((cell, y + 2.0), (cell, y + above + sh + below - 2.0), LIGHT, 0.2);
            }
            for (c, (panel, _, _)) in row.iter().enumerate() {
                let left = cell * c as f64;
                let f = Frame { x0: left + side + (usable - bw * k) / 2.0, top: y + above, xmin, ymax, k };
                cx.panel(*panel, &f, (left, y), show);
            }
            y += above + sh + below;
        }
        if !solved {
            let msg = match &an.status {
                Status::Empty => String::new(),
                other => other.describe(),
            };
            if !msg.is_empty() {
                cx.pen.text((width / 2.0, y + 4.0), msg, 3.0, true, PlotAnchor::Center, ERROR);
                y += 8.0;
            }
        }
        let height = y + 2.0;
        let plot =
            PlotRendered { size: (width, height), viewport: [0.0, 0.0, width, height], prims: cx.pen.prims, errors, ..PlotRendered::default() };
        StructureRendered { plot, analysis: an, resolved: st }
    }
}

impl Ctx<'_> {
    /// Draws one panel in frame `f`, its title at `corner` (its top left).
    fn panel(&mut self, panel: Panel, f: &Frame, corner: P, show: bool) {
        let st = self.st;
        let d = st.diagrams;
        let fu = st.force_unit.symbol();
        let lu = st.length_unit.symbol();
        let title = |pen: &mut Pen, text: String, color: Rgb| pen.text((corner.0 + 3.0, corner.1 + 4.5), text, 3.0, true, PlotAnchor::Left, color);
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
                if show {
                    self.reactions(f);
                }
            }
            Panel::Truss => {
                title(&mut self.pen, format!("{} [{fu}]", st.titles.n), COLOR_N);
                if show {
                    self.truss(f);
                } else {
                    self.members(f, GREY, AXIS_W, false);
                }
            }
            Panel::Diagram(action) => {
                let (dg, color, name, unit) = match action {
                    Action::Axial => (d.n, COLOR_N, st.titles.n.clone(), fu.to_string()),
                    Action::Shear => (d.v, COLOR_V, st.titles.v.clone(), fu.to_string()),
                    Action::Moment => (d.m, COLOR_M, st.titles.m.clone(), format!("{fu}·{lu}")),
                };
                title(&mut self.pen, format!("{name} [{unit}]"), color);
                self.members(f, GREY, AXIS_W, false);
                if show {
                    let dg = Diagram { amplitude: dg.amplitude.min(self.amp_cap), ..dg };
                    self.diagram(f, action, dg, color);
                }
            }
        }
    }
}

/// The panels, top to bottom: (what, room above the structure, below it).
fn plan(st: &Structure, an: &Analysis) -> Vec<(Panel, f64, f64)> {
    let d = st.diagrams;
    let dim_rows = annotate::dimension_room(st);
    let schema_below = if dim_rows > 0 { annotate::DIM_BELOW + annotate::DIM_STEP * (dim_rows - 1) as f64 + 5.0 } else { 14.0 };
    let mut panels: Vec<(Panel, f64, f64)> = vec![(Panel::Schema, 16.0, schema_below)];
    if !an.status.is_solved() {
        return panels;
    }
    if d.reactions {
        panels.push((Panel::Reactions, 16.0, 22.0));
    }
    if an.truss {
        if d.n.show {
            panels.push((Panel::Truss, 12.0, 8.0));
        }
    } else {
        for (action, dg) in [(Action::Axial, d.n), (Action::Shear, d.v), (Action::Moment, d.m)] {
            if dg.show {
                let pad = (dg.amplitude + 6.0).max(9.0);
                panels.push((Panel::Diagram(action), pad, pad));
            }
        }
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
    Truss,
    Diagram(Action),
}
