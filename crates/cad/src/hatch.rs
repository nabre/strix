//! Hatches (docs/cad.md, Annotations): conventional patterns filling a region with its islands
//! (even-odd), and the hatch entity whose boundaries are other entities, found by id, so it
//! follows them. The patterns are this project's own: families of parallel lines, each with its
//! angle, its phase within the spacing and its dashes, sizes in paper mm (annotative).

use crate::geom::Pt;
use crate::model::{Drawing, Entity, Kind, Pattern};

/// Most hatch segments one fill draws.
const MAX_SEGMENTS: usize = 40_000;
/// Most edge tests one fill makes (lines times the region's edges): a region too finely hatched
/// for it is hatched more coarsely.
const MAX_WORK: usize = 6_000_000;
/// The spacing of a solid fill's lines, paper mm: drawn with a stroke as wide, they cover it.
pub(crate) const SOLID_SPACING: f64 = 0.2;

/// One family of parallel lines in a pattern: its angle to the pattern's, its phase as a
/// fraction of the spacing, and its dashes (on, off, … as multiples of the spacing; none: solid).
struct Family {
    angle: f64,
    phase: f64,
    dash: &'static [f64],
}

const fn fam(angle: f64, phase: f64, dash: &'static [f64]) -> Family {
    Family { angle, phase, dash }
}

impl Pattern {
    /// The line families that draw the pattern (none for one drawn otherwise).
    fn families(self) -> &'static [Family] {
        const LINES: [Family; 1] = [fam(0.0, 0.0, &[])];
        const CROSS: [Family; 2] = [fam(0.0, 0.0, &[]), fam(90.0, 0.0, &[])];
        // Pairs of close lines (steel, metal in section).
        const STEEL: [Family; 2] = [fam(0.0, 0.0, &[]), fam(0.0, 0.25, &[])];
        const DASHED: [Family; 1] = [fam(0.0, 0.0, &[2.0, 1.0])];
        match self {
            Pattern::Lines | Pattern::Solid => &LINES,
            Pattern::Cross => &CROSS,
            Pattern::Steel => &STEEL,
            Pattern::Dashed => &DASHED,
            Pattern::Along => &[],
        }
    }
}

/// The segments of `pattern` at `angle` degrees, lines `spacing` drawing units apart, filling
/// the region bounded by `loops` (even-odd: a loop inside another is an island).
pub(crate) fn fill(loops: &[Vec<Pt>], pattern: Pattern, angle: f64, spacing: f64) -> Vec<(Pt, Pt)> {
    let edges: Vec<(Pt, Pt)> = loops
        .iter()
        .filter(|l| l.len() >= 3)
        .flat_map(|l| (0..l.len()).filter_map(move |i| Some((*l.get(i)?, *l.get((i + 1) % l.len())?))))
        .filter(|(a, b)| a.is_finite() && b.is_finite())
        .collect();
    let families = pattern.families();
    if edges.is_empty() || families.is_empty() || !(spacing.is_finite() && spacing > 0.0) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for f in families {
        let dir = Pt::new((angle + f.angle).to_radians().cos(), (angle + f.angle).to_radians().sin());
        let n = dir.left();
        let across = |p: Pt| p.x * n.x + p.y * n.y;
        let along = |p: Pt| p.x * dir.x + p.y * dir.y;
        let (lo, hi) = edges.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), (a, _)| (lo.min(across(*a)), hi.max(across(*a))));
        // Bounded before it starts: the spacing widens so the lines and their edge tests fit.
        let allowed = (MAX_SEGMENTS / families.len()).min(MAX_WORK / edges.len().max(1)).max(1);
        let step = spacing.max((hi - lo) / allowed as f64);
        let phase = f.phase * step;
        let first = ((lo - phase) / step).ceil();
        let count = ((hi - lo) / step).ceil() as usize + 1;
        for k in 0..count.min(allowed + 1) {
            let s = (first + k as f64) * step + phase;
            // Where the line `across = s` crosses the edges, as positions along it; an edge
            // counts when its ends are on either side (half-open, so a vertex counts once).
            let mut ts: Vec<f64> = edges
                .iter()
                .filter_map(|&(a, b)| {
                    let (sa, sb) = (across(a), across(b));
                    if (sa <= s) == (sb <= s) {
                        return None;
                    }
                    let u = (s - sa) / (sb - sa);
                    Some(along(a) + (along(b) - along(a)) * u)
                })
                .filter(|t| t.is_finite())
                .collect();
            ts.sort_by(f64::total_cmp);
            let base = n.scale(s);
            for &[t0, t1] in ts.as_chunks::<2>().0 {
                dashes(t0, t1, f.dash, step, &mut |a, b| out.push((base.plus(dir.scale(a)), base.plus(dir.scale(b)))));
                if out.len() >= MAX_SEGMENTS {
                    return out;
                }
            }
        }
    }
    out
}

/// The dashes of `pattern` (multiples of `step`, from position 0 of the line, so neighbouring
/// pieces line up) within `t0..t1`; the whole of it with no pattern.
fn dashes(t0: f64, t1: f64, pattern: &[f64], step: f64, emit: &mut dyn FnMut(f64, f64)) {
    let cycle: f64 = pattern.iter().sum::<f64>() * step;
    if pattern.is_empty() || !(cycle > 0.0 && cycle.is_finite()) {
        emit(t0, t1);
        return;
    }
    let mut t = (t0 / cycle).floor() * cycle;
    let mut k = 0usize;
    // At most a few hundred dashes per piece: a cycle too fine draws the piece whole.
    if (t1 - t0) / cycle > 500.0 {
        emit(t0, t1);
        return;
    }
    while t < t1 {
        let len = pattern.get(k % pattern.len()).copied().unwrap_or(1.0) * step;
        if k.is_multiple_of(2) {
            let (a, b) = (t.max(t0), (t + len).min(t1));
            if b > a {
                emit(a, b);
            }
        }
        t += len;
        k += 1;
    }
}

impl Drawing {
    /// The loops of hatch `e`: the closed boundaries it names, drawing units.
    pub fn hatch_loops(&self, e: &Entity) -> Vec<Vec<Pt>> {
        let Kind::Hatch { boundary, .. } = &e.kind else { return Vec::new() };
        boundary.iter().filter_map(|id| self.path_of(id).filter(|(_, closed)| *closed).map(|(pts, _)| pts)).collect()
    }

    /// The segments hatch `e` is drawn with, drawing units.
    pub fn hatch_lines(&self, e: &Entity) -> Vec<(Pt, Pt)> {
        let Kind::Hatch { pattern, angle, spacing, .. } = &e.kind else { return Vec::new() };
        let k = crate::edit::paper_per_unit(self);
        let s = if *pattern == Pattern::Solid { SOLID_SPACING } else { *spacing };
        fill(&self.hatch_loops(e), *pattern, *angle, s / k)
    }

    /// The region a click at `p` hatches: the smallest closed object around it, and the closed
    /// objects inside that one (islands, and islands in them). Their ids, the outer first.
    pub fn hatch_boundary_at(&self, p: Pt) -> Option<Vec<String>> {
        let closed: Vec<(String, Vec<Pt>)> = self
            .entities
            .iter()
            .enumerate()
            .filter(|(i, _)| self.selectable(*i))
            .filter_map(|(_, e)| self.path_of(&e.id).filter(|(_, c)| *c).map(|(pts, _)| (e.id.clone(), pts)))
            .collect();
        let area = |pts: &[Pt]| {
            (0..pts.len())
                .filter_map(|i| {
                    let (a, b) = (pts.get(i)?, pts.get((i + 1) % pts.len())?);
                    Some(a.x * b.y - b.x * a.y)
                })
                .sum::<f64>()
                .abs()
                / 2.0
        };
        let (outer_id, outer) = closed.iter().filter(|(_, pts)| crate::point_in_polygon(p, pts)).min_by(|a, b| area(&a.1).total_cmp(&area(&b.1)))?;
        let mut ids = vec![outer_id.clone()];
        ids.extend(
            closed.iter().filter(|(id, pts)| id != outer_id && pts.iter().all(|q| crate::point_in_polygon(*q, outer))).map(|(id, _)| id.clone()),
        );
        Some(ids)
    }
}
