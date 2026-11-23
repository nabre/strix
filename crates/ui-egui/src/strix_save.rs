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
                StrixObjectKind::Plot if one => tl!("1 plot"),
                StrixObjectKind::Plot => tl!("{n} plots"),
                StrixObjectKind::Structure if one => tl!("1 structure"),
                StrixObjectKind::Structure => tl!("{n} structures"),
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

/// Whether a save or export to `name` may go ahead: a format Strix writes, and one that keeps
/// the document's Strix objects (the error says what to do instead).
pub(crate) fn check_save(name: &str, doc: &wordcraft_doc::Document) -> Result<(), String> {
    wordcraft_engine::io::check_writable(name)?;
    wordcraft_engine::strix_objects::check_save(name, doc)
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

/// File › Export › Assessment Pack in the browser (`file.exportPack` without `returnBytes`):
/// the `.zip` archive is downloaded. `None` for any other command, or on the desktop.
pub(crate) fn download_pack(app: &mut crate::WordApp, id: &str, params: &serde_json::Value) -> Option<Result<serde_json::Value, String>> {
    use serde_json::Value;
    if app.services.download.is_none() || id != "file.exportPack" || params.get("returnBytes").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    Some(deliver_pack(app, params))
}

/// [`download_pack`]: builds the archive and hands it to the page.
fn deliver_pack(app: &mut crate::WordApp, params: &serde_json::Value) -> Result<serde_json::Value, String> {
    use serde_json::{Value, json};
    let mut p = params.clone();
    p["returnBytes"] = json!(true);
    let res = app.session.run("file.exportPack", &p).map_err(|e| e.to_string())?;
    let b64 = res["zipBase64"].as_str().ok_or_else(|| "missing zipBase64".to_string())?;
    let bytes = wordcraft_engine::cmd::insert::base64_decode(b64).ok_or_else(|| "bad zipBase64".to_string())?;
    let raw_name = params.get("path").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| format!("{}_pack.zip", app.title_stem()));
    let name = if raw_name.to_ascii_lowercase().ends_with(".zip") { raw_name } else { format!("{raw_name}.zip") };
    let delivered = match (&app.services.host_save, &app.services.download) {
        (Some(host), _) => host(&name, &bytes, false),
        (None, Some(download)) => download(&name, &bytes),
        (None, None) => Ok(()),
    };
    if let Err(e) = delivered {
        let msg = i18n::fmt(tl!("Couldn't download {name}: {error}"), &[("name", &name), ("error", &e)]);
        app.status(msg.clone());
        return Err(msg);
    }
    Ok(json!({"downloaded": name, "bytes": bytes.len(), "files": res["files"]}))
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
