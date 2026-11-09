//! Master documents (Outlining › Master Document): a document that shows other documents — its
//! subdocuments — in place, each still linked to its own file.
//!
//! A subdocument is a block-level rich text content control around its content, tagged
//! `strix-subdoc path=<absolute path>` and titled with the file name. It can't be deleted by
//! accident (`w:lock` sdtLocked) but its content is edited like any other text. The master's own
//! file keeps a copy of every subdocument's content, so it still opens (and prints) when a file is
//! missing, and the tag round-trips through `.xmath` and `.docx` as an ordinary content control.
//!
//! - **Saving** the master (as its own file, not a static export) writes every subdocument edited
//!   in the master back to its file, in that file's format. One that is unchanged isn't touched.
//!   One changed both in the master and on disk since they last agreed is a conflict: the file is
//!   left alone, the master keeps its version (marked `conflict` in the tag, so reopening doesn't
//!   replace it), and `master.saveSubdocuments {"force": true}` or `master.refresh` settle it.
//! - **Opening** the master reloads every subdocument whose file changed since (files win), so
//!   edits made to a chapter on its own show up in the book.
//!
//! What "since they last agreed" means is kept per open document in [`SYNC`]: the hash of the
//! file's bytes and of the content the master showed when they were last loaded or saved.
//! A subdocument's content is saved as the master's styles, lists and notes give it, with the
//! file's own document properties kept. Subdocuments inside a subdocument are part of its content:
//! they are links again when that file is opened as a master itself.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::{Value, json};
use strix_doc::control::{ContentControl, ControlKind, ControlLock, ControlRange};
use strix_doc::edit::Fragment;
use strix_doc::para::InlineObject;
use strix_doc::{Block, Document, Paragraph, Pos, StoryRef};

use super::{delete_selection, sel_result};
use crate::{CmdError, CmdResult, CommandSpec, Selection, Session, p};

const LOC: &str = "Outlining › Master Document";
/// The tag of a subdocument's control starts with this word.
const TAG: &str = "strix-subdoc";
/// Largest subdocument file read.
const MAX_FILE: u64 = 512 << 20;
/// Most subdocuments one master links.
const MAX_SUBDOCS: usize = 500;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("master.insertSubdocument", "Insert Subdocument", LOC, insert)
            .params(r#"{"path"?: string (a document Strix opens: .xmath, .docx, .odt, .rtf…; without it the app asks)}"#),
        CommandSpec::new("master.createSubdocument", "Create Subdocument", LOC, create).params(
            r#"{"path"?: string (the new file; .xmath when it has no extension; without it the app asks), "overwrite"?: bool (replace an existing file; default false)}"#,
        ),
        CommandSpec::new("master.unlinkSubdocument", "Unlink Subdocument", LOC, unlink)
            .params(r#"{"path"?: string (which subdocument; default the one at the caret)}"#)
            .when(no_subdocuments),
        CommandSpec::new("master.refresh", "Reload Subdocuments", LOC, refresh)
            .params(r#"{"path"?: string (one subdocument; default all) } — replaces the content with the files', discarding edits made in the master"#)
            .when(no_subdocuments),
        CommandSpec::new("master.saveSubdocuments", "Save Subdocuments", LOC, save_now)
            .params(r#"{"force"?: bool (also overwrite files changed on disk since; default false)}"#)
            .when(no_subdocuments),
        CommandSpec::new("master.openSubdocument", "Open Subdocument", LOC, open_subdocument)
            .params(r#"{"path"?: string (default the subdocument at the caret)}"#)
            .when(no_subdocuments),
        CommandSpec::new("master.subdocuments", "Subdocuments", LOC, list).pure(),
        CommandSpec::new("view.subdocumentsPane", "Subdocuments Pane", LOC, |s, v| {
            s.view.subdocument_pane = p::bool(v, "value").unwrap_or(!s.view.subdocument_pane);
            Ok(json!({"value": s.view.subdocument_pane}))
        })
        .params(r#"{"value"?: bool}"#)
        .pure(),
    ]
}

fn no_subdocuments(s: &Session) -> Option<&'static str> {
    if subdocuments(&s.doc).is_empty() { Some("The document has no subdocuments.") } else { None }
}

// ---------------------------------------------------------------------------------------------
// Links

/// What a subdocument's tag says.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Link {
    path: PathBuf,
    /// Changed both in the master and in the file: the master's version waits to be settled.
    conflict: bool,
}

impl Link {
    fn parse(tag: &str) -> Option<Link> {
        let rest = tag.strip_prefix(TAG)?.strip_prefix(' ')?;
        let (conflict, rest) = match rest.strip_prefix("conflict ") {
            Some(r) => (true, r),
            None => (false, rest),
        };
        let path = rest.strip_prefix("path=")?;
        if path.is_empty() {
            return None;
        }
        Some(Link { path: PathBuf::from(path), conflict })
    }
    fn tag(&self) -> String {
        let c = if self.conflict { "conflict " } else { "" };
        format!("{TAG} {c}path={}", self.path.to_string_lossy())
    }
}

/// A subdocument of the body: its control and link.
#[derive(Clone, Debug)]
struct Sub {
    range: ControlRange,
    link: Link,
}

/// The body's subdocuments in document order (outermost ones only: one inside another is part of
/// its content).
fn subdocuments(doc: &Document) -> Vec<Sub> {
    let mut out: Vec<Sub> = doc
        .control_ranges(StoryRef::Body)
        .into_iter()
        .filter(|r| r.depth == 0 && r.control.block)
        .filter_map(|range| Link::parse(&range.control.tag).map(|link| Sub { range, link }))
        .collect();
    out.sort_by(|a, b| (&a.range.start.path, a.range.start.off).cmp(&(&b.range.start.path, b.range.start.off)));
    out.truncate(MAX_SUBDOCS);
    out
}

/// The subdocument `v`'s `path` names, else the one at the caret.
fn target(s: &Session, v: &Value) -> Result<Sub, CmdError> {
    let subs = subdocuments(&s.doc);
    if let Some(path) = p::str(v, "path") {
        let want = absolute(Path::new(path));
        return subs
            .into_iter()
            .find(|x| x.link.path == want || x.link.path == Path::new(path))
            .ok_or_else(|| CmdError::Params(format!("no subdocument links {path}")));
    }
    let at = &s.sel.focus;
    subs.into_iter()
        .find(|x| x.range.contains(at))
        .ok_or_else(|| CmdError::Failed("the caret isn't in a subdocument: put it in one, or name it with `path`".into()))
}

fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Where the file of `link` is: its path, or (when that is gone, as after moving the master's
/// folder) the file of the same name beside the master.
fn resolve(s: &Session, link: &Link) -> PathBuf {
    if !link.path.exists()
        && let (Some(dir), Some(name)) = (s.path.as_deref().and_then(Path::parent), link.path.file_name())
    {
        let beside = dir.join(name);
        if beside.exists() {
            return beside;
        }
    }
    link.path.clone()
}

// ---------------------------------------------------------------------------------------------
// When the master and the files last agreed

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Synced {
    /// Hash of the file's bytes.
    disk: u64,
    /// Hash of the content the master showed.
    content: u64,
}

/// By (document id, subdocument file).
static SYNC: Mutex<Option<HashMap<(u64, PathBuf), Synced>>> = Mutex::new(None);

fn synced(id: u64, path: &Path) -> Option<Synced> {
    let map = SYNC.lock().ok()?;
    map.as_ref()?.get(&(id, path.to_path_buf())).copied()
}

fn set_synced(id: u64, path: &Path, v: Synced) {
    if let Ok(mut map) = SYNC.lock() {
        map.get_or_insert_with(HashMap::new).insert((id, path.to_path_buf()), v);
    }
}

/// FNV-1a: stable across builds and platforms.
fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3))
}

fn content_hash(frag: &Fragment) -> u64 {
    serde_json::to_vec(&frag.blocks).map(|b| hash(&b)).unwrap_or(0)
}

/// A subdocument's content, without its control's markers.
fn content(doc: &Document, r: &ControlRange) -> Fragment {
    doc.copy_range(&r.content_start(), &r.end)
}

// ---------------------------------------------------------------------------------------------
// Reading and writing the files

/// The bytes of a subdocument file, at most `max` (files are read through the browser on the web).
#[cfg(not(target_arch = "wasm32"))]
fn read_capped(path: &str, max: u64, what: &str) -> Result<Vec<u8>, String> {
    crate::io::read_file_capped(path, max, what)
}

#[cfg(target_arch = "wasm32")]
fn read_capped(path: &str, _max: u64, _what: &str) -> Result<Vec<u8>, String> {
    Err(format!("{path}: subdocuments are not available on the web"))
}

/// A subdocument file's content, as the master shows it, and the hash of its bytes.
fn load(path: &Path) -> Result<(Fragment, u64), String> {
    let name = path.to_string_lossy();
    let bytes = read_capped(&name, MAX_FILE, "subdocument")?;
    let doc = crate::io::open_bytes(&name, &bytes)?;
    let mut frag = doc.body_fragment();
    super::paste::unlink_sections(&mut frag.blocks);
    Ok((frag, hash(&bytes)))
}

/// The file a subdocument with content `frag` saves as: the master with only that content, the
/// existing file's document properties kept.
fn subdocument_file(master: &Document, frag: &Fragment, old: Option<&Document>) -> Document {
    let mut d = master.clone();
    d.body = frag.blocks.iter().cloned().map(std::sync::Arc::new).collect();
    if let Some(old) = old {
        d.core = old.core.clone();
        d.custom_props = old.custom_props.clone();
    }
    // Only the comments and text boxes the content still has.
    let mut ids = std::collections::BTreeSet::new();
    for path in d.para_paths(StoryRef::Body) {
        if let Some(para) = d.para(StoryRef::Body, &path) {
            ids.extend(para.objects.iter().filter_map(|o| match o {
                InlineObject::CommentStart { id } | InlineObject::CommentEnd { id } => Some(*id),
                _ => None,
            }));
        }
    }
    d.comments.retain(|id, _| ids.contains(id));
    d.prune_text_boxes();
    d
}

/// Write a subdocument's content to `path` in its format; the hash of the bytes written.
fn write(master: &Document, frag: &Fragment, path: &Path) -> Result<u64, String> {
    let name = path.to_string_lossy();
    crate::io::check_writable(&name)?;
    if crate::io::EXPORT_EXTS.iter().any(|e| name.to_ascii_lowercase().ends_with(&format!(".{e}"))) && !crate::io::is_native(&name) {
        return Err(format!("{name}: a subdocument is saved to a document format (.xmath, .docx, .odt, .rtf…), not an export"));
    }
    let old = if path.exists() { read_capped(&name, MAX_FILE, "subdocument").and_then(|b| crate::io::open_bytes(&name, &b)).ok() } else { None };
    let doc = subdocument_file(master, frag, old.as_ref());
    crate::strix_objects::check_save(&name, &doc)?;
    let bytes = crate::io::save_bytes(&name, &doc)?;
    crate::io::write_file(&name, &bytes)?;
    Ok(hash(&bytes))
}

// ---------------------------------------------------------------------------------------------
// Putting content in the master

/// The ids content controls already use.
fn new_control_id(doc: &Document) -> i64 {
    let mut max = 0_i64;
    strix_doc::control::walk_controls(&doc.body, &mut |c| max = max.max(c.id.unwrap_or(0)));
    max.saturating_add(1)
}

fn new_control(doc: &Document, link: &Link) -> ContentControl {
    let mut c = ContentControl::new(ControlKind::RichText);
    c.id = Some(new_control_id(doc));
    c.block = true;
    c.lock = ControlLock::SdtLocked;
    c.tag = link.tag();
    c.title = link.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    c.placeholder_text = String::new();
    c
}

/// The fragment's blocks, beginning and ending with a paragraph (the control's markers go in
/// paragraphs).
fn padded(mut frag: Fragment) -> Fragment {
    if !matches!(frag.blocks.first(), Some(Block::Para(_))) {
        frag.blocks.insert(0, Block::Para(Paragraph::new()));
    }
    if !matches!(frag.blocks.last(), Some(Block::Para(_))) {
        frag.blocks.push(Block::Para(Paragraph::new()));
    }
    frag
}

/// Put `frag` into the empty top-level body paragraph at `at`, inside a new subdocument control
/// for `link`; the caret goes to its start.
fn place(s: &mut Session, at: &Pos, frag: Fragment, link: &Link) -> Result<(), CmdError> {
    let frag = padded(frag);
    let first_props = frag.blocks.first().and_then(Block::as_para).map(|p| p.props.clone());
    let end = s.doc.insert_fragment(at, &frag)?;
    if let Some(props) = first_props {
        s.doc.para_mut(at.story, &at.path)?.props = props;
    }
    let end_off = s.doc.para(end.story, &end.path).map_or(0, Paragraph::len);
    let end = Pos::new(end.story, end.path.clone(), end_off);
    let end_props = s.doc.para(end.story, &end.path).map(|p| p.props_at(end_off).clone()).unwrap_or_default();
    s.doc.insert_object(&end, InlineObject::ControlEnd, &end_props)?;
    let start = Pos::new(at.story, at.path.clone(), 0);
    let start_props = s.doc.para(start.story, &start.path).map(|p| p.props_of_char(0).clone()).unwrap_or_default();
    let ctl = new_control(&s.doc, link);
    let inside = s.doc.insert_object(&start, InlineObject::ControlStart { control: Box::new(ctl) }, &start_props)?;
    s.sel = Selection::caret(inside);
    s.goal_x = None;
    Ok(())
}

/// An empty top-level body paragraph at the caret (after the caret's paragraph when it has text
/// before the caret), with a paragraph after it.
fn empty_paragraph_here(s: &mut Session) -> Result<Pos, CmdError> {
    let at = delete_selection(s)?;
    if at.story != StoryRef::Body || at.path.depth() != 0 {
        return Err(CmdError::Failed("a subdocument goes in the main text, outside tables and text boxes".into()));
    }
    if s.doc.control_at(&at).is_some() {
        return Err(CmdError::Failed("a subdocument can't go inside a content control or another subdocument".into()));
    }
    let len = s.doc.para_at(&at).map_or(0, Paragraph::len);
    let at = if at.off > 0 { s.doc.split_paragraph(&Pos { off: len, ..at })? } else { at };
    s.doc.split_paragraph(&at)?;
    Ok(Pos { off: 0, ..at })
}

fn check_link(s: &Session, path: &Path) -> Result<(), CmdError> {
    if s.path.as_deref().map(absolute).as_deref() == Some(path) {
        return Err(CmdError::Params("a document can't be its own subdocument".into()));
    }
    if subdocuments(&s.doc).iter().any(|x| x.link.path == path) {
        return Err(CmdError::Params(format!("{} is already a subdocument of this document", path.display())));
    }
    if subdocuments(&s.doc).len() >= MAX_SUBDOCS {
        return Err(CmdError::Failed(format!("a master document links at most {MAX_SUBDOCS} subdocuments")));
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Commands

fn insert(s: &mut Session, v: &Value) -> CmdResult {
    let Some(path) = p::str(v, "path") else {
        s.push_ui_request(json!({"open": "insertSubdocument"}));
        return sel_result(s);
    };
    let path = absolute(Path::new(path));
    check_link(s, &path)?;
    let (frag, disk) = load(&path).map_err(CmdError::Failed)?;
    let content = content_hash(&frag);
    let at = empty_paragraph_here(s)?;
    let link = Link { path: path.clone(), conflict: false };
    place(s, &at, frag, &link)?;
    // What the master shows after it went in, which is what later saves compare with.
    let shown = subdocuments(&s.doc).into_iter().find(|x| x.link == link).map_or(content, |x| content_hash(&self::content(&s.doc, &x.range)));
    set_synced(s.document_id(), &path, Synced { disk, content: shown });
    let mut r = sel_result(s)?;
    if let Some(o) = r.as_object_mut() {
        o.insert("subdocument".into(), json!(path));
    }
    Ok(r)
}

fn create(s: &mut Session, v: &Value) -> CmdResult {
    let Some(path) = p::str(v, "path") else {
        s.push_ui_request(json!({"open": "createSubdocument"}));
        return sel_result(s);
    };
    let path = absolute(&super::file::with_native_default(path));
    check_link(s, &path)?;
    if path.exists() && p::bool(v, "overwrite") != Some(true) {
        return Err(CmdError::Failed(format!(
            "{} already exists: pass \"overwrite\": true to replace it, or use master.insertSubdocument to link it",
            path.display()
        )));
    }
    let (a, b) = s.sel.ordered();
    if a.story != StoryRef::Body || a.path.depth() != 0 || b.path.depth() != 0 {
        return Err(CmdError::Failed("select whole paragraphs of the main text to make a subdocument of".into()));
    }
    if s.doc.control_at(&a).is_some() || s.doc.control_at(&b).is_some() {
        return Err(CmdError::Failed("a subdocument can't be made inside a content control or another subdocument".into()));
    }
    // Whole paragraphs, from the first one's start to the last one's end.
    let start = Pos::new(StoryRef::Body, a.path.clone(), 0);
    let last_len = s.doc.para(StoryRef::Body, &b.path).map_or(0, Paragraph::len);
    let end = Pos::new(StoryRef::Body, b.path.clone(), last_len);
    let frag = s.doc.copy_range(&start, &end);
    let disk = write(&s.doc, &frag, &path).map_err(CmdError::Failed)?;
    s.sel = Selection { anchor: start.clone(), focus: end };
    let at = delete_selection(s)?;
    let at = Pos { off: 0, ..at };
    let link = Link { path: path.clone(), conflict: false };
    place(s, &at, frag, &link)?;
    if let Some(x) = subdocuments(&s.doc).into_iter().find(|x| x.link == link) {
        set_synced(s.document_id(), &path, Synced { disk, content: content_hash(&content(&s.doc, &x.range)) });
    }
    let mut r = sel_result(s)?;
    if let Some(o) = r.as_object_mut() {
        o.insert("subdocument".into(), json!(path));
    }
    Ok(r)
}

/// Remove `sub`'s control markers, keeping its content.
fn remove_markers(s: &mut Session, sub: &Sub) -> Result<(), CmdError> {
    let w = strix_doc::para::OBJ.len_utf8();
    let end = &sub.range.end;
    s.doc.para_mut(end.story, &end.path)?.delete(end.off, end.off + w)?;
    let start = &sub.range.start;
    s.doc.para_mut(start.story, &start.path)?.delete(start.off, start.off + w)?;
    Ok(())
}

fn unlink(s: &mut Session, v: &Value) -> CmdResult {
    let sub = target(s, v)?;
    remove_markers(s, &sub)?;
    s.sel = Selection::caret(s.doc.clamp(&sub.range.start));
    let mut r = sel_result(s)?;
    if let Some(o) = r.as_object_mut() {
        o.insert("unlinked".into(), json!(sub.link.path));
    }
    Ok(r)
}

/// Replace `sub`'s content with `frag`, keeping its control (conflict mark cleared).
fn replace(s: &mut Session, sub: &Sub, frag: Fragment) -> Result<(), CmdError> {
    let link = Link { conflict: false, ..sub.link.clone() };
    remove_markers(s, sub)?;
    let start = sub.range.start.clone();
    // Without markers the content ends one marker earlier when it shares the start's paragraph.
    let w = strix_doc::para::OBJ.len_utf8();
    let end_off = if sub.range.end.path == start.path { sub.range.end.off.saturating_sub(w) } else { sub.range.end.off };
    let end = Pos { off: end_off, ..sub.range.end.clone() };
    let at = s.doc.delete_range(&start, &end)?;
    // The content's paragraphs are gone; what is left at `at` is the text before and after it
    // in the same paragraphs (none for a subdocument made here).
    let at = if at.off > 0 { s.doc.split_paragraph(&at)? } else { at };
    if s.doc.para_at(&at).is_some_and(|p| !p.is_empty()) {
        s.doc.split_paragraph(&at)?;
    }
    place(s, &at, frag, &link)
}

/// Reload `sub` from its file; `Ok(false)` when the file can't be read (the master keeps its copy).
fn reload(s: &mut Session, sub: &Sub) -> Result<Result<(), String>, CmdError> {
    let file = resolve(s, &sub.link);
    let (frag, disk) = match load(&file) {
        Ok(x) => x,
        Err(e) => return Ok(Err(e)),
    };
    let caret = s.sel.clone();
    replace(s, sub, frag)?;
    if let Some(x) = subdocuments(&s.doc).into_iter().find(|x| x.link.path == sub.link.path) {
        set_synced(s.document_id(), &sub.link.path, Synced { disk, content: content_hash(&content(&s.doc, &x.range)) });
    }
    s.sel = caret;
    s.clamp_selection();
    Ok(Ok(()))
}

fn refresh(s: &mut Session, v: &Value) -> CmdResult {
    let only = p::str(v, "path").map(|_| target(s, v)).transpose()?.map(|x| x.link.path);
    let (mut reloaded, mut failed) = (Vec::new(), Vec::new());
    let paths: Vec<PathBuf> = subdocuments(&s.doc).into_iter().map(|x| x.link.path).filter(|p| only.as_ref().is_none_or(|o| o == p)).collect();
    for path in paths {
        // Positions move as content is replaced: find it again each time.
        let Some(sub) = subdocuments(&s.doc).into_iter().find(|x| x.link.path == path) else { continue };
        match reload(s, &sub)? {
            Ok(()) => reloaded.push(json!(path)),
            Err(e) => failed.push(json!({"path": path, "error": e})),
        }
    }
    Ok(json!({"reloaded": reloaded, "failed": failed}))
}

/// Write the subdocuments edited in the master back to their files. With `force`, also those
/// changed on disk since; without it those are conflicts, marked in their tags when `mark`.
fn write_back(s: &mut Session, force: bool, mark: bool) -> Value {
    let id = s.document_id();
    let (mut saved, mut unchanged, mut conflicts, mut failed) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for sub in subdocuments(&s.doc) {
        let file = resolve(s, &sub.link);
        let frag = content(&s.doc, &sub.range);
        let now = content_hash(&frag);
        let disk_now = read_capped(&file.to_string_lossy(), MAX_FILE, "subdocument").ok().map(|b| hash(&b));
        let last = synced(id, &sub.link.path);
        let edited = last.is_none_or(|l| l.content != now) || sub.link.conflict;
        let changed_on_disk = match (last, disk_now) {
            (Some(l), Some(d)) => l.disk != d,
            (None, Some(_)) => sub.link.conflict,
            (_, None) => false,
        };
        if !edited && !force {
            unchanged.push(json!(sub.link.path));
            continue;
        }
        if changed_on_disk && !force {
            conflicts.push(json!(sub.link.path));
            if mark && !sub.link.conflict {
                set_tag(s, &sub, Link { conflict: true, ..sub.link.clone() });
            }
            continue;
        }
        match write(&s.doc, &frag, &file) {
            Ok(disk) => {
                set_synced(id, &sub.link.path, Synced { disk, content: now });
                if sub.link.conflict {
                    set_tag(s, &sub, Link { conflict: false, ..sub.link.clone() });
                }
                saved.push(json!(file));
            }
            Err(e) => failed.push(json!({"path": file, "error": e})),
        }
    }
    json!({"saved": saved, "unchanged": unchanged, "conflicts": conflicts, "failed": failed})
}

fn set_tag(s: &mut Session, sub: &Sub, link: Link) {
    if let Some(c) = s.doc.control_mut(&sub.range.start) {
        c.tag = link.tag();
    }
}

fn save_now(s: &mut Session, v: &Value) -> CmdResult {
    Ok(write_back(s, p::bool(v, "force") == Some(true), true))
}

fn open_subdocument(s: &mut Session, v: &Value) -> CmdResult {
    let sub = target(s, v)?;
    let file = resolve(s, &sub.link);
    s.run("file.open", &json!({"path": file.to_string_lossy()}))
}

fn list(s: &mut Session, _: &Value) -> CmdResult {
    let id = s.document_id();
    let subs: Vec<Value> = subdocuments(&s.doc)
        .iter()
        .map(|x| {
            let file = resolve(s, &x.link);
            let now = content_hash(&content(&s.doc, &x.range));
            json!({
                "path": x.link.path,
                "file": file,
                "exists": file.exists(),
                "edited": synced(id, &x.link.path).is_none_or(|l| l.content != now),
                "conflict": x.link.conflict,
                "start": super::pos_json(&x.range.start),
                "text": s.doc.control_text(&x.range).split('\n').map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" · ").chars().take(120).collect::<String>(),
            })
        })
        .collect();
    Ok(json!({"subdocuments": subs}))
}

// ---------------------------------------------------------------------------------------------
// Hooks for file.save and file.open

/// Before the master is saved to `path`: write its edited subdocuments back when `path` becomes
/// the master's own file (not a static export). `None` when it has none.
pub(crate) fn before_save(s: &mut Session, path: &Path) -> Option<Value> {
    let name = path.to_string_lossy();
    if !(crate::io::is_native(&name) || crate::io::is_foreign_save(&name)) || subdocuments(&s.doc).is_empty() {
        return None;
    }
    let r = write_back(s, false, true);
    let count = |k: &str| r.get(k).and_then(Value::as_array).map_or(0, Vec::len);
    if count("conflicts") > 0 {
        s.status = format!(
            "{} subdocument(s) changed on disk too and were not overwritten: reload them (master.refresh) or save them anyway (master.saveSubdocuments with force)",
            count("conflicts")
        );
    } else if count("failed") > 0 {
        s.status = format!("{} subdocument(s) could not be saved", count("failed"));
    }
    Some(r)
}

/// After a master is opened: reload the subdocuments whose files changed (files win), except the
/// ones holding a conflict. `None` when it has none.
pub(crate) fn after_open(s: &mut Session) -> Option<Value> {
    let subs = subdocuments(&s.doc);
    if subs.is_empty() {
        return None;
    }
    let id = s.document_id();
    let (mut reloaded, mut missing, mut conflicts) = (Vec::new(), Vec::new(), Vec::new());
    for path in subs.into_iter().map(|x| x.link.path) {
        let Some(sub) = subdocuments(&s.doc).into_iter().find(|x| x.link.path == path) else { continue };
        if sub.link.conflict {
            conflicts.push(json!(path));
            continue;
        }
        // The file is what counts: its content replaces the master's copy (the same content when
        // nothing changed on disk since the master was saved).
        let now = content_hash(&content(&s.doc, &sub.range));
        match reload(s, &sub) {
            Ok(Ok(())) => reloaded.push(json!(path)),
            Ok(Err(_)) | Err(_) => {
                // Missing: a later save writes the master's copy back (creating the file).
                set_synced(id, &path, Synced { disk: 0, content: now.wrapping_add(1) });
                missing.push(json!(path));
            }
        }
    }
    s.sel = Selection::caret(Pos::new(StoryRef::Body, strix_doc::Path::top(0), 0));
    s.clamp_selection();
    if !missing.is_empty() || !conflicts.is_empty() {
        s.status = format!("Subdocuments: {} not found (the master's copy is shown), {} with unsettled changes", missing.len(), conflicts.len());
    }
    Some(json!({"reloaded": reloaded, "missing": missing, "conflicts": conflicts}))
}

#[cfg(test)]
mod tests;
