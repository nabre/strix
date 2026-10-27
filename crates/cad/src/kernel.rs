//! The geometry kernel (docs/cad.md, Precision): the exact curves of a drawing and the analytic
//! constructions the editor and the measures rely on: intersections, perpendiculars, tangents,
//! angles. Nothing here works on tessellated outlines; tessellation is only for drawing.
//!
//! ## The limit of approximation
//!
//! - **Storage**: coordinates are kept to 1e-9 drawing units ([`RESOLUTION`]; a nanometre in a
//!   millimetre drawing), so a constructed point (an intersection, a foot, a tangent point)
//!   keeps nine decimals when saved.
//! - **Arithmetic**: `f64` (relative error 1.1e-16). Every construction uses forms that do not
//!   lose digits: `atan2` for angles (never `acos`, which loses half the digits near 0° and
//!   180°), `(r − h)(r + h)` for the half chord, the quadratic's stable root, centred
//!   coordinates.
//! - **Tolerance** for "same point", "tangent" and "parallel": [`tolerance`], 1e-9 of the size
//!   of the geometry involved (at least [`RESOLUTION`]): a hundred times above the error the
//!   saved coordinates can carry, a million times below anything drawn. Inside it two curves
//!   touch (one tangent point), outside it they cross or miss.
//! - **Angles** from saved points of a segment `L` long are right to about `2·RESOLUTION / L`
//!   radians (1.2e-9° for a 100-unit side); between exact directions to 1e-15 rad, near 180° to
//!   π's own spacing in `f64` (4.4e-16 rad). A constructed right angle (a perpendicular, a
//!   tangent to its radius) measures 90° to that precision.

use std::f64::consts::{PI, TAU};

use crate::geom::Pt;
use crate::model::{Entity, Kind, P};

/// The finest step a saved coordinate keeps, drawing units.
pub const RESOLUTION: f64 = 1e-9;
/// Tolerance per unit of size of the geometry compared.
pub const REL_TOL: f64 = 1e-9;
/// Two lines are parallel below this sine of their angle.
pub const PARALLEL_SIN: f64 = 1e-12;

/// The tolerance for geometry of size `size` (drawing units).
pub fn tolerance(size: f64) -> f64 {
    (size.abs() * REL_TOL).max(RESOLUTION)
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

impl Curve {
    /// A size for the tolerance: the extent of the curve.
    pub fn size(&self) -> f64 {
        match *self {
            Curve::Seg { a, b } => a.x.abs().max(a.y.abs()).max(b.x.abs()).max(b.y.abs()).max(b.minus(a).len()),
            Curve::Circle { c, r } | Curve::Arc { c, r, .. } => c.x.abs().max(c.y.abs()) + r.abs(),
        }
    }

    /// The arc with DXF bulge `bulge` from `a` to `b` (a segment when straight).
    pub fn from_bulge(a: Pt, b: Pt, bulge: f64) -> Curve {
        let chord = b.minus(a);
        let d = chord.len();
        if bulge == 0.0 || !bulge.is_finite() || d <= RESOLUTION {
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
            Curve::Arc { c, r, start, sweep } => vec![c.polar(r, start.to_degrees()), c.polar(r, (start + sweep).to_degrees())],
        }
    }

    /// The midpoint (of a segment, or along an arc).
    pub fn mid(&self) -> Option<Pt> {
        match *self {
            Curve::Seg { a, b } => Some(a.plus(b).scale(0.5)),
            Curve::Circle { .. } => None,
            Curve::Arc { c, r, start, sweep } => Some(c.polar(r, (start + sweep / 2.0).to_degrees())),
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
                q.into_iter().filter(|p| self.holds(*p, tolerance(self.size()))).collect()
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
                if self.holds(on, tolerance(self.size())) {
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
        let tol = tolerance(self.size().max(p.x.abs()).max(p.y.abs()));
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
        let tol = tolerance(self.size().max(p.x.abs()).max(p.y.abs()));
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
        [base + alpha, base - alpha].into_iter().map(|t| Pt::new(c.x + r * t.cos(), c.y + r * t.sin())).filter(|q| self.holds(*q, tol)).collect()
    }
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
            let segments = if *closed && n > 2 { n } else { n.saturating_sub(1) };
            (0..segments)
                .filter_map(|i| {
                    let (a, b) = (*vs.get(i)?, *vs.get((i + 1) % n)?);
                    Some(Curve::from_bulge(a, b, bulges.get(i).copied().unwrap_or(0.0)))
                })
                .collect()
        }
        Kind::Point { .. } | Kind::Text { .. } | Kind::Dim { .. } => Vec::new(),
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

/// Where line `p`–`q` meets the circle (`c`, `r`): the points ordered along the line, one when
/// tangent within `tol`.
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

/// The points where two curves meet, each within both curves' extents.
pub fn intersect(a: &Curve, b: &Curve) -> Vec<Pt> {
    let tol = tolerance(a.size().max(b.size()));
    let raw = match (*a, *b) {
        (Curve::Seg { a: a1, b: a2 }, Curve::Seg { a: b1, b: b2 }) => line_line(a1, a2, b1, b2).map(|(p, _, _)| p).into_iter().collect(),
        (Curve::Seg { a: p, b: q }, Curve::Circle { c, r } | Curve::Arc { c, r, .. })
        | (Curve::Circle { c, r } | Curve::Arc { c, r, .. }, Curve::Seg { a: p, b: q }) => line_circle(p, q, c, r, tol),
        (Curve::Circle { c: c1, r: r1 } | Curve::Arc { c: c1, r: r1, .. }, Curve::Circle { c: c2, r: r2 } | Curve::Arc { c: c2, r: r2, .. }) => {
            circle_circle(c1, r1, c2, r2, tol)
        }
    };
    raw.into_iter().filter(|p| p.is_finite() && a.holds(*p, tol) && b.holds(*p, tol)).collect()
}

/// The points where curve `a`, extended (a segment as its line, an arc as its circle), meets `b`
/// within `b`'s extent: what Extend runs to.
pub fn intersect_extended(a: &Curve, b: &Curve) -> Vec<Pt> {
    let tol = tolerance(a.size().max(b.size()));
    let full = match *a {
        Curve::Seg { a: p, b: q } => {
            let d = q.minus(p).unit().scale(1e9_f64.min(1e3 * a.size().max(b.size()).max(1.0)));
            Curve::Seg { a: p.minus(d), b: q.plus(d) }
        }
        Curve::Circle { c, r } | Curve::Arc { c, r, .. } => Curve::Circle { c, r },
    };
    intersect(&full, b).into_iter().filter(|p| b.holds(*p, tol)).collect()
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
    rad * 180.0 / PI
}
