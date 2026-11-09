//! Page breaks around exercises (docs/exercise.md, Page breaks): each exercise can start on a new
//! page when another exercise comes before it, and its solution can start on a new page; the
//! results summary can start on a new page. `exercise.pageBreaks` sets them; the Exercise
//! Properties pane shows them.
//!
//! An exercise's two settings are kept on its control, as a `strix:exerciseBreaks` element Word
//! ignores, and [`apply`] (run by `update`) turns them into Page break before on the title and on
//! the solution's heading, so the condition "another exercise before it" follows exercises added,
//! moved or deleted. The summary's setting is Page break before on its first paragraph itself,
//! which the summary keeps when it is rebuilt. A new exercise takes the settings of the last
//! exercise of the document.

use std::sync::Arc;

use serde_json::{Value, json};
use wordcraft_doc::control::ContentControl;
use wordcraft_doc::{Block, InlineObject};

use super::{Layout, TAG_EXERCISE, scan, target, update};
use crate::{CmdError, CmdResult, Session, p};

const EL: &str = "strix:exerciseBreaks";

/// One exercise's page breaks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Breaks {
    /// A new page before the exercise when another exercise comes before it in the document.
    pub exercise: bool,
    /// A new page before the solution.
    pub solution: bool,
}

/// The breaks kept on an exercise control.
pub(crate) fn read(ctl: &ContentControl) -> Breaks {
    let open = format!("<{EL} ");
    let Some(el) = ctl.extra.iter().find(|x| x.starts_with(&open)) else { return Breaks::default() };
    let on = |name: &str| el.contains(&format!("{name}=\"1\""));
    Breaks { exercise: on("exercise"), solution: on("solution") }
}

/// Keep `b` on an exercise control (nothing when both are off).
pub(crate) fn write(ctl: &mut ContentControl, b: Breaks) {
    let open = format!("<{EL} ");
    ctl.extra.retain(|x| !x.starts_with(&open));
    if b != Breaks::default() {
        ctl.extra.push(format!("<{EL} exercise=\"{}\" solution=\"{}\"/>", u8::from(b.exercise), u8::from(b.solution)));
    }
}

/// The breaks of the document's last exercise: what a new one starts with.
pub(crate) fn last(s: &Session) -> Breaks {
    scan(s).exercises.last().map_or_else(Breaks::default, |e| e.breaks)
}

/// Page break before on the titles and solution headings as the exercises' settings say (an
/// exercise without one before it never breaks). Whether anything changed.
pub(crate) fn apply(s: &mut Session, layout: &Layout) -> bool {
    let mut changed = false;
    for (n, e) in layout.exercises.iter().enumerate() {
        changed |= set_break(s, e.start, e.breaks.exercise && n > 0);
        if let Some((heading, _)) = e.solution_blocks {
            changed |= set_break(s, heading, e.breaks.solution);
        }
    }
    changed
}

/// Page break before on top-level paragraph `i`; whether it changed. Off is no property at all.
fn set_break(s: &mut Session, i: usize, on: bool) -> bool {
    let want = on.then_some(true);
    let Some(block) = s.doc.body.get_mut(i) else { return false };
    if matches!(&**block, Block::Para(p) if p.props.page_break_before == want) {
        return false;
    }
    let Block::Para(p) = Arc::make_mut(block) else { return false };
    p.props.page_break_before = want;
    p.touch();
    true
}

/// Whether the results summary starts on a new page.
fn results_break(s: &Session) -> bool {
    scan(s)
        .results
        .and_then(|(from, _)| s.doc.body.get(from))
        .is_some_and(|b| matches!(&**b, Block::Para(p) if p.props.page_break_before == Some(true)))
}

/// `exercise.pageBreaks`: show or change the page breaks of an exercise (or every one with
/// `all`) and of the results summary.
pub(crate) fn command(s: &mut Session, v: &Value) -> CmdResult {
    let layout = scan(s);
    if layout.exercises.is_empty() {
        return Err(CmdError::Disabled("the document has no exercise".into()));
    }
    let all = p::bool(v, "all") == Some(true);
    let which: Vec<usize> = if all { (0..layout.exercises.len()).collect() } else { vec![target(s, v)?] };
    if which.iter().any(|&i| i >= layout.exercises.len()) {
        return Err(CmdError::Params("no such exercise".into()));
    }
    let (exercise, solution, results) = (p::bool(v, "beforeExercise"), p::bool(v, "beforeSolution"), p::bool(v, "beforeResults"));
    let mut changed = false;
    if exercise.is_some() || solution.is_some() {
        for &i in &which {
            let Some(e) = layout.exercises.get(i) else { continue };
            let mut b = e.breaks;
            b.exercise = exercise.unwrap_or(b.exercise);
            b.solution = solution.unwrap_or(b.solution);
            if b != e.breaks {
                set_control(s, e.start, b)?;
                changed = true;
            }
        }
    }
    if let Some(on) = results
        && let Some((from, _)) = layout.results
    {
        changed |= set_break(s, from, on);
    }
    if changed {
        s.touch();
        update(s);
        s.clamp_selection();
    }
    let layout = scan(s);
    let first = which.first().and_then(|&i| layout.exercises.get(i));
    Ok(json!({
        "exercise": if all { Value::Null } else { json!(which.first().map_or(0, |i| i + 1)) },
        "beforeExercise": first.is_some_and(|e| e.breaks.exercise),
        "beforeSolution": first.is_some_and(|e| e.breaks.solution),
        "beforeResults": results_break(s),
        "exercises": layout.exercises.iter().map(|e| json!({"beforeExercise": e.breaks.exercise, "beforeSolution": e.breaks.solution})).collect::<Vec<_>>(),
    }))
}

/// Keep `b` on the exercise control opening in top-level paragraph `start`.
fn set_control(s: &mut Session, start: usize, b: Breaks) -> Result<(), CmdError> {
    let gone = || CmdError::Failed("exercise vanished".into());
    let block = s.doc.body.get_mut(start).ok_or_else(gone)?;
    let Block::Para(p) = Arc::make_mut(block) else { return Err(gone()) };
    let ctl = p
        .objects
        .iter_mut()
        .find_map(|o| match o {
            InlineObject::ControlStart { control } if control.tag == TAG_EXERCISE => Some(control),
            _ => None,
        })
        .ok_or_else(gone)?;
    write(ctl, b);
    p.touch();
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wordcraft_doc::Document;

    use super::super::{STYLE_HEADING, STYLE_TITLE};
    use crate::Session;

    fn run(s: &mut Session, id: &str, v: serde_json::Value) -> serde_json::Value {
        s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
    }

    /// Page break before of the paragraphs in `style`, in order.
    fn breaks(s: &Session, style: &str) -> Vec<bool> {
        s.doc
            .body
            .iter()
            .filter_map(|b| b.as_para())
            .filter(|p| p.props.style.as_deref() == Some(style))
            .map(|p| p.props.page_break_before == Some(true))
            .collect()
    }

    fn two_exercises() -> Session {
        let mut s = Session::new(Document::new());
        run(&mut s, "document.setText", json!({"text": "Intro"}));
        run(&mut s, "exercise.insert", json!({"title": "One", "questions": ["q"]}));
        run(&mut s, "exercise.insert", json!({"title": "Two", "questions": ["q"]}));
        s
    }

    #[test]
    fn a_new_page_before_an_exercise_only_when_another_comes_before() {
        let mut s = two_exercises();
        assert_eq!(breaks(&s, STYLE_TITLE), [false, false]);
        run(&mut s, "exercise.pageBreaks", json!({"all": true, "beforeExercise": true}));
        assert_eq!(breaks(&s, STYLE_TITLE), [false, true], "the first exercise has none before it");
        // A third one takes the setting; deleting the first moves the condition along.
        run(&mut s, "caret.docEnd", json!({}));
        run(&mut s, "exercise.insert", json!({"title": "Three"}));
        assert_eq!(breaks(&s, STYLE_TITLE), [false, true, true]);
        run(&mut s, "exercise.delete", json!({"exercise": 1}));
        assert_eq!(breaks(&s, STYLE_TITLE), [false, true]);
        // Off again for one exercise; one undo step takes it back.
        run(&mut s, "exercise.pageBreaks", json!({"exercise": 2, "beforeExercise": false}));
        assert_eq!(breaks(&s, STYLE_TITLE), [false, false]);
        assert!(s.undo());
        assert_eq!(breaks(&s, STYLE_TITLE), [false, true]);
    }

    #[test]
    fn solution_and_results_can_start_a_page() {
        let mut s = two_exercises();
        let r = run(&mut s, "exercise.pageBreaks", json!({"exercise": 1, "beforeSolution": true, "beforeResults": true}));
        assert_eq!((r["beforeSolution"].as_bool(), r["beforeResults"].as_bool()), (Some(true), Some(true)), "{r}");
        // Headings: solution 1, solution 2, then the summary's.
        assert_eq!(breaks(&s, STYLE_HEADING), [true, false, true]);
        // The summary keeps it when it is rebuilt.
        run(&mut s, "exercise.addQuestion", json!({"text": "more"}));
        run(&mut s, "exercise.update", json!({}));
        assert_eq!(breaks(&s, STYLE_HEADING).last(), Some(&true));
        // Saved and opened again.
        let bytes = crate::io::save_bytes("x.xmath", &s.doc).unwrap();
        let mut s = Session::new(crate::io::open_bytes("x.xmath", &bytes).unwrap());
        let r = run(&mut s, "exercise.pageBreaks", json!({"exercise": 1}));
        assert_eq!((r["beforeSolution"].as_bool(), r["beforeResults"].as_bool()), (Some(true), Some(true)), "{r}");
        run(&mut s, "exercise.update", json!({}));
        assert_eq!(breaks(&s, STYLE_HEADING), [true, false, true]);
    }
}
