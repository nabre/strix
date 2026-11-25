//! `exercise.goWorking`: the caret into the working space of a question, the blocks under its
//! repeated question in the solution, for scripts and agents (the repeated question is read-only
//! and its text appears twice, so finding it by text is unreliable).

use std::sync::Arc;

use serde_json::{Value, json};
use wordcraft_doc::{Block, Path, Pos, StoryRef};

use super::{para, scan, target};
use crate::{CmdError, CmdResult, Session, p};

/// Puts the caret at the end of the working space of question `question` (1-based) of the
/// exercise named by `exercise` (or holding the caret). With no block between this repeated
/// question and the next one (or the end of the solution), an empty paragraph is made there.
pub(super) fn command(s: &mut Session, v: &Value) -> CmdResult {
    let ex = target(s, v)?;
    let n = match p::u64(v, "question") {
        Some(n) if n >= 1 => usize::try_from(n - 1).unwrap_or(usize::MAX),
        _ => return Err(CmdError::Params("question: 1-based number of the question".into())),
    };
    let layout = scan(s);
    let e = layout.exercises.get(ex).ok_or_else(|| CmdError::Params(format!("no exercise {}", ex.saturating_add(1))))?;
    if e.kind.quiz() {
        return Err(CmdError::Disabled("a quiz has no solution to work in".into()));
    }
    let Some(&(_, prompt_end)) = e.prompts.get(n) else {
        return Err(CmdError::Params(format!("the exercise has {} questions with a working space", e.prompts.len())));
    };
    let solution_end = e.solution_blocks.map_or(prompt_end, |(_, end)| end);
    // Up to the next repeated question, or the end of the solution.
    let next = e.prompts.get(n + 1).map_or(solution_end.saturating_add(1), |&(start, _)| start);
    let mut last = next.saturating_sub(1);
    if last <= prompt_end || !matches!(s.doc.body.get(last).map(|b| &**b), Some(Block::Para(_))) {
        // No working space yet (or a table ends it): an empty paragraph right after the prompt.
        let at = prompt_end.saturating_add(1).min(s.doc.body.len());
        s.doc.body.insert(at, Arc::new(Block::Para(para("Normal", "", Vec::new(), 0, None))));
        s.touch();
        last = at;
    }
    let end = s
        .doc
        .body
        .get(last)
        .and_then(|b| b.as_para())
        .map_or(0, |p| p.text.len().saturating_sub(super::items::trailing_block_ends(p) * wordcraft_doc::para::OBJ.len_utf8()));
    s.sel = crate::Selection::caret(Pos::new(StoryRef::Body, Path::top(last), end));
    s.clamp_selection();
    Ok(json!({"exercise": ex + 1, "question": n + 1, "block": last}))
}
