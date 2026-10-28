//! Trim, Extend, Fillet and Chamfer (docs/cad.md). The other objects shown are the cutting and
//! boundary edges, as in AutoCAD's quick mode: Trim takes away the piece picked, between the
//! nearest crossings on either side (the whole object when nothing crosses it); Extend runs the
//! end nearer the pick to the first object it meets. Fillet and Chamfer join two lines with an
//! arc or a bevel, trimming or extending both to it. The ends an edit leaves where they were keep
//! their expressions; the ends it makes are numbers.

use std::f64::consts::{PI, TAU};

use crate::geom::Pt;
use crate::kernel::{Curve, bulge_bends, canon, curves, deg, direction, intersect, intersect_extended, line_line, norm_angle, segments};
use crate::model::{Drawing, Entity, Kind, Num, P};

/// Parameters closer than this are one place (a crossing at an end is the end).
const EPS: f64 = 1e-9;

/// A stretch of an object along its own direction: a segment, or an arc from `start` through
/// `sweep` radians (negative: clockwise, as a polyline arc with a negative bulge).
#[derive(Debug, Clone, Copy)]
enum Span {
    Seg { a: Pt, b: Pt },
    Arc { c: Pt, r: f64, start: f64, sweep: f64 },
}

fn dot(a: Pt, b: Pt) -> f64 {
    a.x * b.x + a.y * b.y
}

/// A constructed point as saved (to the kernel's resolution).
fn pt(p: Pt) -> P {
    P::new(canon(p.x), canon(p.y))
}

impl Span {
    /// The point at parameter `s` (0 at the start, 1 at the end; beyond them on the extension).
    fn at(&self, s: f64) -> Pt {
        match *self {
            Span::Seg { a, b } => a.plus(b.minus(a).scale(s)),
            Span::Arc { c, r, start, sweep } => {
                let t = start + sweep * s;
                Pt::new(c.x + r * t.cos(), c.y + r * t.sin())
            }
        }
    }

    /// The parameter of `p` (on the span's line or circle): a segment's is unbounded; an arc's is
    /// the turn from its start in its own direction, in [0, 2π/|sweep|).
    fn param(&self, p: Pt) -> f64 {
        match *self {
            Span::Seg { a, b } => {
                let d = b.minus(a);
                let l2 = dot(d, d);
                if l2 == 0.0 { 0.0 } else { dot(p.minus(a), d) / l2 }
            }
            Span::Arc { c, start, sweep, .. } => {
                if sweep == 0.0 {
                    return 0.0;
                }
                let t = direction(p.minus(c));
                let turn = if sweep > 0.0 { norm_angle(t - start) } else { norm_angle(start - t) };
                turn / sweep.abs()
            }
        }
    }

    /// How far the parameter can go before an arc comes back to its start.
    fn full(&self) -> f64 {
        match *self {
            Span::Seg { .. } => f64::INFINITY,
            Span::Arc { sweep, .. } => {
                if sweep == 0.0 {
                    1.0
                } else {
                    TAU / sweep.abs()
                }
            }
        }
    }

    /// The exact curve (an arc counter-clockwise, as the kernel keeps them).
    fn curve(&self) -> Curve {
        match *self {
            Span::Seg { a, b } => Curve::Seg { a, b },
            Span::Arc { c, r, start, sweep } => {
                if sweep.abs() >= TAU - EPS {
                    Curve::Circle { c, r }
                } else if sweep > 0.0 {
                    Curve::Arc { c, r, start: norm_angle(start), sweep }
                } else {
                    Curve::Arc { c, r, start: norm_angle(start + sweep), sweep: -sweep }
                }
            }
        }
    }

    /// The curve extended: a segment as its line (the kernel's extended intersection treats it
    /// so), an arc as its circle.
    fn extended(&self) -> Curve {
        match *self {
            Span::Seg { a, b } => Curve::Seg { a, b },
            Span::Arc { c, r, .. } => Curve::Circle { c, r },
        }
    }

    /// The DXF bulge of the part of this span `fraction` of it long.
    fn bulge(&self, fraction: f64) -> f64 {
        match *self {
            Span::Seg { .. } => 0.0,
            Span::Arc { sweep, .. } => (sweep * fraction / 4.0).tan(),
        }
    }
}

/// The spans of a line, circle, arc or polyline, in its own direction, and whether it closes.
fn spans(e: &Entity) -> Option<(Vec<Span>, bool)> {
    let out = match &e.kind {
        Kind::Line { a, b } => (vec![Span::Seg { a: a.pt(), b: b.pt() }], false),
        Kind::Circle { c, r } => (vec![Span::Arc { c: c.pt(), r: r.value.abs(), start: 0.0, sweep: TAU }], true),
        Kind::Arc { c, r, start, end } => {
            let s = norm_angle(start.value.to_radians());
            let mut sweep = norm_angle(end.value.to_radians() - s);
            if sweep == 0.0 {
                sweep = TAU;
            }
            (vec![Span::Arc { c: c.pt(), r: r.value.abs(), start: s, sweep }], sweep >= TAU)
        }
        Kind::Polyline { pts, bulges, closed } => {
            let vs: Vec<Pt> = pts.iter().map(P::pt).collect();
            let n = vs.len();
            let list: Vec<Span> = (0..segments(n, *closed))
                .filter_map(|k| {
                    let (a, b) = (*vs.get(k)?, *vs.get((k + 1) % n)?);
                    let bulge = bulges.get(k).copied().unwrap_or(0.0);
                    if !bulge_bends(a, b, bulge) {
                        return Some(Span::Seg { a, b });
                    }
                    let bulge = bulge.clamp(-1e3, 1e3);
                    let sweep = 4.0 * bulge.atan();
                    let chord = b.minus(a);
                    let d = chord.len();
                    let c = a.plus(chord.scale(0.5)).plus(chord.unit().left().scale(d / 2.0 * (1.0 - bulge * bulge) / (2.0 * bulge)));
                    Some(Span::Arc { c, r: c.minus(a).len(), start: direction(a.minus(c)), sweep })
                })
                .collect();
            (list, *closed && n >= 2)
        }
        _ => return None,
    };
    if out.0.is_empty() || out.0.iter().any(|s| !s.at(0.0).is_finite()) { None } else { Some(out) }
}

/// Where `p` is along the spans: the span index plus the parameter on it.
fn locate(spans: &[Span], p: Pt) -> f64 {
    spans
        .iter()
        .enumerate()
        .map(|(k, s)| {
            let q = s.curve().nearest(p);
            (k as f64 + s.param(q).clamp(0.0, 1.0), q.minus(p).len())
        })
        .min_by(|x, y| x.1.total_cmp(&y.1))
        .map_or(0.0, |(u, _)| u)
}

impl Drawing {
    /// The curves of the objects shown other than entity `i`: the cutting and boundary edges.
    fn edges(&self, i: usize) -> Vec<Curve> {
        self.entities.iter().enumerate().filter(|(j, e)| *j != i && self.layer(&e.layer).visible).flat_map(|(_, e)| curves(e)).collect()
    }

    /// An id no entity has.
    fn fresh_id(&self) -> String {
        let mut n = self.entities.len() + 1;
        while self.entities.iter().any(|x| x.id == format!("e{n}")) {
            n += 1;
        }
        format!("e{n}")
    }

    /// Trims entity `i` (a line, circle, arc or polyline) at the piece picked at `pick`: what
    /// lies between the nearest crossings with the other objects shown on either side of it goes;
    /// with nothing crossing it, the object goes. A circle needs two crossings and becomes an
    /// arc, a closed polyline becomes an open one; a piece out of the middle leaves two objects
    /// (the second gets a new id, the first keeps the object's).
    pub fn trim(&mut self, i: usize, pick: Pt) -> Result<(), String> {
        let e = self.entities.get(i).cloned().ok_or("There is no object there")?;
        let (sp, closed) = spans(&e).ok_or("Trim works on lines, circles, arcs and polylines")?;
        let n = sp.len() as f64;
        let up = locate(&sp, pick);
        let edges = self.edges(i);
        let mut cuts: Vec<f64> = Vec::new();
        for (k, s) in sp.iter().enumerate() {
            let c = s.curve();
            for edge in &edges {
                for p in intersect(&c, edge) {
                    cuts.push(k as f64 + s.param(p).clamp(0.0, 1.0));
                }
            }
        }
        cuts.retain(|u| u.is_finite());
        cuts.sort_by(f64::total_cmp);
        cuts.dedup_by(|a, b| (*a - *b).abs() <= EPS);
        let pieces: Vec<(f64, f64)> = if closed {
            // Periodic: a closing seam is no cut.
            if let (Some(&first), Some(&last)) = (cuts.first(), cuts.last())
                && cuts.len() > 1
                && first <= EPS
                && last >= n - EPS
            {
                cuts.pop();
            }
            match cuts.len() {
                0 => Vec::new(),
                1 => return Err("A closed object is trimmed between two crossings".into()),
                _ => {
                    let lo = cuts.iter().rev().find(|&&u| u < up).or(cuts.last()).copied().unwrap_or(0.0);
                    let hi = cuts.iter().find(|&&u| u > up).or(cuts.first()).copied().unwrap_or(0.0);
                    // What stays runs from the crossing after the pick round to the one before it.
                    vec![(hi, if lo > hi { lo } else { lo + n })]
                }
            }
        } else {
            cuts.retain(|&u| u > EPS && u < n - EPS);
            let lo = cuts.iter().rev().find(|&&u| u < up).copied();
            let hi = cuts.iter().copied().find(|&u| u > up);
            [lo.map(|l| (0.0, l)), hi.map(|h| (h, n))].into_iter().flatten().collect()
        };
        let kinds: Vec<Kind> = pieces.iter().filter_map(|&(u0, u1)| piece(&e, &sp, closed, u0, u1)).collect();
        let mut kinds = kinds.into_iter();
        let Some(first) = kinds.next() else {
            self.remove(&[i]);
            return Ok(());
        };
        if let Some(obj) = self.entities.get_mut(i) {
            obj.kind = first;
        }
        for (k, kind) in kinds.enumerate() {
            if self.entities.len() >= crate::MAX_ENTITIES {
                return Err(format!("a drawing holds at most {} entities", crate::MAX_ENTITIES));
            }
            let mut more = e.clone();
            more.id = self.fresh_id();
            more.kind = kind;
            self.entities.insert((i + 1 + k).min(self.entities.len()), more);
        }
        self.sanitize();
        Ok(())
    }

    /// Extends entity `i` (a line, arc or open polyline) from its end nearer `pick` to the
    /// first object shown it meets on its way: a line along itself, an arc round its circle.
    pub fn extend(&mut self, i: usize, pick: Pt) -> Result<(), String> {
        let e = self.entities.get(i).cloned().ok_or("There is no object there")?;
        let (sp, closed) = spans(&e).ok_or("Extend works on lines, arcs and open polylines")?;
        if closed {
            return Err("A closed object has no end to extend".into());
        }
        let (first, last) = (*sp.first().ok_or("Nothing to extend")?, *sp.last().ok_or("Nothing to extend")?);
        let at_end = pick.minus(last.at(1.0)).len() <= pick.minus(first.at(0.0)).len();
        let span = if at_end { last } else { first };
        let ext = span.extended();
        let full = span.full();
        // How far past the end (or before the start) each meeting is, in the span's parameter.
        let beyond = |s: f64| -> Option<f64> {
            let past = match span {
                Span::Seg { .. } if at_end => s - 1.0,
                Span::Seg { .. } => -s,
                Span::Arc { .. } if s <= 1.0 + EPS || s >= full - EPS => return None,
                Span::Arc { .. } if at_end => s - 1.0,
                Span::Arc { .. } => full - s,
            };
            (past > EPS).then_some(past)
        };
        let best = self
            .edges(i)
            .iter()
            .flat_map(|edge| intersect_extended(&ext, edge))
            .filter_map(|p| beyond(span.param(p)))
            .filter(|d| d.is_finite())
            .min_by(f64::total_cmp)
            .ok_or("Nothing to extend to: no object lies ahead of that end")?;
        let s = if at_end { 1.0 + best } else { -best };
        let q = span.at(s);
        if !q.is_finite() || q.x.abs() > crate::MAX_COORD || q.y.abs() > crate::MAX_COORD {
            return Err("The extension runs out of the drawing".into());
        }
        let Some(obj) = self.entities.get_mut(i) else { return Err("There is no object there".into()) };
        match &mut obj.kind {
            Kind::Line { a, b } => *(if at_end { b } else { a }) = pt(q),
            Kind::Arc { c, start, end, .. } => {
                let angle = Num::new(canon_deg(direction(q.minus(c.pt()))));
                *(if at_end { end } else { start }) = angle;
            }
            Kind::Polyline { pts, bulges, .. } => {
                let m = sp.len();
                if bulges.len() < m {
                    bulges.resize(m, 0.0);
                }
                if at_end {
                    if let Some(v) = pts.last_mut() {
                        *v = pt(q);
                    }
                    if let Some(b) = bulges.get_mut(m - 1) {
                        *b = span.bulge(s);
                    }
                } else {
                    if let Some(v) = pts.first_mut() {
                        *v = pt(q);
                    }
                    if let Some(b) = bulges.first_mut() {
                        *b = span.bulge(1.0 + best);
                    }
                }
                if bulges.iter().all(|b| *b == 0.0) {
                    bulges.clear();
                }
            }
            _ => return Err("Extend works on lines, arcs and open polylines".into()),
        }
        self.sanitize();
        Ok(())
    }

    /// Fillets lines `i` and `j`, picked at `pi` and `pj` (the parts kept): both are trimmed or
    /// extended to an arc of radius `r` touching them (radius 0: to their corner). The arc takes
    /// the first line's layer and stroke.
    pub fn fillet(&mut self, i: usize, pi: Pt, j: usize, pj: Pt, r: f64) -> Result<(), String> {
        if !(r.is_finite() && r >= 0.0) {
            return Err("A fillet radius is 0 or more".into());
        }
        let c = self.corner(i, pi, j, pj)?;
        let half = c.angle / 2.0;
        let d = r / half.tan();
        if d > c.reach1 + EPS || d > c.reach2 + EPS {
            return Err("The radius is too large for these lines".into());
        }
        let (t1, t2) = (c.x.plus(c.u1.scale(d)), c.x.plus(c.u2.scale(d)));
        self.join_lines(i, c.keep1_a, t1, j, c.keep2_a, t2)?;
        if r > 0.0 {
            let centre = c.x.plus(c.u1.plus(c.u2).unit().scale(r / half.sin()));
            let (mut s, mut e) = (direction(t1.minus(centre)), direction(t2.minus(centre)));
            // The short way round: the arc between the tangent points facing the corner.
            if norm_angle(e - s) > PI {
                std::mem::swap(&mut s, &mut e);
            }
            self.add_like(i, Kind::Arc { c: pt(centre), r: Num::new(r), start: Num::new(canon_deg(s)), end: Num::new(canon_deg(e)) })?;
        }
        Ok(())
    }

    /// Chamfers lines `i` and `j`, picked at `pi` and `pj` (the parts kept): both are cut back
    /// `d1` and `d2` from their corner and joined by a line (distances 0: to the corner).
    pub fn chamfer(&mut self, i: usize, pi: Pt, j: usize, pj: Pt, d1: f64, d2: f64) -> Result<(), String> {
        if !(d1.is_finite() && d2.is_finite() && d1 >= 0.0 && d2 >= 0.0) {
            return Err("Chamfer distances are 0 or more".into());
        }
        let c = self.corner(i, pi, j, pj)?;
        if d1 > c.reach1 + EPS || d2 > c.reach2 + EPS {
            return Err("The distances are too large for these lines".into());
        }
        let (t1, t2) = (c.x.plus(c.u1.scale(d1)), c.x.plus(c.u2.scale(d2)));
        self.join_lines(i, c.keep1_a, t1, j, c.keep2_a, t2)?;
        if t1.minus(t2).len() > EPS {
            self.add_like(i, Kind::Line { a: pt(t1), b: pt(t2) })?;
        }
        Ok(())
    }

    /// The corner two picked lines make: where they cross and, for each, the direction from
    /// there to the part kept, how far that part reaches and which end of it stays.
    fn corner(&self, i: usize, pi: Pt, j: usize, pj: Pt) -> Result<Corner, String> {
        if i == j {
            return Err("Pick two different lines".into());
        }
        let line = |k: usize| match self.entities.get(k).map(|e| &e.kind) {
            Some(Kind::Line { a, b }) => Ok((a.pt(), b.pt())),
            _ => Err("Fillet and Chamfer join two lines".to_string()),
        };
        let ((a1, b1), (a2, b2)) = (line(i)?, line(j)?);
        let (x, _, _) = line_line(a1, b1, a2, b2).ok_or("The lines are parallel")?;
        let side = |a: Pt, b: Pt, p: Pt| -> Result<(Pt, bool, f64), String> {
            let q = Curve::Seg { a, b }.nearest(p);
            let mut u = q.minus(x);
            if u.len() <= EPS {
                // Picked at the corner: the part kept is the longer one.
                u = if a.minus(x).len() >= b.minus(x).len() { a.minus(x) } else { b.minus(x) };
            }
            let u = u.unit();
            let (da, db) = (dot(a.minus(x), u), dot(b.minus(x), u));
            let reach = da.max(db);
            if reach <= EPS || !u.is_finite() {
                return Err("Pick each line on the part to keep".into());
            }
            Ok((u, da >= db, reach))
        };
        let (u1, keep1_a, reach1) = side(a1, b1, pi)?;
        let (u2, keep2_a, reach2) = side(a2, b2, pj)?;
        let angle = crate::kernel::angle_between(u1, u2);
        if angle <= 1e-9 || angle >= PI - 1e-9 {
            return Err("The lines are in line".into());
        }
        Ok(Corner { x, u1, u2, keep1_a, keep2_a, reach1, reach2, angle })
    }

    /// Lines `i` and `j` running from the ends they keep to `t1` and `t2`.
    fn join_lines(&mut self, i: usize, keep1_a: bool, t1: Pt, j: usize, keep2_a: bool, t2: Pt) -> Result<(), String> {
        let kept = |k: usize, keep_a: bool| match self.entities.get(k).map(|e| &e.kind) {
            Some(Kind::Line { a, b }) => Some(if keep_a { a.pt() } else { b.pt() }),
            _ => None,
        };
        // A line cut down to nothing would be left.
        if [(i, keep1_a, t1), (j, keep2_a, t2)].iter().any(|&(k, keep_a, t)| kept(k, keep_a).is_none_or(|p| p.minus(t).len() <= EPS)) {
            return Err("That would leave a line with no length".into());
        }
        for (k, keep_a, t) in [(i, keep1_a, t1), (j, keep2_a, t2)] {
            if let Some(Kind::Line { a, b }) = self.entities.get_mut(k).map(|e| &mut e.kind) {
                *(if keep_a { b } else { a }) = pt(t);
            }
        }
        Ok(())
    }

    /// Adds `kind` on the layer and with the stroke of entity `like`.
    fn add_like(&mut self, like: usize, kind: Kind) -> Result<(), String> {
        let mut e = Entity::new(kind);
        if let Some(src) = self.entities.get(like) {
            e.layer = src.layer.clone();
            e.stroke = src.stroke.clone();
        }
        self.add(e).map(|_| ())
    }
}

/// See [`Drawing::corner`].
struct Corner {
    x: Pt,
    u1: Pt,
    u2: Pt,
    keep1_a: bool,
    keep2_a: bool,
    reach1: f64,
    reach2: f64,
    angle: f64,
}

/// An angle in degrees as saved.
fn canon_deg(rad: f64) -> f64 {
    canon(deg(rad))
}

/// The part of entity `e` from `u0` to `u1` along its spans (`u1` may pass the end of a closed
/// one and wrap round). Ends left where they were keep their expressions.
fn piece(e: &Entity, sp: &[Span], closed: bool, u0: f64, u1: f64) -> Option<Kind> {
    let n = sp.len() as f64;
    if u1 - u0 <= EPS {
        return None;
    }
    let made = match &e.kind {
        Kind::Line { a, b } => {
            let s = sp.first()?;
            let na = if u0 <= EPS { a.clone() } else { pt(s.at(u0)) };
            let nb = if u1 >= n - EPS { b.clone() } else { pt(s.at(u1)) };
            Kind::Line { a: na, b: nb }
        }
        Kind::Arc { c, r, start, end } => {
            let s = sp.first()?;
            let angle = |u: f64| match *s {
                Span::Arc { start, sweep, .. } => Num::new(canon_deg(start + sweep * u)),
                Span::Seg { .. } => Num::new(0.0),
            };
            let ns = if u0 <= EPS { start.clone() } else { angle(u0) };
            let ne = if u1 >= n - EPS && !closed { end.clone() } else { angle(u1) };
            Kind::Arc { c: c.clone(), r: r.clone(), start: ns, end: ne }
        }
        Kind::Circle { c, r } => {
            let s = sp.first()?;
            let angle = |u: f64| match *s {
                Span::Arc { start, sweep, .. } => Num::new(canon_deg(norm_angle(start + sweep * u))),
                Span::Seg { .. } => Num::new(0.0),
            };
            Kind::Arc { c: c.clone(), r: r.clone(), start: angle(u0), end: angle(u1) }
        }
        Kind::Polyline { pts, .. } => {
            let nv = pts.len();
            let m = sp.len();
            let vertex = |u: f64| -> Option<P> {
                let k = u.round();
                ((u - k).abs() <= EPS).then(|| pts.get((k as usize) % nv.max(1)).cloned()).flatten()
            };
            let mut out_pts: Vec<P> = Vec::new();
            let mut out_bulges: Vec<f64> = Vec::new();
            let mut u = u0;
            // At most once round, and a part of a span at each end.
            for _ in 0..(2 * m + 2) {
                let k = (u + EPS).floor().max(0.0);
                let span = sp.get((k as usize) % m)?;
                let next = u1.min(k + 1.0);
                let (s0, s1) = ((u - k).max(0.0), next - k);
                out_pts.push(vertex(u).unwrap_or_else(|| pt(span.at(s0))));
                out_bulges.push(span.bulge(s1 - s0));
                u = next;
                if u >= u1 - EPS {
                    break;
                }
            }
            let last = sp.get(((u1 - EPS).floor().max(0.0) as usize) % m)?;
            let k_last = (u1 - EPS).floor().max(0.0);
            out_pts.push(vertex(u1).unwrap_or_else(|| pt(last.at(u1 - k_last))));
            if out_pts.len() < 2 {
                return None;
            }
            if out_bulges.iter().all(|b| *b == 0.0) {
                out_bulges.clear();
            }
            Kind::Polyline { pts: out_pts, bulges: out_bulges, closed: false }
        }
        _ => return None,
    };
    Some(made)
}
