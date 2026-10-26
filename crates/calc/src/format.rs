//! Number formatting and unit-expression rendering.

use crate::Options;
use crate::parser::{BinOp, Expr};
use crate::units::power_text;

/// Formats `value` with [`Options::sig_digits`] significant digits (clamped to 1..=15), without
/// trailing zeros, using `·10^n` scientific notation at or beyond 10⁹ and below 10⁻⁴
/// (`1.235·10^9`, `2.5·10^-5`), and a decimal comma when [`Options::decimal_comma`] is set.
/// Non-finite values (which evaluation never produces) format as `⚠`.
pub fn format_number(value: f64, opts: &Options) -> String {
    if !value.is_finite() {
        return "⚠".to_string();
    }
    if value == 0.0 {
        return "0".to_string();
    }
    let digits = usize::from(opts.sig_digits.clamp(1, 15));
    // `{:e}` rounds correctly to `digits` significant digits: "-3.038e1".
    let sci = format!("{:.*e}", digits - 1, value);
    let Some((mantissa, exponent)) = sci.split_once('e') else {
        return sci;
    };
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let negative = mantissa.starts_with('-');
    let all_digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let significant = all_digits.trim_end_matches('0');
    if significant.is_empty() {
        return "0".to_string();
    }
    let body = if !(-4..9).contains(&exponent) {
        let (first, rest) = significant.split_at_checked(1).unwrap_or((significant, ""));
        let point = if rest.is_empty() { "" } else { "." };
        format!("{first}{point}{rest}·10^{exponent}")
    } else if exponent >= 0 {
        let int_len = usize::try_from(exponent).unwrap_or(0).saturating_add(1);
        match significant.split_at_checked(int_len) {
            Some((int, frac)) if !frac.is_empty() => format!("{int}.{frac}"),
            _ => format!("{significant}{}", "0".repeat(int_len.saturating_sub(significant.len()))),
        }
    } else {
        let zeros = usize::try_from(-exponent).unwrap_or(1).saturating_sub(1);
        format!("0.{}{significant}", "0".repeat(zeros))
    };
    let body = if opts.decimal_comma { body.replace('.', ",") } else { body };
    if negative { format!("-{body}") } else { body }
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
