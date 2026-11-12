//! Picking and grips: filled shapes and texts are taken anywhere on them.

use crate::*;

fn drawing(json: &str) -> Drawing {
    let mut d = Drawing::from_json(json).unwrap();
    d.resolve(&strix_calc::Worksheet::new(strix_calc::Options::default()));
    d
}

#[test]
fn a_solid_hatch_shows_and_moves_its_boundarys_grips() {
    let mut d = drawing(
        r#"{"unit":"m","scale":50,"entities":[
            {"id":"t","type":"polyline","pts":[[0,0],[1,0],[0.5,2]],"closed":true},
            {"type":"hatch","boundary":["t"],"pattern":"solid"}]}"#,
    );
    // Picked inside.
    assert_eq!(d.hits(Pt::new(0.4, 0.5), 0.01), vec![1]);
    // Its grips are the triangle's: dragging the tip reshapes the fill.
    let grips = d.grip_targets(1);
    assert!(grips.iter().all(|(i, _, _)| *i == 0) && grips.len() == 6, "{grips:?}");
    let (i, g, _) = grips.iter().copied().find(|(_, g, _)| *g == Grip::Vertex(2)).unwrap();
    assert!(d.move_grip(i, g, Pt::new(0.5, 3.0)));
    assert!(d.hatch_lines(&d.entities[1].clone()).iter().any(|(a, b)| a.y > 2.5 || b.y > 2.5));
    // Moved on its own, it takes its boundary along; copied, the copy follows its own copy.
    d.translate(&[1], Pt::new(10.0, 0.0));
    assert!(matches!(&d.entities[0].kind, Kind::Polyline { pts, .. } if pts[0].pt().x == 10.0));
    let copies = d.copy(&[1], Pt::new(0.0, 10.0));
    assert_eq!(copies.len(), 2);
    let boundary_copy = d.entities.iter().find(|e| matches!(e.kind, Kind::Polyline { .. }) && e.id != "t").unwrap().id.clone();
    assert!(d.entities.iter().any(|e| matches!(&e.kind, Kind::Hatch { boundary, .. } if boundary == &vec![boundary_copy.clone()])));
    d.transform(&[1], &Affine::rotation(Pt::new(5.0, 0.0), 180.0));
    assert!(matches!(&d.entities[0].kind, Kind::Polyline { pts, .. } if pts[0].pt().x.abs() < 1e-9));
}

#[test]
fn arrowheads_are_picked_inside() {
    // 1:1 in millimetres: the arrowhead is the style's mark size long, 30° across.
    let d = drawing(r#"{"unit":"mm","scale":1,"entities":[{"type":"line","a":[0,50],"b":[0,0],"arrows":"end"}]}"#);
    let size = d.dim_style.mark_size;
    let side = size * 0.5 * 15f64.to_radians().tan() * 0.9;
    // Off the line, inside the arrowhead: picked with no tolerance at all.
    assert_eq!(d.hits(Pt::new(side, size * 0.5), 0.0), vec![0]);
    assert!(d.hits(Pt::new(side, size * 3.0), 0.0).is_empty());
}

#[test]
fn texts_are_picked_inside_their_box_and_gripped_at_its_corners() {
    let mut d = drawing(r#"{"unit":"mm","scale":1,"entities":[{"type":"text","p":[0,0],"value":"q = 12 kN/m","height":5}]}"#);
    let b = d.text_box(&d.entities[0].clone()).unwrap();
    assert!(b[1].x > 25.0 && b[1].x < 50.0 && b[2].y > 4.9 && b[0].y < 0.0, "{b:?}");
    // Picked in its middle, far from its insertion point.
    assert_eq!(d.hits(Pt::new(b[1].x * 0.7, 2.5), 0.1), vec![0]);
    let grips = d.grips(0);
    assert_eq!(grips.iter().filter(|(g, _)| matches!(g, Grip::Corner(_))).count(), 4);
    // A corner dragged moves the text.
    assert!(d.move_grip(0, Grip::Corner(2), b[2].plus(Pt::new(10.0, 5.0))));
    assert!(matches!(&d.entities[0].kind, Kind::Text { p, .. } if (p.pt().x - 10.0).abs() < 1e-9 && (p.pt().y - 5.0).abs() < 1e-9));
    // A window takes it only whole; a crossing one touching it does.
    assert!(d.in_box(Pt::new(9.0, 4.0), Pt::new(12.0, 7.0), false).is_empty());
    assert_eq!(d.in_box(Pt::new(9.0, 4.0), Pt::new(12.0, 7.0), true), vec![0]);
    // Turned and centred, placeholders counted as what they show.
    let t =
        drawing(r#"{"unit":"m","scale":50,"entities":[{"type":"text","p":[1,1],"value":"{{L:both}}","height":3.5,"angle":90,"align":"center"}]}"#);
    let b = t.text_box(&t.entities[0].clone()).unwrap();
    assert!(((b[0].y + b[1].y) / 2.0 - 1.0).abs() < 1e-9 && b[1].y - b[0].y > 0.3 && b[0].x > 1.0 && b[2].x < 1.0, "{b:?}");
}

#[test]
fn a_measured_width_replaces_the_estimate() {
    let mut d = drawing(r#"{"unit":"mm","scale":1,"entities":[{"id":"t","type":"text","p":[0,0],"value":"{{q:both}}","height":7}]}"#);
    // The rendering says what each text shows, for the editor to measure.
    assert_eq!(d.render(None).texts.len(), 1);
    assert_eq!(d.render(None).texts[0].0, "t");
    let estimated = d.text_box(&d.entities[0].clone()).unwrap()[1].x;
    // 4 ems at 7 mm capitals (a 10 mm font): 40 mm.
    d.text_widths.set("t", "{{q:both}}", 4.0);
    assert!((d.text_box(&d.entities[0].clone()).unwrap()[1].x - 40.0).abs() < 1e-9 && estimated != 40.0);
    // Never saved, never compared; a changed value is estimated again; junk is refused.
    let json = d.to_json();
    assert!(!json.contains("textWidths") && Drawing::from_json(&json).unwrap() == d);
    d.text_widths.set("t", "{{q:both}}", f64::NAN);
    d.text_widths.set("", "x", 1.0);
    assert_eq!(d.text_widths.get("t", "{{q:both}}"), Some(4.0));
    assert_eq!(d.text_widths.get("t", "other"), None);
}
