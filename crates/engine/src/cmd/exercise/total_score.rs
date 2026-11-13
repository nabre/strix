//! The assessment's total score (docs/exercise.md, Total score): a line, right-aligned, where the
//! student's score is written against the maximum, `Punteggio: .......... / 20 pt`. It is a
//! locked inline control tagged `strix:scoreTotal` in a paragraph of its own. The maximum is
//! computed when the document has points (the scores table's grand total, else the exercises'
//! question points added up), else the one typed in (`exercise.totalScore {"points"}`), else
//! left as dots to fill in by hand. It is kept up to date with the other automatic parts
//! ([`super::update`]).

use std::sync::Arc;

use serde_json::{Value, json};
use wordcraft_doc::control::{ContentControl, ControlKind, ControlLock};
use wordcraft_doc::para::OBJ;
use wordcraft_doc::props::{Align, CharProps};
use wordcraft_doc::{Block, InlineObject, Paragraph, StoryRef, para_block};

use super::{doc_words, scan};
use crate::{CmdError, CmdResult, Session};

pub const TAG_SCORE_TOTAL: &str = "strix:scoreTotal";
/// The element on the control's properties holding the maximum typed in.
const EL: &str = "strix:scoreTotal";
const MAX_POINTS: f64 = 1.0e6;
/// The dots left for a number to write by hand: the score obtained, a maximum not known.
const DOTS: &str = "..........";
const SHORT_DOTS: &str = ".....";

fn read_manual(ctl: &ContentControl) -> Option<f64> {
    let open = format!("<{EL} ");
    let el = ctl.extra.iter().find(|x| x.starts_with(&open))?;
    let (_, rest) = el.split_once("val=\"")?;
    let (raw, _) = rest.split_once('"')?;
    raw.parse::<f64>().ok().filter(|v| v.is_finite()).map(|v| v.clamp(0.0, MAX_POINTS))
}

fn write_manual(ctl: &mut ContentControl, v: Option<f64>) {
    let open = format!("<{EL} ");
    ctl.extra.retain(|x| !x.starts_with(&open));
    if let Some(v) = v.filter(|v| v.is_finite()).map(|v| v.clamp(0.0, MAX_POINTS)) {
        ctl.extra.push(format!("<{EL} val=\"{v}\"/>"));
    }
}

/// The maximum the document's points give: the scores table's grand total, else the exercises'
/// question points added up; `None` without points.
fn computed(s: &Session) -> Option<f64> {
    if let Some(t) = super::scores::grand_total(s).filter(|v| *v > 0.0 && v.is_finite()) {
        return Some(t);
    }
    let layout = scan(s);
    let sum: f64 = (0..layout.exercises.len()).filter_map(|e| super::items::total(s, &layout, e)).sum();
    (sum > 0.0 && sum.is_finite()).then_some(sum)
}

/// What the control shows for maximum `max`.
fn text(s: &Session, max: Option<f64>) -> String {
    let w = doc_words(s);
    let max = max.map_or_else(|| SHORT_DOTS.to_string(), |v| super::scores::shown(v, super::scores::comma(s)));
    format!("{}: {DOTS} / {max} {}", w.score, w.points_short)
}

/// The top-level paragraphs holding a total score control, with the control's byte span.
fn found(s: &Session) -> Vec<(usize, (usize, usize))> {
    s.doc
        .body
        .iter()
        .enumerate()
        .filter_map(|(i, b)| match &**b {
            Block::Para(p) => super::items::span(p, TAG_SCORE_TOTAL).map(|sp| (i, sp)),
            _ => None,
        })
        .collect()
}

fn control_in(p: &Paragraph, at: usize) -> Option<&ContentControl> {
    match p.object_at(at) {
        Some(InlineObject::ControlStart { control }) => Some(control),
        _ => None,
    }
}

/// Every total score brought up to date with the document's points. Whether anything changed.
pub(crate) fn update(s: &mut Session) -> bool {
    let all = found(s);
    if all.is_empty() {
        return false;
    }
    let auto = computed(s);
    let mut changed = false;
    for (i, sp) in all {
        let Some(Block::Para(p)) = s.doc.body.get(i).map(|b| &**b) else { continue };
        let manual = control_in(p, sp.0).and_then(read_manual);
        let want = text(s, auto.or(manual));
        if p.text.get(sp.0 + OBJ.len_utf8()..sp.1) == Some(want.as_str()) {
            continue;
        }
        let Some(block) = s.doc.body.get_mut(i) else { continue };
        let Block::Para(p) = Arc::make_mut(block) else { continue };
        changed |= super::items::set_content(p, sp, &want);
    }
    if changed {
        s.touch();
        s.clamp_selection();
    }
    changed
}

fn state(s: &Session) -> Value {
    let auto = computed(s);
    let Some((i, sp)) = found(s).into_iter().next() else { return json!({"on": false, "computed": auto}) };
    let manual = match s.doc.body.get(i).map(|b| &**b) {
        Some(Block::Para(p)) => control_in(p, sp.0).and_then(read_manual),
        _ => None,
    };
    json!({"on": true, "points": auto.or(manual), "computed": auto, "manual": manual})
}

/// `exercise.totalScore`: insert (`"on": true`, a right-aligned line after the caret's
/// paragraph) or remove (`false`) the total score; `"points"` sets (number) or clears (null) the
/// maximum used when the document's points give none; without params, reads it.
pub(crate) fn command(s: &mut Session, v: &Value) -> CmdResult {
    match v.get("on").and_then(Value::as_bool) {
        Some(true) if found(s).is_empty() => insert(s)?,
        Some(false) => {
            remove(s);
            return Ok(state(s));
        }
        _ => {}
    }
    if let Some(pts) = v.get("points") {
        let value = match pts {
            Value::Null => None,
            x => Some(
                x.as_f64()
                    .filter(|f| f.is_finite() && *f >= 0.0)
                    .map(|f| f.min(MAX_POINTS))
                    .ok_or_else(|| CmdError::Params("points: a number from 0, or null".into()))?,
            ),
        };
        let all = found(s);
        if all.is_empty() {
            return Err(CmdError::Disabled("the document has no total score".into()));
        }
        for (i, sp) in all {
            let Some(block) = s.doc.body.get_mut(i) else { continue };
            let Block::Para(p) = Arc::make_mut(block) else { continue };
            if let Some(InlineObject::ControlStart { control }) = p.object_at_mut(sp.0) {
                write_manual(control, value);
            }
            p.touch();
        }
        s.touch();
    }
    update(s);
    Ok(state(s))
}

/// The total score line after the caret's top-level paragraph.
fn insert(s: &mut Session) -> Result<(), CmdError> {
    let at = s.sel.focus.clone();
    if at.story != StoryRef::Body || at.path.depth() != 0 || s.doc.control_at(&at).is_some() {
        return Err(CmdError::Failed("put the caret in the main text outside the exercises to insert the total score there".into()));
    }
    let i = at.path.0.first().map_or(0, |&i| i as usize);
    let mut ctl = ContentControl::new(ControlKind::RichText);
    ctl.tag = TAG_SCORE_TOTAL.into();
    ctl.title = doc_words(s).score.into();
    ctl.lock = ControlLock::SdtContentLocked;
    let chr = CharProps { bold: Some(true), ..CharProps::default() };
    let mut p = Paragraph::new();
    p.props.align = Some(Align::Right);
    let _ = p.insert_object(0, InlineObject::ControlStart { control: Box::new(ctl) }, &chr);
    let inner = p.text.len();
    let shown = text(s, computed(s));
    let _ = p.insert_text(inner, &shown, &chr);
    let _ = p.insert_object(inner + shown.len(), InlineObject::ControlEnd, &chr);
    let at_i = (i + 1).min(s.doc.body.len());
    s.doc.body.insert(at_i, para_block(p));
    s.touch();
    Ok(())
}

/// Every total score removed: its paragraph when nothing else is in it, else the control.
fn remove(s: &mut Session) {
    let all = found(s);
    if all.is_empty() {
        return;
    }
    for (i, (a, b)) in all.into_iter().rev() {
        let Some(block) = s.doc.body.get_mut(i) else { continue };
        let Block::Para(p) = Arc::make_mut(block) else { continue };
        let _ = p.delete(a, b + OBJ.len_utf8());
        p.touch();
        if p.text.is_empty() && s.doc.body.len() > 1 {
            s.doc.body.remove(i);
        }
    }
    s.doc.ensure_nonempty();
    s.touch();
    s.clamp_selection();
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wordcraft_doc::Document;

    use crate::Session;

    fn run(s: &mut Session, id: &str, v: serde_json::Value) -> serde_json::Value {
        s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
    }

    fn line(s: &Session) -> Option<String> {
        s.doc.body.iter().filter_map(|b| b.as_para()).find(|p| super::super::items::span(p, super::TAG_SCORE_TOTAL).is_some()).map(|p| p.plain_text())
    }

    fn session() -> Session {
        let mut s = Session::new(Document::new());
        run(&mut s, "document.setText", json!({"text": "Verifica"}));
        std::sync::Arc::make_mut(&mut s.doc.styles).default_chr.lang = Some("it-IT".into());
        s
    }

    /// Computed from the question points, then from the scores table; typed in otherwise.
    #[test]
    fn the_total_score_follows_the_points_or_takes_one_typed_in() {
        let mut s = session();
        // No exercise with points: dots, then the maximum typed in.
        run(&mut s, "caret.docStart", json!({}));
        let st = run(&mut s, "exercise.totalScore", json!({"on": true}));
        assert_eq!(st["on"], true, "{st}");
        assert_eq!(line(&s).as_deref(), Some("Punteggio: .......... / ..... pt"));
        run(&mut s, "exercise.totalScore", json!({"points": 30}));
        assert_eq!(line(&s).as_deref(), Some("Punteggio: .......... / 30 pt"));
        // Exercises with question points: the computed total wins.
        run(&mut s, "caret.docEnd", json!({}));
        run(&mut s, "exercise.insert", json!({"kind": "open", "questions": [{"text": "Uno", "points": 4}, {"text": "Due", "points": 2.5}]}));
        assert_eq!(line(&s).as_deref(), Some("Punteggio: .......... / 6,5 pt"));
        // A scores table: its grand total.
        run(&mut s, "caret.docEnd", json!({}));
        run(&mut s, "exercise.insert", json!({"title": "Libero", "questions": ["q"]}));
        run(&mut s, "caret.docStart", json!({}));
        run(&mut s, "exercise.scores", json!({"on": true}));
        run(&mut s, "exercise.scores", json!({"exercise": 2, "criterion": 1, "points": 3}));
        assert_eq!(line(&s).as_deref(), Some("Punteggio: .......... / 9,5 pt"));
        let st = run(&mut s, "exercise.totalScore", json!({}));
        assert_eq!(st["computed"], json!(9.5), "{st}");
        assert_eq!(st["manual"], json!(30.0), "{st}");
        // Saved and opened again, it is still the total score.
        let bytes = crate::io::save_bytes("t.docx", &s.doc).unwrap();
        let back = Session::new(crate::io::open_bytes("t.docx", &bytes).unwrap());
        assert_eq!(line(&back).as_deref(), Some("Punteggio: .......... / 9,5 pt"));
        // Removed, and one undo brings it back.
        run(&mut s, "exercise.totalScore", json!({"on": false}));
        assert!(line(&s).is_none());
        assert!(s.undo());
        assert!(line(&s).is_some());
    }

    #[test]
    fn hostile_points_are_refused() {
        let mut s = session();
        assert!(s.run("exercise.totalScore", &json!({"points": 3})).is_err());
        run(&mut s, "exercise.totalScore", json!({"on": true}));
        for bad in [json!(-1), json!("x"), json!([1])] {
            assert!(s.run("exercise.totalScore", &json!({"points": bad})).is_err(), "{bad}");
        }
        // Huge values are capped.
        run(&mut s, "exercise.totalScore", json!({"points": f64::MAX}));
        assert_eq!(line(&s).as_deref(), Some("Punteggio: .......... / 1000000 pt"));
    }
}
