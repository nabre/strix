//! Documents Strix saved before its internal names went back to WordCraft's (2026-11,
//! `docs/naming.md`, U5) carry a turned Drawing Canvas's turn under `urn:strix:canvas-xfrm`;
//! they still open with it. New files write upstream's `urn:wordcraft:canvas-xfrm`.

use std::io::{Read, Write};

use wordcraft_doc::fill::Fill;
use wordcraft_doc::para::{CanvasStyle, Float, GroupChild, ShapeKind, Wrap};
use wordcraft_doc::props::{CharProps, Rgb};
use wordcraft_doc::{Document, InlineObject, Paragraph, para_block};

/// `bytes` with `word/document.xml` passed through `edit`.
fn edit_document(bytes: &[u8], edit: impl Fn(&str) -> String) -> Vec<u8> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        let name = entry.name().to_string();
        let mut data = Vec::new();
        entry.read_to_end(&mut data).unwrap();
        if name == "word/document.xml" {
            data = edit(std::str::from_utf8(&data).unwrap()).into_bytes();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(&data).unwrap();
    }
    out.finish().unwrap().into_inner()
}

#[test]
fn a_canvas_turn_saved_under_strixs_old_uri_still_opens() {
    let member = InlineObject::Shape {
        kind: ShapeKind::Rectangle,
        w: 60.0,
        h: 40.0,
        fill: Some(Fill::solid(Rgb(1, 2, 3))),
        stroke: None,
        stroke_width: 0.0,
        float: Float::default(),
        story: None,
        effects: Default::default(),
        freeform: None,
        extra: Default::default(),
    };
    let canvas = InlineObject::Group {
        w: 200.0,
        h: 100.0,
        float: Float { rot: 30.0, flip_h: true, wrap: Wrap::Square, ..Default::default() },
        ch_w: 200.0,
        ch_h: 100.0,
        children: vec![GroupChild { x: 10.0, y: 20.0, obj: member }],
        canvas: Some(CanvasStyle { fill: None, stroke: None, stroke_width: 0.0 }),
    };
    let mut p = Paragraph::with_text("c", CharProps::default());
    p.insert_object(1, canvas, &CharProps::default()).unwrap();
    let mut doc = Document::new();
    doc.body = vec![p].into_iter().map(para_block).collect();
    let bytes = wordcraft_docx::write(&doc).unwrap();
    let old = edit_document(&bytes, |xml| {
        assert!(xml.contains("urn:wordcraft:canvas-xfrm"), "{xml}");
        xml.replace("urn:wordcraft:canvas-xfrm", "urn:strix:canvas-xfrm")
    });
    let read = wordcraft_docx::read(&old).unwrap();
    let para = read.body.iter().find_map(|b| b.as_para()).unwrap();
    let frame = para.objects[0].frame().unwrap().2;
    assert_eq!((frame.rot, frame.flip_h, frame.flip_v), (30.0, true, false));
}
