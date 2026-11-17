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

/// The answer space's style and colour back to the default with null.
#[test]
fn open_answer_space_style_and_colour_reset_with_null() {
    let mut s = Session::new(Document::new());
    run(&mut s, "exercise.insert", json!({"kind": "open", "questions": [{"text": "Descrivi.", "height": 3}]}));
    run(&mut s, "exercise.questions", json!({"exercise": 1, "question": 1, "gridStyle": "dashed", "gridColor": "808080"}));
    run(&mut s, "exercise.questions", json!({"exercise": 1, "question": 1, "gridStyle": null, "gridColor": null}));
    let st = run(&mut s, "exercise.questions", json!({"exercise": 1}));
    assert_eq!(st["questions"][0]["gridStyle"], "solid", "{st}");
    let default = wordcraft_doc::graph_paper::GraphPaper::default().line.color.hex();
    assert_eq!(st["questions"][0]["gridColor"], json!(default), "{st}");
}

/// A script or a bank asking for the table to complete of before gets an open answer.
#[test]
fn table_fill_of_before_is_an_open_answer() {
    let mut s = Session::new(Document::new());
    run(&mut s, "exercise.insert", json!({"kind": "tableFill", "questions": ["Completa la tabella"]}));
    let st = run(&mut s, "exercise.questions", json!({"exercise": 1}));
    assert_eq!(st["questions"][0]["type"], "open", "{st}");
}

// Structure regressions (exercise.rs, items.rs, delete.rs).

/// Top-level block of the first paragraph reading `text` (control markers left out).
fn block_of(s: &Session, text: &str) -> usize {
    s.doc.body.iter().position(|b| b.as_para().is_some_and(|p| super::plain(p) == text)).unwrap_or_else(|| panic!("no paragraph {text:?}"))
}

/// First and last top-level block of exercise `e` (0-based).
fn blocks_of(s: &Session, e: usize) -> (usize, usize) {
    let layout = super::scan(s);
    (layout.exercises[e].start, layout.parts.iter().rposition(|(x, _)| *x == Some(e)).unwrap())
}

/// Wraps exercise `e` in a subdocument control as a master document does (`cmd/master.rs`): its
/// start at the start of the exercise's first paragraph, its end at the end of its last one.
fn wrap_as_subdocument(s: &mut Session, e: usize) {
    use wordcraft_doc::control::{ContentControl, ControlKind, ControlLock};
    use wordcraft_doc::props::CharProps;
    use wordcraft_doc::{InlineObject, Pos};
    let (from, to) = blocks_of(s, e);
    let mut ctl = ContentControl::new(ControlKind::RichText);
    ctl.block = true;
    ctl.lock = ControlLock::SdtLocked;
    ctl.tag = format!("strix-subdoc path=/nowhere/esercizio-{e}.xmath");
    let end = s.doc.body[to].as_para().unwrap().text.len();
    s.doc.insert_object(&Pos::body(to, end), InlineObject::ControlEnd, &CharProps::default()).unwrap();
    s.doc.insert_object(&Pos::body(from, 0), InlineObject::ControlStart { control: Box::new(ctl) }, &CharProps::default()).unwrap();
}

fn question_count(s: &mut Session) -> usize {
    run(s, "exercise.questions", json!({"exercise": 1}))["questions"].as_array().map_or(0, Vec::len)
}

/// New Exercise with the caret in the frontispiece (where Frontispiece leaves it) took the
/// frontispiece's empty paragraph and its break for the exercise: the frontispiece was gone and
/// its break ended the new exercise. The exercise goes after the break.
#[test]
fn a_new_exercise_goes_after_the_frontispiece() {
    let mut s = Session::new(Document::new());
    run(&mut s, "exercise.insert", json!({"title": "Uno", "questions": ["q"]}));
    run(&mut s, "caret.docEnd", json!({}));
    run(&mut s, "exercise.insert", json!({"title": "Due", "questions": ["q"]}));
    run(&mut s, "exercise.frontispiece", json!({"on": true}));
    // The caret is in the frontispiece's empty paragraph.
    run(&mut s, "exercise.insert", json!({"title": "Tre", "questions": ["q"]}));
    let layout = super::scan(&s);
    assert_eq!(layout.front, Some((0, 0)), "the frontispiece is still there");
    assert_eq!(run(&mut s, "exercise.frontispiece", json!({})), json!({"on": true}));
    assert_eq!(layout.exercises.len(), 3);
    assert_eq!(layout.exercises.iter().map(|e| e.title.as_str()).collect::<Vec<_>>(), ["Tre", "Uno", "Due"]);
    assert!(s.doc.body[0].as_para().is_some_and(|p| p.text.is_empty() && p.section.as_ref().is_some_and(|sp| sp.frontispiece)));
    // Written in, with the caret in a paragraph before the one ending in the break.
    s.sel = crate::Selection::caret(wordcraft_doc::Pos::body(0, 0));
    run(&mut s, "text.insert", json!({"text": "Titolo della serie"}));
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "text.insert", json!({"text": "Classe"}));
    s.sel = crate::Selection::caret(wordcraft_doc::Pos::body(0, 3));
    run(&mut s, "exercise.insert", json!({"title": "Quattro", "questions": ["q"]}));
    let layout = super::scan(&s);
    let (_, front_end) = layout.front.expect("the frontispiece is still there");
    assert_eq!(layout.exercises.len(), 4);
    assert!(layout.exercises.iter().all(|e| e.start > front_end), "every exercise after the frontispiece");
    assert_eq!(layout.exercises[0].title, "Quattro");
}

/// Add Note in a quiz closed the questions on the last answer and the exercise on the note: the
/// note was outside the questions (and read as outside the exercise), and a question added after
/// it went inside the last question's options, so it was no question.
#[test]
fn a_note_in_a_quiz_stays_in_the_questions_and_questions_go_on_after_it() {
    for kind in ["choice", "open", "calc", "trueFalse"] {
        let mut s = Session::new(Document::new());
        run(&mut s, "exercise.insert", json!({"kind": kind, "questions": [{"text": "Uno"}]}));
        run(&mut s, "exercise.addNote", json!({"text": "Nota"}));
        let note = block_of(&s, "Nota");
        assert_eq!(super::scan(&s).parts[note], (Some(0), super::Part::Questions), "{kind}");
        // The caret is in the note, in the exercise.
        assert_eq!(run(&mut s, "exercise.info", json!({}))["exercise"], json!(1), "{kind}");
        run(&mut s, "exercise.addQuestion", json!({"text": "Due"}));
        assert_eq!(question_count(&mut s), 2, "{kind}");
        let layout = super::scan(&s);
        assert_eq!(layout.exercises.len(), 1, "{kind}");
        assert_eq!(layout.exercises[0].questions.iter().map(|q| q.text.as_str()).collect::<Vec<_>>(), ["Uno", "Due"], "{kind}");
        // Every control is closed where it was opened: nothing nests.
        let ranges = s.doc.control_ranges(StoryRef::Body);
        let answers: Vec<_> = ranges.iter().filter(|r| r.control.tag == super::TAG_ANSWER).collect();
        assert_eq!(answers.len(), 2, "{kind}");
        assert!(answers.iter().all(|r| !answers.iter().any(|o| o.start != r.start && o.contains(&r.start))), "{kind}: an answer inside another");
        assert!(s.undo() && s.undo());
        assert_eq!(question_count(&mut s), 1, "{kind}");
    }
}

/// A note put outside the questions by Add Note before (the exercise ending on it): a question
/// added goes into the questions, never into the last answer.
#[test]
fn a_question_added_before_a_note_closing_the_exercise_is_a_question() {
    use wordcraft_doc::Pos;
    let mut s = Session::new(Document::new());
    run(&mut s, "exercise.insert", json!({"kind": "choice", "questions": [{"text": "Uno", "choices": ["a", "b"]}]}));
    // The old shape: the exercise's end taken off the last option and put on a note after it.
    let (_, last) = blocks_of(&s, 0);
    let len = s.doc.body[last].as_para().unwrap().text.len();
    let w = wordcraft_doc::para::OBJ.len_utf8();
    s.doc.para_mut(StoryRef::Body, &super::body_path(last)).unwrap().delete(len - w, len).unwrap();
    s.doc.body.insert(last + 1, wordcraft_doc::para_block(super::para(super::STYLE_NOTE, "Nota", Vec::new(), 1, None)));
    assert_eq!(s.doc.control_ranges(StoryRef::Body).iter().filter(|r| r.control.tag == super::TAG_EXERCISE).count(), 1);
    // The note is the exercise's last paragraph: the caret in it is in the exercise, and a new
    // exercise goes after it.
    s.sel = crate::Selection::caret(Pos::body(last + 1, 0));
    assert_eq!(run(&mut s, "exercise.info", json!({}))["exercise"], json!(1));
    assert_eq!(blocks_of(&s, 0).1, last + 1);
    s.sel = crate::Selection::caret(Pos::body(last, 0));
    run(&mut s, "exercise.addQuestion", json!({"text": "Due"}));
    assert_eq!(question_count(&mut s), 2);
    assert_eq!(super::scan(&s).exercises[0].questions.len(), 2);
    run(&mut s, "exercise.insert", json!({"kind": "open", "title": "Dopo"}));
    let ranges = s.doc.control_ranges(StoryRef::Body);
    let exercises: Vec<_> = ranges.iter().filter(|r| r.control.tag == super::TAG_EXERCISE).collect();
    assert_eq!(exercises.len(), 2);
    assert!(!exercises[0].contains(&exercises[1].start), "the new exercise is not inside the first");
}

/// In a master document an exercise ends its subdocument, so its last paragraph closes the
/// subdocument too: a question added to a quiz there was left outside the questions.
#[test]
fn a_question_added_to_a_quiz_ending_a_subdocument_is_a_question() {
    for kind in ["choice", "open", "trueFalse"] {
        let mut s = Session::new(Document::new());
        run(&mut s, "exercise.insert", json!({"kind": kind, "questions": [{"text": "Uno"}]}));
        wrap_as_subdocument(&mut s, 0);
        run(&mut s, "select.text", json!({"text": "Uno"}));
        run(&mut s, "exercise.addQuestion", json!({"text": "Due"}));
        assert_eq!(question_count(&mut s), 2, "{kind}");
        let layout = super::scan(&s);
        assert_eq!(layout.exercises[0].questions.len(), 2, "{kind}");
        // The subdocument still holds the whole exercise.
        let ranges = s.doc.control_ranges(StoryRef::Body);
        let sub = ranges.iter().find(|r| r.control.tag.starts_with("strix-subdoc")).expect("subdocument");
        let ex = ranges.iter().find(|r| r.control.tag == super::TAG_EXERCISE).expect("exercise");
        assert!(sub.contains(&ex.start) && sub.contains(&ex.end), "{kind}");
        // And a note after them.
        run(&mut s, "exercise.addNote", json!({"text": "Nota"}));
        let note = block_of(&s, "Nota");
        assert_eq!(super::scan(&s).parts[note], (Some(0), super::Part::Questions), "{kind}");
    }
}

/// Delete Exercise on an exercise that is a subdocument's content (a master document) left its
/// title's paragraph in the Exercise Title style: an empty numbered "Esercizio 1." that is no
/// exercise, so the titles after it and the summary's labels no longer agreed.
#[test]
fn deleting_an_exercise_in_a_subdocument_leaves_no_title_behind() {
    let mut s = Session::new(Document::new());
    for title in ["Uno", "Due", "Tre"] {
        run(&mut s, "caret.docEnd", json!({}));
        run(&mut s, "exercise.insert", json!({"title": title, "questions": ["q"]}));
    }
    for e in (0..3).rev() {
        wrap_as_subdocument(&mut s, e);
    }
    assert_eq!(super::scan(&s).exercises.len(), 3);
    run(&mut s, "exercise.delete", json!({"exercise": 1}));
    let layout = super::scan(&s);
    assert_eq!(layout.exercises.iter().map(|e| e.title.as_str()).collect::<Vec<_>>(), ["Due", "Tre"]);
    let titles = s.doc.body.iter().filter(|b| b.as_para().is_some_and(|p| p.props.style.as_deref() == Some(super::STYLE_TITLE))).count();
    assert_eq!(titles, 2, "only the exercises' titles are in the title style (it numbers them)");
}

/// A question replaced by two (a paste over it) with nothing written in the solution: the
/// repeated questions of the new ones went after the exercise's end, outside its solution.
#[test]
fn repeated_questions_of_pasted_questions_stay_in_the_solution() {
    let mut s = Session::new(Document::new());
    run(&mut s, "exercise.insert", json!({"title": "T", "questions": ["Prima", "Seconda"]}));
    run(&mut s, "select.text", json!({"text": "Seconda"}));
    run(&mut s, "edit.paste", json!({"text": "Terza\nQuarta"}));
    let layout = super::scan(&s);
    let questions: Vec<&str> = layout.exercises[0].questions.iter().map(|q| q.text.as_str()).collect();
    assert_eq!(questions, ["Prima", "Terza", "Quarta"]);
    let prompts: Vec<(usize, String)> = s
        .doc
        .body
        .iter()
        .enumerate()
        .filter_map(|(i, b)| b.as_para().filter(|p| p.props.style.as_deref() == Some(super::STYLE_PROMPT)).map(|p| (i, super::plain(p))))
        .collect();
    assert_eq!(prompts.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>(), ["a) Prima", "b) Terza", "c) Quarta"]);
    let (_, last) = blocks_of(&s, 0);
    for (i, t) in &prompts {
        assert!(*i <= last && layout.parts[*i] == (Some(0), super::Part::Prompt), "{t} is in the solution: {:?}", layout.parts[*i]);
    }
    assert_eq!(layout.exercises[0].prompts.len(), 3);
}

/// A matching question converted to another mode kept its generated rows, locked, in the new
/// answer (an open answer's space reading `massa  [B]  A. s`).
#[test]
fn a_matching_converted_leaves_no_rows_behind() {
    for (to, lines) in [("open", 0), ("calc", 1), ("cloze", 0)] {
        let mut s = Session::new(Document::new());
        run(
            &mut s,
            "exercise.insert",
            json!({"kind": "matching", "questions": [{"text": "Abbina", "points": 2, "pairs": [["massa", "kg"], ["tempo", "s"]]}]}),
        );
        assert!(text(&s).contains("massa"));
        let st = run(&mut s, "exercise.questionMode", json!({"exercise": 1, "question": 1, "type": to}));
        assert_eq!(st["questions"][0]["type"], to);
        assert_eq!(st["questions"][0]["points"], 2.0, "{to}");
        assert!(!text(&s).contains("massa"), "{to}: {}", text(&s));
        let ranges = s.doc.control_ranges(StoryRef::Body);
        assert!(!ranges.iter().any(|r| r.control.tag == super::stem_items::TAG_MATCH_ROW), "{to}");
        assert_eq!(ranges.iter().filter(|r| r.control.tag == super::items::TAG_LINE).count(), lines, "{to}");
        assert_eq!(super::scan(&s).exercises.len(), 1, "{to}");
        // Still one exercise that questions are added to.
        run(&mut s, "exercise.addQuestion", json!({"text": "Due", "type": "open"}));
        assert_eq!(question_count(&mut s), 2, "{to}");
    }
}

/// An empty title or text given to New Exercise is none given: the placeholder shows, instead
/// of an empty field showing nothing.
#[test]
fn an_empty_title_or_text_shows_its_placeholder() {
    for kind in ["problem", "open"] {
        let mut s = Session::new(Document::new());
        run(&mut s, "exercise.insert", json!({"kind": kind, "title": "", "text": " ", "questions": ["q"]}));
        let ranges = s.doc.control_ranges(StoryRef::Body);
        let title = ranges.iter().find(|r| r.control.tag == super::TAG_TITLE).expect("title");
        assert!(title.control.showing_placeholder, "{kind}");
        assert_eq!(s.doc.control_text(title), "Exercise title", "{kind}");
        let statement = ranges.iter().find(|r| r.control.tag == super::TAG_TEXT);
        if kind == "problem" {
            assert!(statement.is_some_and(|r| r.control.showing_placeholder), "{kind}");
        } else {
            assert!(statement.is_none(), "{kind}: a lone question needs no statement");
        }
    }
}

/// Add Sub-question in a quiz added a question at the first level (`sub` and `level` were read
/// only in a problem).
#[test]
fn a_sub_question_in_a_quiz_is_nested() {
    let mut s = Session::new(Document::new());
    run(&mut s, "exercise.insert", json!({"kind": "open", "questions": ["Uno"]}));
    run(&mut s, "exercise.addQuestion", json!({"text": "Sotto", "sub": true}));
    assert_eq!(run(&mut s, "exercise.info", json!({}))["labels"], json!(["a)", "a.1)"]));
    run(&mut s, "exercise.addQuestion", json!({"text": "Due"}));
    run(&mut s, "exercise.addQuestion", json!({"text": "Sotto due", "sub": true}));
    run(&mut s, "exercise.addQuestion", json!({"text": "Più sotto", "level": 9}));
    let info = run(&mut s, "exercise.info", json!({}));
    assert_eq!(info["labels"], json!(["a)", "a.1)", "b)", "b.1)", "b.1.1)"]));
    assert_eq!(info["levels"], json!([0, 1, 0, 1, 2]));
    assert_eq!(question_count(&mut s), 5);
}

/// An exercise pasted with the caret in the frontispiece went into it (so it was no
/// frontispiece any more): it goes after the Frontispiece Break.
#[test]
fn an_exercise_pasted_in_the_frontispiece_goes_after_it() {
    use wordcraft_doc::Pos;
    let mut s = Session::new(Document::new());
    run(&mut s, "exercise.insert", json!({"title": "Uno", "questions": ["q"]}));
    run(&mut s, "caret.docEnd", json!({}));
    run(&mut s, "exercise.insert", json!({"title": "Due", "questions": ["q"]}));
    run(&mut s, "exercise.frontispiece", json!({"on": true}));
    run(&mut s, "text.insert", json!({"text": "Titolo della serie"}));
    run(&mut s, "text.newParagraph", json!({}));
    run(&mut s, "text.insert", json!({"text": "Classe"}));
    let (a, b) = blocks_of(&s, 1);
    let end = s.doc.body[b].as_para().map_or(0, |p| p.text.len());
    s.sel = crate::Selection { anchor: Pos::body(a, 0), focus: Pos::body(b, end) };
    run(&mut s, "edit.copy", json!({}));
    s.sel = crate::Selection::caret(Pos::body(0, 3));
    run(&mut s, "edit.paste", json!({}));
    let layout = super::scan(&s);
    let (_, front_end) = layout.front.expect("the frontispiece is still there");
    assert_eq!(layout.exercises.len(), 3);
    assert!(layout.exercises.iter().all(|e| e.start > front_end), "every exercise after the frontispiece");
    assert_eq!(layout.exercises.iter().map(|e| e.title.as_str()).collect::<Vec<_>>(), ["Due", "Uno", "Due"]);
}

/// A quiz's only true/false question on the title line of an exercise ending a subdocument (a
/// master document): the line closes the subdocument too, and converting the question left its
/// options outside the questions, so the conversion failed half done.
#[test]
fn a_true_false_on_the_line_ending_a_subdocument_converts() {
    let mut s = Session::new(Document::new());
    run(
        &mut s,
        "exercise.insert",
        json!({"kind": "trueFalse", "titleLine": "statement", "questions": [{"text": "Il sole è una stella.", "correct": 1}]}),
    );
    wrap_as_subdocument(&mut s, 0);
    let st = run(&mut s, "exercise.questionMode", json!({"exercise": 1, "question": 1, "type": "choice"}));
    assert_eq!(st["questions"][0]["choices"], json!(["True", "False", "", ""]), "{st}");
    assert_eq!(st["questions"][0]["correct"], json!(["A"]), "{st}");
    let layout = super::scan(&s);
    assert_eq!(layout.exercises.len(), 1);
    let options: Vec<usize> = (0..s.doc.body.len())
        .filter(|&i| s.doc.body[i].as_para().is_some_and(|p| p.props.style.as_deref() == Some(super::items::STYLE_CHOICE)))
        .collect();
    assert_eq!(options.len(), 4);
    assert!(options.iter().all(|&i| layout.parts[i] == (Some(0), super::Part::Questions)), "{:?}", layout.parts);
    let st = run(&mut s, "exercise.questionMode", json!({"exercise": 1, "question": 1, "type": "trueFalse"}));
    assert_eq!(st["questions"][0]["trueFalse"], true, "{st}");
    assert_eq!(st["questions"].as_array().map(Vec::len), Some(1));
}

/// The Frontispiece switch was greyed out once the last exercise was deleted, though the command
/// itself still answers for a frontispiece left without exercises: it could not be taken away.
#[test]
fn a_frontispiece_left_without_exercises_can_be_removed() {
    let mut s = Session::new(Document::new());
    run(&mut s, "exercise.insert", json!({"title": "Uno", "questions": ["q"]}));
    run(&mut s, "exercise.frontispiece", json!({"on": true}));
    run(&mut s, "text.insert", json!({"text": "Titolo della serie"}));
    run(&mut s, "exercise.delete", json!({"exercise": 1}));
    assert!(super::scan(&s).exercises.is_empty());
    assert_eq!(s.run("exercise.frontispiece", &json!({})).ok(), Some(json!({"on": true})));
    assert_eq!(s.run("exercise.frontispiece", &json!({"on": false})).ok(), Some(json!({"on": false})));
    assert!(!text(&s).contains("Titolo della serie"));
    // Without exercises and without a frontispiece there is nothing to switch.
    assert!(s.run("exercise.frontispiece", &json!({"on": true})).is_err());
}

/// A question setting given for another exercise than the caret's, without naming the question,
/// changed that exercise's last question (the caret being after it): there is no question at
/// the caret in an exercise the caret isn't in.
#[test]
fn a_setting_for_another_exercise_needs_its_question_named() {
    let mut s = Session::new(Document::new());
    run(
        &mut s,
        "exercise.insert",
        json!({"kind": "open", "title": "Uno", "questions": [{"text": "alfa", "points": 1}, {"text": "beta", "points": 1}]}),
    );
    run(&mut s, "caret.docEnd", json!({}));
    run(
        &mut s,
        "exercise.insert",
        json!({"kind": "open", "title": "Due", "questions": [{"text": "gamma", "points": 1}, {"text": "delta", "points": 1}]}),
    );
    // The caret is in the second exercise's title.
    assert_eq!(run(&mut s, "exercise.info", json!({}))["exercise"], json!(2));
    assert!(s.run("exercise.questionPoints", &json!({"exercise": 1, "points": 5})).is_err());
    assert!(s.run("exercise.questionMode", &json!({"exercise": 1, "type": "choice"})).is_err());
    let st = run(&mut s, "exercise.questions", json!({"exercise": 1}));
    assert_eq!(st["total"], 2.0, "{st}");
    assert_eq!(st["questions"][1]["type"], "open", "{st}");
    // Named, or with the caret in it, it is changed.
    let st = run(&mut s, "exercise.questionPoints", json!({"exercise": 1, "question": 2, "points": 5}));
    assert_eq!(st["total"], 6.0, "{st}");
    run(&mut s, "select.text", json!({"text": "delta"}));
    let st = run(&mut s, "exercise.questionPoints", json!({"points": 3}));
    assert_eq!((st["exercise"].clone(), st["total"].clone()), (json!(2), json!(4.0)), "{st}");
}

/// Remove Title with a table right after the statement's first paragraph (what follows that
/// paragraph stays in the part) moved nothing: the line showed the statement's placeholder over
/// a statement part still holding the text.
#[test]
fn a_statement_followed_by_a_table_goes_onto_the_line() {
    use wordcraft_doc::{Block, Pos};
    let w = wordcraft_doc::para::OBJ.len_utf8();
    let table = |s: &Session| s.doc.body.iter().position(|b| matches!(&**b, Block::Table(_))).expect("table");
    let mut s = Session::new(Document::new());
    run(&mut s, "exercise.insert", json!({"title": "Moto", "text": "Un treno parte.", "questions": ["q"]}));
    let st = block_of(&s, "Un treno parte.");
    s.sel = crate::Selection::caret(Pos::body(st, w + "Un treno parte.".len()));
    run(&mut s, "insert.table", json!({"rows": 1, "cols": 1}));
    assert_eq!(table(&s), st + 1);
    assert_eq!(super::scan(&s).parts[st + 1], (Some(0), super::Part::Text));
    let title = super::scan(&s).exercises[0].start;
    s.sel = crate::Selection::caret(Pos::body(title, 2 * w));
    run(&mut s, "exercise.titleLine", json!({"line": "statement"}));
    let line = s.doc.body[title].as_para().map(super::plain).unwrap_or_default();
    assert!(line.contains("Un treno parte."), "{line:?}");
    assert!(!text(&s).contains("Write the exercise here"), "{}", text(&s));
    assert_eq!(text(&s).matches("Un treno parte.").count(), 1);
    assert_eq!(super::scan(&s).parts[table(&s)], (Some(0), super::Part::Text), "the table stays in the statement part");
    // Back: the text is the part's first paragraph again, the table after it; the title returns.
    run(&mut s, "exercise.titleLine", json!({"line": "title"}));
    let line = s.doc.body[title].as_para().map(super::plain).unwrap_or_default();
    assert_eq!(line, "Moto");
    let st = block_of(&s, "Un treno parte.");
    let layout = super::scan(&s);
    assert_eq!((layout.parts[st], table(&s)), ((Some(0), super::Part::Text), st + 1));
    assert_eq!(layout.exercises[0].text_blocks.map(|(a, _)| a), Some(st));
    assert_eq!(layout.exercises.len(), 1);
}
