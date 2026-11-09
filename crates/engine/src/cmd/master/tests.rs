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
