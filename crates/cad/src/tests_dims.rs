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

/// A level mark shows its point's height above the datum in metres, signed, two decimals, in
/// the document's number style; a text of its own and parameters work as in dimensions.
#[test]
fn level_marks_show_the_elevation() {
    let label = |unit: &str, y: f64, datum: &str| {
        let d = Drawing::from_json(&format!(r#"{{"unit":"{unit}","entities":[{{"type":"level","p":[0,{y}],"datum":{datum}}}]}}"#)).unwrap();
        let r = d.render(None);
        r.prims.iter().find_map(|p| if let Prim::Text { text, .. } = p { Some(text.clone()) } else { None }).unwrap()
    };
    assert_eq!(label("mm", 3000.0, "0"), "+3.00");
    assert_eq!(label("m", -1.5, "0"), "\u{2212}1.50");
    assert_eq!(label("cm", 120.0, "120"), "±0.00");
    assert_eq!(label("mm", 0.001, "0"), "±0.00", "less than half a centimetre reads zero");
    assert_eq!(label("m", 2.755, "0.5"), "+2.26");
    // A decimal comma, a text of its own with the value and a parameter.
    let mut ws = strix_calc::Worksheet::new(strix_calc::Options { decimal_comma: true, ..strix_calc::Options::default() });
    ws.eval_statement("h = 3 m");
    let mut d = Drawing::from_json(r#"{"unit":"m","entities":[{"type":"level","p":[0,"h"],"text":"<> (h = {{h}})"}]}"#).unwrap();
    d.resolve(&ws);
    let texts: Vec<String> =
        d.render(Some(&ws)).prims.iter().filter_map(|p| if let Prim::Text { text, .. } = p { Some(text.clone()) } else { None }).collect();
    assert_eq!(texts.len(), 1);
    assert!(texts[0].starts_with("+3,00 (h = 3"), "{texts:?}");
    // Round trip, defaults left out; moving it changes the elevation, the datum stays.
    let mut d = Drawing::from_json(r#"{"entities":[{"type":"level","p":[0,0],"flip":true}]}"#).unwrap();
    let json = d.to_json();
    assert!(json.contains(r#""flip":true"#) && !json.contains("datum") && !json.contains("left"), "{json}");
    assert_eq!(Drawing::from_json(&json).unwrap(), d);
    d.translate(&[0], Pt::new(5.0, 2500.0));
    assert_eq!(d.fields(0, 0).iter().find(|f| f.key == "elevation").unwrap().text, "+2.50");
    d.set_field(0, 0, "datum", "1000").unwrap();
    assert_eq!(d.fields(0, 0).iter().find(|f| f.key == "elevation").unwrap().text, "+1.50");
    d.set_field(0, 0, "left", "true").unwrap();
    assert!(matches!(d.entities[0].kind, Kind::Level { left: true, flip: true, .. }));
    // Flipped, the triangle hangs under the level; on the left, the text ends at the mark.
    let r = d.render(None);
    assert!(r.prims.iter().any(|p| matches!(p, Prim::Text { anchor: Anchor::BaselineRight, .. })));
    // Hostile input: nothing drawn, no panic.
    let mut d = Drawing::from_json(r#"{"entities":[{"type":"level","p":[1e300,"nonsense"],"datum":"x"}]}"#).unwrap();
    d.resolve(&strix_calc::Worksheet::new(strix_calc::Options::default()));
    let _ = d.render(None);
    let _ = d.fields(0, 0);
}
