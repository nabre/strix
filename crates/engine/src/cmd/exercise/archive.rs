//! The archive's document properties (docs/archive.md): what a file of exercises is about — grade,
//! section, profession, subject, teacher, duration and score — kept where Word shows them too, so
//! a folder of exercises can be searched by them.
//!
//! - the **teacher** is the document's author and the **subject** its subject (File › Info);
//! - grade, section, profession, duration and score are **custom document properties**
//!   (`docProps/custom.xml`), grade, duration and score as numbers. Their names are written in
//!   the language of the document's exercises (*Classe*, *Sezione*…), and read in any of the
//!   languages Strix's exercises are written in;
//! - the **score** is computed, never typed: the grand total of the document's scores table
//!   ([`super::scores::grand_total`]), kept up to date with the other automatic parts.

use serde_json::{Value, json};
use wordcraft_doc::CustomProp;

use super::words::{ALL_WORDS, Words};
use super::{doc_words, scan};
use crate::{CmdError, CmdResult, Session};

/// Longest section or profession, and longest subject or teacher, in characters.
const MAX_SHORT: usize = 40;
const MAX_LONG: usize = 255;
/// The grade and duration (minutes) a file may hold.
const GRADES: std::ops::RangeInclusive<u64> = 1..=20;
const MINUTES: std::ops::RangeInclusive<u64> = 1..=600;

/// The custom properties, each by the field of [`Words`] that names it.
#[derive(Clone, Copy, PartialEq)]
enum Field {
    Grade,
    Section,
    Profession,
    Duration,
    Score,
}

impl Field {
    fn name(self, w: &Words) -> &'static str {
        match self {
            Field::Grade => w.grade,
            Field::Section => w.section,
            Field::Profession => w.profession,
            Field::Duration => w.duration,
            Field::Score => w.score,
        }
    }

    /// Whether `name` is this property's in any language.
    fn is(self, name: &str) -> bool {
        ALL_WORDS.iter().any(|w| self.name(w).eq_ignore_ascii_case(name.trim()))
    }
}

/// The value of property `f`, under whichever language's name it has.
fn get(s: &Session, f: Field) -> Option<&str> {
    s.doc.custom_props.iter().find(|p| f.is(&p.name)).map(|p| p.value.as_str())
}

/// Property `f` set to `value` of OOXML type `kind` (`lpwstr`, `i4`, `r8`) under its name in the
/// document's language, or removed (`None`); a copy under another language's name goes. Whether
/// anything changed.
fn put(s: &mut Session, f: Field, value: Option<(&str, &str)>) -> bool {
    let name = f.name(doc_words(s));
    let props = &mut s.doc.custom_props;
    if let Some((kind, value)) = value {
        let same = |p: &CustomProp| p.name == name && p.kind == kind && p.value == value;
        if props.iter().filter(|p| f.is(&p.name)).count() == 1 && props.iter().any(same) {
            return false;
        }
        match props.iter().position(|p| f.is(&p.name)) {
            Some(at) => {
                if let Some(p) = props.get_mut(at) {
                    *p = CustomProp { name: name.to_string(), kind: kind.to_string(), value: value.to_string(), ..CustomProp::default() };
                }
                let mut seen = false;
                props.retain(|p| {
                    if !f.is(&p.name) {
                        return true;
                    }
                    let first = !seen;
                    seen = true;
                    first
                });
            }
            None => props.push(CustomProp { name: name.to_string(), kind: kind.to_string(), value: value.to_string(), ..CustomProp::default() }),
        }
        true
    } else {
        let n = props.len();
        props.retain(|p| !f.is(&p.name));
        props.len() != n
    }
}

/// A number as a property holds it: whole numbers as `i4`, others as `r8` (with a decimal point,
/// whatever the language).
fn number(v: f64) -> (&'static str, String) {
    if v.fract() == 0.0 && v.abs() < 2_147_483_647.0 {
        ("i4", format!("{v:.0}"))
    } else {
        let t = format!("{v:.2}");
        ("r8", t.trim_end_matches('0').trim_end_matches('.').to_string())
    }
}

/// The Score property brought in line with the scores table: its grand total, or no score when the
/// document has exercises but no points in a scores table. A document without exercises keeps
/// what it has (a Word file's own property of the same name stays). Whether anything changed.
pub(crate) fn sync_score(s: &mut Session) -> bool {
    match super::scores::grand_total(s) {
        Some(total) => {
            let (kind, value) = number(total);
            put(s, Field::Score, Some((kind, &value)))
        }
        None if !scan(s).exercises.is_empty() => put(s, Field::Score, None),
        None => false,
    }
}

/// A property's value as JSON: a number when it reads as one, else its text.
fn shown(t: Option<&str>) -> Value {
    match t {
        None => Value::Null,
        Some(t) => t.trim().parse::<i64>().map(Value::from).or_else(|_| t.trim().parse::<f64>().map(Value::from)).unwrap_or_else(|_| json!(t)),
    }
}

/// The document's archive duration in minutes, if set.
pub(super) fn duration_minutes(s: &Session) -> Option<u32> {
    get(s, Field::Duration)?.trim().parse::<f64>().ok().filter(|v| v.is_finite() && *v >= 1.0 && *v <= 600.0).map(|v| v.round() as u32)
}

pub(super) fn state(s: &Session) -> Value {
    let w = doc_words(s);
    let text = |t: &str| if t.is_empty() { Value::Null } else { json!(t) };
    json!({
        "grade": shown(get(s, Field::Grade)),
        "section": shown(get(s, Field::Section)),
        "profession": shown(get(s, Field::Profession)),
        "subject": text(&s.doc.core.subject),
        "teacher": text(&s.doc.core.creator),
        "tags": text(&s.doc.core.keywords),
        "duration": shown(get(s, Field::Duration)),
        "score": shown(get(s, Field::Score)),
        "names": {
            "grade": w.grade, "section": w.section, "profession": w.profession, "duration": w.duration, "score": w.score,
        },
    })
}

/// Text param `k`: `Some(None)` to remove it (null or nothing but spaces), `Some(Some(text))`
/// trimmed, without control characters, at most `max` characters; `None` when absent.
fn text(v: &Value, k: &str, max: usize) -> Result<Option<Option<String>>, CmdError> {
    match v.get(k) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(t)) => {
            let t: String = t.chars().filter(|c| !c.is_control() && *c != wordcraft_doc::para::OBJ).collect();
            let t = t.trim();
            if t.chars().count() > max {
                return Err(CmdError::Params(format!("`{k}` is at most {max} characters")));
            }
            Ok(Some((!t.is_empty()).then(|| t.to_string())))
        }
        Some(_) => Err(CmdError::Params(format!("`{k}` must be text or null"))),
    }
}

/// Whole-number param `k` within `range` (a number or its digits as text): `Some(None)` to
/// remove it; `None` when absent.
fn whole(v: &Value, k: &str, range: std::ops::RangeInclusive<u64>) -> Result<Option<Option<u64>>, CmdError> {
    let bad = || CmdError::Params(format!("`{k}` must be a whole number from {} to {}, or null", range.start(), range.end()));
    let n = match v.get(k) {
        None => return Ok(None),
        Some(Value::Null) => return Ok(Some(None)),
        Some(Value::String(t)) if t.trim().is_empty() => return Ok(Some(None)),
        Some(Value::String(t)) => t.trim().parse::<u64>().map_err(|_| bad())?,
        Some(x) => x.as_u64().ok_or_else(bad)?,
    };
    if range.contains(&n) { Ok(Some(Some(n))) } else { Err(bad()) }
}

/// `archive.properties`: sets the properties given (null or empty text removes one); without
/// params, reads them. The score is the scores table's and can't be set.
pub(crate) fn command(s: &mut Session, v: &Value) -> CmdResult {
    if v.get("score").is_some() {
        return Err(CmdError::Params("`score` is computed: it is the scores table's total (Teaching › Scores)".into()));
    }
    if let Some(m) = v.as_object()
        && let Some(k) = m.keys().find(|k| !matches!(k.as_str(), "grade" | "section" | "profession" | "subject" | "teacher" | "duration" | "tags"))
    {
        return Err(CmdError::Params(format!("unknown property `{k}`")));
    }
    // Every param is checked before anything changes.
    let grade = whole(v, "grade", GRADES)?;
    let duration = whole(v, "duration", MINUTES)?;
    let section = text(v, "section", MAX_SHORT)?;
    let profession = text(v, "profession", MAX_SHORT)?;
    let subject = text(v, "subject", MAX_LONG)?;
    let teacher = text(v, "teacher", MAX_LONG)?;
    let tags = text(v, "tags", MAX_LONG)?;
    let mut changed = false;
    for (f, n) in [(Field::Grade, grade), (Field::Duration, duration)] {
        if let Some(n) = n {
            let value = n.map(|n| n.to_string());
            changed |= put(s, f, value.as_deref().map(|t| ("i4", t)));
        }
    }
    for (f, t) in [(Field::Section, section), (Field::Profession, profession)] {
        if let Some(t) = t {
            changed |= put(s, f, t.as_deref().map(|t| ("lpwstr", t)));
        }
    }
    for (dst, t) in [(&mut s.doc.core.subject, subject), (&mut s.doc.core.creator, teacher), (&mut s.doc.core.keywords, tags)] {
        if let Some(t) = t {
            let t = t.unwrap_or_default();
            if *dst != t {
                *dst = t;
                changed = true;
            }
        }
    }
    if changed {
        s.touch();
    }
    Ok(state(s))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::Session;

    fn run(s: &mut Session, id: &str, v: serde_json::Value) -> serde_json::Value {
        s.run(id, &v).unwrap()
    }

    fn italian() -> Session {
        let mut s = Session::new(wordcraft_doc::Document::new());
        run(&mut s, "review.language", json!({"lang": "it-CH", "default": true}));
        s
    }

    /// The owner's fields: grade, section, profession, subject, teacher and duration, kept as
    /// document properties under Italian names in an Italian document; the score is the scores
    /// table's total and follows it.
    #[test]
    fn the_archive_properties_are_document_properties() {
        let mut s = italian();
        let r = run(
            &mut s,
            "archive.properties",
            json!({"grade": 3, "section": " B ", "profession": "DIC", "subject": "Statica", "teacher": "M. Rossi", "duration": "45"}),
        );
        assert_eq!(r["grade"], 3);
        assert_eq!(r["section"], "B");
        assert_eq!(r["duration"], 45);
        assert_eq!(s.doc.core.subject, "Statica");
        assert_eq!(s.doc.core.creator, "M. Rossi");
        let prop = |s: &Session, n: &str| s.doc.custom_props.iter().find(|p| p.name == n).map(|p| (p.kind.clone(), p.value.clone()));
        assert_eq!(prop(&s, "Classe"), Some(("i4".into(), "3".into())));
        assert_eq!(prop(&s, "Sezione"), Some(("lpwstr".into(), "B".into())));
        assert_eq!(prop(&s, "Professione"), Some(("lpwstr".into(), "DIC".into())));
        assert_eq!(prop(&s, "Durata (min)"), Some(("i4".into(), "45".into())));
        assert_eq!(r["names"]["score"], "Punteggio");

        // The score: none without a scores table, then the table's grand total as it changes.
        run(&mut s, "document.setText", json!({"text": "Verifica"}));
        run(&mut s, "caret.docEnd", json!({}));
        run(&mut s, "exercise.insert", json!({"title": "Trave", "questions": ["Reazioni", "Momento"]}));
        assert!(prop(&s, "Punteggio").is_none());
        run(&mut s, "caret.docStart", json!({}));
        run(&mut s, "exercise.scores", json!({"on": true}));
        run(&mut s, "exercise.scores", json!({"exercise": 1, "criterion": 1, "points": 4}));
        assert_eq!(prop(&s, "Punteggio"), Some(("i4".into(), "4".into())));
        run(&mut s, "exercise.scores", json!({"add": "MOD"}));
        run(&mut s, "exercise.scores", json!({"exercise": 1, "criterion": 2, "points": 1.5}));
        assert_eq!(prop(&s, "Punteggio"), Some(("r8".into(), "5.5".into())));
        assert_eq!(run(&mut s, "archive.properties", json!({}))["score"], 5.5);
        // It can't be typed.
        assert!(s.run("archive.properties", &json!({"score": 10})).is_err());
        // The table removed: no score.
        run(&mut s, "exercise.scores", json!({"on": false}));
        assert!(prop(&s, "Punteggio").is_none());

        // Null or empty text removes a property.
        let r = run(&mut s, "archive.properties", json!({"section": "", "grade": null}));
        assert!(r["section"].is_null() && r["grade"].is_null());
        assert!(prop(&s, "Sezione").is_none() && prop(&s, "Classe").is_none());
    }

    /// Bad values are refused whole: nothing changes.
    #[test]
    fn bad_values_change_nothing() {
        let mut s = italian();
        for bad in [
            json!({"grade": 0}),
            json!({"grade": 21}),
            json!({"grade": "3ª"}),
            json!({"grade": -1}),
            json!({"duration": 601}),
            json!({"duration": 1.5}),
            json!({"section": 3}),
            json!({"section": "x".repeat(41)}),
            json!({"grade": 2, "colour": "red"}),
            json!({"section": "A", "duration": 0}),
        ] {
            assert!(s.run("archive.properties", &bad).is_err(), "{bad}");
        }
        assert!(s.doc.custom_props.is_empty());
    }

    /// Names are read in any language and rewritten in the document's: a property a German
    /// document wrote (Klasse) is the grade of an Italian one, and setting it renames it.
    #[test]
    fn names_are_read_in_any_language() {
        let mut s = italian();
        s.doc.custom_props.push(wordcraft_doc::CustomProp { name: "Klasse".into(), kind: "i4".into(), value: "2".into(), ..Default::default() });
        assert_eq!(run(&mut s, "archive.properties", json!({}))["grade"], 2);
        run(&mut s, "archive.properties", json!({"grade": 4}));
        let names: Vec<&str> = s.doc.custom_props.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Classe"]);
    }

    /// The properties go through .docx as Word reads them: the subject and author in the core
    /// properties, the others in the custom ones with their types.
    #[test]
    fn the_properties_survive_docx() {
        let mut s = italian();
        run(&mut s, "archive.properties", json!({"grade": 1, "profession": "GE", "subject": "Fisica", "teacher": "L. Bianchi", "duration": 30}));
        let bytes = wordcraft_docx::write(&s.doc).unwrap();
        let back = wordcraft_docx::read(&bytes).unwrap();
        assert_eq!(back.core.subject, "Fisica");
        assert_eq!(back.core.creator, "L. Bianchi");
        let mut again = Session::new(back);
        let r = run(&mut again, "archive.properties", json!({}));
        assert_eq!((r["grade"].clone(), r["profession"].clone(), r["duration"].clone()), (json!(1), json!("GE"), json!(30)));
    }
}
