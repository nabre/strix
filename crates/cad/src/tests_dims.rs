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
        d.add(Entity::new(Kind::Angle {
            c: P::new(0.0, 0.0),
            a: P::new(10.0, 0.0),
            b,
            r: 5.0,
            r_paper: 0.0,
            text: text.map(Into::into),
            mark: None,
        }))
        .unwrap();
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
            Kind::Dim { a: P::new(0.0, 0.0), b, offset, offset_units: 0.0, orient, text: None, below: None, mark }
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
        offset_units: 0.0,
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
    let texts: Vec<String> = d
        .render(Some(&ws))
        .prims
        .iter()
        .filter_map(|p| if let Prim::Text { text, .. } = p { Some(text.chars().filter(|&c| !is_marker(c)).collect()) } else { None })
        .collect();
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

/// Flip: a linear dimension's line to the other side of its points, the other side of an
/// angle's arc, a level mark under its level; anything else is left alone. Twice is no change.
#[test]
fn flipping_dimensions() {
    let mut d = Drawing::from_json(
        r#"{"entities":[{"type":"dim","a":[0,0],"b":[50,0],"offset":8,"orient":"horizontal"},
            {"type":"angle","c":[0,0],"a":[10,0],"b":[0,10],"r":5,"text":"<>°"},
            {"type":"level","p":[0,0]},{"type":"line","a":[0,0],"b":[1,1]}]}"#,
    )
    .unwrap();
    let before = d.clone();
    let y = |d: &Drawing| d.dim_frame(&d.entities[0], 1.0).unwrap().fa.y;
    assert!((y(&d) - 8.0).abs() < 1e-9);
    assert_eq!(d.flip_dims(&[0, 1, 2, 3, 99]), 3);
    assert!((y(&d) + 8.0).abs() < 1e-9, "below the points");
    let texts: Vec<String> =
        d.render(None).prims.iter().filter_map(|p| if let Prim::Text { text, .. } = p { Some(text.clone()) } else { None }).collect();
    assert!(texts.contains(&"270°".to_string()), "{texts:?}");
    assert!(matches!(d.entities[2].kind, Kind::Level { flip: true, .. }));
    assert_eq!(d.entities[3], before.entities[3]);
    d.flip_dims(&[0, 1, 2]);
    assert_eq!(d, before);
}

/// A dimension's distance and an angle's radius have a scaled part (drawing units: it follows
/// Scale and the drawing's scale) and an annotative part (paper mm: the same in print). Dragging
/// the line or the arc changes the part it always changed; the other stays.
#[test]
fn scaled_and_annotative_parts() {
    let json = r#"{"unit":"mm","scale":50,"entities":[
        {"type":"dim","a":[0,0],"b":[1000,0],"offset":4,"offsetUnits":500,"orient":"horizontal"},
        {"type":"angle","c":[0,0],"a":[1000,0],"b":[0,1000],"r":250,"rPaper":3,"text":"<>°"}]}"#;
    let mut d = Drawing::from_json(json).unwrap();
    // At 1:50, 500 mm is 10 mm of paper: the line 14 mm above the points.
    let line = |d: &Drawing| {
        let k = d.unit.mm() / d.scale;
        d.dim_frame(&d.entities[0], k).unwrap().fa.y
    };
    assert!((line(&d) - 14.0).abs() < 1e-9, "{}", line(&d));
    // At 1:100 the scaled part halves on paper, the paper part stays.
    d.scale = 100.0;
    assert!((line(&d) - 9.0).abs() < 1e-9, "{}", line(&d));
    // The arc: 250 mm at 1:100 is 2.5 mm, plus 3 mm.
    let arc_reach = |d: &Drawing| {
        let mut only = d.clone();
        only.entities.remove(0);
        let r = only.render(None);
        let pts: Vec<(f64, f64)> = r.prims.iter().find_map(|p| if let Prim::Path { pts, .. } = p { Some(pts.clone()) } else { None }).unwrap();
        // The vertex is at paper (0 - origin.0, origin.1 - 0).
        let c = (-r.origin.0, r.origin.1);
        pts.iter().map(|p| ((p.0 - c.0).powi(2) + (p.1 - c.1).powi(2)).sqrt()).fold(0.0, f64::max)
    };
    assert!((arc_reach(&d) - 5.5).abs() < 0.02, "{}", arc_reach(&d));
    // Scale ×2: the scaled parts double, the paper parts stay.
    d.transform(&[0, 1], &Affine::scaling(Pt::new(0.0, 0.0), 2.0));
    assert!(matches!(d.entities[0].kind, Kind::Dim { offset, offset_units, .. } if offset == 4.0 && offset_units == 1000.0));
    assert!(matches!(d.entities[1].kind, Kind::Angle { r, r_paper, .. } if r == 500.0 && r_paper == 3.0));
    // Dragging the dimension line sets its paper part (the style's default); the scaled part
    // stays. 300 mm at 1:100 is 3 mm.
    let grip = |d: &Drawing, i: usize| d.grips(i).into_iter().find(|(g, _)| *g == Grip::DimLine).unwrap().1;
    assert!(d.move_grip(0, Grip::DimLine, grip(&d, 0).plus(Pt::new(0.0, 300.0))));
    assert!(
        matches!(d.entities[0].kind, Kind::Dim { offset, offset_units, .. } if (offset - 7.0).abs() < 1e-9 && offset_units == 1000.0),
        "{:?}",
        d.entities[0].kind
    );
    // The arc too: 500 + 300 units out (3 mm), dragged to twice that, its paper part is 3 + 8 mm.
    assert!(d.move_grip(1, Grip::DimLine, grip(&d, 1).scale(2.0)));
    assert!(matches!(d.entities[1].kind, Kind::Angle { r, r_paper, .. } if r == 500.0 && (r_paper - 11.0).abs() < 1e-6), "{:?}", d.entities[1].kind);
    // With the style's scaled placing, the drawing part changes instead.
    d.dim_style.scaled = true;
    assert!(d.move_grip(0, Grip::DimLine, grip(&d, 0).plus(Pt::new(0.0, 300.0))));
    assert!(
        matches!(d.entities[0].kind, Kind::Dim { offset, offset_units, .. } if (offset - 7.0).abs() < 1e-9 && (offset_units - 1300.0).abs() < 1e-6),
        "{:?}",
        d.entities[0].kind
    );
    assert!(d.move_grip(1, Grip::DimLine, grip(&d, 1).scale(2.0)));
    assert!(
        matches!(d.entities[1].kind, Kind::Angle { r, r_paper, .. } if (r - 2100.0).abs() < 1e-6 && (r_paper - 11.0).abs() < 1e-6),
        "{:?}",
        d.entities[1].kind
    );
    assert!(Drawing::from_json(&d.to_json()).unwrap().dim_style.scaled);
    // Properties: both parts; JSON writes the new keys only when set.
    let keys: Vec<String> = d.fields(0, 0).into_iter().map(|f| f.key).collect();
    assert!(keys.contains(&"offset".into()) && keys.contains(&"offsetUnits".into()));
    d.set_field(1, 0, "radiusPaper", "0").unwrap();
    d.set_field(0, 0, "offsetUnits", "0").unwrap();
    let out = d.to_json();
    assert!(!out.contains("offsetUnits") && !out.contains("rPaper"), "{out}");
    // A paper-only arc (no scaled part) is allowed; a zero radius in all is not.
    d.set_field(1, 0, "radiusPaper", "6").unwrap();
    d.set_field(1, 0, "radius", "0").unwrap();
    assert!(d.set_field(1, 0, "radiusPaper", "0").is_err());
    assert!(matches!(d.entities[1].kind, Kind::Angle { r, r_paper, .. } if r == 0.0 && r_paper == 6.0));
    assert!((arc_reach(&d) - 6.0).abs() < 0.02);
}

/// A parameter in a dimension's text is set as an equation: the value upright with its unit in
/// the Unit style, the symbol in italics with its subscript lowered; plain text stays plain.
#[test]
fn parameters_in_texts_are_set_as_equations() {
    let mut ws = strix_calc::Worksheet::new(strix_calc::Options::default());
    ws.eval_statement("F_k = 12 kN");
    let d = Drawing::default();
    let text = d.label(Some("Load {{F_k:both}}"), 0.0, Some(&ws));
    let s = spans(&text);
    let got: Vec<_> = s.iter().map(|s| (s.text.as_str(), s.kind, s.math, s.symbol, s.sub)).collect();
    assert_eq!(
        got,
        vec![
            ("Load ", SpanKind::Plain, false, false, false),
            ("F", SpanKind::Plain, true, true, false),
            ("k", SpanKind::Plain, true, true, true),
            (" = 12 ", SpanKind::Plain, true, false, false),
            ("kN", SpanKind::Unit, true, false, false),
        ]
    );
    // A missing parameter stays plain and red; braces of a subscript go.
    let s = spans(&d.label(Some("{{nope}} {{F_k:symbol}}"), 0.0, Some(&ws)));
    assert_eq!(s[0].kind, SpanKind::Missing);
    assert!(!s[0].math);
    let s = spans(&format!("{MATH_START}{SYM_START}σ_{{adm}}{SYM_END}{MATH_END}"));
    assert_eq!(s.iter().map(|s| (s.text.as_str(), s.sub)).collect::<Vec<_>>(), vec![("σ", false), ("adm", true)]);
    // The SVG picture: the symbol in italics, the subscript lowered.
    let mut d = Drawing::from_json(r#"{"entities":[{"type":"text","p":[0,0],"value":"{{F_k:symbol}}"}]}"#).unwrap();
    d.resolve(&ws);
    let svg = d.render(Some(&ws)).to_svg([0, 0, 0]);
    assert!(
        svg.contains(r#"<tspan font-style="italic">F</tspan><tspan font-style="italic" baseline-shift="sub" font-size="70%">k</tspan>"#),
        "{svg}"
    );
}
