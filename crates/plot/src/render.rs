//! Cartesian plot layout and rendering into paper-millimetre primitives.

use strix_calc::{Options, Worksheet, format_number};

use crate::features::{
    IntegralInfo, NotablePoint, RegressionInfo, compute_integral, compute_regression, compute_tangent, extract_data_points, find_extrema, find_roots,
};
use crate::model::{AngleConvention, DataPoint, Plot, RegressionKind, Series, SeriesKind};
use crate::sample::{Branch, ScalarEval, clip_branch, clip_segment, sample_function, sample_parametric, sample_polar};

/// Text alignment anchor in paper millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlotAnchor {
    Left,
    Center,
    Right,
    TopCenter,
    MiddleRight,
}

/// Paper-millimetre drawing primitive (`(0, 0)` at top-left, `y` down).
#[derive(Debug, Clone, PartialEq)]
pub enum PlotPrim {
    Rect { rect: [f64; 4], fill: Option<[u8; 3]>, stroke: Option<([u8; 3], f64)> },
    Path { pts: Vec<(f64, f64)>, closed: bool, stroke: Option<([u8; 3], f64)>, fill: Option<[u8; 3]>, dash: Vec<f64> },
    Dot { at: (f64, f64), r: f64, fill: [u8; 3], stroke: Option<([u8; 3], f64)> },
    Text { at: (f64, f64), text: String, height: f64, bold: bool, anchor: PlotAnchor, color: [u8; 3] },
}

/// Rendered plot primitives, viewport rectangle and mathematical summary.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlotRendered {
    /// Overall `(width_mm, height_mm)`.
    pub size: (f64, f64),
    /// Inner Cartesian frame `[x_mm, y_mm, w_mm, h_mm]`.
    pub viewport: [f64; 4],
    /// The ranges drawn, `[x_min, x_max, y_min, y_max]`: as set, or fitted (`Plot::auto_range`).
    pub range: [f64; 4],
    pub prims: Vec<PlotPrim>,
    pub notables: Vec<NotablePoint>,
    pub integrals: Vec<IntegralInfo>,
    pub regressions: Vec<RegressionInfo>,
    pub errors: Vec<String>,
}

#[derive(Clone, Copy)]
struct Viewport {
    x0: f64,
    y0: f64,
    w: f64,
    h: f64,
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
}

impl Viewport {
    fn to_paper(self, x: f64, y: f64) -> (f64, f64) {
        let tx = (x - self.x_min) / (self.x_max - self.x_min);
        let ty = (y - self.y_min) / (self.y_max - self.y_min);
        (self.x0 + tx * self.w, self.y0 + (1.0 - ty) * self.h)
    }
}

impl Plot {
    /// Renders the complete teacher view (`show_answers = true`).
    pub fn render(&self, ws: Option<&Worksheet>) -> PlotRendered {
        self.render_for(ws, true)
    }

    /// Renders the plot for teacher (`show_answers = true`) or student (`show_answers = false`).
    pub fn render_for(&self, ws: Option<&Worksheet>, show_answers: bool) -> PlotRendered {
        let mut p = self.clone().sanitized();
        if p.auto_range {
            p.auto_range = false;
            let [x0, x1, y0, y1] = crate::fit::fitted_range(&p, ws);
            p.x_min = x0;
            p.x_max = x1;
            p.y_min = y0;
            p.y_max = y1;
            p = p.sanitized();
        }
        let vp = build_viewport(&p);
        let mut out = PlotRendered {
            size: (p.width, p.height),
            viewport: [vp.x0, vp.y0, vp.w, vp.h],
            range: [p.x_min, p.x_max, p.y_min, p.y_max],
            ..PlotRendered::default()
        };
        let opts = ws.map_or_else(Options::default, |w| *w.options());

        draw_frame_and_axes(&p, vp, &opts, &mut out.prims);

        let allow_plot_series = show_answers || !p.answer;
        let mut legend_entries: Vec<(String, [u8; 3], Vec<f64>)> = Vec::new();
        for (idx, series) in p.series.iter().enumerate() {
            let draw = allow_plot_series && (show_answers || !series.answer);
            render_one_series(&p, vp, ws, idx, series, draw, &mut out);
            if draw && p.legend {
                let lbl = series_legend_label(&p, series);
                if !lbl.is_empty() {
                    legend_entries.push((lbl, series.color, series.style.pattern(0.45)));
                }
            }
        }
        if !legend_entries.is_empty() {
            draw_legend(vp, &legend_entries, &mut out.prims);
        }
        out
    }
}

fn build_viewport(p: &Plot) -> Viewport {
    // An axis name goes by its arrow when that axis is in view, else in the margin.
    let y_name_in_margin = !(p.y_label.is_empty() && p.y_unit.is_none()) && !(p.x_min <= 0.0 && 0.0 <= p.x_max);
    let x_name_in_margin = !(p.x_label.is_empty() && p.x_unit.is_none()) && !(p.y_min <= 0.0 && 0.0 <= p.y_max);
    let left = if y_name_in_margin { 15.0 } else { 11.0 };
    let right = 4.5;
    let top = if p.title.is_empty() { 4.5 } else { 9.5 };
    let bottom = if x_name_in_margin { 11.5 } else { 7.5 };
    let w = (p.width - left - right).max(8.0);
    let h = (p.height - top - bottom).max(8.0);
    Viewport { x0: left, y0: top, w, h, x_min: p.x_min, x_max: p.x_max, y_min: p.y_min, y_max: p.y_max }
}

fn draw_frame_and_axes(p: &Plot, vp: Viewport, opts: &Options, prims: &mut Vec<PlotPrim>) {
    prims.push(PlotPrim::Rect { rect: [0.0, 0.0, p.width, p.height], fill: Some([255, 255, 255]), stroke: None });

    let x_step = nice_step(vp.x_max - vp.x_min, (vp.w / 14.0).clamp(3.0, 10.0));
    let y_step = nice_step(vp.y_max - vp.y_min, (vp.h / 11.0).clamp(3.0, 9.0));

    if p.minor_grid {
        draw_grid_lines(vp, x_step / 5.0, y_step / 5.0, [0xED, 0xF1, 0xF5], 0.12, prims);
    }
    if p.grid {
        draw_grid_lines(vp, x_step, y_step, [0xD6, 0xDE, 0xE6], 0.20, prims);
    }

    // Outer viewport border.
    prims.push(PlotPrim::Rect { rect: [vp.x0, vp.y0, vp.w, vp.h], fill: None, stroke: Some(([0x64, 0x74, 0x8B], 0.28)) });

    // Origin Cartesian axes `x = 0` and `y = 0` when inside `[x_min, x_max] × [y_min, y_max]`.
    let axis_col = [0x1E, 0x29, 0x3B];
    if vp.y_min <= 0.0 && 0.0 <= vp.y_max {
        let (_, py) = vp.to_paper(vp.x_min, 0.0);
        prims.push(PlotPrim::Path {
            pts: vec![(vp.x0, py), (vp.x0 + vp.w, py)],
            closed: false,
            stroke: Some((axis_col, 0.38)),
            fill: None,
            dash: Vec::new(),
        });
        arrow_right((vp.x0 + vp.w, py), axis_col, prims);
    }
    if vp.x_min <= 0.0 && 0.0 <= vp.x_max {
        let (px, _) = vp.to_paper(0.0, vp.y_min);
        prims.push(PlotPrim::Path {
            pts: vec![(px, vp.y0 + vp.h), (px, vp.y0)],
            closed: false,
            stroke: Some((axis_col, 0.38)),
            fill: None,
            dash: Vec::new(),
        });
        arrow_up((px, vp.y0), axis_col, prims);
    }

    draw_ticks_and_labels(p, vp, x_step, y_step, opts, prims);
    draw_axis_names(p, vp, prims);
    if p.angle_mark {
        draw_angle_mark(p.angles, vp, prims);
    }
}

/// Each axis's name (label and unit) by its arrow: the abscissa's above the axis at its right
/// end, the ordinate's beside the axis at its top, as Cartesian axes are labelled. An axis out of
/// view has its name in the margin (under the frame, above the left edge).
fn draw_axis_names(p: &Plot, vp: Viewport, prims: &mut Vec<PlotPrim>) {
    let ink = [0x0F, 0x17, 0x2A];
    let name = |at: (f64, f64), text: String, anchor: PlotAnchor| PlotPrim::Text { at, text, height: 2.4, bold: false, anchor, color: ink };
    let xl = axis_caption(&p.x_label, p.x_unit.as_deref());
    if !xl.is_empty() {
        if vp.y_min <= 0.0 && 0.0 <= vp.y_max {
            let (_, py) = vp.to_paper(vp.x_min, 0.0);
            // Above the axis, or under it when the axis runs along the top of the frame.
            let y = if py - 4.0 < vp.y0 { py + 3.6 } else { py - 1.4 };
            prims.push(name((vp.x0 + vp.w - 0.6, y), xl, PlotAnchor::Right));
        } else {
            prims.push(name((vp.x0 + vp.w * 0.5, p.height - 1.5), xl, PlotAnchor::Center));
        }
    }
    let yl = axis_caption(&p.y_label, p.y_unit.as_deref());
    if !yl.is_empty() {
        if vp.x_min <= 0.0 && 0.0 <= vp.x_max {
            let (px, _) = vp.to_paper(0.0, vp.y_min);
            // Right of the arrow, or left of it when the axis runs along the right of the frame.
            if px + 12.0 > vp.x0 + vp.w {
                prims.push(name((px - 1.6, vp.y0 + 2.6), yl, PlotAnchor::Right));
            } else {
                prims.push(name((px + 1.6, vp.y0 + 2.6), yl, PlotAnchor::Left));
            }
        } else {
            prims.push(name((1.5, vp.y0 - 1.2), yl, PlotAnchor::Left));
        }
    }
}

/// The positive direction of angles: a short ray along the reference axis (the positive
/// abscissa, or the positive ordinate in surveying), an arc turning the right way with an arrow
/// at its end, and `θ` inside it. At the origin when it is in view, else near the lower-left
/// corner.
fn draw_angle_mark(convention: AngleConvention, vp: Viewport, prims: &mut Vec<PlotPrim>) {
    let col = [0x47, 0x55, 0x69];
    let origin_in = vp.x_min <= 0.0 && 0.0 <= vp.x_max && vp.y_min <= 0.0 && 0.0 <= vp.y_max;
    let r = (vp.w.min(vp.h) * 0.12).clamp(3.0, 7.0);
    let (cx, cy) = if origin_in { vp.to_paper(0.0, 0.0) } else { (vp.x0 + r + 3.0, vp.y0 + vp.h - r - 3.0) };
    // A point at angle `a` (radians, this convention) and paper distance `d` (y down on paper).
    let at = |a: f64, d: f64| {
        let (x, y) = convention.point(d, a);
        (cx + x, cy - y)
    };
    let sweep = 60f64.to_radians();
    prims.push(PlotPrim::Path { pts: vec![(cx, cy), at(0.0, r + 2.0)], closed: false, stroke: Some((col, 0.25)), fill: None, dash: Vec::new() });
    let n: usize = 24;
    let arc: Vec<(f64, f64)> = (0..=n).map(|k| at(sweep * k as f64 / n as f64, r)).collect();
    if let (Some(&tip), Some(&before)) = (arc.last(), arc.get(n.saturating_sub(2))) {
        let (dx, dy) = (tip.0 - before.0, tip.1 - before.1);
        let len = dx.hypot(dy).max(1e-9);
        let (ux, uy) = (dx / len, dy / len);
        let (bx, by) = (tip.0 - ux * 1.4, tip.1 - uy * 1.4);
        prims.push(PlotPrim::Path { pts: arc, closed: false, stroke: Some((col, 0.3)), fill: None, dash: Vec::new() });
        prims.push(PlotPrim::Path {
            pts: vec![tip, (bx - uy * 0.55, by + ux * 0.55), (bx + uy * 0.55, by - ux * 0.55)],
            closed: true,
            stroke: None,
            fill: Some(col),
            dash: Vec::new(),
        });
    }
    let label = at(sweep * 0.5, r * 0.62);
    prims.push(PlotPrim::Text {
        at: (label.0, label.1 + 0.8),
        text: "θ".to_string(),
        height: 2.1,
        bold: false,
        anchor: PlotAnchor::Center,
        color: col,
    });
}

fn draw_grid_lines(vp: Viewport, x_step: f64, y_step: f64, color: [u8; 3], width: f64, prims: &mut Vec<PlotPrim>) {
    for xv in ticks_in(vp.x_min, vp.x_max, x_step) {
        let (px, _) = vp.to_paper(xv, vp.y_min);
        prims.push(PlotPrim::Path {
            pts: vec![(px, vp.y0), (px, vp.y0 + vp.h)],
            closed: false,
            stroke: Some((color, width)),
            fill: None,
            dash: Vec::new(),
        });
    }
    for yv in ticks_in(vp.y_min, vp.y_max, y_step) {
        let (_, py) = vp.to_paper(vp.x_min, yv);
        prims.push(PlotPrim::Path {
            pts: vec![(vp.x0, py), (vp.x0 + vp.w, py)],
            closed: false,
            stroke: Some((color, width)),
            fill: None,
            dash: Vec::new(),
        });
    }
}

fn draw_ticks_and_labels(p: &Plot, vp: Viewport, x_step: f64, y_step: f64, opts: &Options, prims: &mut Vec<PlotPrim>) {
    let label_col = [0x33, 0x41, 0x55];
    let tick_opts = Options { digits: 4, zeros: false, ..*opts };
    for xv in ticks_in(vp.x_min, vp.x_max, x_step) {
        let (px, _) = vp.to_paper(xv, vp.y_min);
        prims.push(PlotPrim::Path {
            pts: vec![(px, vp.y0 + vp.h), (px, vp.y0 + vp.h + 1.1)],
            closed: false,
            stroke: Some((label_col, 0.25)),
            fill: None,
            dash: Vec::new(),
        });
        let v = if xv.abs() < 1e-10 * x_step { 0.0 } else { xv };
        prims.push(PlotPrim::Text {
            at: (px, vp.y0 + vp.h + 1.5),
            text: format_number(v, &tick_opts),
            height: 2.2,
            bold: false,
            anchor: PlotAnchor::TopCenter,
            color: label_col,
        });
    }
    for yv in ticks_in(vp.y_min, vp.y_max, y_step) {
        let (_, py) = vp.to_paper(vp.x_min, yv);
        prims.push(PlotPrim::Path {
            pts: vec![(vp.x0 - 1.1, py), (vp.x0, py)],
            closed: false,
            stroke: Some((label_col, 0.25)),
            fill: None,
            dash: Vec::new(),
        });
        let v = if yv.abs() < 1e-10 * y_step { 0.0 } else { yv };
        prims.push(PlotPrim::Text {
            at: (vp.x0 - 1.5, py),
            text: format_number(v, &tick_opts),
            height: 2.2,
            bold: false,
            anchor: PlotAnchor::MiddleRight,
            color: label_col,
        });
    }

    if !p.title.is_empty() {
        prims.push(PlotPrim::Text {
            at: (vp.x0 + vp.w * 0.5, 4.8),
            text: p.title.clone(),
            height: 2.9,
            bold: true,
            anchor: PlotAnchor::Center,
            color: [0x0F, 0x17, 0x2A],
        });
    }
}

fn axis_caption(label: &str, unit: Option<&str>) -> String {
    match (label.trim(), unit.map(str::trim).filter(|u| !u.is_empty())) {
        ("", None) => String::new(),
        (l, None) => l.to_string(),
        ("", Some(u)) => format!("[{u}]"),
        (l, Some(u)) if l.contains(u) => l.to_string(),
        (l, Some(u)) => format!("{l} [{u}]"),
    }
}

fn arrow_right(tip: (f64, f64), color: [u8; 3], prims: &mut Vec<PlotPrim>) {
    let (x, y) = tip;
    prims.push(PlotPrim::Path {
        pts: vec![(x, y), (x - 1.8, y - 0.65), (x - 1.8, y + 0.65)],
        closed: true,
        stroke: None,
        fill: Some(color),
        dash: Vec::new(),
    });
}

fn arrow_up(tip: (f64, f64), color: [u8; 3], prims: &mut Vec<PlotPrim>) {
    let (x, y) = tip;
    prims.push(PlotPrim::Path {
        pts: vec![(x, y), (x - 0.65, y + 1.8), (x + 0.65, y + 1.8)],
        closed: true,
        stroke: None,
        fill: Some(color),
        dash: Vec::new(),
    });
}

fn render_one_series(p: &Plot, vp: Viewport, ws: Option<&Worksheet>, idx: usize, series: &Series, draw: bool, out: &mut PlotRendered) {
    let dash = series.style.pattern(series.width);
    match &series.kind {
        SeriesKind::Function { .. } => {
            render_function_series(p, vp, ws, idx, series, draw, out);
        }
        SeriesKind::Parametric { x_expr, y_expr, param, t_min, t_max } => {
            let mut eval = ScalarEval::new(ws, None, None);
            let branches = sample_parametric(&mut eval, param, (x_expr, y_expr), (*t_min, *t_max), (vp.x_max - vp.x_min, vp.y_max - vp.y_min));
            if branches.is_empty()
                && let Some(err) = eval.last_hard_error.take().or_else(|| eval.last_domain_error.take())
            {
                out.errors.push(format!("series {}: {err}", idx + 1));
            }
            if draw {
                emit_clipped_branches(&branches, vp, series.color, series.width, &dash, &mut out.prims);
            }
        }
        SeriesKind::Polar { expr, param, theta } => {
            let mut eval = ScalarEval::new(ws, None, None);
            let [t0, t1] = theta.unwrap_or([0.0, p.angle_unit.turn()]);
            let k = p.angle_unit.to_rad();
            let spans = (vp.x_max - vp.x_min, vp.y_max - vp.y_min);
            let branches = sample_polar(&mut eval, param, expr, (t0 * k, t1 * k), p.angles, spans);
            if branches.is_empty()
                && let Some(err) = eval.last_hard_error.take().or_else(|| eval.last_domain_error.take())
            {
                out.errors.push(format!("series {}: {err}", idx + 1));
            }
            if draw {
                emit_clipped_branches(&branches, vp, series.color, series.width, &dash, &mut out.prims);
            }
        }
        SeriesKind::Data { x_col, y_col, dx_col, dy_col, points, regression, connect } => {
            let pts = extract_data_points(
                ws,
                p.x_unit.as_deref(),
                p.y_unit.as_deref(),
                x_col,
                y_col,
                dx_col.as_deref(),
                dy_col.as_deref(),
                points,
                &mut out.errors,
            );
            if let Some(reg) = compute_regression(idx, *regression, &pts) {
                if draw {
                    draw_regression_curve(vp, &reg, series.color, series.width, &mut out.prims);
                }
                out.regressions.push(reg);
            }
            if draw {
                draw_data_series(vp, &pts, series.color, series.width, *connect, &dash, &mut out.prims);
            }
        }
    }
}

fn render_function_series(p: &Plot, vp: Viewport, ws: Option<&Worksheet>, idx: usize, series: &Series, draw: bool, out: &mut PlotRendered) {
    let SeriesKind::Function { expr, var, domain, roots, extrema, tangent_at, integral } = &series.kind else {
        return;
    };
    let dash = series.style.pattern(series.width);
    let mut eval = ScalarEval::new(ws, p.x_unit.as_deref(), p.y_unit.as_deref());
    let (xa, xb) = match domain {
        Some([a, b]) => (a.max(vp.x_min), b.min(vp.x_max)),
        None => (vp.x_min, vp.x_max),
    };
    if xb <= xa {
        return;
    }
    if let Some([ia, ib]) = integral
        && let Some((info, polys)) = compute_integral(&mut eval, idx, var, expr, *ia, *ib, vp.x_min, vp.x_max, vp.y_min, vp.y_max)
    {
        if draw {
            emit_integral_polys(polys, vp, series.color, &mut out.prims);
        }
        out.integrals.push(info);
    }

    let branches = sample_function(&mut eval, var, expr, xa, xb, vp.y_min, vp.y_max);
    if branches.is_empty()
        && let Some(err) = eval.last_hard_error.take().or_else(|| eval.last_domain_error.take())
    {
        out.errors.push(format!("series {}: {err}", idx + 1));
    }
    if draw {
        emit_clipped_branches(&branches, vp, series.color, series.width, &dash, &mut out.prims);
    }
    collect_and_draw_notables(&mut eval, vp, idx, series, (&branches, *roots, *extrema, *tangent_at), draw, out);
}

fn emit_integral_polys(polys: Vec<Branch>, vp: Viewport, color: [u8; 3], prims: &mut Vec<PlotPrim>) {
    let tint = blend_white(color, 0.22);
    for poly in polys {
        let pts = poly.into_iter().map(|(x, y)| vp.to_paper(x, y)).collect();
        prims.push(PlotPrim::Path { pts, closed: true, stroke: None, fill: Some(tint), dash: Vec::new() });
    }
}

fn collect_and_draw_notables(
    eval: &mut ScalarEval,
    vp: Viewport,
    idx: usize,
    series: &Series,
    flags: (&[Branch], bool, bool, Option<f64>),
    draw: bool,
    out: &mut PlotRendered,
) {
    let SeriesKind::Function { expr, var, .. } = &series.kind else {
        return;
    };
    let (branches, roots, extrema, tangent_at) = flags;
    let y_span = vp.y_max - vp.y_min;
    let mut series_notables = Vec::new();
    if roots {
        series_notables.extend(find_roots(eval, idx, var, expr, branches, y_span));
    }
    if extrema {
        series_notables.extend(find_extrema(eval, idx, var, expr, branches));
    }
    if let Some(x0) = tangent_at
        && let Some((pt, seg)) = compute_tangent(eval, idx, var, expr, x0, vp.x_min, vp.x_max, vp.y_min, vp.y_max)
    {
        if draw && let Some((a, b)) = seg {
            out.prims.push(PlotPrim::Path {
                pts: vec![vp.to_paper(a.0, a.1), vp.to_paper(b.0, b.1)],
                closed: false,
                stroke: Some((series.color, (series.width * 0.85).max(0.25))),
                fill: None,
                dash: vec![1.6, 1.1],
            });
        }
        series_notables.push(pt);
    }
    if draw {
        for npt in &series_notables {
            let inside = npt.x >= vp.x_min && npt.x <= vp.x_max && npt.y >= vp.y_min && npt.y <= vp.y_max;
            if inside {
                out.prims.push(PlotPrim::Dot { at: vp.to_paper(npt.x, npt.y), r: 0.85, fill: [255, 255, 255], stroke: Some((series.color, 0.38)) });
            }
        }
    }
    out.notables.extend(series_notables);
}

fn emit_clipped_branches(branches: &[Branch], vp: Viewport, color: [u8; 3], width: f64, dash: &[f64], prims: &mut Vec<PlotPrim>) {
    for br in branches {
        for clipped in clip_branch(br, vp.x_min, vp.x_max, vp.y_min, vp.y_max) {
            let pts: Vec<(f64, f64)> = clipped.into_iter().map(|(x, y)| vp.to_paper(x, y)).collect();
            if pts.len() >= 2 {
                prims.push(PlotPrim::Path { pts, closed: false, stroke: Some((color, width)), fill: None, dash: dash.to_vec() });
            }
        }
    }
}

fn draw_data_series(vp: Viewport, pts: &[DataPoint], color: [u8; 3], width: f64, connect: bool, dash: &[f64], prims: &mut Vec<PlotPrim>) {
    if connect && pts.len() >= 2 {
        let raw: Branch = pts.iter().map(|p| (p.x, p.y)).collect();
        emit_clipped_branches(&[raw], vp, color, width, dash, prims);
    }
    let err_w = (width * 0.7).clamp(0.2, 0.5);
    let cap_mm = 0.75;
    for pt in pts {
        if let Some(dx) = pt.dx
            && dx > 0.0
            && let Some((a, b)) = clip_segment((pt.x - dx, pt.y), (pt.x + dx, pt.y), vp.x_min, vp.x_max, vp.y_min, vp.y_max)
        {
            let pa = vp.to_paper(a.0, a.1);
            let pb = vp.to_paper(b.0, b.1);
            prims.push(PlotPrim::Path { pts: vec![pa, pb], closed: false, stroke: Some((color, err_w)), fill: None, dash: Vec::new() });
            for end in [pa, pb] {
                prims.push(PlotPrim::Path {
                    pts: vec![(end.0, end.1 - cap_mm), (end.0, end.1 + cap_mm)],
                    closed: false,
                    stroke: Some((color, err_w)),
                    fill: None,
                    dash: Vec::new(),
                });
            }
        }
        if let Some(dy) = pt.dy
            && dy > 0.0
            && let Some((a, b)) = clip_segment((pt.x, pt.y - dy), (pt.x, pt.y + dy), vp.x_min, vp.x_max, vp.y_min, vp.y_max)
        {
            let pa = vp.to_paper(a.0, a.1);
            let pb = vp.to_paper(b.0, b.1);
            prims.push(PlotPrim::Path { pts: vec![pa, pb], closed: false, stroke: Some((color, err_w)), fill: None, dash: Vec::new() });
            for end in [pa, pb] {
                prims.push(PlotPrim::Path {
                    pts: vec![(end.0 - cap_mm, end.1), (end.0 + cap_mm, end.1)],
                    closed: false,
                    stroke: Some((color, err_w)),
                    fill: None,
                    dash: Vec::new(),
                });
            }
        }
        if pt.x >= vp.x_min && pt.x <= vp.x_max && pt.y >= vp.y_min && pt.y <= vp.y_max {
            prims.push(PlotPrim::Dot { at: vp.to_paper(pt.x, pt.y), r: 0.75, fill: color, stroke: None });
        }
    }
}

fn draw_regression_curve(vp: Viewport, reg: &RegressionInfo, color: [u8; 3], width: f64, prims: &mut Vec<PlotPrim>) {
    let c0 = reg.coeffs.first().copied().unwrap_or(0.0);
    let c1 = reg.coeffs.get(1).copied().unwrap_or(0.0);
    let c2 = reg.coeffs.get(2).copied().unwrap_or(0.0);
    let steps = if reg.kind == RegressionKind::Linear { 2 } else { 96 };
    let mut branch = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let x = vp.x_min + (vp.x_max - vp.x_min) * (i as f64 / steps as f64);
        let y = c0 + c1 * x + c2 * x * x;
        branch.push((x, y));
    }
    emit_clipped_branches(&[branch], vp, color, (width * 0.9).max(0.3), &[1.8, 1.1], prims);
}

/// A legend entry: the series' formula, after its name when it has one (`f(x) = sin(x)`) or
/// the ordinate's (`y = x²`); data by their name or columns.
fn series_legend_label(p: &Plot, s: &Series) -> String {
    let named = |lhs: &str, rhs: &str| if s.label.trim().is_empty() { format!("{lhs} = {rhs}") } else { format!("{} = {rhs}", s.label.trim()) };
    match &s.kind {
        SeriesKind::Function { expr, .. } => named(&p.y_name(), &pretty_formula(expr)),
        SeriesKind::Parametric { x_expr, y_expr, .. } => {
            let pair = format!("({}, {})", pretty_formula(x_expr), pretty_formula(y_expr));
            if s.label.trim().is_empty() { pair } else { format!("{} = {pair}", s.label.trim()) }
        }
        SeriesKind::Polar { expr, .. } => named("ρ", &pretty_formula(expr)),
        SeriesKind::Data { .. } if !s.label.is_empty() => s.label.clone(),
        SeriesKind::Data { x_col, y_col, .. } if !x_col.is_empty() && !y_col.is_empty() => format!("{y_col}({x_col})"),
        SeriesKind::Data { .. } => String::new(),
    }
}

/// A formula in the Equation's linear format written as text reads it: single-character
/// powers as superscripts (`x^2` → `x²`), `*` as `·`, `sqrt` as `√`, without the invisible
/// function application (U+2061).
pub(crate) fn pretty_formula(expr: &str) -> String {
    const SUP: [(char, char); 13] = [
        ('0', '⁰'),
        ('1', '¹'),
        ('2', '²'),
        ('3', '³'),
        ('4', '⁴'),
        ('5', '⁵'),
        ('6', '⁶'),
        ('7', '⁷'),
        ('8', '⁸'),
        ('9', '⁹'),
        ('-', '⁻'),
        ('n', 'ⁿ'),
        ('+', '⁺'),
    ];
    let sup = |c: char| SUP.iter().find(|(a, _)| *a == c).map(|(_, b)| *b);
    let chars: Vec<char> = expr.replace("sqrt", "√").replace('\u{2061}', "").chars().collect();
    let mut out = String::with_capacity(expr.len());
    let mut i = 0;
    while let Some(&c) = chars.get(i) {
        match c {
            '*' => out.push('·'),
            '^' => {
                // A power of one character (or a digit run, or `-` and digits) becomes
                // superscripts; anything else stays as written.
                let mut j = i + 1;
                if chars.get(j) == Some(&'-') {
                    j += 1;
                }
                while chars.get(j).is_some_and(char::is_ascii_digit) {
                    j += 1;
                }
                if j == i + 1 && chars.get(j).is_some_and(|c| *c == 'n') {
                    j += 1;
                }
                let run: Option<String> =
                    chars.get(i + 1..j).filter(|r| !r.is_empty() && r != &['-']).and_then(|r| r.iter().map(|c| sup(*c)).collect());
                match run {
                    Some(r) => {
                        out.push_str(&r);
                        i = j;
                        continue;
                    }
                    None => out.push('^'),
                }
            }
            _ => out.push(c),
        }
        i += 1;
    }
    out
}

fn draw_legend(vp: Viewport, entries: &[(String, [u8; 3], Vec<f64>)], prims: &mut Vec<PlotPrim>) {
    let row_h = 3.6;
    let max_chars = entries.iter().map(|(s, _, _)| s.chars().count()).max().unwrap_or(4);
    let box_w = (8.0 + max_chars as f64 * 1.35).clamp(16.0, vp.w * 0.55);
    let box_h = 2.4 + entries.len() as f64 * row_h;
    let bx = vp.x0 + vp.w - box_w - 2.0;
    let by = vp.y0 + 2.0;
    prims.push(PlotPrim::Rect { rect: [bx, by, box_w, box_h], fill: Some([255, 255, 255]), stroke: Some(([0xCB, 0xD5, 0xE1], 0.2)) });
    for (i, (label, color, dash)) in entries.iter().enumerate() {
        let cy = by + 1.8 + i as f64 * row_h + 1.1;
        prims.push(PlotPrim::Path {
            pts: vec![(bx + 1.6, cy), (bx + 6.2, cy)],
            closed: false,
            stroke: Some((*color, 0.45)),
            fill: None,
            // The series' pattern, shortened to fit the 4.6 mm sample.
            dash: dash.iter().map(|v| v * 0.6).collect(),
        });
        prims.push(PlotPrim::Text {
            at: (bx + 7.2, cy + 0.7),
            text: label.chars().take(28).collect(),
            height: 2.1,
            bold: false,
            anchor: PlotAnchor::Left,
            color: [0x1E, 0x29, 0x3B],
        });
    }
}

fn blend_white(c: [u8; 3], alpha: f64) -> [u8; 3] {
    let mix = |v: u8| ((f64::from(v) * alpha + 255.0 * (1.0 - alpha)).round() as i32).clamp(0, 255) as u8;
    [mix(c[0]), mix(c[1]), mix(c[2])]
}

pub(crate) fn nice_step(span: f64, target_ticks: f64) -> f64 {
    let raw = (span.abs() / target_ticks.max(1.0)).max(1e-12);
    let exp = raw.log10().floor();
    let pow = 10f64.powf(exp);
    let frac = raw / pow;
    let nice = if frac <= 1.5 {
        1.0
    } else if frac <= 3.0 {
        2.0
    } else if frac <= 7.0 {
        5.0
    } else {
        10.0
    };
    (nice * pow).max(1e-12)
}

fn ticks_in(lo: f64, hi: f64, step: f64) -> Vec<f64> {
    if !(lo.is_finite() && hi.is_finite() && step.is_finite() && step > 0.0) {
        return Vec::new();
    }
    let first = (lo / step).ceil() as i64;
    let last = (hi / step).floor() as i64;
    let count = (last - first + 1).clamp(0, 200) as usize;
    (0..count).map(|k| (first + k as i64) as f64 * step).filter(|v| *v >= lo - 1e-9 * step && *v <= hi + 1e-9 * step).collect()
}
