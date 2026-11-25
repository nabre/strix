//! Hidden calculation lines (`calc.hide`): a statement that stays in the calculation but isn't
//! printed, for coefficients and service steps. The equation keeps the choice in its own options
//! (`hidden`, saved with it) and shows it as Word's hidden text: on the equation, and on its
//! paragraph mark when the paragraph holds nothing else, so the line takes no room while hidden
//! text isn't shown, shows with Show All (¶) and prints only when hidden text is printed. Every
//! recalculation puts the hidden text back on the equations that keep the choice (a saved file
//! may lose it on the equation itself).

use std::sync::Arc;

use serde_json::{Value, json};
use wordcraft_doc::para::OBJ;
use wordcraft_doc::{Block, InlineObject, Paragraph};

use crate::{CmdError, CmdResult, Session, p};

/// Hides (or shows again) the calculation at the caret, then recalculates.
pub(super) fn command(s: &mut Session, v: &Value) -> CmdResult {
    let at = super::super::equation::equation_at_caret(s).ok_or_else(|| CmdError::Disabled("no equation at the caret".into()))?;
    let para = s.doc.para(at.story, &at.path).ok_or_else(|| CmdError::Failed("no paragraph at the caret".into()))?;
    let Some(InlineObject::Equation { math, .. }) = para.object_at(at.off).filter(|o| matches!(o, InlineObject::Equation { math, .. } if math.calc))
    else {
        return Err(CmdError::Disabled("the equation at the caret isn't a calculation".into()));
    };
    let hidden_now = super::parse_eq_overrides(math.calc_opts.as_deref()).is_some_and(|c| c.hidden == Some(true));
    let on = p::bool(v, "value").unwrap_or(!hidden_now);
    super::apply_scoped_change(s, &json!({"scope": "equation"}), |c, _| c.hidden = on.then_some(true), |_, _| {})?;
    let para = s.doc.para_mut(at.story, &at.path).map_err(|e| CmdError::Failed(e.to_string()))?;
    set_hidden(para, at.off, on);
    s.touch();
    let report = super::recalculate_now(s);
    Ok(json!({"hidden": on, "recalculated": report}))
}

/// Why `calc.hide` can't run: no calculation at the caret.
pub(super) fn disabled(s: &Session) -> Option<&'static str> {
    let at = super::super::equation::equation_at_caret(s)?;
    let calc = s.doc.para(at.story, &at.path).is_some_and(|p| matches!(p.object_at(at.off), Some(InlineObject::Equation { math, .. }) if math.calc));
    (!calc).then_some("the equation at the caret isn't a calculation")
}

/// Hidden text on (or off) the equation at byte `off` of `p`; alone in its paragraph (beside its
/// object's markers), on every run and on the mark too, so the whole line goes.
fn set_hidden(p: &mut Paragraph, off: usize, on: bool) -> bool {
    let want = on.then_some(true);
    let alone = p.text.chars().all(|c| c == OBJ || c.is_whitespace());
    let (from, to) = if alone { (0, p.text.len()) } else { (off, off + OBJ.len_utf8()) };
    let runs_right = p.run_ranges().filter(|(r, _)| r.start < to && r.end > from).all(|(_, props)| props.hidden == want);
    if runs_right && (!alone || p.mark.hidden == want) {
        return false;
    }
    if p.format(from, to, &|c| c.hidden = want).is_err() {
        return false;
    }
    if alone {
        p.mark.hidden = want;
    }
    p.touch();
    true
}

/// Puts the hidden text back on the top-level calculations whose options keep `hidden` (never
/// takes any off: hidden text the user chose stays). Whether anything changed.
pub(super) fn apply(s: &mut Session) -> bool {
    let mut changed = false;
    for block in s.doc.body.iter_mut() {
        let Block::Para(p) = &**block else { continue };
        let offs: Vec<usize> = p
            .objects_with_offsets()
            .filter(|(_, o)| matches!(o, InlineObject::Equation { math, .. } if math.calc && super::parse_eq_overrides(math.calc_opts.as_deref()).is_some_and(|c| c.hidden == Some(true))))
            .map(|(at, _)| at)
            .collect();
        if offs.is_empty() {
            continue;
        }
        let Block::Para(p) = Arc::make_mut(block) else { continue };
        for at in offs {
            changed |= set_hidden(p, at, true);
        }
    }
    changed
}
