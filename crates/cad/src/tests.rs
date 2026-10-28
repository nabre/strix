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
    d.add(Entity::new(Kind::Dim {
        a: P::new(0.0, 0.0),
        b: P::new(4500.0, 0.0),
        offset: 8.0,
        orient: Orient::Aligned,
        text: None,
        below: None,
        mark: None,
    }))
    .unwrap();
    d.add(Entity::new(Kind::Dim {
        a: P::new(0.0, 0.0),
        b: P::new(4500.0, 0.0),
        offset: -8.0,
        orient: Orient::Aligned,
        text: Some("L = {{L}}".into()),
        below: None,
        mark: None,
    }))
    .unwrap();
    d.add(Entity::new(Kind::Dim {
        a: P::new(0.0, 0.0),
        b: P::new(30.0, 40.0),
        offset: 5.0,
        orient: Orient::Horizontal,
        text: Some("<> mm".into()),
        below: None,
        mark: None,
    }))
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

#[test]
fn svg_is_the_drawing_as_seen() {
    let ws = sheet(&["L = 4.5 m"]);
    let d = Drawing::from_json(
        r#"{"unit": "m", "scale": 50, "layers": [{"name": "0"}, {"name": "h", "dash": "dashed", "color": [255, 0, 0]}],
            "entities": [{"type": "line", "a": [0, 0], "b": ["L", 0], "layer": "h"},
                         {"type": "dim", "a": [0, 0], "b": ["L", 0], "offset": 8, "text": "L = {{L}} & {{x}}"},
                         {"type": "text", "p": [1, 1], "value": "<a>", "angle": 30}]}"#,
    )
    .unwrap();
    let mut d = d;
    assert!(d.resolve(&ws).is_empty());
    let svg = d.render(Some(&ws)).to_svg([0x1B, 0x6F, 0x8A]);
    assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""), "{svg}");
    assert!(svg.contains("stroke-dasharray"), "a dashed layer: {svg}");
    assert!(svg.contains("stroke=\"#ff0000\""));
    // Units in the Unit style, missing data in red, text escaped, rotation kept.
    assert!(svg.contains("<tspan fill=\"#1b6f8a\">m</tspan>"), "{svg}");
    assert!(svg.contains("<tspan fill=\"#c01c1c\">[x?]</tspan>"), "{svg}");
    assert!(svg.contains("&amp;") && svg.contains("&lt;a&gt;"));
    assert!(svg.contains("rotate(-30"), "{svg}");
    assert!(!svg.chars().any(is_marker));
    assert!(svg.ends_with("</svg>"));
}

// ---- the geometry kernel: exact constructions and their limit of approximation ----

use crate::kernel::{self, Curve, angle_at, angle_between, canon, deg, intersect, intersect_extended, line_line};

fn seg(ax: f64, ay: f64, bx: f64, by: f64) -> Curve {
    Curve::Seg { a: Pt::new(ax, ay), b: Pt::new(bx, by) }
}

/// A deterministic pseudo-random sequence in [-1, 1).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 11) as f64 / (1u64 << 53) as f64) * 2.0 - 1.0
    }
}

#[test]
fn a_constructed_perpendicular_measures_ninety_degrees() {
    // A slanted segment and a point off it: the foot makes an exact right angle.
    let s = seg(12.345, -7.5, 1234.5, 987.654);
    let p = Pt::new(-300.25, 455.125);
    let feet = s.perpendicular_feet(p);
    assert_eq!(feet.len(), 1);
    let f = feet[0];
    let a = deg(angle_at(f, p, Pt::new(1234.5, 987.654)));
    assert!((a - 90.0).abs() < 1e-11, "{a}");
    // Saved (coordinates to 1e-9) and read back, it still measures 90° to 1e-7°.
    let (fx, fy) = (canon(f.x), canon(f.y));
    let b = deg(angle_at(Pt::new(fx, fy), p, Pt::new(1234.5, 987.654)));
    assert!((b - 90.0).abs() < 1e-7, "{b}");
    // On a circle: both feet lie on the radius through the point.
    let c = Curve::Circle { c: Pt::new(3.0, 4.0), r: 2.5 };
    for q in c.perpendicular_feet(Pt::new(30.0, -12.0)) {
        assert!((q.minus(Pt::new(3.0, 4.0)).len() - 2.5).abs() < 1e-14);
    }
}

#[test]
fn a_tangent_is_square_to_its_radius() {
    let c = Pt::new(-4.75, 10.125);
    let circle = Curve::Circle { c, r: 3.3 };
    let mut rng = Lcg(7);
    for _ in 0..2000 {
        let p = Pt::new(rng.next() * 500.0, rng.next() * 500.0);
        let ts = circle.tangent_points(p);
        if p.minus(c).len() < 3.3 {
            assert!(ts.is_empty());
            continue;
        }
        assert_eq!(ts.len(), 2, "{p:?}");
        for t in ts {
            assert!((t.minus(c).len() - 3.3).abs() < 1e-12, "on the circle");
            let a = deg(angle_at(t, c, p));
            assert!((a - 90.0).abs() < 1e-9, "radius ⟂ tangent: {a}");
        }
    }
    // A point on the circle is its own tangent point.
    let on = c.plus(Pt::new(3.3, 0.0));
    assert_eq!(circle.tangent_points(on), vec![on]);
}

#[test]
fn angles_are_exact_at_every_size() {
    // atan2 keeps tiny angles that acos would round to zero.
    let tiny = angle_between(Pt::new(1.0, 0.0), Pt::new(1.0, 1e-10));
    assert!((tiny - 1e-10).abs() < 1e-24, "{tiny}");
    let near_flat = angle_between(Pt::new(1.0, 0.0), Pt::new(-1.0, 1e-10));
    // Near 180° the limit is π's own spacing in f64 (4.4e-16 rad).
    assert!((std::f64::consts::PI - near_flat - 1e-10).abs() < 1e-15, "{near_flat}");
    assert!((deg(angle_between(Pt::new(2.0, 0.0), Pt::new(0.0, 5.0))) - 90.0).abs() < 1e-13);
    assert!((deg(kernel::angle_ccw(Pt::new(1.0, 0.0), Pt::new(0.0, -1.0))) - 270.0).abs() < 1e-12);
}

#[test]
fn lines_cross_exactly_even_at_grazing_angles() {
    // Two lines through (100, 50) at 1e-7 rad apart.
    let x = Pt::new(100.0, 50.0);
    let t = 1e-7_f64;
    let (p, _, _) = line_line(x.minus(Pt::new(70.0, 0.0)), x.plus(Pt::new(70.0, 0.0)), x.minus(Pt::new(70.0 * t.cos(), 70.0 * t.sin())), x).unwrap();
    assert!(p.minus(x).len() < 1e-6, "{p:?}");
    // Parallel and coincident lines do not cross.
    assert!(line_line(Pt::new(0.0, 0.0), Pt::new(1.0, 1.0), Pt::new(0.0, 1.0), Pt::new(5.0, 6.0)).is_none());
    assert!(line_line(Pt::new(0.0, 0.0), Pt::new(1.0, 1.0), Pt::new(2.0, 2.0), Pt::new(5.0, 5.0)).is_none());
    // Segments cross only within both.
    assert_eq!(intersect(&seg(0.0, 0.0, 10.0, 0.0), &seg(5.0, -1.0, 5.0, 1.0)), vec![Pt::new(5.0, 0.0)]);
    assert!(intersect(&seg(0.0, 0.0, 10.0, 0.0), &seg(15.0, -1.0, 15.0, 1.0)).is_empty());
    // Extended (Extend): the line runs on to the other segment.
    assert_eq!(intersect_extended(&seg(0.0, 0.0, 10.0, 0.0), &seg(15.0, -1.0, 15.0, 1.0)), vec![Pt::new(15.0, 0.0)]);
}

#[test]
fn tangency_is_one_point_within_the_tolerance() {
    let circle = Curve::Circle { c: Pt::new(0.0, 0.0), r: 10.0 };
    // A line touching the circle, and one a hair (below the tolerance) off it: one point.
    for y in [10.0, 10.0 + 1e-12, 10.0 - 1e-12] {
        let pts = intersect(&seg(-50.0, y, 50.0, y), &circle);
        assert_eq!(pts.len(), 1, "{y}");
        assert!((pts[0].len() - 10.0).abs() < 1e-12);
    }
    // Clearly inside: two points, on the circle to the last digits.
    let pts = intersect(&seg(-50.0, 9.999, 50.0, 9.999), &circle);
    assert_eq!(pts.len(), 2);
    for q in &pts {
        assert!((q.len() - 10.0).abs() < 1e-13, "{q:?}");
    }
    // Circles touching outside and inside.
    let a = Curve::Circle { c: Pt::new(0.0, 0.0), r: 3.0 };
    assert_eq!(intersect(&a, &Curve::Circle { c: Pt::new(5.0, 0.0), r: 2.0 }), vec![Pt::new(3.0, 0.0)]);
    assert_eq!(intersect(&a, &Curve::Circle { c: Pt::new(1.0, 0.0), r: 2.0 }), vec![Pt::new(3.0, 0.0)]);
    // Concentric: none.
    assert!(intersect(&a, &Curve::Circle { c: Pt::new(0.0, 0.0), r: 2.0 }).is_empty());
}

#[test]
fn intersections_lie_on_both_curves() {
    let mut rng = Lcg(42);
    for _ in 0..3000 {
        let c1 = Pt::new(rng.next() * 1e3, rng.next() * 1e3);
        let c2 = c1.plus(Pt::new(rng.next() * 50.0, rng.next() * 50.0));
        let (r1, r2) = (1.0 + rng.next().abs() * 40.0, 1.0 + rng.next().abs() * 40.0);
        let (a, b) = (Curve::Circle { c: c1, r: r1 }, Curve::Circle { c: c2, r: r2 });
        for q in intersect(&a, &b) {
            let tol = a.tol().max(b.tol());
            assert!((q.minus(c1).len() - r1).abs() <= tol && (q.minus(c2).len() - r2).abs() <= tol, "{q:?}");
        }
        let s = seg(c2.x - 80.0, c2.y + rng.next() * 30.0, c2.x + 80.0, c2.y + rng.next() * 30.0);
        for q in intersect(&s, &a) {
            assert!((q.minus(c1).len() - r1).abs() <= s.tol().max(a.tol()));
            assert!(s.distance(q) <= s.tol());
        }
    }
}

#[test]
fn arcs_keep_their_extent_and_bulges_are_exact() {
    // A quarter arc from 0° to 90° meets the x axis segment at (r, 0) but not at (−r, 0).
    let arc = Curve::Arc { c: Pt::new(0.0, 0.0), r: 5.0, start: 0.0, sweep: std::f64::consts::FRAC_PI_2 };
    assert_eq!(intersect(&arc, &seg(-10.0, 0.0, 10.0, 0.0)), vec![Pt::new(5.0, 0.0)]);
    // A semicircle bulge (1) from (0, 0) to (10, 0): centre at the midpoint, radius 5.
    let Curve::Arc { c, r, sweep, .. } = Curve::from_bulge(Pt::new(0.0, 0.0), Pt::new(10.0, 0.0), 1.0) else { panic!() };
    assert!(c.minus(Pt::new(5.0, 0.0)).len() < 1e-14 && (r - 5.0).abs() < 1e-14);
    assert!((sweep - std::f64::consts::PI).abs() < 1e-14);
}

#[test]
fn coordinates_are_saved_to_a_nanometre() {
    assert_eq!(canon(0.1 + 0.2), 0.3);
    assert_eq!(canon(-0.0000000001), 0.0);
    assert_eq!(canon(1.234_567_891_4), 1.234_567_891);
    assert_eq!(canon(f64::NAN), 0.0);
    // A drawing keeps a constructed point's nine decimals through JSON.
    let mut d = Drawing::default();
    d.add(Entity::new(Kind::Point { p: P::new(1.0 / 3.0, 2.0 / 3.0) })).unwrap();
    let back = Drawing::from_json(&d.to_json()).unwrap();
    let Kind::Point { p } = &back.entities[0].kind else { panic!() };
    assert!((p.0.value - 1.0 / 3.0).abs() <= 5e-10, "{}", p.0.value);
}

#[test]
fn snaps_are_exact_and_ranked() {
    let d = Drawing::from_json(
        r#"{"entities": [{"type": "line", "a": [0, 0], "b": [100, 0]},
                         {"type": "line", "a": [30, -40], "b": [70, 40]},
                         {"type": "circle", "c": [50, 50], "r": 20}]}"#,
    )
    .unwrap();
    let on = Snaps::default();
    // The two lines cross at (50, 0) exactly.
    let (q, k) = d.snap(Pt::new(50.3, 0.4), 1.0, None, &on).unwrap();
    assert_eq!(k, SnapKind::Intersection);
    assert!(q.minus(Pt::new(50.0, 0.0)).len() < 1e-12);
    // At an end, the end wins over a nearby nearest point.
    assert_eq!(d.snap(Pt::new(0.2, 0.1), 1.0, None, &on).map(|s| s.1), Some(SnapKind::End));
    // Perpendicular from a point to the slanted line: a right angle.
    let from = Pt::new(0.0, 30.0);
    let foot = Curve::Seg { a: Pt::new(30.0, -40.0), b: Pt::new(70.0, 40.0) }.perpendicular_feet(from)[0];
    let (q, k) = d.snap(foot.plus(Pt::new(0.2, 0.0)), 1.0, Some(from), &on).unwrap();
    assert_eq!(k, SnapKind::Perpendicular);
    assert!((deg(angle_at(q, from, Pt::new(70.0, 40.0))) - 90.0).abs() < 1e-10);
    // Tangent from a point to the circle.
    let from = Pt::new(120.0, 50.0);
    let t = Curve::Circle { c: Pt::new(50.0, 50.0), r: 20.0 }.tangent_points(from)[0];
    let (q, k) = d.snap(t.plus(Pt::new(0.1, 0.1)), 1.0, Some(from), &on).unwrap();
    assert_eq!(k, SnapKind::Tangent);
    assert!((deg(angle_at(q, Pt::new(50.0, 50.0), from)) - 90.0).abs() < 1e-10);
    // Quadrant only when asked; the centre from inside the circle.
    assert_ne!(d.snap(Pt::new(70.2, 50.0), 1.0, None, &on).map(|s| s.1), Some(SnapKind::Quadrant));
    let quad = Snaps { quadrant: true, ..on };
    assert_eq!(d.snap(Pt::new(70.2, 50.0), 1.0, None, &quad).map(|s| s.1), Some(SnapKind::Quadrant));
    assert_eq!(d.snap(Pt::new(50.5, 50.5), 1.0, None, &on).map(|s| s.1), Some(SnapKind::Center));
    // Nothing close: no snap; junk input: none, never a panic.
    assert!(d.snap(Pt::new(500.0, 500.0), 1.0, None, &on).is_none());
    assert!(d.snap(Pt::new(f64::NAN, 0.0), 1.0, None, &on).is_none());
    assert!(d.snap(Pt::new(0.0, 0.0), -1.0, None, &on).is_none());
}

#[test]
fn the_picture_has_its_padding() {
    let json = |pad: &str| {
        format!(r#"{{"padding": {pad}, "entities": [{{"type": "line", "a": [0, 0], "b": [40, 0]}}, {{"type": "line", "a": [0, 0], "b": [0, 20]}}]}}"#)
    };
    let d = Drawing::from_json(&json("0")).unwrap();
    let tight = d.render(None).size;
    let d = Drawing::from_json(&json("[1, 2, 3, 4]")).unwrap();
    assert_eq!(d.padding, Padding { top: 1.0, right: 2.0, bottom: 3.0, left: 4.0 });
    let r = d.render(None);
    assert!((r.size.0 - tight.0 - 6.0).abs() < 1e-9 && (r.size.1 - tight.1 - 4.0).abs() < 1e-9, "{:?} {:?}", r.size, tight);
    // Saved as written: one number when even, four otherwise; junk is made safe.
    assert!(d.to_json().contains(r#""padding":[1.0,2.0,3.0,4.0]"#), "{}", d.to_json());
    assert!(Drawing::default().to_json().contains(r#""padding":2.0"#));
    for junk in ["-5", "1e300", "\"x\"", "[1, 2]"] {
        let d = Drawing::from_json(&json(junk)).unwrap();
        for v in [d.padding.top, d.padding.right, d.padding.bottom, d.padding.left] {
            assert!((0.0..=MAX_PADDING).contains(&v), "{junk}: {v}");
        }
    }
}

#[test]
fn grips_change_entities_like_autocad() {
    let mut d = Drawing::from_json(
        r#"{"entities": [{"type": "line", "a": [0, 0], "b": ["L", 0]},
                         {"type": "circle", "c": [50, 50], "r": 10},
                         {"type": "arc", "c": [0, 0], "r": 5, "start": 0, "end": 90},
                         {"type": "polyline", "pts": [[0, 0], [10, 0], [10, 10]], "bulges": [0, 0.5]},
                         {"type": "dim", "a": [0, 0], "b": [10, 0], "offset": 8}]}"#,
    )
    .unwrap();
    assert!(d.resolve(&sheet(&["L = 40"])).is_empty());
    // A line's end keeps its expression; its middle moves the whole line.
    assert!(d.move_grip(0, Grip::B, Pt::new(40.5, 0.0)));
    let Kind::Line { a, b } = &d.entities[0].kind else { panic!() };
    assert_eq!(b.0.expr.as_deref(), Some("L + 0.5"));
    assert!(approx(b.0.value, 40.5) && approx(a.0.value, 0.0));
    assert!(d.move_grip(0, Grip::Mid, Pt::new(20.25, 3.0)));
    let Kind::Line { a, .. } = &d.entities[0].kind else { panic!() };
    assert!(approx(a.1.value, 3.0));
    // A quadrant sets the radius, the centre moves the circle.
    assert!(d.move_grip(1, Grip::Quadrant(1), Pt::new(50.0, 65.0)));
    assert!(matches!(&d.entities[1].kind, Kind::Circle { r, .. } if approx(r.value, 15.0)));
    assert!(!d.move_grip(1, Grip::Quadrant(0), Pt::new(50.0, 50.0)), "a zero radius is refused");
    // An arc's end sets its angle exactly.
    assert!(d.move_grip(2, Grip::ArcEnd, Pt::new(-3.0, 3.0)));
    assert!(matches!(&d.entities[2].kind, Kind::Arc { end, .. } if (end.value - 135.0).abs() < 1e-12));
    // A polyline's arc segment bends through the grip; its straight one moves its vertices.
    assert!(d.move_grip(3, Grip::Segment(1), Pt::new(15.0, 5.0)));
    let Kind::Polyline { pts, bulges, .. } = &d.entities[3].kind else { panic!() };
    assert!((bulges[1] - 1.0).abs() < 1e-12, "a semicircle through (15, 5): {}", bulges[1]);
    assert!(approx(pts[1].0.value, 10.0));
    assert!(d.move_grip(3, Grip::Segment(0), Pt::new(5.0, -2.0)));
    let Kind::Polyline { pts, .. } = &d.entities[3].kind else { panic!() };
    assert!(approx(pts[0].1.value, -2.0) && approx(pts[1].1.value, -2.0));
    assert!(!d.move_grip(3, Grip::Vertex(1), Pt::new(10.0, 10.0)), "a vertex on its neighbour is refused");
    // A dimension's line grip sets its paper distance.
    let line = d.grips(4).into_iter().find(|g| g.0 == Grip::DimLine).unwrap().1;
    assert!(approx(line.y, 8.0), "{line:?}");
    assert!(d.move_grip(4, Grip::DimLine, Pt::new(5.0, -6.0)));
    assert!(matches!(&d.entities[4].kind, Kind::Dim { offset, .. } if approx(*offset, -6.0)));
    // Junk never panics.
    assert!(!d.move_grip(99, Grip::A, Pt::new(0.0, 0.0)));
    assert!(!d.move_grip(0, Grip::Center, Pt::new(0.0, 0.0)));
    assert!(!d.move_grip(0, Grip::A, Pt::new(f64::NAN, 0.0)));
}

fn ids(d: &Drawing) -> Vec<&str> {
    d.entities.iter().map(|e| e.id.as_str()).collect()
}

fn five() -> Drawing {
    let mut d = Drawing::default();
    for i in 0..5 {
        let y = f64::from(i) * 10.0;
        d.add(line((0.0, y), (10.0, y))).unwrap();
    }
    d
}

#[test]
fn draw_order_moves_the_selection_and_returns_where_it_went() {
    let mut d = five();
    assert_eq!(d.reorder(&[1, 3], Order::Front), [3, 4]);
    assert_eq!(ids(&d), ["e1", "e3", "e5", "e2", "e4"]);
    let mut d = five();
    assert_eq!(d.reorder(&[1, 3], Order::Back), [0, 1]);
    assert_eq!(ids(&d), ["e2", "e4", "e1", "e3", "e5"]);
    // One place: each passes the first unselected object; a contiguous run moves together.
    let mut d = five();
    assert_eq!(d.reorder(&[1, 2], Order::Forward), [2, 3]);
    assert_eq!(ids(&d), ["e1", "e4", "e2", "e3", "e5"]);
    let mut d = five();
    assert_eq!(d.reorder(&[1, 3], Order::Backward), [0, 2]);
    assert_eq!(ids(&d), ["e2", "e1", "e4", "e3", "e5"]);
    // Already at the top or bottom: nothing moves.
    let mut d = five();
    assert_eq!(d.reorder(&[4], Order::Forward), [4]);
    assert_eq!(d.reorder(&[0], Order::Backward), [0]);
    assert_eq!(ids(&d), ["e1", "e2", "e3", "e4", "e5"]);
    // Junk indices are ignored.
    assert_eq!(d.reorder(&[99, 2], Order::Front), [4]);
    assert_eq!(d.entities.len(), 5);
}

#[test]
fn hits_are_nearest_first_and_skip_hidden_layers() {
    let mut d = Drawing::default();
    d.add(line((0.0, 0.0), (10.0, 0.0))).unwrap();
    d.add(line((0.0, 1.0), (10.0, 1.0))).unwrap();
    d.add(line((0.0, 0.0), (10.0, 0.0))).unwrap();
    // Two lines on top of each other: the one drawn later first.
    assert_eq!(d.hits(Pt::new(5.0, 0.2), 2.0), [2, 0, 1]);
    d.set_layer(&[2], "Hidden").unwrap();
    d.layers.iter_mut().filter(|l| l.name == "Hidden").for_each(|l| l.visible = false);
    assert_eq!(d.hits(Pt::new(5.0, 0.2), 2.0), [0, 1]);
    assert!(d.hits(Pt::new(50.0, 50.0), 2.0).is_empty());
}

#[test]
fn locked_layers_are_not_picked_and_unprinted_ones_show_only_on_screen() {
    let mut d = Drawing::default();
    d.add(line((0.0, 0.0), (10.0, 0.0))).unwrap();
    d.add(line((0.0, 5.0), (10.0, 5.0))).unwrap();
    d.set_layer(&[1], "Construction").unwrap();
    // Defaults are not written: an old drawing keeps its bytes.
    assert!(!d.to_json().contains("locked") && !d.to_json().contains("print"));
    fn layer(d: &mut Drawing) -> &mut crate::Layer {
        d.layers.iter_mut().find(|l| l.name == "Construction").unwrap()
    }
    layer(&mut d).locked = true;
    assert!(!d.selectable(1) && d.selectable(0));
    assert_eq!(d.hits(Pt::new(5.0, 5.0), 1.0), Vec::<usize>::new());
    layer(&mut d).locked = false;
    layer(&mut d).print = false;
    assert!(d.has_unprinted());
    let paths = |r: &crate::Rendered| r.prims.iter().filter(|p| matches!(p, Prim::Path { .. })).count();
    assert_eq!(paths(&d.render(None)), 1, "the page leaves it out");
    assert_eq!(paths(&d.render_layers(None, true)), 2, "the editor shows it");
    let back = Drawing::from_json(&d.to_json()).unwrap();
    assert!(back.layers.iter().any(|l| l.name == "Construction" && !l.print && !l.locked));
}

#[test]
fn lasso_polygons_and_fences() {
    let mut d = Drawing::default();
    d.add(line((1.0, 1.0), (4.0, 1.0))).unwrap(); // inside
    d.add(line((4.0, 4.0), (20.0, 4.0))).unwrap(); // crosses the edge
    d.add(line((30.0, 30.0), (40.0, 30.0))).unwrap(); // outside
    let tri = [Pt::new(0.0, 0.0), Pt::new(10.0, 0.0), Pt::new(0.0, 10.0)];
    assert_eq!(d.in_polygon(&tri, false), [0]);
    assert_eq!(d.in_polygon(&tri, true), [0, 1]);
    assert!(d.in_polygon(&tri[..2], true).is_empty(), "two points are no polygon");
    // A concave polygon: a line spanning the notch is not wholly inside.
    let u = [
        Pt::new(0.0, 0.0),
        Pt::new(10.0, 0.0),
        Pt::new(10.0, 10.0),
        Pt::new(7.0, 10.0),
        Pt::new(7.0, 2.0),
        Pt::new(3.0, 2.0),
        Pt::new(3.0, 10.0),
        Pt::new(0.0, 10.0),
    ];
    let mut n = Drawing::default();
    n.add(line((1.0, 5.0), (9.0, 5.0))).unwrap();
    assert!(n.in_polygon(&u, false).is_empty());
    assert_eq!(n.in_polygon(&u, true), [0]);
    // A fence takes what it crosses.
    assert_eq!(d.crossing_fence(&[Pt::new(2.0, -1.0), Pt::new(2.0, 2.0), Pt::new(35.0, 31.0)]), [0, 1, 2]);
    assert!(d.crossing_fence(&[Pt::new(-5.0, -5.0)]).is_empty());
}

#[test]
fn select_similar_layers_and_strokes() {
    let mut d = Drawing::default();
    d.add(line((0.0, 0.0), (1.0, 0.0))).unwrap();
    d.add(line((0.0, 1.0), (1.0, 1.0))).unwrap();
    d.add(Entity::new(Kind::Circle { c: P::new(0.0, 0.0), r: Num::new(1.0) })).unwrap();
    d.add(line((0.0, 2.0), (1.0, 2.0))).unwrap();
    d.set_layer(&[3], "Axes").unwrap();
    assert_eq!(d.similar(&[0]), [0, 1], "same family, same layer");
    assert_eq!(d.similar(&[0, 2]), [0, 1, 2]);
    assert!(d.set_layer(&[0], "").is_err());
    // Stroke: own values, then back to ByLayer.
    d.set_stroke(&[0, 1], StrokeEdit { color: Some(Some([255, 0, 0])), width: Some(Some(1e9)), dash: Some(Some(Dash::Dashed)) });
    assert_eq!(d.effective_stroke(0), Some(([255, 0, 0], 10.0, Dash::Dashed)));
    d.set_stroke(&[0], StrokeEdit { color: Some(None), ..StrokeEdit::default() });
    assert_eq!(d.effective_stroke(0).map(|s| s.0), Some([0, 0, 0]));
    assert_eq!(d.effective_stroke(99), None);
    // Layers: a new name, rename (entities follow), remove (entities go to 0).
    let name = d.new_layer_name();
    assert_eq!(name, "Layer1");
    d.ensure_layer(&name).unwrap();
    assert!(d.rename_layer("0", "x").is_err());
    assert!(d.rename_layer("Axes", "Layer1").is_err(), "taken");
    d.rename_layer("Axes", "Centre lines").unwrap();
    assert_eq!(d.entities[3].layer, "Centre lines");
    d.remove_layer("Centre lines").unwrap();
    assert_eq!(d.entities[3].layer, "0");
    assert!(d.remove_layer("0").is_err());
    // The round trip keeps it all.
    assert_eq!(Drawing::from_json(&d.to_json()).unwrap(), d);
}

#[test]
fn selection_options_are_typed_in_english_and_italian() {
    assert_eq!(SelectOption::parse("ALL"), Some(SelectOption::All));
    assert_eq!(SelectOption::parse("t"), Some(SelectOption::All));
    assert_eq!(SelectOption::parse("fp"), Some(SelectOption::WPolygon));
    assert_eq!(SelectOption::parse("cp"), Some(SelectOption::CPolygon));
    assert_eq!(SelectOption::parse(" r "), Some(SelectOption::Remove));
    assert_eq!(SelectOption::parse("previous"), Some(SelectOption::Previous));
    assert_eq!(SelectOption::parse("xyz"), None);
}

#[test]
fn the_properties_focus_steps_through_families_and_objects() {
    let mut d = Drawing::default();
    d.add(Entity::new(Kind::Circle { c: P::new(0.0, 0.0), r: Num::new(1.0) })).unwrap(); // 0
    d.add(line((0.0, 0.0), (1.0, 0.0))).unwrap(); // 1
    d.add(line((0.0, 1.0), (1.0, 1.0))).unwrap(); // 2
    let sel = [0, 1, 2];
    let f = families(&d, &sel);
    assert_eq!(f, [(Family::Line, vec![1, 2]), (Family::Circle, vec![0])]);
    // All: ‹ › go through every object, lines first, then back to all.
    let all = Focus::default();
    assert_eq!(all.scope(&d, &sel).1, [1, 2, 0], "family by family");
    let a = all.step(&d, &sel, true);
    assert_eq!((a.object, a.across), (Some(1), true));
    let b = a.step(&d, &sel, true).step(&d, &sel, true);
    assert_eq!(b.object, Some(0));
    assert_eq!(b.step(&d, &sel, true), Focus::default());
    assert_eq!(all.step(&d, &sel, false).object, Some(0));
    // A family: all of it, then one by one, then all of it again.
    let lines = Focus { family: Some(Family::Line), object: None, across: false };
    assert_eq!(lines.scope(&d, &sel).1, [1, 2]);
    let one = lines.step(&d, &sel, true);
    assert_eq!(one.scope(&d, &sel).1, [1]);
    assert_eq!(one.step(&d, &sel, true).object, Some(2));
    assert_eq!(one.step(&d, &sel, true).step(&d, &sel, true), lines);
    // A family of one object is that object.
    let circle = Focus { family: Some(Family::Circle), object: None, across: false };
    assert_eq!(circle.scope(&d, &sel).0.object, Some(0));
    // The selection changes: a family gone is all; an object gone is its family.
    assert_eq!(circle.scope(&d, &[1, 2]).0, Focus::default());
    let two = Focus { family: Some(Family::Line), object: Some(2), across: false };
    assert_eq!(two.scope(&d, &[1, 2]).1, [2]);
    assert_eq!(two.scope(&d, &[0, 1]).1, [1], "a family of one left");
    assert_eq!(Focus { family: Some(Family::Line), object: Some(99), across: true }.scope(&d, &sel).0, Focus::default());
}

#[test]
fn properties_are_fields_that_take_numbers_or_expressions() {
    let mut d = Drawing::default();
    d.add(line((0.0, 0.0), (3.0, 4.0))).unwrap();
    d.add(Entity::new(Kind::Circle { c: P::new(0.0, 0.0), r: Num::new(2.0) })).unwrap();
    d.add(Entity::new(Kind::Polyline { pts: vec![P::new(0.0, 0.0), P::new(1.0, 0.0)], bulges: vec![], closed: false })).unwrap();
    let get = |d: &Drawing, i: usize, k: &str| d.fields(i, 0).into_iter().find(|f| f.key == k).map(|f| f.text);
    assert_eq!(get(&d, 0, "length").as_deref(), Some("5"));
    assert_eq!(get(&d, 0, "angle").as_deref(), Some("53.130102"));
    // A number, an expression (stays linked), a derived size.
    d.set_field(0, 0, "ax", "1,5").unwrap();
    assert_eq!(get(&d, 0, "ax").as_deref(), Some("1.5"));
    d.set_field(0, 0, "bx", "L/2").unwrap();
    assert_eq!(get(&d, 0, "bx").as_deref(), Some("L/2"));
    let mut d2 = d.clone();
    d2.set_field(0, 0, "ax", "0").unwrap();
    d2.set_field(0, 0, "bx", "3").unwrap();
    d2.set_field(0, 0, "length", "10").unwrap();
    assert_eq!(get(&d2, 0, "length").as_deref(), Some("10"));
    d2.set_field(0, 0, "angle", "90").unwrap();
    assert_eq!(get(&d2, 0, "bx").as_deref(), Some("0"));
    assert!(d2.set_field(0, 0, "length", "-1").is_err());
    assert!(d2.set_field(0, 0, "length", "L").is_err(), "a derived size is a number");
    // Diameter as an expression halves it; a radius is positive.
    d.set_field(1, 0, "diameter", "D").unwrap();
    assert_eq!(get(&d, 1, "r").as_deref(), Some("(D)/2"));
    assert!(d.set_field(1, 0, "r", "0").is_err());
    // A polyline closes only with three points; vertices one at a time.
    assert!(d.set_field(2, 0, "closed", "true").is_err());
    d.set_field(2, 1, "vy", "7").unwrap();
    assert_eq!(d.fields(2, 1)[1].text, "7");
    assert_eq!(d.fields(2, 99)[1].text, "7", "vertex clamped");
    // Junk never panics.
    assert!(d.set_field(0, 0, "nope", "1").is_err());
    assert!(d.set_field(99, 0, "ax", "1").is_err());
    assert!(d.set_field(0, 0, "ax", "").is_err());
    assert!(d.set_field(0, 0, "ax", "inf").is_err());
    assert!(d.fields(99, 0).is_empty());
}

#[test]
fn circles_by_two_and_three_points() {
    use crate::kernel::{circle_2p, circle_3p};
    let (c, r) = circle_2p(Pt::new(0.0, 0.0), Pt::new(10.0, 0.0)).unwrap();
    assert!(approx(c.x, 5.0) && approx(c.y, 0.0) && approx(r, 5.0));
    let (c, r) = circle_3p(Pt::new(5.0, 0.0), Pt::new(0.0, 5.0), Pt::new(-5.0, 0.0)).unwrap();
    assert!(approx(c.x, 0.0) && approx(c.y, 0.0) && approx(r, 5.0));
    // Far from the origin the centred form keeps the digits.
    let o = 1e6;
    let (c, r) = circle_3p(Pt::new(o + 3.0, o), Pt::new(o, o + 3.0), Pt::new(o - 3.0, o)).unwrap();
    assert!((c.x - o).abs() < 1e-6 && (c.y - o).abs() < 1e-6 && approx(r, 3.0));
    assert!(circle_3p(Pt::new(0.0, 0.0), Pt::new(1.0, 1.0), Pt::new(2.0, 2.0)).is_none(), "in line");
    assert!(circle_2p(Pt::new(1.0, 1.0), Pt::new(1.0, 1.0)).is_none());
}

#[test]
fn circles_tangent_to_curves() {
    use crate::kernel::{Curve, circle_ttr, circle_ttt};
    let x_axis = Curve::Seg { a: Pt::new(0.0, 0.0), b: Pt::new(10.0, 0.0) };
    let y_axis = Curve::Seg { a: Pt::new(0.0, 0.0), b: Pt::new(0.0, 10.0) };
    // Tan, Tan, Radius in the corner the picks show.
    let (c, r) = circle_ttr(&x_axis, Pt::new(3.0, 0.0), &y_axis, Pt::new(0.0, 3.0), 2.0).unwrap();
    assert!(approx(c.x, 2.0) && approx(c.y, 2.0) && approx(r, 2.0), "{c:?}");
    let (c, _) = circle_ttr(&x_axis, Pt::new(-3.0, 0.0), &y_axis, Pt::new(0.0, -3.0), 2.0).unwrap();
    assert!(approx(c.x, -2.0) && approx(c.y, -2.0), "{c:?}");
    // A circle and a line: the circle of radius 1 touching both, outside the circle.
    let circle = Curve::Circle { c: Pt::new(0.0, 4.0), r: 2.0 };
    let (c, _) = circle_ttr(&x_axis, Pt::new(3.0, 0.0), &circle, Pt::new(2.0, 3.0), 1.5).unwrap();
    assert!(approx(c.y, 1.5) && approx(c.minus(Pt::new(0.0, 4.0)).len(), 3.5) && c.x > 0.0, "{c:?}");
    // Tan, Tan, Tan on a 3-4-5 triangle: its incircle (radius 1, centre (1, 1)).
    let hyp = Curve::Seg { a: Pt::new(4.0, 0.0), b: Pt::new(0.0, 3.0) };
    let x = Curve::Seg { a: Pt::new(0.0, 0.0), b: Pt::new(4.0, 0.0) };
    let y = Curve::Seg { a: Pt::new(0.0, 0.0), b: Pt::new(0.0, 3.0) };
    let (c, r) = circle_ttt([(&x, Pt::new(1.0, 0.0)), (&y, Pt::new(0.0, 1.0)), (&hyp, Pt::new(2.0, 1.5))]).unwrap();
    assert!((c.x - 1.0).abs() < 1e-7 && (c.y - 1.0).abs() < 1e-7 && (r - 1.0).abs() < 1e-7, "{c:?} {r}");
}

#[test]
fn lines_tangent_to_two_circles() {
    use crate::kernel::{Curve, line_tan_tan};
    let a = Curve::Circle { c: Pt::new(0.0, 0.0), r: 1.0 };
    let b = Curve::Circle { c: Pt::new(10.0, 0.0), r: 1.0 };
    // Picked on top: the outer tangent y = 1.
    let (t1, t2) = line_tan_tan(&a, Pt::new(0.0, 1.0), &b, Pt::new(10.0, 1.0)).unwrap();
    assert!(approx(t1.y, 1.0) && approx(t2.y, 1.0) && approx(t1.x, 0.0) && approx(t2.x, 10.0), "{t1:?} {t2:?}");
    // Picked top then bottom: an inner tangent, square to both radii.
    let (t1, t2) = line_tan_tan(&a, Pt::new(0.0, 1.0), &b, Pt::new(10.0, -1.0)).unwrap();
    assert!(t1.y > 0.0 && t2.y < 0.0);
    let dir = t2.minus(t1);
    assert!((t1.x * dir.x + t1.y * dir.y).abs() < 1e-9, "square to the first radius");
    // A line is not a circle.
    let l = Curve::Seg { a: Pt::new(0.0, 0.0), b: Pt::new(1.0, 0.0) };
    assert!(line_tan_tan(&l, Pt::new(0.0, 0.0), &b, Pt::new(10.0, 1.0)).is_none());
}

#[test]
fn dimension_style_marks_texts_below_and_angles() {
    let mut d = Drawing::default();
    // The default style is not written: old drawings keep their bytes.
    assert!(!d.to_json().contains("dimStyle"));
    d.add(Entity::new(Kind::Dim {
        a: P::new(0.0, 0.0),
        b: P::new(100.0, 0.0),
        offset: 8.0,
        orient: Orient::Aligned,
        text: None,
        below: Some("L".into()),
        mark: Some(Mark::Arrow),
    }))
    .unwrap();
    let r = d.render(None);
    let filled = r.prims.iter().filter(|p| matches!(p, Prim::Path { fill: true, closed: true, .. })).count();
    assert_eq!(filled, 2, "an arrowhead at each end");
    let texts: Vec<(&str, Anchor)> =
        r.prims.iter().filter_map(|p| if let Prim::Text { text, anchor, .. } = p { Some((text.as_str(), *anchor)) } else { None }).collect();
    assert!(texts.contains(&("100", Anchor::BottomCenter)) && texts.contains(&("L", Anchor::TopCenter)), "{texts:?}");
    // The style's text height reaches the dimension; a hostile size is clamped.
    d.dim_style.text = 1e9;
    d.sanitize();
    assert_eq!(d.dim_style.text, 50.0);
    assert!(Drawing::from_json(&d.to_json()).unwrap().dim_style.text == 50.0);
    // An angular dimension of a right angle reads 90°.
    let mut d = Drawing::default();
    d.add(Entity::new(Kind::Angle { c: P::new(0.0, 0.0), a: P::new(10.0, 0.0), b: P::new(0.0, 10.0), r: 20.0, text: None, mark: None })).unwrap();
    let r = d.render(None);
    assert!(r.prims.iter().any(|p| matches!(p, Prim::Text { text, .. } if text == "90°")), "{:?}", r.prims);
    // Hostile radius: nothing drawn, no panic.
    d.entities[0].kind = Kind::Angle { c: P::new(0.0, 0.0), a: P::new(0.0, 0.0), b: P::new(0.0, 0.0), r: f64::NAN, text: None, mark: None };
    d.sanitize();
    let _ = d.render(None);
}

/// Review cases: Tan, Tan, Tan in closed form chooses the circle the picks mean, including
/// incircles far from the origin and circles enclosing the given ones.
#[test]
fn tan_tan_tan_follows_the_picks_exactly() {
    use crate::kernel::{Curve, circle_ttt};
    for o in [0.0, 1e6, 1e8] {
        let x = Curve::Seg { a: Pt::new(o, o), b: Pt::new(o + 4.0, o) };
        let y = Curve::Seg { a: Pt::new(o, o), b: Pt::new(o, o + 3.0) };
        let hyp = Curve::Seg { a: Pt::new(o + 4.0, o), b: Pt::new(o, o + 3.0) };
        let picks = [(&x, Pt::new(o + 3.0, o)), (&y, Pt::new(o, o + 1.0)), (&hyp, Pt::new(o + 2.0, o + 1.5))];
        let (c, r) = circle_ttt(picks).unwrap();
        let tol = 1e-15 * o.max(1.0) * 64.0;
        assert!((c.x - o - 1.0).abs() <= tol.max(1e-12) && (c.y - o - 1.0).abs() <= tol.max(1e-12) && (r - 1.0).abs() <= 1e-12, "{o}: {c:?} {r}");
    }
    // Three unit circles: picked on their outer sides, the circle around them; on their inner
    // sides, the one between them.
    let centres = [Pt::new(0.0, 0.0), Pt::new(4.0, 0.0), Pt::new(2.0, 2.0 * 3f64.sqrt())];
    let g = Pt::new(2.0, 2.0 / 3f64.sqrt());
    let circles = centres.map(|c| Curve::Circle { c, r: 1.0 });
    let pick = |k: usize, outer: bool| {
        let u = centres[k].minus(g).unit();
        centres[k].plus(u.scale(if outer { 1.0 } else { -1.0 }))
    };
    let around = circle_ttt([(&circles[0], pick(0, true)), (&circles[1], pick(1, true)), (&circles[2], pick(2, true))]).unwrap();
    let between = circle_ttt([(&circles[0], pick(0, false)), (&circles[1], pick(1, false)), (&circles[2], pick(2, false))]).unwrap();
    let r0 = 4.0 / 3f64.sqrt();
    assert!((around.1 - (r0 + 1.0)).abs() < 1e-12 && around.0.minus(g).len() < 1e-12, "{around:?}");
    assert!((between.1 - (r0 - 1.0)).abs() < 1e-12 && between.0.minus(g).len() < 1e-12, "{between:?}");
}

/// Review cases: the tolerance follows the feature, not the distance from the origin.
#[test]
fn far_from_the_origin_small_features_stay_exact() {
    use crate::kernel::{Curve, intersect};
    let (x0, y0) = (5e5, 5e6);
    // Two r = 10 circles 3 apart meet twice, as they do at the origin.
    let a = Curve::Circle { c: Pt::new(x0, y0), r: 10.0 };
    let b = Curve::Circle { c: Pt::new(x0 + 3.0, y0), r: 10.0 };
    assert_eq!(intersect(&a, &b).len(), 2);
    // A 2 mm circle and a line 4 mm away do not touch.
    let small = Curve::Circle { c: Pt::new(x0, y0), r: 2.0 };
    let line = Curve::Seg { a: Pt::new(x0 - 10.0, y0 + 4.0), b: Pt::new(x0 + 10.0, y0 + 4.0) };
    assert!(intersect(&small, &line).is_empty());
    // At the coordinate limit a line missing an r = 10 circle by 0.5 misses it.
    let far = 1e9 - 100.0;
    let c = Curve::Circle { c: Pt::new(far, far), r: 10.0 };
    let l = Curve::Seg { a: Pt::new(far - 20.0, far + 10.5), b: Pt::new(far + 20.0, far + 10.5) };
    assert!(intersect(&c, &l).is_empty());
}

/// Review cases: a nearly straight bulge is its chord; the tessellation ends on the vertex.
#[test]
fn nearly_straight_bulges_are_straight() {
    use crate::kernel::{Curve, intersect};
    let (a, b) = (Pt::new(0.0, 0.0), Pt::new(10.0, 0.0));
    assert!(matches!(Curve::from_bulge(a, b, 1e-10), Curve::Seg { .. }));
    let chord = Curve::from_bulge(a, b, 1e-10);
    assert!(intersect(&chord, &Curve::Seg { a: Pt::new(0.0, 20.0), b: Pt::new(10.0, 20.0) }).is_empty());
    assert_eq!(intersect(&chord, &Curve::Seg { a: Pt::new(5.0, -1.0), b: Pt::new(5.0, 1.0) }), vec![Pt::new(5.0, 0.0)]);
    for bulge in [1e-12, 1e-300, 0.5, -3.0, 1e3] {
        assert_eq!(crate::geom::bulge_points(a, b, bulge).last(), Some(&b), "{bulge}");
    }
}

/// Review cases: Tan, Tan, Radius with a radius that cannot touch both has no circle; a
/// tangent line must touch the arcs, not only their circles; near-collinear points make no
/// circle beyond the drawing.
#[test]
fn constructions_refuse_what_is_not_there() {
    use crate::kernel::{Curve, circle_3p, circle_ttr, line_tan_tan};
    let x_axis = Curve::Seg { a: Pt::new(-50.0, 0.0), b: Pt::new(50.0, 0.0) };
    let circle = Curve::Circle { c: Pt::new(0.0, 10.0), r: 5.0 };
    assert!(circle_ttr(&x_axis, Pt::new(0.0, 0.0), &circle, Pt::new(0.0, 5.0), 2.499995).is_none());
    let (c, r) = circle_ttr(&x_axis, Pt::new(0.0, 0.0), &circle, Pt::new(0.0, 5.0), 2.5).unwrap();
    assert!(c.minus(Pt::new(0.0, 2.5)).len() < 1e-6 && r == 2.5, "{c:?}");
    // Arcs facing away from each other: no tangent touches both arcs.
    let left = Curve::Arc { c: Pt::new(0.0, 0.0), r: 1.0, start: 3.0, sweep: 0.5 };
    let right = Curve::Arc { c: Pt::new(10.0, 0.0), r: 1.0, start: 3.0, sweep: 0.5 };
    if let Some((t1, t2)) = line_tan_tan(&left, Pt::new(-1.0, 0.0), &right, Pt::new(9.0, 0.0)) {
        assert!(left.distance(t1) < 1e-9 && right.distance(t2) < 1e-9, "{t1:?} {t2:?}");
    }
    assert!(circle_3p(Pt::new(0.0, 0.0), Pt::new(500.0, 1e-9), Pt::new(1000.0, 0.0)).is_none());
}

/// Review cases: selection works on the exact curves; a crossing window takes what passes
/// through it; locked and hidden layers stay out; window and lasso agree on their borders.
#[test]
fn selection_is_exact() {
    let mut d = Drawing::default();
    d.add(line((-10.0, 5.0), (20.0, 5.0))).unwrap(); // through the box, no end inside
    d.add(Entity::new(Kind::Circle { c: P::new(0.0, 100.0), r: Num::new(10000.0) })).unwrap();
    assert_eq!(d.in_box(Pt::new(0.0, 0.0), Pt::new(10.0, 10.0), true), [0]);
    assert!(d.in_box(Pt::new(0.0, 0.0), Pt::new(10.0, 10.0), false).is_empty());
    // A click right on a big circle is on it (its drawn polygon is 5 units off there).
    let on = Pt::new(10000.0 * 0.3f64.cos(), 100.0 + 10000.0 * 0.3f64.sin());
    assert_eq!(d.hits(on, 1e-6), [1]);
    // A window edge cutting the circle by a hair does not hold it whole.
    assert!(!d.in_box(Pt::new(-10000.0 + 0.01, -9901.0), Pt::new(10001.0, 10101.0), false).contains(&1));
    assert!(d.in_box(Pt::new(-10001.0, -9901.0), Pt::new(10001.0, 10101.0), false).contains(&1));
    // A locked layer is not taken by a window.
    d.set_layer(&[0], "Locked").unwrap();
    d.layers.iter_mut().filter(|l| l.name == "Locked").for_each(|l| l.locked = true);
    assert!(d.in_box(Pt::new(0.0, 0.0), Pt::new(10.0, 10.0), true).is_empty());
    // A point on the border is inside for both.
    let sq = [Pt::new(0.0, 0.0), Pt::new(10.0, 0.0), Pt::new(10.0, 10.0), Pt::new(0.0, 10.0)];
    for p in [Pt::new(10.0, 5.0), Pt::new(0.0, 0.0), Pt::new(5.0, 10.0)] {
        assert!(point_in_polygon(p, &sq), "{p:?}");
    }
    assert!(!point_in_polygon(Pt::new(10.000001, 5.0), &sq));
}

/// Review cases: a segment grip bends the arc through the point; a two-vertex closed polyline
/// (DXF's circle) has both its segments everywhere.
#[test]
fn arc_grips_pass_through_the_point_and_closed_pairs_close() {
    use crate::kernel::{Curve, curves};
    let mut d = Drawing::default();
    d.add(Entity::new(Kind::Polyline { pts: vec![P::new(-1.0, 0.0), P::new(1.0, 0.0)], bulges: vec![0.3], closed: false })).unwrap();
    for to in [Pt::new(0.5, -0.8), Pt::new(0.9, -0.3), Pt::new(0.0, 2.0)] {
        assert!(d.move_grip(0, Grip::Segment(0), to));
        let c = curves(&d.entities[0]);
        assert!(c[0].distance(to) < 1e-9, "{to:?}: {:?}", c[0]);
    }
    let circle = Entity::new(Kind::Polyline { pts: vec![P::new(-1.0, 0.0), P::new(1.0, 0.0)], bulges: vec![1.0, 1.0], closed: true });
    let c = curves(&circle);
    assert_eq!(c.len(), 2);
    assert!(c.iter().all(|k| matches!(k, Curve::Arc { r, .. } if (r - 1.0).abs() < 1e-12)));
}

/// Review case: curves are drawn to a chord tolerance, not a fixed count: a big printed circle
/// stays within 0.01 mm of the true one, a tiny one is not drawn with hundreds of segments.
#[test]
fn tessellation_follows_a_chord_tolerance() {
    use crate::geom::arc_segments;
    let tau = std::f64::consts::TAU;
    let n = arc_segments(100.0, tau, Some(0.01));
    assert!((222..=224).contains(&n), "{n}");
    let step = tau / n as f64;
    assert!(100.0 * (1.0 - (step / 2.0).cos()) <= 0.01);
    assert_eq!(arc_segments(0.5, tau, Some(0.01)), 16);
    assert_eq!(arc_segments(1e9, tau, Some(1e-12)), 4096, "capped");
    assert_eq!(arc_segments(f64::NAN, tau, Some(0.01)), 96, "the fixed count without a usable tolerance");
}

/// Fit: the largest standard scale at which the drawing, dimensions included, fits its space;
/// it follows a parameter that grows the drawing.
#[test]
fn the_scale_fits_the_space() {
    assert_eq!(standard_scales().first(), Some(&0.1));
    let ws = sheet(&["L = 4.5 m"]);
    let mut d =
        Drawing::from_json(r#"{"unit":"mm","entities":[{"type":"line","a":[0,0],"b":["L",0]},{"type":"dim","a":[0,0],"b":["L",0]}]}"#).unwrap();
    d.fit = Some(Fit { width: 160.0, height: 0.0, fixed: false });
    d.resolve(&ws);
    d.apply_fit(Some(&ws));
    // 4500 mm in 160 mm with the padding: 1:25 would be 180 mm wide, 1:50 is 90 mm.
    assert_eq!(d.scale, 50.0);
    assert!(d.render(Some(&ws)).size.0 <= 160.0);
    let ws = sheet(&["L = 9 m"]);
    d.resolve(&ws);
    d.apply_fit(Some(&ws));
    assert_eq!(d.scale, 100.0, "twice as long: the next scale that fits");
    // Kept in the source; a hostile size is clamped; no space, no fit.
    let back = Drawing::from_json(&d.to_json()).unwrap();
    assert_eq!(back.fit, Some(Fit { width: 160.0, height: 0.0, fixed: false }));
    let hostile = Drawing::from_json(r#"{"fit":{"width":-5,"height":1e300}}"#).unwrap();
    assert_eq!(hostile.fit, Some(Fit { width: 0.0, height: 5000.0, fixed: false }));
    assert!(Drawing::from_json(r#"{"fit":{"width":0,"height":0}}"#).unwrap().fit.is_none());
}

/// A hatched strip follows its line or polyline: a band `width` paper mm wide on its side,
/// hatched across at its angle, `spacing` apart; copied with its path it follows the copy,
/// deleted with it it goes; nonsense sizes are clamped.
#[test]
fn strips_follow_their_path() {
    let ws = sheet(&["L = 2 m"]);
    let mut d = Drawing::from_json(
        r#"{"unit":"m","scale":50,"entities":[
            {"id":"g","type":"polyline","pts":[[0,0],["L",0],["L",1]]},
            {"type":"strip","along":"g","width":4,"side":"right","pattern":"along","angle":90,"spacing":2}]}"#,
    )
    .unwrap();
    d.resolve(&ws);
    let strip = d.entities[1].clone();
    // 1:50 in metres: 4 paper mm is 0.2 m, below the path (right of it going along x).
    let s = d.strip_shape(&strip).unwrap();
    assert_eq!(s.path.len(), 3);
    assert!(s.edge.iter().zip(&s.path).all(|(e, p)| e.minus(*p).len() >= 0.2 - 1e-9));
    assert!((s.edge[0].y + 0.2).abs() < 1e-9, "{:?}", s.edge);
    // Lines square to the path, 2 mm (0.1 m) apart, each crossing the whole band.
    let lines = d.strip_lines(&strip);
    assert!(lines.len() >= 25, "{}", lines.len());
    for (a, b) in &lines {
        assert!((b.minus(*a).len() - 0.2).abs() < 1e-6 || b.minus(*a).len() < 0.2 * 4.0 + 1e-9, "{a:?} {b:?}");
    }
    // The path grows with L: so does the strip.
    let ws = sheet(&["L = 4 m"]);
    d.resolve(&ws);
    assert!(d.strip_lines(&strip).len() > lines.len());
    // Drawn as thin lines; solid as one filled band.
    assert!(d.render(None).prims.iter().filter(|p| matches!(p, Prim::Path { closed: false, .. })).count() > 25);
    // Solid: lines close and wide enough to cover the band (islands and concave shapes too).
    d.set_field(1, 0, "pattern", "solid").unwrap();
    let solid: Vec<f64> =
        d.render(None).prims.iter().filter_map(|p| if let Prim::Path { width, closed: false, .. } = p { Some(*width) } else { None }).collect();
    assert!(solid.len() > 100 && solid.iter().all(|w| (*w - 0.28).abs() < 1e-9 || *w > 0.2), "{}", solid.len());
    // Copied together, the copy follows the copied path; deleting the path takes the strip.
    let copies = d.copy(&[0, 1], Pt::new(10.0, 0.0));
    let new_path = d.entities[copies[0]].id.clone();
    assert!(matches!(&d.entities[copies[1]].kind, Kind::Strip { along, .. } if *along == new_path));
    d.remove(&[0]);
    assert!(d.entities.iter().all(|e| !matches!(&e.kind, Kind::Strip { along, .. } if along == "g")));
    // Hostile sizes are clamped; a strip with no path draws nothing.
    let mut h = Drawing::from_json(r#"{"entities":[{"type":"strip","along":"nope","width":1e300,"spacing":0,"angle":-5}]}"#).unwrap();
    assert!(matches!(h.entities[0].kind, Kind::Strip { width, spacing, angle, .. } if width == 100.0 && spacing == 0.2 && angle == 5.0));
    assert!(h.strip_shape(&h.entities[0].clone()).is_none());
    let _ = h.render(None);
    h.sanitize();
}

/// Review: a strip id cut inside a multi-byte character, and a hatch far too fine for its path,
/// neither panic nor run away.
#[test]
fn hostile_strips_stay_bounded() {
    // 70 two-byte characters: byte 64 falls inside the 33rd.
    let id: String = "é".repeat(70);
    let d = Drawing::from_json(&format!(r#"{{"entities":[{{"type":"strip","along":"{id}"}}]}}"#)).unwrap();
    assert!(matches!(&d.entities[0].kind, Kind::Strip { along, .. } if along.chars().count() == 64));
    // A kilometre-long path in millimetres at 1:0.001 (a 1000 km band on paper), hatched 0.2 mm
    // apart: coarser, and quickly.
    let mut d = Drawing::from_json(
        r#"{"unit":"mm","scale":0.001,"entities":[{"id":"g","type":"line","a":[0,0],"b":[1000000,0]},{"type":"strip","along":"g","spacing":0,"pattern":"cross"}]}"#,
    )
    .unwrap();
    d.sanitize();
    let strip = d.entities[1].clone();
    let start = std::time::Instant::now();
    let lines = d.strip_lines(&strip);
    assert!(lines.len() <= 40_000, "{}", lines.len());
    assert!(start.elapsed().as_secs() < 5);
    // Coordinates near the limit with a tiny step: the loop still ends.
    let far = Drawing::from_json(
        r#"{"unit":"mm","scale":1,"entities":[{"id":"g","type":"line","a":[999999000,0],"b":[1000000000,0]},{"type":"strip","along":"g","spacing":0.2}]}"#,
    )
    .unwrap();
    let _ = far.strip_lines(&far.entities[1].clone());
}

/// A dimension is picked where it is drawn (its dimension line, extension lines, text), not on
/// the line it measures.
#[test]
fn dimensions_are_picked_where_they_are_drawn() {
    let mut d = Drawing::default();
    d.add(Entity::new(Kind::Dim {
        a: P::new(0.0, 0.0),
        b: P::new(100.0, 0.0),
        offset: 8.0,
        orient: Orient::Aligned,
        text: None,
        below: None,
        mark: None,
    }))
    .unwrap();
    assert_eq!(d.hits(Pt::new(50.0, 8.0), 0.5), [0], "on the dimension line");
    assert_eq!(d.hits(Pt::new(50.0, 10.5), 0.1), [0], "on its text");
    assert_eq!(d.hits(Pt::new(0.0, 4.0), 0.5), [0], "on an extension line");
    assert!(d.hits(Pt::new(50.0, 0.0), 0.5).is_empty(), "not on the line it measures");
    // A window around the drawn dimension takes it.
    assert_eq!(d.in_box(Pt::new(-5.0, -5.0), Pt::new(105.0, 20.0), false), [0]);
}

/// A hatch fills the region a click is in, islands left out (even-odd), with a conventional
/// pattern; it follows its boundaries and goes with them.
#[test]
fn hatches_fill_regions_with_islands() {
    let mut d = Drawing::from_json(
        r#"{"entities":[
            {"id":"out","type":"polyline","pts":[[0,0],[100,0],[100,60],[0,60]],"closed":true},
            {"id":"hole","type":"circle","c":[50,30],"r":10}]}"#,
    )
    .unwrap();
    // A click between the rectangle and the circle: the rectangle with the circle as island.
    let ids = d.hatch_boundary_at(Pt::new(10.0, 10.0)).unwrap();
    assert_eq!(ids, ["out", "hole"]);
    // A click inside the circle: the circle alone.
    assert_eq!(d.hatch_boundary_at(Pt::new(50.0, 30.0)).unwrap(), ["hole"]);
    assert!(d.hatch_boundary_at(Pt::new(200.0, 0.0)).is_none());
    d.add(Entity::new(Kind::Hatch { boundary: ids, pattern: Pattern::Lines, angle: 0.0, spacing: 5.0 })).unwrap();
    let hatch = d.entities[2].clone();
    let lines = d.hatch_lines(&hatch);
    // Horizontal lines 5 mm apart; none crosses the island.
    assert!(lines.len() >= 11, "{}", lines.len());
    for (a, b) in &lines {
        let mid = a.plus(*b).scale(0.5);
        assert!(mid.minus(Pt::new(50.0, 30.0)).len() > 10.0 - 1e-6, "{a:?} {b:?}");
        assert!((0.0..=100.0).contains(&a.x) && (0.0..=100.0).contains(&b.x));
    }
    // Steel doubles the lines; dashed cuts them; cross adds the other way.
    for (p, more) in [(Pattern::Steel, true), (Pattern::Cross, true)] {
        d.entities[2].kind = Kind::Hatch { boundary: vec!["out".into(), "hole".into()], pattern: p, angle: 0.0, spacing: 5.0 };
        assert_eq!(d.hatch_lines(&d.entities[2].clone()).len() > lines.len(), more, "{p:?}");
    }
    d.entities[2].kind = Kind::Hatch { boundary: vec!["out".into(), "hole".into()], pattern: Pattern::Dashed, angle: 0.0, spacing: 5.0 };
    assert!(d.hatch_lines(&d.entities[2].clone()).iter().all(|(a, b)| b.minus(*a).len() <= 10.0 + 1e-6));
    // Moving the rectangle moves the hatch; deleting the island deletes it.
    d.translate(&[0], Pt::new(1000.0, 0.0));
    assert!(d.hatch_lines(&d.entities[2].clone()).iter().all(|(a, _)| a.x >= 1000.0 - 1e-6 || a.x <= 100.0));
    d.remove(&[1]);
    assert!(d.entities.iter().all(|e| !matches!(e.kind, Kind::Hatch { .. })));
    // Hostile: a pattern far too fine for a huge region stays bounded.
    let big = Drawing::from_json(
        r#"{"unit":"m","scale":0.001,"entities":[{"id":"o","type":"polyline","pts":[[0,0],[1000,0],[1000,1000],[0,1000]],"closed":true},
            {"type":"hatch","boundary":["o"],"pattern":"cross","spacing":0}]}"#,
    )
    .unwrap();
    assert!(big.hatch_lines(&big.entities[1].clone()).len() <= 40_000);
}

/// Scale, rotate and mirror: one affine transform; expressions kept (and still evaluating to
/// the right place), radii scaled, paper sizes untouched, arcs turned the other way by a mirror.
#[test]
fn scale_rotate_and_mirror_keep_expressions() {
    let ws = sheet(&["L = 10 mm"]);
    let mut d = Drawing::from_json(
        r#"{"entities":[{"type":"line","a":[0,0],"b":["L",0]},{"type":"circle","c":[5,5],"r":"L/5"},
            {"type":"arc","c":[0,0],"r":1,"start":0,"end":90},{"type":"text","p":[1,1],"value":"T","height":2.5}]}"#,
    )
    .unwrap();
    d.resolve(&ws);
    d.transform(&[0, 1, 2, 3], &Affine::scaling(Pt::new(0.0, 0.0), 2.0));
    let Kind::Line { b, .. } = &d.entities[0].kind else { panic!() };
    assert_eq!(b.0.expr.as_deref(), Some("(L)*2"));
    assert!(matches!(&d.entities[1].kind, Kind::Circle { r, .. } if r.expr.as_deref() == Some("(L/5)*2")));
    assert!(matches!(&d.entities[3].kind, Kind::Text { height, .. } if *height == 2.5), "paper sizes stay");
    // Re-resolved, the expression lands where the transform put it.
    d.resolve(&ws);
    let Kind::Line { b, .. } = &d.entities[0].kind else { panic!() };
    assert!((b.pt().x - 20.0).abs() < 1e-9);
    // Rotate a quarter turn about the origin.
    d.transform(&[0], &Affine::rotation(Pt::new(0.0, 0.0), 90.0));
    d.resolve(&ws);
    let Kind::Line { b, .. } = &d.entities[0].kind else { panic!() };
    assert!(b.pt().minus(Pt::new(0.0, 20.0)).len() < 1e-9, "{:?}", b.pt());
    // Mirror in the y axis: the arc from 0° to 90° becomes 90° to 180°.
    d.transform(&[2], &Affine::mirror(Pt::new(0.0, 0.0), Pt::new(0.0, 1.0)).unwrap());
    assert!(matches!(&d.entities[2].kind, Kind::Arc { start, end, .. } if (start.value - 90.0).abs() < 1e-9 && (end.value - 180.0).abs() < 1e-9));
    assert!(Affine::mirror(Pt::new(1.0, 1.0), Pt::new(1.0, 1.0)).is_none());
}

/// Offset: a line moved square to itself towards the click, a circle concentric, a polyline
/// with an arc kept concentric (bulge kept) and its corner mitred.
#[test]
fn offsets_go_towards_the_click() {
    let d = Drawing::from_json(
        r#"{"entities":[{"type":"line","a":[0,0],"b":[10,0]},{"type":"circle","c":[0,0],"r":5},
            {"type":"polyline","pts":[[0,0],[10,0],[10,10]],"bulges":[0,0]},
            {"type":"polyline","pts":[[-5,0],[5,0]],"bulges":[1]}]}"#,
    )
    .unwrap();
    let line = d.offset_entity(0, 2.0, Pt::new(5.0, -3.0)).unwrap();
    assert!(matches!(&line.kind, Kind::Line { a, b } if a.pt() == Pt::new(0.0, -2.0) && b.pt() == Pt::new(10.0, -2.0)));
    assert!(matches!(&d.offset_entity(1, 2.0, Pt::new(20.0, 0.0)).unwrap().kind, Kind::Circle { r, .. } if r.value == 7.0));
    assert!(matches!(&d.offset_entity(1, 2.0, Pt::new(1.0, 0.0)).unwrap().kind, Kind::Circle { r, .. } if r.value == 3.0));
    assert!(d.offset_entity(1, 6.0, Pt::new(1.0, 0.0)).is_none(), "no circle of negative radius");
    // An L offset inwards (towards the inside of the corner): the corner moves diagonally.
    let l = d.offset_entity(2, 1.0, Pt::new(8.0, 2.0)).unwrap();
    let Kind::Polyline { pts, .. } = &l.kind else { panic!() };
    assert!(pts[1].pt().minus(Pt::new(9.0, 1.0)).len() < 1e-9, "{:?}", pts[1].pt());
    // A half circle (bulge 1) from (-5,0) to (5,0): offset outwards it is concentric, radius 7.
    let arc = d.offset_entity(3, 2.0, Pt::new(0.0, -10.0)).unwrap();
    let c = crate::kernel::curves(&arc);
    assert!(matches!(c[0], crate::kernel::Curve::Arc { r, .. } if (r - 7.0).abs() < 1e-9), "{c:?}");
    // Not offsettable: a text.
    assert!(d.offset_entity(9, 1.0, Pt::new(0.0, 0.0)).is_none());
}

fn kind(d: &Drawing, i: usize) -> &Kind {
    &d.entities[i].kind
}

/// Trim (quick mode): the piece picked goes, between the nearest crossings; a piece out of the
/// middle leaves two objects; nothing crossing deletes it; the far ends keep their expressions.
#[test]
fn trim_takes_away_the_piece_picked() {
    let mut d = Drawing::from_json(
        r#"{"entities":[{"type":"line","a":[0,0],"b":["L",0]},{"type":"line","a":[3,-1],"b":[3,1]},
            {"type":"line","a":[6,-1],"b":[6,1]}]}"#,
    )
    .unwrap();
    d.resolve(&sheet(&["L = 10 mm"]));
    // The middle piece: two lines left, the second with a new id after the first.
    d.trim(0, Pt::new(4.5, 0.0)).unwrap();
    assert_eq!(d.entities.len(), 4);
    assert!(matches!(kind(&d, 0), Kind::Line { a, b } if a.pt() == Pt::new(0.0, 0.0) && b.pt() == Pt::new(3.0, 0.0)));
    assert!(matches!(kind(&d, 1), Kind::Line { a, b } if a.pt() == Pt::new(6.0, 0.0) && b.0.expr.as_deref() == Some("L")));
    assert_eq!(d.entities[0].id, "e1");
    assert!(d.entities.iter().filter(|e| e.id == d.entities[1].id).count() == 1);
    // Nothing crosses (6,0)–(L,0) but at its end: picked, it goes.
    d.trim(1, Pt::new(9.0, 0.0)).unwrap();
    assert_eq!(d.entities.len(), 3);
    // A cutting line with nothing crossing it goes.
    d.trim(2, Pt::new(6.0, 0.5)).unwrap();
    assert_eq!(d.entities.len(), 2);
    // A circle cut by a line: the picked side goes, an arc stays.
    let mut c = Drawing::from_json(r#"{"entities":[{"type":"circle","c":[0,0],"r":5},{"type":"line","a":[-10,0],"b":[10,0]}]}"#).unwrap();
    c.trim(0, Pt::new(0.0, -5.0)).unwrap();
    assert!(matches!(kind(&c, 0), Kind::Arc { start, end, .. } if approx(start.value, 0.0) && approx(end.value, 180.0)), "{:?}", kind(&c, 0));
    // One crossing is not enough for a closed object.
    let mut t = Drawing::from_json(r#"{"entities":[{"type":"circle","c":[0,0],"r":5},{"type":"line","a":[0,0],"b":[10,0]}]}"#).unwrap();
    assert!(t.trim(0, Pt::new(0.0, 5.0)).is_err());
    // A closed square cut through by a line: it opens, the picked side gone.
    let mut sq = Drawing::from_json(
        r#"{"entities":[{"type":"polyline","pts":[[0,0],[10,0],[10,10],[0,10]],"closed":true},{"type":"line","a":[5,-1],"b":[5,11]}]}"#,
    )
    .unwrap();
    sq.trim(0, Pt::new(10.0, 5.0)).unwrap();
    let Kind::Polyline { pts, closed, .. } = kind(&sq, 0) else { panic!() };
    assert!(!closed);
    let got: Vec<Pt> = pts.iter().map(P::pt).collect();
    assert_eq!(got, vec![Pt::new(5.0, 10.0), Pt::new(0.0, 10.0), Pt::new(0.0, 0.0), Pt::new(5.0, 0.0)]);
    // A polyline half circle (clockwise over the top) cut at its top: each half a quarter.
    let mut arc =
        Drawing::from_json(r#"{"entities":[{"type":"polyline","pts":[[-5,0],[5,0]],"bulges":[-1]},{"type":"line","a":[0,-10],"b":[0,10]}]}"#)
            .unwrap();
    arc.trim(0, Pt::new(4.0, 3.0)).unwrap();
    let Kind::Polyline { pts, bulges, .. } = kind(&arc, 0) else { panic!() };
    assert!(pts[1].pt().minus(Pt::new(0.0, 5.0)).len() < 1e-9, "{:?}", pts[1].pt());
    assert!(approx(bulges[0], -(std::f64::consts::FRAC_PI_8).tan()), "{bulges:?}");
    // No object there: an error, never a panic.
    assert!(d.trim(99, Pt::new(0.0, 0.0)).is_err());
}

/// Extend: the end nearer the pick runs to the first object ahead; an arc goes round its circle;
/// a polyline's end segment grows.
#[test]
fn extend_runs_to_the_next_object() {
    let mut d = Drawing::from_json(
        r#"{"entities":[{"type":"line","a":[0,0],"b":[2,0]},{"type":"line","a":[5,-1],"b":[5,1]},
            {"type":"line","a":[8,-1],"b":[8,1]},{"type":"line","a":[-4,-1],"b":[-4,1]}]}"#,
    )
    .unwrap();
    d.extend(0, Pt::new(1.8, 0.0)).unwrap();
    assert!(matches!(kind(&d, 0), Kind::Line { b, .. } if b.pt() == Pt::new(5.0, 0.0)), "the nearest boundary first");
    d.extend(0, Pt::new(4.0, 0.0)).unwrap();
    assert!(matches!(kind(&d, 0), Kind::Line { b, .. } if b.pt() == Pt::new(8.0, 0.0)));
    d.extend(0, Pt::new(0.5, 0.0)).unwrap();
    assert!(matches!(kind(&d, 0), Kind::Line { a, .. } if a.pt() == Pt::new(-4.0, 0.0)));
    assert!(d.extend(0, Pt::new(7.9, 0.0)).is_err(), "nothing ahead");
    // An arc from 0° to 90° of radius 5, a line along y = -3: its start runs back round to it.
    let mut a =
        Drawing::from_json(r#"{"entities":[{"type":"arc","c":[0,0],"r":5,"start":0,"end":90},{"type":"line","a":[-10,-3],"b":[10,-3]}]}"#).unwrap();
    a.extend(0, Pt::new(4.9, 0.5)).unwrap();
    let want = (-3.0f64).atan2(4.0).to_degrees().rem_euclid(360.0);
    assert!(matches!(kind(&a, 0), Kind::Arc { start, .. } if approx(start.value.rem_euclid(360.0), want)), "{:?}", kind(&a, 0));
    // A polyline's last straight segment.
    let mut p = Drawing::from_json(r#"{"entities":[{"type":"polyline","pts":[[0,0],[0,2],[2,2]]},{"type":"line","a":[6,0],"b":[6,5]}]}"#).unwrap();
    p.extend(0, Pt::new(2.0, 2.0)).unwrap();
    assert!(matches!(kind(&p, 0), Kind::Polyline { pts, .. } if pts[2].pt() == Pt::new(6.0, 2.0)));
    // A circle has no ends.
    let mut c = Drawing::from_json(r#"{"entities":[{"type":"circle","c":[0,0],"r":1}]}"#).unwrap();
    assert!(c.extend(0, Pt::new(1.0, 0.0)).is_err());
}

/// Fillet and Chamfer: two lines trimmed (or extended) to an arc touching both or to a bevel;
/// radius 0 makes the corner.
#[test]
fn fillet_and_chamfer_join_two_lines() {
    let src = r#"{"entities":[{"type":"line","a":[0,0],"b":[10,0]},{"type":"line","a":[12,2],"b":[12,10]}]}"#;
    let mut d = Drawing::from_json(src).unwrap();
    d.fillet(0, Pt::new(3.0, 0.0), 1, Pt::new(12.0, 8.0), 2.0).unwrap();
    assert!(matches!(kind(&d, 0), Kind::Line { a, b } if a.pt() == Pt::new(0.0, 0.0) && b.pt() == Pt::new(10.0, 0.0)));
    assert!(matches!(kind(&d, 1), Kind::Line { a, b } if a.pt() == Pt::new(12.0, 2.0) && b.pt() == Pt::new(12.0, 10.0)));
    let Kind::Arc { c, r, start, end } = kind(&d, 2) else { panic!("{:?}", kind(&d, 2)) };
    assert!(c.pt() == Pt::new(10.0, 2.0) && r.value == 2.0);
    assert!(approx(start.value.rem_euclid(360.0), 270.0) && approx(end.value, 0.0), "{start:?} {end:?}");
    // Radius 0: the corner.
    let mut z = Drawing::from_json(src).unwrap();
    z.fillet(0, Pt::new(3.0, 0.0), 1, Pt::new(12.0, 8.0), 0.0).unwrap();
    assert_eq!(z.entities.len(), 2);
    assert!(matches!(kind(&z, 0), Kind::Line { b, .. } if b.pt() == Pt::new(12.0, 0.0)));
    assert!(matches!(kind(&z, 1), Kind::Line { a, .. } if a.pt() == Pt::new(12.0, 0.0)));
    // Chamfer 1 and 2 from the corner.
    let mut c = Drawing::from_json(src).unwrap();
    c.chamfer(0, Pt::new(3.0, 0.0), 1, Pt::new(12.0, 8.0), 1.0, 2.0).unwrap();
    assert!(matches!(kind(&c, 2), Kind::Line { a, b } if a.pt() == Pt::new(11.0, 0.0) && b.pt() == Pt::new(12.0, 2.0)));
    // Errors, never panics: the same line, a radius too large, not a number, parallel, not lines.
    let mut e = Drawing::from_json(src).unwrap();
    assert!(e.fillet(0, Pt::new(1.0, 0.0), 0, Pt::new(2.0, 0.0), 1.0).is_err());
    assert!(e.fillet(0, Pt::new(1.0, 0.0), 1, Pt::new(12.0, 8.0), 100.0).is_err());
    assert!(e.fillet(0, Pt::new(1.0, 0.0), 1, Pt::new(12.0, 8.0), f64::NAN).is_err());
    assert_eq!(e.to_json(), Drawing::from_json(src).unwrap().to_json(), "a failed fillet changes nothing");
    let mut par = Drawing::from_json(r#"{"entities":[{"type":"line","a":[0,0],"b":[10,0]},{"type":"line","a":[0,1],"b":[10,1]}]}"#).unwrap();
    assert!(par.fillet(0, Pt::new(1.0, 0.0), 1, Pt::new(1.0, 1.0), 1.0).is_err());
    let mut circ = Drawing::from_json(r#"{"entities":[{"type":"line","a":[0,0],"b":[10,0]},{"type":"circle","c":[0,0],"r":1}]}"#).unwrap();
    assert!(circ.chamfer(0, Pt::new(1.0, 0.0), 1, Pt::new(1.0, 0.0), 1.0, 1.0).is_err());
}

/// A text entity's angle reaches its primitive and the SVG (counter-clockwise on paper is a
/// negative SVG rotation), and its estimated extent turns with it.
#[test]
fn rotated_texts_are_drawn_at_their_angle() {
    let d = Drawing::from_json(r#"{"entities":[{"type":"text","p":[0,0],"value":"ABCDEFGH","height":5,"angle":90}]}"#).unwrap();
    let r = d.render(None);
    let Some(Prim::Text { angle, .. }) = r.prims.first() else { panic!("{:?}", r.prims) };
    assert!(approx(*angle, 90.0));
    // Standing up, the text is taller than it is wide.
    assert!(r.size.1 > r.size.0 + 10.0, "{:?}", r.size);
    let svg = r.to_svg([0, 0, 0]);
    assert!(svg.contains("transform=\"rotate(-90 "), "{svg}");
}

/// Readable directions: never upside down, a vertical text reads bottom to top.
#[test]
fn readable_text_directions_never_read_upside_down() {
    use crate::render::readable;
    for (deg, want, turned) in [
        (0.0, 0.0, false),
        (30.0, 30.0, false),
        (90.0, 90.0, false),
        (-90.0, 90.0, true),
        (180.0, 0.0, true),
        (-180.0, 0.0, true),
        (135.0, -45.0, true),
        (-135.0, 45.0, true),
        (270.0, 90.0, true),
        (450.0, 90.0, false),
    ] {
        let (a, t) = readable(deg);
        assert!(approx(a, want) && t == turned, "{deg}: {a} {t}");
    }
    assert_eq!(readable(f64::NAN), (0.0, false));
    assert_eq!(readable(f64::INFINITY), (0.0, false));
}

/// With the style's aligned texts, a steep dimension's text turns with its line, whichever way
/// the line was drawn, and stays readable; by default it reads level, as before.
#[test]
fn aligned_dimension_texts_follow_steep_lines_readably() {
    let text_of = |json: &str| {
        let d = Drawing::from_json(json).unwrap();
        d.render(None).prims.into_iter().find_map(|p| if let Prim::Text { angle, anchor, .. } = p { Some((angle, anchor)) } else { None }).unwrap()
    };
    // Default: level beside the line.
    let (angle, anchor) = text_of(r#"{"entities":[{"type":"dim","a":[0,0],"b":[0,50],"offset":8}]}"#);
    assert!(approx(angle, 0.0) && matches!(anchor, Anchor::MiddleLeft | Anchor::MiddleRight), "{angle} {anchor:?}");
    for (a, b) in [((0, 0), (0, 50)), ((0, 50), (0, 0)), ((0, 0), (20, 50)), ((20, 50), (0, 0)), ((0, 0), (-20, 50)), ((0, 0), (-50, -20))] {
        let json = format!(
            r#"{{"dimStyle":{{"aligned":true}},"entities":[{{"type":"dim","a":[{},{}],"b":[{},{}],"offset":8,"orient":"aligned"}}]}}"#,
            a.0, a.1, b.0, b.1
        );
        let (angle, anchor) = text_of(&json);
        assert!(angle > -90.0 + 1e-9 && angle <= 90.0 + 1e-9, "{a:?}→{b:?}: {angle}");
        let line = f64::from(b.1 - a.1).atan2(f64::from(b.0 - a.0)).to_degrees();
        let off = (angle - line).rem_euclid(180.0);
        assert!(off < 1e-6 || off > 180.0 - 1e-6, "{a:?}→{b:?}: {angle} along {line}");
        assert!(matches!(anchor, Anchor::BottomCenter | Anchor::TopCenter), "{anchor:?}");
    }
    // A vertical one reads bottom to top.
    let (angle, _) = text_of(r#"{"dimStyle":{"aligned":true},"entities":[{"type":"dim","a":[0,50],"b":[0,0],"offset":8,"orient":"vertical"}]}"#);
    assert!(approx(angle, 90.0), "{angle}");
    // An angle's label runs along its arc: a right angle from the x axis has its middle at 45°,
    // its tangent there at 135°, turned readable to −45°.
    let (angle, _) = text_of(r#"{"dimStyle":{"aligned":true},"entities":[{"type":"angle","c":[0,0],"a":[10,0],"b":[0,10],"r":8}]}"#);
    assert!(approx(angle, -45.0), "{angle}");
}

/// The aligned option is written only when on: old drawings read the same and write the same.
#[test]
fn aligned_dimension_texts_keep_json_back_compatible() {
    let old = r#"{"dimStyle":{"text":3.5},"entities":[{"type":"line","a":[0,0],"b":[10,0]}]}"#;
    let d = Drawing::from_json(old).unwrap();
    assert!(!d.dim_style.aligned);
    let json = d.to_json();
    assert!(!json.contains("aligned"), "{json}");
    let mut d2 = d.clone();
    d2.dim_style.aligned = true;
    let json2 = d2.to_json();
    assert!(json2.contains(r#""aligned":true"#), "{json2}");
    assert!(Drawing::from_json(&json2).unwrap().dim_style.aligned);
    assert_eq!(Drawing::from_json(&json2).unwrap().to_json(), json2);
    // The new dash type round-trips by its name.
    let dd = Drawing::from_json(r#"{"layers":[{"name":"0","dash":"dashDoubleDotted"}]}"#).unwrap();
    assert_eq!(dd.layers[0].dash, Dash::DashDoubleDotted);
    assert!(dd.to_json().contains(r#""dash":"dashDoubleDotted""#), "{}", dd.to_json());
}

/// Dash-double-dotted is a long dash and two dots; every dashed type has an even pattern.
#[test]
fn dash_double_dotted_is_a_long_dash_and_two_dots() {
    assert_eq!(Dash::DashDoubleDotted.pattern(), &[6.0, 1.5, 0.5, 1.5, 0.5, 1.5]);
    for d in [Dash::Dashed, Dash::Dotted, Dash::DashDotted, Dash::DashDoubleDotted] {
        assert!(!d.pattern().is_empty() && d.pattern().len() % 2 == 0, "{d:?}");
    }
}

/// A dashed path's pieces: the pattern in paper mm from its first point, across corners and a
/// closed path's closing side; solid is one piece; hostile patterns never hang or panic.
#[test]
fn dash_patterns_cut_paths_into_their_dashes() {
    use crate::render::{MAX_DASHES, dash_runs};
    let len = |run: &Vec<(f64, f64)>| run.windows(2).map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt()).sum::<f64>();
    // 20 mm dashed 4 on / 2 off: dashes at 0–4, 6–10, 12–16, 18–20.
    let runs = dash_runs(&[(0.0, 0.0), (20.0, 0.0)], &[4.0, 2.0]);
    assert_eq!(runs.len(), 4, "{runs:?}");
    assert!(approx(runs[1][0].0, 6.0) && approx(runs[1][1].0, 10.0), "{runs:?}");
    assert!(approx(len(&runs[3]), 2.0));
    // Round a corner: a dash keeps the corner point.
    let runs = dash_runs(&[(0.0, 0.0), (3.0, 0.0), (3.0, 3.0)], &[4.0, 2.0]);
    assert_eq!(runs[0], vec![(0.0, 0.0), (3.0, 0.0), (3.0, 1.0)]);
    // Solid: the whole path.
    assert_eq!(dash_runs(&[(0.0, 0.0), (5.0, 0.0)], &[]).len(), 1);
    // Through a drawing: a dashed layer's line becomes its pattern's dashes, a closed square's
    // dashes run on along its closing side.
    let d = Drawing::from_json(
        r#"{"layers":[{"name":"0","dash":"dashDoubleDotted"}],"entities":[{"type":"line","a":[0,0],"b":[30,0]},
            {"type":"polyline","pts":[[0,10],[10,10],[10,20],[0,20]],"closed":true}]}"#,
    )
    .unwrap();
    let r = d.render(None);
    let pieces = r.prims[0].pieces();
    // 30 mm of an 11.5 mm pattern: dash, dot, dot, dash, dot, dot, dash.
    assert_eq!(pieces.len(), 7, "{pieces:?}");
    assert!(approx(len(&pieces[0]), 6.0) && approx(len(&pieces[1]), 0.5) && approx(len(&pieces[2]), 0.5));
    let total: f64 = r.prims[1].pieces().iter().map(len).sum();
    // 40 mm round: three 11.5 mm patterns (7 mm on in each) and 5.5 mm of a dash.
    assert!(approx(total, 3.0 * 7.0 + 5.5), "{total}");
    // Hostile: tiny, NaN, negative, huge.
    assert_eq!(dash_runs(&[(0.0, 0.0), (1e9, 0.0)], &[1e-3, 1e-3]).len(), MAX_DASHES + 1);
    assert_eq!(dash_runs(&[(0.0, 0.0), (10.0, 0.0)], &[f64::NAN, -1.0]).len(), 1);
    assert!(dash_runs(&[(f64::NAN, 0.0), (10.0, 0.0)], &[1.0, 1.0]).is_empty());
    assert_eq!(dash_runs(&[(0.0, 0.0), (10.0, 0.0)], &[1.0]).len(), 5, "an odd pattern repeats twice over");
    assert!(Prim::Dot { at: (0.0, 0.0), r: 1.0, color: [0, 0, 0] }.pieces().is_empty());
}
