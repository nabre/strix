//! Underlays and inherited layers and styles between the CAD drawings of a document
//! (docs/cad.md, Underlay): the drawings listed, another drawing taken as the selected one's
//! underlay (by its id, so later edits show), its layers and styles merged in.
//!
//! A drawing never takes itself, two drawings never take each other, and an underlay's own
//! underlay is never drawn (one level deep).

use std::sync::Arc;

use serde_json::{Value, json};
use strix_cad::{Drawing, Underlay};
use wordcraft_doc::graphic::GraphicKind;
use wordcraft_doc::{Block, InlineObject, Path, Pos, StoryRef};

use super::{drawing_to_change, selected_source, store};
use crate::{CmdError, CmdResult, Session};

/// Most drawings listed.
const MAX_LISTED: usize = 500;

/// A CAD drawing of the document's body: its number (1, 2, … in reading order), where it is, its
/// source and its alternative text.
pub struct CadEntry {
    pub index: usize,
    pub pos: Pos,
    pub source: Arc<str>,
    pub alt: String,
}

impl CadEntry {
    /// Its id, when it has one.
    fn id(&self) -> Option<String> {
        Drawing::from_json(&self.source).ok().and_then(|d| d.id)
    }

    /// Its name in a list: the name it was given ([`name`]), else its alternative text, else
    /// "Drawing n".
    pub fn name(&self) -> String {
        if let Some(id) = self.id().filter(|id| !is_auto_id(id)) {
            return id;
        }
        let alt = self.alt.trim();
        if alt.is_empty() { format!("Drawing {}", self.index) } else { alt.chars().take(80).collect() }
    }
}

/// The CAD drawings in the document's body paragraphs, in reading order.
pub fn entries(s: &Session) -> Vec<CadEntry> {
    let mut out = Vec::new();
    for (i, b) in s.doc.body.iter().enumerate() {
        let Block::Para(p) = &**b else { continue };
        for (off, o) in p.objects_with_offsets() {
            if out.len() >= MAX_LISTED {
                return out;
            }
            if let InlineObject::Graphic { graphic, alt, .. } = o
                && graphic.kind == GraphicKind::Cad
            {
                let pos = Pos { story: StoryRef::Body, path: Path::top(i), off };
                out.push(CadEntry { index: out.len() + 1, pos, source: graphic.cad.clone().unwrap_or_else(|| Arc::from("")), alt: alt.clone() });
            }
        }
    }
    out
}

/// The drawing a `source` parameter names: its number in [`entries`], or its id.
fn find<'a>(list: &'a [CadEntry], v: &Value) -> Result<&'a CadEntry, CmdError> {
    let found = match v {
        Value::Number(n) => n.as_u64().and_then(|n| list.iter().find(|e| e.index as u64 == n)),
        Value::String(id) => list.iter().find(|e| e.id().as_deref() == Some(id.as_str())),
        _ => None,
    };
    found.ok_or_else(|| CmdError::Params("source: the number of another drawing of the document (cad.drawings), or its id".into()))
}

/// Longest drawing name, characters.
const MAX_NAME: usize = 60;

/// Whether `id` is one [`free_id`] gave (`d1`, `d2`, …) rather than a name typed.
fn is_auto_id(id: &str) -> bool {
    id.strip_prefix('d').is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

/// `text` as a slug: lower case letters and digits, anything else one hyphen, none at the ends;
/// at most [`MAX_NAME`] characters.
pub fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars().flat_map(char::to_lowercase) {
        if out.chars().count() >= MAX_NAME {
            break;
        }
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

/// `cad.name`: names the selected drawing with a slug (its id, which underlays refer to: the
/// drawings that have it as their underlay follow the new name); `{}` reads it.
pub(super) fn name(s: &mut Session, v: &Value) -> CmdResult {
    let (pos, mut d) = drawing_to_change(s)?;
    let Some(typed) = v.get("name") else {
        return Ok(json!({"name": d.id.clone().filter(|id| !is_auto_id(id))}));
    };
    let typed = typed.as_str().ok_or_else(|| CmdError::Params("name: text, such as \"ground-floor\"".into()))?;
    let new = slug(typed);
    if new.is_empty() {
        return Err(CmdError::Params("name: give it at least a letter or a digit".into()));
    }
    if is_auto_id(&new) {
        return Err(CmdError::Params(format!("name: \"{new}\" is kept for drawings without a name")));
    }
    let old = d.id.clone();
    if old.as_deref() == Some(new.as_str()) {
        return Ok(json!({"name": new}));
    }
    let list = entries(s);
    if list.iter().any(|e| e.pos != pos && e.id().as_deref() == Some(new.as_str())) {
        return Err(CmdError::Params(format!("another drawing of the document is already called \"{new}\"")));
    }
    // The drawings that have this one as their underlay follow the new name.
    if let Some(old) = &old {
        for e in list.iter().filter(|e| e.pos != pos) {
            let Ok(mut other) = Drawing::from_json(&e.source) else { continue };
            if let Some(u) = other.underlay.as_mut().filter(|u| &u.source == old) {
                u.source = new.clone();
                store(s, &e.pos, &other)?;
            }
        }
    }
    d.id = Some(new.clone());
    store(s, &pos, &d)?;
    Ok(json!({"name": new}))
}

/// A new id no drawing of `list` has, nor `taken`.
fn free_id(list: &[CadEntry], taken: Option<&str>) -> String {
    let ids: Vec<String> = list.iter().filter_map(CadEntry::id).collect();
    (1..=MAX_LISTED + 2).map(|n| format!("d{n}")).find(|n| !ids.contains(n) && Some(n.as_str()) != taken).unwrap_or_else(|| "d0".into())
}

/// The source drawing to take: read, with an id (given and stored when it had none; `taken` is
/// the id the selected drawing has or will have).
fn source_with_id(s: &mut Session, src: &CadEntry, list: &[CadEntry], taken: Option<&str>) -> Result<Drawing, CmdError> {
    let mut d = Drawing::from_json(&src.source).map_err(|e| CmdError::Failed(format!("the drawing can't be read: {e}")))?;
    if d.id.is_none() {
        d.id = Some(free_id(list, taken));
        store(s, &src.pos, &d)?;
    }
    Ok(d)
}

/// `cad.drawings`: the document's drawings to pick an underlay from.
pub(super) fn drawings(s: &mut Session, _: &Value) -> CmdResult {
    let here = selected_source(s).map(|(p, _)| p);
    let list = entries(s);
    let layout = s.layout();
    let under =
        here.as_ref().and_then(|p| list.iter().find(|e| &e.pos == p)).and_then(|e| Drawing::from_json(&e.source).ok()).and_then(|d| d.underlay);
    let rows: Vec<Value> = list
        .iter()
        .map(|e| {
            let id = e.id();
            json!({
                "index": e.index,
                "id": id,
                "name": e.name(),
                "page": layout.page_of(&e.pos).map(|p| p + 1),
                "current": here.as_ref() == Some(&e.pos),
                "underlay": id.is_some() && under.as_ref().map(|u| &u.source) == id.as_ref(),
            })
        })
        .collect();
    Ok(Value::Array(rows))
}

/// `cad.underlay`: takes another drawing as the selected one's underlay, sets it, or removes it.
pub(super) fn underlay(s: &mut Session, v: &Value) -> CmdResult {
    let (pos, mut d) = drawing_to_change(s)?;
    let mut inherited = None;
    if v.get("remove").and_then(Value::as_bool) == Some(true) {
        d.underlay = None;
    } else if let Some(sv) = v.get("source") {
        let list = entries(s);
        let src = find(&list, sv)?;
        if src.pos == pos {
            return Err(CmdError::Params("a drawing can't be its own underlay".into()));
        }
        let mine = d.id.clone().unwrap_or_else(|| free_id(&list, None));
        let src_d = source_with_id(s, src, &list, Some(&mine))?;
        if src_d.underlay.as_ref().is_some_and(|u| u.source == mine) {
            return Err(CmdError::Params("that drawing has this one as its underlay: two drawings can't be each other's".into()));
        }
        let src_id = src_d.id.clone().unwrap_or_default();
        d.id = Some(mine);
        // The same source keeps where it was placed and how it looks.
        let kept = d.underlay.take().filter(|u| u.source == src_id).unwrap_or_default();
        d.underlay = Some(Underlay { source: src_id, ..kept });
        if v.get("inherit").and_then(Value::as_bool) != Some(false) {
            inherited = Some(d.inherit(&src_d));
        }
    }
    let edits = ["dx", "dy", "fade", "show", "print"].iter().any(|k| v.get(k).is_some());
    if edits {
        let u = d.underlay.as_mut().ok_or_else(|| CmdError::Params("no underlay: give source first".into()))?;
        let num =
            |k: &str| v.get(k).map(|x| x.as_f64().filter(|n| n.is_finite()).ok_or_else(|| CmdError::Params(format!("{k}: a number")))).transpose();
        let flag = |k: &str| v.get(k).map(|x| x.as_bool().ok_or_else(|| CmdError::Params(format!("{k}: true or false")))).transpose();
        if let Some(n) = num("dx")? {
            u.dx = n;
        }
        if let Some(n) = num("dy")? {
            u.dy = n;
        }
        if let Some(n) = num("fade")? {
            u.fade = n;
        }
        if let Some(b) = flag("show")? {
            u.show = b;
        }
        if let Some(b) = flag("print")? {
            u.print = b;
        }
    } else if v.get("remove").is_none() && v.get("source").is_none() {
        return Ok(json!({"underlay": d.underlay}));
    }
    d.sanitize();
    store(s, &pos, &d)?;
    Ok(json!({"underlay": d.underlay, "inherited": inherited}))
}

/// `cad.inherit`: merges another drawing's layers and styles into the selected one, no underlay.
pub(super) fn inherit(s: &mut Session, v: &Value) -> CmdResult {
    let (pos, mut d) = drawing_to_change(s)?;
    let list = entries(s);
    let src = find(&list, v.get("source").unwrap_or(&Value::Null))?;
    if src.pos == pos {
        return Err(CmdError::Params("a drawing can't inherit from itself".into()));
    }
    let src_d = Drawing::from_json(&src.source).map_err(|e| CmdError::Failed(format!("the drawing can't be read: {e}")))?;
    let got = d.inherit(&src_d);
    store(s, &pos, &d)?;
    Ok(json!({"inherited": got}))
}

/// Drawing `d`'s underlay as the editor shows it (in its unit and scale, moved, faded, locked;
/// its own underlay left out), worked out where the source sits; None when it has none or it is
/// hidden; an error (for the status bar) when the source is gone or unreadable.
pub fn underlay_view(s: &Session, d: &Drawing) -> Result<Option<Drawing>, String> {
    let Some(u) = d.underlay.as_ref().filter(|u| u.show) else { return Ok(None) };
    if d.id.as_ref() == Some(&u.source) {
        return Ok(None);
    }
    let list = entries(s);
    let src = list.iter().find(|e| e.id().as_deref() == Some(u.source.as_str())).ok_or("The underlay's drawing is no longer in the document")?;
    let src_d = match super::super::calc::resolved_cad(s, &src.source) {
        Some(r) => r.0.clone(),
        None => Drawing::from_json(&src.source).map_err(|e| format!("The underlay's drawing can't be read: {e}"))?,
    };
    Ok(Some(d.underlay_view(&src_d)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Selection;
    use wordcraft_doc::Document;

    /// Two drawings, a paragraph apart; the second selected.
    fn two() -> Session {
        let mut s = Session::new(Document::new());
        s.run("document.setText", &json!({"text": ""})).unwrap();
        let walls = json!({"layers": [{"name": "0", "color": [0, 0, 0], "width": 0.25, "dash": "solid", "visible": true},
            {"name": "Walls", "color": [200, 0, 0], "width": 0.5, "dash": "solid", "visible": true}],
            "textStyles": [{"name": "Notes", "height": 4}],
            "entities": [{"type": "line", "a": [0, 0], "b": [100, 0], "layer": "Walls"}]});
        s.run("insert.cad", &json!({"drawing": walls})).unwrap();
        s.sel = Selection::caret(s.sel.focus.clone());
        s.run("text.insert", &json!({"text": "\n"})).unwrap();
        s.run("insert.cad", &json!({})).unwrap();
        s
    }

    fn select(s: &mut Session, n: usize) {
        let pos = entries(s).into_iter().find(|e| e.index == n).map(|e| e.pos).unwrap();
        let end = Pos { off: pos.off + '\u{FFFC}'.len_utf8(), ..pos.clone() };
        s.sel = Selection { anchor: pos, focus: end };
    }

    fn current(s: &Session) -> Drawing {
        let (_, src) = selected_source(s).unwrap();
        Drawing::from_json(&src).unwrap()
    }

    #[test]
    fn a_name_is_a_slug_underlays_follow_it() {
        let mut s = two();
        s.run("cad.underlay", &json!({"source": 1})).unwrap();
        select(&mut s, 1);
        assert_eq!(s.run("cad.name", &json!({})).unwrap()["name"], Value::Null);
        assert_eq!(s.run("cad.name", &json!({"name": "  Pianta Piano Terra!"})).unwrap()["name"], json!("pianta-piano-terra"));
        assert_eq!(current(&s).id.as_deref(), Some("pianta-piano-terra"));
        let listed = s.run("cad.drawings", &json!({})).unwrap();
        assert_eq!(listed[0]["name"], json!("pianta-piano-terra"));
        // The second drawing's underlay follows the new name.
        select(&mut s, 2);
        assert_eq!(current(&s).underlay.map(|u| u.source).as_deref(), Some("pianta-piano-terra"));
        // Taken, empty, kept for unnamed drawings, not text: refused.
        assert!(s.run("cad.name", &json!({"name": "Pianta piano terra"})).is_err());
        assert!(s.run("cad.name", &json!({"name": "--- !"})).is_err());
        assert!(s.run("cad.name", &json!({"name": "d7"})).is_err());
        assert!(s.run("cad.name", &json!({"name": 3})).is_err());
        assert_eq!(s.run("cad.name", &json!({"name": "Sezione A-A"})).unwrap()["name"], json!("sezione-a-a"));
        assert_eq!(slug(&"x".repeat(500)).chars().count(), MAX_NAME);
    }

    #[test]
    fn an_underlay_is_another_drawing_by_id_with_its_layers_and_styles() {
        let mut s = two();
        let list = s.run("cad.drawings", &json!({})).unwrap();
        assert_eq!(list.as_array().map(Vec::len), Some(2), "{list}");
        assert_eq!(list[1]["current"], json!(true));
        let r = s.run("cad.underlay", &json!({"source": 1})).unwrap();
        assert_eq!(r["inherited"]["layers"], json!(1), "{r}");
        let d = current(&s);
        assert!(d.layers.iter().any(|l| l.name == "Walls") && d.text_styles.iter().any(|t| t.name == "Notes"));
        let src_id = entries(&s)[0].id().expect("the source got an id");
        assert_eq!(d.underlay.as_ref().map(|u| u.source.clone()), Some(src_id));
        assert_eq!(s.run("cad.drawings", &json!({})).unwrap()[0]["underlay"], json!(true));
        // The editor's view: locked, from the source as it is now.
        let view = underlay_view(&s, &d).unwrap().expect("a view");
        assert!(view.layers.iter().all(|l| l.locked) && view.entities.len() == 1);
        // Placed, faded, hidden, printed, removed; junk refused.
        let r = s.run("cad.underlay", &json!({"dx": 10, "dy": -5, "fade": 2, "print": true})).unwrap();
        assert_eq!((r["underlay"]["dx"].clone(), r["underlay"]["fade"].clone()), (json!(10.0), json!(0.95)));
        for junk in [json!({"dx": "x"}), json!({"show": 1}), json!({"source": 99}), json!({"source": [1]}), json!({"source": "nope"})] {
            assert!(s.run("cad.underlay", &junk).is_err(), "{junk}");
        }
        s.run("cad.underlay", &json!({"show": false})).unwrap();
        assert!(underlay_view(&s, &current(&s)).unwrap().is_none(), "hidden");
        s.run("cad.underlay", &json!({"remove": true})).unwrap();
        assert!(current(&s).underlay.is_none());
        assert!(s.run("cad.underlay", &json!({"fade": 0.5})).is_err(), "nothing to set without an underlay");
    }

    #[test]
    fn printed_underlays_grow_the_picture_and_never_loop() {
        let mut s = two();
        let width = |s: &Session, n: usize| {
            let e = entries(s).into_iter().find(|e| e.index == n).unwrap();
            match s.doc.para(e.pos.story, &e.pos.path).and_then(|p| p.object_at(e.pos.off).cloned()) {
                Some(InlineObject::Graphic { w, .. }) => w,
                _ => 0.0,
            }
        };
        let before = width(&s, 2);
        s.run("cad.underlay", &json!({"source": 1, "dx": -300, "print": true})).unwrap();
        assert!(width(&s, 2) > before + 100.0, "{} {before}", width(&s, 2));
        // Itself, and each other: refused.
        assert!(s.run("cad.underlay", &json!({"source": 2})).is_err());
        select(&mut s, 1);
        let r = s.run("cad.underlay", &json!({"source": 2}));
        assert!(r.is_err(), "{r:?}");
        // A source that is gone shows nothing and says so.
        select(&mut s, 2);
        let d = current(&s);
        select(&mut s, 1);
        s.run("edit.delete", &json!({})).ok();
        let gone = underlay_view(&s, &d);
        assert!(gone.is_err() || entries(&s).len() == 2, "{gone:?}");
    }

    #[test]
    fn inheriting_alone_and_new_drawings_from_another() {
        let mut s = two();
        let r = s.run("cad.inherit", &json!({"source": 1})).unwrap();
        assert_eq!(r["inherited"]["layers"], json!(1));
        assert!(current(&s).underlay.is_none());
        assert!(s.run("cad.inherit", &json!({"source": 2})).is_err(), "not from itself");
        assert!(s.run("cad.inherit", &json!({})).is_err());
        // A new drawing started from the first: an underlay with its layers.
        s.sel = Selection::caret(s.sel.focus.clone());
        s.run("text.insert", &json!({"text": "\n"})).unwrap();
        s.run("insert.cad", &json!({"underlay": 1})).unwrap();
        let d = current(&s);
        assert!(d.underlay.is_some() && d.layers.iter().any(|l| l.name == "Walls"));
        s.sel = Selection::caret(s.sel.focus.clone());
        s.run("insert.cad", &json!({"inherit": 1})).unwrap();
        let d = current(&s);
        assert!(d.underlay.is_none() && d.layers.iter().any(|l| l.name == "Walls"));
        for junk in [json!({"underlay": "zz"}), json!({"inherit": -1}), json!({"underlay": {"a": 1}})] {
            assert!(s.run("insert.cad", &junk).is_err(), "{junk}");
        }
    }
}
