//! Open-answer and multiple-choice exercises (docs/exercise.md, Kinds of exercise).
//!
//! A problem has a statement, data, questions and a worked solution. The other two kinds are
//! quizzes: an exercise **is** its questions, the statement is optional (absent unless asked
//! for), and each question is followed by its **answer** (`strix:exercise.answer`, a block
//! control that can't be deleted):
//!
//! - **open answer**: a space of a given height (cm), blank or squared (a graph paper block,
//!   `ParaProps::graph`, of that height), the answer written in it;
//! - **multiple choice**: the options, lettered A), B)… (a list of their own), each starting with
//!   a check box (`strix:exercise.box`) ticked on the correct ones: more than one may be correct.
//!
//! The answer control keeps the question's settings (a `strix:answerSpace` element Word ignores):
//! its maximum points, the space's height, squares and their size. The question's text is in an
//! inline control (`strix:exercise.item`, its placeholder shown until typed over) followed by the
//! points (`strix:exercise.points`, generated: ` (2 punti)`).
//!
//! The exercise's Solution switch (`exercise.show`) shows or hides the answers: an open answer's
//! text becomes hidden text (the space keeps its height), the boxes of a multiple choice are all
//! empty. A lone question has no letter. The points of the questions add up to the exercise's
//! points, which the scores table shows.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use wordcraft_doc::control::{ContentControl, ControlKind, ControlLock};
use wordcraft_doc::graph_paper::{GraphPaper, GridLineStyle, GridPlacement, PT_PER_MM};
use wordcraft_doc::para::OBJ;
use wordcraft_doc::props::{CharProps, NumRef};
use wordcraft_doc::{Block, InlineObject, Paragraph, StoryRef, para_block};

use super::{Layout, MAX_QUESTIONS, STYLE_QUESTION, TAG_EXERCISE, TAG_QUESTIONS, control, para, scan};
use crate::{CmdError, CmdResult, Session, p};

/// The question's text, inline in its paragraph, with its placeholder.
pub const TAG_ITEM: &str = "strix:exercise.item";
/// The question's maximum points, generated after its text.
pub const TAG_POINTS: &str = "strix:exercise.points";
/// After each question: the answer space or the options, with the question's settings.
pub const TAG_ANSWER: &str = "strix:exercise.answer";
/// The check box at the start of each option.
pub const TAG_BOX: &str = "strix:exercise.box";

pub(super) const STYLE_ANSWER: &str = "ExerciseAnswer";
pub(super) const STYLE_CHOICE: &str = "ExerciseChoice";

const KIND_EL: &str = "strix:exerciseKind";
const ANSWER_EL: &str = "strix:answerSpace";
const CORRECT_EL: &str = "<strix:correct/>";
/// On a box last written with the answers shown: only then is its state a click to take.
const SHOWN_EL: &str = "<strix:shown/>";

/// Options of one question at most.
const MAX_CHOICES: usize = 26;
const MAX_POINTS: f64 = 1000.0;
const MIN_HEIGHT_CM: f32 = 0.5;
const MAX_HEIGHT_CM: f32 = 25.0;
const DEFAULT_HEIGHT_CM: f32 = 4.0;
const DEFAULT_SPACING_MM: f32 = 5.0;
const CHECKED: char = '\u{2612}';
const UNCHECKED: char = '\u{2610}';

/// What an exercise is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) enum Kind {
    /// Statement, data, questions, solution (the original exercise).
    #[default]
    Problem,
    /// Questions answered in a space of their own.
    Open,
    /// Questions answered by ticking options.
    Choice,
}

impl Kind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Kind::Problem => "problem",
            Kind::Open => "open",
            Kind::Choice => "choice",
        }
    }

    fn parse(t: &str) -> Option<Kind> {
        Some(match t {
            "problem" => Kind::Problem,
            "open" => Kind::Open,
            "choice" | "trueFalse" => Kind::Choice,
            _ => return None,
        })
    }

    /// A quiz: questions with answers, no solution.
    pub(crate) fn quiz(self) -> bool {
        self != Kind::Problem
    }
}

/// The kind kept on an exercise control.
pub(crate) fn read_kind(ctl: &ContentControl) -> Kind {
    let open = format!("<{KIND_EL} ");
    ctl.extra
        .iter()
        .find(|x| x.starts_with(&open))
        .and_then(|el| el.split_once("val=\"")?.1.split_once('"').map(|(v, _)| v.to_string()))
        .and_then(|v| Kind::parse(&v))
        .unwrap_or_default()
}

fn write_kind(ctl: &mut ContentControl, k: Kind) {
    let open = format!("<{KIND_EL} ");
    ctl.extra.retain(|x| !x.starts_with(&open));
    if k != Kind::Problem {
        ctl.extra.push(format!("<{KIND_EL} val=\"{}\"/>", k.name()));
    }
}

/// The settings of one question, kept on its answer control.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct Answer {
    /// Open answer (a space) or multiple choice (options).
    pub choice: bool,
    /// The question's maximum points.
    pub points: Option<f64>,
    /// The answer space's height, centimetres.
    pub height: f32,
    /// The space is squared (graph paper) rather than blank.
    pub grid: bool,
    /// The side of a square, millimetres.
    pub spacing: f32,
}

impl Default for Answer {
    fn default() -> Self {
        Answer { choice: false, points: None, height: DEFAULT_HEIGHT_CM, grid: true, spacing: DEFAULT_SPACING_MM }
    }
}

impl Answer {
    fn sanitized(self) -> Answer {
        let d = Answer::default();
        let fin = |v: f32, d: f32| if v.is_finite() { v } else { d };
        Answer {
            choice: self.choice,
            points: self.points.filter(|v| v.is_finite()).map(|v| v.clamp(0.0, MAX_POINTS)),
            height: fin(self.height, d.height).clamp(MIN_HEIGHT_CM, MAX_HEIGHT_CM),
            grid: self.grid,
            spacing: fin(self.spacing, d.spacing).clamp(1.0, 100.0),
        }
    }

    /// The graph paper block of an open answer: squares, or no lines (blank).
    fn graph(&self) -> GraphPaper {
        let mut g = GraphPaper {
            spacing: self.spacing * PT_PER_MM,
            placement: GridPlacement::Height { height: self.height * 10.0 * PT_PER_MM },
            ..GraphPaper::default()
        };
        if !self.grid {
            g.line.style = GridLineStyle::None;
        }
        g.sanitized()
    }
}

fn read_answer(ctl: &ContentControl) -> Answer {
    let open = format!("<{ANSWER_EL}>");
    let close = format!("</{ANSWER_EL}>");
    let Some(raw) = ctl.extra.iter().find_map(|x| x.strip_prefix(&open)?.strip_suffix(&close)) else { return Answer::default() };
    let json = raw.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&");
    serde_json::from_str::<Answer>(&json).unwrap_or_default().sanitized()
}

fn write_answer(ctl: &mut ContentControl, a: &Answer) {
    let open = format!("<{ANSWER_EL}>");
    ctl.extra.retain(|x| !x.starts_with(&open));
    let json = serde_json::to_string(a).unwrap_or_default();
    let escaped = json.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    ctl.extra.push(format!("{open}{escaped}</{ANSWER_EL}>"));
}

/// An inline control (`block` off).
fn inline(tag: &str, title: &str, lock: ControlLock) -> ContentControl {
    let mut c = control(tag, title, lock);
    c.block = false;
    c
}

fn check_box(correct: bool, checked: bool) -> ContentControl {
    let mut c = inline(TAG_BOX, "", ControlLock::SdtLocked);
    c.kind = ControlKind::CheckBox {
        checked,
        checked_char: CHECKED,
        checked_font: String::new(),
        unchecked_char: UNCHECKED,
        unchecked_font: String::new(),
    };
    if correct {
        c.extra.push(CORRECT_EL.into());
    }
    c
}

fn is_correct(c: &ContentControl) -> bool {
    c.extra.iter().any(|x| x == CORRECT_EL)
}

/// The points as the question shows them: ` (2 punti)`, ` (1 punto)`, ` (1,5 punti)`; nothing
/// without points.
fn points_text(points: Option<f64>, w: &super::Words, comma: bool) -> String {
    match points {
        Some(v) => {
            let word = if (v - 1.0).abs() < f64::EPSILON { w.point } else { w.points_word };
            format!(" ({} {word})", super::scores::shown(v, comma))
        }
        None => String::new(),
    }
}

/// Byte ranges of the first inline control tagged `tag` in `p`: its start marker and its end
/// marker. Nesting is counted, so a control inside it doesn't end it.
fn span(p: &Paragraph, tag: &str) -> Option<(usize, usize)> {
    let mut start = None;
    let mut depth = 0usize;
    for (at, o) in p.objects_with_offsets() {
        match o {
            InlineObject::ControlStart { control } => {
                if start.is_none() && control.tag == tag {
                    start = Some(at);
                    depth = 0;
                } else if start.is_some() {
                    depth += 1;
                }
            }
            InlineObject::ControlEnd if start.is_some() => {
                if depth == 0 {
                    return start.map(|a| (a, at));
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// The control whose start marker is at `at` in `p`.
fn control_at_mut(p: &mut Paragraph, at: usize) -> Option<&mut ContentControl> {
    match p.object_at_mut(at) {
        Some(InlineObject::ControlStart { control }) => Some(control),
        _ => None,
    }
}

/// Replaces the content of the control spanning `a..b` (markers) with `text`; whether it changed.
fn set_content(p: &mut Paragraph, (a, b): (usize, usize), text: &str) -> bool {
    let from = a + OBJ.len_utf8();
    if p.text.get(from..b) == Some(text) {
        return false;
    }
    let props = p.props_of_char(a).clone();
    if b > from && p.delete(from, b).is_err() {
        return false;
    }
    if !text.is_empty() && p.insert_text(from, text, &props).is_err() {
        return false;
    }
    p.touch();
    true
}

/// A question's text as the exercise reads it: the item control's content (empty while its
/// placeholder shows), without the points; `None` for a paragraph without an item control.
pub(super) fn question_text(p: &Paragraph) -> Option<String> {
    let (a, b) = span(p, TAG_ITEM)?;
    let showing = matches!(p.object_at(a), Some(InlineObject::ControlStart { control }) if control.showing_placeholder);
    if showing {
        return Some(String::new());
    }
    let t = p.text.get(a + OBJ.len_utf8()..b).unwrap_or_default();
    Some(t.chars().filter(|&c| c != OBJ).collect::<String>().trim().to_string())
}

/// One question of a quiz: its paragraph and the blocks of its answer.
#[derive(Debug, Clone)]
struct Item {
    question: usize,
    answer: Option<(usize, usize)>,
    settings: Answer,
}

/// The answer controls of the body: first and last top-level block, and the settings.
fn answers(s: &Session) -> Vec<(usize, usize, Answer)> {
    s.doc
        .control_ranges(StoryRef::Body)
        .into_iter()
        .filter(|r| r.control.tag == TAG_ANSWER)
        .filter_map(|r| Some((*r.start.path.0.first()? as usize, *r.end.path.0.first()? as usize, read_answer(&r.control))))
        .collect()
}

/// The questions of exercise `e` with their answers.
fn items(s: &Session, layout: &Layout, e: usize) -> Vec<Item> {
    let Some(ex) = layout.exercises.get(e) else { return Vec::new() };
    let Some((from, to)) = ex.question_blocks else { return Vec::new() };
    let all = answers(s);
    let mut out: Vec<Item> = Vec::new();
    for i in from..=to {
        let Some(Block::Para(p)) = s.doc.body.get(i).map(|b| &**b) else { continue };
        if p.props.style.as_deref() == Some(STYLE_QUESTION) && !in_answer(&all, i) {
            if out.len() >= MAX_QUESTIONS {
                break;
            }
            out.push(Item { question: i, answer: None, settings: Answer::default() });
        } else if let Some(&(a, b, set)) = all.iter().find(|(a, _, _)| *a == i)
            && let Some(last) = out.last_mut()
            && last.answer.is_none()
        {
            last.answer = Some((a, b));
            last.settings = set;
        }
    }
    out
}

fn in_answer(all: &[(usize, usize, Answer)], i: usize) -> bool {
    all.iter().any(|(a, b, _)| (*a..=*b).contains(&i))
}

/// The paragraphs of a new question: the question (the item control with `text` or its
/// placeholder, then the points) and its answer.
fn question_blocks(
    s: &mut Session,
    w: &super::Words,
    text: Option<&str>,
    set: Answer,
    choices: &[(String, bool)],
    num: Option<NumRef>,
) -> Vec<Paragraph> {
    let comma = super::scores::comma(s);
    let mut item = inline(TAG_ITEM, w.questions, ControlLock::SdtLocked);
    item.placeholder_text = w.question.into();
    item.showing_placeholder = text.is_none();
    let mut q = para(STYLE_QUESTION, "", Vec::new(), 0, num);
    let props = CharProps::default();
    let _ = q.insert_object(0, InlineObject::ControlStart { control: Box::new(item) }, &props);
    let mut at = q.text.len();
    let _ = q.insert_text(at, text.unwrap_or(w.question), &props);
    at = q.text.len();
    let _ = q.insert_object(at, InlineObject::ControlEnd, &props);
    at = q.text.len();
    let _ =
        q.insert_object(at, InlineObject::ControlStart { control: Box::new(inline(TAG_POINTS, w.points, ControlLock::SdtContentLocked)) }, &props);
    at = q.text.len();
    let _ = q.insert_text(at, &points_text(set.points, w, comma), &props);
    at = q.text.len();
    let _ = q.insert_object(at, InlineObject::ControlEnd, &props);
    let mut answer = control(TAG_ANSWER, w.answer, ControlLock::SdtLocked);
    write_answer(&mut answer, &set);
    let mut out = vec![q];
    if set.choice {
        let list = add_choice_list(s).ok();
        let n = choices.len().max(1);
        let mut opener = Some(answer);
        for k in 0..n {
            let (text, correct) = choices.get(k).cloned().unwrap_or_default();
            let mut p =
                para(STYLE_CHOICE, &text, opener.take().into_iter().collect(), usize::from(k + 1 == n), list.map(|num| NumRef { num, level: 0 }));
            let lead = if k == 0 { OBJ.len_utf8() } else { 0 };
            let _ = p.insert_object(lead, InlineObject::ControlStart { control: Box::new(check_box(correct, correct)) }, &props);
            let _ = p.insert_text(lead + OBJ.len_utf8(), &UNCHECKED.to_string(), &props);
            let end = lead + OBJ.len_utf8() + UNCHECKED.len_utf8();
            let _ = p.insert_object(end, InlineObject::ControlEnd, &props);
            let _ = p.insert_text(end + OBJ.len_utf8(), " ", &props);
            out.push(p);
        }
    } else {
        let mut p = para(STYLE_ANSWER, "", vec![answer], 1, None);
        p.props.graph = Some(Arc::new(set.graph()));
        out.push(p);
    }
    out
}

/// A list for one question's options: A), B), C)…
fn add_choice_list(s: &mut Session) -> Result<u32, CmdError> {
    use wordcraft_doc::numbering::{ListKind, levels_for};
    let mut levels = levels_for(ListKind::UpperLetter);
    levels.truncate(1);
    if let Some(l) = levels.first_mut() {
        l.text = "%1)".into();
        l.indent = 36.0;
        l.hanging = 18.0;
        l.tab = None;
    }
    Arc::make_mut(&mut s.doc.numbering).try_add_levels(Some("Exercise options".into()), levels).map_err(|e| CmdError::Failed(e.to_string()))
}

/// A question given to `exercise.insert` or `exercise.addQuestion`: its text, its settings and
/// its options with the correct ones.
struct NewQuestion {
    text: Option<String>,
    set: Answer,
    choices: Vec<(String, bool)>,
}

fn new_question(s: &Session, v: &Value, kind: Kind, true_false: bool) -> NewQuestion {
    let w = super::words(s);
    let mut set = Answer { choice: kind == Kind::Choice, ..Answer::default() };
    apply_settings(&mut set, v);
    let correct: Vec<usize> = correct_list(v);
    let mut choices: Vec<(String, bool)> = match v.get("choices").and_then(Value::as_array) {
        Some(a) => a.iter().take(MAX_CHOICES).map(|c| (c.as_str().map(clip_text).unwrap_or_default(), false)).collect(),
        None if true_false || p::bool(v, "trueFalse") == Some(true) => vec![(w.true_word.into(), false), (w.false_word.into(), false)],
        None => vec![(String::new(), false); 4],
    };
    for (k, c) in choices.iter_mut().enumerate() {
        c.1 = correct.contains(&k);
    }
    let text = match v {
        Value::String(t) => Some(t.clone()),
        _ => p::str(v, "text").map(str::to_string),
    }
    .map(|t| clip_text(&t))
    .filter(|t| !t.is_empty());
    NewQuestion { text, set: set.sanitized(), choices }
}

/// A question's text as stored: no control or object characters, at most `MAX_TEXT` characters.
fn clip_text(t: &str) -> String {
    t.chars().filter(|&c| !c.is_control() && c != OBJ).take(super::MAX_TEXT).collect::<String>().trim().to_string()
}

/// The correct options named by `correct`: 1-based numbers or letters (`"B"`), one or a list.
fn correct_list(v: &Value) -> Vec<usize> {
    let one = |c: &Value| -> Option<usize> {
        match c {
            Value::Number(n) => n.as_u64().filter(|&n| n >= 1).and_then(|n| usize::try_from(n - 1).ok()),
            Value::String(t) => {
                let t = t.trim().to_ascii_uppercase();
                let mut ch = t.chars();
                match (ch.next(), ch.next()) {
                    (Some(c), None) if c.is_ascii_uppercase() => Some(usize::from(c as u8 - b'A')),
                    _ => None,
                }
            }
            _ => None,
        }
    };
    match v.get("correct") {
        Some(Value::Array(a)) => a.iter().take(MAX_CHOICES).filter_map(one).collect(),
        Some(c) => one(c).into_iter().collect(),
        None => Vec::new(),
    }
}

/// `points`, `height` (cm), `grid`, `spacing` (mm) from `v` onto `set`.
fn apply_settings(set: &mut Answer, v: &Value) {
    match v.get("points") {
        Some(Value::Null) => set.points = None,
        Some(x) => {
            if let Some(n) = x.as_f64().or_else(|| x.as_str().and_then(|t| t.trim().replace(',', ".").parse().ok())) {
                set.points = Some(n);
            }
        }
        None => {}
    }
    if let Some(h) = p::f32(v, "height") {
        set.height = h;
    }
    if let Some(g) = p::bool(v, "grid") {
        set.grid = g;
    }
    if let Some(sp) = p::f32(v, "spacing") {
        set.spacing = sp;
    }
}

/// `exercise.insert` with a quiz kind: title, optional statement, then each question with its
/// answer. Returns the blocks to insert, in order.
pub(super) fn exercise_blocks(s: &mut Session, v: &Value, kind_name: &str) -> Result<Vec<Paragraph>, CmdError> {
    let kind = Kind::parse(kind_name).ok_or_else(|| CmdError::Params("kind: problem, open, choice or trueFalse".into()))?;
    let true_false = kind_name == "trueFalse";
    let w = super::words(s);
    let title = p::str(v, "title").map(str::to_string);
    let text = p::str(v, "text").map(str::to_string);
    let mut questions: Vec<NewQuestion> = v
        .get("questions")
        .and_then(Value::as_array)
        .map(|a| a.iter().take(MAX_QUESTIONS).map(|q| new_question(s, q, kind, true_false)).collect())
        .unwrap_or_default();
    if questions.is_empty() {
        questions.push(new_question(s, &json!({}), kind, true_false));
    }
    let mut exercise = control(TAG_EXERCISE, w.exercise, ControlLock::SdtLocked);
    write_kind(&mut exercise, kind);
    super::breaks::write(&mut exercise, super::breaks::first(s));
    let (shown, _) = super::show::first(s);
    super::show::write(&mut exercise, shown);
    let mut title_ctl = control(super::TAG_TITLE, w.title_part, ControlLock::SdtLocked);
    if title.is_none() {
        title_ctl.placeholder_text = w.title.into();
        title_ctl.showing_placeholder = true;
    }
    let mut blocks = vec![para(super::STYLE_TITLE, title.as_deref().unwrap_or(w.title), vec![exercise, title_ctl], 1, None)];
    // The statement only when asked for (a text, or `"statement": true` for its placeholder).
    if text.is_some() || p::bool(v, "statement") == Some(true) {
        let mut st = control(super::TAG_TEXT, w.statement, ControlLock::SdtLocked);
        if text.is_none() {
            st.placeholder_text = w.text.into();
            st.showing_placeholder = true;
        }
        blocks.push(para("Normal", text.as_deref().unwrap_or(w.text), vec![st], 1, None));
    }
    let list = super::add_question_list(s)?;
    let lone = questions.len() == 1;
    let n = questions.len();
    for (k, q) in questions.iter().enumerate() {
        let num = Some(NumRef { num: if lone { 0 } else { list }, level: 0 });
        let mut new = question_blocks(s, w, q.text.as_deref(), q.set, &q.choices, num);
        if k == 0
            && let Some(first) = new.first_mut()
        {
            let _ = first.insert_object(
                0,
                InlineObject::ControlStart { control: Box::new(control(TAG_QUESTIONS, w.questions, ControlLock::SdtLocked)) },
                &CharProps::default(),
            );
        }
        if k + 1 == n
            && let Some(last) = new.last_mut()
        {
            // The questions and the exercise close on the last answer.
            for _ in 0..2 {
                let end = last.text.len();
                let _ = last.insert_object(end, InlineObject::ControlEnd, &CharProps::default());
            }
        }
        blocks.extend(new);
    }
    Ok(blocks)
}

/// `exercise.addQuestion` in a quiz: a question and its answer after the last answer.
pub(super) fn add_question(s: &mut Session, ex: usize, v: &Value) -> CmdResult {
    let layout = scan(s);
    let e = layout.exercises.get(ex).ok_or_else(|| CmdError::Failed("exercise vanished".into()))?;
    let (_, last) = e.question_blocks.ok_or_else(|| CmdError::Failed("the exercise has no questions part".into()))?;
    let its = items(s, &layout, ex);
    if its.len() >= MAX_QUESTIONS {
        return Err(CmdError::Failed(format!("an exercise holds at most {MAX_QUESTIONS} questions")));
    }
    // A new question is like the last one (its kind of answer, points, space).
    let like = its.last().map(|i| i.settings).unwrap_or(Answer { choice: e.kind == Kind::Choice, ..Answer::default() });
    let mut base = json!({"points": like.points, "height": like.height, "grid": like.grid, "spacing": like.spacing});
    if let (Some(b), Some(o)) = (base.as_object_mut(), v.as_object()) {
        for (k, x) in o {
            b.insert(k.clone(), x.clone());
        }
    }
    let kind = if like.choice { Kind::Choice } else { Kind::Open };
    let q = new_question(s, &base, kind, false);
    let w = super::words(s);
    let num = super::e_list(s, last).map(|num| NumRef { num, level: 0 });
    let new = question_blocks(s, w, q.text.as_deref(), q.set, &q.choices, num);
    // The questions control's end (and the exercise's) move from the last paragraph to the new
    // last one: as many end markers as the old last paragraph ends with past its own controls.
    let block = s.doc.body.get_mut(last).ok_or_else(|| CmdError::Failed("no questions".into()))?;
    let Block::Para(prev) = Arc::make_mut(block) else { return Err(CmdError::Failed("the questions end in a table".into())) };
    let mut moved = 0;
    for _ in 0..2 {
        // Only ends of controls that opened before this paragraph: the questions and the exercise.
        if trailing_outer_ends(prev) > 0 {
            super::take_last_end(prev)?;
            moved += 1;
        }
    }
    let n = new.len();
    for (k, mut p) in new.into_iter().enumerate() {
        if k + 1 == n {
            for _ in 0..moved {
                let end = p.text.len();
                let _ = p.insert_object(end, InlineObject::ControlEnd, &CharProps::default());
            }
        }
        s.doc.body.insert(last + 1 + k, para_block(p));
    }
    // The caret in the new question's placeholder.
    s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(StoryRef::Body, super::body_path(last + 1), OBJ.len_utf8()));
    s.touch();
    super::update(s);
    s.clamp_selection();
    super::super::sel_result(s)
}

/// End markers at the end of `p` that close controls opened in an earlier paragraph.
fn trailing_outer_ends(p: &Paragraph) -> usize {
    let mut open = 0usize;
    let mut outer = 0usize;
    for (_, o) in p.objects_with_offsets() {
        match o {
            InlineObject::ControlStart { .. } => open += 1,
            InlineObject::ControlEnd if open > 0 => open -= 1,
            InlineObject::ControlEnd => outer += 1,
            _ => {}
        }
    }
    // Only those at the very end count (they are what the paragraph closes last).
    let tail = p.text.chars().rev().take_while(|&c| c == OBJ).count();
    outer.min(tail)
}

/// Everything a quiz shows brought in line with its settings: the points after each question,
/// the space of each open answer, the boxes of each option, the answers shown or hidden, a lone
/// question without its letter. Whether anything changed.
pub(super) fn update(s: &mut Session) -> bool {
    let layout = scan(s);
    let w = super::doc_words(s);
    let comma = super::scores::comma(s);
    let mut changed = false;
    for (e, ex) in layout.exercises.iter().enumerate() {
        let its = items(s, &layout, e);
        if its.is_empty() || !(ex.kind.quiz() || its.iter().any(|i| i.answer.is_some())) {
            continue;
        }
        let shown = ex.show.solution;
        // Letters: none for a lone question; the exercise's list for several.
        let lone = its.len() == 1;
        let list = if lone {
            None
        } else {
            let found = its.iter().find_map(|i| {
                s.doc.body.get(i.question).and_then(|b| b.as_para()).and_then(|p| p.props.numbering).map(|n| n.num).filter(|&n| n != 0)
            });
            match found {
                Some(n) => Some(n),
                None => super::add_question_list(s).ok(),
            }
        };
        for it in &its {
            if let Some(block) = s.doc.body.get_mut(it.question) {
                let want_num = if lone { Some(NumRef { num: 0, level: 0 }) } else { None };
                let Block::Para(p) = &**block else { continue };
                let cur = p.props.numbering;
                let new_num = match (want_num, cur, list) {
                    (Some(w0), c, _) if c != Some(w0) => Some(Some(w0)),
                    (None, Some(c), Some(l)) if c.num == 0 => Some(Some(NumRef { num: l, level: c.level })),
                    (None, None, Some(l)) => Some(Some(NumRef { num: l, level: 0 })),
                    _ => None,
                };
                let text = points_text(it.settings.points, w, comma);
                let needs_points = span(p, TAG_POINTS).is_some_and(|sp| p.text.get(sp.0 + OBJ.len_utf8()..sp.1) != Some(text.as_str()));
                if new_num.is_some() || needs_points {
                    let Block::Para(p) = Arc::make_mut(block) else { continue };
                    if let Some(n) = new_num {
                        p.props.numbering = n;
                    }
                    if let Some(sp) = span(p, TAG_POINTS) {
                        set_content(p, sp, &text);
                    }
                    p.touch();
                    changed = true;
                }
            }
            let Some((a, b)) = it.answer else { continue };
            for i in a..=b {
                let Some(block) = s.doc.body.get_mut(i) else { continue };
                changed |= if it.settings.choice { choice_para(block, shown) } else { open_para(block, &it.settings, i == a, shown) };
            }
        }
    }
    if changed {
        s.touch();
        s.clamp_selection();
    }
    changed
}

/// One paragraph of an open answer: the first holds the space (its graph paper as the settings
/// say), the text is hidden while the answers are; whether it changed.
fn open_para(block: &mut Arc<Block>, set: &Answer, first: bool, shown: bool) -> bool {
    let Block::Para(p) = &**block else { return false };
    let graph = first.then(|| Arc::new(set.graph()));
    let want_hidden = (!shown).then_some(true);
    let same_graph = p.props.graph.as_deref() == graph.as_deref();
    let same_hidden = p.runs.iter().all(|r| r.props.hidden == want_hidden) && p.mark.hidden.is_none();
    if same_graph && same_hidden {
        return false;
    }
    let Block::Para(p) = Arc::make_mut(block) else { return false };
    p.props.graph = graph;
    for r in &mut p.runs {
        r.props.hidden = want_hidden;
    }
    // The mark stays: the space keeps its height with its answer hidden.
    p.mark.hidden = None;
    p.touch();
    true
}

/// One option: a check box at its start, ticked when it is correct and the answers show. A box
/// ticked or cleared by hand while they show marks the option correct or not.
fn choice_para(block: &mut Arc<Block>, shown: bool) -> bool {
    let Block::Para(p) = &**block else { return false };
    if p.props.style.as_deref() != Some(STYLE_CHOICE) {
        return false;
    }
    // After the answer control's start (on the first option).
    let mut lead = 0;
    for (at, o) in p.objects_with_offsets() {
        if at != lead || !matches!(o, InlineObject::ControlStart { control } if control.tag != TAG_BOX) {
            break;
        }
        lead = at + OBJ.len_utf8();
    }
    let Some((a, b)) = span(p, TAG_BOX) else {
        // An option without a box (a new one made with Enter): it gets one, empty.
        let Block::Para(p) = Arc::make_mut(block) else { return false };
        let props = p.props_of_char(lead).clone();
        if p.insert_object(lead, InlineObject::ControlStart { control: Box::new(check_box(false, false)) }, &props).is_err() {
            return false;
        }
        let _ = p.insert_text(lead + OBJ.len_utf8(), &UNCHECKED.to_string(), &props);
        let end = lead + OBJ.len_utf8() + UNCHECKED.len_utf8();
        let _ = p.insert_object(end, InlineObject::ControlEnd, &props);
        if !p.text.get(end + OBJ.len_utf8()..).is_some_and(|t| t.starts_with(' ')) {
            let _ = p.insert_text(end + OBJ.len_utf8(), " ", &props);
        }
        p.touch();
        return true;
    };
    let Some(InlineObject::ControlStart { control }) = p.object_at(a) else { return false };
    let checked = matches!(control.kind, ControlKind::CheckBox { checked: true, .. });
    let correct = is_correct(control);
    let was_shown = control.extra.iter().any(|x| x == SHOWN_EL);
    // Shown since the last update: the box is the truth (a click on it changes the answer);
    // just shown again, the mark; hidden: always empty.
    let correct_now = if shown && was_shown { checked } else { correct };
    let checked_now = shown && correct_now;
    let symbol = if checked_now { CHECKED } else { UNCHECKED }.to_string();
    if correct_now == correct && checked_now == checked && was_shown == shown && p.text.get(a + OBJ.len_utf8()..b) == Some(symbol.as_str()) {
        return false;
    }
    let Block::Para(p) = Arc::make_mut(block) else { return false };
    if let Some(c) = control_at_mut(p, a) {
        c.extra.retain(|x| x != CORRECT_EL && x != SHOWN_EL);
        if correct_now {
            c.extra.push(CORRECT_EL.into());
        }
        if shown {
            c.extra.push(SHOWN_EL.into());
        }
        if let ControlKind::CheckBox { checked, .. } = &mut c.kind {
            *checked = checked_now;
        }
    }
    set_content(p, (a, b), &symbol);
    p.touch();
    true
}

/// The text of an option, without its box.
fn choice_text(p: &Paragraph) -> String {
    let after = span(p, TAG_BOX).map_or(0, |(_, b)| b + OBJ.len_utf8());
    p.text.get(after..).unwrap_or_default().chars().filter(|&c| c != OBJ).collect::<String>().trim().to_string()
}

/// The options of an answer with whether each is correct.
fn choices(s: &Session, (a, b): (usize, usize)) -> Vec<(String, bool)> {
    (a..=b)
        .filter_map(|i| s.doc.body.get(i).and_then(|b| b.as_para()))
        .filter(|p| p.props.style.as_deref() == Some(STYLE_CHOICE))
        .take(MAX_CHOICES)
        .map(|p| {
            let correct = span(p, TAG_BOX)
                .is_some_and(|(at, _)| matches!(p.object_at(at), Some(InlineObject::ControlStart { control }) if is_correct(control)));
            (choice_text(p), correct)
        })
        .collect()
}

fn letter(k: usize) -> String {
    super::letter(k).to_ascii_uppercase()
}

/// The exercise's maximum points: its questions' added up (`None` when none has points).
pub(crate) fn total(s: &Session, layout: &Layout, e: usize) -> Option<f64> {
    let pts: Vec<f64> = items(s, layout, e).iter().filter_map(|i| i.settings.points).collect();
    (!pts.is_empty()).then(|| pts.iter().sum()).filter(|v: &f64| v.is_finite())
}

/// What a quiz gives the results summary: for each multiple-choice question, its letter and the
/// correct options (`b) A, C`); an open answer gives nothing.
pub(super) fn summary_lines(s: &Session, layout: &Layout, e: usize) -> Vec<String> {
    let its = items(s, layout, e);
    let labels: Vec<String> = layout.exercises.get(e).map(|x| x.questions.iter().map(|q| q.label.clone()).collect()).unwrap_or_default();
    let lone = its.len() == 1;
    its.iter()
        .enumerate()
        .filter(|(_, it)| it.settings.choice)
        .filter_map(|(k, it)| {
            let right: Vec<String> = choices(s, it.answer?).iter().enumerate().filter(|(_, c)| c.1).map(|(j, _)| letter(j)).collect();
            let right = if right.is_empty() { "—".to_string() } else { right.join(", ") };
            Some(if lone { right } else { format!("{} {right}", labels.get(k).cloned().unwrap_or_default()) })
        })
        .collect()
}

/// A hash of what the quiz parts show, so [`super::update`] runs when a box is clicked or a
/// setting changes.
pub(super) fn fingerprint(s: &Session) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for r in s.doc.control_ranges(StoryRef::Body) {
        if r.control.tag == TAG_BOX {
            (matches!(r.control.kind, ControlKind::CheckBox { checked: true, .. }), is_correct(&r.control), &r.start.path.0).hash(&mut h);
        } else if r.control.tag == TAG_ANSWER {
            let a = read_answer(&r.control);
            (a.choice, a.points.map(f64::to_bits), a.height.to_bits(), a.grid, a.spacing.to_bits(), &r.start.path.0, &r.end.path.0).hash(&mut h);
        }
    }
    h.finish()
}

/// The quiz questions of an exercise as `exercise.questions` reports them.
fn state(s: &Session, e: usize) -> Value {
    let layout = scan(s);
    let Some(ex) = layout.exercises.get(e) else { return Value::Null };
    let its = items(s, &layout, e);
    let lone = its.len() == 1;
    let list: Vec<Value> = its
        .iter()
        .enumerate()
        .map(|(k, it)| {
            let p = s.doc.body.get(it.question).and_then(|b| b.as_para());
            let text = p.and_then(question_text).or_else(|| p.map(super::plain)).unwrap_or_default();
            let label = if lone { String::new() } else { ex.questions.get(k).map(|q| q.label.clone()).unwrap_or_default() };
            let mut v = json!({
                "label": label,
                "text": text,
                "type": if it.settings.choice { "choice" } else { "open" },
                "points": it.settings.points,
            });
            if let Some(o) = v.as_object_mut() {
                if it.settings.choice {
                    let cs = it.answer.map(|r| choices(s, r)).unwrap_or_default();
                    o.insert("choices".into(), json!(cs.iter().map(|c| c.0.clone()).collect::<Vec<_>>()));
                    o.insert("correct".into(), json!(cs.iter().enumerate().filter(|c| c.1.1).map(|(j, _)| letter(j)).collect::<Vec<_>>()));
                } else {
                    o.insert("height".into(), json!((f64::from(it.settings.height) * 100.0).round() / 100.0));
                    o.insert("grid".into(), json!(it.settings.grid));
                    o.insert("spacing".into(), json!((f64::from(it.settings.spacing) * 100.0).round() / 100.0));
                }
            }
            v
        })
        .collect();
    json!({"exercise": e + 1, "kind": ex.kind.name(), "questions": list, "total": total(s, &layout, e)})
}

/// The question at the caret (0-based) in exercise `e`, by the paragraph it is in or the answer.
fn question_at_caret(s: &Session, layout: &Layout, e: usize) -> Option<usize> {
    if s.sel.focus.story != StoryRef::Body {
        return None;
    }
    let top = *s.sel.focus.path.0.first()? as usize;
    items(s, layout, e).iter().rposition(|it| it.question <= top)
}

/// `exercise.questions`: reads a quiz's questions, or changes one's settings (points, space,
/// correct options).
pub(crate) fn command(s: &mut Session, v: &Value) -> CmdResult {
    let e = super::target(s, v)?;
    let layout = scan(s);
    if layout.exercises.get(e).is_none() {
        return Err(CmdError::Params("no such exercise".into()));
    }
    let changes = ["points", "height", "grid", "spacing", "correct"].iter().any(|k| v.get(*k).is_some());
    if !changes {
        return Ok(state(s, e));
    }
    let its = items(s, &layout, e);
    let q = match p::u64(v, "question") {
        Some(n) if n >= 1 => usize::try_from(n - 1).unwrap_or(usize::MAX),
        Some(_) => return Err(CmdError::Params("question: 1-based".into())),
        None => question_at_caret(s, &layout, e).ok_or_else(|| CmdError::Params("question: which one (1-based)".into()))?,
    };
    let it = its.get(q).ok_or_else(|| CmdError::Params("no such question".into()))?;
    let (a, b) =
        it.answer.ok_or_else(|| CmdError::Disabled("this question has no answer part (a problem's question is answered in the solution)".into()))?;
    let mut set = it.settings;
    apply_settings(&mut set, v);
    let set = set.sanitized();
    let block = s.doc.body.get_mut(a).ok_or_else(|| CmdError::Failed("answer vanished".into()))?;
    let Block::Para(p) = Arc::make_mut(block) else { return Err(CmdError::Failed("answer vanished".into())) };
    let at = span_block(p, TAG_ANSWER).ok_or_else(|| CmdError::Failed("answer vanished".into()))?;
    if let Some(c) = control_at_mut(p, at) {
        write_answer(c, &set);
    }
    p.touch();
    if set.choice && v.get("correct").is_some() {
        let want = correct_list(v);
        let mut k = 0usize;
        for i in a..=b {
            let Some(block) = s.doc.body.get_mut(i) else { continue };
            let Block::Para(p) = &**block else { continue };
            if p.props.style.as_deref() != Some(STYLE_CHOICE) {
                continue;
            }
            let Block::Para(p) = Arc::make_mut(block) else { continue };
            if let Some((at, _)) = span(p, TAG_BOX)
                && let Some(c) = control_at_mut(p, at)
            {
                c.extra.retain(|x| x != CORRECT_EL);
                let on = want.contains(&k);
                if on {
                    c.extra.push(CORRECT_EL.into());
                }
                // So `update` takes the mark, not the box, as the truth.
                if let ControlKind::CheckBox { checked, .. } = &mut c.kind {
                    *checked = on;
                }
                p.touch();
            }
            k += 1;
        }
    }
    s.touch();
    super::update(s);
    s.clamp_selection();
    Ok(state(s, e))
}

/// Offset of the start marker of the block control tagged `tag` in `p`.
fn span_block(p: &Paragraph, tag: &str) -> Option<usize> {
    p.objects_with_offsets().find_map(|(at, o)| match o {
        InlineObject::ControlStart { control } if control.tag == tag => Some(at),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wordcraft_doc::Document;

    fn run(s: &mut Session, id: &str, v: Value) -> Value {
        s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
    }

    fn session() -> Session {
        let mut s = Session::new(Document::from_text(""));
        Arc::make_mut(&mut s.doc.styles).default_chr.lang = Some("it-IT".into());
        s
    }

    fn texts(s: &Session) -> Vec<String> {
        s.doc.body.iter().filter_map(|b| b.as_para()).map(|p| p.text.chars().filter(|&c| c != OBJ).collect()).collect()
    }

    #[test]
    fn an_open_answer_exercise_is_its_question_and_a_space() {
        let mut s = session();
        run(
            &mut s,
            "exercise.insert",
            json!({"kind": "open", "questions": [{"text": "Definisci il momento flettente.", "points": 2, "height": 3, "grid": false}]}),
        );
        let t = texts(&s);
        assert!(t.iter().any(|x| x == "Definisci il momento flettente. (2 punti)"), "{t:?}");
        // No statement, no solution, no results summary.
        assert!(!t.iter().any(|x| x.contains("Scrivi qui il testo")), "{t:?}");
        assert!(!t.iter().any(|x| x.contains("Risoluzione") || x.contains("Riepilogo")), "{t:?}");
        let space = s.doc.body.iter().filter_map(|b| b.as_para()).find(|p| p.props.graph.is_some()).expect("space");
        let g = space.props.graph.as_deref().expect("graph");
        assert_eq!(g.line.style, GridLineStyle::None);
        assert!(matches!(g.placement, GridPlacement::Height { height } if (height - 30.0 * PT_PER_MM).abs() < 0.5));
        // A lone question has no letter.
        let q = s.doc.body.iter().filter_map(|b| b.as_para()).find(|p| p.props.style.as_deref() == Some(STYLE_QUESTION)).expect("question");
        assert_eq!(q.props.numbering.map(|n| n.num), Some(0));
        let st = run(&mut s, "exercise.questions", json!({}));
        assert_eq!(st["kind"], "open");
        assert_eq!(st["questions"][0]["text"], "Definisci il momento flettente.");
        assert_eq!(st["questions"][0]["height"], 3.0);
        assert_eq!(st["total"], 2.0);
    }

    #[test]
    fn the_answer_hides_and_the_space_stays() {
        let mut s = session();
        run(&mut s, "exercise.insert", json!({"kind": "open", "questions": ["Che cos'è una trave?"]}));
        let i = s.doc.body.iter().position(|b| b.as_para().is_some_and(|p| p.props.graph.is_some())).expect("space");
        s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(StoryRef::Body, super::super::body_path(i), OBJ.len_utf8()));
        run(&mut s, "text.insert", json!({"text": "Un elemento strutturale."}));
        run(&mut s, "exercise.show", json!({"solution": false}));
        let p = s.doc.body.get(i).and_then(|b| b.as_para()).expect("space");
        assert!(p.props.graph.is_some());
        assert!(p.text.contains("Un elemento strutturale."));
        assert!(p.runs.iter().all(|r| r.props.hidden == Some(true)));
        assert_eq!(p.mark.hidden, None);
        run(&mut s, "exercise.show", json!({"solution": true}));
        let p = s.doc.body.get(i).and_then(|b| b.as_para()).expect("space");
        assert!(p.runs.iter().all(|r| r.props.hidden.is_none()));
    }

    #[test]
    fn multiple_choice_ticks_the_correct_options_and_the_summary_lists_them() {
        let mut s = session();
        run(
            &mut s,
            "exercise.insert",
            json!({"kind": "choice", "questions": [
                {"text": "Quali sono vincoli?", "choices": ["Cerniera", "Forza", "Incastro"], "correct": ["A", "C"], "points": 1},
                {"text": "L'acciaio è duttile?", "choices": ["Sì", "No"], "correct": 1, "points": 0.5}
            ]}),
        );
        let t = texts(&s);
        assert!(t.iter().any(|x| x == "☒ Cerniera"), "{t:?}");
        assert!(t.iter().any(|x| x == "☐ Forza"), "{t:?}");
        assert!(t.iter().any(|x| x == "Quali sono vincoli? (1 punto)"), "{t:?}");
        assert!(t.iter().any(|x| x == "L'acciaio è duttile? (0,5 punti)"), "{t:?}");
        assert!(t.iter().any(|x| x == "a) A, C"), "{t:?}");
        assert!(t.iter().any(|x| x == "b) A"), "{t:?}");
        // The student's copy: every box empty, the marks kept.
        run(&mut s, "exercise.show", json!({"solution": false}));
        assert!(!texts(&s).iter().any(|x| x.contains('☒')), "{:?}", texts(&s));
        run(&mut s, "exercise.show", json!({"solution": true}));
        assert!(texts(&s).iter().any(|x| x == "☒ Incastro"));
        // A click on a box changes the answer: Forza ticked.
        let forza = s.doc.control_ranges(StoryRef::Body).into_iter().filter(|r| r.control.tag == TAG_BOX).nth(1).expect("box");
        run(&mut s, "developer.toggleCheckBox", json!({"at": serde_json::to_value(forza.content_start()).expect("pos")}));
        let st = run(&mut s, "exercise.questions", json!({"question": 1}));
        assert_eq!(st["questions"][0]["correct"], json!(["A", "B", "C"]), "{st}");
        let st = run(&mut s, "exercise.questions", json!({"question": 1, "correct": ["B"]}));
        assert_eq!(st["questions"][0]["correct"], json!(["B"]));
        assert_eq!(st["total"], 1.5);
    }

    #[test]
    fn true_false_and_added_questions_follow_the_last_one() {
        let mut s = session();
        run(&mut s, "exercise.insert", json!({"kind": "trueFalse", "questions": [{"text": "Il ferro è un metallo.", "correct": 1, "points": 1}]}));
        assert!(texts(&s).iter().any(|x| x == "☒ Vero"), "{:?}", texts(&s));
        run(&mut s, "exercise.addQuestion", json!({"text": "L'aria è un solido.", "trueFalse": true, "correct": 2}));
        let st = run(&mut s, "exercise.questions", json!({}));
        assert_eq!(st["questions"].as_array().map(Vec::len), Some(2));
        assert_eq!(st["questions"][1]["correct"], json!(["B"]));
        assert_eq!(st["questions"][1]["points"], 1.0);
        assert_eq!(st["questions"][0]["label"], "a)");
        // Two questions: both lettered by one list.
        let nums: Vec<u32> = s
            .doc
            .body
            .iter()
            .filter_map(|b| b.as_para())
            .filter(|p| p.props.style.as_deref() == Some(STYLE_QUESTION))
            .filter_map(|p| p.props.numbering.map(|n| n.num))
            .collect();
        assert_eq!(nums.len(), 2);
        assert!(nums.iter().all(|&n| n != 0 && n == nums[0]), "{nums:?}");
    }

    #[test]
    fn quiz_points_fill_the_scores_table() {
        let mut s = session();
        run(&mut s, "exercise.insert", json!({"kind": "open", "questions": [{"text": "Uno", "points": 2}, {"text": "Due", "points": 3}]}));
        run(&mut s, "exercise.insert", json!({"kind": "choice", "questions": [{"text": "Tre", "points": 1}]}));
        s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(StoryRef::Body, super::super::body_path(0), 0));
        run(&mut s, "exercise.scores", json!({"on": true}));
        let st = run(&mut s, "exercise.scores", json!({}));
        assert_eq!(st["points"], json!([[5.0], [1.0]]), "{st}");
    }

    #[test]
    fn a_statement_only_when_asked() {
        let mut s = session();
        run(&mut s, "exercise.insert", json!({"kind": "open", "text": "Leggi il brano.", "questions": ["Di cosa parla?"]}));
        assert!(texts(&s).iter().any(|x| x == "Leggi il brano."));
    }

    #[test]
    fn hostile_params_never_panic() {
        let mut s = session();
        run(&mut s, "exercise.insert", json!({"kind": "choice"}));
        for v in [
            json!({"points": -5}),
            json!({"points": "x"}),
            json!({"height": 1e30}),
            json!({"height": -1}),
            json!({"spacing": f64::MAX}),
            json!({"correct": [0, 99, "ZZ", null]}),
            json!({"question": 0, "points": 1}),
            json!({"question": 999, "points": 1}),
            json!({"exercise": 99}),
            json!({"type": 3}),
        ] {
            let _ = s.run("exercise.questions", &v);
        }
        for v in [
            json!({"kind": "nope"}),
            json!({"kind": "open", "questions": [1, null, {"height": "x"}]}),
            json!({"kind": "choice", "questions": [{"choices": [1, 2]}]}),
        ] {
            let _ = s.run("exercise.insert", &v);
        }
        let _ = s.run("exercise.addQuestion", &json!({"choices": "x", "correct": {}}));
    }
}
