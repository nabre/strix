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
        mark_end: None,
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
    // Its own end mark differs from its start's: a tick at the start, an arrow only at the end;
    // set from the Properties pane, read back as written.
    d.set_field(0, 0, "mark", "tick").unwrap();
    d.set_field(0, 0, "markEnd", "arrow").unwrap();
    assert_eq!(filled(&d.render(None)), 1);
    assert!(d.fields(0, 0).iter().any(|f| f.key == "markEnd" && f.text == "arrow"));
    let back = Drawing::from_json(&d.to_json()).unwrap();
    assert!(matches!(back.entities[0].kind, Kind::Dim { mark: Some(Mark::Tick), mark_end: Some(Mark::Arrow), .. }));
    // "As the start" again: the start's mark at both ends.
    d.set_field(0, 0, "markEnd", "asStart").unwrap();
    assert_eq!(filled(&d.render(None)), 0);
    assert!(d.set_field(0, 0, "markEnd", "sparkles").is_err());
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

/// A dimension style can show the unit after linear sizes and after level marks' elevations;
/// off (the default), the number alone, as before. Level marks take their own text and
/// triangle heights; all of it is saved only when set.
#[test]
fn dimension_and_level_texts_show_their_unit_when_the_style_says() {
    let mut d = Drawing { unit: Unit::Mm, ..Drawing::default() };
    d.add(dim()).unwrap();
    d.add(Entity::new(Kind::Level { p: P::new(0.0, 3000.0), datum: Num::new(0.0), text: None, flip: false, left: false })).unwrap();
    let plain: Vec<String> = texts(&d.render(None)).into_iter().map(|t| t.0).collect();
    assert_eq!(plain, vec!["100".to_string(), "+3.00".to_string()]);
    assert!(!d.to_json().contains("showUnit") && !d.to_json().contains("levelUnit"));
    d.dim_style.show_unit = true;
    d.dim_style.level_unit = true;
    d.dim_style.level_text = 4.0;
    d.dim_style.level_mark = 6.0;
    let r = d.render(None);
    let shown: Vec<String> = texts(&r).into_iter().map(|t| t.0.chars().filter(|c| !crate::is_marker(*c)).collect()).collect();
    assert_eq!(shown, vec!["100 mm".to_string(), "+3.00 m".to_string()]);
    assert_eq!(texts(&r)[1].1, 4.0, "the level mark's own text height");
    assert_eq!(texts(&r)[0].1, d.dim_style.text, "dimensions keep theirs");
    // Saved and read back; junk sizes come back within reason.
    let back = Drawing::from_json(&d.to_json()).unwrap();
    assert!(back.dim_style.show_unit && back.dim_style.level_unit && back.dim_style.level_mark == 6.0);
    d.dim_style.level_text = f64::NAN;
    d.dim_style.level_mark = 1e9;
    let s = d.dim_style.clone().sanitized();
    assert_eq!((s.level_text, s.level_mark), (0.0, 50.0));
    // In metres the dimension's unit follows the units shown.
    d.dim_style.units = DimUnits::M;
    let shown: Vec<String> = texts(&d.render(None)).into_iter().map(|t| t.0.chars().filter(|c| !crate::is_marker(*c)).collect()).collect();
    assert!(shown[0].ends_with(" m"), "{shown:?}");
}

/// Slope ticks (a cut's crest): long and short in turn, square to the path on the strip's side,
/// `spacing` apart; Flip puts them on the other side; a hostile spacing stays bounded.
#[test]
fn slope_ticks_alternate_and_flip() {
    let mut d = Drawing::from_json(
        r#"{"entities":[{"id":"p","type":"line","a":[0,0],"b":[100,0]},{"type":"strip","along":"p","pattern":"slope","width":4,"spacing":10,"ratio":0.5}]}"#,
    )
    .unwrap();
    let e = d.entities[1].clone();
    let ticks = d.strip_lines(&e);
    assert_eq!(ticks.len(), 11, "{ticks:?}");
    for (i, (a, b)) in ticks.iter().enumerate() {
        assert!((a.y).abs() < 1e-9 && (a.x - b.x).abs() < 1e-9, "square to the path");
        let want = if i % 2 == 0 { 4.0 } else { 2.0 };
        assert!((b.y - want).abs() < 1e-9, "tick {i}: {b:?}");
    }
    assert_eq!(d.flip_strips(&[0, 1, 99]), 1, "only strips flip; a missing index is skipped");
    let e = d.entities[1].clone();
    assert!(d.strip_lines(&e).iter().all(|(_, b)| b.y < 0.0), "now on the right");
    // The ratio is saved only when not a half, and kept within 0.1–1.
    assert!(!d.to_json().contains("ratio"));
    let mut h = Drawing::from_json(
        r#"{"entities":[{"id":"p","type":"line","a":[0,0],"b":[1e9,0]},{"type":"strip","along":"p","pattern":"slope","spacing":-5,"ratio":99}]}"#,
    )
    .unwrap();
    h.sanitize();
    assert!(matches!(h.entities[1].kind, Kind::Strip { ratio, .. } if ratio == 1.0));
    let e = h.entities[1].clone();
    assert!(h.strip_lines(&e).len() <= 20_000);
    // It renders: thin lines, no fill.
    assert!(d.render(None).prims.len() >= 11);
}

/// A dimension's and a level mark's text has a grip of its own: dragged, the text moves (in
/// scale), with a leader back when it is off its line; the measured value stays; it is saved,
/// and hostile shifts are kept within the limits.
#[test]
fn dimension_and_level_texts_move_with_their_grip() {
    let mut d = Drawing { unit: Unit::Mm, ..Drawing::default() };
    d.add(dim()).unwrap();
    d.add(Entity::new(Kind::Level { p: P::new(0.0, 3000.0), datum: Num::new(0.0), text: None, flip: false, left: false })).unwrap();
    let before = d.render(None).prims.len();
    for i in 0..2 {
        let (_, at) = d.grips(i).into_iter().find(|(g, _)| *g == Grip::Text).expect("a text grip");
        assert!(d.move_grip(i, Grip::Text, Pt::new(at.x + 40.0, at.y + 30.0)));
        let (_, now) = d.grips(i).into_iter().find(|(g, _)| *g == Grip::Text).unwrap();
        assert!((now.x - at.x - 40.0).abs() < 1e-9 && (now.y - at.y - 30.0).abs() < 1e-9);
    }
    let r = d.render(None);
    assert_eq!(r.prims.len(), before + 2, "a leader for each moved text");
    let shown: Vec<String> = texts(&r).into_iter().map(|t| t.0).collect();
    assert_eq!(shown, vec!["100".to_string(), "+3.00".to_string()], "the values stay");
    let back = Drawing::from_json(&d.to_json()).unwrap();
    assert_eq!(back.entities[0].text_shift, Some([40.0, 30.0]));
    // Junk from a file: infinite or on an object with no text to move.
    let mut h = Drawing::from_json(
        r#"{"entities":[{"type":"dim","a":[0,0],"b":[10,0],"textShift":[1e300,-1e300]},{"type":"line","a":[0,0],"b":[1,1],"textShift":[1,1]}]}"#,
    )
    .unwrap();
    h.sanitize();
    assert_eq!(h.entities[0].text_shift, Some([MAX_COORD, -MAX_COORD]));
    assert_eq!(h.entities[1].text_shift, None);
    assert!(!h.move_grip(0, Grip::Text, Pt::new(f64::NAN, 0.0)));
}

/// A scale typed by hand: `1:75`, `1/75`, `75`, `2:1`, a decimal comma; junk is refused and
/// extremes are kept within the range a drawing keeps.
#[test]
fn scales_typed_by_hand() {
    assert_eq!(parse_scale("1:75"), Some(75.0));
    assert_eq!(parse_scale(" 1 / 75 "), Some(75.0));
    assert_eq!(parse_scale("75"), Some(75.0));
    assert_eq!(parse_scale("2:1"), Some(0.5));
    assert_eq!(parse_scale("1:12,5"), Some(12.5));
    for junk in ["", "0", "-5", "1:0", "abc", "1:x", "NaN", "inf", "1:-2", ":", "1:2:3"] {
        assert_eq!(parse_scale(junk), None, "{junk}");
    }
    assert_eq!(parse_scale("1:1e300"), Some(SCALE_RANGE.1));
    assert_eq!(parse_scale("1e300:1"), Some(SCALE_RANGE.0));
    assert_eq!(scale_text(parse_scale("1:75").unwrap()), "1:75");
}
