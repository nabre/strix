//! Strix objects (`strix_objects`): a document saves back to the editable format it came from
//! (.docx, .odt, .rtf…) like in the reference word processor, unless it has calculations,
//! calculated tables, exercises, CAD drawings or graph paper, which only `.xmath` keeps live.

use serde_json::json;
use strix_doc::StoryRef;

use crate::strix_objects::{StrixObjectCount, StrixObjectKind, check_save, describe, has_strix_objects, saves_in_place, strix_objects};
use crate::{Session, io};

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("strix-objects-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn session(text: &str) -> Session {
    Session::new(strix_doc::Document::from_text(text))
}

fn count(s: &Session, kind: StrixObjectKind) -> usize {
    strix_objects(&s.doc).iter().find(|o| o.kind == kind).map_or(0, |o| o.count)
}

/// A document with three calculations and a CAD drawing.
fn with_calculations() -> Session {
    let mut s = session("Beam");
    s.run("caret.docEnd", &json!({})).unwrap();
    for text in ["L = 4.5 m", "q = 2 kN/m", "M = q*L^2/8"] {
        s.run("calc.insert", &json!({"text": text})).unwrap();
        s.run("text.insert", &json!({"text": " "})).unwrap();
    }
    s.run("insert.cad", &json!({})).unwrap();
    s
}

#[test]
fn a_plain_document_has_no_strix_objects() {
    let mut s = session("Plain text");
    s.run("insert.table", &json!({"rows": 2, "cols": 2})).unwrap();
    assert!(strix_objects(&s.doc).is_empty());
    assert!(!has_strix_objects(&s.doc));
    assert_eq!(describe(&[]), "");
}

#[test]
fn calculations_and_cad_drawings_are_counted() {
    let s = with_calculations();
    let found = strix_objects(&s.doc);
    assert_eq!(
        found,
        [StrixObjectCount { kind: StrixObjectKind::Calculation, count: 3 }, StrixObjectCount { kind: StrixObjectKind::CadDrawing, count: 1 }]
    );
    assert_eq!(describe(&found), "3 calculations, 1 CAD drawing");
}

#[test]
fn exercises_calculated_tables_and_graph_paper_are_counted() {
    let mut s = session("Work");
    s.run("exercise.insert", &json!({"title": "Primo", "questions": ["a"]})).unwrap();
    assert_eq!(count(&s, StrixObjectKind::Exercise), 1, "{:?}", strix_objects(&s.doc));
    assert_eq!(count(&s, StrixObjectKind::ResultsSummary), 1, "{:?}", strix_objects(&s.doc));

    let mut s = session("Numbers");
    s.run("insert.table", &json!({"rows": 2, "cols": 2})).unwrap();
    s.run("table.calc", &json!({"value": true})).unwrap();
    assert_eq!(count(&s, StrixObjectKind::CalculatedTable), 1);

    let mut s = session("Grid");
    s.run("graph.insert", &json!({})).unwrap();
    assert_eq!(count(&s, StrixObjectKind::GraphPaper), 1);
    assert_eq!(describe(&strix_objects(&s.doc)), "graph paper");
}

/// Objects in a header, a text box or a table cell count as much as in the body.
#[test]
fn objects_outside_the_body_text_are_found() {
    let s = with_calculations();
    let mut doc = strix_doc::Document::from_text("Cover");
    // The calculations' paragraph moves into a header part and inside a table cell.
    let para = s.doc.body.iter().find(|b| b.as_para().is_some_and(|p| !p.objects.is_empty())).cloned().unwrap();
    doc.parts.insert(7, strix_doc::Part { kind: strix_doc::PartKind::Header, blocks: vec![para.clone()], body: Default::default() });
    assert_eq!(describe(&strix_objects(&doc)), "3 calculations, 1 CAD drawing");
    let mut table = strix_doc::Document::from_text("x");
    let mut t = session("t");
    t.run("insert.table", &json!({"rows": 1, "cols": 1})).unwrap();
    let mut tb = t.doc.body.iter().find(|b| matches!(&***b, strix_doc::Block::Table(_))).cloned().unwrap();
    if let strix_doc::Block::Table(tt) = std::sync::Arc::make_mut(&mut tb) {
        tt.rows[0].cells[0].blocks = vec![para];
    }
    table.body = vec![tb];
    assert_eq!(describe(&strix_objects(&table)), "3 calculations, 1 CAD drawing");
}

#[test]
fn check_save_refuses_lossy_formats_and_allows_the_rest() {
    let s = with_calculations();
    for lossy in ["a.docx", "a.DOCM", "a.dotx", "a.dotm", "a.odt", "a.rtf", "a.txt", "a.xml", "docx"] {
        let e = check_save(lossy, &s.doc).unwrap_err();
        assert!(e.contains("3 calculations, 1 CAD drawing") && e.contains(".xmath"), "{lossy}: {e}");
    }
    assert!(check_save("/d/report.docx", &s.doc).unwrap_err().contains("report.xmath"));
    // Strix's own formats and the static exports stay allowed: exports are copies, not saves.
    for ok in ["a.xmath", "a.xmathm", "a.pdf", "a.png", "a.tex", "a.html", "a.md"] {
        check_save(ok, &s.doc).unwrap();
    }
    // A format Strix only reads is refused whatever the document holds.
    assert!(check_save("a.doc", &session("x").doc).unwrap_err().contains("only opens it"));
    for ok in ["a.docx", "a.odt", "a.rtf", "a.txt", "a.xml", "a.xmath", "a.pdf"] {
        check_save(ok, &session("x").doc).unwrap();
    }
    let p = std::path::Path::new;
    assert!(saves_in_place(p("/d/a.xmath"), &s.doc) && !saves_in_place(p("/d/a.docx"), &s.doc));
    assert!(saves_in_place(p("/d/a.docx"), &session("x").doc) && !saves_in_place(p("/d/a.doc"), &session("x").doc));
    assert!(!saves_in_place(p("/d/a.pdf"), &session("x").doc));
}

/// A document without Strix objects opened from .docx saves back to it in place, as in the
/// reference word processor, and Save As .odt/.rtf makes that its file.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn a_docx_without_strix_objects_saves_back_to_docx() {
    let dir = scratch("docx");
    let path = dir.join("letter.docx");
    let mut s = session("Dear reader");
    s.run("file.saveAs", &json!({"path": path.to_string_lossy()})).unwrap();
    assert_eq!(s.path.as_deref(), Some(path.as_path()), "the .docx is the document's file");
    assert!(!s.dirty);

    let mut s = Session::new(io::open_path(&path).unwrap());
    s.path = Some(path.clone());
    s.run("caret.docEnd", &json!({})).unwrap();
    s.run("text.insert", &json!({"text": ", hello"})).unwrap();
    s.run("file.save", &json!({})).unwrap();
    assert!(!s.dirty, "saved in place");
    assert_eq!(s.path.as_deref(), Some(path.as_path()));
    let back = io::open_path(&path).unwrap();
    assert!(back.plain_text(StoryRef::Body).contains("Dear reader, hello"));

    for other in ["letter.odt", "letter.rtf"] {
        s.run("file.saveAs", &json!({"path": dir.join(other).to_string_lossy()})).unwrap();
        assert_eq!(s.path.as_deref(), Some(dir.join(other).as_path()), "{other} is the document's file");
        assert!(io::open_path(&dir.join(other)).unwrap().plain_text(StoryRef::Body).contains("hello"), "{other}");
    }
    // A Word document is saved with its password again.
    s.run("file.saveAs", &json!({"path": path.to_string_lossy(), "password": "secret"})).unwrap();
    assert!(strix_docx::is_encrypted(&std::fs::read(&path).unwrap()));
    let _ = std::fs::remove_dir_all(&dir);
}

/// With Strix objects, Save and Save As to an editable foreign format are refused with the
/// objects named and `.xmath` proposed, writing nothing; `.xmath` and static exports still work.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn a_document_with_strix_objects_never_saves_to_a_lossy_format() {
    let dir = scratch("lossy");
    let path = dir.join("beam.docx");
    let mut s = session("Beam");
    s.run("file.saveAs", &json!({"path": path.to_string_lossy()})).unwrap();
    let saved = std::fs::read(&path).unwrap();
    s.run("caret.docEnd", &json!({})).unwrap();
    s.run("calc.insert", &json!({"text": "L = 4.5 m"})).unwrap();
    s.run("insert.cad", &json!({})).unwrap();
    assert!(s.dirty);

    let e = s.run("file.save", &json!({})).unwrap_err().to_string();
    assert!(e.contains("1 calculation, 1 CAD drawing") && e.contains("beam.xmath"), "{e}");
    assert_eq!(std::fs::read(&path).unwrap(), saved, "the .docx is untouched");
    assert!(s.dirty, "nothing saved");
    for lossy in ["beam.odt", "beam.rtf", "beam.txt", "beam.xml", "beam.docm"] {
        let e = s.run("file.saveAs", &json!({"path": dir.join(lossy).to_string_lossy()})).unwrap_err().to_string();
        assert!(e.contains("Strix objects") && e.contains(".xmath"), "{lossy}: {e}");
        assert!(!dir.join(lossy).exists(), "{lossy} written");
    }
    assert_eq!(s.path.as_deref(), Some(path.as_path()));

    // Static exports are copies: allowed, and the document keeps its file.
    for copy in ["beam.pdf", "beam.md", "beam.html", "beam.tex"] {
        s.run("file.saveAs", &json!({"path": dir.join(copy).to_string_lossy()})).unwrap();
        assert!(dir.join(copy).is_file(), "{copy}");
        assert!(s.dirty, "{copy} is a copy");
    }
    // Saving as .xmath keeps everything and makes it the document's file.
    let native = dir.join("beam.xmath");
    s.run("file.saveAs", &json!({"path": native.to_string_lossy()})).unwrap();
    assert_eq!(s.path.as_deref(), Some(native.as_path()));
    assert!(!s.dirty);
    assert_eq!(describe(&strix_objects(&io::open_path(&native).unwrap())), "1 calculation, 1 CAD drawing");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Hostile documents (deep tables, huge groups) never panic or recurse without bound.
#[test]
fn hostile_documents_never_panic() {
    let mut s = session("x");
    for _ in 0..40 {
        let _ = s.run("insert.table", &json!({"rows": 1, "cols": 1}));
    }
    let _ = strix_objects(&s.doc);
    for name in ["", ".", "..", "a.", "é", "\u{0}", "docx", ".DOCX", "a/b\\c.docx"] {
        let _ = check_save(name, &s.doc);
        let _ = saves_in_place(std::path::Path::new(name), &s.doc);
    }
}
