//! What an exercise's first line holds after its number (docs/exercise.md, Title line): a
//! **title** (bold, the title's placeholder), or the **statement** itself — no title at all, the
//! instruction or the question written right after `Esercizio 1.` in the body text's weight and
//! size, its placeholder the statement's (`Write the question here.`). Kept on the title
//! control (`strix:titleLine`, an element Word ignores); the number keeps the title's look.

use serde_json::{Value, json};
use wordcraft_doc::control::ContentControl;
use wordcraft_doc::para::OBJ;
use wordcraft_doc::props::CharProps;
use wordcraft_doc::{Block, InlineObject, Paragraph};

use super::items::{Kind, read_kind, span};
use super::{TAG_EXERCISE, TAG_TITLE, Words, exercise_words, scan, target};
use crate::{CmdError, CmdResult, Session, p};

const EL: &str = "<strix:titleLine val=\"statement\"/>";
/// The body text's size when the document's Normal style names none (Word's default).
const NORMAL_SIZE: f32 = 11.0;

/// What the first line holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Line {
    Title,
    Statement,
}

impl Line {
    fn name(self) -> &'static str {
        match self {
            Line::Title => "title",
            Line::Statement => "statement",
        }
    }

    fn parse(t: &str) -> Option<Line> {
        match t {
            "title" => Some(Line::Title),
            "statement" => Some(Line::Statement),
            _ => None,
        }
    }
}

pub(crate) fn read(ctl: &ContentControl) -> Line {
    if ctl.extra.iter().any(|x| x == EL) { Line::Statement } else { Line::Title }
}

/// The placeholder a title control shows for `line` in an exercise of `kind`.
pub(super) fn placeholder(w: &Words, line: Line, kind: Kind) -> &'static str {
    match line {
        Line::Title => w.title,
        Line::Statement if kind.quiz() => w.question,
        Line::Statement => w.text,
    }
}

/// The exercise's kind and the title control's line, read from the title paragraph `p`.
pub(super) fn of_para(p: &Paragraph) -> Option<(Kind, Line)> {
    let mut kind = Kind::default();
    for o in &p.objects {
        if let InlineObject::ControlStart { control } = o {
            if control.tag == TAG_EXERCISE {
                kind = read_kind(control);
            } else if control.tag == TAG_TITLE {
                return Some((kind, read(control)));
            }
        }
    }
    None
}

/// The run properties of a statement written on the title line: the body text's weight and size.
fn statement_props(s: &Session) -> CharProps {
    let size = s.doc.styles.get("Normal").and_then(|st| st.chr.size).unwrap_or(NORMAL_SIZE);
    CharProps { bold: Some(false), size: Some(size), ..CharProps::default() }
}

/// Sets `line` on the title paragraph `p`: the control's element, placeholder and run properties,
/// and the text's weight and size. Whether anything changed.
fn apply(p: &mut Paragraph, line: Line, w: &Words, props: &CharProps) -> bool {
    let Some((kind, old)) = of_para(p) else { return false };
    let Some((a, b)) = span(p, TAG_TITLE) else { return false };
    let shown = placeholder(w, line, kind);
    let mut showing = false;
    for o in &mut p.objects {
        if let InlineObject::ControlStart { control } = o
            && control.tag == TAG_TITLE
        {
            control.extra.retain(|x| x != EL);
            if line == Line::Statement {
                control.extra.push(EL.into());
                control.run_props = Some(props.clone());
            } else {
                control.run_props = None;
            }
            control.placeholder_text = shown.into();
            showing = control.showing_placeholder;
        }
    }
    let from = a + OBJ.len_utf8();
    // A placeholder showing reads the new one.
    if showing && p.text.get(from..b) != Some(shown) {
        let _ = super::items::set_content(p, (a, b), shown);
    }
    let (_, b) = span(p, TAG_TITLE).unwrap_or((a, b));
    // The text's weight and size: the statement's, or the title style's again.
    let (bold, size) = if line == Line::Statement { (props.bold, props.size) } else { (None, None) };
    let differs = p.run_ranges().any(|(r, props)| r.start < b && r.end > from && (props.bold != bold || props.size != size));
    if differs && b > from {
        let _ = p.format(from, b, &|props| {
            props.bold = bold;
            props.size = size;
        });
    }
    let changed = differs || old != line;
    if changed {
        p.touch();
    }
    changed
}

/// `exercise.titleLine`: what the first line of an exercise holds after its number.
pub(super) fn command(s: &mut Session, v: &Value) -> CmdResult {
    let e = target(s, v)?;
    let layout = scan(s);
    let ex = layout.exercises.get(e).ok_or_else(|| CmdError::Params("no such exercise".into()))?;
    let start = ex.start;
    let want = match p::str(v, "line") {
        None => None,
        Some(t) => Some(Line::parse(t).ok_or_else(|| CmdError::Params("line: \"title\" or \"statement\"".into()))?),
    };
    if let Some(line) = want {
        let w = exercise_words(s, &layout, e);
        let props = statement_props(s);
        let block = s.doc.body.get_mut(start).ok_or_else(|| CmdError::Failed("exercise vanished".into()))?;
        let Block::Para(p) = std::sync::Arc::make_mut(block) else { return Err(CmdError::Failed("the exercise has no title line".into())) };
        if apply(p, line, w, &props) {
            s.touch();
            super::update(s);
            s.clamp_selection();
        }
    }
    let line = s.doc.body.get(start).and_then(|b| b.as_para()).and_then(of_para).map_or(Line::Title, |(_, l)| l);
    Ok(json!({"exercise": e + 1, "line": line.name()}))
}

/// `exercise.insert`'s `"titleLine"`: the new exercise's first line (its title paragraph `p`).
pub(super) fn on_insert(s: &Session, p: &mut Paragraph, v: &Value) {
    if p::str(v, "titleLine").and_then(Line::parse) == Some(Line::Statement) {
        let props = statement_props(s);
        apply(p, Line::Statement, super::words(s), &props);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn run(s: &mut Session, id: &str, v: Value) -> Value {
        s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
    }

    fn title_para(s: &Session) -> Paragraph {
        s.doc.body.first().and_then(|b| b.as_para()).cloned().unwrap_or_default()
    }

    #[test]
    fn the_statement_goes_next_to_the_number() {
        let mut s = Session::new(wordcraft_doc::Document::new());
        run(&mut s, "exercise.insert", json!({"kind": "open"}));
        assert_eq!(run(&mut s, "exercise.titleLine", json!({}))["line"], "title");
        let st = run(&mut s, "exercise.titleLine", json!({"line": "statement"}));
        assert_eq!(st["line"], "statement");
        let p = title_para(&s);
        // The placeholder is the question's, in the body text's weight.
        assert!(super::super::plain(&p).contains("Write the question here."), "{}", super::super::plain(&p));
        let (a, _) = span(&p, TAG_TITLE).unwrap_or((0, 0));
        assert_eq!(p.props_of_char(a + OBJ.len_utf8()).bold, Some(false));
        // Typed over, the text keeps that weight.
        run(&mut s, "text.insert", json!({"text": "Rispondi alle domande"}));
        let p = title_para(&s);
        assert!(super::super::plain(&p).contains("Rispondi alle domande"));
        let (a, _) = span(&p, TAG_TITLE).unwrap_or((0, 0));
        assert_eq!(p.props_of_char(a + OBJ.len_utf8()).bold, Some(false));
        // Back to a title: bold again (the style's), the text stays.
        run(&mut s, "exercise.titleLine", json!({"line": "title"}));
        let p = title_para(&s);
        assert_eq!(p.props_of_char(a + OBJ.len_utf8()).bold, None);
        assert!(super::super::plain(&p).contains("Rispondi alle domande"));
    }

    #[test]
    fn inserted_with_the_statement_on_the_first_line() {
        let mut s = Session::new(wordcraft_doc::Document::new());
        run(&mut s, "exercise.insert", json!({"kind": "problem", "titleLine": "statement"}));
        assert_eq!(run(&mut s, "exercise.titleLine", json!({}))["line"], "statement");
        assert!(super::super::plain(&title_para(&s)).contains("Write the exercise here"));
        assert!(s.run("exercise.titleLine", &json!({"line": "nothing"})).is_err());
    }
}
