use serde_json::json;

use crate::Session;

/// The thesaurus the tests read for Italian: our own words, in the MyThes format.
pub(super) const ITALIAN: &str = "UTF-8\n\
casa|2\n\
(s.f.)|abitazione|dimora|casa|(familiare) tetto\n\
(s.f.)|famiglia|dimora\n\
veloce|1\n\
(agg.)|rapido|svelto (familiare)\n";

fn session(lang: &str, text: &str) -> Session {
    let mut s = Session::new(wordcraft_doc::Document::new());
    s.run("document.setText", &json!({"text": text})).unwrap();
    std::sync::Arc::make_mut(&mut s.doc.styles).default_chr.lang = Some(lang.into());
    s
}

/// LO1.3: the word's language reads its downloaded thesaurus, meanings by category, and the
/// answer keeps `word` and `synonyms`.
#[test]
fn the_words_language_reads_its_thesaurus() {
    let mut s = session("it-IT", "Le case");
    // The word at the caret ("case"), found under its base form.
    s.run("caret.docEnd", &json!({})).unwrap();
    let r = s.run("review.thesaurus", &json!({})).unwrap();
    assert_eq!(r["word"], json!("case"));
    assert_eq!(r["entry"], json!("casa"));
    assert_eq!(r["thesaurus"], json!("it_IT"));
    assert_eq!(r["language"], json!("it-IT"));
    // The word itself is left out, notes are dropped, repeats listed once.
    assert_eq!(r["synonyms"], json!(["abitazione", "dimora", "tetto", "famiglia"]));
    assert_eq!(
        r["meanings"],
        json!([
            {"category": "s.f.", "synonyms": ["abitazione", "dimora", "tetto"]},
            {"category": "s.f.", "synonyms": ["famiglia", "dimora"]},
        ])
    );
    // A word given, in another case.
    let r = s.run("review.thesaurus", &json!({"word": "Veloce"})).unwrap();
    assert_eq!(r["synonyms"], json!(["rapido", "svelto"]));
    assert_eq!(r["meanings"][0]["category"], json!("agg."));
}

#[test]
fn the_built_in_list_is_the_fallback() {
    // English: no thesaurus read in tests, so the built-in list answers, offering the download.
    let mut s = session("en-US", "good");
    let r = s.run("review.thesaurus", &json!({"word": "good"})).unwrap();
    assert_eq!(r["synonyms"][0], json!("fine"));
    assert_eq!(r["meanings"], json!([{"category": null, "synonyms": ["fine", "excellent", "decent", "solid", "worthy"]}]));
    assert_eq!(r["thesaurus"], json!(null));
    assert_eq!(r["download"], json!("en_US"));
    // A word the Italian thesaurus doesn't list falls back too (here: nothing).
    let r = s.run("review.thesaurus", &json!({"word": "zzz", "lang": "it-IT"})).unwrap();
    assert_eq!((r["synonyms"].clone(), r["meanings"].clone(), r["thesaurus"].clone()), (json!([]), json!([]), json!(null)));
    // A language without a thesaurus: the built-in list, nothing to download.
    let r = s.run("review.thesaurus", &json!({"word": "good", "lang": "ja-JP"})).unwrap();
    assert_eq!((r["synonyms"][0].clone(), r["download"].clone()), (json!("fine"), json!(null)));
    assert!(s.run("review.thesaurus", &json!({"word": "good", "lang": 5})).is_err());
}

#[test]
fn the_run_language_comes_before_the_documents() {
    let mut s = session("en-US", "casa");
    s.run("select.all", &json!({})).unwrap();
    s.run("review.language", &json!({"lang": "it-IT"})).unwrap();
    s.run("caret.right", &json!({})).unwrap();
    let r = s.run("review.thesaurus", &json!({})).unwrap();
    assert_eq!((r["language"].clone(), r["thesaurus"].clone()), (json!("it-IT"), json!("it_IT")));
}

#[test]
fn download_thesaurus_names_what_it_would_fetch() {
    let mut s = session("ja-JP", "x");
    let e = s.run("proof.downloadThesaurus", &json!({})).unwrap_err().to_string();
    assert!(e.contains("no thesaurus"), "{e}");
    assert_eq!(super::source_for(Some("it-IT")).map(|x| (x.name, x.licence)).ok(), Some(("it_IT", "GPL-3.0")));
    // Tests never download: the download's error, unless this machine has it installed.
    match s.run("proof.downloadThesaurus", &json!({"lang": "es-ES"})) {
        Ok(r) => assert_eq!((r["thesaurus"].clone(), r["downloaded"].clone()), (json!("es"), json!(false))),
        Err(e) => assert!(e.to_string().contains("no downloads in tests"), "{e}"),
    }
    for bad in [json!({"lang": 5}), json!({"force": "yes"}), json!({"lang": ["it"]})] {
        assert!(s.run("proof.downloadThesaurus", &bad).is_err(), "{bad}");
    }
}

#[test]
fn notes_around_a_synonym_are_dropped() {
    for (raw, want) in [
        ("dwelling (generic term)", "dwelling"),
        ("(potocznie) chałupa", "chałupa"),
        ("(a) (b) word (c)", "word"),
        ("(only a note)", "(only a note)"),
        ("plain", "plain"),
        (")(", ")("),
        ("", ""),
    ] {
        assert_eq!(super::plain(raw), want, "{raw}");
    }
}
