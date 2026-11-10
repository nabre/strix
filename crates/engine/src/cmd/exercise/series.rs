//! A series of exercises (docs/exercise.md, A series): two or more exercises in a document all
//! have the same page breaks, parts shown and graph paper, those of the **first** exercise. The
//! commands change every exercise at once; [`conform`] (run by `update`) brings back in line any
//! exercise that differs however it came in (pasted from another document, an older file, a
//! change made outside the commands), so the series' settings always hold.

use wordcraft_doc::para_block;

use super::{breaks, scan, show};
use crate::Session;

/// Every exercise after the first takes the first one's page breaks, parts shown and graph paper
/// (on, off, and its settings). Whether anything changed. Nothing for a lone exercise.
pub(crate) fn conform(s: &mut Session) -> bool {
    let layout = scan(s);
    let Some(first) = layout.exercises.first() else { return false };
    if layout.exercises.len() < 2 {
        return false;
    }
    let (b, sh, grid) = (first.breaks, first.show, show::grid_of(s, first).map(|g| *g));
    let mut changed = false;
    // Last exercise first, so earlier block indices stay valid.
    for e in layout.exercises.iter().skip(1).rev() {
        if e.breaks != b {
            changed |= breaks::set_control(s, e.start, b).is_ok();
        }
        if e.show != sh {
            changed |= show::set_control(s, e.start, sh).is_ok();
            // Shown again: the solution's hidden text goes (`update` hides a hidden one).
            if sh.solution
                && !e.show.solution
                && let Some((from, to)) = e.solution_blocks
            {
                for k in from..=to {
                    if let Some(block) = s.doc.body.get_mut(k) {
                        show::set_hidden(block, false);
                    }
                }
            }
        }
        // The graph paper sits before the solution, after everything read above.
        match (e.grid, grid) {
            (Some(at), Some(g)) => changed |= show::put(s, at, g),
            (Some(at), None) => changed |= show::remove_grid(s, at),
            (None, Some(g)) => {
                let Some((at, _)) = e.solution_blocks else { continue };
                if at <= s.doc.body.len() {
                    s.doc.body.insert(at, para_block(show::grid_para(std::sync::Arc::new(g))));
                    changed = true;
                }
            }
            (None, None) => {}
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
    use serde_json::json;
    use wordcraft_doc::Document;

    use super::super::{STYLE_HEADING, STYLE_TITLE, scan};
    use crate::Session;

    fn run(s: &mut Session, id: &str, v: serde_json::Value) -> serde_json::Value {
        s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
    }

    fn add(s: &mut Session, title: &str) {
        run(s, "caret.docEnd", json!({}));
        run(s, "exercise.insert", json!({"title": title, "questions": ["q"]}));
    }

    /// Settings of each exercise: (page break before it, before its solution, solution shown,
    /// results shown, graph paper).
    fn settings(s: &Session) -> Vec<(bool, bool, bool, bool, bool)> {
        scan(s).exercises.iter().map(|e| (e.breaks.exercise, e.breaks.solution, e.show.solution, e.show.results, e.grid.is_some())).collect()
    }

    /// The owner: "with many exercises they all have the same configuration, the series': page
    /// breaks, graph paper, parts shown. The series forces every option of the first exercise and
    /// a change is propagated automatically".
    #[test]
    fn a_series_forces_the_first_exercises_settings_on_every_exercise() {
        let mut s = Session::new(Document::new());
        run(&mut s, "document.setText", json!({"text": "Intro"}));
        add(&mut s, "Uno");
        // A lone exercise has its own settings…
        run(&mut s, "exercise.pageBreaks", json!({"beforeSolution": true}));
        run(&mut s, "exercise.show", json!({"results": false}));
        run(&mut s, "exercise.grid", json!({"on": true}));
        // …which a second one takes.
        add(&mut s, "Due");
        add(&mut s, "Tre");
        assert_eq!(settings(&s), vec![(false, true, true, false, true); 3]);
        // A change named for one exercise goes to all of them (the series rules).
        let r = run(&mut s, "exercise.pageBreaks", json!({"exercise": 3, "beforeExercise": true, "beforeSolution": false}));
        assert!(r["exercise"].is_null(), "{r}");
        assert_eq!(settings(&s), vec![(true, false, true, false, true); 3]);
        // Only exercises after the first break before themselves; every solution follows.
        let breaks = |s: &Session, style: &str| -> Vec<bool> {
            s.doc
                .body
                .iter()
                .filter_map(|b| b.as_para())
                .filter(|p| p.props.style.as_deref() == Some(style))
                .map(|p| p.props.page_break_before == Some(true))
                .collect()
        };
        assert_eq!(breaks(&s, STYLE_TITLE), [false, true, true]);
        // An exercise that differs however it came in (here: its control edited behind the
        // commands' back) is brought back in line at the next update.
        let third = scan(&s).exercises[2].start;
        super::super::breaks::set_control(&mut s, third, super::super::breaks::Breaks { exercise: false, solution: true }).unwrap();
        super::super::show::set_control(&mut s, third, super::super::show::Show { solution: false, results: true }).unwrap();
        let grid = scan(&s).exercises[2].grid.unwrap();
        assert!(super::super::show::remove_grid(&mut s, grid));
        assert_ne!(settings(&s)[2], settings(&s)[0]);
        assert!(super::super::update(&mut s));
        assert_eq!(settings(&s), vec![(true, false, true, false, true); 3]);
        assert_eq!(breaks(&s, STYLE_HEADING), [false, false, false, false], "no solution starts a page, the summary neither");
        // Deleting the first exercise: the next one already has the series' settings.
        run(&mut s, "exercise.delete", json!({"exercise": 1}));
        assert_eq!(settings(&s), vec![(true, false, true, false, true); 2]);
    }
}
