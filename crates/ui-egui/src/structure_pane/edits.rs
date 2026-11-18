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
        Load::Distributed { member, .. } => !gone.contains(member),
    });
    Some(out)
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

/// Distributed load `k` spread from `from` to `to` along its member (resolved values, clamped to
/// it, at least `gap` long): a `from` or `to` written as an expression stays one, a `to` at the
/// member's end goes back to "to the end". `None` when nothing changed.
pub(super) fn spread(src: &Structure, st: &Structure, k: usize, from: f64, to: f64, gap: f64) -> Option<Load> {
    let Some(Load::Distributed { member, from: rf, to: rt, .. }) = st.loads.get(k) else { return None };
    let len = st.member(member).and_then(|m| st.length(m)).filter(|l| *l > 0.0)?;
    let gap = gap.clamp(EPS, len);
    let (cf, ct) = (rf.value, rt.as_ref().map_or(len, |t| t.value));
    let f = from.clamp(0.0, len - gap);
    let t = to.clamp(f + gap, len);
    if (f - cf).abs() <= EPS && (t - ct).abs() <= EPS {
        return None;
    }
    let mut l = src.loads.get(k)?.clone();
    if let Load::Distributed { from: lf, to: lt, .. } = &mut l {
        if (f - cf).abs() > EPS {
            *lf = if f <= EPS { Num::new(0.0) } else { shifted(lf, f - cf) };
        }
        if (t - ct).abs() > EPS {
            *lt = if t >= len - EPS {
                None
            } else {
                Some(match lt {
                    Some(n) => shifted(n, t - ct),
                    None => Num::new(round(t)),
                })
            };
        }
    }
    Some(l)
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
        match spread(&src, &st, 2, 1.0, 9.0, 0.5).unwrap() {
            Load::Distributed { from, to, .. } => assert_eq!((from.value, to.is_none()), (1.0, true)),
            other => panic!("{other:?}"),
        }
        match spread(&src, &st, 2, 0.0, 4.0, 0.5).unwrap() {
            Load::Distributed { from, to, .. } => assert_eq!((from.value, to.map(|t| t.value)), (0.0, Some(4.0))),
            other => panic!("{other:?}"),
        }
        // Never shorter than the gap, even when the ends cross.
        match spread(&src, &st, 2, 5.0, 2.0, 0.5).unwrap() {
            Load::Distributed { from, to, .. } => assert_eq!((from.value, to.map(|t| t.value)), (5.0, Some(5.5))),
            other => panic!("{other:?}"),
        }
        assert!(spread(&src, &st, 2, 0.0, 6.0, 0.5).is_none(), "unchanged");
        assert!(spread(&src, &st, 0, 0.0, 6.0, 0.5).is_none(), "not distributed");
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
}
