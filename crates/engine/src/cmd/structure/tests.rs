//! `structure.*` commands: insertion, results, edits, calculations, student copies, saving.

use serde_json::json;
use wordcraft_doc::graphic::GraphicKind;
use wordcraft_doc::{Block, Document, InlineObject};

use crate::Session;

fn first_structure(doc: &Document) -> Option<(f32, f32, wordcraft_doc::graphic::Graphic)> {
    doc.body.iter().find_map(|b| match &**b {
        Block::Para(p) => p.objects.iter().find_map(|o| match o {
            InlineObject::Graphic { w, h, graphic, .. } if graphic.kind == GraphicKind::Structure => Some((*w, *h, (**graphic).clone())),
            _ => None,
        }),
        _ => None,
    })
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * (1.0 + b.abs())
}

#[test]
fn inserting_a_beam_draws_it_with_its_results() {
    let mut s = Session::new(Document::new());
    s.run("structure.insert", &json!({})).unwrap();
    let (w, h, g) = first_structure(&s.doc).expect("structure");
    assert!(w > 100.0 && h > 100.0, "{w}×{h}");
    assert!(!g.items.is_empty() && g.svg.is_some());
    let r = s.run("structure.get", &json!({})).unwrap();
    assert_eq!(r["status"], "isostatic", "{r}");
    assert!(close(r["reactions"][0]["fy"].as_f64().unwrap(), 30.0), "{r}");
    let m = r["members"][0]["m"].as_array().unwrap();
    assert!(m.iter().any(|n| n["kind"] == "extreme" && close(n["value"].as_f64().unwrap(), 45.0)), "{m:?}");
}

#[test]
fn edits_build_a_frame_and_follow_the_calculations() {
    let mut s = Session::new(Document::new());
    s.run("insert.equation", &json!({"linear": "L := 8 m", "edit": false, "calc": true})).unwrap();
    s.run("text.insert", &json!({"text": " "})).unwrap();
    s.run("insert.equation", &json!({"linear": "q := 5 kN/m", "edit": false, "calc": true})).unwrap();
    s.run("text.insert", &json!({"text": " "})).unwrap();
    s.run("structure.insert", &json!({"template": "cantilever"})).unwrap();
    // A beam on two supports, L long, with q on it.
    s.run("structure.node", &json!({"id": "B", "x": "L"})).unwrap();
    s.run("structure.support", &json!({"node": "A", "kind": "pin", "angle": 0})).unwrap();
    s.run("structure.support", &json!({"node": "B", "kind": "roller"})).unwrap();
    s.run("structure.load", &json!({"index": 0, "load": {"type": "distributed", "member": "1", "q1": "q"}})).unwrap();
    let r = s.run("structure.get", &json!({})).unwrap();
    assert_eq!(r["status"], "isostatic", "{r}");
    assert!(r["errors"].as_array().unwrap().is_empty(), "{r}");
    // q·L/2 = 20 at each support; q·L²/8 = 40.
    assert!(close(r["reactions"][1]["fy"].as_f64().unwrap(), 20.0), "{r}");
    let m = r["members"][0]["m"].as_array().unwrap();
    assert!(m.iter().any(|n| n["kind"] == "extreme" && close(n["value"].as_f64().unwrap(), 40.0)), "{m:?}");
    // A new node and a member make a frame; removing a node removes what uses it.
    s.run("structure.node", &json!({"x": 0, "y": 3})).unwrap();
    s.run("structure.member", &json!({"a": "A", "b": "C"})).unwrap();
    let r = s.run("structure.get", &json!({})).unwrap();
    assert_eq!(r["structure"]["members"].as_array().unwrap().len(), 2, "{r}");
    s.run("structure.node", &json!({"id": "C", "remove": true})).unwrap();
    let r = s.run("structure.get", &json!({})).unwrap();
    assert_eq!(r["structure"]["members"].as_array().unwrap().len(), 1, "{r}");
    // Two rollers: labile.
    s.run("structure.support", &json!({"node": "A", "kind": "roller"})).unwrap();
    assert_eq!(s.run("structure.get", &json!({})).unwrap()["status"], "labile");
    assert!(s.run("structure.support", &json!({"node": "A", "kind": "hinge"})).is_err());
    assert!(s.run("structure.member", &json!({"a": "A", "b": "A"})).is_err());
}

#[test]
fn settings_and_conventions() {
    let mut s = Session::new(Document::new());
    s.run("structure.insert", &json!({"template": "frame"})).unwrap();
    let r = s
        .run("structure.set", &json!({"diagrams": {"m": {"positive": "up"}, "v": {"show": false}}, "titles": {"v": "V"}, "title": "Portale"}))
        .unwrap();
    assert_eq!(r["structure"]["diagrams"]["m"]["positive"], "up");
    assert_eq!(r["structure"]["diagrams"]["v"]["show"], false);
    assert_eq!(r["structure"]["titles"]["v"], "V");
    let r = s.run("structure.set", &json!({"convention": "italian"})).unwrap();
    assert_eq!(r["structure"]["diagrams"]["m"]["positive"], "down");
    assert!(s.run("structure.set", &json!({"lengthUnit": "ft"})).is_err());
}

#[test]
fn student_copy_keeps_the_size_and_hides_the_answers() {
    let mut s = Session::new(Document::new());
    s.run("exercise.insert", &json!({"title": "Trave", "text": "Traccia i diagrammi.", "questions": ["Reazioni?"]})).unwrap();
    let at = s.doc.body.iter().position(|b| b.as_para().is_some_and(|p| p.plain_text().contains("Traccia i diagrammi"))).unwrap();
    let len = s.doc.body[at].as_para().unwrap().text.len().saturating_sub(3);
    s.sel = crate::Selection::caret(wordcraft_doc::Pos::body(at, len));
    s.run("structure.insert", &json!({})).unwrap();
    let (wt, ht, gt) = first_structure(&s.doc).unwrap();
    s.run("exercise.show", &json!({"solution": false})).unwrap();
    let (ws, hs, gs) = first_structure(&s.doc).unwrap();
    assert!((wt - ws).abs() < 1e-3 && (ht - hs).abs() < 1e-3);
    assert!(gs.items.len() < gt.items.len(), "{} vs {}", gs.items.len(), gt.items.len());
}

#[test]
fn structures_survive_saving() {
    let mut s = Session::new(Document::new());
    s.run("structure.insert", &json!({"template": "truss"})).unwrap();
    let objs = crate::strix_objects::strix_objects(&s.doc);
    assert!(objs.iter().any(|o| o.kind == crate::strix_objects::StrixObjectKind::Structure && o.count == 1), "{objs:?}");
    let (_, _, before) = first_structure(&s.doc).unwrap();
    for name in ["t.docx", "t.xmath"] {
        let bytes = crate::io::save_bytes(name, &s.doc).unwrap();
        let loaded = crate::io::open_bytes(name, &bytes).unwrap();
        let mut s2 = Session::new(loaded);
        s2.run("calc.recalculate", &json!({})).unwrap();
        let (_, _, g) = first_structure(&s2.doc).unwrap_or_else(|| panic!("{name}: the structure is lost"));
        assert_eq!(g.structure, before.structure, "{name}");
        assert!(!g.items.is_empty(), "{name}: drawn again");
    }
}

/// The document's text with every calculation's result, as `strix-cli text` shows it.
fn text_of(s: &Session) -> String {
    // Units are set apart by narrow spaces: plain ones here, so tests read as typed.
    String::from_utf8(crate::io::save_bytes("t.txt", &s.doc).unwrap()).unwrap().replace(['\u{202F}', '\u{2009}', '\u{00A0}'], " ").replace('∕', "/")
}

/// Does `t` hold `part`, spaces aside?
fn has(t: &str, part: &str) -> bool {
    let squeeze = |x: &str| x.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    squeeze(t).contains(&squeeze(part))
}

fn working_of(structure: serde_json::Value) -> Result<String, String> {
    let mut s = Session::new(Document::new());
    s.run("structure.insert", &json!({"structure": structure})).unwrap();
    s.run("structure.working", &json!({})).map_err(|e| e.to_string())?;
    Ok(text_of(&s))
}

fn template_working(template: &str) -> String {
    let mut s = Session::new(Document::new());
    s.run("structure.insert", &json!({"template": template})).unwrap();
    let r = s.run("structure.working", &json!({})).unwrap();
    assert!(r["lines"].as_u64().unwrap() > 5, "{r}");
    text_of(&s)
}

#[test]
fn the_working_solves_the_reactions_and_writes_the_laws() {
    let t = template_working("beam");
    assert!(!t.contains('⚠'), "{t}");
    assert!(has(&t, "ΣM_A = 0:") && has(&t, "V_B·(6 m)") && has(&t, "V_B&=30 kN"), "{t}");
    assert!(has(&t, "M_1(x)=(30 kN)·x-(5 kN/m)·x^2"), "{t}");
    // Equations start with their zero: `0 kN·m = V_B·(6 m) - …`.
    assert!(has(&t, "0 kN·m&=V_B·(6 m)"), "{t}");
    assert!(has(&t, "M_1(3 m)=45 kN·m"), "{t}");
    // A frame: the roller's reaction from the rotation about the pin, exact fractions in the laws.
    let t = template_working("frame");
    assert!(!t.contains('⚠'), "{t}");
    assert!(has(&t, "V_D&=36.667 kN") && has(&t, "H_A&=-10 kN"), "{t}");
    assert!(has(&t, "N_1(x)=(-70 kN)/3") && has(&t, "M_2(6 m)=0 kN·m"), "{t}");
    // A truss: each member's axial force.
    let t = template_working("truss");
    assert!(!t.contains('⚠'), "{t}");
    assert!(has(&t, "N_3=(-25 kN)/2") && has(&t, "N_1=(15 kN)/2"), "{t}");
    // Hyperstatic: one reaction from the stiffness solution, said so, the others by equilibrium.
    let t = template_working("continuous");
    assert!(!t.contains('⚠'), "{t}");
    assert!(has(&t, "Hyperstatic"), "{t}");
    assert!(has(&t, "=62.5 kN") || has(&t, "V_B=(125 kN)/2"), "{t}");
}

#[test]
fn the_working_uses_the_hinge_of_a_gerber_beam() {
    let t = working_of(json!({
        "nodes": [{"id": "A", "x": 0, "y": 0}, {"id": "B", "x": 2, "y": 0, "hinge": true}, {"id": "C", "x": 6, "y": 0}],
        "members": [{"id": "1", "a": "A", "b": "B"}, {"id": "2", "a": "B", "b": "C"}],
        "supports": [{"node": "A", "kind": "fixed", "angle": -90}, {"node": "C", "kind": "roller"}],
        "loads": [{"type": "force", "member": "2", "at": 2, "value": 10}, {"type": "distributed", "member": "1", "q1": 0, "q2": 6}]
    }))
    .unwrap();
    assert!(!t.contains('⚠'), "{t}");
    assert!(has(&t, "ΣM_B = 0 (part about the hinge B, Member 2):") && has(&t, "V_C·(4 m)") && has(&t, "V_C&=5 kN"), "{t}");
    assert!(has(&t, "M_1(2 m)=0 kN·m") && has(&t, "M_1(0 m)=-18 kN·m"), "{t}");
}

#[test]
fn a_labile_structure_has_no_working() {
    let r = working_of(json!({
        "nodes": [{"id": "A", "x": 0, "y": 0}, {"id": "B", "x": 6, "y": 0}],
        "members": [{"id": "1", "a": "A", "b": "B"}],
        "supports": [{"node": "A", "kind": "roller"}, {"node": "B", "kind": "roller"}]
    }));
    assert!(r.is_err_and(|e| e.contains("labile")));
}

#[test]
fn the_working_follows_the_document_decimal_comma() {
    let mut s = Session::new(Document::new());
    s.run("calc.recalculate", &json!({"decimalComma": true})).unwrap();
    s.run(
        "structure.insert",
        &json!({"structure": {
            "nodes": [{"id": "A", "x": 0, "y": 0}, {"id": "B", "x": 2.5, "y": 0}],
            "members": [{"id": "1", "a": "A", "b": "B"}],
            "supports": [{"node": "A", "kind": "pin"}, {"node": "B", "kind": "roller"}],
            "loads": [{"type": "force", "member": "1", "at": 1.25, "value": 7.3}]
        }}),
    )
    .unwrap();
    s.run("structure.working", &json!({})).unwrap();
    let t = text_of(&s);
    assert!(!t.contains('⚠'), "{t}");
}
