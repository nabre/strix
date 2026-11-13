//! Unit tests for `strix-plot`.

use strix_calc::{Options, Worksheet};

use crate::{DataPoint, NotableKind, Plot, PlotPrim, RegressionKind, Series, SeriesKind};

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
