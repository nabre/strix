//! Edits of a whole structure that the sheet's context menu and grips make in one undoable step
//! (`structure.set`): splitting a member at a point, joining a node onto another, turning a load
//! round, moving a load onto another node or member. Each takes the structure as written (`src`,
//! whose numbers may be expressions) and as resolved (`st`, their values, for the geometry), and
//! keeps an expression one where it can (`L/2` on a member split at 2 → `L/2 - 2`).

use strix_structure::{Load, Num, Structure};

use super::{round, shifted};

/// How near (in the structure's units) two points are the same.
const EPS: f64 = 1e-9;

/// The world point at `at` along member `id` from its `a` end (resolved values).
pub(super) fn point_on(st: &Structure, id: &str, at: f64) -> Option<(f64, f64)> {
    let m = st.member(id)?;
    let (a, b) = (st.node(&m.a)?, st.node(&m.b)?);
    let len = st.length(m).filter(|l| *l > 0.0)?;
    let t = (at / len).clamp(0.0, 1.0);
    Some((a.x.value + (b.x.value - a.x.value) * t, a.y.value + (b.y.value - a.y.value) * t))
}

/// `src` with member `id` split `s` from its `a` end by a new node: the member keeps its name
/// from `a` to the node, a new one (same section) goes on to `b`, with the end hinge at `b`; its
/// loads go to the part they are on, a distributed load across the point in two pieces meeting at
/// the value it has there. Returns the structure and the new node's name; `None` when `s` is not
/// strictly inside the member.
pub(super) fn split_member(src: &Structure, st: &Structure, id: &str, s: f64) -> Option<(Structure, String)> {
    let len = st.member(id).and_then(|m| st.length(m))?;
    if !(s.is_finite() && s > EPS && s < len - EPS) {
        return None;
    }
    let (x, y) = point_on(st, id, s)?;
    let mut out = src.clone();
    let node = out.next_node_id();
    let second = out.next_member_id();
    out.nodes.push(strix_structure::Node { id: node.clone(), x: Num::new(round(x)), y: Num::new(round(y)), hinge: false });
    let i = out.members.iter().position(|m| m.id == id)?;
    let mut tail = out.members.get(i)?.clone();
    if let Some(head) = out.members.get_mut(i) {
        head.b = node.clone();
        head.hinge_b = false;
    }
    tail.id = second.clone();
    tail.a = node.clone();
    tail.hinge_a = false;
    out.members.insert(i + 1, tail);
    let mut loads = Vec::with_capacity(out.loads.len() + 1);
    for (k, l) in out.loads.iter().enumerate() {
        let resolved = st.loads.get(k);
        match (l, resolved) {
            (
                Load::Force { member: Some(m), at, .. } | Load::Moment { member: Some(m), at, .. },
                Some(Load::Force { at: rat, .. } | Load::Moment { at: rat, .. }),
            ) if m == id && rat.value > s + EPS => {
                let mut moved = l.clone();
                if let Load::Force { member, at: a, .. } | Load::Moment { member, at: a, .. } = &mut moved {
                    *member = Some(second.clone());
                    *a = shifted(at, -s);
                }
                loads.push(moved);
            }
            (Load::Distributed { member, then, .. }, _) if !then.is_empty() && (member == id || then.iter().any(|m| m == id)) => {
                // Over several members: the new one goes into its line where the member was
                // (the line, and where the load lies along it, stay the same).
                let line = st.load_line(l);
                let mut moved = l.clone();
                if let (Some(line), Load::Distributed { member, then, .. }) = (line, &mut moved) {
                    let mut ids: Vec<String> = Vec::with_capacity(line.parts.len() + 1);
                    for p in &line.parts {
                        if p.member == id {
                            // Its `a` end side first unless it runs against the line.
                            if p.reversed {
                                ids.push(second.clone());
                                ids.push(p.member.clone());
                            } else {
                                ids.push(p.member.clone());
                                ids.push(second.clone());
                            }
                        } else {
                            ids.push(p.member.clone());
                        }
                    }
                    let mut it = ids.into_iter();
                    if let Some(first) = it.next() {
                        *member = first;
                        *then = it.collect();
                    }
                }
                loads.push(moved);
            }
            (Load::Distributed { member, q1, q2, from, to, .. }, Some(Load::Distributed { q1: rq1, q2: rq2, from: rf, to: rt, .. }))
                if member == id =>
            {
                let (f, t) = (rf.value, rt.as_ref().map_or(len, |t| t.value));
                if t <= s + EPS {
                    loads.push(l.clone());
                } else if f >= s - EPS {
                    let mut moved = l.clone();
                    if let Load::Distributed { member, from: mf, to: mt, .. } = &mut moved {
                        *member = second.clone();
                        *mf = if (f - s).abs() <= EPS { Num::new(0.0) } else { shifted(from, -s) };
                        *mt = to.as_ref().map(|t| shifted(t, -s));
                    }
                    loads.push(moved);
                } else {
                    // Across the point: the value there, where the two pieces meet.
                    let q_s = match rq2 {
                        Some(rq2) if t - f > EPS => Some(Num::new(round(rq1.value + (rq2.value - rq1.value) * (s - f) / (t - f)))),
                        _ => None,
                    };
                    let mut head = l.clone();
                    if let Load::Distributed { to: ht, q2: hq2, .. } = &mut head {
                        *ht = Some(Num::new(round(s)));
                        if let Some(q) = &q_s {
                            *hq2 = Some(q.clone());
                        }
                    }
                    let mut tail = l.clone();
                    if let Load::Distributed { member: tm, from: tf, to: tt, q1: tq1, q2: tq2, .. } = &mut tail {
                        *tm = second.clone();
                        *tf = Num::new(0.0);
                        *tt = to.as_ref().map(|t| shifted(t, -s));
                        if let Some(q) = q_s {
                            *tq1 = q;
                            *tq2 = q2.clone();
                        } else {
                            *tq1 = q1.clone();
                        }
                    }
                    loads.push(head);
                    loads.push(tail);
                }
            }
            _ => loads.push(l.clone()),
        }
    }
    out.loads = loads;
    Some((out, node))
}

/// `src` with node `from` joined onto node `onto`: what used `from` (members, its support, its
/// loads) uses `onto`, which keeps its own support if it has one; a member left with both ends on
/// `onto`, or doubling another between the same nodes, goes with its loads. `None` when either
/// node is missing or they are the same.
pub(super) fn merge_nodes(src: &Structure, from: &str, onto: &str) -> Option<Structure> {
    if from == onto || src.node(from).is_none() || src.node(onto).is_none() {
        return None;
    }
    let mut out = src.clone();
    out.nodes.retain(|n| n.id != from);
    for m in &mut out.members {
        if m.a == from {
            m.a = onto.to_string();
        }
        if m.b == from {
            m.b = onto.to_string();
        }
    }
    let mut seen: Vec<(String, String)> = Vec::new();
    let mut gone: Vec<String> = Vec::new();
    out.members.retain(|m| {
        let pair = if m.a <= m.b { (m.a.clone(), m.b.clone()) } else { (m.b.clone(), m.a.clone()) };
        let keep = m.a != m.b && !seen.contains(&pair);
        if keep {
            seen.push(pair);
        } else {
            gone.push(m.id.clone());
        }
        keep
    });
    if out.supports.iter().any(|s| s.node == onto) {
        out.supports.retain(|s| s.node != from);
    } else {
        for s in &mut out.supports {
            if s.node == from {
                s.node = onto.to_string();
            }
        }
    }
    out.loads.retain_mut(|l| match l {
        Load::Force { node, member, .. } | Load::Moment { node, member, .. } => {
            if node.as_deref() == Some(from) {
                *node = Some(onto.to_string());
            }
            !member.as_ref().is_some_and(|m| gone.contains(m))
        }
        Load::Distributed { member, then, .. } => !gone.contains(member) && !then.iter().any(|m| gone.contains(m)),
    });
    Some(out)
}

/// Why two members cannot be joined at a node (shown on the disabled menu item).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NoJoin {
    /// The node does not join exactly two members.
    NotTwo,
    /// The two members are not on one straight line.
    NotAligned,
    /// The node has a support.
    Support,
    /// A hinge at the node (the node's, or a member's end there).
    Hinge,
    /// The members have different sections.
    Section,
}

impl NoJoin {
    /// The reason, in English (translated by the caller).
    pub(super) fn reason(self) -> &'static str {
        match self {
            Self::NotTwo => "The node must join exactly two members",
            Self::NotAligned => "The two members are not aligned",
            Self::Support => "The node has a support",
            Self::Hinge => "There is a hinge at the node",
            Self::Section => "The members have different sections",
        }
    }
}

/// The node two members `p` and `q` share, when they share exactly one.
pub(super) fn shared_node(st: &Structure, p: &str, q: &str) -> Option<String> {
    let (p, q) = (st.member(p)?, st.member(q)?);
    let ends = |m: &strix_structure::Member| [m.a.clone(), m.b.clone()];
    let common: Vec<String> = ends(p).into_iter().filter(|e| ends(q).contains(e)).collect();
    match common.as_slice() {
        [one] if p.id != q.id => Some(one.clone()),
        _ => None,
    }
}

/// `src` with the two aligned members meeting at `node` joined into one and `node` removed (the
/// inverse of [`split_member`]): the member that comes first along the line keeps its name and
/// section and runs on to the other's far end, with its hinge there; the other's loads move onto
/// it (`at` kept as an expression plus the first one's length), a force or couple at the node
/// goes onto it where the node was. A member running the other way is turned round first (its
/// loads measured from the other end, values at that point). `Err` says why it cannot be done.
pub(super) fn join_members(src: &Structure, st: &Structure, node: &str) -> Result<Structure, NoJoin> {
    let at_node: Vec<&strix_structure::Member> = st.members.iter().filter(|m| m.a == node || m.b == node).collect();
    let [p, q] = at_node.as_slice() else { return Err(NoJoin::NotTwo) };
    if p.a == p.b || q.a == q.b {
        return Err(NoJoin::NotTwo);
    }
    if st.supports.iter().any(|s| s.node == node) {
        return Err(NoJoin::Support);
    }
    let hinged_here = |m: &strix_structure::Member| (m.a == node && m.hinge_a) || (m.b == node && m.hinge_b);
    if st.node(node).is_some_and(|n| n.hinge) || hinged_here(p) || hinged_here(q) {
        return Err(NoJoin::Hinge);
    }
    if (&p.e, &p.i, &p.area) != (&q.e, &q.i, &q.area) {
        return Err(NoJoin::Section);
    }
    let far = |m: &strix_structure::Member| if m.a == node { m.b.clone() } else { m.a.clone() };
    let (fp, fq) = (far(p), far(q));
    let xy = |id: &str| st.node(id).map(|n| (n.x.value, n.y.value));
    let (Some(o), Some(a), Some(b)) = (xy(node), xy(&fp), xy(&fq)) else { return Err(NoJoin::NotTwo) };
    if fp == fq {
        return Err(NoJoin::NotAligned);
    }
    // Aligned: the far ends on opposite sides of the node, on one line (sine of the angle
    // between the two directions within 1e-6).
    let (u, v) = ((a.0 - o.0, a.1 - o.1), (b.0 - o.0, b.1 - o.1));
    let (lu, lv) = (u.0.hypot(u.1), v.0.hypot(v.1));
    if !(lu > EPS && lv > EPS && lu.is_finite() && lv.is_finite()) {
        return Err(NoJoin::NotAligned);
    }
    let cross = (u.0 * v.1 - u.1 * v.0) / (lu * lv);
    let dot = (u.0 * v.0 + u.1 * v.1) / (lu * lv);
    if cross.abs() > 1e-6 || dot > -0.5 {
        return Err(NoJoin::NotAligned);
    }
    // The first member along the line: the one ending at the node, else (both start there) the
    // one listed first, turned round.
    let (first, second) = match (p.b == node, q.b == node) {
        (true, _) => (*p, *q),
        (false, true) => (*q, *p),
        (false, false) => (*p, *q),
    };
    let flip_first = first.b != node;
    let flip_second = second.a != node;
    let len1 = st.length(first).unwrap_or(lu);
    let len2 = st.length(second).unwrap_or(lv);
    let (id1, id2) = (first.id.clone(), second.id.clone());
    // A load over several members, the two among them, in pieces first (one per member).
    let (src, st) = (&pieces_on(src, st, &[&id1, &id2]), &pieces_on(st, st, &[&id1, &id2]));
    let mut out = src.clone();
    let hinge_b = if flip_second { second.hinge_a } else { second.hinge_b };
    let start_hinge = if flip_first { first.hinge_b } else { first.hinge_a };
    let start = if flip_first { first.b.clone() } else { first.a.clone() };
    let end = far(second);
    if let Some(m) = out.members.iter_mut().find(|m| m.id == id1) {
        m.a = start;
        m.b = end;
        m.hinge_a = start_hinge;
        m.hinge_b = hinge_b;
    }
    out.members.retain(|m| m.id != id2);
    out.nodes.retain(|n| n.id != node);
    let mut loads = Vec::with_capacity(out.loads.len());
    for (k, l) in src.loads.iter().enumerate() {
        let r = st.loads.get(k).unwrap_or(l);
        let mut l = l.clone();
        match (&mut l, r) {
            (Load::Force { node: n, member, at, .. } | Load::Moment { node: n, member, at, .. }, _) if n.as_deref() == Some(node) => {
                *n = None;
                *member = Some(id1.clone());
                *at = Num::new(round(len1));
            }
            (
                Load::Force { member: Some(m), at, .. } | Load::Moment { member: Some(m), at, .. },
                Load::Force { at: ra, .. } | Load::Moment { at: ra, .. },
            ) if *m == id1 && flip_first => {
                *at = Num::new(round(len1 - ra.value));
            }
            (
                Load::Force { member: Some(m), at, .. } | Load::Moment { member: Some(m), at, .. },
                Load::Force { at: ra, .. } | Load::Moment { at: ra, .. },
            ) if *m == id2 => {
                *m = id1.clone();
                *at = if flip_second { Num::new(round(len1 + len2 - ra.value)) } else { shifted(at, len1) };
            }
            (
                Load::Distributed { member, q1, q2, from, to, angle, axes, .. },
                Load::Distributed { q1: rq1, q2: rq2, from: rf, to: rt, angle: ra, .. },
            ) if *member == id1 || *member == id2 => {
                let on_second = *member == id2;
                let (len, flip, base) = if on_second { (len2, flip_second, len1) } else { (len1, flip_first, 0.0) };
                if flip {
                    // Turned round: measured from the other end, its two values swapped, a
                    // direction in the member's axes turned half round.
                    let (f, t) = (rf.value, rt.as_ref().map_or(len, |t| t.value));
                    *from = Num::new(round(base + len - t));
                    *to = Some(Num::new(round(base + len - f)));
                    if let Some(rq2) = rq2 {
                        *q1 = Num::new(rq2.value);
                        *q2 = Some(Num::new(rq1.value));
                    }
                    if *axes == strix_structure::LoadAxes::Local {
                        let mut d = ra.value + 180.0;
                        if d > 180.0 {
                            d -= 360.0;
                        }
                        *angle = Num::new(round(d));
                    }
                } else if on_second {
                    *from = if rf.value.abs() <= EPS { Num::new(round(base)) } else { shifted(from, base) };
                    *to = to.as_ref().map(|t| shifted(t, base));
                } else if to.is_none() {
                    // It ended at the member's end: it ends where the node was.
                    *to = Some(Num::new(round(len1)));
                }
                *member = id1.clone();
                // Up to the joined member's end: "to the end" again.
                if to.as_ref().is_some_and(|t| (st_value(t, rt, on_second, flip, base, len) - (len1 + len2)).abs() <= EPS) {
                    *to = None;
                }
            }
            _ => {}
        }
        loads.push(l);
    }
    out.loads = loads;
    Ok(out)
}

/// The value a distributed load's new `to` has (`t` as just written: a plain number, or the
/// old expression shifted by `base`, whose value is the old one's plus `base`).
fn st_value(t: &Num, old: &Option<Num>, on_second: bool, flip: bool, base: f64, len: f64) -> f64 {
    if t.expr.is_none() || flip {
        return t.value;
    }
    let old = old.as_ref().map_or(len, |o| o.value);
    if on_second { old + base } else { old }
}

/// `n` negated: `-n`, an expression as `-(e)`, and `-(e)` back to `e`.
pub(super) fn negated(n: &Num) -> Num {
    match &n.expr {
        None => Num::new(-n.value),
        Some(e) => {
            let t = e.trim();
            match t.strip_prefix("-(").and_then(|r| r.strip_suffix(')')) {
                Some(inner) if balanced(inner) => Num { value: -n.value, expr: Some(inner.to_string()) },
                _ => Num { value: -n.value, expr: Some(format!("-({t})")) },
            }
        }
    }
}

/// Whether the brackets of `s` close in order (so `-(a) + (b)` is not taken as `-(…)`).
fn balanced(s: &str) -> bool {
    let mut depth = 0i32;
    for c in s.chars() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// Load `k` acting the other way: its value (and a varying load's other end) negated.
pub(super) fn reversed(src: &Structure, k: usize) -> Option<Load> {
    let mut l = src.loads.get(k)?.clone();
    match &mut l {
        Load::Force { value, .. } | Load::Moment { value, .. } => *value = negated(value),
        Load::Distributed { q1, q2, .. } => {
            *q1 = negated(q1);
            if let Some(q2) = q2 {
                *q2 = negated(q2);
            }
        }
    }
    Some(l)
}

/// Where a point load is put: at a node, or on a member at a distance from its `a` end.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Place {
    Node(String),
    Member(String, f64),
}

/// Point load `k` (a force or a couple) moved to `place`; on its own member, its `at` keeps its
/// expression (`L/2` → `L/2 + 0.5`). `None` for a distributed load or when it did not move.
pub(super) fn placed(src: &Structure, st: &Structure, k: usize, place: &Place) -> Option<Load> {
    let mut l = src.loads.get(k)?.clone();
    let (cur_member, cur_at) = match st.loads.get(k)? {
        Load::Force { member, at, .. } | Load::Moment { member, at, .. } => (member.clone(), at.value),
        Load::Distributed { .. } => return None,
    };
    let (Load::Force { node, member, at, .. } | Load::Moment { node, member, at, .. }) = &mut l else { return None };
    match place {
        Place::Node(id) => {
            if node.as_deref() == Some(id.as_str()) && member.is_none() {
                return None;
            }
            *node = Some(id.clone());
            *member = None;
            *at = Num::new(0.0);
        }
        Place::Member(id, s) => {
            if cur_member.as_deref() == Some(id.as_str()) {
                if (s - cur_at).abs() <= EPS {
                    return None;
                }
                *at = shifted(at, s - cur_at);
            } else {
                *at = Num::new(round(*s));
            }
            *node = None;
            *member = Some(id.clone());
        }
    }
    Some(l)
}

/// The direction (degrees counter-clockwise from the horizontal, in (−180, 180]) of a force whose
/// arrow points from its tail to its point, given that vector in world units, for a force of the
/// sign of `value` (a negative force's arrow points against its angle); turned to a multiple of
/// `step` degrees when `step` is positive.
#[cfg(test)]
pub(super) fn force_angle(dx: f64, dy: f64, value: f64, step: f64) -> Option<f64> {
    if !(dx.is_finite() && dy.is_finite()) || dx.hypot(dy) <= EPS {
        return None;
    }
    let mut a = dy.atan2(dx).to_degrees();
    if value < 0.0 {
        a += 180.0;
    }
    if step > 0.0 {
        a = (a / step).round() * step;
    }
    while a > 180.0 {
        a -= 360.0;
    }
    while a <= -180.0 {
        a += 360.0;
    }
    Some(round(a))
}

/// `src` with each load over several members, one of `ids` among them, in one piece per member
/// (values of `st`, the resolved structure).
fn pieces_on(src: &Structure, st: &Structure, ids: &[&str]) -> Structure {
    let mut out = src.clone();
    out.loads.clear();
    for (k, l) in src.loads.iter().enumerate() {
        match (l, st.loads.get(k)) {
            (Load::Distributed { then, .. }, Some(r)) if !then.is_empty() && st.load_members(r).iter().any(|m| ids.contains(m)) => {
                out.loads.extend(st.spread_pieces(r));
            }
            _ => out.loads.push(l.clone()),
        }
    }
    out
}

/// The straight run of aligned members distributed load `k` lies in (resolved values), read in
/// the load's direction, and where the load's line starts along it.
pub(super) fn run_of(st: &Structure, k: usize) -> Option<(strix_structure::SpreadLine, f64)> {
    let line = st.load_line(st.loads.get(k)?)?;
    let first = line.parts.first()?;
    let (head, then, at) = st.straight_run(&first.member, first.reversed)?;
    let run = st.spread_line(&head, &then)?;
    // A run of one member reads it from `a` to `b`: the load's own member, the same way.
    Some((run, at))
}

/// Where distributed load `k` lies along its straight run (resolved values): from, to.
pub(super) fn on_run(st: &Structure, k: usize) -> Option<(strix_structure::SpreadLine, f64, f64)> {
    let (run, at) = run_of(st, k)?;
    let Some(Load::Distributed { member, then, from, to, .. }) = st.loads.get(k) else { return None };
    let line = st.spread_line(member, then)?;
    let (f, t) = (from.value.clamp(0.0, line.len), to.as_ref().map_or(line.len, |t| t.value).clamp(0.0, line.len));
    let (f, t) = if f <= t { (f, t) } else { (t, f) };
    Some((run, at + f, at + t))
}

/// Distributed load `k` laid from `from` to `to` along its straight run of aligned members
/// (resolved values, clamped to the run, at least `gap` long): it goes over the members that
/// stretch covers, from the first of them. Numbers written as expressions stay ones while the
/// load keeps its way along the line; a load left on one member running against the line is
/// measured from that member's `a` end, its two values swapped. `None` when nothing changed.
pub(super) fn respread(src: &Structure, st: &Structure, k: usize, from: f64, to: f64, gap: f64) -> Option<Load> {
    let (run, _) = run_of(st, k)?;
    let Some(Load::Distributed { member: rm, then: rthen, from: rf, to: rt, .. }) = st.loads.get(k) else { return None };
    let old_line = st.spread_line(rm, rthen)?;
    let (cf, ct) = (rf.value, rt.as_ref().map_or(old_line.len, |t| t.value));
    let gap = gap.clamp(EPS, run.len);
    let f = from.clamp(0.0, run.len - gap);
    let t = to.clamp(f + gap, run.len);
    let covered: Vec<&strix_structure::LinePart> = run.parts.iter().filter(|p| t.min(p.at + p.len) - f.max(p.at) > EPS).collect();
    let (first, rest) = covered.split_first()?;
    let ids: Vec<String> = covered.iter().map(|p| p.member.clone()).collect();
    let base = first.at;
    let (nf, nt) = (f - base, t - base);
    let len = covered.iter().map(|p| p.len).sum::<f64>();
    let same = ids.first() == Some(rm) && ids.get(1..).is_some_and(|r| r == rthen.as_slice());
    if same && (nf - cf).abs() <= EPS && (nt - ct).abs() <= EPS {
        return None;
    }
    let mut l = src.loads.get(k)?.clone();
    let Load::Distributed { member, then, q1, q2, from: lf, to: lt, angle, axes, .. } = &mut l else { return None };
    if rest.is_empty() && first.reversed {
        // One member running against the line: measured from its `a` end.
        let (r0, r1) = (first.len - nt, first.len - nf);
        *member = first.member.clone();
        then.clear();
        *lf = Num::new(if r0 <= EPS { 0.0 } else { round(r0) });
        *lt = if (first.len - r1).abs() <= EPS { None } else { Some(Num::new(round(r1))) };
        if let Some(q) = q2.as_mut() {
            std::mem::swap(q1, q);
        }
        if *axes == strix_structure::LoadAxes::Local {
            let a = angle.value + 180.0;
            *angle = Num::new(round(if a > 180.0 { a - 360.0 } else { a }));
        }
        return Some(l);
    }
    // Shifted by where they land on the new line (`cf`, `ct`: where they were on the old one).
    *member = first.member.clone();
    *then = rest.iter().map(|p| p.member.clone()).collect();
    *lf = if nf <= EPS {
        Num::new(0.0)
    } else if (nf - cf).abs() <= EPS {
        lf.clone()
    } else {
        shifted(&lf.clone(), nf - cf)
    };
    *lt = if (len - nt).abs() <= EPS {
        None
    } else {
        Some(match lt.as_ref() {
            Some(n) if (nt - ct).abs() <= EPS => n.clone(),
            Some(n) => shifted(n, nt - ct),
            None => Num::new(round(nt)),
        })
    };
    Some(l)
}

/// `src` with member `id` turned round (`a` ⇄ `b`, `hingeA` ⇄ `hingeB`): its loads stay at the
/// same world positions and keep the same directions (`len − at`, `q1` ⇄ `q2`, local direction
/// turned by 180°).
pub(super) fn reverse_member(src: &Structure, st: &Structure, id: &str) -> Option<Structure> {
    let rm = st.member(id)?;
    let len = st.length(rm).filter(|l| *l > 0.0)?;
    let mut out = src.clone();
    let m = out.members.iter_mut().find(|m| m.id == id)?;
    std::mem::swap(&mut m.a, &mut m.b);
    std::mem::swap(&mut m.hinge_a, &mut m.hinge_b);
    for (k, l) in out.loads.iter_mut().enumerate() {
        let Some(rl) = st.loads.get(k) else { continue };
        match (l, rl) {
            (
                Load::Force { member: Some(mid), at, .. } | Load::Moment { member: Some(mid), at, .. },
                Load::Force { at: rat, .. } | Load::Moment { at: rat, .. },
            ) if mid == id => {
                *at = Num::new(round((len - rat.value).clamp(0.0, len)));
            }
            (
                Load::Distributed { member, then, q1, q2, from, to, angle, axes, .. },
                Load::Distributed { q1: rq1, q2: rq2, from: rf, to: rt, angle: ra, .. },
            ) if member == id && then.is_empty() => {
                let (f, t) = (rf.value.clamp(0.0, len), rt.as_ref().map_or(len, |t| t.value).clamp(0.0, len));
                let nf = round((len - t).max(0.0));
                let nt = round((len - f).min(len));
                *from = Num::new(nf);
                *to = if (nt - len).abs() <= EPS { None } else { Some(Num::new(nt)) };
                if let Some(rq2) = rq2 {
                    *q1 = Num::new(rq2.value);
                    *q2 = Some(Num::new(rq1.value));
                }
                if *axes == strix_structure::LoadAxes::Local {
                    let mut d = ra.value + 180.0;
                    if d > 180.0 {
                        d -= 360.0;
                    }
                    *angle = Num::new(round(d));
                }
            }
            _ => {}
        }
    }
    Some(out)
}

/// The nodes and members joined to `start` through members (a node, or a member's ends): the
/// connected part of the structure it is in.
pub(super) fn connected(st: &Structure, start: &[String]) -> (Vec<String>, Vec<String>) {
    let mut nodes: Vec<String> = start.to_vec();
    let mut members: Vec<String> = Vec::new();
    let mut i = 0;
    // Each node is visited once; the walk ends when no new node is found.
    while let Some(id) = nodes.get(i).cloned() {
        for m in st.members.iter().filter(|m| m.a == id || m.b == id) {
            if !members.contains(&m.id) {
                members.push(m.id.clone());
            }
            for end in [&m.a, &m.b] {
                if !nodes.contains(end) {
                    nodes.push(end.clone());
                }
            }
        }
        i += 1;
    }
    (nodes, members)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn beam() -> Structure {
        Structure::from_json(
            r#"{"nodes":[{"id":"A","x":0,"y":0},{"id":"B","x":"L","y":0}],
                "members":[{"id":"1","a":"A","b":"B","hingeB":true}],
                "supports":[{"node":"A","kind":"pin"},{"node":"B","kind":"roller"}],
                "loads":[{"type":"force","member":"1","at":"L/2","value":10},
                         {"type":"force","member":"1","at":1,"value":5},
                         {"type":"distributed","member":"1","q1":0,"q2":6},
                         {"type":"moment","node":"B","value":3}]}"#,
        )
        .unwrap()
    }

    /// `src` with its expressions' values (L = 6).
    fn resolved(src: &Structure) -> Structure {
        let mut st = src.clone();
        for n in &mut st.nodes {
            if n.x.expr.as_deref() == Some("L") {
                n.x.value = 6.0;
            }
        }
        if let Some(Load::Force { at, .. }) = st.loads.get_mut(0) {
            at.value = 3.0;
        }
        st
    }

    #[test]
    fn a_split_member_shares_its_loads_and_keeps_expressions() {
        let src = beam();
        let st = resolved(&src);
        let (out, node) = split_member(&src, &st, "1", 2.0).unwrap();
        assert_eq!(node, "C");
        let c = out.node("C").unwrap();
        assert_eq!((c.x.value, c.y.value), (2.0, 0.0));
        let (m1, m2) = (out.member("1").unwrap(), out.member("2").unwrap());
        assert_eq!((m1.a.as_str(), m1.b.as_str(), m1.hinge_b), ("A", "C", false));
        assert_eq!((m2.a.as_str(), m2.b.as_str(), m2.hinge_a, m2.hinge_b), ("C", "B", false, true));
        // The force at L/2 (3) goes on to the second part, its position still an expression.
        match &out.loads[0] {
            Load::Force { member, at, .. } => {
                assert_eq!(member.as_deref(), Some("2"));
                assert_eq!(at.expr.as_deref(), Some("L/2 - 2"));
            }
            other => panic!("{other:?}"),
        }
        // The one at 1 stays on the first.
        assert!(matches!(&out.loads[1], Load::Force { member: Some(m), at, .. } if m == "1" && at.value == 1.0));
        // The triangular load (0 → 6 over 6) splits at q = 2.
        match (&out.loads[2], &out.loads[3]) {
            (
                Load::Distributed { member: a, q1: aq1, q2: aq2, to: at, .. },
                Load::Distributed { member: b, q1: bq1, q2: bq2, from: bf, to: bt, .. },
            ) => {
                assert_eq!((a.as_str(), aq1.value, aq2.as_ref().map(|q| q.value), at.as_ref().map(|t| t.value)), ("1", 0.0, Some(2.0), Some(2.0)));
                assert_eq!((b.as_str(), bq1.value, bq2.as_ref().map(|q| q.value), bf.value, bt.is_none()), ("2", 2.0, Some(6.0), 0.0, true));
            }
            other => panic!("{other:?}"),
        }
        assert!(split_member(&src, &st, "1", 0.0).is_none(), "not at an end");
        assert!(split_member(&src, &st, "1", 6.0).is_none(), "not at an end");
        assert!(split_member(&src, &st, "1", f64::NAN).is_none());
    }

    #[test]
    fn a_node_joined_onto_another_takes_what_used_it() {
        let src = Structure::from_json(
            r#"{"nodes":[{"id":"A","x":0,"y":0},{"id":"B","x":4,"y":0},{"id":"C","x":4,"y":0.1},{"id":"D","x":8,"y":0}],
                "members":[{"id":"1","a":"A","b":"B"},{"id":"2","a":"C","b":"D"},{"id":"3","a":"B","b":"C"},{"id":"4","a":"A","b":"C"}],
                "supports":[{"node":"C","kind":"pin"}],
                "loads":[{"type":"force","node":"C","value":1},{"type":"distributed","member":"3","q1":1},{"type":"force","member":"4","at":1,"value":2}]}"#,
        )
        .unwrap();
        let out = merge_nodes(&src, "C", "B").unwrap();
        assert!(out.node("C").is_none());
        // 3 (B–C) shrinks to nothing, 4 (A–C) doubles 1 (A–B): both go, with their loads.
        let ids: Vec<&str> = out.members.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["1", "2"]);
        assert_eq!(out.member("2").unwrap().a, "B");
        assert_eq!(out.supports[0].node, "B");
        assert_eq!(out.loads.len(), 1);
        assert!(matches!(&out.loads[0], Load::Force { node: Some(n), .. } if n == "B"));
        assert!(merge_nodes(&src, "B", "B").is_none());
        assert!(merge_nodes(&src, "Z", "B").is_none());
    }

    #[test]
    fn joining_undoes_a_split() {
        let src = beam();
        let st = resolved(&src);
        let (split, node) = split_member(&src, &st, "1", 2.0).unwrap();
        // The split structure's values (L = 6; the moved force's `L/2 - 2` is 1).
        let mut st2 = resolved(&split);
        if let Some(Load::Force { at, .. }) = st2.loads.get_mut(0) {
            at.value = 1.0;
        }
        let out = join_members(&split, &st2, &node).unwrap();
        assert!(out.node(&node).is_none());
        assert_eq!(out.members.len(), 1);
        let m = out.member("1").unwrap();
        assert_eq!((m.a.as_str(), m.b.as_str(), m.hinge_b), ("A", "B", true));
        // The force goes back to `L/2`, the two pieces of the triangular load stay two.
        assert!(matches!(&out.loads[0], Load::Force { member: Some(m), at, .. } if m == "1" && at.expr.as_deref() == Some("L/2")));
        assert!(matches!(&out.loads[1], Load::Force { member: Some(m), at, .. } if m == "1" && at.value == 1.0));
        match (&out.loads[2], &out.loads[3]) {
            (Load::Distributed { member: a, from: af, to: at, .. }, Load::Distributed { member: b, from: bf, to: bt, .. }) => {
                assert_eq!((a.as_str(), af.value, at.as_ref().map(|t| t.value)), ("1", 0.0, Some(2.0)));
                assert_eq!((b.as_str(), bf.value, bt.is_none()), ("1", 2.0, true));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn joining_turns_a_member_round_and_takes_the_node_loads() {
        // 1: A→B, 2: C→B (the other way), aligned along x; a force at B, a load on 2.
        let src = Structure::from_json(
            r#"{"nodes":[{"id":"A","x":0,"y":0},{"id":"B","x":4,"y":0},{"id":"C","x":10,"y":0}],
                "members":[{"id":"1","a":"A","b":"B"},{"id":"2","a":"C","b":"B","hingeA":true}],
                "loads":[{"type":"force","node":"B","value":7},
                         {"type":"force","member":"2","at":1,"value":2},
                         {"type":"distributed","member":"2","q1":1,"q2":3,"to":4,"axes":"local","angle":-90}]}"#,
        )
        .unwrap();
        let out = join_members(&src, &src, "B").unwrap();
        let m = out.member("1").unwrap();
        assert_eq!((m.a.as_str(), m.b.as_str(), m.hinge_a, m.hinge_b), ("A", "C", false, true));
        assert!(out.member("2").is_none());
        assert!(matches!(&out.loads[0], Load::Force { node: None, member: Some(m), at, .. } if m == "1" && at.value == 4.0));
        assert!(matches!(&out.loads[1], Load::Force { member: Some(m), at, .. } if m == "1" && at.value == 9.0));
        match &out.loads[2] {
            Load::Distributed { member, q1, q2, from, to, angle, .. } => {
                assert_eq!(
                    (member.as_str(), q1.value, q2.as_ref().map(|q| q.value), from.value, to.as_ref().map(|t| t.value), angle.value),
                    ("1", 3.0, Some(1.0), 6.0, None, 90.0)
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn only_aligned_members_join() {
        let make = |cx: f64, extra: &str| {
            Structure::from_json(&format!(
                r#"{{"nodes":[{{"id":"A","x":0,"y":0}},{{"id":"B","x":4,"y":0}},{{"id":"C","x":{cx},"y":3}}],
                    "members":[{{"id":"1","a":"A","b":"B"}},{{"id":"2","a":"B","b":"C"}}]{extra}}}"#
            ))
            .unwrap()
        };
        let bent = make(8.0, "");
        assert_eq!(join_members(&bent, &bent, "B").err(), Some(NoJoin::NotAligned));
        assert_eq!(join_members(&bent, &bent, "A").err(), Some(NoJoin::NotTwo));
        let mut straight = make(8.0, r#","supports":[{"node":"B","kind":"pin"}]"#);
        for n in &mut straight.nodes {
            if n.id == "C" {
                n.y = Num::new(0.0);
            }
        }
        assert_eq!(join_members(&straight, &straight, "B").err(), Some(NoJoin::Support));
        straight.supports.clear();
        straight.members[1].hinge_a = true;
        assert_eq!(join_members(&straight, &straight, "B").err(), Some(NoJoin::Hinge));
        straight.members[1].hinge_a = false;
        straight.members[1].i = Some(Num::new(2.0));
        assert_eq!(join_members(&straight, &straight, "B").err(), Some(NoJoin::Section));
        straight.members[1].i = None;
        assert!(join_members(&straight, &straight, "B").is_ok());
        assert_eq!(shared_node(&straight, "1", "2").as_deref(), Some("B"));
        assert_eq!(shared_node(&straight, "1", "1"), None);
    }

    #[test]
    fn negating_keeps_expressions_and_undoes_itself() {
        assert_eq!(negated(&Num::new(10.0)).value, -10.0);
        let f = Num { value: 5.0, expr: Some("F".into()) };
        let n = negated(&f);
        assert_eq!((n.expr.as_deref(), n.value), (Some("-(F)"), -5.0));
        assert_eq!(negated(&n), f);
        let odd = Num { value: 1.0, expr: Some("-(a) + (b)".into()) };
        assert_eq!(negated(&odd).expr.as_deref(), Some("-(-(a) + (b))"));
        let src = beam();
        match reversed(&src, 2).unwrap() {
            Load::Distributed { q1, q2, .. } => assert_eq!((q1.value, q2.map(|q| q.value)), (0.0, Some(-6.0))),
            other => panic!("{other:?}"),
        }
        assert!(reversed(&src, 99).is_none());
    }

    #[test]
    fn a_point_load_moves_to_a_node_or_a_member() {
        let src = beam();
        let st = resolved(&src);
        let at_node = placed(&src, &st, 0, &Place::Node("A".into())).unwrap();
        assert!(matches!(&at_node, Load::Force { node: Some(n), member: None, .. } if n == "A"));
        let slid = placed(&src, &st, 0, &Place::Member("1".into(), 3.5)).unwrap();
        assert!(matches!(&slid, Load::Force { at, .. } if at.expr.as_deref() == Some("L/2 + 0.5")));
        assert!(placed(&src, &st, 0, &Place::Member("1".into(), 3.0)).is_none(), "where it was");
        let couple = placed(&src, &st, 3, &Place::Member("1".into(), 2.0)).unwrap();
        assert!(matches!(&couple, Load::Moment { node: None, member: Some(m), at, .. } if m == "1" && at.value == 2.0));
        assert!(placed(&src, &st, 2, &Place::Node("A".into())).is_none(), "a distributed load is not a point load");
    }

    #[test]
    fn force_angles_follow_the_arrow() {
        assert_eq!(force_angle(0.0, -1.0, 10.0, 0.0), Some(-90.0));
        assert_eq!(force_angle(0.0, -1.0, -10.0, 0.0), Some(90.0));
        assert_eq!(force_angle(1.0, 0.27, 1.0, 15.0), Some(15.0));
        assert_eq!(force_angle(-1.0, -0.0001, 1.0, 15.0), Some(180.0));
        assert_eq!(force_angle(0.0, 0.0, 1.0, 15.0), None);
        assert_eq!(force_angle(f64::NAN, 1.0, 1.0, 15.0), None);
    }

    #[test]
    fn a_distributed_load_spreads_within_its_member() {
        let src = beam();
        let st = resolved(&src);
        match respread(&src, &st, 2, 1.0, 9.0, 0.5).unwrap() {
            Load::Distributed { from, to, .. } => assert_eq!((from.value, to.is_none()), (1.0, true)),
            other => panic!("{other:?}"),
        }
        match respread(&src, &st, 2, 0.0, 4.0, 0.5).unwrap() {
            Load::Distributed { from, to, .. } => assert_eq!((from.value, to.map(|t| t.value)), (0.0, Some(4.0))),
            other => panic!("{other:?}"),
        }
        // Never shorter than the gap, even when the ends cross.
        match respread(&src, &st, 2, 5.0, 2.0, 0.5).unwrap() {
            Load::Distributed { from, to, .. } => assert_eq!((from.value, to.map(|t| t.value)), (5.0, Some(5.5))),
            other => panic!("{other:?}"),
        }
        assert!(respread(&src, &st, 2, 0.0, 6.0, 0.5).is_none(), "unchanged");
        assert!(respread(&src, &st, 0, 0.0, 6.0, 0.5).is_none(), "not distributed");
    }

    /// A → B → C → D along x (4 + 2 + 3), member 2 drawn C → B; E above B on a post.
    fn run() -> Structure {
        Structure::from_json(
            r#"{"nodes":[{"id":"A","x":0,"y":0},{"id":"B","x":4,"y":0},{"id":"C","x":6,"y":0},{"id":"D","x":9,"y":0},{"id":"E","x":4,"y":3}],
                "members":[{"id":"1","a":"A","b":"B"},{"id":"2","a":"C","b":"B"},{"id":"3","a":"C","b":"D"},{"id":"4","a":"B","b":"E"}],
                "loads":[{"type":"distributed","member":"1","q1":10,"from":"L","to":3},
                         {"type":"distributed","member":"2","q1":1,"q2":3,"axes":"local","angle":-90}]}"#,
        )
        .unwrap()
    }

    #[test]
    fn a_distributed_load_slides_past_a_node_onto_the_aligned_members() {
        let src = run();
        let mut st = src.clone();
        if let Some(Load::Distributed { from, .. }) = st.loads.get_mut(0) {
            from.value = 1.0;
        }
        let (line, f, t) = on_run(&st, 0).unwrap();
        assert_eq!((line.len, f, t, line.parts.len()), (9.0, 1.0, 3.0, 3));
        // Slid by 2.5 (the post at B is not on the line): over 1 and 2, its `from` still `L`.
        match respread(&src, &st, 0, 3.5, 5.5, 0.01).unwrap() {
            Load::Distributed { member, then, from, to, .. } => {
                assert_eq!((member.as_str(), then.as_slice()), ("1", ["2".to_string()].as_slice()));
                // The expression's value comes when the structure is resolved again.
                assert_eq!((from.expr.as_deref(), to.map(|t| t.value)), (Some("L + 2.5"), Some(5.5)));
            }
            other => panic!("{other:?}"),
        }
        // Stretched to D: over all three, to the end.
        match respread(&src, &st, 0, 1.0, 9.0, 0.01).unwrap() {
            Load::Distributed { member, then, to, .. } => {
                assert_eq!((member.as_str(), then.len(), to.is_none()), ("1", 2, true));
            }
            other => panic!("{other:?}"),
        }
        // Onto 3 alone: from its `a` end, no chain.
        match respread(&src, &st, 0, 7.0, 8.0, 0.01).unwrap() {
            Load::Distributed { member, then, from, to, .. } => {
                assert_eq!((member.as_str(), then.is_empty(), from.expr.as_deref(), to.map(|t| t.value)), ("3", true, Some("L"), Some(2.0)));
            }
            other => panic!("{other:?}"),
        }
        assert!(respread(&src, &st, 0, 1.0, 3.0, 0.01).is_none(), "unchanged");
    }

    #[test]
    fn a_load_left_on_a_member_running_against_the_line_is_turned_round() {
        let src = run();
        let st = src.clone();
        // Load 1 (on 2, drawn C → B, so its line runs from C to B: towards A).
        let (line, f, t) = on_run(&st, 1).unwrap();
        assert_eq!((line.start, line.len, f, t), ((9.0, 0.0), 9.0, 3.0, 5.0));
        // Slid onto 1 alone (from 6 to 8 along the run, which is 2..0 on member 1 read A → B).
        match respread(&src, &st, 1, 6.0, 8.0, 0.01).unwrap() {
            Load::Distributed { member, then, q1, q2, from, to, angle, .. } => {
                assert_eq!((member.as_str(), then.is_empty(), from.value, to.map(|t| t.value)), ("1", true, 1.0, Some(3.0)));
                assert_eq!((q1.value, q2.map(|q| q.value), angle.value), (3.0, Some(1.0), 90.0));
            }
            other => panic!("{other:?}"),
        }
        // Over 2 and 1: a chain read from C, its values as they were.
        match respread(&src, &st, 1, 3.0, 7.0, 0.01).unwrap() {
            Load::Distributed { member, then, q1, from, to, angle, .. } => {
                assert_eq!((member.as_str(), then.as_slice(), from.value, to.map(|t| t.value)), ("2", ["1".to_string()].as_slice(), 0.0, Some(4.0)));
                assert_eq!((q1.value, angle.value), (1.0, -90.0));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn splitting_a_member_under_a_load_over_several_keeps_it_one() {
        let src = Structure::from_json(
            r#"{"nodes":[{"id":"A","x":0,"y":0},{"id":"B","x":4,"y":0},{"id":"C","x":6,"y":0}],
                "members":[{"id":"1","a":"A","b":"B"},{"id":"2","a":"C","b":"B"}],
                "loads":[{"type":"distributed","member":"1","then":["2"],"q1":5,"from":1}]}"#,
        )
        .unwrap();
        let (out, _) = split_member(&src, &src, "2", 0.5).unwrap();
        // 2 (C → B) split at 0.5 from C: the new member 3 runs from there to B, before 2 on the line.
        assert_eq!(out.loads.len(), 1);
        assert!(matches!(&out.loads[0], Load::Distributed { member, then, from, .. }
            if member == "1" && then == &["3".to_string(), "2".to_string()] && from.value == 1.0));
        let line = out.load_line(&out.loads[0]).unwrap();
        assert_eq!(line.len, 6.0);
        // Removing a member it lies on takes it away; joining in pieces first.
        let gone = merge_nodes(&src, "C", "B");
        assert!(gone.is_some_and(|s| s.loads.is_empty()));
        let joined = join_members(&src, &src, "B").unwrap();
        assert_eq!(joined.members.len(), 1);
        assert!(joined.loads.iter().all(|l| matches!(l, Load::Distributed { member, then, .. } if member == "1" && then.is_empty())));
        // Turning a member round leaves it where it is.
        let turned = reverse_member(&src, &src, "2").unwrap();
        assert_eq!(turned.loads, src.loads);
    }

    #[test]
    fn connected_finds_the_whole_part() {
        let st = Structure::from_json(
            r#"{"nodes":[{"id":"A","x":0,"y":0},{"id":"B","x":1,"y":0},{"id":"C","x":2,"y":0},{"id":"D","x":5,"y":0},{"id":"E","x":6,"y":0}],
                "members":[{"id":"1","a":"A","b":"B"},{"id":"2","a":"B","b":"C"},{"id":"3","a":"D","b":"E"}]}"#,
        )
        .unwrap();
        let (nodes, members) = connected(&st, &["C".to_string()]);
        assert_eq!((nodes.len(), members.len()), (3, 2));
        assert!(!nodes.contains(&"D".to_string()));
    }

    #[test]
    fn reversing_a_member_swaps_its_ends_hinges_and_loads_in_place() {
        let st = Structure::from_json(
            r#"{"nodes":[{"id":"A","x":0,"y":0},{"id":"B","x":6,"y":0}],
                "members":[{"id":"1","a":"A","b":"B","hingeB":true}],
                "loads":[
                    {"type":"force","member":"1","at":2,"value":10},
                    {"type":"distributed","member":"1","q1":2,"q2":8,"from":1,"to":4,"axes":"local","angle":-90}
                ]}"#,
        )
        .unwrap();
        let out = reverse_member(&st, &st, "1").unwrap();
        let m = out.member("1").unwrap();
        assert_eq!((m.a.as_str(), m.b.as_str(), m.hinge_a, m.hinge_b), ("B", "A", true, false));
        assert!(matches!(&out.loads[0], Load::Force { at, .. } if (at.value - 4.0).abs() < 1e-9));
        match &out.loads[1] {
            Load::Distributed { q1, q2, from, to, angle, .. } => {
                assert_eq!(
                    (q1.value, q2.as_ref().map(|q| q.value), from.value, to.as_ref().map(|t| t.value), angle.value),
                    (8.0, Some(2.0), 2.0, Some(5.0), 90.0)
                );
            }
            other => panic!("{other:?}"),
        }
    }
}
