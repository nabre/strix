//! Moment arms written from what the drawing gives: the distances its dimension chains measure
//! and the loaded stretches' lengths, so a resultant's arm reads `(1 m + (3 m)/2)` (the chain up
//! to the load, then half the load's length), not a centroid's distance the drawing never shows.
//! A distance no chain measures (along an inclined member) is written by its size.

use strix_structure::{Resultant, ResultantKind};

use super::Units;

/// A sum of terms as a factor: in parentheses when it has more than one.
fn factor(terms: &[String]) -> String {
    match terms {
        [] => String::new(),
        [one] => one.clone(),
        many => format!("({})", many.join(" + ")),
    }
}

fn axis_of(p: (f64, f64), axis: usize) -> f64 {
    if axis == 0 { p.0 } else { p.1 }
}

impl Units {
    /// A length as the drawing's dimensions write it (`0.5 m`, `1.25 m`), so the working quotes
    /// them; a length with no short decimal (`(10 m)/3`) as a fraction.
    pub(super) fn measure(&self, v: f64) -> String {
        let a = v.abs();
        let r = (a * 1e4).round() / 1e4;
        if a.is_finite() && (a - r).abs() <= 1e-9 * a.max(1.0) {
            let t = super::precise(r, self.comma);
            if self.l.is_empty() { t } else { format!("{t} {}", self.l) }
        } else {
            self.length(a)
        }
    }

    /// The distance from `a` to `b` along `axis` as its chain's terms (`["3 m", "1 m"]`), or by
    /// its size when no chain measures it.
    fn distance(&self, axis: usize, a: f64, b: f64) -> Vec<String> {
        match self.chains.segments(axis, a, b) {
            Some(s) => s.iter().map(|d| self.measure(*d)).collect(),
            None if (a - b).abs() <= self.chains.tol.max(1e-12) => Vec::new(),
            None => vec![self.length(b - a)],
        }
    }

    /// The arm from `p` to `q` along `axis` (two stations or points), as a factor.
    pub(super) fn arm_between(&self, axis: usize, p: f64, q: f64) -> String {
        let t = factor(&self.distance(axis, p, q));
        if t.is_empty() { self.length(0.0) } else { t }
    }

    /// The arm along `axis` from `p` to resultant `r`'s point, as a factor: for a distributed
    /// load, the chain to its nearer end and the part of its length up to its centroid.
    pub(super) fn arm(&self, axis: usize, p: f64, r: &Resultant) -> String {
        let tol = self.chains.tol.max(1e-12);
        let c = axis_of(r.at, axis);
        let (a, b) = (axis_of(r.span.0, axis), axis_of(r.span.1, axis));
        let spread = matches!(r.kind, ResultantKind::Spread { .. });
        if !spread || (a - b).abs() <= tol {
            return self.arm_between(axis, p, c);
        }
        // The centroid's fraction of the stretch from `a`: a half, or two thirds for a triangle.
        let ta = (c - a) / (b - a);
        let span = factor(&self.distance(axis, a, b));
        let part = |t: f64| -> Option<String> {
            if (t - 0.5).abs() < 1e-6 {
                Some(format!("{}/2", wrap(&span)))
            } else if (t - 1.0 / 3.0).abs() < 1e-6 {
                Some(format!("{}/3", wrap(&span)))
            } else if (t - 2.0 / 3.0).abs() < 1e-6 {
                Some(format!("2·{}/3", wrap(&span)))
            } else {
                None
            }
        };
        let (lo, hi) = (a.min(b), a.max(b));
        let fallback = || self.length(c - p);
        if p <= lo + tol || p >= hi - tol {
            // Outside the stretch: up to its nearer end, then into it; or up to its farther end,
            // less the part beyond the centroid (`6 m - (3 m)/2`), when the chains say it shorter.
            let ((near, tn), (far, tf)) = if (p - a).abs() <= (p - b).abs() { ((a, ta), (b, 1.0 - ta)) } else { ((b, 1.0 - ta), (a, ta)) };
            let (Some(pn), Some(pf)) = (part(tn), part(tf)) else { return fallback() };
            let mut add = self.distance(axis, p, near);
            add.push(pn);
            let less = self.distance(axis, p, far);
            if less.len() + 1 < add.len() {
                return format!("({} - {pf})", less.join(" + "));
            }
            return factor(&add);
        }
        // Inside it: the part from the end on `p`'s side, less the chain from that end to `p`.
        let (end, t) = if (p - a) * (c - a) > 0.0 && (p - a).abs() < (c - a).abs() { (a, ta) } else { (b, 1.0 - ta) };
        let (Some(part), terms) = (part(t), self.distance(axis, end, p)) else { return fallback() };
        if (c - end).abs() < (p - end).abs() {
            return fallback();
        }
        format!("({part} - {})", wrap(&terms.join(" + ")))
    }
}

/// A term ready to be divided or subtracted: `(3 m)`, `(2 m + 1 m)`.
fn wrap(t: &str) -> String {
    match t.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
        Some(inner) if !inner.contains('(') && !inner.contains(')') => t.to_string(),
        _ => format!("({t})"),
    }
}
