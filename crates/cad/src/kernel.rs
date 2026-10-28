//! The geometry kernel (docs/cad.md, Precision): the exact curves of a drawing and the analytic
//! constructions the editor and the measures rely on: intersections, perpendiculars, tangents,
//! angles, circles through points or touching curves. Nothing here works on tessellated
//! outlines; tessellation is only for drawing.
//!
//! ## The limit of approximation
//!
//! - **Storage**: coordinates are kept to 1e-9 drawing units ([`RESOLUTION`]; a nanometre in a
//!   millimetre drawing), so a constructed point (an intersection, a foot, a tangent point)
//!   keeps nine decimals when saved.
//! - **Arithmetic**: `f64` (relative error 1.1e-16). Every construction uses forms that do not
//!   lose digits: `atan2` for angles (never `acos`, which loses half the digits near 0° and
//!   180°), `(r − h)(r + h)` for the half chord, the quadratic's stable root, coordinates
//!   relative to a point of the geometry (a centre, a segment's start).
//! - **Tolerance** for "same point", "tangent" and "parallel": [`tolerance`], 1e-9 of the size
//!   of the *feature* involved (a segment's length, a radius), at least [`RESOLUTION`], and never
//!   below the rounding noise of the coordinates (64 ulps of the largest). It does not grow with
//!   how far from the origin the geometry is: a 2 mm circle 5 km from the origin is as exact as
//!   one at it. Inside it two curves touch (one tangent point), outside it they cross or miss.
//! - **Angles** from saved points of a segment `L` long are right to about `2·RESOLUTION / L`
//!   radians (1.2e-9° for a 100-unit side); between exact directions to 1e-15 rad, near 180° to
//!   π's own spacing in `f64` (4.4e-16 rad). A constructed right angle (a perpendicular, a
//!   tangent to its radius) measures 90° to that precision.
//! - **Near-straight arcs**: a polyline arc whose sagitta is within the tolerance of its chord
//!   is that chord (its centre would be far beyond the drawing, its tolerance with it).

use std::f64::consts::TAU;

use crate::geom::Pt;
use crate::model::{Entity, Kind, P};

/// The finest step a saved coordinate keeps, drawing units.
pub const RESOLUTION: f64 = 1e-9;
/// Tolerance per unit of size of the feature compared.
pub const REL_TOL: f64 = 1e-9;
/// Two lines are parallel below this sine of their angle.
pub const PARALLEL_SIN: f64 = 1e-12;
/// Coordinates `reach` from the origin carry rounding noise of this many ulps of `reach`.
const NOISE_ULPS: f64 = 64.0;

/// The tolerance for a feature `feature` long (a segment's length, a radius) whose coordinates
/// reach `reach` from the origin.
pub fn tolerance(feature: f64, reach: f64) -> f64 {
    let t = (feature.abs() * REL_TOL).max(RESOLUTION).max(NOISE_ULPS * f64::EPSILON * reach.abs());
    if t.is_finite() { t } else { RESOLUTION }
}

/// A coordinate as saved: rounded to [`RESOLUTION`] below 1e6 units (above, `f64` keeps fewer
/// decimals anyway); never `-0`.
pub fn canon(v: f64) -> f64 {
    if !v.is_finite() {
        return 0.0;
    }
    // Divided by the exact power of ten: the nearest double to the rounded decimal.
    let r = if v.abs() < 1e6 { (v * 1e9).round() / 1e9 } else { v };
    if r == 0.0 { 0.0 } else { r }
}

/// An exact curve: a segment, a circle or a counter-clockwise arc (radians).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Curve {
    Seg {
        a: Pt,
        b: Pt,
    },
    Circle {
        c: Pt,
        r: f64,
    },
    /// From `start` through `sweep` (0 < sweep < 2π) counter-clockwise.
    Arc {
        c: Pt,
        r: f64,
        start: f64,
        sweep: f64,
    },
}

fn dot(a: Pt, b: Pt) -> f64 {
    a.x * b.x + a.y * b.y
}

fn cross(a: Pt, b: Pt) -> f64 {
    a.x * b.y - a.y * b.x
}

/// How far `p` is from the origin along the farther axis.
fn reach_of(p: Pt) -> f64 {
    p.x.abs().max(p.y.abs())
}

/// `a` in [0, 2π).
pub fn norm_angle(a: f64) -> f64 {
    let r = a.rem_euclid(TAU);
    if r >= TAU { 0.0 } else { r }
}

/// The direction of `v`, radians in [0, 2π).
pub fn direction(v: Pt) -> f64 {
    norm_angle(v.y.atan2(v.x))
}

/// The angle between directions `u` and `v`, radians in [0, π]: `atan2(|u×v|, u·v)`, exact to
/// rounding at every angle (`acos` of the dot product is not, near 0 and π).
pub fn angle_between(u: Pt, v: Pt) -> f64 {
    cross(u, v).abs().atan2(dot(u, v))
}

/// The counter-clockwise angle from direction `u` to `v`, radians in [0, 2π).
pub fn angle_ccw(u: Pt, v: Pt) -> f64 {
    norm_angle(cross(u, v).atan2(dot(u, v)))
}

/// The point at `angle` radians and distance `r` from `c`.
fn at_angle(c: Pt, r: f64, angle: f64) -> Pt {
    Pt::new(c.x + r * angle.cos(), c.y + r * angle.sin())
}

/// How many segments a polyline of `n` vertices has (closed: one more, back to the first; two
/// vertices closed are two segments, as DXF writes a circle).
pub fn segments(n: usize, closed: bool) -> usize {
    if closed && n >= 2 { n } else { n.saturating_sub(1) }
}

impl Curve {
    /// The size of the feature: a segment's length, a radius.
    pub fn feature(&self) -> f64 {
        match *self {
            Curve::Seg { a, b } => b.minus(a).len(),
            Curve::Circle { r, .. } | Curve::Arc { r, .. } => r.abs(),
        }
    }

    /// How far the curve reaches from the origin (for the rounding noise of its coordinates).
    pub fn reach(&self) -> f64 {
        match *self {
            Curve::Seg { a, b } => reach_of(a).max(reach_of(b)),
            Curve::Circle { c, r } | Curve::Arc { c, r, .. } => reach_of(c) + r.abs(),
        }
    }

    /// The curve's tolerance ([`tolerance`]).
    pub fn tol(&self) -> f64 {
        tolerance(self.feature(), self.reach())
    }

    /// The tolerance for a construction from point `p` on this curve.
    fn tol_with(&self, p: Pt) -> f64 {
        tolerance(self.feature(), self.reach().max(reach_of(p)))
    }

    /// The arc with DXF bulge `bulge` from `a` to `b`: a segment when straight, or so nearly
    /// straight that its sagitta is within the chord's tolerance.
    pub fn from_bulge(a: Pt, b: Pt, bulge: f64) -> Curve {
        let chord = b.minus(a);
        let d = chord.len();
        if !bulge_bends(a, b, bulge) {
            return Curve::Seg { a, b };
        }
        let bulge = bulge.clamp(-1e3, 1e3);
        let theta = 4.0 * bulge.atan();
        // Centre left of the chord for a positive bulge, d/2 · (1 − b²)/(2b) from its midpoint.
        let c = a.plus(chord.scale(0.5)).plus(chord.unit().left().scale(d / 2.0 * (1.0 - bulge * bulge) / (2.0 * bulge)));
        let r = c.minus(a).len();
        let (sa, sb) = (direction(a.minus(c)), direction(b.minus(c)));
        if theta > 0.0 {
            Curve::Arc { c, r, start: sa, sweep: theta.min(TAU) }
        } else {
            // Clockwise from a to b is counter-clockwise from b to a.
            Curve::Arc { c, r, start: sb, sweep: (-theta).min(TAU) }
        }
    }

    /// Is `p` (on the curve's circle or line) within the curve's extent, with tolerance `tol`?
    fn holds(&self, p: Pt, tol: f64) -> bool {
        match *self {
            Curve::Seg { a, b } => {
                let d = b.minus(a);
                let l2 = dot(d, d);
                if l2 == 0.0 {
                    return p.minus(a).len() <= tol;
                }
                let t = dot(p.minus(a), d) / l2;
                let slack = tol / l2.sqrt();
                (-slack..=1.0 + slack).contains(&t)
            }
            Curve::Circle { .. } => true,
            Curve::Arc { c, r, start, sweep } => {
                let t = norm_angle(direction(p.minus(c)) - start);
                let slack = if r > 0.0 { tol / r } else { 0.0 };
                t <= sweep + slack || t >= TAU - slack
            }
        }
    }

    /// The end points (none for a circle).
    pub fn ends(&self) -> Vec<Pt> {
        match *self {
            Curve::Seg { a, b } => vec![a, b],
            Curve::Circle { .. } => Vec::new(),
            Curve::Arc { c, r, start, sweep } => vec![at_angle(c, r, start), at_angle(c, r, start + sweep)],
        }
    }

    /// The midpoint (of a segment, or along an arc).
    pub fn mid(&self) -> Option<Pt> {
        match *self {
            Curve::Seg { a, b } => Some(a.plus(b).scale(0.5)),
            Curve::Circle { .. } => None,
            Curve::Arc { c, r, start, sweep } => Some(at_angle(c, r, start + sweep / 2.0)),
        }
    }

    /// The centre of a circle or arc.
    pub fn center(&self) -> Option<Pt> {
        match *self {
            Curve::Seg { .. } => None,
            Curve::Circle { c, .. } | Curve::Arc { c, .. } => Some(c),
        }
    }

    /// The quadrant points (0°, 90°, 180°, 270°) on the curve.
    pub fn quadrants(&self) -> Vec<Pt> {
        match *self {
            Curve::Seg { .. } => Vec::new(),
            Curve::Circle { c, r } | Curve::Arc { c, r, .. } => {
                let q = [Pt::new(c.x + r, c.y), Pt::new(c.x, c.y + r), Pt::new(c.x - r, c.y), Pt::new(c.x, c.y - r)];
                q.into_iter().filter(|p| self.holds(*p, self.tol())).collect()
            }
        }
    }

    /// The nearest point of the curve to `p`.
    pub fn nearest(&self, p: Pt) -> Pt {
        match *self {
            Curve::Seg { a, b } => {
                let d = b.minus(a);
                let l2 = dot(d, d);
                if l2 == 0.0 {
                    return a;
                }
                a.plus(d.scale((dot(p.minus(a), d) / l2).clamp(0.0, 1.0)))
            }
            Curve::Circle { c, r } => {
                let v = p.minus(c);
                if v.len() == 0.0 { Pt::new(c.x + r, c.y) } else { c.plus(v.unit().scale(r)) }
            }
            Curve::Arc { c, r, .. } => {
                let v = p.minus(c);
                let on = if v.len() == 0.0 { Pt::new(c.x + r, c.y) } else { c.plus(v.unit().scale(r)) };
                if self.holds(on, self.tol()) {
                    return on;
                }
                let ends = self.ends();
                ends.into_iter().min_by(|x, y| x.minus(p).len().total_cmp(&y.minus(p).len())).unwrap_or(on)
            }
        }
    }

    /// The distance from `p` to the curve.
    pub fn distance(&self, p: Pt) -> f64 {
        self.nearest(p).minus(p).len()
    }

    /// The feet of the perpendiculars from `p` on the curve (its line or circle, within the
    /// curve's extent): one on a segment, two on a circle (the near and the far side).
    pub fn perpendicular_feet(&self, p: Pt) -> Vec<Pt> {
        let tol = self.tol_with(p);
        let out = match *self {
            Curve::Seg { a, b } => {
                let d = b.minus(a);
                let l2 = dot(d, d);
                if l2 == 0.0 {
                    return Vec::new();
                }
                vec![a.plus(d.scale(dot(p.minus(a), d) / l2))]
            }
            Curve::Circle { c, r } | Curve::Arc { c, r, .. } => {
                let v = p.minus(c);
                if v.len() <= tol {
                    return Vec::new();
                }
                let u = v.unit();
                vec![c.plus(u.scale(r)), c.minus(u.scale(r))]
            }
        };
        out.into_iter().filter(|q| self.holds(*q, tol)).collect()
    }

    /// The points where lines from `p` touch the curve (a circle or arc): two from outside, `p`
    /// itself when it is on the circle (within tolerance), none from inside or for a segment.
    pub fn tangent_points(&self, p: Pt) -> Vec<Pt> {
        let (c, r) = match *self {
            Curve::Seg { .. } => return Vec::new(),
            Curve::Circle { c, r } | Curve::Arc { c, r, .. } => (c, r),
        };
        let tol = self.tol_with(p);
        let v = p.minus(c);
        let d = v.len();
        if (d - r).abs() <= tol {
            return if self.holds(p, tol) { vec![p] } else { Vec::new() };
        }
        if d < r {
            return Vec::new();
        }
        // The tangent point is seen from the centre at ±α off the direction to p, where
        // cos α = r/d; α from atan2 of the leg √((d − r)(d + r)) keeps every digit.
        let base = v.y.atan2(v.x);
        let alpha = ((d - r) * (d + r)).sqrt().atan2(r);
        [base + alpha, base - alpha].into_iter().map(|t| at_angle(c, r, t)).filter(|q| self.holds(*q, tol)).collect()
    }
}

/// Whether the polyline segment from `a` to `b` with bulge `bulge` is an arc: its sagitta
/// (`|bulge|·d/2`) is beyond the chord's tolerance.
pub fn bulge_bends(a: Pt, b: Pt, bulge: f64) -> bool {
    let d = b.minus(a).len();
    bulge != 0.0 && bulge.is_finite() && d > RESOLUTION && bulge.abs().min(1e3) * d / 2.0 > tolerance(d, reach_of(a).max(reach_of(b)))
}

/// The curves an entity is made of (texts and dimensions have none).
pub fn curves(e: &Entity) -> Vec<Curve> {
    match &e.kind {
        Kind::Line { a, b } => vec![Curve::Seg { a: a.pt(), b: b.pt() }],
        Kind::Circle { c, r } => vec![Curve::Circle { c: c.pt(), r: r.value.abs() }],
        Kind::Arc { c, r, start, end } => {
            let s = norm_angle(start.value.to_radians());
            let mut sweep = norm_angle(end.value.to_radians() - s);
            if sweep == 0.0 {
                sweep = TAU;
            }
            vec![if sweep >= TAU {
                Curve::Circle { c: c.pt(), r: r.value.abs() }
            } else {
                Curve::Arc { c: c.pt(), r: r.value.abs(), start: s, sweep }
            }]
        }
        Kind::Polyline { pts, bulges, closed } => {
            let vs: Vec<Pt> = pts.iter().map(P::pt).collect();
            let n = vs.len();
            (0..segments(n, *closed))
                .filter_map(|i| {
                    let (a, b) = (*vs.get(i)?, *vs.get((i + 1) % n)?);
                    Some(Curve::from_bulge(a, b, bulges.get(i).copied().unwrap_or(0.0)))
                })
                .collect()
        }
        Kind::Point { .. } | Kind::Text { .. } | Kind::Dim { .. } | Kind::Angle { .. } | Kind::Strip { .. } | Kind::Hatch { .. } => Vec::new(),
    }
}

/// Where two infinite lines `a1`–`a2` and `b1`–`b2` cross, with the parameters along each
/// (0 at the first point, 1 at the second); `None` when parallel within tolerance.
pub fn line_line(a1: Pt, a2: Pt, b1: Pt, b2: Pt) -> Option<(Pt, f64, f64)> {
    let (da, db) = (a2.minus(a1), b2.minus(b1));
    let (la, lb) = (da.len(), db.len());
    if la == 0.0 || lb == 0.0 {
        return None;
    }
    let den = cross(da, db);
    // Parallel when |sin θ| ≤ PARALLEL_SIN: the crossing would be farther than the
    // geometry by more than 1e12 times its size.
    if den.abs() <= la * lb * PARALLEL_SIN {
        return None;
    }
    let w = b1.minus(a1);
    let t = cross(w, db) / den;
    let u = cross(w, da) / den;
    Some((a1.plus(da.scale(t)), t, u))
}

/// Where the infinite line `p`–`q` meets the circle (`c`, `r`): the points ordered along the
/// line, one when tangent within `tol`.
fn line_circle(p: Pt, q: Pt, c: Pt, r: f64, tol: f64) -> Vec<Pt> {
    let d = q.minus(p);
    let l = d.len();
    if l == 0.0 {
        return Vec::new();
    }
    let u = d.scale(1.0 / l);
    // Centred: the foot of the perpendicular from the centre, then ± the half chord.
    let foot = p.plus(u.scale(dot(c.minus(p), u)));
    let h = foot.minus(c).len();
    if (h - r).abs() <= tol {
        return vec![if h > 0.0 { c.plus(foot.minus(c).scale(r / h)) } else { foot }];
    }
    if h > r {
        return Vec::new();
    }
    let s = ((r - h) * (r + h)).sqrt();
    vec![foot.minus(u.scale(s)), foot.plus(u.scale(s))]
}

/// Where two circles meet: two points, one when tangent (outside or inside) within `tol`, none
/// when apart, nested or concentric.
fn circle_circle(c1: Pt, r1: f64, c2: Pt, r2: f64, tol: f64) -> Vec<Pt> {
    let v = c2.minus(c1);
    let d = v.len();
    if d <= tol {
        return Vec::new();
    }
    let u = v.scale(1.0 / d);
    if (d - (r1 + r2)).abs() <= tol {
        return vec![c1.plus(u.scale(r1))];
    }
    if (d - (r1 - r2).abs()).abs() <= tol {
        let side = if r1 >= r2 { 1.0 } else { -1.0 };
        return vec![c1.plus(u.scale(side * r1))];
    }
    if d > r1 + r2 || d < (r1 - r2).abs() {
        return Vec::new();
    }
    // Along the centre line a = (d² + r1² − r2²)/(2d); across it h = √((r1 − a)(r1 + a)).
    let a = (d * d + (r1 - r2) * (r1 + r2)) / (2.0 * d);
    let h = ((r1 - a) * (r1 + a)).max(0.0).sqrt();
    let m = c1.plus(u.scale(a));
    let n = u.left();
    vec![m.plus(n.scale(h)), m.minus(n.scale(h))]
}

/// Where the full curves meet (a segment as its infinite line, an arc as its circle), with
/// tolerance `tol`.
fn meet(a: &Curve, b: &Curve, tol: f64) -> Vec<Pt> {
    let raw = match (*a, *b) {
        (Curve::Seg { a: a1, b: a2 }, Curve::Seg { a: b1, b: b2 }) => line_line(a1, a2, b1, b2).map(|(p, _, _)| p).into_iter().collect(),
        (Curve::Seg { a: p, b: q }, Curve::Circle { c, r } | Curve::Arc { c, r, .. })
        | (Curve::Circle { c, r } | Curve::Arc { c, r, .. }, Curve::Seg { a: p, b: q }) => line_circle(p, q, c, r, tol),
        (Curve::Circle { c: c1, r: r1 } | Curve::Arc { c: c1, r: r1, .. }, Curve::Circle { c: c2, r: r2 } | Curve::Arc { c: c2, r: r2, .. }) => {
            circle_circle(c1, r1, c2, r2, tol)
        }
    };
    raw.into_iter().filter(|p| p.is_finite()).collect()
}

/// The points where two curves meet, each within both curves' extents.
pub fn intersect(a: &Curve, b: &Curve) -> Vec<Pt> {
    let tol = a.tol().max(b.tol());
    meet(a, b, tol).into_iter().filter(|p| a.holds(*p, tol) && b.holds(*p, tol)).collect()
}

/// The points where curve `a`, extended (a segment as its line, an arc as its circle), meets `b`
/// within `b`'s extent: what Extend runs to. The tolerance is the curves' own.
pub fn intersect_extended(a: &Curve, b: &Curve) -> Vec<Pt> {
    let tol = a.tol().max(b.tol());
    meet(a, b, tol).into_iter().filter(|p| b.holds(*p, tol)).collect()
}

/// The angle at `vertex` from `a` to `b`, radians in [0, π].
pub fn angle_at(vertex: Pt, a: Pt, b: Pt) -> f64 {
    angle_between(a.minus(vertex), b.minus(vertex))
}

/// The direction of a curve at point `p` on it (the tangent, counter-clockwise for arcs).
pub fn tangent_at(c: &Curve, p: Pt) -> Pt {
    match *c {
        Curve::Seg { a, b } => b.minus(a).unit(),
        Curve::Circle { c, .. } | Curve::Arc { c, .. } => p.minus(c).unit().left(),
    }
}

/// Radians to degrees.
pub fn deg(rad: f64) -> f64 {
    rad.to_degrees()
}

/// The curve of a shown object nearest `p` (what a click picks for Tan, Tan…), with its
/// distance.
pub fn curve_near(d: &crate::Drawing, p: Pt) -> Option<(Curve, f64)> {
    d.entities.iter().filter(|e| d.layer(&e.layer).visible).flat_map(curves).map(|c| (c, c.distance(p))).min_by(|x, y| x.1.total_cmp(&y.1))
}

/// A constructed circle that is no real circle: no size, or beyond the drawing's limits.
fn usable(c: Pt, r: f64, tol: f64) -> bool {
    c.is_finite() && r.is_finite() && r > tol && r <= crate::MAX_COORD && reach_of(c) <= crate::MAX_COORD
}

/// The circle through `a` and `b` as a diameter: centre and radius.
pub fn circle_2p(a: Pt, b: Pt) -> Option<(Pt, f64)> {
    let (c, r) = (a.plus(b).scale(0.5), b.minus(a).len() / 2.0);
    usable(c, r, tolerance(r, reach_of(a).max(reach_of(b)))).then_some((c, r))
}

/// The circle through three points (centred on the first, so far-off points keep their
/// digits); None when they are in line, or so nearly that the circle is beyond the drawing.
pub fn circle_3p(a: Pt, b: Pt, c: Pt) -> Option<(Pt, f64)> {
    let (u, v) = (b.minus(a), c.minus(a));
    let den = 2.0 * cross(u, v);
    if den.abs() <= u.len() * v.len() * PARALLEL_SIN * 2.0 {
        return None;
    }
    let (uu, vv) = (dot(u, u), dot(v, v));
    let o = Pt::new((v.y * uu - u.y * vv) / den, (u.x * vv - v.x * uu) / den);
    let (centre, r) = (a.plus(o), o.len());
    usable(centre, r, tolerance(r, reach_of(a))).then_some((centre, r))
}

/// The point of a curve's full line or circle nearest `p`.
fn foot(c: &Curve, p: Pt) -> Pt {
    match *c {
        Curve::Seg { a, b } => {
            let d = b.minus(a);
            let l2 = dot(d, d);
            if l2 == 0.0 { a } else { a.plus(d.scale(dot(p.minus(a), d) / l2)) }
        }
        Curve::Circle { c, r } | Curve::Arc { c, r, .. } => {
            let v = p.minus(c);
            if v.len() == 0.0 { Pt::new(c.x + r, c.y) } else { c.plus(v.unit().scale(r)) }
        }
    }
}

/// Where the circle (`x`, `r`) touches curve `c`: the foot on a line; on a circle, the point of
/// the centre line that lies on both (outside, inside or around it).
fn touch_point(c: &Curve, x: Pt, r: f64) -> Pt {
    match *c {
        Curve::Seg { .. } => foot(c, x),
        Curve::Circle { c: cc, r: rho } | Curve::Arc { c: cc, r: rho, .. } => {
            let d = cc.minus(x);
            let len = d.len();
            if len == 0.0 {
                return foot(c, x);
            }
            let u = d.scale(1.0 / len);
            let (near, far) = (x.plus(u.scale(r)), x.minus(u.scale(r)));
            let off = |p: Pt| (p.minus(cc).len() - rho).abs();
            if off(near) <= off(far) { near } else { far }
        }
    }
}

/// How far the touching points of the circle (`x`, `r`) on `curves` are from their picks: the
/// circle the user means is the one touching where they picked.
fn pick_score(curves: &[(&Curve, Pt)], x: Pt, r: f64) -> f64 {
    curves.iter().map(|(c, p)| touch_point(c, x, r).minus(*p).len()).sum()
}

/// The places a centre can be to stay `r` from curve `c`: its line moved both ways (the moved
/// segment stands for its whole line in [`meet`]), or the two concentric circles.
fn offsets(c: &Curve, r: f64) -> Vec<Curve> {
    match *c {
        Curve::Seg { a, b } => {
            let d = b.minus(a);
            if d.len() == 0.0 {
                return Vec::new();
            }
            let n = d.unit().left().scale(r);
            [n, n.scale(-1.0)].into_iter().map(|s| Curve::Seg { a: a.plus(s), b: b.plus(s) }).collect()
        }
        Curve::Circle { c, r: big } | Curve::Arc { c, r: big, .. } => {
            let mut out = vec![Curve::Circle { c, r: big + r }];
            if (big - r).abs() > RESOLUTION {
                out.push(Curve::Circle { c, r: (big - r).abs() });
            }
            out
        }
    }
}

/// A circle of radius `r` touching curves `a` and `b` (AutoCAD's Tan, Tan, Radius): of the
/// possible ones, the one whose touching points are nearest the picked points `pa` and `pb`.
/// The offsets meet with the curves' own tolerance.
pub fn circle_ttr(a: &Curve, pa: Pt, b: &Curve, pb: Pt, r: f64) -> Option<(Pt, f64)> {
    let tol = a.tol().max(b.tol());
    if !(r > tol && r.is_finite()) {
        return None;
    }
    let picks = [(a, pa), (b, pb)];
    offsets(a, r)
        .iter()
        .flat_map(|oa| offsets(b, r).into_iter().flat_map(move |ob| meet(oa, &ob, tol)))
        .filter(|c| usable(*c, r, tol))
        .map(|c| (pick_score(&picks, c, r), c))
        .min_by(|x, y| x.0.total_cmp(&y.0))
        .map(|(_, c)| (c, r))
}

/// One tangency condition on a circle (centre `x`, radius `r`), in a local frame:
/// a line `n·x − k = s·r` (the circle on side `s`), or a circle `|x − c|² = (r + s·ρ)²`
/// (`s = 1` outside it; `s = −1` inside it or around it).
enum Touch {
    Line { n: Pt, k: f64, s: f64 },
    Circle { c: Pt, rho: f64, s: f64 },
}

/// `ax·x + ay·y + ar·r = rhs`.
type Linear = [f64; 4];

/// The circles touching three curves (AutoCAD's Tan, Tan, Tan; the problem of Apollonius), in
/// closed form: for every choice of side on each curve, the conditions are linear in the
/// centre and radius (a line's) or quadratic (a circle's, whose differences are linear), so the
/// circle comes from two linear equations and one quadratic. Of all the real circles, the one
/// touching nearest the three picks.
pub fn circle_ttt(curves: [(&Curve, Pt); 3]) -> Option<(Pt, f64)> {
    let tol = curves.iter().map(|(c, _)| c.tol()).fold(RESOLUTION, f64::max);
    // A local frame at the picks' middle: the arithmetic stays near the origin.
    let o = curves.iter().fold(Pt::default(), |acc, (_, p)| acc.plus(p.scale(1.0 / 3.0)));
    let size = curves.iter().map(|(c, p)| c.feature().max(p.minus(o).len())).fold(1.0, f64::max);
    let mut best: Option<(f64, Pt, f64)> = None;
    for signs in 0..8u8 {
        let touches: Vec<Touch> = curves
            .iter()
            .enumerate()
            .filter_map(|(k, (c, _))| {
                let s = if signs >> k & 1 == 0 { 1.0 } else { -1.0 };
                match **c {
                    Curve::Seg { a, b } => {
                        let n = b.minus(a).unit().left();
                        (n.len() > 0.0).then(|| Touch::Line { n, k: dot(n, a.minus(o)), s })
                    }
                    Curve::Circle { c, r } | Curve::Arc { c, r, .. } => Some(Touch::Circle { c: c.minus(o), rho: r, s }),
                }
            })
            .collect();
        if touches.len() != 3 {
            return None;
        }
        for (x, r) in apollonius(&touches, size) {
            let centre = x.plus(o);
            if !usable(centre, r, tol) {
                continue;
            }
            let score = pick_score(&curves, centre, r);
            if best.is_none_or(|(s, ..)| score < s) {
                best = Some((score, centre, r));
            }
        }
    }
    best.map(|(_, c, r)| (c, r))
}

/// The real solutions (centre, radius > 0) of three tangency conditions; `size` scales the
/// sampling of the final quadratic.
fn apollonius(touches: &[Touch], size: f64) -> Vec<(Pt, f64)> {
    let mut lin: Vec<Linear> = Vec::new();
    let mut quad: Option<(Pt, f64, f64)> = None;
    for t in touches {
        match *t {
            Touch::Line { n, k, s } => lin.push([n.x, n.y, -s, k]),
            Touch::Circle { c, rho, s } => match quad {
                // |x|² − r² − 2c·x − 2sρr + |c|² − ρ² = 0: the difference of two is linear.
                Some((c0, rho0, s0)) => lin.push([
                    -2.0 * (c.x - c0.x),
                    -2.0 * (c.y - c0.y),
                    -2.0 * (s * rho - s0 * rho0),
                    -((dot(c, c) - rho * rho) - (dot(c0, c0) - rho0 * rho0)),
                ]),
                None => quad = Some((c, rho, s)),
            },
        }
    }
    let solutions: Vec<[f64; 3]> = match (quad, lin.as_slice()) {
        (None, [l0, l1, l2]) => solve3(*l0, *l1, *l2).into_iter().collect(),
        (Some((c0, rho0, s0)), [l0, l1]) => {
            // Two unknowns from the linear pair as functions of the third (the pair with the
            // best-conditioned determinant), then the quadratic in that third.
            let Some(param) = affine_pair(*l0, *l1) else { return Vec::new() };
            let f = |t: f64| {
                let [x, y, r] = param(t);
                x * x + y * y - r * r - 2.0 * (c0.x * x + c0.y * y) - 2.0 * s0 * rho0 * r + dot(c0, c0) - rho0 * rho0
            };
            // f is exactly quadratic in t: its coefficients from three samples.
            let h = size.max(1.0);
            let (f0, fp, fm) = (f(0.0), f(h), f(-h));
            let a = (fp + fm - 2.0 * f0) / (2.0 * h * h);
            let b = (fp - fm) / (2.0 * h);
            quadratic_roots(a, b, f0).into_iter().map(param).collect()
        }
        _ => Vec::new(),
    };
    solutions
        .into_iter()
        .filter(|[x, y, r]| x.is_finite() && y.is_finite() && *r > 0.0 && r.is_finite())
        .map(|[x, y, r]| (Pt::new(x, y), r))
        .collect()
}

/// The roots of `a·t² + b·t + c` (the stable form: no cancellation between `−b` and the root).
fn quadratic_roots(a: f64, b: f64, c: f64) -> Vec<f64> {
    let scale = a.abs().max(b.abs()).max(c.abs());
    if scale == 0.0 || !scale.is_finite() {
        return Vec::new();
    }
    if a.abs() <= scale * 1e-14 {
        return if b != 0.0 { vec![-c / b] } else { Vec::new() };
    }
    let disc = b * b - 4.0 * a * c;
    if disc < -(b * b).max((4.0 * a * c).abs()) * 1e-12 {
        return Vec::new();
    }
    let sq = disc.max(0.0).sqrt();
    let q = -0.5 * (b + sq.copysign(b));
    if q == 0.0 {
        return vec![0.0];
    }
    vec![q / a, c / q]
}

/// The 3×3 system of three linear equations, by Cramer's rule; None when singular.
fn solve3(l0: Linear, l1: Linear, l2: Linear) -> Option<[f64; 3]> {
    let det = |a: [f64; 3], b: [f64; 3], c: [f64; 3]| {
        a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0])
    };
    let row = |l: Linear| [l[0], l[1], l[2]];
    let (r0, r1, r2) = (row(l0), row(l1), row(l2));
    let d = det(r0, r1, r2);
    let norm = [r0, r1, r2].iter().map(|r| r.iter().map(|v| v.abs()).fold(0.0, f64::max)).product::<f64>();
    if d.abs() <= norm * 1e-13 || !d.is_finite() {
        return None;
    }
    let with = |col: usize| {
        let swap = |r: [f64; 3], l: Linear| {
            let mut r = r;
            if let Some(v) = r.get_mut(col) {
                *v = l[3];
            }
            r
        };
        det(swap(r0, l0), swap(r1, l1), swap(r2, l2)) / d
    };
    Some([with(0), with(1), with(2)])
}

/// Two linear equations in (x, y, r) solved for two of the unknowns as affine functions of the
/// third: the returned closure gives (x, y, r) for a value of that third unknown.
fn affine_pair(l0: Linear, l1: Linear) -> Option<impl Fn(f64) -> [f64; 3]> {
    // The pair of unknowns to solve for: the one with the largest determinant, relative to the
    // rows' sizes (the free one is the other).
    let norm = |l: Linear| l[0].abs().max(l[1].abs()).max(l[2].abs());
    let scale = norm(l0) * norm(l1);
    if scale == 0.0 || !scale.is_finite() {
        return None;
    }
    let (i, j, free) = [(0usize, 1usize, 2usize), (0, 2, 1), (1, 2, 0)]
        .into_iter()
        .max_by(|p, q| (l0[p.0] * l1[p.1] - l0[p.1] * l1[p.0]).abs().total_cmp(&(l0[q.0] * l1[q.1] - l0[q.1] * l1[q.0]).abs()))?;
    let det = l0[i] * l1[j] - l0[j] * l1[i];
    if det.abs() <= scale * 1e-13 {
        return None;
    }
    Some(move |t: f64| {
        // a_i·u + a_j·v = rhs − a_free·t, for each equation.
        let (e0, e1) = (l0[3] - l0[free] * t, l1[3] - l1[free] * t);
        let u = (e0 * l1[j] - e1 * l0[j]) / det;
        let v = (l0[i] * e1 - l1[i] * e0) / det;
        let mut out = [0.0; 3];
        for (k, val) in [(i, u), (j, v), (free, t)] {
            if let Some(slot) = out.get_mut(k) {
                *slot = val;
            }
        }
        out
    })
}

/// A line touching two circles or arcs (outer and inner tangents): of the up to four, those
/// touching each curve within its extent, the one whose touching points are nearest the picked
/// points. Its two touching points.
pub fn line_tan_tan(a: &Curve, pa: Pt, b: &Curve, pb: Pt) -> Option<(Pt, Pt)> {
    let ((c1, r1), (c2, r2)) = match (*a, *b) {
        (Curve::Circle { c, r } | Curve::Arc { c, r, .. }, Curve::Circle { c: d, r: s } | Curve::Arc { c: d, r: s, .. }) => ((c, r), (d, s)),
        _ => return None,
    };
    let tol = a.tol().max(b.tol());
    let d = c2.minus(c1);
    let l = d.len();
    if l <= tol {
        return None;
    }
    let (u, w) = (d.scale(1.0 / l), d.scale(1.0 / l).left());
    let mut best: Option<(f64, (Pt, Pt))> = None;
    // A tangent line n·x = k with n·c1 − k = r1 and n·c2 − k = s·r2 (s = 1 outer, −1 inner):
    // n·d = s·r2 − r1, so n = q·u ± √(1 − q²)·w with q = (s·r2 − r1)/|d|.
    for s in [1.0, -1.0] {
        let q = (s * r2 - r1) / l;
        if q.abs() > 1.0 {
            continue;
        }
        let h = ((1.0 - q) * (1.0 + q)).sqrt();
        for sign in [1.0, -1.0] {
            let n = u.scale(q).plus(w.scale(sign * h));
            let (t1, t2) = (c1.minus(n.scale(r1)), c2.minus(n.scale(s * r2)));
            if !(a.holds(t1, tol) && b.holds(t2, tol)) {
                continue;
            }
            let score = t1.minus(pa).len() + t2.minus(pb).len();
            if best.is_none_or(|(sc, _)| score < sc) {
                best = Some((score, (t1, t2)));
            }
        }
    }
    best.map(|(_, t)| t)
}
