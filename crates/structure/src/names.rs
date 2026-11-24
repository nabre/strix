//! The loads' names (`docs/structures.md`, Loads): `F` for the concentrated forces, `q` for the
//! distributed loads, `C` for the couples, with one count through all of them (`q_1`, `F_2`,
//! `C_3`…) in the order of their places: left to right, then top to bottom, then force,
//! distributed load, couple. A distributed load that varies names its two ends `q_(1,1)` and
//! `q_(1,2)`. The drawing writes them by the loads' values, the working defines them and its
//! equations use them.

use crate::model::{Load, Structure};

/// A load's name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadName {
    /// `F`, `q` or `C`.
    pub letter: char,
    /// Its number among all the loads, from 1.
    pub number: usize,
    /// A distributed load whose intensity varies: its ends are named, not the load.
    pub varying: bool,
}

/// Unicode subscript digits, for the drawing's plain text.
fn subscript(n: usize) -> String {
    n.to_string()
        .chars()
        .map(|c| match c {
            '0' => '₀',
            '1' => '₁',
            '2' => '₂',
            '3' => '₃',
            '4' => '₄',
            '5' => '₅',
            '6' => '₆',
            '7' => '₇',
            '8' => '₈',
            '9' => '₉',
            other => other,
        })
        .collect()
}

impl LoadName {
    /// The name in linear format, as calc reads it: `F_1`, `q_2`.
    pub fn linear(&self) -> String {
        format!("{}_{}", self.letter, self.number)
    }

    /// A varying load's end `end` (1 or 2) in linear format, `q_(1,2)`; the load's name when it
    /// does not vary.
    pub fn linear_end(&self, end: u8) -> String {
        if self.varying { format!("{}_({},{end})", self.letter, self.number) } else { self.linear() }
    }

    /// The name as the drawing writes it: `F₁`.
    pub fn pretty(&self) -> String {
        format!("{}{}", self.letter, subscript(self.number))
    }

    /// A varying load's end as the drawing writes it, `q₁,₂`; the load's name when it does not
    /// vary.
    pub fn pretty_end(&self, end: u8) -> String {
        if self.varying { format!("{}{},{}", self.letter, subscript(self.number), subscript(usize::from(end))) } else { self.pretty() }
    }
}

/// Does a distributed load from `q1` to `q2` vary along its stretch?
pub(crate) fn varies(q1: f64, q2: f64) -> bool {
    (q1 - q2).abs() > 1e-12 * q1.abs().max(q2.abs())
}

/// A load that does something: its kind's rank (force, distributed, couple) and the point that
/// orders it, the leftmost (then highest) point it acts on; `None` for one that does nothing (a
/// zero value, nowhere) or whose place can't be found.
fn placed(st: &Structure, l: &Load) -> Option<(u8, f64, f64)> {
    // A point `at` along a member from its `a` end.
    let along = |member: &str, at: f64| -> Option<(f64, f64)> {
        let m = st.member(member)?;
        let (na, nb) = (st.node(&m.a)?, st.node(&m.b)?);
        let len = st.length(m)?;
        let t = if len > 0.0 { (at / len).clamp(0.0, 1.0) } else { 0.0 };
        Some((na.x.value + (nb.x.value - na.x.value) * t, na.y.value + (nb.y.value - na.y.value) * t))
    };
    let point = |node: &Option<String>, member: &Option<String>, at: f64| match (member, node) {
        (Some(m), _) => along(m, at),
        (None, Some(n)) => st.node(n).map(|n| (n.x.value, n.y.value)),
        (None, None) => None,
    };
    match l {
        Load::Force { node, member, at, value, .. } if value.value != 0.0 => point(node, member, at.value).map(|(x, y)| (0, x, y)),
        Load::Moment { node, member, at, value } if value.value != 0.0 => point(node, member, at.value).map(|(x, y)| (2, x, y)),
        Load::Distributed { member, then, q1, q2, from, to, .. } => {
            if q1.value == 0.0 && q2.as_ref().is_none_or(|q| q.value == 0.0) {
                return None;
            }
            let line = st.spread_line(member, then)?;
            let (p, q) = (line.point(from.value), line.point(to.as_ref().map_or(line.len, |t| t.value)));
            // The stretch's leftmost end, the higher one on a vertical member.
            let first = if p.0 < q.0 || (p.0 == q.0 && p.1 >= q.1) { p } else { q };
            Some((1, first.0, first.1))
        }
        _ => None,
    }
}

/// A coordinate as an ordering key: rounded to a millionth, so that points that are the same up
/// to rounding tie (`as` saturates, NaN is 0).
fn key(v: f64) -> i64 {
    (v * 1e6).round() as i64
}

/// Each load's name, in the order of [`Structure::loads`] (its numbers resolved); `None` for a
/// load that does nothing (a zero value, nowhere), which the drawing leaves out and the
/// numbering skips. One count runs through all the kinds (`q_1`, `F_2`, `C_3`…), the loads taken
/// left to right, then top to bottom, then force, distributed load, couple (then as listed).
pub fn load_names(st: &Structure) -> Vec<Option<LoadName>> {
    let mut names: Vec<Option<LoadName>> = vec![None; st.loads.len()];
    let mut order: Vec<(usize, u8, f64, f64)> =
        st.loads.iter().enumerate().filter_map(|(k, l)| placed(st, l).map(|(rank, x, y)| (k, rank, x, y))).collect();
    order.sort_by_key(|&(k, rank, x, y)| (key(x), std::cmp::Reverse(key(y)), rank, k));
    for (n, &(k, ..)) in order.iter().enumerate() {
        let Some(load) = st.loads.get(k) else { continue };
        let (letter, varying) = match load {
            Load::Force { .. } => ('F', false),
            Load::Moment { .. } => ('C', false),
            Load::Distributed { q1, q2, .. } => ('q', varies(q1.value, q2.as_ref().map_or(q1.value, |q| q.value))),
        };
        if let Some(slot) = names.get_mut(k) {
            *slot = Some(LoadName { letter, number: n.saturating_add(1), varying });
        }
    }
    names
}
