//! Exercise numbering, a document setting (docs/exercise.md): the prefix of every exercise's
//! label (by default "Exercise" in the document's language; any text, or none for the number
//! alone), the number format, the separator after the number and the first number. It is the list the Exercise Title style
//! numbers with, so it is saved with the document and every title renumbers at once; the results
//! summary heads each exercise with the same label.

use std::sync::Arc;

use serde_json::{Value, json};
use wordcraft_doc::Block;
use wordcraft_doc::props::NumRef;
use wordcraft_doc::section::NumFormat;

use super::{STYLE_TITLE, ensure_styles, update, words};
use crate::{CmdError, CmdResult, Session};

/// The formats offered: their OOXML names.
pub const FORMATS: [&str; 5] = ["decimal", "upperLetter", "lowerLetter", "upperRoman", "lowerRoman"];
/// The separators offered after the number (`Esercizio 1.`, `Esercizio 1:`, `Esercizio 1 -`,
/// `Esercizio 1)`, or a space alone: `Esercizio 1 Title`) and how each is written in the list's
/// level text after `%1`.
pub const SEPARATORS: [(&str, &str); 5] = [(".", "."), (":", ":"), ("-", " -"), (")", ")"), (" ", "")];
/// The default separator: a period.
const DEFAULT_SEPARATOR: &str = ".";
/// Longest prefix, in characters; largest first number (Word's).
const MAX_PREFIX: usize = 40;
const MAX_START: f64 = 32767.0;

/// The numbering of the document's exercises.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Numbering {
    pub prefix: String,
    pub format: NumFormat,
    pub start: u32,
    /// What follows the number in the titles (one of [`SEPARATORS`], or a level text's own).
    pub separator: String,
}

impl Numbering {
    /// The label of exercise `n` (0-based) without the title's separator: `Esercizio 1`, `B`.
    pub fn label(&self, n: usize) -> String {
        let n = u32::try_from(n).unwrap_or(u32::MAX).saturating_add(self.start);
        let number = self.format.format(n);
        if self.prefix.is_empty() { number } else { format!("{} {number}", self.prefix) }
    }

    /// The list level's text: `Esercizio %1.`, `%1)`, `Es. %1 -`.
    fn level_text(&self) -> String {
        let after = SEPARATORS.iter().find(|(id, _)| *id == self.separator).map_or(self.separator.as_str(), |(_, t)| t);
        if self.prefix.is_empty() { format!("%1{after}") } else { format!("{} %1{after}", self.prefix) }
    }
}

/// A lone exercise needs no number: while the document has only one, its title switches the
/// style's list off (`num: 0`, Word's "no numbering" over a style's list) and the summary names
/// no label; with a second exercise both come back. Only the off switch this sets is touched, so
/// a numbering of the user's own on the title stays. Whether anything changed.
pub(crate) fn apply_single(s: &mut Session, layout: &super::Layout) -> bool {
    let lone = layout.exercises.len() == 1;
    let mut changed = false;
    for e in &layout.exercises {
        let Some(block) = s.doc.body.get_mut(e.start) else { continue };
        let Block::Para(p) = &**block else { continue };
        let want = match p.props.numbering {
            None if lone => Some(NumRef { num: 0, level: 0 }),
            Some(n) if n.num == 0 && !lone => None,
            _ => continue,
        };
        let Block::Para(p) = Arc::make_mut(block) else { continue };
        p.props.numbering = want;
        p.touch();
        changed = true;
    }
    changed
}

/// The list exercise titles number with: the Exercise Title style's.
fn title_list(s: &Session) -> Option<u32> {
    let num = s.doc.styles.get(STYLE_TITLE)?.para.numbering?.num;
    s.doc.numbering.num(num).is_some().then_some(num)
}

/// The document's exercise numbering (the default one when it has no exercise styles yet).
pub(crate) fn current(s: &Session) -> Numbering {
    let default = Numbering { prefix: words(s).exercise.to_string(), format: NumFormat::Decimal, start: 1, separator: DEFAULT_SEPARATOR.into() };
    let Some(num) = title_list(s) else { return default };
    let Some(level) = s.doc.numbering.level(num, 0) else { return default };
    let start = s.doc.numbering.num(num).and_then(|n| n.start_overrides.iter().find(|(l, _)| *l == 0).map(|(_, v)| *v)).unwrap_or(level.start);
    let (prefix, after) = level.text.split_once("%1").unwrap_or((level.text.as_str(), DEFAULT_SEPARATOR));
    let after = after.trim();
    let separator = if after.is_empty() { " ".to_string() } else { after.to_string() };
    Numbering { prefix: prefix.trim().to_string(), format: level.format, start, separator }
}

/// A prefix as kept: no control or object characters, no `%` (it would name a list level).
fn clean_prefix(t: &str) -> String {
    t.chars().filter(|&c| !c.is_control() && c != wordcraft_doc::para::OBJ && c != '%').take(MAX_PREFIX).collect::<String>().trim().to_string()
}

/// `exercise.numbering`: sets what the params name, then returns the numbering.
pub(super) fn command(s: &mut Session, v: &Value) -> CmdResult {
    let prefix = match v.get("prefix") {
        None => None,
        Some(Value::Null) => Some(super::doc_words(s).exercise.to_string()),
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
    let separator = match v.get("separator") {
        None | Some(Value::Null) => None,
        Some(Value::String(t)) if SEPARATORS.iter().any(|(id, _)| id == t) => Some(t.clone()),
        Some(_) => return Err(CmdError::Params(r#"separator: one of ".", ":", "-", ")", " ""#.into())),
    };
    if prefix.is_some() || format.is_some() || start.is_some() || separator.is_some() {
        set(s, Change { prefix, format, start, separator })?;
    }
    let n = current(s);
    Ok(json!({
        "prefix": n.prefix,
        "format": n.format.ooxml(),
        "start": n.start,
        "separator": n.separator,
        "default": n.prefix == super::doc_words(s).exercise,
        "example": n.label(0),
    }))
}

/// The settings a change names; the others stay.
#[derive(Debug, Default)]
pub(super) struct Change {
    pub prefix: Option<String>,
    pub format: Option<NumFormat>,
    pub start: Option<u32>,
    pub separator: Option<String>,
}

pub(super) fn set(s: &mut Session, change: Change) -> Result<(), CmdError> {
    let Change { prefix, format, start, separator } = change;
    ensure_styles(s);
    let num = title_list(s).ok_or_else(|| CmdError::Failed("the Exercise Title style has no numbering".into()))?;
    let was = current(s);
    let want = Numbering {
        prefix: prefix.unwrap_or(was.prefix.clone()),
        format: format.unwrap_or(was.format),
        start: start.unwrap_or(was.start),
        separator: separator.unwrap_or(was.separator.clone()),
    };
    if want == was {
        return Ok(());
    }
    let text = want.level_text();
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
    use wordcraft_doc::{Block, Document, Pos, StoryRef};

    fn session() -> Session {
        let mut s = Session::new(Document::new());
        s.run("document.setText", &json!({"text": "Intro"})).unwrap();
        s.doc.styles.default_chr.lang = Some("it-IT".into());
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
        let mut counters = wordcraft_doc::numbering::Counters::default();
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
        assert_eq!(n, json!({"prefix": "Esercizio", "format": "decimal", "start": 1, "separator": ".", "default": true, "example": "Esercizio 1"}));
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
        back.doc.styles.default_chr.lang = Some("it-IT".into());
        let n = back.run("exercise.numbering", &json!({})).unwrap();
        assert_eq!((n["prefix"].clone(), n["format"].clone(), n["start"].clone()), (json!(""), json!("lowerRoman"), json!(3)));
        assert_eq!(labels(&back), ["iii.", "iv."]);
        // A new exercise counts on; null brings the default prefix back.
        s.run("exercise.numbering", &json!({"prefix": null, "format": "decimal", "start": 1})).unwrap();
        assert_eq!(labels(&s), ["Esercizio 1.", "Esercizio 2."]);
        assert!(s.run("exercise.numbering", &json!({})).unwrap()["default"].as_bool().unwrap());
        // Deleting the first leaves a lone exercise: no number on its title or in the summary.
        s.run("exercise.delete", &json!({"exercise": 1})).unwrap();
        assert_eq!(labels(&s), Vec::<String>::new());
        assert_eq!(summary(&s), Vec::<String>::new());
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
        s.run("caret.docEnd", &json!({})).unwrap();
        s.run("exercise.insert", &json!({"title": "Due"})).unwrap();
        assert_eq!(labels(&s), ["Es. 5.", "Es. 6."]);
    }

    /// The owner: "Default" follows the language the document is checked in, the one the status
    /// bar shows — not the saved preference for new documents.
    #[test]
    fn default_prefix_follows_the_document_language() {
        let mut s = Session::new(Document::new());
        s.run("document.setText", &json!({"text": "Intro"})).unwrap();
        s.doc.styles.default_chr.lang = Some("en-US".into());
        s.prefs.default_language = Some("it-IT".into());
        let n = s.run("exercise.numbering", &json!({})).unwrap();
        assert_eq!((n["prefix"].clone(), n["default"].clone()), (json!("Exercise"), json!(true)));
        // The language set from the status bar (Review › Language, as the document's default).
        s.run("review.language", &json!({"lang": "it-IT", "default": true})).unwrap();
        let n = s.run("exercise.numbering", &json!({})).unwrap();
        assert_eq!((n["prefix"].clone(), n["default"].clone()), (json!("Esercizio"), json!(true)));
        s.run("exercise.numbering", &json!({"prefix": null})).unwrap();
        s.run("exercise.insert", &json!({"title": "Uno"})).unwrap();
        s.run("caret.docEnd", &json!({})).unwrap();
        s.run("exercise.insert", &json!({"title": "Due"})).unwrap();
        assert_eq!(labels(&s), ["Esercizio 1.", "Esercizio 2."]);
    }

    /// The owner: "when I have one exercise alone I don't need the number and the prefix". A
    /// lone exercise's title switches the list off and the summary names no label; a second
    /// exercise brings both back, deleting it takes them away again, and the file keeps it.
    #[test]
    fn a_lone_exercise_has_no_number() {
        let mut s = session();
        s.run("exercise.insert", &json!({"title": "Uno", "questions": ["q"]})).unwrap();
        assert_eq!(labels(&s), Vec::<String>::new());
        assert_eq!(summary(&s), Vec::<String>::new());
        assert!(texts(&s).contains(&"a) risultato non indicato".to_string()));
        // Saved as Word's "no numbering" over the style's list, and opened again as it was.
        for name in ["e.xmath", "e.docx"] {
            let bytes = crate::io::save_bytes(name, &s.doc).unwrap();
            let back = Session::new(crate::io::open_bytes(name, &bytes).unwrap());
            assert_eq!(labels(&back), Vec::<String>::new(), "{name}");
        }
        s.run("caret.docEnd", &json!({})).unwrap();
        s.run("exercise.insert", &json!({"title": "Due", "questions": ["q"]})).unwrap();
        assert_eq!(labels(&s), ["Esercizio 1.", "Esercizio 2."]);
        assert_eq!(summary(&s), ["Esercizio 1", "Esercizio 2"]);
        s.run("exercise.delete", &json!({"exercise": 2})).unwrap();
        assert_eq!(labels(&s), Vec::<String>::new());
        // One undo step brings the second exercise, and both numbers, back.
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(labels(&s), ["Esercizio 1.", "Esercizio 2."]);
    }

    /// The owner: besides the prefix, a separator after the number chosen from a list (`.`, `:`,
    /// `-`, `)`, a space). The titles follow at once, the summary keeps the label alone, the file
    /// keeps it, and changing the other settings leaves it as it is.
    #[test]
    fn the_separator_after_the_number_is_chosen() {
        let mut s = two();
        for (sep, want) in [
            (":", ["Esercizio 1:", "Esercizio 2:"]),
            ("-", ["Esercizio 1 -", "Esercizio 2 -"]),
            (")", ["Esercizio 1)", "Esercizio 2)"]),
            (" ", ["Esercizio 1", "Esercizio 2"]),
        ] {
            let n = s.run("exercise.numbering", &json!({"separator": sep})).unwrap();
            assert_eq!(n["separator"], json!(sep));
            assert_eq!(labels(&s), want, "{sep:?}");
            assert_eq!(summary(&s), ["Esercizio 1", "Esercizio 2"], "{sep:?}");
        }
        // The other settings keep the separator; no prefix keeps it too.
        s.run("exercise.numbering", &json!({"separator": ")"})).unwrap();
        s.run("exercise.numbering", &json!({"prefix": "", "format": "lowerLetter"})).unwrap();
        assert_eq!(labels(&s), ["a)", "b)"]);
        s.run("exercise.numbering", &json!({"prefix": null})).unwrap();
        assert_eq!(labels(&s), ["Esercizio a)", "Esercizio b)"]);
        // Saved with the document.
        for name in ["e.xmath", "e.docx"] {
            let bytes = crate::io::save_bytes(name, &s.doc).unwrap();
            let mut back = Session::new(crate::io::open_bytes(name, &bytes).unwrap());
            back.doc.styles.default_chr.lang = Some("it-IT".into());
            assert_eq!(back.run("exercise.numbering", &json!({})).unwrap()["separator"], json!(")"), "{name}");
            assert_eq!(labels(&back), ["Esercizio a)", "Esercizio b)"], "{name}");
        }
        // One undo step brings the period back.
        s.run("exercise.numbering", &json!({"separator": "."})).unwrap();
        assert_eq!(labels(&s), ["Esercizio a.", "Esercizio b."]);
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(labels(&s), ["Esercizio a)", "Esercizio b)"]);
    }

    #[test]
    fn hostile_params_never_panic() {
        let mut s = two();
        for v in [
            json!({"format": "bullet"}),
            json!({"format": 3}),
            json!({"prefix": 5}),
            json!({"start": "x"}),
            json!({"start": [1]}),
            json!({"separator": "%"}),
            json!({"separator": ";"}),
            json!({"separator": 1}),
            json!({"separator": ""}),
        ] {
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
