//! Random edits on random drawings (huge, tiny and degenerate numbers among them): no edit
//! panics, and whatever it leaves saves, reopens and saves again to the same bytes.

use crate::*;

/// A xorshift generator: the same seed, the same edits.
struct R(u64);
impl R {
    fn n(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn u(&mut self, k: usize) -> usize {
        (self.n() % k.max(1) as u64) as usize
    }
    fn f(&mut self) -> f64 {
        match self.u(10) {
            0 => 0.0,
            1 => 1e15 * if self.u(2) == 0 { 1.0 } else { -1.0 },
            2 => 1e-12,
            3 => -(self.u(100) as f64),
            _ => (self.u(20000) as f64) / 100.0 - 50.0,
        }
    }
    fn p(&mut self) -> Pt {
        Pt::new(self.f(), self.f())
    }
}

fn entity(r: &mut R, i: usize) -> String {
    let p = |r: &mut R| format!("[{}, {}]", r.f(), r.f());
    match r.u(11) {
        0 => format!(r#"{{"type":"line","a":{},"b":{}}}"#, p(r), p(r)),
        1 => format!(r#"{{"type":"circle","c":{},"r":{}}}"#, p(r), r.f()),
        2 => format!(r#"{{"type":"arc","c":{},"r":{},"start":{},"end":{}}}"#, p(r), r.f(), r.f(), r.f()),
        3 => {
            let n = 1 + r.u(6);
            let pts: Vec<String> = (0..n).map(|_| p(r)).collect();
            let b: Vec<String> = (0..n).map(|_| format!("{}", r.f() / 50.0)).collect();
            format!(r#"{{"id":"p{i}","type":"polyline","closed":{},"pts":[{}],"bulges":[{}]}}"#, r.u(2) == 0, pts.join(","), b.join(","))
        }
        4 => format!(r#"{{"type":"point","p":{}}}"#, p(r)),
        5 => format!(r#"{{"type":"text","p":{},"value":"ab<>{{{{L}}}}","angle":{}}}"#, p(r), r.f()),
        6 => format!(r#"{{"type":"dim","a":{},"b":{},"offset":{},"orient":"{}"}}"#, p(r), p(r), r.f(), ["aligned", "horizontal", "vertical"][r.u(3)]),
        7 => format!(r#"{{"type":"angle","c":{},"a":{},"b":{},"r":{}}}"#, p(r), p(r), p(r), r.f()),
        8 => format!(r#"{{"type":"level","p":{},"datum":{}}}"#, p(r), r.f()),
        9 => format!(r#"{{"type":"hatch","boundary":["p{}"],"pattern":"{}","spacing":{}}}"#, r.u(i + 1), ["lines", "cross", "solid"][r.u(3)], r.f()),
        _ => format!(r#"{{"type":"strip","along":"p{}","width":{}}}"#, r.u(i + 1), r.f()),
    }
}

/// `CAD_FUZZ=n` runs `n` drawings (default 40).
#[test]
fn random_edits_keep_the_drawing_whole() {
    let seeds: u64 = std::env::var("CAD_FUZZ").ok().and_then(|v| v.parse().ok()).unwrap_or(40);
    for seed in 1..=seeds {
        let mut r = R(seed.wrapping_mul(0x9E3779B97F4A7C15) | 1);
        let n = 1 + r.u(10);
        let es: Vec<String> = (0..n).map(|i| entity(&mut r, i)).collect();
        let json = format!(r#"{{"unit":"mm","scale":{},"entities":[{}]}}"#, 1 + r.u(100), es.join(","));
        let Ok(mut d) = Drawing::from_json(&json) else { continue };
        for step in 0..25 {
            let len = d.entities.len();
            let i = r.u(len);
            let sel: Vec<usize> = (0..len).filter(|_| r.u(2) == 0).collect();
            let op = r.u(22);
            match op {
                0 => d.translate(&sel, r.p()),
                1 => drop(d.copy(&sel, r.p())),
                2 => d.remove(&sel),
                3 => {
                    let g = d.grips(i);
                    if let Some((g, _)) = g.get(r.u(g.len())).cloned() {
                        let to = r.p();
                        d.move_grip(i, g, to);
                    }
                }
                4 => d.transform(&sel, &Affine::scaling(r.p(), r.f())),
                5 => d.transform(&sel, &Affine::rotation(r.p(), r.f())),
                6 => {
                    if let Some(t) = Affine::mirror(r.p(), r.p()) {
                        d.transform(&sel, &t)
                    }
                }
                7 => drop(d.explode(&sel)),
                8 => drop(d.join(&sel)),
                9 => drop(d.stretch(r.p(), r.p(), r.p())),
                10 => drop(d.array_rect(&sel, r.u(5), r.u(5), r.f(), r.f())),
                11 => drop(d.array_polar(&sel, r.p(), r.u(8), r.f())),
                12 => {
                    let pick = r.p();
                    drop(d.trim_preview(i, pick));
                    drop(d.trim(i, pick))
                }
                13 => {
                    let pick = r.p();
                    drop(d.extend_preview(i, pick));
                    drop(d.extend(i, pick))
                }
                14 => drop(d.fillet(i, r.p(), r.u(len), r.p(), r.f())),
                15 => drop(d.chamfer(i, r.p(), r.u(len), r.p(), r.f(), r.f())),
                16 => {
                    if let Some(e) = d.offset_entity(i, r.f(), r.p()) {
                        let _ = d.add(e);
                    }
                }
                17 => drop(d.flip_dims(&sel)),
                18 => drop(d.reorder(&sel, [Order::Front, Order::Back, Order::Forward, Order::Backward][r.u(4)])),
                19 => drop(d.set_field(
                    i,
                    r.u(4),
                    ["x", "y", "r", "start", "end", "offset", "angle", "value", "datum", "width"][r.u(10)],
                    &format!("{}", r.f()),
                )),
                20 => drop(d.set_fixed_scale(r.f(), None)),
                _ => d.match_properties(i, &sel),
            }
            let rendered = d.render(None);
            let _ = rendered.to_svg([0, 0, 0]);
            let j = d.to_json();
            match Drawing::from_json(&j) {
                Ok(back) => {
                    // Saving again gives the same bytes.
                    assert_eq!(back.to_json(), j, "seed {seed} step {step} op {op}");
                }
                Err(e) => panic!("seed {seed} step {step} op {op}: {e}\n{j}"),
            }
        }
    }
}

#[test]
fn a_datum_too_small_to_write_saves_the_same_twice() {
    // Found by the random edits: a datum of 1e-12 was written `0` (to a nanometre), and the
    // drawing read back wrote `"datum": 0` where the first save had left it out.
    let mut d = Drawing::from_json(r#"{"entities": [{"type": "level", "p": [1, 2]}]}"#).unwrap();
    if let Kind::Level { datum, .. } = &mut d.entities[0].kind {
        datum.value = 1e-12;
    }
    let once = d.to_json();
    assert!(!once.contains("datum"), "{once}");
    assert_eq!(Drawing::from_json(&once).unwrap().to_json(), once);
}

#[test]
fn large_arrays_are_added_at_once() {
    // Each copy used to look its id up through every entity and make the whole drawing safe
    // again: an array filling the drawing took seconds. The copies keep fresh, unique ids, in
    // order, and a hatch copied with its boundary follows the copy.
    let mut d = Drawing::from_json(
        r#"{"entities": [
        {"id": "b", "type": "polyline", "closed": true, "pts": [[0, 0], [10, 0], [10, 10], [0, 10]]},
        {"type": "hatch", "boundary": ["b"], "pattern": "lines", "spacing": 2},
        {"type": "line", "a": [0, 0], "b": [10, 10]},
        {"type": "circle", "c": [5, 5], "r": 3},
        {"type": "point", "p": [5, 5]}
    ]}"#,
    )
    .unwrap();
    let started = std::time::Instant::now();
    let made = d.array_rect(&[0, 1, 2, 3, 4], 40, 25, 20.0, 20.0).unwrap();
    assert!(started.elapsed() < std::time::Duration::from_secs(5), "{:?}", started.elapsed());
    assert_eq!(made.len(), 999 * 5);
    assert_eq!(d.entities.len(), MAX_ENTITIES);
    let ids: std::collections::HashSet<&str> = d.entities.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids.len(), d.entities.len(), "ids unique");
    // The last item: 39 columns and 24 rows on, its hatch on its own boundary.
    let last = &d.entities[made[made.len() - 5..][0]];
    assert!(matches!(&last.kind, Kind::Polyline { pts, .. } if pts[0].pt() == Pt::new(780.0, 480.0)), "{last:?}");
    match &d.entities[made[made.len() - 4]].kind {
        Kind::Hatch { boundary, .. } => assert_eq!(boundary, std::slice::from_ref(&last.id)),
        other => panic!("{other:?}"),
    }
    // No room left: a further copy is refused, and the drawing stays as it was.
    assert!(d.array_rect(&[2], 2, 1, 1.0, 0.0).is_err());
    assert!(d.copy(&[2], Pt::new(1.0, 0.0)).is_empty());
    // A polar array turns each item on by its step.
    let mut p = Drawing::from_json(r#"{"entities": [{"type": "point", "p": [10, 0]}]}"#).unwrap();
    p.array_polar(&[0], Pt::new(0.0, 0.0), 4, 360.0).unwrap();
    let at: Vec<Pt> = p.entities.iter().filter_map(|e| if let Kind::Point { p } = &e.kind { Some(p.pt()) } else { None }).collect();
    assert_eq!(at.len(), 4);
    assert!(at[1].minus(Pt::new(0.0, 10.0)).len() < 1e-9 && at[2].minus(Pt::new(-10.0, 0.0)).len() < 1e-9, "{at:?}");
}

/// A closed polyline of 10 000 near-full arcs: about a million points once tessellated.
fn huge_boundary() -> String {
    let pts: Vec<String> = (0..MAX_VERTICES).map(|i| format!("[{}, {}]", (i % 2) as f64 * 10.0, i as f64 * 10.0)).collect();
    let bulges = vec!["900"; MAX_VERTICES].join(",");
    format!(r#"{{"id": "p", "layer": "h", "type": "polyline", "closed": true, "pts": [{}], "bulges": [{bulges}]}}"#, pts.join(","))
}

/// A hatch naming one huge boundary a hundred times works on it once, within its point budget.
#[test]
fn a_hatch_naming_one_huge_boundary_many_times_stays_bounded() {
    let ids = vec![r#""p""#; 100].join(",");
    let json = format!(
        r#"{{"layers": [{{"name": "0"}}, {{"name": "h", "visible": false}}], "entities": [{}, {{"type": "hatch", "boundary": [{ids}], "pattern": "lines"}}]}}"#,
        huge_boundary()
    );
    let d = Drawing::from_json(&json).unwrap();
    let loops = d.hatch_loops(&d.entities[1]);
    assert_eq!(loops.len(), 1, "the boundary once");
    assert!(loops.iter().map(Vec::len).sum::<usize>() <= crate::hatch::MAX_LOOP_POINTS);
    let started = std::time::Instant::now();
    let _ = d.render(None);
    assert!(started.elapsed() < std::time::Duration::from_secs(10), "{:?}", started.elapsed());
}

/// Thousands of hatches that draw little from one huge boundary share the drawing's budget.
#[test]
fn many_hatches_on_one_huge_boundary_share_the_budget() {
    let mut entities = vec![huge_boundary()];
    for pattern in ["along", "lines"] {
        for _ in 0..2000 {
            entities.push(format!(r#"{{"type": "hatch", "boundary": ["p"], "pattern": "{pattern}"}}"#));
        }
    }
    for _ in 0..500 {
        entities.push(r#"{"type": "strip", "along": "p", "pattern": "along"}"#.to_string());
    }
    let json = format!(r#"{{"layers": [{{"name": "0"}}, {{"name": "h", "visible": false}}], "entities": [{}]}}"#, entities.join(","));
    let d = Drawing::from_json(&json).unwrap();
    let started = std::time::Instant::now();
    let _ = d.render(None);
    assert!(started.elapsed() < std::time::Duration::from_secs(20), "{:?}", started.elapsed());
}

/// Layer names from a file are cut, on a character boundary, as ids are.
#[test]
fn a_huge_layer_name_is_cut_on_reading() {
    let name = "é".repeat(3_000_000);
    let points = vec![r#"{"layer": "x", "type": "point", "p": [0, 0]}"#; 4000].join(",");
    let d = Drawing::from_json(&format!(r#"{{"layers": [{{"name": "{name}"}}, {{"name": "0"}}], "entities": [{points}]}}"#)).unwrap();
    assert!(d.layers.iter().all(|l| l.name.chars().count() <= 64));
    let started = std::time::Instant::now();
    let _ = d.render(None);
    assert!(started.elapsed() < std::time::Duration::from_secs(2), "{:?}", started.elapsed());
}
