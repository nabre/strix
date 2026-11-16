//! Every exercise letters its own questions (docs/exercise.md, Structure): the questions start
//! again at a) in each exercise, and each question's options at A). New Exercise gives every
//! exercise lists of its own; a copy of an exercise pasted into the same document comes in with
//! the lists of the one it was copied from, so its questions would go on with c), d).
//! [`conform`] (run by `update`) gives the later exercise lists of its own, however it came in.

use std::collections::BTreeMap;
use std::sync::Arc;

use wordcraft_doc::Block;

use super::{Part, scan};
use crate::Session;

/// Every list numbering the questions (or options) of two exercises stays the first one's; the
/// later exercises get a list of their own, the same look, starting again. Whether anything
/// changed.
pub(super) fn conform(s: &mut Session) -> bool {
    let layout = scan(s);
    if layout.exercises.len() < 2 {
        return false;
    }
    // The exercise each list belongs to (the first one using it), and the lists made here.
    let mut owner: BTreeMap<u32, usize> = BTreeMap::new();
    let mut own: BTreeMap<(usize, u32), u32> = BTreeMap::new();
    let mut changed = false;
    for (i, (ex, part)) in layout.parts.iter().enumerate() {
        let (Some(ex), Part::Questions) = (*ex, *part) else { continue };
        let Some(cur) = s.doc.body.get(i).and_then(|b| b.as_para()).and_then(|p| p.props.numbering).filter(|n| n.num != 0) else { continue };
        if *owner.entry(cur.num).or_insert(ex) == ex {
            continue;
        }
        let num = match own.get(&(ex, cur.num)) {
            Some(n) => *n,
            None => {
                // No list id left: the paragraph keeps its list.
                let Some(n) = Arc::make_mut(&mut s.doc.numbering).restart(cur.num) else { continue };
                own.insert((ex, cur.num), n);
                owner.insert(n, ex);
                n
            }
        };
        let Some(block) = s.doc.body.get_mut(i) else { continue };
        if let Block::Para(p) = Arc::make_mut(block)
            && let Some(n) = p.props.numbering.as_mut()
        {
            n.num = num;
            changed = true;
        }
    }
    if changed {
        s.touch();
    }
    changed
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wordcraft_doc::{Block, Document, Pos};

    use super::super::scan;
    use crate::{Selection, Session};

    fn run(s: &mut Session, id: &str, v: serde_json::Value) -> serde_json::Value {
        s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
    }

    /// The labels of the numbered paragraphs of each exercise, titles left out.
    fn labels(s: &Session) -> Vec<Vec<String>> {
        let layout = scan(s);
        let mut counters = wordcraft_doc::numbering::Counters::default();
        let mut out = vec![Vec::new(); layout.exercises.len()];
        for (i, b) in s.doc.body.iter().enumerate() {
            let Block::Para(p) = &**b else { continue };
            let Some(n) = p.props.numbering.filter(|n| n.num != 0) else { continue };
            let Some((label, _)) = counters.next_label(&s.doc.numbering, n.num, n.level) else { continue };
            if let Some((Some(ex), _)) = layout.parts.get(i) {
                out[*ex].push(label);
            }
        }
        out
    }

    /// Copies exercise `k` whole and pastes it at the end of the document.
    fn paste_copy(s: &mut Session, k: usize) {
        let layout = scan(s);
        let (a, b) = (layout.exercises[k].start, layout.parts.iter().rposition(|(x, _)| *x == Some(k)).unwrap());
        let end = s.doc.body[b].as_para().map_or(0, |p| p.text.len());
        s.sel = Selection { anchor: Pos::body(a, 0), focus: Pos::body(b, end) };
        run(s, "edit.copy", json!({}));
        run(s, "caret.docEnd", json!({}));
        run(s, "edit.paste", json!({}));
    }

    /// The owner: "the numbering of the questions starts again at every exercise".
    #[test]
    fn a_pasted_copy_letters_its_questions_from_a_again() {
        let mut s = Session::new(Document::new());
        run(&mut s, "exercise.insert", json!({"kind": "open", "title": "Uno", "questions": [{"text": "q1"}, {"text": "q2"}]}));
        paste_copy(&mut s, 0);
        assert_eq!(scan(&s).exercises.len(), 2);
        assert_eq!(labels(&s), [["a)", "b)"], ["a)", "b)"]]);
        // A third copy, of the copy; and it holds after an undo and redo of the paste.
        paste_copy(&mut s, 1);
        assert_eq!(labels(&s), [["a)", "b)"], ["a)", "b)"], ["a)", "b)"]]);
        run(&mut s, "edit.undo", json!({}));
        assert_eq!(labels(&s), [["a)", "b)"], ["a)", "b)"]]);
        run(&mut s, "edit.redo", json!({}));
        assert_eq!(labels(&s), [["a)", "b)"], ["a)", "b)"], ["a)", "b)"]]);
        // A question added to the copy goes on in the copy's list only.
        run(&mut s, "exercise.addQuestion", json!({"text": "q3"}));
        assert_eq!(labels(&s), [vec!["a)", "b)"], vec!["a)", "b)"], vec!["a)", "b)", "c)"]]);
    }

    #[test]
    fn a_pasted_copy_letters_its_options_from_a_again() {
        let mut s = Session::new(Document::new());
        let q = |t: &str| json!({"text": t, "choices": ["x", "y"]});
        run(&mut s, "exercise.insert", json!({"kind": "choice", "title": "Uno", "questions": [q("q1"), q("q2")]}));
        let one = ["a)", "A)", "B)", "b)", "A)", "B)"];
        assert_eq!(labels(&s), [one]);
        paste_copy(&mut s, 0);
        assert_eq!(labels(&s), [one, one]);
    }

    #[test]
    fn a_pasted_problem_letters_its_questions_from_a_again() {
        let mut s = Session::new(Document::new());
        run(&mut s, "exercise.insert", json!({"title": "Uno", "text": "t", "questions": ["q1", "q2"]}));
        paste_copy(&mut s, 0);
        assert_eq!(labels(&s), [["a)", "b)"], ["a)", "b)"]]);
    }

    /// Exercises inserted one after the other never shared a list: nothing to do.
    #[test]
    fn exercises_with_their_own_lists_are_left_alone() {
        let mut s = Session::new(Document::new());
        run(&mut s, "exercise.insert", json!({"kind": "open", "title": "Uno", "questions": [{"text": "q1"}, {"text": "q2"}]}));
        run(&mut s, "caret.docEnd", json!({}));
        run(&mut s, "exercise.insert", json!({"kind": "open", "title": "Due", "questions": [{"text": "q3"}, {"text": "q4"}]}));
        assert_eq!(labels(&s), [["a)", "b)"], ["a)", "b)"]]);
        let lists = s.doc.numbering.nums.len();
        assert!(!super::conform(&mut s));
        assert_eq!(s.doc.numbering.nums.len(), lists);
    }
}
