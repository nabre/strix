//! Fonts embedded in a saved Word package when the document says so (Word's *Embed fonts in the
//! file*, `Settings::embed_fonts`; the command is `file.embedFonts`). Strix embeds the fonts the
//! person added or downloaded (Design › Fonts' Atkinson Hyperlegible and Lexend, docs/dsa.md) that
//! the document uses: those another computer is unlikely to have. The system's fonts are left out,
//! as Word's *Do not embed common system fonts* does. Fonts the file already embedded stay.

use std::collections::BTreeSet;
use std::sync::Arc;

use wordcraft_doc::{Block, Blocks, CharProps, Document};
use wordcraft_docx::{FaceSlot, FontToEmbed};

/// Deepest table nesting looked into for fonts.
const MAX_DEPTH: usize = 32;
/// Most distinct font names collected from one document.
const MAX_NAMES: usize = 512;

/// `doc` with its fonts embedded when it asks for that ([`wordcraft_docx::embed_fonts`]).
pub fn embed(mut doc: Document) -> Document {
    if !doc.settings.embed_fonts {
        return doc;
    }
    let fonts = fonts_to_embed(&doc);
    wordcraft_docx::embed_fonts(&mut doc, &fonts);
    doc
}

/// The files of the person's own fonts (added or downloaded) that `doc` uses, by face.
pub fn fonts_to_embed(doc: &Document) -> Vec<FontToEmbed> {
    let used = used_fonts(doc);
    let db = wordcraft_fonts::FontDb::global();
    let mut out: Vec<FontToEmbed> = Vec::new();
    for font in db.user_fonts() {
        for (family, style) in &font.faces {
            if !used.contains(&family.to_lowercase()) {
                continue;
            }
            let face = db.face(family, style);
            if !face.family.eq_ignore_ascii_case(family) {
                continue;
            }
            let slot = match (face.weight >= 600.0, face.italic) {
                (false, false) => FaceSlot::Regular,
                (true, false) => FaceSlot::Bold,
                (false, true) => FaceSlot::Italic,
                (true, true) => FaceSlot::BoldItalic,
            };
            // The face nearest each slot's weight (400 or 700) wins it.
            let target = if matches!(slot, FaceSlot::Bold | FaceSlot::BoldItalic) { 700.0 } else { 400.0 };
            let distance = (face.weight - target).abs();
            let entry = FontToEmbed { family: family.clone(), slot, data: Arc::new(face.data().to_vec()) };
            match out.iter().position(|f| f.slot == slot && f.family.eq_ignore_ascii_case(family)) {
                Some(i) => {
                    let held = db.face(family, &style_of(slot));
                    if distance < (held.weight - target).abs()
                        && let Some(f) = out.get_mut(i)
                    {
                        *f = entry;
                    }
                }
                None => out.push(entry),
            }
        }
    }
    out
}

fn style_of(slot: FaceSlot) -> String {
    match slot {
        FaceSlot::Regular => "Regular",
        FaceSlot::Bold => "Bold",
        FaceSlot::Italic => "Italic",
        FaceSlot::BoldItalic => "Bold Italic",
    }
    .into()
}

/// The font names `doc` asks for, lower case: the theme's, the styles', the text's.
pub fn used_fonts(doc: &Document) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut add = |n: &str| {
        if names.len() < MAX_NAMES && !n.trim().is_empty() {
            names.insert(n.trim().to_lowercase());
        }
    };
    add(&doc.settings.major_font);
    add(&doc.settings.minor_font);
    let mut props = |c: &CharProps| {
        for n in [&c.font, &c.font_ea, &c.font_cs].into_iter().flatten() {
            add(n);
        }
    };
    props(&doc.styles.default_chr);
    for s in &doc.styles.styles {
        props(&s.chr);
    }
    blocks(&doc.body, &mut props, 0);
    for part in doc.parts.values() {
        blocks(&part.blocks, &mut props, 0);
    }
    names
}

fn blocks(bl: &Blocks, f: &mut dyn FnMut(&CharProps), depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    for b in bl.iter() {
        match &**b {
            Block::Para(p) => {
                f(&p.mark);
                for r in &p.runs {
                    f(&r.props);
                }
            }
            Block::Table(t) => {
                for c in t.rows.iter().flat_map(|r| r.cells.iter()) {
                    blocks(&c.blocks, f, depth + 1);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn used_fonts_come_from_the_theme_the_styles_and_the_text() {
        let mut s = crate::Session::new(Document::from_text("one two"));
        s.run("select.all", &serde_json::json!({})).unwrap();
        s.run("format.font", &serde_json::json!({"name": "Lexend"})).unwrap();
        s.run("design.themeFonts", &serde_json::json!({"heading": "Atkinson Hyperlegible", "body": "Atkinson Hyperlegible"})).unwrap();
        let used = used_fonts(&s.doc);
        assert!(used.contains("lexend") && used.contains("atkinson hyperlegible"), "{used:?}");
    }

    #[test]
    fn nothing_is_embedded_unless_asked() {
        let doc = Document::from_text("x");
        let out = embed(doc.clone());
        assert_eq!(out.passthrough, doc.passthrough);
    }
}
