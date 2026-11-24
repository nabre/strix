//! Strix plane structures (`docs/structures.md`): beams, frames and trusses, isostatic or
//! hyperstatic, with their reactions and N, V (T), M diagrams.
//!
//! A [`Structure`] holds nodes, members, supports and loads in real-world units (metres and
//! kilonewtons by default, y up, angles in degrees counter-clockwise). Every number is a [`Num`]:
//! a literal or an expression of the document's calculation parameters (`"L/2"`, `"q"`), so a
//! structure follows the calculations before it ([`Structure::resolve`]).
//!
//! [`analyse`] classifies the structure by the rank of its equilibrium matrix (isostatic,
//! hyperstatic of degree h, labile) and solves it by the stiffness method;
//! [`Structure::render_for`] draws the structure and, one under the other at the same horizontal
//! scale, its reactions and diagrams, as `strix-plot` primitives in paper millimetres (y down).
//!
//! Hostile input never panics: counts are capped, numbers clamped, names cleaned.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod cut;
mod diagram;
mod linalg;
mod model;
mod names;
mod orient;
mod pivot;
mod render;
mod solve;
mod working;

#[cfg(test)]
mod tests;

pub use cut::{Cut, part_before};
pub use diagram::{Action, Notable, NotableKind, notables};
pub use model::{
    Diagram, Diagrams, ForceUnit, Layout, LengthUnit, Load, LoadAxes, LoadPer, Member, Names, Node, Num, Positive, SenseTurn, SenseX, SenseY, Side,
    Structure, Support, SupportKind, Titles, View,
};
pub use names::{LoadName, load_names};
pub use pivot::Pivot;
pub use render::{Chains, StructureRendered, chains};
pub use solve::{Analysis, Forces, MemberResult, Reaction, Status, analyse, analyse_released};
pub use working::{Resultant, ResultantKind, Stretch, resultants, stretches};

/// Maximum nodes in a structure.
pub(crate) const MAX_NODES: usize = 200;
/// Maximum members.
pub(crate) const MAX_MEMBERS: usize = 300;
/// Maximum supports.
pub(crate) const MAX_SUPPORTS: usize = 200;
/// Maximum loads.
pub(crate) const MAX_LOADS: usize = 500;
/// Largest coordinate or value magnitude.
pub(crate) const MAX_COORD: f64 = 1e9;
/// Largest JSON source accepted.
pub(crate) const MAX_JSON_BYTES: usize = 512 * 1024;
