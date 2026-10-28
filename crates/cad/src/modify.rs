//! Modify commands (docs/cad.md): scale, rotate and mirror as one affine transform of the
//! drawing's points, and offset. A coordinate written as an expression stays one, transformed
//! (`L` scaled by 2 about the origin is `(L)*2`); sizes on paper (texts, dimension offsets,
//! hatch spacings) are annotative and do not change.

use crate::geom::Pt;
use crate::kernel::{Curve, canon, curves, direction};
use crate::model::{Drawing, Entity, Kind, Num, P};

/// An affine map of the plane: `p ↦ (a·x + b·y + c, d·x + e·y + f)`, with the factor it scales
/// lengths by and whether it mirrors (which turns arcs and angles the other way).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine {
    pub m: [f64; 6],
    pub scale: f64,
    pub mirror: bool,
}

impl Affine {
    /// Scaling by `k` about `base`.
    pub fn scaling(base: Pt, k: f64) -> Affine {
        Affine { m: [k, 0.0, base.x * (1.0 - k), 0.0, k, base.y * (1.0 - k)], scale: k.abs(), mirror: k < 0.0 }
    }

    /// Rotation by `deg` counter-clockwise about `base`.
    pub fn rotation(base: Pt, deg: f64) -> Affine {
        let (s, c) = deg.to_radians().sin_cos();
        Affine { m: [c, -s, base.x - c * base.x + s * base.y, s, c, base.y - s * base.x - c * base.y], scale: 1.0, mirror: false }
    }

    /// Reflection in the line through `a` and `b` (None when they are one point).
    pub fn mirror(a: Pt, b: Pt) -> Option<Affine> {
        let u = b.minus(a).unit();
        if u.len() == 0.0 {
            return None;
        }
        // Reflect: p' = a + R(p − a), R = [[cos2θ, sin2θ], [sin2θ, −cos2θ]].
        let (c2, s2) = (u.x * u.x - u.y * u.y, 2.0 * u.x * u.y);
        Some(Affine { m: [c2, s2, a.x - c2 * a.x - s2 * a.y, s2, -c2, a.y - s2 * a.x + c2 * a.y], scale: 1.0, mirror: true })
    }

    pub fn apply(&self, p: Pt) -> Pt {
        let [a, b, c, d, e, f] = self.m;
        Pt::new(a * p.x + b * p.y + c, d * p.x + e * p.y + f)
    }

    /// The direction (degrees) a direction `deg` takes.
    fn angle(&self, deg: f64) -> f64 {
        let v = Pt::new(deg.to_radians().cos(), deg.to_radians().sin());
        let [a, b, _, d, e, _] = self.m;
        crate::kernel::deg(direction(Pt::new(a * v.x + b * v.y, d * v.x + e * v.y)))
    }
}

/// A number as written in an expression: plain, to the kernel's resolution.
fn lit(v: f64) -> String {
    let v = canon(v);
    let s = format!("{v}");
    if s == "-0" { "0".into() } else { s }
}

/// `k·x + h·y + t` with the coordinates' expressions kept where there are some.
fn combine(k: f64, x: &Num, h: f64, y: &Num, t: f64) -> Num {
    let value = k * x.value + h * y.value + t;
    if x.expr.is_none() && y.expr.is_none() {
        return Num::new(value);
    }
    let term = |c: f64, n: &Num| -> Option<String> {
        if c == 0.0 {
            return None;
        }
        let text = n.expr.clone().map_or_else(|| lit(n.value), |e| format!("({e})"));
        Some(if c == 1.0 { text } else { format!("{text}*{}", lit(c)) })
    };
    let mut parts: Vec<String> = [term(k, x), term(h, y)].into_iter().flatten().collect();
    if t != 0.0 || parts.is_empty() {
        parts.push(lit(t));
    }
    Num { value, expr: Some(parts.join(" + ").replace("+ -", "- ")) }
}

fn map_p(p: &mut P, t: &Affine) {
    let [a, b, c, d, e, f] = t.m;
    let (x, y) = (p.0.clone(), p.1.clone());
    p.0 = combine(a, &x, b, &y, c);
    p.1 = combine(d, &x, e, &y, f);
}

/// `n` times `k`, an expression kept.
fn times(n: &mut Num, k: f64) {
    if k == 1.0 {
        return;
    }
    n.value *= k;
    if let Some(e) = &n.expr {
        n.expr = Some(format!("({e})*{}", lit(k)));
    }
}

impl Drawing {
    /// The entities at `indices` transformed by `t`. Sizes on paper stay; radii scale; a mirror
    /// turns arcs, polyline bulges and angular dimensions the other way.
    pub fn transform(&mut self, indices: &[usize], t: &Affine) {
        for &i in indices {
            let Some(e) = self.entities.get_mut(i) else { continue };
            match &mut e.kind {
                Kind::Line { a, b } => {
                    map_p(a, t);
                    map_p(b, t);
                }
                // The scaled part of its distance follows; the paper part stays.
                Kind::Dim { a, b, offset_units, .. } => {
                    map_p(a, t);
                    map_p(b, t);
                    *offset_units *= t.scale;
                }
                Kind::Circle { c, r } => {
                    map_p(c, t);
                    times(r, t.scale);
                }
                Kind::Arc { c, r, start, end } => {
                    map_p(c, t);
                    times(r, t.scale);
                    let (s, en) = (t.angle(start.value), t.angle(end.value));
                    // Mirrored, counter-clockwise from start to end becomes the other way round.
                    let (s, en) = if t.mirror { (en, s) } else { (s, en) };
                    *start = Num::new(s);
                    *end = Num::new(en);
                }
                Kind::Polyline { pts, bulges, .. } => {
                    pts.iter_mut().for_each(|p| map_p(p, t));
                    if t.mirror {
                        bulges.iter_mut().for_each(|b| *b = -*b);
                    }
                }
                // A level mark stays upright: its point moves, its elevation follows.
                Kind::Point { p } | Kind::Level { p, .. } => map_p(p, t),
                Kind::Text { p, angle, .. } => {
                    map_p(p, t);
                    if !t.mirror {
                        *angle = t.angle(*angle);
                    }
                }
                Kind::Angle { c, a, b, r, .. } => {
                    map_p(c, t);
                    map_p(a, t);
                    map_p(b, t);
                    *r *= t.scale;
                    if t.mirror {
                        std::mem::swap(a, b);
                    }
                }
                // A strip or a hatch follows its path or boundaries.
                Kind::Strip { .. } | Kind::Hatch { .. } => {}
            }
        }
        self.sanitize();
    }

    /// The entity at `i` offset by `dist` drawing units towards `side`: a line moved square to
    /// itself, a circle or arc concentric, a polyline with its corners mitred (its arcs stay
    /// concentric, their bulges kept). None for other entities or a degenerate result.
    pub fn offset_entity(&self, i: usize, dist: f64, side: Pt) -> Option<Entity> {
        let e = self.entities.get(i)?;
        if !(dist.is_finite() && dist > 0.0) {
            return None;
        }
        let mut out = e.clone();
        match &mut out.kind {
            Kind::Line { a, b } => {
                let (pa, pb) = (a.pt(), b.pt());
                let n = pb.minus(pa).unit().left();
                let s = if (side.minus(pa).x * n.x + side.minus(pa).y * n.y) >= 0.0 { 1.0 } else { -1.0 };
                let d = n.scale(dist * s);
                *a = P::new(pa.x + d.x, pa.y + d.y);
                *b = P::new(pb.x + d.x, pb.y + d.y);
            }
            Kind::Circle { c, r } | Kind::Arc { c, r, .. } => {
                let outside = side.minus(c.pt()).len() > r.value.abs();
                let nr = if outside { r.value.abs() + dist } else { r.value.abs() - dist };
                if nr <= crate::kernel::RESOLUTION {
                    return None;
                }
                *r = Num::new(nr);
            }
            Kind::Polyline { pts, bulges, closed } => {
                let n = pts.len();
                let vs: Vec<Pt> = pts.iter().map(P::pt).collect();
                let seg_count = crate::kernel::segments(n, *closed);
                // The polyline's own direction along a segment: an arc's bulge sign says whether
                // it runs counter-clockwise (positive) from the vertex to the next.
                let dir_sign = |k: usize| if bulges.get(k).copied().unwrap_or(0.0) < 0.0 { -1.0 } else { 1.0 };
                // The left normal of the polyline's direction at `p` on segment `k`.
                let normal_at = |c: &Curve, k: usize, p: Pt| -> Pt {
                    match *c {
                        Curve::Seg { a, b } => b.minus(a).unit().left(),
                        Curve::Circle { .. } | Curve::Arc { .. } => crate::kernel::tangent_at(c, p).left().scale(dir_sign(k)),
                    }
                };
                let segs = curves(e);
                // The side: of the nearest segment, the click's side of the polyline there.
                let (k, near) = segs
                    .iter()
                    .enumerate()
                    .map(|(k, c)| (k, c.nearest(side)))
                    .min_by(|x, y| x.1.minus(side).len().total_cmp(&y.1.minus(side).len()))?;
                let nk = normal_at(segs.get(k)?, k, near);
                let sign = if nk.x * side.minus(near).x + nk.y * side.minus(near).y >= 0.0 { 1.0 } else { -1.0 };
                let curve_of = |k: usize| -> Option<Curve> {
                    let (a, b) = (*vs.get(k)?, *vs.get((k + 1) % n)?);
                    Some(Curve::from_bulge(a, b, bulges.get(k).copied().unwrap_or(0.0)))
                };
                let mut moved = Vec::with_capacity(n);
                for v in 0..n {
                    let p = vs.get(v).copied()?;
                    let before = if v > 0 {
                        Some(v - 1)
                    } else if *closed && seg_count == n {
                        Some(n - 1)
                    } else {
                        None
                    };
                    let after = if v < seg_count { Some(v) } else { None };
                    let mut ns: Vec<Pt> = Vec::new();
                    for k in [before, after].into_iter().flatten() {
                        if let Some(c) = curve_of(k) {
                            ns.push(normal_at(&c, k, p));
                        }
                    }
                    let shift = match ns.as_slice() {
                        [a, b] => {
                            let m = a.plus(*b);
                            if m.len() < 1e-12 {
                                a.scale(dist)
                            } else {
                                let m = m.unit();
                                let cos = (m.x * a.x + m.y * a.y).max(0.25);
                                m.scale(dist / cos)
                            }
                        }
                        [a] => a.scale(dist),
                        _ => Pt::default(),
                    };
                    moved.push(p.plus(shift.scale(sign)));
                }
                if moved.iter().any(|p| !p.is_finite()) {
                    return None;
                }
                *pts = moved.iter().map(|p| P::new(p.x, p.y)).collect();
            }
            _ => return None,
        }
        out.id = String::new();
        Some(out)
    }
}
