//! The point a moment equation of the reactions' working is written about, for the drawing of its
//! lever arms ([`View::Arms`](crate::View)): a node, and the members of the part balanced about it
//! (a hinge's part), or the whole structure.

use serde::{Deserialize, Serialize};

use crate::model::{Load, Structure};

/// Most members a pivot's part names.
const MAX_MEMBERS: usize = 64;

/// The node a rotation is balanced about and the part it balances.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pivot {
    /// The node's id.
    pub node: String,
    /// The part's members (a hinge's part); empty for the whole structure.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<String>,
}

impl Pivot {
    /// The whole structure balanced about `node`.
    pub fn whole(node: &str) -> Pivot {
        Pivot { node: node.to_string(), members: Vec::new() }
    }

    /// Clean names, few members.
    pub(crate) fn sanitized(mut self) -> Pivot {
        let clean = |s: &str, n: usize| -> String { s.trim().chars().filter(|c| !c.is_control()).take(n).collect() };
        self.node = clean(&self.node, 24);
        self.members = self.members.iter().take(MAX_MEMBERS).map(|m| clean(m, 24)).collect();
        self
    }

    /// Is member `id` in the part?
    pub fn has_member(&self, id: &str) -> bool {
        self.members.is_empty() || self.members.iter().any(|m| m == id)
    }

    /// Is node `id` in the part (an end of one of its members)?
    pub fn has_node(&self, st: &Structure, id: &str) -> bool {
        self.members.is_empty() || st.members.iter().any(|m| (m.a == id || m.b == id) && self.has_member(&m.id))
    }

    /// Does load `l` act on the part? On a member, when the member is in it; on a node, when the
    /// node is, the pivot itself aside in a part (a couple there would be ambiguous).
    pub fn has_load(&self, st: &Structure, l: &Load) -> bool {
        if self.members.is_empty() {
            return true;
        }
        match l {
            Load::Distributed { member, then, .. } => self.has_member(member) && then.iter().all(|m| self.has_member(m)),
            Load::Force { member: Some(m), .. } | Load::Moment { member: Some(m), .. } => self.has_member(m),
            Load::Force { node: Some(n), .. } | Load::Moment { node: Some(n), .. } => n != &self.node && self.has_node(st, n),
            _ => false,
        }
    }
}
