use serde_json::json;

use crate::Session;

fn run(s: &mut Session, id: &str, v: serde_json::Value) -> serde_json::Value {
    s.run(id, &v).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn dir(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("strix-master-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A Strix document at `path` with these paragraphs.
fn file(path: &std::path::Path, text: &str) {
    let mut s = Session::new(wordcraft_doc::Document::new());
    run(&mut s, "document.setText", json!({"text": text}));
    run(&mut s, "file.saveAs", json!({"path": path.to_str().unwrap()}));
}

fn text_of(path: &std::path::Path) -> String {
    crate::io::open_path(path).unwrap().plain_text(wordcraft_doc::StoryRef::Body)
}

fn body(s: &Session) -> String {
    s.doc.plain_text(wordcraft_doc::StoryRef::Body)
}

#[test]
fn edits_in_the_master_go_back_to_the_subdocument_file() {
    let d = dir("edit");
    let (ch1, ch2, book) = (d.join("ch1.xmath"), d.join("ch2.xmath"), d.join("book.xmath"));
    file(&ch1, "Chapter one\nFirst text");
    file(&ch2, "Chapter two\nSecond text");
    let mut s = Session::new(wordcraft_doc::Document::new());
    run(&mut s, "document.setText", json!({"text": "Book"}));
    run(&mut s, "caret.docEnd", json!({}));
    run(&mut s, "master.insertSubdocument", json!({"path": ch1.to_str().unwrap()}));
    run(&mut s, "caret.docEnd", json!({}));
    run(&mut s, "master.insertSubdocument", json!({"path": ch2.to_str().unwrap()}));
    let t = body(&s);
    assert!(t.contains("Book") && t.contains("First text") && t.contains("Second text"), "{t:?}");
    let list = run(&mut s, "master.subdocuments", json!({}));
    assert_eq!(list["subdocuments"].as_array().unwrap().len(), 2);
    assert!(s.run("master.insertSubdocument", &json!({"path": ch1.to_str().unwrap()})).is_err(), "linked twice");

    // Edit chapter one in the master and save: its file changes, chapter two's doesn't.
    let ch2_before = std::fs::read(&ch2).unwrap();
    run(&mut s, "select.text", json!({"text": "First text"}));
    run(&mut s, "text.insert", json!({"text": "Edited in the book"}));
    let r = run(&mut s, "file.saveAs", json!({"path": book.to_str().unwrap()}));
    assert_eq!(r["subdocuments"]["saved"].as_array().unwrap().len(), 1, "{r}");
    assert!(text_of(&ch1).contains("Edited in the book"), "{}", text_of(&ch1));
    assert!(!text_of(&ch1).contains("Book"), "only the subdocument's own content");
    assert_eq!(std::fs::read(&ch2).unwrap(), ch2_before, "unchanged subdocument not rewritten");

    // Change chapter two on its own; reopening the master shows it.
    file(&ch2, "Chapter two\nChanged alone");
    let mut s = Session::new(wordcraft_doc::Document::new());
    run(&mut s, "file.open", json!({"path": book.to_str().unwrap()}));
    let t = body(&s);
    assert!(t.contains("Changed alone") && t.contains("Edited in the book") && !t.contains("Second text"), "{t:?}");
    assert_eq!(run(&mut s, "master.subdocuments", json!({}))["subdocuments"].as_array().unwrap().len(), 2);

    // Unlink keeps the text, without the link.
    run(&mut s, "master.unlinkSubdocument", json!({"path": ch2.to_str().unwrap()}));
    assert!(body(&s).contains("Changed alone"));
    assert_eq!(run(&mut s, "master.subdocuments", json!({}))["subdocuments"].as_array().unwrap().len(), 1);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn changes_on_both_sides_are_a_conflict_not_an_overwrite() {
    let d = dir("conflict");
    let ch = d.join("ch.xmath");
    file(&ch, "Original");
    let mut s = Session::new(wordcraft_doc::Document::new());
    run(&mut s, "master.insertSubdocument", json!({"path": ch.to_str().unwrap()}));
    run(&mut s, "select.text", json!({"text": "Original"}));
    run(&mut s, "text.insert", json!({"text": "Master side"}));
    file(&ch, "Disk side");
    let r = run(&mut s, "master.saveSubdocuments", json!({}));
    assert_eq!(r["conflicts"].as_array().unwrap().len(), 1, "{r}");
    assert!(text_of(&ch).contains("Disk side"));
    let r = run(&mut s, "master.saveSubdocuments", json!({"force": true}));
    assert_eq!(r["saved"].as_array().unwrap().len(), 1, "{r}");
    assert!(text_of(&ch).contains("Master side"));
    // Reload takes the file's content back.
    file(&ch, "Disk again");
    run(&mut s, "master.refresh", json!({}));
    assert!(body(&s).contains("Disk again") && !body(&s).contains("Master side"), "{:?}", body(&s));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn create_makes_a_file_of_the_selected_paragraphs() {
    let d = dir("create");
    let ch = d.join("part");
    let mut s = Session::new(wordcraft_doc::Document::new());
    run(&mut s, "document.setText", json!({"text": "Intro\nMoved one\nMoved two\nEnd"}));
    run(&mut s, "select.text", json!({"text": "Moved one"}));
    run(&mut s, "caret.paraDown", json!({"extend": true}));
    run(&mut s, "master.createSubdocument", json!({"path": ch.to_str().unwrap()}));
    let file = d.join("part.xmath");
    let t = text_of(&file);
    assert!(t.contains("Moved one") && !t.contains("Intro") && !t.contains("End"), "{t:?}");
    let b = body(&s);
    assert!(b.contains("Intro") && b.contains("Moved one") && b.contains("End"), "{b:?}");
    assert!(s.run("master.createSubdocument", &json!({"path": file.to_str().unwrap()})).is_err(), "won't replace a file unasked");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn bad_links_are_errors() {
    let mut s = Session::new(crate::sample::sample_document());
    assert!(s.run("master.insertSubdocument", &json!({"path": "/nonexistent/x.xmath"})).is_err());
    assert!(s.run("master.unlinkSubdocument", &json!({})).is_err());
    assert!(s.run("master.refresh", &json!({"path": "/x"})).is_err());
    assert_eq!(super::Link::parse("strix-subdoc path="), None);
    assert_eq!(super::Link::parse("strix-subdoc conflict path=/a b").map(|l| (l.conflict, l.path)), Some((true, "/a b".into())));
}

/// One exercise in its own file, Italian.
fn exercise_file(path: &std::path::Path, title: &str) {
    let mut s = Session::new(wordcraft_doc::Document::new());
    std::sync::Arc::make_mut(&mut s.doc.styles).default_chr.lang = Some("it-IT".into());
    run(&mut s, "exercise.insert", json!({"title": title, "questions": ["Trova x"]}));
    run(&mut s, "file.saveAs", json!({"path": path.to_str().unwrap()}));
}

fn paras(doc: &wordcraft_doc::Document) -> Vec<(Option<String>, String)> {
    doc.body
        .iter()
        .filter_map(|b| match &**b {
            wordcraft_doc::Block::Para(p) => Some((p.props.style.clone(), p.text.replace('\u{FFFC}', ""))),
            _ => None,
        })
        .collect()
}

fn count(doc: &wordcraft_doc::Document, text: &str) -> usize {
    paras(doc).iter().filter(|(_, t)| t == text).count()
}

/// The owner: exercises kept one per file and gathered in a master showed the results summary
/// twice, lost their styles (titles not in Exercise Title, questions without their letters),
/// left empty paragraphs between the files and didn't follow the series. The master rules, as a
/// style update: the files' styles and lists come in, one summary for all, the series' settings
/// on every exercise. Then: "the series' settings are in the master; the files of the single
/// exercises keep their formatting": written back, a file keeps its own settings (with its own
/// summary, so each still opens complete), and the master reopened keeps the master's.
#[test]
fn exercises_in_subdocuments_follow_the_master() {
    let d = dir("exercises");
    let (a, b, book) = (d.join("a.xmath"), d.join("b.xmath"), d.join("book.xmath"));
    exercise_file(&a, "Area");
    exercise_file(&b, "Pendenza");
    let mut s = Session::new(wordcraft_doc::Document::new());
    std::sync::Arc::make_mut(&mut s.doc.styles).default_chr.lang = Some("it-IT".into());
    run(&mut s, "document.setText", json!({"text": "Libro"}));
    for p in [&a, &b] {
        run(&mut s, "caret.docEnd", json!({}));
        run(&mut s, "master.insertSubdocument", json!({"path": p.to_str().unwrap()}));
    }
    let check = |s: &Session| {
        assert_eq!(count(&s.doc, "Riepilogo dei risultati"), 1, "{:?}", paras(&s.doc));
        assert_eq!(count(&s.doc, "Esercizio 1") + count(&s.doc, "Esercizio 2"), 2, "{:?}", paras(&s.doc));
        // No empty paragraph left between the subdocuments, nor before the summary.
        let p = paras(&s.doc);
        let empties = p.iter().filter(|(st, t)| t.is_empty() && st.as_deref() != Some("Normal")).count();
        assert_eq!(empties, 0, "{p:?}");
        // Titles numbered by the Exercise Title style, questions lettered by an existing list.
        let title = s.doc.styles.get("ExerciseTitle").expect("the style came in");
        let num = title.para.numbering.expect("numbered").num;
        assert!(s.doc.numbering.num(num).is_some());
        for blk in s.doc.body.iter() {
            if let wordcraft_doc::Block::Para(p) = &**blk
                && p.props.style.as_deref() == Some("ExerciseQuestion")
            {
                let n = p.props.numbering.expect("lettered");
                assert!(s.doc.numbering.num(n.num).is_some(), "the question's list exists");
            }
        }
    };
    check(&s);
    assert_eq!(run(&mut s, "exercise.numbering", json!({}))["example"], json!("Esercizio 1"));
    // A series setting made in the master is the master's: every exercise shown there has it…
    run(&mut s, "exercise.pageBreaks", json!({"beforeSolution": true}));
    run(&mut s, "exercise.show", json!({"solution": false}));
    // …and a text edit in a file's exercise still goes to the file.
    run(&mut s, "select.text", json!({"text": "Pendenza"}));
    run(&mut s, "text.insert", json!({"text": "Pendenza della rampa"}));
    run(&mut s, "file.saveAs", json!({"path": book.to_str().unwrap()}));
    for p in [&a, &b] {
        let mut alone = Session::new(crate::io::open_path(p).unwrap());
        std::sync::Arc::make_mut(&mut alone.doc.styles).default_chr.lang = Some("it-IT".into());
        assert_eq!(count(&alone.doc, "Riepilogo dei risultati"), 1, "{:?}", paras(&alone.doc));
        // The file keeps its own settings.
        assert_eq!(run(&mut alone, "exercise.pageBreaks", json!({}))["beforeSolution"], json!(false), "{}", p.display());
        assert_eq!(run(&mut alone, "exercise.show", json!({}))["solution"], json!(true), "{}", p.display());
    }
    assert!(text_of(&b).contains("Pendenza della rampa"));
    // Reopened: the files come in again, still one summary, with the master's settings.
    let mut s = Session::new(crate::io::open_path(&book).unwrap());
    s.path = Some(book.clone());
    std::sync::Arc::make_mut(&mut s.doc.styles).default_chr.lang = Some("it-IT".into());
    crate::cmd::master::after_open(&mut s);
    check(&s);
    assert_eq!(run(&mut s, "exercise.pageBreaks", json!({}))["beforeSolution"], json!(true));
    assert_eq!(run(&mut s, "exercise.show", json!({}))["solution"], json!(false));
    let edited =
        run(&mut s, "master.subdocuments", json!({}))["subdocuments"].as_array().unwrap().iter().filter(|x| x["edited"] == json!(true)).count();
    assert_eq!(edited, 0, "the master's settings are no edit of the files");
}

/// The owner: "another master calls up a series: its frontispiece changed there, its layout and
/// what it shows, while the other files stay as they were first thought out". A series file with
/// a frontispiece, linked in a master with its own: the series' frontispiece is left out of the
/// master, and its file keeps it when the master writes an edit back.
#[test]
fn a_series_in_another_master_keeps_its_own_frontispiece() {
    let d = dir("front");
    let (a, series, book) = (d.join("a.xmath"), d.join("series.xmath"), d.join("book.xmath"));
    exercise_file(&a, "Area");
    // The series: a frontispiece then the exercise file's exercises (here copied in, as a master's
    // copy would be).
    let mut sr = Session::new(crate::io::open_path(&a).unwrap());
    std::sync::Arc::make_mut(&mut sr.doc.styles).default_chr.lang = Some("it-IT".into());
    run(&mut sr, "caret.docEnd", json!({}));
    run(&mut sr, "exercise.insert", json!({"title": "Volume", "questions": ["q"]}));
    run(&mut sr, "exercise.frontispiece", json!({"on": true}));
    run(&mut sr, "text.insert", json!({"text": "Verifica di prima"}));
    run(&mut sr, "file.saveAs", json!({"path": series.to_str().unwrap()}));
    let mut s = Session::new(wordcraft_doc::Document::new());
    std::sync::Arc::make_mut(&mut s.doc.styles).default_chr.lang = Some("it-IT".into());
    run(&mut s, "document.setText", json!({"text": ""}));
    run(&mut s, "master.insertSubdocument", json!({"path": series.to_str().unwrap()}));
    assert!(!body(&s).contains("Verifica di prima"), "{}", body(&s));
    run(&mut s, "exercise.frontispiece", json!({"on": true}));
    run(&mut s, "text.insert", json!({"text": "Recupero"}));
    run(&mut s, "select.text", json!({"text": "Volume"}));
    run(&mut s, "text.insert", json!({"text": "Volume del cono"}));
    run(&mut s, "file.saveAs", json!({"path": book.to_str().unwrap()}));
    let file = text_of(&series);
    assert!(file.contains("Verifica di prima") && !file.contains("Recupero") && file.contains("Volume del cono"), "{file}");
}

/// The owner: in a master of exercises Unlink, Reload and Open did nothing. The caret is in an
/// exercise there, which is a content control: Insert refused to go in it (and the app swallowed
/// the error), so no subdocument came in and the buttons stayed grey. Now it goes after the
/// exercise, and with one subdocument Unlink and Open act on it wherever the caret is.
#[test]
fn insert_with_the_caret_in_an_exercise_goes_after_it() {
    let d = dir("in-exercise");
    let (a, b) = (d.join("a.xmath"), d.join("b.xmath"));
    exercise_file(&a, "Area");
    exercise_file(&b, "Pendenza");
    let mut s = Session::new(wordcraft_doc::Document::new());
    std::sync::Arc::make_mut(&mut s.doc.styles).default_chr.lang = Some("it-IT".into());
    run(&mut s, "master.insertSubdocument", json!({"path": a.to_str().unwrap()}));
    // The caret is in the first file's exercise title: the next one goes after that subdocument.
    assert!(s.doc.control_at(&s.sel.focus).is_some());
    run(&mut s, "master.insertSubdocument", json!({"path": b.to_str().unwrap()}));
    let subs = run(&mut s, "master.subdocuments", json!({}));
    let paths: Vec<_> = subs["subdocuments"].as_array().unwrap().iter().map(|x| x["path"].as_str().unwrap().to_string()).collect();
    assert_eq!(paths, [a.to_str().unwrap(), b.to_str().unwrap()]);
    assert_eq!(count(&s.doc, "Riepilogo dei risultati"), 1, "{:?}", paras(&s.doc));
    // With one left, Unlink acts on it wherever the caret is (here in the results summary).
    run(&mut s, "master.unlinkSubdocument", json!({"path": b.to_str().unwrap()}));
    run(&mut s, "caret.docEnd", json!({}));
    assert_eq!(run(&mut s, "master.unlinkSubdocument", json!({}))["unlinked"], json!(a));
    assert!(run(&mut s, "master.subdocuments", json!({}))["subdocuments"].as_array().unwrap().is_empty());
}

/// The master's styles and numbering are the master's: a change to the exercises' numbering (a
/// list) made in the master is no edit of the subdocuments, and their files keep their own.
#[test]
fn style_and_numbering_changes_in_the_master_stay_in_the_master() {
    let d = dir("context");
    let (a, b, book) = (d.join("a.xmath"), d.join("b.xmath"), d.join("book.xmath"));
    exercise_file(&a, "Area");
    exercise_file(&b, "Pendenza");
    let mut s = Session::new(wordcraft_doc::Document::new());
    std::sync::Arc::make_mut(&mut s.doc.styles).default_chr.lang = Some("it-IT".into());
    run(&mut s, "document.setText", json!({"text": "Libro"}));
    for p in [&a, &b] {
        run(&mut s, "caret.docEnd", json!({}));
        run(&mut s, "master.insertSubdocument", json!({"path": p.to_str().unwrap()}));
    }
    run(&mut s, "file.saveAs", json!({"path": book.to_str().unwrap()}));
    let edited = |s: &mut Session| {
        run(s, "master.subdocuments", json!({}))["subdocuments"].as_array().unwrap().iter().filter(|x| x["edited"] == json!(true)).count()
    };
    assert_eq!(edited(&mut s), 0, "just saved");
    // Reopened, nothing is edited either.
    let mut s = Session::new(crate::io::open_path(&book).unwrap());
    s.path = Some(book.clone());
    std::sync::Arc::make_mut(&mut s.doc.styles).default_chr.lang = Some("it-IT".into());
    crate::cmd::master::after_open(&mut s);
    run(&mut s, "file.save", json!({}));
    assert_eq!(edited(&mut s), 0, "saved after opening");
    run(&mut s, "exercise.numbering", json!({"separator": ")"}));
    assert_eq!(edited(&mut s), 0, "the numbering is the master's");
    let r = run(&mut s, "file.save", json!({}));
    assert_eq!(r["subdocuments"]["saved"].as_array().unwrap().len(), 0, "{r}");
    for p in [&a, &b] {
        let mut alone = Session::new(crate::io::open_path(p).unwrap());
        std::sync::Arc::make_mut(&mut alone.doc.styles).default_chr.lang = Some("it-IT".into());
        assert_eq!(run(&mut alone, "exercise.numbering", json!({}))["separator"], json!("."), "{}", p.display());
    }
    // Reopened, the master still has its own.
    let mut s = Session::new(crate::io::open_path(&book).unwrap());
    s.path = Some(book.clone());
    std::sync::Arc::make_mut(&mut s.doc.styles).default_chr.lang = Some("it-IT".into());
    crate::cmd::master::after_open(&mut s);
    assert_eq!(run(&mut s, "exercise.numbering", json!({}))["separator"], json!(")"));
}
