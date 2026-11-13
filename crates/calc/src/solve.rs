//! Solving an equation for its one unknown: symbolic isolation, degree 1–2 polynomials,
//! absolute-value branching, else a numeric root search with algebraic dimensional inference.

use crate::eval::{Ctx, DomainConstraint, Names, Scope, Value, Vector, eval, eval_scalar};
use crate::parser::{BinOp, Expr, Func};
use crate::quantity::{Dim, Quantity, finite};
use crate::{CalcError, MAX_DEPTH};

/// Symbolic steps for a solved quadratic equation `A·x² + B·x + C = 0`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct QuadraticPlan {
    /// Standard form `A·x² + B·x + C` (equated to `0`).
    pub standard_lhs: Expr,
    pub a: Expr,
    pub b: Expr,
    pub c: Expr,
}

/// Symbolic steps for an absolute-value equation `|u(x)| = rhs`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AbsPlan {
    /// Outer steps leading up to `|u(x)| = rhs`.
    pub outer_steps: Vec<(Expr, Expr)>,
    pub u: Expr,
    pub rhs: Expr,
}

/// How [`solve_symbolic`] found a solution.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SymbolicKind {
    Isolated(Expr, Vec<(Expr, Expr)>),
    Quadratic(QuadraticPlan),
    Abs(AbsPlan),
}

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
            Expr::Call(f, args) if f.is_rounding() => args.iter().take(2).for_each(|a| walk(a, bound, out)),
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
pub(crate) fn count(expr: &Expr, x: &str) -> usize {
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
            Expr::Call(f, args) if f.is_rounding() => stack.extend(args.iter().take(2)),
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
/// every single-valued step on the way to it can be inverted.
pub(crate) fn isolate(lhs: &Expr, rhs: &Expr, x: &str) -> Option<Expr> {
    isolate_steps(lhs, rhs, x).map(|(form, _)| form)
}

/// Like [`isolate`], with the equation after each step on the way: `(side, other)` for
/// `side = other`, the side holding `x` on the left (`π·d² = 4·A`, `d² = 4·A/π`, `d = √(4·A/π)`).
/// The last step is `x = form`; none when `x` was alone already. `Func::Abs` returns `None` here
/// and is handled with `±` branching by [`solve_symbolic`].
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
                other = neg_expr(other);
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
                    (BinOp::Pow, true) if b2 == Expr::Num(3.0) => call(Func::Cbrt, other),
                    (BinOp::Pow, true) => bin(BinOp::Pow, other, bin(BinOp::Div, Expr::Num(1.0), b2)),
                    (BinOp::Pow, false) => bin(BinOp::Div, call(Func::Ln, other), call(Func::Ln, a2)),
                };
                side = if left { a } else { b };
            }
            Expr::Call(f, args) => match (f, args.as_slice()) {
                (Func::Root, [arg, n]) if !has(n) => {
                    other = bin(BinOp::Pow, other, n.clone());
                    side = arg;
                }
                (Func::Log, [arg, base]) if !has(base) => {
                    other = bin(BinOp::Pow, base.clone(), other);
                    side = arg;
                }
                (_, [arg]) => {
                    other = match f {
                        Func::Sqrt => bin(BinOp::Pow, other, Expr::Num(2.0)),
                        Func::Cbrt => bin(BinOp::Pow, other, Expr::Num(3.0)),
                        Func::Exp => call(Func::Ln, other),
                        Func::Ln => call(Func::Exp, other),
                        Func::Log => bin(BinOp::Pow, Expr::Num(10.0), other),
                        Func::Log2 => bin(BinOp::Pow, Expr::Num(2.0), other),
                        Func::Sin => call(Func::Asin, other),
                        Func::Cos => call(Func::Acos, other),
                        Func::Tan => call(Func::Atan, other),
                        Func::Asin => call(Func::Sin, other),
                        Func::Acos => call(Func::Cos, other),
                        Func::Atan => call(Func::Tan, other),
                        Func::Sinh => call(Func::Asinh, other),
                        Func::Cosh => call(Func::Acosh, other),
                        Func::Tanh => call(Func::Atanh, other),
                        Func::Asinh => call(Func::Sinh, other),
                        Func::Acosh => call(Func::Cosh, other),
                        Func::Atanh => call(Func::Tanh, other),
                        _ => return None,
                    };
                    side = arg;
                }
                _ => return None,
            },
            _ => return None,
        }
        steps.push((side.clone(), other.clone()));
    }
    None
}

/// Attempts symbolic resolution of `lhs = rhs` for `x` when `isolate_steps` does not apply:
/// - `|u(x)| = c` branching (`Func::Abs`);
/// - symbolic polynomial extraction `poly_coeffs(expr, x)` of degree 1 or 2 with dimensional
///   homogeneity verification, explicit `Δ < 0` error, both roots, and domain-constraint filtering.
pub(crate) fn solve_symbolic(
    lhs: &Expr,
    rhs: &Expr,
    x: &str,
    scope: &Scope,
    extra: &[DomainConstraint],
) -> Option<Result<(Value, SymbolicKind), CalcError>> {
    if let Some(res) = solve_abs(lhs, rhs, x, scope, extra, 0) {
        return Some(res);
    }
    solve_poly(lhs, rhs, x, scope, extra)
}

fn check_constraints(q: Quantity, x: &str, scope: &Scope, extra: &[DomainConstraint]) -> Result<bool, CalcError> {
    for c in scope.constraints_for(x).chain(extra.iter().filter(|c| c.var == x)) {
        if !c.accepts(q)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn pack_roots(mut roots: Vec<Quantity>, x: &str, scope: &Scope, extra: &[DomainConstraint]) -> Result<Value, CalcError> {
    roots.sort_by(|a, b| a.value.total_cmp(&b.value));
    let mut unique: Vec<Quantity> = Vec::new();
    for q in roots {
        if !check_constraints(q, x, scope, extra)? {
            continue;
        }
        let dup = unique.last().is_some_and(|prev| (prev.value - q.value).abs() <= 1e-11 * prev.value.abs().max(q.value.abs()).max(1e-30));
        if !dup {
            unique.push(q);
        }
    }
    match unique.as_slice() {
        [] => Err(CalcError::NoSolution(x.to_string())),
        [one] => Ok(Value::Scalar(*one)),
        many => Ok(Value::Vector(Vector::from_quantities(many)?)),
    }
}

/// Peels invertible outer layers until `|u(x)| = rhs` is reached, then branches `u(x) = -rhs` and
/// `u(x) = +rhs`.
fn solve_abs(
    lhs: &Expr,
    rhs: &Expr,
    x: &str,
    scope: &Scope,
    extra: &[DomainConstraint],
    depth: usize,
) -> Option<Result<(Value, SymbolicKind), CalcError>> {
    if depth > 8 {
        return None;
    }
    let (in_l, in_r) = (count(lhs, x), count(rhs, x));
    if (in_l > 0) == (in_r > 0) {
        return None;
    }
    let (mut side, mut other) = if in_l > 0 { (lhs, rhs.clone()) } else { (rhs, lhs.clone()) };
    let mut outer_steps = Vec::new();
    for _ in 0..=crate::MAX_NODES {
        let has = |e: &Expr| count(e, x) > 0;
        match side {
            Expr::Call(Func::Abs, args) => {
                let [u] = args.as_slice() else { return None };
                let c = match eval_scalar(&other, scope, Names::Worksheet) {
                    Ok(c) => c,
                    Err(e) => return Some(Err(e)),
                };
                if c.value < 0.0 {
                    return Some(Err(CalcError::NoSolution(x.to_string())));
                }
                let plan = AbsPlan { outer_steps, u: u.clone(), rhs: other.clone() };
                if c.value == 0.0 {
                    let branch = solve_branch(u, &other, x, scope, extra, depth + 1)?;
                    return Some(branch.and_then(|rs| Ok((pack_roots(rs, x, scope, extra)?, SymbolicKind::Abs(plan)))));
                }
                let neg_rhs = neg_expr(other.clone());
                let r_neg = solve_branch(u, &neg_rhs, x, scope, extra, depth + 1)?;
                let r_pos = solve_branch(u, &other, x, scope, extra, depth + 1)?;
                return Some(match (r_neg, r_pos) {
                    (Ok(mut a), Ok(b)) => {
                        a.extend(b);
                        pack_roots(a, x, scope, extra).map(|v| (v, SymbolicKind::Abs(plan)))
                    }
                    (Ok(a), Err(CalcError::NoSolution(_))) | (Err(CalcError::NoSolution(_)), Ok(a)) => {
                        pack_roots(a, x, scope, extra).map(|v| (v, SymbolicKind::Abs(plan)))
                    }
                    (Err(CalcError::NoSolution(_)), Err(CalcError::NoSolution(_))) => Err(CalcError::NoSolution(x.to_string())),
                    (Err(e), _) | (_, Err(e)) => Err(e),
                });
            }
            Expr::Neg(e) => {
                other = neg_expr(other);
                side = e;
            }
            Expr::Bin(op, a, b) => {
                let (ha, hb) = (has(a), has(b));
                if ha == hb {
                    return None;
                }
                let (a2, b2) = ((**a).clone(), (**b).clone());
                other = match (op, ha) {
                    (BinOp::Add, true) => bin(BinOp::Sub, other, b2),
                    (BinOp::Add, false) => bin(BinOp::Sub, other, a2),
                    (BinOp::Sub, true) => bin(BinOp::Add, other, b2),
                    (BinOp::Sub, false) => bin(BinOp::Sub, a2, other),
                    (BinOp::Mul, true) => bin(BinOp::Div, other, b2),
                    (BinOp::Mul, false) => bin(BinOp::Div, other, a2),
                    (BinOp::Div, true) if matches!(b2, Expr::Num(_)) => bin(BinOp::Mul, b2, other),
                    (BinOp::Div, true) => bin(BinOp::Mul, other, b2),
                    (BinOp::Div, false) => bin(BinOp::Div, a2, other),
                    _ => return None,
                };
                side = if ha { a } else { b };
            }
            _ => return None,
        }
        outer_steps.push((side.clone(), other.clone()));
    }
    None
}

fn solve_branch(
    lhs: &Expr,
    rhs: &Expr,
    x: &str,
    scope: &Scope,
    extra: &[DomainConstraint],
    depth: usize,
) -> Option<Result<Vec<Quantity>, CalcError>> {
    if let Some((form, _)) = isolate_steps(lhs, rhs, x) {
        return Some(eval(&form, scope, Names::Worksheet).and_then(|v| Ok(vec![v.expect_scalar()?])));
    }
    if let Some(res) = solve_abs(lhs, rhs, x, scope, extra, depth) {
        return Some(res.map(|(v, _)| v.quantities()));
    }
    solve_poly(lhs, rhs, x, scope, &[]).map(|res| res.map(|(v, _)| v.quantities()))
}

#[derive(Debug, Clone)]
struct Coeff {
    expr: Expr,
    qty: Quantity,
}

/// Polynomial `c[0] + c[1]·x + c[2]·x²` with optional terms (an absent term imposes no dimension).
#[derive(Debug, Clone, Default)]
struct Poly([Option<Coeff>; 3]);

impl Poly {
    fn degree(&self) -> usize {
        if self.0[2].is_some() {
            2
        } else if self.0[1].is_some() {
            1
        } else {
            0
        }
    }
}

fn neg_expr(e: Expr) -> Expr {
    match e {
        Expr::Neg(inner) => *inner,
        Expr::Num(0.0) => Expr::Num(0.0),
        other => Expr::Neg(Box::new(other)),
    }
}

fn add_expr(a: Expr, b: Expr) -> Expr {
    match (a, b) {
        (Expr::Num(0.0), r) => r,
        (l, Expr::Num(0.0)) => l,
        (l, Expr::Neg(r)) => bin(BinOp::Sub, l, *r),
        (l, r) => bin(BinOp::Add, l, r),
    }
}

fn mul_expr(a: Expr, b: Expr) -> Expr {
    match (a, b) {
        (Expr::Num(1.0), r) => r,
        (l, Expr::Num(1.0)) => l,
        (Expr::Neg(inner), r) if *inner == Expr::Num(1.0) => neg_expr(r),
        (l, Expr::Neg(inner)) if *inner == Expr::Num(1.0) => neg_expr(l),
        (l, r) => bin(BinOp::Mul, l, r),
    }
}

fn add_coeff(a: Option<Coeff>, b: Option<Coeff>) -> Result<Option<Coeff>, CalcError> {
    Ok(match (a, b) {
        (None, None) => None,
        (Some(c), None) | (None, Some(c)) => Some(c),
        (Some(ca), Some(cb)) => Some(Coeff { expr: add_expr(ca.expr, cb.expr), qty: ca.qty.try_add(cb.qty)? }),
    })
}

fn neg_coeff(c: Coeff) -> Coeff {
    Coeff { expr: neg_expr(c.expr), qty: c.qty.negate() }
}

fn sub_coeff(a: Option<Coeff>, b: Option<Coeff>) -> Result<Option<Coeff>, CalcError> {
    add_coeff(a, b.map(neg_coeff))
}

fn poly_sub(a: &Poly, b: &Poly) -> Result<Poly, CalcError> {
    Ok(Poly([sub_coeff(a.0[0].clone(), b.0[0].clone())?, sub_coeff(a.0[1].clone(), b.0[1].clone())?, sub_coeff(a.0[2].clone(), b.0[2].clone())?]))
}

fn poly_mul(a: &Poly, b: &Poly) -> Result<Option<Poly>, CalcError> {
    if a.degree() + b.degree() > 2 {
        return Ok(None);
    }
    let mut out = Poly::default();
    for (i, ca) in a.0.iter().enumerate() {
        let Some(ca) = ca else { continue };
        for (j, cb) in b.0.iter().enumerate() {
            let Some(cb) = cb else { continue };
            if i + j > 2 {
                return Ok(None);
            }
            let prod = Coeff { expr: mul_expr(ca.expr.clone(), cb.expr.clone()), qty: ca.qty.try_mul(cb.qty)? };
            out.0[i + j] = add_coeff(out.0[i + j].take(), Some(prod))?;
        }
    }
    Ok(Some(out))
}

/// Extracts polynomial coefficients `[C, B, A]` of `expr` in `x` up to degree 2.
fn poly_coeffs(expr: &Expr, x: &str, scope: &Scope, depth: usize) -> Result<Option<Poly>, CalcError> {
    if depth > MAX_DEPTH {
        return Ok(None);
    }
    if count(expr, x) == 0 {
        let q = eval_scalar(expr, scope, Names::Worksheet)?;
        return Ok(Some(Poly([Some(Coeff { expr: expr.clone(), qty: q }), None, None])));
    }
    match expr {
        Expr::Name(n) if n == x => Ok(Some(Poly([None, Some(Coeff { expr: Expr::Num(1.0), qty: Quantity::number(1.0) }), None]))),
        Expr::Neg(inner) => {
            let Some(p) = poly_coeffs(inner, x, scope, depth + 1)? else { return Ok(None) };
            Ok(Some(Poly(p.0.map(|c| c.map(neg_coeff)))))
        }
        Expr::Bin(op @ (BinOp::Add | BinOp::Sub), l, r) => {
            let (Some(pl), Some(pr)) = (poly_coeffs(l, x, scope, depth + 1)?, poly_coeffs(r, x, scope, depth + 1)?) else {
                return Ok(None);
            };
            if *op == BinOp::Add {
                Ok(Some(Poly([
                    add_coeff(pl.0[0].clone(), pr.0[0].clone())?,
                    add_coeff(pl.0[1].clone(), pr.0[1].clone())?,
                    add_coeff(pl.0[2].clone(), pr.0[2].clone())?,
                ])))
            } else {
                poly_sub(&pl, &pr).map(Some)
            }
        }
        Expr::Bin(BinOp::Mul, l, r) => {
            let (Some(pl), Some(pr)) = (poly_coeffs(l, x, scope, depth + 1)?, poly_coeffs(r, x, scope, depth + 1)?) else {
                return Ok(None);
            };
            poly_mul(&pl, &pr)
        }
        Expr::Bin(BinOp::Div, l, r) => {
            if count(r, x) > 0 {
                return Ok(None);
            }
            let qr = eval_scalar(r, scope, Names::Worksheet)?;
            if qr.value == 0.0 {
                return Err(CalcError::DivisionByZero);
            }
            let Some(pl) = poly_coeffs(l, x, scope, depth + 1)? else { return Ok(None) };
            let mut out = [None, None, None];
            for (slot, c) in out.iter_mut().zip(pl.0) {
                if let Some(c) = c {
                    let expr = if **r == Expr::Num(1.0) { c.expr } else { bin(BinOp::Div, c.expr, (**r).clone()) };
                    *slot = Some(Coeff { expr, qty: c.qty.try_div(qr)? });
                }
            }
            Ok(Some(Poly(out)))
        }
        Expr::Bin(BinOp::Pow, base, exp) => {
            if count(exp, x) > 0 {
                return Ok(None);
            }
            let qe = eval_scalar(exp, scope, Names::Worksheet)?;
            if !qe.dim.is_dimensionless() {
                return Err(CalcError::ExponentNotDimensionless { found: crate::units::dim_name(qe.dim) });
            }
            if qe.value == 0.0 {
                return Ok(Some(Poly([Some(Coeff { expr: Expr::Num(1.0), qty: Quantity::number(1.0) }), None, None])));
            }
            if qe.value == 1.0 {
                return poly_coeffs(base, x, scope, depth + 1);
            }
            if qe.value == 2.0 {
                let Some(pb) = poly_coeffs(base, x, scope, depth + 1)? else { return Ok(None) };
                return poly_mul(&pb, &pb);
            }
            Ok(None)
        }
        _ => Ok(None),
    }
}

fn solve_poly(lhs: &Expr, rhs: &Expr, x: &str, scope: &Scope, extra: &[DomainConstraint]) -> Option<Result<(Value, SymbolicKind), CalcError>> {
    let pl = match poly_coeffs(lhs, x, scope, 0) {
        Ok(Some(p)) => p,
        Ok(None) => return None,
        Err(e) => return Some(Err(e)),
    };
    let pr = match poly_coeffs(rhs, x, scope, 0) {
        Ok(Some(p)) => p,
        Ok(None) => return None,
        Err(e) => return Some(Err(e)),
    };
    let p_lr = match poly_sub(&pl, &pr) {
        Ok(p) => p,
        Err(e) => return Some(Err(e)),
    };
    let has_quad = p_lr.0[2].as_ref().is_some_and(|a| a.qty.value.abs() > 0.0);
    if !has_quad {
        return Some(solve_linear_poly(&pl, &pr, x, scope, extra));
    }
    let use_rl = p_lr.0[2].as_ref().is_some_and(|a| a.qty.value < 0.0);
    let p = if use_rl {
        match poly_sub(&pr, &pl) {
            Ok(p) => p,
            Err(e) => return Some(Err(e)),
        }
    } else {
        p_lr
    };
    Some(solve_quadratic_poly(p, x, scope, extra))
}

fn solve_linear_poly(pl: &Poly, pr: &Poly, x: &str, scope: &Scope, extra: &[DomainConstraint]) -> Result<(Value, SymbolicKind), CalcError> {
    let b_net = sub_coeff(pl.0[1].clone(), pr.0[1].clone())?;
    let c_rhs = sub_coeff(pr.0[0].clone(), pl.0[0].clone())?;
    let Some(b) = b_net.filter(|b| b.qty.value != 0.0) else {
        return Err(CalcError::NoSolution(x.to_string()));
    };
    let (q, form) = match c_rhs {
        Some(c) => {
            let q = c.qty.try_div(b.qty)?;
            let form = if b.expr == Expr::Num(1.0) { c.expr } else { bin(BinOp::Div, c.expr, b.expr.clone()) };
            (q, form)
        }
        None => (Quantity::number(0.0), Expr::Num(0.0)),
    };
    let val = pack_roots(vec![q], x, scope, extra)?;
    let x_expr = Expr::Name(x.to_string());
    let mut steps = Vec::new();
    if b.expr != Expr::Num(1.0) {
        let rhs_step = match &form {
            Expr::Bin(BinOp::Div, top, _) => (**top).clone(),
            other => other.clone(),
        };
        steps.push((mul_expr(b.expr, x_expr.clone()), rhs_step));
    }
    steps.push((x_expr, form.clone()));
    Ok((val, SymbolicKind::Isolated(form, steps)))
}

fn solve_quadratic_poly(p: Poly, x: &str, scope: &Scope, extra: &[DomainConstraint]) -> Result<(Value, SymbolicKind), CalcError> {
    let [c_opt, b_opt, Some(a)] = p.0 else {
        return Err(CalcError::NoSolution(x.to_string()));
    };
    let dx = match (&b_opt, &c_opt) {
        (Some(b), Some(c)) => {
            let b2 = b.qty.try_mul(b.qty)?;
            let four_ac = Quantity::number(4.0).try_mul(a.qty)?.try_mul(c.qty)?;
            b2.same_dim(four_ac, "subtract")?;
            b.qty.dim.try_div(a.qty.dim)?
        }
        (Some(b), None) => b.qty.dim.try_div(a.qty.dim)?,
        (None, Some(c)) => c.qty.dim.try_div(a.qty.dim)?.try_root(2)?,
        (None, None) => Dim::NONE,
    };
    let a_val = a.qty.value;
    let b_val = b_opt.as_ref().map_or(0.0, |b| b.qty.value);
    let c_val = c_opt.as_ref().map_or(0.0, |c| c.qty.value);
    let b2_mag = finite(b_val * b_val)?;
    let four_ac_mag = finite(4.0 * a_val * c_val)?;
    let disc = finite(b2_mag - four_ac_mag)?;
    let scale = b2_mag.max(four_ac_mag.abs());
    if (scale == 0.0 && disc < 0.0) || (scale > 0.0 && disc < -1e-12 * scale) {
        return Err(CalcError::NoSolution(x.to_string()));
    }
    let roots = if scale == 0.0 || disc.max(0.0) <= 1e-24 * scale {
        vec![Quantity::new(finite(-b_val / (2.0 * a_val))?, dx)]
    } else {
        let s = disc.max(0.0).sqrt();
        let q = if b_val >= 0.0 { -0.5 * (b_val + s) } else { -0.5 * (b_val - s) };
        let r1 = finite(q / a_val)?;
        let r2 = if q != 0.0 { finite(c_val / q)? } else { 0.0 };
        vec![Quantity::new(r1, dx), Quantity::new(r2, dx)]
    };
    let standard_lhs = build_standard_quadratic_lhs(x, &a.expr, b_opt.as_ref().map(|b| &b.expr), c_opt.as_ref().map(|c| &c.expr));
    let plan = QuadraticPlan { standard_lhs, a: a.expr, b: b_opt.map_or(Expr::Num(0.0), |b| b.expr), c: c_opt.map_or(Expr::Num(0.0), |c| c.expr) };
    let value = pack_roots(roots, x, scope, extra)?;
    Ok((value, SymbolicKind::Quadratic(plan)))
}

fn build_standard_quadratic_lhs(x: &str, a: &Expr, b: Option<&Expr>, c: Option<&Expr>) -> Expr {
    let x_name = Expr::Name(x.to_string());
    let x2 = bin(BinOp::Pow, x_name.clone(), Expr::Num(2.0));
    let mut acc = mul_expr(a.clone(), x2);
    if let Some(b) = b {
        acc = add_expr(acc, mul_expr(b.clone(), x_name));
    }
    if let Some(c) = c {
        acc = add_expr(acc, c.clone());
    }
    acc
}

/// Solves `lhs = rhs` for `x` numerically: infers candidate dimensions algebraically (including
/// generic powers `x^k` and inverse dimensions), then tries known dimensions, secant steps and
/// logarithmic grid sign-change bisection. Returns the first root satisfying active constraints.
pub(crate) fn numeric(lhs: &Expr, rhs: &Expr, x: &str, scope: &Scope, extra: &[DomainConstraint]) -> Result<Quantity, CalcError> {
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
    let eval_dim = |e: &Expr| -> Option<Dim> { ctx_eval(e, None).ok().map(|v| v.dim()) };
    let eval_plain =
        |e: &Expr| -> Option<f64> { ctx_eval(e, None).ok().and_then(|v| v.scalar()).filter(|q| q.dim.is_dimensionless()).map(|q| q.value) };
    let mut dims = infer_unknown_dims(lhs, rhs, x, &eval_dim, &eval_plain);
    dims.push(Dim::NONE);
    for name in known.iter().filter(|n| *n != x) {
        if let Some(d) = eval_dim(&Expr::Name(name.clone())) {
            dims.push(d);
            if let Ok(inv) = Dim::NONE.try_div(d) {
                dims.push(inv);
            }
        }
    }
    for side in [lhs, rhs] {
        if count(side, x) == 0
            && let Some(d) = eval_dim(side)
        {
            dims.push(d);
            if let Ok(inv) = Dim::NONE.try_div(d) {
                dims.push(inv);
            }
            for k in [2i64, 3, 4, -2, -3, -4] {
                if let Ok(r) = d.try_root(k) {
                    dims.push(r);
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
        // number) may vanish elsewhere. Test `1.234` too in case `1.0` hits a pole `(x - 1)`.
        let probe = match f(1.0) {
            Err(CalcError::DivisionByZero) => f(1.234),
            other => other,
        };
        if let Err(
            e @ (CalcError::DimensionMismatch { .. }
            | CalcError::FractionalDimension { .. }
            | CalcError::NeedsDimensionless { .. }
            | CalcError::ExponentNotDimensionless { .. }),
        ) = probe
        {
            last_err = e;
            continue;
        }
        let candidates = roots_of(&f);
        for v in candidates {
            let q = Quantity::new(v, dim);
            if check_constraints(q, x, scope, extra)? {
                return Ok(q);
            }
        }
        last_err = CalcError::NoSolution(x.to_string());
    }
    if spent.get() > crate::MAX_STEPS {
        return Err(CalcError::TooMuchWork);
    }
    Err(last_err)
}

/// Algebraically infers candidate dimensions of `x` from `lhs = rhs` by propagating known
/// dimensions down products, quotients, sums, inverse terms and powers `x^k`.
fn infer_unknown_dims(
    lhs: &Expr,
    rhs: &Expr,
    x: &str,
    eval_dim: &dyn Fn(&Expr) -> Option<Dim>,
    eval_plain: &dyn Fn(&Expr) -> Option<f64>,
) -> Vec<Dim> {
    let mut out = Vec::new();
    let push = |out: &mut Vec<Dim>, d: Dim| {
        if !out.contains(&d) {
            out.push(d);
        }
    };
    #[allow(clippy::too_many_arguments)]
    fn walk(
        e: &Expr,
        target: Option<Dim>,
        x: &str,
        eval_dim: &dyn Fn(&Expr) -> Option<Dim>,
        eval_plain: &dyn Fn(&Expr) -> Option<f64>,
        out: &mut Vec<Dim>,
        push: &dyn Fn(&mut Vec<Dim>, Dim),
        depth: usize,
    ) {
        if depth > 32 || count(e, x) == 0 {
            return;
        }
        match e {
            Expr::Name(n) if n == x => {
                if let Some(t) = target {
                    push(out, t);
                }
            }
            Expr::Neg(a) => walk(a, target, x, eval_dim, eval_plain, out, push, depth + 1),
            Expr::Bin(BinOp::Add | BinOp::Sub, a, b) => {
                let (ca, cb) = (count(a, x), count(b, x));
                let ta = target.or_else(|| (cb == 0).then(|| eval_dim(b)).flatten());
                let tb = target.or_else(|| (ca == 0).then(|| eval_dim(a)).flatten());
                walk(a, ta, x, eval_dim, eval_plain, out, push, depth + 1);
                walk(b, tb, x, eval_dim, eval_plain, out, push, depth + 1);
            }
            Expr::Bin(BinOp::Mul, a, b) => {
                let (ca, cb) = (count(a, x), count(b, x));
                if ca == 0
                    && let (Some(t), Some(da)) = (target, eval_dim(a))
                    && let Ok(tb) = t.try_div(da)
                {
                    walk(b, Some(tb), x, eval_dim, eval_plain, out, push, depth + 1);
                } else if cb == 0
                    && let (Some(t), Some(db)) = (target, eval_dim(b))
                    && let Ok(ta) = t.try_div(db)
                {
                    walk(a, Some(ta), x, eval_dim, eval_plain, out, push, depth + 1);
                } else {
                    walk(a, None, x, eval_dim, eval_plain, out, push, depth + 1);
                    walk(b, None, x, eval_dim, eval_plain, out, push, depth + 1);
                }
            }
            Expr::Bin(BinOp::Div, a, b) => {
                let (ca, cb) = (count(a, x), count(b, x));
                if cb == 0
                    && let (Some(t), Some(db)) = (target, eval_dim(b))
                    && let Ok(ta) = t.try_mul(db)
                {
                    walk(a, Some(ta), x, eval_dim, eval_plain, out, push, depth + 1);
                } else if ca == 0
                    && let (Some(t), Some(da)) = (target, eval_dim(a))
                    && let Ok(tb) = da.try_div(t)
                {
                    walk(b, Some(tb), x, eval_dim, eval_plain, out, push, depth + 1);
                } else {
                    walk(a, None, x, eval_dim, eval_plain, out, push, depth + 1);
                    walk(b, None, x, eval_dim, eval_plain, out, push, depth + 1);
                }
            }
            Expr::Bin(BinOp::Pow, base, exp) => {
                if count(exp, x) == 0
                    && let (Some(t), Some(p)) = (target, eval_plain(exp))
                    && p != 0.0
                {
                    let tb = if p.fract() == 0.0 && p.abs() <= 127.0 { t.try_root(p as i64).ok() } else { t.try_pow(1.0 / p).ok() };
                    if let Some(tb) = tb {
                        walk(base, Some(tb), x, eval_dim, eval_plain, out, push, depth + 1);
                        return;
                    }
                }
                walk(base, None, x, eval_dim, eval_plain, out, push, depth + 1);
                walk(exp, Some(Dim::NONE), x, eval_dim, eval_plain, out, push, depth + 1);
            }
            Expr::Call(Func::Sqrt, args) => {
                if let ([a], Some(t)) = (args.as_slice(), target)
                    && let Ok(ta) = t.try_pow(2.0)
                {
                    walk(a, Some(ta), x, eval_dim, eval_plain, out, push, depth + 1);
                }
            }
            Expr::Call(Func::Cbrt, args) => {
                if let ([a], Some(t)) = (args.as_slice(), target)
                    && let Ok(ta) = t.try_pow(3.0)
                {
                    walk(a, Some(ta), x, eval_dim, eval_plain, out, push, depth + 1);
                }
            }
            Expr::Call(Func::Root, args) => {
                if let ([a, n], Some(t)) = (args.as_slice(), target)
                    && count(n, x) == 0
                    && let Some(p) = eval_plain(n)
                    && let Ok(ta) = t.try_pow(p)
                {
                    walk(a, Some(ta), x, eval_dim, eval_plain, out, push, depth + 1);
                }
            }
            Expr::Call(Func::Abs, args) => {
                if let [a] = args.as_slice() {
                    walk(a, target, x, eval_dim, eval_plain, out, push, depth + 1);
                }
            }
            Expr::Call(_, args) => {
                for a in args {
                    walk(a, None, x, eval_dim, eval_plain, out, push, depth + 1);
                }
            }
            _ => {}
        }
    }
    let (cl, cr) = (count(lhs, x), count(rhs, x));
    let tl = (cr == 0).then(|| eval_dim(rhs)).flatten();
    let tr = (cl == 0).then(|| eval_dim(lhs)).flatten();
    walk(lhs, tl, x, eval_dim, eval_plain, &mut out, &push, 0);
    walk(rhs, tr, x, eval_dim, eval_plain, &mut out, &push, 0);
    out
}

/// Grid points per decade searched for sign changes, from 10⁻¹² to 10¹² (both signs).
const PER_DECADE: i32 = 8;

/// Roots of `f` ordered by preference: non-negative roots from smallest to largest, followed by
/// negative roots from closest to zero outwards.
fn roots_of(f: &dyn Fn(f64) -> Result<f64, CalcError>) -> Vec<f64> {
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
    let mut non_neg: Vec<f64> = roots.iter().copied().filter(|r| *r >= 0.0).collect();
    non_neg.sort_by(f64::total_cmp);
    let mut neg: Vec<f64> = roots.into_iter().filter(|r| *r < 0.0).collect();
    neg.sort_by(|a, b| b.total_cmp(a));
    non_neg.extend(neg);
    non_neg
}
