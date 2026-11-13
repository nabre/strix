//! Inline parametric expressions (`strix:expr`, `{{expr}}`): evaluated inside the enclosing
//! exercise's sandbox (or the main worksheet outside exercises) with units styled in `Unit`.

use std::sync::Arc;

use serde_json::{Value, json};
use wordcraft_doc::control::{ContentControl, ControlKind, ControlLock};
use wordcraft_doc::para::OBJ;
use wordcraft_doc::props::CharProps;
use wordcraft_doc::{Block, InlineObject, Paragraph, StoryRef};

use crate::{CmdError, CmdResult, Session, p};

pub const TAG_EXPR: &str = "strix:expr";
const EL: &str = "strix:expr";
const MAX_EXPR_CHARS: usize = 200;
const MAX_EXPRS_PER_PARA: usize = 64;

fn clean_expr(raw: &str) -> String {
    raw.chars().filter(|&c| !c.is_control() && c != OBJ).take(MAX_EXPR_CHARS).collect::<String>().trim().to_string()
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn unescape_xml(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// Reads the formula stored on a `strix:expr` content control.
pub(crate) fn read_formula(ctl: &ContentControl) -> Option<String> {
    let open = format!("<{EL}>");
    let close = format!("</{EL}>");
    let raw = ctl.extra.iter().find_map(|x| x.strip_prefix(&open)?.strip_suffix(&close))?;
    let f = clean_expr(&unescape_xml(raw));
    (!f.is_empty()).then_some(f)
}

/// Writes `formula` onto a `strix:expr` content control.
pub(crate) fn write_formula(ctl: &mut ContentControl, formula: &str) {
    let open = format!("<{EL}>");
    ctl.extra.retain(|x| !x.starts_with(&open));
    let clean = clean_expr(formula);
    if !clean.is_empty() {
        ctl.extra.push(format!("{open}{}</{EL}>", escape_xml(&clean)));
    }
}

pub(crate) fn make_control(formula: &str) -> ContentControl {
    let mut ctl = ContentControl::new(ControlKind::RichText);
    ctl.tag = TAG_EXPR.into();
    ctl.title = clean_expr(formula);
    ctl.block = false;
    ctl.lock = ControlLock::SdtContentLocked;
    write_formula(&mut ctl, formula);
    ctl
}

/// Evaluates `formula` (optionally ending in `-> unit` or `→ unit`) against `ws` without mutating it.
pub(crate) fn eval_in_sheet(ws: &strix_calc::Worksheet, formula: &str) -> Result<String, String> {
    let (expr_part, unit_part) = split_target_unit(formula);
    let val = ws.eval_expr(expr_part).map_err(|e| e.to_string())?;
    match unit_part {
        Some(u) => strix_calc::format_value_in(&val, u, ws.options()).map(|(_, full)| full).map_err(|e| e.to_string()),
        None => Ok(strix_calc::format_value(&val, ws.options())),
    }
}

fn split_target_unit(formula: &str) -> (&str, Option<&str>) {
    if let Some((lhs, rhs)) = formula.rsplit_once("->").or_else(|| formula.rsplit_once('→')) {
        let u = rhs.trim();
        if !lhs.trim().is_empty() && !u.is_empty() {
            return (lhs.trim(), Some(u));
        }
    }
    (formula.trim(), None)
}

/// Splits a formatted value (`"25 m/s"`, `"-1,5·10³ N"`) into `(number_part, unit_with_leading_space)`.
fn split_number_and_unit(shown: &str) -> (&str, &str) {
    if shown.starts_with('⚠') || shown.starts_with('[') {
        return (shown, "");
    }
    let numeric = |c: char| {
        c.is_ascii_digit()
            || c.is_whitespace()
            || matches!(c, '_' | '±' | '.' | ',' | '+' | '-' | '−' | '\u{2009}' | '\u{202F}' | '\u{00A0}' | '\'' | '·' | '×' | 'E' | 'e')
            || "⁰¹²³⁴⁵⁶⁷⁸⁹⁺⁻".contains(c)
    };
    let chars: Vec<(usize, char)> = shown.char_indices().collect();
    let mut cut = 0usize;
    for (k, &(i, c)) in chars.iter().enumerate() {
        let exp = matches!(c, 'e' | 'E' | '·' | '×');
        let next_num = chars.get(k + 1).is_some_and(|&(_, n)| n.is_ascii_digit() || matches!(n, '+' | '-' | '−'));
        if !numeric(c) || (exp && !next_num) {
            break;
        }
        cut = i + c.len_utf8();
    }
    if cut == 0 || cut >= shown.len() {
        return (shown, "");
    }
    let rest = shown.get(cut..).unwrap_or("");
    if rest.trim().is_empty() {
        return (shown, "");
    }
    (shown.get(..cut).unwrap_or(shown), shown.get(cut..).unwrap_or(""))
}

/// Converts any plain-text `{{formula}}` templates outside existing `strix:expr` controls in `p`
/// into inline `strix:expr` content controls. Returns whether `p` changed.
pub(crate) fn convert_templates_in_para(p: &mut Paragraph) -> bool {
    let mut changed = false;
    for _ in 0..MAX_EXPRS_PER_PARA {
        let Some((from, to, formula)) = find_next_template(p) else { break };
        let props = p.props_of_char(from).clone();
        if p.delete(from, to).is_err() {
            break;
        }
        let ctl = make_control(&formula);
        if p.insert_object(from, InlineObject::ControlStart { control: Box::new(ctl) }, &props).is_err() {
            break;
        }
        let inner = from + OBJ.len_utf8();
        let _ = p.insert_text(inner, &formula, &props);
        let _ = p.insert_object(inner + formula.len(), InlineObject::ControlEnd, &props);
        p.touch();
        changed = true;
    }
    changed
}

fn find_next_template(p: &Paragraph) -> Option<(usize, usize, String)> {
    // Byte ranges inside existing `strix:expr` controls are skipped.
    let excluded = expr_spans(p);
    let mut search_from = 0usize;
    while let Some(rel_open) = p.text.get(search_from..)?.find("{{") {
        let open = search_from + rel_open;
        let after_open = open + 2;
        if excluded.iter().any(|&(a, b)| (a..=b).contains(&open)) {
            search_from = after_open;
            continue;
        }
        let rel_close = p.text.get(after_open..)?.find("}}")?;
        let close = after_open + rel_close;
        let raw = p.text.get(after_open..close).unwrap_or_default();
        if raw.contains(OBJ) || raw.contains("{{") || raw.chars().count() > MAX_EXPR_CHARS {
            search_from = after_open;
            continue;
        }
        let formula = clean_expr(raw);
        if formula.is_empty() {
            search_from = close + 2;
            continue;
        }
        return Some((open, close + 2, formula));
    }
    None
}

/// Byte spans `(start_marker_byte, end_marker_byte)` of all `strix:expr` controls in `p`.
pub(crate) fn expr_spans(p: &Paragraph) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut active: Option<(usize, usize)> = None;
    for (at, o) in p.objects_with_offsets() {
        match o {
            InlineObject::ControlStart { control } => {
                if active.is_none() && matches!(control.tag.as_str(), TAG_EXPR | super::stem_items::TAG_BLANK) {
                    active = Some((at, 0));
                } else if let Some((_, d)) = &mut active {
                    *d += 1;
                }
            }
            InlineObject::ControlEnd => {
                if let Some((start, d)) = &mut active {
                    if *d == 0 {
                        out.push((*start, at));
                        active = None;
                    } else {
                        *d -= 1;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Converts `{{formula}}` templates across all body paragraphs (and table cells).
pub(crate) fn convert_all_templates(s: &mut Session) -> bool {
    let mut changed = false;
    for b in s.doc.body.iter_mut() {
        changed |= convert_block(b, 0);
    }
    if changed {
        s.touch();
        s.clamp_selection();
    }
    changed
}

fn convert_block(block: &mut Arc<Block>, depth: usize) -> bool {
    if depth > 16 {
        return false;
    }
    match &**block {
        Block::Para(p) if p.text.contains("{{") && p.text.contains("}}") => {
            let Block::Para(p) = Arc::make_mut(block) else { return false };
            convert_templates_in_para(p)
        }
        Block::Para(_) => false,
        Block::Table(t) => {
            let any = t.rows.iter().flat_map(|r| &r.cells).flat_map(|c| &c.blocks).any(|cb| has_template(cb, depth + 1));
            if !any {
                return false;
            }
            let Block::Table(t) = Arc::make_mut(block) else { return false };
            let mut ch = false;
            for r in &mut t.rows {
                for c in &mut r.cells {
                    for cb in &mut c.blocks {
                        ch |= convert_block(cb, depth + 1);
                    }
                }
            }
            ch
        }
    }
}

fn has_template(block: &Block, depth: usize) -> bool {
    if depth > 16 {
        return false;
    }
    match block {
        Block::Para(p) => p.text.contains("{{") && p.text.contains("}}"),
        Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).flat_map(|c| &c.blocks).any(|cb| has_template(cb, depth + 1)),
    }
}

/// Collects `(top_block, formula)` for every `strix:expr` control (or unexpanded `{{expr}}`) in `blocks`.
pub(crate) fn collect_exprs(blocks: &[Arc<Block>]) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (top, b) in blocks.iter().enumerate() {
        walk_collect(b, top, 0, &mut out);
    }
    out
}

fn walk_collect(b: &Block, top: usize, depth: usize, out: &mut Vec<(usize, String)>) {
    if depth > 16 || out.len() >= 2_000 {
        return;
    }
    match b {
        Block::Para(p) => {
            for (a, _) in expr_spans(p) {
                if let Some(InlineObject::ControlStart { control }) = p.object_at(a)
                    && let Some(f) = read_formula(control)
                {
                    out.push((top, f));
                }
            }
            let mut clone = p.clone();
            while let Some((from, to, f)) = find_next_template(&clone) {
                out.push((top, f));
                if clone.delete(from, to).is_err() {
                    break;
                }
            }
        }
        Block::Table(t) => {
            for r in &t.rows {
                for c in &r.cells {
                    for cb in &c.blocks {
                        walk_collect(cb, top, depth + 1, out);
                    }
                }
            }
        }
    }
}

/// Evaluates all `strix:expr` controls in `s.doc.body` against the evaluated worksheets `(main, exercises)`,
/// updating their displayed text + `Unit` run style and returning `(updated_count, errors)`.
pub(crate) fn apply_sheets(
    s: &mut Session,
    zones: &[Option<usize>],
    main: &strix_calc::Worksheet,
    exercises: &std::collections::BTreeMap<usize, strix_calc::Worksheet>,
) -> (usize, Vec<Value>) {
    let converted = convert_all_templates(s);
    let mut has_any = false;
    for b in &s.doc.body {
        if block_has_expr(b, 0) {
            has_any = true;
            break;
        }
    }
    if !has_any {
        return (usize::from(converted), Vec::new());
    }
    super::super::calc::unit_color(s);
    let mut updates = usize::from(converted);
    let mut errors = Vec::new();
    let layout = super::scan(s);
    for (top, b) in s.doc.body.iter_mut().enumerate() {
        let ws = zones.get(top).copied().flatten().and_then(|e| exercises.get(&e)).unwrap_or(main);
        let teacher = zones.get(top).copied().flatten().and_then(|e| layout.exercises.get(e)).is_none_or(|e| e.show.solution);
        apply_block(b, top, ws, teacher, 0, &mut updates, &mut errors);
    }
    if updates > 0 {
        s.touch();
        s.clamp_selection();
    }
    (updates, errors)
}

fn block_has_expr(b: &Block, depth: usize) -> bool {
    if depth > 16 {
        return false;
    }
    match b {
        Block::Para(p) => !expr_spans(p).is_empty(),
        Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).flat_map(|c| &c.blocks).any(|cb| block_has_expr(cb, depth + 1)),
    }
}

fn apply_block(
    block: &mut Arc<Block>,
    top: usize,
    ws: &strix_calc::Worksheet,
    teacher: bool,
    depth: usize,
    updates: &mut usize,
    errors: &mut Vec<Value>,
) {
    if depth > 16 {
        return;
    }
    if !block_has_expr(block, depth) {
        return;
    }
    match &**block {
        Block::Para(p) => {
            let planned = plan_para_updates(p, top, ws, teacher, errors);
            if planned.is_empty() {
                return;
            }
            let Block::Para(p) = Arc::make_mut(block) else { return };
            for (idx, shown) in planned.into_iter().rev() {
                let spans = expr_spans(p);
                if let Some(&(a, b)) = spans.get(idx)
                    && write_shown_in_span(p, (a, b), &shown)
                {
                    *updates += 1;
                }
            }
        }
        Block::Table(_) => {
            let Block::Table(t) = Arc::make_mut(block) else { return };
            for r in &mut t.rows {
                for c in &mut r.cells {
                    for cb in &mut c.blocks {
                        apply_block(cb, top, ws, teacher, depth + 1, updates, errors);
                    }
                }
            }
        }
    }
}

fn plan_para_updates(p: &Paragraph, top: usize, ws: &strix_calc::Worksheet, teacher: bool, errors: &mut Vec<Value>) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (idx, (a, b)) in expr_spans(p).into_iter().enumerate() {
        let Some(InlineObject::ControlStart { control }) = p.object_at(a) else { continue };
        let Some(formula) = read_formula(control) else { continue };
        let mut desired = match eval_in_sheet(ws, &formula) {
            Ok(val) => val,
            Err(err) => {
                errors.push(json!({"expr": formula, "top": top, "error": err}));
                format!("⚠ {formula}")
            }
        };
        if control.tag == super::stem_items::TAG_BLANK && !teacher {
            let unit = split_number_and_unit(&desired).1.trim();
            desired = if unit.is_empty() { "____________".into() } else { format!("____________ {unit}") };
        }
        let current = p.text.get(a + OBJ.len_utf8()..b).unwrap_or_default();
        let (_, unit_part) = split_number_and_unit(&desired);
        let unit_styled = unit_part.is_empty() || {
            let unit_start = a + OBJ.len_utf8() + desired.len() - unit_part.len();
            p.props_of_char(unit_start).style.as_deref() == Some(super::super::calc::UNIT_STYLE)
        };
        if current != desired || !unit_styled {
            out.push((idx, desired));
        }
    }
    out
}

fn write_shown_in_span(p: &mut Paragraph, (a, b): (usize, usize), shown: &str) -> bool {
    let from = a + OBJ.len_utf8();
    let mut base = p.props_of_char(a).clone();
    if base.style.as_deref() == Some(super::super::calc::UNIT_STYLE) {
        base.style = None;
    }
    if b > from && p.delete(from, b).is_err() {
        return false;
    }
    let (num_part, unit_part) = split_number_and_unit(shown);
    if !num_part.is_empty() && p.insert_text(from, num_part, &base).is_err() {
        return false;
    }
    if !unit_part.is_empty() {
        let unit_props = CharProps { style: Some(super::super::calc::UNIT_STYLE.into()), ..base.clone() };
        let at = from + num_part.len();
        if p.insert_text(at, unit_part, &unit_props).is_err() {
            return false;
        }
    }
    p.touch();
    true
}

/// Evaluates a multiple-choice option paragraph `p` against `ws`, stripping its check box and
/// teacher rationale so option collisions across variants can be detected cleanly.
pub(crate) fn render_choice_with_sheet(p: &Paragraph, ws: &strix_calc::Worksheet) -> String {
    let mut clone = p.clone();
    convert_templates_in_para(&mut clone);
    for (a, b) in expr_spans(&clone).into_iter().rev() {
        if let Some(InlineObject::ControlStart { control }) = clone.object_at(a)
            && let Some(formula) = read_formula(control)
            && let Ok(shown) = eval_in_sheet(ws, &formula)
        {
            let _ = write_shown_in_span(&mut clone, (a, b), &shown);
        }
    }
    super::items::choice_text(&clone)
}

/// Whether any block in `blocks` holds a `strix:expr` control or a `{{...}}` template.
pub(crate) fn has_any_expr_or_template(blocks: &[Arc<Block>]) -> bool {
    blocks.iter().any(|b| block_has_expr(b, 0) || has_template(b, 0))
}

/// Hash of all `strix:expr` formulas, displayed texts, and unexpanded `{{...}}` templates in `blocks`.
pub(crate) fn fingerprint(blocks: &[Arc<Block>]) -> u64 {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for (top, b) in blocks.iter().enumerate() {
        hash_block(b, top, 0, &mut h);
    }
    h.finish()
}

fn hash_block(b: &Block, top: usize, depth: usize, h: &mut impl std::hash::Hasher) {
    use std::hash::Hash;
    if depth > 16 {
        return;
    }
    match b {
        Block::Para(p) => {
            for (a, end) in expr_spans(p) {
                if let Some(InlineObject::ControlStart { control }) = p.object_at(a) {
                    (top, a, read_formula(control), p.text.get(a + OBJ.len_utf8()..end)).hash(h);
                }
            }
            if p.text.contains("{{") && p.text.contains("}}") {
                (top, &p.text).hash(h);
            }
        }
        Block::Table(t) => {
            for r in &t.rows {
                for c in &r.cells {
                    for cb in &c.blocks {
                        hash_block(cb, top, depth + 1, h);
                    }
                }
            }
        }
    }
}

/// `exercise.insertExpr`: inserts a live inline `strix:expr` control at the caret (or updates one
/// the caret is inside).
pub(crate) fn insert_command(s: &mut Session, v: &Value) -> CmdResult {
    let raw = p::req_str(v, "expr")?;
    let formula = clean_expr(raw);
    if formula.is_empty() {
        return Err(CmdError::Params("expr: non-empty expression required".into()));
    }
    if s.sel.focus.story != StoryRef::Body {
        return Err(CmdError::Disabled("inline expressions go in the document body".into()));
    }
    let focus = s.sel.focus.clone();
    {
        let para = s.doc.para_mut(focus.story, &focus.path).map_err(|e| CmdError::Failed(e.to_string()))?;
        // If caret is already inside a `strix:expr`, update its formula in place.
        if let Some(&(a, _)) = expr_spans(para).iter().find(|&&(a, b)| (a..=b + OBJ.len_utf8()).contains(&focus.off)) {
            if let Some(InlineObject::ControlStart { control }) = para.object_at_mut(a) {
                control.title = formula.clone();
                write_formula(control, &formula);
            }
            para.touch();
        } else {
            let at = focus.off.min(para.text.len());
            let props = if at > 0 { para.props_of_char(at.saturating_sub(1)).clone() } else { CharProps::default() };
            let ctl = make_control(&formula);
            para.insert_object(at, InlineObject::ControlStart { control: Box::new(ctl) }, &props).map_err(|e| CmdError::Failed(e.to_string()))?;
            let inner = at + OBJ.len_utf8();
            let _ = para.insert_text(inner, &formula, &props);
            let _ = para.insert_object(inner + formula.len(), InlineObject::ControlEnd, &props);
            para.touch();
            s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(focus.story, focus.path.clone(), inner + formula.len() + OBJ.len_utf8()));
        }
    }
    s.touch();
    let opts = super::super::calc::options_now(s);
    super::super::calc::recalculate(s, &opts);
    super::update(s);
    s.clamp_selection();
    Ok(json!({"expr": formula}))
}
