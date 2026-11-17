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

/// An underlay printed with the drawing counts in its picture and in the fitted scale: every
/// object of the underlay lies inside the maximum size, not only the drawing's own; one shown
/// only in the editor (or with no source known) does not count.
#[test]
fn a_printed_underlay_counts_in_the_fitted_scale() {
    // The drawing: 1 m wide; its underlay: 4 m wide, from x = -3 m. Maximum size 100 mm wide.
    let mut me = d(r#"{"id":"b","fit":{"width":100,"height":0},"padding":0,"entities":[{"type":"line","a":[0,0],"b":[1000,0]}]}"#);
    let src = std::sync::Arc::new(d(r#"{"id":"a","entities":[{"type":"line","a":[-3000,0],"b":[1000,0]}]}"#));
    me.apply_fit(None);
    let alone = me.scale;
    assert_eq!(alone, 10.0, "1 m in 100 mm: 1:10");
    // Shown in the editor only: the scale stays.
    me.underlay = Some(Underlay { source: "a".into(), ..Underlay::default() });
    me.under_source = UnderSource(Some(src.clone()));
    me.apply_fit(None);
    assert_eq!(me.scale, alone);
    assert_eq!(me.with_printed_underlay(me.render(None)).size, me.render(None).size);
    // Printed: the 4 m of both fit in 100 mm at 1:50.
    me.underlay.as_mut().unwrap().print = true;
    me.apply_fit(None);
    assert_eq!(me.scale, 50.0);
    let both = me.with_printed_underlay(me.render(None));
    assert!(both.size.0 <= 100.0 + 1e-9 && both.size.0 > 70.0, "{:?}", both.size);
    assert!(me.fitting_scales(None).iter().all(|n| *n >= 50.0));
    assert!(me.clone().set_fixed_scale(20.0, None).is_err(), "at 1:20 the underlay is 200 mm");
    // The source unknown, hidden or another drawing's: as without an underlay.
    let mut hidden = me.clone();
    hidden.underlay.as_mut().unwrap().show = false;
    assert!(hidden.printed_underlay().is_none());
    let mut other = me.clone();
    other.underlay.as_mut().unwrap().source = "c".into();
    assert!(other.printed_underlay().is_none());
    // Never saved, never compared.
    assert!(!me.to_json().contains("-3000"));
    let mut plain = me.clone();
    plain.under_source = UnderSource::default();
    assert_eq!(plain, me);
}

/// A source with an id lends its layers nested under its name, unlocked, not printed when its
/// underlay does not print; a rename of the source follows; they are never renamed by hand and
/// removed only when empty.
#[test]
fn inherited_layers_nest_under_their_drawing() {
    let src = d(r#"{"id":"plan","layers":[{"name":"0"},{"name":"Walls","locked":true},{"name":"other/Axes","from":"other"}],"entities":[]}"#);
    let mut me = d(r#"{"id":"b","layers":[{"name":"0"},{"name":"Walls","color":[0,0,255]}],"entities":[]}"#);
    me.underlay = Some(Underlay { source: "plan".into(), ..Underlay::default() });
    let got = me.inherit(&src);
    assert_eq!(got.layers, 3);
    let names: Vec<&str> = me.layers.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["0", "Walls", "plan/0", "plan/Walls", "other/Axes"]);
    assert_eq!(me.layer("Walls").color, [0, 0, 255], "its own Walls untouched");
    let w = me.layer("plan/Walls");
    assert!(!w.locked && !w.print && w.from.as_deref() == Some("plan"));
    assert_eq!(me.print_layers_from("plan", true), 2);
    // Drawn on: kept; renamed: refused.
    me.entities.push(Entity { layer: "plan/Walls".into(), ..Entity::new(Kind::Point { p: P::new(0.0, 0.0) }) });
    assert!(me.rename_layer("plan/Walls", "x").is_err());
    assert!(me.remove_layer("plan/Walls").is_err());
    assert!(me.remove_layer("plan/0").is_ok(), "empty: it goes");
    // The source renamed: the names and the objects follow.
    assert!(me.rename_layers_from("plan", "ground"));
    assert_eq!(me.entities[0].layer, "ground/Walls");
    assert!(me.layers.iter().any(|l| l.name == "ground/Walls" && l.from.as_deref() == Some("ground")));
    // Saved and read back.
    let back = Drawing::from_json(&me.to_json()).unwrap();
    assert_eq!(back.layer("ground/Walls").from.as_deref(), Some("ground"));
}

/// All the source's styles come with it, nested under its name: its Standard dimension style,
/// its named ones (their text style following) and its text styles; a rename follows.
#[test]
fn inherited_styles_nest_under_their_drawing() {
    let src = d(r#"{"id":"plan","dimStyle":{"text":4},"dimStyles":[{"name":"Big","style":{"text":6,"textStyle":"Notes"}}],
        "textStyles":[{"name":"Notes","height":3}],"entities":[]}"#);
    let mut me = d(r#"{"id":"b","dimStyles":[{"name":"Big","style":{"text":2}}],"entities":[]}"#);
    let got = me.inherit(&src);
    assert_eq!((got.dim_styles, got.text_styles), (2, 1));
    let names: Vec<&str> = me.dim_styles.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Big", "plan/Standard", "plan/Big"]);
    assert_eq!(me.dim_style_named(Some("plan/Standard")).text, 4.0);
    assert_eq!(me.dim_style_named(Some("plan/Big")).text_style.as_deref(), Some("plan/Notes"));
    assert!(me.text_style_named(Some("plan/Notes")).is_some());
    assert_eq!(me.dim_style_named(Some("Big")).text, 2.0, "its own Big untouched");
    me.current_dim_style = Some("plan/Big".into());
    assert!(me.rename_layers_from("plan", "ground"));
    assert_eq!(me.current_dim_style.as_deref(), Some("ground/Big"));
    assert_eq!(me.dim_style_named(Some("ground/Big")).text_style.as_deref(), Some("ground/Notes"));
    assert!(me.text_style_named(Some("ground/Notes")).is_some());
}

/// A source in metres whose sizes are expressions shows at its real size under a millimetre
/// drawing: the values the expressions came to are converted like the plain numbers.
#[test]
fn an_underlay_with_expressions_is_converted_to_the_drawings_unit() {
    let mut ws = strix_calc::Worksheet::new(strix_calc::Options::default());
    ws.eval_statement("L = 4.5 m");
    let mut src = d(r#"{"id":"plan","unit":"m","entities":[{"type":"line","a":[0,0],"b":["L",2]},{"type":"circle","c":["L",0],"r":"L/9"}]}"#);
    assert!(src.resolve(&ws).is_empty());
    let host = d(r#"{"unit":"mm","underlay":{"source":"plan","dx":100},"entities":[]}"#);
    let view = host.underlay_view(&src);
    let Kind::Line { a, b } = &view.entities[0].kind else { panic!() };
    assert_eq!((a.pt(), b.pt()), (Pt::new(100.0, 0.0), Pt::new(4600.0, 2000.0)));
    let Kind::Circle { c, r } = &view.entities[1].kind else { panic!() };
    assert_eq!((c.pt(), r.value), (Pt::new(4600.0, 0.0), 500.0));
}
