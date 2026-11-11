//! Exercises follow their language (docs/exercise.md): when Set Proofing Language changes the
//! language of an exercise (the one its title is in, the language its dictionary checks with),
//! the words Strix wrote for it change language too: the titles on the parts' frames, the
//! placeholders still shown, the solution's heading, the numbering's prefix while it is the
//! default one, the results summary and the scores table's default criterion. A word is translated
//! only while it is one of the defaults ([`ALL_WORDS`]): whatever the user wrote stays as written.
//! A language without its own words (Latin, Romansh…) leaves the words as they are.

use std::sync::Arc;

use wordcraft_doc::para::OBJ;
use wordcraft_doc::{Block, InlineObject, Paragraph};

use super::words::ALL_WORDS;
use super::{
    Layout, Part, TAG_DATA, TAG_EXERCISE, TAG_PROMPT, TAG_QUESTIONS, TAG_RESULTS, TAG_SOLUTION, TAG_TEXT, TAG_TITLE, Words, exercise_words,
    numbering, scan, scores, update, words_lang,
};
use crate::Session;

type Field = fn(&Words) -> &'static str;

/// The language of top-level block `i`: its first character's, through the styles and the
/// document default; the caret's when the block isn't a paragraph.
pub(super) fn lang_at(s: &Session, i: usize) -> String {
    let Some(Block::Para(p)) = s.doc.body.get(i).map(|b| &**b) else { return words_lang(s) };
    let off = p.text.char_indices().find(|&(_, c)| c != OBJ).map_or(0, |(k, _)| k);
    s.doc.styles.resolve_char(p.props.style.as_deref(), p.props_of_char(off)).lang.or_else(|| crate::cmd::default_lang(s)).unwrap_or_default()
}

/// The word a part's frame is titled with.
fn title_field(tag: &str) -> Option<Field> {
    Some(match tag {
        TAG_EXERCISE => |w| w.exercise,
        TAG_TITLE => |w| w.title_part,
        TAG_TEXT => |w| w.statement,
        TAG_QUESTIONS => |w| w.questions,
        TAG_SOLUTION => |w| w.solution,
        TAG_PROMPT => |w| w.prompt,
        TAG_DATA => |w| w.data,
        TAG_RESULTS => |w| w.results,
        _ => return None,
    })
}

/// The placeholder a part shows while it is empty.
fn placeholder_field(tag: &str) -> Option<Field> {
    Some(match tag {
        TAG_TITLE => |w| w.title,
        TAG_TEXT => |w| w.text,
        TAG_QUESTIONS => |w| w.question,
        _ => return None,
    })
}

/// The language exercise `e`'s words are in now: the one whose frame titles and placeholders it
/// shows most of (none when it shows none of any).
pub(super) fn present(s: &Session, layout: &Layout, e: usize) -> Option<&'static Words> {
    let mut score = [0usize; ALL_WORDS.len()];
    for (block, &(ex, _)) in s.doc.body.iter().zip(&layout.parts) {
        let Block::Para(p) = &**block else { continue };
        if ex != Some(e) {
            continue;
        }
        for o in &p.objects {
            let InlineObject::ControlStart { control } = o else { continue };
            let (title, placeholder) = (title_field(&control.tag), placeholder_field(&control.tag));
            for (k, w) in ALL_WORDS.iter().enumerate() {
                let hits = usize::from(title.is_some_and(|f| f(w) == control.title))
                    + usize::from(placeholder.is_some_and(|f| f(w) == control.placeholder_text));
                if let Some(n) = score.get_mut(k) {
                    *n = n.saturating_add(hits);
                }
            }
        }
    }
    let best = score.iter().copied().max().filter(|&m| m > 0)?;
    score.iter().position(|&n| n == best).and_then(|k| ALL_WORDS.get(k).copied())
}

/// `to`'s word for `field` when `old` is some language's word for it and not already `to`'s.
fn swap(old: &str, field: Field, to: &Words) -> Option<&'static str> {
    let new = field(to);
    (old != new && ALL_WORDS.iter().any(|w| field(w) == old)).then_some(new)
}

/// Replaces the first `old` in `p`'s text with `new`, in the same character formatting.
fn replace_text(p: &mut Paragraph, old: &str, new: &str) -> bool {
    let Some(at) = p.text.find(old) else { return false };
    let props = p.props_of_char(at).clone();
    if p.delete(at, at + old.len()).is_err() {
        return false;
    }
    if p.insert_text(at, new, &props).is_err() {
        // Put the old words back rather than leave the part empty.
        let _ = p.insert_text(at, old, &props);
        return false;
    }
    true
}

/// Paragraph `p` (in part `part`) with its default words in `to`'s language; `None`: unchanged.
fn translate(p: &Paragraph, part: Part, to: &Words) -> Option<Paragraph> {
    let mut out = p.clone();
    let mut changed = false;
    // Placeholders on show: their text is the paragraph's, swapped after the objects.
    let mut shown: Vec<(String, &'static str)> = Vec::new();
    for o in &mut out.objects {
        let InlineObject::ControlStart { control } = o else { continue };
        if let Some(new) = title_field(&control.tag).and_then(|f| swap(&control.title, f, to)) {
            control.title = new.into();
            changed = true;
        }
        if let Some(new) = placeholder_field(&control.tag).and_then(|f| swap(&control.placeholder_text, f, to)) {
            if control.showing_placeholder {
                shown.push((control.placeholder_text.clone(), new));
            }
            control.placeholder_text = new.into();
            changed = true;
        }
    }
    for (old, new) in shown {
        changed |= replace_text(&mut out, &old, new);
    }
    // The solution's heading is plain text after its control's start.
    if part == Part::Solution && p.objects.iter().any(|o| matches!(o, InlineObject::ControlStart { control } if control.tag == TAG_SOLUTION)) {
        let heading: String = p.text.chars().filter(|&c| c != OBJ).collect();
        if let Some(new) = swap(&heading, |w| w.solution, to) {
            changed |= replace_text(&mut out, &heading, new);
        }
    }
    changed.then_some(out)
}

/// Puts every exercise's default words in its language, then the numbering's default prefix, the
/// results summary and the scores table's default criterion in the first exercise's. Returns
/// whether anything changed.
pub(crate) fn follow_language(s: &mut Session) -> bool {
    let layout = scan(s);
    if layout.exercises.is_empty() {
        return false;
    }
    let ex_words: Vec<&'static Words> = (0..layout.exercises.len()).map(|e| exercise_words(s, &layout, e)).collect();
    let Some(&doc_words) = ex_words.first() else { return false };
    let mut changed = false;
    for (i, block) in s.doc.body.iter_mut().enumerate() {
        let Some(&(ex, part)) = layout.parts.get(i) else { break };
        let to = match (ex, part) {
            (Some(e), _) => ex_words.get(e).copied().unwrap_or(doc_words),
            (None, Part::Results) => doc_words,
            _ => continue,
        };
        let Block::Para(p) = &**block else { continue };
        if let Some(new) = translate(p, part, to) {
            *block = Arc::new(Block::Para(new));
            changed = true;
        }
    }
    if changed {
        s.touch();
    }
    let prefix = numbering::current(s).prefix;
    if let Some(new) = swap(&prefix, |w| w.exercise, doc_words) {
        // `set` rebuilds the summary too.
        changed |= numbering::set(s, numbering::Change { prefix: Some(new.to_string()), ..Default::default() }).is_ok();
    }
    changed |= scores::follow_language(s, |old| swap(old, |w| w.points, doc_words));
    changed |= update(s);
    s.clamp_selection();
    changed
}

#[cfg(test)]
mod tests {
    use super::super::plain;
    use super::*;
    use serde_json::json;
    use wordcraft_doc::Document;

    fn session(lang: &str) -> Session {
        let mut s = Session::new(Document::new());
        s.run("document.setText", &json!({"text": "Intro"})).unwrap();
        std::sync::Arc::make_mut(&mut s.doc.styles).default_chr.lang = Some(lang.into());
        s
    }

    fn texts(s: &Session) -> Vec<String> {
        s.doc
            .body
            .iter()
            .filter_map(|b| match &**b {
                Block::Para(p) => Some(plain(p)),
                Block::Table(_) => None,
            })
            .filter(|t| !t.is_empty())
            .collect()
    }

    fn titles(s: &Session) -> Vec<String> {
        s.doc
            .body
            .iter()
            .filter_map(|b| match &**b {
                Block::Para(p) => Some(p.objects.iter().filter_map(|o| match o {
                    InlineObject::ControlStart { control } => Some(control.title.clone()),
                    _ => None,
                })),
                Block::Table(_) => None,
            })
            .flatten()
            .collect()
    }

    /// The owner: an exercise inserted in English turns Italian when the document's language
    /// (the dictionary) becomes Italian, and back.
    #[test]
    fn default_words_follow_the_proofing_language() {
        let mut s = session("en-US");
        s.run("exercise.insert", &json!({})).unwrap();
        let t = texts(&s);
        assert!(t.contains(&"Exercise title".to_string()) && t.contains(&"Solution".to_string()), "{t:?}");
        assert_eq!(s.run("exercise.numbering", &json!({})).unwrap()["example"], json!("Exercise 1"));

        s.run("review.language", &json!({"lang": "it-IT", "default": true})).unwrap();
        let t = texts(&s);
        for want in [
            "Titolo dell'esercizio",
            "Scrivi qui il testo dell'esercizio: dati, figure, tabelle.",
            "Scrivi qui la domanda.",
            "Risoluzione",
            "Riepilogo dei risultati",
        ] {
            assert!(t.contains(&want.to_string()), "{want}: {t:?}");
        }
        assert!(!t.iter().any(|x| x == "Solution" || x == "Results" || x == "Exercise title"), "{t:?}");
        let ti = titles(&s);
        for want in ["Esercizio", "Titolo", "Testo", "Domande", "Risoluzione", "Domande (automatiche)", "Riepilogo dei risultati"] {
            assert!(ti.contains(&want.to_string()), "{want}: {ti:?}");
        }
        let n = s.run("exercise.numbering", &json!({})).unwrap();
        assert_eq!((n["example"].clone(), n["default"].clone()), (json!("Esercizio 1"), json!(true)));
        // The placeholder still behaves as one: it is what an emptied part shows.
        assert!(scan(&s).exercises.first().is_some_and(|e| e.no_questions));

        s.run("review.language", &json!({"lang": "fr-FR", "default": true})).unwrap();
        let t = texts(&s);
        assert!(t.contains(&"Titre de l'exercice".to_string()) && t.contains(&"Récapitulatif des résultats".to_string()), "{t:?}");
        assert_eq!(s.run("exercise.numbering", &json!({})).unwrap()["example"], json!("Exercice 1"));

        // A language without its own words keeps the ones there; one undo step takes a change back.
        s.run("review.language", &json!({"lang": "pl-PL", "default": true})).unwrap();
        assert!(texts(&s).contains(&"Titre de l'exercice".to_string()), "{:?}", texts(&s));
        s.run("review.language", &json!({"lang": "de-DE", "default": true})).unwrap();
        assert!(texts(&s).contains(&"Titel der Aufgabe".to_string()), "{:?}", texts(&s));
        s.run("edit.undo", &json!({})).unwrap();
        assert!(texts(&s).contains(&"Titre de l'exercice".to_string()), "{:?}", texts(&s));
    }

    /// The owner: a Latin (or Romansh, Spanish…) text has no words of its own, so the exercise keeps
    /// the ones it has instead of turning English; a new exercise there takes them too.
    #[test]
    fn a_language_without_words_keeps_them() {
        let mut s = session("it-IT");
        s.run("exercise.insert", &json!({})).unwrap();
        for lang in ["la", "rm-CH", "es-ES"] {
            s.run("review.language", &json!({"lang": lang, "default": true})).unwrap();
            let t = texts(&s);
            assert!(t.contains(&"Titolo dell'esercizio".to_string()) && t.contains(&"Riepilogo dei risultati".to_string()), "{lang}: {t:?}");
            assert!(titles(&s).contains(&"Esercizio".to_string()), "{lang}");
        }
        s.run("caret.docEnd", &json!({})).unwrap();
        s.run("exercise.insert", &json!({})).unwrap();
        assert_eq!(texts(&s).iter().filter(|t| *t == "Titolo dell'esercizio").count(), 2, "{:?}", texts(&s));
        assert_eq!(s.run("exercise.numbering", &json!({})).unwrap()["example"], json!("Esercizio 1"));

        // Swiss German and three-letter tags are recognised.
        s.run("review.language", &json!({"lang": "gsw-CH", "default": true})).unwrap();
        assert!(texts(&s).contains(&"Titel der Aufgabe".to_string()), "{:?}", texts(&s));
        s.run("review.language", &json!({"lang": "ita", "default": true})).unwrap();
        assert!(texts(&s).contains(&"Titolo dell'esercizio".to_string()), "{:?}", texts(&s));
    }

    /// The scores table's default criterion and its header follow the language; a criterion the
    /// user named stays.
    #[test]
    fn scores_table_follows_the_language() {
        let mut s = session("it-IT");
        s.run("exercise.insert", &json!({})).unwrap();
        s.run("caret.docStart", &json!({})).unwrap();
        s.run("exercise.scores", &json!({"on": true})).unwrap();
        s.run("exercise.scores", &json!({"add": "Ordine"})).unwrap();
        assert_eq!(s.run("exercise.scores", &json!({})).unwrap()["criteria"], json!(["Punti", "Ordine"]));
        s.run("review.language", &json!({"lang": "de-DE", "default": true})).unwrap();
        assert_eq!(s.run("exercise.scores", &json!({})).unwrap()["criteria"], json!(["Punkte", "Ordine"]));
        let header: Vec<String> = s
            .doc
            .body
            .iter()
            .find_map(|b| match &**b {
                Block::Table(t) => {
                    t.rows.first().map(|r| r.cells.iter().map(|c| c.blocks.iter().filter_map(|b| b.as_para()).map(plain).collect()).collect())
                }
                Block::Para(_) => None,
            })
            .unwrap_or_default();
        assert_eq!(header.first().map(String::as_str), Some("Aufgabe"), "{header:?}");
        s.run("review.language", &json!({"lang": "la", "default": true})).unwrap();
        assert_eq!(s.run("exercise.scores", &json!({})).unwrap()["criteria"], json!(["Punkte", "Ordine"]));
    }

    /// What the user wrote stays as written: their title, statement, questions and own prefix.
    #[test]
    fn written_text_and_a_chosen_prefix_stay() {
        let mut s = session("en-US");
        s.run("exercise.insert", &json!({"title": "Beam", "text": "A beam of span L.", "questions": ["The moment"]})).unwrap();
        s.run("exercise.numbering", &json!({"prefix": "Problem"})).unwrap();
        s.run("review.language", &json!({"lang": "it-IT", "default": true})).unwrap();
        let t = texts(&s);
        for want in ["Beam", "A beam of span L.", "The moment", "Risoluzione"] {
            assert!(t.contains(&want.to_string()), "{want}: {t:?}");
        }
        assert_eq!(s.run("exercise.numbering", &json!({})).unwrap()["example"], json!("Problem 1"));
    }

    /// A selection set to another language moves only the exercises it covers; a document
    /// without exercises is untouched.
    #[test]
    fn each_exercise_follows_its_own_title() {
        let mut s = session("en-US");
        assert!(!follow_language(&mut s));
        s.run("exercise.insert", &json!({})).unwrap();
        let first = scan(&s).exercises.first().map(|e| e.start).unwrap();
        // Only the first exercise's title paragraph turns German.
        if let Some(Block::Para(p)) = s.doc.body.get_mut(first).map(Arc::make_mut) {
            for r in &mut p.runs {
                r.props.lang = Some("de-DE".into());
            }
        }
        assert!(follow_language(&mut s));
        let t = texts(&s);
        assert!(t.contains(&"Titel der Aufgabe".to_string()) && t.contains(&"Lösung".to_string()), "{t:?}");
        assert!(!follow_language(&mut s), "nothing left to translate");
    }
}
