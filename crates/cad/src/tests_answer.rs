//! Tests for `Layer.answer` and `Entity.answer` (F6 — drawings to be completed by students).

use crate::{Drawing, Entity, Fit, Kind, Num, P, Prim};

fn line_entity(a: (f64, f64), b: (f64, f64)) -> Entity {
    Entity::new(Kind::Line { a: P(Num::new(a.0), Num::new(a.1)), b: P(Num::new(b.0), Num::new(b.1)) })
}

#[test]
fn answer_layer_and_entity_json_round_trip() {
    let mut d = Drawing::default();
    d.ensure_layer("Solution").unwrap();
    d.layers.iter_mut().find(|l| l.name == "Solution").unwrap().answer = true;

    let base = line_entity((0.0, 0.0), (40.0, 0.0));
    d.add(base).unwrap();

    let mut ans_ent = line_entity((0.0, 0.0), (0.0, 30.0));
    ans_ent.answer = true;
    d.add(ans_ent).unwrap();

    let mut on_sol_layer = line_entity((40.0, 0.0), (0.0, 30.0));
    on_sol_layer.layer = "Solution".into();
    d.add(on_sol_layer).unwrap();

    let json = d.to_json();
    assert!(json.contains("\"answer\":true"), "answer flag serialized when true: {json}");

    let back = Drawing::from_json(&json).unwrap();
    assert!(!back.layers[0].answer);
    assert!(back.layers[1].answer);
    assert!(!back.entities[0].answer);
    assert!(back.entities[1].answer);
    assert!(!back.entities[2].answer);

    // Default false is omitted from JSON.
    let mut clean = Drawing::default();
    clean.add(line_entity((0.0, 0.0), (10.0, 0.0))).unwrap();
    assert!(!clean.to_json().contains("answer"));
}

#[test]
fn student_render_keeps_exact_bounds_origin_scale_and_base_coords() {
    let mut d = Drawing { fit: Some(Fit { width: 80.0, height: 60.0, fixed: false }), ..Drawing::default() };
    d.ensure_layer("AnswerLayer").unwrap();
    d.layers.iter_mut().find(|l| l.name == "AnswerLayer").unwrap().answer = true;

    // Base segment sits inside [10..50, 10..10], while answer objects extend bounds to [-20..120, -10..90].
    d.add(line_entity((10.0, 10.0), (50.0, 10.0))).unwrap();

    let mut ans_vec = line_entity((10.0, 10.0), (120.0, 90.0));
    ans_vec.answer = true;
    d.add(ans_vec).unwrap();

    let mut ans_layer_line = line_entity((-20.0, -10.0), (10.0, 10.0));
    ans_layer_line.layer = "AnswerLayer".into();
    d.add(ans_layer_line).unwrap();

    d.apply_fit(None);
    let teacher = d.render_for(None, false, true);
    let student = d.render_for(None, false, false);

    // Pagination & placement invariance: exact same paper size, origin and scale!
    assert_eq!(teacher.size, student.size);
    assert_eq!(teacher.origin, student.origin);
    assert_eq!(teacher.scale, student.scale);

    // Teacher sees all 3 lines; student sees only the 1 base line at identical paper coordinates!
    assert_eq!(teacher.prims.len(), 3);
    assert_eq!(student.prims.len(), 1);
    assert_eq!(student.prims[0], teacher.prims[0]);

    // When all entities are marked answer, student copy still reserves the exact full bounding box!
    d.set_answer(&[0], true);
    let all_hidden = d.render_for(None, false, false);
    assert_eq!(all_hidden.size, teacher.size);
    assert_eq!(all_hidden.origin, teacher.origin);
    assert!(all_hidden.prims.is_empty());

    // Properties field round-trip on `"answer"`.
    assert_eq!(d.fields(0, 0).into_iter().find(|f| f.key == "answer").unwrap().text, "true");
    d.set_field(0, 0, "answer", "false").unwrap();
    assert!(!d.entities[0].answer);
    if let Prim::Path { pts, .. } = &d.render_for(None, false, false).prims[0] {
        assert_eq!(pts.len(), 2);
    } else {
        panic!("expected base path");
    }
}
