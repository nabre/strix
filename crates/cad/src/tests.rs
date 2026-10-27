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
    let texts: Vec<&str> = r.prims.iter().filter_map(|p| if let Prim::Text { text, .. } = p { Some(text.as_str()) } else { None }).collect();
    assert_eq!(texts, ["4500", "L = 4.5 m", "30 mm"]);
    assert_eq!(d.label(Some("{{nope}}"), 1.0, Some(&ws)), "?");
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
