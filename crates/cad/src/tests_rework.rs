//! Explode, Join, Match Properties, Stretch and arrays.

use crate::*;

fn kind(d: &Drawing, i: usize) -> &Kind {
    &d.entities[i].kind
}

/// Explode: a polyline becomes its lines and arcs, in place, the vertices' expressions kept; its
/// strip goes with it; other objects stay.
#[test]
fn explode_makes_lines_and_arcs() {
    let mut d = Drawing::from_json(
        r#"{"entities":[{"type":"polyline","pts":[[0,0],["L",0],["L",5]],"bulges":[0,1]},
            {"type":"strip","along":"e1"},{"type":"circle","c":[0,0],"r":1}]}"#,
    )
    .unwrap();
    let made = d.explode(&[0, 2]);
    assert_eq!(made, vec![0, 1]);
    assert!(matches!(kind(&d, 0), Kind::Line { b, .. } if b.0.expr.as_deref() == Some("L")));
    let Kind::Arc { c, r, .. } = kind(&d, 1) else { panic!("{:?}", kind(&d, 1)) };
    assert!(r.value == 2.5 && c.pt().minus(Pt::new(0.0, 2.5)).len() < 1e-9, "{c:?} {r:?}");
    assert_eq!(d.entities.len(), 3, "the strip went, the circle stayed");
    assert!(matches!(kind(&d, 2), Kind::Circle { .. }));
    assert_ne!(d.entities[0].id, d.entities[1].id);
    // A closed rectangle: four lines.
    let mut r = Drawing::from_json(r#"{"entities":[{"type":"polyline","pts":[[0,0],[1,0],[1,1],[0,1]],"closed":true}]}"#).unwrap();
    assert_eq!(r.explode(&[0, 0, 99]).len(), 4);
}

/// Join: lines and arcs meeting end to end (in any order and direction) become one polyline,
/// closed when the ends meet; an arc keeps its bulge; strays stay.
#[test]
fn join_chains_objects_end_to_end() {
    let mut d = Drawing::from_json(
        r#"{"entities":[{"type":"line","a":[0,0],"b":[10,0]},{"type":"line","a":[10,10],"b":[0,10]},
            {"type":"arc","c":[10,5],"r":5,"start":-90,"end":90},{"type":"line","a":[0,0],"b":[0,10]},
            {"type":"line","a":[50,50],"b":[60,50]}]}"#,
    )
    .unwrap();
    let i = d.join(&[0, 1, 2, 3, 4]).unwrap();
    assert_eq!(i, 0);
    assert_eq!(d.entities.len(), 2, "the stray line stays");
    let Kind::Polyline { pts, bulges, closed } = kind(&d, 0) else { panic!() };
    assert!(*closed);
    assert_eq!(pts.len(), 4);
    assert!(bulges.iter().any(|b| (b.abs() - 1.0).abs() < 1e-9), "{bulges:?}");
    // The joined outline's area is the square plus half the disc.
    let curves = kernel::curves(&d.entities[0]);
    assert_eq!(curves.len(), 4);
    // Nothing meeting: an error, nothing changed.
    let mut apart = Drawing::from_json(r#"{"entities":[{"type":"line","a":[0,0],"b":[1,0]},{"type":"line","a":[5,5],"b":[6,5]}]}"#).unwrap();
    let before = apart.to_json();
    assert!(apart.join(&[0, 1]).is_err());
    assert_eq!(apart.to_json(), before);
    assert!(apart.join(&[0]).is_err());
}

/// Match Properties: layer, stroke and the shared look.
#[test]
fn match_properties_copies_the_look() {
    let mut d = Drawing::from_json(
        r#"{"layers":[{"name":"0"},{"name":"A"}],"entities":[
            {"type":"text","layer":"A","stroke":{"color":[255,0,0]},"p":[0,0],"value":"x","height":5},
            {"type":"text","p":[1,1],"value":"y","height":2.5},{"type":"line","a":[0,0],"b":[1,0]}]}"#,
    )
    .unwrap();
    d.match_properties(0, &[1, 2, 7]);
    assert_eq!(d.entities[1].layer, "A");
    assert_eq!(d.entities[2].stroke, d.entities[0].stroke);
    assert!(matches!(kind(&d, 1), Kind::Text { height, .. } if *height == 5.0));
}

/// Stretch: points inside the crossing window move, the rest stay; expressions follow.
#[test]
fn stretch_moves_the_points_inside() {
    let mut d = Drawing::from_json(
        r#"{"entities":[{"type":"polyline","pts":[[0,0],["L",0],["L",5],[0,5]],"closed":true},
            {"type":"dim","a":[0,0],"b":["L",0]},{"type":"circle","c":[20,20],"r":1}]}"#,
    )
    .unwrap();
    let mut ws = strix_calc::Worksheet::new(strix_calc::Options::default());
    ws.eval_statement("L = 10 mm");
    d.resolve(&ws);
    let n = d.stretch(Pt::new(8.0, -1.0), Pt::new(12.0, 6.0), Pt::new(5.0, 0.0));
    assert_eq!(n, 2, "the rectangle and the dimension; the circle is outside");
    let Kind::Polyline { pts, .. } = kind(&d, 0) else { panic!() };
    assert_eq!(pts[0].pt(), Pt::new(0.0, 0.0));
    assert_eq!(pts[1].0.expr.as_deref(), Some("L + 5"));
    assert!(matches!(kind(&d, 1), Kind::Dim { b, .. } if b.0.expr.as_deref() == Some("L + 5")));
    assert_eq!(d.stretch(Pt::new(100.0, 100.0), Pt::new(101.0, 101.0), Pt::new(1.0, 0.0)), 0);
}

/// Arrays: rectangular (columns × rows, spacing) and polar (round a centre, turned).
#[test]
fn arrays_copy_in_rows_and_round() {
    let mut d = Drawing::from_json(r#"{"entities":[{"type":"line","a":[0,0],"b":[1,0]}]}"#).unwrap();
    let copies = d.array_rect(&[0], 3, 2, 10.0, 5.0).unwrap();
    assert_eq!(copies.len(), 5);
    assert!(d.entities.iter().any(|e| matches!(&e.kind, Kind::Line { a, .. } if a.pt() == Pt::new(20.0, 5.0))));
    let mut p = Drawing::from_json(r#"{"entities":[{"type":"line","a":[5,0],"b":[6,0]}]}"#).unwrap();
    assert_eq!(p.array_polar(&[0], Pt::new(0.0, 0.0), 4, 360.0).unwrap().len(), 3);
    assert!(p.entities.iter().any(|e| matches!(&e.kind, Kind::Line { a, .. } if a.pt().minus(Pt::new(0.0, 5.0)).len() < 1e-9)));
    // Half a turn, 3 items: 0°, 90°, 180°.
    let mut h = Drawing::from_json(r#"{"entities":[{"type":"point","p":[1,0]}]}"#).unwrap();
    h.array_polar(&[0], Pt::new(0.0, 0.0), 3, 180.0).unwrap();
    assert!(matches!(kind(&h, 2), Kind::Point { p } if p.pt().minus(Pt::new(-1.0, 0.0)).len() < 1e-9));
    // Limits: errors, never panics or runaway allocations.
    assert!(d.array_rect(&[0], 0, 3, 1.0, 1.0).is_err());
    assert!(d.array_rect(&[0], usize::MAX, usize::MAX, 1.0, 1.0).is_err());
    assert!(d.array_rect(&[0], 2, 2, f64::NAN, 1.0).is_err());
    assert!(d.array_rect(&[], 2, 2, 1.0, 1.0).is_err());
    assert!(p.array_polar(&[0], Pt::new(0.0, 0.0), 1, 360.0).is_err());
    assert!(p.array_polar(&[0], Pt::new(0.0, 0.0), 4, f64::INFINITY).is_err());
    let mut big = Drawing::default();
    for k in 0..100 {
        big.add(Entity::new(Kind::Point { p: P::new(k as f64, 0.0) })).unwrap();
    }
    let all: Vec<usize> = (0..100).collect();
    assert!(big.array_rect(&all, 10, 10, 1.0, 1.0).is_err(), "over the entity limit");
}

/// A maximum size: the scales offered are those that fit; the scale is fitted, or picked by
/// hand among them (kept as long as it fits); old fitted drawings read as fitted.
#[test]
fn a_maximum_size_offers_the_scales_that_fit() {
    let mut d = Drawing::from_json(r#"{"unit":"m","entities":[{"type":"line","a":[0,0],"b":[9,0]}],"padding":0}"#).unwrap();
    assert_eq!(d.fitting_scales(None), standard_scales(), "no limit: every scale");
    d.set_max_size(Some((100.0, 0.0)), None);
    assert_eq!(d.scale, 100.0, "9 m in 100 mm: 1:100");
    let offered = d.fitting_scales(None);
    assert_eq!(offered.first(), Some(&100.0));
    assert!(offered.iter().all(|n| *n >= 100.0));
    assert!(d.set_fixed_scale(50.0, None).is_err(), "1:50 is 180 mm");
    d.set_fixed_scale(200.0, None).unwrap();
    assert!(d.fit.unwrap().fixed);
    d.apply_fit(None);
    assert_eq!(d.scale, 200.0, "picked by hand: the fitting leaves it");
    // A smaller limit the hand-picked scale still meets: kept; one it does not: fitted.
    d.set_max_size(Some((60.0, 0.0)), None);
    assert_eq!((d.scale, d.fit.unwrap().fixed), (200.0, true));
    d.set_max_size(Some((30.0, 0.0)), None);
    assert_eq!((d.scale, d.fit.unwrap().fixed), (500.0, false));
    d.fit_scale(None).unwrap();
    d.set_max_size(None, None);
    assert!(d.fit.is_none() && d.fit_scale(None).is_err());
    assert!(d.set_fixed_scale(f64::NAN, None).is_err());
    let old = Drawing::from_json(r#"{"fit":{"width":50}}"#).unwrap();
    assert!(!old.fit.unwrap().fixed);
    assert!(!old.to_json().contains("fixed"));
}

/// Trim: a cutting line through the vertex where a polyline's arc starts cuts it there, not at
/// the arc's other end too (the vertex seen from the arc's centre a hair before its start).
#[test]
fn trim_cuts_a_polyline_where_its_arc_starts() {
    for k in 0..400 {
        let (s, c) = (0.3 + f64::from(k) * 0.0137).sin_cos();
        if s < 0.05 {
            continue;
        }
        // A line ending on the vertex (10, 0), from below it at every slope: it meets the
        // polyline nowhere else.
        let json = format!(
            r#"{{"entities":[{{"type":"polyline","pts":[[0,0],[10,0],[10,10],[0,10]],"bulges":[0,1,0]}},
                {{"type":"line","a":[{},{}],"b":[10,0]}}]}}"#,
            10.0 - 3.0 * c,
            -3.0 * s
        );
        let mut d = Drawing::from_json(&json).unwrap();
        // Picked on the last side: the arc and that side go, from the vertex on.
        let piece = d.trim_preview(0, Pt::new(5.0, 10.0)).unwrap();
        assert!(piece[0].minus(Pt::new(10.0, 0.0)).len() < 1e-6, "{k}: {:?}", piece[0]);
        d.trim(0, Pt::new(5.0, 10.0)).unwrap();
        let Kind::Polyline { pts, .. } = kind(&d, 0) else { panic!("{k}: {:?}", kind(&d, 0)) };
        assert_eq!(pts.len(), 2, "{k}: {pts:?}");
    }
}

/// Explode: an arrowhead stays on the end it was on when the polyline's arc ran clockwise (the
/// arc made of it runs counter-clockwise, the other way).
#[test]
fn an_exploded_clockwise_arc_keeps_its_arrowhead() {
    let mut d = Drawing::from_json(r#"{"entities":[{"type":"polyline","pts":[[-5,0],[5,0]],"bulges":[-1],"arrows":"end"}]}"#).unwrap();
    let tips = |d: &Drawing| d.arrowhead_triangles(&d.entities[0]).iter().map(|t| t[0]).collect::<Vec<Pt>>();
    let before = tips(&d);
    assert!(before.len() == 1 && before[0].minus(Pt::new(5.0, 0.0)).len() < 1e-9, "{before:?}");
    d.explode(&[0]);
    assert!(matches!(kind(&d, 0), Kind::Arc { .. }));
    let after = tips(&d);
    assert!(after.len() == 1 && after[0].minus(Pt::new(5.0, 0.0)).len() < 1e-9, "{after:?}");
    // Counter-clockwise, the arc runs as the polyline did.
    let mut ccw = Drawing::from_json(r#"{"entities":[{"type":"polyline","pts":[[-5,0],[5,0]],"bulges":[1],"arrows":"end"}]}"#).unwrap();
    ccw.explode(&[0]);
    assert_eq!(ccw.entities[0].arrows, Ends::End);
}

/// Stretch: an arc inside the window whole moves, wherever its centre is; one only partly
/// inside stays.
#[test]
fn stretch_moves_an_arc_inside_whole() {
    // A shallow arc over the top of a circle of radius 100: its centre is far below the window.
    let json = r#"{"entities":[{"type":"arc","c":[0,0],"r":100,"start":80,"end":100}]}"#;
    let mut d = Drawing::from_json(json).unwrap();
    assert_eq!(d.stretch(Pt::new(-30.0, 90.0), Pt::new(30.0, 110.0), Pt::new(5.0, 0.0)), 1);
    assert!(matches!(kind(&d, 0), Kind::Arc { c, .. } if c.pt() == Pt::new(5.0, 0.0)), "{:?}", kind(&d, 0));
    // Its ends inside but its top outside the window: not inside whole.
    let mut d = Drawing::from_json(json).unwrap();
    assert_eq!(d.stretch(Pt::new(-30.0, 90.0), Pt::new(30.0, 99.0), Pt::new(5.0, 0.0)), 0);
    // A whole circle written as an arc moves with its centre, as a circle does.
    let mut d = Drawing::from_json(r#"{"entities":[{"type":"arc","c":[0,0],"r":100,"start":0,"end":360}]}"#).unwrap();
    assert_eq!(d.stretch(Pt::new(-1.0, -1.0), Pt::new(1.0, 1.0), Pt::new(5.0, 0.0)), 1);
}
