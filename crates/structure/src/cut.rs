//! A section through a member, for the drawing of one stretch of a written working
//! ([`View::Section`](crate::View)): where the stretch lies on the structure, where it is cut, and
//! the part before the cut whose equilibrium gives the stretch's laws.

use serde::{Deserialize, Serialize};

use crate::diagram::Action;
use crate::model::Structure;

/// One stretch of a member, cut at its middle: its laws `N_tag(x)`, `T_tag(x)`, `M_tag(x)` come
/// from the equilibrium of the part before the cut (the member's first end's side).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Cut {
    /// The member's id.
    pub member: String,
    /// The stretch, from the member's first node (the structure's length unit).
    pub from: f64,
    pub to: f64,
    /// The stretch's tag in the laws' names (`1`, `2a`).
    pub tag: String,
    /// One internal force alone on the cut (an exercise's diagram part), or `None` for all
    /// three (`N`, `T`, `M` together in a whole working).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<Action>,
}

impl Cut {
    /// Where the member is cut: half way along the stretch.
    pub fn at(&self) -> f64 {
        (self.from + self.to) / 2.0
    }

    /// Clean names, finite numbers, `from` before `to`.
    pub(crate) fn sanitized(mut self) -> Cut {
        self.member = self.member.trim().chars().filter(|c| !c.is_control()).take(24).collect();
        // As the member's id is kept (`1-2`), with its stretch's letter: the working writes it so.
        self.tag = self.tag.trim().chars().filter(|c| !c.is_control()).take(26).collect();
        let clean = |v: f64| if v.is_finite() { v.clamp(-crate::MAX_COORD, crate::MAX_COORD) } else { 0.0 };
        let (a, b) = (clean(self.from), clean(self.to));
        (self.from, self.to) = if a <= b { (a, b) } else { (b, a) };
        self
    }
}

/// The nodes of the part of `st` before a section of member `cut`: those reached from its first
/// node `start` without the member; `None` when its other node `end` is among them (the member
/// closes a ring: no part lies before its sections alone).
pub fn part_before(st: &Structure, cut: &str, start: &str, end: &str) -> Option<Vec<String>> {
    let mut seen = vec![start.to_string()];
    let mut frontier = vec![start.to_string()];
    while let Some(v) = frontier.pop() {
        for m in st.members.iter().filter(|m| m.id != cut) {
            let other = if m.a == v {
                &m.b
            } else if m.b == v {
                &m.a
            } else {
                continue;
            };
            if !seen.contains(other) {
                if seen.len() > st.nodes.len() {
                    return None;
                }
                seen.push(other.clone());
                frontier.push(other.clone());
            }
        }
    }
    (!seen.iter().any(|x| x == end)).then_some(seen)
}
