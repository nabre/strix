//! Keeping text where an exercise wants it (docs/exercise.md, Structure).
//!
//! - **Typing stays in the fields.** A question's paragraph is its text (`strix:exercise.item`)
//!   then its generated points; a title is its control after the exercise's start; an option
//!   starts with its box. A caret beside those fields (before the question's text, between it
//!   and the points, after the points, before an option's box) would type text that belongs to
//!   no field: the text goes into the nearest field instead. A calculation's result line is
//!   generated whole, so typing on it is refused.
//! - **Exercises never nest.** Pasting or inserting content that holds an exercise with the caret
//!   in another exercise (or in the results summary) puts it right after that exercise (before
//!   the summary).

use wordcraft_doc::para::OBJ;
use wordcraft_doc::{Block, InlineObject, Paragraph, Pos, StoryRef, para_block};

use super::items::{TAG_LINE, span};
use super::{Part, TAG_BOX, TAG_EXERCISE, TAG_ITEM, TAG_POINTS, TAG_TITLE, scan};
use crate::{CmdError, Selection, Session};

/// Why typing on a result line is refused.
const LINE_REFUSED: &str = "The result line is generated from the working: write the working in the space above it.";

/// Where a caret at `off` in `p` types instead, when it is beside an exercise field; `Err` when
/// nothing may be typed there.
fn redirect(p: &Paragraph, off: usize) -> Result<Option<usize>, CmdError> {
    let w = OBJ.len_utf8();
    // A question of a quiz: its text control, then the points.
    if let Some((a, b)) = span(p, TAG_ITEM) {
        return Ok(if off <= a {
            Some(a + w)
        } else if off > b {
            Some(b)
        } else {
            None
        });
    }
    // A title: after the exercise's start, its own control to the end of the paragraph.
    if let Some((a, b)) = span(p, TAG_TITLE) {
        return Ok(if off <= a {
            Some(a + w)
        } else if off > b {
            Some(b)
        } else {
            None
        });
    }
    // A problem's question: its text, then the points (generated) at the end.
    if let Some((a, _)) = span(p, TAG_POINTS) {
        return Ok((off > a).then_some(a));
    }
    // A calculation's result line: generated whole.
    if span(p, TAG_LINE).is_some() {
        return Err(CmdError::Disabled(LINE_REFUSED.into()));
    }
    // An option: the box first, then its text.
    if let Some((a, b)) = span(p, TAG_BOX) {
        return Ok((off <= a).then_some(b + w));
    }
    Ok(None)
}

/// Before typing at a collapsed caret: a caret beside an exercise field moves into it (see the
/// module docs), so nothing is typed outside the fields.
pub(crate) fn typing_target(s: &mut Session) -> Result<(), CmdError> {
    if !s.sel.is_collapsed() || s.sel.focus.story != StoryRef::Body || s.sel.focus.path.0.len() != 1 {
        return Ok(());
    }
    let f = s.sel.focus.clone();
    let Some(p) = s.doc.para_at(&f) else { return Ok(()) };
    // Only paragraphs of an exercise have these fields.
    if !s.doc.control_ranges(StoryRef::Body).iter().any(|r| r.control.tag == TAG_EXERCISE && r.contains(&f)) {
        return Ok(());
    }
    if let Some(off) = redirect(p, f.off)?
        && off <= p.text.len()
        && p.text.is_char_boundary(off)
    {
        s.sel = Selection::caret(Pos { off, ..f });
    }
    Ok(())
}

/// Whether `blocks` hold the start of an exercise.
fn holds_exercise(blocks: &[Block]) -> bool {
    blocks.iter().any(|b| match b {
        Block::Para(p) => p.objects.iter().any(|o| matches!(o, InlineObject::ControlStart { control } if control.tag == TAG_EXERCISE)),
        Block::Table(_) => false,
    })
}

/// Before `blocks` go in at the selection: when they hold an exercise and the selection is in an
/// exercise (or in the results summary), the caret moves to a new empty paragraph right after
/// that exercise (before the summary), so the exercises never nest. Whether it moved.
pub(crate) fn outside_for(s: &mut Session, blocks: &[Block]) -> bool {
    if !holds_exercise(blocks) || s.sel.focus.story != StoryRef::Body {
        return false;
    }
    let Some(top) = s.sel.focus.path.0.first().map(|&t| t as usize) else { return false };
    let layout = scan(s);
    let at = match layout.parts.get(top) {
        Some((Some(e), _)) => {
            let e = *e;
            layout.parts.iter().rposition(|(x, _)| *x == Some(e)).map_or(top + 1, |last| last + 1)
        }
        Some((None, Part::Results)) => layout.parts.iter().position(|(_, part)| *part == Part::Results).unwrap_or(top),
        _ => return false,
    };
    let at = at.min(s.doc.body.len());
    s.doc.body.insert(at, para_block(Paragraph::new()));
    s.sel = Selection::caret(Pos::new(StoryRef::Body, super::body_path(at), 0));
    s.touch();
    true
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use wordcraft_doc::Pos;

    use super::*;
    use crate::Session;

    fn run(s: &mut Session, id: &str, v: Value) -> Value {
        s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
    }

    fn para_text(s: &Session, i: usize) -> String {
        s.doc.body.get(i).and_then(|b| b.as_para()).map(super::super::plain).unwrap_or_default()
    }

    /// The question's paragraph index of a lone open quiz just inserted in a blank document.
    fn quiz() -> Session {
        let mut s = Session::new(wordcraft_doc::Document::new());
        run(&mut s, "exercise.insert", json!({"kind": "open", "title": "Uno", "questions": [{"text": "Domanda", "points": 2}]}));
        s
    }

    fn question_para(s: &Session) -> usize {
        (0..s.doc.body.len()).find(|&i| s.doc.body[i].as_para().is_some_and(|p| span(p, TAG_ITEM).is_some())).unwrap_or(0)
    }

    #[test]
    fn typing_after_the_points_goes_into_the_question() {
        let mut s = quiz();
        let q = question_para(&s);
        let len = s.doc.body[q].as_para().map_or(0, |p| p.text.len());
        // The very end of the paragraph: after the points control (and the controls closing).
        s.sel = Selection::caret(Pos::body(q, len));
        s.clamp_selection();
        run(&mut s, "text.insert", json!({"text": "?"}));
        let p = s.doc.body[q].as_para().cloned().unwrap_or_default();
        let (a, b) = span(&p, TAG_ITEM).unwrap_or((0, 0));
        let item = p.text.get(a + OBJ.len_utf8()..b).unwrap_or_default().to_string();
        assert_eq!(item, "Domanda?");
        assert!(para_text(&s, q).contains("/ 2"), "{}", para_text(&s, q));
    }

    #[test]
    fn typing_before_the_question_goes_into_it() {
        let mut s = quiz();
        let q = question_para(&s);
        s.sel = Selection::caret(Pos::body(q, 0));
        run(&mut s, "text.insert", json!({"text": "X"}));
        let p = s.doc.body[q].as_para().cloned().unwrap_or_default();
        let (a, b) = span(&p, TAG_ITEM).unwrap_or((0, 0));
        assert_eq!(p.text.get(a + OBJ.len_utf8()..b), Some("XDomanda"));
    }

    #[test]
    fn typing_before_the_title_goes_into_it() {
        let mut s = quiz();
        s.sel = Selection::caret(Pos::body(0, 0));
        run(&mut s, "text.insert", json!({"text": "Z"}));
        let p = s.doc.body[0].as_para().cloned().unwrap_or_default();
        let (a, b) = span(&p, TAG_TITLE).unwrap_or((0, 0));
        assert_eq!(p.text.get(a + OBJ.len_utf8()..b), Some("ZUno"));
    }

    #[test]
    fn a_result_line_takes_no_typing() {
        let mut s = Session::new(wordcraft_doc::Document::new());
        run(&mut s, "exercise.insert", json!({"kind": "calc"}));
        let line = (0..s.doc.body.len()).find(|&i| s.doc.body[i].as_para().is_some_and(|p| span(p, TAG_LINE).is_some()));
        let Some(line) = line else { panic!("no result line") };
        let len = s.doc.body[line].as_para().map_or(0, |p| p.text.len());
        s.sel = Selection::caret(Pos::body(line, len));
        let before = para_text(&s, line);
        assert!(s.run("text.insert", &json!({"text": "x"})).is_err());
        assert_eq!(para_text(&s, line), before);
    }

    #[test]
    fn pasting_an_exercise_inside_another_goes_after_it() {
        let mut s = Session::new(wordcraft_doc::Document::new());
        run(&mut s, "exercise.insert", json!({"kind": "open", "title": "Uno"}));
        run(&mut s, "exercise.insert", json!({"kind": "open", "title": "Due"}));
        // Copy the second exercise whole, then paste with the caret in the first one's question.
        let layout = scan(&s);
        let (a, b) = (layout.exercises[1].start, layout.parts.iter().rposition(|(x, _)| *x == Some(1)).unwrap_or(0));
        let end = s.doc.body[b].as_para().map_or(0, |p| p.text.len());
        s.sel = Selection { anchor: Pos::body(a, 0), focus: Pos::body(b, end) };
        run(&mut s, "edit.copy", json!({}));
        let q = question_para(&s);
        s.sel = Selection::caret(Pos::body(q, OBJ.len_utf8() * 2));
        s.clamp_selection();
        run(&mut s, "edit.paste", json!({}));
        let layout = scan(&s);
        assert_eq!(layout.exercises.len(), 3);
        let titles: Vec<String> = layout.exercises.iter().map(|e| e.title.clone()).collect();
        assert_eq!(titles, ["Uno", "Due", "Due"]);
        // The first exercise is whole: its blocks come before the second's.
        let first_end = layout.parts.iter().rposition(|(x, _)| *x == Some(0)).unwrap_or(0);
        assert!(first_end < layout.exercises[1].start);
    }
}
