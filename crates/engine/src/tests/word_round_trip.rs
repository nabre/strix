//! Strix objects through Word (`word_envelope`, docs/word-round-trip.md): a `.docx` Strix writes
//! keeps its calculations, calculated tables, CAD drawings, graph paper and exercise data live
//! after Word has saved it (Word drops Strix's own markup but keeps content controls and custom
//! XML parts), and an object changed in Word waits for the person to recalculate or keep it.
//!
//! Word itself isn't run here: [`as_word_saves`] takes out of the package what Word drops (every
//! `strix:` attribute and element, the CAD drawings' source parts and their picture extension).

use std::io::{Cursor, Read, Write};

use serde_json::json;
use wordcraft_doc::graphic::GraphicKind;
use wordcraft_doc::{Block, Document, InlineObject};

use crate::strix_objects::{StrixObjectKind, strix_objects};
use crate::word_envelope::{PENDING, Resolution, pending, resolve};
use crate::{Session, io};

/// A document with one of each Strix object (two calculations, a calculated table, a CAD
/// drawing, graph paper in the text and an exercise).
fn rich() -> Session {
    let mut s = Session::new(Document::from_text("Beam"));
    s.run("caret.docEnd", &json!({})).unwrap();
    s.run("calc.insert", &json!({"text": "L = 4.5 m"})).unwrap();
    s.run("text.insert", &json!({"text": " "})).unwrap();
    s.run("calc.insert", &json!({"text": "A = L*2 ="})).unwrap();
    s.run("text.newParagraph", &json!({})).unwrap();
    s.run("insert.cad", &json!({})).unwrap();
    s.run("caret.docEnd", &json!({})).unwrap();
    s.run("text.newParagraph", &json!({})).unwrap();
    s.run("insert.table", &json!({"rows": 2, "cols": 2})).unwrap();
    s.run("table.calc", &json!({"value": true})).unwrap();
    s.run("caret.docEnd", &json!({})).unwrap();
    s.run("graph.insert", &json!({})).unwrap();
    s.run("caret.docEnd", &json!({})).unwrap();
    s.run("exercise.insert", &json!({"title": "Primo", "questions": ["a"]})).unwrap();
    s
}

fn kinds(doc: &Document) -> Vec<(StrixObjectKind, usize)> {
    strix_objects(doc).into_iter().map(|o| (o.kind, o.count)).collect()
}

/// The package's parts, by name.
fn parts(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..zip.len())
        .map(|i| {
            let mut f = zip.by_index(i).unwrap();
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            (f.name().to_string(), b)
        })
        .collect()
}

fn package(parts: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in parts {
        w.start_file(name.as_str(), zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(bytes).unwrap();
    }
    w.finish().unwrap().into_inner()
}

/// Remove every element named `strix:…` (with its content) and every `strix:…` attribute.
fn drop_strix_markup(xml: &str) -> String {
    let mut s = xml.to_string();
    // Elements: <strix:x …/> and <strix:x …>…</strix:x>.
    while let Some(i) = s.find("<strix:") {
        let rest = &s[i..];
        let name_end = rest.find([' ', '>', '/']).unwrap();
        let name = rest[1..name_end].to_string();
        let open_end = rest.find('>').unwrap();
        let end = if rest[..open_end].ends_with('/') {
            i + open_end + 1
        } else {
            let close = format!("</{name}>");
            i + rest.find(&close).unwrap() + close.len()
        };
        s.replace_range(i..end, "");
    }
    // Attributes: ` strix:x="…"` (and the namespace declaration).
    while let Some(i) = s.find(" strix:").or_else(|| s.find(" xmlns:strix=")) {
        let rest = &s[i..];
        let q1 = rest.find('"').unwrap();
        let q2 = rest[q1 + 1..].find('"').unwrap();
        s.replace_range(i..i + q1 + q2 + 2, "");
    }
    // The CAD source's picture extension, now empty.
    while let Some(i) = s.find("<a:ext uri=\"urn:io.github.nabre.strix:cad\">") {
        let end = i + s[i..].find("</a:ext>").unwrap() + "</a:ext>".len();
        s.replace_range(i..end, "");
    }
    s.replace(" strix", " ").replace("<a:extLst></a:extLst>", "")
}

/// What Word leaves of a package Strix wrote when it saves it: no `strix:` markup, no CAD source
/// parts. Content controls and custom XML parts stay. `edit` changes `word/document.xml` the way
/// a person typing in Word would.
fn as_word_saves(bytes: &[u8], edit: &dyn Fn(String) -> String) -> Vec<u8> {
    let out: Vec<(String, Vec<u8>)> = parts(bytes)
        .into_iter()
        .filter(|(name, _)| !name.starts_with("word/strix/"))
        .map(|(name, b)| {
            if name.ends_with(".xml") && !name.starts_with("customXml/") {
                let mut xml = drop_strix_markup(&String::from_utf8(b).unwrap());
                if name == "word/document.xml" {
                    xml = edit(xml);
                }
                (name, xml.into_bytes())
            } else {
                (name, b)
            }
        })
        .collect();
    package(&out)
}

fn equations(doc: &Document) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    for b in &doc.body {
        if let Block::Para(p) = &**b {
            for o in &p.objects {
                if let InlineObject::Equation { linear, math, .. } = o {
                    let text = if math.nodes.is_empty() { linear.clone() } else { wordcraft_doc::math::to_linear(&math.nodes) };
                    out.push((text, math.calc));
                }
            }
        }
    }
    out
}

fn wrappers(doc: &Document) -> usize {
    let mut n = 0;
    let mut walk = |bl: &wordcraft_doc::Blocks| {
        for b in bl {
            match &**b {
                Block::Para(p) => {
                    n += p
                        .objects
                        .iter()
                        .filter(|o| matches!(o, InlineObject::ControlStart { control } if control.tag.starts_with("strix:obj:")))
                        .count();
                }
                Block::Table(t) => n += t.controls.open.iter().filter(|c| c.tag.starts_with("strix:obj:")).count(),
            }
        }
    };
    walk(&doc.body);
    n
}

#[test]
fn a_docx_strix_reads_back_is_what_it_wrote() {
    let s = rich();
    let before = kinds(&s.doc);
    assert_eq!(before.len(), 6, "{before:?}");
    let bytes = io::save_bytes("beam.docx", &s.doc).unwrap();
    let names: Vec<String> = parts(&bytes).into_iter().map(|(n, _)| n).collect();
    assert!(names.iter().any(|n| n == crate::word_envelope::PART), "{names:?}");
    let back = io::open_bytes("beam.docx", &bytes).unwrap();
    assert_eq!(kinds(&back), before);
    assert_eq!(wrappers(&back), 0, "the wrappers are gone");
    assert!(pending(&back).is_empty());
    assert!(!back.passthrough.keys().any(|k| k.starts_with("customXml/")), "the Strix part is taken out: {:?}", back.passthrough.keys());
    // Saved again, the part is there once.
    let again = io::save_bytes("beam.docx", &back).unwrap();
    assert_eq!(parts(&again).iter().filter(|(_, b)| String::from_utf8_lossy(b).contains(crate::word_envelope::NS)).count(), 1);
}

#[test]
fn xmath_files_carry_no_envelope() {
    let s = rich();
    let bytes = io::save_bytes("beam.xmath", &s.doc).unwrap();
    let all = parts(&bytes);
    assert!(!all.iter().any(|(n, _)| n.starts_with("customXml/")), "no custom XML part in a .xmath");
    let doc = all.iter().find(|(n, _)| n == "word/document.xml").map(|(_, b)| String::from_utf8_lossy(b).into_owned()).unwrap();
    assert!(!doc.contains("strix:obj:"), "no wrappers in a .xmath");
}

#[test]
fn a_document_without_strix_objects_gets_no_envelope() {
    let s = Session::new(Document::from_text("Dear reader"));
    let bytes = io::save_bytes("letter.docx", &s.doc).unwrap();
    assert!(!parts(&bytes).iter().any(|(_, b)| String::from_utf8_lossy(b).contains(crate::word_envelope::NS)));
}

/// Word drops `strix:` markup and the CAD source parts: everything comes back live from the
/// custom XML part.
#[test]
fn objects_come_back_live_after_word_saves_the_file() {
    let s = rich();
    let before = kinds(&s.doc);
    let bytes = io::save_bytes("beam.docx", &s.doc).unwrap();
    let word = as_word_saves(&bytes, &|x| x);
    let doc_xml = parts(&word).into_iter().find(|(n, _)| n == "word/document.xml").map(|(_, b)| String::from_utf8(b).unwrap()).unwrap();
    assert!(!doc_xml.contains("strix:calc") && doc_xml.contains("strix:obj:"), "Word's file has the wrappers, not the markup");
    // Without the envelope Word's file would have lost them.
    let bare = io::open_bytes("beam.docx", &as_word_saves(&io::save_bytes("beam.xmath", &s.doc).unwrap(), &|x| x)).unwrap();
    assert!(kinds(&bare).len() < before.len(), "{:?}", kinds(&bare));

    let back = io::open_bytes("beam.docx", &word).unwrap();
    assert_eq!(kinds(&back), before);
    assert!(pending(&back).is_empty(), "{:?}", pending(&back));
    assert_eq!(wrappers(&back), 0);
    // The CAD drawing is a drawing again, from its source.
    let cad = back.body.iter().filter_map(|b| b.as_para()).flat_map(|p| &p.objects).find_map(|o| match o {
        InlineObject::Graphic { graphic, .. } if graphic.kind == GraphicKind::Cad => graphic.cad.clone(),
        _ => None,
    });
    let original = s.doc.body.iter().filter_map(|b| b.as_para()).flat_map(|p| &p.objects).find_map(|o| match o {
        InlineObject::Graphic { graphic, .. } if graphic.kind == GraphicKind::Cad => graphic.cad.clone(),
        _ => None,
    });
    assert_eq!(cad, original);
    // Live: the calculations recompute in a session.
    let mut s2 = Session::new(back);
    s2.run("calc.recalculate", &json!({})).unwrap();
    assert!(equations(&s2.doc).iter().all(|(_, calc)| *calc), "{:?}", equations(&s2.doc));
}

/// A result typed over in Word: that calculation stays as typed (static) until the person
/// decides; the others come back live.
#[test]
fn a_result_changed_in_word_waits_for_a_decision() {
    let mut s = rich();
    s.run("calc.recalculate", &json!({})).unwrap();
    let shown = equations(&s.doc);
    let result = shown.iter().find(|(t, _)| t.starts_with("A")).map(|(t, _)| t.clone()).unwrap();
    assert!(result.contains('9'), "A = L·2 = 9 m: {result}");
    let bytes = io::save_bytes("beam.docx", &s.doc).unwrap();
    // In Word, the person types 10 over the 9.
    let word = as_word_saves(&bytes, &|x| x.replacen("=9\u{2009}<", "=10\u{2009}<", 1));
    let back = io::open_bytes("beam.docx", &word).unwrap();
    let waiting = pending(&back);
    assert_eq!(waiting.len(), 1, "{waiting:?}");
    assert_eq!(waiting[0].source.kind_id(), "calculation");
    assert_eq!(wrappers(&back), 1, "its wrapper stays until it is decided");
    let eqs = equations(&back);
    assert_eq!(eqs.iter().filter(|(_, calc)| *calc).count(), 1, "L is live, A is static: {eqs:?}");
    assert!(eqs.iter().any(|(t, calc)| t.contains("10") && !calc), "{eqs:?}");

    // Keep: the 10 stays, static, and nothing waits.
    let mut kept = back.clone();
    assert_eq!(resolve(&mut kept, None, Resolution::Keep), 1);
    assert!(pending(&kept).is_empty() && !kept.passthrough.contains_key(PENDING));
    assert_eq!(wrappers(&kept), 0);
    assert!(equations(&kept).iter().any(|(t, calc)| t.contains("10") && !calc));

    // Recalculate, through the command: live again, and Strix writes 9 back.
    let mut s2 = Session::new(back);
    let listed = s2.run("file.externalEdits", &json!({})).unwrap();
    assert_eq!(listed["pending"].as_array().map(Vec::len), Some(1), "{listed}");
    let r = s2.run("file.externalEdits", &json!({"action": "recalculate"})).unwrap();
    assert_eq!(r["resolved"], 1);
    assert_eq!(wrappers(&s2.doc), 0);
    s2.run("calc.recalculate", &json!({})).unwrap();
    let eqs = equations(&s2.doc);
    assert!(eqs.iter().all(|(_, calc)| *calc), "{eqs:?}");
    assert!(eqs.iter().any(|(t, _)| t.starts_with('A') && t.contains('9') && !t.contains("10")), "{eqs:?}");
    // Nothing left to decide.
    assert!(s2.run("file.externalEdits", &json!({"action": "keep"})).is_err());
}

/// Saving again before deciding: the pending objects count as kept, and no stale wrapper or
/// number gets mixed up with the new ones.
#[test]
fn saving_with_pending_objects_starts_clean() {
    let mut s = rich();
    s.run("calc.recalculate", &json!({})).unwrap();
    let bytes = io::save_bytes("beam.docx", &s.doc).unwrap();
    let back = io::open_bytes("beam.docx", &as_word_saves(&bytes, &|x| x.replacen("=9\u{2009}<", "=10\u{2009}<", 1))).unwrap();
    assert_eq!(pending(&back).len(), 1);
    let again = io::open_bytes("beam.docx", &io::save_bytes("beam.docx", &back).unwrap()).unwrap();
    assert!(pending(&again).is_empty());
    assert_eq!(wrappers(&again), 0);
    let eqs = equations(&again);
    assert!(eqs.iter().any(|(t, calc)| t.contains("10") && !calc), "{eqs:?}");
}

/// `file.open` says how many objects changed elsewhere wait for a decision.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn opening_says_how_many_objects_wait() {
    let mut s = rich();
    s.run("calc.recalculate", &json!({})).unwrap();
    let bytes = io::save_bytes("beam.docx", &s.doc).unwrap();
    let dir = std::env::temp_dir().join(format!("strix-word-trip-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("beam.docx");
    std::fs::write(&path, as_word_saves(&bytes, &|x| x.replacen("=9\u{2009}<", "=10\u{2009}<", 1))).unwrap();
    let mut s2 = Session::new(Document::from_text(""));
    let r = s2.run("file.open", &json!({"path": path.to_string_lossy()})).unwrap();
    assert_eq!(r["externalEdits"], 1, "{r}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A hostile Strix part (junk, huge numbers, unknown kinds) never panics and changes nothing it
/// shouldn't.
#[test]
fn a_hostile_strix_part_never_panics() {
    let s = rich();
    let bytes = io::save_bytes("beam.docx", &s.doc).unwrap();
    for junk in [
        "",
        "<objects xmlns=\"urn:io.github.nabre.strix:objects:2026\"><data>{</data></objects>",
        "<objects xmlns=\"urn:io.github.nabre.strix:objects:2026\"><data>{\"objects\":[{\"id\":4294967295,\"kind\":\"cad\",\"json\":\"&#xFFFFFFFF;\"}],\"sections\":[{\"n\":18446744073709551615}]}</data></objects>",
        "<objects xmlns=\"urn:io.github.nabre.strix:objects:2026\"><x:data xmlns:x=\"y\">{\"objects\":[{\"id\":1,\"kind\":\"nope\"}]}</x:data></objects>",
        "<objects xmlns=\"urn:io.github.nabre.strix:objects:2026\"><data>&amp;&#;&#x;&lt</data></objects>",
    ] {
        let out: Vec<(String, Vec<u8>)> = parts(&bytes)
            .into_iter()
            .map(|(n, b)| if String::from_utf8_lossy(&b).contains(crate::word_envelope::NS) { (n, junk.as_bytes().to_vec()) } else { (n, b) })
            .collect();
        let _ = io::open_bytes("beam.docx", &package(&out));
        let _ = io::open_bytes("beam.docx", &as_word_saves(&package(&out), &|x| x));
    }
}
