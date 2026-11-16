//! Closed forms of sums: what a `∑` is worth as a formula, without adding up its terms.
//!
//! One identity is known, the **binomial theorem**, in its general shape:
//!
//! ```text
//! ∑_(k=0)^n (n¦k)·u^k·v^(n−k) = (u + v)^n
//! ```
//!
//! - the sum runs from 0 to the coefficient's own `n` (any expression without `k`);
//! - the coefficient is written `(n¦k)`, `binom(n; k)` or, the same number, with `n − k` below;
//! - `u^k` and `v^(n−k)` may each be missing (then 1: `∑ (n¦k) = 2^n`, `∑ (n¦k)·x^k = (1 + x)^n`)
//!   or several (`2^k·x^k` is `(2·x)^k`); a negative base alternates (`(−1)^k·x^k` gives
//!   `(a − x)^n`);
//! - factors without `k` come out of the sum (`c·…`, `…/d`).
//!
//! It holds for every whole `n ≥ 0`, which a sum's upper limit is. A worksheet that knows the
//! values still adds the terms up, and shows the closed form only when it gives the same value
//! ([`crate::Worksheet`]); with names unknown the closed form is the result.

use crate::parser::{BinOp, Expr, Func, Nary};
use crate::solve::free_names;

/// Deepest expression walked; deeper ones have no closed form.
const MAX_DEPTH: usize = 128;

/// `expr` with every sum that has a closed form replaced by it; `None` when it has none.
pub(crate) fn closed_form(expr: &Expr) -> Option<Expr> {
    let mut found = false;
    let out = rewrite(expr, &mut found, 0)?;
    found.then_some(out)
}

/// True when `expr` still holds a `∑`, `∏` or `∫`.
pub(crate) fn has_nary(expr: &Expr) -> bool {
    let mut stack = vec![expr];
    while let Some(e) = stack.pop() {
        match e {
            Expr::Nary(_) => return true,
            Expr::Num(_) | Expr::Name(_) | Expr::Unit(_) => {}
            Expr::Neg(a) => stack.push(a),
            Expr::Bin(_, a, b) | Expr::Index(a, b) | Expr::Range(a, b) => {
                stack.push(a);
                stack.push(b);
            }
            Expr::Call(_, args) | Expr::Apply(_, args) | Expr::Vector(args) => stack.extend(args.iter()),
        }
    }
    false
}

/// A copy of `expr` with its closed sums rewritten (`found` set when any was); `None` past
/// [`MAX_DEPTH`].
fn rewrite(expr: &Expr, found: &mut bool, depth: usize) -> Option<Expr> {
    if depth > MAX_DEPTH {
        return None;
    }
    let mut go = |e: &Expr| rewrite(e, found, depth + 1);
    Some(match expr {
        Expr::Num(_) | Expr::Name(_) | Expr::Unit(_) => expr.clone(),
        Expr::Neg(a) => Expr::Neg(Box::new(go(a)?)),
        Expr::Bin(op, a, b) => Expr::Bin(*op, Box::new(go(a)?), Box::new(go(b)?)),
        Expr::Index(a, b) => Expr::Index(Box::new(go(a)?), Box::new(go(b)?)),
        Expr::Range(a, b) => Expr::Range(Box::new(go(a)?), Box::new(go(b)?)),
        Expr::Call(f, args) => Expr::Call(*f, args.iter().map(&mut go).collect::<Option<_>>()?),
        Expr::Apply(name, args) => Expr::Apply(name.clone(), args.iter().map(&mut go).collect::<Option<_>>()?),
        Expr::Vector(items) => Expr::Vector(items.iter().map(&mut go).collect::<Option<_>>()?),
        Expr::Nary(n) => match binomial_theorem(n) {
            Some(closed) => {
                *found = true;
                closed
            }
            None => {
                let part = |e: &Option<Box<Expr>>, go: &mut dyn FnMut(&Expr) -> Option<Expr>| match e {
                    Some(e) => go(e).map(|e| Some(Box::new(e))),
                    None => Some(None),
                };
                let (from, to) = (part(&n.from, &mut go)?, part(&n.to, &mut go)?);
                Expr::Nary(Nary { op: n.op, var: n.var.clone(), from, to, body: Box::new(go(&n.body)?) })
            }
        },
    })
}

fn reads(expr: &Expr, name: &str) -> bool {
    let mut names = Vec::new();
    free_names(expr, &mut names);
    names.iter().any(|n| n == name)
}

fn bin(op: BinOp, a: Expr, b: Expr) -> Expr {
    Expr::Bin(op, Box::new(a), Box::new(b))
}

/// The factors of a product, in order: `(factor, divides)`; a sign in front of any of them
/// flips `negative`. `None` past [`MAX_DEPTH`].
fn factors<'e>(expr: &'e Expr, divides: bool, negative: &mut bool, out: &mut Vec<(&'e Expr, bool)>, depth: usize) -> Option<()> {
    if depth > MAX_DEPTH {
        return None;
    }
    match expr {
        Expr::Bin(BinOp::Mul, a, b) => {
            factors(a, divides, negative, out, depth + 1)?;
            factors(b, divides, negative, out, depth + 1)?;
        }
        Expr::Bin(BinOp::Div, a, b) => {
            factors(a, divides, negative, out, depth + 1)?;
            factors(b, !divides, negative, out, depth + 1)?;
        }
        Expr::Neg(a) => {
            *negative = !*negative;
            factors(a, divides, negative, out, depth + 1)?;
        }
        other => out.push((other, divides)),
    }
    Some(())
}

/// The product of the bases raised to one exponent (`2^k·x^k` is `(2·x)^k`), with its sign
/// apart: `(negative, product)`; no product when every base is 1 or −1.
fn base_product(bases: &[&Expr]) -> (bool, Option<Expr>) {
    let mut negative = false;
    let mut product: Option<Expr> = None;
    for base in bases {
        let mut base = *base;
        while let Expr::Neg(inner) = base {
            negative = !negative;
            base = inner;
        }
        if matches!(base, Expr::Num(v) if *v == 1.0) {
            continue;
        }
        product = Some(match product {
            Some(p) => bin(BinOp::Mul, p, base.clone()),
            None => base.clone(),
        });
    }
    (negative, product)
}

/// `u + v` for the two signed bases of the theorem, each 1 when it has no product, written the
/// way it reads best: `x + a`, `a - x`, `1 + x`, `1 - x`, `2`. `None` when it is 0 (`1 − 1`:
/// `0^n` says nothing useful) or would start with two signs.
fn base_sum(u: (bool, Option<Expr>), v: (bool, Option<Expr>)) -> Option<Expr> {
    let one = || Expr::Num(1.0);
    Some(match (u, v) {
        // Numbers alone: 1 + 1, 1 − 1, −1 − 1.
        ((nu, None), (nv, None)) => match (nu, nv) {
            (false, false) => Expr::Num(2.0),
            _ => return None,
        },
        ((false, Some(u)), (false, Some(v))) => bin(BinOp::Add, u, v),
        ((true, Some(u)), (false, Some(v))) => bin(BinOp::Sub, v, u),
        ((false, Some(u)), (true, Some(v))) => bin(BinOp::Sub, u, v),
        ((false, None), (false, Some(x))) | ((false, Some(x)), (false, None)) => bin(BinOp::Add, one(), x),
        ((false, None), (true, Some(x))) | ((true, Some(x)), (false, None)) => bin(BinOp::Sub, one(), x),
        ((true, None), (false, Some(x))) | ((false, Some(x)), (true, None)) => bin(BinOp::Sub, x, one()),
        _ => return None,
    })
}

/// The binomial theorem (see the module docs): the closed form of `sum`, when it has that shape.
fn binomial_theorem(sum: &Nary) -> Option<Expr> {
    let (Some(k), Some(from), Some(n)) = (&sum.var, &sum.from, &sum.to) else { return None };
    if sum.op != '∑' || !matches!(**from, Expr::Num(v) if v == 0.0) || reads(n, k) {
        return None;
    }
    let is_k = |e: &Expr| matches!(e, Expr::Name(name) if name == k);
    let is_n_minus_k = |e: &Expr| matches!(e, Expr::Bin(BinOp::Sub, a, b) if **a == **n && is_k(b));
    let mut negative = false;
    let mut parts = Vec::new();
    factors(&sum.body, false, &mut negative, &mut parts, 0)?;
    let (mut coefficients, mut constants) = (0usize, Vec::new());
    let (mut with_k, mut with_rest): (Vec<&Expr>, Vec<&Expr>) = (Vec::new(), Vec::new());
    for (factor, divides) in parts {
        if !reads(factor, k) {
            constants.push((factor, divides));
            continue;
        }
        match factor {
            _ if divides => return None,
            Expr::Call(Func::Binom | Func::Choose, args) => match args.as_slice() {
                [top, below] if *top == **n && (is_k(below) || is_n_minus_k(below)) => coefficients += 1,
                _ => return None,
            },
            Expr::Bin(BinOp::Pow, base, exponent) if !reads(base, k) => {
                if is_k(exponent) {
                    with_k.push(base);
                } else if is_n_minus_k(exponent) {
                    with_rest.push(base);
                } else {
                    return None;
                }
            }
            _ => return None,
        }
    }
    if coefficients != 1 {
        return None;
    }
    let base = base_sum(base_product(&with_k), base_product(&with_rest))?;
    let power = bin(BinOp::Pow, base, (**n).clone());
    // The factors without `k` around it, as they were written: `c·(x + a)^n/d`.
    let mut out: Option<Expr> = None;
    for (c, _) in constants.iter().filter(|(_, divides)| !divides) {
        out = Some(match out {
            Some(o) => bin(BinOp::Mul, o, (*c).clone()),
            None => (*c).clone(),
        });
    }
    let mut out = match out {
        Some(o) => bin(BinOp::Mul, o, power),
        None => power,
    };
    for (d, _) in constants.iter().filter(|(_, divides)| *divides) {
        out = bin(BinOp::Div, out, (*d).clone());
    }
    Some(if negative { Expr::Neg(Box::new(out)) } else { out })
}

/// True when `lhs = rhs` is one of the identities above read either way: one side is the closed
/// form of the other, whatever the order of its terms and factors (`(a + x)^n`).
pub(crate) fn identity(lhs: &Expr, rhs: &Expr) -> bool {
    let same = |sum: &Expr, other: &Expr| {
        closed_form(sum).is_some_and(|c| match (canonical(&c, 0), canonical(other, 0)) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        })
    };
    same(lhs, rhs) || same(rhs, lhs)
}

/// `expr` as text with the terms of its sums and the factors of its products in one order, so
/// `x + a` and `a + x` read the same. `None` past [`MAX_DEPTH`].
fn canonical(expr: &Expr, depth: usize) -> Option<String> {
    if depth > MAX_DEPTH {
        return None;
    }
    fn chain<'e>(expr: &'e Expr, op: BinOp, out: &mut Vec<&'e Expr>, depth: usize) -> Option<()> {
        if depth > MAX_DEPTH {
            return None;
        }
        match expr {
            Expr::Bin(o, a, b) if *o == op => {
                chain(a, op, out, depth + 1)?;
                chain(b, op, out, depth + 1)?;
            }
            other => out.push(other),
        }
        Some(())
    }
    let go = |e: &Expr| canonical(e, depth + 1);
    let list = |items: &[Expr]| items.iter().map(go).collect::<Option<Vec<_>>>().map(|v| v.join("; "));
    Some(match expr {
        Expr::Bin(op @ (BinOp::Add | BinOp::Mul), ..) => {
            let mut items = Vec::new();
            chain(expr, *op, &mut items, depth)?;
            let mut items = items.into_iter().map(go).collect::<Option<Vec<_>>>()?;
            items.sort();
            format!("({})", items.join(if *op == BinOp::Add { " + " } else { "·" }))
        }
        Expr::Bin(op, a, b) => format!("({} {op:?} {})", go(a)?, go(b)?),
        Expr::Neg(a) => format!("(-{})", go(a)?),
        Expr::Index(a, b) => format!("{}[{}]", go(a)?, go(b)?),
        Expr::Range(a, b) => format!("{}..{}", go(a)?, go(b)?),
        Expr::Call(f, args) => format!("{}({})", f.name(), list(args)?),
        Expr::Apply(name, args) => format!("{name}({})", list(args)?),
        Expr::Vector(items) => format!("[{}]", list(items)?),
        Expr::Num(_) | Expr::Name(_) | Expr::Unit(_) | Expr::Nary(_) => crate::format::render(expr),
    })
}

#[cfg(test)]
mod tests {
    use crate::*;

    fn run(statements: &[&str], steps: bool) -> RegionResult {
        let opts = Options { steps, ..Options::default() };
        evaluate(statements, &opts).pop().unwrap()
    }

    fn last(statements: &[&str]) -> String {
        run(statements, false).display.unwrap_or_default()
    }

    fn stepped(statements: &[&str]) -> String {
        run(statements, true).display.unwrap_or_default()
    }

    /// The sum as the equation editor writes it (Word's linear format).
    const SUM: &str = "∑_(k=0)^n▒((n¦k)x^k a^(n-k))";

    #[test]
    fn the_binomial_sum_is_worked_out() {
        // With its values it is a number, the terms added up.
        let data = ["n = 3", "x = 2", "a = 5"];
        assert_eq!(last(&[data[0], data[1], data[2], &format!("{SUM}=")]), format!("{SUM} = 343"));
        // The steps go through the closed form: the formula, its values, the result.
        assert_eq!(stepped(&[data[0], data[1], data[2], &format!("{SUM}=")]), format!("{SUM} = (x + a)^n = (2 + 5)^3 = 343"));
        // With units: every term is a volume.
        assert_eq!(stepped(&["n = 3", "x = 2 m", "a = 0.5 m", &format!("{SUM}=")]), format!("{SUM} = (x + a)^n = ((2 m) + (0.5 m))^3 = 15.625 m³"));
        // Written with the function, with spaces and dots, with numbers.
        assert_eq!(last(&["∑_(k=0)^4▒binom(4; k)·2^k·3^(4 - k) ="]), "∑_(k=0)^4▒binom(4; k)·2^k·3^(4 - k) = 625");
        assert_eq!(stepped(&["∑_(k=0)^4▒(4¦k)·2^k·3^(4 - k) ="]), "∑_(k=0)^4▒(4¦k)·2^k·3^(4 - k) = (2 + 3)^4 = 625");
    }

    #[test]
    fn the_binomial_sum_line_by_line() {
        let lines = |statements: &[&str]| {
            let opts = Options { steps: true, layout: Layout::Lines, ..Options::default() };
            let r = evaluate(statements, &opts).pop().unwrap();
            (r.display.unwrap_or_default(), r.steps.unwrap_or_default())
        };
        // The formula alone stays on the statement's line, like a result.
        assert_eq!(lines(&[&format!("{SUM}=")]), (format!("{SUM} = (x + a)^n"), vec![]));
        // With values: the closed form, its values, the result, one under the other.
        let worked = lines(&["n = 3", "x = 2", "a = 5", &format!("{SUM}=")]);
        assert_eq!(worked, (SUM.to_string(), vec!["= (x + a)^n".to_string(), "= (2 + 5)^3".to_string(), "= 343".to_string()]));
        // Read back as the statement and its last line.
        assert_eq!(lines(&["n = 3", "x = 2", "a = 5", &format!("{SUM} = 343")]), worked);
    }

    #[test]
    fn the_binomial_sum_without_values_is_its_closed_form() {
        assert_eq!(last(&[&format!("{SUM}=")]), format!("{SUM} = (x + a)^n"));
        let r = run(&[&format!("{SUM}=")], true);
        assert_eq!((r.error, r.value, r.kind), (None, None, RegionKind::Evaluation));
        // Some names known: still the formula, by name.
        assert_eq!(last(&["n = 3", &format!("{SUM}=")]), format!("{SUM} = (x + a)^n"));
        // Word's own minus sign and invisible brackets.
        assert_eq!(last(&["∑_(k=0)^n▒〖(n¦k) x^k a^(n−k)〗="]), "∑_(k=0)^n▒〖(n¦k) x^k a^(n−k)〗 = (x + a)^n");
        // The shapes of the theorem.
        for (sum, closed) in [
            ("∑_(k=0)^n▒(n¦k)", "2^n"),
            ("∑_(k=0)^n▒(n¦k)·x^k", "(1 + x)^n"),
            ("∑_(k=0)^n▒(n¦k)·b^(n-k)", "(1 + b)^n"),
            ("∑_(k=0)^n▒(n¦(n-k))·x^k·y^(n-k)", "(x + y)^n"),
            ("∑_(k=0)^n▒(n¦k)·(-1)^k·x^k·y^(n-k)", "(y - x)^n"),
            ("∑_(k=0)^n▒(n¦k)·(-x)^k", "(1 - x)^n"),
            ("∑_(k=0)^n▒(n¦k)·x^k·(-1)^(n-k)", "(x - 1)^n"),
            ("∑_(k=0)^n▒(n¦k)·2^k·x^k·y^(n-k)", "(2·x + y)^n"),
            ("∑_(k=0)^n▒c·(n¦k)·x^k·y^(n-k)/w", "c·(x + y)^n/w"),
            ("∑_(k=0)^n▒-(n¦k)·x^k·y^(n-k)", "-(x + y)^n"),
            ("∑_(j=0)^(r+1)▒((r+1)¦j)·p^j·q^(r+1-j)", "(p + q)^(r + 1)"),
            // Names spelled like units (the metre, the hour, the newton) are names here.
            ("∑_(k=0)^N▒(N¦k)·x^k·h^(N-k)", "(x + h)^N"),
            ("∑_(j=0)^(m+1)▒((m+1)¦j)·p^j·q^(m+1-j)", "(p + q)^(m + 1)"),
            ("2·∑_(k=0)^n▒(n¦k)·x^k·y^(n-k) + 1", "2·(x + y)^n + 1"),
        ] {
            assert_eq!(last(&[&format!("{sum} =")]), format!("{sum} = {closed}"), "{sum}");
        }
    }

    #[test]
    fn sums_that_are_not_the_theorem_keep_their_error() {
        for sum in [
            "∑_(k=1)^n▒(n¦k)·x^k·y^(n-k)",
            "∑_(k=0)^r▒(n¦k)·x^k·y^(n-k)",
            "∑_(k=0)^n▒(n¦k)·x^(2·k)",
            "∑_(k=0)^n▒(n¦k)·k^k",
            "∑_(k=0)^n▒(n¦k)·(n¦k)",
            "∑_(k=0)^n▒(n¦k)·k",
            "∑_(k=0)^n▒(n¦k)/x^k",
            "∑_(k=0)^n▒x^k·y^(n-k)",
            "∑_(k=0)^n▒(n¦k)·(-1)^k",
            "∏_(k=0)^n▒(n¦k)·x^k",
            "∑_(k=0)^n▒(n¦k)·x^k + ∑_(i=1)^m▒i",
        ] {
            let r = run(&[&format!("{sum} =")], false);
            assert!(matches!(r.error, Some(CalcError::UnknownName(_))), "{sum}: {:?}", r.display);
            assert_eq!(r.display, Some(format!("{sum} = ⚠")), "{sum}");
        }
        // A unit asked for needs a number.
        let r = run(&[&format!("{SUM}= m^3")], false);
        assert!(matches!(r.error, Some(CalcError::UnknownName(_))), "{:?}", r.display);
        // Constants are values: with nothing unknown the terms are added up.
        assert_eq!(last(&["∑_(k=0)^2▒(2¦k)·π^k·e^(2-k) ="]), "∑_(k=0)^2▒(2¦k)·π^k·e^(2-k) = 34.338");
        // Values that break the sum are errors, not formulas.
        assert!(matches!(run(&["n = 2.5", "x = 1", "a = 1", &format!("{SUM}=")], false).error, Some(CalcError::NotWhole { .. })));
    }

    #[test]
    fn what_the_sum_showed_is_read_back() {
        // The formula it showed is ours: once the values are there, the number takes its place.
        let shown = format!("{SUM} = (x + a)^n");
        assert_eq!(last(&[&shown]), shown);
        assert_eq!(last(&["n = 3", "x = 2", "a = 5", &shown]), format!("{SUM} = 343"));
        // As the built-up equation spells it back.
        assert_eq!(last(&["n = 3", "x = 2", "a = 5", &format!("{SUM}=(x+a)^n")]), format!("{SUM} = 343"));
        // Its steps too, and a stale result; values taken away, the formula is back.
        let steps = format!("{SUM} = (x + a)^n = (2 + 5)^3 = 343");
        assert_eq!(stepped(&["n = 3", "x = 2", "a = 5", &steps]), steps);
        assert_eq!(stepped(&["n = 2", "x = 2", "a = 5", &steps]), format!("{SUM} = (x + a)^n = (2 + 5)^2 = 49"));
        // A number alone after `=` with names unknown is an equation, as ever.
        assert!(matches!(run(&[&format!("{SUM} = 343")], false).error, Some(CalcError::Underdetermined(_))));
        assert_eq!(last(&["n = 3", "a = 5", &format!("{SUM} = 343")]), format!("{SUM} = 343 ⇒ x = 2"));
        // A unit asked for after the working is kept.
        assert_eq!(
            stepped(&["n = 2", "x = 2 m", "a = 1 m", &format!("{SUM} = (x + a)^n = ((2 m) + (1 m))^2 = 90000 cm^2")]),
            format!("{SUM} = (x + a)^n = ((2 m) + (1 m))^2 = 90000 cm²")
        );
        // A definition goes the same way.
        let defined = stepped(&["n = 3", "x = 2", "a = 5", &format!("y = {SUM}")]);
        assert_eq!(defined, format!("y = {SUM} = (x + a)^n = (2 + 5)^3 = 343"));
        assert_eq!(stepped(&["n = 3", "x = 2", "a = 5", &defined]), defined);
        let defined = stepped(&["n = 3", "x = 2", "a = 5", &format!("y := {SUM} =")]);
        assert_eq!(defined, format!("y := {SUM} = (x + a)^n = (2 + 5)^3 = 343"));
        assert_eq!(stepped(&["n = 3", "x = 2", "a = 5", &defined, "y ="]), "y = 343");
        // Another formula after `=` is the user's equation, not ours.
        let r = run(&[&format!("{SUM} = (x + b)^n")], false);
        assert!(matches!(r.error, Some(CalcError::Underdetermined(_))));
    }

    #[test]
    fn the_identity_written_out_holds() {
        // The gallery's binomial expansion, a calculation like any equation: nothing to report.
        for identity in
            ["(x+a)^n=∑_(k=0)^n▒〖(n¦k) x^k a^(n−k)〗", "(a + x)^n = ∑_(k=0)^n▒(n¦k)·x^k·a^(n-k)", "∑_(k=0)^n▒(n¦k)·x^k·a^(n-k) = (a + x)^n"]
        {
            let r = run(&[identity], false);
            assert_eq!((r.kind, r.error, r.display), (RegionKind::Check, None, None), "{identity}");
            // One name known, two, all three: still an identity, then a check that holds.
            assert_eq!(run(&["n = 3", identity], false).error, None, "{identity}");
            assert_eq!(run(&["n = 3", "a = 5", identity], false).error, None, "{identity}");
            assert_eq!(run(&["n = 3", "a = 5", "x = 2", identity], false).error, None, "{identity}");
        }
        // Not the identity: reported as before.
        assert!(matches!(run(&["(x+a)^m=∑_(k=0)^n▒〖(n¦k) x^k a^(n−k)〗"], false).error, Some(CalcError::Underdetermined(_))));
    }

    #[test]
    fn a_statement_that_cannot_be_read_keeps_its_sum() {
        // The `=` of the lower limit is not the statement's: the sum stays whole before `= ⚠`.
        let r = run(&["∑_(k=0)^n▒(k # 2) ="], false);
        assert!(matches!(r.error, Some(CalcError::Syntax { .. })));
        assert_eq!(r.display.as_deref(), Some("∑_(k=0)^n▒(k # 2) = ⚠"));
        let r = run(&["y := ∑_(k=0)^n▒(k # 2) ="], false);
        assert_eq!(r.display.as_deref(), Some("y := ∑_(k=0)^n▒(k # 2) = ⚠"));
        // Brackets left open: the last `=`.
        let r = run(&["∑_(k=0▒k # ="], false);
        assert_eq!(r.display.as_deref(), Some("∑_(k=0▒k # = ⚠"));
    }
}
