//! The free body the reactions' working balances ([`View::Equilibrium`](crate::View)): the
//! structure with its supports faded and their reactions drawn as unknowns (`H_A`, `V_A`, `V_B`,
//! `M_A`, pointing the way they count positive), each distributed load faded with its resultant
//! (`F_1`, a trapezoid's `F_(2,1)` and `F_(2,2)`) at its centroid, each askew force faded with its
//! horizontal and vertical parts (`F_(3,h)`, `F_(3,v)`), named as the working names them, and the
//! dimensions with a station at each resultant, which the moments' arms are read from.

use strix_plot::{PlotAnchor, PlotPrim};

use super::{ARROW, BLACK, Ctx, Frame, GAP, GREY, LOAD, MEMBER_W, P, Pen, REACTION, Rgb, SPREAD, TEXT, add, lighter, mul, pd};
use crate::model::{Load, SupportKind};
use crate::names::load_names;
use crate::working::{ResultantKind, resultants};

/// A direction's component below this is none (the force runs along an axis).
const TOL: f64 = 1e-9;
/// A subscript's height, as a share of the text's.
const SUB: f64 = 0.72;

/// Does a direction have both a horizontal and a vertical part?
fn askew(d: (f64, f64)) -> bool {
    d.0.abs() > TOL && d.1.abs() > TOL
}

/// A name's subscript from a node name, as the working writes it: its letters and digits (its
/// number among the supports when it has none).
fn node_sub(id: &str, fallback: usize) -> String {
    let s: String = id.chars().filter(|c| c.is_alphanumeric()).take(8).collect();
    if s.is_empty() { fallback.to_string() } else { s }
}

/// A glyph's width as a share of the text's height (a sans-serif's, roughly).
fn glyph(c: char) -> f64 {
    match c {
        'M' | 'W' | 'm' | 'w' => 0.85,
        'F' | 'E' | 'L' | 'P' | 'T' => 0.6,
        ',' | '.' | 'i' | 'l' | 'j' => 0.3,
        c if c.is_uppercase() => 0.72,
        _ => 0.55,
    }
}

impl Pen {
    /// `base` with the subscript `sub` (`F` and `2,h`: F₂,ₕ), beside `at` on the side `dir`
    /// points to, as [`Pen::beside`] places a text.
    pub(super) fn subscripted(&mut self, at: P, dir: P, base: &str, sub: &str, c: Rgb) {
        let wb = base.chars().map(glyph).sum::<f64>() * TEXT + 0.5;
        let ws = sub.chars().map(glyph).sum::<f64>() * TEXT * SUB;
        let w = wb + ws;
        let (x, y) = if dir.0.abs() >= 0.6 {
            if dir.0 > 0.0 { (at.0 + GAP, at.1 + TEXT * 0.4) } else { (at.0 - GAP - w, at.1 + TEXT * 0.4) }
        } else if dir.1 > 0.0 {
            (at.0 - w / 2.0, at.1 + TEXT * 0.85 + GAP)
        } else {
            (at.0 - w / 2.0, at.1 - GAP)
        };
        self.prims.push(PlotPrim::Text { at: (x, y), text: base.to_string(), height: TEXT, bold: false, anchor: PlotAnchor::Left, color: c });
        if !sub.is_empty() {
            let at = (x + wb, y + TEXT * 0.3);
            self.prims.push(PlotPrim::Text { at, text: sub.to_string(), height: TEXT * SUB, bold: false, anchor: PlotAnchor::Left, color: c });
        }
    }
}

impl Ctx<'_> {
    /// The free body of the reactions' working.
    pub(super) fn equilibrium(&mut self, f: &Frame) {
        let st = self.st;
        let faded = lighter(LOAD, 0.6);
        self.members(f, BLACK, MEMBER_W, true);
        self.supports(f, lighter(GREY, 0.45));
        // What the resultants and the parts stand for, faded and unnamed.
        let askew_force = |l: &Load| match l {
            Load::Force { angle, .. } => {
                let a = angle.value.to_radians();
                askew((a.cos(), a.sin()))
            }
            _ => false,
        };
        self.loads_as(f, faded, false, &|_, l| matches!(l, Load::Distributed { .. }) || askew_force(l));
        let names = load_names(st);
        let mut stations: Vec<(String, f64)> = Vec::new();
        for r in resultants(st) {
            let Some(name) = names.get(r.load).copied().flatten() else { continue };
            let n = name.number.to_string();
            let p = f.p(r.at.0, r.at.1);
            let value = r.value();
            if value == 0.0 || !value.is_finite() {
                continue;
            }
            let (idx, len) = match r.kind {
                ResultantKind::Couple { .. } => {
                    self.pen.couple(p, 4.0, value > 0.0, LOAD, 0.35);
                    self.pen.label(add(p, (5.5, -5.5)), name.pretty(), LOAD);
                    continue;
                }
                ResultantKind::Force { .. } => (vec![n], ARROW),
                ResultantKind::Spread { triangle, .. } => {
                    // Its centroid is a station of the dimensions: the arms are read there.
                    if let Some(Load::Distributed { member, .. }) = st.loads.get(r.load)
                        && let Some(a) = st.member(member).and_then(|m| st.node(&m.a))
                    {
                        stations.push((member.clone(), (r.at.0 - a.x.value).hypot(r.at.1 - a.y.value)));
                    }
                    let (q1, q2) = match st.loads.get(r.load) {
                        Some(Load::Distributed { q1, q2, .. }) => (q1.value, q2.as_ref().map_or(q1.value, |q| q.value)),
                        _ => (0.0, 0.0),
                    };
                    let idx = if q1 != 0.0 && q2 != q1 { vec![n, if triangle { "2" } else { "1" }.to_string()] } else { vec![n] };
                    // Above the load's band of arrows.
                    (idx, SPREAD + ARROW * 0.7)
                }
            };
            // The way it acts on the paper.
            let d = mul(pd(r.dir), value.signum());
            if !askew(r.dir) {
                self.named_arrow(p, d, len, &idx, LOAD, (0.0, -1.0));
                continue;
            }
            // An askew distributed load's resultant faded, as its load (a force is drawn so).
            if matches!(r.kind, ResultantKind::Spread { .. }) {
                let tail = add(p, mul(d, -len));
                self.pen.arrow(tail, add(p, mul(d, -0.5)), faded, 0.35);
            }
            // Its parts, as long as their share of it; the level one named on the side the
            // other one's tail is not.
            let level_side = (0.0, if d.1 > 0.0 { 1.0 } else { -1.0 });
            for (part, c) in [("h", (d.0, 0.0)), ("v", (0.0, d.1))] {
                let size = c.0.hypot(c.1);
                let dc = mul(c, 1.0 / size.max(TOL));
                let mut i = idx.clone();
                i.push(part.to_string());
                self.named_arrow(p, dc, (len * size).max(5.0), &i, LOAD, level_side);
            }
        }
        self.dimensions_with(f, &stations);
        self.unknowns(f);
        self.node_names(f);
    }

    /// An arrow of `len` mm acting at `p` along paper direction `d`, named `F` with the indices
    /// `idx` beyond its tail; a level one beside its middle, on the paper side `level_side`.
    fn named_arrow(&mut self, p: P, d: P, len: f64, idx: &[String], c: Rgb, level_side: P) {
        let tail = add(p, mul(d, -len));
        self.pen.arrow(tail, add(p, mul(d, -0.5)), c, 0.4);
        let sub = idx.join(",");
        if d.1.abs() < 0.3 {
            // A level arrow (along a beam) is named beside its middle, clear of the member.
            let mid = add(p, mul(d, -len / 2.0));
            self.pen.subscripted(add(mid, mul(level_side, 0.6)), level_side, "F", &sub, c);
        } else {
            self.pen.subscripted(add(tail, mul(d, -0.8)), mul(d, -1.0), "F", &sub, c);
        }
    }

    /// The reactions as unknowns, named as the working names them, each pointing the way it
    /// counts positive (along x, along y, along a roller's direction; a couple turning the way a
    /// rotation counts positive), as the structure's [`Positive`](crate::Positive) says.
    fn unknowns(&mut self, f: &Frame) {
        self.unknowns_where(f, &|_| true);
    }

    /// [`Ctx::unknowns`] of the supports at the nodes `at` picks.
    pub(super) fn unknowns_where(&mut self, f: &Frame, at: &dyn Fn(&str) -> bool) {
        let an = self.an;
        let pos = self.st.positive;
        let (sx, sy) = (pos.sx(), pos.sy());
        for (k, r) in an.reactions.iter().enumerate() {
            if !at(&r.node) {
                continue;
            }
            let Some(p) = self.node_p(f, &r.node) else { continue };
            let s = node_sub(&r.node, k + 1);
            let phi = r.angle.to_radians();
            let below = (phi.sin(), phi.cos());
            let mut comps: Vec<((f64, f64), &str)> = Vec::new();
            match r.kind {
                SupportKind::Fixed | SupportKind::Pin => {
                    comps.push(((sx, 0.0), "H"));
                    comps.push(((0.0, sy), "V"));
                }
                SupportKind::Roller | SupportKind::RollerCircle | SupportKind::Link | SupportKind::Slider => {
                    let n = (-phi.sin(), phi.cos());
                    if n.0.abs() < TOL {
                        comps.push(((0.0, sy), "V"));
                    } else if n.1.abs() < TOL {
                        comps.push(((sx, 0.0), "H"));
                    } else {
                        comps.push((n, "R"));
                    }
                }
            }
            // The node's name takes its free side.
            let mut taken: Vec<P> = vec![self.free_side(&r.node)];
            for (axis, letter) in comps {
                let ap = pd(axis);
                let side = self.arrow_side(&r.node, ap, below);
                taken.push(side);
                let far = add(p, mul(side, ARROW + 2.0));
                let near = add(p, mul(side, 1.0));
                if ap.0 * side.0 + ap.1 * side.1 < 0.0 {
                    self.pen.arrow(far, near, REACTION, 0.4);
                } else {
                    self.pen.arrow(near, far, REACTION, 0.4);
                }
                self.pen.subscripted(add(far, mul(side, 0.8)), side, letter, &s, REACTION);
            }
            if matches!(r.kind, SupportKind::Fixed | SupportKind::Slider) {
                self.pen.couple(p, 5.0, pos.sr() > 0.0, REACTION, 0.4);
                let free = self.free_side_avoiding(&r.node, &taken);
                self.pen.subscripted(add(p, mul(free, 6.5)), free, "M", &s, REACTION);
            }
        }
    }
}
