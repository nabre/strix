//! Explode, Join, Match Properties, Stretch and arrays (docs/cad.md). Ends and vertices an edit
//! leaves where they were keep their expressions; Stretch and the arrays move points the way
//! Move does (`L` becomes `L + 10`).

use crate::edit::shift_p;
use crate::geom::Pt;
use crate::kernel::{Curve, RESOLUTION, bulge_bends, canon, deg, norm_angle, segments};
use crate::model::{Drawing, Entity, Kind, Num, P};
use crate::modify::Affine;
use crate::{MAX_ENTITIES, MAX_VERTICES};

/// Most items one array makes.
pub const MAX_ARRAY: usize = 1000;

/// A constructed point as saved (to the kernel's resolution).
fn pt(p: Pt) -> P {
    P::new(canon(p.x), canon(p.y))
}

/// One segment of a chain being joined: from `a` to `b` with DXF `bulge`.
#[derive(Debug, Clone)]
struct Link {
    a: P,
    b: P,
    bulge: f64,
}

/// What an entity brings to a join: a line, an arc (not a whole circle), an open polyline.
fn links(e: &Entity) -> Option<Vec<Link>> {
    match &e.kind {
        Kind::Line { a, b } => Some(vec![Link { a: a.clone(), b: b.clone(), bulge: 0.0 }]),
        Kind::Arc { c, r, start, end } => {
            let (cp, r) = (c.pt(), r.value.abs());
            let s = start.value.to_radians();
            let sweep = norm_angle(end.value.to_radians() - s);
            if sweep == 0.0 || r <= RESOLUTION {
                return None;
            }
            let at = |t: f64| Pt::new(cp.x + r * t.cos(), cp.y + r * t.sin());
            Some(vec![Link { a: pt(at(s)), b: pt(at(s + sweep)), bulge: (sweep / 4.0).tan() }])
        }
        Kind::Polyline { pts, bulges, closed: false } if pts.len() >= 2 => Some(
            pts.windows(2)
                .enumerate()
                .filter_map(|(k, w)| Some(Link { a: w.first()?.clone(), b: w.get(1)?.clone(), bulge: bulges.get(k).copied().unwrap_or(0.0) }))
                .collect(),
        ),
        _ => None,
    }
}

/// The chain run the other way.
fn reversed(chain: Vec<Link>) -> Vec<Link> {
    chain.into_iter().rev().map(|l| Link { a: l.b, b: l.a, bulge: -l.bulge }).collect()
}

/// The ends of a chain.
fn ends(chain: &[Link]) -> Option<(Pt, Pt)> {
    Some((chain.first()?.a.pt(), chain.last()?.b.pt()))
}

impl Drawing {
    /// Explodes the polylines among `indices` into their lines and arcs (other objects stay as
    /// they are). A strip along an exploded polyline or a hatch bounded by it goes with it.
    /// Returns the indices of the pieces.
    pub fn explode(&mut self, indices: &[usize]) -> Vec<usize> {
        let mut sorted = indices.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        let mut made: Vec<String> = Vec::new();
        let mut gone: Vec<String> = Vec::new();
        for &i in sorted.iter().rev() {
            let Some(e) = self.entities.get(i).cloned() else { continue };
            let Kind::Polyline { pts, bulges, closed } = &e.kind else { continue };
            let n = pts.len();
            let kinds: Vec<Kind> = (0..segments(n, *closed))
                .filter_map(|k| {
                    let (a, b) = (pts.get(k)?, pts.get((k + 1) % n)?);
                    let bulge = bulges.get(k).copied().unwrap_or(0.0);
                    if !bulge_bends(a.pt(), b.pt(), bulge) {
                        return Some(Kind::Line { a: a.clone(), b: b.clone() });
                    }
                    match Curve::from_bulge(a.pt(), b.pt(), bulge) {
                        Curve::Arc { c, r, start, sweep } => Some(Kind::Arc {
                            c: pt(c),
                            r: Num::new(canon(r)),
                            start: Num::new(canon(deg(start))),
                            end: Num::new(canon(deg(start + sweep))),
                        }),
                        _ => Some(Kind::Line { a: a.clone(), b: b.clone() }),
                    }
                })
                .collect();
            if kinds.is_empty() || self.entities.len() + kinds.len() > MAX_ENTITIES {
                continue;
            }
            gone.push(e.id.clone());
            for (k, kind) in kinds.into_iter().enumerate() {
                let mut piece = e.clone();
                piece.kind = kind;
                if k == 0 {
                    if let Some(obj) = self.entities.get_mut(i) {
                        *obj = piece;
                        made.push(obj.id.clone());
                    }
                } else {
                    piece.id = self.fresh_id();
                    made.push(piece.id.clone());
                    self.entities.insert((i + k).min(self.entities.len()), piece);
                }
            }
        }
        // Their strips and hatches had the polyline's shape: they go.
        self.entities.retain(|e| match &e.kind {
            Kind::Strip { along, .. } => !gone.contains(along),
            Kind::Hatch { boundary, .. } => !boundary.iter().any(|b| gone.contains(b)),
            _ => true,
        });
        self.sanitize();
        self.entities.iter().enumerate().filter(|(_, e)| made.contains(&e.id)).map(|(i, _)| i).collect()
    }

    /// Joins the lines, arcs and open polylines among `indices` that meet end to end into one
    /// polyline (closed when its ends meet), in place of the first of them, on its layer and
    /// with its stroke. Objects that do not meet the chain stay as they are. Returns the index
    /// of the polyline.
    pub fn join(&mut self, indices: &[usize]) -> Result<usize, String> {
        let mut sorted = indices.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        let mut pieces: Vec<(usize, Vec<Link>)> =
            sorted.iter().filter_map(|&i| Some((i, links(self.entities.get(i)?)?))).filter(|(_, l)| !l.is_empty()).collect();
        if pieces.len() < 2 {
            return Err("Select two or more lines, arcs or open polylines to join".into());
        }
        let extent =
            pieces.iter().flat_map(|(_, l)| l.iter().flat_map(|k| [k.a.pt(), k.b.pt()])).map(|p| p.x.abs().max(p.y.abs())).fold(1.0, f64::max);
        let tol = (extent * 1e-9).max(1e-6);
        let near = |p: Pt, q: Pt| p.minus(q).len() <= tol;
        let (first, mut chain) = pieces.remove(0);
        let mut used = vec![first];
        while let Some((start, end)) = ends(&chain) {
            let found =
                pieces.iter().position(|(_, l)| ends(l).is_some_and(|(s, e)| near(s, end) || near(e, end) || near(e, start) || near(s, start)));
            let Some(k) = found else { break };
            let (i, l) = pieces.remove(k);
            let Some((s, e)) = ends(&l) else { break };
            if near(s, end) {
                chain.extend(l);
            } else if near(e, end) {
                chain.extend(reversed(l));
            } else if near(e, start) {
                chain = l.into_iter().chain(chain).collect();
            } else {
                chain = reversed(l).into_iter().chain(chain).collect();
            }
            used.push(i);
        }
        if used.len() < 2 {
            return Err("Nothing to join: the objects must meet end to end".into());
        }
        let (start, end) = ends(&chain).ok_or("Nothing to join")?;
        let closed = chain.len() >= 2 && near(start, end);
        let mut pts: Vec<P> = chain.iter().map(|l| l.a.clone()).collect();
        if !closed && let Some(l) = chain.last() {
            pts.push(l.b.clone());
        }
        if pts.len() > MAX_VERTICES {
            return Err(format!("a polyline has at most {MAX_VERTICES} vertices"));
        }
        let mut bulges: Vec<f64> = chain.iter().map(|l| l.bulge).collect();
        if bulges.iter().all(|b| *b == 0.0) {
            bulges.clear();
        }
        if let Some(obj) = self.entities.get_mut(first) {
            obj.kind = Kind::Polyline { pts, bulges, closed };
        }
        // The others go (with their strips and hatches); none is before the first.
        self.remove(used.get(1..).unwrap_or_default());
        self.sanitize();
        Ok(first)
    }

    /// Gives the entities at `targets` the layer and stroke of entity `source`, and what they
    /// share of its own look: a text's height, a hatch's or strip's pattern, angle and spacing
    /// (and a strip's width), a dimension's end mark.
    pub fn match_properties(&mut self, source: usize, targets: &[usize]) {
        let Some(src) = self.entities.get(source).cloned() else { return };
        for &t in targets {
            if t == source {
                continue;
            }
            let Some(e) = self.entities.get_mut(t) else { continue };
            e.layer = src.layer.clone();
            e.stroke = src.stroke.clone();
            match (&src.kind, &mut e.kind) {
                (Kind::Text { height, .. }, Kind::Text { height: h, .. }) => *h = *height,
                (Kind::Hatch { pattern, angle, spacing, .. }, Kind::Hatch { pattern: p, angle: a, spacing: s, .. }) => {
                    (*p, *a, *s) = (*pattern, *angle, *spacing);
                }
                (Kind::Strip { width, pattern, angle, spacing, .. }, Kind::Strip { width: w, pattern: p, angle: a, spacing: s, .. }) => {
                    (*w, *p, *a, *s) = (*width, *pattern, *angle, *spacing);
                }
                (Kind::Dim { mark, .. }, Kind::Dim { mark: m, .. }) | (Kind::Angle { mark, .. }, Kind::Angle { mark: m, .. }) => *m = *mark,
                _ => {}
            }
        }
        self.sanitize();
    }

    /// Stretches by `d` what a crossing window from `a` to `b` takes: the points of lines,
    /// polylines and dimensions inside it move, the rest stay; circles, points and texts move when
    /// their centre or point is inside; an arc moves when it is inside whole. Objects on locked
    /// or hidden layers stay. Returns how many objects changed.
    pub fn stretch(&mut self, a: Pt, b: Pt, d: Pt) -> usize {
        let (lo, hi) = (Pt::new(a.x.min(b.x), a.y.min(b.y)), Pt::new(a.x.max(b.x), a.y.max(b.y)));
        let inside = |p: &P| {
            let p = p.pt();
            p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y
        };
        let mut changed = 0;
        for i in 0..self.entities.len() {
            if !self.selectable(i) {
                continue;
            }
            let arc_inside = self.entities.get(i).is_some_and(|e| {
                matches!(&e.kind, Kind::Arc { .. }) && crate::kernel::curves(e).iter().all(|c| c.ends().iter().all(|q| inside(&pt(*q))))
            });
            let Some(e) = self.entities.get_mut(i) else { continue };
            let mut moved = false;
            let mut step = |p: &mut P| {
                if inside(p) {
                    shift_p(p, d);
                    moved = true;
                }
            };
            match &mut e.kind {
                Kind::Line { a, b } | Kind::Dim { a, b, .. } => {
                    step(a);
                    step(b);
                }
                Kind::Polyline { pts, .. } => pts.iter_mut().for_each(&mut step),
                Kind::Circle { c, .. } | Kind::Point { p: c } | Kind::Text { p: c, .. } | Kind::Level { p: c, .. } => step(c),
                Kind::Arc { c, .. } => {
                    if arc_inside && inside(c) {
                        step(c);
                    }
                }
                Kind::Angle { c, a, b, .. } => {
                    step(c);
                    step(a);
                    step(b);
                }
                Kind::Strip { .. } | Kind::Hatch { .. } => {}
            }
            if moved {
                changed += 1;
            }
        }
        if changed > 0 {
            self.sanitize();
        }
        changed
    }

    /// A rectangular array of the entities at `indices`: `cols` × `rows` items, `dx` and `dy`
    /// apart (the originals are the first). Returns the copies' indices.
    pub fn array_rect(&mut self, indices: &[usize], cols: usize, rows: usize, dx: f64, dy: f64) -> Result<Vec<usize>, String> {
        let items = cols.saturating_mul(rows);
        if cols == 0 || rows == 0 || items > MAX_ARRAY {
            return Err(format!("An array has 1 to {MAX_ARRAY} items"));
        }
        if !(dx.is_finite() && dy.is_finite()) {
            return Err("The spacing is not a number".into());
        }
        self.room_for(indices.len(), items)?;
        let mut out = Vec::new();
        for r in 0..rows {
            for c in 0..cols {
                if r == 0 && c == 0 {
                    continue;
                }
                out.extend(self.copy(indices, Pt::new(dx * c as f64, dy * r as f64)));
            }
        }
        Ok(out)
    }

    /// A polar array of the entities at `indices`: `count` items round `centre` over `fill`
    /// degrees (360: a whole turn, the items evenly spaced; less: the last at the end of it),
    /// each turned as it goes round. Returns the copies' indices.
    pub fn array_polar(&mut self, indices: &[usize], centre: Pt, count: usize, fill: f64) -> Result<Vec<usize>, String> {
        if !(2..=MAX_ARRAY).contains(&count) {
            return Err(format!("A polar array has 2 to {MAX_ARRAY} items"));
        }
        if !(fill.is_finite() && fill != 0.0 && fill.abs() <= 360.0) {
            return Err("The angle to fill is up to 360°".into());
        }
        self.room_for(indices.len(), count)?;
        let step = if (fill.abs() - 360.0).abs() < 1e-9 { fill / count as f64 } else { fill / (count - 1) as f64 };
        let mut out = Vec::new();
        for k in 1..count {
            let copies = self.copy(indices, Pt::default());
            self.transform(&copies, &Affine::rotation(centre, step * k as f64));
            out.extend(copies);
        }
        Ok(out)
    }

    /// Whether `items` copies of `n` entities fit in the drawing.
    fn room_for(&self, n: usize, items: usize) -> Result<(), String> {
        if n == 0 {
            return Err("Select the objects first".into());
        }
        if self.entities.len().saturating_add(n.saturating_mul(items.saturating_sub(1))) > MAX_ENTITIES {
            return Err(format!("a drawing holds at most {MAX_ENTITIES} entities"));
        }
        Ok(())
    }
}
