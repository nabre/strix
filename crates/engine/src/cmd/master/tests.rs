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
    s.doc.styles.default_chr.lang = Some("it-IT".into());
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
/// on every exercise, and the files written back carry them (with their own summary, so each
/// still opens complete).
#[test]
fn exercises_in_subdocuments_follow_the_master() {
    let d = dir("exercises");
    let (a, b, book) = (d.join("a.xmath"), d.join("b.xmath"), d.join("book.xmath"));
    exercise_file(&a, "Area");
    exercise_file(&b, "Pendenza");
    let mut s = Session::new(wordcraft_doc::Document::new());
    s.doc.styles.default_chr.lang = Some("it-IT".into());
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
    // A series setting made in the master reaches every file's exercise.
    run(&mut s, "exercise.pageBreaks", json!({"beforeSolution": true}));
    let r = run(&mut s, "file.saveAs", json!({"path": book.to_str().unwrap()}));
    assert_eq!(r["subdocuments"]["saved"].as_array().unwrap().len(), 2, "{r}");
    for p in [&a, &b] {
        let mut alone = Session::new(crate::io::open_path(p).unwrap());
        alone.doc.styles.default_chr.lang = Some("it-IT".into());
        assert_eq!(count(&alone.doc, "Riepilogo dei risultati"), 1, "{:?}", paras(&alone.doc));
        assert_eq!(run(&mut alone, "exercise.pageBreaks", json!({}))["beforeSolution"], json!(true));
    }
    // Reopened: the files come in again, still one summary.
    let mut s = Session::new(crate::io::open_path(&book).unwrap());
    s.path = Some(book.clone());
    s.doc.styles.default_chr.lang = Some("it-IT".into());
    crate::cmd::master::after_open(&mut s);
    check(&s);
}
