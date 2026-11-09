//! Which parts of an exercise show, and its graph paper (docs/exercise.md, Parts shown): the
//! solution (the working) and its lines in the results summary can be hidden, and graph paper can
//! be ruled after the questions, before the solution. With one exercise in the document the
//! settings are that exercise's; in a **series** (two or more) the series rules: every change
//! applies to every exercise, and a new exercise takes the series' settings.
//!
//! The visibility is kept on the exercise control (a `strix:exerciseShow` element Word ignores)
//! and shown as hidden text, so nothing is lost: [`apply`] (run by `update`) hides a hidden
//! solution again after its repeated questions are rebuilt, and the summary writes the lines of
//! an exercise whose results are hidden as hidden text. The graph paper is an ordinary graph
//! paper block (`ParaProps::graph`) between the questions and the solution, so the Graph Paper
//! pane changes it like any other; in a series a change made there goes to every exercise.

use std::sync::Arc;

use serde_json::{Value, json};
use wordcraft_doc::control::ContentControl;
use wordcraft_doc::graph_paper::{GraphPaper, GridPlacement};
use wordcraft_doc::{Block, InlineObject, para_block};

use super::{Exercise, Layout, TAG_EXERCISE, para, scan, target, update};
use crate::{CmdError, CmdResult, Session, p};

const EL: &str = "strix:exerciseShow";

/// The parts of an exercise that show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Show {
    /// The solution: its heading, the repeated questions and the working.
    pub solution: bool,
    /// Its lines in the results summary.
    pub results: bool,
}

impl Default for Show {
    fn default() -> Self {
        Show { solution: true, results: true }
    }
}

/// The settings kept on an exercise control.
pub(crate) fn read(ctl: &ContentControl) -> Show {
    let open = format!("<{EL} ");
    let Some(el) = ctl.extra.iter().find(|x| x.starts_with(&open)) else { return Show::default() };
    let off = |name: &str| el.contains(&format!("{name}=\"0\""));
    Show { solution: !off("solution"), results: !off("results") }
}

/// Keep `sh` on an exercise control (nothing when everything shows).
pub(crate) fn write(ctl: &mut ContentControl, sh: Show) {
    let open = format!("<{EL} ");
    ctl.extra.retain(|x| !x.starts_with(&open));
    if sh != Show::default() {
        ctl.extra.push(format!("<{EL} solution=\"{}\" results=\"{}\"/>", u8::from(sh.solution), u8::from(sh.results)));
    }
}

/// What a new exercise starts with: the settings and the graph paper of the document's last one.
pub(crate) fn last(s: &Session) -> (Show, Option<Arc<GraphPaper>>) {
    let layout = scan(s);
    let Some(e) = layout.exercises.last() else { return (Show::default(), None) };
    (e.show, grid_of(s, e))
}

/// The graph paper of an exercise.
fn grid_of(s: &Session, e: &Exercise) -> Option<Arc<GraphPaper>> {
    e.grid.and_then(|i| s.doc.body.get(i)).and_then(|b| b.as_para()).and_then(|p| p.props.graph.clone())
}

/// The graph paper block of a new exercise (an empty paragraph).
pub(crate) fn grid_para(g: Arc<GraphPaper>) -> wordcraft_doc::Paragraph {
    let mut p = para("Normal", "", Vec::new(), 0, None);
    p.props.graph = Some(g);
    p
}

/// Hidden text on (or off) every run and paragraph mark of `b`, tables' cells included; whether
/// anything changed. Depth is bounded by the document model (nested tables).
fn set_hidden(b: &mut Arc<Block>, on: bool) -> bool {
    let want = on.then_some(true);
    if !needs(b, want) {
        return false;
    }
    match Arc::make_mut(b) {
        Block::Para(p) => {
            for r in &mut p.runs {
                r.props.hidden = want;
            }
            p.mark.hidden = want;
            p.touch();
        }
        Block::Table(t) => {
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    for b in cell.blocks.iter_mut() {
                        set_hidden(b, on);
                    }
                }
            }
        }
    }
    true
}

/// Whether `b` has a run or mark whose hidden setting isn't `want`.
fn needs(b: &Block, want: Option<bool>) -> bool {
    match b {
        Block::Para(p) => p.mark.hidden != want || p.runs.iter().any(|r| r.props.hidden != want),
        Block::Table(t) => t.rows.iter().any(|r| r.cells.iter().any(|c| c.blocks.iter().any(|b| needs(b, want)))),
    }
}

/// Hide the solutions their exercises hide (again after the repeated questions are rebuilt).
/// Whether anything changed.
pub(crate) fn apply(s: &mut Session, layout: &Layout) -> bool {
    let mut changed = false;
    for e in layout.exercises.iter().filter(|e| !e.show.solution) {
        let Some((from, to)) = e.solution_blocks else { continue };
        for i in from..=to {
            if let Some(b) = s.doc.body.get_mut(i) {
                changed |= set_hidden(b, true);
            }
        }
    }
    changed
}

/// The exercises a setting applies to: every one in a series (the series rules), else the one
/// the params or the caret name.
fn targets(s: &Session, v: &Value, layout: &Layout) -> Result<Vec<usize>, CmdError> {
    if layout.exercises.is_empty() {
        return Err(CmdError::Disabled("the document has no exercise".into()));
    }
    if layout.exercises.len() > 1 {
        return Ok((0..layout.exercises.len()).collect());
    }
    // The only exercise, wherever the cursor is; one named must be it.
    if v.get("exercise").is_some() && target(s, v)? != 0 {
        return Err(CmdError::Params("no such exercise".into()));
    }
    Ok(vec![0])
}

/// Keep `sh` on the exercise control opening in top-level paragraph `start`.
fn set_control(s: &mut Session, start: usize, sh: Show) -> Result<(), CmdError> {
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
    write(ctl, sh);
    p.touch();
    Ok(())
}

/// The state every exercise command of this module returns.
fn state(s: &Session) -> Value {
    let layout = scan(s);
    let first = layout.exercises.first();
    let grid = first.and_then(|e| e.grid.zip(grid_of(s, e))).map(|(i, g)| crate::cmd::graph::to_json(crate::cmd::graph::Target::Para(i), &g));
    json!({
        "series": layout.exercises.len() > 1,
        "solution": first.is_none_or(|e| e.show.solution),
        "results": first.is_none_or(|e| e.show.results),
        "grid": grid,
        "exercises": layout.exercises.iter().map(|e| json!({"solution": e.show.solution, "results": e.show.results, "grid": e.grid.is_some()})).collect::<Vec<_>>(),
    })
}

/// `exercise.show`: show or hide the solution and the results of an exercise (of every one in a
/// series).
pub(crate) fn command(s: &mut Session, v: &Value) -> CmdResult {
    let layout = scan(s);
    let which = targets(s, v, &layout)?;
    let (solution, results) = (p::bool(v, "solution"), p::bool(v, "results"));
    let mut changed = false;
    for &i in &which {
        let Some(e) = layout.exercises.get(i) else { continue };
        let want = Show { solution: solution.unwrap_or(e.show.solution), results: results.unwrap_or(e.show.results) };
        if want == e.show {
            continue;
        }
        set_control(s, e.start, want)?;
        // Shown again: the solution's hidden text goes (`update` hides a hidden one).
        if want.solution
            && !e.show.solution
            && let Some((from, to)) = e.solution_blocks
        {
            for k in from..=to {
                if let Some(b) = s.doc.body.get_mut(k) {
                    set_hidden(b, false);
                }
            }
        }
        changed = true;
    }
    if changed {
        s.touch();
        update(s);
        s.clamp_selection();
    }
    Ok(state(s))
}

/// `exercise.grid`: rule graph paper after the questions of an exercise (of every one in a
/// series), take it away, or change it with the graph paper parameters.
pub(crate) fn grid_command(s: &mut Session, v: &Value) -> CmdResult {
    let layout = scan(s);
    let which = targets(s, v, &layout)?;
    let on = p::bool(v, "on");
    if on.is_none() && !which.iter().any(|&i| layout.exercises.get(i).is_some_and(|e| e.grid.is_some())) && has_grid_params(v) {
        return Err(CmdError::Disabled("the exercise has no graph paper (\"on\": true rules it)".into()));
    }
    // The template a new block starts from: the series' (any exercise's) graph paper, else the default.
    let base = layout.exercises.iter().find_map(|e| grid_of(s, e)).map_or_else(GraphPaper::default, |g| *g);
    let fresh = in_flow(crate::cmd::graph::apply(base, v)?)?;
    let mut changed = false;
    // Last exercise first, so earlier block indices stay valid.
    for &i in which.iter().rev() {
        let Some(e) = layout.exercises.get(i) else { continue };
        match (e.grid, on) {
            (Some(at), Some(false)) => changed |= remove_grid(s, at),
            (Some(at), _) => {
                let Some(old) = grid_of(s, e) else { continue };
                let g = in_flow(crate::cmd::graph::apply(*old, v)?)?;
                changed |= put(s, at, g);
            }
            (None, Some(true)) => {
                // After the questions (and the notes between them), before the solution.
                let Some(at) = e.solution_blocks.map(|(from, _)| from) else { continue };
                s.doc.body.insert(at, para_block(grid_para(Arc::new(fresh))));
                changed = true;
            }
            (None, _) => {}
        }
    }
    if changed {
        s.touch();
        update(s);
        s.clamp_selection();
    }
    Ok(state(s))
}

/// Whether `v` names a graph paper parameter (anything but the exercise and the switch).
fn has_grid_params(v: &Value) -> bool {
    v.as_object().is_some_and(|o| o.keys().any(|k| k != "on" && k != "exercise"))
}

/// An exercise's graph paper stays in the text: a background would leave the exercise.
fn in_flow(g: GraphPaper) -> Result<GraphPaper, CmdError> {
    if g.placement == GridPlacement::Background {
        return Err(CmdError::Params("an exercise's graph paper is in the text: placement can't be background".into()));
    }
    Ok(g)
}

/// Graph paper `g` on top-level paragraph `at`; whether it changed.
fn put(s: &mut Session, at: usize, g: GraphPaper) -> bool {
    let Some(block) = s.doc.body.get_mut(at) else { return false };
    if matches!(&**block, Block::Para(p) if p.props.graph.as_deref() == Some(&g)) {
        return false;
    }
    let Block::Para(p) = Arc::make_mut(block) else { return false };
    p.props.graph = Some(Arc::new(g));
    p.touch();
    true
}

/// The graph paper block at `at` goes: with its paragraph when empty, else only the grid.
fn remove_grid(s: &mut Session, at: usize) -> bool {
    let Some(block) = s.doc.body.get_mut(at) else { return false };
    let empty = matches!(&**block, Block::Para(p) if p.text.is_empty());
    if empty {
        s.doc.body.remove(at);
        return true;
    }
    let Block::Para(p) = Arc::make_mut(block) else { return false };
    p.props.graph = None;
    p.touch();
    true
}

/// After `graph.set` changed the graph paper block at top-level paragraph `at`: when it is an
/// exercise's and the document has a series, every exercise's graph paper takes it (the series
/// rules).
pub(crate) fn grid_follows(s: &mut Session, at: usize) {
    let layout = scan(s);
    if layout.exercises.len() < 2 || !layout.exercises.iter().any(|e| e.grid == Some(at)) {
        return;
    }
    let Some(g) = s.doc.body.get(at).and_then(|b| b.as_para()).and_then(|p| p.props.graph.as_deref().copied()) else { return };
    for e in &layout.exercises {
        if let Some(i) = e.grid.filter(|&i| i != at) {
            put(s, i, g);
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wordcraft_doc::{Block, Document};

    use super::super::{STYLE_HEADING, STYLE_PROMPT, STYLE_RESULT, STYLE_TITLE};
    use crate::Session;

    fn run(s: &mut Session, id: &str, v: serde_json::Value) -> serde_json::Value {
        s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
    }

    fn session() -> Session {
        let mut s = Session::new(Document::new());
        run(&mut s, "document.setText", json!({"text": "Intro"}));
        s.doc.styles.default_chr.lang = Some("it-IT".into());
        s
    }

    fn add(s: &mut Session, title: &str) {
        run(s, "caret.docEnd", json!({}));
        run(s, "exercise.insert", json!({"title": title, "questions": ["q"]}));
    }

    /// Whether the paragraphs in `style` are hidden text, in order.
    fn hidden(s: &Session, style: &str) -> Vec<bool> {
        s.doc
            .body
            .iter()
            .filter_map(|b| b.as_para())
            .filter(|p| p.props.style.as_deref() == Some(style))
            .map(|p| p.mark.hidden == Some(true) && p.runs.iter().all(|r| r.props.hidden == Some(true)))
            .collect()
    }

    /// The square side (mm) of the top-level paragraphs with graph paper.
    fn grids(s: &Session) -> Vec<f32> {
        let mm = |pt: f32| (pt / wordcraft_doc::graph_paper::PT_PER_MM * 10.0).round() / 10.0;
        s.doc.body.iter().filter_map(|b| b.as_para()).filter_map(|p| p.props.graph.as_ref().map(|g| mm(g.spacing))).collect()
    }

    /// The owner: "choose which parts show — results, working — with toggles, for a lone exercise
    /// and for the series; in a series the series rules". Hidden text, so nothing is lost.
    #[test]
    fn solution_and_results_can_be_hidden() {
        let mut s = session();
        add(&mut s, "Uno");
        // A lone exercise: its own settings.
        let r = run(&mut s, "exercise.show", json!({"solution": false}));
        assert_eq!((r["series"].as_bool(), r["solution"].as_bool(), r["results"].as_bool()), (Some(false), Some(false), Some(true)), "{r}");
        assert_eq!(hidden(&s, STYLE_HEADING), [true, false], "the solution's heading hidden, the summary's not");
        assert_eq!(hidden(&s, STYLE_PROMPT), [true]);
        // Rebuilt repeated questions stay hidden.
        run(&mut s, "exercise.addQuestion", json!({"text": "altra"}));
        assert_eq!(hidden(&s, STYLE_PROMPT), [true, true]);
        // Working typed before it was hidden is hidden too, and comes back with it.
        run(&mut s, "exercise.show", json!({"solution": true}));
        assert_eq!(hidden(&s, STYLE_PROMPT), [false, false]);
        assert_eq!(hidden(&s, STYLE_HEADING), [false, false]);
        // Results: the exercise's lines in the summary, and the heading when none shows.
        run(&mut s, "exercise.show", json!({"results": false}));
        assert!(hidden(&s, STYLE_RESULT).iter().all(|h| *h), "{:?}", hidden(&s, STYLE_RESULT));
        assert_eq!(hidden(&s, STYLE_HEADING), [false, true]);
        // A series: a new exercise takes the settings; a change goes to every exercise, even
        // with another one named.
        add(&mut s, "Due");
        let r = run(&mut s, "exercise.show", json!({}));
        assert_eq!(r["exercises"], json!([{"solution": true, "results": false, "grid": false}, {"solution": true, "results": false, "grid": false}]));
        let r = run(&mut s, "exercise.show", json!({"exercise": 1, "solution": false, "results": true}));
        assert_eq!(r["exercises"], json!([{"solution": false, "results": true, "grid": false}, {"solution": false, "results": true, "grid": false}]));
        assert_eq!(hidden(&s, STYLE_HEADING), [true, true, false]);
        assert!(hidden(&s, STYLE_RESULT).iter().all(|h| !h));
        // One undo step; saved and opened again.
        assert!(s.undo());
        assert_eq!(hidden(&s, STYLE_HEADING), [false, false, true]);
        assert!(s.redo());
        for name in ["e.xmath", "e.docx"] {
            let bytes = crate::io::save_bytes(name, &s.doc).unwrap();
            let mut back = Session::new(crate::io::open_bytes(name, &bytes).unwrap());
            let r = run(&mut back, "exercise.show", json!({}));
            assert_eq!((r["solution"].as_bool(), r["results"].as_bool()), (Some(false), Some(true)), "{name}: {r}");
        }
    }

    /// A hidden solution still gives its results to the summary (the answer key without the working).
    #[test]
    fn a_hidden_solution_still_gives_its_results() {
        let mut s = Session::new(Document::new());
        run(&mut s, "file.new", json!({"template": "calc"}));
        let results = |s: &Session| -> Vec<String> {
            s.doc
                .body
                .iter()
                .filter_map(|b| b.as_para())
                .filter(|p| p.props.style.as_deref() == Some(STYLE_RESULT))
                .map(super::super::plain)
                .collect()
        };
        let before = results(&s);
        assert!(before.iter().any(|t| t.contains('=')), "{before:?}");
        run(&mut s, "exercise.show", json!({"exercise": 1, "solution": false}));
        assert_eq!(results(&s), before);
    }

    /// The owner: graph paper for each exercise after the statement and the questions, before
    /// the solution, set like the graph paper command; in a series for every exercise.
    #[test]
    fn graph_paper_between_the_questions_and_the_solution() {
        let mut s = session();
        add(&mut s, "Uno");
        assert!(s.run("exercise.grid", &json!({"spacing": 4})).is_err(), "no graph paper to change yet");
        // A lone exercise is the target wherever the cursor is (the ribbon shows its state).
        s.run("caret.docEnd", &json!({})).unwrap();
        assert_eq!(run(&mut s, "exercise.show", json!({}))["solution"], json!(true));
        assert!(s.run("exercise.show", &json!({"exercise": 2})).is_err());
        let r = run(&mut s, "exercise.grid", json!({"on": true, "spacing": 4}));
        assert_eq!(r["grid"]["spacing"].as_f64(), Some(4.0), "{r}");
        // Right before the solution's heading, inside the exercise.
        let at = s.doc.body.iter().position(|b| b.as_para().is_some_and(|p| p.props.graph.is_some())).unwrap();
        assert_eq!(s.doc.body[at + 1].as_para().and_then(|p| p.props.style.clone()).as_deref(), Some(STYLE_HEADING));
        assert_eq!(super::super::scan(&s).parts[at].0, Some(0));
        // A background would leave the exercise.
        assert!(s.run("exercise.grid", &json!({"placement": "background"})).is_err());
        // A second exercise takes it; a change goes to both.
        add(&mut s, "Due");
        assert_eq!(grids(&s), [4.0, 4.0]);
        run(&mut s, "exercise.grid", json!({"spacing": 6}));
        assert_eq!(grids(&s), [6.0, 6.0]);
        // The Graph Paper pane (graph.set) on one exercise's graph paper: the series follows.
        let second =
            s.doc.body.iter().enumerate().filter(|(_, b)| b.as_para().is_some_and(|p| p.props.graph.is_some())).nth(1).map(|(i, _)| i).unwrap();
        s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(wordcraft_doc::StoryRef::Body, super::super::body_path(second), 0));
        run(&mut s, "graph.set", json!({"spacing": 5, "majorEvery": 2}));
        assert_eq!(grids(&s), [5.0, 5.0]);
        // Off for the series; deleting an exercise takes its graph paper with it.
        run(&mut s, "exercise.grid", json!({"on": false}));
        assert!(grids(&s).is_empty());
        assert_eq!(super::super::scan(&s).exercises.len(), 2);
        run(&mut s, "exercise.grid", json!({"on": true}));
        assert_eq!(grids(&s), [5.0, 5.0], "the last settings come back");
        run(&mut s, "exercise.delete", json!({"exercise": 1}));
        assert_eq!(grids(&s), [5.0]);
        assert!(s.doc.body.iter().filter_map(|b| b.as_para()).any(|p| p.props.style.as_deref() == Some(STYLE_TITLE)));
    }

    #[test]
    fn hostile_params_never_panic() {
        let mut s = session();
        for v in [json!({"exercise": 99, "solution": false}), json!({"solution": "x"}), json!({"exercise": -1})] {
            let _ = s.run("exercise.show", &v);
        }
        assert!(s.run("exercise.show", &json!({})).is_err(), "no exercise yet");
        add(&mut s, "Uno");
        for v in
            [json!({"on": "yes"}), json!({"on": true, "spacing": "x"}), json!({"on": true, "pages": 1e30}), json!({"exercise": 1e30, "on": true})]
        {
            let _ = s.run("exercise.grid", &v);
        }
        for v in [json!({"on": 3}), json!({"on": null}), json!([1])] {
            let _ = s.run("exercise.frontispiece", &v);
        }
        assert!(matches!(&*s.doc.body[0], Block::Para(_)));
    }
}
