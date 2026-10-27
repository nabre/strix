//! Plane geometry helpers.

/// A point or vector.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Pt {
    pub x: f64,
    pub y: f64,
}

impl Pt {
    pub const fn new(x: f64, y: f64) -> Pt {
        Pt { x, y }
    }

    pub fn plus(self, o: Pt) -> Pt {
        Pt::new(self.x + o.x, self.y + o.y)
    }

    pub fn minus(self, o: Pt) -> Pt {
        Pt::new(self.x - o.x, self.y - o.y)
    }

    pub fn scale(self, k: f64) -> Pt {
        Pt::new(self.x * k, self.y * k)
    }

    pub fn len(self) -> f64 {
        self.x.hypot(self.y)
    }

    /// The unit vector along `self` (zero for a zero vector).
    pub fn unit(self) -> Pt {
        let l = self.len();
        if l > 0.0 { self.scale(1.0 / l) } else { Pt::default() }
    }

    /// Rotated a quarter turn counter-clockwise: the left normal.
    pub fn left(self) -> Pt {
        Pt::new(-self.y, self.x)
    }

    /// The point at `angle` degrees and distance `r` from `self`.
    pub fn polar(self, r: f64, angle_deg: f64) -> Pt {
        let a = angle_deg.to_radians();
        Pt::new(self.x + r * a.cos(), self.y + r * a.sin())
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

/// Segments used for a full circle when no chord tolerance is given (outlines on screen);
/// arcs use a proportional share (at least 2).
pub const CIRCLE_SEGMENTS: usize = 96;
/// Most segments one arc is drawn with, whatever the tolerance asks.
const MAX_ARC_SEGMENTS: usize = 4096;

/// How many segments draw an arc of radius `r` sweeping `sweep` radians with chords at most
/// `tol` off it; with no usable tolerance, the fixed share of [`CIRCLE_SEGMENTS`]. A chord
/// spanning φ is r·(1 − cos φ/2) = 2r·sin²(φ/4) off the arc, so φ = 4·asin(√(tol/2r)): this
/// form keeps its digits where `acos(1 − tol/r)` rounds to 0 (huge radii).
pub fn arc_segments(r: f64, sweep: f64, tol: Option<f64>) -> usize {
    let n = match tol {
        Some(t) if t > 0.0 && r > t && t.is_finite() && r.is_finite() => {
            let span = 4.0 * (t / (2.0 * r)).sqrt().min(1.0).asin();
            if span > 0.0 { sweep / span } else { f64::INFINITY }
        }
        _ => CIRCLE_SEGMENTS as f64 * sweep / std::f64::consts::TAU,
    };
    if n.is_nan() { 2 } else { (n.ceil().min(MAX_ARC_SEGMENTS as f64) as usize).clamp(2, MAX_ARC_SEGMENTS) }
}

/// Points along an arc of centre `c` and radius `r` from `start` to `end` degrees,
/// counter-clockwise (an `end` below `start` goes past 360°).
pub fn arc_points(c: Pt, r: f64, start: f64, end: f64) -> Vec<Pt> {
    arc_points_tol(c, r, start, end, None)
}

/// [`arc_points`] with chords at most `tol` off the arc.
pub fn arc_points_tol(c: Pt, r: f64, start: f64, end: f64, tol: Option<f64>) -> Vec<Pt> {
    let mut sweep = (end - start) % 360.0;
    if sweep <= 0.0 {
        sweep += 360.0;
    }
    let n = arc_segments(r, sweep.to_radians(), tol);
    (0..=n).map(|k| c.polar(r, start + sweep * k as f64 / n as f64)).collect()
}

/// Points along a polyline segment from `a` to `b` with DXF bulge `bulge` (tan of a quarter of
/// the arc's included angle; positive bends left, 0 is straight). `b` is the last point; `a` is
/// not repeated.
pub fn bulge_points(a: Pt, b: Pt, bulge: f64) -> Vec<Pt> {
    bulge_points_tol(a, b, bulge, None)
}

/// [`bulge_points`] with chords at most `tol` off the arc.
pub fn bulge_points_tol(a: Pt, b: Pt, bulge: f64, tol: Option<f64>) -> Vec<Pt> {
    // As the kernel reads it: nearly straight is straight (the centre would be far off).
    if !crate::kernel::bulge_bends(a, b, bulge) {
        return vec![b];
    }
    let bulge = bulge.clamp(-1e3, 1e3);
    let theta = 4.0 * bulge.atan();
    let chord = b.minus(a);
    let d = chord.len();
    // DXF: a positive bulge turns counter-clockwise around a centre left of the chord, offset
    // from its midpoint by d/2 · (1 − b²)/(2b).
    let c = a.plus(chord.scale(0.5)).plus(chord.unit().left().scale(d / 2.0 * (1.0 - bulge * bulge) / (2.0 * bulge)));
    let r = c.minus(a).len();
    let start = (a.y - c.y).atan2(a.x - c.x);
    let n = arc_segments(r, theta.abs(), tol);
    // The last point is the vertex itself, not its rounding through the centre.
    (1..n)
        .map(|k| {
            let t = start + theta * k as f64 / n as f64;
            Pt::new(c.x + r * t.cos(), c.y + r * t.sin())
        })
        .chain(std::iter::once(b))
        .collect()
}
