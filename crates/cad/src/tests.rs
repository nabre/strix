use strix_calc::{Options, Worksheet};

use crate::*;

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn sheet(statements: &[&str]) -> Worksheet {
    let mut ws = Worksheet::new(Options::default());
    for s in statements {
        ws.eval_statement(s);
    }
    ws
}

fn line(a: (f64, f64), b: (f64, f64)) -> Entity {
    Entity::new(Kind::Line { a: P::new(a.0, a.1), b: P::new(b.0, b.1) })
}

#[test]
fn json_is_canonical_and_round_trips() {
    let mut d = Drawing::default();
    d.add(line((0.0, 0.0), (100.0, 0.0))).unwrap();
    d.add(Entity::new(Kind::Circle { c: P(Num::expr("L/2"), Num::new(0.0)), r: Num::new(10.123_456) })).unwrap();
    let json = d.to_json();
    assert!(json.contains(r#""c":["L/2",0.0],"r":10.123"#), "{json}");
    let back = Drawing::from_json(&json).unwrap();
    assert_eq!(back.to_json(), json, "the same drawing gives the same bytes");
    assert_eq!(back.entities[0].id, "e1");
    assert_eq!(back.entities[1].id, "e2");
}

#[test]
fn coordinates_follow_the_calculations() {
    let ws = sheet(&["L = 4.5 m", "h = 300 mm", "α = 30°"]);
    let mut d = Drawing::from_json(
        r#"{"unit": "mm", "scale": 50, "entities": [
            {"type": "line", "a": [0, 0], "b": ["L", 0]},
            {"type": "line", "a": ["L", 0], "b": ["L", "h"]},
            {"type": "arc", "c": [0, 0], "r": "h", "start": 0, "end": "α"},
            {"type": "line", "a": [0, 0], "b": ["x", 0]}
        ]}"#,
    )
    .unwrap();
    let errors = d.resolve(&ws);
    let Kind::Line { b, .. } = &d.entities[0].kind else { panic!() };
    assert!(approx(b.0.value, 4500.0), "4.5 m is 4500 in a mm drawing");
    let Kind::Line { b, .. } = &d.entities[1].kind else { panic!() };
    assert!(approx(b.1.value, 300.0));
    let Kind::Arc { end, .. } = &d.entities[2].kind else { panic!() };
    assert!(approx(end.value, 30.0), "angles come out in degrees");
    assert_eq!(errors.len(), 1);
    assert!(errors[0].1.contains("unknown name x"), "{errors:?}");
    // A length in the wrong kind is refused.
    let mut d = Drawing::from_json(r#"{"entities": [{"type": "point", "p": ["2 kN", 0]}]}"#).unwrap();
    assert!(d.resolve(&ws)[0].1.contains("a coordinate is a length"));
}

#[test]
fn paper_size_follows_the_scale() {
    let mut d = Drawing { unit: Unit::M, scale: 100.0, ..Drawing::default() };
    d.add(line((0.0, 0.0), (5.0, 2.0))).unwrap();
    let r = d.render(None);
    // 5 m at 1:100 is 50 mm, plus 2 mm margins.
    assert!(approx(r.size.0, 54.0) && approx(r.size.1, 24.0), "{:?}", r.size);
    let Prim::Path { pts, width, .. } = &r.prims[0] else { panic!() };
    // y is flipped: the start (0, 0) is at the bottom-left.
    assert!(approx(pts[0].0, 2.0) && approx(pts[0].1, 22.0), "{pts:?}");
    assert!(approx(*width, 0.35), "widths are paper mm, from the layer");
}

#[test]
fn a_dimension_shows_the_size_or_a_parameter() {
    let ws = sheet(&["L = 4.5 m"]);
    let mut d = Drawing { unit: Unit::Mm, scale: 50.0, ..Drawing::default() };
    d.add(Entity::new(Kind::Dim { a: P::new(0.0, 0.0), b: P::new(4500.0, 0.0), offset: 8.0, orient: Orient::Aligned, text: None })).unwrap();
    d.add(Entity::new(Kind::Dim {
        a: P::new(0.0, 0.0),
        b: P::new(4500.0, 0.0),
        offset: -8.0,
        orient: Orient::Aligned,
        text: Some("L = {{L}}".into()),
    }))
    .unwrap();
    d.add(Entity::new(Kind::Dim { a: P::new(0.0, 0.0), b: P::new(30.0, 40.0), offset: 5.0, orient: Orient::Horizontal, text: Some("<> mm".into()) }))
        .unwrap();
    let r = d.render(Some(&ws));
    let texts: Vec<String> = r
        .prims
        .iter()
        .filter_map(|p| if let Prim::Text { text, .. } = p { Some(text.chars().filter(|&c| !is_marker(c)).collect::<String>()) } else { None })
        .collect();
    assert_eq!(texts, ["4500", "L = 4.5 m", "30 mm"]);
    // The unit of a parameter is marked for the Unit style.
    assert_eq!(d.label(Some("L = {{L}}"), 1.0, Some(&ws)), format!("L = 4.5 {UNIT_START}m{UNIT_END}"));
    assert_eq!(d.label(Some("{{nope}}"), 1.0, Some(&ws)), format!("{MISSING_START}[nope?]{MISSING_END}"));
    // Modes, as the prototype's dimension texts: value, symbol, both.
    let plain = |t: &str| fill(t, Some(&ws)).chars().filter(|&c| !is_marker(c)).collect::<String>();
    assert_eq!(plain("{{L}}"), "4.5 m");
    assert_eq!(plain("{{L:value}}"), "4.5 m");
    assert_eq!(plain("{{L:symbol}}"), "L");
    assert_eq!(plain("{{ L : both }}"), "L = 4.5 m");
    assert_eq!(plain("luce {{L:both}}, {{x}}"), "luce L = 4.5 m, [x?]");
    // Unterminated or nested braces never loop.
    assert_eq!(d.label(Some("{{L"), 1.0, Some(&ws)), "{{L");
}

#[test]
fn polylines_with_bulges_become_arcs() {
    // A half circle from (0,0) to (10,0), counter-clockwise: it dips below the chord.
    let pts = bulge_points(Pt::new(0.0, 0.0), Pt::new(10.0, 0.0), 1.0);
    let lowest = pts.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    assert!(approx(lowest, -5.0), "{lowest}");
    assert!(approx(pts.last().unwrap().x, 10.0));
    let d = Drawing::from_json(r#"{"entities": [{"type": "polyline", "pts": [[0,0],[10,0],[10,10]], "bulges": [1, 0], "closed": true}]}"#).unwrap();
    let Prim::Path { pts, closed, .. } = &d.render(None).prims[0] else { panic!() };
    assert!(*closed && pts.len() > 10);
}

#[test]
fn typed_points_like_autocad() {
    let last = Some(Pt::new(10.0, 10.0));
    assert_eq!(parse_point("5,7", last, None), Ok(Pt::new(5.0, 7.0)));
    assert_eq!(parse_point("@5,-2", last, None), Ok(Pt::new(15.0, 8.0)));
    let p = parse_point("@10<90", last, None).unwrap();
    assert!(approx(p.x, 10.0) && approx(p.y, 20.0));
    let p = parse_point("10<0", None, None).unwrap();
    assert!(approx(p.x, 10.0) && approx(p.y, 0.0));
    // Direct distance: 5 towards the pointer.
    let p = parse_point("5", last, Some(Pt::new(10.0, 100.0))).unwrap();
    assert!(approx(p.x, 10.0) && approx(p.y, 15.0));
    assert_eq!(parse_point("5", last, last), Err(InputError::NoDirection));
    assert_eq!(parse_point("@1,1", None, None), Err(InputError::NoPrevious));
    for bad in ["", "a,b", "1,2,3", "10<30<5", "@5", "1e999,0", "NaN,1"] {
        assert!(matches!(parse_point(bad, last, None), Err(InputError::Malformed(_))), "{bad}");
    }
}

#[test]
fn hostile_drawings_are_made_safe() {
    let many = format!(r#"{{"entities": [{}]}}"#, vec![r#"{"type":"point","p":[1e308,-1e308]}"#; MAX_ENTITIES + 10].join(","));
    let d = Drawing::from_json(&many).unwrap();
    assert_eq!(d.entities.len(), MAX_ENTITIES);
    let d = Drawing::from_json(
        r#"{"scale": -5, "layers": [{"name": "x", "width": 1e300}], "entities": [
            {"id": "a", "type": "text", "p": [0,0], "value": "hi", "height": -1},
            {"id": "a", "type": "polyline", "pts": [], "bulges": [1, 2]},
            {"type": "dim", "a": [0,0], "b": [0,0], "offset": 1e99},
            {"type": "circle", "c": [0,0], "r": "1/0"}
        ]}"#,
    )
    .unwrap();
    assert_eq!(d.layers[0].name, "0", "layer 0 always exists");
    assert_ne!(d.entities[0].id, d.entities[1].id, "ids are unique");
    let mut d2 = d.clone();
    let errors = d2.resolve(&sheet(&[]));
    assert_eq!(errors.len(), 1, "{errors:?}");
    let _ = d2.render(None);
    for junk in ["", "{", r#"{"entities": 5}"#, r#"{"entities": [{"type": "nope"}]}"#] {
        assert!(Drawing::from_json(junk).is_err(), "{junk}");
    }
}

#[test]
fn length_parameters_count_in_the_drawing_unit() {
    let ws = sheet(&["L = 4.5 m", "α = 30°", "F = 2 kN"]);
    let mut d = Drawing::from_json(
        r#"{"unit": "m", "entities": [
            {"type": "line", "a": ["L - 0.15", 0], "b": ["L/2 + 0.3", "300 mm"]},
            {"type": "arc", "c": [0, 0], "r": 1, "start": 0, "end": "α"},
            {"type": "point", "p": ["F", 0]}
        ]}"#,
    )
    .unwrap();
    let errors = d.resolve(&ws);
    let Kind::Line { a, b } = &d.entities[0].kind else { panic!() };
    assert!(approx(a.0.value, 4.35) && approx(b.0.value, 2.55) && approx(b.1.value, 0.3), "{a:?} {b:?}");
    let Kind::Arc { end, .. } = &d.entities[1].kind else { panic!() };
    assert!(approx(end.value, 30.0));
    assert_eq!(errors.len(), 1, "a force is no coordinate: {errors:?}");
}

#[test]
fn a_costly_drawing_shares_the_documents_budget() {
    let ws = sheet(&[]);
    // Every coordinate is a big sum: the budget runs out and the rest is reported, quickly.
    let heavy = "∑_(i=1)^9999▒∑_(j=1)^99▒1";
    let entities: Vec<String> = (0..200).map(|_| format!(r#"{{"type": "point", "p": ["{heavy}", 0]}}"#)).collect();
    let mut d = Drawing::from_json(&format!(r#"{{"entities": [{}]}}"#, entities.join(","))).unwrap();
    let started = std::time::Instant::now();
    let errors = d.resolve(&ws);
    assert!(errors.iter().any(|(_, e)| e.contains("too many steps")), "{:?}", errors.first());
    assert!(ws.scope().spent() > 0, "the drawing's work counts in the document's budget");
    assert!(started.elapsed() < std::time::Duration::from_secs(20));
}

#[test]
fn editing_snaps_picks_and_keeps_expressions() {
    let ws = sheet(&["L = 4 m"]);
    let mut d = Drawing::from_json(
        r#"{"unit": "m", "entities": [
        {"type": "line", "a": [0, 0], "b": ["L", 0]},
        {"type": "circle", "c": [2, 2], "r": 0.5}
    ]}"#,
    )
    .unwrap();
    d.resolve(&ws);
    let snaps = d.snap_points();
    assert!(snaps.contains(&(Pt::new(4.0, 0.0), SnapKind::End)));
    assert!(snaps.contains(&(Pt::new(2.0, 0.0), SnapKind::Mid)));
    assert!(snaps.contains(&(Pt::new(2.0, 2.0), SnapKind::Center)));
    assert_eq!(d.hit(Pt::new(1.0, 0.05), 0.1), Some(0));
    assert_eq!(d.hit(Pt::new(2.5, 2.0), 0.1), Some(1));
    assert_eq!(d.hit(Pt::new(3.0, 3.0), 0.1), None);
    assert_eq!(d.in_box(Pt::new(-1.0, -1.0), Pt::new(5.0, 0.5), false), [0]);
    assert_eq!(d.in_box(Pt::new(1.0, 1.0), Pt::new(2.0, 2.0), true), [1]);
    // A move keeps the link to L.
    d.translate(&[0], Pt::new(0.5, 0.0));
    let Kind::Line { b, .. } = &d.entities[0].kind else { panic!() };
    assert_eq!(b.0.expr.as_deref(), Some("L + 0.5"));
    let mut back = Drawing::from_json(&d.to_json()).unwrap();
    back.resolve(&ws);
    let Kind::Line { b, .. } = &back.entities[0].kind else { panic!() };
    assert!(approx(b.0.value, 4.5));
    // Copy, then remove.
    let copies = d.copy(&[1], Pt::new(1.0, 0.0));
    assert_eq!(copies, [2]);
    d.remove(&[0, 2, 99]);
    assert_eq!(d.entities.len(), 1);
    // The paper position of a drawing point.
    let r = back.render(Some(&ws));
    assert!(r.scale > 0.0 && r.origin.1 > 0.0);
}

#[test]
fn texts_take_placeholders_too() {
    let ws = sheet(&["q = 12 kN/m"]);
    let d = Drawing::from_json(r#"{"entities": [{"type": "text", "p": [0, 0], "value": "carico {{q:both}}"}]}"#).unwrap();
    let r = d.render(Some(&ws));
    let Prim::Text { text, .. } = &r.prims[0] else { panic!() };
    assert_eq!(text.chars().filter(|&c| !is_marker(c)).collect::<String>(), "carico q = 12 kN/m");
}
