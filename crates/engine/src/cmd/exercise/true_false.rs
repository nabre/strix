//! True/false questions answered on their statement's line (docs/question-modes.md, True or
//! false).
//!
//! A true/false question's answer is inline in its question paragraph, after the statement and
//! before the points: an inline `strix:exercise.answer` control (`block` off) holding a tab, the
//! True box, a tab, the False box. Two right-aligned tab stops at the same distance from the
//! right margin in every statement put the boxes in two columns at the right of the text column,
//! left of the points, so a battery of statements lines up (the boxes sit on the statement's last
//! line, where the points are). No right indent: the points go to the margin with a positional
//! tab, which a line wraps before when it is past the right indent. Above
//! the first statement of each run of true/false questions a generated header line
//! (`strix:exercise.trueFalseHead`) gives the columns their names: the initials of True and False
//! in the exercise's language (V F, T F, W F).
//!
//! A true/false question is made (and converted to) as the two options of a multiple choice,
//! each with its box; [`restructure`] moves those boxes onto the statement's line, so files from
//! before are laid out the same way. Converting it to another mode first puts the options back
//! ([`expand`]). The boxes keep the multiple choice's meaning: ticked on the correct one while the
//! answers show, empty in the students' copy, a box ticked by hand while they show marks its
//! answer correct (and clears the other).

use std::sync::Arc;

use wordcraft_doc::control::{ContentControl, ControlKind, ControlLock};
use wordcraft_doc::para::OBJ;
use wordcraft_doc::props::{CharProps, TabAlign, TabLeader, TabStop};
use wordcraft_doc::{Block, InlineObject, Paragraph, StoryRef, para_block};

use super::items::{self, CHECKED, CORRECT_EL, Item, SHOWN_EL, STYLE_CHOICE, TAG_ANSWER, TAG_BOX, TAG_ITEM, TAG_POINTS, UNCHECKED, span};
use super::words::Words;
use super::{Layout, control, para, scan};
use crate::Session;

/// The header line over a battery of true/false statements: the columns' names.
pub(super) const TAG_HEAD: &str = "strix:exercise.trueFalseHead";

/// Room left at the right of the columns for the points (`.......... / 10 pt`).
const POINTS_ROOM_PT: f32 = 70.0;
/// The width of one column (True, False).
const COLUMN_PT: f32 = 28.0;
/// About half a box's width: the header's names are centred over the boxes.
const BOX_HALF_PT: f32 = 5.0;

/// Where the boxes go in a text column: the right edges of the True and False boxes.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Columns {
    true_at: f32,
    false_at: f32,
}

fn columns(width: f32) -> Columns {
    let width = if width.is_finite() { width.clamp(4.0 * COLUMN_PT, 4000.0) } else { 451.0 };
    let room = (POINTS_ROOM_PT + 2.0 * COLUMN_PT).min(width / 2.0);
    let false_at = width - room + 2.0 * COLUMN_PT;
    Columns { true_at: false_at - COLUMN_PT, false_at }
}

/// A statement's tab stops: each box right-aligned at its column's edge.
fn statement_tabs(c: &Columns) -> Vec<TabStop> {
    vec![
        TabStop { pos: c.true_at, align: TabAlign::Right, leader: TabLeader::None },
        TabStop { pos: c.false_at, align: TabAlign::Right, leader: TabLeader::None },
    ]
}

/// The header's tab stops: each name centred over its box.
fn head_tabs(c: &Columns) -> Vec<TabStop> {
    vec![
        TabStop { pos: c.true_at - BOX_HALF_PT, align: TabAlign::Center, leader: TabLeader::None },
        TabStop { pos: c.false_at - BOX_HALF_PT, align: TabAlign::Center, leader: TabLeader::None },
    ]
}

fn initial(word: &str) -> String {
    word.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default()
}

/// The name of column `j` (0 True, 1 False) as the header shows it: the word's initial.
pub(super) fn column_letter(w: &Words, j: usize) -> Option<String> {
    match j {
        0 => Some(initial(w.true_word)),
        1 => Some(initial(w.false_word)),
        _ => None,
    }
}

/// The header's text: the initials of True and False, each after its tab.
fn head_text(w: &Words) -> String {
    format!("\t{}\t{}", initial(w.true_word), initial(w.false_word))
}

/// Whether `t` is True or False in one of the exercise languages (a box named by Strix, not by
/// the author).
fn is_tf_word(t: &str) -> bool {
    super::words::ALL_WORDS.iter().any(|w| t.eq_ignore_ascii_case(w.true_word) || t.eq_ignore_ascii_case(w.false_word))
}

/// The start and end markers of a question paragraph's inline answer control.
pub(super) fn answer_span(p: &Paragraph) -> Option<(usize, usize)> {
    let (a, b) = span(p, TAG_ANSWER)?;
    matches!(p.object_at(a), Some(InlineObject::ControlStart { control }) if !control.block).then_some((a, b))
}

/// The boxes inside the answer control spanning `a..b`: their start and end markers.
fn boxes(p: &Paragraph, (a, b): (usize, usize)) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut open = None;
    for (at, o) in p.objects_with_offsets() {
        if at <= a || at >= b {
            continue;
        }
        match o {
            InlineObject::ControlStart { control } if open.is_none() && control.tag == TAG_BOX => open = Some(at),
            InlineObject::ControlEnd => {
                if let Some(x) = open.take() {
                    out.push((x, at));
                }
            }
            _ => {}
        }
    }
    out
}

fn control_at(p: &Paragraph, at: usize) -> Option<&ContentControl> {
    match p.object_at(at) {
        Some(InlineObject::ControlStart { control }) => Some(control),
        _ => None,
    }
}

/// The box controls of an inline answer, in order (True, False).
fn box_controls(p: &Paragraph, sp: (usize, usize)) -> Vec<ContentControl> {
    boxes(p, sp).into_iter().filter_map(|(x, _)| control_at(p, x).cloned()).collect()
}

/// A box's state: ticked, marked correct, last written with the answers shown.
fn state_of(c: &ContentControl) -> (bool, bool, bool) {
    let checked = matches!(c.kind, ControlKind::CheckBox { checked: true, .. });
    (checked, items::is_correct(c), c.extra.iter().any(|x| x == SHOWN_EL))
}

/// Which boxes are correct now: while the answers show and showed at the last update, a box is
/// the truth (a click changes the answer); else its mark. True or false, not both: a box just
/// ticked by hand clears the other.
fn correct_now(states: &[(bool, bool, bool)], shown: bool) -> Vec<bool> {
    let now: Vec<bool> = states.iter().map(|&(checked, correct, was)| if shown && was { checked } else { correct }).collect();
    if now.len() == 2 && now.iter().all(|&x| x) {
        let fresh: Vec<bool> = states.iter().map(|&(checked, correct, was)| shown && was && checked && !correct).collect();
        if fresh.iter().filter(|&&f| f).count() == 1 {
            return fresh;
        }
    }
    now
}

/// The paragraph at `a` when `(a, b)` is an inline answer (a true/false question's).
fn inline_para(s: &Session, (a, b): (usize, usize)) -> Option<(&Paragraph, (usize, usize))> {
    if a != b {
        return None;
    }
    let p = s.doc.body.get(a)?.as_para()?;
    Some((p, answer_span(p)?))
}

/// The options of an inline answer (each box's name) with whether each is correct.
pub(super) fn choices(s: &Session, r: (usize, usize)) -> Option<Vec<(String, bool)>> {
    let (p, sp) = inline_para(s, r)?;
    Some(box_controls(p, sp).iter().map(|c| (c.title.clone(), items::is_correct(c))).collect())
}

/// The diagnostic rationales of an inline answer's boxes.
pub(super) fn rationales(s: &Session, r: (usize, usize)) -> Option<Vec<Option<String>>> {
    let (p, sp) = inline_para(s, r)?;
    Some(box_controls(p, sp).iter().map(super::items_ext::read_rationale).collect())
}

/// `exercise.questions` `correct` on an inline answer: marks those boxes correct (0-based).
/// Whether `(a, b)` is one (so nothing else is to be done).
pub(super) fn apply_correct(s: &mut Session, r: (usize, usize), want: Option<&[usize]>) -> bool {
    let Some((p, sp)) = inline_para(s, r) else { return false };
    let found = boxes(p, sp);
    let Some(want) = want else { return true };
    let Some(block) = s.doc.body.get_mut(r.0) else { return true };
    let Block::Para(p) = Arc::make_mut(block) else { return true };
    for (k, (x, _)) in found.into_iter().enumerate() {
        if let Some(c) = items::control_at_mut(p, x) {
            c.extra.retain(|e| e != CORRECT_EL);
            let on = want.contains(&k);
            if on {
                c.extra.push(CORRECT_EL.into());
            }
            if let ControlKind::CheckBox { checked, .. } = &mut c.kind {
                *checked = on;
            }
        }
    }
    p.touch();
    true
}

/// Inserts each box after a tab (and its name in `names`, bold, when there is one) at `at`, its
/// symbol showing its state; the offset after them.
fn insert_boxes(p: &mut Paragraph, mut at: usize, boxes: Vec<ContentControl>, names: &[String]) -> Option<usize> {
    // On a title line (named boxes), the question's weight and size, not the title's.
    let props = match span(p, TAG_ITEM) {
        Some((a, _)) if !names.is_empty() => p.props_of_char(a + OBJ.len_utf8()).clone(),
        _ => CharProps::default(),
    };
    for (k, c) in boxes.into_iter().enumerate() {
        let sym = if matches!(c.kind, ControlKind::CheckBox { checked: true, .. }) { CHECKED } else { UNCHECKED }.to_string();
        p.insert_text(at, "\t", &props).ok()?;
        at += 1;
        if let Some(name) = names.get(k) {
            let named = format!("{name} ");
            p.insert_text(at, &named, &props).ok()?;
            let _ = p.format(at, at + name.len(), &|c| c.bold = Some(true));
            at += named.len();
        }
        p.insert_object(at, InlineObject::ControlStart { control: Box::new(c) }, &props).ok()?;
        at += OBJ.len_utf8();
        p.insert_text(at, &sym, &props).ok()?;
        at += sym.len();
        p.insert_object(at, InlineObject::ControlEnd, &props).ok()?;
        at += OBJ.len_utf8();
    }
    Some(at)
}

/// The content an inline answer shows for `boxes`: tab, box, tab, box (each box after its name
/// in `names`, when there is one).
fn content_text(boxes: &[ContentControl], names: &[String]) -> String {
    boxes
        .iter()
        .enumerate()
        .map(|(k, c)| {
            let sym = if matches!(c.kind, ControlKind::CheckBox { checked: true, .. }) { CHECKED } else { UNCHECKED };
            let name = names.get(k).map(|n| format!("{n} ")).unwrap_or_default();
            format!("\t{name}{OBJ}{sym}{OBJ}")
        })
        .collect()
}

/// The boxes' names on a title line holding the question (no header above it): the columns'
/// letters; none elsewhere.
fn names(p: &Paragraph, w: &Words) -> Vec<String> {
    if super::question_line::joined(p) { (0..2).filter_map(|j| column_letter(w, j)).collect() } else { Vec::new() }
}

/// A true/false question's line brought in line: its two boxes (ticked on the correct one while
/// the answers show, named True and False in the exercise's language) and its tabs. Whether it
/// changed; nothing for any other block.
pub(super) fn sync(block: &mut Arc<Block>, shown: bool, width: f32, w: &Words) -> bool {
    let Block::Para(p) = &**block else { return false };
    if !items::is_question(p) {
        return false;
    }
    let Some(sp) = answer_span(p) else { return false };
    let names = names(p, w);
    let have = box_controls(p, sp);
    let states: Vec<_> = (0..2).map(|k| have.get(k).map_or((false, false, false), state_of)).collect();
    let now = correct_now(&states, shown);
    let want: Vec<ContentControl> = (0..2)
        .map(|k| {
            let mut c = have.get(k).cloned().unwrap_or_else(|| items::check_box(false, false));
            c.extra.retain(|x| x != CORRECT_EL && x != SHOWN_EL);
            let on = now.get(k).copied().unwrap_or(false);
            if on {
                c.extra.push(CORRECT_EL.into());
            }
            if shown {
                c.extra.push(SHOWN_EL.into());
            }
            if let ControlKind::CheckBox { checked, .. } = &mut c.kind {
                *checked = shown && on;
            }
            let word = if k == 0 { w.true_word } else { w.false_word };
            if c.title.is_empty() || is_tf_word(&c.title) {
                c.title = word.into();
            }
            c
        })
        .collect();
    let content = content_text(&want, &names);
    let cols = columns(width);
    let tabs = statement_tabs(&cols);
    let same = have == want && p.text.get(sp.0 + OBJ.len_utf8()..sp.1) == Some(content.as_str()) && p.props.tabs.as_deref() == Some(tabs.as_slice());
    if same {
        return false;
    }
    let mut np = p.clone();
    let from = sp.0 + OBJ.len_utf8();
    if sp.1 > from && np.delete(from, sp.1).is_err() {
        return false;
    }
    if insert_boxes(&mut np, from, want, &names).is_none() {
        return false;
    }
    np.props.tabs = Some(tabs);
    np.touch();
    *block = para_block(np);
    true
}

/// Moves the positions of the selection in the body: `f` maps a top-level block and offset.
pub(super) fn remap(s: &mut Session, f: &dyn Fn(usize, usize) -> (usize, usize)) {
    for pos in [&mut s.sel.anchor, &mut s.sel.focus] {
        if pos.story != StoryRef::Body {
            continue;
        }
        let Some(top) = pos.path.0.first().copied() else { continue };
        let nested = pos.path.0.len() > 1;
        let (t, off) = f(top as usize, pos.off);
        if let Some(x) = pos.path.0.first_mut() {
            *x = u32::try_from(t).unwrap_or(u32::MAX);
        }
        if !nested {
            pos.off = off;
        }
    }
}

/// Every true/false question laid out on its line: options of a true/false question (as made,
/// or in a file from before) moved onto its statement's line, and a header over each battery.
/// Whether anything changed.
pub(super) fn restructure(s: &mut Session) -> bool {
    let any = s
        .doc
        .control_ranges(StoryRef::Body)
        .iter()
        .any(|r| r.control.tag == TAG_HEAD || (r.control.tag == TAG_ANSWER && items::read_answer(&r.control).true_false));
    if !any {
        return false;
    }
    let mut changed = false;
    let layout = scan(s);
    for (e, ex) in layout.exercises.iter().enumerate().rev() {
        let w = super::exercise_words(s, &layout, e);
        for it in items::items(s, &layout, e).iter().rev() {
            changed |= collapse(s, it, ex.show.solution, w);
        }
    }
    let layout = scan(s);
    for e in (0..layout.exercises.len()).rev() {
        changed |= heads(s, &layout, e);
    }
    if changed {
        s.touch();
    }
    changed
}

/// A true/false question answered by two options (paragraphs after it) gets them as two boxes
/// on its line. Whether it did.
fn collapse(s: &mut Session, it: &Item, shown: bool, w: &Words) -> bool {
    if !it.settings.true_false {
        return false;
    }
    let q = it.question;
    let Some((a, b)) = it.answer else { return false };
    if a != q + 1 || b != a + 1 {
        return false;
    }
    let mut answer = None;
    let mut opts = Vec::new();
    for (k, i) in (a..=b).enumerate() {
        let Some(p) = s.doc.body.get(i).and_then(|x| x.as_para()) else { return false };
        if p.props.style.as_deref() != Some(STYLE_CHOICE) {
            return false;
        }
        if i == a {
            answer = p.objects.iter().find_map(|o| match o {
                InlineObject::ControlStart { control } if control.tag == TAG_ANSWER => Some((**control).clone()),
                _ => None,
            });
        }
        let Some(mut c) = span(p, TAG_BOX).and_then(|(x, _)| control_at(p, x)).cloned() else { return false };
        let text = items::choice_text(p);
        c.title = if text.is_empty() { (if k == 0 { w.true_word } else { w.false_word }).into() } else { text };
        opts.push(c);
    }
    let Some(mut answer) = answer else { return false };
    answer.block = false;
    let states: Vec<_> = opts.iter().map(state_of).collect();
    for (c, on) in opts.iter_mut().zip(correct_now(&states, shown)) {
        c.extra.retain(|x| x != CORRECT_EL && x != SHOWN_EL);
        if on {
            c.extra.push(CORRECT_EL.into());
        }
        if let ControlKind::CheckBox { checked, .. } = &mut c.kind {
            *checked = false;
        }
    }
    let outer = items::outer_ends_after_answer(s, a, b);
    let Some(Block::Para(p)) = s.doc.body.get(q).map(|x| &**x) else { return false };
    let mut np = p.clone();
    let at = span(&np, TAG_POINTS).map_or_else(|| np.text.len().saturating_sub(items::trailing_outer_ends(&np) * OBJ.len_utf8()), |(x, _)| x);
    let props = CharProps::default();
    if np.insert_object(at, InlineObject::ControlStart { control: Box::new(answer) }, &props).is_err() {
        return false;
    }
    let names = names(&np, w);
    let Some(end) = insert_boxes(&mut np, at + OBJ.len_utf8(), opts, &names) else { return false };
    if np.insert_object(end, InlineObject::ControlEnd, &props).is_err() {
        return false;
    }
    for _ in 0..outer {
        let x = np.text.len();
        if np.insert_object(x, InlineObject::ControlEnd, &props).is_err() {
            return false;
        }
    }
    np.touch();
    let inserted = end + OBJ.len_utf8() - at;
    if let Some(block) = s.doc.body.get_mut(q) {
        *block = para_block(np);
    }
    s.doc.body.drain(a..=b);
    remap(s, &|top, off| {
        if (a..=b).contains(&top) {
            (q, at)
        } else if top > b {
            (top - 2, off)
        } else if top == q && off >= at {
            (q, off + inserted)
        } else {
            (top, off)
        }
    });
    true
}

/// A true/false question's boxes on its line go back to being its two options (paragraphs after
/// it, as a multiple choice has), before it is converted to another mode. Whether it did.
pub(super) fn expand(s: &mut Session, it: &Item, shown: bool) -> bool {
    let q = it.question;
    if it.answer != Some((q, q)) {
        return false;
    }
    let Some(Block::Para(p)) = s.doc.body.get(q).map(|x| &**x) else { return false };
    let Some(sp) = answer_span(p) else { return false };
    let Some(mut answer) = control_at(p, sp.0).cloned() else { return false };
    answer.block = true;
    let ctls = box_controls(p, sp);
    let states: Vec<_> = ctls.iter().map(state_of).collect();
    let now = correct_now(&states, shown);
    let mut init: Vec<(String, bool, Option<String>)> =
        ctls.iter().zip(now).map(|(c, on)| (c.title.clone(), on, super::items_ext::read_rationale(c))).collect();
    init.resize(2, (String::new(), false, None));
    let outer = items::trailing_outer_ends(p);
    let mut np = p.clone();
    for _ in 0..outer {
        if super::take_last_end(&mut np).is_err() {
            return false;
        }
    }
    if np.delete(sp.0, sp.1 + OBJ.len_utf8()).is_err() {
        return false;
    }
    np.props.tabs = None;
    np.touch();
    let paras = items::build_choice_paras(s, answer, &init, outer);
    let n = paras.len();
    if let Some(block) = s.doc.body.get_mut(q) {
        *block = para_block(np);
    }
    for (k, para) in paras.into_iter().enumerate() {
        s.doc.body.insert(q + 1 + k, para_block(para));
    }
    let cut = sp.0;
    remap(s, &|top, off| {
        if top > q {
            (top + n, off)
        } else if top == q && off > cut {
            (q, cut)
        } else {
            (top, off)
        }
    });
    s.touch();
    true
}

fn is_head(s: &Session, i: usize) -> bool {
    s.doc.body.get(i).and_then(|b| b.as_para()).is_some_and(|p| span(p, TAG_HEAD).is_some())
}

/// The headers of exercise `e`'s true/false batteries: one right before the first statement of
/// each run of true/false questions (questions with nothing but a header between them), none
/// anywhere else, each naming the columns in the exercise's language. Whether any changed.
fn heads(s: &mut Session, layout: &Layout, e: usize) -> bool {
    let Some(ex) = layout.exercises.get(e) else { return false };
    let Some((from, to)) = ex.question_blocks else { return false };
    let its = items::items(s, layout, e);
    let w = super::exercise_words(s, layout, e);
    let inline: Vec<bool> = its.iter().map(|it| it.answer == Some((it.question, it.question))).collect();
    let mut need = Vec::new();
    for (k, it) in its.iter().enumerate() {
        // A question on the title line names its boxes: no header goes above the number.
        if !inline.get(k).copied().unwrap_or(false) || it.question == ex.start {
            continue;
        }
        let joined = k > 0
            && inline.get(k - 1).copied().unwrap_or(false)
            && its.get(k - 1).is_some_and(|prev| (prev.question + 1..it.question).all(|i| is_head(s, i)));
        if !joined {
            need.push(it.question);
        }
    }
    let mut changed = false;
    // (index, insert): removals and insertions, last first so earlier indices stay valid.
    let mut ops: Vec<(usize, bool)> = Vec::new();
    let found: Vec<usize> = (from.saturating_sub(1)..=to).filter(|&i| is_head(s, i)).collect();
    for h in found {
        if need.contains(&(h + 1)) {
            changed |= refresh_head(s, h, w);
        } else {
            ops.push((h, false));
        }
    }
    for &q in &need {
        if q == 0 || !is_head(s, q - 1) {
            ops.push((q, true));
        }
    }
    ops.sort_by_key(|op| std::cmp::Reverse(op.0));
    for (i, insert) in ops {
        changed |= if insert { insert_head(s, i, w) } else { remove_head(s, i) };
    }
    changed
}

/// Formats a header's text: its names bold.
fn bold_head(p: &mut Paragraph) {
    if let Some((a, b)) = span(p, TAG_HEAD) {
        let _ = p.format(a + OBJ.len_utf8(), b, &|c| c.bold = Some(true));
    }
}

/// The header at `h` naming the columns as `w` does, at the columns of its text column.
fn refresh_head(s: &mut Session, h: usize, w: &Words) -> bool {
    let tabs = head_tabs(&columns(items::text_width_at(s, h)));
    let text = head_text(w);
    let Some(block) = s.doc.body.get_mut(h) else { return false };
    let Block::Para(p) = &**block else { return false };
    let Some(sp) = span(p, TAG_HEAD) else { return false };
    if p.text.get(sp.0 + OBJ.len_utf8()..sp.1) == Some(text.as_str()) && p.props.tabs.as_deref() == Some(tabs.as_slice()) {
        return false;
    }
    let Block::Para(p) = Arc::make_mut(block) else { return false };
    items::set_content(p, sp, &text);
    bold_head(p);
    p.props.tabs = Some(tabs);
    p.touch();
    true
}

/// Puts a header before the question at `q`. The controls opening at the start of the question
/// before its text (the questions part's, on the first one) open on the header instead.
fn insert_head(s: &mut Session, q: usize, w: &Words) -> bool {
    let width = items::text_width_at(s, q);
    let Some(Block::Para(qp)) = s.doc.body.get(q).map(|b| &**b) else { return false };
    let mut lead = Vec::new();
    let mut moved = 0;
    for (at, o) in qp.objects_with_offsets() {
        match o {
            InlineObject::ControlStart { control } if at == moved && control.tag != TAG_ITEM => {
                lead.push((**control).clone());
                moved = at + OBJ.len_utf8();
            }
            _ => break,
        }
    }
    let mut np = qp.clone();
    if moved > 0 {
        if np.delete(0, moved).is_err() {
            return false;
        }
        np.touch();
    }
    let mut head = para("Normal", "", lead, 0, None);
    let at = head.text.len();
    let mut ctl = control(TAG_HEAD, "", ControlLock::SdtContentLocked);
    ctl.block = false;
    let props = CharProps::default();
    if head.insert_object(at, InlineObject::ControlStart { control: Box::new(ctl) }, &props).is_err()
        || head.insert_object(at + OBJ.len_utf8(), InlineObject::ControlEnd, &props).is_err()
    {
        return false;
    }
    items::set_content(&mut head, (at, at + OBJ.len_utf8()), &head_text(w));
    bold_head(&mut head);
    head.props.tabs = Some(head_tabs(&columns(width)));
    head.props.space_before = Some(0.0);
    head.props.space_after = Some(0.0);
    head.props.keep_next = Some(true);
    head.touch();
    if let Some(block) = s.doc.body.get_mut(q) {
        *block = para_block(np);
    }
    s.doc.body.insert(q, para_block(head));
    remap(s, &|top, off| {
        if top == q {
            (q + 1, off.saturating_sub(moved))
        } else if top > q {
            (top + 1, off)
        } else {
            (top, off)
        }
    });
    true
}

/// Removes the header at `h`; the controls opening on it open on the paragraph after it. A
/// header that closes controls stays.
pub(super) fn remove_head(s: &mut Session, h: usize) -> bool {
    let Some(Block::Para(hp)) = s.doc.body.get(h).map(|b| &**b) else { return false };
    if items::trailing_outer_ends(hp) > 0 {
        return false;
    }
    let mut lead = Vec::new();
    let mut at_end = 0;
    for (at, o) in hp.objects_with_offsets() {
        match o {
            InlineObject::ControlStart { control } if at == at_end && control.tag != TAG_HEAD => {
                lead.push((**control).clone());
                at_end = at + OBJ.len_utf8();
            }
            _ => break,
        }
    }
    let moved = lead.len() * OBJ.len_utf8();
    if !lead.is_empty() {
        let Some(Block::Para(next)) = s.doc.body.get(h + 1).map(|b| &**b) else { return false };
        let mut np = next.clone();
        let props = CharProps::default();
        for (k, c) in lead.into_iter().enumerate() {
            if np.insert_object(k * OBJ.len_utf8(), InlineObject::ControlStart { control: Box::new(c) }, &props).is_err() {
                return false;
            }
        }
        np.touch();
        if let Some(block) = s.doc.body.get_mut(h + 1) {
            *block = para_block(np);
        }
    }
    s.doc.body.remove(h);
    remap(s, &|top, off| {
        if top == h {
            (h, 0)
        } else if top == h + 1 {
            (h, off + moved)
        } else if top > h + 1 {
            (top - 1, off)
        } else {
            (top, off)
        }
    });
    true
}

#[cfg(test)]
mod tests {
    use super::super::STYLE_QUESTION;
    use super::*;
    use serde_json::{Value, json};
    use wordcraft_doc::Document;

    fn run(s: &mut Session, id: &str, v: Value) -> Value {
        s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
    }

    fn session(lang: &str) -> Session {
        let mut s = Session::new(Document::from_text(""));
        Arc::make_mut(&mut s.doc.styles).default_chr.lang = Some(lang.into());
        s
    }

    fn plain(p: &Paragraph) -> String {
        p.text.chars().filter(|&c| c != OBJ).collect()
    }

    fn questions(s: &Session) -> Vec<&Paragraph> {
        s.doc.body.iter().filter_map(|b| b.as_para()).filter(|p| p.props.style.as_deref() == Some(STYLE_QUESTION)).collect()
    }

    fn heads(s: &Session) -> Vec<usize> {
        (0..s.doc.body.len()).filter(|&i| is_head(s, i)).collect()
    }

    fn battery(s: &mut Session) {
        run(
            s,
            "exercise.insert",
            json!({"kind": "trueFalse", "questions": [
                {"text": "Il ferro è un metallo.", "correct": 1, "points": 1},
                {"text": "L'aria è un solido e questa affermazione è abbastanza lunga da andare a capo su più righe nella colonna del testo.", "correct": 2, "points": 1},
                {"text": "L'acqua bolle a 100 °C.", "correct": 1, "points": 2}
            ]}),
        );
    }

    #[test]
    fn a_battery_has_its_boxes_in_two_columns_under_one_header() {
        let mut s = session("it-IT");
        battery(&mut s);
        let qs = questions(&s);
        assert_eq!(qs.len(), 3);
        // Each statement: its text, the two boxes after tabs, then the points.
        assert!(
            plain(qs[0]).starts_with("Il ferro è un metallo.\t☒\t☐\t") || plain(qs[0]).starts_with("Il ferro è un metallo.\t☒\t☐"),
            "{:?}",
            plain(qs[0])
        );
        assert!(plain(qs[1]).contains("testo.\t☐\t☒"), "{:?}", plain(qs[1]));
        // The same tab stops on every statement: the columns line up.
        let tabs = qs[0].props.tabs.clone().expect("tabs");
        assert_eq!(tabs.len(), 2);
        assert!(tabs.iter().all(|t| t.align == TabAlign::Right));
        assert!(qs.iter().all(|p| p.props.tabs.as_ref() == Some(&tabs)));
        assert!(tabs[0].pos < tabs[1].pos && tabs[1].pos <= items::text_width_at(&s, 0) - POINTS_ROOM_PT + 0.01);
        // No option paragraphs left.
        assert!(!s.doc.body.iter().filter_map(|b| b.as_para()).any(|p| p.props.style.as_deref() == Some(STYLE_CHOICE)));
        // One header, right before the first statement, naming the columns V and F.
        let h = heads(&s);
        assert_eq!(h.len(), 1);
        let head = s.doc.body.get(h[0]).and_then(|b| b.as_para()).expect("head");
        assert_eq!(plain(head), "\tV\tF");
        assert_eq!(s.doc.body.get(h[0] + 1).and_then(|b| b.as_para()).and_then(items::question_text).as_deref(), Some("Il ferro è un metallo."));
        // The header's names sit over the boxes.
        let ht = head.props.tabs.clone().expect("head tabs");
        assert!((ht[0].pos - (tabs[0].pos - BOX_HALF_PT)).abs() < 0.01 && (ht[1].pos - (tabs[1].pos - BOX_HALF_PT)).abs() < 0.01);
        // The exercise reads them as before: three true/false questions, labels, correct answers.
        let st = run(&mut s, "exercise.questions", json!({"exercise": 1}));
        let q = st["questions"].as_array().expect("questions");
        assert_eq!(q.len(), 3);
        assert_eq!(q[0]["label"], "a)");
        assert_eq!(q[1]["trueFalse"], true);
        assert_eq!(q[1]["choices"], json!(["Vero", "Falso"]));
        assert_eq!(q[0]["correct"], json!(["A"]));
        assert_eq!(q[1]["correct"], json!(["B"]));
        assert_eq!(st["total"], 4.0);
        // Running the update again changes nothing.
        let before = s.doc.body.clone();
        assert!(!super::super::update(&mut s));
        assert_eq!(s.doc.body, before);
    }

    #[test]
    fn the_students_copy_has_empty_boxes_and_a_click_changes_the_answer() {
        let mut s = session("it-IT");
        battery(&mut s);
        run(&mut s, "exercise.show", json!({"exercise": 1, "solution": false}));
        assert!(questions(&s).iter().all(|p| !plain(p).contains('☒')), "{:?}", questions(&s).iter().map(|p| plain(p)).collect::<Vec<_>>());
        run(&mut s, "exercise.show", json!({"exercise": 1, "solution": true}));
        assert!(plain(questions(&s)[0]).contains("\t☒\t☐"));
        // The teacher ticks False on the first statement: it becomes the answer, True clears.
        let falso = s.doc.control_ranges(StoryRef::Body).into_iter().filter(|r| r.control.tag == TAG_BOX).nth(1).expect("box");
        run(&mut s, "developer.toggleCheckBox", json!({"at": falso.content_start()}));
        assert!(plain(questions(&s)[0]).contains("\t☐\t☒"), "{:?}", plain(questions(&s)[0]));
        let st = run(&mut s, "exercise.questions", json!({"exercise": 1}));
        assert_eq!(st["questions"][0]["correct"], json!(["B"]));
        // And through the command.
        run(&mut s, "exercise.questions", json!({"exercise": 1, "question": 1, "correct": "A"}));
        assert!(plain(questions(&s)[0]).contains("\t☒\t☐"), "{:?}", plain(questions(&s)[0]));
    }

    #[test]
    fn converting_moves_the_boxes_and_the_header_follows_the_battery() {
        let mut s = session("it-IT");
        battery(&mut s);
        let first = heads(&s)[0];
        // The first statement becomes an open question: the header moves to the second.
        run(&mut s, "exercise.questionMode", json!({"exercise": 1, "question": 1, "type": "open"}));
        let st = run(&mut s, "exercise.questions", json!({"exercise": 1}));
        assert_eq!(st["questions"][0]["type"], "open");
        assert_eq!(st["questions"].as_array().map(Vec::len), Some(3));
        let h = heads(&s);
        assert_eq!(h.len(), 1);
        assert!(h[0] > first);
        assert_eq!(
            s.doc.body.get(h[0] + 1).and_then(|b| b.as_para()).and_then(items::question_text).as_deref(),
            Some(st["questions"][1]["text"].as_str().unwrap_or_default())
        );
        assert!(questions(&s)[0].props.tabs.is_none());
        // To multiple choice: Vero, Falso and two empty options, the correct one kept.
        run(&mut s, "exercise.questionMode", json!({"exercise": 1, "question": 3, "type": "choice"}));
        let st = run(&mut s, "exercise.questions", json!({"exercise": 1}));
        assert_eq!(st["questions"][2]["choices"], json!(["Vero", "Falso", "", ""]));
        assert_eq!(st["questions"][2]["correct"], json!(["A"]));
        assert_eq!(st["questions"][2]["trueFalse"], false);
        // Back to true/false, and the first one too: one battery again, one header at the top.
        run(&mut s, "exercise.questionMode", json!({"exercise": 1, "question": 3, "type": "trueFalse"}));
        run(&mut s, "exercise.questionMode", json!({"exercise": 1, "question": 1, "type": "trueFalse"}));
        let st = run(&mut s, "exercise.questions", json!({"exercise": 1}));
        assert!(st["questions"].as_array().is_some_and(|q| q.iter().all(|q| q["trueFalse"] == true)), "{st}");
        assert_eq!(st["questions"][2]["choices"], json!(["Vero", "Falso"]));
        assert_eq!(st["questions"][2]["correct"], json!(["A"]));
        assert_eq!(heads(&s), vec![first]);
        // The scan still finds the exercise's three questions.
        let layout = scan(&s);
        assert_eq!(layout.exercises.first().map(|e| e.questions.len()), Some(3));
    }

    #[test]
    fn mixed_quizzes_get_a_header_over_each_run_and_added_statements_join_it() {
        let mut s = session("it-IT");
        run(
            &mut s,
            "exercise.insert",
            json!({"kind": "open", "questions": [
                "Che cos'è un rettangolo?",
                {"text": "Un quadrato è un rettangolo.", "type": "trueFalse", "correct": 1},
                {"text": "Un rombo è un quadrato.", "type": "trueFalse", "correct": 2},
                "Disegna un trapezio.",
                {"text": "Un trapezio ha due lati paralleli.", "type": "trueFalse", "correct": 1}
            ]}),
        );
        let qs: Vec<usize> = (0..s.doc.body.len())
            .filter(|&i| s.doc.body.get(i).and_then(|b| b.as_para()).is_some_and(|p| p.props.style.as_deref() == Some(STYLE_QUESTION)))
            .collect();
        assert_eq!(qs.len(), 5);
        assert_eq!(heads(&s), vec![qs[1] - 1, qs[4] - 1]);
        // A statement added at the end joins the last battery: no new header.
        run(&mut s, "exercise.addQuestion", json!({"text": "Un cerchio ha angoli.", "type": "trueFalse", "correct": 2}));
        assert_eq!(heads(&s).len(), 2);
        let st = run(&mut s, "exercise.questions", json!({"exercise": 1}));
        assert_eq!(st["questions"].as_array().map(Vec::len), Some(6));
        assert_eq!(st["questions"][5]["correct"], json!(["B"]));
        // The exercise still ends after its last statement: the text goes on after it.
        let layout = scan(&s);
        let e = layout.exercises.first().expect("exercise");
        assert_eq!(e.questions.len(), 6);
    }

    #[test]
    fn the_header_speaks_the_exercise_language() {
        for (lang, want) in [("en-GB", "\tT\tF"), ("de-DE", "\tW\tF"), ("fr-FR", "\tV\tF")] {
            let mut s = session(lang);
            run(&mut s, "exercise.insert", json!({"kind": "trueFalse", "questions": [{"text": "x", "correct": 1}, {"text": "y", "correct": 2}]}));
            let h = heads(&s);
            assert_eq!(h.len(), 1, "{lang}");
            assert_eq!(s.doc.body.get(h[0]).and_then(|b| b.as_para()).map(plain).as_deref(), Some(want), "{lang}");
        }
    }

    #[test]
    fn the_results_summary_names_the_answers_like_the_header() {
        for (lang, t, f) in [("it-IT", "V", "F"), ("en-GB", "T", "F"), ("de-DE", "W", "F")] {
            let mut s = session(lang);
            run(
                &mut s,
                "exercise.insert",
                json!({"kind": "trueFalse", "questions": [
                    {"text": "x", "correct": 1},
                    {"text": "y", "correct": 2},
                    {"text": "z", "type": "choice", "choices": ["p", "q", "r"], "correct": [2]}
                ]}),
            );
            run(&mut s, "exercise.show", json!({"solution": true, "results": true}));
            let lines: Vec<String> = s.doc.body.iter().filter_map(|b| b.as_para()).map(plain).collect();
            for want in [format!("a) {t}"), format!("b) {f}"), "c) B".to_string()] {
                assert!(lines.contains(&want), "{lang}: {want} in {lines:?}");
            }
        }
    }

    #[test]
    fn options_from_before_move_onto_the_line() {
        let mut s = session("it-IT");
        battery(&mut s);
        // Put the first statement's options back, as files from before have them.
        let layout = scan(&s);
        let its = items::items(&s, &layout, 0);
        assert!(expand(&mut s, &its[0], true));
        assert_eq!(s.doc.body.iter().filter_map(|b| b.as_para()).filter(|p| p.props.style.as_deref() == Some(STYLE_CHOICE)).count(), 2);
        super::super::update(&mut s);
        assert_eq!(s.doc.body.iter().filter_map(|b| b.as_para()).filter(|p| p.props.style.as_deref() == Some(STYLE_CHOICE)).count(), 0);
        assert!(plain(questions(&s)[0]).contains("\t☒\t☐"));
        assert_eq!(heads(&s).len(), 1);
    }

    #[test]
    fn a_battery_survives_saving_as_docx() {
        let mut s = session("it-IT");
        battery(&mut s);
        let bytes = wordcraft_docx::write(&s.doc).expect("write");
        let mut back = Session::new(wordcraft_docx::read(&bytes).expect("read"));
        super::super::update(&mut back);
        let st = run(&mut back, "exercise.questions", json!({"exercise": 1}));
        assert_eq!(st["questions"].as_array().map(Vec::len), Some(3), "{st}");
        assert_eq!(st["questions"][1]["correct"], json!(["B"]));
        assert_eq!(st["questions"][2]["points"], 2.0);
        assert_eq!(heads(&back).len(), 1);
        let qs = questions(&back);
        assert!(plain(qs[0]).contains("\t☒\t☐"), "{:?}", plain(qs[0]));
        assert_eq!(qs[0].props.tabs, questions(&s)[0].props.tabs);
    }
}
