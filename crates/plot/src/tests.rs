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
