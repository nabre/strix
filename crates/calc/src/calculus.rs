//! Calculus: derivatives of user functions (`f′(2)`, `f″(2)`) and definite integrals
//! (`∫_0^2 f(x) dx`), both computed numerically.
//!
//! - **Derivatives** are central differences refined by one Richardson step, so a smooth
//!   function's slope is right to about ten significant digits.
//! - **Integrals** use adaptive Gauss–Kronrod (7–15 points), splitting the worst interval until
//!   the estimated error is below 10⁻¹⁰ of the result or [`MAX_INTERVALS`] are in use. The
//!   nodes never touch the limits, so `∫_0^1 1/√x dx` works.
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

/// The `order`-th derivative (1 or 2) of `f` at `x`; `f` takes and gives SI magnitudes.
pub(crate) fn derivative(f: &mut dyn FnMut(f64) -> Result<f64, CalcError>, x: f64, order: usize) -> Result<f64, CalcError> {
    let base = x.abs().max(1.0);
    let (h, value, scale) = match order {
        1 => {
            let h = 1e-3 * base;
            let mut d = |h: f64| -> Result<(f64, f64), CalcError> {
                let (a, b) = (f(x + h)?, f(x - h)?);
                Ok(((a - b) / (2.0 * h), a.abs().max(b.abs())))
            };
            let (d1, s1) = d(h)?;
            let (d2, s2) = d(h / 2.0)?;
            (h, (4.0 * d2 - d1) / 3.0, s1.max(s2) / base)
        }
        2 => {
            let h = 5e-3 * base;
            let mid = f(x)?;
            let mut d = |h: f64| -> Result<(f64, f64), CalcError> {
                let (a, b) = (f(x + h)?, f(x - h)?);
                Ok(((a - 2.0 * mid + b) / (h * h), a.abs().max(b.abs()).max(mid.abs())))
            };
            let (d1, s1) = d(h)?;
            let (d2, s2) = d(h / 2.0)?;
            (h, (4.0 * d2 - d1) / 3.0, s1.max(s2) / (base * base))
        }
        _ => return Err(CalcError::Domain { function: "derivative".to_string() }),
    };
    if !value.is_finite() || !h.is_finite() {
        return Err(CalcError::NotReal);
    }
    // Noise around a zero slope (`f′(0)` of `x²`).
    if value.abs() <= 1e-9 * scale {
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

/// `∫_a^b f`; `f` takes and gives SI magnitudes.
pub(crate) fn integral(f: &mut dyn FnMut(f64) -> Result<f64, CalcError>, a: f64, b: f64) -> Result<f64, CalcError> {
    if a == b {
        return Ok(0.0);
    }
    // Intervals: (a, b, estimate, error, ∫|f|).
    let (k, e, s) = kronrod(f, a, b)?;
    let mut parts = vec![(a, b, k, e, s)];
    loop {
        let total: f64 = parts.iter().map(|p| p.2).sum();
        let error: f64 = parts.iter().map(|p| p.3).sum();
        let abs: f64 = parts.iter().map(|p| p.4).sum();
        let done = error <= 1e-10 * total.abs().max(1e-3 * abs) || error == 0.0;
        if done || parts.len() >= MAX_INTERVALS {
            if !total.is_finite() {
                return Err(CalcError::NotReal);
            }
            if total.abs() <= 1e-11 * abs {
                return Ok(0.0);
            }
            return Ok(round_digits(total));
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
