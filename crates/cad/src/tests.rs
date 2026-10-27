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

use crate::kernel::{self, Curve, angle_at, angle_between, canon, deg, intersect, intersect_extended, line_line, tolerance};

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
            let tol = tolerance(a.size().max(b.size()));
            assert!((q.minus(c1).len() - r1).abs() <= tol && (q.minus(c2).len() - r2).abs() <= tol, "{q:?}");
        }
        let s = seg(c2.x - 80.0, c2.y + rng.next() * 30.0, c2.x + 80.0, c2.y + rng.next() * 30.0);
        for q in intersect(&s, &a) {
            assert!((q.minus(c1).len() - r1).abs() <= tolerance(s.size().max(a.size())));
            assert!(s.distance(q) <= tolerance(s.size()));
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
