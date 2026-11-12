//! Blocks: made from objects, inserted, annotative ones kept the same size on paper.

use crate::{Affine, Drawing, Entity, Kind, P, Pt, Unit};

/// A drawing at 1:50 in cm with a 2 × 1 rectangle at (10, 10) and a line elsewhere.
fn drawing() -> Drawing {
    let mut d = Drawing { unit: Unit::Cm, scale: 50.0, ..Drawing::default() };
    d.add(Entity::new(Kind::Polyline {
        pts: vec![P::new(10.0, 10.0), P::new(12.0, 10.0), P::new(12.0, 11.0), P::new(10.0, 11.0)],
        bulges: Vec::new(),
        closed: true,
    }))
    .unwrap();
    d.add(Entity::new(Kind::Line { a: P::new(0.0, 0.0), b: P::new(100.0, 0.0) })).unwrap();
    d
}

fn width(d: &Drawing, indices: &[usize]) -> f64 {
    let (lo, hi) = d.bounds_of(indices).unwrap();
    hi.x - lo.x
}

fn members(d: &Drawing, group: &str) -> Vec<usize> {
    d.entities.iter().enumerate().filter(|(_, e)| e.group.as_deref() == Some(group)).map(|(i, _)| i).collect()
}

#[test]
fn a_block_replaces_its_objects_where_they_were() {
    let mut d = drawing();
    let made = d.create_block("Door", &[0], Pt::new(10.0, 10.0), false, true).unwrap();
    assert_eq!(made.len(), 1);
    assert_eq!(d.entities.len(), 2);
    let b = d.block("Door").unwrap();
    assert_eq!((b.scale, b.unit, b.annotative), (50.0, Unit::Cm, false));
    // Stored round its base point.
    let (lo, _) = Drawing { entities: b.entities.clone(), ..Drawing::default() }.bounds_of(&[0]).unwrap();
    assert!(lo.x.abs() < 1e-9 && lo.y.abs() < 1e-9);
    // The insertion is where the objects were, one group with a record.
    let i = made[0];
    assert!((width(&d, &made) - 2.0).abs() < 1e-9);
    let r = d.insert_of(i).unwrap();
    assert_eq!((r.block.as_str(), r.at), ("Door", [10.0, 10.0]));
    // A name is unique and not empty.
    assert!(d.create_block("Door", &[1], Pt::new(0.0, 0.0), false, false).is_err());
    assert!(d.create_block("  ", &[1], Pt::new(0.0, 0.0), false, false).is_err());
    assert!(d.create_block("X", &[], Pt::new(0.0, 0.0), false, false).is_err());
}

#[test]
fn an_annotative_block_keeps_its_paper_size_at_every_scale() {
    let mut d = drawing();
    d.create_block("Tag", &[0], Pt::new(10.0, 10.0), true, false).unwrap();
    // Drawn at 1:50; the drawing now at 1:100: proposed twice as large in the drawing.
    d.scale = 100.0;
    assert_eq!(d.proposed_factor("Tag"), Some(2.0));
    let made = d.insert_block("Tag", Pt::new(50.0, 50.0), 1.0, 0.0).unwrap();
    assert!((width(&d, &made) - 4.0).abs() < 1e-9, "{}", width(&d, &made));
    // Paper size: 4 cm at 1:100 is 0.4 mm, as 2 cm at 1:50.
    let group = d.entities[made[0]].group.clone().unwrap();
    // The scale changes: rescaled about its insertion point once the drawing is made safe.
    d.scale = 25.0;
    d.sanitize();
    let m = members(&d, &group);
    assert!((width(&d, &m) - 1.0).abs() < 1e-9, "{}", width(&d, &m));
    let (lo, _) = d.bounds_of(&m).unwrap();
    assert!((lo.x - 50.0).abs() < 1e-9 && (lo.y - 50.0).abs() < 1e-9, "{lo:?}");
    // Drawn without being made safe (a fit trying scales): as at that scale too.
    d.scale = 50.0;
    let rd = d.render(None);
    let mut e = d.clone();
    e.sanitize();
    assert_eq!(rd.size, e.render(None).size);
    // Saved and read back the same.
    let back = Drawing::from_json(&e.to_json()).unwrap();
    assert_eq!(back.blocks, e.blocks);
    assert_eq!(back.inserts, e.inserts);
}

#[test]
fn a_plain_block_keeps_its_size_in_the_drawing() {
    let mut d = drawing();
    d.create_block("Door", &[0], Pt::new(10.0, 10.0), false, false).unwrap();
    d.scale = 100.0;
    assert_eq!(d.proposed_factor("Door"), Some(1.0));
    let made = d.insert_block("Door", Pt::new(0.0, 0.0), 3.0, 90.0).unwrap();
    // Turned a quarter: its height is 2 × 3.
    let (lo, hi) = d.bounds_of(&made).unwrap();
    assert!((hi.y - lo.y - 6.0).abs() < 1e-9 && (hi.x - lo.x - 3.0).abs() < 1e-9);
    d.scale = 20.0;
    d.sanitize();
    let (lo2, hi2) = d.bounds_of(&made).unwrap();
    assert!((hi2.y - lo2.y - 6.0).abs() < 1e-9);
}

#[test]
fn an_insertion_follows_moves_copies_and_scaling() {
    let mut d = drawing();
    let made = d.create_block("Tag", &[0], Pt::new(10.0, 10.0), true, true).unwrap();
    d.translate(&made, Pt::new(5.0, 0.0));
    assert_eq!(d.insert_of(made[0]).unwrap().at, [15.0, 10.0]);
    d.transform(&made, &Affine::scaling(Pt::new(15.0, 10.0), 2.0));
    assert!((d.insert_of(made[0]).unwrap().factor - 2.0).abs() < 1e-12);
    let copies = d.copy(&made, Pt::new(0.0, 20.0));
    let c = d.insert_of(copies[0]).unwrap().clone();
    assert_eq!(c.at, [15.0, 30.0]);
    assert_ne!(Some(&c.group), d.entities[made[0]].group.as_ref());
    assert_eq!(d.inserts.len(), 2);
    // The scale halves: both shrink about their own insertion points.
    d.scale = 25.0;
    d.sanitize();
    let m = members(&d, &c.group);
    assert!((width(&d, &m) - 2.0).abs() < 1e-9, "{}", width(&d, &m));
    let (lo, _) = d.bounds_of(&m).unwrap();
    assert!((lo.x - 15.0).abs() < 1e-9 && (lo.y - 30.0).abs() < 1e-9);
    // Explode: plain objects again, no insertion.
    let i = members(&d, &c.group);
    d.explode(&i);
    d.sanitize();
    assert_eq!(d.inserts.len(), 1);
}

#[test]
fn hostile_blocks_never_panic() {
    let text = r#"{"scale":50,"entities":[{"type":"line","a":[0,0],"b":[1,1],"group":"g1"}],
        "blocks":[{"name":"","scale":-3},{"name":"A","scale":1e300,"unit":"m","entities":[{"type":"circle","c":[0,0],"r":1e300}]},{"name":"A"}],
        "inserts":[{"group":"g1","block":"A","at":[1e308,-1e308],"factor":-1,"drawnAt":0},{"group":"nope","block":"A"}]}"#;
    let mut d = Drawing::from_json(text).unwrap();
    {
        assert!(d.blocks.len() <= 1);
        assert!(d.inserts.len() <= 1);
        let _ = d.render(None);
        let _ = d.insert_block("A", Pt::new(f64::NAN, 0.0), 1.0, 0.0);
        let _ = d.insert_block("A", Pt::new(0.0, 0.0), f64::INFINITY, f64::NAN);
        let _ = d.insert_block("A", Pt::new(0.0, 0.0), 1e-300, 1e300);
        let _ = d.create_block("B", &[usize::MAX], Pt::new(0.0, 0.0), true, true);
        d.scale = 1e-9;
        d.sanitize();
        let _ = d.render(None);
        let _ = d.to_json();
    }
}
