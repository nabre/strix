//! Unit tests for Phase 3 features (`D4`, `F1`, `F2`, `F3`, `F9`, `F4`, and Word `.docx` round-trip).

use std::io::{Cursor, Read};

use serde_json::json;
use wordcraft_doc::{Block, Document, Pos, StoryRef};

use super::items::STYLE_CHOICE;
use super::{body_path, plain, scan};
use crate::Session;

fn it_session() -> Session {
    let mut s = Session::new(Document::new());
    s.run("document.setText", &json!({"text": ""})).unwrap();
    std::sync::Arc::make_mut(&mut s.doc.styles).default_chr.lang = Some("it-IT".into());
    s
}

fn body_texts(s: &Session) -> Vec<String> {
    s.doc
        .body
        .iter()
        .map(|b| match &**b {
            Block::Para(p) => plain(p),
            Block::Table(t) => {
                let cells: Vec<String> = t
                    .rows
                    .iter()
                    .map(|r| {
                        r.cells
                            .iter()
                            .map(|c| c.blocks.iter().filter_map(|cb| cb.as_para().map(plain)).collect::<Vec<_>>().join(" "))
                            .collect::<Vec<_>>()
                            .join(" | ")
                    })
                    .collect();
                format!("[table: {}]", cells.join(" // "))
            }
        })
        .collect()
}

fn decode_b64(input: &str) -> Vec<u8> {
    fn val(b: u8) -> Option<u8> {
        match b {
            b'A'..=b'Z' => Some(b - b'A'),
            b'a'..=b'z' => Some(b - b'a' + 26),
            b'0'..=b'9' => Some(b - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let clean: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    let mut out = Vec::new();
    for chunk in clean.chunks(4) {
        if chunk.len() < 2 {
            break;
        }
        let Some(v0) = val(chunk[0]) else { break };
        let Some(v1) = val(chunk[1]) else { break };
        out.push((v0 << 2) | (v1 >> 4));
        if let Some(&c2) = chunk.get(2).filter(|&&c| c != b'=')
            && let Some(v2) = val(c2)
        {
            out.push((v1 << 4) | (v2 >> 2));
            if let Some(&c3) = chunk.get(3).filter(|&&c| c != b'=')
                && let Some(v3) = val(c3)
            {
                out.push((v2 << 6) | v3);
            }
        }
    }
    out
}

#[test]
fn d4_continuous_validation_across_all_variants() {
    let mut s = it_session();
    s.run("exercise.insert", &json!({"title": "Cinematica", "questions": ["Velocità", "Accelerazione"]})).unwrap();
    s.run(
        "exercise.setData",
        &json!({
            "data": {
                "items": [
                    {"symbol": "d", "description": "spazio", "unit": "m", "values": ["100", "50", "80"]},
                    {"symbol": "t", "description": "tempo", "unit": "s", "values": ["10", "0", "-4"]}
                ],
                "variants": ["A", "B", "C"],
                "active": 0
            }
        }),
    )
    .unwrap();
    let texts = body_texts(&s);
    let sol = texts.iter().position(|x| x == "a) Velocità").unwrap() + 1;
    s.sel = crate::Selection::caret(Pos::new(StoryRef::Body, body_path(sol), 0));
    s.run("calc.insert", &json!({"text": "v = d/√(t*1 s) ="} )).unwrap();

    // Active variant is A (clean), while B divides by 0 and C takes sqrt of negative!
    let info = s.run("exercise.data", &json!({"exercise": 1})).unwrap();
    let verr = info["variantErrors"].as_object().expect("variantErrors map");
    assert!(!verr.contains_key("A"), "variant A should be clean: {verr:?}");
    assert!(verr.contains_key("B"), "variant B must flag division by zero: {verr:?}");
    assert!(verr.contains_key("C"), "variant C must flag negative root: {verr:?}");
}

#[test]
fn f1_grading_table_presets_curves_visibility_and_calc_tolerance() {
    let mut s = it_session();
    s.run(
        "exercise.insert",
        &json!({
            "kind": "open",
            "title": "Calcolo",
            "questions": [
                {"text": "Area", "type": "calc", "points": 6, "height": 3.0},
                {"text": "Perimetro", "type": "open", "points": 4, "height": 2.0}
            ]
        }),
    )
    .unwrap();
    // Insert Scores table outside exercises (at start of doc or end).
    s.run("caret.docEnd", &json!({})).unwrap();
    s.run("text.newParagraph", &json!({})).unwrap();
    s.run("exercise.scores", &json!({"on": true})).unwrap();
    s.run("caret.docEnd", &json!({})).unwrap();
    s.run("text.newParagraph", &json!({})).unwrap();

    // Insert Italian 1..10 grading scale auto-bound to scores total (10 pts).
    let g_it = s.run("exercise.grading", &json!({"on": true, "preset": "it-10", "step": 0.5, "visibility": "teacher"})).unwrap();
    assert_eq!(g_it["on"], true);
    assert_eq!(g_it["effectiveMaxPoints"], json!(10.0));
    let scale_it = g_it["scale"].as_array().unwrap();
    assert_eq!(scale_it.first().unwrap()["grade"], json!(1.0));
    assert_eq!(scale_it.last().unwrap()["grade"], json!(10.0));

    // Switch to German inverted 6..1 scale (`de-6`): 0 pts -> 6.0 (fail), max pts -> 1.0 (pass).
    let g_de = s.run("exercise.grading", &json!({"preset": "de-6", "curve": "piecewiseLinear", "step": 0.5})).unwrap();
    let scale_de = g_de["scale"].as_array().unwrap();
    assert_eq!(scale_de.first().unwrap()["grade"], json!(6.0));
    assert_eq!(scale_de.first().unwrap()["pass"], json!(false));
    assert_eq!(scale_de.last().unwrap()["grade"], json!(1.0));
    assert_eq!(scale_de.last().unwrap()["pass"], json!(true));

    // Student copy hides `visibility: "teacher"` grading table; teacher copy shows it.
    s.run("exercise.show", &json!({"solution": false, "results": false})).unwrap();
    let hidden_txt = body_texts(&s).join("\n");
    assert!(!hidden_txt.contains("6,0") && !hidden_txt.contains("6.0"), "teacher-only grading hidden on student copy: {hidden_txt}");
    s.run("exercise.show", &json!({"solution": true, "results": true})).unwrap();

    // Work out the calc question and attach a 2% tolerance.
    let q_state = s.run("exercise.questions", &json!({"exercise": 1})).unwrap();
    let wpos = q_state["questions"][0]["workingPos"].clone();
    s.run("caret.set", &json!({"pos": wpos})).unwrap();
    s.run("calc.insert", &json!({"text": "A = 5 cm * 4 cm = cm^2"})).unwrap();
    s.run("exercise.questions", &json!({"exercise": 1, "question": 1, "tolerance": "5%"})).unwrap();

    let q_after = s.run("exercise.questions", &json!({"exercise": 1})).unwrap();
    let line_teacher = q_after["questions"][0]["line"].as_str().unwrap_or_default().to_string();
    assert!(
        line_teacher.contains("±5%") && line_teacher.contains("19") && line_teacher.contains("21"),
        "teacher line shows tolerance interval: {line_teacher}"
    );

    // Student copy hides both value and tolerance interval!
    s.run("exercise.show", &json!({"solution": false})).unwrap();
    let q_student = s.run("exercise.questions", &json!({"exercise": 1})).unwrap();
    let line_student = q_student["questions"][0]["line"].as_str().unwrap_or_default();
    assert!(line_student.contains("____") && !line_student.contains("±5%"), "student line hides tolerance: {line_student}");
}

#[test]
fn f2_inline_expr_rationales_shuffle_and_docx_round_trip() {
    let mut s = it_session();
    s.run(
        "exercise.insert",
        &json!({
            "kind": "choice",
            "title": "Moto uniforme",
            "statement": true,
            "text": "Un corpo viaggia per {{t}} alla velocità {{v}}.",
            "questions": [{
                "text": "Quanto spazio percorre?",
                "points": 2,
                "choices": ["{{v * t -> m}}", "{{v / t -> m/s^2}}", "{{v + t * 1 m/s^2 -> m/s}}", "0 m"],
                "correct": "A"
            }]
        }),
    )
    .unwrap();
    s.run(
        "exercise.setData",
        &json!({
            "data": {
                "items": [
                    {"symbol": "v", "unit": "m/s", "values": ["10", "20"]},
                    {"symbol": "t", "unit": "s", "values": ["5", "4"]}
                ],
                "variants": ["A", "B"],
                "active": 0
            }
        }),
    )
    .unwrap();
    s.run(
        "exercise.questions",
        &json!({
            "exercise": 1,
            "question": 1,
            "shuffleChoices": true,
            "rationales": [null, "Hai diviso invece di moltiplicare", "Somma dimensionalmente errata", null]
        }),
    )
    .unwrap();

    // Variant A (first variant): canonical order, evaluated `50 m` with `Unit` run style, rationale shown on teacher copy.
    let txt_a = body_texts(&s).join("\n");
    assert!(txt_a.contains("50 m"), "{txt_a}");
    assert!(txt_a.contains("Hai diviso invece di moltiplicare"), "teacher copy shows rationale: {txt_a}");
    let has_unit_style = s.doc.body.iter().any(|b| {
        b.as_para().is_some_and(|p| {
            p.props.style.as_deref() == Some(STYLE_CHOICE) && p.runs.iter().any(|r| r.props.style.as_deref() == Some(crate::cmd::calc::UNIT_STYLE))
        })
    });
    assert!(has_unit_style, "strix:expr units in choices must use Unit character style");

    // Student copy hides rationales (`hidden == Some(true)`).
    s.run("exercise.show", &json!({"solution": false})).unwrap();
    for b in &s.doc.body {
        if let Block::Para(p) = &**b
            && let Some((ra, rb)) = super::items::span(p, super::TAG_RATIONALE)
        {
            assert_eq!(p.props_of_char(ra + wordcraft_doc::para::OBJ.len_utf8()).hidden, Some(true), "rationale hidden for student: {ra}..{rb}");
        }
    }
    s.run("exercise.show", &json!({"solution": true})).unwrap();

    // Variant B: `20 * 4 = 80 m`, and `shuffleChoices: true` deterministically permutes options away from A's order!
    let st_a = s.run("exercise.questions", &json!({"exercise": 1})).unwrap();
    let choices_a = st_a["questions"][0]["choices"].clone();
    s.run("exercise.variant", &json!({"exercise": 1, "active": 1})).unwrap();
    let st_b = s.run("exercise.questions", &json!({"exercise": 1})).unwrap();
    let choices_b = st_b["questions"][0]["choices"].clone();
    assert!(choices_b.as_array().unwrap().iter().any(|c| c.as_str() == Some("80 m")), "variant B evaluates to 80 m: {choices_b}");
    assert_ne!(choices_a, choices_b, "variant B permutes choices");
    // Wherever `80 m` moved, the correct box travelled with it!
    let pos_80 = choices_b.as_array().unwrap().iter().position(|c| c.as_str() == Some("80 m")).unwrap();
    let expected_letter = char::from(b'A' + pos_80 as u8).to_string();
    assert_eq!(st_b["questions"][0]["correct"], json!([expected_letter]));

    // Switching back to Variant A restores exact canonical order (`50 m` at A)!
    s.run("exercise.variant", &json!({"exercise": 1, "active": 0})).unwrap();
    let st_back = s.run("exercise.questions", &json!({"exercise": 1})).unwrap();
    assert_eq!(st_back["questions"][0]["choices"][0], json!("50 m"));
    assert_eq!(st_back["questions"][0]["correct"], json!(["A"]));

    // Word `.docx` round-trip preserves `strix:expr`, `shuffleChoices`, and `rationales`!
    let docx_bytes = crate::io::save_bytes("quiz.docx", &s.doc).unwrap();
    let reopened = crate::io::open_bytes("quiz.docx", &docx_bytes).unwrap();
    let mut s2 = Session::new(reopened);
    s2.run("exercise.variant", &json!({"exercise": 1, "active": 1})).unwrap();
    let st_rt = s2.run("exercise.questions", &json!({"exercise": 1})).unwrap();
    assert_eq!(st_rt["questions"][0]["shuffleChoices"], json!(true));
    assert!(st_rt["questions"][0]["choices"].as_array().unwrap().iter().any(|c| c.as_str() == Some("80 m")));
    assert!(st_rt["questions"][0]["rationales"].as_array().unwrap().iter().any(|r| r.as_str() == Some("Hai diviso invece di moltiplicare")));
}

#[test]
fn f3_controlled_variant_generator_respects_domains_and_constraints() {
    let mut s = it_session();
    s.run("exercise.insert", &json!({"title": "Rettangolo", "questions": ["Area e rapporto", "Perimetro"]})).unwrap();
    let texts = body_texts(&s);
    let sol = texts.iter().position(|x| x == "a) Area e rapporto").unwrap() + 1;
    s.sel = crate::Selection::caret(Pos::new(StoryRef::Body, body_path(sol), 0));
    s.run("calc.insert", &json!({"text": "r = L/b ="} )).unwrap();
    let mut at = s.sel.focus.clone();
    at.off -= wordcraft_doc::para::OBJ.len_utf8();
    s.sel = crate::Selection::caret(at);
    s.run("equation.answer", &json!({"question": 1})).unwrap();
    let sol_b = body_texts(&s).iter().position(|x| x == "b) Perimetro").unwrap() + 1;
    s.sel = crate::Selection::caret(Pos::new(StoryRef::Body, body_path(sol_b), 0));
    s.run("calc.insert", &json!({"text": "P = 2 * (L + b) ="} )).unwrap();
    let mut at_b = s.sel.focus.clone();
    at_b.off -= wordcraft_doc::para::OBJ.len_utf8();
    s.sel = crate::Selection::caret(at_b);
    s.run("equation.answer", &json!({"question": 2})).unwrap();

    let gen1 = s
        .run(
            "exercise.generateVariants",
            &json!({
                "exercise": 1,
                "count": 4,
                "seed": 12345,
                "domains": [
                    {"symbol": "L", "description": "base", "unit": "cm", "min": 6, "max": 24, "step": 2},
                    {"symbol": "b", "description": "altezza", "unit": "cm", "values": ["2", "4", "5", "8"]}
                ],
                "constraints": {
                    "maxDecimals": 1,
                    "positiveResults": true,
                    "distinctFinalResults": true,
                    "extra": ["L > b"]
                }
            }),
        )
        .unwrap();
    assert_eq!(gen1["generated"], json!(4));
    assert_eq!(gen1["variants"], json!(["A", "B", "C", "D"]));

    // Determinism check: running again with the same seed yields identical values.
    let snapshot = gen1["data"].clone();
    let gen2 = s
        .run(
            "exercise.generateVariants",
            &json!({
                "exercise": 1,
                "count": 4,
                "seed": 12345,
                "domains": [
                    {"symbol": "L", "description": "base", "unit": "cm", "min": 6, "max": 24, "step": 2},
                    {"symbol": "b", "description": "altezza", "unit": "cm", "values": ["2", "4", "5", "8"]}
                ],
                "constraints": {
                    "maxDecimals": 1,
                    "positiveResults": true,
                    "distinctFinalResults": true,
                    "extra": ["L > b"]
                }
            }),
        )
        .unwrap();
    assert_eq!(gen2["data"], snapshot);

    // Verify all 4 generated variants have 0 errors and satisfy L > b and <= 1 decimal on r = L/b.
    let check = s.run("exercise.check", &json!({})).unwrap();
    assert_eq!(check["errors"], json!(0), "{check}");
}

#[test]
fn f9_and_f4_preflight_check_and_export_pack_zip() {
    let mut s = it_session();
    s.run("archive.properties", &json!({"duration": 60})).unwrap();
    s.run(
        "exercise.insert",
        &json!({
            "kind": "choice",
            "title": "Quiz diagnostico",
            "questions": [{
                "text": "Quanto vale 2·x?",
                "points": 3,
                "choices": ["{{2 * x}}", "{{x + 2}}", "10"],
                "correct": []
            }]
        }),
    )
    .unwrap();
    // Note: `x = 2` makes `2*x = 4` and `x+2 = 4` collide in Variant B! Plus `unused_k` is unused, and `correct` is empty!
    s.run(
        "exercise.setData",
        &json!({
            "duration": 20,
            "data": {
                "items": [
                    {"symbol": "x", "values": ["3", "2"]},
                    {"symbol": "unused_k", "values": ["7", "9"]}
                ],
                "variants": ["A", "B"],
                "active": 0
            }
        }),
    )
    .unwrap();

    let chk = s.run("exercise.check", &json!({})).unwrap();
    assert_eq!(chk["ok"], json!(false));
    let codes: Vec<&str> = chk["issues"].as_array().unwrap().iter().filter_map(|i| i["code"].as_str()).collect();
    assert!(codes.contains(&"choice_no_correct"), "{codes:?}");
    assert!(codes.contains(&"choice_duplicate_option"), "must detect option collision 4 == 4 in variant B: {codes:?}");
    assert!(codes.contains(&"unused_initial_datum"), "{codes:?}");
    assert!(codes.contains(&"duration_mismatch"), "{codes:?}");

    // Fix the quiz so it's valid, add a grading table, and export the Assessment Pack as an in-memory ZIP!
    s.run("exercise.questions", &json!({"exercise": 1, "question": 1, "correct": "A"})).unwrap();
    s.run(
        "exercise.setData",
        &json!({
            "data": {
                "items": [{"symbol": "x", "values": ["3", "4"]}],
                "variants": ["A", "B"],
                "active": 0,
                "duration": 60
            }
        }),
    )
    .unwrap();
    s.run("caret.docEnd", &json!({})).unwrap();
    s.run("text.newParagraph", &json!({})).unwrap();
    s.run("exercise.grading", &json!({"on": true, "preset": "it-10"})).unwrap();

    let before_ex_count = scan(&s).exercises.len();
    let pack = s.run("file.exportPack", &json!({"returnBytes": true})).unwrap();
    assert_eq!(pack["exported"], json!(true));
    assert_eq!(scan(&s).exercises.len(), before_ex_count, "Session must not be mutated by exportPack");

    let b64 = pack["zipBase64"].as_str().expect("zipBase64 string");
    let zip_bytes = decode_b64(b64);
    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes)).expect("valid ZIP");
    let mut names = Vec::new();
    for idx in 0..archive.len() {
        let mut entry = archive.by_index(idx).unwrap();
        names.push(entry.name().to_string());
        let mut head = [0u8; 5];
        entry.read_exact(&mut head).unwrap();
        assert_eq!(&head, b"%PDF-", "{} must be a valid PDF", entry.name());
    }
    assert!(names.contains(&"Fila_A.pdf".to_string()), "{names:?}");
    assert!(names.contains(&"Fila_B.pdf".to_string()), "{names:?}");
    assert!(names.contains(&"Fila_A_DSA.pdf".to_string()), "{names:?}");
    assert!(names.contains(&"Fila_B_DSA.pdf".to_string()), "{names:?}");
    assert!(names.contains(&"Correttore_Docente.pdf".to_string()), "{names:?}");
}
