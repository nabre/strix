//! Named text and dimension styles, the marks at each end and their width, and the drawing's unit.

use crate::*;

fn dim() -> Entity {
    Entity::new(Kind::Dim {
        a: P::new(0.0, 0.0),
        b: P::new(100.0, 0.0),
        offset: 8.0,
        offset_units: 0.0,
        orient: Orient::Aligned,
        text: None,
        below: None,
        mark: None,
    })
}

fn filled(r: &Rendered) -> usize {
    r.prims.iter().filter(|p| matches!(p, Prim::Path { fill: true, .. })).count()
}

fn texts(r: &Rendered) -> Vec<(String, f64, TextLook)> {
    r.prims
        .iter()
        .filter_map(|p| if let Prim::Text { text, height, look, .. } = p { Some((text.clone(), *height, look.clone())) } else { None })
        .collect()
}

/// A dimension style can end its line with a different mark from the one it starts with, and
/// give the marks their own width; the line runs past a tick but not into an arrow.
#[test]
fn a_dimension_style_has_a_start_and_an_end_mark_and_a_mark_width() {
    let mut d = Drawing::default();
    d.dim_style.mark = Mark::Tick;
    d.dim_style.mark_end = Some(Mark::Arrow);
    d.dim_style.mark_width = 0.7;
    d.add(dim()).unwrap();
    let r = d.render(None);
    assert_eq!(filled(&r), 1, "one arrowhead, at the end: {:?}", r.prims);
    let widths: Vec<f64> = r.prims.iter().filter_map(|p| if let Prim::Path { width, .. } = p { Some(*width) } else { None }).collect();
    assert_eq!(widths.iter().filter(|w| (**w - 0.7).abs() < 1e-9).count(), 2, "the tick and the arrowhead: {widths:?}");
    // Read back as written.
    let back = Drawing::from_json(&d.to_json()).unwrap();
    assert_eq!(back.dim_style, d.dim_style);
    // A dimension's own mark is at both ends.
    if let Some(Kind::Dim { mark, .. }) = d.entities.first_mut().map(|e| &mut e.kind) {
        *mark = Some(Mark::Arrow);
    }
    assert_eq!(filled(&d.render(None)), 2);
    // A width from a file is kept within reason.
    let wild = Drawing::from_json(r#"{"dimStyle": {"markWidth": 1e9, "markEnd": "sparkles"}}"#).unwrap();
    assert_eq!((wild.dim_style.mark_width, wild.dim_style.mark_end), (2.0, None));
}

/// Named dimension styles: a dimension takes its style's look, Standard when its style is gone;
/// new, renamed and deleted styles carry the objects that take them along.
#[test]
fn named_dimension_styles_follow_their_objects() {
    let mut d = Drawing::default();
    let name = d.new_style(StyleKind::Dim, "Arrows", None).unwrap();
    assert_eq!(d.current_dim_style.as_deref(), Some("Arrows"));
    d.dim_style_mut(&name).unwrap().mark = Mark::Arrow;
    d.dim_style_mut(&name).unwrap().text = 5.0;
    d.add(dim()).unwrap();
    d.set_style(&[0], "Arrows").unwrap();
    let r = d.render(None);
    assert_eq!(filled(&r), 2);
    assert_eq!(texts(&r).first().map(|t| t.1), Some(5.0));
    assert!(d.new_style(StyleKind::Dim, "Arrows", None).is_err(), "names are unique");
    assert!(d.new_style(StyleKind::Dim, STANDARD, None).is_err());
    assert_eq!(d.free_style_name(StyleKind::Dim, "Arrows"), "Arrows 2");
    d.rename_style(StyleKind::Dim, "Arrows", "Arch").unwrap();
    assert_eq!((d.entities[0].style.as_deref(), d.current_dim_style.as_deref()), (Some("Arch"), Some("Arch")));
    let back = Drawing::from_json(&d.to_json()).unwrap();
    assert_eq!(back, d, "kept in the drawing");
    assert_eq!(d.delete_style(StyleKind::Dim, "Arch").unwrap(), 1);
    assert_eq!((d.entities[0].style.as_deref(), d.current_dim_style.as_deref()), (None, None));
    assert_eq!(filled(&d.render(None)), 0, "back to Standard's ticks");
    assert!(d.delete_style(StyleKind::Dim, STANDARD).is_err());
    // A text style doesn't go on a dimension.
    d.new_style(StyleKind::Text, "Notes", None).unwrap();
    assert!(d.set_style(&[0], "Notes").is_err());
}

/// Text styles: a text's font, bold and italic follow its style; a new text takes the current
/// style's height; a dimension style can name the text style of its texts.
#[test]
fn text_styles_set_the_font_and_weight() {
    let mut d = Drawing::default();
    d.new_style(StyleKind::Text, "Title", None).unwrap();
    {
        let s = d.text_style_mut("Title").unwrap();
        s.font = Some("Liberation Serif".into());
        s.bold = true;
        s.height = 7.0;
    }
    assert_eq!(d.new_text_height(), 7.0);
    d.add(Entity::new(Kind::Text { p: P::new(0.0, 0.0), value: "Plan".into(), height: 2.5, angle: 0.0, align: Align::Left })).unwrap();
    d.set_style(&[0], "Title").unwrap();
    let r = d.render(None);
    let (_, height, look) = texts(&r).into_iter().next().unwrap();
    assert_eq!(height, 7.0, "the style's height when given");
    assert_eq!(look, TextLook { font: Some("Liberation Serif".into()), bold: true, italic: false });
    let svg = d.render(None).to_svg([0, 0, 0]);
    assert!(svg.contains(r#"font-family="Liberation Serif""#) && svg.contains(r#"font-weight="bold""#), "{svg}");
    // The dimension texts in a text style.
    d.dim_style.text_style = Some("Title".into());
    d.add(dim()).unwrap();
    assert!(texts(&d.render(None)).iter().all(|t| t.2.bold));
    // Renamed or deleted, the dimension style follows.
    d.rename_style(StyleKind::Text, "Title", "Heading").unwrap();
    assert_eq!(d.dim_style.text_style.as_deref(), Some("Heading"));
    d.delete_style(StyleKind::Text, "Heading").unwrap();
    assert_eq!((d.dim_style.text_style.as_deref(), d.entities[0].style.as_deref()), (None, None));
    // Junk styles from a file are dropped; a current style that isn't there is Standard.
    let junk = Drawing::from_json(
        r#"{"textStyles": [{"name": ""}, {"name": "Standard"}, {"name": "A", "height": 1e9}, {"name": "A"}], "currentTextStyle": "B"}"#,
    )
    .unwrap();
    assert_eq!(junk.text_styles.len(), 1);
    assert_eq!((junk.text_styles[0].height, junk.current_text_style.clone()), (50.0, None));
}

/// The drawing's unit: converted, a drawing keeps its real size (and its look on paper);
/// without, its numbers stay and mean the new unit. Expressions are left alone.
#[test]
fn the_drawing_unit_converts_or_reinterprets() {
    let mut d = Drawing::default();
    d.add(Entity::new(Kind::Line { a: P::new(0.0, 0.0), b: P::new(1000.0, 500.0) })).unwrap();
    d.add(Entity::new(Kind::Circle { c: P(Num::expr("L"), Num::new(0.0)), r: Num::new(250.0) })).unwrap();
    let paper = d.render(None).size;
    let mut m = d.clone();
    m.set_unit(Unit::M, true);
    assert_eq!(m.unit, Unit::M);
    let Kind::Line { b, .. } = &m.entities[0].kind else { panic!() };
    assert_eq!((b.0.value, b.1.value), (1.0, 0.5));
    let Kind::Circle { c, r } = &m.entities[1].kind else { panic!() };
    assert_eq!((c.0.expr.as_deref(), r.value), (Some("L"), 0.25));
    let size = m.render(None).size;
    assert!((size.0 - paper.0).abs() < 1e-6 && (size.1 - paper.1).abs() < 1e-6, "{size:?} vs {paper:?}");
    let mut kept = d.clone();
    kept.set_unit(Unit::Cm, false);
    let Kind::Line { b, .. } = &kept.entities[0].kind else { panic!() };
    assert_eq!((kept.unit, b.0.value), (Unit::Cm, 1000.0));
}
