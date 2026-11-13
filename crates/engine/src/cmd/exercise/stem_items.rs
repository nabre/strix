//! Laboratory question helpers, isolated from upstream editor code.
use crate::{CmdError, CmdResult, Session, p};
use serde_json::{Value, json};
use std::sync::Arc;
use wordcraft_doc::{Block, InlineObject};

pub const TAG_BLANK: &str = "strix:blank";
pub(super) const TAG_MATCH_ROW: &str = "strix:matchingRow";

pub(crate) fn blank(s: &mut Session, v: &Value) -> CmdResult {
    super::expr::insert_command(s, v)?;
    let focus = s.sel.focus.clone();
    let para = s.doc.para_mut(focus.story, &focus.path)?;
    let spans = super::expr::expr_spans(para);
    let span = spans
        .iter()
        .copied()
        .find(|(a, b)| (*a..=b.saturating_add(wordcraft_doc::para::OBJ.len_utf8())).contains(&focus.off))
        .or_else(|| spans.into_iter().rev().find(|(_, end)| *end <= focus.off));
    if let Some((a, _)) = span {
        if let Some(InlineObject::ControlStart { control }) = para.object_at_mut(a) {
            control.tag = TAG_BLANK.into();
        }
        para.touch();
    }
    super::super::calc::recalculate_now(s);
    Ok(json!({"blank": true}))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn update_matching(
    s: &mut Session,
    (a, b): (usize, usize),
    pairs: &[(String, String)],
    ex: usize,
    question: usize,
    variant: usize,
    name: &str,
    teacher: bool,
) -> bool {
    let perm = super::items_ext::choice_permutation(ex, question, name, variant.saturating_add(1), "A", pairs.len(), true);
    let mut lines = Vec::new();
    for (row, pair) in pairs.iter().enumerate() {
        let answer = perm.iter().position(|i| *i == row).and_then(|i| char::from_u32(u32::from(b'A').saturating_add(i as u32))).unwrap_or('?');
        let right = perm.get(row).and_then(|i| pairs.get(*i)).map(|p| p.1.as_str()).unwrap_or("");
        let letter = char::from_u32(u32::from(b'A').saturating_add(row as u32)).unwrap_or('?');
        lines.push(if teacher { format!("{}  [{answer}]\t{letter}. {right}", pair.0) } else { format!("{}  [   ]\t{letter}. {right}", pair.0) });
    }
    let shown = lines.join("\n");
    let width =
        s.doc.sections().into_iter().find(|(end, _)| *end >= a).map_or_else(|| s.doc.last_section.text_width(), |(_, section)| section.text_width());
    let tabs = vec![wordcraft_doc::props::TabStop {
        pos: (width * 0.5).max(36.0),
        align: wordcraft_doc::props::TabAlign::Left,
        leader: wordcraft_doc::props::TabLeader::None,
    }];
    let mut changed = false;
    for top in a..=b {
        let Some(block) = s.doc.body.get_mut(top) else { continue };
        let Block::Para(para) = Arc::make_mut(block) else { continue };
        if let Some(span) = super::items::span(para, TAG_MATCH_ROW) {
            if para.props.tabs.as_ref() != Some(&tabs) {
                para.props.tabs = Some(tabs.clone());
                para.touch();
                changed = true;
            }
            changed |= super::items::set_content(para, span, &shown);
        }
    }
    changed
}

pub(crate) fn table_fill(s: &mut Session, v: &Value) -> CmdResult {
    let (path, row, col) = s.sel.focus.path.cell().ok_or_else(|| CmdError::Disabled("put the caret in a calculated table".into()))?;
    let table = s.doc.table_mut(s.sel.focus.story, &path)?;
    if !table.props.calc {
        return Err(CmdError::Disabled("the table must be calculated".into()));
    }
    let current = table.rows.get(row).and_then(|r| r.cells.get(col)).is_some_and(|c| c.props.student_blank);
    let want = p::bool(v, "studentBlank").unwrap_or(!current);
    let column = p::bool(v, "column").unwrap_or(false);
    for (r, line) in table.rows.iter_mut().enumerate() {
        if r != row && !column {
            continue;
        }
        if column && r == 0 {
            continue;
        }
        if let Some(cell) = line.cells.get_mut(col) {
            cell.props.student_blank = want;
            if !want {
                for b in &mut cell.blocks {
                    super::show::set_hidden(b, false);
                }
            }
        }
    }
    s.touch();
    table_blanks(s);
    Ok(json!({"studentBlank": want, "column": column}))
}

/// The calculation still reads the original text; only its visibility changes.
pub(crate) fn table_blanks(s: &mut Session) {
    let layout = super::scan(s);
    for (top, block) in s.doc.body.iter_mut().enumerate() {
        let teacher = layout.parts.get(top).and_then(|(ex, _)| *ex).and_then(|ex| layout.exercises.get(ex)).is_none_or(|ex| ex.show.solution);
        let Block::Table(t) = &**block else { continue };
        if !t.rows.iter().flat_map(|r| &r.cells).any(|c| c.props.student_blank) {
            continue;
        }
        let Block::Table(t) = Arc::make_mut(block) else { continue };
        for cell in t.rows.iter_mut().flat_map(|r| &mut r.cells).filter(|c| c.props.student_blank) {
            for b in &mut cell.blocks {
                super::show::set_hidden(b, !teacher);
            }
        }
    }
}

/// Exercise indexes included in the scores table. Worked examples have no score row.
pub(super) fn graded(s: &Session) -> Vec<usize> {
    let layout = super::scan(s);
    (0..layout.exercises.len())
        .filter(|&e| {
            let items = super::items::items(s, &layout, e);
            items.is_empty() || items.iter().any(|i| i.settings.mode != super::items::Mode::Worked)
        })
        .collect()
}

const TAG_GUIDE: &str = "strix:dsaGuide";

pub(super) fn problem_guides(s: &mut Session, answers: &std::collections::HashMap<(usize, u32), Vec<String>>) -> bool {
    let layout = super::scan(s);
    let math = super::dsa::math_enabled(s);
    let opts = super::super::calc::options_now(s);
    let mut changed = false;
    for (e, ex) in layout.exercises.iter().enumerate().rev() {
        let Some((heading, _)) = ex.solution_blocks else { continue };
        let mut desired = Vec::new();
        if math && !ex.show.solution && ex.kind == super::items::Kind::Problem {
            for (q, _) in ex.questions.iter().enumerate() {
                let key = (e, u32::try_from(q.saturating_add(1)).unwrap_or(u32::MAX));
                if let Some(found) = answers.get(&key).and_then(|values| values.last()) {
                    desired.push(super::items::line_text(Some(found), false, None, &opts));
                }
            }
        }
        let mut start = heading;
        while start > ex.start && s.doc.body.get(start - 1).and_then(|b| b.as_para()).is_some_and(|p| super::items::span(p, TAG_GUIDE).is_some()) {
            start -= 1;
        }
        let current: Vec<_> = s.doc.body.get(start..heading).unwrap_or_default().iter().filter_map(|b| b.as_para()).map(super::plain).collect();
        if current == desired {
            continue;
        }
        s.doc.body.drain(start..heading);
        for (offset, text) in desired.into_iter().enumerate() {
            let ctl = super::control(TAG_GUIDE, "Result", wordcraft_doc::control::ControlLock::SdtContentLocked);
            let p = super::para(super::items::STYLE_LINE, &text, vec![ctl], 1, None);
            s.doc.body.insert(start.saturating_add(offset), wordcraft_doc::para_block(p));
        }
        changed = true;
    }
    changed
}
