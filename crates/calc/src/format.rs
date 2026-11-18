//! Number formatting and unit-expression rendering.

use crate::parser::{BinOp, Expr, Func};
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
        // The zeros the cap leaves at the end are no digits of the number: `0.30000000000000004`
        // is `0.3`, with nothing below it for `ceil` or `floor` to round away.
        while ds.len() > 1 && ds.last() == Some(&0) {
            ds.pop();
        }
    }
    (v < 0.0, ds, exp)
}

/// Direction in which [`step_decimal`] rounds the shortest decimal digits of a number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RoundMode {
    HalfAwayFromZero,
    Floor,
    Ceil,
    Trunc,
}

/// `v` rounded to `places` decimals (negative: to tens, hundreds…) using `mode` on its shortest
/// decimal form (capped at 15 significant digits), so `round(1.005; 2)` is `1.01` and
/// `floor(0.29; 2)` stays `0.29` without binary floating-point drift.
pub(crate) fn step_decimal(v: f64, places: i32, mode: RoundMode) -> f64 {
    if !v.is_finite() || v == 0.0 {
        return v;
    }
    let (negative, mut ds, mut exp) = decimal_digits(v);
    let unit = -places;
    let keep = i64::from(exp) - i64::from(unit) + 1;
    if keep >= ds.len() as i64 {
        return v;
    }
    let up = match mode {
        RoundMode::HalfAwayFromZero => keep >= 0 && ds.get(keep as usize).is_some_and(|&d| d >= 5),
        RoundMode::Trunc => false,
        RoundMode::Floor => negative,
        RoundMode::Ceil => !negative,
    };
    if keep <= 0 {
        if up {
            ds = vec![1];
            exp = unit;
        } else {
            return 0.0;
        }
    } else {
        let keep = keep as usize;
        ds.truncate(keep);
        if up {
            let mut i = keep;
            loop {
                if i == 0 {
                    ds.insert(0, 1);
                    exp += 1;
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
            return 0.0;
        }
    }
    let digits: String = ds.iter().map(|d| char::from(b'0' + d)).collect();
    let shift = i64::from(exp) - (ds.len() as i64 - 1);
    let r: f64 = format!("{digits}e{shift}").parse().unwrap_or(v.abs());
    if r == 0.0 {
        0.0
    } else if negative {
        -r
    } else {
        r
    }
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
    render_with(expr, &|_| None)
}

/// [`render`], with names `sub` gives text for written as that text (`L` → `(4.5 m)`): the
/// substitution line of a solution. The text must already carry the parentheses it needs.
pub(crate) fn render_with(expr: &Expr, sub: &dyn Fn(&str) -> Option<String>) -> String {
    render_nodes(expr, &|e: &Expr| match e {
        Expr::Name(name) => sub(name),
        _ => None,
    })
}

/// [`render`], with any node `sub` gives text for written as that text (a parameter by its
/// value, a written quantity converted to another unit: `30 cm` → `0.3 m`).
/// `text` without one pair of parentheses around all of it (`(3.464 m)` → `3.464 m`); as it is
/// when they close earlier (`(a) + (b)`).
fn unwrapped(text: &str) -> &str {
    let Some(inner) = text.strip_prefix('(').and_then(|t| t.strip_suffix(')')) else { return text };
    let mut depth = 0_i32;
    for c in inner.chars() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return text;
                }
            }
            _ => {}
        }
    }
    if depth == 0 { inner } else { text }
}

pub(crate) fn render_nodes(expr: &Expr, sub: &dyn Fn(&Expr) -> Option<String>) -> String {
    if let Some(text) = sub(expr) {
        return text;
    }
    let render = |e: &Expr| render_nodes(e, sub);
    let wrap = |e: &Expr, parens: bool| if parens { format!("({})", render_nodes(e, sub)) } else { render_nodes(e, sub) };
    match expr {
        Expr::Num(v) if v.is_infinite() => "∞".to_string(),
        Expr::Num(v) => format!("{v}"),
        Expr::Name(name) => name.clone(),
        Expr::Unit(name) => name.clone(),
        Expr::Neg(inner) => format!("-{}", wrap(inner, is_sum(inner))),
        Expr::Call(func, args) => {
            let args: Vec<String> =
                args.iter().enumerate().map(|(i, a)| if func.is_rounding() && i == 2 { crate::format::render(a) } else { render(a) }).collect();
            match (func, args.as_slice()) {
                (Func::Sqrt, [a]) => format!("√({a})"),
                // As the Equation tab builds them: the degree first, the inverse as a power on
                // the name, bars, a base under the logarithm.
                (Func::Cbrt, [a]) => format!("√(3&{a})"),
                (Func::Root, [a, n]) => format!("√({n}&{a})"),
                (f, [a]) if let Some(direct) = f.inverse_of() => format!("{}^(−1)\u{2061}({a})", direct.name()),
                (Func::Norm, [a]) => format!("├‖{a}‖┤"),
                (Func::Log, [a, base]) => {
                    let plain = base.chars().all(|c| c.is_alphanumeric() || c == '.');
                    if plain { format!("log_{base}\u{2061}({a})") } else { format!("log_({base})\u{2061}({a})") }
                }
                (Func::Lt | Func::Le | Func::Gt | Func::Ge | Func::Ne | Func::Eq, [a, b]) => format!("{a}{}{b}", func.name()),
                // Several comparisons at once, as they are read: `0≤x≤2` (each one starts where
                // the one before ends).
                (Func::All, _) => match expr {
                    Expr::Call(_, comparisons) => {
                        let mut chain = String::new();
                        for (k, comparison) in comparisons.iter().enumerate() {
                            match comparison {
                                Expr::Call(op, sides) if k > 0 && sides.len() == 2 => {
                                    chain.push_str(op.name());
                                    chain.push_str(&sides.get(1).map(render).unwrap_or_default());
                                }
                                other => chain.push_str(&render(other)),
                            }
                        }
                        chain
                    }
                    _ => args.join(""),
                },
                (Func::Cases, _) => {
                    // The case for whatever is left is written as it is read, `otherwise`.
                    let always = |k: usize| matches!(expr, Expr::Call(_, cases) if matches!(cases.get(2 * k + 1), Some(Expr::Num(v)) if *v == 1.0));
                    let rows: Vec<String> = args
                        .chunks(2)
                        .enumerate()
                        .map(|(k, c)| match c {
                            [value, _] if always(k) => format!("{value},&otherwise"),
                            other => other.join(",&"),
                        })
                        .collect();
                    format!("├{{█({})┤", rows.join("@"))
                }
                (Func::Matrix | Func::Det, [columns, cells @ ..]) if !cells.is_empty() => {
                    let per_row = columns.parse::<usize>().unwrap_or(1).max(1);
                    let rows: Vec<String> = cells.chunks(per_row).map(|r| r.join("&")).collect();
                    let matrix = format!("■({})", rows.join("@"));
                    if *func == Func::Det { format!("├|{matrix}|┤") } else { matrix }
                }
                // A coefficient stacks single operands only: `(n¦k)`, `((n + 1)¦k)`.
                (Func::Choose, [_, _]) => {
                    let stacked: Vec<String> = match expr {
                        Expr::Call(_, operands) => operands.iter().map(|a| wrap(a, !matches!(a, Expr::Name(_) | Expr::Num(_)))).collect(),
                        _ => args,
                    };
                    format!("({})", stacked.join("¦"))
                }
                _ => format!("{}({})", func.name(), args.iter().map(|a| unwrapped(a)).collect::<Vec<_>>().join("; ")),
            }
        }
        Expr::Apply(name, args) => {
            // A value substituted in its own parentheses needs no second pair: `M((3.464 m))`.
            let args: Vec<String> = args.iter().map(|a| unwrapped(&render(a)).to_string()).collect();
            format!("{name}({})", args.join("; "))
        }
        Expr::Vector(items) => {
            let items: Vec<String> = items.iter().map(render).collect();
            format!("[{}]", items.join("; "))
        }
        Expr::Index(base, index) => format!("{}[{}]", wrap(base, !matches!(**base, Expr::Name(_))), render(index)),
        Expr::Range(from, to) => format!("{}..{}", render(from), render(to)),
        Expr::Nary(n) if n.op == '∫' => {
            let limit = |e: &Option<Box<Expr>>| e.as_ref().map(|e| render(e)).unwrap_or_default();
            let var = n.var.as_deref().unwrap_or("x");
            format!("∫_({})^({})▒〖{} d{var}〗", limit(&n.from), limit(&n.to), wrap(&n.body, is_sum(&n.body)))
        }
        Expr::Nary(n) if crate::structures::is_limit(n.op) => {
            let to = n.from.as_ref().map(|e| render(e)).unwrap_or_default();
            let side = match n.op {
                crate::structures::LIMIT_RIGHT => "^+",
                crate::structures::LIMIT_LEFT => "^-",
                _ => "",
            };
            format!("lim┬({}→{to}{side})\u{2061}〖{}〗", n.var.as_deref().unwrap_or("x"), render(&n.body))
        }
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
            // A written quantity reads as written: `30 cm`, `12 kN/m`.
            BinOp::Mul if is_quantity(expr) => format!("{} {}", render(left), render(right)),
            // Numbers and their units: `[4; 5; 6] m`, `10³ m` (read back as units).
            BinOp::Mul if is_numeric(left) && !matches!(**left, Expr::Neg(_)) && is_units(right) => {
                format!("{} {}", render(left), render(right))
            }
            BinOp::Mul => format!("{}·{}", wrap(left, is_sum(left)), wrap(right, is_sum(right) || is_div(right))),
            BinOp::Div => {
                format!("{}/{}", wrap(left, is_sum(left)), wrap(right, is_sum(right) || is_product(right)))
            }
            BinOp::Pow => {
                let base = wrap(left, !matches!(**left, Expr::Name(_) | Expr::Num(_)));
                match integer(right) {
                    Some(n) => power_text(&base, n),
                    None => match &**right {
                        // A name, or the value written for it (`3`, `(2 m)`).
                        Expr::Name(_) => {
                            let exponent = render(right);
                            let plain = exponent.starts_with('(') || exponent.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | ','));
                            if plain { format!("{base}^{exponent}") } else { format!("{base}^({exponent})") }
                        }
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

/// Units alone, multiplied, divided and raised to powers (`kN`, `m/s²`).
pub(crate) fn is_units(expr: &Expr) -> bool {
    match expr {
        Expr::Unit(_) => true,
        Expr::Bin(BinOp::Mul | BinOp::Div, l, r) => is_units(l) && is_units(r),
        Expr::Bin(BinOp::Pow, base, exp) => is_units(base) && integer(exp).is_some(),
        _ => false,
    }
}

/// A quantity as written: a number and its units (`30 cm`, `9.81 m/s²`).
pub(crate) fn is_quantity(expr: &Expr) -> bool {
    matches!(expr, Expr::Bin(BinOp::Mul, n, u) if matches!(**n, Expr::Num(_)) && is_units(u))
}

/// Numbers alone: a number, a vector of numbers, a power of numbers (`10^3`), negated. Units
/// right after one are read as units, as after a number (`[4; 5; 6] m`, `10³ m`).
pub(crate) fn is_numeric(expr: &Expr) -> bool {
    match expr {
        Expr::Num(_) => true,
        Expr::Neg(inner) => is_numeric(inner),
        Expr::Vector(items) => !items.is_empty() && items.iter().all(is_numeric),
        Expr::Bin(BinOp::Pow, base, exp) => is_numeric(base) && is_numeric(exp),
        _ => false,
    }
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
