//! Strix's own custom XML part (Strix, not upstream WordCraft): a package part under
//! `customXml/` that Word keeps when it saves a file (it is a custom XML data store, ECMA-376
//! Part 1 §15.2.4), used to carry the sources of Strix objects through Word
//! (`wordcraft_engine::word_envelope`, docs/word-round-trip.md).
//!
//! The reader already keeps every custom XML part the package or the main document relates to in
//! [`Document::passthrough`] (`keep_custom_xml`), and the writer writes them back
//! (`custom_xml_rels`). These two functions add such a part to a document before it is written
//! and take it out of a document after it is read, so the rest of WordCraft never sees it.

use std::sync::Arc;

use wordcraft_doc::Document;

use crate::package::{CUSTOM_XML_ROOTS, EMBEDDED_PARTS, EmbeddedManifest, EmbeddedPart, manifest_field_ok, rel_is, rt};

/// The content type the part is written with.
const CONTENT_TYPE: &str = "application/xml";
/// Most bytes of a part [`take`] reads back.
pub const MAX_PART: usize = 64 * 1024 * 1024;

/// Add `bytes` as a custom XML part of the package named `name` (`customXml/…`), related to the
/// package. A part of that name already kept from a file is replaced.
pub fn add(doc: &mut Document, name: &str, bytes: Vec<u8>) {
    if !manifest_field_ok(name) {
        return;
    }
    let mut manifest = doc.passthrough.get(EMBEDDED_PARTS).map(|m| EmbeddedManifest::parse(m)).unwrap_or_default();
    manifest.parts.insert(name.to_string(), EmbeddedPart { content_type: CONTENT_TYPE.to_string(), rels: Vec::new() });
    doc.passthrough.insert(EMBEDDED_PARTS.into(), Arc::new(manifest.to_bytes()));
    doc.passthrough.insert(name.to_string(), Arc::new(bytes));
    let mut roots: Vec<String> = doc
        .passthrough
        .get(CUSTOM_XML_ROOTS)
        .map(|r| String::from_utf8_lossy(r).lines().filter(|l| !l.ends_with(&format!("\t{name}"))).map(str::to_string).collect())
        .unwrap_or_default();
    roots.push(format!("P\t{}\t{name}", rt::CUSTOM_XML));
    doc.passthrough.insert(CUSTOM_XML_ROOTS.into(), Arc::new(format!("{}\n", roots.join("\n")).into_bytes()));
}

/// Take out of `doc` the first custom XML part `ours` recognises (Word may have renamed it
/// `customXml/itemN.xml` and given it a properties part), with the parts it relates to, and
/// return its bytes. `None` when there is none: `doc` is left as it was.
pub fn take(doc: &mut Document, ours: &dyn Fn(&[u8]) -> bool) -> Option<Vec<u8>> {
    let roots = doc.passthrough.get(CUSTOM_XML_ROOTS).cloned()?;
    let roots = String::from_utf8_lossy(&roots).into_owned();
    let mut found: Option<(String, Vec<u8>)> = None;
    let mut kept_lines = Vec::new();
    for line in roots.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        if found.is_none()
            && let [_, kind, target] = fields.as_slice()
            && rel_is(kind, rt::CUSTOM_XML)
            && let Some(bytes) = doc.passthrough.get(*target).filter(|b| b.len() <= MAX_PART && ours(b))
        {
            found = Some(((*target).to_string(), bytes.to_vec()));
            continue;
        }
        kept_lines.push(line.to_string());
    }
    let (part, bytes) = found?;
    // The part and what it relates to (Word's `itemPropsN.xml`), unless another kept root still
    // reaches them.
    let mut manifest = doc.passthrough.get(EMBEDDED_PARTS).map(|m| EmbeddedManifest::parse(m)).unwrap_or_default();
    let still_used: Vec<String> = kept_lines.iter().filter_map(|l| l.split('\t').nth(2).map(str::to_string)).collect();
    let mut todo = vec![part];
    let mut removed = 0usize;
    while let Some(p) = todo.pop() {
        if still_used.contains(&p) || removed > 64 {
            continue;
        }
        removed += 1;
        if let Some(info) = manifest.parts.remove(&p) {
            todo.extend(info.rels.into_iter().filter(|r| !r.external).map(|r| r.target));
        }
        doc.passthrough.remove(&p);
    }
    if manifest.parts.is_empty() {
        doc.passthrough.remove(EMBEDDED_PARTS);
    } else {
        doc.passthrough.insert(EMBEDDED_PARTS.into(), Arc::new(manifest.to_bytes()));
    }
    if kept_lines.is_empty() {
        doc.passthrough.remove(CUSTOM_XML_ROOTS);
    } else {
        doc.passthrough.insert(CUSTOM_XML_ROOTS.into(), Arc::new(format!("{}\n", kept_lines.join("\n")).into_bytes()));
    }
    Some(bytes)
}
