//! Limits, `lim┬(x→a)⁡f(x)`, computed numerically.
//!
//! Each side is approached on its own (`x = a ± s·h` with `h → 0`, or `x = ±1/h` for a limit at
//! infinity), two ways:
//!
//! 1. **Extrapolation.** `f` is sampled at `h = 2⁻², …, 2⁻¹¹` and the polynomial through the
//!    samples is read at `h = 0` (Neville). Where `f` is smooth in `h` this is right to ten
//!    digits without ever going near the point, so `(1 − cos x)/x²` does not cancel to nothing.
//!    It is accepted when leaving the first sample out gives the same value.
//! 2. **Approach.** Otherwise `f` is sampled at `h = 10⁻¹, …, 10⁻¹²`: the values must settle
//!    (`e^(−1/x)`), or close in geometrically, and then the sequence is extrapolated (`√x`,
//!    `x·ln x`).
//!
//! A limit that does neither is an error, never a number: values that keep growing are an
//! infinite limit, anything else does not settle (`sin(1/x)`). The two sides must agree; a
//! side where the function does not exist (`√x` left of 0) is left out.

use crate::CalcError;

/// The side a limit is taken from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum From {
    Both,
    Right,
    Left,
}

/// Relative tolerance two estimates of a limit agree within.
const TOLERANCE: f64 = 1e-7;

/// `v` rounded to `digits` significant digits.
fn round_to(v: f64, digits: i32) -> f64 {
    if v == 0.0 || !v.is_finite() {
        return v;
    }
    let magnitude = v.abs().log10().floor();
    let scale = 10f64.powf(f64::from(digits - 1) - magnitude);
    let rounded = (v * scale).round() / scale;
    if rounded.is_finite() { rounded } else { v }
}

/// The value at 0 of the polynomial through `(hs[i], vs[i])` (Neville's scheme).
fn neville(hs: &[f64], vs: &[f64]) -> f64 {
    let mut p: Vec<f64> = vs.to_vec();
    let n = p.len().min(hs.len());
    for level in 1..n {
        for i in 0..n.saturating_sub(level) {
            let (Some(&hi), Some(&hj)) = (hs.get(i), hs.get(i + level)) else { continue };
            let (Some(&a), Some(&b)) = (p.get(i), p.get(i + 1)) else { continue };
            let denominator = hi - hj;
            if denominator == 0.0 {
                continue;
            }
            if let Some(slot) = p.get_mut(i) {
                // P(0) from the two lower-order values.
                *slot = (hi * b - hj * a) / denominator;
            }
        }
    }
    p.first().copied().unwrap_or(f64::NAN)
}

/// The limit of `g(h)` for `h → 0⁺`.
fn one_sided(g: &mut dyn FnMut(f64) -> Result<f64, CalcError>) -> Result<f64, CalcError> {
    // 1. Extrapolation over smooth samples.
    let hs: Vec<f64> = (2..=11).map(|k| 0.5f64.powi(k)).collect();
    let mut vs = Vec::with_capacity(hs.len());
    for &h in &hs {
        vs.push(g(h)?);
    }
    let scale = vs.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    let all = neville(&hs, &vs);
    let fewer = neville(hs.get(1..).unwrap_or_default(), vs.get(1..).unwrap_or_default());
    if all.is_finite() && fewer.is_finite() && (all - fewer).abs() <= TOLERANCE * scale.max(all.abs()) {
        return Ok(if all.abs() <= 1e-9 * scale { 0.0 } else { round_to(all, 10) });
    }
    // 2. Approach.
    let mut vs = Vec::with_capacity(12);
    for k in 1..=12 {
        vs.push(g(10f64.powi(-k))?);
    }
    let scale = vs.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    let diffs: Vec<f64> = vs.windows(2).map(|w| match w {
        [a, b] => b - a,
        _ => 0.0,
    }).collect();
    let last = vs.last().copied().unwrap_or(f64::NAN);
    let tail = diffs.get(diffs.len().saturating_sub(4)..).unwrap_or_default();
    let snap = |v: f64| if v.abs() <= 1e-6 * scale { 0.0 } else { v };
    if tail.iter().all(|d| d.abs() <= 1e-6 * scale) {
        return Ok(snap(round_to(last, 8)));
    }
    // Closing in geometrically: each step a steady fraction (at most half) of the one before.
    let ratios: Vec<f64> = tail.windows(2).map(|w| match w {
        [a, b] if *a != 0.0 => b / a,
        _ => f64::NAN,
    }).collect();
    let steady = ratios.iter().all(|r| *r > 0.0 && *r <= 0.5) && ratios.windows(2).all(|w| matches!(w, [a, b] if (a - b).abs() <= 0.1 * a.abs()));
    if steady && let (Some(&r), Some(&d)) = (ratios.last(), tail.last()) {
        let limit = last + d * r / (1.0 - r);
        if limit.is_finite() && (limit - last).abs() <= 1e-2 * scale {
            return Ok(snap(round_to(limit, 6)));
        }
    }
    // Growing without end, on one side of zero: an infinite limit.
    let grows = vs.get(vs.len().saturating_sub(6)..).unwrap_or_default();
    let same_sign = grows.iter().all(|v| *v > 0.0) || grows.iter().all(|v| *v < 0.0);
    let rising = grows.windows(2).all(|w| matches!(w, [a, b] if b.abs() > a.abs()));
    let unbroken = tail.windows(2).all(|w| matches!(w, [a, b] if b.abs() >= 0.9 * a.abs()));
    if same_sign && rising && unbroken {
        return Err(CalcError::Structure(if last > 0.0 { "the limit is infinite (+∞)" } else { "the limit is infinite (−∞)" }));
    }
    Err(CalcError::Structure("the limit does not settle: it does not exist, or it is approached too slowly to compute"))
}

/// The limit of `f(x)` for `x → a` (`a` may be `±∞`); `f` takes and gives SI magnitudes.
pub(crate) fn limit(f: &mut dyn FnMut(f64) -> Result<f64, CalcError>, a: f64, from: From) -> Result<f64, CalcError> {
    if a.is_nan() {
        return Err(CalcError::NotReal);
    }
    if a.is_infinite() {
        let sign = a.signum();
        return one_sided(&mut |h| f(sign / h));
    }
    let s = a.abs().max(1.0);
    let mut side = |sign: f64| one_sided(&mut |h| f(a + sign * s * h));
    match from {
        From::Right => side(1.0),
        From::Left => side(-1.0),
        From::Both => {
            let (right, left) = (side(1.0), side(-1.0));
            // A side where the function does not exist takes no part.
            let outside = |e: &CalcError| matches!(e, CalcError::Domain { .. } | CalcError::NotReal);
            match (right, left) {
                (Ok(r), Ok(l)) => {
                    if (r - l).abs() <= 1e-6 * r.abs().max(l.abs()).max(f64::MIN_POSITIVE) || (r == 0.0 && l == 0.0) {
                        Ok(r)
                    } else {
                        Err(CalcError::Structure("the limits from the left and from the right differ: ask for one side, x→a⁺ or x→a⁻"))
                    }
                }
                (Ok(v), Err(e)) | (Err(e), Ok(v)) => {
                    if outside(&e) {
                        Ok(v)
                    } else {
                        Err(e)
                    }
                }
                (Err(e), Err(_)) => Err(e),
            }
        }
    }
}
