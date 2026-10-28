//! Dimension chains: continued and baseline dimensions (AutoCAD's Continue and Baseline), drawn
//! from a linear dimension already in the drawing.

use crate::edit::{dim_offset, paper_per_unit};
use crate::geom::Pt;
use crate::model::{Drawing, Entity, Kind};

impl Drawing {
    /// The newest linear dimension (the last one in the list): the base of a chain when none is
    /// chosen.
    pub fn last_linear_dim(&self) -> Option<usize> {
        self.entities.iter().rposition(|e| matches!(e.kind, Kind::Dim { .. }))
    }

    /// The row spacing of baseline dimensions, paper mm: a text and its gaps.
    pub fn dim_row(&self) -> f64 {
        let st = self.dim_style;
        (st.text * 1.5 + st.text_gap).max(2.0)
    }

    /// A linear dimension continuing dimension `base` to `p`: from the base's second point, its
    /// line on the base's line, the same orientation and look. None when `base` is no linear
    /// dimension or `p` is its second point.
    pub fn dim_continue(&self, base: usize, p: Pt) -> Option<Entity> {
        let e = self.entities.get(base)?;
        let Kind::Dim { b, .. } = &e.kind else { return None };
        let from = b.clone();
        self.chained(e, from, p, 0.0)
    }

    /// A baseline dimension from dimension `base` to `p`: from the base's first point, its line
    /// one row ([`Drawing::dim_row`]) beyond the base's, away from the measured points.
    pub fn dim_baseline(&self, base: usize, p: Pt) -> Option<Entity> {
        let e = self.entities.get(base)?;
        let Kind::Dim { a, .. } = &e.kind else { return None };
        let from = a.clone();
        self.chained(e, from, p, self.dim_row())
    }

    /// Flips the dimensions among `indices` (the rest are left alone): a linear dimension's line
    /// goes to the other side of its points, an angular dimension measures the other side of its
    /// arc (the angle's complement to a full turn), a level mark hangs under its level or stands
    /// on it again. Returns how many were flipped.
    pub fn flip_dims(&mut self, indices: &[usize]) -> usize {
        let mut n = 0;
        for &i in indices {
            let Some(e) = self.entities.get_mut(i) else { continue };
            match &mut e.kind {
                Kind::Dim { offset, offset_units, .. } => {
                    *offset = -*offset;
                    *offset_units = -*offset_units;
                }
                Kind::Angle { a, b, .. } => std::mem::swap(a, b),
                Kind::Level { flip, .. } => *flip = !*flip,
                _ => continue,
            }
            n += 1;
        }
        n
    }

    /// A copy of dimension `base` measuring from `from` to `p`, its line on the base's line moved
    /// `shift` paper mm further out.
    fn chained(&self, base: &Entity, from: crate::model::P, p: Pt, shift: f64) -> Option<Entity> {
        let Kind::Dim { orient, mark, offset_units, .. } = &base.kind else { return None };
        let k = paper_per_unit(self);
        let frame = self.dim_frame(base, k)?;
        let a = from.pt();
        if p.minus(a).len() <= crate::kernel::RESOLUTION {
            return None;
        }
        // A point on the new line, in drawing units: the base's line, `shift` further out.
        let at = frame.fa.plus(frame.normal.scale(frame.side * shift)).scale(1.0 / k);
        // The scaled part as the base's; the paper part makes up the rest.
        let offset = dim_offset(self, a, p, *orient, at) - offset_units * k;
        let mut e = base.clone();
        e.id = String::new();
        e.kind = Kind::Dim {
            a: from,
            b: crate::model::P::new(p.x, p.y),
            offset,
            offset_units: *offset_units,
            orient: *orient,
            text: None,
            below: None,
            mark: *mark,
        };
        Some(e)
    }
}
