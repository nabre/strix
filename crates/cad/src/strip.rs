//! Hatched strips along a line or polyline (docs/cad.md, Annotations): the band `width` paper
//! mm wide on one side of the path, its corners mitred, and the hatch lines across it. The path
//! is another entity's, found by id, so the strip follows it when it moves or when the
//! calculations redraw it.

use crate::edit::outline;
use crate::geom::Pt;
use crate::kernel::{RESOLUTION, line_line};
use crate::model::{Drawing, Entity, Kind, Pattern, Side};

/// Most hatch lines one strip draws.
const MAX_HATCH_LINES: usize = 20_000;
/// Most intersection tests one strip's hatch makes (lines times the band's sides): a band too
/// finely hatched for it is hatched more coarsely.
const MAX_HATCH_WORK: usize = 4_000_000;
/// A mitre is at most this many widths long; a sharper corner is cut back to it.
const MITRE_LIMIT: f64 = 4.0;

/// A strip laid out in drawing units: the path, its offset edge, and the band between them as
/// one polygon (the path, then the edge back).
#[derive(Debug, Clone, PartialEq)]
pub struct StripShape {
    pub path: Vec<Pt>,
    pub edge: Vec<Pt>,
    pub closed: bool,
    pub polygon: Vec<Pt>,
}

/// `pts` without points on top of the one before (and, closed, without the repeated first).
fn clean(pts: &[Pt], closed: bool) -> Vec<Pt> {
    let mut out: Vec<Pt> = Vec::with_capacity(pts.len());
    for &p in pts {
        if p.is_finite() && out.last().is_none_or(|q: &Pt| q.minus(p).len() > RESOLUTION) {
            out.push(p);
        }
    }
    if closed && out.len() > 2 && out.first().zip(out.last()).is_some_and(|(a, b)| a.minus(*b).len() <= RESOLUTION) {
        out.pop();
    }
    out
}

/// The polyline `pts` moved `dist` along its left normals (negative: right), corners mitred
/// (no further than [`MITRE_LIMIT`] times `dist` from the vertex).
fn offset(pts: &[Pt], dist: f64, closed: bool) -> Vec<Pt> {
    let n = pts.len();
    let normal = |i: usize| -> Option<Pt> {
        let (a, b) = (*pts.get(i)?, *pts.get((i + 1) % n)?);
        let u = b.minus(a).unit();
        (u.len() > 0.0).then(|| u.left())
    };
    (0..n)
        .map(|i| {
            let p = pts.get(i).copied().unwrap_or_default();
            let before = if i > 0 {
                normal(i - 1)
            } else if closed {
                normal(n - 1)
            } else {
                None
            };
            let after = if i + 1 < n || closed { normal(i) } else { None };
            let shift = match (before, after) {
                (Some(a), Some(b)) => {
                    let m = a.plus(b);
                    if m.len() < 1e-12 {
                        a.scale(dist)
                    } else {
                        let m = m.unit();
                        let cos = m.x * a.x + m.y * a.y;
                        let len = (dist / cos.max(1e-12)).clamp(-MITRE_LIMIT * dist.abs(), MITRE_LIMIT * dist.abs());
                        m.scale(len)
                    }
                }
                (Some(a), None) | (None, Some(a)) => a.scale(dist),
                (None, None) => Pt::default(),
            };
            p.plus(shift)
        })
        .collect()
}

impl Drawing {
    /// The path the entity with id `along` draws, drawing units, and whether it is closed: a
    /// line, a polyline (arcs tessellated), a circle or an arc.
    pub(crate) fn path_of(&self, along: &str) -> Option<(Vec<Pt>, bool)> {
        let e = self.entities.iter().find(|e| e.id == along)?;
        let closed = match &e.kind {
            Kind::Line { .. } | Kind::Arc { .. } => false,
            Kind::Polyline { closed, .. } => *closed,
            Kind::Circle { .. } => true,
            _ => return None,
        };
        let pts = clean(outline(e).first()?, closed);
        (pts.len() >= 2).then_some((pts, closed))
    }

    /// The strip entity `e` laid out, or None when its path is gone or degenerate.
    pub fn strip_shape(&self, e: &Entity) -> Option<StripShape> {
        let Kind::Strip { along, width, side, .. } = &e.kind else { return None };
        let (path, closed) = self.path_of(along)?;
        let units = width / crate::edit::paper_per_unit(self);
        let dist = if *side == Side::Left { units } else { -units };
        let edge = offset(&path, dist, closed);
        // The band as one polygon: along the path, then back along the edge; a closed path
        // returns to its start on both, so the seam between them has no width.
        let mut polygon = path.clone();
        if closed {
            polygon.extend(path.first().copied());
            polygon.extend(edge.first().copied());
        }
        polygon.extend(edge.iter().rev().copied());
        Some(StripShape { path, edge, closed, polygon }).filter(|s| s.polygon.iter().all(|p| p.is_finite()))
    }

    /// The hatch lines of strip `e`, drawing units: a conventional pattern filling the band, or
    /// (Along) lines from the path across the band at the strip's angle to each segment,
    /// `spacing` paper mm apart (measured square to the lines), the spacing running on from one
    /// segment to the next.
    pub fn strip_lines(&self, e: &Entity) -> Vec<(Pt, Pt)> {
        let (Kind::Strip { pattern, angle, spacing, side, width, .. }, Some(shape)) = (&e.kind, self.strip_shape(e)) else { return Vec::new() };
        let k = crate::edit::paper_per_unit(self);
        // A conventional pattern fills the band like a hatch; Along draws across the path.
        if *pattern != Pattern::Along {
            let s = if *pattern == Pattern::Solid { crate::hatch::SOLID_SPACING } else { *spacing };
            return crate::hatch::fill(std::slice::from_ref(&shape.polygon), *pattern, *angle, s / k);
        }
        let towards = if *side == Side::Left { 1.0 } else { -1.0 };
        let angles: &[f64] = &[*angle];
        let n = shape.path.len();
        let segments = if shape.closed { n } else { n.saturating_sub(1) };
        let sides: Vec<(Pt, Pt)> =
            (0..shape.polygon.len()).filter_map(|i| Some((*shape.polygon.get(i)?, *shape.polygon.get((i + 1) % shape.polygon.len())?))).collect();
        let mut out = Vec::new();
        // The work is bounded before it starts: the step is widened so that the lines, and the
        // intersection tests each of them makes, stay within their budgets.
        let total: f64 = (0..segments).filter_map(|i| Some(shape.path.get((i + 1) % n)?.minus(*shape.path.get(i)?).len())).sum();
        let allowed = (MAX_HATCH_LINES / angles.len().max(1)).min(MAX_HATCH_WORK / sides.len().max(1)).max(1);
        for &deg in angles {
            let (sin, cos) = deg.to_radians().sin_cos();
            // Along the path, lines `spacing` apart square to them are spacing / sin θ apart.
            let step = ((spacing / k) / sin.abs().max(0.05)).max(total / allowed as f64);
            if !(step.is_finite() && step > 0.0) {
                continue;
            }
            let mut tries = 0usize;
            let reach = (width / k) / sin.abs().max(0.05) * 2.0;
            let mut phase = step / 2.0;
            for i in 0..segments {
                let (Some(&a), Some(&b)) = (shape.path.get(i), shape.path.get((i + 1) % n)) else { continue };
                let len = b.minus(a).len();
                let u = b.minus(a).unit();
                let dir = u.scale(cos).plus(u.left().scale(towards * sin));
                let mut t = phase;
                while t < len && out.len() < MAX_HATCH_LINES && tries <= allowed + segments {
                    tries += 1;
                    let p = a.plus(u.scale(t));
                    // The line runs across the band to the first side it meets beyond its start.
                    let far = p.plus(dir.scale(reach));
                    let hit = sides
                        .iter()
                        .filter_map(|&(c, d)| {
                            line_line(p, far, c, d).filter(|(_, s, w)| *s > 1e-9 && (-1e-9..=1.0 + 1e-9).contains(w)).map(|(q, s, _)| (s, q))
                        })
                        .min_by(|x, y| x.0.total_cmp(&y.0));
                    if let Some((_, q)) = hit {
                        out.push((p, q));
                    }
                    // Far out, a small step may round away: then the segment is done.
                    let next = t + step;
                    if next <= t {
                        break;
                    }
                    t = next;
                }
                phase = t - len;
            }
        }
        out
    }
}

impl Drawing {
    /// The line, polyline, circle or arc nearest `p` (what a strip can follow), with its distance.
    pub fn nearest_path(&self, p: Pt) -> Option<(usize, f64)> {
        self.entities
            .iter()
            .enumerate()
            .filter(|(i, e)| {
                self.selectable(*i) && matches!(e.kind, Kind::Line { .. } | Kind::Polyline { .. } | Kind::Circle { .. } | Kind::Arc { .. })
            })
            .filter_map(|(i, e)| crate::kernel::curves(e).iter().map(|c| c.distance(p)).reduce(f64::min).map(|d| (i, d)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// Which side of the path of entity `i` the point `p` is on, looking along the path.
    pub fn side_of(&self, i: usize, p: Pt) -> Side {
        let id = self.entities.get(i).map(|e| e.id.clone()).unwrap_or_default();
        let Some((path, closed)) = self.path_of(&id) else { return Side::Left };
        let n = path.len();
        let segments = if closed { n } else { n.saturating_sub(1) };
        let nearest = (0..segments)
            .filter_map(|k| {
                let (a, b) = (*path.get(k)?, *path.get((k + 1) % n)?);
                let seg = crate::kernel::Curve::Seg { a, b };
                Some((seg.distance(p), a, b))
            })
            .min_by(|x, y| x.0.total_cmp(&y.0));
        match nearest {
            Some((_, a, b)) => {
                let (u, v) = (b.minus(a), p.minus(a));
                if u.x * v.y - u.y * v.x >= 0.0 { Side::Left } else { Side::Right }
            }
            None => Side::Left,
        }
    }
}
