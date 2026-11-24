//! The members in the order of the structure's geometry (`docs/structures.md`, Names): left to
//! right by their middles, then bottom to top, as a reader goes along a beam or a frame. The
//! written working takes its stretches in this order whatever the names; [`Structure::in_order`]
//! renames the nodes (`A`, `B`, `C`… as the ordered members meet them) and the members (`1`, `2`,
//! `3`…) so the names follow it too.

use crate::model::{Load, Structure};

/// A coordinate as an ordering key: rounded to a millionth, so that points that are the same up
/// to rounding tie (`as` saturates, NaN is 0).
fn key(v: f64) -> i64 {
    (v * 1e6).round() as i64
}

/// A node's name by its place: `A`…`Z`, then `N27`, `N28`….
fn node_name(k: usize) -> String {
    match u8::try_from(k).ok().filter(|k| *k < 26) {
        Some(k) => char::from(b'A'.saturating_add(k)).to_string(),
        None => format!("N{}", k.saturating_add(1)),
    }
}

/// The members' indices in [`Structure::members`] in the geometry's order: left to right by
/// their middles, then bottom to top, then as listed. A member whose nodes are missing goes last.
pub fn member_order(st: &Structure) -> Vec<usize> {
    let mut keys: Vec<(usize, Option<(i64, i64)>)> = st
        .members
        .iter()
        .enumerate()
        .map(|(k, m)| {
            let mid = match (st.node(&m.a), st.node(&m.b)) {
                (Some(a), Some(b)) => Some((key((a.x.value + b.x.value) / 2.0), key((a.y.value + b.y.value) / 2.0))),
                _ => None,
            };
            (k, mid)
        })
        .collect();
    keys.sort_by_key(|&(k, mid)| (mid.is_none(), mid.unwrap_or_default(), k));
    keys.into_iter().map(|(k, _)| k).collect()
}

/// Member `id`'s place in [`member_order`], from 1; `None` for no such member.
pub fn member_rank(st: &Structure, id: &str) -> Option<usize> {
    member_order(st).iter().position(|&k| st.members.get(k).is_some_and(|m| m.id == id)).map(|p| p.saturating_add(1))
}

impl Structure {
    /// The structure with its nodes and members renamed in the geometry's order: the members
    /// `1`, `2`, `3`… as [`member_order`] takes them, the nodes `A`, `B`, `C`… as those members
    /// meet them (each member from its left end, a vertical one from its lower end), the nodes no
    /// member reaches last, left to right then bottom to top. Members, supports, loads, the
    /// section and the pivot follow their names; the lists keep their order.
    pub fn in_order(&self) -> Structure {
        let order = member_order(self);
        let mut nodes: Vec<String> = Vec::new();
        let meet = |id: &str, nodes: &mut Vec<String>| {
            if self.node(id).is_some() && !nodes.iter().any(|n| n == id) {
                nodes.push(id.to_string());
            }
        };
        for &k in &order {
            let Some(m) = self.members.get(k) else { continue };
            let (Some(a), Some(b)) = (self.node(&m.a), self.node(&m.b)) else { continue };
            let first_a = (key(a.x.value), key(a.y.value)) <= (key(b.x.value), key(b.y.value));
            let (p, q) = if first_a { (&m.a, &m.b) } else { (&m.b, &m.a) };
            meet(p, &mut nodes);
            meet(q, &mut nodes);
        }
        let mut alone: Vec<(i64, i64, usize)> =
            self.nodes.iter().enumerate().filter(|(_, n)| !nodes.contains(&n.id)).map(|(k, n)| (key(n.x.value), key(n.y.value), k)).collect();
        alone.sort_unstable();
        for (_, _, k) in alone {
            if let Some(n) = self.nodes.get(k) {
                meet(&n.id.clone(), &mut nodes);
            }
        }
        let node_map: Vec<(String, String)> = nodes.into_iter().enumerate().map(|(k, old)| (old, node_name(k))).collect();
        let member_map: Vec<(String, String)> =
            order.iter().enumerate().filter_map(|(r, &k)| self.members.get(k).map(|m| (m.id.clone(), r.saturating_add(1).to_string()))).collect();
        let node_of = |id: &str| node_map.iter().find(|(o, _)| o == id).map_or_else(|| id.to_string(), |(_, n)| n.clone());
        let member_of = |id: &str| member_map.iter().find(|(o, _)| o == id).map_or_else(|| id.to_string(), |(_, n)| n.clone());
        let mut st = self.clone();
        for n in &mut st.nodes {
            n.id = node_of(&n.id);
        }
        for m in &mut st.members {
            m.id = member_of(&m.id);
            m.a = node_of(&m.a);
            m.b = node_of(&m.b);
        }
        for s in &mut st.supports {
            s.node = node_of(&s.node);
        }
        for l in &mut st.loads {
            match l {
                Load::Force { node, member, .. } | Load::Moment { node, member, .. } => {
                    if let Some(n) = node.as_mut() {
                        *n = node_of(n);
                    }
                    if let Some(m) = member.as_mut() {
                        *m = member_of(m);
                    }
                }
                Load::Distributed { member, then, .. } => {
                    *member = member_of(member);
                    for m in then.iter_mut() {
                        *m = member_of(m);
                    }
                }
            }
        }
        if let Some(c) = st.cut.as_mut() {
            c.member = member_of(&c.member);
        }
        if let Some(p) = st.pivot.as_mut() {
            p.node = node_of(&p.node);
            for m in p.members.iter_mut() {
                *m = member_of(m);
            }
        }
        st
    }

    /// Do the names already follow the geometry's order ([`Structure::in_order`] would change
    /// none)?
    pub fn names_in_order(&self) -> bool {
        let tidy = self.in_order();
        tidy.nodes.iter().zip(&self.nodes).all(|(a, b)| a.id == b.id) && tidy.members.iter().zip(&self.members).all(|(a, b)| a.id == b.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Member, Node, Support, SupportKind};

    fn structure(nodes: &[(&str, f64, f64)], members: &[(&str, &str, &str)]) -> Structure {
        Structure {
            nodes: nodes.iter().map(|&(id, x, y)| Node { id: id.into(), x: x.into(), y: y.into(), hinge: false }).collect(),
            members: members.iter().map(|&(id, a, b)| Member { id: id.into(), a: a.into(), b: b.into(), ..Member::default() }).collect(),
            ..Structure::default()
        }
    }

    #[test]
    fn a_portal_frame_goes_up_across_and_down() {
        // Drawn beam first, the right column top to bottom.
        let st =
            structure(&[("P", 0.0, 4.0), ("Q", 6.0, 4.0), ("R", 0.0, 0.0), ("S", 6.0, 0.0)], &[("b", "P", "Q"), ("r", "Q", "S"), ("l", "R", "P")]);
        assert_eq!(member_order(&st), [2, 0, 1]);
        assert_eq!(member_rank(&st, "r"), Some(3));
        assert!(!st.names_in_order());
        let tidy = st.in_order();
        let names: Vec<(&str, &str, &str)> = tidy.members.iter().map(|m| (m.id.as_str(), m.a.as_str(), m.b.as_str())).collect();
        // The left column A–B, the beam B–C, the right column D at its foot.
        assert_eq!(names, [("2", "B", "C"), ("3", "C", "D"), ("1", "A", "B")]);
        let nodes: Vec<&str> = tidy.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(nodes, ["B", "C", "A", "D"]);
        assert!(tidy.names_in_order());
    }

    #[test]
    fn names_swap_without_clashing_and_hostile_input_is_kept() {
        let mut st = structure(&[("B", 0.0, 0.0), ("A", 5.0, 0.0), ("Z", 9.0, 9.0)], &[("2", "B", "A"), ("1", "A", "missing")]);
        st.supports.push(Support { node: "A".into(), kind: SupportKind::Pin, angle: Default::default() });
        st.loads.push(Load::Force { node: Some("B".into()), member: None, at: Default::default(), value: 1.0.into(), angle: Default::default() });
        let tidy = st.in_order();
        assert_eq!(tidy.members[0].id, "1");
        assert_eq!((tidy.members[0].a.as_str(), tidy.members[0].b.as_str()), ("A", "B"));
        // A member without its nodes goes last, its missing node's name kept.
        assert_eq!((tidy.members[1].id.as_str(), tidy.members[1].b.as_str()), ("2", "missing"));
        assert_eq!(tidy.nodes[2].id, "C");
        assert_eq!(tidy.supports[0].node, "B");
        assert!(matches!(&tidy.loads[0], Load::Force { node: Some(n), .. } if n == "A"));
        assert_eq!(node_name(25), "Z");
        assert_eq!(node_name(26), "N27");
        let _ = Structure::default().in_order();
    }
}
