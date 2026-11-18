//! What the drawing measures: the structure's dimensions (chains of distances along x below it
//! and along y beside it, between its nodes and the points where loads start, end or act),
//! inclined members' slopes and the angles inclined forces make with their members.

use strix_plot::PlotPrim;

use super::{Ctx, Frame, GREY, P, Pen, Rgb, TEXT, add, mul, pd, unit};
use crate::model::{Load, LoadAxes, Structure};

/// Dimension lines' colour.
pub(super) const DIM: Rgb = [0x55, 0x55, 0x55];
/// Distance from the structure's lowest point (or its supports) to the first dimension line.
const DIM_BELOW: f64 = 12.0;
/// Between two dimension lines.
const DIM_STEP: f64 = 6.5;
/// From the structure's rightmost point to the vertical dimension line.
pub(super) const DIM_RIGHT: f64 = 10.0;
/// Angles within this many degrees of 0°, 90° or 180° are not written (they are evident).
const SQUARE_TOL: f64 = 0.5;

impl Pen {
    /// `text` centred on `at`, turned `angle` degrees counter-clockwise.
    pub(super) fn turned(&mut self, at: P, text: String, height: f64, angle: f64, c: Rgb) {
        self.prims.push(PlotPrim::TextAt { at, text, height, angle, color: c });
    }

    /// An arc round `c` of radius `r` from paper angle `a0` to `a1` (radians, y down).
    pub(super) fn arc(&mut self, c: P, r: f64, a0: f64, a1: f64, col: Rgb, w: f64) {
        let n = (((a1 - a0).abs() / 0.12).ceil() as usize).clamp(2, 64);
        let pts: Vec<P> = (0..=n)
            .map(|k| {
                let t = a0 + (a1 - a0) * k as f64 / n as f64;
                (c.0 + r * t.cos(), c.1 + r * t.sin())
            })
            .collect();
        self.poly(pts, col, w);
    }
}

/// The angle (degrees counter-clockwise on the page) that makes text run along paper direction
/// `d`, kept readable (between −90° and 90°).
pub(super) fn reading_angle(d: P) -> f64 {
    let mut a = -d.1.atan2(d.0).to_degrees();
    if !a.is_finite() {
        return 0.0;
    }
    while a > 90.0 + 1e-9 {
        a -= 180.0;
    }
    while a <= -90.0 + 1e-9 {
        a += 180.0;
    }
    a
}

/// The box (`[x0, y0, x1, y1]`) of `text` centred on `at` and turned `angle` degrees.
pub(super) fn turned_box(at: P, text: &str, height: f64, angle: f64) -> [f64; 4] {
    let w = text.chars().count() as f64 * height * 0.55;
    let h = height * 1.1;
    let (s, c) = angle.to_radians().sin_cos();
    let (hx, hy) = (c.abs() * w / 2.0 + s.abs() * h / 2.0, s.abs() * w / 2.0 + c.abs() * h / 2.0);
    [at.0 - hx, at.1 - hy, at.0 + hx, at.1 + hy]
}

/// Sorted distinct values (within `tol`).
fn distinct(mut v: Vec<f64>, tol: f64) -> Vec<f64> {
    v.retain(|x| x.is_finite());
    v.sort_by(f64::total_cmp);
    v.dedup_by(|a, b| (*a - *b).abs() <= tol);
    v
}

/// The acute-or-right angle between two lines (degrees, 0…90).
fn line_angle(a: P, b: P) -> f64 {
    let c = (a.0 * b.0 + a.1 * b.1).abs().clamp(0.0, 1.0);
    c.acos().to_degrees()
}

fn square(deg: f64) -> bool {
    deg <= SQUARE_TOL || (deg - 90.0).abs() <= SQUARE_TOL || (deg - 180.0).abs() <= SQUARE_TOL
}

/// What the dimensions measure.
struct Measures {
    /// The members' nodes.
    nodes: Vec<(f64, f64)>,
    /// Where loads act, start and end on horizontal members (measured along x) and on vertical
    /// ones (along y).
    on_level: Vec<(f64, f64)>,
    on_plumb: Vec<(f64, f64)>,
    /// Inclined members with loads: the member and the distances along it from its first node.
    inclined: Vec<(String, Vec<f64>)>,
}

/// Whether a member is level (`Some(true)`), plumb (`Some(false)`) or neither.
fn bearing(st: &Structure, member: &str) -> Option<bool> {
    let m = st.member(member)?;
    let (na, nb) = (st.node(&m.a)?, st.node(&m.b)?);
    let (dx, dy) = (nb.x.value - na.x.value, nb.y.value - na.y.value);
    let len = dx.hypot(dy);
    if len <= 0.0 {
        return None;
    }
    let deg = line_angle((dx / len, dy / len), (1.0, 0.0));
    if deg <= SQUARE_TOL {
        Some(true)
    } else if (deg - 90.0).abs() <= SQUARE_TOL {
        Some(false)
    } else {
        None
    }
}

fn measures(st: &Structure) -> Measures {
    measures_with(st, &[])
}

/// [`measures`] with more points along members (`(member, distance from its first node)`).
fn measures_with(st: &Structure, extra: &[(String, f64)]) -> Measures {
    let nodes: Vec<(f64, f64)> =
        st.nodes.iter().filter(|n| st.members.iter().any(|m| m.a == n.id || m.b == n.id)).map(|n| (n.x.value, n.y.value)).collect();
    let mut out = Measures { nodes, on_level: Vec::new(), on_plumb: Vec::new(), inclined: Vec::new() };
    let add_at = |out: &mut Measures, member: &str, at: f64| {
        let Some(m) = st.member(member) else { return };
        let (Some(na), Some(nb)) = (st.node(&m.a), st.node(&m.b)) else { return };
        let Some(len) = st.length(m).filter(|l| *l > 0.0) else { return };
        let at = at.clamp(0.0, len);
        let t = at / len;
        let p = (na.x.value + (nb.x.value - na.x.value) * t, na.y.value + (nb.y.value - na.y.value) * t);
        match bearing(st, member) {
            Some(true) => out.on_level.push(p),
            Some(false) => out.on_plumb.push(p),
            None => {
                if at > 1e-9 * len && at < len * (1.0 - 1e-9) {
                    match out.inclined.iter_mut().find(|(id, _)| id == member) {
                        Some((_, ats)) => ats.push(at),
                        None => out.inclined.push((member.to_string(), vec![at])),
                    }
                }
            }
        }
    };
    for l in &st.loads {
        match l {
            Load::Force { member: Some(m), at, .. } | Load::Moment { member: Some(m), at, .. } => add_at(&mut out, m, at.value),
            Load::Distributed { member, from, to, .. } => {
                add_at(&mut out, member, from.value);
                if let Some(len) = st.member(member).and_then(|m| st.length(m)) {
                    add_at(&mut out, member, to.as_ref().map_or(len, |t| t.value));
                }
            }
            _ => {}
        }
    }
    for (m, at) in extra {
        add_at(&mut out, m, *at);
    }
    out
}

/// The chains of x below the structure, nearest first: with the loads on level members (when
/// they add stations), between the nodes, and overall.
fn x_rows(ms: &Measures, tol: f64) -> Vec<Vec<f64>> {
    let main = distinct(ms.nodes.iter().map(|p| p.0).collect(), tol);
    let detail = distinct(ms.nodes.iter().chain(&ms.on_level).map(|p| p.0).collect(), tol);
    let mut rows = Vec::new();
    if detail.len() > main.len() {
        rows.push(detail);
    }
    if main.len() >= 2 {
        if let (Some(&a), Some(&b)) = (main.first(), main.last())
            && main.len() > 2
        {
            rows.push(main);
            rows.push(vec![a, b]);
        } else {
            rows.push(main);
        }
    }
    rows
}

fn tolerance(ms: &Measures) -> f64 {
    1e-6 * (ms.nodes.iter().fold(0.0_f64, |a, p| a.max(p.0.abs()).max(p.1.abs())) + 1.0)
}

/// The heights (along y) the chain at the structure's right measures.
fn y_stops(ms: &Measures, tol: f64) -> Vec<f64> {
    distinct(ms.nodes.iter().chain(&ms.on_plumb).map(|p| p.1).collect(), tol)
}

/// The distances the drawing's dimensions give: the chains along x (nearest the structure first,
/// each a row of stations whose neighbours are dimensioned) and the chain along y.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Chains {
    pub x: Vec<Vec<f64>>,
    pub y: Vec<Vec<f64>>,
    /// Two stations closer than this are the same.
    pub tol: f64,
}

/// The chains [`Ctx::dimensions`] draws (whether or not the drawing shows them).
pub fn chains(st: &Structure) -> Chains {
    let ms = measures(st);
    if ms.nodes.len() < 2 {
        return Chains::default();
    }
    let tol = tolerance(&ms);
    let ys = y_stops(&ms, tol);
    Chains { x: x_rows(&ms, tol), y: if ys.len() >= 2 { vec![ys] } else { Vec::new() }, tol }
}

impl Ctx<'_> {
    /// Chains of distances below the structure (x) and at its right (y), and along each
    /// inclined member that carries loads.
    pub(super) fn dimensions(&mut self, f: &Frame) {
        if !self.st.diagrams.dimensions {
            return;
        }
        self.dimensions_with(f, &[]);
    }

    /// [`Self::dimensions`] (whatever the structure's setting) with stations at more points along
    /// members (`(member, distance from its first node)`): the free body's resultants.
    pub(super) fn dimensions_with(&mut self, f: &Frame, extra: &[(String, f64)]) {
        let ms = measures_with(self.st, extra);
        if ms.nodes.len() < 2 {
            return;
        }
        let tol = tolerance(&ms);
        let all: Vec<(f64, f64)> = ms.nodes.iter().chain(&ms.on_level).chain(&ms.on_plumb).copied().collect();
        let ymin = all.iter().map(|p| p.1).fold(f64::MAX, f64::min);
        let xmax = all.iter().map(|p| p.0).fold(f64::MIN, f64::max);
        let lu = self.st.length_unit.symbol();
        let label = |cx: &Self, d: f64| format!("{} {lu}", cx.num(d));

        // Along x, below everything.
        let base = f.p(0.0, ymin).1 + DIM_BELOW;
        let lowest = |x: f64| all.iter().filter(|p| (p.0 - x).abs() <= tol).map(|p| f.p(p.0, p.1).1).fold(f64::MIN, f64::max);
        for (r, row) in x_rows(&ms, tol).iter().enumerate() {
            let y = base + DIM_STEP * r as f64;
            for &x in row {
                let px = f.p(x, 0.0).0;
                let from = if r == 0 { lowest(x) + 1.5 } else { base + 1.0 };
                self.pen.line((px, from.min(y - 0.5)), (px, y + 1.2), GREY, 0.15);
                self.pen.line((px - 0.9, y + 0.9), (px + 0.9, y - 0.9), DIM, 0.3);
            }
            if let (Some(&a), Some(&b)) = (row.first(), row.last()) {
                self.pen.line((f.p(a, 0.0).0, y), (f.p(b, 0.0).0, y), DIM, 0.2);
            }
            for w in row.windows(2) {
                let (pa, pb) = (f.p(w[0], 0.0).0, f.p(w[1], 0.0).0);
                let text = label(self, w[1] - w[0]);
                let width = text.chars().count() as f64 * TEXT * 0.55;
                // A narrow stretch has its text below the line.
                let at = if width + 1.0 < pb - pa { ((pa + pb) / 2.0, y - 1.6) } else { ((pa + pb) / 2.0, y + 2.0) };
                self.pen.turned(at, text, TEXT, 0.0, DIM);
            }
        }
        // Along y, at the right.
        let ys = y_stops(&ms, tol);
        if ys.len() >= 2 {
            let x = f.p(xmax, 0.0).0 + DIM_RIGHT;
            let rightmost = |y: f64| all.iter().filter(|p| (p.1 - y).abs() <= tol).map(|p| f.p(p.0, p.1).0).fold(f64::MIN, f64::max);
            for &yy in &ys {
                let py = f.p(0.0, yy).1;
                self.pen.line((rightmost(yy) + 1.5, py), (x + 1.2, py), GREY, 0.15);
                self.pen.line((x - 0.9, py + 0.9), (x + 0.9, py - 0.9), DIM, 0.3);
            }
            if let (Some(&a), Some(&b)) = (ys.first(), ys.last()) {
                self.pen.line((x, f.p(0.0, a).1), (x, f.p(0.0, b).1), DIM, 0.2);
            }
            for w in ys.windows(2) {
                let (pa, pb) = (f.p(0.0, w[0]).1, f.p(0.0, w[1]).1);
                let text = label(self, w[1] - w[0]);
                let width = text.chars().count() as f64 * TEXT * 0.55;
                let gap = if width + 1.0 < (pa - pb).abs() { -1.6 } else { 2.0 };
                self.pen.turned((x + gap, (pa + pb) / 2.0), text, TEXT, 90.0, DIM);
            }
        }
        // Along inclined members, on their reference fibres' side: distances from the first node.
        for (id, ats) in &ms.inclined {
            let st = self.st;
            let Some(m) = st.member(id) else { continue };
            let (Some(na), Some(nb)) = (st.node(&m.a), st.node(&m.b)) else { continue };
            let Some(len) = st.length(m).filter(|l| *l > 0.0) else { continue };
            let (c, s) = ((nb.x.value - na.x.value) / len, (nb.y.value - na.y.value) / len);
            let mut stops = ats.clone();
            stops.push(0.0);
            stops.push(len);
            let stops = distinct(stops, tol);
            let u = pd((c, s));
            let side = pd((s, -c));
            let off = 4.5;
            let on = |t: f64| f.p(na.x.value + c * t, na.y.value + s * t);
            let at = |t: f64, k: f64| add(on(t), mul(side, k));
            for &t in &stops {
                self.pen.line(at(t, 1.0), at(t, off + 1.2), GREY, 0.15);
                let tick = (u.0 * 0.64 - side.0 * 0.64, u.1 * 0.64 - side.1 * 0.64);
                let mid = at(t, off);
                self.pen.line(add(mid, mul(tick, -1.0)), add(mid, tick), DIM, 0.3);
            }
            if let (Some(&a), Some(&b)) = (stops.first(), stops.last()) {
                self.pen.line(at(a, off), at(b, off), DIM, 0.2);
            }
            let angle = reading_angle(u);
            for w in stops.windows(2) {
                let text = label(self, w[1] - w[0]);
                let width = text.chars().count() as f64 * TEXT * 0.55;
                let room = (w[1] - w[0]) * f.k;
                let k = if width + 1.0 < room { off + 1.8 } else { off + 4.2 };
                let mid = (w[0] + w[1]) / 2.0;
                self.pen.turned(at(mid, k), text, TEXT, angle, DIM);
            }
        }
    }

    /// Each inclined member's slope: an arc from the horizontal at its lower end, with the angle.
    pub(super) fn slopes(&mut self, f: &Frame) {
        if !self.st.diagrams.dimensions {
            return;
        }
        let st = self.st;
        // Each slope once: members as steep as one already marked are evident from the chains.
        let mut marked: Vec<f64> = Vec::new();
        for m in &st.members {
            let (Some(na), Some(nb)) = (st.node(&m.a), st.node(&m.b)) else { continue };
            let (dx, dy) = (nb.x.value - na.x.value, nb.y.value - na.y.value);
            let len = dx.hypot(dy);
            if len <= 0.0 {
                continue;
            }
            let deg = line_angle((dx / len, dy / len), (1.0, 0.0));
            if square(deg) || marked.iter().any(|d| (d - deg).abs() < 0.01) {
                continue;
            }
            marked.push(deg);
            // From the lower end, towards the other.
            let (low, high) = if na.y.value <= nb.y.value { (na, nb) } else { (nb, na) };
            let p = f.p(low.x.value, low.y.value);
            let q = f.p(high.x.value, high.y.value);
            let along = unit((q.0 - p.0, q.1 - p.1));
            let horiz = (along.0.signum(), 0.0);
            let r = 7.0_f64.min(0.3 * len * f.k);
            let text = format!("{}°", self.num(deg));
            self.angle_dimension(p, horiz, along, r, text);
        }
    }

    /// The angle each inclined concentrated force (and each inclined distributed load) makes
    /// with its member: an arc between the member and the force's line, with the angle.
    pub(super) fn force_angles(&mut self, f: &Frame) {
        if !self.st.diagrams.dimensions {
            return;
        }
        let st = self.st;
        for l in &st.loads {
            match l {
                Load::Force { node, member, at, value, angle } => {
                    if value.value == 0.0 {
                        continue;
                    }
                    let Some(p) = self.point_of(f, node.as_deref(), member.as_deref(), at.value) else { continue };
                    let a = angle.value.to_radians();
                    // From the point towards the arrow's tail.
                    let tail = mul(pd((a.cos(), a.sin())), -value.value.signum());
                    let halves = self.half_lines(f, node.as_deref(), member.as_deref(), at.value);
                    self.angle_mark(p, tail, &halves, 6.0);
                }
                Load::Distributed { member, q1, q2, from, angle, axes, .. } => {
                    if *axes == LoadAxes::Local && square(angle.value.rem_euclid(180.0)) {
                        continue;
                    }
                    let Some(m) = st.member(member) else { continue };
                    let (Some(na), Some(nb)) = (st.node(&m.a), st.node(&m.b)) else { continue };
                    let Some(len) = st.length(m).filter(|l| *l > 0.0) else { continue };
                    let (c, s) = ((nb.x.value - na.x.value) / len, (nb.y.value - na.y.value) / len);
                    let th = angle.value.to_radians();
                    let g = match axes {
                        LoadAxes::Global => (th.cos(), th.sin()),
                        LoadAxes::Local => (c * th.cos() - s * th.sin(), s * th.cos() + c * th.sin()),
                    };
                    let x0 = from.value.clamp(0.0, len);
                    let p = f.p(na.x.value + c * x0, na.y.value + s * x0);
                    let q = if q1.value != 0.0 { q1.value } else { q2.as_ref().map_or(1.0, |q| q.value) };
                    let tail = mul(pd(g), if q < 0.0 { 1.0 } else { -1.0 });
                    // The member's side the load lies along, from its start.
                    let fwd = pd((c, s));
                    self.angle_mark(p, tail, &[fwd], 4.5);
                }
                Load::Moment { .. } => {}
            }
        }
    }

    /// The paper directions of the members leaving the point a load acts at.
    fn half_lines(&self, f: &Frame, node: Option<&str>, member: Option<&str>, at: f64) -> Vec<P> {
        let st = self.st;
        let mut out = Vec::new();
        if let Some(id) = member {
            let Some(m) = st.member(id) else { return out };
            let (Some(na), Some(nb)) = (st.node(&m.a), st.node(&m.b)) else { return out };
            let (pa, pb) = (f.p(na.x.value, na.y.value), f.p(nb.x.value, nb.y.value));
            let d = unit((pb.0 - pa.0, pb.1 - pa.1));
            let len = st.length(m).unwrap_or(0.0);
            if at < len - 1e-9 {
                out.push(d);
            }
            if at > 1e-9 {
                out.push(mul(d, -1.0));
            }
            return out;
        }
        let Some(id) = node else { return out };
        let Some(n) = st.node(id) else { return out };
        for m in &st.members {
            let other = if m.a == id {
                st.node(&m.b)
            } else if m.b == id {
                st.node(&m.a)
            } else {
                None
            };
            if let Some(o) = other {
                out.push(unit(pd((o.x.value - n.x.value, o.y.value - n.y.value))));
            }
        }
        out
    }

    /// An arc at `p` from the half-line nearest `tail` to `tail`, with the angle between them,
    /// unless the force runs along or across a member there (evident).
    fn angle_mark(&mut self, p: P, tail: P, halves: &[P], r: f64) {
        if halves.is_empty() {
            return;
        }
        if halves.iter().any(|h| square(line_angle(*h, tail))) {
            return;
        }
        let Some(h) = halves.iter().copied().min_by(|a, b| {
            let da = (a.0 * tail.0 + a.1 * tail.1).clamp(-1.0, 1.0).acos();
            let db = (b.0 * tail.0 + b.1 * tail.1).clamp(-1.0, 1.0).acos();
            da.total_cmp(&db)
        }) else {
            return;
        };
        let deg = (h.0 * tail.0 + h.1 * tail.1).clamp(-1.0, 1.0).acos().to_degrees();
        let text = format!("{}°", self.num(deg));
        self.angle_dimension(p, h, tail, r, text);
    }

    /// An angle drawn as the dimensions are: a thin extension line along `from` (the reference:
    /// the horizontal, a member), an arc of radius `r` round `p` from `from` to `to` in the
    /// dimension lines' colour with an oblique tick at each end, and the angle beside it.
    fn angle_dimension(&mut self, p: P, from: P, to: P, r: f64, text: String) {
        let (a0, a1) = (from.1.atan2(from.0), to.1.atan2(to.0));
        let a1 = if (a1 - a0).abs() > std::f64::consts::PI { a1 + std::f64::consts::TAU * (a0 - a1).signum() } else { a1 };
        self.pen.line(add(p, mul(from, 1.0)), add(p, mul(from, r + 1.2)), GREY, 0.15);
        self.pen.arc(p, r, a0, a1, DIM, 0.2);
        for a in [a0, a1] {
            let (radial, tangent) = ((a.cos(), a.sin()), (-a.sin(), a.cos()));
            let mid = add(p, mul(radial, r));
            let tick = mul(add(radial, tangent), 0.64);
            self.pen.line(add(mid, mul(tick, -1.0)), add(mid, tick), DIM, 0.3);
        }
        let mid = (a0 + a1) / 2.0;
        let at = add(p, mul((mid.cos(), mid.sin()), r + 3.2));
        self.pen.turned(at, text, TEXT, 0.0, DIM);
    }
}
