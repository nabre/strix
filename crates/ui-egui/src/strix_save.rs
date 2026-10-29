//! Saving a document that has Strix objects (calculations, calculated tables, exercises, CAD
//! drawings, graph paper: `strix_engine::strix_objects`). Strix saves to `.docx`, `.odt`, `.rtf`…
//! like the reference word processor, but only while the document has none: with any, Save goes
//! to Save As with `.xmath` proposed, Save As offers only Strix's own formats and the static
//! exports, and File › Export disables the editable formats, each saying why.

use strix_engine::strix_objects::{StrixObjectCount, StrixObjectKind, strix_objects};

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

/// Why `doc` saves only as a Strix document (or a static export), or `None` when it has no Strix
/// objects and every format is open to it.
pub(crate) fn native_only_reason(doc: &strix_doc::Document) -> Option<String> {
    let objects = strix_objects(doc);
    (!objects.is_empty()).then(|| {
        i18n::fmt(
            tl!(
                "This document has Strix objects ({objects}). Word and the other editable formats would lose them, so it saves only as a Strix document (.xmath). PDF, PNG, LaTeX, HTML and Markdown exports stay available as static copies."
            ),
            &[("objects", &objects_text(&objects))],
        )
    })
}

/// The note on File › Save As for a document with Strix objects.
pub(crate) fn save_as_note(doc: &strix_doc::Document, ui: &mut egui::Ui) {
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
        let mut s = strix_engine::Session::new(strix_doc::Document::from_text("Beam"));
        assert_eq!(native_only_reason(&s.doc), None);
        s.run("caret.docEnd", &json!({})).unwrap();
        s.run("calc.insert", &json!({"text": "L = 4.5 m"})).unwrap();
        s.run("calc.insert", &json!({"text": "q = 2 kN/m"})).unwrap();
        s.run("insert.cad", &json!({})).unwrap();
        let reason = native_only_reason(&s.doc).unwrap();
        assert!(reason.contains("2 calculations") && reason.contains("1 CAD drawing") && reason.contains(".xmath"), "{reason}");
    }
}
