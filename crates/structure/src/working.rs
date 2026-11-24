//! What a written working needs: the loads as resultants in the structure's axes, and the laws
//! of the internal forces stretch by stretch as polynomials.

use crate::diagram::Action;
use crate::linalg::{Mat, solve};
use crate::model::{Load, LoadAxes, LoadPer, Structure};
use crate::solve::MemberResult;

/// What a resultant comes from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ResultantKind {
    /// A concentrated force of `value` along the resultant's direction.
    Force { value: f64 },
    /// A distributed load: `q` over `length` (the member's length loaded, or its projection
    /// across the load for a load per projection); with `triangle`, half of `q·length` (the
    /// triangular part of a trapezoid).
    Spread { q: f64, length: f64, triangle: bool },
    /// A couple (counter-clockwise positive).
    Couple { value: f64 },
}

/// A load as a resultant at a point, in the structure's axes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Resultant {
    /// Index of the load in [`Structure::loads`].
    pub load: usize,
    pub at: (f64, f64),
    /// Unit direction of a force (a couple's is unused).
    pub dir: (f64, f64),
    pub kind: ResultantKind,
    /// A distributed load's loaded stretch, from the end where a triangle's intensity is zero
    /// (its centroid is two thirds of the way); a concentrated load's point twice.
    pub span: ((f64, f64), (f64, f64)),
}

impl Resultant {
    /// The force (or couple) the resultant amounts to, signed along `dir`.
    pub fn value(&self) -> f64 {
        match self.kind {
            ResultantKind::Force { value } | ResultantKind::Couple { value } => value,
            ResultantKind::Spread { q, length, triangle } => q * length * if triangle { 0.5 } else { 1.0 },
        }
    }
}

/// The structure's loads as resultants (its numbers resolved): a trapezoid as a rectangle and a
/// triangle, as texts work them out.
pub fn resultants(st: &Structure) -> Vec<Resultant> {
    let mut out = Vec::new();
    let geometry = |member: &str| -> Option<((f64, f64), (f64, f64), f64)> {
        let m = st.member(member)?;
        let (a, b) = (st.node(&m.a)?, st.node(&m.b)?);
        let len = st.length(m)?;
        if len <= 0.0 {
            return None;
        }
        Some(((a.x.value, a.y.value), ((b.x.value - a.x.value) / len, (b.y.value - a.y.value) / len), len))
    };
    for (k, l) in st.loads.iter().enumerate() {
        match l {
            Load::Force { node, member, at, value, .. } | Load::Moment { node, member, at, value } if node.is_some() || member.is_some() => {
                let point = match (node, member) {
                    (Some(n), None) => st.node(n).map(|n| (n.x.value, n.y.value)),
                    (_, Some(m)) => geometry(m).map(|(a, d, len)| {
                        let s = at.value.clamp(0.0, len);
                        (a.0 + d.0 * s, a.1 + d.1 * s)
                    }),
                    _ => None,
                };
                let Some(p) = point else { continue };
                if let Load::Force { angle, .. } = l {
                    let th = angle.value.to_radians();
                    out.push(Resultant {
                        load: k,
                        at: p,
                        dir: (th.cos(), th.sin()),
                        kind: ResultantKind::Force { value: value.value },
                        span: (p, p),
                    });
                } else {
                    out.push(Resultant { load: k, at: p, dir: (0.0, 0.0), kind: ResultantKind::Couple { value: value.value }, span: (p, p) });
                }
            }
            Load::Distributed { member, then, q1, q2, from, to, angle, axes, per } => {
                // Its line: the member's, or the aligned members' it goes on over.
                let Some(line) = st.spread_line(member, then) else { continue };
                let (a, d, len) = (line.start, line.dir, line.len);
                let s0 = from.value.clamp(0.0, len);
                let s1 = to.as_ref().map_or(len, |t| t.value).clamp(0.0, len);
                let (s0, s1) = if s0 <= s1 { (s0, s1) } else { (s1, s0) };
                if s1 - s0 <= 0.0 {
                    continue;
                }
                let th = angle.value.to_radians();
                let dir = match axes {
                    LoadAxes::Global => (th.cos(), th.sin()),
                    LoadAxes::Local => (d.0 * th.cos() - d.1 * th.sin(), d.1 * th.cos() + d.0 * th.sin()),
                };
                // Per projection: the member's length across the load.
                let across = (d.0 * dir.1 - d.1 * dir.0).abs();
                let factor = match per {
                    LoadPer::Length => 1.0,
                    LoadPer::Projection => across,
                };
                let length = (s1 - s0) * factor;
                let (q1, q2) = (q1.value, q2.as_ref().map_or(q1.value, |q| q.value));
                let point = |s: f64| (a.0 + d.0 * s, a.1 + d.1 * s);
                let span = (point(s0), point(s1));
                if q1 != 0.0 {
                    out.push(Resultant {
                        load: k,
                        at: point((s0 + s1) / 2.0),
                        dir,
                        kind: ResultantKind::Spread { q: q1, length, triangle: false },
                        span,
                    });
                }
                if q2 != q1 {
                    out.push(Resultant {
                        load: k,
                        at: point(s0 + (s1 - s0) * 2.0 / 3.0),
                        dir,
                        kind: ResultantKind::Spread { q: q2 - q1, length, triangle: true },
                        span,
                    });
                }
            }
            _ => {}
        }
    }
    out
}

/// One stretch of a member: the laws `c0 + c1·x + c2·x² + c3·x³` of N, V and M, `x` from the
/// member's `a` end, valid for `x0 ≤ x ≤ x1`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stretch {
    pub x0: f64,
    pub x1: f64,
    pub n: [f64; 4],
    pub v: [f64; 4],
    pub m: [f64; 4],
}

/// The laws of member `m`'s internal forces, stretch by stretch (exact: within a stretch they are
/// polynomials of degree 3 at most).
pub fn stretches(m: &MemberResult) -> Vec<Stretch> {
    let bps = m.breakpoints();
    let mut out = Vec::new();
    for w in bps.windows(2) {
        let (x0, x1) = (w[0], w[1]);
        let span = x1 - x0;
        if span <= 1e-12 * (1.0 + m.length) {
            continue;
        }
        let xs: Vec<f64> = [0.1, 0.37, 0.63, 0.9].iter().map(|t| x0 + span * t).collect();
        let fit = |a: Action| -> [f64; 4] {
            let mut mat = Mat::zeros(4, 4);
            let mut rhs = vec![0.0; 4];
            for (i, &x) in xs.iter().enumerate() {
                for j in 0..4 {
                    mat.add(i, j, x.powi(j as i32));
                }
                if let Some(r) = rhs.get_mut(i) {
                    *r = a.of(m.forces(x, true));
                }
            }
            let c = solve(&mat, &rhs, 1e-14).unwrap_or_else(|| vec![0.0; 4]);
            // Round-off: a term far below the law's size at the stretch's end is zero.
            let scale = xs.iter().map(|&x| a.of(m.forces(x, true)).abs()).fold(0.0_f64, f64::max).max(1e-12);
            let xm = x1.abs().max(x0.abs()).max(1e-9);
            let mut out = [0.0; 4];
            for (k, slot) in out.iter_mut().enumerate() {
                let v = c.get(k).copied().unwrap_or(0.0);
                *slot = if (v * xm.powi(k as i32)).abs() <= 1e-9 * scale { 0.0 } else { v };
            }
            out
        };
        out.push(Stretch { x0, x1, n: fit(Action::Axial), v: fit(Action::Shear), m: fit(Action::Moment) });
    }
    out
}
