//! Keeping an exercise's structure while editing (docs/exercise.md, Structure): Backspace and
//! Delete never join a paragraph across the edge of a part that can't be deleted (the statement
//! into the title, a question into the statement) nor take letters out of a placeholder, and a
//! part emptied of everything, also of its
//! extra empty paragraphs, shows its placeholder again.

use wordcraft_doc::control::ControlRange;
use wordcraft_doc::para::OBJ;
use wordcraft_doc::{Block, InlineObject, Paragraph, Pos, StoryRef};

use super::{TAG_ITEM, TAG_QUESTIONS, TAG_TEXT, TAG_TITLE};
use crate::{CmdError, Selection, Session};

/// Bytes of the control start markers a paragraph begins with.
fn leading_starts(p: &Paragraph) -> usize {
    let mut end = 0;
    for (at, o) in p.objects_with_offsets() {
        if at != end || !matches!(o, InlineObject::ControlStart { .. }) {
            break;
        }
        end = at + OBJ.len_utf8();
    }
    end
}

/// Where the control end markers a paragraph ends with begin.
fn trailing_ends(p: &Paragraph) -> usize {
    let mut start = p.text.len();
    let marks: Vec<(usize, bool)> = p.objects_with_offsets().map(|(at, o)| (at, matches!(o, InlineObject::ControlEnd))).collect();
    for (at, end) in marks.into_iter().rev() {
        if !end || at + OBJ.len_utf8() != start {
            break;
        }
        start = at;
    }
    start
}

/// Nothing but control markers: no text, no picture, no other object.
fn bare(p: &Paragraph) -> bool {
    p.text.chars().all(|c| c == OBJ) && p.objects.iter().all(|o| matches!(o, InlineObject::ControlStart { .. } | InlineObject::ControlEnd))
}

/// A block control that can't be deleted (an exercise part) starts in `path`'s leading markers
/// (`start`) or ends in its trailing ones (`!start`).
fn locked_edge(ranges: &[ControlRange], s: &Session, story: StoryRef, path: &wordcraft_doc::Path, start: bool) -> bool {
    let Some(p) = s.doc.para(story, path) else { return false };
    let (lead, trail) = (leading_starts(p), trailing_ends(p));
    ranges.iter().any(|r| {
        r.control.block
            && !r.control.lock.can_delete()
            && if start { r.start.path == *path && r.start.off < lead } else { r.end.path == *path && r.end.off >= trail }
    })
}

/// Whether Backspace (`back`) or Delete at a caret `f` would join two paragraphs across the edge
/// of a part that can't be deleted. Joining an empty paragraph outside any control is harmless
/// (the text stays as it was) and goes ahead.
pub(crate) fn keeps_edge(s: &Session, f: &Pos, back: bool) -> bool {
    let Some(p) = s.doc.para_at(f) else { return false };
    // A placeholder showing is no text: Backspace or Delete in it takes nothing away.
    if s.doc
        .control_at(f)
        .is_some_and(|r| r.control.showing_placeholder && [TAG_TEXT, TAG_QUESTIONS, TAG_TITLE, TAG_ITEM].contains(&r.control.tag.as_str()))
    {
        return true;
    }
    let ranges = s.doc.control_ranges(f.story);
    if ranges.is_empty() {
        return false;
    }
    let i = f.path.last();
    let here_empty = bare(p) && s.doc.control_at(f).is_none();
    if back {
        if f.off > leading_starts(p) || i == 0 {
            return false;
        }
        let prev = f.path.with_last(i - 1);
        if !matches!(s.doc.block(f.story, &prev), Some(Block::Para(_))) {
            return false;
        }
        locked_edge(&ranges, s, f.story, &f.path, true) || (!here_empty && locked_edge(&ranges, s, f.story, &prev, false))
    } else {
        if f.off < trailing_ends(p) {
            return false;
        }
        let next = f.path.with_last(i + 1);
        if !matches!(s.doc.block(f.story, &next), Some(Block::Para(_))) {
            return false;
        }
        locked_edge(&ranges, s, f.story, &f.path, false) || (!here_empty && locked_edge(&ranges, s, f.story, &next, true))
    }
}

/// After a deletion: the statement, questions or title at the caret left with only empty
/// paragraphs becomes one empty paragraph, so the placeholder comes back (a single empty one
/// already does).
pub(crate) fn collapse_empty_part(s: &mut Session) -> Result<(), CmdError> {
    let at = s.sel.focus.clone();
    let Some(r) = s.doc.control_at(&at) else { return Ok(()) };
    if r.control.showing_placeholder || ![TAG_TEXT, TAG_QUESTIONS, TAG_TITLE].contains(&r.control.tag.as_str()) || r.start.path == r.end.path {
        return Ok(());
    }
    let paths = s.doc.paths_between(&r.start, &r.end);
    if !paths.iter().all(|p| s.doc.para(r.start.story, p).is_some_and(bare)) {
        return Ok(());
    }
    // Only this control's markers in between: a nested control keeps its paragraphs.
    let inner = s.doc.control_ranges(r.start.story).into_iter().any(|o| o.start != r.start && r.contains(&o.start));
    if inner {
        return Ok(());
    }
    let start = r.content_start();
    s.doc.delete_range(&start, &r.end)?;
    s.sel = Selection::caret(start);
    Ok(())
}
