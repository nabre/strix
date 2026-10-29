//! The two parts of a dimension's distance, for the editor's overlay: a linear dimension's line
//! sits a scaled distance (drawing units, it follows the drawing) plus an annotative one (paper
//! mm, the same in print) from its measured points; an angle's arc a scaled radius plus an
//! annotative one from its vertex. [`Drawing::dim_parts`] says where each part lies, in drawing
//! units (y up), so the editor can show the reference line or arc at the scaled distance and
//! the stretch from there to the dimension line or arc. Never part of the printed drawing.

use crate::edit::{dim_dir, paper_per_unit};
use crate::geom::Pt;
use crate::model::{Drawing, Entity, Kind};
use crate::render::RIGHT_ANGLE_TOL;

/// Most segments a reference arc is drawn with.
const MAX_REFERENCE_SEGMENTS: usize = 120;

/// Where a dimension's distance is scaled and where it is annotative, in drawing units.
#[derive(Debug, Clone, PartialEq)]
pub struct DimParts {
    /// The reference at the scaled distance: a linear dimension's line parallel to its
    /// dimension line between the extension lines, an angle's arc (or, for a right angle drawn as
    /// a square, the square) at its scaled radius. One point (the vertex) for an angle with no
    /// scaled radius.
    pub reference: Vec<Pt>,
    /// The scaled part, along the normal (an angle's bisector) in the dimension's middle: from
    /// where the distance starts (the farther measured point's level, the vertex) to the
    /// reference. Both ends the same point when the part is zero.
    pub scaled: [Pt; 2],
    /// The annotative part: from the reference on to the dimension line or arc.
    pub annotative: [Pt; 2],
    /// The parts' sizes: the scaled one in drawing units, the annotative one in paper mm (signed
    /// for a linear dimension: negative when it runs back towards the measured points).
    pub scaled_size: f64,
    pub annotative_size: f64,
}

impl Drawing {
    /// The scaled and annotative parts of dimension `e`'s distance (an entity of this drawing,
    /// or one about to be: its scale and style count); None when it is no linear or angular
    /// dimension or it is degenerate (its points on each other, a zero angle).
    pub fn dim_parts(&self, e: &Entity) -> Option<DimParts> {
        let k = paper_per_unit(self);
        let parts = match &e.kind {
            Kind::Dim { a, b, offset, offset_units, orient, .. } => {
                let (a, b) = (a.pt(), b.pt());
                let dir = dim_dir(a, b, *orient);
                if dir.len() == 0.0 {
                    return None;
                }
                let n = dir.left();
                let level = |q: Pt| q.x * n.x + q.y * n.y;
                // As the rendering places the line: past the farther point along the normal by
                // the whole distance (the paper part in drawing units plus the scaled part).
                let total = offset / k + offset_units;
                let base = if total >= 0.0 { level(a).max(level(b)) } else { level(a).min(level(b)) };
                let (reference, line) = (base + offset_units, base + total);
                let at = |q: Pt, l: f64| q.plus(n.scale(l - level(q)));
                let m = a.plus(b).scale(0.5);
                DimParts {
                    reference: vec![at(a, reference), at(b, reference)],
                    scaled: [at(m, base), at(m, reference)],
                    annotative: [at(m, reference), at(m, line)],
                    scaled_size: *offset_units,
                    annotative_size: *offset,
                }
            }
            Kind::Angle { c, a, b, r, r_paper, text, .. } => {
                let (c, a, b) = (c.pt(), a.pt(), b.pt());
                let (ua, ub) = (a.minus(c).unit(), b.minus(c).unit());
                let (start, sweep) = (crate::kernel::direction(ua), crate::kernel::angle_ccw(ua, ub));
                if ua.len() == 0.0 || ub.len() == 0.0 || sweep == 0.0 {
                    return None;
                }
                let (scaled, arc) = (r.max(0.0), r.max(0.0) + r_paper.max(0.0) / k);
                // A right angle with no text of its own is drawn as a square of side `r` on the
                // vertex: its reference is the square too, the parts along its diagonal.
                let square = text.is_none() && (sweep - std::f64::consts::FRAC_PI_2).abs() <= RIGHT_ANGLE_TOL;
                let along = if square {
                    ua.plus(ub)
                } else {
                    let half = start + sweep / 2.0;
                    Pt::new(half.cos(), half.sin())
                };
                let reference = if scaled <= 0.0 {
                    vec![c]
                } else if square {
                    vec![c.plus(ua.scale(scaled)), c.plus(ua.plus(ub).scale(scaled)), c.plus(ub.scale(scaled))]
                } else {
                    // About 3° a segment, at least two.
                    let steps = ((sweep.to_degrees() / 3.0).ceil() as usize).clamp(2, MAX_REFERENCE_SEGMENTS);
                    (0..=steps)
                        .map(|s| {
                            let t = start + sweep * s as f64 / steps as f64;
                            c.plus(Pt::new(t.cos(), t.sin()).scale(scaled))
                        })
                        .collect()
                };
                let (mid_ref, mid_arc) = (c.plus(along.scale(scaled)), c.plus(along.scale(arc)));
                DimParts { reference, scaled: [c, mid_ref], annotative: [mid_ref, mid_arc], scaled_size: scaled, annotative_size: r_paper.max(0.0) }
            }
            _ => return None,
        };
        let finite = parts.reference.iter().chain(&parts.scaled).chain(&parts.annotative).all(|p| p.is_finite());
        finite.then_some(parts)
    }
}
