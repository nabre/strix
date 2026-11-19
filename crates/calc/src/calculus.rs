//! Calculus: derivatives of user functions (`f′(2)`, `f″(2)`) and definite integrals
//! (`∫_0^2 f(x) dx`), both computed numerically.
//!
//! - **Derivatives** are central differences refined by one Richardson step, so a smooth
//!   function's slope is right to about ten significant digits.
//! - **Integrals** use adaptive Gauss–Kronrod (7–15 points), splitting the worst interval until
//!   the estimated error is below 10⁻¹⁰ of the result or [`MAX_INTERVALS`] are in use. The
//!   nodes never touch the limits, so `∫_0^1 1/√x dx` works.
//!
//! - **Improper integrals** (`∫_0^∞`, `∫_(-∞)^∞`) are taken decade by decade from the finite
//!   limit (or from 0) and then over the tail, mapped onto a finite interval; one that does not
//!   settle is reported as not converging.
//!
//! Results are rounded to [`DIGITS`] significant digits, which hides the last bits of noise
//! (`f′(1) = 2`, not `1.9999999999`).

use crate::CalcError;
use crate::parser::{BinOp, Expr};

/// The most primes a derivative takes (`f″`).
pub(crate) const MAX_ORDER: usize = 2;

/// The most intervals an integral is split into.
const MAX_INTERVALS: usize = 200;

/// Significant digits kept in a result.
const DIGITS: i32 = 10;

/// The prime written for each order of derivative.
pub(crate) const PRIME: char = '′';

/// Reads primes after a name ending at character `i` (`'`, `′`, `″`, `‴`, also as a superscript
/// `^′`) into `name` as [`PRIME`]s; returns the index after them.
pub(crate) fn primes(chars: &[(usize, char)], i: usize, name: &mut String) -> usize {
    let ch = |i: usize| chars.get(i).map(|&(_, c)| c);
    let count = |c: char| match c {
        '\'' | '′' => 1,
        '″' => 2,
        '‴' => 3,
        _ => 0,
    };
    let start = if ch(i) == Some('^') && ch(i.saturating_add(1)).is_some_and(|c| count(c) > 0) { i.saturating_add(1) } else { i };
    let mut j = start;
    let mut n = 0usize;
    while let Some(k) = ch(j).map(count).filter(|&k| k > 0) {
        n = n.saturating_add(k);
        j = j.saturating_add(1);
    }
    if n == 0 {
        return i;
    }
    for _ in 0..n.min(8) {
        name.push(PRIME);
    }
    j
}

/// Splits trailing primes off a name: `("f", 2)` for `f′′`.
pub(crate) fn split_primes(name: &str) -> (&str, usize) {
    let base = name.trim_end_matches(PRIME);
    let order = name.chars().rev().take_while(|&c| c == PRIME).count();
    (base, order)
}

/// The variable a differential names: `x` for `dx` or `ⅆx`.
fn differential(name: &str) -> Option<String> {
    let rest = name.strip_prefix('d').or_else(|| name.strip_prefix('ⅆ'))?;
    (!rest.is_empty() && rest.chars().next().is_some_and(char::is_alphabetic)).then(|| rest.to_string())
}

fn is_d(e: &Expr) -> bool {
    matches!(e, Expr::Name(n) | Expr::Unit(n) if n == "d" || n == "ⅆ")
}

/// Takes the differential off the end of an integrand: `(x², "x")` for `x² dx`, also `d x`,
/// `ⅆx` and `1/x dx` (read `1/(x·dx)`). `Err` gives the expression back when it ends with none.
pub(crate) fn split_differential(e: Expr) -> Result<(Expr, String), Expr> {
    match e {
        Expr::Name(ref n) | Expr::Unit(ref n) => match differential(n) {
            Some(var) => Ok((Expr::Num(1.0), var)),
            None => Err(e),
        },
        Expr::Bin(BinOp::Mul, l, r) => {
            if let Expr::Name(n) | Expr::Unit(n) = &*r {
                if let Some(var) = differential(n) {
                    return Ok((*l, var));
                }
                if is_d(&l) {
                    return Ok((Expr::Num(1.0), n.clone()));
                }
                if let Expr::Bin(BinOp::Mul, ll, lr) = &*l
                    && is_d(lr)
                {
                    return Ok(((**ll).clone(), n.clone()));
                }
            }
            match split_differential(*r) {
                Ok((body, var)) => Ok((Expr::Bin(BinOp::Mul, l, Box::new(body)), var)),
                Err(r) => Err(Expr::Bin(BinOp::Mul, l, Box::new(r))),
            }
        }
        Expr::Bin(BinOp::Div, l, r) => match split_differential(*r) {
            Ok((body, var)) => Ok((Expr::Bin(BinOp::Div, l, Box::new(body)), var)),
            Err(r) => Err(Expr::Bin(BinOp::Div, l, Box::new(r))),
        },
        other => Err(other),
    }
}

/// `v` rounded to [`DIGITS`] significant digits.
fn round_digits(v: f64) -> f64 {
    if v == 0.0 || !v.is_finite() {
        return v;
    }
    let magnitude = v.abs().log10().floor();
    let scale = 10f64.powf(f64::from(DIGITS - 1) - magnitude);
    let rounded = (v * scale).round() / scale;
    if rounded.is_finite() { rounded } else { v }
}

/// How many times a derivative's step is made ten times smaller before giving up on finding the
/// scale the function varies on.
const STEP_LEVELS: i32 = 9;

/// The `order`-th derivative (1 or 2) of `f` at `x`; `f` takes and gives SI magnitudes.
///
/// The step starts at a thousandth of `|x|` (at least of 1) and is made smaller until two
/// estimates agree, so a function that varies on a much smaller scale (a time constant in
/// microseconds) is still differentiated at its own; one that never settles keeps the first
/// estimate.
pub(crate) fn derivative(f: &mut dyn FnMut(f64) -> Result<f64, CalcError>, x: f64, order: usize) -> Result<f64, CalcError> {
    let base = x.abs().max(1.0);
    let start = match order {
        1 => 1e-3 * base,
        2 => 5e-3 * base,
        _ => return Err(CalcError::Domain { function: "derivative".to_string() }),
    };
    let mid = if order == 2 { f(x)? } else { 0.0 };
    // The difference quotient at step `h`, and the size of the values it is made of.
    let mut d = |h: f64| -> Result<(f64, f64), CalcError> {
        let (a, b) = (f(x + h)?, f(x - h)?);
        Ok(if order == 1 {
            ((a - b) / (2.0 * h), a.abs().max(b.abs()))
        } else {
            ((a - 2.0 * mid + b) / (h * h), a.abs().max(b.abs()).max(mid.abs()))
        })
    };
    // The first estimate, kept when no step settles: (value, noise); and the first error, when
    // the function has no value that far from `x` at any step.
    let mut first: Option<(f64, f64)> = None;
    let mut failed: Option<CalcError> = None;
    let mut found: Option<(f64, f64)> = None;
    for level in 0..STEP_LEVELS {
        let h = start * 10f64.powi(-level);
        let coarse = d(h).and_then(|a| Ok((a, d(h / 2.0)?)));
        let ((d1, s1), (d2, s2)) = match coarse {
            Ok(pair) => pair,
            Err(e) => {
                failed.get_or_insert(e);
                continue;
            }
        };
        // One Richardson step, and what the values' last bits can make of a zero slope
        // (`f′(0)` of `x²`) at the first step.
        let value = (4.0 * d2 - d1) / 3.0;
        let noise = 1e-9 * s1.max(s2) / if order == 1 { base } else { base * base };
        if first.is_none() {
            first = Some((value, noise));
        }
        if !value.is_finite() {
            continue;
        }
        let Ok((d4, _)) = d(h / 4.0) else { continue };
        let finer = (4.0 * d4 - d2) / 3.0;
        let agree = (value - finer).abs() <= 1e-6 * finer.abs();
        if level == 0 {
            // The estimate is the one it always was; a slope within the noise is zero.
            if agree || (value.abs() <= noise && finer.abs() <= noise) {
                found = Some((value, noise));
                break;
            }
            continue;
        }
        // At a smaller step the values' last bits weigh more (ten times a level for a slope, a
        // hundred for a curvature): two estimates within them agree by chance (both 0, a small
        // slope on a large value), and are no sign that the step is the function's scale. Such
        // a derivative keeps the first estimate (`p″` of `101325 + x²/2` is 1, not 0).
        let rounding = noise * 10f64.powi(level * order as i32);
        if agree && finer.abs() > rounding {
            // The two are extrapolated once more.
            found = Some(((16.0 * finer - value) / 15.0, noise));
            break;
        }
    }
    let Some((value, noise)) = found.or(first) else {
        return Err(failed.unwrap_or(CalcError::NotReal));
    };
    if !value.is_finite() {
        return Err(CalcError::NotReal);
    }
    // Noise around a zero slope.
    if value.abs() <= noise {
        return Ok(0.0);
    }
    Ok(round_digits(value))
}

/// Gauss–Kronrod 7–15 abscissae (positive half; the last is the centre).
const XGK: [f64; 8] = [
    0.991_455_371_120_812_6,
    0.949_107_912_342_758_5,
    0.864_864_423_359_769_1,
    0.741_531_185_599_394_4,
    0.586_087_235_467_691_1,
    0.405_845_151_377_397_2,
    0.207_784_955_007_898_5,
    0.0,
];

/// Kronrod weights for [`XGK`].
const WGK: [f64; 8] = [
    0.022_935_322_010_529_22,
    0.063_092_092_629_978_55,
    0.104_790_010_322_250_18,
    0.140_653_259_715_525_92,
    0.169_004_726_639_267_9,
    0.190_350_578_064_785_4,
    0.204_432_940_075_298_9,
    0.209_482_141_084_727_83,
];

/// Gauss weights at `XGK[1]`, `XGK[3]`, `XGK[5]` and the centre.
const WG: [f64; 4] = [0.129_484_966_168_869_7, 0.279_705_391_489_276_7, 0.381_830_050_505_118_9, 0.417_959_183_673_469_4];

/// One Gauss–Kronrod rule on `[a, b]`: the estimate, its error and the integral of `|f|`.
fn kronrod(f: &mut dyn FnMut(f64) -> Result<f64, CalcError>, a: f64, b: f64) -> Result<(f64, f64, f64), CalcError> {
    let centre = 0.5 * (a + b);
    let half = 0.5 * (b - a);
    let fc = f(centre)?;
    let (mut k, mut g, mut abs) = (fc * WGK[7], fc * WG[3], fc.abs() * WGK[7]);
    for (j, (&x, &w)) in XGK.iter().zip(WGK.iter()).take(7).enumerate() {
        let (f1, f2) = (f(centre - half * x)?, f(centre + half * x)?);
        k += w * (f1 + f2);
        abs += w * (f1.abs() + f2.abs());
        if j % 2 == 1
            && let Some(&wg) = WG.get(j / 2)
        {
            g += wg * (f1 + f2);
        }
    }
    let err = ((k - g) * half).abs();
    Ok((k * half, err, abs * half.abs()))
}

/// The sign of a limit at infinity: `∞` (or `+∞`) gives +∞, `-∞` gives −∞; `None` for any
/// other limit.
pub(crate) fn infinite(limit: &Expr) -> Option<f64> {
    match limit {
        Expr::Num(v) if v.is_infinite() => Some(*v),
        Expr::Neg(inner) => infinite(inner).map(|v| -v),
        _ => None,
    }
}

/// Adaptive Gauss–Kronrod over the intervals between `breaks`: the worst interval is split until
/// the estimated error is below 10⁻¹⁰ of the result or `max` intervals are in use. Gives the
/// estimate, its error and the integral of `|f|`.
fn adaptive(f: &mut dyn FnMut(f64) -> Result<f64, CalcError>, breaks: &[f64], max: usize) -> Result<(f64, f64, f64), CalcError> {
    // Intervals: (a, b, estimate, error, ∫|f|).
    let mut parts = Vec::with_capacity(breaks.len());
    for (&a, &b) in breaks.iter().zip(breaks.iter().skip(1)) {
        if a != b {
            let (k, e, s) = kronrod(f, a, b)?;
            parts.push((a, b, k, e, s));
        }
    }
    loop {
        let total: f64 = parts.iter().map(|p| p.2).sum();
        let error: f64 = parts.iter().map(|p| p.3).sum();
        let abs: f64 = parts.iter().map(|p| p.4).sum();
        let done = error <= 1e-10 * total.abs().max(1e-3 * abs) || error == 0.0;
        if done || parts.len() >= max || parts.is_empty() {
            return Ok((total, error, abs));
        }
        let worst = parts.iter().enumerate().max_by(|x, y| x.1.3.total_cmp(&y.1.3)).map_or(0, |(i, _)| i);
        let (pa, pb, ..) = parts.swap_remove(worst);
        let mid = 0.5 * (pa + pb);
        let (k1, e1, s1) = kronrod(f, pa, mid)?;
        let (k2, e2, s2) = kronrod(f, mid, pb)?;
        parts.push((pa, mid, k1, e1, s1));
        parts.push((mid, pb, k2, e2, s2));
    }
}

/// The powers of ten an improper integral is sampled over, from its finite limit (or from 0):
/// 10⁻⁹ to 10⁹, three intervals per decade (they start at 1, 2 and 5), so an integrand that
/// lives at any such scale is seen.
const DECADES: i32 = 9;

/// `∫_0^∞ f(a + sign·u) du`: decade by decade (in three pieces each) up to 10⁹, then the tail with `u = 10⁹/s`, whose
/// nodes never reach `s = 0`. Gives the estimate, its error and the integral of `|f|`.
fn half_line(f: &mut dyn FnMut(f64) -> Result<f64, CalcError>, a: f64, sign: f64) -> Result<(f64, f64, f64), CalcError> {
    let reach = 10f64.powi(DECADES);
    let mut breaks = vec![0.0];
    for j in -DECADES..DECADES {
        breaks.extend([1.0, 2.0, 5.0].map(|m| m * 10f64.powi(j)));
    }
    breaks.push(reach);
    // The intervals it starts from do not eat into the ones it may split.
    let (k1, e1, s1) = adaptive(&mut |u| f(a + sign * u), &breaks, MAX_INTERVALS + breaks.len())?;
    let (k2, e2, s2) = adaptive(&mut |s| Ok(f(a + sign * reach / s)? * reach / (s * s)), &[0.0, 1.0], MAX_INTERVALS)?;
    Ok((k1 + k2, e1 + e2, s1 + s2))
}

/// `∫_a^b f`; `f` takes and gives SI magnitudes. A limit may be infinite: the integral is then
/// improper, and an error when it does not converge.
pub(crate) fn integral(f: &mut dyn FnMut(f64) -> Result<f64, CalcError>, a: f64, b: f64) -> Result<f64, CalcError> {
    if a.is_nan() || b.is_nan() {
        return Err(CalcError::NotReal);
    }
    if a == b {
        return Ok(0.0);
    }
    if a > b && (a.is_infinite() || b.is_infinite()) {
        return integral(f, b, a).map(|v| -v);
    }
    let (total, error, abs) = match (a.is_finite(), b.is_finite()) {
        (true, true) => adaptive(f, &[a, b], MAX_INTERVALS)?,
        (true, false) => half_line(f, a, 1.0)?,
        (false, true) => half_line(f, b, -1.0)?,
        (false, false) => {
            let (k1, e1, s1) = half_line(f, 0.0, -1.0)?;
            let (k2, e2, s2) = half_line(f, 0.0, 1.0)?;
            (k1 + k2, e1 + e2, s1 + s2)
        }
    };
    // The integral must have settled: one that diverges (`∫_1^∞ 1/x dx`, `∫_0^1 1/x dx`) keeps a
    // large error however far it is refined.
    let improper = a.is_infinite() || b.is_infinite();
    if !total.is_finite() {
        return Err(if improper { CalcError::Diverges } else { CalcError::NotReal });
    }
    // Between finite limits the estimate of the error is a cautious one (an integrand with many
    // waves is right long before it says so): only an error that stays large is no value.
    let settled = if improper { 1e-6 } else { 1e-3 };
    if error > settled * total.abs().max(1e-3 * abs) {
        return Err(CalcError::Diverges);
    }
    if total.abs() <= 1e-11 * abs {
        return Ok(0.0);
    }
    Ok(round_digits(total))
}

#[cfg(test)]
mod tests {
    use crate::*;

    fn last(statements: &[&str]) -> String {
        evaluate(statements, &Options::default()).pop().and_then(|r| r.display).unwrap_or_default()
    }

    fn error(statements: &[&str]) -> Option<CalcError> {
        evaluate(statements, &Options::default()).pop().and_then(|r| r.error)
    }

    #[test]
    fn improper_integrals() {
        assert_eq!(last(&["∫_(-∞)^∞▒〖e^(-x^2) dx〗 ="]), "∫_(-∞)^∞▒〖e^(-x^2) dx〗 = 1.772");
        assert_eq!(last(&["∫_(−∞)^(+∞)▒〖e^(−x^2) dx〗 = 99"]), "∫_(−∞)^(+∞)▒〖e^(−x^2) dx〗 = 1.772");
        assert_eq!(last(&["∫_0^∞ e^(-x) dx ="]), "∫_0^∞ e^(-x) dx = 1");
        assert_eq!(last(&["∫_1^∞ 1/x^2 dx ="]), "∫_1^∞ 1/x^2 dx = 1");
        assert_eq!(last(&["∫_-∞^0 e^x dx ="]), "∫_-∞^0 e^x dx = 1");
        assert_eq!(last(&["∫_0^∞ 1/(1 + x^2) dx ="]), "∫_0^∞ 1/(1 + x^2) dx = 1.571");
        // The limits the other way round change the sign; the same limit twice is nothing.
        assert_eq!(last(&["∫_∞^0 e^(-x) dx ="]), "∫_∞^0 e^(-x) dx = -1");
        assert_eq!(last(&["∫_∞^∞ e^(-x) dx ="]), "∫_∞^∞ e^(-x) dx = 0");
        // Whatever the scale of the integrand: a time constant in microseconds, a wide bell.
        assert_eq!(last(&["τ = 2 µs", "∫_(0 s)^∞ e^(-t/τ) dt = µs"]), "∫_(0 s)^∞ e^(-t/τ) dt = 2 µs");
        assert_eq!(last(&["s = 1000", "∫_(-∞)^∞ e^(-(x/s)^2) dx ="]), "∫_(-∞)^∞ e^(-(x/s)^2) dx = 1772.454");
        // The normal distribution has area 1, mean μ.
        let bell = ["μ = 3", "σ = 0.5", "φ(x) = 1/(σ·√(2·π))·e^(-(x - μ)^2/(2·σ^2))"];
        assert_eq!(last(&[bell[0], bell[1], bell[2], "∫_(-∞)^∞ φ(x) dx ="]), "∫_(-∞)^∞ φ(x) dx = 1");
        assert_eq!(last(&[bell[0], bell[1], bell[2], "∫_(-∞)^∞ x·φ(x) dx ="]), "∫_(-∞)^∞ x·φ(x) dx = 3");
        // A narrow bell away from the powers of ten is still seen.
        assert_eq!(last(&["μ = 500", "σ = 1", bell[2], "∫_(-∞)^∞ φ(x) dx ="]), "∫_(-∞)^∞ φ(x) dx = 1");
        assert_eq!(last(&["μ = 170", "σ = 7", bell[2], "∫_0^∞ φ(x) dx ="]), "∫_0^∞ φ(x) dx = 1");
        // A solved unknown and a definition read it like any value.
        assert_eq!(last(&["A = ∫_0^∞ e^(-2·x) dx", "A ="]), "A = 0.5");
    }

    #[test]
    fn improper_integrals_that_do_not_converge() {
        for diverging in ["∫_1^∞ 1/x dx =", "∫_0^∞ x dx =", "∫_(-∞)^∞ 1 dx =", "∫_0^∞ sin(x) dx =", "∫_1^∞ 1/√x dx ="] {
            assert_eq!(error(&[diverging]), Some(CalcError::Diverges), "{diverging}");
        }
        // One that converges too slowly to be found is the same error, never a number.
        assert_eq!(error(&["∫_0^∞ sin(x)/x dx ="]), Some(CalcError::Diverges));
        assert_eq!(CalcError::Diverges.to_string(), "no finite value found for the integral: it may not converge");
        assert_eq!(error(&["∫_0^∞ e^(x^2) dx ="]), Some(CalcError::Overflow));
        // Infinity is a limit of an integral, never a value.
        for wrong in ["∞ =", "x = ∞", "2·∞ =", "∑_(k=0)^∞▒k =", "∫_0^(2·∞) x dx =", "f(x) = x", "sin(∞) ="] {
            let e = error(&[wrong]);
            assert!(wrong == "f(x) = x" || e == Some(CalcError::Infinite), "{wrong}: {e:?}");
        }
    }

    #[test]
    fn derivatives_of_user_functions() {
        assert_eq!(last(&["f(x) = x^2 - 4", "f′(3) ="]), "f′(3) = 6");
        assert_eq!(last(&["f(x) = x^2 - 4", "f'(3) ="]), "f'(3) = 6");
        assert_eq!(last(&["f(x) = x^3", "f″(2) ="]), "f″(2) = 12");
        assert_eq!(last(&["f(x) = x^3", "f''(2) ="]), "f''(2) = 12");
        assert_eq!(last(&["f(x) = x^2", "f′(0) ="]), "f′(0) = 0");
        assert_eq!(last(&["f(x) = sin(x)", "f′(0) ="]), "f′(0) = 1");
        // Units follow: a position over time gives a speed.
        assert_eq!(last(&["a = 3 m/s^2", "s(t) = a·t^2", "s′(2 s) ="]), "s′(2 s) = 12 m/s");
        // In a definition and a tangent line.
        assert_eq!(last(&["f(x) = x^2", "m = f′(1)", "q = f(1) - m·1", "q ="]), "q = -1");
        // Solvable like any call: the plot's analysis relies on it.
        assert_eq!(last(&["f(x) = x^3 - 3 x", "0 = f′(x_1); x_1 < 0"]), "0 = f′(x_1); x_1 < 0 ⇒ x_1 = -1");
        assert_eq!(last(&["f(x) = x^2 - 4", "0 = f(x_1); x_1 > -3; x_1 < -1"]), "0 = f(x_1); x_1 > -3; x_1 < -1 ⇒ x_1 = -2");
        assert!(error(&["f(x) = x^2", "f‴(1) ="]).is_some());
        assert!(error(&["g(a; b) = a·b", "g′(1; 2) ="]).is_some());
        assert!(error(&["f′(1) ="]).is_some());
        // `f′(x) = …` is no definition of a function `f′`.
        assert!(evaluate(&["f′(x) = 2 x"], &Options::default()).first().and_then(|r| r.name.clone()).as_deref() != Some("f′"));
    }

    #[test]
    fn definite_integrals() {
        assert_eq!(last(&["∫_0^3▒〖x^2 dx〗 ="]), "∫_0^3▒〖x^2 dx〗 = 9");
        assert_eq!(last(&["∫_0^1 x^2 + 1 dx ="]), "∫_0^1 x^2 + 1 dx = 1.333");
        assert_eq!(last(&["f(x) = x^2 - 4", "A = ∫_(-2)^2 f(x) dx", "A ="]), "A = -10.667");
        assert_eq!(last(&["∫_0^π sin(x) dx ="]), "∫_0^π sin(x) dx = 2");
        assert_eq!(last(&["∫_(-1)^1 x dx ="]), "∫_(-1)^1 x dx = 0");
        assert_eq!(last(&["∫_0^1 1/√x dx ="]), "∫_0^1 1/√x dx = 2");
        assert_eq!(last(&["∫_1^2 1/x dx ="]), "∫_1^2 1/x dx = 0.693");
        assert_eq!(last(&["∫_2^0 x dx ="]), "∫_2^0 x dx = -2");
        assert_eq!(last(&["∫_0^2 ⅆt ="]), "∫_0^2 ⅆt = 2");
        // Units: a speed over time is a distance.
        assert_eq!(last(&["a = 3 m/s^2", "∫_(0 s)^(2 s) a·t dt ="]), "∫_(0 s)^(2 s) a·t dt = 6 m");
        // As a document's equation writes them: the unit against the number, the script ending at `^`.
        assert_eq!(last(&["f(x) = (6m)-x", "∫_0m^6m▒(f(x)^2dx) ="]), "∫_0m^6m▒(f(x)^2dx) = 72 m³");
        assert_eq!(last(&["∫_(-1m)^2m▒(x dx) ="]), "∫_(-1m)^2m▒(x dx) = 1.5 m²");
        // The integration variable is bound, not an unknown to solve for.
        assert_eq!(last(&["a = 2", "∫_0^a x dx ="]), "∫_0^a x dx = 2");
        assert!(error(&["∫_0^1 x ="]).is_some());
        assert!(error(&["∫_0 x dx ="]).is_some());
        assert!(error(&["∫_(0 m)^(1 s) x dx ="]).is_some());
    }

    #[test]
    fn hostile_calculus_never_panics() {
        for t in [
            "∫",
            "∫_",
            "∫_^",
            "∫_0^1",
            "∫_0^1▒",
            "∫_0^1 dx dx =",
            "∫_0^1 1/x dx =",
            "∫_0^1e300 x dx =",
            "∫_(-1e308)^1e308 x^9 dx =",
            "f′",
            "f′(",
            "′",
            "x′ =",
            "f'''''''''''(1) =",
            "∫_0^1 ∫_0^1 x y dx dy =",
            "∫_0^1 tan(1/x) dx =",
        ] {
            let _ = evaluate(&["f(x) = 1/x", t], &Options::default());
        }
        let _ = evaluate(&["f(x) = f′(x)", "f(1) ="], &Options::default());
        let _ = evaluate(&["f(x) = ∫_0^x f(t) dt", "f(1) ="], &Options::default());
    }
}
