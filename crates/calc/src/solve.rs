//! Solving an equation for its one unknown: symbolic isolation, else a numeric root search.

use crate::eval::{Ctx, Names, Scope, Value};
use crate::parser::{BinOp, Expr, Func};
use crate::quantity::{Dim, Quantity};
use crate::{CalcError, MAX_DEPTH};

/// The names `expr` reads that are not bound by a `∑`/`∏` inside it, in first-use order.
pub(crate) fn free_names(expr: &Expr, out: &mut Vec<String>) {
    fn walk(expr: &Expr, bound: &mut Vec<String>, out: &mut Vec<String>) {
        match expr {
            Expr::Num(_) | Expr::Unit(_) => {}
            Expr::Name(n) => {
                if !bound.contains(n) && !out.contains(n) {
                    out.push(n.clone());
                }
            }
            Expr::Neg(e) => walk(e, bound, out),
            Expr::Bin(_, a, b) | Expr::Index(a, b) | Expr::Range(a, b) => {
                walk(a, bound, out);
                walk(b, bound, out);
            }
            Expr::Call(_, args) | Expr::Apply(_, args) | Expr::Vector(args) => args.iter().for_each(|a| walk(a, bound, out)),
            Expr::Nary(n) => {
                for e in [&n.from, &n.to].into_iter().flatten() {
                    walk(e, bound, out);
                }
                // A variable iterated without limits is read (its values); with limits it is bound.
                if let Some(var) = &n.var {
                    if n.from.is_none() && !bound.contains(var) && !out.contains(var) {
                        out.push(var.clone());
                    }
                    bound.push(var.clone());
                    walk(&n.body, bound, out);
                    bound.pop();
                } else {
                    walk(&n.body, bound, out);
                }
            }
        }
    }
    walk(expr, &mut Vec::new(), out);
}

/// How many times `expr` reads `x`.
fn count(expr: &Expr, x: &str) -> usize {
    let mut n = 0;
    let mut stack = vec![expr];
    while let Some(e) = stack.pop() {
        match e {
            Expr::Num(_) | Expr::Unit(_) => {}
            Expr::Name(name) => n += usize::from(name == x),
            Expr::Neg(a) => stack.push(a),
            Expr::Bin(_, a, b) | Expr::Index(a, b) | Expr::Range(a, b) => {
                stack.push(a);
                stack.push(b);
            }
            Expr::Call(_, args) | Expr::Apply(_, args) | Expr::Vector(args) => stack.extend(args.iter()),
            Expr::Nary(nary) => {
                stack.extend([&nary.from, &nary.to].into_iter().flatten().map(|b| &**b));
                stack.push(&nary.body);
            }
        }
    }
    n
}

fn bin(op: BinOp, a: Expr, b: Expr) -> Expr {
    Expr::Bin(op, Box::new(a), Box::new(b))
}

fn call(f: Func, a: Expr) -> Expr {
    Expr::Call(f, vec![a])
}

/// `lhs = rhs` rewritten as `x = <expression without x>`, when `x` occurs exactly once and
/// every step on the way to it can be inverted (`+ - · / ^ √ exp ln log sin cos tan …`). Roots
/// and inverse functions give their principal value.
pub(crate) fn isolate(lhs: &Expr, rhs: &Expr, x: &str) -> Option<Expr> {
    isolate_steps(lhs, rhs, x).map(|(form, _)| form)
}

/// Like [`isolate`], with the equation after each step on the way: `(side, other)` for
/// `side = other`, the side holding `x` on the left (`π·d² = 4·A`, `d² = 4·A/π`, `d = √(4·A/π)`).
/// The last step is `x = form`; none when `x` was alone already.
pub(crate) fn isolate_steps(lhs: &Expr, rhs: &Expr, x: &str) -> Option<(Expr, Vec<(Expr, Expr)>)> {
    let (in_left, in_right) = (count(lhs, x), count(rhs, x));
    if in_left + in_right != 1 {
        return None;
    }
    let (mut side, mut other) = if in_left == 1 { (lhs, rhs.clone()) } else { (rhs, lhs.clone()) };
    let mut steps: Vec<(Expr, Expr)> = Vec::new();
    // Each step peels one node, so the parser's node limit bounds the loop.
    for _ in 0..=crate::MAX_NODES {
        let has = |e: &Expr| count(e, x) > 0;
        match side {
            Expr::Name(n) if n == x => return Some((other, steps)),
            Expr::Neg(e) => {
                other = Expr::Neg(Box::new(other));
                side = e;
            }
            Expr::Bin(op, a, b) => {
                let left = has(a);
                let (a2, b2) = ((**a).clone(), (**b).clone());
                other = match (op, left) {
                    (BinOp::Add, true) => bin(BinOp::Sub, other, b2),
                    (BinOp::Add, false) => bin(BinOp::Sub, other, a2),
                    (BinOp::Sub, true) => bin(BinOp::Add, other, b2),
                    (BinOp::Sub, false) => bin(BinOp::Sub, a2, other),
                    (BinOp::Mul, true) => bin(BinOp::Div, other, b2),
                    (BinOp::Mul, false) => bin(BinOp::Div, other, a2),
                    // A number goes in front: `4·A`, not `A·4`.
                    (BinOp::Div, true) if matches!(b2, Expr::Num(_)) => bin(BinOp::Mul, b2, other),
                    (BinOp::Div, true) => bin(BinOp::Mul, other, b2),
                    (BinOp::Div, false) => bin(BinOp::Div, a2, other),
                    (BinOp::Pow, true) if b2 == Expr::Num(2.0) => call(Func::Sqrt, other),
                    (BinOp::Pow, true) => bin(BinOp::Pow, other, bin(BinOp::Div, Expr::Num(1.0), b2)),
                    (BinOp::Pow, false) => bin(BinOp::Div, call(Func::Ln, other), call(Func::Ln, a2)),
                };
                side = if left { a } else { b };
            }
            Expr::Call(f, args) => {
                let [arg] = args.as_slice() else { return None };
                other = match f {
                    Func::Sqrt => bin(BinOp::Pow, other, Expr::Num(2.0)),
                    Func::Abs => other,
                    Func::Exp => call(Func::Ln, other),
                    Func::Ln => call(Func::Exp, other),
                    Func::Log => bin(BinOp::Pow, Expr::Num(10.0), other),
                    Func::Sin => call(Func::Asin, other),
                    Func::Cos => call(Func::Acos, other),
                    Func::Tan => call(Func::Atan, other),
                    Func::Asin => call(Func::Sin, other),
                    Func::Acos => call(Func::Cos, other),
                    Func::Atan => call(Func::Tan, other),
                    _ => return None,
                };
                side = arg;
            }
            _ => return None,
        }
        steps.push((side.clone(), other.clone()));
    }
    None
}

/// Solves `lhs = rhs` for `x` numerically: tries a few dimensions for `x` (a plain number, and
/// those of the equation's known values), then secant steps from several starting points, then
/// a scan for a sign change refined by bisection. Returns the first root found.
pub(crate) fn numeric(lhs: &Expr, rhs: &Expr, x: &str, scope: &Scope) -> Result<Quantity, CalcError> {
    let mut dims = vec![Dim::NONE];
    let mut known = Vec::new();
    free_names(lhs, &mut known);
    free_names(rhs, &mut known);
    // One budget for the whole search: the grid, the bisections and the secant steps.
    let spent = std::cell::Cell::new(0usize);
    let ctx_eval = |e: &Expr, at: Option<Quantity>| -> Result<Value, CalcError> {
        let locals = at.map(|q| vec![(x.to_string(), Value::Scalar(q))]).unwrap_or_default();
        let mut ctx = Ctx::new(scope, Names::Worksheet, locals).with_steps(spent.get());
        let result = ctx.eval(e);
        spent.set(ctx.steps());
        result
    };
    for name in known.iter().filter(|n| *n != x) {
        if let Ok(v) = ctx_eval(&Expr::Name(name.clone()), None) {
            dims.push(v.dim());
        }
    }
    for side in [lhs, rhs] {
        if count(side, x) == 0
            && let Ok(v) = ctx_eval(side, None)
        {
            let d = v.dim();
            dims.push(d);
            for p in [0.5, 1.0 / 3.0] {
                if let Ok(root) = d.try_pow(p) {
                    dims.push(root);
                }
            }
        }
    }
    let mut last_err = CalcError::NoSolution(x.to_string());
    let mut tried = Vec::new();
    for dim in dims {
        if tried.contains(&dim) || tried.len() >= MAX_DEPTH {
            continue;
        }
        tried.push(dim);
        let f = |v: f64| -> Result<f64, CalcError> {
            let q = Some(Quantity::new(v, dim));
            let a = ctx_eval(lhs, q)?.expect_scalar()?;
            let b = ctx_eval(rhs, q)?.expect_scalar()?;
            Ok(a.try_sub(b)?.value)
        };
        // A unit error means the wrong dimension; other failures at 1 (a logarithm of a negative
        // number) may vanish elsewhere.
        if let Err(
            e @ (CalcError::DimensionMismatch { .. }
            | CalcError::FractionalDimension { .. }
            | CalcError::NeedsDimensionless { .. }
            | CalcError::ExponentNotDimensionless { .. }),
        ) = f(1.0)
        {
            last_err = e;
            continue;
        }
        if let Some(v) = root(&f) {
            return Ok(Quantity::new(v, dim));
        }
        last_err = CalcError::NoSolution(x.to_string());
    }
    if spent.get() > crate::MAX_STEPS {
        return Err(CalcError::TooMuchWork);
    }
    Err(last_err)
}

/// Grid points per decade searched for sign changes, from 10⁻¹² to 10¹² (both signs).
const PER_DECADE: i32 = 8;

/// A root of `f`, if one is found: every sign change on a logarithmic grid between ±10⁻¹² and
/// ±10¹² is refined by bisection (a change across a pole is dropped), and secant steps catch
/// roots that touch zero without crossing it. Of the roots found, the smallest non-negative one
/// wins, else the negative one closest to zero.
fn root(f: &dyn Fn(f64) -> Result<f64, CalcError>) -> Option<f64> {
    let mut grid: Vec<f64> = (-12 * PER_DECADE..=12 * PER_DECADE).map(|k| 10f64.powf(f64::from(k) / f64::from(PER_DECADE))).collect();
    let negatives: Vec<f64> = grid.iter().rev().map(|p| -p).collect();
    grid.splice(0..0, negatives.into_iter().chain([0.0]));
    let values: Vec<(f64, f64)> = grid.iter().filter_map(|&p| f(p).ok().filter(|v| v.is_finite()).map(|v| (p, v))).collect();
    let mut roots = Vec::new();
    for pair in values.windows(2) {
        let (Some(&(a0, fa0)), Some(&(b0, fb0))) = (pair.first(), pair.get(1)) else { continue };
        if fa0 == 0.0 {
            roots.push(a0);
            continue;
        }
        if fa0.signum() == fb0.signum() || fb0 == 0.0 {
            continue;
        }
        let (mut a, mut fa, mut b) = (a0, fa0, b0);
        for _ in 0..200 {
            let m = 0.5 * (a + b);
            let Ok(fm) = f(m) else { break };
            if fm == 0.0 || (b - a).abs() <= 1e-15 * m.abs().max(1e-300) {
                a = m;
                b = m;
                break;
            }
            if fm.signum() == fa.signum() {
                (a, fa) = (m, fm);
            } else {
                b = m;
            }
        }
        let m = 0.5 * (a + b);
        // A sign change across a pole (1/x) is not a root: the refined value must be small.
        if f(m).is_ok_and(|fm| fm.abs() <= 1e-9 * fa0.abs().max(fb0.abs())) {
            roots.push(m);
        }
    }
    if let Some(&(last, fl)) = values.last()
        && fl == 0.0
    {
        roots.push(last);
    }
    // Secant steps for roots that only touch zero (x² = 0).
    for &x0 in &[1.0, -1.0, 0.5, 2.0, 10.0, 0.1, 100.0, -10.0] {
        let (mut a, mut b) = (x0, x0 * 1.01 + 1e-6);
        let (Ok(mut fa), Ok(mut fb)) = (f(a), f(b)) else { continue };
        let scale = fa.abs().max(fb.abs()).max(1e-300);
        for _ in 0..100 {
            if fb.abs() <= 1e-12 * scale {
                roots.push(b);
                break;
            }
            let d = fb - fa;
            if d == 0.0 || !d.is_finite() {
                break;
            }
            let c = b - fb * (b - a) / d;
            let Ok(fc) = f(c) else { break };
            if !c.is_finite() || !fc.is_finite() {
                break;
            }
            (a, fa, b, fb) = (b, fb, c, fc);
        }
    }
    let smallest_non_negative = roots.iter().copied().filter(|r| *r >= 0.0).reduce(f64::min);
    smallest_non_negative.or_else(|| roots.iter().copied().filter(|r| *r < 0.0).reduce(f64::max))
}
