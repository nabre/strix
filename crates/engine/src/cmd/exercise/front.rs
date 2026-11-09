//! The frontispiece of a series of exercises (docs/exercise.md, Frontispiece): free content at
//! the very start of the document (title of the series, class, date, instructions…) in a control
//! that can't be deleted by accident; the first thing after it starts a new page. It can run to
//! several pages: a page break inside it starts the next one.

use std::sync::Arc;

use serde_json::{Value, json};
use wordcraft_doc::Block;
use wordcraft_doc::control::ControlLock;

use super::{STYLE_FRONT, TAG_FRONT, control, ensure_styles, para, para_block, scan, update, words};
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

/// `exercise.frontispiece`: add (`"on": true`) or remove (`false`) the frontispiece; without
/// params, whether there is one.
pub(crate) fn command(s: &mut Session, v: &Value) -> CmdResult {
    let layout = scan(s);
    if layout.exercises.is_empty() && layout.front.is_none() {
        return Err(CmdError::Disabled("the document has no exercise".into()));
    }
    match (p::bool(v, "on"), layout.front) {
        (Some(true), None) => {
            ensure_styles(s);
            let w = words(s);
            let mut ctl = control(TAG_FRONT, w.front_part, ControlLock::SdtLocked);
            ctl.placeholder_text = w.front.into();
            ctl.showing_placeholder = true;
            s.doc.body.insert(0, para_block(para(STYLE_FRONT, w.front, vec![ctl], 1, None)));
            set_break(s, 1, true);
            s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(wordcraft_doc::StoryRef::Body, super::body_path(0), 0));
            s.touch();
            update(s);
            s.clamp_selection();
        }
        (Some(false), Some((from, to))) => {
            if s.doc.body.get(from..=to).is_some() {
                s.doc.body.drain(from..=to);
                // What followed it no longer needs a new page (`update` sets an exercise's again).
                set_break(s, from, false);
                s.doc.ensure_nonempty();
                s.touch();
                update(s);
                s.clamp_selection();
            }
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

    use super::super::{STYLE_FRONT, STYLE_TITLE, plain, scan};
    use crate::Session;

    fn run(s: &mut Session, id: &str, v: serde_json::Value) -> serde_json::Value {
        s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
    }

    fn breaks(s: &Session) -> Vec<(String, bool)> {
        s.doc.body.iter().filter_map(|b| b.as_para()).map(|p| (plain(p), p.props.page_break_before == Some(true))).collect()
    }

    /// The owner: a frontispiece for a series, possibly of several pages (page breaks inside it).
    #[test]
    fn a_frontispiece_starts_the_document() {
        let mut s = Session::new(Document::new());
        run(&mut s, "document.setText", json!({"text": ""}));
        s.doc.styles.default_chr.lang = Some("it-IT".into());
        run(&mut s, "exercise.insert", json!({"title": "Uno", "questions": ["q"]}));
        run(&mut s, "caret.docEnd", json!({}));
        run(&mut s, "exercise.insert", json!({"title": "Due", "questions": ["q"]}));
        assert_eq!(run(&mut s, "exercise.frontispiece", json!({})), json!({"on": false}));
        assert_eq!(run(&mut s, "exercise.frontispiece", json!({"on": true})), json!({"on": true}));
        let b = breaks(&s);
        assert_eq!(b[0].0, "Frontespizio: titolo della serie, classe, data, istruzioni.");
        assert_eq!(s.doc.body[0].as_para().and_then(|p| p.props.style.clone()).as_deref(), Some(STYLE_FRONT));
        // The first exercise starts a new page.
        assert_eq!(b[1], ("Uno".to_string(), true), "{b:?}");
        // Typed over (the caret is in it); a page break inside gives it a second page.
        run(&mut s, "text.insert", json!({"text": "Verifica di statica"}));
        run(&mut s, "text.pageBreak", json!({}));
        run(&mut s, "text.insert", json!({"text": "Istruzioni"}));
        let layout = scan(&s);
        let (from, to) = layout.front.unwrap();
        let front: String = (from..=to).filter_map(|i| s.doc.body[i].as_para()).map(plain).collect();
        assert!(front.contains("Verifica di statica") && front.contains("Istruzioni"), "{front:?}");
        assert_eq!(layout.exercises.len(), 2);
        assert!(
            s.doc.body[to + 1].as_para().is_some_and(|p| p.props.page_break_before == Some(true) && p.props.style.as_deref() == Some(STYLE_TITLE))
        );
        // Exercises renumber as usual; saved and opened again it is still the frontispiece.
        let bytes = crate::io::save_bytes("f.xmath", &s.doc).unwrap();
        let mut back = Session::new(crate::io::open_bytes("f.xmath", &bytes).unwrap());
        assert_eq!(run(&mut back, "exercise.frontispiece", json!({})), json!({"on": true}));
        // Removed: its content goes, the first exercise no longer breaks; one undo brings it back.
        run(&mut s, "exercise.frontispiece", json!({"on": false}));
        let b = breaks(&s);
        assert_eq!(b[0], ("Uno".to_string(), false), "{b:?}");
        assert!(s.undo());
        assert!(scan(&s).front.is_some());
    }
}
