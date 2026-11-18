//! `structure.mode`: a structure passes from one way of writing it to the other
//! (`docs/structures.md`, Modes): a technical report (the structure with all its diagrams and,
//! when it has one, its working written after it) or an exercise on the structure (the
//! structure in the statement, the questions, the working and the drawings in the solution).
//!
//! Also [`follow_values`]: a structure whose numbers are calculation parameters writes its
//! working again when a parameter changes, in either mode.

use std::sync::Arc;

use serde_json::{Value, json};
use strix_structure::{Structure, View};
use wordcraft_doc::graphic::GraphicKind;
use wordcraft_doc::para::OBJ;
use wordcraft_doc::{Block, InlineObject, Pos, StoryRef};

use crate::{CmdError, CmdResult, Selection, Session, p};

/// At most this many structures follow their parameters after one calculation.
const MAX_FOLLOWED: usize = 32;

/// The structure `structure.mode` acts on: the selected one, else the first one of the exercise
/// at the caret. With the exercise it is in (0-based), when it is in one.
pub(super) fn target(s: &Session) -> Option<(Pos, Arc<str>, Option<usize>)> {
    let zones = super::super::exercise::zones(s);
    let zone = |pos: &Pos| {
        if pos.story != StoryRef::Body || pos.path.0.len() != 1 {
            return None;
        }
        pos.path.0.first().and_then(|&t| zones.get(t as usize).copied().flatten())
    };
    if let Some((pos, src)) = super::selected_source(s) {
        let e = zone(&pos);
        return Some((pos, src, e));
    }
    let e = super::super::exercise::exercise_at_caret(s)?;
    for (i, b) in s.doc.body.iter().enumerate() {
        if zones.get(i).copied().flatten() != Some(e) {
            continue;
        }
        let Block::Para(para) = &**b else { continue };
        for (off, o) in para.objects_with_offsets() {
            if let InlineObject::Graphic { graphic, .. } = o
                && graphic.kind == GraphicKind::Structure
                && let Some(src) = graphic.structure.clone()
            {
                let pos = Pos::new(StoryRef::Body, wordcraft_doc::Path(vec![u32::try_from(i).unwrap_or(u32::MAX)]), off);
                return Some((pos, src, Some(e)));
            }
        }
    }
    None
}

/// Why `structure.mode` can't run, if it can't.
pub(super) fn disabled(s: &Session) -> Option<&'static str> {
    if target(s).is_some() { None } else { Some("select a structure, or put the cursor in an exercise on a structure") }
}

pub(super) fn run(s: &mut Session, v: &Value) -> CmdResult {
    let (pos, src, exercise) = target(s).ok_or_else(|| CmdError::Disabled("select a structure first".into()))?;
    let to_exercise = match p::str(v, "mode") {
        None => exercise.is_none(),
        Some("exercise") => true,
        Some("report" | "structure") => false,
        Some(other) => return Err(CmdError::Params(format!("mode: \"{other}\" is not report or exercise"))),
    };
    let st = Structure::from_json(&src).map_err(|e| CmdError::Failed(format!("the structure can't be read: {e}")))?;
    match (to_exercise, exercise) {
        (true, None) => to_exercise_mode(s, pos, &src, st, v),
        (false, Some(e)) => to_report_mode(s, e, st, v),
        // Already so: nothing to do.
        (true, Some(_)) => Ok(json!({"mode": "exercise", "changed": false})),
        (false, None) => Ok(json!({"mode": "report", "changed": false})),
    }
}

/// The report's structure (and its working) give way to an exercise on it, where it was.
fn to_exercise_mode(s: &mut Session, pos: Pos, src: &Arc<str>, st: Structure, v: &Value) -> CmdResult {
    if pos.story != StoryRef::Body || pos.path.0.len() != 1 {
        return Err(CmdError::Disabled("an exercise goes in the body, outside tables".into()));
    }
    let top = pos.path.0.first().map_or(0, |&t| t as usize);
    // The parameters it reads, defined around it, are the exercise's data (its sandbox doesn't
    // see the document's).
    let data = super::super::calc::worksheet_at(s, src).map(|ws| parameters(&st, ws)).unwrap_or_default();
    // Its working first (it comes after the structure), then the structure.
    if !st.group.is_empty()
        && let Some((a, b)) = super::working::live_range(s, &super::working::live_name(&st.group, super::working::Part::Whole))
        && a > top
        && b < s.doc.body.len()
    {
        s.doc.body.drain(a..=b);
    }
    let end = Pos { off: pos.off + OBJ.len_utf8(), ..pos.clone() };
    let at = s.doc.delete_range(&pos, &end).map_err(|e| CmdError::Failed(e.to_string()))?;
    s.sel = Selection::caret(at);
    s.touch();
    let mut params = v.clone();
    let src = Structure { group: String::new(), working_key: String::new(), view: View::All, ..st };
    if let Some(o) = params.as_object_mut() {
        o.remove("mode");
        o.remove("template");
        o.insert("structure".into(), Value::String(src.to_json()));
    }
    let r = super::exercise::run(s, &params)?;
    if !data.is_empty() {
        let items: Vec<Value> = data.iter().map(|(name, value)| json!({"equation": format!("{name} = {value}"), "values": [""]})).collect();
        s.run("exercise.setData", &json!({"data": {"items": items, "variants": ["A"], "active": 0}}))?;
    }
    Ok(json!({"mode": "exercise", "changed": true, "exercise": r["exercise"], "group": r["group"], "data": data.len()}))
}

/// Exercise `e` on the structure gives way to the structure with all its diagrams and its
/// working (unless `"working": false`), where the exercise was.
fn to_report_mode(s: &mut Session, e: usize, st: Structure, v: &Value) -> CmdResult {
    let st = Structure { view: View::All, working_key: String::new(), ..st }.sanitized();
    // The exercise's data (its active variant), written as calculations before the structure.
    let info = s.run("exercise.data", &json!({"exercise": e.saturating_add(1)})).unwrap_or(Value::Null);
    let active = info["data"]["active"].as_u64().and_then(|a| usize::try_from(a).ok()).unwrap_or(0);
    let data: Vec<(String, String)> = info["data"]["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|it| {
                    let symbol = it["symbol"].as_str().map(str::trim).filter(|t| !t.is_empty())?;
                    let value = it["values"].get(active).and_then(Value::as_str).filter(|t| !t.trim().is_empty())?;
                    let unit = it["unit"].as_str().unwrap_or_default().trim();
                    let eq = if unit.is_empty() { format!("{symbol} = {value}") } else { format!("{symbol} = {value} {unit}") };
                    Some((symbol.to_string(), eq))
                })
                .take(MAX_FOLLOWED)
                .collect()
        })
        .unwrap_or_default();
    s.run("exercise.delete", &json!({"exercise": e.saturating_add(1)}))?;
    // The caret is in the empty paragraph the exercise left: the structure goes there.
    let at = s.sel.focus.clone();
    let props = s.typing_props();
    let end = s.doc.insert_object(&at, super::graphic(s, &st), &props).map_err(|e| CmdError::Failed(e.to_string()))?;
    s.sel = Selection { anchor: at.clone(), focus: end };
    s.touch();
    // The data the document doesn't define there yet, as calculations before it (one defined
    // already keeps the document's value: a second `q = …` would read it, not define it).
    let src = super::selected_source(s).map(|(_, src)| src);
    let ws = src.as_ref().and_then(|src| super::super::calc::worksheet_at(s, src));
    let (written_data, kept): (Vec<_>, Vec<_>) = data.into_iter().partition(|(name, _)| ws.as_ref().is_none_or(|ws| ws.scope().get(name).is_none()));
    let mut at = at;
    for (_, eq) in &written_data {
        s.sel = Selection::caret(at.clone());
        s.run("calc.insert", &json!({"text": eq}))?;
        s.run("text.newParagraph", &json!({}))?;
        at = s.sel.focus.clone();
    }
    if !written_data.is_empty() {
        // The structure is at the start of the paragraph the caret is in.
        s.sel = Selection { anchor: Pos { off: 0, ..at.clone() }, focus: Pos { off: OBJ.len_utf8(), ..at } };
        if super::selected_source(s).is_none() {
            return Err(CmdError::Failed("the structure vanished".into()));
        }
    }
    super::super::calc::recalculate_now(s);
    let mut written = Value::Null;
    if p::bool(v, "working").unwrap_or(true) {
        // In the words the structure kept, unless others are given.
        let mut params = v.clone();
        let mut labels: serde_json::Map<String, Value> = st.working_labels.iter().map(|(k, t)| (k.clone(), Value::String(t.clone()))).collect();
        if let Some(given) = v.get("labels").and_then(Value::as_object) {
            labels.extend(given.clone());
        }
        if let Some(o) = params.as_object_mut() {
            o.insert("labels".into(), Value::Object(labels));
        }
        written = match super::working::run(s, &params) {
            Ok(r) => r,
            // A structure that can't be solved (its parameters gone with the exercise) stays
            // without a working.
            Err(e) => json!({"error": e.to_string()}),
        };
    }
    let kept: Vec<String> = kept.into_iter().map(|(name, _)| name).collect();
    Ok(json!({"mode": "report", "changed": true, "group": st.group, "data": written_data.len(), "kept": kept, "working": written}))
}

/// The parameters structure `st` reads that worksheet `ws` defines, each with its value written
/// in full (`q`, `10 kN/m`), in the order they are first read.
fn parameters(st: &Structure, mut ws: strix_calc::Worksheet) -> Vec<(String, String)> {
    let mut opts = *ws.options();
    opts.digits = 12;
    opts.count = strix_calc::Count::Significant;
    opts.zeros = false;
    ws.set_options(opts);
    let mut names: Vec<String> = Vec::new();
    for expr in st.expressions() {
        for name in expr.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
            if name.chars().next().is_some_and(char::is_alphabetic) && !names.iter().any(|n| n == name) && ws.scope().get(name).is_some() {
                names.push(name.to_string());
            }
        }
    }
    names.into_iter().filter_map(|n| ws.shown(&n).map(|v| (n, v.replace('\u{2215}', "/")))).take(MAX_FOLLOWED).collect()
}

/// FNV-1a: stable across builds, so a saved key still matches.
fn fnv(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// What the working of structure `src` (as it sits in the body) is written from: its values as
/// the calculations give them there, whether it can be solved, the decimal separator.
pub(super) fn values_key(s: &Session, src: &Arc<str>) -> String {
    let Some(r) = super::super::calc::resolved_structure(s, src) else { return String::new() };
    let plain = Structure { view: View::All, width: 0.0, working_key: String::new(), ..r.resolved };
    let comma = super::super::calc::options_now(s).decimal_comma;
    // Its debug form: the JSON writes a parameter's name, not its value.
    let text = format!("{plain:?}|{}|{comma}", r.analysis.status.describe());
    format!("{:016x}", fnv(text.as_bytes()))
}

/// After a calculation: each structure with parameters and a written working (in a report or in
/// an exercise) whose values changed writes its working again. Returns whether one did.
pub(crate) fn follow_values(s: &mut Session) -> bool {
    // A document just opened stays as it was saved.
    if s.calc_seen.is_none() {
        return false;
    }
    let mut groups: Vec<String> = Vec::new();
    let mut todo: Vec<(Pos, Arc<str>, Structure)> = Vec::new();
    for (i, b) in s.doc.body.iter().enumerate() {
        let Block::Para(para) = &**b else { continue };
        for (off, o) in para.objects_with_offsets() {
            let InlineObject::Graphic { graphic, .. } = o else { continue };
            if graphic.kind != GraphicKind::Structure {
                continue;
            }
            let Some(src) = graphic.structure.clone() else { continue };
            let Ok(st) = Structure::from_json(&src) else { continue };
            // The first drawing of each group stands for the group.
            if st.group.is_empty() || groups.contains(&st.group) {
                continue;
            }
            groups.push(st.group.clone());
            if st.expressions().is_empty() {
                continue;
            }
            let pos = Pos::new(StoryRef::Body, wordcraft_doc::Path(vec![u32::try_from(i).unwrap_or(u32::MAX)]), off);
            todo.push((pos, src, st));
        }
    }
    todo.retain(|(_, _, st)| super::working::has_live(s, &st.group));
    todo.truncate(MAX_FOLLOWED);
    let mut changed = false;
    // The last first: a working written again moves only what comes after it.
    for (pos, src, st) in todo.into_iter().rev() {
        let key = values_key(s, &src);
        if key.is_empty() || key == st.working_key {
            continue;
        }
        let st = Structure { working_key: key, ..st };
        let Ok(src) = super::put(s, &pos, &st) else { continue };
        let mut pos = pos;
        if super::working::refresh(s, &src, &st, &mut pos).is_ok() {
            changed = true;
        }
    }
    if changed {
        s.touch();
    }
    changed
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use strix_structure::{Structure, View};
    use wordcraft_doc::graphic::GraphicKind;
    use wordcraft_doc::para::OBJ;
    use wordcraft_doc::{Block, Document, InlineObject, Pos, StoryRef};

    use crate::{Selection, Session};

    fn structures(s: &Session) -> Vec<(usize, usize, Structure)> {
        let mut out = Vec::new();
        for (i, b) in s.doc.body.iter().enumerate() {
            let Block::Para(p) = &**b else { continue };
            for (off, o) in p.objects_with_offsets() {
                if let InlineObject::Graphic { graphic, .. } = o
                    && graphic.kind == GraphicKind::Structure
                {
                    out.push((i, off, Structure::from_json(graphic.structure.as_deref().unwrap()).unwrap()));
                }
            }
        }
        out
    }

    fn text(s: &Session) -> String {
        s.doc.body.iter().filter_map(|b| if let Block::Para(p) = &**b { Some(p.text.replace(OBJ, "")) } else { None }).collect::<Vec<_>>().join("\n")
    }

    fn select(s: &mut Session, k: usize) {
        let (i, off, _) = structures(s)[k].clone();
        let at = Pos::new(StoryRef::Body, wordcraft_doc::Path(vec![i as u32]), off);
        s.sel = Selection { anchor: at.clone(), focus: Pos { off: off + OBJ.len_utf8(), ..at } };
    }

    fn exercises(s: &Session) -> usize {
        crate::cmd::exercise::context(s).1
    }

    #[test]
    fn a_report_becomes_an_exercise_and_back() {
        let mut s = Session::new(Document::new());
        s.run("text.insert", &json!({"text": "Before"})).unwrap();
        s.run("text.newParagraph", &json!({})).ok();
        s.run("structure.insert", &json!({"template": "beam"})).unwrap();
        s.run("structure.node", &json!({"id": "B", "x": 8})).unwrap();
        s.run("structure.working", &json!({"labels": {"heading": "Svolgimento"}})).unwrap();
        assert!(text(&s).contains("Svolgimento"), "{}", text(&s));
        // The structure, and the free body in its working.
        assert_eq!(structures(&s).len(), 2);

        // To an exercise: the working goes, the exercise has the same structure.
        let r = s.run("structure.mode", &json!({"labels": {"title": "Trave"}})).unwrap();
        assert_eq!(r["mode"], "exercise", "{r}");
        assert_eq!(exercises(&s), 1);
        let t = text(&s);
        assert!(t.starts_with("Before") && t.contains("Trave") && !t.contains("Svolgimento:"), "{t}");
        let found = structures(&s);
        assert_eq!(found[0].2.view, View::Structure);
        assert!(found.len() >= 3 && found.iter().all(|f| f.2.node("B").map(|n| n.x.value) == Some(8.0)));

        // Back, with the caret in the exercise: one structure with all its diagrams and its working.
        let title = s.doc.body.iter().position(|b| matches!(&**b, Block::Para(p) if p.text.contains("Trave"))).unwrap();
        s.sel = Selection::caret(Pos::new(StoryRef::Body, wordcraft_doc::Path(vec![title as u32]), 0));
        let r = s.run("structure.mode", &json!({})).unwrap();
        assert_eq!(r["mode"], "report", "{r}");
        assert_eq!(exercises(&s), 0);
        let found = structures(&s);
        assert_eq!(found.len(), 2);
        assert_eq!((found[0].2.view, found[1].2.view), (View::All, View::Equilibrium));
        assert_eq!(found[0].2.node("B").map(|n| n.x.value), Some(8.0));
        // The working's words kept in the structure come back.
        assert!(text(&s).contains("Svolgimento"), "{}", text(&s));
        assert!(text(&s).starts_with("Before"));

        // Already a report: nothing changes.
        select(&mut s, 0);
        let r = s.run("structure.mode", &json!({"mode": "report"})).unwrap();
        assert_eq!(r["changed"], false);
        assert!(matches!(s.run("structure.mode", &json!({"mode": "nope"})), Err(crate::CmdError::Params(_))));
    }

    #[test]
    fn mode_needs_a_structure() {
        let mut s = Session::new(Document::new());
        assert!(matches!(s.run("structure.mode", &json!({})), Err(crate::CmdError::Disabled(_))));
    }

    /// The equations' linear texts, in reading order.
    fn equations(s: &Session) -> Vec<String> {
        s.doc
            .body
            .iter()
            .filter_map(|b| if let Block::Para(p) = &**b { Some(p) } else { None })
            .flat_map(|p| p.objects.iter())
            .filter_map(|o| match o {
                InlineObject::Equation { linear, .. } => Some(linear.to_string()),
                _ => None,
            })
            .collect()
    }

    /// A beam whose load is the parameter `q`, defined by a calculation before it.
    fn parametric(s: &mut Session, q: &str) {
        s.run("calc.insert", &json!({"text": format!("q = {q}")})).unwrap();
        s.run("text.newParagraph", &json!({})).ok();
        let st = json!({"nodes": [{"id": "A", "x": 0, "y": 0}, {"id": "B", "x": 6, "y": 0}],
            "members": [{"id": "1", "a": "A", "b": "B"}],
            "supports": [{"node": "A", "kind": "pin"}, {"node": "B", "kind": "roller"}],
            "loads": [{"type": "distributed", "member": "1", "q1": "q", "angle": -90}]});
        s.run("structure.insert", &json!({"structure": st})).unwrap();
    }

    /// Sets the calculation `q = …` (the first paragraph's) to `q`, as typing it would.
    fn set_q(s: &mut Session, q: &str) {
        s.sel = Selection::caret(Pos::new(StoryRef::Body, wordcraft_doc::Path(vec![0]), 0));
        s.run("equation.set", &json!({"linear": format!("q = {q}")})).unwrap();
    }

    #[test]
    fn a_report_working_follows_its_parameters() {
        let mut s = Session::new(Document::new());
        parametric(&mut s, "10 kN/m");
        if !s.run("structure.working", &json!({})).is_ok() {
            panic!("{}", text(&s));
        }
        s.run("caret.docEnd", &json!({})).ok();
        let before = equations(&s).join("\n");
        assert!(before.contains("10"), "{before}");
        set_q(&mut s, "20 kN/m");
        let after = equations(&s).join("\n");
        assert_ne!(before, after);
        assert!(after.contains("60\u{2009}kN"), "the reactions follow q: {after}");
    }

    #[test]
    fn the_parameters_become_the_exercise_data_and_back() {
        let mut s = Session::new(Document::new());
        parametric(&mut s, "10 kN/m");
        s.run("structure.working", &json!({})).unwrap();
        let r = s.run("structure.mode", &json!({"mode": "exercise"})).unwrap();
        assert_eq!(r["data"], 1, "{r}");
        s.run("caret.docEnd", &json!({})).ok();
        let data = s.run("exercise.data", &json!({"exercise": 1})).unwrap();
        assert_eq!(data["data"]["items"][0]["symbol"], "q", "{data}");
        assert_eq!(data["data"]["items"][0]["values"][0], "10", "{data}");
        let before = equations(&s).join("\n");
        assert!(before.contains("30\u{2009}kN"), "{before}");

        // A datum changed: the exercise's working follows.
        let items = json!([{"symbol": "q", "unit": "kN/m", "values": ["20"]}]);
        s.run("exercise.setData", &json!({"exercise": 1, "data": {"items": items, "variants": ["A"], "active": 0}})).unwrap();
        let after = equations(&s).join("\n");
        assert!(after.contains("60\u{2009}kN"), "the reactions follow q: {after}");

        // Back to a report: the document defines q already (above), so the structure reads it.
        let title = s.doc.body.iter().position(|b| matches!(&**b, Block::Para(p) if p.text.contains("Structure"))).unwrap();
        s.sel = Selection::caret(Pos::new(StoryRef::Body, wordcraft_doc::Path(vec![title as u32]), 0));
        let r = s.run("structure.mode", &json!({})).unwrap();
        assert_eq!((r["data"].clone(), r["kept"].clone()), (json!(0), json!(["q"])), "{r}");
        assert!(equations(&s).join("\n").contains("30\u{2009}kN"));
    }

    #[test]
    fn an_exercise_data_become_calculations_before_the_report() {
        let st = json!({"nodes": [{"id": "A", "x": 0, "y": 0}, {"id": "B", "x": 6, "y": 0}],
            "members": [{"id": "1", "a": "A", "b": "B"}],
            "supports": [{"node": "A", "kind": "pin"}, {"node": "B", "kind": "roller"}],
            "loads": [{"type": "distributed", "member": "1", "q1": "q", "angle": -90}]});
        let mut s = Session::new(Document::new());
        s.run("structure.exercise", &json!({"structure": st})).unwrap();
        let items = json!([{"symbol": "q", "unit": "kN/m", "values": ["20"]}]);
        s.run("exercise.setData", &json!({"exercise": 1, "data": {"items": items, "variants": ["A"], "active": 0}})).unwrap();
        assert!(equations(&s).join("\n").contains("60\u{2009}kN"), "{:?}", equations(&s));
        let r = s.run("structure.mode", &json!({"mode": "report"})).unwrap();
        assert_eq!(r["data"], 1, "{r}");
        let eqs = equations(&s);
        assert!(eqs.first().is_some_and(|e| e.starts_with("q=20")), "{eqs:?}");
        assert!(eqs.iter().any(|e| e.contains("60\u{2009}kN")), "{eqs:?}");
        // The structure, and the free body in its working.
        assert_eq!(structures(&s).len(), 2);
        assert!(structures(&s)[0].0 > 0, "the structure after its data");
    }
}
