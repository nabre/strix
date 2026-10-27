//! Exercises (docs/exercise.md): a rigid structure for a worked problem, made of locked block
//! content controls so it can't be broken by editing:
//!
//! - the **title** (style Exercise Title), in the exercise control `strix:exercise`;
//! - the **statement** (`strix:exercise.text`): free content, laid out with every tool;
//! - the **questions** (`strix:exercise.questions`): paragraphs in the Exercise Question style
//!   are questions, lettered a), b)… and nested up to three levels (a.1), a.1.1)) by their list
//!   level (Tab, Shift+Tab); any other content there (notes in the Exercise Note style, tables,
//!   figures) is free, between the questions;
//! - the **solution** (`strix:exercise.solution`): a heading, then each question repeated
//!   automatically (one read-only `strix:exercise.prompt` per question) followed by its free
//!   working, kept when the questions change; an equation in it is marked as the result of a
//!   question (`Math::answer`, `equation.answer`: by default the question it is worked under);
//! - the **data** (`strix:exercise.data`, read-only): the exercise's initial data with their
//!   values in the active variant, generated from the data kept on the exercise control (Exercise
//!   Properties pane, `exercise.setData`); they are the first parameters of the exercise's
//!   sandbox;
//! - one **results summary** (`strix:results`, read-only) at the end of the document, listing
//!   every exercise's questions with the value of their result equations.
//!
//! The automatic parts are rebuilt after every command that changes what they show
//! ([`update`]), like calculations; nothing else is touched.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
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
pub const TAG_DATA: &str = "strix:exercise.data";
/// The element on the exercise control's properties holding its data and variants (JSON).
const DATA_EL: &str = "strix:exerciseData";
/// Most data items and variants of one exercise; longest data text, in characters.
const MAX_DATA: usize = 100;
const MAX_VARIANTS: usize = 26;
const MAX_DATA_TEXT: usize = 200;
const STYLE_DATA: &str = "ExerciseData";

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
const STYLE_NOTE: &str = "ExerciseNote";
/// Deepest question nesting (a), a.1), a.1.1)), 0-based.
const MAX_LEVEL: u8 = 2;
/// Indent per question level in the repeated questions, points.
const LEVEL_INDENT: f32 = 18.0;

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
            .params(r#"{"text"?: string, "level"?: 0–2, "sub"?: bool (one level below the last question)} → adds a question at the end of the questions of the exercise at the caret"#),
        CommandSpec::new("exercise.addNote", "Add Note", "Math › Exercise", add_note)
            .when(|s| if exercise_at_caret(s).is_some() { None } else { Some("the cursor isn't in an exercise") })
            .params(r#"{"text"?: string} → adds a note (free text, not a question) at the end of the questions of the exercise at the caret"#),
        CommandSpec::new("equation.answer", "Result Of", "Math › Exercise", answer)
            .when(|s| if super::equation::equation_at_caret(s).is_some() { None } else { Some("no equation at the caret") })
            .params(r#"{"question": n (1-based; 0 = none)} → the equation at the caret is the result of that question of its exercise"#),
        CommandSpec::new("exercise.info", "Exercise Info", "Math › Exercise", info)
            .pure()
            .params(r#"→ {"exercise": n|null, "title", "questions": [string], "labels": ["a)", "a.1)"…], "levels": [n], "answer": n (of the equation at the caret, 0 none), "exercises": n}"#),
        CommandSpec::new("exercise.data", "Exercise Data", "Math › Exercise", data_info)
            .pure()
            .params(r#"{"exercise"?: n (1-based; default: at the caret)} → {"exercise", "data": {"items": [{"symbol", "description", "unit", "values": [string per variant]}], "variants": [name], "active": n}, "errors": [string|null per item]}"#),
        CommandSpec::new("exercise.setData", "Set Exercise Data", "Math › Exercise", set_data)
            .params(r#"{"exercise"?: n, "data": {"items": [...], "variants": [...], "active": n}} → stores the exercise's data and variants, then recalculates"#),
        CommandSpec::new("exercise.variant", "Variant", "Math › Exercise", set_variant)
            .params(r#"{"exercise"?: n, "active": n (0-based)} → makes that variant the active one"#),
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
    /// Titles of the controls, shown on their frames.
    statement: &'static str,
    questions: &'static str,
    prompt: &'static str,
    data: &'static str,
    variant: &'static str,
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
            statement: "Testo",
            questions: "Domande",
            prompt: "Domande (automatiche)",
            data: "Dati",
            variant: "variante",
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
            statement: "Statement",
            questions: "Questions",
            prompt: "Questions (automatic)",
            data: "Data",
            variant: "variant",
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
            STYLE_DATA,
            "Exercise Data",
            ParaProps { space_after: Some(1.0), ..Default::default() },
            CharProps { no_proof: Some(true), ..Default::default() },
        ),
        para_style(
            STYLE_NOTE,
            "Exercise Note",
            ParaProps { space_after: Some(3.0), indent_left: Some(18.0), ..Default::default() },
            CharProps { italic: Some(true), ..Default::default() },
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

/// A new list for an exercise's questions: a), then a.1), then a.1.1).
fn add_question_list(s: &mut Session) -> Result<u32, CmdError> {
    use strix_doc::numbering::levels_for;
    use strix_doc::section::NumFormat;
    let mut levels = levels_for(ListKind::LowerLetter);
    // Each level hangs its label wide enough for `a.1.1)`, so the text never jumps to a tab.
    for (i, text, indent, hanging) in [(0usize, "%1)", 18.0, 18.0), (1, "%1.%2)", 50.0, 28.0), (2, "%1.%2.%3)", 90.0, 36.0)] {
        if let Some(l) = levels.get_mut(i) {
            if i > 0 {
                l.format = NumFormat::Decimal;
            }
            l.text = text.into();
            l.indent = indent;
            l.hanging = hanging;
            l.tab = None;
        }
    }
    s.doc.numbering.try_add_levels(Some("Exercise questions".into()), levels).map_err(|e| CmdError::Failed(e.to_string()))
}

fn insert(s: &mut Session, v: &Value) -> CmdResult {
    if s.sel.focus.story != StoryRef::Body || s.sel.focus.path.cell().is_some() {
        return Err(CmdError::Disabled("an exercise goes in the body, outside tables".into()));
    }
    ensure_styles(s);
    let w = words(s);
    let title = p::str(v, "title").map(str::to_string);
    let text = p::str(v, "text").map(str::to_string);
    // Questions: strings, or {"text", "level"} for nested ones.
    let questions: Vec<(String, u8)> = v
        .get("questions")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|q| match q {
                    Value::String(t) => Some((t.clone(), 0)),
                    Value::Object(_) => Some((p::str(q, "text")?.to_string(), p::u64(q, "level").map_or(0, |l| l.min(u64::from(MAX_LEVEL)) as u8))),
                    _ => None,
                })
                .take(MAX_QUESTIONS)
                .collect()
        })
        .unwrap_or_default();
    let list = add_question_list(s)?;
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
    let mut statement = control(TAG_TEXT, w.statement, ControlLock::SdtLocked);
    if text.is_none() {
        statement = placeholder(statement, w.text);
    }
    blocks.push(para("Normal", text.as_deref().unwrap_or(w.text), vec![statement], 1, None));
    let mut qc = control(TAG_QUESTIONS, w.questions, ControlLock::SdtLocked);
    if questions.is_empty() {
        qc = placeholder(qc, w.question);
        blocks.push(para(STYLE_QUESTION, w.question, vec![qc], 1, num));
    } else {
        let n = questions.len();
        let mut opener = Some(qc);
        for (i, (q, level)) in questions.iter().enumerate() {
            let num = Some(NumRef { num: list, level: *level });
            blocks.push(para(STYLE_QUESTION, q, opener.take().into_iter().collect(), usize::from(i + 1 == n), num));
        }
    }
    blocks.push(para(STYLE_HEADING, w.solution, vec![control(TAG_SOLUTION, w.solution, ControlLock::SdtLocked)], 0, None));
    // The questions repeated: filled in by `update`.
    blocks.push(para(STYLE_PROMPT, "", vec![control(TAG_PROMPT, w.prompt, ControlLock::SdtContentLocked)], 1, None));
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
    Data,
    Results,
    Other,
}

/// A question: its text, level (0 = a), 1 = a.1), 2 = a.1.1)) and label.
#[derive(Debug, Clone, Default, PartialEq, Hash)]
struct Question {
    text: String,
    level: u8,
    label: String,
}

#[derive(Debug, Default)]
struct Exercise {
    title: String,
    questions: Vec<Question>,
    /// Questions counted per level so far, for the labels.
    counters: [usize; 3],
    /// The data and variants, from the exercise control.
    data: ExerciseData,
    /// The top-level block holding the exercise control's start.
    start: usize,
    /// First and last top-level block of the statement, and of the generated data.
    text_blocks: Option<(usize, usize)>,
    data_blocks: Option<(usize, usize)>,
    /// The repeated questions, one control each: first and last top-level block of each (one
    /// block, but a document from before per-question prompts has one control over all).
    prompts: Vec<(usize, usize)>,
    /// First and last top-level block of the solution (heading, prompts and working).
    solution_blocks: Option<(usize, usize)>,
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

impl Exercise {
    /// The label of the next question at `level`: `a)`, `a.1)`, `a.1.1)`.
    fn next_label(&mut self, level: u8) -> String {
        let level = usize::from(level.min(MAX_LEVEL));
        if let Some(c) = self.counters.get_mut(level) {
            *c += 1;
        }
        for deeper in self.counters.iter_mut().skip(level + 1) {
            *deeper = 0;
        }
        let [a, b, c] = self.counters;
        let first = letter(a.max(1) - 1);
        match level {
            0 => format!("{first})"),
            1 => format!("{first}.{})", b.max(1)),
            _ => format!("{first}.{}.{})", b.max(1), c.max(1)),
        }
    }
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
                            out.exercises.push(Exercise { data: read_data(control), start: i, ..Exercise::default() });
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
        } else if has(TAG_DATA) {
            Part::Data
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
        if matches!(part, Part::Solution | Part::Prompt) {
            grow(&mut e.solution_blocks);
        }
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
                } else if p.props.style.as_deref() == Some(STYLE_QUESTION) && e.questions.len() < MAX_QUESTIONS && !plain(p).is_empty() {
                    // Only question paragraphs count; notes and other content sit between them.
                    let level = p.props.numbering.map_or(0, |n| n.level).min(MAX_LEVEL);
                    let label = e.next_label(level);
                    e.questions.push(Question { text: plain(p).chars().take(MAX_TEXT).collect(), level, label });
                }
            }
            (Part::Questions, _) => grow(&mut e.question_blocks),
            (Part::Prompt, _) => {
                // A prompt control opening here is the next question's; a block without one
                // continues the control before it.
                let opens = matches!(&**b, Block::Para(p) if p.objects.iter().any(|o| matches!(o, InlineObject::ControlStart { control } if control.tag == TAG_PROMPT)));
                match e.prompts.last_mut() {
                    Some(g) if !opens && g.1 + 1 == i => g.1 = i,
                    _ if opens => e.prompts.push((i, i)),
                    _ => {}
                }
            }
            (Part::Data, _) => grow(&mut e.data_blocks),
            (Part::Text, _) => grow(&mut e.text_blocks),
            _ => {}
        }
    }
    out
}

/// The initial data of an exercise and its variants: each item has one value per variant.
#[derive(Debug, Clone, Default, PartialEq, Hash, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ExerciseData {
    pub items: Vec<DataItem>,
    /// Variant names (`A`, `B`…); at least one when there are items.
    pub variants: Vec<String>,
    /// The active variant (0-based).
    pub active: usize,
}

/// One datum: `L` "span" = 4.5 m.
#[derive(Debug, Clone, Default, PartialEq, Hash, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DataItem {
    pub symbol: String,
    pub description: String,
    pub unit: String,
    /// The value in each variant, as typed (`4.5`, `2·10^3`).
    pub values: Vec<String>,
}

/// Data text as stored: no control or object characters (the unit marker and U+FFFC are
/// reserved), at most [`MAX_DATA_TEXT`] characters.
fn clip(s: &str) -> String {
    s.chars().filter(|&c| !c.is_control() && c != OBJ).take(MAX_DATA_TEXT).collect::<String>().trim().to_string()
}

impl ExerciseData {
    /// Limits enforced: at most [`MAX_DATA`] items and [`MAX_VARIANTS`] variants, one value per
    /// variant, short texts, a valid active variant.
    pub fn sanitize(&mut self) {
        self.items.truncate(MAX_DATA);
        self.variants.truncate(MAX_VARIANTS);
        self.variants.iter_mut().for_each(|v| *v = clip(v));
        if self.variants.is_empty() && !self.items.is_empty() {
            self.variants.push("A".into());
        }
        let n = self.variants.len();
        for item in &mut self.items {
            item.symbol = clip(&item.symbol);
            item.description = clip(&item.description);
            item.unit = clip(&item.unit);
            item.values.resize(n, String::new());
            item.values.iter_mut().for_each(|v| *v = clip(v));
        }
        self.active = self.active.min(n.saturating_sub(1));
    }

    /// The definitions of the active variant: `L = 4.5 m`.
    pub fn statements(&self) -> Vec<String> {
        self.items
            .iter()
            .filter_map(|i| {
                let value = i.values.get(self.active)?;
                (!i.symbol.is_empty() && !value.is_empty()).then(|| format!("{} = {} {}", i.symbol, value, i.unit).trim().to_string())
            })
            .collect()
    }

    /// The lines of the generated data part.
    fn lines(&self, w: &Words) -> Vec<(&'static str, String)> {
        let statements: Vec<(String, String)> = self
            .items
            .iter()
            .filter_map(|i| {
                let value = i.values.get(self.active).filter(|v| !v.is_empty())?;
                // The unit follows a marker: it is set in the Unit style.
                let unit = if i.unit.is_empty() { String::new() } else { format!(" {UNIT_MARK}{}", i.unit) };
                (!i.symbol.is_empty()).then(|| (i.description.clone(), format!("{} = {value}{unit}", i.symbol)))
            })
            .collect();
        if statements.is_empty() {
            return Vec::new();
        }
        let mut head = w.data.to_string();
        if self.variants.len() > 1
            && let Some(name) = self.variants.get(self.active)
        {
            head.push_str(&format!(" ({} {name})", w.variant));
        }
        let mut out = vec![(STYLE_HEADING, head)];
        for (description, statement) in statements {
            out.push((STYLE_DATA, if description.is_empty() { statement } else { format!("{description}: {statement}") }));
        }
        out
    }
}

/// The data kept on an exercise control (empty when none or unreadable).
fn read_data(ctl: &ContentControl) -> ExerciseData {
    let open = format!("<{DATA_EL}>");
    let close = format!("</{DATA_EL}>");
    let Some(raw) = ctl.extra.iter().find_map(|x| x.strip_prefix(&open)?.strip_suffix(&close)) else { return ExerciseData::default() };
    let json = raw.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&");
    let mut data: ExerciseData = serde_json::from_str(&json).unwrap_or_default();
    data.sanitize();
    data
}

/// Stores `data` on an exercise control, as a `strix:` element Word ignores.
fn write_data(ctl: &mut ContentControl, data: &ExerciseData) {
    let open = format!("<{DATA_EL}>");
    ctl.extra.retain(|x| !x.starts_with(&open));
    if data.items.is_empty() {
        return;
    }
    let json = serde_json::to_string(data).unwrap_or_default();
    let escaped = json.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    ctl.extra.push(format!("{open}{escaped}</{DATA_EL}>"));
}

/// Generated paragraphs: the first opens `ctl`, the last closes it.
/// Marks where a unit starts in a generated line (never in user text: it is a control character).
const UNIT_MARK: char = '\u{1F}';

fn generated(ctl: ContentControl, lines: &[(&'static str, String)]) -> Vec<Paragraph> {
    let n = lines.len();
    let mut first = Some(ctl);
    lines
        .iter()
        .enumerate()
        .map(|(i, (style, text))| {
            let (before, unit) = text.split_once(UNIT_MARK).unwrap_or((text.as_str(), ""));
            // Cleaned and cut exactly as `para` stores them, so the offsets below are its own.
            let keep = |c: &char| *c != OBJ && *c != '\n' && *c != UNIT_MARK;
            let before: String = before.chars().filter(keep).take(MAX_TEXT).collect();
            let room = MAX_TEXT.saturating_sub(before.chars().count());
            let unit: String = unit.chars().filter(keep).take(room).collect();
            let opens: Vec<ContentControl> = first.take().into_iter().collect();
            let lead = opens.len() * OBJ.len_utf8();
            let mut p = para(style, &format!("{before}{unit}"), opens, usize::from(i + 1 == n), None);
            // The unit in the Unit style (docs/calc.md), like the units of calculations.
            let a = lead.saturating_add(before.len());
            let b = a.saturating_add(unit.len());
            if !unit.is_empty() && b <= p.text.len() && p.text.is_char_boundary(a) && p.text.is_char_boundary(b) {
                let props = p.runs.first().map(|r| r.props.clone()).unwrap_or_default();
                let unit_props = CharProps { style: Some(super::calc::UNIT_STYLE.into()), ..props.clone() };
                p.runs = vec![Run { len: a, props: props.clone() }, Run { len: b - a, props: unit_props }, Run { len: p.text.len() - b, props }];
                p.runs.retain(|r| r.len > 0);
            }
            p
        })
        .collect()
}

/// The definitions each exercise's sandbox starts with (its data, active variant).
pub(crate) fn seeds(s: &Session) -> std::collections::BTreeMap<usize, Vec<String>> {
    scan(s).exercises.iter().enumerate().filter(|(_, e)| !e.data.items.is_empty()).map(|(i, e)| (i, e.data.statements())).collect()
}

/// The exercise named by `exercise` (1-based) or holding the caret.
fn target(s: &Session, v: &Value) -> Result<usize, CmdError> {
    match p::u64(v, "exercise") {
        Some(n) if n >= 1 => Ok(n as usize - 1),
        Some(_) => Err(CmdError::Params("exercise: 1-based".into())),
        None => exercise_at_caret(s).ok_or_else(|| CmdError::Disabled("the cursor isn't in an exercise".into())),
    }
}

fn data_info(s: &mut Session, v: &Value) -> CmdResult {
    let ex = target(s, v)?;
    let layout = scan(s);
    let e = layout.exercises.get(ex).ok_or_else(|| CmdError::Params("no such exercise".into()))?;
    // Each datum is checked as the sandbox reads it.
    let mut ws = strix_calc::Worksheet::new(super::calc::options_now(s));
    let errors: Vec<Value> = e
        .data
        .items
        .iter()
        .map(|i| {
            let value = i.values.get(e.data.active).cloned().unwrap_or_default();
            if i.symbol.is_empty() || value.is_empty() {
                return Value::Null;
            }
            let r = ws.eval_statement(&format!("{} = {} {}", i.symbol, value, i.unit));
            match (r.error, r.name.as_deref() == Some(i.symbol.as_str())) {
                (Some(e), _) => json!(e.to_string()),
                (None, false) => json!(format!("{} is not a name", i.symbol)),
                (None, true) => Value::Null,
            }
        })
        .collect();
    Ok(json!({"exercise": ex + 1, "data": e.data, "errors": errors}))
}

/// Changes the data of exercise `ex` with `f`, then rebuilds and recalculates.
fn with_data(s: &mut Session, ex: usize, f: impl FnOnce(&mut ExerciseData)) -> CmdResult {
    let layout = scan(s);
    let e = layout.exercises.get(ex).ok_or_else(|| CmdError::Params("no such exercise".into()))?;
    let mut data = e.data.clone();
    f(&mut data);
    data.sanitize();
    let block = s.doc.body.get_mut(e.start).ok_or_else(|| CmdError::Failed("exercise vanished".into()))?;
    let Block::Para(p) = Arc::make_mut(block) else { return Err(CmdError::Failed("exercise vanished".into())) };
    let mut found = false;
    for o in &mut p.objects {
        if let InlineObject::ControlStart { control } = o
            && control.tag == TAG_EXERCISE
        {
            write_data(control, &data);
            found = true;
            break;
        }
    }
    if !found {
        return Err(CmdError::Failed("exercise vanished".into()));
    }
    p.touch();
    s.touch();
    update(s);
    s.clamp_selection();
    Ok(json!({"exercise": ex + 1, "data": data}))
}

fn set_data(s: &mut Session, v: &Value) -> CmdResult {
    let ex = target(s, v)?;
    let new: ExerciseData =
        v.get("data").cloned().map(serde_json::from_value).transpose().map_err(|e| CmdError::Params(format!("data: {e}")))?.unwrap_or_default();
    with_data(s, ex, |d| *d = new)
}

fn set_variant(s: &mut Session, v: &Value) -> CmdResult {
    let ex = target(s, v)?;
    let active = p::u64(v, "active").ok_or_else(|| CmdError::Params("active: the 0-based variant".into()))? as usize;
    with_data(s, ex, |d| d.active = active)
}

/// A hash of what the automatic parts show besides results: titles and questions.
pub(crate) fn signature(s: &Session) -> u64 {
    use std::hash::{Hash, Hasher};
    let layout = scan(s);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for e in &layout.exercises {
        (&e.title, &e.questions, e.data.statements()).hash(&mut h);
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
    let last_level = e.questions.last().map_or(0, |q| q.level);
    let level = match (p::u64(v, "level"), p::bool(v, "sub")) {
        (Some(l), _) => l.min(u64::from(MAX_LEVEL)) as u8,
        (None, Some(true)) => (last_level + 1).min(MAX_LEVEL),
        _ => 0,
    };
    let list = e_list(s, last);
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
        let numbering = list.map(|num| NumRef { num, level });
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
        let numbering = list.map(|num| NumRef { num, level });
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

/// The question list of the exercise whose questions end at block `last`: the list of its first
/// numbered question paragraph.
fn e_list(s: &Session, last: usize) -> Option<u32> {
    s.doc.body.get(..=last)?.iter().rev().take(MAX_QUESTIONS * 4).find_map(|b| match &**b {
        Block::Para(p) if p.props.style.as_deref() == Some(STYLE_QUESTION) => p.props.numbering.map(|n| n.num).filter(|&n| n != 0),
        _ => None,
    })
}

/// Adds a note: free text between the questions, not counted as one.
fn add_note(s: &mut Session, v: &Value) -> CmdResult {
    let ex = exercise_at_caret(s).ok_or_else(|| CmdError::Disabled("the cursor isn't in an exercise".into()))?;
    let layout = scan(s);
    let e = layout.exercises.get(ex).ok_or_else(|| CmdError::Failed("exercise vanished".into()))?;
    let (_, last) = e.question_blocks.ok_or_else(|| CmdError::Failed("the exercise has no questions part".into()))?;
    if e.no_questions {
        return Err(CmdError::Failed("write a question first".into()));
    }
    let text = p::str(v, "text").unwrap_or_default().to_string();
    let block = s.doc.body.get_mut(last).ok_or_else(|| CmdError::Failed("no questions".into()))?;
    let Block::Para(prev) = Arc::make_mut(block) else { return Err(CmdError::Failed("the questions end in a table".into())) };
    let Some(end) = prev.objects.iter().rposition(|o| matches!(o, InlineObject::ControlEnd)) else {
        return Err(CmdError::Failed("the questions part has no end".into()));
    };
    prev.objects.remove(end);
    if let Some(b) = prev.text.rfind(OBJ) {
        prev.text.replace_range(b..b + OBJ.len_utf8(), "");
        let total = prev.text.len();
        prev.runs =
            if total == 0 { Vec::new() } else { vec![Run { len: total, props: prev.runs.first().map(|r| r.props.clone()).unwrap_or_default() }] };
    }
    prev.touch();
    let new = para(STYLE_NOTE, &text, Vec::new(), 1, None);
    let caret = new.text.len() - OBJ.len_utf8();
    s.doc.body.insert(last + 1, para_block(new));
    s.sel = crate::Selection::caret(strix_doc::Pos::new(StoryRef::Body, body_path(last + 1), caret));
    s.touch();
    update(s);
    s.clamp_selection();
    super::sel_result(s)
}

fn answer(s: &mut Session, v: &Value) -> CmdResult {
    let at = super::equation::equation_at_caret(s).ok_or_else(|| CmdError::Disabled("no equation at the caret".into()))?;
    // Without a number, the question the equation is worked under (the prompt above it).
    let question = match p::u64(v, "question") {
        Some(q) => q,
        None => question_above(s, &at).ok_or_else(|| CmdError::Params("question: the 1-based question number (0 = none)".into()))?,
    };
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

/// The 1-based number of the question whose prompt is the nearest above `at` in its solution.
fn question_above(s: &Session, at: &strix_doc::Pos) -> Option<u64> {
    let top = at.path.0.first().map(|&t| t as usize)?;
    let layout = scan(s);
    let e = layout.exercises.iter().find(|e| e.solution_blocks.is_some_and(|(a, b)| (a..=b).contains(&top)))?;
    let above = e.prompts.iter().filter(|g| g.0 < top).count();
    (above > 0 && !e.questions.is_empty()).then_some(above as u64)
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
        "questions": e.map(|e| e.questions.iter().map(|q| q.text.clone()).collect::<Vec<_>>()).unwrap_or_default(),
        "labels": e.map(|e| e.questions.iter().map(|q| q.label.clone()).collect::<Vec<_>>()).unwrap_or_default(),
        "levels": e.map(|e| e.questions.iter().map(|q| q.level).collect::<Vec<_>>()).unwrap_or_default(),
        "answer": answer,
        "exercises": layout.exercises.len(),
    }))
}

/// The repeated question `q`: its own read-only control, indented like its list.
fn prompt_para(ctl: &ContentControl, q: Option<&Question>) -> Paragraph {
    let text = q.map(|q| format!("{} {}", q.label, q.text)).unwrap_or_default();
    let mut p = para(STYLE_PROMPT, &text, vec![ctl.clone()], 1, None);
    if let Some(q) = q.filter(|q| q.level > 0) {
        p.props.indent_left = Some(LEVEL_INDENT * f32::from(q.level));
    }
    p
}

/// The question text a prompt paragraph repeats (without its label).
fn prompt_text(s: &Session, i: usize) -> Option<String> {
    match s.doc.body.get(i).map(|b| &**b) {
        Some(Block::Para(p)) => Some(plain(p).split_once(' ').map(|(_, t)| t.trim().to_string()).unwrap_or_default()),
        _ => None,
    }
}

/// Pairs of (prompt, question) indices in a longest common subsequence of equal texts.
fn matched(old: &[String], new: &[&str]) -> Vec<(usize, usize)> {
    let (n, m) = (old.len(), new.len());
    let mut t = vec![vec![0u16; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            let v = if old.get(i).map(String::as_str) == new.get(j).copied() {
                t.get(i + 1).and_then(|r| r.get(j + 1)).map_or(0, |v| v + 1)
            } else {
                let a = t.get(i + 1).and_then(|r| r.get(j)).copied().unwrap_or(0);
                let b = t.get(i).and_then(|r| r.get(j + 1)).copied().unwrap_or(0);
                a.max(b)
            };
            if let Some(c) = t.get_mut(i).and_then(|r| r.get_mut(j)) {
                *c = v;
            }
        }
    }
    let at = |i: usize, j: usize| t.get(i).and_then(|r| r.get(j)).copied().unwrap_or(0);
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < n && j < m {
        if old.get(i).map(String::as_str) == new.get(j).copied() {
            out.push((i, j));
            i += 1;
            j += 1;
        } else if at(i + 1, j) >= at(i, j + 1) {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

/// Makes the last block of the solution ending at `end` an empty paragraph holding only the
/// control ends, so new prompts can go before it without taking the user's working with them.
/// Returns that block's index.
fn free_end(s: &mut Session, end: usize) -> Option<usize> {
    let Some(Block::Para(last)) = s.doc.body.get(end).map(|b| &**b) else { return None };
    if plain(last).is_empty() && last.objects.iter().all(|o| matches!(o, InlineObject::ControlEnd)) {
        return Some(end);
    }
    let Some(Block::Para(last)) = s.doc.body.get_mut(end).map(Arc::make_mut) else { return None };
    let mut ends = 0;
    while last.text.ends_with(OBJ) && matches!(last.objects.last(), Some(InlineObject::ControlEnd)) && ends < 8 {
        last.text.pop();
        last.objects.pop();
        if let Some(r) = last.runs.last_mut() {
            r.len = r.len.saturating_sub(OBJ.len_utf8());
            if r.len == 0 {
                last.runs.pop();
            }
        }
        ends += 1;
    }
    last.touch();
    s.doc.body.insert(end + 1, para_block(para("Normal", "", Vec::new(), ends, None)));
    Some(end + 1)
}

/// Rebuilds the repeated questions of exercise `e`, one prompt per question. The working the
/// user wrote after a prompt stays with it: a prompt whose question is unchanged keeps its place,
/// a removed question's prompt goes (its working joins the question before), a new question's
/// prompt comes in before the next kept one, with an empty paragraph to write in.
fn update_prompts(s: &mut Session, e: &Exercise) -> bool {
    let Some(&(first, _)) = e.prompts.first() else { return false };
    let Some(ctl) = opener(s, first, TAG_PROMPT) else { return false };
    let want: Vec<Option<&Question>> = if e.questions.is_empty() { vec![None] } else { e.questions.iter().map(Some).collect() };
    let mut changed = false;
    // A document from before per-question prompts (one control over all) or a lost layout:
    // the first group is rewritten, one prompt and a paragraph to write in per question.
    if e.prompts.iter().any(|&(a, b)| b > a) {
        let mut new = Vec::new();
        for (k, q) in want.iter().enumerate() {
            new.push(prompt_para(&ctl, *q));
            if k + 1 < want.len() {
                new.push(para("Normal", "", Vec::new(), 0, None));
            }
        }
        for &(a, b) in e.prompts.iter().skip(1).rev() {
            s.doc.body.drain(a..=b);
        }
        return replace(s, first, e.prompts.first().map_or(first, |g| g.1), new);
    }
    let slots: Vec<usize> = e.prompts.iter().map(|g| g.0).collect();
    let texts: Vec<String> = slots.iter().map(|&i| prompt_text(s, i).unwrap_or_default()).collect();
    let wanted: Vec<&str> = want.iter().map(|q| q.map_or("", |q| q.text.as_str())).collect();
    let pairs: Vec<(usize, usize)> = if slots.len() == want.len() || (slots.len() == 1 && texts.first().is_some_and(String::is_empty)) {
        // As many prompts as questions (a question edited in place), or the empty placeholder
        // taking the first question.
        (0..slots.len().min(want.len())).map(|k| (k, k)).collect()
    } else {
        matched(&texts, &wanted)
    };
    // Edits from the bottom up, so the indices above stay valid. A new question goes before the
    // next kept prompt, or at the end of the solution.
    let end = e.solution_blocks.map_or(slots.last().copied().unwrap_or(first), |r| r.1);
    let mut inserts: Vec<(usize, Paragraph)> = Vec::new();
    let mut next_slot: Option<usize> = None;
    for (j, q) in want.iter().enumerate().rev() {
        match pairs.iter().find(|(_, w)| *w == j) {
            Some(&(k, _)) => next_slot = slots.get(k).copied(),
            None => inserts.push((next_slot.unwrap_or(usize::MAX), prompt_para(&ctl, *q))),
        }
    }
    // Kept prompts rewritten in place (labels and texts may change), removed ones dropped.
    for (k, &i) in slots.iter().enumerate().rev() {
        match pairs.iter().find(|(o, _)| *o == k) {
            Some(&(_, j)) => {
                let new = prompt_para(&ctl, want.get(j).copied().flatten());
                changed |= replace(s, i, i, vec![new]);
            }
            None => {
                s.doc.body.remove(i);
                changed = true;
                inserts.iter_mut().for_each(|(at, _)| {
                    if *at != usize::MAX && *at > i {
                        *at -= 1;
                    }
                });
            }
        }
    }
    if inserts.is_empty() {
        return changed;
    }
    let removed = slots.iter().enumerate().filter(|&(k, &i)| i <= end && !pairs.iter().any(|&(o, _)| o == k)).count();
    let end = end.saturating_sub(removed);
    // At the end, before the paragraph closing the solution.
    let tail = inserts.iter().any(|(at, _)| *at == usize::MAX).then(|| free_end(s, end)).flatten();
    // `inserts` is bottom-up for each anchor: inserting each at its anchor puts them in order.
    for (at, p) in inserts {
        let at = if at == usize::MAX { tail.unwrap_or(end) } else { at };
        if at > s.doc.body.len() {
            continue;
        }
        s.doc.body.insert(at, para_block(p));
        // Room to write: a prompt is never right after or before another one.
        if is_prompt(s, at + 1) {
            s.doc.body.insert(at + 1, para_block(para("Normal", "", Vec::new(), 0, None)));
        }
        if at > 0 && is_prompt(s, at - 1) {
            s.doc.body.insert(at, para_block(para("Normal", "", Vec::new(), 0, None)));
        }
    }
    true
}

fn is_prompt(s: &Session, i: usize) -> bool {
    matches!(s.doc.body.get(i).map(|b| &**b), Some(Block::Para(p)) if p.props.style.as_deref() == Some(STYLE_PROMPT))
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
        changed |= update_prompts(s, e);
    }
    // The data parts. The repeated questions may have changed size: read the structure again, so
    // every range below is current.
    let layout = scan(s);
    if layout.exercises.iter().any(|e| !e.data.items.is_empty()) {
        super::calc::unit_color(s);
    }
    for e in layout.exercises.iter().rev() {
        let lines = e.data.lines(&w);
        match (e.data_blocks, lines.is_empty()) {
            (Some((from, to)), true) => {
                // Only blocks that are the generated data part are removed.
                let all_data = (from..=to).all(|i| layout.parts.get(i).is_some_and(|(_, part)| *part == Part::Data));
                if all_data && s.doc.body.get(from..=to).is_some() {
                    s.doc.body.drain(from..=to);
                    changed = true;
                }
            }
            (Some((from, to)), false) => {
                let ctl = opener(s, from, TAG_DATA).unwrap_or_else(|| control(TAG_DATA, w.data, ControlLock::SdtContentLocked));
                changed |= replace(s, from, to, generated(ctl, &lines));
            }
            (None, false) => {
                let Some((_, end)) = e.text_blocks else { continue };
                let new = generated(control(TAG_DATA, w.data, ControlLock::SdtContentLocked), &lines);
                let at = (end + 1).min(s.doc.body.len());
                for (k, p) in new.into_iter().enumerate() {
                    s.doc.body.insert(at + k, para_block(p));
                }
                changed = true;
            }
            (None, true) => {}
        }
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
        let mut title = if e.title.is_empty() { String::new() } else { format!(" — {}", e.title) };
        if e.data.variants.len() > 1
            && let Some(name) = e.data.variants.get(e.data.active)
        {
            title.push_str(&format!(" ({} {name})", w.variant));
        }
        lines.push((STYLE_RESULT, format!("{} {}{title}", w.exercise, n + 1)));
        for (k, q) in e.questions.iter().enumerate() {
            let question = u32::try_from(k + 1).unwrap_or(u32::MAX);
            let value = answers.get(&(n, question)).map_or_else(|| w.no_result.to_string(), |found| found.join("; "));
            // Only the question's letter and its result: the question itself is above, in the exercise.
            lines.push((STYLE_RESULT, format!("{} {value}", q.label)));
        }
    }
    let ctl = layout
        .results
        .and_then(|(from, _)| opener(s, from, TAG_RESULTS))
        .unwrap_or_else(|| control(TAG_RESULTS, w.results, ControlLock::SdtContentLocked));
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
                "",
                "b) La reazione",
                "",
                "Riepilogo dei risultati",
                "Esercizio 1 — Trave appoggiata",
                "a) risultato non indicato",
                "b) risultato non indicato",
            ],
        );
        // The structure is made of locked controls.
        let layout = scan(&s);
        assert_eq!(layout.exercises.len(), 1);
        let questions: Vec<&str> = layout.exercises[0].questions.iter().map(|q| q.text.as_str()).collect();
        assert_eq!(questions, ["Il momento massimo", "La reazione"]);
        assert_eq!(opener(&s, 1, TAG_EXERCISE).map(|c| c.lock), Some(ControlLock::SdtLocked));
        assert_eq!(opener(&s, 6, TAG_PROMPT).map(|c| c.lock), Some(ControlLock::SdtContentLocked));
        // A second exercise: the summary stays one, at the end.
        s.sel = crate::Selection::caret(strix_doc::Pos::new(StoryRef::Body, body_path(9), 0));
        s.run("exercise.insert", &json!({"title": "Secondo", "questions": ["x"]})).unwrap();
        let t = texts(&s);
        assert_eq!(t.iter().filter(|x| *x == "Riepilogo dei risultati").count(), 1);
        assert_eq!(t.last().map(String::as_str), Some("a) risultato non indicato"));
        assert!(t.contains(&"Esercizio 2 — Secondo".to_string()), "{t:?}");
        let second = t.iter().position(|x| x == "a) x").unwrap();
        assert_eq!(t[second + 1..second + 3], ["", "Riepilogo dei risultati"], "{t:?}");
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
        assert!(t.contains(&"a) M = 30.375 kJ".to_string()), "{t:?}");
        assert!(t.contains(&"b) risultato non indicato".to_string()), "{t:?}");
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
    fn working_between_the_questions_stays_with_its_question() {
        let mut s = session();
        s.run("exercise.insert", &json!({"title": "T", "questions": ["Prima", "Seconda"]})).unwrap();
        let write = |s: &mut Session, after: &str, text: &str| {
            let at = texts(s).iter().position(|t| t == after).unwrap() + 1;
            s.sel = crate::Selection::caret(strix_doc::Pos::new(StoryRef::Body, body_path(at), 0));
            s.run("text.insert", &json!({"text": text})).unwrap();
        };
        // Each question has its own space in the solution.
        write(&mut s, "a) Prima", "lavoro a");
        write(&mut s, "b) Seconda", "lavoro b");
        let solution = |s: &Session| {
            let t = texts(s);
            let from = t.iter().position(|t| t == "Risoluzione").unwrap();
            let to = t.iter().position(|t| t == "Riepilogo dei risultati").unwrap();
            t[from + 1..to].to_vec()
        };
        assert_eq!(solution(&s), ["a) Prima", "lavoro a", "b) Seconda", "lavoro b"]);
        // A new question comes after the working of the last one, with room to write.
        s.sel = crate::Selection::caret(strix_doc::Pos::new(StoryRef::Body, body_path(texts(&s).iter().position(|t| t == "Seconda").unwrap()), 0));
        s.run("exercise.addQuestion", &json!({"text": "Terza"})).unwrap();
        assert_eq!(solution(&s), ["a) Prima", "lavoro a", "b) Seconda", "lavoro b", "c) Terza", ""]);
        // A question edited in place keeps its working.
        let q = texts(&s).iter().position(|t| t == "Prima").unwrap();
        let end = match &*s.doc.body[q] {
            Block::Para(p) => p.text.len(),
            Block::Table(_) => 0,
        };
        s.sel = crate::Selection::caret(strix_doc::Pos::new(StoryRef::Body, body_path(q), end));
        s.run("text.insert", &json!({"text": " bis"})).unwrap();
        assert_eq!(solution(&s)[..2], ["a) Prima bis", "lavoro a"]);
        // A removed question's prompt goes; the working stays, and the next prompts relabel.
        let q = texts(&s).iter().position(|t| t == "Prima bis").unwrap();
        let end = match &*s.doc.body[q] {
            Block::Para(p) => p.text.len(),
            Block::Table(_) => 0,
        };
        let pos = |off| strix_doc::Pos::new(StoryRef::Body, body_path(q), off);
        s.sel = crate::Selection { anchor: pos(OBJ.len_utf8()), focus: pos(end) };
        s.run("text.delete", &json!({})).unwrap();
        assert_eq!(solution(&s), ["lavoro a", "a) Seconda", "lavoro b", "b) Terza", ""]);
        // An equation worked under a question is its result by default.
        let at = texts(&s).iter().position(|t| t == "lavoro b").unwrap();
        s.sel = crate::Selection::caret(strix_doc::Pos::new(StoryRef::Body, body_path(at), "lavoro b".len()));
        s.run("calc.insert", &json!({"text": "x = 2"})).unwrap();
        assert_eq!(s.run("equation.answer", &json!({})).unwrap()["answer"], json!(1));
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
    fn questions_nest_and_notes_sit_between_them() {
        let mut s = session();
        s.run(
            "exercise.insert",
            &json!({"title": "T", "questions": ["Reazioni", {"text": "In A", "level": 1}, {"text": "In B", "level": 1}, {"text": "Verticale", "level": 2}, "Momento"]}),
        )
        .unwrap();
        // A note after the questions, then one more question: the note is not a question.
        s.run("exercise.addNote", &json!({"text": "Si trascuri il peso proprio."})).unwrap();
        s.run("exercise.addQuestion", &json!({"text": "Taglio"})).unwrap();
        s.run("exercise.addQuestion", &json!({"text": "In mezzeria", "sub": true})).unwrap();
        let info = s.run("exercise.info", &json!({})).unwrap();
        assert_eq!(info["labels"], json!(["a)", "a.1)", "a.2)", "a.2.1)", "b)", "c)", "c.1)"]));
        assert_eq!(info["questions"][5], "Taglio");
        let t = texts(&s);
        assert!(t.contains(&"Si trascuri il peso proprio.".to_string()), "{t:?}");
        assert!(t.contains(&"a.2.1) Verticale".to_string()), "{t:?}");
        assert!(t.contains(&"c.1) risultato non indicato".to_string()), "{t:?}");
        assert!(!t.iter().any(|x| x.contains("trascuri") && x.contains(')')), "a note is never repeated as a question: {t:?}");
    }

    #[test]
    fn data_and_variants_feed_the_sandbox() {
        let mut s = session();
        s.run("exercise.insert", &json!({"title": "Trave", "questions": ["Momento"]})).unwrap();
        let data = json!({
            "items": [
                {"symbol": "L", "description": "luce", "unit": "m", "values": ["4.5", "6"]},
                {"symbol": "q", "description": "carico", "unit": "kN/m", "values": ["12", "10"]}
            ],
            "variants": ["A", "B"],
            "active": 0
        });
        s.run("exercise.setData", &json!({"data": data})).unwrap();
        let t = texts(&s);
        assert!(t.contains(&"Dati (variante A)".to_string()), "{t:?}");
        assert!(t.contains(&"luce: L = 4.5 m".to_string()), "{t:?}");
        // The unit is set in the Unit style.
        let unit_runs = s.doc.body.iter().filter_map(|b| match &**b {
            Block::Para(p) => Some(p.runs.iter().filter(|r| r.props.style.as_deref() == Some(super::super::calc::UNIT_STYLE)).count()),
            _ => None,
        });
        assert_eq!(unit_runs.sum::<usize>(), 2);
        // The solution uses the data without defining them.
        let free = t.iter().position(|x| x == "a) Momento").unwrap() + 1;
        s.sel = crate::Selection::caret(strix_doc::Pos::new(StoryRef::Body, body_path(free), 0));
        s.run("calc.insert", &json!({"text": "M = q·L^2/8"})).unwrap();
        let mut at = s.sel.focus.clone();
        at.off -= OBJ.len_utf8();
        s.sel = crate::Selection::caret(at);
        s.run("equation.answer", &json!({"question": 1})).unwrap();
        assert!(texts(&s).contains(&"a) M = 30.375 kJ".to_string()), "{:?}", texts(&s));
        // Variant B: everything follows.
        s.run("exercise.variant", &json!({"exercise": 1, "active": 1})).unwrap();
        let t = texts(&s);
        assert!(t.contains(&"luce: L = 6 m".to_string()), "{t:?}");
        assert!(t.contains(&"a) M = 45 kJ".to_string()), "{t:?}");
        assert!(t.contains(&"Esercizio 1 — Trave (variante B)".to_string()), "{t:?}");
        // The data survive saving.
        let bytes = crate::io::save_bytes("e.xmath", &s.doc).unwrap();
        let doc = crate::io::open_bytes("e.xmath", &bytes).unwrap();
        let mut back = Session::new(doc);
        let info = back.run("exercise.data", &json!({"exercise": 1})).unwrap();
        assert_eq!(info["data"]["items"][1]["values"], json!(["12", "10"]));
        assert_eq!(info["data"]["active"], 1);
        // Bad data are reported per item.
        s.run(
            "exercise.setData",
            &json!({"exercise": 1, "data": {"items": [{"symbol": "x y", "values": ["1"]}, {"symbol": "z", "unit": "m", "values": ["2 $"]}]}}),
        )
        .unwrap();
        let errors = s.run("exercise.data", &json!({"exercise": 1})).unwrap()["errors"].clone();
        assert!(errors[0].is_string() && errors[1].is_string(), "{errors}");
        // No data: the data part goes away.
        s.run("exercise.setData", &json!({"exercise": 1, "data": {}})).unwrap();
        assert!(!texts(&s).iter().any(|x| x.starts_with("Dati")));
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
    fn hostile_data_never_breaks_the_runs_or_other_exercises() {
        let mut s = session();
        s.run("exercise.insert", &json!({"title": "Uno", "questions": ["q"]})).unwrap();
        let end = s.doc.body.len() - 1;
        s.sel = crate::Selection::caret(strix_doc::Pos::new(StoryRef::Body, body_path(end), 0));
        s.run("exercise.insert", &json!({"title": "Due", "questions": ["r"]})).unwrap();
        // Object, newline and marker characters, and texts past every limit.
        let junk = format!("{}\u{1F}\n{}", "\u{FFFC}".repeat(50), "é".repeat(5000));
        let data = json!({"items": [{"symbol": "x", "description": junk, "unit": junk, "values": [junk]}]});
        s.run("exercise.setData", &json!({"exercise": 1, "data": data})).unwrap();
        s.run("exercise.setData", &json!({"exercise": 2, "data": {"items": [{"symbol": "y", "unit": "m", "values": ["2"]}]}})).unwrap();
        for b in &s.doc.body {
            if let Block::Para(p) = &**b {
                assert_eq!(p.runs.iter().map(|r| r.len).sum::<usize>(), p.text.len(), "runs cover the text exactly");
            }
        }
        // Removing the first exercise's data leaves the second one's and the user's text alone.
        s.run("exercise.addQuestion", &json!({"text": "más"})).ok();
        s.run("exercise.setData", &json!({"exercise": 1, "data": {}})).unwrap();
        let t = texts(&s);
        assert!(t.contains(&"Intro".to_string()) && t.contains(&"Due".to_string()) && t.contains(&"y = 2 m".to_string()), "{t:?}");
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
