//! `structure.working`: the structure's solution written after it as live calculations
//! (`docs/structures.md`, Working).
//!
//! The reactions come from equilibrium equations written with the structure's numbers, each with
//! one unknown left, so calc solves it and shows its steps (`V_B·(6 m) − (10 kN/m)·(6 m)·(3 m) =
//! 0 kN·m ⇒ … ⇒ V_B = 30 kN`): the whole structure's translations and rotations about its
//! supports, and the rotation of a part about an internal hinge. An unknown no equation isolates
//! (a hyperstatic structure's redundants) is given its value from the stiffness solution, and said
//! so. Then, member by member and stretch by stretch, the laws `N(x)`, `T(x)`, `M(x)` as calc
//! functions (`x` from the member's first node) and their values at the stretch's ends and
//! extremes; a truss gets each member's axial force.

use serde_json::{Value, json};
use strix_structure::{Action, MemberResult, NotableKind, Resultant, ResultantKind, Status, Structure, SupportKind, notables, resultants, stretches};
use wordcraft_doc::math::{Math, parse_linear, to_linear};
use wordcraft_doc::para::Paragraph;
use wordcraft_doc::{Block, InlineObject};

use crate::{CmdError, CmdResult, Session};

/// At most this many lines are written.
const MAX_LINES: usize = 400;

/// The words of the working, from the interface (English by default).
struct Labels {
    heading: String,
    reactions: String,
    signs: String,
    hyperstatic: String,
    together: String,
    forces: String,
    conventions: String,
    member: String,
    axial: String,
    hinge: String,
    /// A stretch's extent: `{a}` and `{b}` its ends.
    range: String,
}

impl Labels {
    fn from(v: &Value) -> Labels {
        let get = |k: &str, d: &str| {
            v.get("labels").and_then(|l| l.get(k)).and_then(Value::as_str).map(|s| s.chars().take(200).collect()).unwrap_or_else(|| d.to_string())
        };
        Labels {
            heading: get("heading", "Working"),
            reactions: get("reactions", "Reactions"),
            signs: get("signs", "Reactions are positive along x, along y and counter-clockwise: a negative one acts the other way."),
            hyperstatic: get("hyperstatic", "Hyperstatic: from the solution by the stiffness method"),
            together: get("together", "From the equations solved together"),
            forces: get("forces", "Internal forces"),
            conventions: get(
                "conventions",
                "x from the member's first node; N positive in tension, T positive turning clockwise, M positive stretching the reference fibres.",
            ),
            member: get("member", "Member"),
            axial: get("axial", "Axial forces"),
            hinge: get("hinge", "part about the hinge"),
            range: get("range", "x from {a} to {b}"),
        }
    }
}

/// One line of the working: words, then equations (linear format, calculated).
#[derive(Debug, Clone, PartialEq)]
struct Line {
    label: String,
    /// A formula shown, not calculated, after the label (`ΣM_A = 0`, `0 m ≤ x ≤ 6 m`): in math,
    /// so its subscripts and symbols are typeset; then `after` in words.
    note: Option<String>,
    after: String,
    equations: Vec<String>,
    display: bool,
    /// Its equations' steps one under the other (a long equilibrium equation fits the page).
    stepped: bool,
}

impl Line {
    fn words(label: String) -> Line {
        Line { label, note: None, after: String::new(), equations: Vec::new(), display: false, stepped: false }
    }

    fn eqs(equations: Vec<String>, display: bool) -> Line {
        Line { label: String::new(), note: None, after: String::new(), equations, display, stepped: false }
    }

    fn noted(label: String, note: String, after: String) -> Line {
        Line { label, note: Some(note), after, equations: Vec::new(), display: false, stepped: false }
    }
}

pub(super) fn run(s: &mut Session, v: &Value) -> CmdResult {
    let (pos, src) = super::selected_source(s).ok_or_else(|| CmdError::Disabled("select a structure first".into()))?;
    let st = Structure::from_json(&src).map_err(|e| CmdError::Failed(format!("the structure can't be read: {e}")))?;
    let r = crate::cmd::calc::resolved_structure(s, &src)
        .unwrap_or_else(|| st.render(Some(&strix_calc::Worksheet::new(crate::cmd::calc::options_now(s)))));
    if !r.analysis.status.is_solved() {
        return Err(CmdError::Failed(format!("the structure can't be solved: {}", r.analysis.status.describe())));
    }
    let comma = crate::cmd::calc::options_now(s).decimal_comma;
    let lines = lines(&r.resolved, &r.analysis, &Labels::from(v), comma);
    let props = s.doc.para_at(&pos).map(|p| p.mark.clone()).unwrap_or_default();
    let mut path = pos.path.clone();
    for line in &lines {
        let mut para = Paragraph::with_text(&line.label, props.clone());
        if let Some(note) = &line.note {
            let math = Math { nodes: parse_linear(note), calc: false, ..Math::default() };
            let obj = InlineObject::Equation { linear: to_linear(&math.nodes), math, display: false };
            let at = para.text.len();
            para.insert_object(at, obj, &props).map_err(|e| CmdError::Failed(e.to_string()))?;
            let at = para.text.len();
            para.insert_text(at, &line.after, &props).map_err(|e| CmdError::Failed(e.to_string()))?;
        }
        let labelled = !line.label.is_empty() || line.note.is_some();
        for (k, eq) in line.equations.iter().enumerate() {
            if k > 0 || labelled {
                let at = para.text.len();
                // Wide spaces between equations: a `;` would read as one of their conditions.
                para.insert_text(at, if k > 0 { "\u{2003}\u{2003}" } else { " " }, &props).map_err(|e| CmdError::Failed(e.to_string()))?;
            }
            let calc_opts = line.stepped.then(|| r#"{"layout":"lines"}"#.to_string());
            let math = Math { nodes: parse_linear(eq), calc: true, calc_opts, ..Math::default() };
            let obj = InlineObject::Equation { linear: to_linear(&math.nodes), math, display: line.display };
            let at = para.text.len();
            para.insert_object(at, obj, &props).map_err(|e| CmdError::Failed(e.to_string()))?;
        }
        path = path.with_last(path.last().saturating_add(1));
        s.doc.insert_block(pos.story, &path, Block::Para(para)).map_err(|e| CmdError::Failed(e.to_string()))?;
    }
    s.touch();
    let report = crate::cmd::calc::recalculate_now(s);
    Ok(json!({ "lines": lines.len(), "recalculated": report }))
}

/// A number as calc reads it: six significant digits, no trailing zeros, the document's decimal
/// separator.
fn num(v: f64, comma: bool) -> String {
    if !v.is_finite() || v.abs() < 1e-12 {
        return "0".to_string();
    }
    let mag = v.abs().log10().floor() as i32;
    let decimals = (5 - mag).clamp(0, 9) as usize;
    let mut t = format!("{v:.decimals$}");
    if t.contains('.') {
        t = t.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    if t == "-0" {
        t = "0".to_string();
    }
    if comma { t.replace('.', ",") } else { t }
}

/// A quantity `|v| unit` as calc reads it, exact when it is a simple fraction: `(30 kN)`,
/// `(70 kN)/3`; else with eight significant digits.
fn qty(v: f64, unit: &str, comma: bool) -> String {
    let a = v.abs();
    let unit = if unit.is_empty() { String::new() } else { format!(" {unit}") };
    if a.is_finite() && a < 1e12 {
        for q in 1..=1000_u32 {
            let p = (a * f64::from(q)).round();
            if (a - p / f64::from(q)).abs() <= 1e-9 * a.max(1.0) {
                return if q == 1 { format!("({p}{unit})") } else { format!("({p}{unit})/{q}") };
            }
        }
    }
    format!("({}{unit})", precise(a, comma))
}

/// [`qty`] with its sign inside the parentheses (`(-25 kN)/2`): calc reads a definition
/// `N = -(25 kN)/2` as no number.
fn signed(v: f64, unit: &str, comma: bool) -> String {
    let t = qty(v, unit, comma);
    if v < 0.0 && t != "(0)" && !t.starts_with("(0 ") { t.replacen('(', "(-", 1) } else { t }
}

/// A function's argument: a quantity without the parentheses a plain one needs elsewhere.
fn arg(v: f64, unit: &str, comma: bool) -> String {
    let t = signed(v, unit, comma);
    match t.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
        Some(inner) if !inner.contains(')') => inner.to_string(),
        _ => t,
    }
}

/// A number with eight significant digits.
fn precise(v: f64, comma: bool) -> String {
    if !v.is_finite() || v.abs() < 1e-12 {
        return "0".to_string();
    }
    let mag = v.abs().log10().floor() as i32;
    let decimals = (7 - mag).clamp(0, 12) as usize;
    let mut t = format!("{v:.decimals$}");
    if t.contains('.') {
        t = t.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    if comma { t.replace('.', ",") } else { t }
}

/// A name's subscript from a node or member name: its letters and digits.
fn sub(id: &str, fallback: usize) -> String {
    let s: String = id.chars().filter(|c| c.is_alphanumeric()).take(8).collect();
    if s.is_empty() { fallback.to_string() } else { s }
}

/// What a reaction component does.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Acts {
    /// A force along this unit direction.
    Along((f64, f64)),
    /// A couple.
    Turns,
}

#[derive(Debug, Clone)]
struct Unknown {
    name: String,
    node: String,
    at: (f64, f64),
    acts: Acts,
    /// Its value from the solution (for a redundant).
    value: f64,
}

fn unknowns(st: &Structure, an: &strix_structure::Analysis) -> Vec<Unknown> {
    let mut out = Vec::new();
    for (k, r) in an.reactions.iter().enumerate() {
        let Some(n) = st.node(&r.node) else { continue };
        let at = (n.x.value, n.y.value);
        let s = sub(&r.node, k + 1);
        let mut push =
            |prefix: &str, acts: Acts, value: f64| out.push(Unknown { name: format!("{prefix}_{s}"), node: r.node.clone(), at, acts, value });
        match r.kind {
            SupportKind::Fixed | SupportKind::Pin => {
                push("H", Acts::Along((1.0, 0.0)), r.fx);
                push("V", Acts::Along((0.0, 1.0)), r.fy);
            }
            SupportKind::Roller | SupportKind::Link | SupportKind::Slider => {
                let phi = r.angle.to_radians();
                let n = (-phi.sin(), phi.cos());
                if n.0.abs() < 1e-9 {
                    push("V", Acts::Along((0.0, 1.0)), r.fy);
                } else if n.1.abs() < 1e-9 {
                    push("H", Acts::Along((1.0, 0.0)), r.fx);
                } else {
                    push("R", Acts::Along(n), r.fx * n.0 + r.fy * n.1);
                }
            }
        }
        if matches!(r.kind, SupportKind::Fixed | SupportKind::Slider) {
            push("M", Acts::Turns, r.m);
        }
    }
    out
}

/// An equilibrium equation: its unknowns' coefficients (with their terms' text) and its loads'
/// terms.
#[derive(Debug, Clone)]
struct Equation {
    /// Its name as a formula (`ΣM_A = 0`) and words after it.
    label: String,
    after: String,
    /// Moments (`0 kN·m = …`) or forces (`0 kN = …`).
    moment: bool,
    /// Unknown index, coefficient, text of the term without its sign.
    unknowns: Vec<(usize, f64, String)>,
    /// Signed value, text without its sign.
    loads: Vec<(f64, String)>,
}

/// Units of the structure as calc writes them.
struct Units {
    f: String,
    l: String,
    comma: bool,
}

impl Units {
    fn force(&self, v: f64) -> String {
        qty(v, &self.f, self.comma)
    }

    fn length(&self, v: f64) -> String {
        qty(v, &self.l, self.comma)
    }

    fn moment(&self, v: f64) -> String {
        qty(v, &format!("{}·{}", self.f, self.l), self.comma)
    }

    fn spread(&self, v: f64) -> String {
        qty(v, &format!("{}/{}", self.f, self.l), self.comma)
    }

    /// A direction's component as a factor: nothing for ±1.
    fn factor(&self, c: f64) -> String {
        if (c.abs() - 1.0).abs() < 1e-9 { String::new() } else { format!("·{}", num(c.abs(), self.comma)) }
    }

    /// The force a resultant amounts to, as text (positive).
    fn resultant(&self, r: &Resultant) -> String {
        match r.kind {
            ResultantKind::Force { value } => self.force(value),
            ResultantKind::Couple { value } => self.moment(value),
            ResultantKind::Spread { q, length, triangle } => {
                format!("{}·{}{}", self.spread(q), self.length(length), if triangle { "/2" } else { "" })
            }
        }
    }
}

const TOL: f64 = 1e-9;

/// Translation along `axis` (0: x, 1: y) of the loads and unknowns in `part` (all when `None`).
fn translation(axis: usize, us: &[Unknown], rs: &[Resultant], u: &Units, label: String) -> Equation {
    let mut eq = Equation { label, after: ":".to_string(), moment: false, unknowns: Vec::new(), loads: Vec::new() };
    for (i, k) in us.iter().enumerate() {
        if let Acts::Along(d) = k.acts {
            let c = if axis == 0 { d.0 } else { d.1 };
            if c.abs() > TOL {
                eq.unknowns.push((i, c, format!("{}{}", k.name, u.factor(c))));
            }
        }
    }
    for r in rs {
        if matches!(r.kind, ResultantKind::Couple { .. }) {
            continue;
        }
        let c = if axis == 0 { r.dir.0 } else { r.dir.1 };
        let v = r.value() * c;
        if v.abs() > TOL * r.value().abs().max(1e-12) && c.abs() > TOL {
            eq.loads.push((v, format!("{}{}", u.resultant(r), u.factor(c))));
        }
    }
    eq
}

/// Rotation about `p` of the loads `rs` and the unknowns `us` (counter-clockwise positive).
fn rotation(p: (f64, f64), us: &[(usize, &Unknown)], rs: &[&Resultant], u: &Units, label: String) -> Equation {
    let mut eq = Equation { label, after: ":".to_string(), moment: true, unknowns: Vec::new(), loads: Vec::new() };
    for &(i, k) in us {
        let (dx, dy) = (k.at.0 - p.0, k.at.1 - p.1);
        match k.acts {
            Acts::Turns => eq.unknowns.push((i, 1.0, k.name.clone())),
            Acts::Along(d) => {
                let arm = d.1 * dx - d.0 * dy;
                if arm.abs() > TOL * (1.0 + dx.abs() + dy.abs()) {
                    eq.unknowns.push((i, arm, format!("{}·{}", k.name, u.length(arm))));
                }
            }
        }
    }
    for r in rs {
        let (dx, dy) = (r.at.0 - p.0, r.at.1 - p.1);
        if let ResultantKind::Couple { value } = r.kind {
            eq.loads.push((value, u.resultant(r)));
            continue;
        }
        let v = r.value();
        // Fy·dx − Fx·dy, a term each.
        let fy = v * r.dir.1 * dx;
        if fy.abs() > TOL * v.abs().max(1e-12) * (1.0 + dx.abs()) {
            eq.loads.push((fy, format!("{}{}·{}", u.resultant(r), u.factor(r.dir.1), u.length(dx))));
        }
        let fx = -v * r.dir.0 * dy;
        if fx.abs() > TOL * v.abs().max(1e-12) * (1.0 + dy.abs()) {
            eq.loads.push((fx, format!("{}{}·{}", u.resultant(r), u.factor(r.dir.0), u.length(dy))));
        }
    }
    eq
}

impl Equation {
    fn text(&self, us: &[Unknown], u: &Units) -> String {
        let mut out = String::new();
        let push = |sign: f64, t: &str, out: &mut String| {
            if out.is_empty() {
                if sign < 0.0 {
                    out.push('-');
                }
            } else {
                out.push_str(if sign < 0.0 { " - " } else { " + " });
            }
            out.push_str(t);
        };
        for (i, c, t) in &self.unknowns {
            if us.get(*i).is_some() {
                push(c.signum(), t, &mut out);
            }
        }
        for (v, t) in &self.loads {
            push(v.signum(), t, &mut out);
        }
        if out.is_empty() {
            out.push('0');
        }
        let zero = if self.moment { format!("0 {}·{}", u.f, u.l) } else { format!("0 {}", u.f) };
        format!("{zero} = {out}")
    }

    /// The unknowns in it that are not known yet.
    fn open(&self, known: &[bool]) -> Vec<usize> {
        let big = self.unknowns.iter().map(|(_, c, _)| c.abs()).fold(0.0_f64, f64::max);
        self.unknowns.iter().filter(|(i, c, _)| c.abs() > 1e-9 * big && !known.get(*i).copied().unwrap_or(true)).map(|(i, _, _)| *i).collect()
    }
}

/// The parts a structure falls into at its internal hinges: for each hinge point, the members on
/// one side (those reached from one of its member ends without going through the point).
fn hinge_parts(st: &Structure) -> Vec<(String, Vec<usize>)> {
    // Vertices: (node, group); the member ends rigidly joined at a node share group 0, a hinged
    // end has a group of its own.
    let mut ends: Vec<[(usize, usize); 2]> = Vec::new();
    let mut groups: Vec<usize> = vec![1; st.nodes.len()];
    for m in &st.members {
        let (Some(a), Some(b)) = (st.node_index(&m.a), st.node_index(&m.b)) else { return Vec::new() };
        let mut end = |n: usize, hinged: bool| {
            let hinged = hinged || st.nodes.get(n).is_some_and(|x| x.hinge);
            if hinged {
                let g = groups.get(n).copied().unwrap_or(1);
                if let Some(slot) = groups.get_mut(n) {
                    *slot += 1;
                }
                (n, g)
            } else {
                (n, 0)
            }
        };
        let ea = end(a, m.hinge_a);
        let eb = end(b, m.hinge_b);
        ends.push([ea, eb]);
    }
    let mut out = Vec::new();
    for (p, node) in st.nodes.iter().enumerate() {
        // The groups meeting at p.
        let mut gs: Vec<usize> = ends.iter().flatten().filter(|(n, _)| *n == p).map(|(_, g)| *g).collect();
        gs.sort_unstable();
        gs.dedup();
        if gs.len() < 2 {
            continue;
        }
        for &g in &gs {
            // Members reached from (p, g) without other groups of p.
            let mut reached = vec![false; ends.len()];
            let mut frontier = vec![(p, g)];
            let mut seen: Vec<(usize, usize)> = vec![(p, g)];
            let mut closed_loop = false;
            while let Some(v) = frontier.pop() {
                for (k, e) in ends.iter().enumerate() {
                    if reached.get(k).copied().unwrap_or(true) {
                        continue;
                    }
                    let other = if e[0] == v {
                        e[1]
                    } else if e[1] == v {
                        e[0]
                    } else {
                        continue;
                    };
                    if let Some(r) = reached.get_mut(k) {
                        *r = true;
                    }
                    if other.0 == p && other.1 != g {
                        closed_loop = true;
                    }
                    // A rigid vertex stands for every rigid end at its node; hinged ends at other
                    // nodes still join the rest there through the node's translation.
                    let joined: Vec<(usize, usize)> =
                        if other.0 == p { vec![other] } else { ends.iter().flatten().filter(|x| x.0 == other.0).copied().collect() };
                    for j in joined {
                        if !seen.contains(&j) {
                            seen.push(j);
                            frontier.push(j);
                        }
                    }
                }
            }
            if closed_loop {
                continue;
            }
            let members: Vec<usize> = reached.iter().enumerate().filter(|(_, r)| **r).map(|(k, _)| k).collect();
            if !members.is_empty() && members.len() < st.members.len() {
                out.push((node.id.clone(), members));
            }
        }
    }
    out
}

fn lines(st: &Structure, an: &strix_structure::Analysis, labels: &Labels, comma: bool) -> Vec<Line> {
    let u = Units { f: st.force_unit.symbol().to_string(), l: st.length_unit.symbol().to_string(), comma };
    let mut out = vec![Line::words(labels.heading.clone()), Line::words(format!("{}:", labels.reactions))];
    let us = unknowns(st, an);
    let rs = resultants(st);
    // The candidate equations: rotations about the supports first, then the translations, then
    // the parts about the hinges.
    let mut eqs: Vec<Equation> = Vec::new();
    let all: Vec<(usize, &Unknown)> = us.iter().enumerate().collect();
    let all_rs: Vec<&Resultant> = rs.iter().collect();
    let mut points: Vec<String> = Vec::new();
    for k in &us {
        if !points.contains(&k.node) {
            points.push(k.node.clone());
        }
    }
    for id in &points {
        if let Some(n) = st.node(id) {
            eqs.push(rotation((n.x.value, n.y.value), &all, &all_rs, &u, format!("ΣM_{} = 0", sub(id, 0))));
        }
    }
    eqs.push(translation(0, &us, &rs, &u, "ΣF_x = 0".to_string()));
    eqs.push(translation(1, &us, &rs, &u, "ΣF_y = 0".to_string()));
    for (hinge, members) in hinge_parts(st) {
        let Some(h) = st.node(&hinge) else { continue };
        let in_part = |node: &str| members.iter().any(|&k| st.members.get(k).is_some_and(|m| m.a == node || m.b == node));
        // A support at the hinge itself is shared: the part's equation would be ambiguous.
        if us.iter().any(|k| k.node == hinge) {
            continue;
        }
        let part_us: Vec<(usize, &Unknown)> = us.iter().enumerate().filter(|(_, k)| in_part(&k.node)).collect();
        let part_rs: Vec<&Resultant> = rs
            .iter()
            .filter(|r| match st.loads.get(r.load) {
                Some(strix_structure::Load::Distributed { member, .. }) => {
                    members.iter().any(|&k| st.members.get(k).is_some_and(|m| &m.id == member))
                }
                Some(strix_structure::Load::Force { member: Some(m), .. } | strix_structure::Load::Moment { member: Some(m), .. }) => {
                    members.iter().any(|&k| st.members.get(k).is_some_and(|x| &x.id == m))
                }
                // A load on a node: in the part when the node is (at the hinge it has no arm, but a
                // couple there is ambiguous: left out).
                Some(strix_structure::Load::Force { node: Some(n), .. }) => n != &hinge && in_part(n),
                Some(strix_structure::Load::Moment { node: Some(n), .. }) => n != &hinge && in_part(n),
                _ => false,
            })
            .collect();
        let names: Vec<String> = members.iter().filter_map(|&k| st.members.get(k).map(|m| m.id.clone())).collect();
        let mut e = rotation((h.x.value, h.y.value), &part_us, &part_rs, &u, format!("ΣM_{} = 0", sub(&hinge, 0)));
        e.after = format!(" ({} {}, {} {}):", labels.hinge, hinge, labels.member, names.join(", "));
        eqs.push(e);
    }
    let mut known = vec![false; us.len()];
    let mut said_redundant = false;
    while known.iter().any(|k| !k) && out.len() < MAX_LINES {
        if let Some(pos) = eqs.iter().position(|e| e.open(&known).len() == 1) {
            let e = eqs.remove(pos);
            if let Some(&i) = e.open(&known).first()
                && let Some(slot) = known.get_mut(i)
            {
                *slot = true;
            }
            out.push(Line::noted(String::new(), e.label.clone(), e.after.clone()));
            out.push(Line { stepped: true, ..Line::eqs(vec![e.text(&us, &u)], true) });
            continue;
        }
        // No equation isolates an unknown: the first one left takes its value from the solution.
        let Some(i) = known.iter().position(|k| !k) else { break };
        if !said_redundant {
            let why = if matches!(an.status, Status::Hyperstatic { .. }) { &labels.hyperstatic } else { &labels.together };
            out.push(Line::words(format!("{why}:")));
            said_redundant = true;
        }
        if let (Some(k), Some(slot)) = (us.get(i), known.get_mut(i)) {
            *slot = true;
            let unit = match k.acts {
                Acts::Turns => format!("{}·{}", u.f, u.l),
                Acts::Along(_) => u.f.clone(),
            };
            out.push(Line::eqs(vec![format!("{} = {}", k.name, signed(k.value, &unit, comma))], false));
        }
    }
    out.push(Line::words(labels.signs.clone()));
    if an.truss {
        out.push(Line::words(format!("{}:", labels.axial)));
        let defs: Vec<String> = an
            .members
            .iter()
            .map(|m| {
                let n = m.forces(m.length * 0.5, true).n;
                format!("N_{} = {}", sub(&m.id, 0), signed(n, &u.f, comma))
            })
            .collect();
        for chunk in defs.chunks(4) {
            out.push(Line::eqs(chunk.to_vec(), false));
        }
        return out;
    }
    out.push(Line::words(format!("{}: {}", labels.forces, labels.conventions)));
    let shear = {
        let t: String = st.titles.v.chars().filter(|c| c.is_alphabetic()).take(2).collect();
        if t.is_empty() { "T".to_string() } else { t }
    };
    for m in &an.members {
        if out.len() >= MAX_LINES {
            break;
        }
        member_lines(st, m, &u, labels, &shear, &mut out);
    }
    out.truncate(MAX_LINES);
    out
}

/// A law `c0 + c1·x + …` with its terms' units (`units[k]` for the term in `x^k`).
fn law(c: &[f64; 4], units: [&str; 4], comma: bool) -> String {
    let mut out = String::new();
    for (k, &v) in c.iter().enumerate() {
        if v == 0.0 {
            continue;
        }
        let unit = units.get(k).copied().unwrap_or("");
        let first = out.is_empty();
        let term = format!(
            "{}{}",
            if first { signed(v, unit, comma) } else { qty(v, unit, comma) },
            match k {
                0 => String::new(),
                1 => "·x".to_string(),
                _ => format!("·x^{k}"),
            }
        );
        if !first {
            out.push_str(if v < 0.0 { " - " } else { " + " });
        }
        out.push_str(&term);
    }
    if out.is_empty() { format!("(0 {})", units.first().copied().unwrap_or("")) } else { out }
}

fn member_lines(st: &Structure, m: &MemberResult, u: &Units, labels: &Labels, shear: &str, out: &mut Vec<Line>) {
    let Some(src) = st.member(&m.id) else { return };
    let parts = stretches(m);
    let id = sub(&m.id, 0);
    let (f, l) = (u.f.as_str(), u.l.as_str());
    let fl = format!("{f}/{l}");
    let fl2 = format!("{f}/{l}^2");
    let fl3 = format!("{f}/{l}^3");
    let fm = format!("{f}·{l}");
    let extremes: Vec<f64> = notables(m, Action::Moment).into_iter().filter(|n| n.kind == NotableKind::Extreme).map(|n| n.x).collect();
    for (k, s) in parts.iter().enumerate() {
        let tag =
            if parts.len() > 1 { format!("{id}{}", char::from(b'a'.saturating_add(u8::try_from(k.min(25)).unwrap_or(25)))) } else { id.clone() };
        let at = |x: f64| arg(x, l, u.comma);
        let plain = |x: f64| format!("{} {l}", num(x, u.comma));
        let range = labels.range.replace("{a}", &plain(s.x0)).replace("{b}", &plain(s.x1));
        out.push(Line::words(format!("{} {} ({}–{}), {range}:", labels.member, m.id, src.a, src.b)));
        out.push(Line::eqs(vec![format!("N_{tag}(x) = {}", law(&s.n, [f, &fl, &fl2, &fl3], u.comma))], true));
        out.push(Line::eqs(vec![format!("{shear}_{tag}(x) = {}", law(&s.v, [f, &fl, &fl2, &fl3], u.comma))], true));
        out.push(Line::eqs(vec![format!("M_{tag}(x) = {}", law(&s.m, [&fm, f, &fl, &fl2], u.comma))], true));
        let mut values = Vec::new();
        if s.n[1..].iter().any(|c| *c != 0.0) {
            values.push(format!("N_{tag}({}) = {f}", at(s.x0)));
            values.push(format!("N_{tag}({}) = {f}", at(s.x1)));
        }
        values.push(format!("{shear}_{tag}({}) = {f}", at(s.x0)));
        values.push(format!("{shear}_{tag}({}) = {f}", at(s.x1)));
        values.push(format!("M_{tag}({}) = {fm}", at(s.x0)));
        for x in extremes.iter().filter(|x| **x > s.x0 + 1e-9 && **x < s.x1 - 1e-9) {
            values.push(format!("M_{tag}({}) = {fm}", at(*x)));
        }
        values.push(format!("M_{tag}({}) = {fm}", at(s.x1)));
        out.push(Line::eqs(values, false));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_as_calc_reads_them() {
        assert_eq!(num(36.666_666_6, false), "36.6667");
        assert_eq!(num(30.0, false), "30");
        assert_eq!(num(-2.5, true), "-2,5");
        assert_eq!(num(1e-15, false), "0");
        assert_eq!(law(&[0.0, 30.0, -5.0, 0.0], ["kN·m", "kN", "kN/m", "kN/m^2"], false), "(30 kN)·x - (5 kN/m)·x^2");
        assert_eq!(qty(-70.0 / 3.0, "kN", false), "(70 kN)/3");
        assert_eq!(qty(4.0, "m", false), "(4 m)");
        assert_eq!(signed(-12.5, "kN", false), "(-25 kN)/2");
        assert_eq!(arg(3.0, "m", false), "3 m");
        assert_eq!(arg(15.0 / 8.0, "m", false), "(15 m)/8");
        assert_eq!(law(&[-70.0 / 3.0, 0.0, 0.0, 0.0], ["kN", "", "", ""], false), "(-70 kN)/3");
        assert_eq!(qty(2.0_f64.sqrt(), "m", true), "(1,4142136 m)");
        assert_eq!(law(&[0.0; 4], ["kN", "", "", ""], false), "(0 kN)");
    }
}
