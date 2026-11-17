//! Automatic ranges (`Plot::auto_range`): the view that shows everything the plot has to show.
//!
//! The abscissa spans what the analysis found (roots, extrema, tangent points, integral bounds),
//! the data and the parametric and polar curves; with none of these it stays as set. The
//! ordinate spans the functions over that abscissa (their central 96 %, so a pole doesn't
//! flatten the rest), the analysis points, the data with their error bars and the curves. Both
//! get a margin and are rounded outward to a tidy step.

use strix_calc::Worksheet;

use crate::features::extract_data_points;
use crate::model::{Plot, SeriesKind};
use crate::sample::{ScalarEval, sample_parametric, sample_polar};

/// Samples per function when measuring the ordinate.
const FIT_SAMPLES: usize = 240;

/// `[x_min, x_max, y_min, y_max]` fitted to `p`'s content (`p` with `auto_range` off).
pub(crate) fn fitted_range(p: &Plot, ws: Option<&Worksheet>) -> [f64; 4] {
    let first = p.render_for(ws, true);
    let mut xs: Vec<f64> = Vec::new();
    let mut ys: Vec<f64> = Vec::new();
    for n in &first.notables {
        xs.push(n.x);
        ys.push(n.y);
    }
    for ig in &first.integrals {
        xs.extend([ig.a, ig.b]);
    }
    for s in &p.series {
        match &s.kind {
            SeriesKind::Function { .. } => {}
            SeriesKind::Parametric { x_expr, y_expr, param, t_min, t_max } => {
                let mut eval = ScalarEval::new(ws, None, None);
                for br in sample_parametric(&mut eval, param, (x_expr, y_expr), (*t_min, *t_max), (f64::INFINITY, f64::INFINITY)) {
                    for (x, y) in br {
                        xs.push(x);
                        ys.push(y);
                    }
                }
            }
            SeriesKind::Polar { expr, param, theta } => {
                let mut eval = ScalarEval::new(ws, None, None);
                let [t0, t1] = theta.unwrap_or([0.0, p.angle_unit.turn()]);
                let k = p.angle_unit.to_rad();
                for br in sample_polar(&mut eval, param, expr, (t0 * k, t1 * k), p.angles, (f64::INFINITY, f64::INFINITY)) {
                    for (x, y) in br {
                        xs.push(x);
                        ys.push(y);
                    }
                }
            }
            SeriesKind::Data { x_col, y_col, dx_col, dy_col, points, .. } => {
                let mut errs = Vec::new();
                let pts = extract_data_points(
                    ws,
                    p.x_unit.as_deref(),
                    p.y_unit.as_deref(),
                    x_col,
                    y_col,
                    dx_col.as_deref(),
                    dy_col.as_deref(),
                    points,
                    &mut errs,
                );
                for pt in pts {
                    let (dx, dy) = (pt.dx.unwrap_or(0.0), pt.dy.unwrap_or(0.0));
                    xs.extend([pt.x - dx, pt.x + dx]);
                    ys.extend([pt.y - dy, pt.y + dy]);
                }
            }
        }
    }
    let (x0, x1) = match hull(&xs) {
        // Wide enough to be a range of its own, however small the numbers (metres for
        // nanometres), as long as the abscissae can still be told apart.
        Some((a, b)) if b - a > 1e-9 && b - a > 1e-9 * a.abs().max(b.abs()) => tidy(a, b),
        // One abscissa (a lone measurement, a tangent point) outside the ranges set: they go
        // out to it.
        Some((a, _)) if a < p.x_min || a > p.x_max => tidy(a.min(p.x_min), a.max(p.x_max)),
        _ => (p.x_min, p.x_max),
    };

    // The functions over the new abscissa.
    for s in &p.series {
        if let SeriesKind::Function { expr, var, domain, .. } = &s.kind {
            let (a, b) = match domain {
                Some([da, db]) => (da.max(x0), db.min(x1)),
                None => (x0, x1),
            };
            if b <= a {
                continue;
            }
            let mut eval = ScalarEval::new(ws, p.x_unit.as_deref(), p.y_unit.as_deref());
            let mut vals: Vec<f64> = (0..=FIT_SAMPLES)
                .filter_map(|i| eval.eval_at(var, expr, a + (b - a) * (i as f64 / FIT_SAMPLES as f64)))
                .filter(|v| v.is_finite())
                .collect();
            vals.sort_by(f64::total_cmp);
            let cut = vals.len() * 2 / 100;
            if let (Some(lo), Some(hi)) = (vals.get(cut), vals.get(vals.len().saturating_sub(1 + cut))) {
                ys.extend([*lo, *hi]);
            }
        }
    }
    let ys: Vec<f64> = ys.into_iter().filter(|v| v.is_finite() && v.abs() < 1e9).collect();
    let (y0, y1) = match hull(&ys) {
        Some((a, b)) if b - a > 1e-9 => tidy(a, b),
        Some((a, _)) => tidy(a - 1.0, a + 1.0),
        None => (p.y_min, p.y_max),
    };
    [x0, x1, y0, y1]
}

fn hull(v: &[f64]) -> Option<(f64, f64)> {
    let mut it = v.iter().copied().filter(|x| x.is_finite() && x.abs() < 1e9);
    let first = it.next()?;
    Some(it.fold((first, first), |(lo, hi), x| (lo.min(x), hi.max(x))))
}

/// `[lo, hi]` with a 8 % margin, rounded outward to a tidy step; the origin is kept in view when
/// it is close (within a quarter of the span), so the axes stay drawn.
fn tidy(lo: f64, hi: f64) -> (f64, f64) {
    let span = (hi - lo).max(1e-9);
    let (mut lo, mut hi) = (lo - span * 0.08, hi + span * 0.08);
    if lo > 0.0 && lo < span * 0.25 {
        lo = 0.0;
    }
    if hi < 0.0 && -hi < span * 0.25 {
        hi = 0.0;
    }
    let step = crate::render::nice_step(hi - lo, 8.0);
    // Multiples of the step, without binary noise (`0.30000000000000004` → `0.3`).
    let decimals = (-step.log10().floor()).clamp(0.0, 12.0) as i32;
    let scale = 10f64.powi(decimals);
    let clean = |v: f64| (v * scale).round() / scale;
    let a = clean((lo / step).floor() * step);
    let b = clean((hi / step).ceil() * step);
    (a.clamp(-1e9, 1e9), b.clamp(-1e9, 1e9))
}
