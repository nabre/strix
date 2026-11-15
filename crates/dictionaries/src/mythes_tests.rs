//! The MyThes reader: synthetic files of our own in the shapes the published thesauri have, and
//! hostile ones. `STRIX_REAL_THESAURI` (a folder of downloaded `.dat` files) also reads real ones.

use super::*;

fn meanings(t: &Thesaurus, word: &str, lang: &str) -> Vec<(Option<String>, Vec<String>)> {
    t.lookup(word, Some(lang)).map(|(_, m)| m.into_iter().map(|m| (m.category, m.synonyms)).collect()).unwrap_or_default()
}

#[test]
fn reads_entries_in_utf8_with_their_categories() {
    let t = parse("\u{feff}UTF-8\r\nlibro|2\r\n(s.m.)|volume|tomo\r\n   (-) |testo\r\n\r\nVeloce|1\r\n[a]|rapido|svelto|\r\n".as_bytes()).unwrap();
    assert_eq!(t.len(), 2);
    assert_eq!(meanings(&t, "libro", "it"), vec![(Some("s.m.".into()), vec!["volume".into(), "tomo".into()]), (None, vec!["testo".into()])]);
    // Case aside; empty synonyms dropped; brackets are a category too.
    assert_eq!(meanings(&t, "VELOCE", "it"), vec![(Some("a".into()), vec!["rapido".into(), "svelto".into()])]);
    assert_eq!(t.lookup("assente", Some("it")), None);
}

#[test]
fn reads_single_byte_encodings() {
    // Spanish in ISO 8859-1 (as the published file is), Russian in KOI8-R and in Windows-1251.
    let mut es = b"ISO8859-1\n".to_vec();
    es.extend(b"alt\xedsimo|1\n-|Dios|Alt\xedsimo\n");
    let t = parse(&es).unwrap();
    assert_eq!(meanings(&t, "Alt\u{ed}simo", "es"), vec![(None, vec!["Dios".into(), "Alt\u{ed}simo".into()])]);
    for (label, word) in
        [("KOI8-R", &b"\xd3\xcc\xcf\xd7\xcf"[..]), ("CP-1251", &b"\xf1\xeb\xee\xe2\xee"[..]), ("ISO-8859-5", &b"\xe1\xdb\xde\xd2\xde"[..])]
    {
        let mut ru = format!("{label}\n").into_bytes();
        ru.extend(word);
        ru.extend(b"|1\n-|x\n");
        let t = parse(&ru).unwrap_or_else(|e| panic!("{label}: {e}"));
        assert_eq!(meanings(&t, "слово", "ru"), vec![(None, vec!["x".into()])], "{label}");
    }
}

#[test]
fn finds_a_words_base_form() {
    let t = parse("UTF-8\ncasa|1\n-|dimora\nhouse|1\n(noun)|home\nrun|1\n(verb)|race\nacqua|1\n-|liquido\nteacher|1\n(noun)|instructor\n".as_bytes())
        .unwrap();
    assert_eq!(t.lookup("case", Some("it-IT")).map(|(w, _)| w), Some("casa".into()));
    assert_eq!(t.lookup("houses", Some("en-US")).map(|(w, _)| w), Some("house".into()));
    assert_eq!(t.lookup("teacher\u{2019}s", None).map(|(w, _)| w), Some("teacher".into()));
    assert_eq!(t.lookup("l\u{2019}acqua", Some("it")).map(|(w, _)| w), Some("acqua".into()));
    assert_eq!(t.lookup("«casa»", Some("it")).map(|(w, _)| w), Some("casa".into()));
    // No ending rules for a language without simple ones; nothing shorter than 3 letters stripped.
    assert_eq!(t.lookup("houses", Some("pl")), None);
    assert!(base_forms("is", Some("en")).iter().all(|b| b.chars().count() >= 2));
}

/// A thesaurus of one entry followed by `n` copies of `line`.
fn stray(n: usize, line: &str) -> Vec<u8> {
    format!("UTF-8\ncasa|1\n-|dimora\n{}", format!("{line}\n").repeat(n)).into_bytes()
}

#[test]
fn a_few_stray_lines_are_skipped() {
    // As in a published file: a meaning broken over two lines, then a count smaller than the
    // meanings there are, and a meaning without a separator. The entries around them still read.
    let t = parse("UTF-8\nmetoda|1\n-|metoda|sposób|środek;\ntechnika\nmeteor|1\n-|aerolit\n-|bolid\nparola|2\n-|termine\nsenza barra\n".as_bytes())
        .unwrap();
    assert_eq!(meanings(&t, "metoda", "pl"), vec![(None, vec!["metoda".into(), "sposób".into(), "środek;".into()])]);
    assert_eq!(meanings(&t, "meteor", "pl"), vec![(None, vec!["aerolit".into()])]);
    assert_eq!(meanings(&t, "parola", "it"), vec![(None, vec!["termine".into()])]);
    assert!(parse(&stray(STRAY_LINES, "riga spuria")).is_ok());
}

#[test]
fn a_stray_first_meaning_line_does_not_reach_the_next_entry() {
    // The meanings start on the line after the entry: a stray line there doesn't shift them
    // onto the next entry's line.
    let t = parse("UTF-8\nparola|2\nsenza barra\n-|termine\nnext|1\n-|x\n".as_bytes()).unwrap();
    assert_eq!(meanings(&t, "parola", "it"), vec![(None, vec!["termine".into()])]);
    assert_eq!(meanings(&t, "next", "it"), vec![(None, vec!["x".into()])]);
}

#[test]
fn a_count_larger_than_the_meanings_does_not_swallow_the_next_entry() {
    let t = parse("UTF-8\nparola|3\n-|termine\nnext|1\n-|x\nultima|2\n(s.f.)|fine\n".as_bytes());
    // The last entry still declares more than the file has: that stays an error.
    assert!(t.unwrap_err().contains("declares 2 meanings, the file ends after 1"));
    let t = parse("UTF-8\nparola|3\n-|termine\nnext|1\n-|x\n(s.)|y\n".as_bytes()).unwrap();
    assert_eq!(meanings(&t, "parola", "it"), vec![(None, vec!["termine".into()])]);
    assert_eq!(meanings(&t, "next", "it"), vec![(None, vec!["x".into()])]);
    assert_eq!(t.len(), 2);
}

#[test]
fn hostile_files_are_errors_never_panics() {
    let long = format!("UTF-8\n{}|1\n-|a\n", "x".repeat(MAX_LINE + 1));
    let many = format!("UTF-8\nparola|{}\n-|a\n", MAX_MEANINGS + 1);
    let huge = format!("UTF-8\nparola|{}\n-|a\n", "9".repeat(40));
    let cases: Vec<(Vec<u8>, &str)> = vec![
        (Vec::new(), "empty"),
        (b"  \n\n".to_vec(), "empty"),
        (b"UTF-8\n".to_vec(), "no entries"),
        (b"UTF-8\nparola|0\n".to_vec(), "no entries"),
        (b"UTF-16\nparola|1\n-|a\n".to_vec(), "unsupported encoding"),
        (b"ISCII-DEVANAGARI\nparola|1\n-|a\n".to_vec(), "unknown encoding"),
        (b"UTF-8\nparola|2\n-|a\n".to_vec(), "declares 2 meanings, the file ends after 1"),
        (b"UTF-8\nparola|1\n".to_vec(), "the file ends after 0"),
        // Lines that aren't entries nor meanings, past the few a published file may have.
        (stray(STRAY_LINES + 1, "parola"), "not a thesaurus"),
        (stray(STRAY_LINES + 1, "parola|-1"), "not a thesaurus"),
        (stray(STRAY_LINES + 1, "|1"), "not a thesaurus"),
        (b"UTF-8\nparola|1\nsenza barra\n".to_vec(), "no entries"),
        (many.into_bytes(), "more meanings"),
        (huge.into_bytes(), "more meanings"),
        (long.into_bytes(), "longer than"),
        (b"UTF-8\npar\xffola|1\n-|a\n".to_vec(), "not valid UTF-8"),
        (b"UTF-8\nparola|1\n-|a\x00\n".to_vec(), "not a text file"),
        (b"<html><body>404</body></html>".to_vec(), "web page"),
    ];
    for (bytes, want) in cases {
        let e = parse(&bytes).map(|t| t.len()).unwrap_err();
        assert!(e.contains(want), "{want}: {e}");
    }
    // Truncated anywhere: an error or a smaller thesaurus, never a panic.
    let good = "UTF-8\nàèìòù|2\n(s.)|è|ò\n-|ù\nb|1\n-|c\n".as_bytes();
    for cut in 0..good.len() {
        let _ = parse(good.get(..cut).unwrap_or_default());
    }
}

#[test]
fn a_file_over_the_cap_is_refused_before_reading() {
    let dir = std::env::temp_dir().join(format!("strix-mythes-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("th_xx.dat");
    std::fs::write(&path, "UTF-8\nparola|1\n-|termine\n").unwrap();
    assert!(load(&path, 10).unwrap_err().contains("larger than"));
    assert_eq!(load(&path, MAX_FILE).unwrap().len(), 1);
    assert!(load(&dir.join("missing.dat"), MAX_FILE).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

/// The published thesauri, when a folder of them is given (`STRIX_REAL_THESAURI`): each reads,
/// and a common word of its language is found.
#[test]
fn reads_real_thesauri_when_given() {
    let Some(dir) = std::env::var_os("STRIX_REAL_THESAURI") else { return };
    let dir = std::path::PathBuf::from(dir);
    for (file, lang, word) in [
        ("th_it_IT_v2.dat", "it", "casa"),
        ("th_es_v2.dat", "es", "casa"),
        ("th_pt_PT.dat", "pt-PT", "casa"),
        ("th_pl_PL_v2.dat", "pl", "dom"),
        ("th_ru_RU_M_aot_and_v2.dat", "ru", "дом"),
        ("th_id_ID_v2.dat", "id", "rumah"),
        ("thes_cs_CZ.dat", "cs", "dům"),
        ("th_en_US_v2.dat", "en", "house"),
    ] {
        let path = dir.join(file);
        if !path.exists() {
            continue;
        }
        let t = load(&path, MAX_FILE).unwrap_or_else(|e| panic!("{e}"));
        let found = t.lookup(word, Some(lang));
        assert!(found.as_ref().is_some_and(|(_, m)| !m.is_empty()), "{file}: {word}");
        eprintln!("{file}: {} words; {word}: {:?}", t.len(), found.map(|(_, m)| m.into_iter().take(2).collect::<Vec<_>>()));
    }
}
