//! A quiz's only question on the first line (docs/exercise.md, Title line): when the line holds
//! the statement (no title) and the exercise is a quiz with a single question and nothing written
//! on the line, the question **moves** next to the number — `Esercizio 1.` *Scrivi qui la
//! domanda.* — instead of the line showing the statement's placeholder over a question that
//! shows its own. The title paragraph and the question's paragraph become one: the title control
//! stays, empty, and the question (its text, its points, a true/false question's boxes) follows
//! it in the body text's weight and size; its answer (options, space) stays below. The question's
//! list is kept on the title control (`strix:titleLineQuestion`) for when it leaves.
//!
//! The question goes back to a paragraph of its own as soon as that no longer holds: a second
//! question, a title on the line again, text written on the line, data shown, another kind.
//! A true/false question on the line names its boxes (`V ☐  F ☐`): there is no header line
//! above the number (true_false.rs).

use std::sync::Arc;

use wordcraft_doc::para::OBJ;
use wordcraft_doc::props::{CharProps, NumRef, ParaProps};
use wordcraft_doc::{Block, InlineObject, Paragraph, para_block};

use super::items::{TAG_ITEM, items, span};
use super::title_line::{self, Line};
use super::{STYLE_QUESTION, STYLE_TITLE, TAG_TITLE, Words, exercise_words, scan};
use crate::Session;

/// The question's list, kept on the title control while the question is on the line.
const LIST_EL: &str = "strix:titleLineQuestion";

/// Whether title paragraph `p` holds the exercise's question.
pub(super) fn joined(p: &Paragraph) -> bool {
    p.props.style.as_deref() == Some(STYLE_TITLE) && span(p, TAG_TITLE).is_some() && span(p, TAG_ITEM).is_some()
}

/// The question list kept on a title paragraph holding the question.
pub(super) fn kept_list(p: &Paragraph) -> Option<NumRef> {
    let open = format!("<{LIST_EL} ");
    let el = title_ctl_ref(p)?.extra.iter().find(|x| x.starts_with(&open))?.clone();
    let attr = |name: &str| -> Option<u32> {
        let key = format!("{name}=\"");
        let from = el.find(&key)? + key.len();
        el.get(from..)?.split('"').next()?.parse().ok()
    };
    let num = attr("num").filter(|&n| n != 0)?;
    let level = u8::try_from(attr("level").unwrap_or(0)).unwrap_or(0).min(super::MAX_LEVEL);
    Some(NumRef { num, level })
}

fn title_ctl_ref(p: &Paragraph) -> Option<&wordcraft_doc::control::ContentControl> {
    p.objects.iter().find_map(|o| match o {
        InlineObject::ControlStart { control } if control.tag == TAG_TITLE => Some(&**control),
        _ => None,
    })
}

fn title_ctl(p: &mut Paragraph) -> Option<&mut wordcraft_doc::control::ContentControl> {
    p.objects.iter_mut().find_map(|o| match o {
        InlineObject::ControlStart { control } if control.tag == TAG_TITLE => Some(&mut **control),
        _ => None,
    })
}

/// Whether nothing is written on the line: the title control empty or showing its placeholder.
fn line_empty(p: &Paragraph) -> bool {
    let Some((a, b)) = span(p, TAG_TITLE) else { return false };
    title_ctl_ref(p).is_some_and(|c| c.showing_placeholder) || p.text.get(a + OBJ.len_utf8()..b).is_none_or(|t| t.chars().all(|c| c == OBJ))
}

/// Every exercise's only question on its line or in a paragraph of its own, as the module docs
/// say. Whether anything changed.
pub(super) fn conform(s: &mut Session) -> bool {
    let props = title_line::statement_props(s);
    let mut changed = false;
    let layout = scan(s);
    for e in (0..layout.exercises.len()).rev() {
        let Some(ex) = layout.exercises.get(e) else { continue };
        let at = ex.start;
        let Some(p) = s.doc.body.get(at).and_then(|b| b.as_para()) else { continue };
        let Some((kind, line)) = title_line::of_para(p) else { continue };
        let is_joined = joined(p);
        let its = items(s, &layout, e);
        let w = exercise_words(s, &layout, e);
        let one = its.len() == 1 && ex.questions.len() == 1 && ex.questions.first().is_some_and(|q| q.level == 0);
        let want = line == Line::Statement && kind.quiz() && one && ex.data.items.is_empty() && line_empty(p);
        if is_joined && !want {
            changed |= split(s, at, w, &props, title_line::placeholder(w, line, kind));
        } else if is_joined {
            changed |= keep_line(s, at, &props);
        } else if want && let Some(q) = its.first().map(|it| it.question) {
            changed |= join(s, at, q, &props);
        }
    }
    if changed {
        s.touch();
        s.clamp_selection();
    }
    changed
}

/// The line holding the question shows no placeholder of its own (emptied by hand, it showed
/// the statement's again), and the question keeps the body text's weight and size (boxes and
/// points rewritten on it come in the title's).
fn keep_line(s: &mut Session, at: usize, props: &CharProps) -> bool {
    let Some(Block::Para(p)) = s.doc.body.get(at).map(|b| &**b) else { return false };
    let Some((_, b)) = span(p, TAG_TITLE) else { return false };
    let cut = b + OBJ.len_utf8();
    let showing = title_ctl_ref(p).is_some_and(|c| c.showing_placeholder);
    let unset = p.run_ranges().any(|(r, pr)| r.end > cut && (pr.bold.is_none() || pr.size.is_none()));
    if !showing && !unset {
        return false;
    }
    let Some(Block::Para(p)) = s.doc.body.get_mut(at).map(Arc::make_mut) else { return false };
    if showing {
        clear_line(p);
    }
    if let Some((_, b)) = span(p, TAG_TITLE) {
        let cut = b + OBJ.len_utf8();
        let len = p.text.len();
        if len > cut {
            let _ = p.format(cut, len, &|pr| {
                pr.bold = pr.bold.or(props.bold);
                pr.size = pr.size.or(props.size);
            });
        }
    }
    p.touch();
    true
}

/// Empties the title control: no text, no placeholder showing.
fn clear_line(p: &mut Paragraph) {
    if let Some((a, b)) = span(p, TAG_TITLE) {
        let from = a + OBJ.len_utf8();
        if b > from {
            let _ = p.delete(from, b);
        }
    }
    if let Some(c) = title_ctl(p) {
        c.showing_placeholder = false;
    }
}

/// Moves the question paragraph at `q` onto the line of the title paragraph at `at`. Whether it
/// did.
fn join(s: &mut Session, at: usize, mut q: usize, props: &CharProps) -> bool {
    // Only the question right after the line (a true/false header between them goes).
    if q == at + 2 && is_head(s, at + 1) {
        if !super::true_false::remove_head(s, at + 1) {
            return false;
        }
        q = at + 1;
    }
    if q != at + 1 {
        return false;
    }
    let Some(Block::Para(qp)) = s.doc.body.get(q).map(|b| &**b) else { return false };
    if qp.props.style.as_deref() != Some(STYLE_QUESTION) {
        return false;
    }
    let mut mid = qp.clone();
    let list = mid.props.numbering.filter(|n| n.num != 0);
    // In the body text's weight and size, as a statement on the line is.
    let len = mid.text.len();
    if len > 0 {
        let _ = mid.format(0, len, &|pr| {
            pr.bold = pr.bold.or(props.bold);
            pr.size = pr.size.or(props.size);
        });
    }
    let Some(Block::Para(title)) = s.doc.body.get_mut(at).map(Arc::make_mut) else { return false };
    clear_line(title);
    if let Some(c) = title_ctl(title) {
        let open = format!("<{LIST_EL} ");
        c.extra.retain(|x| !x.starts_with(&open));
        if let Some(n) = list {
            c.extra.push(format!("<{LIST_EL} num=\"{}\" level=\"{}\"/>", n.num, n.level));
        }
    }
    let cut = title.text.len();
    title.append(mid);
    title.touch();
    s.doc.body.remove(q);
    super::true_false::remap(s, &|top, off| {
        if top == q {
            (at, off + cut)
        } else if top > q {
            (top - 1, off)
        } else if top == at {
            (at, off.min(cut))
        } else {
            (top, off)
        }
    });
    true
}

fn is_head(s: &Session, i: usize) -> bool {
    s.doc.body.get(i).and_then(|b| b.as_para()).is_some_and(|p| span(p, super::true_false::TAG_HEAD).is_some())
}

/// The question on the line of the title paragraph at `at` goes back to a paragraph of its own
/// after it; an empty line shows `shown`, its placeholder. Whether it did.
fn split(s: &mut Session, at: usize, _w: &Words, props: &CharProps, shown: &str) -> bool {
    let Some(Block::Para(title)) = s.doc.body.get_mut(at).map(Arc::make_mut) else { return false };
    let Some((_, b)) = span(title, TAG_TITLE) else { return false };
    let cut = b + OBJ.len_utf8();
    let list = kept_list(title);
    let Ok(mut rest) = title.split_off(cut) else { return false };
    if let Some(c) = title_ctl(title) {
        let open = format!("<{LIST_EL} ");
        c.extra.retain(|x| !x.starts_with(&open));
    }
    title.props.tabs = None;
    if line_empty(title) {
        title_line::show_placeholder(title, shown, props);
    }
    title.touch();
    // The weight and size the line gave it are the question's own anyway.
    let len = rest.text.len();
    if len > 0 {
        let _ = rest.format(0, len, &|pr| {
            if pr.bold == props.bold {
                pr.bold = None;
            }
            if pr.size == props.size {
                pr.size = None;
            }
        });
    }
    rest.props = ParaProps { style: Some(STYLE_QUESTION.into()), numbering: list, ..ParaProps::default() };
    rest.touch();
    s.doc.body.insert(at + 1, para_block(rest));
    super::true_false::remap(s, &|top, off| {
        if top == at && off >= cut {
            (at + 1, off - cut)
        } else if top > at {
            (top + 1, off)
        } else {
            (top, off)
        }
    });
    true
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::super::plain;
    use super::*;

    fn run(s: &mut Session, id: &str, v: Value) -> Value {
        s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
    }

    fn italian() -> Session {
        let mut s = Session::new(wordcraft_doc::Document::new());
        std::sync::Arc::make_mut(&mut s.doc.styles).default_chr.lang = Some("it-IT".into());
        s
    }

    fn texts(s: &Session) -> Vec<String> {
        s.doc.body.iter().filter_map(|b| b.as_para()).map(plain).collect()
    }

    fn first(s: &Session) -> Paragraph {
        s.doc.body.first().and_then(|b| b.as_para()).cloned().unwrap_or_default()
    }

    fn count(s: &Session, needle: &str) -> usize {
        texts(s).iter().map(|t| t.matches(needle).count()).sum()
    }

    const STATEMENT: &str = "Scrivi qui il testo";
    const QUESTION: &str = "Scrivi qui la domanda";

    /// The owner: with no title, every kind showed the statement's placeholder on the line and
    /// the question's under it; the question moves onto the line instead.
    #[test]
    fn every_kind_has_its_only_question_on_the_line() {
        for kind in ["open", "drawing", "choice", "trueFalse", "cloze", "calc", "matching", "worked"] {
            let mut s = italian();
            run(&mut s, "exercise.insert", json!({"kind": kind, "titleLine": "statement"}));
            let line = plain(&first(&s));
            assert!(line.contains(QUESTION), "{kind}: {line:?}");
            assert!(!line.contains(STATEMENT), "{kind}: {line:?}");
            assert_eq!(count(&s, QUESTION), 1, "{kind}: {:?}", texts(&s));
            assert_eq!(count(&s, STATEMENT), 0, "{kind}: {:?}", texts(&s));
            assert!(joined(&first(&s)), "{kind}");
            // In a series, the number stays the title's.
            run(&mut s, "exercise.insert", json!({"kind": kind, "titleLine": "statement"}));
            let last = s.doc.body.iter().filter_map(|b| b.as_para()).rfind(|p| joined(p)).cloned().unwrap_or_default();
            let n = s.doc.styles.resolve_para(&last.props).numbering;
            assert!(n.is_some_and(|n| n.num != 0), "{kind}: {n:?}");
            // One question, counted once.
            let st = run(&mut s, "exercise.questions", json!({"exercise": 1}));
            assert_eq!(st["questions"].as_array().map(Vec::len), Some(1), "{kind}: {st}");
            // In the body text's weight.
            let p = first(&s);
            let at = p.text.find(QUESTION).unwrap_or(0);
            assert_eq!(p.props_of_char(at).bold, Some(false), "{kind}");
        }
    }

    #[test]
    fn typing_writes_the_question_on_the_line() {
        let mut s = italian();
        run(&mut s, "exercise.insert", json!({"kind": "open", "titleLine": "statement"}));
        run(&mut s, "select.text", json!({"text": "Scrivi qui la domanda."}));
        run(&mut s, "text.insert", json!({"text": "Quanto fa 2 + 2?"}));
        assert!(plain(&first(&s)).contains("Quanto fa 2 + 2?"), "{:?}", texts(&s));
        let st = run(&mut s, "exercise.questions", json!({"exercise": 1}));
        assert_eq!(st["questions"][0]["text"], "Quanto fa 2 + 2?", "{st}");
        // Saved and opened again: still on the line.
        for name in ["e.docx", "e.xmath"] {
            let bytes = crate::io::save_bytes(name, &s.doc).unwrap();
            let mut back = Session::new(crate::io::open_bytes(name, &bytes).unwrap());
            run(&mut back, "exercise.update", json!({}));
            assert!(joined(&first(&back)), "{name}");
            assert!(plain(&first(&back)).contains("Quanto fa 2 + 2?"), "{name}");
        }
    }

    #[test]
    fn a_second_question_or_a_title_puts_it_back_below() {
        let mut s = italian();
        run(&mut s, "exercise.insert", json!({"kind": "open", "titleLine": "statement", "questions": ["Uno"]}));
        assert!(plain(&first(&s)).contains("Uno"));
        let list = kept_list(&first(&s));
        // A second question: both under the line, numbered in their list; the line shows the
        // statement's placeholder again.
        run(&mut s, "exercise.addQuestion", json!({"text": "Due"}));
        assert!(!joined(&first(&s)), "{:?}", texts(&s));
        assert!(plain(&first(&s)).contains(STATEMENT), "{:?}", texts(&s));
        let st = run(&mut s, "exercise.questions", json!({"exercise": 1}));
        assert_eq!(st["questions"].as_array().map(Vec::len), Some(2), "{st}");
        let lists: Vec<_> = s
            .doc
            .body
            .iter()
            .filter_map(|b| b.as_para())
            .filter(|p| p.props.style.as_deref() == Some(STYLE_QUESTION))
            .map(|p| p.props.numbering)
            .collect();
        assert!(list.is_none(), "a lone question has no letter");
        assert!(lists.iter().all(|l| l.is_some_and(|n| n.num != 0)) && lists.first() == lists.get(1), "{lists:?}");
        // Undo: on the line again.
        assert!(s.undo());
        assert!(joined(&first(&s)));
        // A title: the question goes below, the title on the line, bold.
        run(&mut s, "exercise.titleLine", json!({"line": "title"}));
        assert!(!joined(&first(&s)));
        assert!(plain(&first(&s)).contains("Titolo"), "{:?}", texts(&s));
        let q = s.doc.body.get(1).and_then(|b| b.as_para()).cloned().unwrap_or_default();
        assert_eq!(q.props.style.as_deref(), Some(STYLE_QUESTION));
        let at = q.text.find("Uno").unwrap_or(0);
        assert_eq!(q.props_of_char(at).bold, None, "the line's weight is not left on the question");
        // And back.
        run(&mut s, "exercise.titleLine", json!({"line": "statement"}));
        assert!(joined(&first(&s)), "{:?}", texts(&s));
    }

    #[test]
    fn true_false_on_the_line_names_its_boxes() {
        let mut s = italian();
        run(
            &mut s,
            "exercise.insert",
            json!({"kind": "trueFalse", "titleLine": "statement", "questions": [{"text": "Il sole è una stella.", "correct": 1}]}),
        );
        let line = plain(&first(&s));
        // The answer shown: the True box ticked, both named.
        assert!(line.contains("Il sole è una stella.\tV ☒\tF ☐"), "{line:?}");
        // No header line left.
        assert!(!s.doc.body.iter().any(|b| b.as_para().is_some_and(|p| span(p, super::super::true_false::TAG_HEAD).is_some())));
        let st = run(&mut s, "exercise.questions", json!({"exercise": 1}));
        assert_eq!(st["questions"][0]["correct"], json!(["A"]), "{st}");
        // Running the update again changes nothing.
        let before = s.doc.body.clone();
        assert!(!super::super::update(&mut s));
        assert_eq!(s.doc.body, before);
        // Back below: the header comes back, the boxes lose their names.
        run(&mut s, "exercise.addQuestion", json!({"text": "Due"}));
        assert!(!joined(&first(&s)), "{:?}", texts(&s));
        assert!(!texts(&s).iter().any(|t| t.contains("V ☐")), "{:?}", texts(&s));
        assert!(s.doc.body.iter().any(|b| b.as_para().is_some_and(|p| span(p, super::super::true_false::TAG_HEAD).is_some())));
    }

    #[test]
    fn a_problem_or_a_written_statement_keeps_the_line() {
        // A problem: the line holds the statement, the questions stay below.
        let mut s = italian();
        run(&mut s, "exercise.insert", json!({"kind": "problem", "titleLine": "statement"}));
        assert!(!joined(&first(&s)));
        // A statement written on the line: the question stays below.
        let mut s = italian();
        run(&mut s, "exercise.insert", json!({"kind": "open", "text": "Leggi il brano.", "titleLine": "statement"}));
        assert!(!joined(&first(&s)), "{:?}", texts(&s));
        assert!(plain(&first(&s)).contains("Leggi il brano."));
    }
}
