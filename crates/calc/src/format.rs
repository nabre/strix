//! Number formatting and unit-expression rendering.

use crate::parser::{BinOp, Expr};
use crate::units::power_text;
use crate::{Count, Options};

/// Formats `value` the way results are shown (`docs/calc.md`, Number format):
///
/// - [`Count::Auto`] (default): [`Options::digits`] decimal places for |x| ≥ 1; below 1, as many
///   places as keep `digits` significant digits (`0.0035` with 3 → `0.0035`).
/// - [`Count::Decimals`]: always `digits` decimal places (`0.0001` with 2 → `0`).
/// - [`Count::Significant`]: `digits` significant digits (`1234.5` with 2 → `1200`).
///
/// Rounding is half away from zero on the shortest decimal form of the number, capped at 15
/// significant digits, so `2.675` rounds to `2.68`. Trailing zeros are dropped unless
/// [`Options::zeros`]. Scientific notation `1.235·10^9` is used at or beyond 10⁷, and below 10⁻³
/// except with [`Count::Decimals`]. [`Options::decimal_comma`] writes `,`. Non-finite values
/// (which evaluation never produces) format as `⚠`.
pub fn format_number(value: f64, opts: &Options) -> String {
    if !value.is_finite() {
        return "⚠".to_string();
    }
    let digits = i32::from(opts.digits.min(15));
    let (negative, mut ds, mut exp) = decimal_digits(value);
    let abs = value.abs();
    let scientific = |mut ds: Vec<u8>, mut exp: i32| {
        let places = match opts.count {
            Count::Auto => digits.max(2),
            Count::Decimals => digits,
            Count::Significant => (digits - 1).max(0),
        };
        // The mantissa keeps `places` decimals: digits down to 10^(exp - places).
        let unit = exp - places;
        round_at(&mut ds, &mut exp, unit);
        format!("{}·10^{exp}", fixed(&ds, 0, places, opts.zeros))
    };
    let body = if abs != 0.0 && (abs >= 1e7 || (abs < 1e-3 && opts.count != Count::Decimals)) {
        scientific(ds, exp)
    } else {
        let places = match opts.count {
            Count::Decimals => digits,
            Count::Auto if abs >= 1.0 || abs == 0.0 => digits,
            Count::Auto | Count::Significant => digits.max(1) - 1 - exp,
        };
        let (original, original_exp) = (ds.clone(), exp);
        round_at(&mut ds, &mut exp, -places);
        // Rounding up to 10⁷ (9999999.9996) crosses into scientific notation.
        if exp >= 7 { scientific(original, original_exp) } else { fixed(&ds, exp, places, opts.zeros) }
    };
    let body = if opts.decimal_comma { body.replace('.', ",") } else { body };
    let zero = body.chars().all(|c| matches!(c, '0' | '.' | ','));
    if negative && !zero { format!("-{body}") } else { body }
}

/// The shortest decimal digits of |v| and the power of ten of the first one (v = d.ddd × 10^exp),
/// capped at 15 significant digits; zero is `["0"], 0`.
fn decimal_digits(v: f64) -> (bool, Vec<u8>, i32) {
    let text = format!("{:e}", v.abs());
    let (mantissa, e) = text.split_once('e').unwrap_or((text.as_str(), "0"));
    let mut exp: i32 = e.parse().unwrap_or(0);
    let mut ds: Vec<u8> = mantissa.bytes().filter(u8::is_ascii_digit).map(|b| b - b'0').collect();
    if ds.iter().all(|&d| d == 0) {
        return (false, vec![0], 0);
    }
    if ds.len() > 15 {
        let unit = exp - 14;
        round_at(&mut ds, &mut exp, unit);
    }
    (v < 0.0, ds, exp)
}

/// Rounds the digits (first at 10^exp) half away from zero to the power of ten `unit`.
fn round_at(ds: &mut Vec<u8>, exp: &mut i32, unit: i32) {
    // Digits kept: those at 10^exp down to 10^unit.
    let keep = i64::from(*exp) - i64::from(unit) + 1;
    if keep >= ds.len() as i64 {
        return;
    }
    if keep < 0 {
        *ds = vec![0];
        *exp = unit;
        return;
    }
    let keep = keep as usize;
    let up = ds.get(keep).is_some_and(|&d| d >= 5);
    ds.truncate(keep);
    if up {
        let mut i = keep;
        loop {
            if i == 0 {
                ds.insert(0, 1);
                *exp += 1;
                break;
            }
            i -= 1;
            if let Some(d) = ds.get_mut(i) {
                if *d == 9 {
                    *d = 0;
                } else {
                    *d += 1;
                    break;
                }
            }
        }
    }
    if ds.is_empty() || ds.iter().all(|&d| d == 0) {
        *ds = vec![0];
        *exp = 0;
    }
}

/// Digits `ds` (the first at 10^exp) as fixed-point text with `places` decimals (negative: the
/// number ends in zeros); trailing decimal zeros are dropped unless `zeros`.
fn fixed(ds: &[u8], exp: i32, places: i32, zeros: bool) -> String {
    let digit = |power: i32| -> char {
        let index = i64::from(exp) - i64::from(power);
        usize::try_from(index).ok().and_then(|i| ds.get(i)).map_or('0', |&d| char::from(b'0' + d))
    };
    let mut int = String::new();
    for power in (0..=exp.max(0)).rev() {
        int.push(digit(power));
    }
    let int = match int.trim_start_matches('0') {
        "" => "0".to_string(),
        rest => rest.to_string(),
    };
    let mut frac: String = (1..=places.max(0)).map(|k| digit(-k)).collect();
    if !zeros {
        frac.truncate(frac.trim_end_matches('0').len());
    }
    if frac.is_empty() { int } else { format!("{int}.{frac}") }
}

/// Renders a unit expression in normal form: `·` between factors, `²`/`³` or `^n` for powers,
/// parentheses only where needed. Re-parsing the result gives the same value and the same text.
pub(crate) fn render(expr: &Expr) -> String {
    match expr {
        Expr::Num(v) => format!("{v}"),
        Expr::Name(name) => name.clone(),
        Expr::Neg(inner) => format!("-{}", wrap(inner, is_sum(inner))),
        Expr::Call(func, args) => {
            let args: Vec<String> = args.iter().map(render).collect();
            format!("{}({})", func.name(), args.join("; "))
        }
        Expr::Vector(items) => {
            let items: Vec<String> = items.iter().map(render).collect();
            format!("[{}]", items.join("; "))
        }
        Expr::Index(base, index) => format!("{}[{}]", wrap(base, !matches!(**base, Expr::Name(_))), render(index)),
        Expr::Range(from, to) => format!("{}..{}", render(from), render(to)),
        Expr::Nary(n) => {
            let lower = match (&n.var, &n.from) {
                (Some(var), Some(from)) => format!("_({var}={})", render(from)),
                (Some(var), None) => format!("_{var}"),
                _ => String::new(),
            };
            let upper = n.to.as_ref().map(|t| format!("^({})", render(t))).unwrap_or_default();
            format!("{}{lower}{upper}▒({})", n.op, render(&n.body))
        }
        Expr::Bin(op, left, right) => match op {
            BinOp::Add => format!("{} + {}", render(left), render(right)),
            BinOp::Sub => format!("{} - {}", render(left), wrap(right, is_sum(right))),
            BinOp::Mul => format!("{}·{}", wrap(left, is_sum(left)), wrap(right, is_sum(right) || is_div(right))),
            BinOp::Div => {
                format!("{}/{}", wrap(left, is_sum(left)), wrap(right, is_sum(right) || is_product(right)))
            }
            BinOp::Pow => {
                let base = wrap(left, !matches!(**left, Expr::Name(_) | Expr::Num(_)));
                match integer(right) {
                    Some(n) => power_text(&base, n),
                    None => match &**right {
                        Expr::Name(name) => format!("{base}^{name}"),
                        other => format!("{base}^({})", render(other)),
                    },
                }
            }
        },
    }
}

/// The integer an exponent expression spells (`2`, `-1`), if it is one.
fn integer(expr: &Expr) -> Option<i64> {
    let (v, sign) = match expr {
        Expr::Num(v) => (*v, 1),
        Expr::Neg(inner) => match &**inner {
            Expr::Num(v) => (*v, -1),
            _ => return None,
        },
        _ => return None,
    };
    if v.fract() == 0.0 && v.abs() <= 1e6 { Some(sign * v as i64) } else { None }
}

fn wrap(expr: &Expr, parens: bool) -> String {
    if parens { format!("({})", render(expr)) } else { render(expr) }
}

fn is_sum(expr: &Expr) -> bool {
    matches!(expr, Expr::Bin(BinOp::Add | BinOp::Sub, _, _))
}

fn is_div(expr: &Expr) -> bool {
    matches!(expr, Expr::Bin(BinOp::Div, _, _))
}

fn is_product(expr: &Expr) -> bool {
    matches!(expr, Expr::Bin(BinOp::Mul | BinOp::Div, _, _) | Expr::Neg(_))
}
