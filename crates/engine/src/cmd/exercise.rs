//! Exercises (docs/exercise.md): a rigid structure for a worked problem, made of locked block
//! content controls so it can't be broken by editing:
//!
//! - the **title** (style Exercise Title), in the exercise control `strix:exercise`: the user's
//!   text, in a control of its own (`strix:exercise.title`) whose placeholder is only the title;
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
use wordcraft_doc::control::{ContentControl, ControlKind, ControlLock};
use wordcraft_doc::numbering::ListKind;
use wordcraft_doc::para::OBJ;
use wordcraft_doc::props::{CharProps, NumRef, ParaProps};
use wordcraft_doc::styles::{Style, StyleKind};
use wordcraft_doc::{Block, Document, InlineObject, Paragraph, Run, StoryRef, para_block};

use crate::{CmdError, CmdResult, CommandSpec, Session, p};
use words::Words;

mod archive;
mod archive_bank;
mod breaks;
mod check;
mod delete;
mod dsa;
mod dsa_runs;
pub(crate) mod expr;
mod front;
pub(crate) mod grading;
mod items;
mod items_ext;
mod language;
mod numbering;
mod pack;
mod scores;
mod series;
mod show;
pub(crate) mod stem_items;
mod structure;
#[cfg(test)]
mod tests_phase3;
#[cfg(test)]
mod tests_phase4;
#[cfg(test)]
mod tests_phase5;
mod total_score;
mod variants_gen;
mod words;
pub use delete::{covered_by_selection, delete_targets};
pub(crate) use delete::{goes_with, refusal, taken_whole, tidy, widen_selection};
pub use expr::TAG_EXPR;
pub use grading::TAG_GRADING;
pub use items::{TAG_ANSWER, TAG_BOX, TAG_ITEM, TAG_POINTS};
pub use items_ext::TAG_RATIONALE;
pub(crate) use language::follow_language;
pub(crate) use show::grid_follows;
pub(crate) use structure::{collapse_empty_part, keeps_edge};

pub const TAG_EXERCISE: &str = "strix:exercise";
/// The title's own control: only the title paragraph, so its placeholder is only the title.
pub const TAG_TITLE: &str = "strix:exercise.title";
pub const TAG_TEXT: &str = "strix:exercise.text";
pub const TAG_QUESTIONS: &str = "strix:exercise.questions";
pub const TAG_SOLUTION: &str = "strix:exercise.solution";
pub const TAG_PROMPT: &str = "strix:exercise.prompt";
pub const TAG_RESULTS: &str = "strix:results";
/// The tag a master document's subdocuments start with (`cmd/master.rs`).
const SUBDOC_TAG: &str = "strix-subdoc";
pub const TAG_DATA: &str = "strix:exercise.data";
/// The frontispiece of the series, at the start of the document.
pub const TAG_FRONT: &str = "strix:frontispiece";
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
const STYLE_FRONT: &str = "ExerciseFrontispiece";
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
        CommandSpec::new("exercise.insert", "New Exercise", "Teaching › Exercise", insert)
            .when(body)
            .params(r#"{"kind"?: "problem" (default) | "open" (open answers) | "choice" (multiple choice) | "trueFalse" | "calc" (calculations) | "drawing" (CAD drawings), "title"?: string, "text"?: string, "statement"?: bool (a quiz's statement placeholder), "questions"?: [string | {"text", "type"?: "open" | "choice" | "trueFalse" | "calc" | "drawing" (this question's mode; default: the kind's), "level"?, "points"?, "height"?: cm, "grid"?: bool, "spacing"?: mm, "choices"?: [string], "correct"?: "B" | 2 | [..], "drawing"?: CAD source (a drawing question's solution; default: the starter drawing)}]} → inserts an exercise after the paragraph at the caret. A problem: title, statement, questions, solution, and the results summary at the end of the document. A quiz (open, choice, trueFalse): title, a statement only when given, each question followed by its answer (a space of `height` cm, squared or blank; or options A), B)… with a box ticked on the correct ones), its points after it"#),
        CommandSpec::new("exercise.questions", "Question Settings", "Teaching › Exercise", items::command)
            .when(|s| if exercise_at_caret(s).is_some() || !scan(s).exercises.is_empty() { None } else { Some("the document has no exercise") })
            .params(r#"{"exercise"?: n (1-based; default: at the caret), "question"?: n (1-based; default: at the caret), "points"?: number | null (the question's maximum points), "height"?: cm (an open answer's space, 0.5–25), "grid"?: bool (squared / blank), "spacing"?: mm (the side of a square), "tolerance"?: string | null (a calc question's acceptable tolerance, e.g. "2%" or "0.1 cm"), "shuffleChoices"?: bool (deterministic per-variant option permutation), "rationales"?: [string | null] (per-option teacher diagnostic notes), "correct"?: "B" | 2 | ["A", "C"] (a multiple choice's correct options)} → changes those given on that question; without them, reads every question → {"exercise", "kind": "problem"|"open"|"choice", "questions": [{"label", "text", "type": "problem"|"open"|"choice"|"calc"|"drawing", "points", "height"?, "grid"?, "spacing"?, "tolerance"?, "line"?, "result"?, "workingPos"?, "scale"?, "cadSelect"?, "trueFalse"?, "shuffleChoices"?, "choices"?, "rationales"?, "correct"?}], "total": points | null}"#),
        CommandSpec::new("exercise.questionMode", "Question Mode", "Teaching › Exercise", items::set_mode)
            .when(|s| if exercise_at_caret(s).is_some() || !scan(s).exercises.is_empty() { None } else { Some("the document has no exercise") })
            .params(r#"{"exercise"?: n (1-based; default: at the caret), "question"?: n (1-based; default: at the caret), "type": "open" | "choice" | "trueFalse" | "calc" | "drawing"} → converts a quiz question's answer part in place while preserving its question paragraph, points, space settings, and existing working/options across compatible modes"#),
        CommandSpec::new("exercise.questionPoints", "Question Points", "Teaching › Exercise", items::set_question_points)
            .when(|s| if exercise_at_caret(s).is_some() || !scan(s).exercises.is_empty() { None } else { Some("the document has no exercise") })
            .params(r#"{"exercise"?: n (1-based; default: at the caret), "question"?: n (1-based; default: at the caret), "points": number | null} → sets or clears the maximum points on a question of any exercise (problem or quiz), feeding the scores table's first criterion"#),
        CommandSpec::new("exercise.addQuestion", "Add Question", "Teaching › Exercise", add_question)
            .when(|s| if exercise_at_caret(s).is_some() { None } else { Some("the cursor isn't in an exercise") })
            .params(r#"{"text"?: string, "level"?: 0–2, "sub"?: bool (one level below the last question); in a quiz "type"?: "open" | "choice" | "trueFalse" | "calc" | "drawing", "points"?, "height"?, "grid"?, "spacing"?, "choices"?, "trueFalse"?: bool, "correct"?, "drawing"? as exercise.insert (default: like the last question)} → adds a question at the end of the questions of the exercise at the caret"#),
        CommandSpec::new("exercise.addNote", "Add Note", "Teaching › Exercise", add_note)
            .when(|s| if exercise_at_caret(s).is_some() { None } else { Some("the cursor isn't in an exercise") })
            .params(r#"{"text"?: string} → adds a note (free text, not a question) at the end of the questions of the exercise at the caret"#),
        CommandSpec::new("equation.answer", "Result Of", "Teaching › Exercise", answer)
            .when(|s| if super::equation::equation_at_caret(s).is_some() { None } else { Some("no equation at the caret") })
            .params(r#"{"question"?: n (1-based; 0 = none; default: the question it is worked under), "toggle"?: bool (a result is taken off)} → the equation at the caret is the result of that question of its exercise"#),
        CommandSpec::new("exercise.info", "Exercise Info", "Teaching › Exercise", info)
            .pure()
            .params(r#"→ {"exercise": n|null, "title", "questions": [string], "labels": ["a)", "a.1)"…], "levels": [n], "answer": n (of the equation at the caret, 0 none), "exercises": n}"#),
        CommandSpec::new("exercise.data", "Exercise Data", "Teaching › Exercise", data_info)
            .pure()
            .params(r#"{"exercise"?: n (1-based; default: at the caret)} → {"exercise", "data": {"items": [{"symbol", "description", "unit", "values": [string per variant]}], "variants": [name], "active": n, "duration"?: minutes}, "errors": [string|null per item], "variantErrors": {variantName: [string]}}"#),
        CommandSpec::new("exercise.setData", "Set Exercise Data", "Teaching › Exercise", set_data)
            .params(r#"{"exercise"?: n, "data"?: {"items": [...], "variants": [...], "active": n, "duration"?: minutes | null}, "duration"?: minutes | null} → stores the exercise's data and variants, then recalculates"#),
        CommandSpec::new("exercise.variant", "Variant", "Teaching › Exercise", set_variant)
            .params(r#"{"exercise"?: n, "active": n (0-based)} → makes that variant the active one"#),
        CommandSpec::new("exercise.generateVariants", "Generate Variants", "Teaching › Exercise", variants_gen::command)
            .when(|s| if exercise_at_caret(s).is_some() || !scan(s).exercises.is_empty() { None } else { Some("the document has no exercise") })
            .params(r#"{"exercise"?: n (1-based; default: at the caret), "count"?: 1–26 (default 4), "seed"?: u64, "domains"?: [{"symbol", "description"?, "unit"?, "values"?: [string], "min"?: number, "max"?: number, "step"?: number}], "constraints"?: {"maxDecimals"?: 0–9, "positiveResults"?: bool, "distinctFinalResults"?: bool, "extra"?: [string]}} → deterministically generates N validated variants within 5000 candidate evaluations"#),
        CommandSpec::new("exercise.blank", "Inline Blank", "Teaching › Exercise", stem_items::blank)
            .params(r#"{"expr": string} → inserts a live strix:blank; students see a blank with the unit, teachers see the calculated answer"#),
        CommandSpec::new("exercise.tableFill", "Student Blank Cell", "Teaching › Exercise", stem_items::table_fill)
            .params(r#"{"studentBlank"?: bool, "column"?: bool} → mark the cell or column at the caret; the table remains calculated"#),
        CommandSpec::new("exercise.insertExpr", "Inline Expression", "Teaching › Exercise", expr::insert_command)
            .when(body)
            .params(r#"{"expr": string (e.g. "v * t -> m")} → inserts an inline strix:expr control at the caret evaluated in the exercise's sandbox (or main worksheet) with its unit in the Unit style"#),
        CommandSpec::new("exercise.delete", "Delete Exercise", "Teaching › Exercise", delete::delete)
            .when(|s| if scan(s).exercises.is_empty() { Some("the document has no exercise") } else { None })
            .params(r#"{"exercise"?: n | [n] (1-based; default: the exercises the selection covers whole, else the one at the caret)} → removes those exercises whole (title, statement, data, questions, solution) in one undo step; an empty paragraph stays where each was; the others renumber and the results summary follows (it goes with the last exercise) → {"deleted": [n], "exercises": n left}"#),
        CommandSpec::new("exercise.numbering", "Exercise Numbering", "Teaching › Exercise", numbering::command)
            .params(r#"{"prefix"?: string | null (null: the default, "Exercise" in the document's language; "": the number alone), "format"?: "decimal" | "upperLetter" | "lowerLetter" | "upperRoman" | "lowerRoman", "start"?: n (1–32767), "separator"?: "." | ":" | "-" | ")" | " " (after the number in the titles; " ": a space alone)} → the numbering of every exercise's label, a document setting (titles and results summary renumber at once); without params, the current one → {"prefix", "format", "start", "separator", "default": bool, "example": "Esercizio 1"}"#),
        CommandSpec::new("exercise.pageBreaks", "Page Breaks", "Teaching › Exercise", breaks::command)
            .when(|s| if scan(s).exercises.is_empty() { Some("the document has no exercise") } else { None })
            .params(r#"{"exercise"?: n (1-based; a lone exercise is the target wherever the caret is), "beforeExercise"?: bool (a new page before each exercise another exercise comes before), "beforeSolution"?: bool (a new page before the solution), "beforeResults"?: bool (a new page before the results summary)} → changes those given; in a series (two or more exercises) every exercise has the same settings, the first one's, and a change applies to all; a new exercise takes the series' → {"exercise" (null in a series), "beforeExercise", "beforeSolution", "beforeResults", "exercises": [{"beforeExercise", "beforeSolution"}]}"#),
        CommandSpec::new("exercise.show", "Parts Shown", "Teaching › Exercise", show::command)
            .when(|s| if scan(s).exercises.is_empty() { Some("the document has no exercise") } else { None })
            .params(r#"{"exercise"?: n (1-based; default: at the caret), "solution"?: bool (the solution: heading, repeated questions, working), "results"?: bool (its lines in the results summary)} → shows or hides those parts (hidden text: nothing is lost); in a series (two or more exercises) every exercise has the same settings, the first one's, and a change applies to all; a new exercise takes the series' → {"series", "solution", "results", "grid", "exercises": [{"solution", "results", "grid"}]}"#),
        CommandSpec::new("exercise.grid", "Exercise Graph Paper", "Teaching › Exercise", show::grid_command)
            .when(|s| if scan(s).exercises.is_empty() { Some("the document has no exercise") } else { None })
            .params(r#"{"exercise"?: n, "on"?: bool (rules graph paper after the questions, before the solution; false takes it away), and graph.set's parameters ("placement" but not "background", "pages", "height", "spacing", "style", "color", "width", "majorEvery", "majorStyle", "majorColor", "majorWidth", "followColumns", "frameWidth")} → in a series (two or more exercises) it applies to every exercise, and a change in the Graph Paper pane to one exercise's graph paper goes to all → as exercise.show, "grid": the first exercise's graph paper as graph.get gives it, or null"#),
        CommandSpec::new("exercise.frontispiece", "Frontispiece", "Teaching › Exercise", front::command)
            .when(|s| if scan(s).exercises.is_empty() { Some("the document has no exercise") } else { None })
            .params(r#"{"on"?: bool} → adds (true) or removes (false) the frontispiece: your own pages at the start of the document (title of the series, class, date…), as many as you like, ended by a Frontispiece Break (a next-page section break marked as the frontispiece's, shown with the formatting marks); added, it is an empty paragraph with the break and the caret in it; removed, everything before the break goes with it → {"on": bool}"#),
        CommandSpec::new("exercise.scores", "Scores Table", "Teaching › Scores", scores::command)
            .when(|s| if scan(s).exercises.is_empty() { Some("the document has no exercise") } else { None })
            .params(r#"{"on"?: bool (insert the scores table after the caret's paragraph, outside the exercises / remove it), "add"?: name | true (a criterion: two columns, one left empty and one for its points), "remove"?: n, "rename"?: n, "name"?: text, "exercise"?: n, "criterion"?: n, "points"?: number | null} → {"on", "criteria": [name], "points": [[number | null]] (per exercise, per criterion), "exercises"}; the rows are the document's exercises, a Total column with more than one criterion and a Total row with more than one exercise"#),
        CommandSpec::new("exercise.totalScore", "Total Score", "Teaching › Scores", total_score::command)
            .params(r#"{"on"?: bool (insert a right-aligned `Punteggio: .......... / 20 pt` line after the caret's paragraph, outside the exercises / remove it), "points"?: number | null (the maximum used when the document's points give none)} → {"on", "points" (the maximum shown), "computed" (the scores table's grand total, else the exercises' question points added up; null without points), "manual"}"#),
        CommandSpec::new("exercise.grading", "Grading Scale", "Teaching › Scores", grading::command)
            .params(r#"{"on"?: bool (insert or remove the strix:grading scale table), "preset"?: "it-10" | "ch-6" | "de-6" | "fr-20" | "percent" | "custom", "maxPoints"?: number | null (null = bound automatically to strix:scores grand total), "minGrade"?: number, "maxGrade"?: number, "passRatio"?: 0.05–0.95, "passGrade"?: number, "curve"?: "linear" | "piecewiseLinear", "step"?: 0.1 | 0.25 | 0.5 | 1.0, "pointsStep"?: number | null, "visibility"?: "teacher" | "both"} → {"on", "config", "maxPoints", "scale": [{"points", "grade", "passing"}]}"#),
        CommandSpec::new("exercise.check", "Check Assessment", "Teaching › Check", check::command)
            .pure()
            .params(r#"→ {"ok": bool, "errors": n, "warnings": n, "infos": n, "issues": [{"kind": "error"|"warning"|"info", "code", "message", "exercise"?, "variant"?, "pos"}]}; inspects all variants A..Z, choices, subdocuments, scores, durations and laid-out answer heights without mutating the session"#),
        CommandSpec::new("file.exportPack", "Export Assessment Pack", "Teaching › Export", pack::command)
            .when(|s| if scan(s).exercises.is_empty() { Some("the document has no exercise") } else { None })
            .params(r#"{"path"?: string (directory or .zip file path), "returnBytes"?: bool (returns base64 zip in zipBase64), "includeDsa"?: bool (default true), "includeTeacher"?: bool (default true)} → exports Student PDFs per variant, DSA Student PDFs, and Teacher Key PDF (with Grading Scale and Multi-Variant Synoptic Table)"#),
        CommandSpec::new("design.dsa", "DSA", "Design › Document Formatting", dsa::command).params(
            r#"{"reading"?: bool (dyslexia: 14 pt, wider letter spacing), "writing"?: bool (dysgraphia: 1 cm graph paper, room under each question), "math"?: bool (dyscalculia, working memory: the data framed), "none"?: true (all off)} → turns those versions on or off; they combine over a common base (13 pt, line spacing 1.5, left aligned, no italics, no hyphenation, each exercise on a new page). Formatting only (styles, hyphenation, the series' page breaks and graph paper), kept in the document so taking it off brings back its own; in a master document the files keep theirs. Without params, the versions on → {"reading", "writing", "math", "on"}"#,
        ),
        CommandSpec::new("archive.scan", "Exercise Bank", "Teaching › Archive", archive_bank::scan).pure()
            .params(r#"{"path": folder, "query"?: string, "subject"?, "grade"?, "tags"?} → {"entries", "errors", "truncated"}; reads only OOXML property parts (at most 1000 files)"#),
        CommandSpec::new("archive.importBundle", "Import Exercise Bundle", "Teaching › Archive", archive_bank::bundle).pure()
            .params(r#"{"data": base64 ZIP, "query"?: string} → {"entries": [{"name", "data", "properties"}], "errors"}; no filesystem extraction, available on the web"#),
        CommandSpec::new("archive.insertCopy", "Insert Exercise Copy", "Teaching › Archive", archive_bank::insert)
            .params(r#"{"path": file.docx, "data"?: base64} → inserts a standalone copy at the selection; use master.insertSubdocument to link a desktop file"#),
        CommandSpec::new("archive.properties", "Archive Properties", "Teaching › Archive", archive::command).params(
            r#"{"grade"?: 1–20, "section"?: text, "profession"?: text, "subject"?: text (the document's subject), "teacher"?: text (the document's author), "duration"?: minutes 1–600} → sets those given (null or empty text removes one) as document properties Word shows too: grade, section, profession, duration and score are custom properties named in the language of the document's exercises (Classe, Sezione, Professione, Durata (min), Punteggio); the score is the scores table's grand total, computed, never set; without params, reads them → {"grade", "section", "profession", "subject", "teacher", "duration", "score", "names": {…}}"#,
        ),
        CommandSpec::new("exercise.update", "Update Summary", "Teaching › Exercise", |s, _| {
            let changed = update(s);
            Ok(json!({"changed": changed}))
        })
        .params(r#"→ rebuilds the questions repeated in each solution and the results summary"#),
    ]
}

/// The language exercise words are written in: the one at the caret, resolved through the styles
/// and the document default — the language the status bar shows and the dictionary checks with —
/// else the editing language (a new document's).
fn words_lang(s: &Session) -> String {
    let style = s.doc.para_at(&s.sel.focus).and_then(|p| p.props.style.clone());
    s.doc.styles.resolve_char(style.as_deref(), &s.typing_props()).lang.or_else(|| crate::cmd::default_lang(s)).unwrap_or_default()
}

/// The words for a language without its own: the document's (its default language's), else the
/// editing language's (a new document's, from the interface), else English.
fn fallback_words(s: &Session) -> &'static Words {
    crate::cmd::doc_lang(&s.doc)
        .as_deref()
        .and_then(words::lookup)
        .or_else(|| s.editing_lang.as_deref().and_then(words::lookup))
        .unwrap_or(&words::EN)
}

/// The words of the language exercises are inserted in: the caret's ([`words_lang`]); for a
/// language without its own words, those the document's exercises already use, else
/// [`fallback_words`].
fn words(s: &Session) -> &'static Words {
    words::lookup(&words_lang(s)).unwrap_or_else(|| {
        let layout = scan(s);
        (0..layout.exercises.len()).find_map(|e| language::present(s, &layout, e)).unwrap_or_else(|| fallback_words(s))
    })
}

/// The words of exercise `e`: its language's ([`language::lang_at`] its title); for a language
/// without its own words, those it already has, so they stay as they are; else
/// [`fallback_words`].
fn exercise_words(s: &Session, layout: &Layout, e: usize) -> &'static Words {
    let lang = layout.exercises.get(e).map(|x| language::lang_at(s, x.start)).unwrap_or_default();
    words::lookup(&lang).or_else(|| language::present(s, layout, e)).unwrap_or_else(|| fallback_words(s))
}

/// The "(numerical solution)" note for top-level block `top`, in that block's or the document's language.
pub(super) fn numeric_note_at(s: &Session, top: usize) -> &'static str {
    words::lookup(&language::lang_at(s, top)).unwrap_or_else(|| fallback_words(s)).numeric_solution
}

/// The words of the document's exercises, for the parts generated for all of them (the results
/// summary, the data, the numbering's default prefix, the scores table): the first exercise's
/// ([`exercise_words`]), else the caret's.
fn doc_words(s: &Session) -> &'static Words {
    let layout = scan(s);
    if layout.exercises.is_empty() { words(s) } else { exercise_words(s, &layout, 0) }
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
            STYLE_FRONT,
            "Exercise Frontispiece",
            ParaProps { align: Some(wordcraft_doc::props::Align::Center), space_before: Some(72.0), space_after: Some(12.0), ..Default::default() },
            CharProps { bold: Some(true), size: Some(20.0), ..Default::default() },
        ),
        para_style(
            items::STYLE_ANSWER,
            "Exercise Answer",
            ParaProps { space_after: Some(6.0), ..Default::default() },
            CharProps { color: Some(items::ANSWER_COLOUR), ..Default::default() },
        ),
        para_style(items::STYLE_CHOICE, "Exercise Option", ParaProps { space_after: Some(2.0), ..Default::default() }, CharProps::default()),
        para_style(
            items::STYLE_LINE,
            "Exercise Result Line",
            ParaProps { space_before: Some(4.0), space_after: Some(6.0), ..Default::default() },
            CharProps { no_proof: Some(true), ..Default::default() },
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
            std::sync::Arc::make_mut(&mut s.doc.styles).styles.push(st);
        }
    }
    // The title numbers itself (Esercizio 1., Esercizio 2. …): one list in the style, so every
    // exercise of the document counts on, also in documents made before it was numbered.
    if s.doc.styles.get(STYLE_TITLE).is_some_and(|st| st.para.numbering.is_none())
        && let Ok(num) = add_title_list(s)
        && let Some(st) = std::sync::Arc::make_mut(&mut s.doc.styles).styles.iter_mut().find(|st| st.id == STYLE_TITLE)
    {
        st.para.numbering = Some(NumRef { num, level: 0 });
    }
}

/// The list exercise titles are numbered with: `Esercizio 1.` (the word in the document's
/// language), a space, then the title.
fn add_title_list(s: &mut Session) -> Result<u32, CmdError> {
    use wordcraft_doc::numbering::{LevelSuffix, levels_for};
    let word = words(s).exercise;
    let mut levels = levels_for(ListKind::Numbered);
    levels.truncate(1);
    if let Some(l) = levels.first_mut() {
        l.text = format!("{word} %1.");
        l.indent = 0.0;
        l.hanging = 0.0;
        l.tab = None;
        l.suffix = LevelSuffix::Space;
    }
    std::sync::Arc::make_mut(&mut s.doc.numbering).try_add_levels(Some("Exercise titles".into()), levels).map_err(|e| CmdError::Failed(e.to_string()))
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
fn body_path(i: usize) -> wordcraft_doc::Path {
    wordcraft_doc::Path(vec![u32::try_from(i).unwrap_or(u32::MAX)])
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
    use wordcraft_doc::numbering::levels_for;
    use wordcraft_doc::section::NumFormat;
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
    std::sync::Arc::make_mut(&mut s.doc.numbering)
        .try_add_levels(Some("Exercise questions".into()), levels)
        .map_err(|e| CmdError::Failed(e.to_string()))
}

fn insert(s: &mut Session, v: &Value) -> CmdResult {
    if s.sel.focus.story != StoryRef::Body || s.sel.focus.path.cell().is_some() {
        return Err(CmdError::Disabled("an exercise goes in the body, outside tables".into()));
    }
    ensure_styles(s);
    if let Some(kind) = p::str(v, "kind").filter(|k| *k != "problem") {
        let (blocks, drawings) = items::exercise_blocks(s, v, kind)?;
        let placed = place(s, blocks)?;
        if drawings.is_empty() {
            return Ok(placed);
        }
        // The drawing questions get their solution's drawing; the caret goes back to the title.
        let caret = s.sel.clone();
        let ex = exercise_at_caret(s).ok_or_else(|| CmdError::Failed("exercise vanished".into()))?;
        items::add_drawings(s, ex, &drawings)?;
        s.sel = caret;
        s.touch();
        update(s);
        s.clamp_selection();
        return super::sel_result(s);
    }
    let w = words(s);
    let title = p::str(v, "title").map(str::to_string);
    let text = p::str(v, "text").map(str::to_string);
    // Questions: strings, or {"text", "level", "points"} for nested / scored ones.
    let comma = scores::comma(s);
    let questions: Vec<(String, u8, Option<f64>)> = v
        .get("questions")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|q| match q {
                    Value::String(t) => Some((t.clone(), 0, None)),
                    Value::Object(_) => Some((
                        p::str(q, "text")?.to_string(),
                        p::u64(q, "level").map_or(0, |l| l.min(u64::from(MAX_LEVEL)) as u8),
                        q.get("points").and_then(Value::as_f64).filter(|n| n.is_finite()),
                    )),
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
    // The title is the user's text, in a control of its own that can't be deleted: its
    // placeholder (only the title, never the whole exercise) is replaced by typing and comes
    // back when the title is deleted.
    let mut exercise = control(TAG_EXERCISE, w.exercise, ControlLock::SdtLocked);
    // Page breaks, parts shown and graph paper as the series (its first exercise) has them.
    breaks::write(&mut exercise, breaks::first(s));
    let (shown, grid) = show::first(s);
    show::write(&mut exercise, shown);
    let mut title_ctl = control(TAG_TITLE, w.title_part, ControlLock::SdtLocked);
    if title.is_none() {
        title_ctl = placeholder(title_ctl, w.title);
    }
    blocks.push(para(STYLE_TITLE, title.as_deref().unwrap_or(w.title), vec![exercise, title_ctl], 1, None));
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
        for (i, (q, level, pts)) in questions.iter().enumerate() {
            let num = Some(NumRef { num: list, level: *level });
            let mut p = para(STYLE_QUESTION, q, opener.take().into_iter().collect(), usize::from(i + 1 == n), num);
            if let Some(pts) = *pts {
                items::set_problem_points_para(&mut p, Some(pts), w, comma);
            }
            blocks.push(p);
        }
    }
    if let Some(g) = grid {
        blocks.push(show::grid_para(g));
    }
    blocks.push(para(STYLE_HEADING, w.solution, vec![control(TAG_SOLUTION, w.solution, ControlLock::SdtLocked)], 0, None));
    // The questions repeated: filled in by `update`.
    blocks.push(para(STYLE_PROMPT, "", vec![control(TAG_PROMPT, w.prompt, ControlLock::SdtContentLocked)], 1, None));
    // The free space of the solution closes the solution and the exercise.
    blocks.push(para("Normal", "", Vec::new(), 2, None));
    place(s, blocks)
}

/// The blocks of a new exercise put after the paragraph at the caret (never inside another
/// exercise or the results summary), the caret in its title.
fn place(s: &mut Session, mut blocks: Vec<Paragraph>) -> CmdResult {
    let top = s.sel.focus.path.0.first().map_or(0, |&t| t as usize);
    let mut at = top.saturating_add(1).min(s.doc.body.len());
    // Never inside another exercise: after it (the caret in one), or before the results
    // summary (the caret in it).
    let layout = scan(s);
    let inside = match layout.parts.get(top) {
        Some((Some(e), _)) => {
            let e = *e;
            at = layout.parts.iter().rposition(|(x, _)| *x == Some(e)).map_or(at, |last| last + 1).min(s.doc.body.len());
            true
        }
        Some((None, Part::Results)) => {
            at = layout.parts.iter().position(|(_, part)| *part == Part::Results).unwrap_or(top);
            true
        }
        _ => false,
    };
    // An empty paragraph at the caret gives way to the exercise (no blank line before the
    // title); a section break it carried moves to the exercise's last paragraph.
    if !inside
        && let Some(Block::Para(p)) = s.doc.body.get(top).map(|b| &**b)
        && p.text.is_empty()
        && p.objects.is_empty()
        // A graph-paper block is an empty paragraph too, and not one to give way.
        && p.props.graph.is_none()
    {
        let section = p.section.clone();
        if let (Some(sec), Some(last)) = (section, blocks.last_mut()) {
            last.section = Some(sec);
        }
        s.doc.body.remove(top);
        at = top;
    }
    for (i, b) in blocks.into_iter().enumerate() {
        s.doc.body.insert(at + i, para_block(b));
    }
    // The caret goes into the title, ready to type.
    s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(StoryRef::Body, body_path(at), 2 * OBJ.len_utf8()));
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
    /// Problem, open answer or multiple choice (docs/exercise.md, Kinds of exercise).
    kind: items::Kind,
    title: String,
    questions: Vec<Question>,
    /// Questions counted per level so far, for the labels.
    counters: [usize; 3],
    /// The data and variants, from the exercise control.
    data: ExerciseData,
    /// Its page breaks, from the exercise control.
    breaks: breaks::Breaks,
    /// The parts that show, from the exercise control.
    show: show::Show,
    /// Its graph paper block, between the questions and the solution.
    grid: Option<usize>,
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
    /// First and last top-level block of the frontispiece: from the document's start to the
    /// paragraph ending in its Frontispiece Break, before any exercise.
    front: Option<(usize, usize)>,
    /// A frontispiece from before the break, in a control (`TAG_FRONT`): first and last block,
    /// and whether it still shows its placeholder. [`front::migrate`] turns it into a break.
    legacy_front: Option<(usize, usize, bool)>,
}

impl Layout {
    /// Block `i`, outside every control (`top`) and before any exercise, ending in the
    /// Frontispiece Break ends the frontispiece (the first such one).
    fn note_front_break(&mut self, i: usize, b: &Block, top: bool) {
        if self.front.is_none()
            && self.exercises.is_empty()
            && top
            && matches!(b, Block::Para(p) if p.section.as_ref().is_some_and(|sp| sp.frontispiece))
        {
            self.front = Some((0, i));
        }
    }

    /// Block `i` (inside the controls `tags`) belongs to an old frontispiece control when it is
    /// in it and in no exercise; the range grows over contiguous blocks only.
    fn note_front(&mut self, i: usize, tags: &[(String, bool)]) {
        let has = |t: &str| tags.iter().any(|(x, _)| x == t);
        if !has(TAG_FRONT) || has(TAG_EXERCISE) {
            return;
        }
        let placeholder = tags.iter().any(|(t, ph)| t == TAG_FRONT && *ph);
        match self.legacy_front {
            None => self.legacy_front = Some((i, i, placeholder)),
            Some((a, b, ph)) if b + 1 == i => self.legacy_front = Some((a, i, ph || placeholder)),
            Some(_) => {}
        }
    }
}

impl Exercise {
    /// Graph paper in no part, before the solution, is the exercise's (the first such block).
    fn note_grid(&mut self, p: &Paragraph, i: usize) {
        if p.props.graph.is_some() && self.solution_blocks.is_none() && self.grid.is_none() {
            self.grid = Some(i);
        }
    }

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
                            out.exercises.push(Exercise {
                                data: read_data(control),
                                breaks: breaks::read(control),
                                show: show::read(control),
                                kind: items::read_kind(control),
                                start: i,
                                ..Exercise::default()
                            });
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
        // A summary inside a subdocument (a master's copy from before subdocuments left theirs
        // out) is not the document's: the master has its own at the end.
        let part = if has(TAG_RESULTS) && !tags.iter().any(|(t, _)| t.starts_with(SUBDOC_TAG)) {
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
        out.note_front(i, &tags);
        out.note_front_break(i, b, stack.is_empty());
        let Some(e) = ex.and_then(|e| out.exercises.get_mut(e)) else { continue };
        if matches!(part, Part::Solution | Part::Prompt) {
            grow(&mut e.solution_blocks);
        }
        match (part, &**b) {
            (Part::Title, Block::Para(p)) => {
                // The title control's placeholder (in documents from before it, the exercise
                // control's) is no title.
                let placeholder = tags.iter().any(|(t, ph)| (t == TAG_EXERCISE || t == TAG_TITLE) && *ph);
                if !placeholder {
                    e.title = plain(p);
                }
            }
            (Part::Questions, Block::Para(p)) => {
                grow(&mut e.question_blocks);
                if tags.iter().any(|(t, ph)| t == TAG_QUESTIONS && *ph) {
                    e.no_questions = true;
                } else if p.props.style.as_deref() == Some(STYLE_QUESTION)
                    && e.questions.len() < MAX_QUESTIONS
                    && !has(items::TAG_ANSWER)
                    && let Some(text) = items::question_text(p).or_else(|| items::problem_question_text(p))
                {
                    // Only question paragraphs count; notes and other content sit between them.
                    // A quiz's question is its item control, empty while its placeholder shows.
                    let level = p.props.numbering.map_or(0, |n| n.level).min(MAX_LEVEL);
                    let label = e.next_label(level);
                    e.questions.push(Question { text: text.chars().take(MAX_TEXT).collect(), level, label });
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
            (Part::Other, Block::Para(p)) => e.note_grid(p, i),
            (Part::Text, _) => grow(&mut e.text_blocks),
            _ => {}
        }
    }
    out
}

fn default_true() -> bool {
    true
}

fn is_true(v: &bool) -> bool {
    *v
}

/// The initial data of an exercise and its variants: each item has one value per variant.
#[derive(Debug, Clone, PartialEq, Hash, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ExerciseData {
    pub items: Vec<DataItem>,
    /// Variant names (`A`, `B`…); at least one when there are items.
    pub variants: Vec<String>,
    /// The active variant (0-based).
    pub active: usize,
    /// Whether the generated `Data` block is shown in the exercise (`true` by default).
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub show_data: bool,
    /// Per-exercise calculation and number-format overrides (`docs/calc.md`).
    #[serde(default, skip_serializing_if = "super::calc::CalcOverrides::is_empty")]
    pub calc: super::calc::CalcOverrides,
    /// Estimated duration of this exercise in minutes (`1..=600`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<u32>,
}

impl Default for ExerciseData {
    fn default() -> Self {
        Self { items: Vec::new(), variants: Vec::new(), active: 0, show_data: true, calc: Default::default(), duration: None }
    }
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
        self.calc.sanitize();
        self.duration = self.duration.filter(|&m| m >= 1).map(|m| m.min(600));
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
        if !self.show_data {
            return Vec::new();
        }
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
    if data.items.is_empty() && data.show_data && data.calc.is_empty() && data.duration.is_none() {
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

/// Title, active variant name (when multi-variant), and jump position of each exercise's data/start.
pub(crate) fn sandbox_meta(s: &Session) -> std::collections::BTreeMap<usize, (String, Option<String>, wordcraft_doc::Pos)> {
    scan(s)
        .exercises
        .into_iter()
        .enumerate()
        .map(|(i, e)| {
            let variant = (e.data.variants.len() > 1).then(|| e.data.variants.get(e.data.active).cloned()).flatten();
            let block = e.data_blocks.map_or(e.start, |(a, _)| a);
            let pos = s.doc.clamp(&wordcraft_doc::Pos::body(block, 0));
            (i, (e.title, variant, pos))
        })
        .collect()
}

/// Per-exercise calculation overrides, keyed by 0-based exercise index.
pub(super) fn exercise_calc(s: &Session) -> std::collections::BTreeMap<usize, super::calc::CalcOverrides> {
    scan(s).exercises.into_iter().enumerate().filter(|(_, e)| !e.data.calc.is_empty()).map(|(i, e)| (i, e.data.calc)).collect()
}

/// Updates the calculation overrides of the exercise named by `v` (or at the caret).
pub(super) fn set_exercise_calc(s: &mut Session, v: &Value, f: impl FnOnce(&mut super::calc::CalcOverrides)) -> Result<usize, CmdError> {
    let ex = target(s, v)?;
    with_data(s, ex, |d| {
        f(&mut d.calc);
        d.calc.sanitize();
    })?;
    Ok(ex)
}

/// The exercise named by `exercise` (1-based) or holding the caret.
fn target(s: &Session, v: &Value) -> Result<usize, CmdError> {
    match p::u64(v, "exercise") {
        Some(n) if n >= 1 => Ok(usize::try_from(n - 1).unwrap_or(usize::MAX)),
        Some(_) => Err(CmdError::Params("exercise: 1-based".into())),
        None => exercise_at_caret(s).ok_or_else(|| CmdError::Disabled("the cursor isn't in an exercise".into())),
    }
}

fn variant_errors_map(s: &Session, layout: &Layout, ex: usize, e: &Exercise) -> serde_json::Map<String, Value> {
    let mut map = serde_json::Map::new();
    let v_count = e.data.variants.len();
    if v_count == 0 {
        return map;
    }
    for (vi, v_name) in e.data.variants.iter().enumerate() {
        let mut errs: Vec<String> = Vec::new();
        let mut z_opts = super::calc::options_now(s);
        e.data.calc.apply_to(&mut z_opts);
        let mut ws_check = strix_calc::Worksheet::new(z_opts);
        let mut stmts = Vec::new();
        for item in &e.data.items {
            let val = item.values.get(vi).cloned().unwrap_or_default();
            if item.symbol.is_empty() || val.is_empty() {
                continue;
            }
            let stmt = format!("{} = {val} {}", item.symbol, item.unit).trim().to_string();
            let r = ws_check.eval_statement(&stmt);
            if r.error.is_none() && r.name.as_deref() != Some(item.symbol.as_str()) {
                errs.push(format!("{} is not a name", item.symbol));
            }
            stmts.push(stmt);
        }
        let eval = super::calc::eval_exercise_variant(s, ex, &stmts);
        for (_, msg) in eval.errors {
            if !errs.contains(&msg) {
                errs.push(msg);
            }
        }
        // Also flag evaluated duplicate options in multiple-choice questions for this variant.
        for (qi, it) in items::items(s, layout, ex).into_iter().enumerate() {
            if it.settings.mode == items::Mode::Choice
                && let Some((a, b)) = it.answer
            {
                let mut seen = std::collections::BTreeSet::new();
                for bi in a..=b {
                    if let Some(p) = s.doc.body.get(bi).and_then(|bl| bl.as_para())
                        && p.props.style.as_deref() == Some(items::STYLE_CHOICE)
                    {
                        let rendered = expr::render_choice_with_sheet(p, &eval.ws);
                        if !rendered.is_empty() && !seen.insert(rendered.clone()) {
                            let msg = format!("question {}: duplicate option '{rendered}'", qi + 1);
                            if !errs.contains(&msg) {
                                errs.push(msg);
                            }
                        }
                    }
                }
            }
        }
        if !errs.is_empty() {
            map.insert(v_name.clone(), json!(errs));
        }
    }
    map
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
    let variant_errors = variant_errors_map(s, &layout, ex, e);
    Ok(json!({"exercise": ex + 1, "data": e.data, "errors": errors, "variantErrors": variant_errors}))
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
    let top_duration = match v.get("duration") {
        Some(Value::Null) => Some(None),
        Some(x) => {
            let n = x.as_u64().ok_or_else(|| CmdError::Params("duration: minutes 1–600 or null".into()))?;
            if !(1..=600).contains(&n) {
                return Err(CmdError::Params("duration: minutes 1–600".into()));
            }
            Some(Some(u32::try_from(n).unwrap_or(600)))
        }
        None => None,
    };
    if v.get("data").is_none()
        && let Some(dur) = top_duration
    {
        return with_data(s, ex, |d| d.duration = dur);
    }
    let has_calc = v.get("data").and_then(|d| d.get("calc")).is_some();
    let has_duration = v.get("data").and_then(|d| d.get("duration")).is_some();
    let new: ExerciseData =
        v.get("data").cloned().map(serde_json::from_value).transpose().map_err(|e| CmdError::Params(format!("data: {e}")))?.unwrap_or_default();
    with_data(s, ex, |d| {
        let keep_calc = d.calc.clone();
        let keep_duration = d.duration;
        *d = new;
        if !has_calc {
            d.calc = keep_calc;
        }
        if let Some(dur) = top_duration {
            d.duration = dur;
        } else if !has_duration {
            d.duration = keep_duration;
        }
    })
}

fn set_variant(s: &mut Session, v: &Value) -> CmdResult {
    let ex = target(s, v)?;
    let active = p::u64(v, "active").ok_or_else(|| CmdError::Params("active: the 0-based variant".into()))?;
    let active = usize::try_from(active).unwrap_or(usize::MAX);
    with_data(s, ex, |d| d.active = active)
}

/// A hash of what the automatic parts show besides results: titles and questions.
pub(crate) fn signature(s: &Session) -> u64 {
    use std::hash::{Hash, Hasher};
    let layout = scan(s);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for e in &layout.exercises {
        (&e.title, &e.questions, e.data.statements(), e.data.show_data, &e.data.calc, e.breaks, e.show, e.grid.is_some()).hash(&mut h);
    }
    layout.results.is_some().hash(&mut h);
    // Points typed in a scores table change its totals.
    scores::fingerprint(s).hash(&mut h);
    grading::fingerprint(s).hash(&mut h);
    // Boxes ticked and the settings of the quizzes' questions.
    items::fingerprint(s).hash(&mut h);
    h.finish()
}

/// The exercise (sandbox) of each top-level body block, `None` outside exercises.
pub(crate) fn zones(s: &Session) -> Vec<Option<usize>> {
    scan(s).parts.into_iter().map(|(e, _)| e).collect()
}

/// Exercises whose solution is hidden (`show.solution == false`), plus whether every exercise in
/// the document hides its solution (for CAD drawings and plots outside any exercise).
pub(crate) fn hidden_solutions(s: &Session) -> (std::collections::BTreeSet<usize>, bool) {
    let layout = scan(s);
    let hidden: std::collections::BTreeSet<usize> = layout.exercises.iter().enumerate().filter(|(_, e)| !e.show.solution).map(|(i, _)| i).collect();
    let all_hidden = !layout.exercises.is_empty() && hidden.len() == layout.exercises.len();
    (hidden, all_hidden)
}

/// The exercise holding the caret.
pub(super) fn exercise_at_caret(s: &Session) -> Option<usize> {
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
    if e.kind.quiz() {
        return items::add_question(s, ex, v);
    }
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
    let pts = v.get("points").and_then(Value::as_f64).filter(|n| n.is_finite());
    let w = exercise_words(s, &layout, ex);
    let comma = scores::comma(s);
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
        let mut p = para(STYLE_QUESTION, &text, opens, ends, numbering);
        let caret = p.text.len() - OBJ.len_utf8() * ends;
        if let Some(pts) = pts {
            items::set_problem_points_para(&mut p, Some(pts), w, comma);
        }
        *prev = p;
        s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(StoryRef::Body, body_path(last), caret));
    } else {
        // The questions control's end moves to the new last question.
        take_last_end(prev)?;
        let numbering = list.map(|num| NumRef { num, level });
        let mut new = para(STYLE_QUESTION, &text, Vec::new(), 1, numbering);
        let caret = new.text.len() - OBJ.len_utf8();
        if let Some(pts) = pts {
            items::set_problem_points_para(&mut new, Some(pts), w, comma);
        }
        s.doc.body.insert(last + 1, para_block(new));
        s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(StoryRef::Body, body_path(last + 1), caret));
    }
    s.touch();
    update(s);
    s.clamp_selection();
    super::sel_result(s)
}

/// Takes the questions control's end off `prev`, the last paragraph of the questions, to close it
/// on a new paragraph after it. Only that marker goes: the question's text keeps its formatting
/// and its tracked changes.
fn take_last_end(prev: &mut Paragraph) -> Result<(), CmdError> {
    let no_end = || CmdError::Failed("the questions part has no end".into());
    let b = prev.text.rfind(OBJ).ok_or_else(no_end)?;
    if !matches!(prev.object_at(b), Some(InlineObject::ControlEnd)) {
        return Err(no_end());
    }
    prev.delete(b, b + OBJ.len_utf8()).map_err(|e| CmdError::Failed(e.to_string()))?;
    prev.touch();
    Ok(())
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
    take_last_end(prev)?;
    let new = para(STYLE_NOTE, &text, Vec::new(), 1, None);
    let caret = new.text.len() - OBJ.len_utf8();
    s.doc.body.insert(last + 1, para_block(new));
    s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(StoryRef::Body, body_path(last + 1), caret));
    s.touch();
    update(s);
    s.clamp_selection();
    super::sel_result(s)
}

fn answer(s: &mut Session, v: &Value) -> CmdResult {
    let at = super::equation::equation_at_caret(s).ok_or_else(|| CmdError::Disabled("no equation at the caret".into()))?;
    let current = match s.doc.para_at(&at).and_then(|p| p.object_at(at.off)) {
        Some(InlineObject::Equation { math, .. }) => math.answer,
        _ => 0,
    };
    // Without a number, the question the equation is worked under (the prompt above it); the
    // Result button (`toggle`) takes a result off again.
    let question = match p::u64(v, "question") {
        Some(q) => q,
        None if p::bool(v, "toggle") == Some(true) && current != 0 => 0,
        None => question_above(s, &at).ok_or_else(|| CmdError::Params("the equation isn't worked under a question of an exercise".into()))?,
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
fn question_above(s: &Session, at: &wordcraft_doc::Pos) -> Option<u64> {
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
                // Its working joins the question above: an empty paragraph meeting another
                // empty one goes, so no blank line is left over (the one holding the
                // solution's end stays).
                let blank = |s: &Session, k: usize, markers: bool| {
                    matches!(s.doc.body.get(k).map(|b| &**b), Some(Block::Para(p)) if p.text.chars().all(|c| c == OBJ)
                        && (markers || p.objects.is_empty())
                        && p.objects.iter().all(|o| matches!(o, InlineObject::ControlEnd))
                        && p.props.style.as_deref().is_none_or(|st| st == "Normal"))
                };
                // The first question's working, with no question above, goes when empty too.
                let heading =
                    i > 0 && matches!(s.doc.body.get(i - 1).map(|b| &**b), Some(Block::Para(p)) if p.props.style.as_deref() == Some(STYLE_HEADING));
                let gone = if i > 0 && blank(s, i - 1, false) && blank(s, i, true) {
                    Some(i - 1)
                } else if heading && blank(s, i, false) {
                    Some(i)
                } else {
                    None
                };
                if let Some(g) = gone {
                    s.doc.body.remove(g);
                    inserts.iter_mut().for_each(|(at, _)| {
                        if *at != usize::MAX && *at > g {
                            *at -= 1;
                        }
                    });
                }
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
            && old.iter().zip(&new).all(|(o, n)| matches!(&**o, Block::Para(p) if p.text == n.text && p.objects == n.objects && p.props == n.props && p.mark.hidden == n.mark.hidden))
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

/// Exercises made before the title had a control of its own showed the title's placeholder on
/// the exercise control, whose content is the whole exercise: typing over it would have replaced
/// the exercise, so it was refused and the title couldn't be edited. Their title paragraph gets
/// its own control, showing the placeholder; the exercise control stops showing one. Returns
/// whether anything changed.
/// The title's paragraph style can't be changed: it is what numbers the title and what marks it
/// as the exercise's title. A paragraph holding a title control that another style reached (Apply
/// Style, the Styles pane, Clear Formatting, Format Painter, a paste) takes Exercise Title back;
/// change the Exercise Title style itself to restyle every title. Whether anything changed.
fn title_styles(s: &mut Session) -> bool {
    let mut changed = false;
    for block in s.doc.body.iter_mut() {
        let Block::Para(p) = &**block else { continue };
        if p.props.style.as_deref() == Some(STYLE_TITLE)
            || !p.objects.iter().any(|o| matches!(o, InlineObject::ControlStart { control } if control.tag == TAG_TITLE))
        {
            continue;
        }
        let Block::Para(p) = Arc::make_mut(block) else { continue };
        p.props.style = Some(STYLE_TITLE.into());
        p.touch();
        changed = true;
    }
    changed
}

fn title_controls(s: &mut Session) -> bool {
    let w = doc_words(s);
    let mut changed = false;
    for block in s.doc.body.iter_mut() {
        let Block::Para(p) = &**block else { continue };
        let markers: Vec<(usize, &InlineObject)> =
            p.objects_with_offsets().filter(|(_, o)| matches!(o, InlineObject::ControlStart { .. } | InlineObject::ControlEnd)).collect();
        // Only the old shape: the exercise control showing a placeholder is the paragraph's only
        // control marker (nothing else opens or closes in the title).
        let [(at, InlineObject::ControlStart { control: ctl })] = markers.as_slice() else { continue };
        if ctl.tag != TAG_EXERCISE || !ctl.showing_placeholder {
            continue;
        }
        let (at, shown) = (*at, ctl.placeholder_shown().to_string());
        let Block::Para(p) = Arc::make_mut(block) else { continue };
        let props = p.props_of_char(at).clone();
        let mut title_ctl = control(TAG_TITLE, w.title_part, ControlLock::SdtLocked);
        title_ctl.placeholder_text = if shown.is_empty() { w.title.to_string() } else { shown };
        title_ctl.showing_placeholder = true;
        // The start right after the exercise's, the end closing the paragraph.
        if p.insert_object(at + OBJ.len_utf8(), InlineObject::ControlStart { control: Box::new(title_ctl) }, &props).is_err() {
            continue;
        }
        let end = p.text.len();
        if p.insert_object(end, InlineObject::ControlEnd, &props).is_err() {
            // Never a start without its end: take it out again.
            let _ = p.delete(at + OBJ.len_utf8(), at + 2 * OBJ.len_utf8());
            continue;
        }
        for o in &mut p.objects {
            if let InlineObject::ControlStart { control: ctl } = o
                && ctl.tag == TAG_EXERCISE
            {
                ctl.showing_placeholder = false;
            }
        }
        changed = true;
    }
    changed
}

/// A saved document keeps a control's placeholder text only while the control shows it, so after
/// reopening, an emptied title, statement or questions part would show the generic "Type here.":
/// the exercise's own words go back on its controls. Nothing shown changes.
fn placeholders(s: &mut Session) {
    let w = doc_words(s);
    let wanted = |tag: &str| match tag {
        TAG_TITLE => Some(w.title),
        TAG_TEXT => Some(w.text),
        TAG_QUESTIONS => Some(w.question),
        _ => None,
    };
    let lacks = |o: &InlineObject| matches!(o, InlineObject::ControlStart { control } if control.placeholder_text.is_empty() && wanted(&control.tag).is_some());
    for block in s.doc.body.iter_mut() {
        if !matches!(&**block, Block::Para(p) if p.objects.iter().any(lacks)) {
            continue;
        }
        let Block::Para(p) = Arc::make_mut(block) else { continue };
        for o in &mut p.objects {
            if let InlineObject::ControlStart { control } = o
                && control.placeholder_text.is_empty()
                && let Some(text) = wanted(&control.tag)
            {
                control.placeholder_text = text.to_string();
            }
        }
    }
}

/// `t` with its first letter upper case (`variante A` → `Variante A`).
fn capitalized(t: &str) -> String {
    let mut c = t.chars();
    c.next().map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
}

/// The first block of the results summary, when the document has one.
/// A control of an exercise that never shows a placeholder when emptied: an answer (an empty
/// space prints empty), the points, an option's box.
pub(crate) fn no_placeholder(tag: &str) -> bool {
    [TAG_ANSWER, TAG_POINTS, TAG_BOX, TAG_EXPR, TAG_RATIONALE, total_score::TAG_SCORE_TOTAL].contains(&tag)
}

pub(crate) fn results_start(s: &Session) -> Option<usize> {
    scan(s).results.map(|(from, _)| from)
}

/// `doc` without its results summary and its frontispiece: a subdocument's content as a master
/// shows it (the master makes one summary for all its exercises, and has its own frontispiece).
pub(crate) fn without_results(doc: Document) -> Document {
    let mut s = Session::new(doc);
    front::migrate(&mut s);
    if let Some((from, to)) = scan(&s).results
        && s.doc.body.get(from..=to).is_some()
    {
        s.doc.body.drain(from..=to);
    }
    if let Some((from, to)) = scan(&s).front
        && s.doc.body.get(from..=to).is_some()
    {
        s.doc.body.drain(from..=to);
    }
    s.doc.ensure_nonempty();
    s.doc
}

/// The series' settings of a document (its first exercise's), to keep over a change that brings
/// in other exercises (a master reloading its subdocuments).
pub(crate) use series::Settings as SeriesSettings;

pub(crate) fn series_settings(s: &Session) -> Option<SeriesSettings> {
    series::settings(s)
}

/// Every exercise takes `set` (when there is one), then the automatic parts are rebuilt.
pub(crate) fn impose_series(s: &mut Session, set: Option<&SeriesSettings>) -> bool {
    match set {
        Some(set) => series::impose(s, set),
        None => update(s),
    }
}

/// A subdocument's file `doc`, written from a master, given back what is the file's own and not
/// the master's (docs/master-documents.md): its styles and document defaults, the exercises'
/// numbering, the series' settings (page breaks, parts shown, graph paper) and its frontispiece,
/// all as in `old`, the file as it was. The content edited in the master stays.
pub(crate) fn keep_own(doc: Document, old: &Document) -> Document {
    let mut own = Session::new(old.clone());
    front::migrate(&mut own);
    let had_exercises = !scan(&own).exercises.is_empty();
    let set = series::settings(&own);
    let numbering = numbering::current(&own);
    let front: Vec<Arc<Block>> = scan(&own).front.and_then(|(a, b)| own.doc.body.get(a..=b)).map(<[Arc<Block>]>::to_vec).unwrap_or_default();
    let mut s = Session::new(doc);
    // The master's DSA formatting stays in the master.
    dsa::leave_out(&mut s.doc, &own.doc);
    // The file's styles, each list a style numbers with still the one this document has.
    std::sync::Arc::make_mut(&mut s.doc.styles).default_chr = own.doc.styles.default_chr.clone();
    std::sync::Arc::make_mut(&mut s.doc.styles).default_para = own.doc.styles.default_para.clone();
    for st in &own.doc.styles.styles {
        let mut st = st.clone();
        st.para.numbering = s.doc.styles.get(&st.id).and_then(|x| x.para.numbering);
        std::sync::Arc::make_mut(&mut s.doc.styles).upsert(st);
    }
    // Its own frontispiece, in place of any.
    if let Some((from, to)) = scan(&s).front
        && s.doc.body.get(from..=to).is_some()
    {
        s.doc.body.drain(from..=to);
    }
    for (k, b) in front.into_iter().enumerate() {
        s.doc.body.insert(k, b);
    }
    if had_exercises {
        let numbering::Numbering { prefix, format, start, separator } = numbering;
        let _ =
            numbering::set(&mut s, numbering::Change { prefix: Some(prefix), format: Some(format), start: Some(start), separator: Some(separator) });
    }
    impose_series(&mut s, set.as_ref());
    s.doc.ensure_nonempty();
    s.doc
}

/// `doc` with its automatic parts up to date, its results summary included: a subdocument's
/// file, written from a master, opened alone.
pub(crate) fn with_results(doc: Document) -> Document {
    let mut s = Session::new(doc);
    update(&mut s);
    s.doc
}

/// Rebuilds the automatic parts: the questions repeated in each solution and the results summary
/// (moved to, or created at, the end of the document). Returns whether anything changed.
pub(crate) fn update(s: &mut Session) -> bool {
    // A frontispiece in a control (documents from before the Frontispiece Break) becomes one.
    let mut changed = front::migrate(s);
    changed |= expr::convert_all_templates(s);
    changed |= title_controls(s);
    changed |= title_styles(s);
    placeholders(s);
    // A series: every exercise has its first one's page breaks, parts shown and graph paper.
    changed |= series::conform(s);
    // The scores tables: one row per exercise, the totals.
    changed |= scores::update(s);
    // The grading scale tables: bound to scores/exercise totals and teacher/both visibility.
    changed |= grading::update(s);
    // The archive's Score property: the scores table's grand total.
    changed |= archive::sync_score(s);
    // The total score lines: the scores table's grand total or the question points.
    changed |= total_score::update(s);
    let layout = scan(s);
    if layout.exercises.is_empty() {
        // No exercise left: the summary goes with the last one.
        let Some((from, to)) = layout.results else { return changed };
        if s.doc.body.get(from..=to).is_none() {
            return changed;
        }
        s.doc.body.drain(from..=to);
        s.doc.ensure_nonempty();
        s.touch();
        s.clamp_selection();
        return true;
    }
    let w = doc_words(s);
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
        let lines = e.data.lines(w);
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
                // After the statement; a quiz without one, after the title.
                let Some(end) = e.text_blocks.map(|(_, end)| end).or_else(|| e.kind.quiz().then_some(e.start)) else { continue };
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
    // Page breaks before the exercises and their solutions, as each one's settings say.
    let layout = scan(s);
    changed |= breaks::apply(s, &layout);
    // A lone exercise's title has no number.
    changed |= numbering::apply_single(s, &layout);
    // Hidden solutions stay hidden (their repeated questions were rebuilt).
    changed |= show::apply(s, &layout);
    // Quizzes: points, answer spaces, boxes, answers shown or hidden, a lone question unlettered.
    changed |= items::update(s);
    // The answers: the values of the result equations, by exercise and question.
    let layout = scan(s);
    let opts = super::calc::options_now(s);
    let mut answers: std::collections::HashMap<(usize, u32), Vec<String>> = std::collections::HashMap::new();
    for (top, question, text) in super::calc::answers(s, &opts) {
        if let Some(ex) = layout.parts.get(top).and_then(|(x, _)| *x) {
            answers.entry((ex, question)).or_default().push(text);
        }
    }
    // Calculation questions: their result lines.
    changed |= items::result_lines(s, &layout, &answers);
    changed |= stem_items::problem_guides(s, &answers);
    stem_items::table_blanks(s);
    let layout = scan(s);
    // Hidden results are hidden text in the summary; the heading too when every one is hidden.
    let mut lines = vec![(STYLE_HEADING, w.results.to_string(), layout.exercises.iter().all(|e| !e.show.results))];
    // Each exercise by its label alone (`Esercizio 1`, as the document numbers them); a lone
    // exercise has no label, so only its variant heads it (when it has several).
    let labels = numbering::current(s);
    let lone = layout.exercises.len() == 1;
    let mut any = false;
    for (n, e) in layout.exercises.iter().enumerate() {
        // A quiz gives its multiple choices' correct options; open answers give nothing.
        let quiz = e.kind.quiz().then(|| items::summary_lines(s, &layout, n, &answers, w.no_result));
        if quiz.as_ref().is_some_and(Vec::is_empty) {
            continue;
        }
        any = true;
        let variant = (e.data.variants.len() > 1).then(|| e.data.variants.get(e.data.active)).flatten();
        let head = match (lone, variant) {
            (false, None) => Some(labels.label(n)),
            (false, Some(name)) => Some(format!("{} ({} {name})", labels.label(n), w.variant)),
            (true, Some(name)) => Some(capitalized(&format!("{} {name}", w.variant))),
            (true, None) => None,
        };
        let hide = !e.show.results;
        if let Some(head) = head {
            lines.push((STYLE_RESULT, head, hide));
        }
        if let Some(quiz) = quiz {
            lines.extend(quiz.into_iter().map(|l| (STYLE_RESULT, l, hide)));
            continue;
        }
        for (k, q) in e.questions.iter().enumerate() {
            let question = u32::try_from(k + 1).unwrap_or(u32::MAX);
            let value = answers.get(&(n, question)).map_or_else(|| w.no_result.to_string(), |found| found.join("; "));
            // Only the question's letter and its result: the question itself is above, in the exercise.
            lines.push((STYLE_RESULT, format!("{} {value}", q.label), hide));
        }
    }
    // Only open answers: nothing to sum up, no summary.
    if !any {
        if let Some((from, to)) = layout.results
            && s.doc.body.get(from..=to).is_some()
        {
            s.doc.body.drain(from..=to);
            s.doc.ensure_nonempty();
            changed = true;
        }
        if changed {
            s.touch();
            s.clamp_selection();
        }
        return changed;
    }
    let ctl = layout
        .results
        .and_then(|(from, _)| opener(s, from, TAG_RESULTS))
        .unwrap_or_else(|| control(TAG_RESULTS, w.results, ControlLock::SdtContentLocked));
    let n = lines.len();
    let mut first = Some(ctl);
    let mut new: Vec<Paragraph> = lines
        .iter()
        .enumerate()
        .map(|(i, (style, text, hide))| {
            let mut p = para(style, text, first.take().into_iter().collect(), usize::from(i + 1 == n), None);
            if *hide {
                for r in &mut p.runs {
                    r.props.hidden = Some(true);
                }
                p.mark.hidden = Some(true);
            }
            p
        })
        .collect();
    // The summary keeps its Page break before (exercise.pageBreaks `beforeResults`).
    let new_page = layout.results.and_then(|(from, _)| s.doc.body.get(from)).and_then(|b| b.as_para()).and_then(|p| p.props.page_break_before);
    if let Some(p) = new.first_mut() {
        p.props.page_break_before = new_page;
    }
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
    use wordcraft_doc::Document;

    fn session() -> Session {
        let mut s = Session::new(Document::new());
        s.run("document.setText", &json!({"text": "Intro"})).unwrap();
        std::sync::Arc::make_mut(&mut s.doc.styles).default_chr.lang = Some("it-IT".into());
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

    /// A new exercise at the caret never takes the graph-paper block the caret is in.
    #[test]
    fn a_new_exercise_never_takes_the_graph_paper_at_the_caret() {
        let mut s = session();
        s.run("document.setText", &json!({"text": ""})).unwrap();
        s.run("graph.insert", &json!({"placement": "pageEnd"})).unwrap();
        assert!(s.doc.body.iter().any(|b| matches!(&**b, Block::Para(p) if p.props.graph.is_some())));
        s.run("exercise.insert", &json!({"title": "Primo", "questions": ["q"]})).unwrap();
        assert!(s.doc.body.iter().any(|b| matches!(&**b, Block::Para(p) if p.props.graph.is_some())), "{:?}", texts(&s));
        assert_eq!(scan(&s).exercises.len(), 1);
    }

    /// Adding a question or a note after the last question keeps that question's formatting.
    #[test]
    fn adding_a_question_or_a_note_keeps_the_last_questions_formatting() {
        for command in ["exercise.addQuestion", "exercise.addNote"] {
            let mut s = session();
            s.run("exercise.insert", &json!({"title": "T", "questions": ["Trova x"]})).unwrap();
            let q = texts(&s).iter().position(|t| t == "Trova x").unwrap();
            let Block::Para(p) = &*s.doc.body[q] else { panic!() };
            let x = p.text.find("x").unwrap();
            let at = |off| wordcraft_doc::Pos::new(StoryRef::Body, body_path(q), off);
            s.sel = crate::Selection { anchor: at(x), focus: at(x + 1) };
            s.run("format.bold", &json!({})).unwrap();
            s.run(command, &json!({"text": "Poi"})).unwrap();
            let Block::Para(p) = &*s.doc.body[q] else { panic!() };
            assert_eq!(p.props_of_char(x).bold, Some(true), "{command}: x stays bold");
            assert_ne!(p.props_of_char(p.text.find("Trova").unwrap()).bold, Some(true), "{command}: the rest stays plain");
            assert_eq!(scan(&s).exercises.len(), 1, "{command}");
            assert!(texts(&s).contains(&"Poi".to_string()), "{command}");
        }
    }

    /// The owner: the paragraph style of the title can't be changed. Another style applied to it
    /// (Apply Style, Normal, a heading) gives Exercise Title back at once, so the title keeps its
    /// number and stays the exercise's title; its text is still free.
    #[test]
    fn the_title_style_cannot_be_changed() {
        let mut s = session();
        s.run("exercise.insert", &json!({"title": "Uno", "questions": ["q"]})).unwrap();
        s.run("caret.docEnd", &json!({})).unwrap();
        s.run("exercise.insert", &json!({"title": "Due", "questions": ["q"]})).unwrap();
        let styles = |s: &Session| -> Vec<Option<String>> {
            texts(s)
                .iter()
                .zip(s.doc.body.iter())
                .filter(|(t, _)| *t == "Uno" || *t == "Due")
                .map(|(_, b)| match &**b {
                    Block::Para(p) => p.props.style.clone(),
                    Block::Table(_) => None,
                })
                .collect()
        };
        for style in ["Normal", "Heading1", "ExerciseQuestion"] {
            s.run("select.text", &json!({"text": "Uno"})).unwrap();
            s.run("para.style", &json!({"style": style})).unwrap();
            assert_eq!(styles(&s), [Some(STYLE_TITLE.to_string()), Some(STYLE_TITLE.to_string())], "{style}");
            let e = scan(&s);
            assert_eq!(e.exercises.len(), 2, "{style}");
            assert_eq!(e.exercises.iter().map(|e| e.questions.len()).collect::<Vec<_>>(), [1, 1], "{style}");
        }
        // Over both titles at once (and the text between): both keep it.
        s.run("select.all", &json!({})).unwrap();
        s.run("para.style", &json!({"style": "Normal"})).unwrap();
        assert_eq!(styles(&s), [Some(STYLE_TITLE.to_string()), Some(STYLE_TITLE.to_string())]);
        assert_eq!(scan(&s).exercises.len(), 2);
    }

    /// An exercise number past what the platform counts never wraps onto another exercise.
    #[test]
    fn a_huge_exercise_number_names_no_exercise() {
        let mut s = session();
        s.run("exercise.insert", &json!({"title": "T"})).unwrap();
        for n in [u64::MAX, 4_294_967_297, 2] {
            assert!(s.run("exercise.variant", &json!({"exercise": n, "active": 0})).is_err(), "{n}");
        }
        assert!(s.run("exercise.variant", &json!({"exercise": 1, "active": u64::MAX})).is_ok());
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
        s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(StoryRef::Body, body_path(9), 0));
        s.run("exercise.insert", &json!({"title": "Secondo", "questions": ["x"]})).unwrap();
        let t = texts(&s);
        assert_eq!(t.iter().filter(|x| *x == "Riepilogo dei risultati").count(), 1);
        assert_eq!(t.last().map(String::as_str), Some("a) risultato non indicato"));
        assert!(t.contains(&"Esercizio 2".to_string()) && !t.iter().any(|x| x.contains(" — ")), "{t:?}");
        let second = t.iter().position(|x| x == "a) x").unwrap();
        assert_eq!(t[second + 1..second + 3], ["", "Riepilogo dei risultati"], "{t:?}");
    }

    /// Exercise titles number themselves through their style: Esercizio 1., Esercizio 2.; a
    /// document whose title style has no numbering gets it with the next exercise.
    #[test]
    fn exercise_titles_are_numbered() {
        let mut s = session();
        s.run("exercise.insert", &json!({"title": "Primo", "questions": ["q"]})).unwrap();
        // Numbering taken away (a document from before): the next insert puts it back.
        if let Some(st) = std::sync::Arc::make_mut(&mut s.doc.styles).styles.iter_mut().find(|st| st.id == STYLE_TITLE) {
            st.para.numbering = None;
        }
        // The caret still in the first exercise's title: the second goes after the first.
        s.run("exercise.insert", &json!({"title": "Secondo", "questions": ["q"]})).unwrap();
        let first = scan(&s);
        assert_eq!(first.exercises.len(), 2);
        let t = texts(&s);
        let (one, two) = (t.iter().position(|x| x == "Primo").unwrap(), t.iter().position(|x| x == "Secondo").unwrap());
        assert!(t[one..two].iter().any(|x| x == "a) q"), "the first is whole before the second: {t:?}");
        // The caret in the results summary: a new exercise goes before it.
        let summary = t.iter().position(|x| x == "Riepilogo dei risultati").unwrap();
        s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(StoryRef::Body, body_path(summary + 1), 0));
        s.run("exercise.insert", &json!({"title": "Terzo", "questions": ["q"]})).unwrap();
        let t = texts(&s);
        assert!(t.iter().position(|x| x == "Terzo").unwrap() < t.iter().position(|x| x == "Riepilogo dei risultati").unwrap(), "{t:?}");
        let mut counters = wordcraft_doc::numbering::Counters::default();
        let labels: Vec<String> = s
            .doc
            .body
            .iter()
            .filter_map(|b| match &**b {
                Block::Para(p) if p.props.style.as_deref() == Some(STYLE_TITLE) => {
                    let n = s.doc.styles.resolve_para(&p.props).numbering?;
                    counters.next_label(&s.doc.numbering, n.num, n.level).map(|(l, _)| l)
                }
                _ => None,
            })
            .collect();
        assert_eq!(labels, ["Esercizio 1.", "Esercizio 2.", "Esercizio 3."]);
    }

    #[test]
    fn an_empty_paragraph_at_the_caret_gives_way_to_the_exercise() {
        let mut s = session();
        s.run("document.setText", &json!({"text": ""})).unwrap();
        s.run("exercise.insert", &json!({"title": "Primo", "questions": ["q"]})).unwrap();
        let t = texts(&s);
        assert_eq!(t.first().map(String::as_str), Some("Primo"), "no blank line before the title: {t:?}");
        assert_eq!(scan(&s).exercises.len(), 1);
        // A paragraph with text stays where it is.
        let mut s = session();
        s.run("exercise.insert", &json!({"title": "Primo", "questions": ["q"]})).unwrap();
        assert_eq!(texts(&s)[..2], ["Intro", "Primo"]);
    }

    #[test]
    fn result_equations_fill_the_summary() {
        let mut s = session();
        s.run("exercise.insert", &json!({"title": "Trave", "questions": ["Momento", "Taglio"]})).unwrap();
        // Type the solution in the free paragraph after the repeated questions.
        let free = texts(&s).iter().position(|t| t == "b) Taglio").unwrap() + 1;
        s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(StoryRef::Body, body_path(free), 0));
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
        assert!(t.contains(&"a) M = 30.375 kN·m".to_string()), "{t:?}");
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
            s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(StoryRef::Body, body_path(at), 0));
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
        s.sel =
            crate::Selection::caret(wordcraft_doc::Pos::new(StoryRef::Body, body_path(texts(&s).iter().position(|t| t == "Seconda").unwrap()), 0));
        s.run("exercise.addQuestion", &json!({"text": "Terza"})).unwrap();
        assert_eq!(solution(&s), ["a) Prima", "lavoro a", "b) Seconda", "lavoro b", "c) Terza", ""]);
        // A question edited in place keeps its working.
        let q = texts(&s).iter().position(|t| t == "Prima").unwrap();
        let end = match &*s.doc.body[q] {
            Block::Para(p) => p.text.len(),
            Block::Table(_) => 0,
        };
        s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(StoryRef::Body, body_path(q), end));
        s.run("text.insert", &json!({"text": " bis"})).unwrap();
        assert_eq!(solution(&s)[..2], ["a) Prima bis", "lavoro a"]);
        // A removed question's prompt goes; the working stays, and the next prompts relabel.
        let q = texts(&s).iter().position(|t| t == "Prima bis").unwrap();
        let end = match &*s.doc.body[q] {
            Block::Para(p) => p.text.len(),
            Block::Table(_) => 0,
        };
        let pos = |off| wordcraft_doc::Pos::new(StoryRef::Body, body_path(q), off);
        s.sel = crate::Selection { anchor: pos(OBJ.len_utf8()), focus: pos(end) };
        s.run("text.delete", &json!({})).unwrap();
        assert_eq!(solution(&s), ["lavoro a", "a) Seconda", "lavoro b", "b) Terza", ""]);
        // An equation worked under a question is its result by default.
        let at = texts(&s).iter().position(|t| t == "lavoro b").unwrap();
        s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(StoryRef::Body, body_path(at), "lavoro b".len()));
        s.run("calc.insert", &json!({"text": "x = 2"})).unwrap();
        assert_eq!(s.run("equation.answer", &json!({})).unwrap()["answer"], json!(1));
        // The Result button: off, then on again (the question it is worked under).
        assert_eq!(s.run("equation.answer", &json!({"toggle": true})).unwrap()["answer"], json!(0));
        assert_eq!(s.run("equation.answer", &json!({"toggle": true})).unwrap()["answer"], json!(1));
    }

    #[test]
    fn an_exercise_is_a_sandbox() {
        let mut s = session();
        s.run("calc.insert", &json!({"text": "a = 2 m"})).unwrap();
        s.run("exercise.insert", &json!({"title": "Uno", "questions": ["q"]})).unwrap();
        let free = texts(&s).iter().position(|t| t == "a) q").unwrap() + 1;
        s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(StoryRef::Body, body_path(free), 0));
        // Inside, the document's `a` is unknown; `b` defined inside stays inside.
        s.run("calc.insert", &json!({"text": "b = 3 m"})).unwrap();
        s.run("text.insert", &json!({"text": " "})).unwrap();
        s.run("calc.insert", &json!({"text": "a ="})).unwrap();
        s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(StoryRef::Body, body_path(0), 0));
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
        s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(StoryRef::Body, body_path(free), 0));
        s.run("calc.insert", &json!({"text": "M = q·L^2/8"})).unwrap();
        let mut at = s.sel.focus.clone();
        at.off -= OBJ.len_utf8();
        s.sel = crate::Selection::caret(at);
        s.run("equation.answer", &json!({"question": 1})).unwrap();
        assert!(texts(&s).contains(&"a) M = 30.375 kN·m".to_string()), "{:?}", texts(&s));
        // Variant B: everything follows.
        s.run("exercise.variant", &json!({"exercise": 1, "active": 1})).unwrap();
        let t = texts(&s);
        assert!(t.contains(&"luce: L = 6 m".to_string()), "{t:?}");
        assert!(t.contains(&"a) M = 45 kN·m".to_string()), "{t:?}");
        // A lone exercise has no label: the variant alone heads its results.
        assert!(t.contains(&"Variante B".to_string()), "{t:?}");
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

    /// The title paragraph's text (without the control markers).
    fn title(s: &Session) -> String {
        s.doc
            .body
            .iter()
            .find_map(|b| match &**b {
                Block::Para(p) if p.props.style.as_deref() == Some(STYLE_TITLE) => Some(plain(p)),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// The first exercise's title as the exercise reads it (Exercise Info, the summary's source).
    fn known_title(s: &Session) -> String {
        scan(s).exercises.first().map(|e| e.title.clone()).unwrap_or_default()
    }

    /// The title is the user's text: typed over its placeholder (New Exercise without a title,
    /// as the ribbon makes it), selected and replaced, deleted with Backspace and Delete; the
    /// structure stays, and the summary, Exercise Info, undo and saving follow it. A title
    /// placeholder on the exercise control (whose content is the whole exercise) made typing try
    /// to replace the whole exercise, which its locked parts refused.
    #[test]
    fn the_title_is_editable_in_place() {
        let mut s = session();
        s.run("exercise.insert", &json!({})).unwrap();
        assert_eq!(title(&s), "Titolo dell'esercizio");
        assert_eq!(s.run("exercise.info", &json!({})).unwrap()["title"], json!(""));
        // Typing at the caret (left in the title) replaces the placeholder, only it.
        s.run("text.insert", &json!({"text": "Trave"})).unwrap();
        assert_eq!(title(&s), "Trave");
        let layout = scan(&s);
        assert_eq!(layout.exercises.len(), 1);
        assert!(
            layout.exercises[0].text_blocks.is_some()
                && layout.exercises[0].question_blocks.is_some()
                && layout.exercises[0].solution_blocks.is_some()
        );
        assert_eq!(s.run("exercise.info", &json!({})).unwrap()["title"], json!("Trave"));
        assert_eq!(known_title(&s), "Trave");
        // Typing goes on at the end, Backspace and Delete take letters away.
        s.run("text.insert", &json!({"text": " appoggiata"})).unwrap();
        s.run("text.backspace", &json!({})).unwrap();
        s.run("caret.home", &json!({})).unwrap();
        s.run("text.delete", &json!({})).unwrap();
        assert_eq!(title(&s), "rave appoggiat");
        // Selected and replaced.
        s.run("select.text", &json!({"text": "rave appoggiat"})).unwrap();
        s.run("text.insert", &json!({"text": "Ponte"})).unwrap();
        assert_eq!(title(&s), "Ponte");
        assert_eq!(known_title(&s), "Ponte");
        // Undo and redo take the title, and the summary, with them.
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(title(&s), "rave appoggiat");
        assert_eq!(known_title(&s), "rave appoggiat");
        s.run("edit.redo", &json!({})).unwrap();
        assert_eq!(title(&s), "Ponte");
        assert_eq!(known_title(&s), "Ponte");
        // Saved and opened again: the title stays, and stays editable.
        let bytes = crate::io::save_bytes("e.xmath", &s.doc).unwrap();
        let mut doc = crate::io::open_bytes("e.xmath", &bytes).unwrap();
        // Italian as it opens (the words come from the preferences, else the document's language).
        std::sync::Arc::make_mut(&mut doc.styles).default_chr.lang = Some("it-IT".into());
        let mut s = Session::new(doc);
        std::sync::Arc::make_mut(&mut s.doc.styles).default_chr.lang = Some("it-IT".into());
        assert_eq!(title(&s), "Ponte");
        let at = s.doc.body.iter().position(|b| matches!(&**b, Block::Para(p) if p.props.style.as_deref() == Some(STYLE_TITLE))).unwrap();
        s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(StoryRef::Body, body_path(at), 2 * OBJ.len_utf8()));
        s.run("text.insert", &json!({"text": "Il "})).unwrap();
        assert_eq!(title(&s), "Il Ponte");
        assert_eq!(known_title(&s), "Il Ponte");
        // All of it deleted: the placeholder comes back, the structure stays.
        s.run("select.text", &json!({"text": "Il Ponte"})).unwrap();
        s.run("text.delete", &json!({})).unwrap();
        assert_eq!(title(&s), "Titolo dell'esercizio");
        assert_eq!(known_title(&s), "");
        assert_eq!(scan(&s).exercises.len(), 1);
        s.run("text.insert", &json!({"text": "Di nuovo"})).unwrap();
        assert_eq!(title(&s), "Di nuovo");
    }

    /// Exercises made before the title had its own control (the placeholder on the exercise
    /// control) get one as they open, so their title can be typed over.
    #[test]
    fn an_old_exercise_gets_its_title_control() {
        let mut s = session();
        s.run("exercise.insert", &json!({})).unwrap();
        // The old shape: the title control taken away, the exercise control showing the placeholder.
        let mut doc = s.doc.clone();
        let at = doc.body.iter().position(|b| matches!(&**b, Block::Para(p) if p.props.style.as_deref() == Some(STYLE_TITLE))).unwrap();
        let Some(Block::Para(p)) = doc.body.get_mut(at).map(Arc::make_mut) else { panic!("no title") };
        let end = p.text.len() - OBJ.len_utf8();
        p.delete(end, p.text.len()).unwrap();
        p.delete(OBJ.len_utf8(), 2 * OBJ.len_utf8()).unwrap();
        for o in &mut p.objects {
            if let InlineObject::ControlStart { control } = o {
                control.placeholder_text = "Titolo dell'esercizio".into();
                control.showing_placeholder = true;
            }
        }
        assert_eq!(p.objects.len(), 1);
        let mut s = Session::new(doc);
        std::sync::Arc::make_mut(&mut s.doc.styles).default_chr.lang = Some("it-IT".into());
        assert!(opener(&s, at, TAG_TITLE).is_some_and(|c| c.showing_placeholder));
        assert!(opener(&s, at, TAG_EXERCISE).is_some_and(|c| !c.showing_placeholder));
        s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(StoryRef::Body, body_path(at), OBJ.len_utf8()));
        s.run("text.insert", &json!({"text": "Vecchio"})).unwrap();
        assert_eq!(title(&s), "Vecchio");
        assert_eq!(known_title(&s), "Vecchio");
        assert_eq!(scan(&s).exercises.len(), 1);
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
        s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(StoryRef::Body, body_path(end), 0));
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

    /// The owner: the placeholders of the statement and of the questions "don't always work",
    /// and the structure must hold. Deleting a selection that empties a part (also of extra
    /// empty paragraphs) brings its placeholder back; Backspace at the start of an empty part
    /// never joins it to the part before (the statement used to end up in the title); questions
    /// removed leave no blank line behind in the solution.
    #[test]
    fn placeholders_come_back_and_parts_never_join() {
        fn part(s: &Session, tag: &str) -> (bool, String) {
            let ranges = s.doc.control_ranges(StoryRef::Body);
            let r = ranges.iter().find(|r| r.control.tag == tag).unwrap();
            (r.control.showing_placeholder, s.doc.control_text(r))
        }
        for (tag, shown) in [(TAG_TEXT, "Scrivi qui il testo dell'esercizio: dati, figure, tabelle."), (TAG_QUESTIONS, "Scrivi qui la domanda.")] {
            let mut s = session();
            s.run("exercise.insert", &json!({})).unwrap();
            let fresh = texts(&s);
            let ranges = s.doc.control_ranges(StoryRef::Body);
            let start = ranges.iter().find(|r| r.control.tag == tag).unwrap().content_start();
            s.sel = crate::Selection::caret(start);
            s.run("text.insert", &json!({"text": "Due"})).unwrap();
            s.run("text.newParagraph", &json!({})).unwrap();
            s.run("text.insert", &json!({"text": "Tre"})).unwrap();
            assert_eq!(part(&s, tag), (false, "Due\nTre".to_string()), "{tag}");
            s.run("select.text", &json!({"text": "Due"})).unwrap();
            s.run("text.delete", &json!({})).unwrap();
            s.run("select.text", &json!({"text": "Tre"})).unwrap();
            s.run("text.delete", &json!({})).unwrap();
            assert_eq!(part(&s, tag), (true, shown.to_string()), "{tag}: the placeholder is back");
            for _ in 0..3 {
                s.run("text.backspace", &json!({})).unwrap();
            }
            for _ in 0..2 {
                s.run("text.delete", &json!({})).unwrap();
            }
            assert_eq!(part(&s, tag), (true, shown.to_string()), "{tag}");
            assert_eq!(texts(&s), fresh, "{tag}: nothing joined, no blank line left");
            assert_eq!(scan(&s).exercises.len(), 1);
        }
        // Delete at the end of the title, Backspace at the start of the questions: refused edges.
        let mut s = session();
        s.run("exercise.insert", &json!({"title": "Uno", "text": "Dati", "questions": ["Trova x"]})).unwrap();
        let fresh = texts(&s);
        s.run("select.text", &json!({"text": "Uno"})).unwrap();
        s.run("caret.end", &json!({})).unwrap();
        s.run("text.delete", &json!({})).unwrap();
        s.run("select.text", &json!({"text": "Trova"})).unwrap();
        s.run("caret.home", &json!({})).unwrap();
        s.run("text.backspace", &json!({})).unwrap();
        assert_eq!(texts(&s), fresh);
    }
}
