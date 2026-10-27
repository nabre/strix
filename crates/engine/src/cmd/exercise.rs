//! Exercises (docs/exercise.md): a rigid structure for a worked problem, made of locked block
//! content controls so it can't be broken by editing:
//!
//! - the **title** (style Exercise Title), in the exercise control `strix:exercise`;
//! - the **statement** (`strix:exercise.text`): free content, laid out with every tool;
//! - the **questions** (`strix:exercise.questions`): one paragraph each, lettered a), b)…;
//! - the **solution** (`strix:exercise.solution`): a heading, the questions repeated
//!   automatically (`strix:exercise.prompt`, read-only), then free content; an equation in it is
//!   marked as the result of a question (`Math::answer`, `equation.answer`);
//! - one **results summary** (`strix:results`, read-only) at the end of the document, listing
//!   every exercise's questions with the value of their result equations.
//!
//! The automatic parts are rebuilt after every command that changes what they show
//! ([`update`]), like calculations; nothing else is touched.

use std::sync::Arc;

use serde_json::{Value, json};
use strix_doc::control::{ContentControl, ControlKind, ControlLock};
use strix_doc::numbering::ListKind;
use strix_doc::para::OBJ;
use strix_doc::props::{CharProps, NumRef, ParaProps};
use strix_doc::styles::{Style, StyleKind};
use strix_doc::{Block, InlineObject, Paragraph, Run, StoryRef, para_block};

use crate::{CmdError, CmdResult, CommandSpec, Session, p};

pub const TAG_EXERCISE: &str = "strix:exercise";
pub const TAG_TEXT: &str = "strix:exercise.text";
pub const TAG_QUESTIONS: &str = "strix:exercise.questions";
pub const TAG_SOLUTION: &str = "strix:exercise.solution";
pub const TAG_PROMPT: &str = "strix:exercise.prompt";
pub const TAG_RESULTS: &str = "strix:results";

/// Most questions in one exercise; most exercises summarised.
const MAX_QUESTIONS: usize = 100;
const MAX_EXERCISES: usize = 1000;
/// Longest question or title text kept in the generated parts, in characters.
const MAX_TEXT: usize = 2000;

const STYLE_TITLE: &str = "ExerciseTitle";
const STYLE_QUESTION: &str = "ExerciseQuestion";
const STYLE_HEADING: &str = "ExerciseHeading";
const STYLE_PROMPT: &str = "ExercisePrompt";
const STYLE_RESULT: &str = "ExerciseResult";

pub fn specs() -> Vec<CommandSpec> {
    let body = |s: &Session| {
        if s.sel.focus.story == StoryRef::Body && s.sel.focus.path.cell().is_none() {
            None
        } else {
            Some("an exercise goes in the body, outside tables")
        }
    };
    vec![
        CommandSpec::new("exercise.insert", "New Exercise", "Math › Exercise", insert)
            .when(body)
            .params(r#"{"title"?: string, "text"?: string, "questions"?: [string]} → inserts an exercise after the paragraph at the caret: title, statement, questions, solution; the results summary goes (or stays) at the end of the document"#),
        CommandSpec::new("exercise.addQuestion", "Add Question", "Math › Exercise", add_question)
            .when(|s| if exercise_at_caret(s).is_some() { None } else { Some("the cursor isn't in an exercise") })
            .params(r#"{"text"?: string} → adds a question at the end of the exercise at the caret"#),
        CommandSpec::new("equation.answer", "Result Of", "Math › Exercise", answer)
            .when(|s| if super::equation::equation_at_caret(s).is_some() { None } else { Some("no equation at the caret") })
            .params(r#"{"question": n (1-based; 0 = none)} → the equation at the caret is the result of that question of its exercise"#),
        CommandSpec::new("exercise.info", "Exercise Info", "Math › Exercise", info)
            .pure()
            .params(r#"→ {"exercise": n|null, "title", "questions": [string], "answer": n (of the equation at the caret, 0 none), "exercises": n}"#),
        CommandSpec::new("exercise.update", "Update Summary", "Math › Exercise", |s, _| {
            let changed = update(s);
            Ok(json!({"changed": changed}))
        })
        .params(r#"→ rebuilds the questions repeated in each solution and the results summary"#),
    ]
}

/// Words of the generated parts, in the document's language (Italian or English).
struct Words {
    exercise: &'static str,
    solution: &'static str,
    results: &'static str,
    title: &'static str,
    text: &'static str,
    question: &'static str,
    no_result: &'static str,
}

fn words(s: &Session) -> Words {
    let lang = s.prefs.default_language.clone().or_else(|| s.doc.styles.default_chr.lang.clone()).unwrap_or_default();
    if lang.to_ascii_lowercase().starts_with("it") {
        Words {
            exercise: "Esercizio",
            solution: "Risoluzione",
            results: "Riepilogo dei risultati",
            title: "Titolo dell'esercizio",
            text: "Scrivi qui il testo dell'esercizio: dati, figure, tabelle.",
            question: "Scrivi qui la domanda.",
            no_result: "risultato non indicato",
        }
    } else {
        Words {
            exercise: "Exercise",
            solution: "Solution",
            results: "Results",
            title: "Exercise title",
            text: "Write the exercise here: data, figures, tables.",
            question: "Write the question here.",
            no_result: "no result marked",
        }
    }
}

/// Adds the exercise styles the document lacks.
fn ensure_styles(s: &mut Session) {
    let para_style = |id: &str, name: &str, para: ParaProps, chr: CharProps| Style {
        id: id.into(),
        name: name.into(),
        kind: StyleKind::Paragraph,
        based_on: Some("Normal".into()),
        next: Some("Normal".into()),
        para,
        chr,
        priority: Some(12),
        quick: true,
        ..Default::default()
    };
    let wanted = [
        para_style(
            STYLE_TITLE,
            "Exercise Title",
            ParaProps { space_before: Some(18.0), space_after: Some(6.0), keep_next: Some(true), outline_level: Some(0), ..Default::default() },
            CharProps { bold: Some(true), size: Some(16.0), ..Default::default() },
        ),
        para_style(STYLE_QUESTION, "Exercise Question", ParaProps { space_after: Some(3.0), ..Default::default() }, CharProps::default()),
        para_style(
            STYLE_HEADING,
            "Exercise Heading",
            ParaProps { space_before: Some(12.0), space_after: Some(4.0), keep_next: Some(true), ..Default::default() },
            CharProps { bold: Some(true), size: Some(12.0), ..Default::default() },
        ),
        para_style(
            STYLE_PROMPT,
            "Exercise Prompt",
            ParaProps { space_after: Some(2.0), ..Default::default() },
            // Generated text is not proofed (a lone "a)" is no spelling error).
            CharProps { italic: Some(true), no_proof: Some(true), ..Default::default() },
        ),
        para_style(
            STYLE_RESULT,
            "Exercise Result",
            ParaProps { space_after: Some(2.0), ..Default::default() },
            CharProps { no_proof: Some(true), ..Default::default() },
        ),
    ];
    for st in wanted {
        if s.doc.styles.get(&st.id).is_none() {
            s.doc.styles.styles.push(st);
        }
    }
}

fn control(tag: &str, title: &str, lock: ControlLock) -> ContentControl {
    let mut c = ContentControl::new(ControlKind::RichText);
    c.tag = tag.into();
    c.title = title.into();
    c.lock = lock;
    c.block = true;
    c
}

/// A paragraph: `starts` controls opening at its start, `text`, then `ends` controls closing.
fn para(style: &str, text: &str, starts: Vec<ContentControl>, ends: usize, numbering: Option<NumRef>) -> Paragraph {
    let mut p = Paragraph::new();
    let mut t = String::new();
    for c in starts {
        t.push(OBJ);
        p.objects.push(InlineObject::ControlStart { control: Box::new(c) });
    }
    t.extend(text.chars().filter(|&c| c != OBJ && c != '\n').take(MAX_TEXT));
    for _ in 0..ends {
        t.push(OBJ);
        p.objects.push(InlineObject::ControlEnd);
    }
    if !t.is_empty() {
        p.runs.push(Run { len: t.len(), props: CharProps::default() });
    }
    p.text = t;
    p.props.style = Some(style.into());
    p.props.numbering = numbering;
    p
}

/// The path of top-level body block `i`.
fn body_path(i: usize) -> strix_doc::Path {
    strix_doc::Path(vec![u32::try_from(i).unwrap_or(u32::MAX)])
}

fn letter(k: usize) -> String {
    let mut n = k;
    let mut out = String::new();
    loop {
        out.insert(0, char::from(b'a' + (n % 26) as u8));
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    out
}

fn insert(s: &mut Session, v: &Value) -> CmdResult {
    if s.sel.focus.story != StoryRef::Body || s.sel.focus.path.cell().is_some() {
        return Err(CmdError::Disabled("an exercise goes in the body, outside tables".into()));
    }
    ensure_styles(s);
    let w = words(s);
    let title = p::str(v, "title").map(str::to_string);
    let text = p::str(v, "text").map(str::to_string);
    let questions: Vec<String> = v
        .get("questions")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).take(MAX_QUESTIONS).map(str::to_string).collect())
        .unwrap_or_default();
    let list = s.doc.numbering.try_add_list(ListKind::LowerLetter).map_err(|e| CmdError::Failed(e.to_string()))?;
    let num = Some(NumRef { num: list, level: 0 });
    let placeholder = |mut c: ContentControl, shown: &str| {
        c.placeholder_text = shown.into();
        c.showing_placeholder = true;
        c
    };
    let mut blocks = Vec::new();
    let mut exercise = control(TAG_EXERCISE, w.exercise, ControlLock::SdtLocked);
    if title.is_none() {
        exercise = placeholder(exercise, w.title);
    }
    blocks.push(para(STYLE_TITLE, title.as_deref().unwrap_or(w.title), vec![exercise], 0, None));
    let mut statement = control(TAG_TEXT, "Statement", ControlLock::SdtLocked);
    if text.is_none() {
        statement = placeholder(statement, w.text);
    }
    blocks.push(para("Normal", text.as_deref().unwrap_or(w.text), vec![statement], 1, None));
    let mut qc = control(TAG_QUESTIONS, "Questions", ControlLock::SdtLocked);
    if questions.is_empty() {
        qc = placeholder(qc, w.question);
        blocks.push(para(STYLE_QUESTION, w.question, vec![qc], 1, num));
    } else {
        let n = questions.len();
        let mut opener = Some(qc);
        for (i, q) in questions.iter().enumerate() {
            blocks.push(para(STYLE_QUESTION, q, opener.take().into_iter().collect(), usize::from(i + 1 == n), num));
        }
    }
    blocks.push(para(STYLE_HEADING, w.solution, vec![control(TAG_SOLUTION, "Solution", ControlLock::SdtLocked)], 0, None));
    // The questions repeated: filled in by `update`.
    blocks.push(para(STYLE_PROMPT, "", vec![control(TAG_PROMPT, "Questions (automatic)", ControlLock::SdtContentLocked)], 1, None));
    // The free space of the solution closes the solution and the exercise.
    blocks.push(para("Normal", "", Vec::new(), 2, None));
    let top = s.sel.focus.path.0.first().map_or(0, |&t| t as usize);
    let at = top.saturating_add(1).min(s.doc.body.len());
    for (i, b) in blocks.into_iter().enumerate() {
        s.doc.body.insert(at + i, para_block(b));
    }
    // The caret goes to the title, ready to type.
    s.sel = crate::Selection::caret(strix_doc::Pos::new(StoryRef::Body, body_path(at), OBJ.len_utf8()));
    s.touch();
    update(s);
    s.clamp_selection();
    super::sel_result(s)
}

/// Where a top-level body block sits in the exercise structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Title,
    Text,
    Questions,
    Solution,
    Prompt,
    Results,
    Other,
}

#[derive(Debug, Default)]
struct Exercise {
    title: String,
    questions: Vec<String>,
    /// First and last top-level block of the repeated questions.
    prompt: Option<(usize, usize)>,
    /// First and last top-level block of the questions.
    question_blocks: Option<(usize, usize)>,
    /// The questions control still shows its placeholder.
    no_questions: bool,
}

#[derive(Debug, Default)]
struct Layout {
    exercises: Vec<Exercise>,
    /// Exercise and part of each top-level block.
    parts: Vec<(Option<usize>, Part)>,
    results: Option<(usize, usize)>,
}

fn plain(p: &Paragraph) -> String {
    p.text.chars().filter(|&c| c != OBJ).collect::<String>().trim().to_string()
}

/// Reads the exercise structure from the body's control markers.
fn scan(s: &Session) -> Layout {
    let mut out = Layout::default();
    let mut stack: Vec<(String, bool)> = Vec::new();
    let mut current: Option<usize> = None;
    for (i, b) in s.doc.body.iter().enumerate() {
        let mut tags: Vec<(String, bool)> = stack.clone();
        if let Block::Para(p) = &**b {
            for o in &p.objects {
                match o {
                    InlineObject::ControlStart { control } => {
                        if control.tag == TAG_EXERCISE && out.exercises.len() < MAX_EXERCISES {
                            out.exercises.push(Exercise::default());
                            current = Some(out.exercises.len() - 1);
                        }
                        stack.push((control.tag.clone(), control.showing_placeholder));
                        tags.push((control.tag.clone(), control.showing_placeholder));
                    }
                    InlineObject::ControlEnd => {
                        if let Some((tag, _)) = stack.pop()
                            && tag == TAG_EXERCISE
                        {
                            current = None;
                        }
                    }
                    _ => {}
                }
            }
        }
        let has = |t: &str| tags.iter().any(|(x, _)| x == t);
        let in_exercise = has(TAG_EXERCISE);
        let part = if has(TAG_RESULTS) {
            Part::Results
        } else if !in_exercise {
            Part::Other
        } else if has(TAG_PROMPT) {
            Part::Prompt
        } else if has(TAG_QUESTIONS) {
            Part::Questions
        } else if has(TAG_TEXT) {
            Part::Text
        } else if has(TAG_SOLUTION) {
            Part::Solution
        } else if matches!(&**b, Block::Para(p) if p.props.style.as_deref() == Some(STYLE_TITLE)) {
            Part::Title
        } else {
            Part::Other
        };
        let ex = if in_exercise { out.exercises.len().checked_sub(1).filter(|_| current.is_some() || part != Part::Other) } else { None };
        out.parts.push((ex, part));
        // A range grows only over contiguous blocks: a later control with the same tag (pasted,
        // say) is another instance and is left alone, so a rewrite never spans user content.
        let grow = |r: &mut Option<(usize, usize)>| match *r {
            None => *r = Some((i, i)),
            Some((a, b)) if b + 1 == i => *r = Some((a, i)),
            Some(_) => {}
        };
        if part == Part::Results {
            grow(&mut out.results);
        }
        let Some(e) = ex.and_then(|e| out.exercises.get_mut(e)) else { continue };
        match (part, &**b) {
            (Part::Title, Block::Para(p)) => {
                let placeholder = tags.iter().any(|(t, ph)| t == TAG_EXERCISE && *ph);
                if !placeholder {
                    e.title = plain(p);
                }
            }
            (Part::Questions, Block::Para(p)) => {
                grow(&mut e.question_blocks);
                if tags.iter().any(|(t, ph)| t == TAG_QUESTIONS && *ph) {
                    e.no_questions = true;
                } else if e.questions.len() < MAX_QUESTIONS && !plain(p).is_empty() {
                    e.questions.push(plain(p).chars().take(MAX_TEXT).collect());
                }
            }
            (Part::Prompt, _) => grow(&mut e.prompt),
            _ => {}
        }
    }
    out
}

/// A hash of what the automatic parts show besides results: titles and questions.
pub(crate) fn signature(s: &Session) -> u64 {
    use std::hash::{Hash, Hasher};
    let layout = scan(s);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for e in &layout.exercises {
        (&e.title, &e.questions).hash(&mut h);
    }
    layout.results.is_some().hash(&mut h);
    h.finish()
}

/// The exercise (sandbox) of each top-level body block, `None` outside exercises.
pub(crate) fn zones(s: &Session) -> Vec<Option<usize>> {
    scan(s).parts.into_iter().map(|(e, _)| e).collect()
}

/// The exercise holding the caret.
fn exercise_at_caret(s: &Session) -> Option<usize> {
    if s.sel.focus.story != StoryRef::Body {
        return None;
    }
    let top = *s.sel.focus.path.0.first()? as usize;
    scan(s).parts.get(top).and_then(|(e, _)| *e)
}

fn add_question(s: &mut Session, v: &Value) -> CmdResult {
    let ex = exercise_at_caret(s).ok_or_else(|| CmdError::Disabled("the cursor isn't in an exercise".into()))?;
    let layout = scan(s);
    let e = layout.exercises.get(ex).ok_or_else(|| CmdError::Failed("exercise vanished".into()))?;
    let (_, last) = e.question_blocks.ok_or_else(|| CmdError::Failed("the exercise has no questions part".into()))?;
    if e.questions.len() >= MAX_QUESTIONS {
        return Err(CmdError::Failed(format!("an exercise holds at most {MAX_QUESTIONS} questions")));
    }
    let text = p::str(v, "text").unwrap_or_default().to_string();
    let block = s.doc.body.get_mut(last).ok_or_else(|| CmdError::Failed("no questions".into()))?;
    let Block::Para(prev) = Arc::make_mut(block) else { return Err(CmdError::Failed("the questions end in a table".into())) };
    if e.no_questions {
        // Replace the placeholder with the first question.
        let opens: Vec<ContentControl> = prev
            .objects
            .iter()
            .filter_map(|o| match o {
                InlineObject::ControlStart { control } => {
                    let mut c = (**control).clone();
                    c.showing_placeholder = false;
                    Some(c)
                }
                _ => None,
            })
            .collect();
        let ends = prev.objects.iter().filter(|o| matches!(o, InlineObject::ControlEnd)).count();
        let numbering = prev.props.numbering;
        *prev = para(STYLE_QUESTION, &text, opens, ends, numbering);
        s.sel = crate::Selection::caret(strix_doc::Pos::new(StoryRef::Body, body_path(last), prev.text.len() - OBJ.len_utf8() * ends));
    } else {
        // The questions control's end moves to the new last question.
        let Some(end) = prev.objects.iter().rposition(|o| matches!(o, InlineObject::ControlEnd)) else {
            return Err(CmdError::Failed("the questions part has no end".into()));
        };
        prev.objects.remove(end);
        if let Some(b) = prev.text.rfind(OBJ) {
            prev.text.replace_range(b..b + OBJ.len_utf8(), "");
            let total: usize = prev.text.len();
            prev.runs =
                if total == 0 { Vec::new() } else { vec![Run { len: total, props: prev.runs.first().map(|r| r.props.clone()).unwrap_or_default() }] };
        }
        prev.touch();
        let numbering = prev.props.numbering;
        let new = para(STYLE_QUESTION, &text, Vec::new(), 1, numbering);
        let caret = new.text.len() - OBJ.len_utf8();
        s.doc.body.insert(last + 1, para_block(new));
        s.sel = crate::Selection::caret(strix_doc::Pos::new(StoryRef::Body, body_path(last + 1), caret));
    }
    s.touch();
    update(s);
    s.clamp_selection();
    super::sel_result(s)
}

fn answer(s: &mut Session, v: &Value) -> CmdResult {
    let at = super::equation::equation_at_caret(s).ok_or_else(|| CmdError::Disabled("no equation at the caret".into()))?;
    let question = p::u64(v, "question").ok_or_else(|| CmdError::Params("question: the 1-based question number (0 = none)".into()))?;
    let question = u32::try_from(question.min(MAX_QUESTIONS as u64)).unwrap_or(0);
    let para = s.doc.para_mut(at.story, &at.path).map_err(|e| CmdError::Failed(e.to_string()))?;
    let Some(InlineObject::Equation { math, .. }) = para.object_at_mut(at.off) else {
        return Err(CmdError::Failed("no equation there".into()));
    };
    math.answer = question;
    math.omml.clear();
    para.touch();
    s.touch();
    let changed = update(s);
    Ok(json!({"answer": question, "summaryChanged": changed}))
}

fn info(s: &mut Session, _: &Value) -> CmdResult {
    let layout = scan(s);
    let ex = exercise_at_caret(s);
    let e = ex.and_then(|i| layout.exercises.get(i));
    let answer = super::equation::equation_at_caret(s)
        .and_then(|at| s.doc.para_at(&at).and_then(|p| p.object_at(at.off)).cloned())
        .and_then(|o| match o {
            InlineObject::Equation { math, .. } => Some(math.answer),
            _ => None,
        })
        .unwrap_or(0);
    Ok(json!({
        "exercise": ex.map(|i| i + 1),
        "title": e.map(|e| e.title.clone()),
        "questions": e.map(|e| e.questions.clone()).unwrap_or_default(),
        "answer": answer,
        "exercises": layout.exercises.len(),
    }))
}

/// Replaces top-level blocks `from..=to` with `new` when they differ; returns whether it did.
fn replace(s: &mut Session, from: usize, to: usize, new: Vec<Paragraph>) -> bool {
    let same = s.doc.body.get(from..=to).is_some_and(|old| {
        old.len() == new.len()
            && old.iter().zip(&new).all(|(o, n)| matches!(&**o, Block::Para(p) if p.text == n.text && p.objects == n.objects && p.props == n.props))
    });
    if same || to >= s.doc.body.len() || from > to {
        return false;
    }
    s.doc.body.splice(from..=to, new.into_iter().map(para_block));
    true
}

/// The opening control of the paragraph at top-level block `i` with tag `tag`.
fn opener(s: &Session, i: usize, tag: &str) -> Option<ContentControl> {
    match s.doc.body.get(i).map(|b| &**b) {
        Some(Block::Para(p)) => p.objects.iter().find_map(|o| match o {
            InlineObject::ControlStart { control } if control.tag == tag => Some((**control).clone()),
            _ => None,
        }),
        _ => None,
    }
}

/// Rebuilds the automatic parts: the questions repeated in each solution and the results summary
/// (moved to, or created at, the end of the document). Returns whether anything changed.
pub(crate) fn update(s: &mut Session) -> bool {
    let layout = scan(s);
    if layout.exercises.is_empty() && layout.results.is_none() {
        return false;
    }
    let w = words(s);
    let mut changed = false;
    // The repeated questions, last exercise first so earlier block indices stay valid.
    for e in layout.exercises.iter().rev() {
        let Some((from, to)) = e.prompt else { continue };
        let Some(ctl) = opener(s, from, TAG_PROMPT) else { continue };
        let new: Vec<Paragraph> = if e.questions.is_empty() {
            vec![para(STYLE_PROMPT, "", vec![ctl], 1, None)]
        } else {
            let n = e.questions.len();
            let mut first = Some(ctl);
            e.questions
                .iter()
                .enumerate()
                .map(|(k, q)| para(STYLE_PROMPT, &format!("{}) {q}", letter(k)), first.take().into_iter().collect(), usize::from(k + 1 == n), None))
                .collect()
        };
        changed |= replace(s, from, to, new);
    }
    // The answers: the values of the result equations, by exercise and question.
    let layout = scan(s);
    let opts = super::calc::options_now(s);
    let mut answers: std::collections::HashMap<(usize, u32), Vec<String>> = std::collections::HashMap::new();
    for (top, question, text) in super::calc::answers(s, &opts) {
        if let Some(ex) = layout.parts.get(top).and_then(|(x, _)| *x) {
            answers.entry((ex, question)).or_default().push(text);
        }
    }
    let mut lines = vec![(STYLE_HEADING, w.results.to_string())];
    for (n, e) in layout.exercises.iter().enumerate() {
        let title = if e.title.is_empty() { String::new() } else { format!(" — {}", e.title) };
        lines.push((STYLE_RESULT, format!("{} {}{title}", w.exercise, n + 1)));
        for (k, q) in e.questions.iter().enumerate() {
            let question = u32::try_from(k + 1).unwrap_or(u32::MAX);
            let value = answers.get(&(n, question)).map_or_else(|| w.no_result.to_string(), |found| found.join("; "));
            lines.push((STYLE_RESULT, format!("{}) {q}: {value}", letter(k))));
        }
    }
    let ctl = layout
        .results
        .and_then(|(from, _)| opener(s, from, TAG_RESULTS))
        .unwrap_or_else(|| control(TAG_RESULTS, "Results", ControlLock::SdtContentLocked));
    let n = lines.len();
    let mut first = Some(ctl);
    let new: Vec<Paragraph> = lines
        .iter()
        .enumerate()
        .map(|(i, (style, text))| para(style, text, first.take().into_iter().collect(), usize::from(i + 1 == n), None))
        .collect();
    match layout.results {
        // Kept at the very end: one summary for every exercise.
        Some((from, to)) if to + 1 == s.doc.body.len() => changed |= replace(s, from, to, new),
        Some((from, to)) => {
            s.doc.body.drain(from..=to);
            s.doc.body.extend(new.into_iter().map(para_block));
            changed = true;
        }
        None => {
            s.doc.body.extend(new.into_iter().map(para_block));
            changed = true;
        }
    }
    if changed {
        s.touch();
        s.clamp_selection();
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use strix_doc::Document;

    fn session() -> Session {
        let mut s = Session::new(Document::new());
        s.run("document.setText", &json!({"text": "Intro"})).unwrap();
        s.prefs.default_language = Some("it-IT".into());
        s
    }

    fn texts(s: &Session) -> Vec<String> {
        s.doc
            .body
            .iter()
            .map(|b| match &**b {
                Block::Para(p) => plain(p),
                Block::Table(_) => "[table]".into(),
            })
            .collect()
    }

    #[test]
    fn an_exercise_has_a_rigid_structure_and_one_summary_at_the_end() {
        let mut s = session();
        s.run(
            "exercise.insert",
            &json!({"title": "Trave appoggiata", "text": "Una trave di luce L = 4.5 m…", "questions": ["Il momento massimo", "La reazione"]}),
        )
        .unwrap();
        let t = texts(&s);
        assert_eq!(
            t,
            [
                "Intro",
                "Trave appoggiata",
                "Una trave di luce L = 4.5 m…",
                "Il momento massimo",
                "La reazione",
                "Risoluzione",
                "a) Il momento massimo",
                "b) La reazione",
                "",
                "Riepilogo dei risultati",
                "Esercizio 1 — Trave appoggiata",
                "a) Il momento massimo: risultato non indicato",
                "b) La reazione: risultato non indicato",
            ],
        );
        // The structure is made of locked controls.
        let layout = scan(&s);
        assert_eq!(layout.exercises.len(), 1);
        assert_eq!(layout.exercises[0].questions, ["Il momento massimo", "La reazione"]);
        assert_eq!(opener(&s, 1, TAG_EXERCISE).map(|c| c.lock), Some(ControlLock::SdtLocked));
        assert_eq!(opener(&s, 6, TAG_PROMPT).map(|c| c.lock), Some(ControlLock::SdtContentLocked));
        // A second exercise: the summary stays one, at the end.
        s.sel = crate::Selection::caret(strix_doc::Pos::new(StoryRef::Body, body_path(8), 0));
        s.run("exercise.insert", &json!({"title": "Secondo", "questions": ["x"]})).unwrap();
        let t = texts(&s);
        assert_eq!(t.iter().filter(|x| *x == "Riepilogo dei risultati").count(), 1);
        assert_eq!(t.last().map(String::as_str), Some("a) x: risultato non indicato"));
        assert!(t.contains(&"Esercizio 2 — Secondo".to_string()), "{t:?}");
    }

    #[test]
    fn result_equations_fill_the_summary() {
        let mut s = session();
        s.run("exercise.insert", &json!({"title": "Trave", "questions": ["Momento", "Taglio"]})).unwrap();
        // Type the solution in the free paragraph after the repeated questions.
        let free = texts(&s).iter().position(|t| t == "b) Taglio").unwrap() + 1;
        s.sel = crate::Selection::caret(strix_doc::Pos::new(StoryRef::Body, body_path(free), 0));
        for t in ["L = 4.5 m", "q = 12 kN/m", "M = q·L^2/8"] {
            s.run("calc.insert", &json!({"text": t})).unwrap();
            s.run("text.insert", &json!({"text": " "})).unwrap();
        }
        // The caret is after the last equation: step back onto it and mark it.
        let mut at = s.sel.focus.clone();
        at.off -= 1 + OBJ.len_utf8();
        s.sel = crate::Selection::caret(at);
        s.run("equation.answer", &json!({"question": 1})).unwrap();
        let t = texts(&s);
        assert!(t.contains(&"a) Momento: M = 30.375 kJ".to_string()), "{t:?}");
        assert!(t.contains(&"b) Taglio: risultato non indicato".to_string()), "{t:?}");
        // The mark survives saving.
        let bytes = crate::io::save_bytes("e.xmath", &s.doc).unwrap();
        let doc = crate::io::open_bytes("e.xmath", &bytes).unwrap();
        let marked = doc.body.iter().any(
            |b| matches!(&**b, Block::Para(p) if p.objects.iter().any(|o| matches!(o, InlineObject::Equation { math, .. } if math.answer == 1))),
        );
        assert!(marked);
    }

    #[test]
    fn adding_questions_updates_the_repeated_questions() {
        let mut s = session();
        s.run("exercise.insert", &json!({})).unwrap();
        // Empty: placeholders; the caret is in the exercise.
        assert_eq!(s.run("exercise.info", &json!({})).unwrap()["questions"], json!([]));
        s.run("exercise.addQuestion", &json!({"text": "Prima"})).unwrap();
        s.run("exercise.addQuestion", &json!({"text": "Seconda"})).unwrap();
        let info = s.run("exercise.info", &json!({})).unwrap();
        assert_eq!(info["questions"], json!(["Prima", "Seconda"]));
        let t = texts(&s);
        assert!(t.contains(&"a) Prima".to_string()) && t.contains(&"b) Seconda".to_string()), "{t:?}");
    }

    #[test]
    fn an_exercise_is_a_sandbox() {
        let mut s = session();
        s.run("calc.insert", &json!({"text": "a = 2 m"})).unwrap();
        s.run("exercise.insert", &json!({"title": "Uno", "questions": ["q"]})).unwrap();
        let free = texts(&s).iter().position(|t| t == "a) q").unwrap() + 1;
        s.sel = crate::Selection::caret(strix_doc::Pos::new(StoryRef::Body, body_path(free), 0));
        // Inside, the document's `a` is unknown; `b` defined inside stays inside.
        s.run("calc.insert", &json!({"text": "b = 3 m"})).unwrap();
        s.run("text.insert", &json!({"text": " "})).unwrap();
        s.run("calc.insert", &json!({"text": "a ="})).unwrap();
        s.sel = crate::Selection::caret(strix_doc::Pos::new(StoryRef::Body, body_path(0), 0));
        s.run("caret.lineEnd", &json!({})).ok();
        s.run("calc.insert", &json!({"text": "b ="})).unwrap();
        let r = s.run("calc.recalculate", &json!({})).unwrap();
        let errors: Vec<String> = r["errors"].as_array().unwrap().iter().map(|e| e["error"].as_str().unwrap_or_default().to_string()).collect();
        assert!(errors.iter().any(|e| e.contains("unknown name a")), "{errors:?}");
        assert!(errors.iter().any(|e| e.contains("unknown name b")), "{errors:?}");
        // The document's parameters are only its own.
        let vars = s.run("calc.variables", &json!({})).unwrap();
        let names: Vec<&str> = vars["variables"].as_array().unwrap().iter().map(|v| v["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["a"]);
    }

    #[test]
    fn a_second_summary_never_takes_user_text_with_it() {
        let mut s = session();
        s.run("exercise.insert", &json!({"title": "Uno", "questions": ["q"]})).unwrap();
        // A copy of the summary pasted at the top, with the user's text between the two.
        let n = s.doc.body.len();
        let summary: Vec<_> = s.doc.body[n - 3..].to_vec();
        s.doc.body.splice(0..0, summary);
        update(&mut s);
        let after = texts(&s);
        assert!(after.contains(&"Intro".to_string()), "{after:?}");
        assert!(after.contains(&"Uno".to_string()), "{after:?}");
    }

    #[test]
    fn hostile_params_never_panic() {
        let mut s = session();
        for v in [json!({"questions": "x"}), json!({"questions": [1, null, "a"]}), json!({"title": 5}), json!({"questions": vec!["q"; 500]})] {
            let _ = s.run("exercise.insert", &v);
        }
        let _ = s.run("exercise.addQuestion", &json!({"text": "€".repeat(10_000)}));
        let _ = s.run("equation.answer", &json!({"question": u64::MAX}));
        let _ = s.run("exercise.update", &json!({}));
        let _ = s.run("exercise.info", &json!({}));
    }
}
