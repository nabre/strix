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
            r#"{"structure"?: object|string, "title"?: string, "width"?: mm, "lengthUnit"?: "m"|"cm"|"mm", "forceUnit"?: "kN"|"N"|"daN" (the numbers converted: 1 m becomes 100 cm; not with "structure" or "convert": false), "convert"?: bool, "answer"?: bool, "convention"?: "italian", "diagrams"?: object (merged: {"n"|"v"|"m": {"show"?, "positive"?: "up"|"down", "amplitude"?}, "reactions"?, "values"?, "hatch"?, "signs"?, "dimensions"?, "stations"?}), "titles"?: {"n"?, "v"?, "m"?} (the symbols, used in the working too), "names"?: {"n"?, "v"?, "m"?} (the panels' names in place of the symbols, e.g. "Sforzo normale"; "" gives the symbol back), "positive"?: {"x"?: "right"|"left", "y"?: "up"|"down", "rotation"?: "ccw"|"cw"} (merged: the senses the reactions' working counts positive, drawn as a symbol by its free body; the reactions are taken positive so and the equilibrium equations sum so), "layout"?: "auto"|"column"|"grid"} → updates the selected structure and redraws it"#,
        ),
        CommandSpec::new("structure.node", "Structure Node", PLACE, node).when(selected).params(
            r#"{"id"?: string, "x"?: num|expr, "y"?: num|expr, "hinge"?: bool, "rename"?: string, "remove"?: bool} → adds a node (a new name when id is absent or new), changes it, or removes it with what uses it"#,
        ),
        CommandSpec::new("structure.member", "Structure Member", PLACE, member).when(selected).params(
            r#"{"id"?: string, "a"?: node, "b"?: node, "hingeA"?: bool, "hingeB"?: bool, "e"?: num|expr|null, "i"?: num|expr|null, "area"?: num|expr|null, "remove"?: bool} → adds a member between nodes a and b, changes it, or removes it with its loads"#,
        ),
        CommandSpec::new("structure.support", "Structure Support", PLACE, support).when(selected).params(
            r#"{"node": string, "kind"?: "fixed"|"pin"|"roller"|"rollerCircle"|"link"|"slider", "angle"?: num|expr, "remove"?: bool} → puts a support on a node (replacing its support), turns it, or removes it"#,
        ),
        CommandSpec::new("structure.working", "Write Working", PLACE, working::run).when(selected).params(
            r#"{"labels"?: {"heading"?, "loads"?, "reactions"?, "signs"?, "hyperstatic"?, "together"?, "force1"? ("… {X} …"), "force2"?, "system0"?, "system"? ("… {i} …"), "congruence"?, "eiconst"?, "eaconst"?, "noaxial"?, "cramer"?, "forces"?, "conventions"?, "member"?, "axial"?, "hinge"?, "range"? ("x from {a} to {b}"): string}} → writes after the selected structure its solution as live calculations: the loads' names defined (F_1, q_1, a varying load's ends q_(1,1) and q_(1,2), couples C_1), the equilibrium equations giving the reactions (a hyperstatic structure of degree 1 or 2: its redundants first, by the force method), then each member's laws N(x), T(x), M(x) stretch by stretch with their values (a truss: its axial forces)"#,
        ),
        CommandSpec::new("structure.exercise", "Structure Exercise", PLACE, exercise::run).params(
            r#"{"structure"?: object|string, "template"?: "beam"|"cantilever"|"continuous"|"frame"|"truss", "labels"?: {"title"?, "text"?, "jump"?, "nozero"? ({T}: the shear's name), and structure.working's labels}, "questions"?: {"reactions"?, "axial"?, "shear"?, "zeros"?, "maxima"?, "moments"?, "trussAxial"?: string}} → a new exercise on a structure: the statement with the structure and its loads, six questions (reactions; N; T; where T is zero; the greatest moment there; M — a truss: reactions and axial forces), and in the solution under each question its working (T's law solved for its zero, M's law there) and its drawing (the reactions; one diagram each), all drawings of one structure (a change to one is made to the others)"#,
        ),
        CommandSpec::new("structure.mode", "Switch Structure Mode", PLACE, mode::run).when(mode::disabled).params(
            r#"{"mode"?: "report"|"exercise" (default: the other one), "working"?: bool (to a report: write its working, default true), "labels"?, "questions"? (as structure.exercise's)} → the selected structure (or the exercise at the caret) passes from a technical report (the structure with all its diagrams and its working after it) to an exercise on it (the structure in the statement, the questions, the working and the drawings in the solution), or back, where it was; the working follows the structure, and its parameters, in either mode"#,
        ),
        CommandSpec::new("structure.load", "Structure Load", PLACE, load).when(selected).params(
            r#"{"index"?: n, "load"?: object ({"type": "force"|"moment"|"distributed", ...}), "remove"?: bool} → adds a load, replaces load `index`, or removes it"#,
        ),
    ]
}

/// Whether the structure `structure.mode` would act on is in an exercise (so it would become a
/// report).
pub fn in_exercise(s: &Session) -> bool {
    mode::target(s).is_some_and(|t| t.2.is_some())
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

/// The drawing a change to the structure at `pos` is made to: `pos`, unless it is in one of its
/// group's written workings (a free body, a stretch), which the change writes again; then the
/// group's drawing outside them, the structure itself (a report's, an exercise's statement)
/// before the others.
fn base_of(s: &Session, pos: &wordcraft_doc::Pos, src: &str) -> wordcraft_doc::Pos {
    let Ok(st) = Structure::from_json(src) else { return pos.clone() };
    let block = |p: &wordcraft_doc::Pos| {
        (p.story == wordcraft_doc::StoryRef::Body && p.path.0.len() == 1).then(|| p.path.0.first().map(|&t| t as usize)).flatten()
    };
    if !block(pos).is_some_and(|i| working::rewrites(s, &st.group, i)) {
        return pos.clone();
    }
    let mut found: Option<(bool, wordcraft_doc::Pos)> = None;
    for (i, b) in s.doc.body.iter().enumerate() {
        let wordcraft_doc::Block::Para(p) = &**b else { continue };
        if working::rewrites(s, &st.group, i) {
            continue;
        }
        for (off, o) in p.objects_with_offsets() {
            if let InlineObject::Graphic { graphic, .. } = o
                && graphic.kind == GraphicKind::Structure
                && let Some(other) = graphic.structure.as_deref().and_then(|j| Structure::from_json(j).ok())
                && other.group == st.group
            {
                let itself = matches!(other.view, strix_structure::View::All | strix_structure::View::Structure);
                if found.as_ref().is_none_or(|(was, _)| itself && !was) {
                    let path = wordcraft_doc::Path(vec![u32::try_from(i).unwrap_or(u32::MAX)]);
                    found = Some((itself, wordcraft_doc::Pos::new(wordcraft_doc::StoryRef::Body, path, off)));
                }
            }
        }
    }
    found.map_or_else(|| pos.clone(), |(_, p)| p)
}

/// Selects the structure itself in place of the selected drawing of its written working (a free
/// body, a stretch), which a change writes again, so its editor edits the structure; returns
/// whether the selection moved.
pub fn select_base(s: &mut Session) -> bool {
    let Some((pos, src)) = selected_source(s) else { return false };
    let base = base_of(s, &pos, &src);
    if base == pos {
        return false;
    }
    let end = wordcraft_doc::Pos { off: base.off.saturating_add(wordcraft_doc::para::OBJ.len_utf8()), ..base.clone() };
    s.sel = Selection { anchor: base, focus: end };
    true
}

/// User-authored strings of the selected structure (title, names, expressions), so interface
/// localization audits treat them as document content.
pub fn selected_structure_texts(s: &Session) -> Vec<String> {
    let Some((_, src)) = selected_source(s) else { return Vec::new() };
    let Ok(st) = Structure::from_json(&src) else { return Vec::new() };
    let mut out = vec![st.title.clone(), st.titles.n.clone(), st.titles.v.clone(), st.titles.m.clone()];
    out.extend([st.names.n.clone(), st.names.v.clone(), st.names.m.clone()]);
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
        then: Vec::new(),
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
            supports: vec![sup("A", SupportKind::Pin, 0.0), sup("B", SupportKind::RollerCircle, 0.0)],
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
            supports: vec![sup("A", SupportKind::Pin, 0.0), sup("B", SupportKind::RollerCircle, 0.0), sup("C", SupportKind::RollerCircle, 0.0)],
            loads: vec![uniform("1", 10.0), uniform("2", 10.0)],
            ..Structure::default()
        },
        "frame" => Structure {
            nodes: vec![n(0.0, 0.0, "A"), n(0.0, 4.0, "B"), n(6.0, 4.0, "C"), n(6.0, 0.0, "D")],
            members: vec![bar("1", "A", "B"), bar("2", "B", "C"), bar("3", "C", "D")],
            supports: vec![sup("A", SupportKind::Pin, 0.0), sup("D", SupportKind::RollerCircle, 0.0)],
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
                supports: vec![sup("A", SupportKind::Pin, 0.0), sup("C", SupportKind::RollerCircle, 0.0)],
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

fn apply_settings(st: &mut Structure, v: &Value, convert: bool) -> Result<(), CmdError> {
    if let Some(t) = p::str(v, "title") {
        st.title = t.to_string();
    }
    if let Some(w) = v.get("width").and_then(Value::as_f64).filter(|w| w.is_finite()) {
        st.width = w;
    }
    let mut lu = st.length_unit;
    let mut fu = st.force_unit;
    if let Some(u) = p::str(v, "lengthUnit") {
        lu = strix_structure::LengthUnit::parse(u).ok_or_else(|| CmdError::Params(format!("lengthUnit: \"{u}\" is not m, cm or mm")))?;
    }
    if let Some(u) = p::str(v, "forceUnit") {
        fu = strix_structure::ForceUnit::parse(u).ok_or_else(|| CmdError::Params(format!("forceUnit: \"{u}\" is not kN, N or daN")))?;
    }
    // A change of unit keeps the structure as it is (1 m becomes 100 cm), unless its numbers
    // come in the same call (a structure given with them, or "convert": false): those are
    // already in the new units.
    if convert && p::bool(v, "convert").unwrap_or(true) {
        st.convert_units(lu, fu);
    } else {
        st.length_unit = lu;
        st.force_unit = fu;
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
    if let Some(t) = v.get("names").filter(|t| t.is_object()) {
        let mut cur = serde_json::to_value(&st.names).map_err(|e| CmdError::Failed(e.to_string()))?;
        merge(&mut cur, t);
        st.names = serde_json::from_value(cur).map_err(|e| CmdError::Params(format!("names: {e}")))?;
    }
    if let Some(t) = v.get("positive").filter(|t| t.is_object()) {
        let mut cur = serde_json::to_value(st.positive).map_err(|e| CmdError::Failed(e.to_string()))?;
        merge(&mut cur, t);
        st.positive = serde_json::from_value(cur).map_err(|e| CmdError::Params(format!("positive: {e}")))?;
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
    let given = structure_param(v)?;
    let convert = given.is_none();
    let mut st = match given {
        Some(st) => st,
        None => template(p::str(v, "template").unwrap_or("beam"))?,
    };
    apply_settings(&mut st, v, convert)?;
    let obj = graphic(s, &st.sanitized());
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

/// A structure as an inline graphic, drawn with no parameters (the recalculation draws it again
/// where it lands).
fn graphic(s: &Session, st: &Structure) -> InlineObject {
    let rendered = st.render(None).plot;
    let (w, h, items) = super::plot::items_of(&rendered, &s.doc.settings.minor_font);
    let svg = Some(rendered.to_svg().into());
    let graphic = Graphic { kind: GraphicKind::Structure, items, w, h, structure: Some(st.to_json().into()), svg, ..Default::default() };
    InlineObject::Graphic { w, h, alt: String::new(), float: Float::default(), graphic: Arc::new(graphic) }
}

/// The other drawings of `st`'s group in the body (an exercise's statement and solution) take
/// its change, each keeping its own view and width.
fn sync_group(s: &mut Session, pos: &wordcraft_doc::Pos, st: &Structure) {
    if st.group.is_empty() {
        return;
    }
    for i in 0..s.doc.body.len() {
        let here = pos.story == wordcraft_doc::StoryRef::Body && pos.path.0.len() == 1 && pos.path.0.first().is_some_and(|&t| t as usize == i);
        let Some(block) = s.doc.body.get(i) else { continue };
        let wordcraft_doc::Block::Para(p) = &**block else { continue };
        let offs: Vec<usize> = p
            .objects_with_offsets()
            .filter_map(|(off, o)| match o {
                InlineObject::Graphic { graphic, .. } if graphic.kind == GraphicKind::Structure && !(here && off == pos.off) => {
                    let other = Structure::from_json(graphic.structure.as_deref()?).ok()?;
                    (other.group == st.group).then_some(off)
                }
                _ => None,
            })
            .collect();
        if offs.is_empty() {
            continue;
        }
        let Some(block) = s.doc.body.get_mut(i) else { continue };
        let wordcraft_doc::Block::Para(p) = Arc::make_mut(block) else { continue };
        for off in offs {
            if let Some(InlineObject::Graphic { graphic, .. }) = p.object_at_mut(off)
                && let Some(other) = graphic.structure.as_deref().and_then(|src| Structure::from_json(src).ok())
            {
                let mine = Structure { view: other.view, width: other.width, cut: other.cut.clone(), pivot: other.pivot.clone(), ..st.clone() };
                Arc::make_mut(graphic).structure = Some(mine.to_json().into());
            }
        }
        p.touch();
    }
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

/// The selected structure (the structure itself when a drawing of its working is selected),
/// changed by `edit`, stored and redrawn; returns `structure.get`.
fn edit(s: &mut Session, edit: impl FnOnce(&mut Structure) -> Result<(), CmdError>) -> CmdResult {
    // A drawing of the structure's working is written again by the change: the structure takes
    // it, and stays selected.
    select_base(s);
    let (pos, src) = selected_source(s).ok_or_else(|| CmdError::Disabled("select a structure first".into()))?;
    let mut st = Structure::from_json(&src).map_err(|e| CmdError::Failed(format!("the structure can't be read: {e}")))?;
    edit(&mut st)?;
    store(s, &pos, &st.sanitized())?;
    get(s, &json!({}))
}

fn set(s: &mut Session, v: &Value) -> CmdResult {
    let replacement = structure_param(v)?;
    let convert = replacement.is_none();
    let mut shear = None;
    let r = edit(s, |st| {
        // The symbol the questions were written with: the laws' (`T`), or, in an exercise
        // written before the questions took it, the diagram's title as it was (`T [kN]`).
        let old = [working::shear_name(st), st.titles.v.trim().to_string()];
        if let Some(new) = replacement {
            *st = new;
        }
        apply_settings(st, v, convert)?;
        shear = Some((old, working::shear_name(st)));
        Ok(())
    })?;
    // An exercise's questions name the shear by its symbol: they take the new one.
    if let Some((old, new)) = shear.filter(|([old, _], new)| old != new)
        && let Some((pos, _)) = selected_source(s)
    {
        let mut changed = exercise::rename_shear(s, &pos, &old[0], &new);
        if !changed && old[1] != old[0] {
            changed = exercise::rename_shear(s, &pos, &old[1], &new);
        }
        if changed {
            super::calc::recalculate_now(s);
        }
    }
    Ok(r)
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
        Load::Distributed { member, then, .. } => members.contains(member) || then.iter().any(|m| members.contains(m)),
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
            Some(k) => Some(
                SupportKind::parse(k)
                    .ok_or_else(|| CmdError::Params(format!("kind: \"{k}\" is not fixed, pin, roller, rollerCircle, link or slider")))?,
            ),
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

/// `st` in place of the structure at `pos` and in the other drawings of its group, not redrawn
/// yet; returns its source.
fn put(s: &mut Session, pos: &wordcraft_doc::Pos, st: &Structure) -> Result<Arc<str>, CmdError> {
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
    sync_group(s, pos, st);
    Ok(json_arc)
}

fn store(s: &mut Session, pos: &wordcraft_doc::Pos, st: &Structure) -> Result<(), CmdError> {
    let mut json_arc = put(s, pos, st)?;
    // A structure with parameters and a working keeps what it is written from, so a
    // recalculation writes the working again only when a parameter changes.
    let keyed;
    let st = if !st.expressions().is_empty() && working::has_live(s, &st.group) {
        keyed = Structure { working_key: mode::values_key(s, &json_arc), ..st.clone() };
        if keyed.working_key != st.working_key {
            json_arc = put(s, pos, &keyed)?;
        }
        &keyed
    } else {
        st
    };
    // Its written workings follow it (a structure that can't be solved says so there).
    let mut pos = pos.clone();
    working::refresh(s, &json_arc, st, &mut pos)?;
    let pos = &pos;
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

/// A group name no structure in the body has yet: `structure-1`, `structure-2`…
fn free_group(s: &Session) -> String {
    let mut used: Vec<String> = Vec::new();
    for b in &s.doc.body {
        let wordcraft_doc::Block::Para(p) = &**b else { continue };
        for (_, o) in p.objects_with_offsets() {
            if let InlineObject::Graphic { graphic, .. } = o
                && graphic.kind == GraphicKind::Structure
                && let Some(st) = graphic.structure.as_deref().and_then(|src| Structure::from_json(src).ok())
                && !st.group.is_empty()
            {
                used.push(st.group);
            }
        }
    }
    let mut n = used.len() + 1;
    for k in 1..=used.len() + 1 {
        let name = format!("structure-{k}");
        if !used.contains(&name) {
            n = k;
            break;
        }
    }
    format!("structure-{n}")
}

mod exercise;
mod mode;
pub(crate) use mode::follow_values;
mod working;

#[cfg(test)]
mod tests;
