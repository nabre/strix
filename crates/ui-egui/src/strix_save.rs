//! Saving a document that has Strix objects (calculations, calculated tables, exercises, CAD
//! drawings, graph paper: `wordcraft_engine::strix_objects`). Strix saves to `.docx`, `.odt`,
//! `.rtf`… like the reference word processor. Strix's own formats and Word documents keep the
//! objects live (a Word document carries them through Word: docs/word-round-trip.md); with any,
//! the other editable formats are left out: Save goes to Save As with `.xmath` proposed, Save As
//! offers Strix's own formats, Word documents and the static exports, and File › Export disables
//! the rest, each saying why.

use wordcraft_engine::strix_objects::{StrixObjectCount, StrixObjectKind, strix_objects};

use crate::i18n;

/// The objects as a list in the interface language: `3 calculations, 1 CAD drawing`.
pub(crate) fn objects_text(objects: &[StrixObjectCount]) -> String {
    objects
        .iter()
        .map(|o| {
            let one = o.count == 1;
            let text = match o.kind {
                StrixObjectKind::Calculation if one => tl!("1 calculation"),
                StrixObjectKind::Calculation => tl!("{n} calculations"),
                StrixObjectKind::CalculatedTable if one => tl!("1 calculated table"),
                StrixObjectKind::CalculatedTable => tl!("{n} calculated tables"),
                StrixObjectKind::Exercise if one => tl!("1 exercise"),
                StrixObjectKind::Exercise => tl!("{n} exercises"),
                StrixObjectKind::ResultsSummary if one => tl!("1 results summary"),
                StrixObjectKind::ResultsSummary => tl!("{n} results summaries"),
                StrixObjectKind::CadDrawing if one => tl!("1 CAD drawing"),
                StrixObjectKind::CadDrawing => tl!("{n} CAD drawings"),
                StrixObjectKind::GraphPaper => tl!("graph paper"),
            };
            i18n::fmt(text, &[("n", &o.count.to_string())])
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Whether saving to `ext_or_name` (a bare extension or a file name) would lose Strix objects: an
/// editable format other than Strix's own and Word documents.
pub(crate) fn loses_objects(ext_or_name: &str) -> bool {
    wordcraft_engine::io::is_foreign_save(ext_or_name) && !wordcraft_engine::strix_objects::keeps_objects(ext_or_name)
}

/// Why `doc` saves only as a Strix document, a Word document (or a static export), or `None` when
/// it has no Strix objects and every format is open to it.
pub(crate) fn native_only_reason(doc: &wordcraft_doc::Document) -> Option<String> {
    let objects = strix_objects(doc);
    (!objects.is_empty()).then(|| {
        i18n::fmt(
            tl!(
                "This document has Strix objects ({objects}). They stay live in a Strix document (.xmath) and in a Word document (.docx); OpenDocument, RTF, plain text and Word XML would lose them. PDF, PNG, LaTeX, HTML and Markdown exports stay available as static copies."
            ),
            &[("objects", &objects_text(&objects))],
        )
    })
}

/// The note on File › Save As for a document with Strix objects.
pub(crate) fn save_as_note(doc: &wordcraft_doc::Document, ui: &mut egui::Ui) {
    if let Some(reason) = native_only_reason(doc) {
        let t = crate::theme::Tokens::get(ui.ctx());
        ui.label(egui::RichText::new(reason).color(t.text_dim));
        ui.add_space(8.0);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn the_reason_lists_the_objects_found() {
        let mut s = wordcraft_engine::Session::new(wordcraft_doc::Document::from_text("Beam"));
        assert_eq!(native_only_reason(&s.doc), None);
        s.run("caret.docEnd", &json!({})).unwrap();
        s.run("calc.insert", &json!({"text": "L = 4.5 m"})).unwrap();
        s.run("calc.insert", &json!({"text": "q = 2 kN/m"})).unwrap();
        s.run("insert.cad", &json!({})).unwrap();
        let reason = native_only_reason(&s.doc).unwrap();
        assert!(reason.contains("2 calculations") && reason.contains("1 CAD drawing") && reason.contains(".xmath"), "{reason}");
    }
}
