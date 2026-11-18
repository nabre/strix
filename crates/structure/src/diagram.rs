//! Notable values of the diagrams: at the ends of each stretch, at jumps, extremes and zeros.

use crate::solve::{Forces, MemberResult};

/// Which internal force.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Axial force N.
    Axial,
    /// Shear V (T).
    Shear,
    /// Bending moment M.
    Moment,
}

impl Action {
    pub fn of(self, f: Forces) -> f64 {
        match self {
            Action::Axial => f.n,
            Action::Shear => f.v,
            Action::Moment => f.m,
        }
    }
}

/// What makes a value notable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotableKind {
    /// At the end of a stretch (a member's end, a load's position).
    End,
    /// A local maximum or minimum inside a stretch (M where V is zero).
    Extreme,
    /// The diagram crosses zero.
    Zero,
}

/// A notable value at `x` along a member.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Notable {
    pub x: f64,
    pub value: f64,
    pub kind: NotableKind,
}

const SAMPLES: usize = 48;

/// The notable values of `action` along member `m`, by position.
pub fn notables(m: &MemberResult, action: Action) -> Vec<Notable> {
    let bps = m.breakpoints();
    let val = |x: f64, after: bool| action.of(m.forces(x, after));
    let scale = bps.iter().flat_map(|&x| [val(x, false).abs(), val(x, true).abs()]).fold(0.0_f64, f64::max);
    let tol = 1e-9 * scale.max(1e-12);
    let mut out: Vec<Notable> = Vec::new();
    let push = |n: Notable, out: &mut Vec<Notable>| {
        let same =
            out.last().is_some_and(|l| (l.x - n.x).abs() <= 1e-9 * (1.0 + m.length) && (l.value - n.value).abs() <= tol.max(1e-9 * n.value.abs()));
        if !same {
            out.push(n);
        }
    };
    for w in bps.windows(2) {
        let (x0, x1) = (w[0], w[1]);
        if x1 - x0 <= 1e-12 * (1.0 + m.length) {
            continue;
        }
        let v0 = val(x0, true);
        push(Notable { x: x0, value: clean(v0, tol), kind: NotableKind::End }, &mut out);
        // Inside: extremes of M (zeros of V) and zeros of the action.
        let mut inner: Vec<Notable> = Vec::new();
        let xs: Vec<f64> = (0..=SAMPLES).map(|k| x0 + (x1 - x0) * k as f64 / SAMPLES as f64).collect();
        let at = |x: f64, a: Action| a.of(m.forces(x, x <= x0));
        if action == Action::Moment {
            for x in sign_changes(&xs, |x| at(x, Action::Shear), tol_for(m, Action::Shear, &bps)) {
                let v = at(x, Action::Moment);
                inner.push(Notable { x, value: clean(v, tol), kind: NotableKind::Extreme });
            }
        }
        for x in sign_changes(&xs, |x| at(x, action), tol) {
            inner.push(Notable { x, value: 0.0, kind: NotableKind::Zero });
        }
        inner.sort_by(|a, b| a.x.total_cmp(&b.x));
        for n in inner {
            push(n, &mut out);
        }
        let v1 = val(x1, false);
        push(Notable { x: x1, value: clean(v1, tol), kind: NotableKind::End }, &mut out);
    }
    out
}

fn tol_for(m: &MemberResult, a: Action, bps: &[f64]) -> f64 {
    1e-9 * bps.iter().map(|&x| a.of(m.forces(x, true)).abs()).fold(1e-12, f64::max)
}

fn clean(v: f64, tol: f64) -> f64 {
    if v.abs() <= tol { 0.0 } else { v }
}

/// Where `f` changes sign strictly inside the sampled range (bisected), ignoring the ends.
fn sign_changes(xs: &[f64], f: impl Fn(f64) -> f64, tol: f64) -> Vec<f64> {
    let mut out = Vec::new();
    let vals: Vec<f64> = xs.iter().map(|&x| f(x)).collect();
    let n = xs.len();
    for k in 0..n.saturating_sub(1) {
        let (Some(&a), Some(&b), Some(&fa), Some(&fb)) = (xs.get(k), xs.get(k + 1), vals.get(k), vals.get(k + 1)) else { continue };
        let first = k == 0;
        let last = k + 2 == n;
        // A zero at an end of the stretch is the end's value, not a crossing.
        if (first && fa.abs() <= tol) || (last && fb.abs() <= tol) {
            continue;
        }
        let crossing =
            (fa < -tol && fb > tol) || (fa > tol && fb < -tol) || (fb.abs() <= tol && !last && vals.get(k + 2).is_some_and(|&fc| fc * fa < 0.0));
        if !crossing {
            continue;
        }
        let (mut lo, mut hi, mut flo) = (a, b, fa);
        if fb.abs() <= tol {
            out.push(b);
            continue;
        }
        for _ in 0..60 {
            let mid = 0.5 * (lo + hi);
            let fm = f(mid);
            if fm.abs() <= tol * 1e-3 {
                lo = mid;
                hi = mid;
                break;
            }
            if (fm < 0.0) == (flo < 0.0) {
                lo = mid;
                flo = fm;
            } else {
                hi = mid;
            }
        }
        let x = 0.5 * (lo + hi);
        if out.last().is_none_or(|l: &f64| (x - l).abs() > 1e-9) {
            out.push(x);
        }
    }
    out
}
