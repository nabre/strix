//! Exercise numbering, a document setting (docs/exercise.md): the prefix of every exercise's
//! label (by default "Exercise" in the document's language; any text, or none for the number
//! alone), the number format and the first number. It is the list the Exercise Title style
//! numbers with, so it is saved with the document and every title renumbers at once; the results
//! summary heads each exercise with the same label.

use serde_json::{Value, json};
use strix_doc::section::NumFormat;

use super::{STYLE_TITLE, ensure_styles, update, words};
use crate::{CmdError, CmdResult, Session};

/// The formats offered: their OOXML names.
pub const FORMATS: [&str; 5] = ["decimal", "upperLetter", "lowerLetter", "upperRoman", "lowerRoman"];
/// Longest prefix, in characters; largest first number (Word's).
const MAX_PREFIX: usize = 40;
const MAX_START: f64 = 32767.0;

/// The numbering of the document's exercises.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Numbering {
    pub prefix: String,
    pub format: NumFormat,
    pub start: u32,
}

impl Numbering {
    /// The label of exercise `n` (0-based) without the title's final period: `Esercizio 1`, `B`.
    pub fn label(&self, n: usize) -> String {
        let n = u32::try_from(n).unwrap_or(u32::MAX).saturating_add(self.start);
        let number = self.format.format(n);
        if self.prefix.is_empty() { number } else { format!("{} {number}", self.prefix) }
    }
}

/// The list exercise titles number with: the Exercise Title style's.
fn title_list(s: &Session) -> Option<u32> {
    let num = s.doc.styles.get(STYLE_TITLE)?.para.numbering?.num;
    s.doc.numbering.num(num).is_some().then_some(num)
}

/// The document's exercise numbering (the default one when it has no exercise styles yet).
pub(crate) fn current(s: &Session) -> Numbering {
    let default = Numbering { prefix: words(s).exercise.to_string(), format: NumFormat::Decimal, start: 1 };
    let Some(num) = title_list(s) else { return default };
    let Some(level) = s.doc.numbering.level(num, 0) else { return default };
    let start = s.doc.numbering.num(num).and_then(|n| n.start_overrides.iter().find(|(l, _)| *l == 0).map(|(_, v)| *v)).unwrap_or(level.start);
    let prefix = level.text.split_once("%1").map_or(level.text.as_str(), |(p, _)| p).trim().to_string();
    Numbering { prefix, format: level.format, start }
}

/// A prefix as kept: no control or object characters, no `%` (it would name a list level).
fn clean_prefix(t: &str) -> String {
    t.chars().filter(|&c| !c.is_control() && c != strix_doc::para::OBJ && c != '%').take(MAX_PREFIX).collect::<String>().trim().to_string()
}

/// `exercise.numbering`: sets what the params name, then returns the numbering.
pub(super) fn command(s: &mut Session, v: &Value) -> CmdResult {
    let prefix = match v.get("prefix") {
        None => None,
        Some(Value::Null) => Some(words(s).exercise.to_string()),
        Some(Value::String(t)) => Some(clean_prefix(t)),
        Some(_) => return Err(CmdError::Params("prefix: a text, or null for the default".into())),
    };
    let format = match v.get("format") {
        None | Some(Value::Null) => None,
        Some(Value::String(f)) if FORMATS.contains(&f.as_str()) => Some(NumFormat::from_ooxml(f)),
        Some(_) => return Err(CmdError::Params(format!("format: one of {}", FORMATS.join(", ")))),
    };
    let start = match v.get("start") {
        None | Some(Value::Null) => None,
        Some(n) => match n.as_f64().filter(|f| f.is_finite()) {
            Some(f) => Some(f.round().clamp(1.0, MAX_START) as u32),
            None => return Err(CmdError::Params("start: a number from 1".into())),
        },
    };
    if prefix.is_some() || format.is_some() || start.is_some() {
        set(s, prefix, format, start)?;
    }
    let n = current(s);
    Ok(json!({
        "prefix": n.prefix,
        "format": n.format.ooxml(),
        "start": n.start,
        "default": n.prefix == words(s).exercise,
        "example": n.label(0),
    }))
}

fn set(s: &mut Session, prefix: Option<String>, format: Option<NumFormat>, start: Option<u32>) -> Result<(), CmdError> {
    ensure_styles(s);
    let num = title_list(s).ok_or_else(|| CmdError::Failed("the Exercise Title style has no numbering".into()))?;
    let was = current(s);
    let want = Numbering { prefix: prefix.unwrap_or(was.prefix.clone()), format: format.unwrap_or(was.format), start: start.unwrap_or(was.start) };
    if want == was {
        return Ok(());
    }
    let text = if want.prefix.is_empty() { "%1.".to_string() } else { format!("{} %1.", want.prefix) };
    let numbering = &mut s.doc.numbering;
    let Some(n) = numbering.nums.iter_mut().find(|n| n.id == num) else { return Err(CmdError::Failed("the exercise list is missing".into())) };
    // The list's own level 0 and start would hide the definition's.
    n.level_overrides.retain(|(l, _)| *l != 0);
    n.start_overrides.retain(|(l, _)| *l != 0);
    let abstract_id = n.abstract_id;
    let level = numbering
        .abstracts
        .iter_mut()
        .find(|a| a.id == abstract_id)
        .and_then(|a| a.levels.first_mut())
        .ok_or_else(|| CmdError::Failed("the exercise list has no level".into()))?;
    level.text = text;
    level.format = want.format;
    level.start = want.start;
    s.touch();
    update(s);
    s.clamp_selection();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::{body_path, plain, scan};
    use super::*;
    use strix_doc::{Block, Document, Pos, StoryRef};

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
                Block::Table(_) => String::new(),
            })
            .collect()
    }

    /// The labels the titles show, as the layout numbers them.
    fn labels(s: &Session) -> Vec<String> {
        let mut counters = strix_doc::numbering::Counters::default();
        s.doc
            .body
            .iter()
            .filter_map(|b| match &**b {
                Block::Para(p) if p.props.style.as_deref() == Some(STYLE_TITLE) => {
                    let n = s.doc.styles.resolve_para(&p.props).numbering?;
                    counters.next_label(&s.doc.numbering, n.num, n.level).map(|(l, _)| l)
                }
                _ => None,
            })
            .collect()
    }

    /// The summary's exercise lines (after its heading, those that aren't a question's).
    fn summary(s: &Session) -> Vec<String> {
        let t = texts(s);
        let from = t.iter().position(|x| x == "Riepilogo dei risultati").map_or(t.len(), |i| i + 1);
        t[from..].iter().filter(|x| !x.contains(") ")).cloned().collect()
    }

    fn two() -> Session {
        let mut s = session();
        s.run("exercise.insert", &json!({"title": "Uno", "questions": ["q"]})).unwrap();
        let end = texts(&s).iter().position(|x| x == "Riepilogo dei risultati").unwrap() - 1;
        s.sel = crate::Selection::caret(Pos::new(StoryRef::Body, body_path(end), 0));
        s.run("exercise.insert", &json!({"title": "Due", "questions": ["q"]})).unwrap();
        s
    }

    /// The owner asked to name the exercises their own way: a prefix (Esercizio by default, any
    /// text, or none), a number format and a first number, for the whole document; the titles
    /// and the summary (only the label there, never the title) renumber at once.
    #[test]
    fn exercise_numbering_is_a_document_setting() {
        let mut s = two();
        let n = s.run("exercise.numbering", &json!({})).unwrap();
        assert_eq!(n, json!({"prefix": "Esercizio", "format": "decimal", "start": 1, "default": true, "example": "Esercizio 1"}));
        assert_eq!(labels(&s), ["Esercizio 1.", "Esercizio 2."]);
        assert_eq!(summary(&s), ["Esercizio 1", "Esercizio 2"]);
        let n = s.run("exercise.numbering", &json!({"prefix": "Problema", "format": "upperLetter", "start": 3})).unwrap();
        assert_eq!((n["example"].clone(), n["default"].clone()), (json!("Problema C"), json!(false)));
        assert_eq!(labels(&s), ["Problema C.", "Problema D."]);
        assert_eq!(summary(&s), ["Problema C", "Problema D"]);
        // One undo step brings the old numbering back.
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(labels(&s), ["Esercizio 1.", "Esercizio 2."]);
        s.run("edit.redo", &json!({})).unwrap();
        // No prefix: the number alone; the other settings stay.
        s.run("exercise.numbering", &json!({"prefix": "", "format": "lowerRoman"})).unwrap();
        assert_eq!(labels(&s), ["iii.", "iv."]);
        assert_eq!(summary(&s), ["iii", "iv"]);
        // Saved with the document.
        let bytes = crate::io::save_bytes("e.xmath", &s.doc).unwrap();
        let mut back = Session::new(crate::io::open_bytes("e.xmath", &bytes).unwrap());
        back.prefs.default_language = Some("it-IT".into());
        let n = back.run("exercise.numbering", &json!({})).unwrap();
        assert_eq!((n["prefix"].clone(), n["format"].clone(), n["start"].clone()), (json!(""), json!("lowerRoman"), json!(3)));
        assert_eq!(labels(&back), ["iii.", "iv."]);
        // A new exercise counts on; null brings the default prefix back.
        s.run("exercise.numbering", &json!({"prefix": null, "format": "decimal", "start": 1})).unwrap();
        assert_eq!(labels(&s), ["Esercizio 1.", "Esercizio 2."]);
        assert!(s.run("exercise.numbering", &json!({})).unwrap()["default"].as_bool().unwrap());
        // Deleting the first renumbers the second.
        s.run("exercise.delete", &json!({"exercise": 1})).unwrap();
        assert_eq!(summary(&s), ["Esercizio 1"]);
        assert_eq!(scan(&s).exercises.len(), 1);
    }

    #[test]
    fn reading_the_numbering_changes_nothing() {
        let mut s = session();
        let before = s.doc.clone();
        let n = s.run("exercise.numbering", &json!({})).unwrap();
        assert_eq!(n["example"], json!("Esercizio 1"));
        assert!(s.doc.same_content(&before));
        // Set before any exercise: the first one takes it.
        s.run("exercise.numbering", &json!({"prefix": "Es.", "start": 5})).unwrap();
        s.run("exercise.insert", &json!({"title": "Uno"})).unwrap();
        assert_eq!(labels(&s), ["Es. 5."]);
    }

    #[test]
    fn hostile_params_never_panic() {
        let mut s = two();
        for v in [json!({"format": "bullet"}), json!({"format": 3}), json!({"prefix": 5}), json!({"start": "x"}), json!({"start": [1]})] {
            assert!(s.run("exercise.numbering", &v).is_err(), "{v}");
        }
        for (start, want) in [(json!(0), 1), (json!(-7), 1), (json!(1e12), 32767), (json!(2.6), 3), (json!(u64::MAX), 32767)] {
            let n = s.run("exercise.numbering", &json!({"start": start})).unwrap();
            assert_eq!(n["start"], json!(want));
        }
        // Control characters, object markers and `%` never reach the list's text; long prefixes are cut.
        let n = s.run("exercise.numbering", &json!({"prefix": "a%1\u{FFFC}\nb"})).unwrap();
        assert_eq!(n["prefix"], json!("a1b"));
        let n = s.run("exercise.numbering", &json!({"prefix": "x".repeat(10_000)})).unwrap();
        assert_eq!(n["prefix"].as_str().map(str::len), Some(40));
        assert_eq!(labels(&s).len(), 2);
    }
}
