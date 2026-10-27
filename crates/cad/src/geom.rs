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

/// Segments used for a full circle; arcs use a proportional share (at least 2).
pub const CIRCLE_SEGMENTS: usize = 96;

/// Points along an arc of centre `c` and radius `r` from `start` to `end` degrees,
/// counter-clockwise (an `end` below `start` goes past 360°).
pub fn arc_points(c: Pt, r: f64, start: f64, end: f64) -> Vec<Pt> {
    let mut sweep = (end - start) % 360.0;
    if sweep <= 0.0 {
        sweep += 360.0;
    }
    let n = ((CIRCLE_SEGMENTS as f64 * sweep / 360.0).ceil() as usize).clamp(2, CIRCLE_SEGMENTS);
    (0..=n).map(|k| c.polar(r, start + sweep * k as f64 / n as f64)).collect()
}

/// Points along a polyline segment from `a` to `b` with DXF bulge `bulge` (tan of a quarter of
/// the arc's included angle; positive bends left, 0 is straight). `b` is the last point; `a` is
/// not repeated.
pub fn bulge_points(a: Pt, b: Pt, bulge: f64) -> Vec<Pt> {
    if bulge == 0.0 || !bulge.is_finite() || a == b {
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
    let n = ((CIRCLE_SEGMENTS as f64 * theta.abs() / std::f64::consts::TAU).ceil() as usize).clamp(2, CIRCLE_SEGMENTS);
    (1..=n)
        .map(|k| {
            let t = start + theta * k as f64 / n as f64;
            Pt::new(c.x + r * t.cos(), c.y + r * t.sin())
        })
        .collect()
}
