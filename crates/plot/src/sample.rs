//! Adaptive sampling, discontinuity/pole splitting and viewport clipping.

use strix_calc::{CalcError, Options, Quantity, Value, Worksheet, unit};

use crate::MAX_SAMPLES;

/// Evaluates scalar expressions `f(var)` against a [`Worksheet`], handling unit scaling or
/// coherent-SI fallback when dimensioned parameters mix with plain plot coordinates.
pub(crate) struct ScalarEval {
    rich: Worksheet,
    plain: Worksheet,
    x_factor: Quantity,
    y_scale: Option<(f64, strix_calc::Dim)>,
    budget_left: usize,
    pub(crate) last_hard_error: Option<String>,
    pub(crate) last_domain_error: Option<String>,
}

impl ScalarEval {
    pub(crate) fn new(ws: Option<&Worksheet>, x_unit: Option<&str>, y_unit: Option<&str>) -> Self {
        let opts = ws.map_or_else(Options::default, |w| *w.options());
        let rich = ws.cloned().unwrap_or_else(|| Worksheet::new(opts));
        let mut plain = Worksheet::new(opts);
        if let Some(w) = ws {
            for (name, val) in w.scope().iter() {
                match val {
                    Value::Scalar(q) => {
                        plain.define(name, Value::Scalar(Quantity::number(q.value)));
                    }
                    Value::Vector(v) => {
                        let nums: Vec<Quantity> = v.iter().map(|q| Quantity::number(q.value)).collect();
                        if let Ok(vec) = strix_calc::Vector::from_quantities(&nums) {
                            plain.define(name, Value::Vector(vec));
                        }
                    }
                }
            }
            for (fname, params) in w.scope().functions() {
                // Keep user functions reachable through `rich`; `plain` handles scalar SI fallback.
                let _ = (fname, params);
            }
        }
        if plain.scope().get("g").is_none() {
            plain.define("g", Value::Scalar(Quantity::number(9.80665)));
        }
        let x_factor = x_unit.and_then(|u| unit_quantity(&rich, u)).unwrap_or_else(|| Quantity::number(1.0));
        let y_scale = y_unit.and_then(|u| unit_quantity(&rich, u)).filter(|q| q.value.abs() > 1e-30).map(|q| (q.value, q.dim));
        ScalarEval { rich, plain, x_factor, y_scale, budget_left: MAX_SAMPLES.saturating_mul(4), last_hard_error: None, last_domain_error: None }
    }

    pub(crate) fn eval_at(&mut self, var: &str, expr: &str, x: f64) -> Option<f64> {
        if self.budget_left == 0 || !x.is_finite() {
            return None;
        }
        self.budget_left = self.budget_left.saturating_sub(1);

        let qx = Quantity::new(x * self.x_factor.value, self.x_factor.dim);
        self.rich.define(var, Value::Scalar(qx));
        // Reset per-sheet step counter on our local sandbox clone so a 500-point plot never
        // exhausts a shared document budget while `self.budget_left` bounds total work.
        self.rich.scope().set_spent(0);
        match self.rich.eval_expr(expr) {
            Ok(Value::Scalar(q)) => self.scale_output(q),
            Ok(Value::Vector(_)) => {
                self.record_error("plot expression evaluated to a vector instead of a scalar");
                None
            }
            Err(
                CalcError::DimensionMismatch { .. }
                | CalcError::ExponentNotDimensionless { .. }
                | CalcError::FractionalDimension { .. }
                | CalcError::WrongUnit { .. },
            ) if self.x_factor.dim.is_dimensionless() => {
                self.plain.define(var, Value::Scalar(Quantity::number(x)));
                self.plain.scope().set_spent(0);
                match self.plain.eval_expr(expr) {
                    Ok(Value::Scalar(q)) if q.value.is_finite() => Some(q.value),
                    Err(e) => {
                        if self.last_domain_error.is_none() {
                            self.last_domain_error = Some(e.to_string());
                        }
                        None
                    }
                    _ => None,
                }
            }
            Err(e @ (CalcError::DivisionByZero | CalcError::NotReal | CalcError::Overflow | CalcError::Domain { .. })) => {
                if self.last_domain_error.is_none() {
                    self.last_domain_error = Some(e.to_string());
                }
                None
            }
            Err(other) => {
                self.record_error(&other.to_string());
                None
            }
        }
    }

    fn scale_output(&mut self, q: Quantity) -> Option<f64> {
        if !q.value.is_finite() {
            return None;
        }
        if let Some((factor, dim)) = self.y_scale {
            if !q.dim.is_dimensionless() && q.dim != dim {
                self.record_error("expression unit does not match plot yUnit");
                return None;
            }
            let v = q.value / factor;
            v.is_finite().then_some(v)
        } else {
            Some(q.value)
        }
    }

    fn record_error(&mut self, msg: &str) {
        if self.last_hard_error.is_none() {
            self.last_hard_error = Some(msg.to_string());
        }
    }
}

pub(crate) fn unit_quantity(ws: &Worksheet, u: &str) -> Option<Quantity> {
    let trimmed = u.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(q) = unit(trimmed) {
        return Some(q);
    }
    match ws.eval_expr(&format!("1 {trimmed}")) {
        Ok(Value::Scalar(q)) if q.value.is_finite() && q.value.abs() > 0.0 => Some(q),
        _ => None,
    }
}

/// Raw continuous branch of sampled `(x, y)` pairs before viewport clipping.
pub(crate) type Branch = Vec<(f64, f64)>;

/// Adaptively samples `y = f(x)` over `[xa, xb]`, splitting across undefined points, vertical
/// asymptotes (`1/x`, `tan(x)`) and step discontinuities (`sign(x)`, `floor(x)`).
pub(crate) fn sample_function(eval: &mut ScalarEval, var: &str, expr: &str, xa: f64, xb: f64, y_min: f64, y_max: f64) -> Vec<Branch> {
    let base_n: usize = 192;
    let y_span = (y_max - y_min).abs().max(1e-6);
    let mut grid: Vec<(f64, Option<f64>)> = Vec::with_capacity(base_n + 1);
    for i in 0..=base_n {
        let t = i as f64 / base_n as f64;
        let x = xa + (xb - xa) * t;
        let y = eval.eval_at(var, expr, x);
        grid.push((x, y));
    }

    let mut branches: Vec<Branch> = Vec::new();
    let mut current: Branch = Vec::new();

    for i in 0..base_n {
        let (Some(&(x0, y0_opt)), Some(&(x1, y1_opt))) = (grid.get(i), grid.get(i + 1)) else {
            continue;
        };
        match (y0_opt, y1_opt) {
            (Some(y0), Some(y1)) => {
                if current.is_empty() {
                    current.push((x0, y0));
                }
                refine_segment(eval, var, expr, (x0, y0), (x1, y1), y_min, y_max, y_span, 0, &mut branches, &mut current);
            }
            (Some(y0), None) => {
                if current.is_empty() {
                    current.push((x0, y0));
                }
                probe_boundary_right(eval, var, expr, x0, x1, &mut current);
                flush_branch(&mut branches, &mut current);
            }
            (None, Some(y1)) => {
                flush_branch(&mut branches, &mut current);
                probe_boundary_left(eval, var, expr, x0, x1, &mut current);
                current.push((x1, y1));
            }
            (None, None) => {
                flush_branch(&mut branches, &mut current);
            }
        }
    }
    flush_branch(&mut branches, &mut current);
    branches
}

#[allow(clippy::too_many_arguments)]
fn refine_segment(
    eval: &mut ScalarEval,
    var: &str,
    expr: &str,
    p0: (f64, f64),
    p1: (f64, f64),
    y_min: f64,
    y_max: f64,
    y_span: f64,
    depth: u8,
    branches: &mut Vec<Branch>,
    current: &mut Branch,
) {
    let (x0, y0) = p0;
    let (x1, y1) = p1;
    let dy = (y1 - y0).abs();

    // Both points way outside the viewport on the same side -> no need to subdivide.
    let margin = y_span * 2.0;
    if (y0 > y_max + margin && y1 > y_max + margin) || (y0 < y_min - margin && y1 < y_min - margin) {
        current.push(p1);
        return;
    }

    let xm = 0.5 * (x0 + x1);
    let y_bounds = (y_min, y_max, y_span);
    let need_mid = depth < 5 && (dy > 0.04 * y_span || depth == 0 && dy > 0.01 * y_span);
    if !need_mid {
        if is_jump_or_pole(eval, var, expr, p0, p1, y_bounds) {
            flush_branch(branches, current);
            current.push(p1);
        } else {
            current.push(p1);
        }
        return;
    }

    let Some(ym) = eval.eval_at(var, expr, xm) else {
        // Undefined midpoint -> split across domain hole or pole!
        flush_branch(branches, current);
        current.push(p1);
        return;
    };

    if depth >= 4 && is_discontinuous_triple(p0, (xm, ym), p1, y_bounds) {
        flush_branch(branches, current);
        current.push(p1);
        return;
    }

    let lin_mid = 0.5 * (y0 + y1);
    let bend = (ym - lin_mid).abs();
    if bend <= 0.002 * y_span && dy <= 0.35 * y_span {
        current.push((xm, ym));
        current.push(p1);
        return;
    }

    refine_segment(eval, var, expr, p0, (xm, ym), y_min, y_max, y_span, depth + 1, branches, current);
    refine_segment(eval, var, expr, (xm, ym), p1, y_min, y_max, y_span, depth + 1, branches, current);
}

fn is_discontinuous_triple(p0: (f64, f64), pm: (f64, f64), p1: (f64, f64), y_bounds: (f64, f64, f64)) -> bool {
    let (y_min, y_max, y_span) = y_bounds;
    let (_, y0) = p0;
    let (_, ym) = pm;
    let (_, y1) = p1;
    let jump = (y1 - y0).abs();
    if jump < 0.5 * y_span {
        return false;
    }
    // Opposite-sign blowup across a vertical asymptote (e.g. `1/x`, `tan(x)`).
    if (y0 > y_max && y1 < y_min) || (y0 < y_min && y1 > y_max) {
        return true;
    }
    // Non-monotonic spike or step discontinuity where one half-interval still carries almost the
    // entire jump after 4 bisections (`dx / 16`).
    let d0 = (ym - y0).abs();
    let d1 = (y1 - ym).abs();
    if ym > y0.max(y1) + y_span || ym < y0.min(y1) - y_span {
        return true;
    }
    d0.max(d1) > 0.85 * jump && jump > 0.6 * y_span
}

fn is_jump_or_pole(eval: &mut ScalarEval, var: &str, expr: &str, p0: (f64, f64), p1: (f64, f64), y_bounds: (f64, f64, f64)) -> bool {
    let (y_min, y_max, y_span) = y_bounds;
    let (x0, y0) = p0;
    let (x1, y1) = p1;
    if (y1 - y0).abs() < 0.6 * y_span {
        return false;
    }
    if (y0 > y_max && y1 < y_min) || (y0 < y_min && y1 > y_max) {
        return true;
    }
    let xm = 0.5 * (x0 + x1);
    let Some(ym) = eval.eval_at(var, expr, xm) else {
        return true;
    };
    is_discontinuous_triple(p0, (xm, ym), p1, y_bounds)
}

/// Binary-searches up to 8 steps towards a domain boundary (such as `sqrt(x)` near `x = 0`) so the
/// curve reaches the endpoint cleanly.
fn probe_boundary_right(eval: &mut ScalarEval, var: &str, expr: &str, mut ok_x: f64, mut bad_x: f64, current: &mut Branch) {
    let mut best: Option<(f64, f64)> = None;
    for _ in 0..8 {
        let mid = 0.5 * (ok_x + bad_x);
        if let Some(y) = eval.eval_at(var, expr, mid) {
            ok_x = mid;
            best = Some((mid, y));
        } else {
            bad_x = mid;
        }
    }
    if let Some(pt) = best {
        current.push(pt);
    }
}

fn probe_boundary_left(eval: &mut ScalarEval, var: &str, expr: &str, mut bad_x: f64, mut ok_x: f64, current: &mut Branch) {
    let mut best: Option<(f64, f64)> = None;
    for _ in 0..8 {
        let mid = 0.5 * (bad_x + ok_x);
        if let Some(y) = eval.eval_at(var, expr, mid) {
            ok_x = mid;
            best = Some((mid, y));
        } else {
            bad_x = mid;
        }
    }
    if let Some(pt) = best {
        current.push(pt);
    }
}

fn flush_branch(branches: &mut Vec<Branch>, current: &mut Branch) {
    if current.len() >= 2 {
        branches.push(std::mem::take(current));
    } else {
        current.clear();
    }
}

/// Samples a parametric curve `(x(t), y(t))` over `[t_min, t_max]`.
pub(crate) fn sample_parametric(eval: &mut ScalarEval, param: &str, exprs: (&str, &str), t_range: (f64, f64), spans: (f64, f64)) -> Vec<Branch> {
    let (x_expr, y_expr) = exprs;
    let (t_min, t_max) = t_range;
    let (x_span, y_span) = spans;
    let steps: usize = 256;
    let mut branches: Vec<Branch> = Vec::new();
    let mut current: Branch = Vec::new();
    for i in 0..=steps {
        let u = i as f64 / steps as f64;
        let t = t_min + (t_max - t_min) * u;
        let xy = eval.eval_at(param, x_expr, t).zip(eval.eval_at(param, y_expr, t));
        match xy {
            Some((x, y)) => {
                if let Some(&(px, py)) = current.last() {
                    let jump_x = (x - px).abs() > 1.5 * x_span.max(1e-6);
                    let jump_y = (y - py).abs() > 1.5 * y_span.max(1e-6);
                    if jump_x || jump_y {
                        flush_branch(&mut branches, &mut current);
                    }
                }
                current.push((x, y));
            }
            None => flush_branch(&mut branches, &mut current),
        }
    }
    flush_branch(&mut branches, &mut current);
    branches
}

/// Clips a continuous branch to the rectangle `[x_min, x_max] × [y_min, y_max]` via Liang–Barsky,
/// producing zero or more clean interior polyline pieces.
pub(crate) fn clip_branch(branch: &[(f64, f64)], x_min: f64, x_max: f64, y_min: f64, y_max: f64) -> Vec<Branch> {
    let mut out: Vec<Branch> = Vec::new();
    let mut run: Branch = Vec::new();
    for w in branch.windows(2) {
        let (Some(&a), Some(&b)) = (w.first(), w.get(1)) else {
            continue;
        };
        match clip_segment(a, b, x_min, x_max, y_min, y_max) {
            Some((ca, cb)) => {
                let continues = run.last().is_some_and(|&(lx, ly)| (lx - ca.0).abs() < 1e-9 && (ly - ca.1).abs() < 1e-9);
                if !continues {
                    flush_branch(&mut out, &mut run);
                    run.push(ca);
                }
                run.push(cb);
            }
            None => flush_branch(&mut out, &mut run),
        }
    }
    flush_branch(&mut out, &mut run);
    out
}

/// Liang–Barsky segment clipper against `[x_min, x_max] × [y_min, y_max]`.
pub(crate) fn clip_segment(a: (f64, f64), b: (f64, f64), x_min: f64, x_max: f64, y_min: f64, y_max: f64) -> Option<((f64, f64), (f64, f64))> {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let mut u1 = 0.0f64;
    let mut u2 = 1.0f64;
    let checks = [(-dx, a.0 - x_min), (dx, x_max - a.0), (-dy, a.1 - y_min), (dy, y_max - a.1)];
    for (p, q) in checks {
        if p.abs() <= 1e-15 {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                if r > u2 {
                    return None;
                }
                u1 = u1.max(r);
            } else {
                if r < u1 {
                    return None;
                }
                u2 = u2.min(r);
            }
        }
    }
    Some(((a.0 + u1 * dx, a.1 + u1 * dy), (a.0 + u2 * dx, a.1 + u2 * dy)))
}
