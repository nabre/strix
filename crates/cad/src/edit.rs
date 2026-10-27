//! Editing helpers for the drawing editor: snap points, picking, window selection and moves that
//! keep a coordinate's link to the calculations (`L` moved by 0.5 becomes `L + 0.5`).

use crate::geom::{Pt, arc_points, bulge_points};
use crate::model::{Drawing, Entity, Kind, Num, P};

/// What a snap point is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapKind {
    /// An end of a line, arc or polyline segment, a polyline vertex, a point.
    End,
    /// The middle of a line or polyline segment.
    Mid,
    /// The centre of a circle or arc.
    Center,
}

/// The polylines an entity is drawn with, in drawing units (y up), for picking and highlighting.
pub fn outline(e: &Entity) -> Vec<Vec<Pt>> {
    match &e.kind {
        Kind::Line { a, b } | Kind::Dim { a, b, .. } => vec![vec![a.pt(), b.pt()]],
        Kind::Circle { c, r } => vec![arc_points(c.pt(), r.value.abs(), 0.0, 360.0)],
        Kind::Arc { c, r, start, end } => vec![arc_points(c.pt(), r.value.abs(), start.value, end.value)],
        Kind::Polyline { pts, bulges, closed } => {
            let vs: Vec<Pt> = pts.iter().map(P::pt).collect();
            let Some(&first) = vs.first() else { return Vec::new() };
            let mut out = vec![first];
            let n = vs.len();
            let segments = if *closed { n } else { n.saturating_sub(1) };
            for i in 0..segments {
                let (Some(&a), Some(&b)) = (vs.get(i), vs.get((i + 1) % n)) else { continue };
                out.extend(bulge_points(a, b, bulges.get(i).copied().unwrap_or(0.0)));
            }
            vec![out]
        }
        Kind::Point { p } | Kind::Text { p, .. } => vec![vec![p.pt()]],
    }
}

/// Distance from `p` to the segment `a`–`b`.
fn seg_dist(p: Pt, a: Pt, b: Pt) -> f64 {
    let ab = b.minus(a);
    let l2 = ab.x * ab.x + ab.y * ab.y;
    if l2 == 0.0 {
        return p.minus(a).len();
    }
    let t = (((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / l2).clamp(0.0, 1.0);
    p.minus(a.plus(ab.scale(t))).len()
}

fn mid(a: Pt, b: Pt) -> Pt {
    a.plus(b).scale(0.5)
}

impl Drawing {
    /// The points a click can snap to, in drawing units.
    pub fn snap_points(&self) -> Vec<(Pt, SnapKind)> {
        let mut out = Vec::new();
        for e in &self.entities {
            match &e.kind {
                Kind::Line { a, b } => {
                    out.push((a.pt(), SnapKind::End));
                    out.push((b.pt(), SnapKind::End));
                    out.push((mid(a.pt(), b.pt()), SnapKind::Mid));
                }
                Kind::Circle { c, .. } => out.push((c.pt(), SnapKind::Center)),
                Kind::Arc { c, r, start, end } => {
                    out.push((c.pt(), SnapKind::Center));
                    out.push((c.pt().polar(r.value.abs(), start.value), SnapKind::End));
                    out.push((c.pt().polar(r.value.abs(), end.value), SnapKind::End));
                }
                Kind::Polyline { pts, closed, .. } => {
                    let vs: Vec<Pt> = pts.iter().map(P::pt).collect();
                    out.extend(vs.iter().map(|&p| (p, SnapKind::End)));
                    let n = vs.len();
                    let segments = if *closed { n } else { n.saturating_sub(1) };
                    for i in 0..segments {
                        if let (Some(&a), Some(&b)) = (vs.get(i), vs.get((i + 1) % n)) {
                            out.push((mid(a, b), SnapKind::Mid));
                        }
                    }
                }
                Kind::Point { p } => out.push((p.pt(), SnapKind::End)),
                Kind::Text { .. } | Kind::Dim { .. } => {}
            }
        }
        out.retain(|(p, _)| p.is_finite());
        out
    }

    /// The topmost entity within `tol` drawing units of `p` (entities drawn later are on top).
    pub fn hit(&self, p: Pt, tol: f64) -> Option<usize> {
        let mut best: Option<(usize, f64)> = None;
        for (i, e) in self.entities.iter().enumerate() {
            let d = outline(e)
                .iter()
                .map(|line| match line.as_slice() {
                    [only] => only.minus(p).len(),
                    _ => line.windows(2).map(|w| seg_dist(p, w[0], w[1])).fold(f64::INFINITY, f64::min),
                })
                .fold(f64::INFINITY, f64::min);
            if d <= tol && best.is_none_or(|(_, bd)| d <= bd) {
                best = Some((i, d));
            }
        }
        best.map(|(i, _)| i)
    }

    /// The entities in the box `a`–`b`: wholly inside (`crossing` false, a left-to-right window)
    /// or touching it (`crossing` true, right to left).
    pub fn in_box(&self, a: Pt, b: Pt, crossing: bool) -> Vec<usize> {
        let (x0, x1) = (a.x.min(b.x), a.x.max(b.x));
        let (y0, y1) = (a.y.min(b.y), a.y.max(b.y));
        let inside = |p: &Pt| p.x >= x0 && p.x <= x1 && p.y >= y0 && p.y <= y1;
        self.entities
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                let pts: Vec<Pt> = outline(e).into_iter().flatten().collect();
                !pts.is_empty() && if crossing { pts.iter().any(inside) } else { pts.iter().all(inside) }
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Moves the entities at `indices` by `d` (drawing units). A coordinate written as an
    /// expression stays one: `L` becomes `L + 0.5`.
    pub fn translate(&mut self, indices: &[usize], d: Pt) {
        for &i in indices {
            if let Some(e) = self.entities.get_mut(i) {
                move_kind(&mut e.kind, d);
            }
        }
    }

    /// Copies the entities at `indices`, moved by `d`; returns the copies' indices.
    pub fn copy(&mut self, indices: &[usize], d: Pt) -> Vec<usize> {
        let mut out = Vec::new();
        for &i in indices {
            let Some(mut e) = self.entities.get(i).cloned() else { continue };
            move_kind(&mut e.kind, d);
            if self.add(e).is_ok() {
                out.push(self.entities.len() - 1);
            }
        }
        out
    }

    /// Removes the entities at `indices`.
    pub fn remove(&mut self, indices: &[usize]) {
        let mut sorted = indices.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        for &i in sorted.iter().rev() {
            if i < self.entities.len() {
                self.entities.remove(i);
            }
        }
    }
}

/// `n` moved by `d`, keeping an expression: `L` → `L + 0.5`, `L + 0.5` → `L + 1` stays readable
/// enough as `(L + 0.5) + 0.5`.
fn shift(n: &mut Num, d: f64) {
    if d == 0.0 || !d.is_finite() {
        return;
    }
    n.value += d;
    if let Some(e) = &n.expr {
        let step = format!("{}", (d.abs() * 1000.0).round() / 1000.0);
        let op = if d < 0.0 { "-" } else { "+" };
        let base = if e.contains(['+', '-']) { format!("({e})") } else { e.clone() };
        n.expr = Some(format!("{base} {op} {step}"));
    }
}

fn shift_p(p: &mut P, d: Pt) {
    shift(&mut p.0, d.x);
    shift(&mut p.1, d.y);
}

fn move_kind(k: &mut Kind, d: Pt) {
    match k {
        Kind::Line { a, b } | Kind::Dim { a, b, .. } => {
            shift_p(a, d);
            shift_p(b, d);
        }
        Kind::Circle { c, .. } | Kind::Arc { c, .. } => shift_p(c, d),
        Kind::Polyline { pts, .. } => pts.iter_mut().for_each(|p| shift_p(p, d)),
        Kind::Point { p } | Kind::Text { p, .. } => shift_p(p, d),
    }
}
