//! Arrowheads on open paths, and transforms, picking and limits kept coherent with rendering.

use crate::*;

fn d(json: &str) -> Drawing {
    Drawing::from_json(json).unwrap()
}

/// The filled paths (arrowheads) a drawing renders.
fn heads(d: &Drawing) -> Vec<Vec<(f64, f64)>> {
    d.render(None)
        .prims
        .into_iter()
        .filter_map(|p| match p {
            Prim::Path { pts, fill: true, .. } => Some(pts),
            _ => None,
        })
        .collect()
}

#[test]
fn open_paths_carry_arrowheads_where_asked() {
    let line = |arrows: &str| d(&format!(r#"{{"entities":[{{"type":"line","a":[0,0],"b":[100,0],"arrows":"{arrows}"}}]}}"#));
    assert_eq!(heads(&line("none")).len(), 0);
    assert_eq!(heads(&line("end")).len(), 1);
    assert_eq!(heads(&line("both")).len(), 2);
    // The end's arrowhead has its tip at the end and lies on the line.
    let r = line("end").render(None);
    let tip = r.prims.iter().find_map(|p| match p {
        Prim::Path { pts, fill: true, .. } => pts.first().copied(),
        _ => None,
    });
    let end = r.prims.iter().find_map(|p| match p {
        Prim::Path { pts, fill: false, .. } => pts.last().copied(),
        _ => None,
    });
    assert_eq!(tip, end);
    // Written only when set; read back the same.
    assert!(!line("none").to_json().contains("arrows"));
    assert_eq!(d(&line("start").to_json()).entities[0].arrows, Ends::Start);
    // Closed shapes have no ends.
    let closed = d(r#"{"entities":[{"type":"polyline","pts":[[0,0],[10,0],[10,10]],"closed":true,"arrows":"both"}]}"#);
    assert_eq!(heads(&closed).len(), 0);
    // An arc's arrowhead follows the curve's direction at its end.
    let arc = d(r#"{"entities":[{"type":"arc","c":[0,0],"r":50,"start":0,"end":90,"arrows":"end"}]}"#);
    assert_eq!(heads(&arc).len(), 1);
}

#[test]
fn the_properties_pane_sets_arrows_on_open_paths_only() {
    let mut dr = d(r#"{"entities":[{"type":"line","a":[0,0],"b":[10,0]},{"type":"circle","c":[0,0],"r":5}]}"#);
    assert!(dr.fields(0, 0).iter().any(|f| f.key == "arrows"));
    assert!(!dr.fields(1, 0).iter().any(|f| f.key == "arrows"));
    dr.set_field(0, 0, "arrows", "both").unwrap();
    assert_eq!(dr.entities[0].arrows, Ends::Both);
    assert!(dr.set_field(0, 0, "arrows", "sideways").is_err());
    assert!(dr.set_field(1, 0, "arrows", "end").is_err());
}

/// A piece cut from a path keeps only the arrowheads of the ends it still has.
#[test]
fn trimmed_and_exploded_pieces_keep_their_own_arrows() {
    let mut dr = d(r#"{"entities":[{"type":"line","a":[0,0],"b":[100,0],"arrows":"both"},
        {"type":"line","a":[40,-10],"b":[40,10]},{"type":"line","a":[60,-10],"b":[60,10]}]}"#);
    dr.trim(0, Pt::new(50.0, 0.0)).unwrap();
    let arrows: Vec<Ends> = dr.entities.iter().filter(|e| matches!(&e.kind, Kind::Line { a, .. } if a.1.value == 0.0)).map(|e| e.arrows).collect();
    assert_eq!(arrows.len(), 2, "{:?}", dr.entities);
    assert!(arrows.contains(&Ends::Start) && arrows.contains(&Ends::End), "{arrows:?}");
    let mut p = d(r#"{"entities":[{"type":"polyline","pts":[[0,0],[10,0],[10,10]],"arrows":"end"}]}"#);
    p.explode(&[0]);
    assert_eq!(p.entities.iter().map(|e| e.arrows).collect::<Vec<_>>(), vec![Ends::None, Ends::End]);
}

/// Scaling by a negative factor turns the drawing half round: arcs keep their sweep.
#[test]
fn a_negative_scale_is_a_half_turn_not_a_mirror() {
    let mut dr = d(r#"{"entities":[{"type":"arc","c":[0,0],"r":10,"start":0,"end":90}]}"#);
    dr.transform(&[0], &Affine::scaling(Pt::new(0.0, 0.0), -1.0));
    let Kind::Arc { start, end, .. } = &dr.entities[0].kind else { panic!() };
    assert!((start.value - 180.0).abs() < 1e-9 && (end.value - 270.0).abs() < 1e-9, "{start:?} {end:?}");
}

/// A mirrored dimension stays on its side of what it measures; a turned horizontal one is
/// vertical.
#[test]
fn mirrored_and_rotated_dimensions_stay_where_they_were_drawn() {
    let dim = r#"{"entities":[{"type":"dim","a":[0,0],"b":[100,0],"offset":8,"orient":"horizontal"}]}"#;
    let line_y = |dr: &Drawing| {
        let f = dr.dim_frame(&dr.entities[0], crate::edit::paper_per_unit(dr)).unwrap();
        f.fa.y
    };
    let mut dr = d(dim);
    let before = line_y(&dr);
    dr.transform(&[0], &Affine::mirror(Pt::new(0.0, 0.0), Pt::new(0.0, 1.0)).unwrap());
    assert!((line_y(&dr) - before).abs() < 1e-9, "the line stays above");
    let mut dr = d(dim);
    dr.transform(&[0], &Affine::rotation(Pt::new(0.0, 0.0), 90.0));
    assert!(matches!(&dr.entities[0].kind, Kind::Dim { orient: Orient::Vertical, .. }));
    let r = dr.render(None);
    assert!(r.prims.iter().any(|p| matches!(p, Prim::Text { text, .. } if text == "100")), "{:?}", r.prims);
    let mut dr = d(dim);
    dr.transform(&[0], &Affine::rotation(Pt::new(0.0, 0.0), 30.0));
    assert!(matches!(&dr.entities[0].kind, Kind::Dim { orient: Orient::Aligned, .. }));
}

/// A strip along a mirrored path changes side, so it stays on the same side of the shape.
#[test]
fn a_strip_on_a_mirrored_path_changes_side() {
    let mut dr = d(r#"{"entities":[{"id":"p","type":"line","a":[0,0],"b":[100,0]},{"type":"strip","along":"p","side":"left"}]}"#);
    dr.transform(&[0], &Affine::mirror(Pt::new(0.0, 0.0), Pt::new(0.0, 1.0)).unwrap());
    assert!(matches!(&dr.entities[1].kind, Kind::Strip { side: Side::Right, .. }));
}

/// A dimension's text is picked where it is drawn, on either side.
#[test]
fn a_dimension_text_is_picked_where_it_is_drawn() {
    for json in
        [r#"{"entities":[{"type":"dim","a":[0,0],"b":[100,0],"offset":-8}]}"#, r#"{"entities":[{"type":"dim","a":[100,0],"b":[0,0],"offset":8}]}"#]
    {
        let dr = d(json);
        let r = dr.render(None);
        let k = crate::edit::paper_per_unit(&dr);
        let f = dr.dim_frame(&dr.entities[0], k).unwrap();
        // The text sits above the line (y up), whichever way the dimension runs.
        let at = Pt::new(50.0, (f.fa.y + 2.5) / k);
        assert!(!dr.hits(at, 0.5).is_empty(), "{json}: {:?}", r.prims);
    }
}

/// Dragging a dimension's line between its two points leaves it at the farther one.
#[test]
fn a_dimension_line_never_jumps_past_the_pointer() {
    let dr = d(r#"{"entities":[]}"#);
    let off = dim_offset(&dr, Pt::new(0.0, 0.0), Pt::new(10.0, 5.0), Orient::Horizontal, Pt::new(5.0, 3.0));
    assert_eq!(off, 0.0);
}

#[test]
fn limits_match_what_the_pane_allows() {
    // Text heights up to 50 mm, as the pane and the dimension style.
    let dr = d(r#"{"entities":[{"type":"text","p":[0,0],"value":"A","height":30}]}"#);
    assert!(matches!(&dr.entities[0].kind, Kind::Text { height, .. } if *height == 30.0));
    // An id of 40 accented letters (80 bytes) stays the id its strip names.
    let id = "é".repeat(40);
    let dr = d(&format!(r#"{{"entities":[{{"id":"{id}","type":"line","a":[0,0],"b":[100,0]}},{{"type":"strip","along":"{id}"}}]}}"#));
    assert_eq!(dr.entities[0].id, id);
    assert!(!dr.strip_lines(&dr.entities[1]).is_empty());
    // An unknown layer is layer 0, for drawing and picking alike.
    let dr = d(r#"{"layers":[{"name":"hidden","visible":false},{"name":"0"}],"entities":[{"layer":"nope","type":"line","a":[0,0],"b":[100,0]}]}"#);
    assert!(dr.selectable(0));
    assert!(!dr.render(None).prims.is_empty());
}

/// With aligned texts off, a steep dimension keeps its second text.
#[test]
fn a_steep_dimension_keeps_its_text_below() {
    let dr = d(r#"{"dimStyle":{"aligned":false},"entities":[{"type":"dim","a":[0,0],"b":[0,100],"orient":"vertical","below":"x"}]}"#);
    let texts: Vec<String> =
        dr.render(None).prims.into_iter().filter_map(|p| if let Prim::Text { text, .. } = p { Some(text) } else { None }).collect();
    assert!(texts.contains(&"x".to_string()), "{texts:?}");
}
