//! Dimensions: the right-angle symbol, continued and baseline dimensions.

use crate::*;

fn paths(r: &Rendered) -> Vec<&Vec<(f64, f64)>> {
    r.prims.iter().filter_map(|p| if let Prim::Path { pts, .. } = p { Some(pts) } else { None }).collect()
}

/// A right angle without a text of its own is the square on the vertex with a dot in it: no arc,
/// no "90°", no end marks. A text of its own, or an angle a hair off 90°, keeps the arc.
#[test]
fn a_right_angle_is_drawn_as_a_square() {
    let angle = |b: P, text: Option<&str>| {
        let mut d = Drawing::default();
        d.add(Entity::new(Kind::Angle { c: P::new(0.0, 0.0), a: P::new(10.0, 0.0), b, r: 5.0, text: text.map(Into::into), mark: None })).unwrap();
        d.render(None)
    };
    let r = angle(P::new(0.0, 10.0), None);
    assert!(!r.prims.iter().any(|p| matches!(p, Prim::Text { .. })), "{:?}", r.prims);
    assert_eq!(r.prims.iter().filter(|p| matches!(p, Prim::Dot { .. })).count(), 1);
    let square = paths(&r);
    assert_eq!(square.len(), 1, "the square only: {square:?}");
    // Three corners 5 mm out along each side and on the diagonal (paper y down, placed).
    let s = square[0];
    assert_eq!(s.len(), 3);
    let side = |i: usize, j: usize| ((s[i].0 - s[j].0).powi(2) + (s[i].1 - s[j].1).powi(2)).sqrt();
    assert!((side(0, 1) - 5.0).abs() < 1e-9 && (side(1, 2) - 5.0).abs() < 1e-9, "{s:?}");
    // The same angle with its own text, or 89.9°, is an arc with its value.
    let texts =
        |r: &Rendered| r.prims.iter().filter_map(|p| if let Prim::Text { text, .. } = p { Some(text.clone()) } else { None }).collect::<Vec<_>>();
    assert_eq!(texts(&angle(P::new(0.0, 10.0), Some("<>°"))), vec!["90°"]);
    let off = angle(P::new(10.0 * 89.9f64.to_radians().cos(), 10.0 * 89.9f64.to_radians().sin()), None);
    assert_eq!(texts(&off), vec!["89.9°"]);
    // A parametric right angle (sides from expressions) is still square.
    let mut d = Drawing::from_json(r#"{"entities":[{"type":"angle","c":[0,0],"a":["L",0],"b":[0,"L"],"r":4}]}"#).unwrap();
    let mut ws = strix_calc::Worksheet::new(strix_calc::Options::default());
    ws.eval_statement("L = 3 m");
    d.resolve(&ws);
    assert!(!d.render(Some(&ws)).prims.iter().any(|p| matches!(p, Prim::Text { .. })));
}

/// Continue: each new dimension starts at the last one's second point, its line on the same
/// line; Baseline: each starts at the base's first point, a row further out. Both keep the
/// orientation, the look and the first point's expression.
#[test]
fn continued_and_baseline_dimensions() {
    let mut d = Drawing::from_json(r#"{"entities":[{"type":"dim","a":[0,0],"b":["L",0],"offset":8,"orient":"horizontal","mark":"arrow"}]}"#).unwrap();
    let k = 1.0;
    let line_y = |d: &Drawing, i: usize| {
        let f = d.dim_frame(&d.entities[i], k).unwrap();
        assert!((f.fa.y - f.fb.y).abs() < 1e-9);
        f.fa.y
    };
    d.entities[0].kind = match d.entities[0].kind.clone() {
        Kind::Dim { b, offset, orient, mark, .. } => {
            let mut b = b;
            b.0.value = 100.0;
            Kind::Dim { a: P::new(0.0, 0.0), b, offset, orient, text: None, below: None, mark }
        }
        other => other,
    };
    assert_eq!(d.last_linear_dim(), Some(0));
    assert!((line_y(&d, 0) - 8.0).abs() < 1e-9);
    // Continue to x = 250 (and the point a little lower: the line stays).
    let e = d.dim_continue(0, Pt::new(250.0, -3.0)).unwrap();
    let Kind::Dim { a, orient, mark, text, .. } = &e.kind else { panic!("{e:?}") };
    assert_eq!(a.0.expr.as_deref(), Some("L"), "starts on the base's second point, its expression kept");
    assert_eq!((*orient, *mark, text.clone()), (Orient::Horizontal, Some(Mark::Arrow), None));
    d.add(e).unwrap();
    assert!((line_y(&d, 1) - 8.0).abs() < 1e-9, "on the base's line");
    assert_eq!(d.last_linear_dim(), Some(1));
    // Baseline from the first: from x = 0, one row further up.
    let row = d.dim_row();
    let e = d.dim_baseline(0, Pt::new(250.0, 0.0)).unwrap();
    d.add(e).unwrap();
    assert!((line_y(&d, 2) - (8.0 + row)).abs() < 1e-9, "{} vs {}", line_y(&d, 2), 8.0 + row);
    let e = d.dim_baseline(2, Pt::new(400.0, 0.0)).unwrap();
    d.add(e).unwrap();
    assert!((line_y(&d, 3) - (8.0 + 2.0 * row)).abs() < 1e-9);
    // Below the points (a negative offset), the rows go down.
    let mut d = Drawing::default();
    d.add(Entity::new(Kind::Dim {
        a: P::new(0.0, 0.0),
        b: P::new(50.0, 0.0),
        offset: -6.0,
        orient: Orient::Horizontal,
        text: None,
        below: None,
        mark: None,
    }))
    .unwrap();
    let e = d.dim_baseline(0, Pt::new(80.0, 0.0)).unwrap();
    d.add(e).unwrap();
    assert!((line_y(&d, 1) - (-6.0 - d.dim_row())).abs() < 1e-9);
    // At another scale the row is still paper mm.
    let mut d =
        Drawing::from_json(r#"{"unit":"m","scale":100,"entities":[{"type":"dim","a":[0,0],"b":[4,0],"offset":5,"orient":"horizontal"}]}"#).unwrap();
    let k = 10.0;
    let e = d.dim_baseline(0, Pt::new(6.0, 0.0)).unwrap();
    d.add(e).unwrap();
    let y = |i: usize| d.dim_frame(&d.entities[i], k).unwrap().fa.y;
    assert!((y(1) - y(0) - d.dim_row()).abs() < 1e-9, "{} {}", y(0), y(1));
    // Nothing to chain from: not a linear dimension, or the point is the start.
    d.add(Entity::new(Kind::Point { p: P::new(0.0, 0.0) })).unwrap();
    assert!(d.dim_continue(2, Pt::new(1.0, 1.0)).is_none() && d.dim_continue(99, Pt::new(1.0, 1.0)).is_none());
    assert!(d.dim_continue(0, Pt::new(4.0, 0.0)).is_none());
}
