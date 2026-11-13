//! Notable mathematical features (roots, extrema, tangents, integrals) and experimental data
//! extraction + least-squares regression.

use serde::{Deserialize, Serialize};
use strix_calc::{Value, Worksheet};

use crate::MAX_DATA_POINTS;
use crate::model::{DataPoint, RegressionKind};
use crate::sample::{Branch, ScalarEval, clip_segment, unit_quantity};

/// Classification of a notable mathematical point on a function curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NotableKind {
    Root,
    Minimum,
    Maximum,
    Tangent,
}

/// A notable point `(x, y)` discovered on series `series_index`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotablePoint {
    pub series_index: usize,
    pub kind: NotableKind,
    pub x: f64,
    pub y: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slope: Option<f64>,
}

/// Numerical definite integral computed on `[a, b]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegralInfo {
    pub series_index: usize,
    pub a: f64,
    pub b: f64,
    pub value: f64,
}

/// Least-squares regression summary for a [`SeriesKind::Data`](crate::SeriesKind::Data) series.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegressionInfo {
    pub series_index: usize,
    pub kind: RegressionKind,
    /// `[c0, c1]` for `c0 + c1·x` or `[c0, c1, c2]` for `c0 + c1·x + c2·x²`.
    pub coeffs: Vec<f64>,
    pub r2: f64,
    pub equation: String,
}

const MAX_NOTABLES_PER_SERIES: usize = 32;

/// Finds zeros `f(x) = 0` along continuous branches via 40-step bisection.
pub(crate) fn find_roots(eval: &mut ScalarEval, series_index: usize, var: &str, expr: &str, branches: &[Branch], y_span: f64) -> Vec<NotablePoint> {
    let mut out: Vec<NotablePoint> = Vec::new();
    let tol_y = 1e-5 * y_span.max(1.0);
    for br in branches {
        for w in br.windows(2) {
            if out.len() >= MAX_NOTABLES_PER_SERIES {
                return out;
            }
            let (Some(&(mut a, mut fa)), Some(&(mut b, fb))) = (w.first(), w.get(1)) else {
                continue;
            };
            if fa.abs() <= 1e-12 {
                push_unique_notable(&mut out, series_index, NotableKind::Root, a, 0.0, None);
                continue;
            }
            if fb.abs() <= 1e-12 {
                push_unique_notable(&mut out, series_index, NotableKind::Root, b, 0.0, None);
                continue;
            }
            if (fa > 0.0) == (fb > 0.0) {
                continue;
            }
            for _ in 0..40 {
                let mid = 0.5 * (a + b);
                let Some(fm) = eval.eval_at(var, expr, mid) else {
                    break;
                };
                if fm.abs() <= 1e-13 {
                    a = mid;
                    b = mid;
                    break;
                }
                if (fa > 0.0) != (fm > 0.0) {
                    b = mid;
                } else {
                    a = mid;
                    fa = fm;
                }
            }
            let root = 0.5 * (a + b);
            if let Some(fr) = eval.eval_at(var, expr, root)
                && fr.abs() <= tol_y
            {
                push_unique_notable(&mut out, series_index, NotableKind::Root, root, 0.0, None);
            }
        }
        // Also detect tangential roots (`x^2 = 0`) where `|f(x_i)|` touches zero without crossing.
        for triple in br.windows(3) {
            if out.len() >= MAX_NOTABLES_PER_SERIES {
                return out;
            }
            let (Some(&(x0, y0)), Some(&(_, y1)), Some(&(x2, y2))) = (triple.first(), triple.get(1), triple.get(2)) else {
                continue;
            };
            if y1.abs() < y0.abs() && y1.abs() <= y2.abs() && y1.abs() <= tol_y {
                let (rx, ry) = golden_section(eval, var, expr, x0, x2, true);
                if ry.abs() <= tol_y * 0.1 {
                    push_unique_notable(&mut out, series_index, NotableKind::Root, rx, 0.0, None);
                }
            }
        }
    }
    out
}

/// Finds strict local minima and maxima along continuous branches via 32-step golden-section search.
pub(crate) fn find_extrema(eval: &mut ScalarEval, series_index: usize, var: &str, expr: &str, branches: &[Branch]) -> Vec<NotablePoint> {
    let mut out: Vec<NotablePoint> = Vec::new();
    for br in branches {
        for triple in br.windows(3) {
            if out.len() >= MAX_NOTABLES_PER_SERIES {
                return out;
            }
            let (Some(&(x0, y0)), Some(&(_, y1)), Some(&(x2, y2))) = (triple.first(), triple.get(1), triple.get(2)) else {
                continue;
            };
            if y1 < y0 && y1 <= y2 && (y0 - y1).max(y2 - y1) > 1e-12 {
                let (xm, ym) = golden_section_signed(eval, var, expr, x0, x2, true);
                push_unique_notable(&mut out, series_index, NotableKind::Minimum, xm, ym, None);
            } else if y1 > y0 && y1 >= y2 && (y1 - y0).max(y1 - y2) > 1e-12 {
                let (xm, ym) = golden_section_signed(eval, var, expr, x0, x2, false);
                push_unique_notable(&mut out, series_index, NotableKind::Maximum, xm, ym, None);
            }
        }
    }
    out
}

fn golden_section(eval: &mut ScalarEval, var: &str, expr: &str, mut a: f64, mut b: f64, abs_val: bool) -> (f64, f64) {
    let phi = 0.5 * (5.0f64.sqrt() - 1.0);
    let mut c = b - phi * (b - a);
    let mut d = a + phi * (b - a);
    let score = |e: &mut ScalarEval, x: f64| e.eval_at(var, expr, x).map(|y| if abs_val { y.abs() } else { y }).unwrap_or(f64::INFINITY);
    let mut fc = score(eval, c);
    let mut fd = score(eval, d);
    for _ in 0..32 {
        if fc < fd {
            b = d;
            d = c;
            fd = fc;
            c = b - phi * (b - a);
            fc = score(eval, c);
        } else {
            a = c;
            c = d;
            fc = fd;
            d = a + phi * (b - a);
            fd = score(eval, d);
        }
    }
    let xm = 0.5 * (a + b);
    let ym = eval.eval_at(var, expr, xm).unwrap_or(0.0);
    (xm, ym)
}

fn golden_section_signed(eval: &mut ScalarEval, var: &str, expr: &str, mut a: f64, mut b: f64, minimize: bool) -> (f64, f64) {
    let phi = 0.5 * (5.0f64.sqrt() - 1.0);
    let mut c = b - phi * (b - a);
    let mut d = a + phi * (b - a);
    let score = |e: &mut ScalarEval, x: f64| e.eval_at(var, expr, x).map(|y| if minimize { y } else { -y }).unwrap_or(f64::INFINITY);
    let mut fc = score(eval, c);
    let mut fd = score(eval, d);
    for _ in 0..32 {
        if fc < fd {
            b = d;
            d = c;
            fd = fc;
            c = b - phi * (b - a);
            fc = score(eval, c);
        } else {
            a = c;
            c = d;
            fc = fd;
            d = a + phi * (b - a);
            fd = score(eval, d);
        }
    }
    let xm = 0.5 * (a + b);
    let ym = eval.eval_at(var, expr, xm).unwrap_or(0.0);
    (xm, ym)
}

fn push_unique_notable(out: &mut Vec<NotablePoint>, series_index: usize, kind: NotableKind, x: f64, y: f64, slope: Option<f64>) {
    if !x.is_finite() || !y.is_finite() {
        return;
    }
    let dup = out.iter().any(|p| p.kind == kind && (p.x - x).abs() < 1e-4);
    if !dup {
        out.push(NotablePoint { series_index, kind, x: clean_zero(x), y: clean_zero(y), slope });
    }
}

/// Computes the tangent line at `x0` clipped to `[x_min, x_max] × [y_min, y_max]` and its point.
#[allow(clippy::too_many_arguments)]
pub(crate) fn compute_tangent(
    eval: &mut ScalarEval,
    series_index: usize,
    var: &str,
    expr: &str,
    x0: f64,
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
) -> Option<(NotablePoint, Option<((f64, f64), (f64, f64))>)> {
    let y0 = eval.eval_at(var, expr, x0)?;
    let h = 1e-5 * (x_max - x_min).abs().max(1.0);
    let yp = eval.eval_at(var, expr, x0 + h)?;
    let ym = eval.eval_at(var, expr, x0 - h)?;
    let slope = (yp - ym) / (2.0 * h);
    if !slope.is_finite() {
        return None;
    }
    let pt = NotablePoint { series_index, kind: NotableKind::Tangent, x: clean_zero(x0), y: clean_zero(y0), slope: Some(clean_zero(slope)) };
    let ya = y0 + slope * (x_min - x0);
    let yb = y0 + slope * (x_max - x0);
    let seg = clip_segment((x_min, ya), (x_max, yb), x_min, x_max, y_min, y_max);
    Some((pt, seg))
}

/// Computes the definite integral `∫_a^b f(x) dx` via composite Simpson's rule (256 sub-intervals)
/// and returns closed viewport-clipped polygon(s) between `y = f(x)` and `y = 0`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn compute_integral(
    eval: &mut ScalarEval,
    series_index: usize,
    var: &str,
    expr: &str,
    a: f64,
    b: f64,
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
) -> Option<(IntegralInfo, Vec<Branch>)> {
    let n: usize = 256;
    let h = (b - a) / n as f64;
    let mut sum = 0.0;
    for i in 0..=n {
        let x = a + h * i as f64;
        let y = eval.eval_at(var, expr, x)?;
        let weight = if i == 0 || i == n {
            1.0
        } else if i % 2 == 1 {
            4.0
        } else {
            2.0
        };
        sum += weight * y;
    }
    let value = clean_zero(sum * h / 3.0);
    if !value.is_finite() {
        return None;
    }
    let info = IntegralInfo { series_index, a: clean_zero(a), b: clean_zero(b), value };

    // Build shaded polygon(s) clipped to `[x_min, x_max] × [y_min, y_max]`.
    let xa = a.max(x_min);
    let xb = b.min(x_max);
    let mut polys: Vec<Branch> = Vec::new();
    if xb > xa + 1e-9 {
        let poly_n: usize = 128;
        let y_base = 0.0f64.clamp(y_min, y_max);
        let mut top: Branch = Vec::with_capacity(poly_n + 3);
        top.push((xa, y_base));
        for i in 0..=poly_n {
            let x = xa + (xb - xa) * (i as f64 / poly_n as f64);
            if let Some(y) = eval.eval_at(var, expr, x) {
                top.push((x, y.clamp(y_min, y_max)));
            }
        }
        top.push((xb, y_base));
        if top.len() >= 4 {
            polys.push(top);
        }
    }
    Some((info, polys))
}

/// Extracts experimental `(x ± dx, y ± dy)` points from inline points or `Worksheet` vectors.
#[allow(clippy::too_many_arguments)]
pub(crate) fn extract_data_points(
    ws: Option<&Worksheet>,
    x_unit: Option<&str>,
    y_unit: Option<&str>,
    x_col: &str,
    y_col: &str,
    dx_col: Option<&str>,
    dy_col: Option<&str>,
    inline: &[DataPoint],
    errors: &mut Vec<String>,
) -> Vec<DataPoint> {
    if !inline.is_empty() && (x_col.is_empty() || y_col.is_empty()) {
        return inline.iter().copied().take(MAX_DATA_POINTS).collect();
    }
    let Some(w) = ws else {
        return inline.iter().copied().take(MAX_DATA_POINTS).collect();
    };
    if x_col.is_empty() || y_col.is_empty() {
        return inline.iter().copied().take(MAX_DATA_POINTS).collect();
    }
    let x_div = x_unit.and_then(|u| unit_quantity(w, u)).map_or(1.0, |q| q.value.max(1e-30));
    let y_div = y_unit.and_then(|u| unit_quantity(w, u)).map_or(1.0, |q| q.value.max(1e-30));

    let Some(Value::Vector(vx)) = w.scope().get(x_col) else {
        errors.push(format!("unknown vector column '{x_col}'"));
        return inline.iter().copied().take(MAX_DATA_POINTS).collect();
    };
    let Some(Value::Vector(vy)) = w.scope().get(y_col) else {
        errors.push(format!("unknown vector column '{y_col}'"));
        return inline.iter().copied().take(MAX_DATA_POINTS).collect();
    };
    let vdx = dx_col.and_then(|n| match w.scope().get(n) {
        Some(Value::Vector(v)) => Some(v),
        _ => None,
    });
    let vdy = dy_col.and_then(|n| match w.scope().get(n) {
        Some(Value::Vector(v)) => Some(v),
        _ => None,
    });

    let qx: Vec<_> = vx.iter().take(MAX_DATA_POINTS).collect();
    let qy: Vec<_> = vy.iter().take(MAX_DATA_POINTS).collect();
    let qdx: Vec<f64> = vdx.map(|v| v.values.iter().copied().take(MAX_DATA_POINTS).collect()).unwrap_or_default();
    let qdy: Vec<f64> = vdy.map(|v| v.values.iter().copied().take(MAX_DATA_POINTS).collect()).unwrap_or_default();

    let count = qx.len().min(qy.len());
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let (Some(ix), Some(iy)) = (qx.get(i), qy.get(i)) else {
            continue;
        };
        let dx = qdx.get(i).copied().or(ix.unc).map(|u| (u / x_div).abs()).filter(|u| u.is_finite() && *u > 0.0);
        let dy = qdy.get(i).copied().or(iy.unc).map(|u| (u / y_div).abs()).filter(|u| u.is_finite() && *u > 0.0);
        if let Some(pt) = (DataPoint { x: ix.value / x_div, y: iy.value / y_div, dx, dy }).sanitized() {
            out.push(pt);
        }
    }
    out
}

/// Computes linear (`y = c0 + c1·x`) or quadratic (`y = c0 + c1·x + c2·x²`) least-squares fit.
pub(crate) fn compute_regression(series_index: usize, kind: RegressionKind, pts: &[DataPoint]) -> Option<RegressionInfo> {
    match kind {
        RegressionKind::None => None,
        RegressionKind::Linear => fit_linear(series_index, pts),
        RegressionKind::Quadratic => fit_quadratic(series_index, pts),
    }
}

fn fit_linear(series_index: usize, pts: &[DataPoint]) -> Option<RegressionInfo> {
    if pts.len() < 2 {
        return None;
    }
    let n = pts.len() as f64;
    let sx: f64 = pts.iter().map(|p| p.x).sum();
    let sy: f64 = pts.iter().map(|p| p.y).sum();
    let xm = sx / n;
    let ym = sy / n;
    let mut sxx = 0.0;
    let mut sxy = 0.0;
    for p in pts {
        let dx = p.x - xm;
        sxx += dx * dx;
        sxy += dx * (p.y - ym);
    }
    if sxx.abs() <= 1e-15 {
        return None;
    }
    let slope = clean_zero(sxy / sxx);
    let intercept = clean_zero(ym - slope * xm);
    let r2 = r_squared(pts, ym, |x| intercept + slope * x);
    let sign = if intercept >= 0.0 { "+" } else { "-" };
    let eq = format!("y = {:.4}x {sign} {:.4} (R² = {:.4})", slope, intercept.abs(), r2);
    Some(RegressionInfo { series_index, kind: RegressionKind::Linear, coeffs: vec![intercept, slope], r2, equation: eq })
}

fn fit_quadratic(series_index: usize, pts: &[DataPoint]) -> Option<RegressionInfo> {
    if pts.len() < 3 {
        return None;
    }
    let mut s = [0.0f64; 5];
    let mut t = [0.0f64; 3];
    for p in pts {
        let x = p.x;
        let x2 = x * x;
        s[0] += 1.0;
        s[1] += x;
        s[2] += x2;
        s[3] += x2 * x;
        s[4] += x2 * x2;
        t[0] += p.y;
        t[1] += x * p.y;
        t[2] += x2 * p.y;
    }
    let mat = [[s[0], s[1], s[2], t[0]], [s[1], s[2], s[3], t[1]], [s[2], s[3], s[4], t[2]]];
    let [c0, c1, c2] = solve_3x3(mat)?;
    let ym = t[0] / s[0].max(1.0);
    let r2 = r_squared(pts, ym, |x| c0 + c1 * x + c2 * x * x);
    let eq = format!("y = {:.4}x² + {:.4}x + {:.4} (R² = {:.4})", c2, c1, c0, r2);
    Some(RegressionInfo {
        series_index,
        kind: RegressionKind::Quadratic,
        coeffs: vec![clean_zero(c0), clean_zero(c1), clean_zero(c2)],
        r2,
        equation: eq,
    })
}

fn solve_3x3(mut a: [[f64; 4]; 3]) -> Option<[f64; 3]> {
    for col in 0..3 {
        let mut piv = col;
        let mut best = a.get(col).and_then(|r| r.get(col)).copied().unwrap_or(0.0).abs();
        for row in (col + 1)..3 {
            let v = a.get(row).and_then(|r| r.get(col)).copied().unwrap_or(0.0).abs();
            if v > best {
                best = v;
                piv = row;
            }
        }
        if best <= 1e-15 {
            return None;
        }
        a.swap(col, piv);
        let diag = a.get(col).and_then(|r| r.get(col)).copied().unwrap_or(1.0);
        if let Some(piv_row) = a.get_mut(col) {
            for cell in piv_row.iter_mut().skip(col) {
                *cell /= diag;
            }
        }
        let piv_copy = a.get(col).copied().unwrap_or([0.0; 4]);
        for (row, r_mut) in a.iter_mut().enumerate() {
            if row == col {
                continue;
            }
            let factor = r_mut.get(col).copied().unwrap_or(0.0);
            for (c, cell) in r_mut.iter_mut().enumerate().skip(col) {
                *cell -= factor * piv_copy.get(c).copied().unwrap_or(0.0);
            }
        }
    }
    let c0 = a.first().and_then(|r| r.get(3)).copied()?;
    let c1 = a.get(1).and_then(|r| r.get(3)).copied()?;
    let c2 = a.get(2).and_then(|r| r.get(3)).copied()?;
    let out = [c0, c1, c2];
    out.iter().all(|v| v.is_finite()).then_some(out)
}

fn r_squared(pts: &[DataPoint], ym: f64, pred: impl Fn(f64) -> f64) -> f64 {
    let mut ss_tot = 0.0;
    let mut ss_res = 0.0;
    for p in pts {
        let dt = p.y - ym;
        let dr = p.y - pred(p.x);
        ss_tot += dt * dt;
        ss_res += dr * dr;
    }
    if ss_tot <= 1e-15 { 1.0 } else { clean_zero((1.0 - ss_res / ss_tot).clamp(0.0, 1.0)) }
}

fn clean_zero(v: f64) -> f64 {
    if v.abs() < 1e-12 { 0.0 } else { v }
}
