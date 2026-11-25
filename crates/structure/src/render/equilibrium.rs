//! The free body the reactions' working balances ([`View::Equilibrium`](crate::View)): the
//! structure with its supports faded and their reactions drawn as unknowns (`H_A`, `V_A`, `V_B`,
//! `M_A`, pointing the way they count positive), each distributed load faded with its resultant
//! (`F_1`, a trapezoid's `F_(2,1)` and `F_(2,2)`) at its centroid, each askew force faded with its
//! horizontal and vertical parts (`F_(3,h)`, `F_(3,v)`), named as the working names them, and the
//! dimensions with a station at each resultant, which the moments' arms are read from.

use strix_plot::{PlotAnchor, PlotPrim};

use super::{ARROW, BLACK, Ctx, Frame, GAP, GREY, LOAD, MEMBER_W, P, Pen, REACTION, Rgb, SPREAD, TEXT, add, lighter, mul, pd, unit};
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
pub(super) fn node_sub(id: &str, fallback: usize) -> String {
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
        self.subscripted_then(at, dir, base, sub, "", c);
    }

    /// The paper width [`Pen::subscripted`] gives `base` with the subscript `sub`.
    fn subscripted_width(base: &str, sub: &str) -> f64 {
        base.chars().map(glyph).sum::<f64>() * TEXT + 0.5 + sub.chars().map(glyph).sum::<f64>() * TEXT * SUB
    }

    /// [`Pen::subscripted`] followed by `tail` (`H`, `A` and ` = 10 kN`: H_A = 10 kN).
    pub(super) fn subscripted_then(&mut self, at: P, dir: P, base: &str, sub: &str, tail: &str, c: Rgb) {
        let wb = base.chars().map(glyph).sum::<f64>() * TEXT + 0.5;
        let ws = sub.chars().map(glyph).sum::<f64>() * TEXT * SUB;
        // The tail's width taken generously (as the layout's boxes take a text's): on the left
        // the text ends before `at`, a wider estimate only leaves room there.
        let wt = tail.chars().count() as f64 * TEXT * 0.65;
        let w = wb + ws + wt;
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
        if !tail.is_empty() {
            // Right after the subscript, on the left too: the generous width leaves it ending
            // before `at`.
            self.prims.push(PlotPrim::Text {
                at: (x + wb + ws, y),
                text: tail.to_string(),
                height: TEXT,
                bold: false,
                anchor: PlotAnchor::Left,
                color: c,
            });
        }
    }
}

impl Ctx<'_> {
    /// The free body of the reactions' working.
    pub(super) fn equilibrium(&mut self, f: &Frame) {
        self.members(f, BLACK, MEMBER_W, true);
        self.supports(f, lighter(GREY, 0.45));
        let (stations, mut taken) = self.resultants_named(f, &|_| true);
        // The dimensions after the reactions, so they keep clear of them.
        taken.extend(self.unknowns(f));
        self.dimensions_with(f, &stations);
        self.node_names_clear_of(f, &|_| true, &taken);
    }

    /// The names of the nodes `at` picks, each clear of the arrows drawn at it: `taken` holds
    /// each arrow's paper point and the paper direction it (or its name) takes from there.
    pub(super) fn node_names_clear_of(&mut self, f: &Frame, at: &dyn Fn(&str) -> bool, taken: &[(P, P)]) {
        let st = self.st;
        let sides = |id: &str| -> Vec<P> {
            let Some(p) = self.node_p(f, id) else { return Vec::new() };
            taken.iter().filter(|(q, _)| (q.0 - p.0).hypot(q.1 - p.1) < 0.5).map(|&(_, d)| d).collect()
        };
        let sides: Vec<(String, Vec<P>)> = st.nodes.iter().map(|n| (n.id.clone(), sides(&n.id))).collect();
        let of = |id: &str| sides.iter().find(|(n, _)| n == id).map(|(_, d)| d.clone()).unwrap_or_default();
        self.node_names_avoiding(f, BLACK, true, at, &of);
    }

    /// The loads `which` picks as the free body draws them: faded where they act, each
    /// distributed load's resultant and each askew force's parts named as the working names
    /// them. The distributed loads' centroids, as stations for the dimensions; and each arrow's
    /// point with the paper directions its tail and its name take from there, for the nodes'
    /// names to keep clear of.
    #[allow(clippy::type_complexity)]
    pub(super) fn resultants_named(&mut self, f: &Frame, which: &dyn Fn(&Load) -> bool) -> (Vec<(String, f64)>, Vec<(P, P)>) {
        let st = self.st;
        let faded = lighter(LOAD, 0.6);
        // What the resultants and the parts stand for, faded and unnamed.
        let askew_force = |l: &Load| match l {
            Load::Force { angle, .. } => {
                let a = angle.value.to_radians();
                askew((a.cos(), a.sin()))
            }
            _ => false,
        };
        self.loads_as(f, faded, false, &|_, l| which(l) && (matches!(l, Load::Distributed { .. }) || askew_force(l)));
        let names = load_names(st);
        let mut stations: Vec<(String, f64)> = Vec::new();
        let mut taken: Vec<(P, P)> = Vec::new();
        for r in resultants(st) {
            if !st.loads.get(r.load).is_some_and(which) {
                continue;
            }
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
                    if let Some(line) = st.loads.get(r.load).and_then(|l| st.load_line(l))
                        && let Some((member, at)) = line.locate(line.along(r.at))
                    {
                        stations.push((member.to_string(), at));
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
            taken.push((p, mul(d, -1.0)));
            if !askew(r.dir) {
                let name = self.named_arrow(p, d, len, &idx, LOAD, (0.0, -1.0));
                taken.push((p, name));
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
                taken.push((p, mul(dc, -1.0)));
                let mut i = idx.clone();
                i.push(part.to_string());
                let name = self.named_arrow(p, dc, (len * size).max(5.0), &i, LOAD, level_side);
                taken.push((p, name));
            }
        }
        (stations, taken)
    }

    /// An arrow of `len` mm acting at `p` along paper direction `d`, named `F` with the indices
    /// `idx` beyond its tail; a level one beside its middle, on the paper side `level_side`.
    /// Returns the paper direction from `p` to its name.
    fn named_arrow(&mut self, p: P, d: P, len: f64, idx: &[String], c: Rgb, level_side: P) -> P {
        let tail = add(p, mul(d, -len));
        self.pen.arrow(tail, add(p, mul(d, -0.5)), c, 0.4);
        let sub = idx.join(",");
        if d.1.abs() < 0.3 {
            // A level arrow (along a beam) is named beside it, clear of the member: by its
            // middle, or its tail's way from there when the name is wider than half the arrow,
            // so it does not reach past the point (where a member or a node's name may stand).
            let half = (Pen::subscripted_width("F", &sub) / 2.0 + 2.0).max(len / 2.0);
            let mid = add(p, mul(d, -half));
            self.pen.subscripted(add(mid, mul(level_side, 0.6)), level_side, "F", &sub, c);
            unit(add(mul(d, -half), mul(level_side, 2.0)))
        } else {
            self.pen.subscripted(add(tail, mul(d, -0.8)), mul(d, -1.0), "F", &sub, c);
            mul(d, -1.0)
        }
    }

    /// The reactions as unknowns, named as the working names them, each pointing the way it
    /// counts positive (along x, along y, along a roller's direction; a couple turning the way a
    /// rotation counts positive), as the structure's [`Positive`](crate::Positive) says.
    fn unknowns(&mut self, f: &Frame) -> Vec<(P, P)> {
        self.unknowns_where(f, &|_| true, false)
    }

    /// [`Ctx::unknowns`] of the supports at the nodes `at` picks. With `acting` (the stretches,
    /// once the reactions are known) each points the way it acts: one that came out negative the
    /// other way, as the laws take it from the internal forces on (`H_A := -H_A`).
    /// Returns each node's point with the paper directions its arrows and names take from it.
    pub(super) fn unknowns_where(&mut self, f: &Frame, at: &dyn Fn(&str) -> bool, acting: bool) -> Vec<(P, P)> {
        let mut out: Vec<(P, P)> = Vec::new();
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
            // Each component along the way it counts positive.
            let mut comps: Vec<((f64, f64), &str)> = Vec::new();
            // The reactions' size, against which a negative one is more than round-off.
            let scale = an.reactions.iter().map(|q| q.fx.abs().max(q.fy.abs())).fold(1e-12, f64::max);
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
                // The arrow stands on the side the positive way picks, as the reactions' result.
                let side = self.arrow_side(&r.node, pd(axis), below);
                // The way it acts, when asked: a negative one turned round.
                let v = r.fx * axis.0 + r.fy * axis.1;
                let axis = if acting && v < -1e-9 * scale { (-axis.0, -axis.1) } else { axis };
                let ap = pd(axis);
                taken.push(side);
                let far = add(p, mul(side, ARROW + 2.0));
                let near = add(p, mul(side, 1.0));
                if ap.0 * side.0 + ap.1 * side.1 < 0.0 {
                    self.pen.arrow(far, near, REACTION, 0.4);
                } else {
                    self.pen.arrow(near, far, REACTION, 0.4);
                }
                let (lat, ldir, extra) = self.arrow_label(&r.node, side, far, below, None);
                taken.extend(extra);
                self.pen.subscripted(lat, ldir, letter, &s, REACTION);
            }
            if matches!(r.kind, SupportKind::Fixed | SupportKind::Slider) {
                let mscale = an.reactions.iter().map(|q| q.m.abs()).fold(1e-12, f64::max);
                let ccw = if acting && r.m * pos.sr() < -1e-9 * mscale { r.m > 0.0 } else { pos.sr() > 0.0 };
                self.pen.couple(p, 5.0, ccw, REACTION, 0.4);
                let free = self.free_side_avoiding(&r.node, &taken);
                self.pen.subscripted(add(p, mul(free, 6.5)), free, "M", &s, REACTION);
                taken.push(free);
            }
            // The first is the name's own free side, not an arrow.
            out.extend(taken.into_iter().skip(1).map(|d| (p, d)));
        }
        out
    }
}
