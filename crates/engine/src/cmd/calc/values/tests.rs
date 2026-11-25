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
    // The steps write the inverse with its `a`.
    assert!(last.contains("=asin\u{2061}(((3m)-(1m))/((3m)+(1m)))=30°"), "{last}");
    assert!(run(&mut plain, "calc.recalculate", json!({}))["errors"].as_array().unwrap().is_empty());
}

/// From a conversion report: spaces around `/` and `^` don't make empty fractions or powers, and
/// the formula spells functions as its working does.
#[test]
fn spaces_around_fractions_and_powers_are_read_as_written() {
    let mut plain = Session::new(Document::new());
    for t in ["ε = atan(3 / 100) =", "r = 2 m", "A = r ^ 2·pi ="] {
        let r = run(&mut plain, "calc.insert", json!({"text": t}));
        assert!(r["shown"].is_string(), "{r}");
        run(&mut plain, "text.insert", json!({"text": " "}));
    }
    let all = shown(&plain);
    // The inverse keeps its `a`, applied to its argument.
    assert_eq!(all[0], "ε=atan\u{2061}(3/100)=1.718°", "{all:?}");
    assert_eq!(all[2], "A=r^2·pi=(2m)²·pi=12.566m²", "{all:?}");
}

/// A calculated table in one command: filled, calculated, the caret after it.
#[test]
fn a_calculated_table_from_its_cells() {
    let mut s = Session::new(Document::new());
    let r = run(&mut s, "calc.insertTable", json!({"cells": [["L [m]", "A = L^2"], ["2", ""], ["3", ""]]}));
    assert_eq!(r["cells"][1][1], "4 m²", "{r}");
    assert_eq!(r["cells"][2][1], "9 m²", "{r}");
    // The caret is in the paragraph after the table: a statement goes there, outside it.
    assert!(s.sel.focus.path.cell().is_none());
    run(&mut s, "calc.insert", json!({"text": "x = 2 m ="}));
    assert!(matches!(&*s.doc.body[0], Block::Table(_)));
    // Junk cells are refused, never a panic.
    let mut s2 = Session::new(Document::new());
    assert!(s2.run("calc.insertTable", &json!({"cells": [[{"a": 1}]]})).is_err());
    assert!(s2.run("calc.insertTable", &json!({"cells": []})).is_err());
}

/// A rounding step set on one equation: its result rounded up to it, the rounded value used
/// after, kept with the document.
#[test]
fn a_rounding_step_on_one_equation() {
    let mut s = Session::new(Document::new());
    run(&mut s, "calc.insert", json!({"text": "t = 0.37 m/2 = m"}));
    run(&mut s, "text.insert", json!({"text": " "}));
    s.sel = crate::Selection::caret(Pos::new(StoryRef::Body, Path::top(0), 0));
    let r = run(&mut s, "calc.options", json!({"scope": "equation", "step": 0.05, "stepMode": "up"}));
    assert_eq!((r["step"].as_str(), r["stepMode"].as_str()), (Some("0.05"), Some("up")), "{r}");
    assert_eq!(shown(&s)[0], "t=(0.37m)/2=0.2m");
    let end = s.doc.body[0].as_para().map_or(0, |p| p.text.len());
    s.sel = crate::Selection::caret(Pos::new(StoryRef::Body, Path::top(0), end));
    run(&mut s, "calc.insert", json!({"text": "u = 2·t ="}));
    assert_eq!(shown(&s)[1], "u=2·t=2·(0.2m)=0.4m", "the next equation reads the rounded value");
    // Down, by its mode alone.
    s.sel = crate::Selection::caret(Pos::new(StoryRef::Body, Path::top(0), 0));
    run(&mut s, "calc.options", json!({"scope": "equation", "stepMode": "down"}));
    assert_eq!(shown(&s)[0], "t=(0.37m)/2=0.15m");
    // Saved and opened again.
    let bytes = crate::io::save_bytes("w.docx", &s.doc).unwrap();
    let mut s2 = Session::new(crate::io::open_bytes("w.docx", &bytes).unwrap());
    run(&mut s2, "calc.recalculate", json!({}));
    assert_eq!(shown(&s2)[0], "t=(0.37m)/2=0.15m");
    // Refused: a bad step, a step in preferences.
    assert!(s.run("calc.options", &json!({"scope": "equation", "step": -1})).is_err());
    assert!(s.run("calc.options", &json!({"scope": "equation", "step": "abc"})).is_err());
    assert!(s.run("calc.options", &json!({"scope": "equation", "stepMode": "sideways"})).is_err());
    assert!(s.run("calc.options", &json!({"scope": "prefs", "step": 0.05})).is_err());
    // `none` takes it off.
    run(&mut s, "calc.options", json!({"scope": "equation", "step": "none"}));
    assert_eq!(shown(&s)[0], "t=(0.37m)/2=0.185m");
}

/// A calculation hidden with `calc.hide`: still calculated, its line wholly hidden (so it is
/// neither laid out nor printed while hidden text isn't shown), kept hidden in a saved file.
#[test]
fn a_hidden_calculation_still_counts() {
    let mut s = Session::new(Document::new());
    run(&mut s, "calc.insert", json!({"text": "k = 1.25"}));
    run(&mut s, "calc.hide", json!({}));
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "format.hidden", json!({"value": false}));
    run(&mut s, "calc.insert", json!({"text": "x = 2·k ="}));
    assert_eq!(shown(&s)[1], "x=2·k=2·1.25=2.5");
    let hidden = |s: &Session| {
        let p = s.doc.body[0].as_para().unwrap();
        p.mark.hidden == Some(true) && p.runs.iter().all(|r| r.props.hidden == Some(true))
    };
    assert!(hidden(&s));
    assert!(s.doc.body[1].as_para().unwrap().runs.iter().all(|r| r.props.hidden != Some(true)));
    let bytes = crate::io::save_bytes("w.docx", &s.doc).unwrap();
    let mut s2 = Session::new(crate::io::open_bytes("w.docx", &bytes).unwrap());
    // The equation keeps the choice; recalculating puts the hidden text back.
    run(&mut s2, "calc.recalculate", json!({}));
    assert!(hidden(&s2), "hidden after reopening");
    assert_eq!(shown(&s2)[1], "x=2·k=2·1.25=2.5");
    // Shown again by the same command.
    s.sel = crate::Selection::caret(Pos::new(StoryRef::Body, Path::top(0), 0));
    let r = run(&mut s, "calc.hide", json!({}));
    assert_eq!(r["hidden"], false);
    assert!(s.doc.body[0].as_para().unwrap().runs.iter().all(|r| r.props.hidden.is_none()));
    // Nothing to hide without an equation.
    assert!(Session::new(Document::new()).run("calc.hide", &json!({})).is_err());
}
