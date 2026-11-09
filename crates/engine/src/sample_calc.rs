//! The "Calculation and exercise" template (original text, generated in code): live formulas
//! (calculations) and a worked exercise whose statement holds a CAD drawing sized by the
//! exercise's data. Built by running the same commands the ribbon runs, so it is exactly what a
//! user would get by making it by hand.

use serde_json::{Value, json};
use wordcraft_doc::props::CharProps;
use wordcraft_doc::{Block, Document, Paragraph, Pos, StoryRef};

use crate::{CmdError, Selection, Session};

/// The beam in the exercise's statement: a simply supported beam of span `L` (metres, from the
/// exercise's data) under a uniform load `q`, with its supports, load and span dimension.
const BEAM: &str = r##"{"unit":"m","scale":50,"layers":[{"name":"0"},{"name":"loads","width":0.35},{"name":"ground","width":0.18}],"entities":[
    {"type":"polyline","pts":[[0,0],["L",0],["L",0.25],[0,0.25]],"closed":true,"stroke":{"width":0.5}},
    {"type":"polyline","pts":[[0,0],[-0.2,-0.35],[0.2,-0.35]],"closed":true},
    {"type":"polyline","pts":[["L",0],["L-0.2",-0.35],["L+0.2",-0.35]],"closed":true},
    {"type":"circle","c":[0,0],"r":0.03},
    {"type":"circle","c":["L",0],"r":0.03},
    {"id":"g1","layer":"ground","type":"line","a":[-0.3,-0.35],"b":[0.3,-0.35]},
    {"id":"g2","layer":"ground","type":"line","a":["L-0.3",-0.45],"b":["L+0.3",-0.45]},
    {"layer":"ground","type":"strip","along":"g1","side":"right","width":2,"pattern":"lines"},
    {"layer":"ground","type":"strip","along":"g2","side":"right","width":2,"pattern":"lines"},
    {"layer":"ground","type":"line","a":["L-0.25",-0.39],"b":["L+0.25",-0.39]},
    {"layer":"loads","type":"line","a":[0,0.8],"b":["L",0.8]},
    {"layer":"loads","type":"line","a":[0,0.8],"b":[0,0.3],"arrows":"end"},
    {"layer":"loads","type":"line","a":["L/4",0.8],"b":["L/4",0.3],"arrows":"end"},
    {"layer":"loads","type":"line","a":["L/2",0.8],"b":["L/2",0.3],"arrows":"end"},
    {"layer":"loads","type":"line","a":["3*L/4",0.8],"b":["3*L/4",0.3],"arrows":"end"},
    {"layer":"loads","type":"line","a":["L",0.8],"b":["L",0.3],"arrows":"end"},
    {"layer":"loads","type":"text","p":["L/2",0.92],"value":"{{q:both}}","height":3.5,"align":"center"},
    {"type":"dim","a":[0,-0.45],"b":["L",-0.45],"offset":-8,"text":"{{L:both}}"}
]}"##;

fn para(text: &str, style: Option<&str>) -> Block {
    let mut p = Paragraph::with_text(text, CharProps::default());
    p.props.style = style.map(str::to_string);
    Block::Para(p)
}

/// The caret at the end of top-level body block `i`.
fn caret_at_end(s: &mut Session, i: usize) {
    let len = match s.doc.body.get(i).map(|b| &**b) {
        Some(Block::Para(p)) => p.text.len(),
        _ => 0,
    };
    s.sel = Selection::caret(Pos::new(StoryRef::Body, wordcraft_doc::Path(vec![u32::try_from(i).unwrap_or(u32::MAX)]), len));
}

/// The first top-level paragraph whose text (objects left out, trimmed) is `text`.
fn find(s: &Session, text: &str) -> Result<usize, CmdError> {
    s.doc
        .body
        .iter()
        .position(|b| matches!(&**b, Block::Para(p) if p.text.chars().filter(|&c| c != wordcraft_doc::para::OBJ).collect::<String>().trim() == text))
        .ok_or_else(|| CmdError::Failed(format!("template: no paragraph \"{text}\"")))
}

fn run(s: &mut Session, id: &str, params: Value) -> Result<(), CmdError> {
    s.run(id, &params).map(|_| ())
}

/// Calculations one per paragraph from the caret on: each statement, then Enter (none after the
/// last). With `answer`, the last one is marked as the result of that question.
fn calcs(s: &mut Session, lines: &[&str], answer: Option<u32>) -> Result<(), CmdError> {
    for (k, line) in lines.iter().enumerate() {
        if k > 0 {
            run(s, "text.newParagraph", json!({}))?;
        }
        run(s, "calc.insert", json!({"text": line, "display": true}))?;
    }
    if let Some(q) = answer {
        // Step back onto the equation just inserted.
        let mut at = s.sel.focus.clone();
        at.off = at.off.saturating_sub(wordcraft_doc::para::OBJ.len_utf8());
        s.sel = Selection::caret(at);
        run(s, "equation.answer", json!({"question": q}))?;
    }
    Ok(())
}

/// The template on `paper` (a blank document on the region's paper).
pub fn calc_exercise(paper: Document) -> Result<Document, CmdError> {
    let mut d = paper;
    d.core.title = "Calculation and exercise".into();
    d.core.creator = "Strix Writer".into();
    d.body = vec![
        para("Calculation and exercise", Some("Title")),
        para(
            "Formulas in this document are live calculations: change a value and every result after it follows. Type a statement such as \"b = 0.3 m\" in Math › Calculation, end it with = to see its value.",
            None,
        ),
        para("Formulas", Some("Heading1")),
        para("A rectangular section, its area and its moment of inertia:", None),
        para("", None),
    ]
    .into_iter()
    .map(std::sync::Arc::new)
    .collect();
    let mut s = Session::new(d);
    caret_at_end(&mut s, 4);
    calcs(&mut s, &["b = 0.3 m", "h = 0.5 m", "A = b·h =", "I = b·h^3/12 ="], None)?;
    // The exercise after the last formula (its title is the exercise's own heading).
    run(
        &mut s,
        "exercise.insert",
        json!({
            "title": "Simply supported beam",
            "text": "A beam of span L rests on a pin and a roller and carries a uniform load q along its whole length.",
            "questions": ["The support reactions", "The largest bending moment"],
        }),
    )?;
    // The data, in two variants; the drawing and the solution use them.
    run(
        &mut s,
        "exercise.setData",
        json!({"data": {
            "items": [
                {"symbol": "L", "description": "span", "unit": "m", "values": ["4.5", "6"]},
                {"symbol": "q", "description": "uniform load", "unit": "kN/m", "values": ["12", "8"]}
            ],
            "variants": ["A", "B"],
            "active": 0
        }}),
    )?;
    // The drawing under the statement.
    let statement = find(&s, "A beam of span L rests on a pin and a roller and carries a uniform load q along its whole length.")?;
    caret_at_end(&mut s, statement);
    run(&mut s, "text.newParagraph", json!({}))?;
    let drawing: Value = serde_json::from_str(BEAM).map_err(|e| CmdError::Failed(e.to_string()))?;
    run(&mut s, "insert.cad", json!({"drawing": drawing}))?;
    // The working under each repeated question.
    let first = find(&s, "a) The support reactions")? + 1;
    caret_at_end(&mut s, first);
    calcs(&mut s, &["R = q·L/2"], Some(1))?;
    let second = find(&s, "b) The largest bending moment")? + 1;
    caret_at_end(&mut s, second);
    calcs(&mut s, &["M = q·L^2/8"], Some(2))?;
    run(&mut s, "calc.recalculate", json!({}))?;
    s.sel = Selection::caret(s.doc.start_of(StoryRef::Body));
    Ok(s.doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(d: &Document) -> Vec<String> {
        d.body.iter().map(|b| if let Block::Para(p) = &**b { p.plain_text() } else { "<table>".into() }).collect()
    }

    #[test]
    fn the_template_calculates_and_draws() {
        let d = calc_exercise(Document::new()).unwrap();
        let t = texts(&d);
        // The formulas are live: their values are worked out.
        assert!(t.iter().any(|x| x.starts_with("A=b·h=") && x.ends_with("0.15\u{2009}m²")), "{t:?}");
        // The exercise: data, questions, worked solution and the results summary.
        for want in ["Simply supported beam", "Data (variant A)", "span: L = 4.5 m", "a) R = 27 kN", "b) M = 30.375 kN·m"] {
            assert!(t.iter().any(|x| x == want), "{want}: {t:?}");
        }
        // The statement holds the drawing, sized by the data: its span dimension reads 4.5.
        let statement = t.iter().position(|x| x.starts_with("A beam of span L")).unwrap();
        let Block::Para(p) = &*d.body[statement + 1] else { panic!("no drawing paragraph") };
        let graphic = p.objects.iter().find_map(|o| match o {
            wordcraft_doc::para::InlineObject::Graphic { graphic, .. } if graphic.kind == wordcraft_doc::graphic::GraphicKind::Cad => Some(graphic),
            _ => None,
        });
        let g = graphic.expect("a CAD drawing under the statement");
        let labels: Vec<&str> = g
            .items
            .iter()
            .filter_map(|i| if let wordcraft_doc::graphic::GraphicItem::Text { text, .. } = i { Some(text.as_str()) } else { None })
            .collect();
        assert!(labels.iter().any(|l| l.contains("4.5") || l.contains("4,5")), "{labels:?}");
    }

    /// The second variant: everything follows the data.
    #[test]
    fn the_template_follows_its_variant() {
        let mut s = Session::new(calc_exercise(Document::new()).unwrap());
        s.run("exercise.variant", &json!({"exercise": 1, "active": 1})).unwrap();
        let t = texts(&s.doc);
        assert!(t.iter().any(|x| x == "b) M = 36 kN·m"), "{t:?}");
    }
}
