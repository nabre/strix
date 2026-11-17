//! Unit tests for `strix-plot`.

use strix_calc::{Options, Worksheet};

use crate::{AngleConvention, AngleUnit, DataPoint, NotableKind, Plot, PlotPrim, RegressionKind, Series, SeriesKind};

#[test]
fn poles_and_asymptotes_split_without_vertical_jump_lines() {
    let plot = Plot {
        series: vec![Series {
            kind: SeriesKind::Function {
                expr: "1/x".into(),
                var: "x".into(),
                domain: None,
                roots: false,
                extrema: false,
                tangent_at: None,
                integral: None,
            },
            ..Series::default()
        }],
        ..Plot::default()
    };
    let rendered = plot.render(None);
    let curve_paths: Vec<&Vec<(f64, f64)>> = rendered
        .prims
        .iter()
        .filter_map(|p| match p {
            PlotPrim::Path { pts, stroke: Some((col, _)), .. } if *col == plot.series[0].color => Some(pts),
            _ => None,
        })
        .collect();
    assert!(curve_paths.len() >= 2, "1/x must split into left and right branches across x = 0, got {}", curve_paths.len());
    let mid_x = rendered.viewport[0] + rendered.viewport[2] * 0.5;
    for pts in curve_paths {
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let crosses_zero = (a.0 < mid_x && b.0 > mid_x) || (a.0 > mid_x && b.0 < mid_x);
            assert!(!crosses_zero, "no curve chord may bridge the vertical asymptote at x = 0: {a:?} -> {b:?}");
        }
    }
}

#[test]
fn roots_extrema_tangent_and_integral_are_accurate() {
    let mut ws = Worksheet::new(Options::default());
    ws.eval_statement("k := 3");

    let mut plot = Plot {
        x_min: -3.0,
        x_max: 3.0,
        y_min: -5.0,
        y_max: 10.0,
        series: vec![Series {
            kind: SeriesKind::Function {
                expr: "x^3 - k*x".into(),
                var: "x".into(),
                domain: None,
                roots: true,
                extrema: true,
                tangent_at: Some(1.5),
                integral: Some([0.0, 2.0]),
            },
            ..Series::default()
        }],
        ..Plot::default()
    };

    let r = plot.render(Some(&ws));
    assert!(r.errors.is_empty(), "unexpected errors: {:?}", r.errors);

    // Roots of `x^3 - 3x = 0`: `-√3`, `0`, `+√3`.
    let mut roots: Vec<f64> = r.notables.iter().filter(|n| n.kind == NotableKind::Root).map(|n| n.x).collect();
    roots.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(roots.len(), 3, "roots: {roots:?}");
    assert!((roots[0] + 3.0f64.sqrt()).abs() < 1e-4);
    assert!(roots[1].abs() < 1e-4);
    assert!((roots[2] - 3.0f64.sqrt()).abs() < 1e-4);

    // Local max at `(-1, 2)` and local min at `(1, -2)`.
    let max_pt = r.notables.iter().find(|n| n.kind == NotableKind::Maximum).unwrap();
    let min_pt = r.notables.iter().find(|n| n.kind == NotableKind::Minimum).unwrap();
    assert!((max_pt.x + 1.0).abs() < 1e-3 && (max_pt.y - 2.0).abs() < 1e-3);
    assert!((min_pt.x - 1.0).abs() < 1e-3 && (min_pt.y + 2.0).abs() < 1e-3);

    // Tangent at `x = 1.5`: `f'(x) = 3x² - 3 => f'(1.5) = 3.75`.
    let tan_pt = r.notables.iter().find(|n| n.kind == NotableKind::Tangent).unwrap();
    assert!((tan_pt.slope.unwrap() - 3.75).abs() < 1e-4);

    // Definite integral `∫_0^2 (x³ - 3x) dx = [x⁴/4 - 3x²/2]_0^2 = 4 - 6 = -2`.
    assert_eq!(r.integrals.len(), 1);
    assert!((r.integrals[0].value + 2.0).abs() < 1e-5);

    // Parametric curve reading `k` from Worksheet.
    plot.series.push(Series {
        kind: SeriesKind::Parametric {
            x_expr: "k*cos(t)".into(),
            y_expr: "k*sin(t)".into(),
            param: "t".into(),
            t_min: 0.0,
            t_max: std::f64::consts::TAU,
        },
        ..Series::default()
    });
    let svg = plot.render(Some(&ws)).to_svg();
    assert!(svg.starts_with("<svg ") && svg.ends_with("</svg>"));
}

#[test]
fn experimental_data_error_bars_and_regression_from_worksheet_and_inline() {
    let mut ws = Worksheet::new(Options::default());
    ws.eval_statement("t_vec := [1 s; 2 s; 3 s; 4 s]");
    ws.eval_statement("x_vec := [2.1 ± 0.1 m; 3.9 ± 0.1 m; 6.0 ± 0.2 m; 8.1 ± 0.2 m]");

    let plot = Plot {
        x_min: 0.0,
        x_max: 5.0,
        y_min: 0.0,
        y_max: 10.0,
        x_unit: Some("s".into()),
        y_unit: Some("m".into()),
        series: vec![Series {
            kind: SeriesKind::Data {
                x_col: "t_vec".into(),
                y_col: "x_vec".into(),
                dx_col: None,
                dy_col: None,
                points: Vec::new(),
                regression: RegressionKind::Linear,
                connect: false,
            },
            ..Series::default()
        }],
        ..Plot::default()
    };

    let r = plot.render(Some(&ws));
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    assert_eq!(r.regressions.len(), 1);
    let reg = &r.regressions[0];
    assert!((reg.coeffs[1] - 2.01).abs() < 0.05, "slope ~ 2: {reg:?}");
    assert!(reg.r2 > 0.995, "R² ~ 1: {reg:?}");
    let dots = r.prims.iter().filter(|p| matches!(p, PlotPrim::Dot { .. })).count();
    assert_eq!(dots, 4, "4 experimental data markers expected");

    // Quadratic regression on inline points `y = 1 + 2x + 0.5x²`.
    let quad_plot = Plot {
        x_min: 0.0,
        x_max: 5.0,
        y_min: 0.0,
        y_max: 25.0,
        series: vec![Series {
            kind: SeriesKind::Data {
                x_col: String::new(),
                y_col: String::new(),
                dx_col: None,
                dy_col: None,
                points: (0..=4)
                    .map(|i| {
                        let x = f64::from(i);
                        DataPoint { x, y: 1.0 + 2.0 * x + 0.5 * x * x, dx: Some(0.05), dy: Some(0.1) }
                    })
                    .collect(),
                regression: RegressionKind::Quadratic,
                connect: false,
            },
            ..Series::default()
        }],
        ..Plot::default()
    };
    let qr = quad_plot.render(None);
    assert_eq!(qr.regressions.len(), 1);
    let q = &qr.regressions[0];
    assert!((q.coeffs[0] - 1.0).abs() < 1e-6);
    assert!((q.coeffs[1] - 2.0).abs() < 1e-6);
    assert!((q.coeffs[2] - 0.5).abs() < 1e-6);
    assert!((q.r2 - 1.0).abs() < 1e-9);
}

#[test]
fn student_teacher_dual_mode_preserves_exact_bounds_and_axes() {
    let mut plot = Plot {
        width: 130.0,
        height: 85.0,
        answer: true,
        grid: true,
        minor_grid: true,
        series: vec![Series {
            kind: SeriesKind::Function {
                expr: "sin(x)".into(),
                var: "x".into(),
                domain: None,
                roots: true,
                extrema: true,
                tangent_at: None,
                integral: Some([0.0, std::f64::consts::PI]),
            },
            ..Series::default()
        }],
        ..Plot::default()
    };

    let teacher = plot.render_for(None, true);
    let student = plot.render_for(None, false);

    assert_eq!(teacher.size, student.size);
    assert_eq!(teacher.viewport, student.viewport);
    assert!(student.prims.len() < teacher.prims.len(), "student plot must omit curve/integral/notable primitives");
    let has_series_color = student.prims.iter().any(|p| match p {
        PlotPrim::Path { stroke: Some((c, _)), .. } => *c == plot.series[0].color,
        PlotPrim::Dot { .. } => true,
        _ => false,
    });
    assert!(!has_series_color, "student plot must only show Cartesian plane & grid");

    // JSON round-trip preserves `answer: true` and sanitizes hostile values without panicking.
    let json = plot.to_json();
    let back = Plot::from_json(&json).unwrap();
    assert!(back.answer);
    plot.width = f64::NAN;
    plot.x_min = f64::INFINITY;
    let safe = plot.sanitized().render_for(None, false);
    assert_eq!(safe.size.0, 120.0);
}

#[test]
fn renaming_the_abscissa_renames_the_function_variable() {
    let mut plot = Plot {
        series: vec![Series {
            kind: SeriesKind::Function {
                expr: "2x + exp(x) - x_1".into(),
                var: "x".into(),
                domain: None,
                roots: false,
                extrema: false,
                tangent_at: None,
                integral: None,
            },
            label: "f(x)".into(),
            ..Series::default()
        }],
        ..Plot::default()
    };
    plot.set_x_label("E");
    assert_eq!(plot.series[0].label, "f(E)");
    assert_eq!(plot.x_name(), "E");
    let SeriesKind::Function { expr, var, .. } = &plot.series[0].kind else { panic!("function expected") };
    assert_eq!(var, "E");
    assert_eq!(expr, "2E + exp(E) - x_1");
    // A label that is no name (`Tempo (s)`) leaves the variable `x`... of the old name `E`.
    plot.set_x_label("Tempo (s)");
    assert_eq!(plot.x_name(), "x");
    let SeriesKind::Function { var, .. } = &plot.series[0].kind else { panic!("function expected") };
    assert_eq!(var, "x");
}

fn polar_plot(angles: AngleConvention, unit: AngleUnit, theta: Option<[f64; 2]>) -> Plot {
    Plot {
        angles,
        angle_unit: unit,
        series: vec![Series { kind: SeriesKind::Polar { expr: "2".into(), param: "θ".into(), theta }, ..Series::default() }],
        ..Plot::default()
    }
}

/// The ends of the polar curve's first path, back in plot coordinates.
fn polar_ends(plot: &Plot) -> ((f64, f64), (f64, f64)) {
    let r = plot.render(None);
    let [vx, vy, vw, vh] = r.viewport;
    let to_plot =
        |(px, py): (f64, f64)| (plot.x_min + (px - vx) / vw * (plot.x_max - plot.x_min), plot.y_max - (py - vy) / vh * (plot.y_max - plot.y_min));
    let pts = r
        .prims
        .iter()
        .find_map(|p| match p {
            PlotPrim::Path { pts, stroke: Some((col, _)), .. } if *col == plot.series[0].color && pts.len() > 10 => Some(pts.clone()),
            _ => None,
        })
        .expect("polar curve drawn");
    (to_plot(pts[0]), to_plot(*pts.last().expect("points")))
}

#[test]
fn polar_curves_follow_the_angle_convention_and_unit() {
    // Mathematical, radians, a quarter turn: from (2, 0) counter-clockwise to (0, 2).
    let math = polar_plot(AngleConvention::Math, AngleUnit::Rad, Some([0.0, std::f64::consts::FRAC_PI_2]));
    let (a, b) = polar_ends(&math);
    assert!((a.0 - 2.0).abs() < 0.05 && a.1.abs() < 0.05, "{a:?}");
    assert!(b.0.abs() < 0.05 && (b.1 - 2.0).abs() < 0.05, "{b:?}");
    // Surveying, gon, 0 to 100 gon: from north (0, 2) clockwise to east (2, 0).
    let survey = polar_plot(AngleConvention::Survey, AngleUnit::Gon, Some([0.0, 100.0]));
    let (a, b) = polar_ends(&survey);
    assert!(a.0.abs() < 0.05 && (a.1 - 2.0).abs() < 0.05, "{a:?}");
    assert!((b.0 - 2.0).abs() < 0.05 && b.1.abs() < 0.05, "{b:?}");
    // Degrees: 90° is the same quarter turn as π/2.
    let deg = polar_plot(AngleConvention::Math, AngleUnit::Deg, Some([0.0, 90.0]));
    let (_, b) = polar_ends(&deg);
    assert!(b.0.abs() < 0.05 && (b.1 - 2.0).abs() < 0.05, "{b:?}");
    // The angle is an angle for calc: `sin(θ)` is right in gon too (ρ = sin θ peaks at 100 gon).
    let mut ws = Worksheet::new(Options::default());
    ws.eval_statement("k := 1");
    let mut sine = polar_plot(AngleConvention::Survey, AngleUnit::Gon, Some([100.0, 100.5]));
    if let SeriesKind::Polar { expr, .. } = &mut sine.series[0].kind {
        *expr = "sin(θ)".into();
    }
    let (a, _) = polar_ends(&sine);
    assert!((a.0 - 1.0).abs() < 0.05 && a.1.abs() < 0.05, "{a:?}");
}

#[test]
fn angle_mark_turns_the_conventions_way() {
    // The θ inside the arc sits between the reference axis and the way angles turn: up-right of
    // the origin for both, but nearer the abscissa in mathematics and nearer the ordinate in
    // surveying.
    for (angles, nearer_x) in [(AngleConvention::Math, true), (AngleConvention::Survey, false)] {
        let plot = Plot { angles, angle_mark: true, series: Vec::new(), ..Plot::default() };
        let r = plot.render(None);
        let [vx, vy, vw, vh] = r.viewport;
        let (ox, oy) = (vx + vw * 0.5, vy + vh * 0.5);
        let theta = r
            .prims
            .iter()
            .find_map(|p| match p {
                PlotPrim::Text { at, text, .. } if text == "θ" => Some(*at),
                _ => None,
            })
            .expect("θ drawn");
        let (dx, dy) = (theta.0 - ox, oy - theta.1);
        assert!(dx > 0.0 && dy > 0.0, "{angles:?}: {dx}, {dy}");
        assert_eq!(dx > dy, nearer_x, "{angles:?}: {dx}, {dy}");
    }
}

#[test]
fn axis_names_sit_at_the_arrows() {
    let plot = Plot { x_label: "E".into(), y_label: "N".into(), series: Vec::new(), ..Plot::default() };
    let r = plot.render(None);
    let [vx, vy, vw, vh] = r.viewport;
    let at = |name: &str| {
        r.prims
            .iter()
            .find_map(|p| match p {
                PlotPrim::Text { at, text, .. } if text == name => Some(*at),
                _ => None,
            })
            .expect("axis name drawn")
    };
    let (ex, ey) = at("E");
    assert!((ex - (vx + vw)).abs() < 2.0 && (ey - (vy + vh * 0.5)).abs() < 3.0, "E by the abscissa's arrow: {ex}, {ey}");
    let (nx, ny) = at("N");
    assert!((nx - (vx + vw * 0.5)).abs() < 3.0 && (ny - vy).abs() < 4.0, "N by the ordinate's arrow: {nx}, {ny}");
    // An axis out of view has its name in the margin under the frame.
    let off = Plot { y_min: 1.0, y_max: 5.0, ..plot };
    let r = off.render(None);
    let e = r.prims.iter().find_map(|p| match p {
        PlotPrim::Text { at, text, .. } if text == "E" => Some(*at),
        _ => None,
    });
    assert!(e.is_some_and(|(_, y)| y > r.viewport[1] + r.viewport[3]), "{e:?}");
}

#[test]
fn automatic_ranges_show_the_analysis() {
    let plot = Plot {
        x_min: -1.0,
        x_max: 1.0,
        y_min: -1.0,
        y_max: 1.0,
        auto_range: true,
        series: vec![Series {
            kind: SeriesKind::Function {
                expr: "x^2 - 4".into(),
                var: "x".into(),
                domain: None,
                roots: false,
                extrema: false,
                tangent_at: Some(3.0),
                integral: Some([-3.0, 0.5]),
            },
            ..Series::default()
        }],
        ..Plot::default()
    };
    let r = plot.render(None);
    let [x0, x1, y0, y1] = r.range;
    assert!(x0 <= -3.0 && x1 >= 3.0, "tangent point and integral bound in view: {:?}", r.range);
    assert!(y0 <= -4.0 && y1 >= 5.0, "minimum −4 and f(3) = 5 in view: {:?}", r.range);
    assert!(x1 - x0 < 10.0, "not much wider than needed: {:?}", r.range);
    // Tidy numbers.
    for v in r.range {
        assert_eq!(v, (v * 1e6).round() / 1e6, "{v}");
    }
    // Nothing to fit: the ranges stay as set.
    let empty = Plot { auto_range: true, series: Vec::new(), ..Plot::default() };
    assert_eq!(empty.render(None).range, [-5.0, 5.0, -5.0, 5.0]);
}

#[test]
fn legend_shows_each_formula_by_its_colour() {
    let f = |expr: &str, label: &str, color: [u8; 3]| Series {
        label: label.into(),
        color,
        kind: SeriesKind::Function {
            expr: expr.into(),
            var: "x".into(),
            domain: None,
            roots: false,
            extrema: false,
            tangent_at: None,
            integral: None,
        },
        ..Series::default()
    };
    let plot = Plot { legend: true, series: vec![f("x^2", "", [1, 2, 3]), f("2*sin(x)", "f(x)", [4, 5, 6])], ..Plot::default() };
    let r = plot.render(None);
    let texts: Vec<&str> = r.prims.iter().filter_map(|p| if let PlotPrim::Text { text, .. } = p { Some(text.as_str()) } else { None }).collect();
    assert!(texts.contains(&"y = x²"), "{texts:?}");
    assert!(texts.contains(&"f(x) = 2·sin(x)"), "{texts:?}");
    // Each entry has a line in its series' colour (the curve's own paths aside: short ones).
    for col in [[1, 2, 3], [4, 5, 6]] {
        let swatch = r.prims.iter().any(|p| matches!(p, PlotPrim::Path { pts, stroke: Some((c, _)), .. } if *c == col && pts.len() == 2 && (pts[1].0 - pts[0].0 - 4.6).abs() < 1e-6));
        assert!(swatch, "legend line in {col:?}");
    }
    // Off: no entries.
    let off = Plot { legend: false, ..plot };
    assert!(!off.render(None).prims.iter().any(|p| matches!(p, PlotPrim::Text { text, .. } if text == "y = x²")));
}

#[test]
fn formulas_read_as_text() {
    use crate::render::pretty_formula;
    assert_eq!(pretty_formula("x^2 + 3*x^-1 - x^(1/2)"), "x² + 3·x⁻¹ - x^(1/2)");
    assert_eq!(pretty_formula("sqrt(x) + sin\u{2061}x"), "√(x) + sinx");
    assert_eq!(pretty_formula("e^x + 2^10 + x^n"), "e^x + 2¹⁰ + xⁿ");
    assert_eq!(pretty_formula("x^"), "x^");
}

/// A function series with nothing switched on.
fn function(expr: &str) -> Series {
    Series {
        kind: SeriesKind::Function {
            expr: expr.into(),
            var: "x".into(),
            domain: None,
            roots: false,
            extrema: false,
            tangent_at: None,
            integral: None,
        },
        ..Series::default()
    }
}

/// A function series with its roots and extrema asked for.
fn analysed(expr: &str) -> Series {
    let mut s = function(expr);
    if let SeriesKind::Function { roots, extrema, .. } = &mut s.kind {
        (*roots, *extrema) = (true, true);
    }
    s
}

fn roots_of(r: &crate::PlotRendered) -> Vec<f64> {
    r.notables.iter().filter(|n| n.kind == NotableKind::Root).map(|n| n.x).collect()
}

/// The open paths drawn in `col` (the curve's pieces).
fn curve_paths(r: &crate::PlotRendered, col: [u8; 3]) -> Vec<Vec<(f64, f64)>> {
    r.prims
        .iter()
        .filter_map(|p| match p {
            PlotPrim::Path { pts, stroke: Some((c, _)), closed: false, .. } if *c == col => Some(pts.clone()),
            _ => None,
        })
        .collect()
}

/// The labels under the abscissa's ticks, left to right.
fn x_tick_labels(r: &crate::PlotRendered) -> Vec<String> {
    r.prims
        .iter()
        .filter_map(|p| match p {
            PlotPrim::Text { text, anchor: crate::PlotAnchor::TopCenter, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_range_ending_on_a_tick_has_that_tick() {
    // 0.3 / 0.05 is 5.999…: the tick on the range's end went missing.
    for (lo, hi, first, last) in [(0.0, 0.3, "0", "0.3"), (-0.3, 0.3, "-0.3", "0.3"), (0.0, 0.6, "0", "0.6"), (0.1, 0.7, "0.1", "0.7")] {
        let labels = x_tick_labels(&Plot { x_min: lo, x_max: hi, series: Vec::new(), ..Plot::default() }.render(None));
        assert_eq!(labels.first().map(String::as_str), Some(first), "[{lo}, {hi}]: {labels:?}");
        assert_eq!(labels.last().map(String::as_str), Some(last), "[{lo}, {hi}]: {labels:?}");
    }
}

#[test]
fn tick_labels_tell_the_ticks_apart() {
    // A narrow range far from zero: four digits wrote the same number under every tick.
    for (lo, hi) in [(0.1, 0.1001), (1e7, 1e7 + 10.0), (5.0, 5.0 + 1e-9), (1000.0, 1001.0), (-5.0, 5.0)] {
        let labels = x_tick_labels(&Plot { x_min: lo, x_max: hi, series: Vec::new(), ..Plot::default() }.render(None));
        assert!(labels.len() >= 3, "[{lo}, {hi}]: {labels:?}");
        assert!(labels.windows(2).all(|w| w[0] != w[1]), "[{lo}, {hi}]: {labels:?}");
    }
    // The usual ranges keep their short labels.
    let labels = x_tick_labels(&Plot { series: Vec::new(), ..Plot::default() }.render(None));
    assert_eq!(labels, ["-5", "-4", "-3", "-2", "-1", "0", "1", "2", "3", "4", "5"]);
}

#[test]
fn a_zero_the_curve_only_touches_is_found() {
    // No sample falls on the zero and the sign never changes.
    for (expr, at) in [("(x-1)^2", 1.0), ("(x-0.3)^2", 0.3), ("abs(x-1)", 1.0), ("-(x+2.2)^2", -2.2)] {
        let roots = roots_of(&Plot { series: vec![analysed(expr)], ..Plot::default() }.render(None));
        assert_eq!(roots.len(), 1, "{expr}: {roots:?}");
        assert!((roots[0] - at).abs() < 1e-4, "{expr}: {roots:?}");
    }
    // A curve that stays clear of the axis has none.
    for expr in ["x^2 + 0.01", "abs(x-1) + 0.001", "cos(x) + 1.01"] {
        let roots = roots_of(&Plot { series: vec![analysed(expr)], ..Plot::default() }.render(None));
        assert!(roots.is_empty(), "{expr}: {roots:?}");
    }
}

#[test]
fn a_stretch_of_zeros_is_not_a_zero_per_sample() {
    // floor(x) is 0 on all of [0, 1): no isolated zero to mark there.
    let roots = roots_of(&Plot { series: vec![analysed("floor(x)")], ..Plot::default() }.render(None));
    assert!(roots.len() <= 2, "{roots:?}");
    let roots = roots_of(&Plot { series: vec![analysed("abs(x) - x")], ..Plot::default() }.render(None));
    assert!(roots.len() <= 1, "{roots:?}");
    // Isolated zeros on samples are still found (x = 0 is a sample here).
    assert_eq!(roots_of(&Plot { series: vec![analysed("x^3")], ..Plot::default() }.render(None)), [0.0]);
}

#[test]
fn small_ordinates_have_no_zeros_where_the_curve_is_clear_of_the_axis() {
    // In a view a millionth tall, a curve between 4e-7 and 6e-7 never comes near zero.
    let plot = Plot { y_min: 0.0, y_max: 1e-6, series: vec![analysed("5*10^(-7) + 10^(-7)*sin(x)")], ..Plot::default() };
    let r = plot.render(None);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    assert_eq!(roots_of(&r), Vec::<f64>::new());
    // And one that crosses it has its zeros.
    let plot = Plot { y_min: -1e-6, y_max: 1e-6, series: vec![analysed("10^(-7)*sin(x)")], ..Plot::default() };
    let roots = roots_of(&plot.render(None));
    assert_eq!(roots.len(), 3, "{roots:?}");
}

#[test]
fn steps_are_not_joined_by_risers() {
    // floor(x) and sign(x) jump: no stroke may climb the jump (a tenth of the view and more).
    for expr in ["floor(x)", "sign(x)", "floor(x/2)*2"] {
        let plot = Plot { series: vec![analysed(expr)], ..Plot::default() };
        let r = plot.render(None);
        let h = r.viewport[3];
        let paths = curve_paths(&r, plot.series[0].color);
        for w in paths.iter().flat_map(|pts| pts.windows(2)) {
            assert!((w[1].1 - w[0].1).abs() < 0.05 * h, "{expr}: a riser from {:?} to {:?}", w[0], w[1]);
        }
        assert!(paths.len() >= 2, "{expr}: {} pieces", paths.len());
        // The flat steps have no extrema.
        let extrema = r.notables.iter().filter(|n| n.kind != NotableKind::Root).count();
        assert_eq!(extrema, 0, "{expr}: {:?}", r.notables);
    }
    // Steep but continuous curves stay whole: a vertical tangent, a sharp bend, a fast wave.
    for expr in ["sqrt(x)", "abs(x)", "sin(50*x)", "x^3", "exp(x)"] {
        let plot = Plot { x_min: 0.0, x_max: 5.0, series: vec![function(expr)], ..Plot::default() };
        let r = plot.render(None);
        assert_eq!(curve_paths(&r, plot.series[0].color).len(), 1, "{expr}");
    }
}

fn regression_of(points: &[(f64, f64)], kind: RegressionKind) -> Option<crate::RegressionInfo> {
    let plot = Plot {
        series: vec![Series {
            kind: SeriesKind::Data {
                x_col: String::new(),
                y_col: String::new(),
                dx_col: None,
                dy_col: None,
                points: points.iter().map(|&(x, y)| DataPoint { x, y, dx: None, dy: None }).collect(),
                regression: kind,
                connect: false,
            },
            ..Series::default()
        }],
        ..Plot::default()
    };
    plot.render(None).regressions.into_iter().next()
}

#[test]
fn regression_works_at_any_scale_of_the_abscissa() {
    // Abscissae in SI (nanometres as metres): the fit was refused as if they were all equal.
    let lin = regression_of(&[(1e-9, 1.0), (2e-9, 2.0), (3e-9, 3.0)], RegressionKind::Linear).expect("linear fit");
    assert!((lin.coeffs[1] - 1e9).abs() < 1.0 && lin.coeffs[0].abs() < 1e-6 && (lin.r2 - 1.0).abs() < 1e-9, "{lin:?}");
    let quad = regression_of(&[(1e-9, 1.0), (2e-9, 4.0), (3e-9, 9.0), (4e-9, 16.0)], RegressionKind::Quadratic).expect("quadratic fit");
    assert!((quad.coeffs[2] / 1e18 - 1.0).abs() < 1e-6 && (quad.r2 - 1.0).abs() < 1e-9, "{quad:?}");
    // Years: y = (x − 2022)² + 3 = x² − 4044·x + 4088487.
    let years: Vec<(f64, f64)> = (2018..=2025).map(|y| (f64::from(y), (f64::from(y) - 2022.0).powi(2) + 3.0)).collect();
    let q = regression_of(&years, RegressionKind::Quadratic).expect("quadratic fit");
    assert!((q.coeffs[2] - 1.0).abs() < 1e-6, "{q:?}");
    assert!((q.coeffs[1] + 4044.0).abs() < 1e-3, "{q:?}");
    assert!((q.coeffs[0] - 4_088_487.0).abs() < 1.0, "{q:?}");
    assert!((q.r2 - 1.0).abs() < 1e-9, "{q:?}");
    // Far from zero: an exact parabola is fitted exactly.
    let far: Vec<(f64, f64)> = (0..8).map(|i| (1e6 + f64::from(i), (f64::from(i) - 3.0).powi(2))).collect();
    let q = regression_of(&far, RegressionKind::Quadratic).expect("quadratic fit");
    assert!((q.coeffs[2] - 1.0).abs() < 1e-4 && (q.r2 - 1.0).abs() < 1e-6, "{q:?}");
    // Equal abscissae still have no line, and two distinct ones no parabola.
    assert!(regression_of(&[(1.0, 1.0), (1.0, 2.0), (1.0, 3.0)], RegressionKind::Linear).is_none());
    assert!(regression_of(&[(0.1 + 0.2, 1.0), (0.3, 2.0), (0.3, 3.0)], RegressionKind::Linear).is_none());
    assert!(regression_of(&[(1.0, 1.0), (1.0, 2.0), (2.0, 3.0), (2.0, 5.0)], RegressionKind::Quadratic).is_none());
}

#[test]
fn regression_equations_write_negative_terms_with_a_minus() {
    let q = regression_of(&[(0.0, -1.0), (1.0, -2.0), (2.0, -5.0), (3.0, -10.0)], RegressionKind::Quadratic).expect("quadratic fit");
    assert!(q.equation.starts_with("y = -1.0000x² + 0.0000x - 1.0000"), "{}", q.equation);
    let l = regression_of(&[(0.0, -1.0), (1.0, -3.0), (2.0, -5.0)], RegressionKind::Linear).expect("linear fit");
    assert!(l.equation.starts_with("y = -2.0000x - 1.0000"), "{}", l.equation);
}

#[test]
fn an_extremum_that_cannot_be_evaluated_is_not_reported_at_zero() {
    use crate::features::{find_extrema, find_roots};
    use crate::sample::ScalarEval;
    // The evaluation budget is spent: the searches have no values to go by.
    let mut eval = ScalarEval::new(None, None, None);
    while eval.eval_at("x", "x^2 + 1", 0.5).is_some() {}
    let branch = vec![vec![(-1.0, 2.0), (0.0, 1.0), (1.0, 2.0)]];
    assert!(find_extrema(&mut eval, 0, "x", "x^2 + 1", &branch, 10.0).is_empty());
    let touching = vec![vec![(-1.0, 1.0), (0.1, 0.01), (1.0, 1.0)]];
    assert!(find_roots(&mut eval, 0, "x", "x^2", &touching, (10.0, 10.0)).is_empty());
}

fn data_series(points: &[(f64, f64)]) -> Series {
    Series {
        kind: SeriesKind::Data {
            x_col: String::new(),
            y_col: String::new(),
            dx_col: None,
            dy_col: None,
            points: points.iter().map(|&(x, y)| DataPoint { x, y, dx: None, dy: None }).collect(),
            regression: RegressionKind::None,
            connect: false,
        },
        ..Series::default()
    }
}

#[test]
fn automatic_ranges_take_in_a_lone_point_and_small_abscissae() {
    // One measurement outside the ranges set: the abscissa goes out to it.
    let r = Plot { auto_range: true, series: vec![data_series(&[(20.0, 7.0)])], ..Plot::default() }.render(None);
    assert!(r.range[0] <= -5.0 && r.range[1] >= 20.0, "{:?}", r.range);
    assert!(r.prims.iter().any(|p| matches!(p, PlotPrim::Dot { .. })), "the point is drawn");
    // A tangent point alone, left of them.
    let mut tangent = function("x^2");
    if let SeriesKind::Function { tangent_at, .. } = &mut tangent.kind {
        *tangent_at = Some(-12.0);
    }
    let r = Plot { auto_range: true, series: vec![tangent], ..Plot::default() }.render(None);
    assert!(r.range[0] <= -12.0 && r.range[1] >= 5.0, "{:?}", r.range);
    // Inside them: they stay as set.
    let r = Plot { auto_range: true, series: vec![data_series(&[(3.0, 7.0)])], ..Plot::default() }.render(None);
    assert_eq!(r.range[..2], [-5.0, 5.0]);
    // Measurements in SI (nanometres as metres) span their own width, as the ordinate does.
    let r = Plot { auto_range: true, series: vec![data_series(&[(1e-9, 7e-9), (3e-9, 9e-9)])], ..Plot::default() }.render(None);
    assert!(r.range[0] <= 1e-9 && r.range[1] >= 3e-9 && r.range[1] - r.range[0] < 1e-8, "{:?}", r.range);
}

#[test]
fn a_rise_onto_a_flat_stretch_is_no_extremum() {
    // A staircase only climbs, and x up to 1 then 1 has no maximum to mark.
    for expr in ["floor(10*x)/10", "min(x, 1)", "max(x, -2)"] {
        let r = Plot { series: vec![analysed(expr)], ..Plot::default() }.render(None);
        assert!(r.errors.is_empty(), "{expr}: {:?}", r.errors);
        let extrema: Vec<_> = r.notables.iter().filter(|n| n.kind != NotableKind::Root).collect();
        assert!(extrema.is_empty(), "{expr}: {extrema:?}");
    }
    // A peak halfway between two samples (both at the same height) is still one maximum.
    let r = Plot { series: vec![analysed("1 - (x - 5/192)^2")], ..Plot::default() }.render(None);
    let maxima: Vec<_> = r.notables.iter().filter(|n| n.kind == NotableKind::Maximum).collect();
    assert_eq!(maxima.len(), 1, "{maxima:?}");
    assert!((maxima[0].x - 5.0 / 192.0).abs() < 1e-4 && (maxima[0].y - 1.0).abs() < 1e-6, "{maxima:?}");
    // And a flat valley floor between two slopes is one minimum.
    let r = Plot { series: vec![analysed("max(abs(x) - 1, 0)")], ..Plot::default() }.render(None);
    let minima: Vec<_> = r.notables.iter().filter(|n| n.kind == NotableKind::Minimum).collect();
    assert_eq!(minima.len(), 1, "{minima:?}");
}

#[test]
fn a_legend_on_the_narrowest_plot_does_not_panic() {
    // 20 mm wide leaves a frame 8 mm wide: the legend's box takes what it can of it.
    for width in [20.0, 30.0, 44.0, 45.0, 120.0] {
        let plot = Plot { width, height: 20.0, legend: true, series: vec![function("x^2")], ..Plot::default() };
        let r = plot.render(None);
        assert!(r.prims.iter().any(|p| matches!(p, PlotPrim::Text { text, .. } if text == "y = x²")), "{width}");
        assert!(!r.to_svg().is_empty());
    }
}

#[test]
fn the_tangent_in_a_narrow_range_is_the_curves_slope_there() {
    // A range a millionth wide: the slope was taken across ten times the whole view.
    let mut s = function("sin(10^7*x)");
    if let SeriesKind::Function { tangent_at, .. } = &mut s.kind {
        *tangent_at = Some(0.0);
    }
    let plot = Plot { x_min: -5e-7, x_max: 5e-7, y_min: -1.0, y_max: 1.0, series: vec![s], ..Plot::default() };
    let r = plot.render(None);
    let slope = r.notables.iter().find(|n| n.kind == NotableKind::Tangent).and_then(|n| n.slope).expect("tangent");
    assert!((slope / 1e7 - 1.0).abs() < 1e-3, "{slope}");
}

/// The ordinate of `expr` at `x`, as the plot reads it (its tangent point there).
fn ordinate(plot: &Plot, ws: &Worksheet, expr: &str, x: f64) -> f64 {
    let mut s = function(expr);
    if let SeriesKind::Function { tangent_at, .. } = &mut s.kind {
        *tangent_at = Some(x);
    }
    let r = Plot { series: vec![s], ..plot.clone() }.render(Some(ws));
    assert!(r.errors.is_empty(), "{expr}: {:?}", r.errors);
    r.notables.iter().find(|n| n.kind == NotableKind::Tangent).map(|n| n.y).expect("a tangent point")
}

/// A plain number is plotted as it is, whatever the ordinate's unit (its label, then); a
/// quantity is converted to it.
#[test]
fn a_plain_number_is_plotted_as_is_under_an_axis_unit() {
    let mut ws = Worksheet::new(Options::default());
    ws.eval_statement("L := 30 cm");
    let plot = Plot { x_min: 0.0, x_max: 5.0, y_min: 0.0, y_max: 500.0, y_unit: Some("cm".into()), ..Plot::default() };
    let close = |a: f64, b: f64| (a - b).abs() < 1e-9 * b.abs().max(1.0);
    // Plain numbers, evaluated with units or in the fallback (`L + x` adds a length to a number).
    assert!(close(ordinate(&plot, &ws, "x^2", 2.0), 4.0));
    assert!(close(ordinate(&plot, &ws, "L + x", 2.0), 2.3));
    // Quantities: in the ordinate's unit.
    assert!(close(ordinate(&plot, &ws, "L*x", 2.0), 60.0));
    assert!(close(ordinate(&plot, &ws, "L", 2.0), 30.0));
    let both = Plot { x_unit: Some("cm".into()), ..plot.clone() };
    assert!(close(ordinate(&both, &ws, "L + x", 2.0), 32.0));
    // Without a unit on the ordinate, coherent SI as ever.
    let bare = Plot { y_unit: None, ..plot.clone() };
    assert!(close(ordinate(&bare, &ws, "L*x", 2.0), 0.6));
    assert!(close(ordinate(&bare, &ws, "x^2", 2.0), 4.0));
}

/// The analysis stays inside the series' domain: the area asked for is cut to it, a tangent
/// point outside it is none.
#[test]
fn the_domain_bounds_the_tangent_and_the_integral() {
    let series = |tangent: Option<f64>, area: Option<[f64; 2]>| {
        let mut s = function("x^2");
        if let SeriesKind::Function { domain, tangent_at, integral, .. } = &mut s.kind {
            (*domain, *tangent_at, *integral) = (Some([1.0, 2.0]), tangent, area);
        }
        s
    };
    let render = |s: Series| Plot { series: vec![s], ..Plot::default() }.render(None);
    let r = render(series(None, Some([-3.0, 3.0])));
    let ig = r.integrals.first().expect("the integral over the domain");
    assert_eq!((ig.a, ig.b), (1.0, 2.0));
    assert!((ig.value - 7.0 / 3.0).abs() < 1e-6, "{ig:?}");
    // The shading keeps to the domain too: no filled point left of 1 or right of 2.
    let vp = r.viewport;
    let x_of = |px: f64| -5.0 + (px - vp[0]) / vp[2] * 10.0;
    let filled: Vec<f64> = r
        .prims
        .iter()
        .filter_map(|p| match p {
            PlotPrim::Path { pts, closed: true, fill: Some(_), .. } if pts.len() > 8 => Some(pts.iter().map(|q| x_of(q.0)).collect::<Vec<f64>>()),
            _ => None,
        })
        .flatten()
        .collect();
    assert!(!filled.is_empty());
    assert!(filled.iter().all(|x| (1.0 - 1e-6..=2.0 + 1e-6).contains(x)), "{filled:?}");
    // An area wholly outside the domain is none; one inside it is as asked.
    assert!(render(series(None, Some([-3.0, 0.0]))).integrals.is_empty());
    let r = render(series(None, Some([1.25, 1.75])));
    assert_eq!(r.integrals.first().map(|ig| (ig.a, ig.b)), Some((1.25, 1.75)));
    // The tangent: at a point of the domain (its ends too), not outside it.
    for (x0, there) in [(1.5, true), (1.0, true), (2.0, true), (0.5, false), (3.0, false)] {
        let r = render(series(Some(x0), None));
        assert_eq!(r.notables.iter().any(|n| n.kind == NotableKind::Tangent), there, "tangent at {x0}");
    }
    // Automatic ranges follow: the abscissa is not stretched to bounds outside the domain.
    let r = Plot { auto_range: true, series: vec![series(Some(40.0), Some([-30.0, 3.0]))], ..Plot::default() }.render(None);
    assert!(r.range[0] > -6.0 && r.range[1] < 6.0, "{:?}", r.range);
}

/// Zeros are told apart by the width of the view, not by a fixed distance: a view narrower than
/// a thousandth keeps its distinct zeros.
#[test]
fn zeros_in_a_narrow_view_are_told_apart() {
    let plot = Plot { x_min: -1e-5, x_max: 1e-5, y_min: -1.0, y_max: 1.0, series: vec![analysed("sin(10^6*x)")], ..Plot::default() };
    let r = plot.render(None);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    let mut roots = roots_of(&r);
    roots.sort_by(f64::total_cmp);
    assert_eq!(roots.len(), 7, "{roots:?}");
    for (k, x) in (-3..=3).zip(&roots) {
        assert!((x - f64::from(k) * std::f64::consts::PI * 1e-6).abs() < 1e-10, "{roots:?}");
    }
    let extrema = r.notables.iter().filter(|n| n.kind != NotableKind::Root).count();
    assert_eq!(extrema, 6);
    // At the usual width a zero found twice is still one.
    let r = Plot { series: vec![analysed("(x - 1)^2*(x + 2)")], ..Plot::default() }.render(None);
    let mut roots = roots_of(&r);
    roots.sort_by(f64::total_cmp);
    assert_eq!(roots.len(), 2, "{roots:?}");
}
