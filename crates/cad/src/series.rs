//! Series (rectangular arrays) between a base point and an end point, spaced one of three ways,
//! and groups: an array's items made one object, picked and moved together (docs/cad.md).

use crate::geom::Pt;
use crate::model::Drawing;
use crate::rework::MAX_ARRAY;

/// How a series between a base point and an end point is spaced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Spacing {
    /// The items typed, the last one exactly on the end point: the spacing is what it comes to.
    #[default]
    Fit,
    /// The spacing typed, kept exactly: as many items as fit, what is left over past the last.
    Pitch,
    /// The spacing typed as the most there may be: as few items as keep within it, spread evenly
    /// so the last one is on the end point.
    Spread,
}

impl Spacing {
    pub const ALL: [(Spacing, &'static str); 3] = [(Spacing::Fit, "fit"), (Spacing::Pitch, "pitch"), (Spacing::Spread, "spread")];

    pub fn name(self) -> &'static str {
        Spacing::ALL.iter().find(|(s, _)| *s == self).map_or("fit", |(_, n)| n)
    }

    pub fn parse(s: &str) -> Option<Spacing> {
        Spacing::ALL.iter().find(|(_, n)| n.eq_ignore_ascii_case(s)).map(|(s, _)| *s)
    }
}

/// Between the names of a group path ([`crate::Entity::group`]): `outer/inner`.
pub(crate) const GROUP_SEP: char = '/';

/// The groups an entity is in, outermost first.
pub(crate) fn group_path(e: &crate::Entity) -> impl Iterator<Item = &str> {
    e.group.as_deref().into_iter().flat_map(|g| g.split(GROUP_SEP))
}

/// The outermost group an entity is in: the one picked, moved and exploded whole.
pub(crate) fn outer_group(e: &crate::Entity) -> Option<&str> {
    group_path(e).next()
}

/// Whether an entity is in group `name`, at any level.
pub(crate) fn in_group(e: &crate::Entity, name: &str) -> bool {
    group_path(e).any(|g| g == name)
}

/// Lengths shorter than this are no length (drawing units).
const EPS: f64 = 1e-9;

/// The items along a length `len` (signed) and the step between them: `count` items for
/// [`Spacing::Fit`], else as `pitch` says. Errors on a count, pitch or length out of range.
pub fn series_axis(len: f64, spacing: Spacing, count: usize, pitch: f64) -> Result<(usize, f64), String> {
    if !len.is_finite() {
        return Err("The end point is not a number".into());
    }
    match spacing {
        Spacing::Fit => {
            if count == 0 || count > MAX_ARRAY {
                return Err(format!("An array has 1 to {MAX_ARRAY} items"));
            }
            if count == 1 {
                return Ok((1, 0.0));
            }
            if len.abs() < EPS {
                return Err("The end point is on the base point: the items would overlap".into());
            }
            Ok((count, len / (count - 1) as f64))
        }
        Spacing::Pitch | Spacing::Spread => {
            if !(pitch.is_finite() && pitch > EPS) {
                return Err("Type the spacing first".into());
            }
            if len.abs() < EPS {
                return Ok((1, 0.0));
            }
            let parts = len.abs() / pitch;
            // A hair's tolerance: 100 / 25 is 4 spaces, not 3.999… or 4.000…1.
            let spaces = if spacing == Spacing::Pitch { (parts + 1e-9).floor() } else { (parts - 1e-9).ceil().max(1.0) };
            if !(spaces.is_finite() && spaces < MAX_ARRAY as f64) {
                return Err(format!("An array has 1 to {MAX_ARRAY} items"));
            }
            let n = spaces as usize + 1;
            if n == 1 {
                return Ok((1, 0.0));
            }
            let step = if spacing == Spacing::Pitch { pitch.copysign(len) } else { len / spaces };
            Ok((n, step))
        }
    }
}

/// What a series came to: its items each way and the steps between them (drawing units).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Series {
    pub cols: usize,
    pub rows: usize,
    /// From one column to the next (along the line, for a single row or column).
    pub col_step: Pt,
    /// From one row to the next.
    pub row_step: Pt,
}

impl Series {
    /// The series from `from` to `to`: a single row or column (`cols` or `rows` 1) runs along
    /// the line between them, in any direction; more of each make a grid with `to` its far
    /// corner, columns along x and rows along y. For [`Spacing::Fit`] the counts are `cols` ×
    /// `rows`; otherwise they come from the lengths and `pitch`.
    pub fn plan(from: Pt, to: Pt, cols: usize, rows: usize, spacing: Spacing, pitch: f64) -> Result<Series, String> {
        if cols == 0 || rows == 0 || cols.saturating_mul(rows) > MAX_ARRAY {
            return Err(format!("An array has 1 to {MAX_ARRAY} items"));
        }
        let d = to.minus(from);
        if cols == 1 || rows == 1 {
            let len = d.len();
            let (n, step) = series_axis(len, spacing, cols.max(rows), pitch)?;
            let unit = if len > EPS { d.scale(1.0 / len) } else { Pt::default() };
            return Ok(Series { cols: n, rows: 1, col_step: unit.scale(step), row_step: Pt::default() });
        }
        let (c, dx) = series_axis(d.x, spacing, cols, pitch)?;
        let (r, dy) = series_axis(d.y, spacing, rows, pitch)?;
        if c.saturating_mul(r) > MAX_ARRAY {
            return Err(format!("An array has 1 to {MAX_ARRAY} items"));
        }
        Ok(Series { cols: c, rows: r, col_step: Pt::new(dx, 0.0), row_step: Pt::new(0.0, dy) })
    }

    /// Each item's offset from the first (the originals), every item but the first.
    fn offsets(&self) -> Vec<Pt> {
        let (c, r) = (self.col_step, self.row_step);
        (0..self.rows).flat_map(|j| (0..self.cols).map(move |i| c.scale(i as f64).plus(r.scale(j as f64)))).skip(1).collect()
    }
}

impl Drawing {
    /// A series of the entities at `indices` from `from` to `to`, `(cols, rows)` as
    /// [`Series::plan`] takes them, made one group with them. Returns the copies' indices and the series.
    pub fn array_series(
        &mut self,
        indices: &[usize],
        from: Pt,
        to: Pt,
        (cols, rows): (usize, usize),
        spacing: Spacing,
        pitch: f64,
    ) -> Result<(Vec<usize>, Series), String> {
        let s = Series::plan(from, to, cols, rows, spacing, pitch)?;
        let items = s.cols.saturating_mul(s.rows);
        if items < 2 {
            return Err("The spacing is longer than from the base point to the end point: no copies".into());
        }
        if !(s.col_step.x.is_finite() && s.col_step.y.is_finite() && s.row_step.x.is_finite() && s.row_step.y.is_finite()) {
            return Err("The spacing is not a number".into());
        }
        self.room_for(indices.len(), items)?;
        let copies = self.copies(indices, &s.offsets()).concat();
        self.group_with(indices, &copies);
        Ok((copies, s))
    }

    /// Makes the entities at `indices` and `more` (and the hatches' boundaries) one group, a new
    /// one, around the groups they are in: those stay inside it (an array of arrays, or of
    /// blocks, keeps them; Explode takes one level apart at a time).
    pub(crate) fn group_with(&mut self, indices: &[usize], more: &[usize]) {
        let mut all = self.with_boundaries(indices);
        all.extend_from_slice(more);
        let mut seen = std::collections::HashSet::new();
        all.retain(|i| seen.insert(*i));
        let name = self.fresh_group();
        for i in all {
            if let Some(e) = self.entities.get_mut(i) {
                e.group = Some(match e.group.take() {
                    Some(inner) => format!("{name}{GROUP_SEP}{inner}"),
                    None => name.clone(),
                });
            }
        }
    }

    /// A group name no entity's group path holds: `g1`, `g2`…
    pub(crate) fn fresh_group(&self) -> String {
        let taken: std::collections::HashSet<&str> = self.entities.iter().flat_map(group_path).collect();
        let mut n = 1usize;
        while taken.contains(format!("g{n}").as_str()) {
            n += 1;
        }
        format!("g{n}")
    }

    /// `indices` with the other members of their outermost groups, each once, in order.
    pub fn with_groups(&self, indices: &[usize]) -> Vec<usize> {
        let groups: std::collections::HashSet<&str> = indices.iter().filter_map(|&i| outer_group(self.entities.get(i)?)).collect();
        let mut out: Vec<usize> = indices.iter().copied().filter(|&i| i < self.entities.len()).collect();
        if !groups.is_empty() {
            out.extend(self.entities.iter().enumerate().filter(|(_, e)| outer_group(e).is_some_and(|g| groups.contains(g))).map(|(i, _)| i));
        }
        let mut seen = std::collections::HashSet::new();
        out.retain(|i| seen.insert(*i));
        out
    }

    /// Takes the entities at `indices` out of their outermost groups (the whole groups: a group
    /// never keeps a part), one level: the groups inside stay. A block insertion taken apart is
    /// no longer one. Returns how many left a group.
    pub fn ungroup(&mut self, indices: &[usize]) -> usize {
        let all = self.with_groups(indices);
        let mut n = 0;
        for i in all {
            if let Some(e) = self.entities.get_mut(i)
                && let Some(g) = e.group.take()
            {
                e.group = g.split_once(GROUP_SEP).map(|(_, inner)| inner.to_string());
                n += 1;
            }
        }
        if n > 0 && !self.inserts.is_empty() {
            let groups: std::collections::HashSet<String> = self.entities.iter().flat_map(group_path).map(str::to_string).collect();
            self.inserts.retain(|r| groups.contains(&r.group));
        }
        n
    }

    /// Whether any of the entities at `indices` is in a group.
    pub fn any_grouped(&self, indices: &[usize]) -> bool {
        indices.iter().any(|&i| self.entities.get(i).is_some_and(|e| e.group.is_some()))
    }
}
