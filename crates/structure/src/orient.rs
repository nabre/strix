//! Members oriented by the structure's axes: however a member was drawn, its local `x` runs left
//! to right (a vertical one bottom to top), so stretches, laws, signs and the side diagrams are
//! drawn on agree along the whole structure.

use crate::model::{Load, LoadAxes, Num, Structure};

/// Does a member from `a` to `b` run against the structure's axes (right to left, or straight
/// down)?
fn backwards(a: (f64, f64), b: (f64, f64)) -> bool {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = dx.hypot(dy);
    if !len.is_finite() || len <= 0.0 {
        return false;
    }
    let tol = 1e-9 * len;
    dx < -tol || (dx.abs() <= tol && dy < 0.0)
}

/// `len − v`, its expression dropped (the value is the one the structure has now).
fn mirrored(v: &Num, len: f64) -> Num {
    Num::new(len - v.value)
}

impl Structure {
    /// The structure with each member drawn against its axes turned round: its ends, its end
    /// hinges and the positions of the loads on it swapped, each load where it was. Its numbers'
    /// values (resolve them first); a remapped position loses its expression. The section
    /// ([`Structure::cut`]) is left as it is: it is always measured on oriented members.
    pub(crate) fn oriented(mut self) -> Structure {
        let mut turned: Vec<(String, f64)> = Vec::new();
        for i in 0..self.members.len() {
            let Some(m) = self.members.get(i) else { continue };
            let (Some(a), Some(b)) = (self.node(&m.a), self.node(&m.b)) else { continue };
            let (pa, pb) = ((a.x.value, a.y.value), (b.x.value, b.y.value));
            if !backwards(pa, pb) {
                continue;
            }
            let len = (pb.0 - pa.0).hypot(pb.1 - pa.1);
            if let Some(m) = self.members.get_mut(i) {
                std::mem::swap(&mut m.a, &mut m.b);
                std::mem::swap(&mut m.hinge_a, &mut m.hinge_b);
                turned.push((m.id.clone(), len));
            }
        }
        if turned.is_empty() {
            return self;
        }
        let length_of = |id: &str| turned.iter().find(|(m, _)| m == id).map(|&(_, len)| len);
        for l in &mut self.loads {
            match l {
                Load::Force { member: Some(m), at, .. } | Load::Moment { member: Some(m), at, .. } => {
                    if let Some(len) = length_of(m) {
                        *at = mirrored(at, len);
                    }
                }
                Load::Distributed { member, q1, q2, from, to, angle, axes, .. } => {
                    let Some(len) = length_of(member) else { continue };
                    // The solver puts `q1` at the nearer end of the stretch, `q2` at the farther.
                    let (s0, s1) = (from.value, to.as_ref().map_or(len, |t| t.value));
                    let (near, far) = if s0 <= s1 { (s0, s1) } else { (s1, s0) };
                    let whole = s0 == 0.0 && to.is_none();
                    *from = Num::new(len - far);
                    *to = if whole { None } else { Some(Num::new(len - near)) };
                    if let Some(q2) = q2.as_mut() {
                        std::mem::swap(q1, q2);
                    }
                    if *axes == LoadAxes::Local {
                        // The member's axis turned half round: the same direction is 180° away.
                        let a = angle.value + 180.0;
                        *angle = Num::new(if a > 180.0 { a - 360.0 } else { a });
                    }
                }
                _ => {}
            }
        }
        self
    }
}
