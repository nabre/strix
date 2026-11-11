//! DSA formatting (Design › Document Formatting › DSA, docs/dsa.md): a test laid out for students
//! with specific learning difficulties, in three versions that can be combined — **Reading**
//! (dyslexia), **Writing** (dysgraphia, dysorthography), **Maths and memory** (dyscalculia,
//! working memory) — over a common base (larger type, line spacing 1.5, left aligned, no
//! italics, no hyphenation, each exercise on a new page).
//!
//! It is formatting only: styles, the document's hyphenation and the exercises' series settings
//! (page breaks, graph paper). The text is never touched, so in a master document nothing reaches
//! the subdocuments' files (they keep their own styles and series settings).
//!
//! It can be taken off: before changing a style, its formatting is kept in a hidden style
//! (`StrixDsaBase-<id>`), and the settings it changes in a hidden marker style (`StrixDsa`), whose
//! name records the versions on and the settings as they were. Word keeps hidden styles, so a
//! file saved as `.docx` and opened again still goes back to its own formatting.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use wordcraft_doc::graph_paper::PT_PER_MM;
use wordcraft_doc::props::{Align, Border, BorderStyle, Borders, LineSpacing, Rgb};
use wordcraft_doc::styles::{Style, StyleKind};

use super::{STYLE_DATA, STYLE_QUESTION, scan};
use crate::{CmdError, CmdResult, Session};

/// The marker style's id, and the prefix of its name before the recorded state.
const MARKER: &str = "StrixDsa";
const MARKER_NAME: &str = "Strix DSA ";
/// The prefix of a kept style's id and name.
const BASE: &str = "StrixDsaBase-";
const BASE_NAME: &str = "Strix DSA base: ";
/// Longest marker name read back, in bytes.
const MAX_MARKER: usize = 4096;

/// Body text size: the base's, Reading's.
const SIZE: f32 = 13.0;
const SIZE_READING: f32 = 14.0;
/// Extra space between letters, as a share of the size (WCAG 1.4.12: 0.12 em).
const LETTER_SPACING: f32 = 0.12;
/// Space after a question: the base's, Reading's, Writing's (room to write), points.
const QUESTION_AFTER: f32 = 8.0;
const QUESTION_AFTER_READING: f32 = 12.0;
const QUESTION_AFTER_WRITING: f32 = 36.0;
/// Writing's graph paper: 1 cm squares, heavier lines (pt).
const GRID_MM: f32 = 10.0;
const GRID_WIDTH: f32 = 0.75;
/// The frame around the data (Maths and memory).
const DATA_BORDER: Border = Border { style: BorderStyle::Single, width: 1.0, color: Some(Rgb(0x40, 0x40, 0x40)), space: 4.0 };

/// The versions on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct Versions {
    reading: bool,
    writing: bool,
    math: bool,
}

impl Versions {
    fn any(self) -> bool {
        self.reading || self.writing || self.math
    }
}

/// What the marker records: the versions on, and the settings as they were before.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Marker {
    #[serde(flatten)]
    on: Versions,
    /// The document's automatic hyphenation.
    hyphenation: bool,
    /// The series' page break before each exercise (`None`: no exercises then).
    before_exercise: Option<bool>,
    /// The series' graph paper square (mm) and line weight (pt), when it had graph paper and
    /// Writing changed it.
    grid: Option<(f32, f32)>,
}

fn marker(s: &Session) -> Option<Marker> {
    let st = s.doc.styles.styles.iter().find(|st| st.id == MARKER)?;
    let json = st.name.strip_prefix(MARKER_NAME).filter(|j| j.len() <= MAX_MARKER)?;
    serde_json::from_str(json).ok()
}

/// The versions on now.
fn current(s: &Session) -> Versions {
    marker(s).map(|m| m.on).unwrap_or_default()
}

fn hidden_style(id: String, name: String, kind: StyleKind) -> Style {
    Style { id, name, kind, hidden: true, ..Style::default() }
}

/// The styles the base and the versions change: the paragraph and character styles but the
/// hidden ones (these markers among them).
fn touched(st: &Style) -> bool {
    matches!(st.kind, StyleKind::Paragraph | StyleKind::Character) && !st.id.starts_with(MARKER)
}

/// Back to the document's own formatting: every kept style's formatting returns to its style,
/// the settings to what the marker recorded; the hidden styles go. Whether anything changed.
fn restore(s: &mut Session) -> Result<bool, CmdError> {
    let m = marker(s);
    let sheet = Arc::make_mut(&mut s.doc.styles);
    let kept: Vec<Style> = sheet.styles.iter().filter(|st| st.id.starts_with(BASE)).cloned().collect();
    let had_marker = sheet.styles.iter().any(|st| st.id == MARKER);
    if kept.is_empty() && !had_marker {
        return Ok(false);
    }
    for k in &kept {
        let Some(id) = k.id.strip_prefix(BASE) else { continue };
        if let Some(st) = sheet.styles.iter_mut().find(|st| st.id == id) {
            st.para = k.para.clone();
            st.chr = k.chr.clone();
        }
    }
    sheet.styles.retain(|st| !st.id.starts_with(MARKER));
    if let Some(m) = m {
        s.doc.settings.auto_hyphenation = m.hyphenation;
        let has_exercises = !scan(s).exercises.is_empty();
        if has_exercises {
            if let Some(b) = m.before_exercise {
                super::breaks::command(s, &json!({"beforeExercise": b}))?;
            }
            if let Some((mm, width)) = m.grid {
                super::show::grid_command(s, &json!({"spacing": mm, "width": width}))?;
            }
        }
    }
    Ok(true)
}

/// The first exercise's graph paper square (mm) and line weight (pt), when it has graph paper.
fn grid_now(s: &Session) -> Option<(f32, f32)> {
    let layout = scan(s);
    let first = layout.exercises.first()?;
    let g = super::show::grid_of(s, first)?;
    Some((g.spacing / PT_PER_MM, g.line.width))
}

/// The versions `on` laid over the document's own formatting (no marker present).
fn apply(s: &mut Session, on: Versions) -> Result<(), CmdError> {
    let layout = scan(s);
    let first = layout.exercises.first();
    let mut m = Marker { on, hyphenation: s.doc.settings.auto_hyphenation, before_exercise: first.map(|e| e.breaks.exercise), grid: None };
    if on.writing {
        m.grid = grid_now(s);
    }
    let size = if on.reading { SIZE_READING } else { SIZE };
    let question_after = if on.writing {
        QUESTION_AFTER_WRITING
    } else if on.reading {
        QUESTION_AFTER_READING
    } else {
        QUESTION_AFTER
    };
    let sheet = Arc::make_mut(&mut s.doc.styles);
    let mut kept = Vec::new();
    for st in sheet.styles.iter_mut().filter(|st| touched(st)) {
        let before = (st.para.clone(), st.chr.clone());
        if st.chr.italic == Some(true) {
            // No italics: emphasis in bold.
            st.chr.italic = None;
            st.chr.bold = Some(true);
        }
        if st.kind == StyleKind::Paragraph && st.para.align == Some(Align::Justify) {
            st.para.align = Some(Align::Left);
        }
        if st.id == "Normal" {
            st.chr.size = Some(size);
            st.para.align = Some(Align::Left);
            st.para.line_spacing = Some(LineSpacing::Multiple(1.5));
            st.para.space_after = Some(st.para.space_after.unwrap_or(0.0).max(8.0));
            if on.reading {
                st.chr.spacing = Some((size * LETTER_SPACING * 10.0).round() / 10.0);
            }
        }
        if st.id == STYLE_QUESTION {
            st.para.space_after = Some(st.para.space_after.unwrap_or(0.0).max(question_after));
        }
        if st.id == STYLE_DATA && on.math {
            let b = Some(DATA_BORDER);
            st.para.borders = Some(Borders { top: b, left: b, bottom: b, right: b, between: None, ..Borders::default() });
        }
        if (st.para.clone(), st.chr.clone()) != before {
            let mut k = hidden_style(format!("{BASE}{}", st.id), format!("{BASE_NAME}{}", st.name), st.kind);
            (k.para, k.chr) = before;
            kept.push(k);
        }
    }
    sheet.styles.extend(kept);
    let name = serde_json::to_string(&m).map_err(|e| CmdError::Failed(e.to_string()))?;
    sheet.styles.push(hidden_style(MARKER.into(), format!("{MARKER_NAME}{name}"), StyleKind::Paragraph));
    s.doc.settings.auto_hyphenation = false;
    if first.is_some() {
        super::breaks::command(s, &json!({"beforeExercise": true}))?;
        if on.writing && m.grid.is_some() {
            super::show::grid_command(s, &json!({"spacing": GRID_MM, "width": GRID_WIDTH}))?;
        }
    }
    Ok(())
}

/// A subdocument's file written from a master (`super::keep_own`): the master's DSA formatting
/// stays the master's. A style the file doesn't have comes with the formatting it had before
/// DSA formatting, and the hidden styles go (the file's own come back with its other styles).
pub(super) fn leave_out(doc: &mut wordcraft_doc::Document, file: &wordcraft_doc::Document) {
    if !doc.styles.styles.iter().any(|st| st.id.starts_with(MARKER)) {
        return;
    }
    let sheet = Arc::make_mut(&mut doc.styles);
    let kept: Vec<Style> = sheet.styles.iter().filter(|st| st.id.starts_with(BASE)).cloned().collect();
    for k in &kept {
        let Some(id) = k.id.strip_prefix(BASE) else { continue };
        if file.styles.get(id).is_none()
            && let Some(st) = sheet.styles.iter_mut().find(|st| st.id == id)
        {
            st.para = k.para.clone();
            st.chr = k.chr.clone();
        }
    }
    sheet.styles.retain(|st| !st.id.starts_with(MARKER));
}

fn state(s: &Session) -> Value {
    let on = current(s);
    json!({"reading": on.reading, "writing": on.writing, "math": on.math, "on": on.any()})
}

/// `design.dsa`: turns the versions given on or off (`"none": true` all off); without params,
/// the versions on.
pub(crate) fn command(s: &mut Session, v: &Value) -> CmdResult {
    if let Some(m) = v.as_object()
        && let Some(k) = m.keys().find(|k| !matches!(k.as_str(), "reading" | "writing" | "math" | "none"))
    {
        return Err(CmdError::Params(format!("unknown parameter `{k}`")));
    }
    let flag = |k: &str| match v.get(k) {
        None => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(CmdError::Params(format!("`{k}` must be true or false"))),
    };
    let (reading, writing, math, none) = (flag("reading")?, flag("writing")?, flag("math")?, flag("none")?);
    let was = current(s);
    let want = if none == Some(true) {
        Versions::default()
    } else {
        Versions { reading: reading.unwrap_or(was.reading), writing: writing.unwrap_or(was.writing), math: math.unwrap_or(was.math) }
    };
    if want == was && (want.any() || marker(s).is_none()) {
        return Ok(state(s));
    }
    restore(s)?;
    if want.any() {
        apply(s, want)?;
    }
    s.touch();
    Ok(state(s))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wordcraft_doc::Document;
    use wordcraft_doc::props::{Align, LineSpacing};

    use crate::Session;

    fn run(s: &mut Session, id: &str, v: serde_json::Value) -> serde_json::Value {
        s.run(id, &v).unwrap()
    }

    /// Two exercises with graph paper, an italic emphasis style and hyphenation on.
    fn test() -> Session {
        let mut s = Session::new(Document::new());
        run(&mut s, "review.language", json!({"lang": "it-CH", "default": true}));
        run(&mut s, "document.setText", json!({"text": "Verifica"}));
        for t in ["Trave", "Pilastro"] {
            run(&mut s, "caret.docEnd", json!({}));
            run(&mut s, "exercise.insert", json!({"title": t, "questions": ["Reazioni"]}));
        }
        run(&mut s, "exercise.grid", json!({"on": true}));
        s.doc.settings.auto_hyphenation = true;
        s
    }

    fn style<'a>(s: &'a Session, id: &str) -> &'a wordcraft_doc::styles::Style {
        s.doc.styles.styles.iter().find(|st| st.id == id).unwrap()
    }

    fn grid(s: &mut Session) -> serde_json::Value {
        run(s, "exercise.show", json!({}))["grid"].clone()
    }

    /// Reading and Maths together, then off: the base and both versions apply, and taking them
    /// off brings back the document's own styles and settings exactly.
    #[test]
    fn versions_combine_and_come_off() {
        let mut s = test();
        let styles_before = s.doc.styles.clone();
        let breaks_before = run(&mut s, "exercise.pageBreaks", json!({}))["beforeExercise"].clone();
        let grid_before = grid(&mut s);

        let r = run(&mut s, "design.dsa", json!({"reading": true, "math": true}));
        assert_eq!((r["reading"].clone(), r["writing"].clone(), r["math"].clone()), (json!(true), json!(false), json!(true)));
        let normal = style(&s, "Normal");
        assert_eq!(normal.chr.size, Some(14.0));
        assert_eq!(normal.chr.spacing, Some(1.7));
        assert_eq!(normal.para.line_spacing, Some(LineSpacing::Multiple(1.5)));
        assert_eq!(normal.para.align, Some(Align::Left));
        assert!(style(&s, super::STYLE_DATA).para.borders.is_some_and(|b| b.top.is_some() && b.right.is_some()));
        assert!(s.doc.styles.styles.iter().filter(|st| st.chr.italic == Some(true) && !st.hidden).count() == 0, "no italic style left");
        assert!(!s.doc.settings.auto_hyphenation);
        assert_eq!(run(&mut s, "exercise.pageBreaks", json!({}))["beforeExercise"], true);
        // Writing alone was off: the graph paper stays as it was.
        assert_eq!(grid(&mut s), grid_before);

        // Writing added: 1 cm squares, more room under each question.
        run(&mut s, "design.dsa", json!({"writing": true}));
        assert_eq!(run(&mut s, "design.dsa", json!({}))["writing"], true);
        assert!((grid(&mut s)["spacing"].as_f64().unwrap() - 10.0).abs() < 0.01, "{}", grid(&mut s));
        assert_eq!(style(&s, super::STYLE_QUESTION).para.space_after, Some(36.0));
        assert_eq!(style(&s, "Normal").chr.size, Some(14.0), "Reading's size still holds");

        // All off: everything as it was.
        let r = run(&mut s, "design.dsa", json!({"none": true}));
        assert_eq!(r["on"], false);
        assert_eq!(s.doc.styles, styles_before);
        assert!(s.doc.settings.auto_hyphenation);
        assert_eq!(run(&mut s, "exercise.pageBreaks", json!({}))["beforeExercise"], breaks_before);
        assert_eq!(grid(&mut s), grid_before);
        // Off again changes nothing.
        let before = s.doc.styles.clone();
        run(&mut s, "design.dsa", json!({"none": true}));
        assert_eq!(s.doc.styles, before);
    }

    /// The formatting goes through .docx: opened again, the versions are still on and taking
    /// them off still brings back the document's own styles.
    #[test]
    fn it_comes_off_after_a_docx_round_trip() {
        let mut s = test();
        let normal_before = style(&s, "Normal").clone();
        run(&mut s, "design.dsa", json!({"reading": true}));
        let bytes = wordcraft_docx::write(&s.doc).unwrap();
        let mut back = Session::new(wordcraft_docx::read(&bytes).unwrap());
        assert_eq!(run(&mut back, "design.dsa", json!({}))["reading"], true);
        run(&mut back, "design.dsa", json!({"reading": false}));
        let normal = style(&back, "Normal");
        assert_eq!((normal.chr.size, normal.chr.spacing), (normal_before.chr.size, normal_before.chr.spacing));
        assert!(!back.doc.styles.styles.iter().any(|st| st.id.starts_with(super::MARKER)));
    }

    /// Bad params change nothing.
    #[test]
    fn bad_params_change_nothing() {
        let mut s = test();
        let before = s.doc.styles.clone();
        for bad in [json!({"reading": 1}), json!({"colour": true}), json!({"none": "yes"}), json!({"writing": true, "math": "x"})] {
            assert!(s.run("design.dsa", &bad).is_err(), "{bad}");
        }
        assert_eq!(s.doc.styles, before);
    }

    /// Without exercises it formats the text alone.
    #[test]
    fn a_document_without_exercises() {
        let mut s = Session::new(Document::new());
        run(&mut s, "document.setText", json!({"text": "Testo"}));
        run(&mut s, "design.dsa", json!({"writing": true}));
        assert_eq!(style(&s, "Normal").chr.size, Some(13.0));
        run(&mut s, "design.dsa", json!({"none": true}));
        assert!(!s.doc.styles.styles.iter().any(|st| st.id.starts_with(super::MARKER)));
    }

    /// The owner's rule: the master's formatting stays in the master. With DSA formatting in a
    /// master document, its subdocuments' files keep their own styles and graph paper, also when
    /// their text is edited there and saved back.
    #[test]
    fn in_a_master_the_files_keep_their_own_formatting() {
        let d = std::env::temp_dir().join(format!("strix-dsa-master-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let (a, b, book) = (d.join("a.xmath"), d.join("b.xmath"), d.join("book.xmath"));
        for (p, title) in [(&a, "Area"), (&b, "Pendenza")] {
            let mut f = Session::new(Document::new());
            std::sync::Arc::make_mut(&mut f.doc.styles).default_chr.lang = Some("it-IT".into());
            run(&mut f, "exercise.insert", json!({"title": title, "questions": ["Trova x"]}));
            run(&mut f, "exercise.grid", json!({"on": true}));
            run(&mut f, "file.saveAs", json!({"path": p.to_str().unwrap()}));
        }
        let own = |p: &std::path::Path| {
            let mut f = Session::new(crate::io::open_path(p).unwrap());
            let normal = style(&f, "Normal").clone();
            let marked = f.doc.styles.styles.iter().any(|st| st.id.starts_with(super::MARKER));
            (normal.chr.size, normal.para.line_spacing, marked, grid(&mut f)["spacing"].clone(), f.doc.plain_text(wordcraft_doc::StoryRef::Body))
        };
        let before = own(&a);
        let mut s = Session::new(Document::new());
        std::sync::Arc::make_mut(&mut s.doc.styles).default_chr.lang = Some("it-IT".into());
        run(&mut s, "document.setText", json!({"text": "Verifica"}));
        for p in [&a, &b] {
            run(&mut s, "caret.docEnd", json!({}));
            run(&mut s, "master.insertSubdocument", json!({"path": p.to_str().unwrap()}));
        }
        run(&mut s, "file.saveAs", json!({"path": book.to_str().unwrap()}));
        run(&mut s, "design.dsa", json!({"reading": true, "writing": true, "math": true}));
        assert_eq!(style(&s, "Normal").chr.size, Some(14.0));
        // Saved, the files keep their own formatting. (A change to the series' settings in a
        // master still makes it write the files again, with their own settings: a known gap of
        // master documents, docs/master-documents.md.)
        run(&mut s, "file.save", json!({}));
        assert_eq!(own(&a), before);
        // Text edited in the master goes back; the formatting doesn't.
        run(&mut s, "select.text", json!({"text": "Area"}));
        run(&mut s, "text.insert", json!({"text": "Superficie"}));
        run(&mut s, "file.save", json!({}));
        let after = own(&a);
        assert!(after.4.contains("Superficie"), "{}", after.4);
        assert_eq!((after.0, after.1, after.2, after.3), (before.0, before.1, before.2, before.3));
        let _ = std::fs::remove_dir_all(&d);
    }
}
