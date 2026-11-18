//! The loads' names (`docs/structures.md`, Loads): each kind numbered in the order the loads are
//! listed, `F_1`, `F_2`… for the concentrated forces, `q_1`, `q_2`… for the distributed loads and
//! `C_1`, `C_2`… for the couples. A distributed load that varies names its two ends `q_(1,1)` and
//! `q_(1,2)`. The drawing writes them by the loads' values, the working defines them and its
//! equations use them.

use crate::model::{Load, Structure};

/// A load's name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadName {
    /// `F`, `q` or `C`.
    pub letter: char,
    /// Its number among the loads of its kind, from 1.
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

/// Each load's name, in the order of [`Structure::loads`] (its numbers resolved); `None` for a
/// load that does nothing (a zero value, nowhere), which the drawing leaves out and the
/// numbering skips.
pub fn load_names(st: &Structure) -> Vec<Option<LoadName>> {
    let (mut forces, mut spreads, mut couples) = (0_usize, 0_usize, 0_usize);
    st.loads
        .iter()
        .map(|l| match l {
            Load::Force { node, member, value, .. } if value.value != 0.0 && (node.is_some() || member.is_some()) => {
                forces = forces.saturating_add(1);
                Some(LoadName { letter: 'F', number: forces, varying: false })
            }
            Load::Moment { node, member, value, .. } if value.value != 0.0 && (node.is_some() || member.is_some()) => {
                couples = couples.saturating_add(1);
                Some(LoadName { letter: 'C', number: couples, varying: false })
            }
            Load::Distributed { q1, q2, .. } => {
                let (a, b) = (q1.value, q2.as_ref().map_or(q1.value, |q| q.value));
                if a == 0.0 && b == 0.0 {
                    return None;
                }
                spreads = spreads.saturating_add(1);
                Some(LoadName { letter: 'q', number: spreads, varying: varies(a, b) })
            }
            _ => None,
        })
        .collect()
}
