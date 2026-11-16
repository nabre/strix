//! Data shown as values: the user's example, an exercise about a channel's water section.

use serde_json::{Value, json};
use wordcraft_doc::{Block, Document, Path, Pos, StoryRef};

use crate::Session;

fn run(s: &mut Session, id: &str, v: Value) -> Value {
    s.run(id, &v).unwrap()
}

fn text(s: &Session) -> String {
    s.doc.plain_text(StoryRef::Body)
}

/// The text of every calc equation, in order, without spaces.
fn equations(s: &Session) -> Vec<String> {
    super::super::regions(&s.doc.body).into_iter().filter_map(|(_, t)| t.map(|t| t.replace(char::is_whitespace, ""))).collect()
}

/// What the equations show (their linear text, without spaces).
fn shown(s: &Session) -> Vec<String> {
    let mut out = Vec::new();
    for b in &s.doc.body {
        if let Block::Para(p) = &**b {
            for o in &p.objects {
                if let wordcraft_doc::InlineObject::Equation { math, .. } = o
                    && math.calc
                {
                    out.push(wordcraft_doc::math::to_linear(&math.nodes).replace(char::is_whitespace, ""));
                }
            }
        }
    }
    out
}

/// An exercise whose statement shows the height only as a value (`{{h}}`: "2 m") and whose
/// data part declares `r`; the caret in its solution, where the working goes.
fn exercise(variants: &[(&str, &str)]) -> Session {
    let mut s = Session::new(Document::new());
    run(
        &mut s,
        "exercise.insert",
        json!({"title": "Canale", "text": "Calcola la sezione dell'acqua quando la sua altezza è di {{h}}.", "questions": ["Sezione"]}),
    );
    let h: Vec<&str> = variants.iter().map(|(h, _)| *h).collect();
    let r: Vec<&str> = variants.iter().map(|(_, r)| *r).collect();
    let names: Vec<String> = (0..variants.len()).map(|i| ((b'A' + u8::try_from(i).unwrap()) as char).to_string()).collect();
    let data = json!({"items": [
        {"symbol": "h", "description": "altezza", "unit": "m", "values": h},
        {"symbol": "r", "description": "larghezza", "unit": "m", "values": r}
    ], "variants": names, "active": 0});
    run(&mut s, "exercise.setData", json!({"exercise": 1, "data": data}));
    // The paragraph after the solution's repeated question.
    let q = s.doc.body.iter().rposition(|b| matches!(&**b, Block::Para(p) if p.text.contains("Sezione"))).unwrap();
    s.sel = crate::Selection::caret(Pos::new(StoryRef::Body, Path::top(q + 1), 0));
    s
}

fn last_shown(s: &Session) -> String {
    shown(s).last().cloned().unwrap_or_default()
}

#[test]
fn a_datum_shown_only_as_a_value_is_written_as_its_value() {
    let mut s = exercise(&[("2", "0.75"), ("3", "0.5")]);
    let t = text(&s);
    assert!(t.contains("altezza è di 2 m."), "{t}");
    // h has no data line (the statement shows it), r has one.
    assert!(t.contains("r = 0.75 m") && !t.contains("h = 2 m"), "{t}");
    run(&mut s, "calc.steps", json!({"value": true}));
    run(&mut s, "calc.insert", json!({"text": "A = h*r"}));
    run(&mut s, "calc.recalculate", json!({}));
    assert_eq!(last_shown(&s), "A=(2m)*r=(2m)·(0.75m)=1.5m²");
    // Read back, the calculation is still `A = h*r`: another variant works it out from h.
    assert_eq!(equations(&s).last().map(String::as_str), Some("A=h*r"));
    run(&mut s, "exercise.variant", json!({"exercise": 1, "active": 1}));
    assert_eq!(last_shown(&s), "A=(3m)*r=(3m)·(0.5m)=1.5m²");
    // Recalculating again changes nothing.
    run(&mut s, "calc.recalculate", json!({}));
    assert_eq!(last_shown(&s), "A=(3m)*r=(3m)·(0.5m)=1.5m²");
    // Saved and opened again, the source goes with the equation.
    let bytes = crate::io::save_bytes("w.docx", &s.doc).unwrap();
    let mut s2 = Session::new(crate::io::open_bytes("w.docx", &bytes).unwrap());
    assert_eq!(equations(&s2).last().map(String::as_str), Some("A=h*r"));
    run(&mut s2, "exercise.variant", json!({"exercise": 1, "active": 0}));
    assert_eq!(last_shown(&s2), "A=(2m)*r=(2m)·(0.75m)=1.5m²");
    // Changing the equation's number format keeps its source.
    let at = s.doc.body.iter().rposition(|b| matches!(&**b, Block::Para(p) if p.objects.iter().any(|o| matches!(o, wordcraft_doc::InlineObject::Equation { math, .. } if math.calc)))).unwrap();
    s.sel = crate::Selection::caret(Pos::new(StoryRef::Body, Path::top(at), 0));
    run(&mut s, "calc.options", json!({"scope": "equation", "digits": 4}));
    run(&mut s, "calc.recalculate", json!({}));
    assert_eq!(equations(&s).last().map(String::as_str), Some("A=h*r"));
}

#[test]
fn the_unknown_on_the_right_and_lines_show_values_too() {
    let mut s = exercise(&[("2", "0.75")]);
    run(&mut s, "calc.steps", json!({"value": true}));
    run(&mut s, "calc.insert", json!({"text": "h*r = A"}));
    run(&mut s, "calc.recalculate", json!({}));
    assert_eq!(last_shown(&s), "(2m)*r=A⇒A=(2m)·r=(2m)·(0.75m)=1.5m²");
    assert_eq!(equations(&s).last().map(String::as_str), Some("h*r=A"));
    // Line by line: the statement and every row of its working.
    run(&mut s, "calc.layout", json!({"value": "lines"}));
    let rows = last_shown(&s);
    assert!(rows.contains("(2m)*r&=A") && rows.contains("A&=(2m)·r") && rows.contains("A&=(2m)·(0.75m)"), "{rows}");
    assert_eq!(equations(&s).last().map(String::as_str), Some("h*r=A"));
}

#[test]
fn a_datum_whose_symbol_is_shown_keeps_its_name() {
    let mut s = exercise(&[("2", "0.75")]);
    // The height shown as `h = 2 m`: the reader knows h.
    let top = s.doc.body.iter().position(|b| matches!(&**b, Block::Para(p) if p.text.contains("altezza"))).unwrap();
    let p = s.doc.body.get(top).and_then(|b| b.as_para()).unwrap();
    let (a, _) = crate::cmd::exercise::expr::expr_spans(p)[0];
    let back = s.sel.clone();
    s.sel = crate::Selection::caret(Pos::new(StoryRef::Body, Path::top(top), a + wordcraft_doc::para::OBJ.len_utf8()));
    run(&mut s, "exercise.inlineValue", json!({"show": "both"}));
    s.sel = back;
    run(&mut s, "calc.steps", json!({"value": true}));
    run(&mut s, "calc.insert", json!({"text": "A = h*r"}));
    run(&mut s, "calc.recalculate", json!({}));
    assert_eq!(last_shown(&s), "A=h*r=(2m)·(0.75m)=1.5m²");
    // Outside an exercise, names are always kept.
    let mut plain = Session::new(Document::new());
    for t in ["h = 2 m", "r = 0.75 m", "A = h*r"] {
        run(&mut plain, "calc.insert", json!({"text": t}));
        run(&mut plain, "text.insert", json!({"text": " "}));
    }
    assert!(shown(&plain).iter().all(|e| !e.contains("(2m)*r")), "{:?}", shown(&plain));
}

/// The user's equation, built up (sin with the superscript −1 and Word's function application),
/// works out as an angle.
#[test]
fn a_built_up_inverse_sine_is_worked_out() {
    let mut plain = Session::new(Document::new());
    run(&mut plain, "calc.steps", json!({"value": true}));
    for t in ["R = 3 m", "r = 1 m", "α = sin^(−1)\u{2061}((R−r)/(R+r))"] {
        run(&mut plain, "calc.insert", json!({"text": t}));
        run(&mut plain, "text.insert", json!({"text": " "}));
    }
    run(&mut plain, "calc.recalculate", json!({}));
    let last = last_shown(&plain);
    assert!(last.starts_with("α=sin^(−1)\u{2061}") && last.ends_with("=30°"), "{last}");
    // The steps write the inverse as it is written, the −1 on the function's name.
    assert!(last.contains("=sin^(−1)\u{2061}(((3m)-(1m))/((3m)+(1m)))=30°"), "{last}");
    assert!(run(&mut plain, "calc.recalculate", json!({}))["errors"].as_array().unwrap().is_empty());
}
