//! Editing helpers for the drawing editor: snap points, picking, window selection and moves that
//! keep a coordinate's link to the calculations (`L` moved by 0.5 becomes `L + 0.5`).

use crate::geom::{Pt, arc_points, bulge_points};
use crate::kernel::{Curve, curves, intersect};
use crate::model::{Drawing, Entity, Kind, Num, Orient, P};

/// What a snap point is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapKind {
    /// An end of a line, arc or polyline segment, a polyline vertex, a point.
    End,
    /// The middle of a line or polyline segment.
    Mid,
    /// The centre of a circle or arc.
    Center,
    /// A point entity.
    Node,
    /// A circle's or arc's point at 0°, 90°, 180° or 270°.
    Quadrant,
    /// Where two curves cross (or touch).
    Intersection,
    /// The foot of the perpendicular from the previous point.
    Perpendicular,
    /// Where a line from the previous point touches a circle or arc.
    Tangent,
    /// The nearest point on a curve.
    Nearest,
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

/// Which object snaps are on (the prototype's defaults: all but Quadrant).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snaps {
    pub end: bool,
    pub mid: bool,
    pub center: bool,
    pub node: bool,
    pub quadrant: bool,
    pub intersection: bool,
    pub perpendicular: bool,
    pub tangent: bool,
    pub nearest: bool,
}

impl Default for Snaps {
    fn default() -> Self {
        Snaps {
            end: true,
            mid: true,
            center: true,
            node: true,
            quadrant: false,
            intersection: true,
            perpendicular: true,
            tangent: true,
            nearest: true,
        }
    }
}

impl Snaps {
    pub fn on(&self, k: SnapKind) -> bool {
        match k {
            SnapKind::End => self.end,
            SnapKind::Mid => self.mid,
            SnapKind::Center => self.center,
            SnapKind::Node => self.node,
            SnapKind::Quadrant => self.quadrant,
            SnapKind::Intersection => self.intersection,
            SnapKind::Perpendicular => self.perpendicular,
            SnapKind::Tangent => self.tangent,
            SnapKind::Nearest => self.nearest,
        }
    }
}

impl SnapKind {
    /// Lower wins at equal distance: a node or an end before an intersection, before a centre…
    fn priority(self) -> u8 {
        match self {
            SnapKind::Node => 0,
            SnapKind::End => 1,
            SnapKind::Intersection => 2,
            SnapKind::Center => 3,
            SnapKind::Quadrant | SnapKind::Mid => 4,
            SnapKind::Perpendicular => 5,
            SnapKind::Tangent => 6,
            SnapKind::Nearest => 7,
        }
    }

    /// All kinds, in priority order.
    pub const ALL: [SnapKind; 9] = [
        SnapKind::Node,
        SnapKind::End,
        SnapKind::Intersection,
        SnapKind::Center,
        SnapKind::Quadrant,
        SnapKind::Mid,
        SnapKind::Perpendicular,
        SnapKind::Tangent,
        SnapKind::Nearest,
    ];
}

/// Curves considered for one snap: the nearest ones only, so the pairwise intersections stay cheap.
const SNAP_CURVES: usize = 40;

impl Drawing {
    /// The snap point within `tol` drawing units of pointer `p`, exact (computed on the curves,
    /// not on their drawn outlines): the nearest, ties to the higher priority. Perpendicular and
    /// tangent are from `from`, the previous point. Nearest applies only when nothing else does.
    pub fn snap(&self, p: Pt, tol: f64, from: Option<Pt>, on: &Snaps) -> Option<(Pt, SnapKind)> {
        if !p.is_finite() || tol.is_nan() || tol <= 0.0 {
            return None;
        }
        let mut near: Vec<(f64, Curve)> = Vec::new();
        let mut cands: Vec<(Pt, SnapKind)> = Vec::new();
        for e in &self.entities {
            if let Kind::Point { p: q } = &e.kind {
                cands.push((q.pt(), SnapKind::Node));
            }
            for c in curves(e) {
                let d = c.distance(p);
                // A circle's centre snaps from inside it too.
                let centre_near = c.center().is_some_and(|k| k.minus(p).len() <= tol);
                if d <= 2.0 * tol || centre_near {
                    near.push((d, c));
                }
            }
        }
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        near.truncate(SNAP_CURVES);
        for (_, c) in &near {
            cands.extend(c.ends().into_iter().map(|q| (q, SnapKind::End)));
            cands.extend(c.mid().map(|q| (q, SnapKind::Mid)));
            cands.extend(c.center().map(|q| (q, SnapKind::Center)));
            cands.extend(c.quadrants().into_iter().map(|q| (q, SnapKind::Quadrant)));
            if let Some(f) = from {
                cands.extend(c.perpendicular_feet(f).into_iter().map(|q| (q, SnapKind::Perpendicular)));
                cands.extend(c.tangent_points(f).into_iter().map(|q| (q, SnapKind::Tangent)));
            }
        }
        for (i, (_, a)) in near.iter().enumerate() {
            for (_, b) in near.iter().skip(i + 1) {
                cands.extend(intersect(a, b).into_iter().map(|q| (q, SnapKind::Intersection)));
            }
        }
        let found: Vec<(f64, Pt, SnapKind)> = cands
            .into_iter()
            .filter(|(q, k)| on.on(*k) && q.is_finite())
            .map(|(q, k)| (q.minus(p).len(), q, k))
            .filter(|(d, _, _)| *d <= tol)
            .collect();
        // The nearest; among those within a hair of it, the higher priority (an end over the
        // intersection at the same place).
        let dmin = found.iter().map(|f| f.0).fold(f64::INFINITY, f64::min);
        let best =
            found.into_iter().filter(|f| f.0 <= dmin + tol * 1e-6).min_by(|a, b| a.2.priority().cmp(&b.2.priority()).then(a.0.total_cmp(&b.0)));
        if let Some((_, q, k)) = best {
            return Some((q, k));
        }
        if on.nearest {
            return near.first().filter(|(d, _)| *d <= tol).map(|(_, c)| (c.nearest(p), SnapKind::Nearest));
        }
        None
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
        // To the kernel's resolution: a point taken to a snap stays exactly there.
        let step = format!("{}", crate::kernel::canon(d.abs()));
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

/// A grip: a point of an entity that can be taken to change it (AutoCAD and Allplan grips).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grip {
    /// A line's or dimension's first or second point.
    A,
    B,
    /// A line's midpoint: moves the whole line.
    Mid,
    /// A circle's or arc's centre: moves it.
    Center,
    /// A circle's quadrant (0–3): changes the radius.
    Quadrant(u8),
    /// An arc's start or end: changes that angle; its middle changes the radius.
    ArcStart,
    ArcEnd,
    ArcMid,
    /// A polyline's vertex, or the middle of its segment `i` (a straight segment moves with its
    /// two vertices; an arc segment bends through the point).
    Vertex(usize),
    Segment(usize),
    /// A point's or text's position.
    Point,
    /// A dimension's line: changes its distance from the measured points.
    DimLine,
}

/// The paper mm a drawing unit measures.
fn paper_per_unit(d: &Drawing) -> f64 {
    let k = d.unit.mm() / d.scale;
    if k.is_finite() && k > 0.0 { k } else { 1.0 }
}

/// A dimension's direction for its orientation.
fn dim_dir(a: Pt, b: Pt, orient: Orient) -> Pt {
    match orient {
        Orient::Aligned => b.minus(a).unit(),
        Orient::Horizontal => Pt::new(if b.x >= a.x { 1.0 } else { -1.0 }, 0.0),
        Orient::Vertical => Pt::new(0.0, if b.y >= a.y { 1.0 } else { -1.0 }),
    }
}

/// The paper offset that puts the line of a dimension from `a` to `b` through `at`: past the
/// farther measured point along the normal.
pub fn dim_offset(d: &Drawing, a: Pt, b: Pt, orient: Orient, at: Pt) -> f64 {
    let n = dim_dir(a, b, orient).left();
    let level = |q: Pt| q.x * n.x + q.y * n.y;
    let (na, nb, nl) = (level(a), level(b), level(at));
    let units = if nl >= na.max(nb) { nl - na.max(nb) } else { nl - na.min(nb) };
    (units * paper_per_unit(d)).clamp(-500.0, 500.0)
}

impl Drawing {
    /// The grips of entity `i`, with where they are.
    pub fn grips(&self, i: usize) -> Vec<(Grip, Pt)> {
        let Some(e) = self.entities.get(i) else { return Vec::new() };
        match &e.kind {
            Kind::Line { a, b } => vec![(Grip::A, a.pt()), (Grip::B, b.pt()), (Grip::Mid, mid(a.pt(), b.pt()))],
            Kind::Circle { c, r } => {
                let (c, r) = (c.pt(), r.value.abs());
                let mut out = vec![(Grip::Center, c)];
                out.extend((0..4u8).map(|k| (Grip::Quadrant(k), c.polar(r, f64::from(k) * 90.0))));
                out
            }
            Kind::Arc { c, r, start, end } => {
                let (c, r) = (c.pt(), r.value.abs());
                let mut sweep = (end.value - start.value).rem_euclid(360.0);
                if sweep == 0.0 {
                    sweep = 360.0;
                }
                vec![
                    (Grip::Center, c),
                    (Grip::ArcStart, c.polar(r, start.value)),
                    (Grip::ArcEnd, c.polar(r, end.value)),
                    (Grip::ArcMid, c.polar(r, start.value + sweep / 2.0)),
                ]
            }
            Kind::Polyline { pts, bulges, closed } => {
                let vs: Vec<Pt> = pts.iter().map(P::pt).collect();
                let mut out: Vec<(Grip, Pt)> = vs.iter().enumerate().map(|(k, &p)| (Grip::Vertex(k), p)).collect();
                let n = vs.len();
                let segments = if *closed && n > 2 { n } else { n.saturating_sub(1) };
                for k in 0..segments {
                    let (Some(&a), Some(&b)) = (vs.get(k), vs.get((k + 1) % n)) else { continue };
                    if let Some(m) = Curve::from_bulge(a, b, bulges.get(k).copied().unwrap_or(0.0)).mid() {
                        out.push((Grip::Segment(k), m));
                    }
                }
                out
            }
            Kind::Point { p } | Kind::Text { p, .. } => vec![(Grip::Point, p.pt())],
            Kind::Dim { a, b, offset, orient, .. } => {
                let (pa, pb) = (a.pt(), b.pt());
                let n = dim_dir(pa, pb, *orient).left();
                let level = |q: Pt| q.x * n.x + q.y * n.y;
                let units = offset / paper_per_unit(self);
                let at = if units >= 0.0 { level(pa).max(level(pb)) + units } else { level(pa).min(level(pb)) + units };
                let m = mid(pa, pb);
                vec![(Grip::A, pa), (Grip::B, pb), (Grip::DimLine, m.plus(n.scale(at - level(m))))]
            }
        }
    }

    /// Entity `i` with grip `g` taken to `to`. A coordinate written as an expression stays one
    /// (`L` dragged by 0.5 becomes `L + 0.5`); a size the drag sets anew (a radius, an angle,
    /// a bulge, a dimension's distance) becomes a number. Returns false, changing nothing, when
    /// the grip is not the entity's or the result would be degenerate (a zero radius, a
    /// polyline vertex on its neighbour).
    pub fn move_grip(&mut self, i: usize, g: Grip, to: Pt) -> bool {
        if !to.is_finite() {
            return false;
        }
        let Some(from) = self.grips(i).into_iter().find(|(k, _)| *k == g).map(|(_, p)| p) else { return false };
        let d = to.minus(from);
        let offset_at = match self.entities.get(i).map(|e| &e.kind) {
            Some(Kind::Dim { a, b, orient, .. }) => dim_offset(self, a.pt(), b.pt(), *orient, to),
            _ => 0.0,
        };
        let Some(e) = self.entities.get_mut(i) else { return false };
        let tiny = crate::kernel::RESOLUTION;
        match (&mut e.kind, g) {
            (Kind::Line { a, .. } | Kind::Dim { a, .. }, Grip::A) => shift_p(a, d),
            (Kind::Line { b, .. } | Kind::Dim { b, .. }, Grip::B) => shift_p(b, d),
            (Kind::Line { a, b }, Grip::Mid) => {
                shift_p(a, d);
                shift_p(b, d);
            }
            (Kind::Circle { c, .. } | Kind::Arc { c, .. }, Grip::Center) => shift_p(c, d),
            (Kind::Circle { c, r }, Grip::Quadrant(_)) | (Kind::Arc { c, r, .. }, Grip::ArcMid) => {
                let nr = to.minus(c.pt()).len();
                if nr <= tiny {
                    return false;
                }
                *r = Num::new(nr);
            }
            (Kind::Arc { c, start, .. }, Grip::ArcStart) => *start = Num::new(crate::kernel::deg(crate::kernel::direction(to.minus(c.pt())))),
            (Kind::Arc { c, end, .. }, Grip::ArcEnd) => *end = Num::new(crate::kernel::deg(crate::kernel::direction(to.minus(c.pt())))),
            (Kind::Polyline { pts, closed, .. }, Grip::Vertex(v)) => {
                let n = pts.len();
                let prev = if v > 0 {
                    Some(v - 1)
                } else if *closed {
                    n.checked_sub(1)
                } else {
                    None
                };
                let next = if v + 1 < n {
                    Some(v + 1)
                } else if *closed {
                    Some(0)
                } else {
                    None
                };
                let clash =
                    [prev, next].into_iter().flatten().filter(|&j| j != v).any(|j| pts.get(j).is_some_and(|q| q.pt().minus(to).len() <= tiny));
                if clash {
                    return false;
                }
                let Some(p) = pts.get_mut(v) else { return false };
                shift_p(p, d);
            }
            (Kind::Polyline { pts, bulges, .. }, Grip::Segment(s)) => {
                let n = pts.len().max(1);
                let (Some(a), Some(b)) = (pts.get(s).map(P::pt), pts.get((s + 1) % n).map(P::pt)) else { return false };
                if bulges.get(s).copied().unwrap_or(0.0) == 0.0 {
                    for j in [s, (s + 1) % n] {
                        if let Some(p) = pts.get_mut(j) {
                            shift_p(p, d);
                        }
                    }
                } else {
                    // The arc bends through the point: bulge = sagitta / half chord, positive
                    // when the arc bulges to the right of the chord (counter-clockwise from a).
                    let chord = b.minus(a);
                    let len = chord.len();
                    if len <= tiny {
                        return false;
                    }
                    let left = (chord.x * (to.y - a.y) - chord.y * (to.x - a.x)) / len;
                    let nb = -2.0 * left / len;
                    if !nb.is_finite() || nb.abs() <= 1e-12 {
                        return false;
                    }
                    if bulges.len() <= s {
                        bulges.resize(s + 1, 0.0);
                    }
                    if let Some(slot) = bulges.get_mut(s) {
                        *slot = nb.clamp(-1e3, 1e3);
                    }
                }
            }
            (Kind::Point { p } | Kind::Text { p, .. }, Grip::Point) => shift_p(p, d),
            (Kind::Dim { offset, .. }, Grip::DimLine) => *offset = offset_at,
            _ => return false,
        }
        true
    }
}
