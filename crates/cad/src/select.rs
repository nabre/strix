//! Selection and object properties for the drawing editor, AutoCAD-style (docs/cad.md, stage 3):
//! the objects under the pointer nearest first (for cycling through overlapping ones), lasso and
//! polygon windows, fences, typed selection options, Select Similar, draw order, layers and
//! strokes of a selection, and the Properties pane's focus (all the selection, one family, one
//! object of it, like AutoCAD's Quick Properties).

use crate::MAX_LAYERS;
use crate::geom::Pt;
use crate::kernel::{Curve, curves, intersect};
use crate::model::{Dash, Drawing, Kind, Layer, Stroke};

/// An object family: what the Properties pane groups the selection by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Family {
    Line,
    Polyline,
    Circle,
    Arc,
    Point,
    Dim,
    Text,
    Hatch,
}

impl Family {
    /// In the Properties pane's order: geometry, then annotations.
    pub const ALL: [Family; 8] =
        [Family::Line, Family::Polyline, Family::Circle, Family::Arc, Family::Point, Family::Dim, Family::Text, Family::Hatch];

    pub fn of(k: &Kind) -> Family {
        match k {
            Kind::Line { .. } => Family::Line,
            Kind::Polyline { .. } => Family::Polyline,
            Kind::Circle { .. } => Family::Circle,
            Kind::Arc { .. } => Family::Arc,
            Kind::Point { .. } => Family::Point,
            Kind::Dim { .. } | Kind::Angle { .. } => Family::Dim,
            Kind::Text { .. } => Family::Text,
            Kind::Strip { .. } => Family::Hatch,
        }
    }

    /// The family's name (English; the UI translates it).
    pub fn name(self) -> &'static str {
        match self {
            Family::Line => "Line",
            Family::Polyline => "Polyline",
            Family::Circle => "Circle",
            Family::Arc => "Arc",
            Family::Point => "Point",
            Family::Dim => "Dimension",
            Family::Text => "Text",
            Family::Hatch => "Hatch",
        }
    }
}

/// A selection option typed at the command line (AutoCAD's words, in English and Italian).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectOption {
    /// Everything selectable.
    All,
    /// A polyline that takes what it crosses.
    Fence,
    /// A polygon window (objects wholly inside).
    WPolygon,
    /// A crossing polygon (objects inside or touching).
    CPolygon,
    /// Clicks remove from the selection.
    Remove,
    /// Clicks add to the selection.
    Add,
    /// The selection of the last command.
    Previous,
    /// Undo the last selection step.
    Undo,
}

impl SelectOption {
    pub fn parse(text: &str) -> Option<SelectOption> {
        Some(match text.trim().to_lowercase().as_str() {
            "all" | "t" | "tutto" => SelectOption::All,
            "f" | "fence" | "i" | "interseca" | "intercetta" => SelectOption::Fence,
            "wp" | "wpolygon" | "fp" => SelectOption::WPolygon,
            "cp" | "cpolygon" | "ip" => SelectOption::CPolygon,
            "r" | "remove" | "rimuovi" => SelectOption::Remove,
            "a" | "add" | "aggiungi" => SelectOption::Add,
            "p" | "previous" | "precedente" => SelectOption::Previous,
            "u" | "undo" | "annulla" => SelectOption::Undo,
            _ => return None,
        })
    }
}

/// How the selected objects move in the draw order (the first object is drawn first, under the
/// others).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    /// Above every other object.
    Front,
    /// Under every other object.
    Back,
    /// One place up, past the first unselected object above.
    Forward,
    /// One place down.
    Backward,
}

/// Changes to the stroke of a selection: `Some(None)` sets a field back to ByLayer.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StrokeEdit {
    pub color: Option<Option<[u8; 3]>>,
    pub width: Option<Option<f64>>,
    pub dash: Option<Option<Dash>>,
}

/// Which side of the line `o`→`p` the point `q` is on: positive to the left, negative to the
/// right, zero exactly on it. Shewchuk's adaptive predicate (the `robust` crate): the sign is
/// exact for any doubles, where the plain cross product misjudges points near the line.
fn orient(o: Pt, p: Pt, q: Pt) -> f64 {
    let c = |v: Pt| robust::Coord { x: v.x, y: v.y };
    robust::orient2d(c(o), c(p), c(q))
}

/// Do segments `a`–`b` and `c`–`d` meet (touching counts)?
fn segments_meet(a: Pt, b: Pt, c: Pt, d: Pt) -> bool {
    let cross = orient;
    let on = |p: Pt, q: Pt, r: Pt| r.x >= p.x.min(q.x) && r.x <= p.x.max(q.x) && r.y >= p.y.min(q.y) && r.y <= p.y.max(q.y);
    let (d1, d2, d3, d4) = (cross(c, d, a), cross(c, d, b), cross(a, b, c), cross(a, b, d));
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0)) {
        return true;
    }
    (d1 == 0.0 && on(c, d, a)) || (d2 == 0.0 && on(c, d, b)) || (d3 == 0.0 && on(a, b, c)) || (d4 == 0.0 && on(a, b, d))
}

/// Is `p` inside the polygon `poly` (even-odd rule)? A point on an edge is inside, so a window
/// and a lasso agree on their borders. The crossing test is the exact orientation sign.
pub fn point_in_polygon(p: Pt, poly: &[Pt]) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (Some(&a), Some(&b)) = (poly.get(i), poly.get(j)) else { break };
        if segments_meet(p, p, a, b) {
            return true;
        }
        // The edge crosses the horizontal through p, on p's right (the sign of the orientation
        // of a→b against p, flipped for a downward edge).
        if (a.y > p.y) != (b.y > p.y) {
            let side = orient(a, b, p);
            if (b.y > a.y && side < 0.0) || (b.y < a.y && side > 0.0) {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// The segments of a closed polygon or an open fence.
fn edges(pts: &[Pt], closed: bool) -> Vec<(Pt, Pt)> {
    let mut out: Vec<(Pt, Pt)> = pts.windows(2).map(|w| (w[0], w[1])).collect();
    if closed
        && pts.len() > 2
        && let (Some(&first), Some(&last)) = (pts.first(), pts.last())
    {
        out.push((last, first));
    }
    out
}

/// How far `p` is from entity `e`: from its exact curves when it has some, else from its
/// outline (a point, a text's position, a dimension's lines).
fn entity_distance(d: &Drawing, e: &crate::Entity, p: Pt) -> f64 {
    let exact = curves(e);
    if !exact.is_empty() {
        return exact.iter().map(|c| c.distance(p)).fold(f64::INFINITY, f64::min);
    }
    d.shape(e)
        .iter()
        .map(|line| match line.as_slice() {
            [only] => only.minus(p).len(),
            // A closed ring (a strip's band, a dimension's text) is picked inside too.
            ring if ring.len() > 3 && ring.first() == ring.last() && point_in_polygon(p, ring) => 0.0,
            _ => line.windows(2).map(|w| seg_dist(p, w[0], w[1])).fold(f64::INFINITY, f64::min),
        })
        .fold(f64::INFINITY, f64::min)
}

/// Distance from `p` to segment `a`–`b`.
fn seg_dist(p: Pt, a: Pt, b: Pt) -> f64 {
    let ab = b.minus(a);
    let l2 = ab.x * ab.x + ab.y * ab.y;
    if l2 == 0.0 {
        return p.minus(a).len();
    }
    let t = (((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / l2).clamp(0.0, 1.0);
    p.minus(a.plus(ab.scale(t))).len()
}

impl Drawing {
    /// Can entity `i` be picked (its layer is shown and not locked)?
    pub fn selectable(&self, i: usize) -> bool {
        self.entities.get(i).is_some_and(|e| self.layers.iter().find(|l| l.name == e.layer).is_none_or(|l| l.visible && !l.locked))
    }

    /// The entities within `tol` of `p`, nearest first; at the same distance the one drawn on
    /// top first. Clicking again in the same place, or Shift+Space, steps through them.
    pub fn hits(&self, p: Pt, tol: f64) -> Vec<usize> {
        let mut found: Vec<(usize, f64)> = self
            .entities
            .iter()
            .enumerate()
            .filter(|(i, _)| self.selectable(*i))
            .filter_map(|(i, e)| {
                let d = entity_distance(self, e, p);
                (d <= tol).then_some((i, d))
            })
            .collect();
        found.sort_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)));
        found.into_iter().map(|(i, _)| i).collect()
    }

    /// The entities in the box `a`–`b` (the two-click window): the polygon of its corners.
    pub fn in_box(&self, a: Pt, b: Pt, crossing: bool) -> Vec<usize> {
        let (x0, x1, y0, y1) = (a.x.min(b.x), a.x.max(b.x), a.y.min(b.y), a.y.max(b.y));
        self.in_polygon(&[Pt::new(x0, y0), Pt::new(x1, y0), Pt::new(x1, y1), Pt::new(x0, y1)], crossing)
    }

    /// The entities in the polygon `poly` (a lasso or a polygon window): wholly inside, or, with
    /// `crossing`, inside or touching its edge. Lines, circles, arcs and polylines are tested on
    /// their exact curves (an edge cutting a circle cuts it, however finely it is drawn); points,
    /// texts and dimensions on their outlines.
    pub fn in_polygon(&self, poly: &[Pt], crossing: bool) -> Vec<usize> {
        if poly.len() < 3 {
            return Vec::new();
        }
        let sides = edges(poly, true);
        let side_curves: Vec<Curve> = sides.iter().map(|&(a, b)| Curve::Seg { a, b }).collect();
        self.entities
            .iter()
            .enumerate()
            .filter(|(i, _)| self.selectable(*i))
            .filter(|(_, e)| {
                let exact = curves(e);
                if !exact.is_empty() {
                    let cut = exact.iter().any(|c| side_curves.iter().any(|s| !intersect(c, s).is_empty()));
                    // Not cut: the curve is wholly inside or wholly outside; one point tells.
                    let first = exact.first().and_then(|c| c.ends().first().copied().or_else(|| c.quadrants().first().copied()));
                    let inside = first.is_some_and(|p| point_in_polygon(p, poly));
                    return if crossing { cut || inside } else { inside && !cut };
                }
                let lines = self.shape(e);
                let pts: Vec<Pt> = lines.iter().flatten().copied().collect();
                if pts.is_empty() {
                    return false;
                }
                let inside = pts.iter().filter(|p| point_in_polygon(**p, poly)).count();
                let cuts = || lines.iter().any(|l| l.windows(2).any(|w| sides.iter().any(|&(c, d)| segments_meet(w[0], w[1], c, d))));
                if crossing { inside > 0 || cuts() } else { inside == pts.len() && !cuts() }
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// The entities a fence (an open polyline) crosses.
    pub fn crossing_fence(&self, fence: &[Pt]) -> Vec<usize> {
        let sides = edges(fence, false);
        self.entities
            .iter()
            .enumerate()
            .filter(|(i, _)| self.selectable(*i))
            .filter(|(_, e)| {
                self.shape(e).iter().any(|l| match l.as_slice() {
                    [only] => sides.iter().any(|&(a, b)| seg_dist(*only, a, b) <= 1e-9),
                    _ => l.windows(2).any(|w| sides.iter().any(|&(c, d)| segments_meet(w[0], w[1], c, d))),
                })
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Select Similar: the selectable entities of the same family and on the same layer as one of
    /// those at `indices`.
    pub fn similar(&self, indices: &[usize]) -> Vec<usize> {
        let like: Vec<(Family, &str)> =
            indices.iter().filter_map(|&i| self.entities.get(i)).map(|e| (Family::of(&e.kind), e.layer.as_str())).collect();
        self.entities
            .iter()
            .enumerate()
            .filter(|(i, e)| self.selectable(*i) && like.contains(&(Family::of(&e.kind), e.layer.as_str())))
            .map(|(i, _)| i)
            .collect()
    }

    /// Moves the entities at `indices` in the draw order; returns their new indices (the selection
    /// follows them).
    pub fn reorder(&mut self, indices: &[usize], order: Order) -> Vec<usize> {
        let n = self.entities.len();
        let mut picked = vec![false; n];
        for &i in indices {
            if let Some(p) = picked.get_mut(i) {
                *p = true;
            }
        }
        let mut slots: Vec<(usize, bool)> = (0..n).map(|i| (i, picked.get(i).copied().unwrap_or(false))).collect();
        match order {
            Order::Front => slots.sort_by_key(|&(_, p)| p),
            Order::Back => slots.sort_by_key(|&(_, p)| !p),
            Order::Forward => {
                // From the top down: each selected object swaps with the unselected one above it.
                for k in (0..n.saturating_sub(1)).rev() {
                    if slots.get(k).is_some_and(|s| s.1) && slots.get(k + 1).is_some_and(|s| !s.1) {
                        let mut j = k;
                        // A contiguous run moves up together.
                        while j > 0 && slots.get(j - 1).is_some_and(|s| s.1) {
                            j -= 1;
                        }
                        if let Some(run) = slots.get_mut(j..=k + 1) {
                            run.rotate_right(1);
                        }
                    }
                }
            }
            Order::Backward => {
                let mut k = 1;
                while k < n {
                    if slots.get(k).is_some_and(|s| s.1) && slots.get(k - 1).is_some_and(|s| !s.1) {
                        let mut j = k;
                        while j + 1 < n && slots.get(j + 1).is_some_and(|s| s.1) {
                            j += 1;
                        }
                        if let Some(run) = slots.get_mut(k - 1..=j) {
                            run.rotate_left(1);
                        }
                        k = j + 2;
                    } else {
                        k += 1;
                    }
                }
            }
        }
        let old = std::mem::take(&mut self.entities);
        let mut cells: Vec<Option<crate::model::Entity>> = old.into_iter().map(Some).collect();
        let mut out = Vec::new();
        for (new, &(i, p)) in slots.iter().enumerate() {
            if let Some(e) = cells.get_mut(i).and_then(Option::take) {
                self.entities.push(e);
                if p {
                    out.push(new);
                }
            }
        }
        out
    }

    /// Puts the entities at `indices` on layer `name` (made if missing).
    pub fn set_layer(&mut self, indices: &[usize], name: &str) -> Result<(), String> {
        self.ensure_layer(name)?;
        for &i in indices {
            if let Some(e) = self.entities.get_mut(i) {
                e.layer = name.to_string();
            }
        }
        Ok(())
    }

    /// Changes the stroke of the entities at `indices`.
    pub fn set_stroke(&mut self, indices: &[usize], edit: StrokeEdit) {
        for &i in indices {
            if let Some(e) = self.entities.get_mut(i) {
                if let Some(c) = edit.color {
                    e.stroke.color = c;
                }
                if let Some(w) = edit.width {
                    e.stroke.width = w.filter(|w| w.is_finite()).map(|w| w.clamp(0.05, 10.0));
                }
                if let Some(d) = edit.dash {
                    e.stroke.dash = d;
                }
            }
        }
    }

    /// The layer `name`, added with layer 0's stroke if missing.
    pub fn ensure_layer(&mut self, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 64 {
            return Err("a layer name has 1 to 64 characters".into());
        }
        if self.layers.iter().any(|l| l.name == name) {
            return Ok(());
        }
        if self.layers.len() >= MAX_LAYERS {
            return Err(format!("a drawing holds at most {MAX_LAYERS} layers"));
        }
        self.layers.push(Layer { name: name.to_string(), ..Layer::default() });
        Ok(())
    }

    /// A new layer's name: `Layer1`, `Layer2`… (the first free).
    pub fn new_layer_name(&self) -> String {
        (1..=MAX_LAYERS + 1).map(|n| format!("Layer{n}")).find(|n| !self.layers.iter().any(|l| &l.name == n)).unwrap_or_default()
    }

    /// Renames layer `from` (not `0`); its entities follow.
    pub fn rename_layer(&mut self, from: &str, to: &str) -> Result<(), String> {
        let to = to.trim();
        if from == "0" {
            return Err("layer 0 cannot be renamed".into());
        }
        if to.is_empty() || to.chars().count() > 64 {
            return Err("a layer name has 1 to 64 characters".into());
        }
        if from == to {
            return Ok(());
        }
        if self.layers.iter().any(|l| l.name == to) {
            return Err(format!("there is already a layer {to}"));
        }
        let layer = self.layers.iter_mut().find(|l| l.name == from).ok_or_else(|| format!("no layer {from}"))?;
        layer.name = to.to_string();
        for e in self.entities.iter_mut().filter(|e| e.layer == from) {
            e.layer = to.to_string();
        }
        Ok(())
    }

    /// Removes layer `name` (not `0`); its entities go to layer `0`.
    pub fn remove_layer(&mut self, name: &str) -> Result<(), String> {
        if name == "0" {
            return Err("layer 0 cannot be removed".into());
        }
        let before = self.layers.len();
        self.layers.retain(|l| l.name != name);
        if self.layers.len() == before {
            return Err(format!("no layer {name}"));
        }
        for e in self.entities.iter_mut().filter(|e| e.layer == name) {
            e.layer = "0".into();
        }
        Ok(())
    }

    /// The stroke entity `i` is drawn with (its own, else its layer's).
    pub fn effective_stroke(&self, i: usize) -> Option<([u8; 3], f64, Dash)> {
        let e = self.entities.get(i)?;
        let l = self.layer(&e.layer);
        let Stroke { color, width, dash } = &e.stroke;
        Some((color.unwrap_or(l.color), width.unwrap_or(l.width), dash.unwrap_or(l.dash)))
    }
}

/// The families of the selection, each with its objects in drawing order.
pub fn families(d: &Drawing, selection: &[usize]) -> Vec<(Family, Vec<usize>)> {
    let mut sel: Vec<usize> = selection.iter().copied().filter(|&i| i < d.entities.len()).collect();
    sel.sort_unstable();
    sel.dedup();
    Family::ALL
        .iter()
        .filter_map(|&f| {
            let of: Vec<usize> = sel.iter().copied().filter(|&i| d.entities.get(i).is_some_and(|e| Family::of(&e.kind) == f)).collect();
            (!of.is_empty()).then_some((f, of))
        })
        .collect()
}

/// What the Properties pane works on: all the selection (`family` none), one family (`object`
/// none) or one object. `across`: the object was reached stepping through the whole selection,
/// so ‹ › go on across families.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Focus {
    pub family: Option<Family>,
    pub object: Option<usize>,
    pub across: bool,
}

impl Focus {
    /// The focus made valid for the selection now (a family gone: all the selection; an object
    /// gone: its family; a family of one object is that object), with the objects it covers.
    pub fn scope(self, d: &Drawing, selection: &[usize]) -> (Focus, Vec<usize>) {
        let fams = families(d, selection);
        let all: Vec<usize> = fams.iter().flat_map(|(_, v)| v.iter().copied()).collect();
        if self.across {
            if let Some(i) = self.object.filter(|i| all.contains(i))
                && let Some(e) = d.entities.get(i)
            {
                return (Focus { family: Some(Family::of(&e.kind)), object: Some(i), across: true }, vec![i]);
            }
            return (Focus::default(), all);
        }
        let Some((f, of)) = self.family.and_then(|f| fams.iter().find(|(g, _)| *g == f)) else { return (Focus::default(), all) };
        if let [only] = of.as_slice() {
            return (Focus { family: Some(*f), object: Some(*only), across: false }, vec![*only]);
        }
        match self.object.filter(|i| of.contains(i)) {
            Some(i) => (Focus { family: Some(*f), object: Some(i), across: false }, vec![i]),
            None => (Focus { family: Some(*f), object: None, across: false }, of.clone()),
        }
    }

    /// The next (`forward`) or previous object: within the family, all → first → … → last → all;
    /// from all the selection, the same across every selected object.
    pub fn step(self, d: &Drawing, selection: &[usize], forward: bool) -> Focus {
        let (s, _) = self.scope(d, selection);
        let fams = families(d, selection);
        let order: Vec<usize> = if s.family.is_none() || s.across {
            fams.iter().flat_map(|(_, v)| v.iter().copied()).collect()
        } else {
            fams.iter().find(|(f, _)| Some(*f) == s.family).map(|(_, v)| v.clone()).unwrap_or_default()
        };
        let n = order.len();
        if n < 2 {
            return s;
        }
        // Position n is "all", between the last and the first.
        let at = s.object.and_then(|i| order.iter().position(|&x| x == i)).unwrap_or(n);
        let next = if forward { (at + 1) % (n + 1) } else { (at + n) % (n + 1) };
        let across = s.family.is_none() || s.across;
        match order.get(next) {
            None if across => Focus::default(),
            None => Focus { family: s.family, object: None, across: false },
            Some(&i) => Focus { family: d.entities.get(i).map(|e| Family::of(&e.kind)), object: Some(i), across },
        }
    }
}
