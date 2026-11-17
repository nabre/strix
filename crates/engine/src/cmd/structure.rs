//! Plane structures in the document (`docs/structures.md`): beams, frames and trusses with their
//! reactions and N, V (T), M diagrams.
//!
//! A `GraphicKind::Structure` graphic holds a [`strix_structure::Structure`] JSON source; it is
//! drawn as a plot is (`strix-plot` primitives), in reading order against the worksheet it sees
//! (the document's, or its exercise's variant sandbox), so lengths and loads may be calculation
//! parameters. Student copies (`exercise.show` off) show the structure and empty panels of the
//! same size; teacher copies the reactions and diagrams.

use std::sync::Arc;

use serde_json::{Value, json};
use strix_structure::{Action, Load, LoadAxes, LoadPer, Member, Node, Num, Status, Structure, Support, SupportKind, notables};
use wordcraft_doc::InlineObject;
use wordcraft_doc::graphic::{Graphic, GraphicKind};
use wordcraft_doc::para::Float;

use crate::{CmdError, CmdResult, CommandSpec, Selection, Session, p};

const PLACE: &str = "Structures › Structure";

pub fn specs() -> Vec<CommandSpec> {
    let selected = |s: &Session| if is_selected(s) { None } else { Some("select a structure first") };
    vec![
        CommandSpec::new("structure.insert", "Structure", PLACE, insert).params(
            r#"{"structure"?: object|string, "template"?: "beam"|"cantilever"|"continuous"|"frame"|"truss", "openPane"?: bool} → inserts a plane structure with its reactions and diagrams in line at the caret (a simply supported beam by default)"#,
        ),
        CommandSpec::new("structure.get", "Structure Source", PLACE, get).pure().when(selected).params(
            r#"→ {"structure": object, "resolved": object (its numbers' values), "status": "isostatic"|"hyperstatic"|"labile"|"invalid"|"empty", "degree"?: n, "message": string, "truss": bool, "reactions": [{"node", "kind", "fx", "fy", "m"}], "members": [{"id", "length", "n": [...], "v": [...], "m": [...]}], "errors": [...]} (notable values: {"x", "value", "kind": "end"|"extreme"|"zero"})"#,
        ),
        CommandSpec::new("structure.set", "Set Structure", PLACE, set).when(selected).params(
            r#"{"structure"?: object|string, "title"?: string, "width"?: mm, "lengthUnit"?: "m"|"cm"|"mm", "forceUnit"?: "kN"|"N"|"daN", "answer"?: bool, "convention"?: "italian", "diagrams"?: object (merged: {"n"|"v"|"m": {"show"?, "positive"?: "up"|"down", "amplitude"?}, "reactions"?, "values"?, "hatch"?, "signs"?}), "titles"?: {"n"?, "v"?, "m"?}, "layout"?: "auto"|"column"|"grid"} → updates the selected structure and redraws it"#,
        ),
        CommandSpec::new("structure.node", "Structure Node", PLACE, node).when(selected).params(
            r#"{"id"?: string, "x"?: num|expr, "y"?: num|expr, "hinge"?: bool, "rename"?: string, "remove"?: bool} → adds a node (a new name when id is absent or new), changes it, or removes it with what uses it"#,
        ),
        CommandSpec::new("structure.member", "Structure Member", PLACE, member).when(selected).params(
            r#"{"id"?: string, "a"?: node, "b"?: node, "hingeA"?: bool, "hingeB"?: bool, "e"?: num|expr|null, "i"?: num|expr|null, "area"?: num|expr|null, "remove"?: bool} → adds a member between nodes a and b, changes it, or removes it with its loads"#,
        ),
        CommandSpec::new("structure.support", "Structure Support", PLACE, support).when(selected).params(
            r#"{"node": string, "kind"?: "fixed"|"pin"|"roller"|"link"|"slider", "angle"?: num|expr, "remove"?: bool} → puts a support on a node (replacing its support), turns it, or removes it"#,
        ),
        CommandSpec::new("structure.load", "Structure Load", PLACE, load).when(selected).params(
            r#"{"index"?: n, "load"?: object ({"type": "force"|"moment"|"distributed", ...}), "remove"?: bool} → adds a load, replaces load `index`, or removes it"#,
        ),
    ]
}

/// Whether the selected inline object is a structure.
pub fn is_selected(s: &Session) -> bool {
    selected_source(s).is_some()
}

fn selected_source(s: &Session) -> Option<(wordcraft_doc::Pos, Arc<str>)> {
    let (pos, obj) = super::objects::selected(s)?;
    match obj {
        InlineObject::Graphic { graphic, .. } if graphic.kind == GraphicKind::Structure => {
            Some((pos, graphic.structure.clone().unwrap_or_else(|| Arc::from(""))))
        }
        _ => None,
    }
}

/// User-authored strings of the selected structure (title, names, expressions), so interface
/// localization audits treat them as document content.
pub fn selected_structure_texts(s: &Session) -> Vec<String> {
    let Some((_, src)) = selected_source(s) else { return Vec::new() };
    let Ok(st) = Structure::from_json(&src) else { return Vec::new() };
    let mut out = vec![st.title.clone(), st.titles.n.clone(), st.titles.v.clone(), st.titles.m.clone()];
    out.extend(st.nodes.iter().map(|n| n.id.clone()));
    out.extend(st.members.iter().map(|m| m.id.clone()));
    out.extend(st.expressions());
    out.into_iter().filter(|t| !t.is_empty()).collect()
}

fn n(x: f64, y: f64, id: &str) -> Node {
    Node { id: id.into(), x: x.into(), y: y.into(), hinge: false }
}

fn bar(id: &str, a: &str, b: &str) -> Member {
    Member { id: id.into(), a: a.into(), b: b.into(), ..Member::default() }
}

fn sup(node: &str, kind: SupportKind, angle: f64) -> Support {
    Support { node: node.into(), kind, angle: angle.into() }
}

fn uniform(member: &str, q: f64) -> Load {
    Load::Distributed {
        member: member.into(),
        q1: q.into(),
        q2: None,
        from: Num::default(),
        to: None,
        angle: Num::new(-90.0),
        axes: LoadAxes::Global,
        per: LoadPer::Length,
    }
}

fn force(node: &str, value: f64, angle: f64) -> Load {
    Load::Force { node: Some(node.into()), member: None, at: Num::default(), value: value.into(), angle: angle.into() }
}

/// A ready-made structure to start from.
fn template(name: &str) -> Result<Structure, CmdError> {
    let st = match name {
        "beam" => Structure {
            nodes: vec![n(0.0, 0.0, "A"), n(6.0, 0.0, "B")],
            members: vec![bar("1", "A", "B")],
            supports: vec![sup("A", SupportKind::Pin, 0.0), sup("B", SupportKind::Roller, 0.0)],
            loads: vec![uniform("1", 10.0)],
            ..Structure::default()
        },
        "cantilever" => Structure {
            nodes: vec![n(0.0, 0.0, "A"), n(3.0, 0.0, "B")],
            members: vec![bar("1", "A", "B")],
            supports: vec![sup("A", SupportKind::Fixed, -90.0)],
            loads: vec![force("B", 10.0, -90.0)],
            ..Structure::default()
        },
        "continuous" => Structure {
            nodes: vec![n(0.0, 0.0, "A"), n(5.0, 0.0, "B"), n(10.0, 0.0, "C")],
            members: vec![bar("1", "A", "B"), bar("2", "B", "C")],
            supports: vec![sup("A", SupportKind::Pin, 0.0), sup("B", SupportKind::Roller, 0.0), sup("C", SupportKind::Roller, 0.0)],
            loads: vec![uniform("1", 10.0), uniform("2", 10.0)],
            ..Structure::default()
        },
        "frame" => Structure {
            nodes: vec![n(0.0, 0.0, "A"), n(0.0, 4.0, "B"), n(6.0, 4.0, "C"), n(6.0, 0.0, "D")],
            members: vec![bar("1", "A", "B"), bar("2", "B", "C"), bar("3", "C", "D")],
            supports: vec![sup("A", SupportKind::Pin, 0.0), sup("D", SupportKind::Roller, 0.0)],
            loads: vec![uniform("2", 10.0), force("B", 10.0, 0.0)],
            ..Structure::default()
        },
        "truss" => {
            let mut st = Structure {
                nodes: vec![n(0.0, 0.0, "A"), n(3.0, 0.0, "B"), n(6.0, 0.0, "C"), n(1.5, 2.0, "D"), n(4.5, 2.0, "E")],
                members: vec![
                    bar("1", "A", "B"),
                    bar("2", "B", "C"),
                    bar("3", "A", "D"),
                    bar("4", "D", "B"),
                    bar("5", "B", "E"),
                    bar("6", "E", "C"),
                    bar("7", "D", "E"),
                ],
                supports: vec![sup("A", SupportKind::Pin, 0.0), sup("C", SupportKind::Roller, 0.0)],
                loads: vec![force("D", 10.0, -90.0), force("E", 10.0, -90.0)],
                ..Structure::default()
            };
            for node in &mut st.nodes {
                node.hinge = true;
            }
            st
        }
        other => return Err(CmdError::Params(format!("template: \"{other}\" is not beam, cantilever, continuous, frame or truss"))),
    };
    Ok(st)
}

fn structure_param(v: &Value) -> Result<Option<Structure>, CmdError> {
    match v.get("structure") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Structure::from_json(text).map(Some).map_err(CmdError::Params),
        Some(other) => Structure::from_json(&other.to_string()).map(Some).map_err(CmdError::Params),
    }
}

/// A number or an expression from the params.
fn num(v: &Value, key: &str) -> Result<Option<Num>, CmdError> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(x @ (Value::Number(_) | Value::String(_))) => {
            serde_json::from_value::<Num>(x.clone()).map(Some).map_err(|e| CmdError::Params(format!("{key}: {e}")))
        }
        Some(_) => Err(CmdError::Params(format!("{key}: a number or an expression"))),
    }
}

/// An optional number that `null` clears: `Some(None)` clears, `None` leaves it.
fn opt_num(v: &Value, key: &str) -> Result<Option<Option<Num>>, CmdError> {
    match v.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(_) => num(v, key).map(Some),
    }
}

/// Merges `patch` into `base` (objects key by key).
fn merge(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(b), Value::Object(p)) => {
            for (k, v) in p {
                merge(b.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (b, p) => *b = p.clone(),
    }
}

fn apply_settings(st: &mut Structure, v: &Value) -> Result<(), CmdError> {
    if let Some(t) = p::str(v, "title") {
        st.title = t.to_string();
    }
    if let Some(w) = v.get("width").and_then(Value::as_f64).filter(|w| w.is_finite()) {
        st.width = w;
    }
    if let Some(u) = p::str(v, "lengthUnit") {
        st.length_unit = strix_structure::LengthUnit::parse(u).ok_or_else(|| CmdError::Params(format!("lengthUnit: \"{u}\" is not m, cm or mm")))?;
    }
    if let Some(u) = p::str(v, "forceUnit") {
        st.force_unit = strix_structure::ForceUnit::parse(u).ok_or_else(|| CmdError::Params(format!("forceUnit: \"{u}\" is not kN, N or daN")))?;
    }
    if let Some(a) = p::bool(v, "answer") {
        st.answer = a;
    }
    if let Some(d) = v.get("diagrams").filter(|d| d.is_object()) {
        let mut cur = serde_json::to_value(st.diagrams).map_err(|e| CmdError::Failed(e.to_string()))?;
        merge(&mut cur, d);
        st.diagrams = serde_json::from_value(cur).map_err(|e| CmdError::Params(format!("diagrams: {e}")))?;
    }
    if let Some(t) = v.get("titles").filter(|t| t.is_object()) {
        let mut cur = serde_json::to_value(&st.titles).map_err(|e| CmdError::Failed(e.to_string()))?;
        merge(&mut cur, t);
        st.titles = serde_json::from_value(cur).map_err(|e| CmdError::Params(format!("titles: {e}")))?;
    }
    match p::str(v, "layout") {
        None => {}
        Some("auto") => st.layout = strix_structure::Layout::Auto,
        Some("column") => st.layout = strix_structure::Layout::Column,
        Some("grid") => st.layout = strix_structure::Layout::Grid,
        Some(other) => return Err(CmdError::Params(format!("layout: \"{other}\" is not auto, column or grid"))),
    }
    match p::str(v, "convention") {
        None => {}
        Some("italian") => st.diagrams = st.diagrams.italian(),
        Some(other) => return Err(CmdError::Params(format!("convention: \"{other}\" is not italian"))),
    }
    Ok(())
}

fn insert(s: &mut Session, v: &Value) -> CmdResult {
    let mut st = match structure_param(v)? {
        Some(st) => st,
        None => template(p::str(v, "template").unwrap_or("beam"))?,
    };
    apply_settings(&mut st, v)?;
    let st = st.sanitized();
    let rendered = st.render(None).plot;
    let (w, h, items) = super::plot::items_of(&rendered, &s.doc.settings.minor_font);
    let svg = Some(rendered.to_svg().into());
    let graphic = Graphic { kind: GraphicKind::Structure, items, w, h, structure: Some(st.to_json().into()), svg, ..Default::default() };
    let obj = InlineObject::Graphic { w, h, alt: String::new(), float: Float::default(), graphic: Arc::new(graphic) };
    let props = s.typing_props();
    let at = super::delete_selection(s)?;
    let end = s.doc.insert_object(&at, obj, &props)?;
    s.sel = Selection { anchor: at, focus: end };
    super::calc::recalculate_now(s);
    s.touch();
    if p::bool(v, "openPane").unwrap_or(false) {
        s.view.structure_pane = true;
    }
    super::sel_result(s)
}

fn notable_json(m: &strix_structure::MemberResult, a: Action) -> Value {
    notables(m, a)
        .into_iter()
        .map(|n| {
            let kind = match n.kind {
                strix_structure::NotableKind::End => "end",
                strix_structure::NotableKind::Extreme => "extreme",
                strix_structure::NotableKind::Zero => "zero",
            };
            json!({"x": n.x, "value": n.value, "kind": kind})
        })
        .collect()
}

fn get(s: &mut Session, _: &Value) -> CmdResult {
    let (_, src) = selected_source(s).ok_or_else(|| CmdError::Disabled("select a structure first".into()))?;
    let st = Structure::from_json(&src).map_err(|e| CmdError::Failed(format!("the structure can't be read: {e}")))?;
    let r = super::calc::resolved_structure(s, &src).unwrap_or_else(|| st.render(Some(&strix_calc::Worksheet::new(super::calc::options_now(s)))));
    let an = &r.analysis;
    let (status, degree) = match &an.status {
        Status::Empty => ("empty", None),
        Status::Invalid(_) => ("invalid", None),
        Status::Isostatic => ("isostatic", Some(0)),
        Status::Hyperstatic { degree } => ("hyperstatic", Some(*degree)),
        Status::Labile { degree, .. } => ("labile", Some(*degree)),
    };
    let reactions: Vec<Value> =
        an.reactions.iter().map(|r| json!({"node": r.node, "kind": r.kind.name(), "fx": r.fx, "fy": r.fy, "m": r.m})).collect();
    let members: Vec<Value> = an
        .members
        .iter()
        .map(|m| json!({"id": m.id, "length": m.length, "n": notable_json(m, Action::Axial), "v": notable_json(m, Action::Shear), "m": notable_json(m, Action::Moment)}))
        .collect();
    Ok(json!({
        "structure": st,
        "resolved": r.resolved.values_only(),
        "status": status,
        "degree": degree,
        "message": an.status.describe(),
        "truss": an.truss,
        "reactions": reactions,
        "members": members,
        "errors": r.plot.errors,
    }))
}

/// The selected structure, changed by `edit`, stored and redrawn; returns `structure.get`.
fn edit(s: &mut Session, edit: impl FnOnce(&mut Structure) -> Result<(), CmdError>) -> CmdResult {
    let (pos, src) = selected_source(s).ok_or_else(|| CmdError::Disabled("select a structure first".into()))?;
    let mut st = Structure::from_json(&src).map_err(|e| CmdError::Failed(format!("the structure can't be read: {e}")))?;
    edit(&mut st)?;
    store(s, &pos, &st.sanitized())?;
    get(s, &json!({}))
}

fn set(s: &mut Session, v: &Value) -> CmdResult {
    let replacement = structure_param(v)?;
    edit(s, |st| {
        if let Some(new) = replacement {
            *st = new;
        }
        apply_settings(st, v)
    })
}

fn node(s: &mut Session, v: &Value) -> CmdResult {
    let (x, y) = (num(v, "x")?, num(v, "y")?);
    edit(s, |st| {
        let id = p::str(v, "id").map(str::trim).filter(|t| !t.is_empty()).map(str::to_string);
        if p::bool(v, "remove").unwrap_or(false) {
            let id = id.ok_or_else(|| CmdError::Params("remove: which node (id)".into()))?;
            if st.node(&id).is_none() {
                return Err(CmdError::Params(format!("no node {id}")));
            }
            st.nodes.retain(|n| n.id != id);
            let gone: Vec<String> = st.members.iter().filter(|m| m.a == id || m.b == id).map(|m| m.id.clone()).collect();
            st.members.retain(|m| m.a != id && m.b != id);
            st.supports.retain(|s| s.node != id);
            st.loads.retain(|l| !load_uses(l, Some(&id), &gone));
            return Ok(());
        }
        let id = id.unwrap_or_else(|| st.next_node_id());
        let rename = p::str(v, "rename").map(str::trim).filter(|t| !t.is_empty()).map(str::to_string);
        if let Some(new) = &rename
            && *new != id
            && st.node(new).is_some()
        {
            return Err(CmdError::Params(format!("rename: node {new} exists")));
        }
        match st.nodes.iter_mut().find(|n| n.id == id) {
            Some(n) => {
                if let Some(x) = x {
                    n.x = x;
                }
                if let Some(y) = y {
                    n.y = y;
                }
                if let Some(h) = p::bool(v, "hinge") {
                    n.hinge = h;
                }
            }
            None => {
                if st.nodes.len() >= 200 {
                    return Err(CmdError::Failed("a structure has at most 200 nodes".into()));
                }
                st.nodes.push(Node {
                    id: id.clone(),
                    x: x.unwrap_or_default(),
                    y: y.unwrap_or_default(),
                    hinge: p::bool(v, "hinge").unwrap_or(false),
                });
            }
        }
        if let Some(new) = rename {
            for n in st.nodes.iter_mut().filter(|n| n.id == id) {
                n.id.clone_from(&new);
            }
            for m in &mut st.members {
                if m.a == id {
                    m.a.clone_from(&new);
                }
                if m.b == id {
                    m.b.clone_from(&new);
                }
            }
            for s in st.supports.iter_mut().filter(|s| s.node == id) {
                s.node.clone_from(&new);
            }
            for l in &mut st.loads {
                if let Load::Force { node: Some(n), .. } | Load::Moment { node: Some(n), .. } = l
                    && *n == id
                {
                    n.clone_from(&new);
                }
            }
        }
        Ok(())
    })
}

/// Does load `l` act on node `node` or on one of `members`?
fn load_uses(l: &Load, node: Option<&str>, members: &[String]) -> bool {
    match l {
        Load::Force { node: n, member: m, .. } | Load::Moment { node: n, member: m, .. } => {
            (n.is_some() && n.as_deref() == node) || m.as_ref().is_some_and(|m| members.contains(m))
        }
        Load::Distributed { member, .. } => members.contains(member),
    }
}

fn member(s: &mut Session, v: &Value) -> CmdResult {
    let (e, i, area) = (opt_num(v, "e")?, opt_num(v, "i")?, opt_num(v, "area")?);
    edit(s, |st| {
        let id = p::str(v, "id").map(str::trim).filter(|t| !t.is_empty()).map(str::to_string);
        if p::bool(v, "remove").unwrap_or(false) {
            let id = id.ok_or_else(|| CmdError::Params("remove: which member (id)".into()))?;
            if st.member(&id).is_none() {
                return Err(CmdError::Params(format!("no member {id}")));
            }
            st.members.retain(|m| m.id != id);
            st.loads.retain(|l| !load_uses(l, None, std::slice::from_ref(&id)));
            return Ok(());
        }
        let id = id.unwrap_or_else(|| st.next_member_id());
        let (a, b) = (p::str(v, "a").map(str::to_string), p::str(v, "b").map(str::to_string));
        for node in [&a, &b].into_iter().flatten() {
            if st.node(node).is_none() {
                return Err(CmdError::Params(format!("no node {node}")));
            }
        }
        let m = match st.members.iter().position(|m| m.id == id) {
            Some(k) => st.members.get_mut(k),
            None => {
                let (Some(a), Some(b)) = (a.clone(), b.clone()) else {
                    return Err(CmdError::Params("a new member needs its nodes a and b".into()));
                };
                if a == b {
                    return Err(CmdError::Params("a member joins two different nodes".into()));
                }
                if st.members.len() >= 300 {
                    return Err(CmdError::Failed("a structure has at most 300 members".into()));
                }
                st.members.push(Member { id: id.clone(), a, b, ..Member::default() });
                st.members.last_mut()
            }
        };
        let Some(m) = m else { return Err(CmdError::Failed("no member there".into())) };
        if let Some(a) = a {
            m.a = a;
        }
        if let Some(b) = b {
            m.b = b;
        }
        if m.a == m.b {
            return Err(CmdError::Params("a member joins two different nodes".into()));
        }
        if let Some(h) = p::bool(v, "hingeA") {
            m.hinge_a = h;
        }
        if let Some(h) = p::bool(v, "hingeB") {
            m.hinge_b = h;
        }
        if let Some(e) = e {
            m.e = e;
        }
        if let Some(i) = i {
            m.i = i;
        }
        if let Some(a) = area {
            m.area = a;
        }
        Ok(())
    })
}

fn support(s: &mut Session, v: &Value) -> CmdResult {
    let angle = num(v, "angle")?;
    edit(s, |st| {
        let node = p::str(v, "node").ok_or_else(|| CmdError::Params("node: which node".into()))?.to_string();
        if st.node(&node).is_none() {
            return Err(CmdError::Params(format!("no node {node}")));
        }
        if p::bool(v, "remove").unwrap_or(false) {
            st.supports.retain(|s| s.node != node);
            return Ok(());
        }
        let kind = match p::str(v, "kind") {
            Some(k) => {
                Some(SupportKind::parse(k).ok_or_else(|| CmdError::Params(format!("kind: \"{k}\" is not fixed, pin, roller, link or slider")))?)
            }
            None => None,
        };
        match st.supports.iter_mut().find(|s| s.node == node) {
            Some(s) => {
                if let Some(k) = kind {
                    s.kind = k;
                }
                if let Some(a) = angle {
                    s.angle = a;
                }
            }
            None => st.supports.push(Support { node, kind: kind.unwrap_or_default(), angle: angle.unwrap_or_default() }),
        }
        Ok(())
    })
}

fn load(s: &mut Session, v: &Value) -> CmdResult {
    let index = v.get("index").and_then(Value::as_u64).and_then(|i| usize::try_from(i).ok());
    let parsed: Option<Load> = match v.get("load") {
        None | Some(Value::Null) => None,
        Some(l) => Some(serde_json::from_value(l.clone()).map_err(|e| CmdError::Params(format!("load: {e}")))?),
    };
    edit(s, |st| {
        if p::bool(v, "remove").unwrap_or(false) {
            let k = index.filter(|k| *k < st.loads.len()).ok_or_else(|| CmdError::Params("remove: which load (index)".into()))?;
            st.loads.remove(k);
            return Ok(());
        }
        let l = parsed.ok_or_else(|| CmdError::Params("load: the load".into()))?;
        match index {
            Some(k) => *st.loads.get_mut(k).ok_or_else(|| CmdError::Params(format!("no load {k}")))? = l,
            None => {
                if st.loads.len() >= 500 {
                    return Err(CmdError::Failed("a structure has at most 500 loads".into()));
                }
                st.loads.push(l);
            }
        }
        Ok(())
    })
}

fn store(s: &mut Session, pos: &wordcraft_doc::Pos, st: &Structure) -> Result<(), CmdError> {
    let json_arc: Arc<str> = st.to_json().into();
    {
        let para = s.doc.para_mut(pos.story, &pos.path).map_err(|e| CmdError::Failed(e.to_string()))?;
        {
            let Some(InlineObject::Graphic { graphic, .. }) = para.object_at_mut(pos.off) else {
                return Err(CmdError::Failed("no structure there".into()));
            };
            if graphic.kind != GraphicKind::Structure {
                return Err(CmdError::Failed("not a structure".into()));
            }
            Arc::make_mut(graphic).structure = Some(json_arc.clone());
        }
        para.touch();
    }
    let report = super::calc::recalculate_now(s);
    // A structure the recalculation does not reach (outside the body) is drawn here.
    if report["changed"].as_u64().unwrap_or(0) == 0 && super::calc::resolved_structure(s, &json_arc).is_none() {
        let rendered = st.render(None).plot;
        let (dw, dh, items) = super::plot::items_of(&rendered, &s.doc.settings.minor_font);
        let svg: Arc<str> = rendered.to_svg().into();
        if let Ok(para) = s.doc.para_mut(pos.story, &pos.path) {
            let mut updated = false;
            if let Some(InlineObject::Graphic { w, h, graphic, .. }) = para.object_at_mut(pos.off) {
                let g = Arc::make_mut(graphic);
                g.items = items;
                g.svg = Some(svg);
                (g.w, g.h) = (dw, dh);
                (*w, *h) = (dw, dh);
                updated = true;
            }
            if updated {
                para.touch();
            }
        }
    }
    s.touch();
    Ok(())
}

#[cfg(test)]
mod tests;
