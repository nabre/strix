//! Inline values (`strix:expr`): deleted with Backspace or Delete, shown three ways, and left out
//! of the data part when they recall a datum.
use crate::Session;
use serde_json::{Value, json};
use wordcraft_doc::para::OBJ;
use wordcraft_doc::{Block, Document, Path, Pos, StoryRef};

fn run(s: &mut Session, id: &str, v: Value) -> Value {
    s.run(id, &v).unwrap()
}
fn text(s: &Session) -> String {
    s.doc.plain_text(StoryRef::Body)
}

/// An exercise whose statement recalls `R` and `L`, with data `R`, `L` and `h`.
fn exercise() -> Session {
    let mut s = Session::new(Document::new());
    run(&mut s, "exercise.insert", json!({"title": "Galleria", "text": "Raggio {{R}} e lato {{L -> cm}}.", "questions": ["Area"]}));
    let item = |sym: &str, v: &str| json!({"symbol": sym, "description": "", "unit": "m", "values": [v]});
    run(
        &mut s,
        "exercise.setData",
        json!({"exercise": 1, "data": {"items": [item("R", "1.5"), item("L", "2"), item("h", "3")], "variants": ["A"], "active": 0}}),
    );
    s
}

/// The statement paragraph and the spans of its values.
fn statement(s: &Session) -> (usize, Vec<(usize, usize)>) {
    let top = s.doc.body.iter().position(|b| matches!(&**b, Block::Para(p) if p.text.contains("Raggio"))).unwrap();
    let Some(Block::Para(p)) = s.doc.body.get(top).map(|b| &**b) else { return (top, Vec::new()) };
    (top, super::expr::expr_spans(p))
}

fn caret(s: &mut Session, top: usize, off: usize) {
    s.sel = crate::Selection::caret(Pos::new(StoryRef::Body, Path::top(top), off));
}

fn data_lines(s: &Session) -> Vec<String> {
    text(s).lines().filter(|l| l.contains(" = ")).map(str::to_string).collect()
}

#[test]
fn data_recalled_in_the_statement_has_no_line_in_the_data_part() {
    let s = exercise();
    let t = text(&s);
    assert!(t.contains("Raggio 1.5 m e lato 200 cm."), "{t}");
    assert_eq!(data_lines(&s), vec!["h = 3 m"], "{t}");
}

#[test]
fn backspace_and_delete_take_an_inline_value_whole() {
    let mut s = exercise();
    let (top, spans) = statement(&s);
    // Backspace just after the first value.
    caret(&mut s, top, spans[0].1 + OBJ.len_utf8());
    run(&mut s, "text.backspace", json!({}));
    let t = text(&s);
    assert!(t.contains("Raggio  e lato 200 cm."), "{t}");
    // The datum is no longer shown in the statement: the data part lists it again.
    assert!(data_lines(&s).iter().any(|l| l == "R = 1.5 m"), "{t}");
    // Delete just before the remaining value, then Undo brings it back.
    let (top, spans) = statement(&s);
    caret(&mut s, top, spans[0].0);
    run(&mut s, "text.delete", json!({}));
    assert!(text(&s).contains("e lato ."), "{}", text(&s));
    run(&mut s, "edit.undo", json!({}));
    assert!(text(&s).contains("e lato 200 cm."), "{}", text(&s));
}

#[test]
fn a_selection_ending_inside_a_value_takes_it_whole() {
    let mut s = exercise();
    let (top, spans) = statement(&s);
    let start = s.doc.para(StoryRef::Body, &Path::top(top)).unwrap().text.find("Raggio").unwrap();
    s.sel = crate::Selection {
        anchor: Pos::new(StoryRef::Body, Path::top(top), start),
        focus: Pos::new(StoryRef::Body, Path::top(top), spans[0].0 + OBJ.len_utf8() + 1),
    };
    run(&mut s, "text.delete", json!({}));
    assert!(text(&s).contains(" e lato 200 cm."), "{}", text(&s));
    assert!(!text(&s).contains("Raggio"), "{}", text(&s));
}

#[test]
fn an_inline_value_is_shown_three_ways_and_edited() {
    let mut s = exercise();
    let (top, spans) = statement(&s);
    caret(&mut s, top, spans[0].0 + OBJ.len_utf8());
    let info = run(&mut s, "exercise.inlineValue", json!({}));
    assert_eq!((info["expr"].as_str(), info["show"].as_str(), info["shown"].as_str()), (Some("R"), Some("value"), Some("1.5 m")));
    run(&mut s, "exercise.inlineValue", json!({"show": "both"}));
    assert!(text(&s).contains("Raggio R = 1.5 m e"), "{}", text(&s));
    // Recalculating keeps it so.
    run(&mut s, "calc.recalculate", json!({}));
    assert!(text(&s).contains("Raggio R = 1.5 m e"), "{}", text(&s));
    run(&mut s, "exercise.inlineValue", json!({"show": "number"}));
    assert!(text(&s).contains("Raggio 1.5 e"), "{}", text(&s));
    run(&mut s, "exercise.inlineValue", json!({"show": "value", "expr": "R -> cm"}));
    assert!(text(&s).contains("Raggio 150 cm e"), "{}", text(&s));
    // Still a recall of R: no data line for it.
    assert_eq!(data_lines(&s), vec!["h = 3 m"]);
    assert!(s.run("exercise.inlineValue", &json!({"show": "bogus"})).is_err());
    assert!(s.run("exercise.inlineValue", &json!({"expr": ""})).is_err());
    run(&mut s, "exercise.inlineValue", json!({"remove": true}));
    assert!(text(&s).contains("Raggio  e"), "{}", text(&s));
    assert!(s.run("exercise.inlineValue", &json!({})).is_err());
}

#[test]
fn values_from_before_are_unlocked_for_deletion() {
    let mut s = exercise();
    let (top, spans) = statement(&s);
    // A document saved when values could not be deleted.
    if let Some(Block::Para(p)) = s.doc.body.get_mut(top).map(std::sync::Arc::make_mut)
        && let Some(wordcraft_doc::InlineObject::ControlStart { control }) = p.object_at_mut(spans[0].0)
    {
        control.lock = wordcraft_doc::control::ControlLock::SdtContentLocked;
    }
    caret(&mut s, top, spans[0].1 + OBJ.len_utf8());
    run(&mut s, "text.backspace", json!({}));
    assert!(text(&s).contains("Raggio  e lato"), "{}", text(&s));
}

#[test]
fn data_recalled_by_a_drawing_in_the_statement_has_no_line() {
    let mut s = exercise();
    let (top, _) = statement(&s);
    let end = s.doc.para(StoryRef::Body, &Path::top(top)).unwrap().text.len();
    caret(&mut s, top, end);
    let drawing = json!({"unit": "m", "scale": 50, "entities": [
        {"type": "text", "p": [0, 0], "value": "{{h:both}}", "height": 3.5},
        {"type": "text", "p": [0, 1], "value": "{{R:symbol}}", "height": 3.5}
    ]});
    run(&mut s, "insert.cad", json!({"drawing": drawing}));
    run(&mut s, "calc.recalculate", json!({}));
    // h is shown by the drawing, R and L by the text: no data part left at all.
    assert!(data_lines(&s).is_empty(), "{}", text(&s));
}
