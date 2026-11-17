//! Columns for a selection that starts or ends in a table (Layout › Columns with a table
//! selected): as in Word, the whole table goes into the new section, so a section of columns can
//! start (or end) with a table, with no empty paragraph before it.

use serde_json::json;
use wordcraft_doc::section::SectionStart;
use wordcraft_doc::{Block, Paragraph, Path, Pos, StoryRef};

use super::breaks;
use crate::{CmdError, Selection, Session};

/// One end of the selection: a place in a top-level paragraph, or a top-level table (its block).
#[derive(Clone, Copy, PartialEq)]
enum End {
    Para,
    Table(usize),
}

fn block(p: &Pos) -> usize {
    p.path.0.first().copied().unwrap_or(0) as usize
}

fn end_of(s: &Session, p: &Pos) -> Option<End> {
    if p.story != StoryRef::Body {
        return None;
    }
    if p.path.depth() == 0 {
        return Some(End::Para);
    }
    let i = block(p);
    matches!(s.doc.body.get(i).map(|b| &**b), Some(Block::Table(_))).then_some(End::Table(i))
}

/// Whether the selection is in the main text, with an end in a top-level table (and the other in
/// a top-level paragraph or table).
pub(super) fn touches_table(s: &Session) -> bool {
    if s.sel.is_collapsed() {
        return false;
    }
    let (a, b) = s.sel.ordered();
    match (end_of(s, &a), end_of(s, &b)) {
        (Some(x), Some(y)) => matches!(x, End::Table(_)) || matches!(y, End::Table(_)),
        _ => false,
    }
}

/// End a section at top-level paragraph `i`: it takes the props of the section it was in, and
/// the section after it keeps them, starting as `start` (as a section break typed there does).
fn end_section_at(s: &mut Session, i: usize, start: SectionStart) -> Result<(), CmdError> {
    let cur = s.doc.section_mut(i).clone();
    s.doc.para_mut(StoryRef::Body, &Path::top(i))?.section = Some(Box::new(cur.clone()));
    let mut next = cur;
    next.start = start;
    *s.doc.section_mut(i.saturating_add(1)) = next;
    Ok(())
}

fn ends_section(s: &Session, i: usize) -> bool {
    s.doc.body.get(i).and_then(|b| b.as_para()).is_some_and(|p| p.section.is_some())
}

/// Continuous section breaks around a selection with an end in a table (see
/// [`touches_table`]): a selected table goes whole into the section between them. Before a table,
/// the paragraph above it ends the section above (the break shows at the end of its text, as in
/// Word), so the new section starts with the table; only a table right under another table gets an
/// empty paragraph to hold the break. After a table, an empty paragraph under it holds the break;
/// one with text is split at its start. No break where the selection already meets a section
/// boundary. The selection keeps its text.
pub(super) fn table_selection_to_sections(s: &mut Session) -> Result<(), CmdError> {
    let old = s.sel.clone();
    let (a, b) = old.ordered();
    let (Some(start), Some(end)) = (end_of(s, &a), end_of(s, &b)) else {
        return Err(CmdError::Params("selected text in the main text is needed".into()));
    };
    let last_block = s.doc.body.len().saturating_sub(1);
    let after_break = |s: &Session, i: usize| i.checked_sub(1).is_some_and(|j| ends_section(s, j));
    // The end first: the start's position doesn't move.
    match end {
        End::Table(u) => {
            let next = u.saturating_add(1);
            let para = s.doc.body.get(next).and_then(|b| b.as_para()).map(|p| (p.text.is_empty(), p.section.is_some()));
            match para {
                // The document's last paragraph, or one already ending a section: the section ends there.
                Some((true, false)) if next < last_block => end_section_at(s, next, SectionStart::Continuous)?,
                Some((false, false)) => {
                    s.sel = Selection::caret(Pos::body(next, 0));
                    breaks(s, &json!({"kind": "continuous"}))?;
                }
                _ => {}
            }
        }
        End::Para => {
            let len = s.doc.para_at(&b).map(|q| q.len()).unwrap_or(0);
            if !(b.off == len && (ends_section(s, block(&b)) || block(&b) == last_block)) {
                s.sel = Selection::caret(b.clone());
                breaks(s, &json!({"kind": "continuous"}))?;
            }
        }
    }
    let before = s.doc.body.len();
    let mut first = a.clone();
    match start {
        End::Table(t) if t == 0 || after_break(s, t) => {}
        End::Table(t) => {
            let above_is_para = s.doc.body.get(t.saturating_sub(1)).is_some_and(|b| b.as_para().is_some());
            if above_is_para {
                end_section_at(s, t.saturating_sub(1), SectionStart::Continuous)?;
            } else {
                s.doc.insert_block(StoryRef::Body, &Path::top(t), Block::Para(Paragraph::new()))?;
                end_section_at(s, t, SectionStart::Continuous)?;
            }
        }
        End::Para if a.off == 0 && (block(&a) == 0 || after_break(s, block(&a))) => {}
        End::Para => {
            s.sel = Selection::caret(a.clone());
            breaks(s, &json!({"kind": "continuous"}))?;
            first = s.sel.focus.clone();
        }
    }
    // Blocks added before the selection's end by the start's break move it down.
    let added = s.doc.body.len().saturating_sub(before) as u32;
    let shift = |p: &Pos| {
        let mut p = p.clone();
        if let Some(top) = p.path.0.first_mut() {
            *top = top.saturating_add(added);
        }
        p
    };
    if matches!(start, End::Table(_)) {
        first = shift(&a);
    }
    let last = shift(&b);
    s.sel = if old.anchor <= old.focus { Selection { anchor: first, focus: last } } else { Selection { anchor: last, focus: first } };
    Ok(())
}
