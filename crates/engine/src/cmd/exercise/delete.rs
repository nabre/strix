//! Deleting whole exercises (docs/exercise.md). The parts of an exercise are locked against
//! deletion one by one, so an exercise goes only whole: with `exercise.delete` (the exercise at
//! the caret, or those a selection covers), or with Delete, Backspace or Cut over a selection that
//! covers all of it, which then takes its control markers too.

use serde_json::{Value, json};
use strix_doc::control::ControlRange;
use strix_doc::para::OBJ;
use strix_doc::props::{CharProps, ParaProps};
use strix_doc::{InlineObject, Pos, StoryRef};

use super::{
    MAX_EXERCISES, STYLE_DATA, STYLE_HEADING, STYLE_PROMPT, STYLE_RESULT, STYLE_TITLE, TAG_EXERCISE, TAG_RESULTS, exercise_at_caret, scan, update,
};
use crate::{CmdError, CmdResult, Selection, Session};

/// Why a deletion of part of an exercise is refused, and how to delete all of it.
pub(crate) const REFUSED: &str = "The parts of an exercise can't be deleted one by one. To delete the whole exercise, use Delete Exercise (Math › Exercise, or right-click it), or select all of it and press Delete.";

/// The message for a refused deletion touching the control tagged `tag`: [`REFUSED`] for an
/// exercise's parts, none for other controls.
pub(crate) fn refusal(tag: &str) -> Option<&'static str> {
    tag.starts_with(TAG_EXERCISE).then_some(REFUSED)
}

fn le(a: &Pos, b: &Pos) -> bool {
    (&a.path, a.off) <= (&b.path, b.off)
}

/// Just after the end marker of `r`.
fn after(r: &ControlRange) -> Pos {
    Pos { off: r.end.off + OBJ.len_utf8(), ..r.end.clone() }
}

/// The control range of each exercise, in [`scan`] order (`None` for one whose control has no
/// end: a damaged document).
fn ranges(s: &Session) -> Vec<Option<ControlRange>> {
    let all: Vec<ControlRange> = s
        .doc
        .control_ranges(StoryRef::Body)
        .into_iter()
        .filter(|r| r.control.tag == TAG_EXERCISE && r.start.path.0.len() == 1)
        .take(MAX_EXERCISES)
        .collect();
    scan(s).exercises.iter().map(|e| all.iter().find(|r| r.start.path.0.first().map(|&t| t as usize) == Some(e.start)).cloned()).collect()
}

/// Where the exercise's visible content starts: after the control starts at its start (the
/// exercise's own and its title's), where a selection made with the mouse begins.
fn lead(s: &Session, r: &ControlRange) -> Pos {
    let w = OBJ.len_utf8();
    let mut off = r.start.off;
    if let Some(p) = s.doc.para_at(&r.start) {
        for _ in 0..16 {
            if !matches!(p.object_at(off), Some(InlineObject::ControlStart { .. })) {
                break;
            }
            off += w;
        }
    }
    Pos { off, ..r.start.clone() }
}

/// Where its visible content ends: before the control ends closing its last paragraph.
fn tail(s: &Session, r: &ControlRange) -> Pos {
    let w = OBJ.len_utf8();
    let mut off = r.end.off;
    if let Some(p) = s.doc.para_at(&r.end) {
        for _ in 0..16 {
            match off.checked_sub(w) {
                Some(before) if matches!(p.object_at(before), Some(InlineObject::ControlEnd)) => off = before,
                _ => break,
            }
        }
    }
    Pos { off, ..r.end.clone() }
}

/// The exercises (0-based, with their ranges) whose whole visible content `a..b` covers.
fn covered(s: &Session, a: &Pos, b: &Pos) -> Vec<(usize, ControlRange)> {
    if a == b || a.story != StoryRef::Body || b.story != StoryRef::Body {
        return Vec::new();
    }
    ranges(s).into_iter().enumerate().filter_map(|(i, r)| Some((i, r?))).filter(|(_, r)| le(a, &lead(s, r)) && le(&tail(s, r), b)).collect()
}

/// The exercises (0-based) the selection covers whole.
pub fn covered_by_selection(s: &Session) -> Vec<usize> {
    let (a, b) = s.sel.ordered();
    covered(s, &a, &b).into_iter().map(|(i, _)| i).collect()
}

/// The exercises `exercise.delete` removes without a number: those the selection covers whole,
/// else the one at the caret. Empty when there is none.
pub fn delete_targets(s: &Session) -> Vec<usize> {
    let covered = covered_by_selection(s);
    if !covered.is_empty() {
        return covered;
    }
    exercise_at_caret(s).into_iter().collect()
}

/// The spans (start marker to just after the end marker) of the exercise controls a deletion of
/// `a..b` takes whole: their locked parts go with them. The results summary taken whole goes
/// too: it is rebuilt from the exercises left (and goes with the last one).
pub(crate) fn taken_whole(controls: &[ControlRange], a: &Pos, b: &Pos) -> Vec<(Pos, Pos)> {
    controls
        .iter()
        .filter(|r| (r.control.tag == TAG_EXERCISE || r.control.tag == TAG_RESULTS) && le(a, &r.start) && le(&after(r), b))
        .map(|r| (r.start.clone(), after(r)))
        .collect()
}

/// Whether control `r` is inside one of the `spans` [`taken_whole`] found.
pub(crate) fn goes_with(spans: &[(Pos, Pos)], r: &ControlRange) -> bool {
    spans.iter().any(|(a, b)| le(a, &r.start) && le(&after(r), b))
}

/// What a deletion widened over whole exercises does to the paragraph it leaves.
pub(crate) struct Tidy {
    /// The deletion starts at the start of a paragraph: the paragraph left is the one it ends in,
    /// with that paragraph's properties (as when whole paragraphs are deleted).
    joined: Option<(ParaProps, CharProps)>,
}

/// Before deleting the selection: when it covers whole exercises (their visible content, as the
/// mouse or Select All selects them), it grows to take their control markers too, so the
/// exercises go whole instead of being refused by their locks. Returns how to tidy the paragraph
/// left once deleted, `None` when the selection covers no whole exercise.
pub(crate) fn widen_selection(s: &mut Session) -> Option<Tidy> {
    let (a, b) = s.sel.ordered();
    let found = covered(s, &a, &b);
    if found.is_empty() {
        return None;
    }
    let (mut na, mut nb) = (a, b);
    for (_, r) in &found {
        if le(&r.start, &na) {
            na = r.start.clone();
        }
        let end = after(r);
        if le(&nb, &end) {
            nb = end;
        }
    }
    let joined = (na.off == 0 && na.path != nb.path).then(|| s.doc.para_at(&nb).map(|p| (p.props.clone(), p.mark.clone()))).flatten();
    s.sel = if le(&s.sel.anchor, &s.sel.focus) { Selection { anchor: na, focus: nb } } else { Selection { anchor: nb, focus: na } };
    Some(Tidy { joined })
}

/// An exercise's own paragraph style: out of an exercise it means nothing (a title style would
/// even number the paragraph as an exercise).
fn exercise_style(p: &ParaProps) -> bool {
    matches!(p.style.as_deref(), Some(STYLE_TITLE | STYLE_HEADING | STYLE_PROMPT | STYLE_RESULT | STYLE_DATA))
}

/// After a deletion that took whole exercises: the paragraph left at `at` takes the properties
/// [`Tidy`] says, and drops an exercise's style when it is no longer in an exercise.
pub(crate) fn tidy(s: &mut Session, at: &Pos, t: Tidy) {
    if at.story != StoryRef::Body {
        return;
    }
    let top = at.path.0.first().map(|&i| i as usize);
    let in_exercise = top.is_some_and(|top| scan(s).parts.get(top).is_some_and(|(e, _)| e.is_some()));
    let Ok(p) = s.doc.para_mut(at.story, &at.path) else { return };
    if let Some((props, mark)) = t.joined {
        p.props = props;
        p.mark = mark;
    }
    if !in_exercise && exercise_style(&p.props) {
        p.props = ParaProps::default();
        p.mark = CharProps { ins: p.mark.ins, del: p.mark.del, ..CharProps::default() };
    }
    p.touch();
}

/// `exercise.delete`: removes whole exercises, each with everything in it; an empty paragraph
/// stays where each was (text sharing its first or last paragraph stays too). The summary is
/// rebuilt, and goes with the last exercise.
pub(super) fn delete(s: &mut Session, v: &Value) -> CmdResult {
    let ranges = ranges(s);
    let mut targets: Vec<usize> = match v.get("exercise") {
        None | Some(Value::Null) => delete_targets(s),
        Some(Value::Array(a)) => {
            let mut out = Vec::new();
            for n in a.iter().take(MAX_EXERCISES) {
                match n.as_u64() {
                    Some(n) if n >= 1 => out.push(usize::try_from(n - 1).unwrap_or(usize::MAX)),
                    _ => return Err(CmdError::Params("exercise: 1-based numbers".into())),
                }
            }
            out
        }
        Some(n) => match n.as_u64() {
            Some(n) if n >= 1 => vec![usize::try_from(n - 1).unwrap_or(usize::MAX)],
            _ => return Err(CmdError::Params("exercise: a 1-based number, or a list of them".into())),
        },
    };
    if targets.is_empty() {
        return Err(CmdError::Disabled("the cursor isn't in an exercise".into()));
    }
    targets.sort_unstable();
    targets.dedup();
    if targets.iter().any(|&t| t >= ranges.len()) {
        return Err(CmdError::Params("no such exercise".into()));
    }
    // One inside another (a damaged document) goes with the outer one.
    let spans: Vec<(Pos, Pos)> =
        targets.iter().filter_map(|&t| ranges.get(t).and_then(Option::as_ref)).map(|r| (r.start.clone(), after(r))).collect();
    targets.retain(|&t| {
        let Some(r) = ranges.get(t).and_then(Option::as_ref) else { return true };
        !spans.iter().any(|(a, b)| (a, b) != (&r.start, &after(r)) && le(a, &r.start) && le(&after(r), b))
    });
    let mut caret = None;
    // Last first, so the positions of the ones before stay valid.
    for &t in targets.iter().rev() {
        let Some(r) = ranges.get(t).and_then(Option::as_ref) else {
            return Err(CmdError::Failed(format!("exercise {} has no end: its structure is damaged", t + 1)));
        };
        // Text before the exercise in its first paragraph keeps that paragraph's properties;
        // without it, the paragraph left is a plain one.
        let head = r.start.off > 0;
        let at = s.doc.delete_range(&r.start, &after(r))?;
        if !head && let Ok(p) = s.doc.para_mut(at.story, &at.path) {
            p.props = ParaProps::default();
            p.mark = CharProps::default();
            p.touch();
        }
        caret = Some(at);
    }
    if let Some(at) = caret {
        s.sel = Selection::caret(at);
    }
    s.touch();
    update(s);
    s.doc.ensure_nonempty();
    s.clamp_selection();
    let left = scan(s).exercises.len();
    Ok(json!({"deleted": targets.iter().map(|t| t + 1).collect::<Vec<_>>(), "exercises": left}))
}

#[cfg(test)]
mod tests {
    use super::super::{TAG_TITLE, body_path, plain};
    use super::*;
    use strix_doc::{Block, Document};

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

    fn at(i: usize, off: usize) -> Pos {
        Pos::new(StoryRef::Body, body_path(i), off)
    }

    fn len(s: &Session, i: usize) -> usize {
        match s.doc.body.get(i).map(|b| &**b) {
            Some(Block::Para(p)) => p.text.len(),
            _ => 0,
        }
    }

    fn find(s: &Session, text: &str) -> usize {
        texts(s).iter().position(|x| x == text).unwrap()
    }

    fn style(s: &Session, i: usize) -> Option<String> {
        match s.doc.body.get(i).map(|b| &**b) {
            Some(Block::Para(p)) => p.props.style.clone(),
            _ => None,
        }
    }

    /// Two exercises after "Intro" ("Uno", "Due"), and "Fine" between the second and the summary.
    fn two() -> Session {
        let mut s = session();
        s.run(
            "document.setText",
            &json!({"text": "Intro
Fine"}),
        )
        .unwrap();
        s.sel = Selection::caret(at(0, 0));
        s.run("exercise.insert", &json!({"title": "Uno", "text": "Testo uno", "questions": ["q1"]})).unwrap();
        // The caret in the first exercise: the second goes after it.
        s.run("exercise.insert", &json!({"title": "Due", "questions": ["q2"]})).unwrap();
        assert_eq!(scan(&s).exercises.len(), 2, "{:?}", texts(&s));
        assert_eq!(scan(&s).parts.get(find(&s, "Fine")).map(|p| p.0), Some(None), "Fine is outside the exercises");
        assert_eq!(texts(&s)[find(&s, "Fine") + 1], "Riepilogo dei risultati");
        s
    }

    /// The owner couldn't delete an exercise: every part is locked against deletion. Delete
    /// Exercise removes the one at the caret whole, in one undo step; the others renumber and
    /// the summary follows, and goes with the last exercise.
    #[test]
    fn delete_exercise_removes_the_one_at_the_caret() {
        let mut s = two();
        let before = texts(&s);
        // Outside every exercise there is nothing to delete at the caret.
        s.sel = Selection::caret(at(0, 0));
        assert!(matches!(s.run("exercise.delete", &json!({})), Err(CmdError::Disabled(_))));
        assert!(delete_targets(&s).is_empty());
        let statement = find(&s, "Testo uno");
        s.sel = Selection::caret(at(statement, 3));
        assert_eq!(delete_targets(&s), [0]);
        let r = s.run("exercise.delete", &json!({})).unwrap();
        assert_eq!((r["deleted"].clone(), r["exercises"].clone()), (json!([1]), json!(1)));
        let t = texts(&s);
        assert!(!t.iter().any(|x| x == "Uno" || x == "Testo uno" || x == "a) q1"), "{t:?}");
        // An empty plain paragraph where it was, with the caret in it.
        assert_eq!(t[..3], ["Intro", "", "Due"], "{t:?}");
        assert_eq!(s.sel.focus.path, body_path(1));
        assert_eq!(style(&s, 1), None);
        // The one left is exercise 1, whole, and the summary lists only it.
        assert_eq!(s.run("exercise.info", &json!({})).unwrap()["exercises"], json!(1));
        let summary = &t[find(&s, "Riepilogo dei risultati")..];
        assert_eq!(summary.iter().filter(|x| x.starts_with("Esercizio")).count(), 1, "{summary:?}");
        assert!(summary.iter().any(|x| x == "a) risultato non indicato") && !summary.iter().any(|x| x.starts_with("Esercizio 2")), "{summary:?}");
        // Undo brings it back exactly; redo removes it again.
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(texts(&s), before);
        s.run("edit.redo", &json!({})).unwrap();
        assert_eq!(texts(&s)[..3], ["Intro", "", "Due"]);
        // The last one, by number: the summary goes with it.
        s.sel = Selection::caret(at(0, 0));
        s.run("exercise.delete", &json!({"exercise": 1})).unwrap();
        let t = texts(&s);
        assert!(!t.iter().any(|x| x == "Riepilogo dei risultati" || x == "Due"), "{t:?}");
        assert!(scan(&s).exercises.is_empty() && scan(&s).results.is_none());
        assert_eq!(t.first().map(String::as_str), Some("Intro"));
        assert!(t.contains(&"Fine".to_string()));
        assert!(matches!(s.run("exercise.delete", &json!({})), Err(CmdError::Disabled(_))), "no exercise left");
    }

    /// Selecting a whole exercise as the mouse does (from its title's text to the end of its last
    /// paragraph) and pressing Backspace deletes it whole; the paragraph left is a plain one.
    #[test]
    fn backspace_over_a_whole_exercise_deletes_it() {
        let mut s = two();
        let title = find(&s, "Uno");
        let last = find(&s, "Due") - 1;
        // From after the title's control starts (where the text begins) to the end of the text.
        let lead = 2 * OBJ.len_utf8();
        s.sel = Selection { anchor: at(last, len(&s, last)), focus: at(title, lead) };
        assert_eq!(covered_by_selection(&s), [0]);
        s.run("text.backspace", &json!({})).unwrap();
        let t = texts(&s);
        assert_eq!(t[..3], ["Intro", "", "Due"], "{t:?}");
        assert!(matches!(style(&s, 1).as_deref(), None | Some("Normal")), "no exercise title style (and number) left behind: {:?}", style(&s, 1));
        assert_eq!(scan(&s).exercises.len(), 1);
        assert_eq!(s.doc.control_ranges(StoryRef::Body).iter().filter(|r| r.control.tag == TAG_TITLE).count(), 1);
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(scan(&s).exercises.len(), 2);
    }

    /// From the start of the first exercise's paragraph to the start of the next: the next
    /// exercise keeps its title paragraph (as when whole paragraphs are deleted), and becomes 1.
    #[test]
    fn delete_from_paragraph_start_to_the_next_exercise() {
        let mut s = two();
        let (uno, due) = (find(&s, "Uno"), find(&s, "Due"));
        s.sel = Selection { anchor: at(uno, 0), focus: at(due, 0) };
        s.run("text.delete", &json!({})).unwrap();
        let t = texts(&s);
        assert_eq!(t[..2], ["Intro", "Due"], "{t:?}");
        assert_eq!(style(&s, 1).as_deref(), Some(STYLE_TITLE));
        let layout = scan(&s);
        assert_eq!(layout.exercises.len(), 1);
        assert_eq!(layout.exercises[0].title, "Due");
    }

    /// Select All and Delete clears the document, exercises and summary included; Cut over a
    /// whole exercise copies it whole, so Paste brings it back.
    #[test]
    fn select_all_delete_and_cut_take_exercises_whole() {
        let mut s = two();
        s.run("select.all", &json!({})).unwrap();
        assert_eq!(covered_by_selection(&s), [0, 1]);
        s.run("text.delete", &json!({})).unwrap();
        assert_eq!(texts(&s), [""]);
        assert_eq!(style(&s, 0), None);
        assert!(scan(&s).exercises.is_empty() && scan(&s).results.is_none());
        s.run("edit.undo", &json!({})).unwrap();
        // Cut: from the end of "Intro" to the end of the first exercise.
        let last = find(&s, "Due") - 1;
        s.sel = Selection { anchor: at(0, len(&s, 0)), focus: at(last, len(&s, last)) };
        s.run("edit.cut", &json!({})).unwrap();
        assert_eq!(scan(&s).exercises.len(), 1);
        assert!(s.clipboard_text.contains("Uno"), "{}", s.clipboard_text);
        let fine = find(&s, "Fine");
        s.sel = Selection::caret(at(fine, len(&s, fine)));
        s.run("edit.paste", &json!({})).unwrap();
        let layout = scan(&s);
        assert_eq!(layout.exercises.len(), 2, "{:?}", texts(&s));
        assert_eq!(layout.exercises[1].title, "Uno");
    }

    /// A part of an exercise alone is still refused, and the message says how to delete the
    /// whole exercise.
    #[test]
    fn deleting_part_of_an_exercise_says_how_to_delete_it_all() {
        let mut s = two();
        let before = texts(&s);
        let (title, statement) = (find(&s, "Uno"), find(&s, "Testo uno"));
        for (a, b) in [(at(0, 0), at(statement, 4)), (at(title, 0), at(statement, len(&s, statement)))] {
            s.sel = Selection { anchor: a, focus: b };
            assert!(covered_by_selection(&s).is_empty());
            let e = s.run("text.delete", &json!({})).unwrap_err();
            assert!(e.to_string().contains("Delete Exercise"), "{e}");
            assert_eq!(texts(&s), before);
        }
    }

    #[test]
    fn hostile_params_never_panic() {
        let mut s = two();
        for v in [
            json!({"exercise": 0}),
            json!({"exercise": u64::MAX}),
            json!({"exercise": -1}),
            json!({"exercise": "x"}),
            json!({"exercise": 1.5}),
            json!({"exercise": [0]}),
            json!({"exercise": [u64::MAX, 1]}),
            json!({"exercise": vec![1; 5000]}),
            json!({"exercise": [null]}),
            json!({"exercise": {}}),
        ] {
            let _ = s.run("exercise.delete", &v);
        }
        let mut s = two();
        s.run("exercise.delete", &json!({"exercise": [2, 1, 2]})).unwrap();
        assert!(scan(&s).exercises.is_empty());
    }
}
