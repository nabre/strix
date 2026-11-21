//! Strix: lines holding inline math keep room above and below it (`para/math_leading.rs`).
#[allow(unused_imports)]
use crate::*;
use wordcraft_doc::Pos;
use wordcraft_doc::para::InlineObject;

fn lay(doc: &Document) -> DocLayout {
    let mut c = LayoutCache::new();
    layout(doc, &mut c, &LayoutOptions::default())
}

fn eq(s: &str) -> InlineObject {
    InlineObject::Equation {
        linear: s.into(),
        display: false,
        math: wordcraft_doc::math::Math { nodes: wordcraft_doc::math::parse_linear(s), ..Default::default() },
    }
}

/// Two calculations on lines of one paragraph (Shift+Enter between them): the subscripts and the
/// parentheses of the first never come near the second, with a quarter of the font size free
/// below the first and above the second (the lines took only the math's height and looked glued).
#[test]
fn lines_of_math_keep_wide_spacing() {
    crate::test_fonts();
    let mut d = Document::from_text("");
    let p = d.para_mut(StoryRef::Body, &Path::top(0)).unwrap();
    // The first equation's U+FFFC takes three bytes, the line break one.
    p.insert_object(0, eq("F_(2,h)=(10 kN)·cos(30°)"), &Default::default()).unwrap();
    p.insert_text(3, "\n", &Default::default()).unwrap();
    p.insert_object(4, eq("F_(2,v)=(10 kN)·sin(30°)"), &Default::default()).unwrap();
    let l = lay(&d);
    let first = l.equation_box(&Pos::body(0, 0), 0).unwrap();
    let second = l.equation_box(&Pos::body(0, 4), 0).unwrap();
    assert!(second.base > first.base, "{first:?} {second:?}");
    // The math's own font size (the equation's text is 12 pt here).
    let size = 12.0;
    let pad = 0.25 * size;
    let pitch = second.base - first.base;
    let needed = first.math.descent + pad + pad + second.math.ascent;
    assert!(pitch >= needed - 0.1, "pitch {pitch} < {needed}: {first:?} {second:?}");
    // The first line's box holds its math with that room above it.
    assert!(first.base - first.math.ascent - first.top >= pad - 0.1, "{first:?}");
}
