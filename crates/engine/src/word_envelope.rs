//! Strix objects through Word (docs/word-round-trip.md): a `.docx` Strix writes carries its Strix
//! objects in a form Word keeps when it saves the file, and a file that comes back is made live
//! again.
//!
//! Strix's own markup (`strix:calc` on an equation or a table, the CAD source attached to its
//! picture, the exercise data on its content control, graph paper on a section or paragraph) is
//! in a namespace other programs may ignore, and Word drops it when it saves. So a Word package
//! (`.docx`, `.docm`, `.dotx`, `.dotm`) gets, on top of that markup:
//!
//! - every calculation, calculated table, CAD drawing and graph-paper paragraph wrapped in a
//!   content control tagged `strix:obj:N` (Word keeps content controls and their tags);
//! - a custom XML part (`customXml/strixObjects.xml`, which Word keeps too) holding, for each
//!   object N, its source and a fingerprint of what it showed, and the exercise data and the
//!   sections' graph paper and frontispiece, in document order.
//!
//! Reading any Word package ([`unwrap`]) takes that part out, puts back the markup Word dropped
//! and removes the wrappers, so the document is exactly what a `.xmath` would hold. An object
//! whose content no longer matches its fingerprint was edited in another program: it stays as it
//! is (static, its wrapper kept) and is listed as pending ([`PENDING`]), for the person to choose
//! between recalculating it from its Strix source and keeping the change ([`resolve`],
//! `file.externalEdits`). `.xmath` files are written without any of this.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use wordcraft_doc::control::{ContentControl, ControlKind, ControlLock};
use wordcraft_doc::graph_paper::GraphPaper;
use wordcraft_doc::graphic::{Graphic, GraphicKind};
use wordcraft_doc::para::OBJ;
use wordcraft_doc::{Block, Blocks, Document, InlineObject, Paragraph};

use crate::cmd::exercise::TAG_EXERCISE;

/// The tag of an object's wrapper: `strix:obj:` and the object's number.
pub(crate) const TAG_PREFIX: &str = "strix:obj:";
/// The namespace of the custom XML part.
pub(crate) const NS: &str = "urn:io.github.nabre.strix:objects:2026";
/// The part's name when Strix writes it (Word may rename it `customXml/itemN.xml`).
pub(crate) const PART: &str = "customXml/strixObjects.xml";
/// `Document::passthrough` key: the objects edited in another program, still to decide (JSON
/// [`Entry`] list). Never written to a file.
pub const PENDING: &str = "strix:objects.pending";
/// The manifest's format version.
const VERSION: u32 = 1;
/// Most objects, sections and exercises a manifest is read with (hostile files).
const MAX_ENTRIES: usize = 100_000;
/// Deepest table nesting walked (the DOCX reader keeps 24).
const MAX_DEPTH: usize = 32;
/// The exercise control's data element (`cmd/exercise.rs`), kept as its `w:sdtPr` child.
const EXERCISE_EXTRA: &str = "<strix:";

/// What an object was in Strix, enough to make it live again.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Source {
    /// A calculated equation, an answer or a stepped calculation (`Math`'s flags).
    Equation {
        #[serde(default)]
        calc: bool,
        #[serde(default)]
        answer: u32,
        #[serde(default)]
        stepped: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        opts: Option<String>,
    },
    /// A calculated table (`TableProps`' calc settings).
    Table {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        blanks: Vec<(usize, usize)>,
        #[serde(default)]
        names: Option<String>,
        #[serde(default)]
        math: bool,
        #[serde(default)]
        index: bool,
        /// The header row's columns not summed in the total row (`CellProps::calc_no_total`).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        no_total: Vec<usize>,
    },
    /// A CAD drawing: its `strix-cad` JSON, and what its picture was (`svg`: a fingerprint of the
    /// SVG written; `w`, `h`: its size in points).
    Cad {
        json: String,
        #[serde(default)]
        svg: u64,
        #[serde(default)]
        w: f32,
        #[serde(default)]
        h: f32,
    },
    /// A mathematical or experimental plot: its `strix-plot` JSON and SVG fingerprint.
    Plot {
        json: String,
        #[serde(default)]
        svg: u64,
        #[serde(default)]
        w: f32,
        #[serde(default)]
        h: f32,
    },
    /// Graph paper in the text flow (`ParaProps::graph`, its attribute form).
    GraphParagraph { graph: String },
}

impl Source {
    /// A stable id for scripts (`calculation`, `calculatedTable`, `cadDrawing`, `plot`, `graphPaper`).
    pub fn kind_id(&self) -> &'static str {
        match self {
            Source::Equation { .. } => "calculation",
            Source::Table { .. } => "calculatedTable",
            Source::Cad { .. } => "cadDrawing",
            Source::Plot { .. } => "plot",
            Source::GraphParagraph { .. } => "graphPaper",
        }
    }
}

/// One wrapped object: its number (the wrapper's tag), the fingerprint of what it showed (0: not
/// checked) and its source.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: u32,
    #[serde(default)]
    pub fp: u64,
    #[serde(flatten)]
    pub source: Source,
}

/// A section's Strix settings, by its place among the document's sections (0-based).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct SectionEntry {
    n: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    graph: Option<String>,
    #[serde(default)]
    front: bool,
}

/// An exercise's data, by its place among the document's exercises (0-based).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct ExerciseEntry {
    n: usize,
    extra: Vec<String>,
}

/// Any other `strix:*` content control's `<strix:...>` extra elements, by tag and 0-based index
/// among controls of that tag in document order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct ControlEntry {
    tag: String,
    n: usize,
    extra: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Manifest {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    objects: Vec<Entry>,
    #[serde(default)]
    sections: Vec<SectionEntry>,
    #[serde(default)]
    exercises: Vec<ExerciseEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    controls: Vec<ControlEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    calc: Option<crate::cmd::calc::CalcOverrides>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    dsa_styles: Vec<wordcraft_doc::styles::Style>,
}

impl Manifest {
    fn is_empty(&self) -> bool {
        self.objects.is_empty()
            && self.sections.is_empty()
            && self.exercises.is_empty()
            && self.controls.is_empty()
            && self.calc.is_none()
            && self.dsa_styles.is_empty()
    }
}

// ---------------------------------------------------------------------------------------------
// Fingerprints

/// FNV-1a, 64 bits: stable across runs and platforms (what a fingerprint in a file needs).
fn hash(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// A fingerprint of shown text: whitespace doesn't count (writers and Word space equations and
/// cells differently). Never 0, which means "not checked".
fn text_fp(text: &str) -> u64 {
    let s: String = text.chars().filter(|c| !c.is_whitespace() && *c != OBJ).collect();
    hash(s.as_bytes()).max(1)
}

/// What an equation shows, as linear text.
fn equation_text(linear: &str, math: &wordcraft_doc::math::Math) -> String {
    if math.nodes.is_empty() { linear.to_string() } else { wordcraft_doc::math::to_linear(&math.nodes) }
}

/// What a paragraph shows: its text, with equations as their linear text.
fn para_text(p: &Paragraph, out: &mut String) {
    let mut k = 0usize;
    for c in p.text.chars() {
        if c == OBJ {
            if let Some(InlineObject::Equation { linear, math, .. }) = p.objects.get(k) {
                out.push_str(&equation_text(linear, math));
            }
            k += 1;
        } else {
            out.push(c);
        }
    }
}

fn blocks_text(bl: &Blocks, out: &mut String, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    for b in bl {
        match &**b {
            Block::Para(p) => {
                para_text(p, out);
                out.push('\u{1}');
            }
            Block::Table(t) => table_text(t, out, depth + 1),
        }
    }
}

fn table_text(t: &wordcraft_doc::table::Table, out: &mut String, depth: usize) {
    for r in &t.rows {
        for c in &r.cells {
            blocks_text(&c.blocks, out, depth);
            out.push('\u{2}');
        }
        out.push('\u{3}');
    }
}

fn table_fp(t: &wordcraft_doc::table::Table) -> u64 {
    let mut s = String::new();
    table_text(t, &mut s, 0);
    text_fp(&s)
}

// ---------------------------------------------------------------------------------------------
// Which objects

fn is_strix_equation(o: &InlineObject) -> bool {
    matches!(o, InlineObject::Equation { math, .. } if math.calc || math.answer > 0 || math.stepped || math.calc_opts.is_some())
}

fn is_cad(o: &InlineObject) -> bool {
    matches!(o, InlineObject::Graphic { graphic, .. } if graphic.kind == GraphicKind::Cad && graphic.cad.is_some())
}

fn is_plot(o: &InlineObject) -> bool {
    matches!(o, InlineObject::Graphic { graphic, .. } if graphic.kind == GraphicKind::Plot && graphic.plot.is_some())
}

/// The source and fingerprint of an inline Strix object, `None` for any other object.
fn inline_entry(o: &InlineObject) -> Option<(Source, u64)> {
    match o {
        InlineObject::Equation { linear, math, .. } if is_strix_equation(o) => Some((
            Source::Equation { calc: math.calc, answer: math.answer, stepped: math.stepped, opts: math.calc_opts.clone() },
            text_fp(&equation_text(linear, math)),
        )),
        InlineObject::Graphic { w, h, graphic, .. } if is_cad(o) => {
            let svg = graphic.svg.as_deref().map_or(0, |s| hash(s.as_bytes()));
            Some((Source::Cad { json: graphic.cad.as_deref().unwrap_or("").to_string(), svg, w: *w, h: *h }, 0))
        }
        InlineObject::Graphic { w, h, graphic, .. } if is_plot(o) => {
            let svg = graphic.svg.as_deref().map_or(0, |s| hash(s.as_bytes()));
            Some((Source::Plot { json: graphic.plot.as_deref().unwrap_or("").to_string(), svg, w: *w, h: *h }, 0))
        }
        _ => None,
    }
}

fn wrapper(id: u32) -> ContentControl {
    let mut c = ContentControl::new(ControlKind::RichText);
    c.tag = format!("{TAG_PREFIX}{id}");
    c.title = "Strix".into();
    c.lock = ControlLock::SdtLocked;
    c.placeholder_text = String::new();
    c
}

fn wrapper_id(c: &ContentControl) -> Option<u32> {
    c.tag.strip_prefix(TAG_PREFIX)?.parse().ok()
}

// ---------------------------------------------------------------------------------------------
// Writing

/// `doc` as a Word package should hold it: its Strix objects wrapped and their sources in the
/// custom XML part (see the module docs). A document without Strix objects comes back as it is
/// (without wrappers left from objects still pending).
pub fn wrap(doc: &Document) -> Document {
    let mut d = doc.clone();
    strip_wrappers(&mut d);
    d.passthrough.remove(PENDING);
    let has_objects = crate::strix_objects::has_strix_objects(&d);
    let doc_calc = crate::cmd::calc::doc_overrides(&d);
    let dsa_styles: Vec<_> = d.styles.styles.iter().filter(|st| st.id.starts_with("StrixDsa")).cloned().collect();
    if dsa_styles.is_empty() && !has_objects && !doc_calc.as_ref().is_some_and(|c| !c.matches_default_options()) {
        return d;
    }
    let mut m = Manifest { version: VERSION, calc: doc_calc.filter(|c| !c.is_empty()), dsa_styles, ..Default::default() };
    let mut next = 1u32;
    for_stories(&mut d, &mut |bl| wrap_blocks(bl, &mut m, &mut next, 0));
    sections(&d, &mut |n, sp| {
        let graph = sp.graph_paper.as_ref().map(|g| g.to_attr());
        if graph.is_some() || sp.frontispiece {
            m.sections.push(SectionEntry { n, graph, front: sp.frontispiece });
        }
    });
    let mut n = 0usize;
    exercises(&d, &mut |c| {
        let extra: Vec<String> = c.extra.iter().filter(|x| x.starts_with(EXERCISE_EXTRA)).cloned().collect();
        if !extra.is_empty() {
            m.exercises.push(ExerciseEntry { n, extra });
        }
        n += 1;
    });
    let mut tag_counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    other_strix_controls(&d, &mut |c| {
        let idx = *tag_counts.entry(c.tag.clone()).and_modify(|k| *k += 1).or_insert(0);
        let extra: Vec<String> = c.extra.iter().filter(|x| x.starts_with(EXERCISE_EXTRA)).cloned().collect();
        if !extra.is_empty() && m.controls.len() < MAX_ENTRIES {
            m.controls.push(ControlEntry { tag: c.tag.clone(), n: idx, extra });
        }
    });
    if !m.is_empty() {
        wordcraft_docx::strix_custom_xml::add(&mut d, PART, manifest_xml(&m));
    }
    d
}

fn for_stories(d: &mut Document, f: &mut dyn FnMut(&mut Blocks)) {
    f(&mut d.body);
    for part in d.parts.values_mut() {
        f(&mut part.blocks);
    }
}

fn wrap_blocks(bl: &mut Blocks, m: &mut Manifest, next: &mut u32, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    for b in bl.iter_mut() {
        if !holds_strix(b, depth) {
            continue;
        }
        match Arc::make_mut(b) {
            Block::Para(p) => wrap_para(p, m, next),
            Block::Table(t) => {
                if t.props.calc && m.objects.len() < MAX_ENTRIES {
                    let id = take_id(next);
                    let source = Source::Table {
                        blanks: t
                            .rows
                            .iter()
                            .enumerate()
                            .flat_map(|(r, row)| row.cells.iter().enumerate().filter_map(move |(c, cell)| cell.props.student_blank.then_some((r, c))))
                            .collect(),
                        names: t.props.calc_names.map(|n| n.name().to_string()),
                        math: t.props.calc_math,
                        index: t.props.calc_index,
                        no_total: t
                            .rows
                            .first()
                            .map(|row| row.cells.iter().enumerate().filter(|(_, c)| c.props.calc_no_total).map(|(c, _)| c).collect())
                            .unwrap_or_default(),
                    };
                    m.objects.push(Entry { id, fp: table_fp(t), source });
                    // Innermost: it opens last and closes first, inside any control already here.
                    t.controls.open.push(wrapper(id));
                    t.controls.close = t.controls.close.saturating_add(1);
                }
                for c in t.rows.iter_mut().flat_map(|r| r.cells.iter_mut()) {
                    wrap_blocks(&mut c.blocks, m, next, depth + 1);
                }
            }
        }
    }
}

fn take_id(next: &mut u32) -> u32 {
    let id = *next;
    *next = next.saturating_add(1);
    id
}

/// Whether a block has anything to wrap (so others aren't copied).
fn holds_strix(b: &Block, depth: usize) -> bool {
    if depth > MAX_DEPTH {
        return false;
    }
    match b {
        Block::Para(p) => p.props.graph.is_some() || p.objects.iter().any(|o| is_strix_equation(o) || is_cad(o) || is_plot(o)),
        Block::Table(t) => t.props.calc || t.rows.iter().flat_map(|r| &r.cells).flat_map(|c| &c.blocks).any(|b| holds_strix(b, depth + 1)),
    }
}

fn wrap_para(p: &mut Paragraph, m: &mut Manifest, next: &mut u32) {
    // Numbers in reading order; markers inserted from the end so offsets stay right.
    let mut found: Vec<(usize, u32)> = Vec::new();
    for (off, o) in p.objects_with_offsets() {
        if m.objects.len() >= MAX_ENTRIES {
            break;
        }
        if let Some((source, fp)) = inline_entry(o) {
            let id = take_id(next);
            m.objects.push(Entry { id, fp, source });
            found.push((off, id));
        }
    }
    for (off, id) in found.into_iter().rev() {
        enclose(p, off, off + OBJ.len_utf8(), id);
    }
    if let Some(g) = p.props.graph.as_ref()
        && m.objects.len() < MAX_ENTRIES
    {
        let id = take_id(next);
        m.objects.push(Entry { id, fp: 0, source: Source::GraphParagraph { graph: g.to_attr() } });
        let end = p.text.len();
        enclose(p, 0, end, id);
    }
}

/// Wrap bytes `a..b` of `p` in object `id`'s control.
fn enclose(p: &mut Paragraph, a: usize, b: usize, id: u32) {
    let props_end = p.props_at(b).clone();
    let props_start = p.props_at(a).clone();
    if p.insert_object(b, InlineObject::ControlEnd, &props_end).is_ok()
        && p.insert_object(a, InlineObject::ControlStart { control: Box::new(wrapper(id)) }, &props_start).is_err()
    {
        // Never leave an end without its start.
        let _ = p.delete(b, b + OBJ.len_utf8());
    }
}

/// Each section's properties in order: the body's section breaks, then the last section.
fn sections(d: &Document, f: &mut dyn FnMut(usize, &wordcraft_doc::SectionProps)) {
    let mut n = 0usize;
    for b in &d.body {
        if let Block::Para(p) = &**b
            && let Some(sp) = &p.section
        {
            f(n, sp);
            n += 1;
        }
    }
    f(n, &d.last_section);
}

fn sections_mut(d: &mut Document, f: &mut dyn FnMut(usize, &mut wordcraft_doc::SectionProps)) {
    let mut n = 0usize;
    for b in d.body.iter_mut() {
        if matches!(&**b, Block::Para(p) if p.section.is_some())
            && let Block::Para(p) = Arc::make_mut(b)
            && let Some(sp) = p.section.as_mut()
        {
            f(n, sp);
            n += 1;
        }
    }
    f(n, &mut d.last_section);
}

/// Each exercise control in document order (body, then the other stories).
fn exercises(d: &Document, f: &mut dyn FnMut(&ContentControl)) {
    fn walk(bl: &Blocks, f: &mut dyn FnMut(&ContentControl), depth: usize) {
        if depth > MAX_DEPTH {
            return;
        }
        for b in bl {
            match &**b {
                Block::Para(p) => {
                    for o in &p.objects {
                        if let InlineObject::ControlStart { control } = o
                            && control.tag == TAG_EXERCISE
                        {
                            f(control);
                        }
                    }
                }
                Block::Table(t) => {
                    t.controls.open.iter().filter(|c| c.tag == TAG_EXERCISE).for_each(&mut *f);
                    for c in t.rows.iter().flat_map(|r| &r.cells) {
                        walk(&c.blocks, f, depth + 1);
                    }
                }
            }
        }
    }
    walk(&d.body, f, 0);
    for part in d.parts.values() {
        walk(&part.blocks, f, 0);
    }
}

fn exercises_mut(d: &mut Document, f: &mut dyn FnMut(&mut ContentControl)) {
    fn walk(bl: &mut Blocks, f: &mut dyn FnMut(&mut ContentControl), depth: usize) {
        if depth > MAX_DEPTH {
            return;
        }
        for b in bl.iter_mut() {
            let has = match &**b {
                Block::Para(p) => p.objects.iter().any(|o| matches!(o, InlineObject::ControlStart { control } if control.tag == TAG_EXERCISE)),
                Block::Table(_) => true,
            };
            if !has {
                continue;
            }
            match Arc::make_mut(b) {
                Block::Para(p) => {
                    for o in p.objects.iter_mut() {
                        if let InlineObject::ControlStart { control } = o
                            && control.tag == TAG_EXERCISE
                        {
                            f(control);
                        }
                    }
                }
                Block::Table(t) => {
                    t.controls.open.iter_mut().filter(|c| c.tag == TAG_EXERCISE).for_each(&mut *f);
                    for c in t.rows.iter_mut().flat_map(|r| r.cells.iter_mut()) {
                        walk(&mut c.blocks, f, depth + 1);
                    }
                }
            }
        }
    }
    walk(&mut d.body, f, 0);
    for part in d.parts.values_mut() {
        walk(&mut part.blocks, f, 0);
    }
}

fn is_other_strix_tag(tag: &str) -> bool {
    tag.starts_with("strix:") && tag != TAG_EXERCISE && !tag.starts_with(TAG_PREFIX)
}

fn other_strix_controls(d: &Document, f: &mut dyn FnMut(&ContentControl)) {
    fn walk(bl: &Blocks, f: &mut dyn FnMut(&ContentControl), depth: usize) {
        if depth > MAX_DEPTH {
            return;
        }
        for b in bl {
            match &**b {
                Block::Para(p) => {
                    for o in &p.objects {
                        if let InlineObject::ControlStart { control } = o
                            && is_other_strix_tag(&control.tag)
                        {
                            f(control);
                        }
                    }
                }
                Block::Table(t) => {
                    t.controls.open.iter().filter(|c| is_other_strix_tag(&c.tag)).for_each(&mut *f);
                    for r in &t.rows {
                        r.controls.open.iter().filter(|c| is_other_strix_tag(&c.tag)).for_each(&mut *f);
                        for c in &r.cells {
                            c.controls.open.iter().filter(|ctl| is_other_strix_tag(&ctl.tag)).for_each(&mut *f);
                            walk(&c.blocks, f, depth + 1);
                        }
                    }
                }
            }
        }
    }
    walk(&d.body, f, 0);
    for part in d.parts.values() {
        walk(&part.blocks, f, 0);
    }
}

fn other_strix_controls_mut(d: &mut Document, f: &mut dyn FnMut(&mut ContentControl)) {
    fn walk(bl: &mut Blocks, f: &mut dyn FnMut(&mut ContentControl), depth: usize) {
        if depth > MAX_DEPTH {
            return;
        }
        for b in bl.iter_mut() {
            let has = match &**b {
                Block::Para(p) => p.objects.iter().any(|o| matches!(o, InlineObject::ControlStart { control } if is_other_strix_tag(&control.tag))),
                Block::Table(_) => true,
            };
            if !has {
                continue;
            }
            match Arc::make_mut(b) {
                Block::Para(p) => {
                    for o in p.objects.iter_mut() {
                        if let InlineObject::ControlStart { control } = o
                            && is_other_strix_tag(&control.tag)
                        {
                            f(control);
                        }
                    }
                }
                Block::Table(t) => {
                    t.controls.open.iter_mut().filter(|c| is_other_strix_tag(&c.tag)).for_each(&mut *f);
                    for r in &mut t.rows {
                        r.controls.open.iter_mut().filter(|c| is_other_strix_tag(&c.tag)).for_each(&mut *f);
                        for c in &mut r.cells {
                            c.controls.open.iter_mut().filter(|ctl| is_other_strix_tag(&ctl.tag)).for_each(&mut *f);
                            walk(&mut c.blocks, f, depth + 1);
                        }
                    }
                }
            }
        }
    }
    walk(&mut d.body, f, 0);
    for part in d.parts.values_mut() {
        walk(&mut part.blocks, f, 0);
    }
}

/// The custom XML part: the manifest as JSON in one element.
fn manifest_xml(m: &Manifest) -> Vec<u8> {
    let json = serde_json::to_string(m).unwrap_or_default();
    let escaped = json.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<objects xmlns=\"{NS}\" version=\"{VERSION}\"><data>{escaped}</data></objects>")
        .into_bytes()
}

// ---------------------------------------------------------------------------------------------
// Reading

/// Whether a custom XML part is Strix's.
fn is_ours(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(4096)];
    String::from_utf8_lossy(head).contains(NS)
}

/// The manifest in the custom XML part, leniently: the text of its `data` element (with or
/// without a prefix), unescaped.
fn parse_manifest(bytes: &[u8]) -> Option<Manifest> {
    let s = std::str::from_utf8(bytes).ok()?;
    let mut from = 0usize;
    let start = loop {
        let i = from + s.get(from..)?.find('<')?;
        let rest = s.get(i + 1..)?;
        let name_end = rest.find(|c: char| c == '>' || c == '/' || c.is_whitespace())?;
        let name = rest.get(..name_end)?;
        let local = name.rsplit(':').next().unwrap_or(name);
        if local == "data" {
            break i + 1 + rest.find('>')? + 1;
        }
        from = i + 1;
    };
    let end = start + s.get(start..)?.find("</")?;
    let text = unescape(s.get(start..end)?);
    let mut m: Manifest = serde_json::from_str(&text).ok()?;
    m.objects.truncate(MAX_ENTRIES);
    m.sections.truncate(MAX_ENTRIES);
    m.exercises.truncate(MAX_ENTRIES);
    m.controls.truncate(MAX_ENTRIES);
    Some(m)
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(rest.get(..i).unwrap_or(""));
        let after = rest.get(i..).unwrap_or("");
        let Some(semi) = after.find(';').filter(|&k| k <= 12) else {
            out.push('&');
            rest = after.get(1..).unwrap_or("");
            continue;
        };
        let ent = after.get(1..semi).unwrap_or("");
        let c = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => ent
                .strip_prefix("#x")
                .or_else(|| ent.strip_prefix("#X"))
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| ent.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match c {
            Some(c) => {
                out.push(c);
                rest = after.get(semi + 1..).unwrap_or("");
            }
            None => {
                out.push('&');
                rest = after.get(1..).unwrap_or("");
            }
        }
    }
    out.push_str(rest);
    out
}

/// After reading a Word package: take the Strix part out, put back what Word dropped, remove the
/// wrappers, and list the objects edited elsewhere under [`PENDING`] (see the module docs).
/// Returns how many objects are pending. A document with neither the part nor wrappers is left
/// untouched.
pub fn unwrap(doc: &mut Document) -> usize {
    let manifest = wordcraft_docx::strix_custom_xml::take(doc, &is_ours).map(|b| parse_manifest(&b).unwrap_or_default());
    if manifest.is_none() && !has_wrappers(doc) {
        return 0;
    }
    let m = manifest.unwrap_or_default();
    let media = doc.media.clone();
    let mut pending: Vec<Entry> = Vec::new();
    let mut ctx = Unwrap { entries: &m.objects, media: &media, pending: &mut pending, resolve: None, only_listed: false };
    for_stories(doc, &mut |bl| ctx.blocks(bl, 0));
    sections_mut(doc, &mut |n, sp| {
        if let Some(e) = m.sections.iter().find(|e| e.n == n) {
            if sp.graph_paper.is_none() {
                sp.graph_paper = e.graph.as_deref().and_then(GraphPaper::from_attr).map(Arc::new);
            }
            sp.frontispiece |= e.front;
        }
    });
    let mut n = 0usize;
    exercises_mut(doc, &mut |c| {
        if !c.extra.iter().any(|x| x.starts_with(EXERCISE_EXTRA))
            && let Some(e) = m.exercises.iter().find(|e| e.n == n)
        {
            c.extra.extend(e.extra.iter().cloned());
        }
        n += 1;
    });
    if !m.controls.is_empty() {
        let mut tag_counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
        other_strix_controls_mut(doc, &mut |c| {
            let idx = *tag_counts.entry(c.tag.clone()).and_modify(|k| *k += 1).or_insert(0);
            if !c.extra.iter().any(|x| x.starts_with(EXERCISE_EXTRA))
                && let Some(e) = m.controls.iter().find(|e| e.tag == c.tag && e.n == idx)
            {
                c.extra.extend(e.extra.iter().cloned());
            }
        });
    }
    for style in m.dsa_styles.iter().take(MAX_ENTRIES).filter(|st| st.id.starts_with("StrixDsa")) {
        if doc.styles.get(&style.id).is_none() {
            Arc::make_mut(&mut doc.styles).styles.push(style.clone());
        }
    }
    if let Some(calc) = &m.calc
        && !doc.passthrough.contains_key(crate::cmd::calc::DOC_CALC_OPTIONS)
    {
        crate::cmd::calc::set_doc_overrides(doc, calc);
    }
    let count = pending.len();
    if count > 0 {
        doc.passthrough.insert(PENDING.into(), Arc::new(serde_json::to_vec(&pending).unwrap_or_default()));
    } else {
        doc.passthrough.remove(PENDING);
    }
    count
}

/// The objects edited in another program, still to decide.
pub fn pending(doc: &Document) -> Vec<Entry> {
    doc.passthrough.get(PENDING).and_then(|b| serde_json::from_slice(b).ok()).unwrap_or_default()
}

/// What to do with pending objects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolution {
    /// Make them live again from their Strix source (recalculated, redrawn): the change made
    /// elsewhere is replaced.
    Recalculate,
    /// Keep the change: they stay static, their Strix source is dropped.
    Keep,
}

/// Resolve the pending objects `ids` (all when `None`). Returns how many were resolved.
pub fn resolve(doc: &mut Document, ids: Option<&[u32]>, how: Resolution) -> usize {
    let all = pending(doc);
    let (chosen, rest): (Vec<Entry>, Vec<Entry>) = all.into_iter().partition(|e| ids.is_none_or(|ids| ids.contains(&e.id)));
    if chosen.is_empty() {
        return 0;
    }
    let media = doc.media.clone();
    let mut unused = Vec::new();
    let mut ctx = Unwrap { entries: &chosen, media: &media, pending: &mut unused, resolve: Some(how), only_listed: true };
    for_stories(doc, &mut |bl| ctx.blocks(bl, 0));
    if rest.is_empty() {
        doc.passthrough.remove(PENDING);
    } else {
        doc.passthrough.insert(PENDING.into(), Arc::new(serde_json::to_vec(&rest).unwrap_or_default()));
    }
    chosen.len()
}

fn has_wrappers(doc: &Document) -> bool {
    fn blocks(bl: &Blocks, depth: usize) -> bool {
        depth <= MAX_DEPTH
            && bl.iter().any(|b| match &**b {
                Block::Para(p) => p.objects.iter().any(|o| matches!(o, InlineObject::ControlStart { control } if wrapper_id(control).is_some())),
                Block::Table(t) => {
                    t.controls.open.iter().any(|c| wrapper_id(c).is_some())
                        || t.rows.iter().flat_map(|r| &r.cells).any(|c| blocks(&c.blocks, depth + 1))
                }
            })
    }
    blocks(&doc.body, 0) || doc.parts.values().any(|p| blocks(&p.blocks, 0))
}

/// Remove every `strix:obj:` wrapper, changing nothing else (what [`wrap`] starts from).
fn strip_wrappers(doc: &mut Document) {
    if !has_wrappers(doc) {
        return;
    }
    let media = doc.media.clone();
    let mut unused = Vec::new();
    let mut ctx = Unwrap { entries: &[], media: &media, pending: &mut unused, resolve: Some(Resolution::Keep), only_listed: false };
    for_stories(doc, &mut |bl| ctx.blocks(bl, 0));
}

/// One pass over the stories: each wrapper's object checked against its entry and made live
/// again, or kept wrapped and listed as pending; `resolve` set: the wrapped (pending) objects
/// decided that way.
struct Unwrap<'a> {
    entries: &'a [Entry],
    media: &'a std::collections::BTreeMap<String, Arc<Vec<u8>>>,
    pending: &'a mut Vec<Entry>,
    resolve: Option<Resolution>,
    /// Leave wrappers without an entry alone (resolving some pending objects, not all).
    only_listed: bool,
}

/// What to do with one wrapper.
enum Verdict {
    /// Put the source back and remove the wrapper.
    Restore,
    /// Remove the wrapper only (no entry, or the change is kept).
    Unwrap,
    /// Leave it wrapped: pending.
    Pending,
}

impl Unwrap<'_> {
    fn entry(&self, id: u32) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// The verdict for an entry whose content matches (`unchanged`) or not.
    fn verdict(&mut self, entry: Option<&Entry>, unchanged: bool) -> Verdict {
        let Some(entry) = entry else { return if self.only_listed { Verdict::Pending } else { Verdict::Unwrap } };
        match self.resolve {
            Some(Resolution::Recalculate) => Verdict::Restore,
            Some(Resolution::Keep) => Verdict::Unwrap,
            None if unchanged => Verdict::Restore,
            None => {
                self.pending.push(entry.clone());
                Verdict::Pending
            }
        }
    }

    fn blocks(&mut self, bl: &mut Blocks, depth: usize) {
        if depth > MAX_DEPTH {
            return;
        }
        for b in bl.iter_mut() {
            if !self.touches(b, depth) {
                continue;
            }
            match Arc::make_mut(b) {
                Block::Para(p) => self.para(p),
                Block::Table(t) => {
                    self.table(t);
                    for c in t.rows.iter_mut().flat_map(|r| r.cells.iter_mut()) {
                        self.blocks(&mut c.blocks, depth + 1);
                    }
                }
            }
        }
    }

    fn touches(&self, b: &Block, depth: usize) -> bool {
        if depth > MAX_DEPTH {
            return false;
        }
        match b {
            Block::Para(p) => p.objects.iter().any(|o| matches!(o, InlineObject::ControlStart { control } if wrapper_id(control).is_some())),
            Block::Table(t) => {
                t.controls.open.iter().any(|c| wrapper_id(c).is_some())
                    || t.rows.iter().flat_map(|r| &r.cells).flat_map(|c| &c.blocks).any(|b| self.touches(b, depth + 1))
            }
        }
    }

    fn table(&mut self, t: &mut wordcraft_doc::table::Table) {
        let Some(i) = t.controls.open.iter().position(|c| wrapper_id(c).is_some()) else { return };
        let id = t.controls.open.get(i).and_then(wrapper_id).unwrap_or(0);
        let entry = self.entry(id).cloned();
        let unchanged = entry.as_ref().is_some_and(|e| e.fp == 0 || e.fp == table_fp(t));
        match self.verdict(entry.as_ref(), unchanged) {
            Verdict::Pending => return,
            Verdict::Restore => {
                if let Some(Entry { source: Source::Table { names, math, index, blanks, no_total }, .. }) = &entry {
                    t.props.calc = true;
                    t.props.calc_names = names.as_deref().and_then(wordcraft_doc::props::CellNames::parse);
                    t.props.calc_math = *math;
                    t.props.calc_index = *index;
                    for &c in no_total.iter().take(MAX_ENTRIES) {
                        if let Some(cell) = t.rows.first_mut().and_then(|row| row.cells.get_mut(c)) {
                            cell.props.calc_no_total = true;
                        }
                    }
                    for &(r, c) in blanks.iter().take(MAX_ENTRIES) {
                        if let Some(cell) = t.rows.get_mut(r).and_then(|row| row.cells.get_mut(c)) {
                            cell.props.student_blank = true;
                        }
                    }
                }
            }
            Verdict::Unwrap => {}
        }
        t.controls.open.remove(i);
        // The wrapper closed with the table (it was the innermost control opening here).
        t.controls.close = t.controls.close.saturating_sub(1);
    }

    fn para(&mut self, p: &mut Paragraph) {
        // Wrappers in this paragraph: (start offset, end offset or None, id). Inner ones and later
        // ones first, so offsets stay right as markers go.
        let mut stack: Vec<(usize, Option<u32>)> = Vec::new();
        let mut found: Vec<(usize, Option<usize>, u32)> = Vec::new();
        for (off, o) in p.objects_with_offsets() {
            match o {
                InlineObject::ControlStart { control } => stack.push((off, wrapper_id(control))),
                InlineObject::ControlEnd => {
                    if let Some((start, Some(id))) = stack.pop() {
                        found.push((start, Some(off), id));
                    }
                }
                _ => {}
            }
        }
        // A wrapper Word left without its end in this paragraph: its start goes.
        found.extend(stack.into_iter().filter_map(|(start, id)| id.map(|id| (start, None, id))));
        found.sort_by_key(|f| std::cmp::Reverse(f.0));
        for (start, end, id) in found {
            self.one(p, start, end, id);
        }
    }

    fn one(&mut self, p: &mut Paragraph, start: usize, end: Option<usize>, id: u32) {
        let entry = self.entry(id).cloned();
        let inner: Vec<usize> = match end {
            Some(end) => p.object_offsets().into_iter().filter(|&o| o > start && o < end).collect(),
            None => Vec::new(),
        };
        let unchanged = match &entry {
            None => true,
            Some(e) => self.unchanged(p, e, &inner),
        };
        match self.verdict(entry.as_ref(), unchanged) {
            Verdict::Pending => return,
            Verdict::Restore => {
                if let Some(e) = &entry {
                    restore(p, e, &inner);
                }
            }
            Verdict::Unwrap => {}
        }
        if let Some(end) = end {
            let _ = p.delete(end, end + OBJ.len_utf8());
        }
        let _ = p.delete(start, start + OBJ.len_utf8());
    }

    /// Whether the content of a wrapper still shows what its entry recorded.
    fn unchanged(&self, p: &Paragraph, e: &Entry, inner: &[usize]) -> bool {
        // The objects inside other than control markers.
        let objects: Vec<&InlineObject> = inner
            .iter()
            .filter_map(|&o| p.object_at(o))
            .filter(|o| !matches!(o, InlineObject::ControlStart { .. } | InlineObject::ControlEnd))
            .collect();
        match &e.source {
            Source::Equation { .. } => match objects.as_slice() {
                [InlineObject::Equation { linear, math, .. }] => e.fp == 0 || text_fp(&equation_text(linear, math)) == e.fp,
                _ => false,
            },
            Source::Cad { json, svg, w, h } => match objects.as_slice() {
                // Word kept the drawing as Strix wrote it.
                [InlineObject::Graphic { graphic, .. }] if graphic.kind == GraphicKind::Cad => graphic.cad.as_deref() == Some(json.as_str()),
                [InlineObject::Image { media, w: iw, h: ih, crop, .. }] => {
                    let bytes = self.media.get(media).map(|b| b.as_slice()).unwrap_or(&[]);
                    let looks_svg = String::from_utf8_lossy(&bytes[..bytes.len().min(256)]).contains("<svg");
                    if looks_svg {
                        // Our SVG: the same bytes.
                        *svg == 0 || hash(bytes) == *svg
                    } else {
                        // A picture Word made from it (its own rendering of the SVG): untouched as
                        // long as it isn't cropped and keeps its proportions.
                        let ratio = |a: f32, b: f32| if b > 0.0 { a / b } else { 0.0 };
                        crop.iter().all(|c| c.abs() < 1e-4) && (ratio(*iw, *ih) - ratio(*w, *h)).abs() < 0.02
                    }
                }
                _ => false,
            },
            Source::Plot { json, svg, w, h } => match objects.as_slice() {
                [InlineObject::Graphic { graphic, .. }] if graphic.kind == GraphicKind::Plot => graphic.plot.as_deref() == Some(json.as_str()),
                [InlineObject::Image { media, w: iw, h: ih, crop, .. }] => {
                    let bytes = self.media.get(media).map(|b| b.as_slice()).unwrap_or(&[]);
                    let looks_svg = String::from_utf8_lossy(&bytes[..bytes.len().min(256)]).contains("<svg");
                    if looks_svg {
                        *svg == 0 || hash(bytes) == *svg
                    } else {
                        let ratio = |a: f32, b: f32| if b > 0.0 { a / b } else { 0.0 };
                        crop.iter().all(|c| c.abs() < 1e-4) && (ratio(*iw, *ih) - ratio(*w, *h)).abs() < 0.02
                    }
                }
                _ => false,
            },
            // Graph paper shows nothing to edit.
            Source::GraphParagraph { .. } => true,
            Source::Table { .. } => true,
        }
    }
}

/// Put an entry's source back on the objects inside its wrapper.
fn restore(p: &mut Paragraph, e: &Entry, inner: &[usize]) {
    match &e.source {
        Source::Equation { calc, answer, stepped, opts } => {
            for &o in inner {
                if let Some(InlineObject::Equation { math, .. }) = p.object_at_mut(o) {
                    math.calc = *calc;
                    math.answer = *answer;
                    math.stepped = *stepped;
                    math.calc_opts.clone_from(opts);
                }
            }
        }
        Source::Cad { json, .. } => {
            for &o in inner {
                let Some(obj) = p.object_at_mut(o) else { continue };
                let frame = match &*obj {
                    InlineObject::Image { w, h, alt, float, .. } | InlineObject::Graphic { w, h, alt, float, .. } => {
                        Some((*w, *h, alt.clone(), float.clone()))
                    }
                    _ => None,
                };
                if let Some((w, h, alt, float)) = frame {
                    let graphic = Arc::new(Graphic { kind: GraphicKind::Cad, w, h, cad: Some(json.as_str().into()), ..Default::default() });
                    *obj = InlineObject::Graphic { w, h, alt, float, graphic };
                }
            }
        }
        Source::Plot { json, .. } => {
            for &o in inner {
                let Some(obj) = p.object_at_mut(o) else { continue };
                let frame = match &*obj {
                    InlineObject::Image { w, h, alt, float, .. } | InlineObject::Graphic { w, h, alt, float, .. } => {
                        Some((*w, *h, alt.clone(), float.clone()))
                    }
                    _ => None,
                };
                if let Some((w, h, alt, float)) = frame {
                    let graphic = Arc::new(Graphic { kind: GraphicKind::Plot, w, h, plot: Some(json.as_str().into()), ..Default::default() });
                    *obj = InlineObject::Graphic { w, h, alt, float, graphic };
                }
            }
        }
        Source::GraphParagraph { graph } => {
            if p.props.graph.is_none() {
                p.props.graph = GraphPaper::from_attr(graph).map(Arc::new);
            }
        }
        Source::Table { .. } => {}
    }
    p.touch();
}
