//! Underlays and inherited layers and styles.

use crate::*;

fn d(json: &str) -> Drawing {
    Drawing::from_json(json).unwrap()
}

/// Inheriting adds the layers and styles this drawing lacks; a name it has keeps its own
/// definition; the current layer and styles stay.
#[test]
fn inheriting_adds_new_names_and_keeps_existing_ones() {
    let src = d(
        r#"{"layers":[{"name":"0","color":[255,0,0],"width":0.5,"dash":"dashed","visible":true},{"name":"Walls","color":[0,0,255],"width":0.5,"dash":"solid","visible":true}],
        "dimStyles":[{"name":"Plan","style":{"text":5}}],"textStyles":[{"name":"Notes","height":4}],"entities":[]}"#,
    );
    let mut dst = d(
        r#"{"layers":[{"name":"0","color":[0,0,0],"width":0.25,"dash":"solid","visible":true}],"dimStyles":[{"name":"Plan","style":{"text":2}}],"currentDimStyle":"Plan","entities":[]}"#,
    );
    let got = dst.inherit(&src);
    assert_eq!(got, Inherited { layers: 1, dim_styles: 0, text_styles: 1 });
    assert_eq!(dst.layer("0").color, [0, 0, 0], "an existing layer keeps its definition");
    assert_eq!(dst.layer("Walls").color, [0, 0, 255]);
    assert_eq!(dst.dim_styles[0].style.text, 2.0, "an existing style keeps its definition");
    assert!(dst.text_styles.iter().any(|t| t.name == "Notes"));
    assert_eq!(dst.current_dim_style.as_deref(), Some("Plan"));
    // Again: nothing more.
    assert_eq!(dst.inherit(&src), Inherited::default());
    // Never past the limits.
    let many = Drawing { layers: (0..200).map(|i| Layer { name: format!("L{i}"), ..Layer::default() }).collect(), ..Drawing::default() };
    let mut full = Drawing::default();
    full.inherit(&many);
    assert!(full.layers.len() <= MAX_LAYERS);
}

/// The underlay as shown: locked, faded, moved by its offset, in this drawing's unit and
/// scale, without an underlay of its own; snaps find its geometry; never its own underlay.
#[test]
fn the_underlay_is_locked_faded_moved_and_one_level_deep() {
    let src =
        d(r#"{"unit":"m","id":"a","underlay":{"source":"z"},"layers":[{"name":"0","color":[0,0,0],"width":0.25,"dash":"solid","visible":true}],
        "entities":[{"type":"line","a":[0,0],"b":[1,0],"stroke":{"color":[0,0,0]}}]}"#);
    let mut me = d(r#"{"unit":"mm","scale":50,"id":"b","entities":[]}"#);
    me.underlay = Some(Underlay { source: "a".into(), dx: 100.0, dy: 0.0, fade: 0.5, ..Underlay::default() });
    let v = me.underlay_view(&src);
    assert_eq!((v.unit, v.scale), (Unit::Mm, 50.0));
    assert!(v.underlay.is_none() && v.id.is_none(), "one level deep");
    assert!(v.layers.iter().all(|l| l.locked));
    assert_eq!(v.layers[0].color, [128, 128, 128]);
    let Kind::Line { a, b } = &v.entities[0].kind else { panic!() };
    assert_eq!((a.0.value, b.0.value), (100.0, 1100.0), "1 m is 1000 mm, moved 100");
    // Snaps on its geometry: the end at x = 1100.
    let snapped = v.snap(Pt::new(1099.0, 0.5), 5.0, None, &Snaps::default());
    assert!(snapped.is_some_and(|(p, _)| (p.x - 1100.0).abs() < 1e-9), "{snapped:?}");
    // Never its own underlay; junk is kept within reason.
    let mut selfish = d(r#"{"id":"x","underlay":{"source":"x"},"entities":[]}"#);
    selfish.sanitize();
    assert!(selfish.underlay.is_none());
    let mut junk = d(r#"{"id":"x","underlay":{"source":"y","dx":1e308,"dy":-1e308,"fade":7},"entities":[]}"#);
    junk.sanitize();
    let u = junk.underlay.unwrap();
    assert_eq!((u.dx, u.dy, u.fade), (MAX_COORD, -MAX_COORD, 0.95));
    // A huge source is cut to the limit.
    let big = Drawing { entities: (0..6000).map(|i| Entity::new(Kind::Point { p: P::new(f64::from(i), 0.0) })).collect(), ..Drawing::default() };
    assert!(me.underlay_view(&big).entities.len() <= MAX_UNDERLAY_ENTITIES);
}

/// Printed with its underlay, the drawing's picture grows to hold both, aligned by their
/// drawing coordinates.
#[test]
fn a_rendering_with_its_underlay_beneath() {
    let me = d(r#"{"entities":[{"type":"line","a":[0,0],"b":[10,0]}]}"#);
    let under = d(r#"{"entities":[{"type":"line","a":[-50,0],"b":[0,0]}]}"#);
    let r = me.render(None);
    let ur = me.underlay_view(&under).render(None);
    let both = r.clone().with_under(ur);
    assert!(both.size.0 > r.size.0 + 40.0, "{:?} {:?}", both.size, r.size);
    assert_eq!(both.prims.len(), 2);
    // The drawing's own line ends where it did, in drawing coordinates.
    let Prim::Path { pts, .. } = &both.prims[1] else { panic!() };
    let x = (pts[1].0 + both.origin.0) / both.scale;
    assert!((x - 10.0).abs() < 1e-9, "{x}");
}
