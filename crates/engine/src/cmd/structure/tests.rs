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
    assert!(has(&t, "ΣM_A = 0:") && has(&t, "V_B·6 m") && has(&t, "V_B&=30 kN"), "{t}");
    assert!(has(&t, "M_1(x)=V_A·x-q_1·(x^2)/2"), "{t}");
    // Equations start with their zero: `0 kN·m = V_B·6 m - …`.
    assert!(has(&t, "0 kN·m&=V_B·6 m"), "{t}");
    assert!(has(&t, "M_1(3 m)=45 kN·m"), "{t}");
    // A frame: the roller's reaction from the rotation about the pin, exact fractions in the laws.
    let t = template_working("frame");
    assert!(!t.contains('⚠'), "{t}");
    assert!(has(&t, "V_D&=36.667 kN") && has(&t, "H_A&=-10 kN"), "{t}");
    assert!(has(&t, "N_1(x)=-V_A") && has(&t, "M_2(6 m)=0 kN·m"), "{t}");
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
    assert!(has(&t, "ΣM_B = 0 (part about the hinge B, Member 2):") && has(&t, "V_C·4 m") && has(&t, "V_C&=5 kN"), "{t}");
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

/// Moment arms come from the drawing's chains and the loads' lengths, never a centroid's distance
/// the drawing doesn't show (q_2 on 3.5…4.5 m: `(3 m + 0.5 m + (1 m)/2)`, not `4 m`).
#[test]
fn moment_arms_are_written_from_the_dimensions() {
    let t = working_of(json!({
        "nodes": [{"id": "A", "x": 0, "y": 0}, {"id": "B", "x": 6, "y": 0}],
        "members": [{"id": "1", "a": "A", "b": "B"}],
        "supports": [{"node": "A", "kind": "pin"}, {"node": "B", "kind": "roller"}],
        "loads": [
            {"type": "distributed", "member": "1", "q1": 10, "to": 3},
            {"type": "distributed", "member": "1", "q1": 10, "from": 3.5, "to": 4.5},
            {"type": "distributed", "member": "1", "q1": 0, "q2": 6, "from": 4.5}
        ]
    }))
    .unwrap();
    assert!(!t.contains('⚠'), "{t}");
    let first = t.lines().find(|l| l.contains("0 kN·m") && l.contains("V_B")).unwrap_or_default();
    assert!(has(first, "q_1·3 m·(3 m)/2"), "{first}");
    assert!(has(first, "q_2·1 m·(3 m + 0.5 m + (1 m)/2)"), "{first}");
    // The triangle grows towards B: its centroid two thirds of its length from its start.
    assert!(has(first, "·(6 m - (1.5 m)/3)"), "{first}");
    assert!(!has(first, "·4 m") && !has(first, "·5.5 m"), "{first}");
    // About B, the whole length less half the load's: fewer terms than the chain up to it.
    let about_b = t.lines().find(|l| l.contains("0 kN·m") && l.contains("V_A")).unwrap_or_default();
    assert!(has(about_b, "q_1·3 m·(6 m - (3 m)/2)"), "{about_b}");
    // q·3·1.5 + q·1·4 + 3·1.5/2·(4.5+1) = 45 + 40 + 24.75 = 109.75 → V_B = 18.2917.
    assert!(has(&t, "V_B&=18.292 kN"), "{t}");
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

/// Each load is named, one count through all the kinds by place (`q_1`, a varying load's ends
/// `q_(2,1)` and `q_(2,2)`, `F_4`, `C_5`): the working defines the names in that order and its
/// equations use them, and calc solves them to the same reactions.
#[test]
fn the_working_names_the_loads() {
    let t = working_of(json!({
        "nodes": [{"id": "A", "x": 0, "y": 0}, {"id": "B", "x": 6, "y": 0}],
        "members": [{"id": "1", "a": "A", "b": "B"}],
        "supports": [{"node": "A", "kind": "pin"}, {"node": "B", "kind": "roller"}],
        "loads": [
            {"type": "distributed", "member": "1", "q1": 10},
            {"type": "force", "member": "1", "at": 3, "value": 0},
            {"type": "force", "member": "1", "at": 2, "value": 12},
            {"type": "distributed", "member": "1", "q1": 2, "q2": 5},
            {"type": "distributed", "member": "1", "q1": 0, "q2": 4},
            {"type": "moment", "node": "B", "value": 6}
        ]
    }))
    .unwrap();
    assert!(!t.contains('⚠'), "{t}");
    // Defined with their sizes; the zero force is not numbered.
    assert!(has(&t, "Loads:"), "{t}");
    let defs = ["q_1=(10 kN/m)", "q_(2,1)=(2 kN/m)", "q_(2,2)=(5 kN/m)", "q_(3,2)=(4 kN/m)", "F_4=(12 kN)", "C_5=(6 kN·m)"];
    let flat: String = t.chars().filter(|c| !c.is_whitespace()).collect();
    let mut from = 0;
    for d in defs {
        let d: String = d.chars().filter(|c| !c.is_whitespace()).collect();
        let Some(at) = flat.get(from..).and_then(|rest| rest.find(&d)) else { panic!("{d} after the previous one: {t}") };
        from += at + d.len();
    }
    assert!(!has(&t, "F_1") && !has(&t, "C_1") && !has(&t, "F_6") && !has(&t, "q_(3,1)"), "{t}");
    // The equations use the names: rectangle and triangle of the trapezoid, the triangle alone.
    for term in ["q_1·6 m", "F_4·2 m", "q_(2,1)·6 m", "(q_(2,2)-q_(2,1))·(6 m)/2", "q_(3,2)·(6 m)/2", "·2·(6 m)/3", "+C_5"] {
        assert!(has(&t, term), "{term}: {t}");
    }
    // ΣM_A: V_B·6 = 10·6·3 + 12·2 + 2·6·3 + 3·6/2·4 + 4·6/2·4 − 6 = 318.
    assert!(has(&t, "V_B&=53 kN") && has(&t, "V_A&=52 kN"), "{t}");
}

/// Selects the `k`th structure of the body.
fn select_structure(s: &mut Session, k: usize) {
    use wordcraft_doc::para::OBJ;
    use wordcraft_doc::{Path, Pos, StoryRef};
    let mut found = Vec::new();
    for (i, b) in s.doc.body.iter().enumerate() {
        let Block::Para(p) = &**b else { continue };
        for (off, o) in p.objects_with_offsets() {
            if matches!(o, InlineObject::Graphic { graphic, .. } if graphic.kind == GraphicKind::Structure) {
                found.push((i, off));
            }
        }
    }
    let (i, off) = found[k];
    let at = Pos::new(StoryRef::Body, Path(vec![i as u32]), off);
    s.sel = crate::Selection { anchor: at.clone(), focus: Pos { off: off + OBJ.len_utf8(), ..at } };
}

/// The uniform load of the beam template from 0 to `to`.
fn shortened(to: f64) -> serde_json::Value {
    json!({"index": 0, "load": {"type": "distributed", "member": "1", "q1": 10, "to": to, "angle": -90}})
}

#[test]
fn the_working_follows_the_structure() {
    let mut s = Session::new(Document::new());
    s.run("structure.insert", &json!({"template": "beam"})).unwrap();
    s.run("structure.working", &json!({"labels": {"heading": "Svolgimento"}})).unwrap();
    let t = text_of(&s);
    assert!(has(&t, "q_1·6 m·(6 m)/2") && has(&t, "V_B&=30 kN"), "{t}");
    // The load shortened in the editor: the working is written again in its place, in its words.
    s.run("structure.load", &shortened(4.0)).unwrap();
    let t = text_of(&s);
    assert!(has(&t, "q_1·4 m·(4 m)/2") && !has(&t, "q_1·6 m·(6 m)/2"), "{t}");
    assert!(has(&t, "V_B&=13.333 kN"), "{t}");
    assert_eq!(t.matches("Svolgimento").count(), 1, "{t}");
    assert_eq!(t.matches("ΣM_A").count(), 1, "written once: {t}");
    // The structure is still the one selected.
    assert_eq!(s.run("structure.get", &json!({})).unwrap()["status"], "isostatic");
    // One undo step brings the working back with the load.
    s.undo();
    let t = text_of(&s);
    assert!(has(&t, "q_1·6 m·(6 m)/2") && !has(&t, "q_1·4 m·(4 m)/2"), "{t}");
    // Writing it again rewrites it in place.
    select_structure(&mut s, 0);
    s.run("structure.working", &json!({"labels": {"heading": "Svolgimento"}})).unwrap();
    assert_eq!(text_of(&s).matches("ΣM_A").count(), 1, "{}", text_of(&s));
}

#[test]
fn a_working_waits_while_its_structure_cannot_be_solved() {
    let mut s = Session::new(Document::new());
    s.run("structure.insert", &json!({"template": "beam"})).unwrap();
    s.run("structure.working", &json!({"labels": {"unsolved": "Non risolvibile."}})).unwrap();
    s.run("structure.support", &json!({"node": "B", "remove": true})).unwrap();
    let t = text_of(&s);
    assert!(t.contains("Non risolvibile.") && !t.contains("ΣM_A"), "{t}");
    s.run("structure.support", &json!({"node": "B", "kind": "roller"})).unwrap();
    let t = text_of(&s);
    assert!(!t.contains("Non risolvibile.") && has(&t, "V_B&=30 kN"), "{t}");
}

#[test]
fn an_exercise_working_follows_any_of_its_drawings() {
    let mut s = Session::new(Document::new());
    s.run("structure.exercise", &json!({"labels": {"loads": "Carichi"}})).unwrap();
    assert!(has(&text_of(&s), "q_1·6 m·(6 m)/2"));
    // The diagram of M in the solution, changed: the statement and every part follow.
    select_structure(&mut s, 4);
    s.run("structure.load", &shortened(4.0)).unwrap();
    let t = text_of(&s);
    assert!(has(&t, "q_1·4 m·(4 m)/2") && !has(&t, "q_1·6 m·(6 m)/2"), "{t}");
    assert_eq!(t.matches("Carichi").count(), 1, "{t}");
    assert_eq!(s.run("structure.get", &json!({})).unwrap()["structure"]["view"], "moment");
}

#[test]
fn a_saved_working_still_follows_its_structure() {
    let mut s = Session::new(Document::new());
    s.run("structure.insert", &json!({"template": "beam"})).unwrap();
    s.run("structure.working", &json!({"labels": {"heading": "Svolgimento"}})).unwrap();
    for name in ["t.docx", "t.xmath"] {
        let bytes = crate::io::save_bytes(name, &s.doc).unwrap();
        let mut s2 = Session::new(crate::io::open_bytes(name, &bytes).unwrap());
        s2.run("calc.recalculate", &json!({})).unwrap();
        select_structure(&mut s2, 0);
        s2.run("structure.load", &shortened(4.0)).unwrap();
        let t = text_of(&s2);
        assert!(has(&t, "q_1·4 m·(4 m)/2") && !has(&t, "q_1·6 m·(6 m)/2"), "{name}: {t}");
        assert_eq!(t.matches("Svolgimento").count(), 1, "{name}: {t}");
    }
}

/// The laws are written with the reactions' and the loads' names, from the part before the
/// section, and calc works out their values from them; a member in a closed ring keeps its
/// numbers.
#[test]
fn the_laws_use_the_names_of_the_reactions_and_the_loads() {
    let t = working_of(json!({
        "nodes": [{"id": "A", "x": 0, "y": 0}, {"id": "B", "x": 6, "y": 0}],
        "members": [{"id": "1", "a": "A", "b": "B"}],
        "supports": [{"node": "A", "kind": "pin"}, {"node": "B", "kind": "roller"}],
        "loads": [{"type": "distributed", "member": "1", "q1": 10, "to": 4}, {"type": "force", "member": "1", "at": 5, "value": 20}]
    }))
    .unwrap();
    assert!(!t.contains('⚠'), "{t}");
    for law in [
        "T_1a(x)=V_A-q_1·x",
        "M_1a(x)=V_A·x-q_1·(x^2)/2",
        "T_1b(x)=V_A-q_1·4 m",
        "M_1b(x)=V_A·x-q_1·4 m·(x-2 m)",
        "T_1c(x)=V_A-q_1·4 m-F_2",
        "M_1c(x)=V_A·x-q_1·4 m·(x-2 m)-F_2·(x-5 m)",
    ] {
        assert!(has(&t, law), "{law}: {t}");
    }
    assert!(has(&t, "T_1a(4 m)=-10 kN") && has(&t, "M_1c(5 m)=30 kN·m"), "{t}");
    // A cantilever: the fixed end's couple.
    let t = template_working("cantilever");
    assert!(has(&t, "M_1(x)=V_A·x-M_A") && has(&t, "M_1(0 m)=-30 kN·m"), "{t}");
    // A closed frame: no part lies before a section alone, the numbers stay.
    let t = working_of(json!({
        "nodes": [{"id": "A", "x": 0, "y": 0}, {"id": "B", "x": 0, "y": 3}, {"id": "C", "x": 4, "y": 3}, {"id": "D", "x": 4, "y": 0}],
        "members": [{"id": "1", "a": "A", "b": "B"}, {"id": "2", "a": "B", "b": "C"}, {"id": "3", "a": "C", "b": "D"}, {"id": "4", "a": "D", "b": "A"}],
        "supports": [{"node": "A", "kind": "pin"}, {"node": "D", "kind": "roller"}],
        "loads": [{"type": "distributed", "member": "2", "q1": 10}]
    }))
    .unwrap();
    assert!(!t.contains('⚠') && has(&t, "M_2(x)=") && !has(&t, "M_2(x)=V_"), "{t}");
}
