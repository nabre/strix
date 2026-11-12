//! Series between a base and an end point (fit, exact spacing, spread) and groups.

use crate::*;

fn xs(d: &Drawing) -> Vec<f64> {
    d.entities
        .iter()
        .filter_map(|e| match &e.kind {
            Kind::Point { p } => Some((p.pt().x * 1e6).round() / 1e6),
            _ => None,
        })
        .collect()
}

fn one_point() -> Drawing {
    Drawing::from_json(r#"{"entities":[{"type":"point","p":[0,0]}]}"#).unwrap()
}

/// The owner: the items end exactly at the end point, take a spacing exactly (the rest left
/// over), or are spread evenly within a spacing.
#[test]
fn series_fit_pitch_and_spread() {
    let (a, b) = (Pt::new(0.0, 0.0), Pt::new(100.0, 0.0));
    // Fit: 5 items, the last on the end point, 25 apart.
    let mut d = one_point();
    let (copies, s) = d.array_series(&[0], a, b, (5, 1), Spacing::Fit, 0.0).unwrap();
    assert_eq!((copies.len(), s.cols), (4, 5));
    assert_eq!(xs(&d), vec![0.0, 25.0, 50.0, 75.0, 100.0]);
    // Exact spacing 30: 0, 30, 60, 90 (10 left over).
    let mut d = one_point();
    d.array_series(&[0], a, b, (5, 1), Spacing::Pitch, 30.0).unwrap();
    assert_eq!(xs(&d), vec![0.0, 30.0, 60.0, 90.0]);
    // An exact fit stays exact: 100 / 25 is four spaces.
    let mut d = one_point();
    d.array_series(&[0], a, b, (1, 1), Spacing::Pitch, 25.0).unwrap();
    assert_eq!(xs(&d).len(), 5);
    // Spread within 30: four spaces of 25, the last on the end point.
    let mut d = one_point();
    d.array_series(&[0], a, b, (1, 1), Spacing::Spread, 30.0).unwrap();
    assert_eq!(xs(&d), vec![0.0, 25.0, 50.0, 75.0, 100.0]);
    // Backwards and slanted: along the line, whatever its direction.
    let mut d = one_point();
    d.array_series(&[0], a, Pt::new(-30.0, -40.0), (3, 1), Spacing::Fit, 0.0).unwrap();
    assert!(matches!(&d.entities[2].kind, Kind::Point { p } if p.pt().minus(Pt::new(-30.0, -40.0)).len() < 1e-9));
    // A grid: the far corner fits 3 × 2.
    let mut d = one_point();
    let (_, s) = d.array_series(&[0], a, Pt::new(40.0, 10.0), (3, 2), Spacing::Fit, 0.0).unwrap();
    assert_eq!((s.cols, s.rows, s.col_step, s.row_step), (3, 2, Pt::new(20.0, 0.0), Pt::new(0.0, 10.0)));
    assert_eq!(d.entities.len(), 6);
    // A grid with a spacing: the counts come from the lengths.
    let mut d = one_point();
    let (_, s) = d.array_series(&[0], a, Pt::new(40.0, 25.0), (2, 2), Spacing::Pitch, 10.0).unwrap();
    assert_eq!((s.cols, s.rows), (5, 3));
}

/// Errors, never panics: no spacing, a spacing longer than the length, junk numbers, too many.
#[test]
fn series_refuses_what_cannot_be() {
    let (a, b) = (Pt::new(0.0, 0.0), Pt::new(100.0, 0.0));
    let mut d = one_point();
    assert!(d.array_series(&[0], a, b, (1, 1), Spacing::Pitch, 0.0).is_err(), "no spacing");
    assert!(d.array_series(&[0], a, b, (1, 1), Spacing::Pitch, 150.0).is_err(), "no copies");
    assert!(d.array_series(&[0], a, a, (4, 1), Spacing::Fit, 0.0).is_err(), "all on one point");
    assert!(d.array_series(&[0], a, b, (1, 1), Spacing::Pitch, 1e-6).is_err(), "too many");
    assert!(d.array_series(&[0], a, Pt::new(f64::NAN, 0.0), (3, 1), Spacing::Fit, 0.0).is_err());
    assert!(d.array_series(&[0], a, b, (1, 1), Spacing::Spread, f64::INFINITY).is_err());
    assert!(d.array_series(&[0], a, b, (usize::MAX, 2), Spacing::Fit, 0.0).is_err());
    assert!(d.array_series(&[], a, b, (3, 1), Spacing::Fit, 0.0).is_err());
    assert_eq!(d.entities.len(), 1, "nothing made");
    for (s, n) in Spacing::ALL {
        assert_eq!((Spacing::parse(n), s.name()), (Some(s), n));
    }
}

/// The owner: an array is one object. Its items are one group (picked together), a copy of it
/// is a group of its own, Explode ungroups it, and the group is saved.
#[test]
fn arrays_are_one_group() {
    let mut d = Drawing::from_json(r#"{"entities":[{"type":"line","a":[0,0],"b":[1,0]},{"type":"circle","c":[50,50],"r":1}]}"#).unwrap();
    d.array_series(&[0], Pt::new(0.0, 0.0), Pt::new(30.0, 0.0), (4, 1), Spacing::Fit, 0.0).unwrap();
    let group = d.entities[0].group.clone();
    assert!(group.is_some());
    assert_eq!(d.entities.iter().filter(|e| e.group == group).count(), 4);
    assert!(d.entities[1].group.is_none(), "the circle is not in it");
    // Picking one picks the four.
    let last = d.entities.len() - 1;
    let mut picked = d.with_groups(&[last]);
    picked.sort_unstable();
    assert_eq!(picked, vec![0, 2, 3, 4]);
    // Saved and read back.
    let back = Drawing::from_json(&d.to_json()).unwrap();
    assert_eq!(back.with_groups(&[0]).len(), 4);
    // A copy of the array is another group.
    let copies = d.copy(&picked, Pt::new(0.0, 10.0));
    let other = d.entities[copies[0]].group.clone();
    assert!(other.is_some() && other != group);
    assert_eq!(d.with_groups(&[copies[0]]).len(), 4);
    // A polar and a rectangular array group too.
    let mut p = Drawing::from_json(r#"{"entities":[{"type":"point","p":[5,0]}]}"#).unwrap();
    p.array_polar(&[0], Pt::new(0.0, 0.0), 4, 360.0).unwrap();
    assert_eq!(p.with_groups(&[0]).len(), 4);
    p.array_rect(&[0], 2, 1, 1.0, 0.0).unwrap();
    assert_eq!(p.with_groups(&[0]).len(), 2, "an array of a group's part makes a new group");
    // Explode: on their own again, unexploded.
    let n = d.entities.len();
    let members = d.explode(&[0]);
    assert_eq!((members.len(), d.entities.len()), (4, n));
    assert!(members.iter().all(|&i| d.entities[i].group.is_none()));
    assert_eq!(d.with_groups(&[0]), vec![0]);
    // Junk group names are cut or dropped.
    let j = Drawing::from_json(&format!(
        r#"{{"entities":[{{"type":"point","p":[0,0],"group":"{}"}},{{"type":"point","p":[0,0],"group":"  "}}]}}"#,
        "é".repeat(200)
    ))
    .unwrap();
    assert_eq!(j.entities[0].group.as_ref().map(|g| g.chars().count()), Some(64));
    assert!(j.entities[1].group.is_none());
}
