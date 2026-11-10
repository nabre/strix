//! The frontispiece of a series of exercises (docs/exercise.md, Frontispiece): your own pages
//! at the very start of the document (title of the series, class, date, instructions…), as long
//! as you like, ended by a **Frontispiece Break**: a next-page section break marked as the
//! frontispiece's (`SectionProps::frontispiece`), shown with the formatting marks like any section
//! break. Everything before it is the frontispiece, free text with no frame or placeholder.

use std::sync::Arc;

use serde_json::{Value, json};
use wordcraft_doc::section::SectionStart;
use wordcraft_doc::{Block, Paragraph, SectionProps, StoryRef};

use super::{TAG_FRONT, para_block, scan, update};
use crate::{CmdError, CmdResult, Session, p};

/// Page break before on top-level block `i` (when it is a paragraph); whether it changed.
pub(crate) fn set_break(s: &mut Session, i: usize, on: bool) -> bool {
    let want = on.then_some(true);
    let Some(block) = s.doc.body.get_mut(i) else { return false };
    if !matches!(&**block, Block::Para(p) if p.props.page_break_before != want) {
        return false;
    }
    let Block::Para(p) = Arc::make_mut(block) else { return false };
    p.props.page_break_before = want;
    p.touch();
    true
}

/// The properties of the section top-level block `i` is in (its page, margins, headers), as
/// the frontispiece's: a next-page section marked as the frontispiece.
fn front_section(s: &Session, i: usize) -> SectionProps {
    let props = s.doc.sections().into_iter().find(|(end, _)| *end >= i).map_or_else(|| s.doc.last_section.clone(), |(_, sp)| sp.clone());
    SectionProps { frontispiece: true, start: SectionStart::NextPage, fmt_change: None, ..props }
}

/// Top-level paragraph `i` ends in the Frontispiece Break; whether it changed.
fn put_break(s: &mut Session, i: usize) -> bool {
    let sp = front_section(s, i);
    let Some(block) = s.doc.body.get_mut(i) else { return false };
    let Block::Para(p) = Arc::make_mut(block) else { return false };
    match p.section.as_mut() {
        Some(own) if own.frontispiece => return false,
        // Already a section break: it becomes the frontispiece's.
        Some(own) => own.frontispiece = true,
        None => p.section = Some(Box::new(sp)),
    }
    p.touch();
    true
}

/// A frontispiece from before the break (a control round its content, with a placeholder)
/// becomes the content alone, ended by a Frontispiece Break; a placeholder still shown goes.
/// Whether anything changed.
pub(crate) fn migrate(s: &mut Session) -> bool {
    let layout = scan(s);
    let Some((from, to, placeholder)) = layout.legacy_front else { return false };
    if layout.front.is_some() || from != 0 {
        return false;
    }
    let ranges: Vec<_> = s.doc.control_ranges(StoryRef::Body).into_iter().filter(|r| r.control.tag == TAG_FRONT).collect();
    let w = wordcraft_doc::para::OBJ.len_utf8();
    // Last first, so earlier offsets stay valid.
    for r in ranges.iter().rev() {
        if let Ok(p) = s.doc.para_mut(r.end.story, &r.end.path) {
            let _ = p.delete(r.end.off, r.end.off + w);
        }
        if let Ok(p) = s.doc.para_mut(r.start.story, &r.start.path) {
            let _ = p.delete(r.start.off, r.start.off + w);
        }
    }
    if placeholder {
        for i in from..=to {
            if let Some(block) = s.doc.body.get_mut(i)
                && let Block::Para(p) = Arc::make_mut(block)
            {
                let len = p.text.len();
                let _ = p.delete(0, len);
                p.props.style = None;
            }
        }
    }
    put_break(s, to);
    // The break starts the new page: the page break the control's frontispiece put after it goes.
    set_break(s, to + 1, false);
    s.touch();
    true
}

/// `exercise.frontispiece`: add (`"on": true`) or remove (`false`) the frontispiece; without
/// params, whether there is one. Added, it is an empty paragraph at the start of the document
/// ending in the Frontispiece Break, the caret in it.
pub(crate) fn command(s: &mut Session, v: &Value) -> CmdResult {
    // Asked only (the ribbon and the pane, every frame), nothing changes.
    if p::bool(v, "on").is_none() {
        let layout = scan(s);
        if layout.exercises.is_empty() && layout.front.is_none() && layout.legacy_front.is_none() {
            return Err(CmdError::Disabled("the document has no exercise".into()));
        }
        return Ok(json!({"on": layout.front.is_some() || layout.legacy_front.is_some()}));
    }
    migrate(s);
    let layout = scan(s);
    if layout.exercises.is_empty() && layout.front.is_none() {
        return Err(CmdError::Disabled("the document has no exercise".into()));
    }
    match (p::bool(v, "on"), layout.front) {
        (Some(true), None) => {
            let sp = front_section(s, 0);
            let mut para = Paragraph::new();
            para.section = Some(Box::new(sp));
            s.doc.body.insert(0, para_block(para));
            // The break starts the new page.
            set_break(s, 1, false);
            s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(StoryRef::Body, super::body_path(0), 0));
            s.touch();
            update(s);
            s.clamp_selection();
        }
        (Some(false), Some((from, to))) if s.doc.body.get(from..=to).is_some() => {
            s.doc.body.drain(from..=to);
            s.doc.ensure_nonempty();
            s.touch();
            update(s);
            s.clamp_selection();
        }
        _ => {}
    }
    let layout = scan(s);
    Ok(json!({"on": layout.front.is_some()}))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wordcraft_doc::Document;

    use super::super::{STYLE_TITLE, TAG_FRONT, plain, scan};
    use crate::Session;

    fn run(s: &mut Session, id: &str, v: serde_json::Value) -> serde_json::Value {
        s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
    }

    fn series() -> Session {
        let mut s = Session::new(Document::new());
        run(&mut s, "document.setText", json!({"text": ""}));
        std::sync::Arc::make_mut(&mut s.doc.styles).default_chr.lang = Some("it-IT".into());
        run(&mut s, "exercise.insert", json!({"title": "Uno", "questions": ["q"]}));
        run(&mut s, "caret.docEnd", json!({}));
        run(&mut s, "exercise.insert", json!({"title": "Due", "questions": ["q"]}));
        s
    }

    fn front_break(s: &Session) -> Option<usize> {
        s.doc.body.iter().position(|b| b.as_para().is_some_and(|p| p.section.as_ref().is_some_and(|sp| sp.frontispiece)))
    }

    /// The owner: "the frontispiece can be longer than one page: mark it with a break, where I
    /// write before it; no 'Frontispiece' text field".
    #[test]
    fn a_frontispiece_is_free_text_before_its_break() {
        let mut s = series();
        assert_eq!(run(&mut s, "exercise.frontispiece", json!({})), json!({"on": false}));
        assert_eq!(run(&mut s, "exercise.frontispiece", json!({"on": true})), json!({"on": true}));
        // An empty paragraph ending in the Frontispiece Break, no control, no placeholder.
        assert_eq!(front_break(&s), Some(0));
        let first = s.doc.body[0].as_para().unwrap();
        assert!(first.text.is_empty() && first.objects.is_empty(), "{:?}", first.text);
        // The first exercise doesn't add a page break of its own: the section break starts its page.
        assert!(s.doc.body[1].as_para().is_some_and(|p| p.props.style.as_deref() == Some(STYLE_TITLE) && p.props.page_break_before.is_none()));
        // Typed in (the caret is in it), with more paragraphs and pages: the break stays at its end.
        run(&mut s, "text.insert", json!({"text": "Verifica di statica"}));
        run(&mut s, "text.newParagraph", json!({}));
        run(&mut s, "text.pageBreak", json!({}));
        run(&mut s, "text.insert", json!({"text": "Istruzioni"}));
        let (from, to) = scan(&s).front.unwrap();
        assert_eq!((from, Some(to)), (0, front_break(&s)));
        let front: String = (from..=to).filter_map(|i| s.doc.body[i].as_para()).map(plain).collect();
        assert!(front.contains("Verifica di statica") && front.contains("Istruzioni"), "{front:?}");
        assert_eq!(scan(&s).exercises.len(), 2);
        // Saved and opened again it is still the frontispiece, in .xmath and .docx.
        for name in ["f.xmath", "f.docx"] {
            let bytes = crate::io::save_bytes(name, &s.doc).unwrap();
            let mut back = Session::new(crate::io::open_bytes(name, &bytes).unwrap());
            assert_eq!(run(&mut back, "exercise.frontispiece", json!({})), json!({"on": true}), "{name}");
        }
        // Removed: its content and break go; one undo brings it back.
        run(&mut s, "exercise.frontispiece", json!({"on": false}));
        assert_eq!(front_break(&s), None);
        assert!(s.doc.body[0].as_para().is_some_and(|p| plain(p) == "Uno"));
        assert!(s.undo());
        assert!(scan(&s).front.is_some());
    }

    /// A frontispiece from before (a control with its placeholder) becomes the break.
    #[test]
    fn an_old_frontispiece_control_becomes_a_break() {
        let mut s = series();
        let mut ctl = super::super::control(TAG_FRONT, "Frontespizio", wordcraft_doc::control::ControlLock::SdtLocked);
        ctl.placeholder_text = "Frontespizio: titolo".into();
        ctl.showing_placeholder = true;
        s.doc.body.insert(0, super::super::para_block(super::super::para("Normal", "Frontespizio: titolo", vec![ctl], 1, None)));
        super::set_break(&mut s, 1, true);
        assert!(super::super::update(&mut s));
        assert_eq!(front_break(&s), Some(0));
        let first = s.doc.body[0].as_para().unwrap();
        assert!(first.text.is_empty(), "the placeholder goes: {:?}", first.text);
        assert!(s.doc.body[1].as_para().is_some_and(|p| p.props.page_break_before.is_none()));
        assert_eq!(scan(&s).legacy_front, None);
    }
}
