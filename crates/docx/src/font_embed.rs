//! Fonts embedded in a saved package (Word's *Embed fonts in the file*, `w:embedTrueTypeFonts`):
//! the font table names each font and relates its files, obfuscated as ECMA-376 Part 1, 17.8.1
//! says (the first 32 bytes XORed with the font key, a GUID whose 16 bytes are read in reverse).
//!
//! [`embed_fonts`] doesn't write anything itself: it puts a font table, its manifest entry and the
//! font parts among the document's kept parts, which the writer copies as it copies a font table
//! read from a file (`write::embed`). A table kept from the file stays: its fonts are kept and the
//! new ones are added to it. Only TrueType outlines whose licence allows embedding are embedded
//! ([`embeddable`]): Word embeds no others.

use std::sync::Arc;

use wordcraft_doc::Document;

use crate::package::{EMBEDDED_PARTS, EmbeddedManifest, EmbeddedPart, FONT_TABLE_PART, Rel, font_table_key};
use crate::xml;

/// Most font files embedded in one package.
const MAX_FONTS: usize = 64;
/// Most bytes of font files embedded in one package.
const MAX_BYTES: usize = 128 << 20;

const REL_FONT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/font";
const CT_FONT: &str = "application/vnd.openxmlformats-officedocument.obfuscatedFont";
const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// Which of a family's four faces a file is (`w:embedRegular` … `w:embedBoldItalic`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaceSlot {
    Regular,
    Bold,
    Italic,
    BoldItalic,
}

impl FaceSlot {
    fn element(self) -> &'static str {
        match self {
            FaceSlot::Regular => "w:embedRegular",
            FaceSlot::Bold => "w:embedBold",
            FaceSlot::Italic => "w:embedItalic",
            FaceSlot::BoldItalic => "w:embedBoldItalic",
        }
    }
}

/// A font file to embed: the family name the text asks for, its face, the file.
#[derive(Clone, Debug)]
pub struct FontToEmbed {
    pub family: String,
    pub slot: FaceSlot,
    pub data: Arc<Vec<u8>>,
}

/// Whether `data` is a TrueType font (not a collection, not CFF outlines) whose OS/2 `fsType`
/// allows embedding: not Restricted License alone, not bitmaps only. A font without an OS/2 table
/// is installable.
pub fn embeddable(data: &[u8]) -> bool {
    let be16 = |at: usize| data.get(at..at.checked_add(2)?).map(|b| u16::from_be_bytes([b[0], b[1]]));
    let be32 = |at: usize| data.get(at..at.checked_add(4)?).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    match data.get(..4) {
        Some([0, 1, 0, 0]) | Some(b"true") => {}
        _ => return false,
    }
    let Some(tables) = be16(4) else { return false };
    for i in 0..usize::from(tables) {
        let rec = 12usize.saturating_add(i.saturating_mul(16));
        if data.get(rec..rec.saturating_add(4)) != Some(b"OS/2") {
            continue;
        }
        let Some(fs_type) = be32(rec.saturating_add(8)).and_then(|off| be16((off as usize).saturating_add(8))) else { return false };
        let restricted_only = fs_type & 0x000f == 0x0002;
        let bitmap_only = fs_type & 0x0200 != 0;
        return !restricted_only && !bitmap_only;
    }
    data.len() >= 12
}

/// A deterministic font key for `data` (any GUID will do; the same file gets the same key, so
/// saving twice writes the same package).
fn key_for(data: &[u8], n: usize) -> ([u8; 16], String) {
    // FNV-1a, twice with different seeds, over the file and its position.
    let hash = |seed: u64| {
        let mut h = seed;
        for b in data.iter().take(1 << 20).chain(n.to_le_bytes().iter()) {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        h ^ (data.len() as u64)
    };
    let mut guid = [0u8; 16];
    guid[..8].copy_from_slice(&hash(0xcbf2_9ce4_8422_2325).to_be_bytes());
    guid[8..].copy_from_slice(&hash(0x8422_2325_cbf2_9ce4).to_be_bytes());
    let hex: String = guid.iter().map(|b| format!("{b:02X}")).collect();
    let text = format!(
        "{{{}-{}-{}-{}-{}}}",
        hex.get(0..8).unwrap_or(""),
        hex.get(8..12).unwrap_or(""),
        hex.get(12..16).unwrap_or(""),
        hex.get(16..20).unwrap_or(""),
        hex.get(20..32).unwrap_or("")
    );
    let mut key = [0u8; 16];
    for (i, b) in guid.iter().enumerate() {
        if let Some(k) = key.get_mut(15 - i) {
            *k = *b;
        }
    }
    (key, text)
}

/// `w:font` elements of a kept font table, as XML, except those for `skip` (ignoring case).
fn kept_fonts(doc: &Document, skip: &[String]) -> (Vec<String>, Vec<Rel>) {
    let Some(key) = font_table_key(&doc.passthrough) else { return (Vec::new(), Vec::new()) };
    let Some(table) = doc.passthrough.get(key).and_then(|b| xml::parse(b).ok()) else { return (Vec::new(), Vec::new()) };
    let rels = doc
        .passthrough
        .get(EMBEDDED_PARTS)
        .map(|m| EmbeddedManifest::parse(m))
        .and_then(|m| m.parts.get(key).map(|p| p.rels.clone()))
        .unwrap_or_default();
    let fonts = table
        .children("w:font")
        .filter(|f| f.attr("w:name").is_none_or(|n| !skip.iter().any(|s| s.eq_ignore_ascii_case(n))))
        .map(|f| f.to_xml())
        .collect();
    (fonts, rels)
}

/// Embeds `fonts` in `doc`'s kept parts (see the module docs). Files that aren't [`embeddable`]
/// are left out, as are those past the limits. Returns how many files were embedded.
pub fn embed_fonts(doc: &mut Document, fonts: &[FontToEmbed]) -> usize {
    let mut families: Vec<String> = Vec::new();
    let mut chosen: Vec<&FontToEmbed> = Vec::new();
    let mut bytes = 0usize;
    for f in fonts.iter().filter(|f| embeddable(&f.data)) {
        // One file per face, and a file once per family (a variable font's instances are one file).
        let repeat = |c: &&FontToEmbed| c.family.eq_ignore_ascii_case(&f.family) && (c.slot == f.slot || c.data == f.data);
        if chosen.len() >= MAX_FONTS || chosen.iter().any(repeat) {
            continue;
        }
        let Some(total) = bytes.checked_add(f.data.len()).filter(|t| *t <= MAX_BYTES) else {
            log::warn!("docx: fonts to embed past {MAX_BYTES} bytes; {} left out", f.family);
            continue;
        };
        bytes = total;
        if !families.iter().any(|x| x.eq_ignore_ascii_case(&f.family)) {
            families.push(f.family.clone());
        }
        chosen.push(f);
    }
    if chosen.is_empty() {
        return 0;
    }
    let (kept, mut rels) = kept_fonts(doc, &families);
    let old_key = font_table_key(&doc.passthrough).map(str::to_string);
    let mut manifest = doc.passthrough.get(EMBEDDED_PARTS).map(|m| EmbeddedManifest::parse(m)).unwrap_or_default();
    let mut table = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:fonts xmlns:w="{W}" xmlns:r="{R}">"#);
    for k in &kept {
        table.push_str(k);
    }
    let mut n = 0usize;
    for family in &families {
        table.push_str(&format!(
            r#"<w:font w:name="{}"><w:charset w:val="00"/><w:family w:val="auto"/><w:pitch w:val="variable"/>"#,
            xml::esc(family)
        ));
        // CT_Font's sequence: regular, bold, italic, bold italic.
        for slot in [FaceSlot::Regular, FaceSlot::Bold, FaceSlot::Italic, FaceSlot::BoldItalic] {
            let Some(f) = chosen.iter().find(|c| c.slot == slot && c.family.eq_ignore_ascii_case(family)) else { continue };
            n = n.saturating_add(1);
            let (key, guid) = key_for(&f.data, n);
            let mut data = f.data.to_vec();
            for (i, b) in data.iter_mut().take(32).enumerate() {
                *b ^= key.get(i % 16).copied().unwrap_or(0);
            }
            // Names and ids no kept part or relationship uses.
            let mut part = format!("word/fonts/strixFont{n}.odttf");
            let mut extra = 0u32;
            while doc.passthrough.contains_key(&part) || manifest.parts.contains_key(&part) {
                extra = extra.saturating_add(1);
                part = format!("word/fonts/strixFont{n}_{extra}.odttf");
            }
            let mut id = format!("rIdStrixFont{n}");
            while rels.iter().any(|r| r.id == id) {
                id.push('_');
            }
            table.push_str(&format!(r#"<{} r:id="{id}" w:fontKey="{guid}"/>"#, slot.element()));
            rels.push(Rel { id, kind: REL_FONT.into(), target: part.clone(), external: false });
            manifest.parts.insert(part.clone(), EmbeddedPart { content_type: CT_FONT.into(), rels: Vec::new() });
            doc.passthrough.insert(part, Arc::new(data));
        }
        table.push_str("</w:font>");
    }
    table.push_str("</w:fonts>");
    if let Some(old) = old_key {
        doc.passthrough.remove(&old);
        manifest.parts.remove(&old);
    }
    manifest.parts.insert(
        FONT_TABLE_PART.into(),
        EmbeddedPart { content_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml".into(), rels },
    );
    doc.passthrough.insert(FONT_TABLE_PART.into(), Arc::new(table.into_bytes()));
    doc.passthrough.insert(EMBEDDED_PARTS.into(), Arc::new(manifest.to_bytes()));
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal TrueType header with an OS/2 table whose fsType is `fs_type`.
    fn ttf(fs_type: u16) -> Vec<u8> {
        let mut d = vec![0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
        d.extend_from_slice(b"OS/2");
        d.extend_from_slice(&[0; 4]);
        d.extend_from_slice(&28u32.to_be_bytes());
        d.extend_from_slice(&16u32.to_be_bytes());
        d.extend_from_slice(&[0; 8]);
        d.extend_from_slice(&fs_type.to_be_bytes());
        d.extend_from_slice(&[0x55; 40]);
        d
    }

    #[test]
    fn only_truetype_fonts_that_allow_it_are_embeddable() {
        assert!(embeddable(&ttf(0)), "installable");
        assert!(embeddable(&ttf(0x0008)), "editable");
        assert!(embeddable(&ttf(0x0004)), "preview & print");
        assert!(!embeddable(&ttf(0x0002)), "restricted");
        assert!(!embeddable(&ttf(0x0200)), "bitmaps only");
        assert!(!embeddable(b"OTTO\0\0\0\0\0\0\0\0"), "CFF");
        assert!(!embeddable(b"ttcf"), "collection");
        assert!(!embeddable(&[]));
        let mut cut = ttf(0);
        cut.truncate(30);
        assert!(!embeddable(&cut), "OS/2 past the end");
    }

    #[test]
    fn embedded_fonts_round_trip_through_a_package() {
        let mut doc = Document::from_text("Hello");
        doc.settings.embed_fonts = true;
        let data = Arc::new(ttf(0));
        let fonts = [
            FontToEmbed { family: "Lexend".into(), slot: FaceSlot::Regular, data: data.clone() },
            FontToEmbed { family: "Lexend".into(), slot: FaceSlot::Bold, data: data.clone() },
            FontToEmbed { family: "Shut".into(), slot: FaceSlot::Regular, data: Arc::new(ttf(2)) },
        ];
        assert_eq!(embed_fonts(&mut doc, &fonts), 1, "one file: the repeat (the same file as bold) and the restricted font are left out");
        let bytes = crate::write(&doc).unwrap();
        let back = crate::read(&bytes).unwrap();
        assert!(back.settings.embed_fonts);
        let got = crate::embedded_fonts(&back);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "Lexend");
        assert_eq!(got[0].data, *data, "de-obfuscated to the same file");
        // The stored part is obfuscated, and saving the read document again keeps it.
        let pkg = String::from_utf8_lossy(&bytes).to_string();
        assert!(pkg.contains("word/fonts/strixFont1.odttf"));
        let again = crate::read(&crate::write(&back).unwrap()).unwrap();
        assert_eq!(crate::embedded_fonts(&again).len(), 1);
        // Embedding into a document read back keeps its font and adds the new one.
        let mut more = back.clone();
        assert_eq!(embed_fonts(&mut more, &[FontToEmbed { family: "Other".into(), slot: FaceSlot::Bold, data: Arc::new(ttf(8)) }]), 1);
        let names: Vec<String> = crate::embedded_fonts(&crate::read(&crate::write(&more).unwrap()).unwrap()).into_iter().map(|f| f.name).collect();
        assert_eq!(names, ["Lexend", "Other"]);
    }
}
