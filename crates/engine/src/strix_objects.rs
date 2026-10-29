//! Strix objects: the parts of a document whose live behaviour only Strix's own formats
//! (`.xmath`, `.xmathm`) keep. Word and the other editable formats (DOCX, ODT, RTF, plain text,
//! Word XML) would keep at most their last look: a calculation becomes a plain equation, a
//! calculated table a plain table, an exercise loses its locked structure and results summary, a
//! CAD drawing its parametric source.
//!
//! Like the reference word processor, Strix saves a document back to the editable format it was
//! opened from, and Save As offers those formats, **as long as the document has no Strix
//! objects** ([`strix_objects`]). With any, a save to one of them is refused ([`check_save`]) with
//! the objects named and `.xmath` proposed: never a silently lossy file. Static exports (PDF, PNG,
//! LaTeX, HTML, Markdown) stay allowed: they are copies, not saves.

use std::path::Path;

use serde::Serialize;
use strix_doc::{Block, Document, InlineObject};

use crate::cmd::exercise::{TAG_EXERCISE, TAG_RESULTS};

/// How deep nested tables and groups are walked (hostile files).
const MAX_DEPTH: usize = 16;

/// A kind of Strix object, in the order they are listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StrixObjectKind {
    /// A calculated equation (a calculation region, `Math::calc`).
    Calculation,
    /// A calculated table (`TableProps::calc`).
    CalculatedTable,
    /// An exercise (its locked structure, data, variants and sandbox).
    Exercise,
    /// The exercises' results summary.
    ResultsSummary,
    /// A CAD drawing (its parametric source).
    CadDrawing,
    /// Graph paper behind a section's pages or in the text flow.
    GraphPaper,
}

impl StrixObjectKind {
    /// Every kind, in listing order.
    pub const ALL: [StrixObjectKind; 6] = [
        StrixObjectKind::Calculation,
        StrixObjectKind::CalculatedTable,
        StrixObjectKind::Exercise,
        StrixObjectKind::ResultsSummary,
        StrixObjectKind::CadDrawing,
        StrixObjectKind::GraphPaper,
    ];

    /// A stable id for scripts (`calculation`, `cadDrawing`…).
    pub fn id(self) -> &'static str {
        match self {
            StrixObjectKind::Calculation => "calculation",
            StrixObjectKind::CalculatedTable => "calculatedTable",
            StrixObjectKind::Exercise => "exercise",
            StrixObjectKind::ResultsSummary => "resultsSummary",
            StrixObjectKind::CadDrawing => "cadDrawing",
            StrixObjectKind::GraphPaper => "graphPaper",
        }
    }

    /// The English noun for `count` of them (`1 calculation`, `3 calculations`).
    pub fn noun(self, count: usize) -> &'static str {
        let one = count == 1;
        match self {
            StrixObjectKind::Calculation if one => "calculation",
            StrixObjectKind::Calculation => "calculations",
            StrixObjectKind::CalculatedTable if one => "calculated table",
            StrixObjectKind::CalculatedTable => "calculated tables",
            StrixObjectKind::Exercise if one => "exercise",
            StrixObjectKind::Exercise => "exercises",
            StrixObjectKind::ResultsSummary if one => "results summary",
            StrixObjectKind::ResultsSummary => "results summaries",
            StrixObjectKind::CadDrawing if one => "CAD drawing",
            StrixObjectKind::CadDrawing => "CAD drawings",
            StrixObjectKind::GraphPaper => "graph paper",
        }
    }
}

/// How many objects of one kind a document has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct StrixObjectCount {
    pub kind: StrixObjectKind,
    pub count: usize,
}

/// The Strix objects in `doc` (body, headers and footers, notes, comments, text boxes, tables and
/// groups at any reasonable depth): one entry per kind found, in [`StrixObjectKind::ALL`]'s
/// order. Empty when the document has none, so it can be saved in any editable format.
pub fn strix_objects(doc: &Document) -> Vec<StrixObjectCount> {
    let mut counts = [0usize; StrixObjectKind::ALL.len()];
    let mut add = |kind: StrixObjectKind| {
        if let Some(c) = counts.get_mut(kind as usize) {
            *c = c.saturating_add(1);
        }
    };
    if doc.last_section.graph_paper.is_some() {
        add(StrixObjectKind::GraphPaper);
    }
    blocks(&doc.body, &mut add, 0);
    for part in doc.parts.values() {
        blocks(&part.blocks, &mut add, 0);
    }
    StrixObjectKind::ALL.iter().zip(counts).filter(|(_, n)| *n > 0).map(|(&kind, count)| StrixObjectCount { kind, count }).collect()
}

/// Whether `doc` has any Strix object ([`strix_objects`]).
pub fn has_strix_objects(doc: &Document) -> bool {
    !strix_objects(doc).is_empty()
}

fn blocks(bl: &strix_doc::Blocks, add: &mut dyn FnMut(StrixObjectKind), depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    for b in bl {
        match &**b {
            Block::Para(p) => {
                if p.props.graph.is_some() || p.section.as_ref().is_some_and(|s| s.graph_paper.is_some()) {
                    add(StrixObjectKind::GraphPaper);
                }
                for o in &p.objects {
                    object(o, add, depth);
                }
            }
            Block::Table(t) => {
                if t.props.calc {
                    add(StrixObjectKind::CalculatedTable);
                }
                let wraps = std::iter::once(&t.controls)
                    .chain(t.rows.iter().flat_map(|r| std::iter::once(&r.controls).chain(r.cells.iter().map(|c| &c.controls))));
                for w in wraps {
                    w.open.iter().for_each(|c| control(&c.tag, add));
                }
                for c in t.rows.iter().flat_map(|r| &r.cells) {
                    blocks(&c.blocks, add, depth + 1);
                }
            }
        }
    }
}

fn control(tag: &str, add: &mut dyn FnMut(StrixObjectKind)) {
    match tag {
        TAG_EXERCISE => add(StrixObjectKind::Exercise),
        TAG_RESULTS => add(StrixObjectKind::ResultsSummary),
        _ => {}
    }
}

fn object(o: &InlineObject, add: &mut dyn FnMut(StrixObjectKind), depth: usize) {
    match o {
        InlineObject::Equation { math, .. } if math.calc => add(StrixObjectKind::Calculation),
        InlineObject::Graphic { graphic, .. } if graphic.cad.is_some() => add(StrixObjectKind::CadDrawing),
        InlineObject::ControlStart { control: c } => control(&c.tag, add),
        InlineObject::Group { children, .. } if depth < MAX_DEPTH => {
            for c in children {
                object(&c.obj, add, depth + 1);
            }
        }
        _ => {}
    }
}

/// The objects as a short English list: `3 calculations, 1 CAD drawing`.
pub fn describe(objects: &[StrixObjectCount]) -> String {
    objects
        .iter()
        .map(|o| if o.kind == StrixObjectKind::GraphPaper { o.kind.noun(1).to_string() } else { format!("{} {}", o.count, o.kind.noun(o.count)) })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Whether a plain Save (no path) writes `doc` back to its file at `path`: Strix's own files
/// always, another editable format Strix writes (`.docx`, `.odt`…, [`crate::io::FOREIGN_SAVE_EXTS`])
/// only while the document has no Strix objects. Otherwise Save asks where (Save As, `.xmath`).
pub fn saves_in_place(path: &Path, doc: &Document) -> bool {
    let name = path.to_string_lossy();
    crate::io::is_native(&name) || (crate::io::is_foreign_save(&name) && !has_strix_objects(doc))
}

/// Whether a user-facing save may write `doc` to `ext_or_name` (a bare extension or a file
/// name): [`crate::io::check_writable`] for the format, then, for an editable format that isn't
/// Strix's own, no Strix objects. The error names the objects and proposes `.xmath`.
pub fn check_save(ext_or_name: &str, doc: &Document) -> Result<(), String> {
    crate::io::check_writable(ext_or_name)?;
    if !crate::io::is_foreign_save(ext_or_name) {
        return Ok(());
    }
    let objects = strix_objects(doc);
    if objects.is_empty() {
        return Ok(());
    }
    Err(lossy_save_error(ext_or_name, &objects))
}

/// The refusal for saving a document with Strix `objects` to `ext_or_name`.
pub fn lossy_save_error(ext_or_name: &str, objects: &[StrixObjectCount]) -> String {
    let file = ext_or_name.rsplit(['/', '\\']).next().unwrap_or(ext_or_name);
    let (shown, proposed) = if file.contains('.') {
        let p = crate::io::native_path(Path::new(ext_or_name));
        (file.to_string(), format!(" (file.saveAs with a path such as {:?})", p.to_string_lossy()))
    } else {
        (format!(".{}", file.to_ascii_lowercase()), String::new())
    };
    format!(
        "this document has Strix objects ({}) that {shown} would lose: their calculations, exercise structure and CAD sources stay live only in Strix's own format. Save it as a Strix document (.{}){proposed}; PDF, PNG, LaTeX, HTML and Markdown exports stay available as static copies",
        describe(objects),
        crate::io::NATIVE_DOC_EXT,
    )
}
