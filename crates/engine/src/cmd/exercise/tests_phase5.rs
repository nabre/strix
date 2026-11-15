//! Laboratory, STEM questions, DSA and archive regressions.
use crate::Session;
use serde_json::{Value, json};
use wordcraft_doc::{Document, StoryRef};
fn run(s: &mut Session, id: &str, v: Value) -> Value {
    s.run(id, &v).unwrap()
}
fn text(s: &Session) -> String {
    s.doc.plain_text(StoryRef::Body)
}

#[test]
fn phase5_cloze_worked_and_matching_student_teacher() {
    let mut s = Session::new(Document::new());
    run(
        &mut s,
        "exercise.insert",
        json!({"kind":"open", "questions":[
            {"text":"Esempio", "type":"worked", "points":10},
            {"text":"La lunghezza è {{2 m -> cm}}", "type":"cloze", "points":2},
            {"text":"Abbina", "type":"matching", "points":3,"pairs":[["massa","kg"],["tempo","s"],["lunghezza","m"]]}
        ]}),
    );
    run(&mut s, "calc.recalculate", json!({}));
    let info = run(&mut s, "exercise.questions", json!({"exercise":1}));
    assert_eq!(info["questions"][0]["points"], Value::Null);
    assert_eq!(info["total"], 5.0);
    assert!(text(&s).contains("200 cm"), "{}", text(&s));
    run(&mut s, "exercise.show", json!({"solution":false}));
    run(&mut s, "calc.recalculate", json!({}));
    assert!(text(&s).contains("____________ cm"), "{}", text(&s));
    assert!(text(&s).contains("[   ]"));
    assert!(!text(&s).contains("[A]"));
    run(&mut s, "exercise.show", json!({"solution":true}));
    run(&mut s, "calc.recalculate", json!({}));
    assert!(text(&s).contains("200 cm"));
    assert!(!text(&s).contains("[   ]"));
    let bytes = crate::io::save_bytes("test.docx", &s.doc).unwrap();
    let mut back = Session::new(crate::io::open_bytes("test.docx", &bytes).unwrap());
    assert_eq!(run(&mut back, "exercise.questions", json!({"exercise":1}))["questions"][2]["type"], "matching");
}

#[test]
fn phase5_worked_example_has_no_scores_row() {
    let mut s = Session::new(Document::from_text("Laboratorio"));
    run(&mut s, "exercise.insert", json!({"kind":"worked","questions":[{"text":"Esempio","points":10}]}));
    run(&mut s, "caret.docEnd", json!({}));
    run(&mut s, "exercise.insert", json!({"kind":"open","questions":[{"text":"Prova","points":4}]}));
    run(&mut s, "caret.docStart", json!({}));
    run(&mut s, "exercise.scores", json!({"on":true}));
    let scores = run(&mut s, "exercise.scores", json!({}));
    assert_eq!(scores["exercises"], 1);
    assert_eq!(scores["points"][0][0], 4.0);
    assert!(s.run("exercise.scores", &json!({"exercise":1,"criterion":1,"points":8})).is_err());
}

#[test]
fn phase5_direct_italic_and_tabular_digits_restore_after_word() {
    let mut s = Session::new(Document::from_text("Laboratorio 12345"));
    run(&mut s, "select.all", json!({}));
    run(&mut s, "format.italic", json!({"value":true}));
    let original = s.doc.body.first().unwrap().as_para().unwrap().runs.clone();
    run(&mut s, "design.dsa", json!({"math":true}));
    let p = s.doc.body.first().unwrap().as_para().unwrap();
    assert!(p.runs.iter().all(|r| r.props.italic == Some(false)));
    let normal = s.doc.styles.get("Normal").unwrap();
    assert_eq!(normal.chr.tabular_digits, Some(true));
    let bytes = crate::io::save_bytes("test.docx", &s.doc).unwrap();
    let mut s = Session::new(crate::io::open_bytes("test.docx", &bytes).unwrap());
    run(&mut s, "design.dsa", json!({"none":true}));
    let back = &s.doc.body.first().unwrap().as_para().unwrap().runs;
    assert_eq!(back.iter().map(|r| r.props.italic).collect::<Vec<_>>(), original.iter().map(|r| r.props.italic).collect::<Vec<_>>());
    assert!(back.iter().all(|r| r.props.style.as_deref().is_none_or(|id| !id.starts_with("StrixDsa"))));
}

#[test]
fn phase5_uncertainty_options_and_docx_roundtrip() {
    let mut s = Session::new(Document::new());
    run(&mut s, "calc.options", json!({"uncertainty":"rss","scope":"document"}));
    run(&mut s, "calc.insert", json!({"text":"x := (3 ± 0.3) + (4 ± 0.4) ="}));
    let r = run(&mut s, "calc.recalculate", json!({}));
    assert_eq!(r["errors"], json!([]));
    let bytes = crate::io::save_bytes("test.docx", &s.doc).unwrap();
    let back = Session::new(crate::io::open_bytes("test.docx", &bytes).unwrap());
    assert_eq!(super::super::calc::resolved_options(&back).uncertainty, strix_calc::UncertaintyMode::Rss);
}

#[test]
fn phase5_archive_metadata_and_zip_bundle() {
    use std::io::{Cursor, Write};
    let mut s = Session::new(Document::new());
    run(&mut s, "archive.properties", json!({"subject":"Fisica","tags":"laboratorio, misure","grade":3}));
    let bytes = crate::io::save_bytes("lab.docx", &s.doc).unwrap();
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("lab.docx", zip::write::SimpleFileOptions::default()).unwrap();
    zip.write_all(&bytes).unwrap();
    let bundle = zip.finish().unwrap().into_inner();
    let data = crate::cmd::insert::base64_encode(&bundle);
    let indexed = run(&mut s, "archive.importBundle", json!({"data":data,"tags":"misure"}));
    assert_eq!(indexed["entries"].as_array().unwrap().len(), 1);
    assert_eq!(indexed["entries"][0]["properties"]["subject"], "Fisica");
    assert_eq!(indexed["errors"], json!([]));
}

#[test]
fn phase5_master_view_does_not_write_clean_subdocument() {
    let dir = std::env::temp_dir().join(format!("strix-phase5-master-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("exercise.xmath");
    let master = dir.join("master.xmath");
    let mut child = Session::new(Document::new());
    run(&mut child, "exercise.insert", json!({"title":"Laboratorio","questions":["Misura"]}));
    run(&mut child, "file.saveAs", json!({"path":file}));
    let original = std::fs::read(&file).unwrap();
    let mut s = Session::new(Document::new());
    run(&mut s, "master.insertSubdocument", json!({"path":file}));
    run(&mut s, "file.saveAs", json!({"path":master}));
    run(&mut s, "exercise.show", json!({"solution":false}));
    run(&mut s, "design.dsa", json!({"math":true,"writing":true}));
    let report = run(&mut s, "master.saveSubdocuments", json!({}));
    assert_eq!(report["saved"], json!([]), "{report}");
    assert_eq!(std::fs::read(&file).unwrap(), original);
    // An actual content edit still gets written, even after a view change.
    run(&mut s, "select.text", json!({"text":"Laboratorio"}));
    run(&mut s, "text.insert", json!({"text":"Misure sperimentali"}));
    run(&mut s, "exercise.show", json!({"solution":true}));
    assert_eq!(run(&mut s, "master.saveSubdocuments", json!({}))["saved"].as_array().unwrap().len(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn phase5_dsa_guided_problem_result() {
    let mut s = Session::new(Document::new());
    run(&mut s, "exercise.insert", json!({"title":"Area","questions":["Calcola l’area"]}));
    let layout = super::scan(&s);
    let exercise = layout.exercises.first().unwrap();
    let free = exercise.solution_blocks.unwrap().1;
    let p = s.doc.body.get(free).unwrap().as_para().unwrap();
    let offset = p.text.len().saturating_sub(wordcraft_doc::para::OBJ.len_utf8());
    s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(StoryRef::Body, super::body_path(free), offset));
    run(&mut s, "calc.insert", json!({"text":"A = 12 cm²"}));
    let mut pos = s.sel.focus.clone();
    pos.off = pos.off.saturating_sub(wordcraft_doc::para::OBJ.len_utf8());
    s.sel = crate::Selection::caret(pos);
    run(&mut s, "equation.answer", json!({"question":1}));
    run(&mut s, "design.dsa", json!({"math":true}));
    run(&mut s, "exercise.show", json!({"solution":false}));
    let guides: Vec<_> = s.doc.body.iter().filter_map(|b| b.as_para()).filter(|p| super::items::span(p, "strix:dsaGuide").is_some()).collect();
    assert_eq!(guides.len(), 1, "{}", text(&s));
    assert!(super::plain(guides[0]).contains("A ="));
    assert!(super::plain(guides[0]).contains("cm²"));
    run(&mut s, "design.dsa", json!({"none":true}));
    assert!(!s.doc.body.iter().filter_map(|b| b.as_para()).any(|p| super::items::span(p, "strix:dsaGuide").is_some()));
}

#[test]
fn phase5_worked_conversion_keeps_other_criterion_points() {
    let mut s = Session::new(Document::from_text("Laboratorio"));
    for title in ["Uno", "Due"] {
        run(&mut s, "caret.docEnd", json!({}));
        run(&mut s, "exercise.insert", json!({"kind":"open","title":title,"questions":[{"text":"Domanda","points":2}]}));
    }
    run(&mut s, "caret.docStart", json!({}));
    run(&mut s, "exercise.scores", json!({"on":true}));
    run(&mut s, "exercise.scores", json!({"add":"Metodo"}));
    // The question points are the row's total: the first criterion is typed, the last balances it.
    run(&mut s, "exercise.scores", json!({"exercise":2,"criterion":1,"points":1.5}));
    run(&mut s, "exercise.questionMode", json!({"exercise":1,"question":1,"type":"worked"}));
    let scores = run(&mut s, "exercise.scores", json!({}));
    assert_eq!(scores["points"][0], json!([1.5, 0.5]));
    run(&mut s, "exercise.questionMode", json!({"exercise":1,"question":1,"type":"open"}));
    let scores = run(&mut s, "exercise.scores", json!({}));
    assert_eq!(scores["points"][1], json!([1.5, 0.5]));
}

#[test]
fn phase5_archive_scan_respects_automation_read_root() {
    let dir = std::env::temp_dir().join(format!("strix-phase5-archive-{}", std::process::id()));
    let allowed = dir.join("allowed");
    let outside = dir.join("outside");
    std::fs::create_dir_all(&allowed).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    let mut s = Session::new(Document::new());
    let bytes = crate::io::save_bytes("lab.docx", &s.doc).unwrap();
    std::fs::write(allowed.join("lab.docx"), &bytes).unwrap();
    let roots = crate::file_access::AutomationRoots::new(Some(&allowed), None).unwrap().unwrap();
    crate::file_access::confine(Some(&roots), || {
        assert_eq!(run(&mut s, "archive.scan", json!({"path":allowed}))["entries"].as_array().unwrap().len(), 1);
        assert!(s.run("archive.scan", &json!({"path":outside})).is_err());
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn phase5_blank_at_existing_expression_targets_that_expression() {
    let mut s = Session::new(Document::new());
    run(&mut s, "exercise.insertExpr", json!({"expr":"2 m"}));
    run(&mut s, "text.insert", json!({"text":" "}));
    run(&mut s, "exercise.insertExpr", json!({"expr":"3 m"}));
    let focus = s.sel.focus.clone();
    let spans = super::expr::expr_spans(s.doc.para(focus.story, &focus.path).unwrap());
    s.sel.focus.off = spans.get(1).unwrap().0 + wordcraft_doc::para::OBJ.len_utf8();
    s.sel.anchor = s.sel.focus.clone();
    run(&mut s, "exercise.blank", json!({"expr":"4 m"}));
    let p = s.doc.para(focus.story, &focus.path).unwrap();
    let tags: Vec<_> = p
        .objects
        .iter()
        .filter_map(|o| if let wordcraft_doc::InlineObject::ControlStart { control } = o { Some(control.tag.as_str()) } else { None })
        .collect();
    assert_eq!(tags, vec![super::expr::TAG_EXPR, "strix:blank"]);
}

#[test]
fn open_answer_space_has_its_grid_style_and_colour() {
    let mut s = Session::new(Document::new());
    run(&mut s, "exercise.insert", json!({"kind": "open", "questions": [{"text": "Descrivi.", "height": 3}]}));
    let st = run(&mut s, "exercise.questions", json!({"exercise": 1}));
    assert_eq!(st["questions"][0]["gridStyle"], "solid");
    run(&mut s, "exercise.questions", json!({"exercise": 1, "question": 1, "gridStyle": "dotted", "gridColor": "#808080"}));
    let st = run(&mut s, "exercise.questions", json!({"exercise": 1}));
    assert_eq!(st["questions"][0]["gridStyle"], "dotted", "{st}");
    assert_eq!(st["questions"][0]["gridColor"], "808080", "{st}");
    let g = s.doc.body.iter().filter_map(|b| b.as_para()).find_map(|p| p.props.graph.clone()).expect("space");
    assert_eq!(g.line.style, wordcraft_doc::graph_paper::GridLineStyle::Dotted);
    assert_eq!(g.line.color, wordcraft_doc::Rgb(0x80, 0x80, 0x80));
    // Off, the space is blank; back on, the style is still there. Junk is ignored.
    run(&mut s, "exercise.questions", json!({"exercise": 1, "question": 1, "grid": false}));
    run(&mut s, "exercise.questions", json!({"exercise": 1, "question": 1, "grid": true, "gridColor": "zz"}));
    let st = run(&mut s, "exercise.questions", json!({"exercise": 1}));
    assert_eq!(st["questions"][0]["gridStyle"], "dotted", "{st}");
    assert_eq!(st["questions"][0]["gridColor"], "808080", "{st}");
}
