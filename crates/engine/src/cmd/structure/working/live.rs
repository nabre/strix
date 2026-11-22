//! A written working kept up to date (`docs/structures.md`, Working): each working (the whole
//! one of `structure.working`, an exercise's part under its question) lies between the two ends
//! of a hidden bookmark named after the structure's group and the part, and a change to the
//! structure writes every working of its group again in place, in the words it was first
//! written in (kept in the structure, [`Structure::working_labels`]).

use std::cmp::Reverse;
use std::sync::Arc;

use serde_json::{Value, json};
use strix_structure::Structure;
use wordcraft_doc::para::Paragraph;
use wordcraft_doc::{Block, InlineObject, Pos, StoryRef, para_block};

use crate::{CmdError, Session};

/// Hidden bookmarks (a leading underscore) holding a structure's working.
const PREFIX: &str = "_StrixWorking_";

/// The labels a working's words come from (`structure.working`'s and an exercise's).
const KEYS: [&str; 29] = [
    "heading",
    "loads",
    "reactions",
    "signs",
    "hyperstatic",
    "together",
    "force1",
    "force2",
    "system0",
    "system",
    "congruence",
    "eiconst",
    "eaconst",
    "noaxial",
    "cramer",
    "forces",
    "conventions",
    "member",
    "axial",
    "hinge",
    "range",
    "stretch",
    "cut",
    "cutOne",
    "values",
    "valuesMax",
    "jump",
    "nozero",
    "unsolved",
];

/// What a bookmarked working holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::cmd::structure) enum Part {
    /// `structure.working`'s whole working.
    Whole,
    /// An exercise's parts, one per question.
    Reactions,
    Axial,
    Shear,
    Zeros,
    Maxima,
    Moments,
}

const PARTS: [Part; 7] = [Part::Whole, Part::Reactions, Part::Axial, Part::Shear, Part::Zeros, Part::Maxima, Part::Moments];

impl Part {
    fn tag(self) -> &'static str {
        match self {
            Part::Whole => "w",
            Part::Reactions => "r",
            Part::Axial => "n",
            Part::Shear => "t",
            Part::Zeros => "z",
            Part::Maxima => "x",
            Part::Moments => "m",
        }
    }
}

/// The bookmark of `group`'s working `part` (letters, digits and underscores, at most 40
/// characters, as Word names bookmarks).
pub(in crate::cmd::structure) fn name(group: &str, part: Part) -> String {
    let g: String = group.chars().filter(char::is_ascii_alphanumeric).take(20).collect();
    format!("{PREFIX}{g}_{}", part.tag())
}

/// Whether `group` has a written working in the body (whole, or an exercise's part).
pub(in crate::cmd::structure) fn any(s: &Session, group: &str) -> bool {
    if group.is_empty() {
        return false;
    }
    let names: Vec<String> = PARTS.iter().map(|&p| name(group, p)).collect();
    s.doc.body.iter().any(|b| match &**b {
        Block::Para(p) => p.objects.iter().any(|o| matches!(o, InlineObject::BookmarkStart { name } if names.contains(name))),
        _ => false,
    })
}

/// Keeps in `st` the working's words given in `v`'s `labels`.
pub(in crate::cmd::structure) fn keep_labels(st: &mut Structure, v: &Value) {
    let Some(labels) = v.get("labels").and_then(Value::as_object) else { return };
    for k in KEYS {
        if let Some(t) = labels.get(k).and_then(Value::as_str) {
            st.working_labels.insert(k.to_string(), t.to_string());
        }
    }
}

/// The params writing `st`'s working in the words it keeps.
fn params(st: &Structure) -> Value {
    json!({ "labels": st.working_labels })
}

/// Puts `paras` between the ends of bookmark `name` (nothing when there are none).
pub(in crate::cmd::structure) fn wrap(paras: &mut [Paragraph], name: &str) -> Result<(), CmdError> {
    if let Some(first) = paras.first_mut() {
        let props = first.mark.clone();
        first.insert_object(0, InlineObject::BookmarkStart { name: name.to_string() }, &props).map_err(|e| CmdError::Failed(e.to_string()))?;
    }
    if let Some(last) = paras.last_mut() {
        let (props, at) = (last.mark.clone(), last.text.len());
        last.insert_object(at, InlineObject::BookmarkEnd { name: name.to_string() }, &props).map_err(|e| CmdError::Failed(e.to_string()))?;
    }
    Ok(())
}

/// The body's paragraphs from the one where bookmark `name` starts to the one where it ends.
pub(in crate::cmd::structure) fn range(s: &Session, name: &str) -> Option<(usize, usize)> {
    let mut start = None;
    for (i, b) in s.doc.body.iter().enumerate() {
        let Block::Para(p) = &**b else { continue };
        for (_, o) in p.objects_with_offsets() {
            match o {
                InlineObject::BookmarkStart { name: n } if n == name && start.is_none() => start = Some(i),
                InlineObject::BookmarkEnd { name: n } if n == name => {
                    if let Some(a) = start {
                        return Some((a, i));
                    }
                }
                _ => {}
            }
        }
    }
    None
}

/// Writes `paras` in place of the body's paragraphs `a..=b` between the ends of bookmark
/// `name`; the selection and `pos` follow the blocks after them.
pub(in crate::cmd::structure) fn replace(
    s: &mut Session,
    (a, b): (usize, usize),
    mut paras: Vec<Paragraph>,
    name: &str,
    pos: &mut Pos,
) -> Result<(), CmdError> {
    if a > b || b >= s.doc.body.len() {
        return Err(CmdError::Failed("the working's place vanished".into()));
    }
    wrap(&mut paras, name)?;
    let (old, new) = (b - a + 1, paras.len());
    s.doc.body.splice(a..=b, paras.into_iter().map(para_block));
    let follow = |p: &mut Pos| {
        if p.story != StoryRef::Body {
            return;
        }
        let Some(top) = p.path.0.first().map(|&t| t as usize) else { return };
        if top > b {
            let moved = (top + new).saturating_sub(old);
            if let Some(t) = p.path.0.first_mut() {
                *t = u32::try_from(moved).unwrap_or(u32::MAX);
            }
        } else if top >= a {
            // Inside what was written again: at its start.
            p.path.0 = vec![u32::try_from(a).unwrap_or(u32::MAX)];
            p.off = 0;
        }
    };
    follow(&mut s.sel.anchor);
    follow(&mut s.sel.focus);
    follow(pos);
    Ok(())
}

/// Writes again every working of `st`'s group (the structure `src` at `pos`, just changed);
/// `pos` follows its block. Returns whether one was written.
pub(in crate::cmd::structure) fn refresh(s: &mut Session, src: &Arc<str>, st: &Structure, pos: &mut Pos) -> Result<bool, CmdError> {
    if st.group.is_empty() {
        return Ok(false);
    }
    let mut found: Vec<(Part, (usize, usize))> = PARTS.iter().filter_map(|&p| range(s, &name(&st.group, p)).map(|r| (p, r))).collect();
    if found.is_empty() {
        return Ok(false);
    }
    let v = params(st);
    let props = |s: &Session, i: usize| match s.doc.body.get(i).map(|b| &**b) {
        Some(Block::Para(p)) => p.mark.clone(),
        _ => wordcraft_doc::CharProps::default(),
    };
    let first = found.iter().map(|(_, (a, _))| *a).min().unwrap_or(0);
    let p0 = props(s, first);
    let whole = found.iter().any(|(p, _)| *p == Part::Whole).then(|| super::whole(s, src, &v, &p0));
    let mut parts = found.iter().any(|(p, _)| *p != Part::Whole).then(|| super::parts(s, src, &v, &p0));
    let unsolved = super::Labels::from(&v).unsolved;
    // The last first, so the earlier ones keep their places.
    found.sort_by_key(|(_, (a, _))| Reverse(*a));
    let mut exercise = false;
    for (part, r) in found {
        let props = props(s, r.0);
        let written: Result<Vec<Paragraph>, CmdError> = match part {
            Part::Whole => whole.clone().unwrap_or_else(|| Ok(Vec::new())),
            _ => match &mut parts {
                Some(Ok(p)) => Ok(std::mem::take(match part {
                    Part::Reactions => &mut p.reactions,
                    Part::Axial => &mut p.axial,
                    Part::Shear => &mut p.shear,
                    Part::Zeros => &mut p.zeros,
                    Part::Maxima => &mut p.maxima,
                    Part::Whole | Part::Moments => &mut p.moments,
                })),
                Some(Err(e)) => Err(e.clone()),
                None => Ok(Vec::new()),
            },
        };
        exercise |= part != Part::Whole;
        let mut paras = match written {
            Ok(paras) => paras,
            // A structure that can't be solved (while it's being changed): says so in the first
            // place, and is written again once it can.
            Err(_) if r.0 == first => vec![Paragraph::with_text(&unsolved, props.clone())],
            Err(_) => Vec::new(),
        };
        if paras.is_empty() {
            // An empty paragraph keeps the bookmark, so a working can come back there.
            paras.push(Paragraph::with_text("", props));
        }
        replace(s, r, paras, &name(&st.group, part), pos)?;
    }
    if exercise {
        crate::cmd::exercise::update(s);
    }
    s.clamp_selection();
    Ok(true)
}
