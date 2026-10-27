//! Strix live calculations with units (see `docs/calc.md`).
//!
//! A *calc region* is an equation whose linear text is a statement such as `L := 4.5 m`
//! (define), `M =` (evaluate) or `σ := M/W = MPa` (define and show). [`evaluate`] runs a list
//! of statements in document order with one shared [`Scope`] and returns, for each, the value and
//! the new text to write back into the equation.
//!
//! Values are [`Quantity`]s: an `f64` magnitude in coherent SI units plus a [`Dim`] (exponents of
//! the seven SI base dimensions). Arithmetic checks dimensions and never panics; every failure is
//! a [`CalcError`] with a plain-English message.
//!
//! # Expression syntax
//!
//! Numbers (`1.5`, `2e-3`, `1,5` with [`Options::decimal_comma`]), names (Latin and Greek letters,
//! digits after the first character, `_` subscripts: `f_ck`, `σ_max`), units (`kN`, `m²`, `°`,
//! `%`), operators `+ - * · × / ^ ² ³`, parentheses, functions
//! (`sqrt √ abs exp ln log sin cos tan asin acos atan min max round`) and constants
//! (`π pi e g`).
//!
//! **Precedence**, loosest first: `+ -`; explicit `* · × /` (left to right); unary minus;
//! *implicit* multiplication (juxtaposition); `^` and `² ³` (right to left). Implicit
//! multiplication binds tighter than `/`, the way engineers write units: `12 kN/m` is
//! `(12·kN)/m` and `2 kN/4 m²` is `(2·kN)/(4·m²)`; the flip side is that `1/2 m` means
//! `1/(2·m)`. Write `1/2*m` or `(1/2) m` for half a metre. A number never multiplies
//! implicitly from the right (`2 3` is an error).
//!
//! **Name resolution** in an expression: a parameter defined in the worksheet first, then a
//! constant, then a unit. So `m := 3` makes later `m` the number 3, and a bare `g` is standard
//! gravity (write `kg` or `mg`, or ask for `= g` as a display unit, for grams). The display unit
//! after `=` is resolved as units only.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod error;
mod eval;
mod format;
mod lexer;
mod parser;
mod quantity;
mod solve;
mod table;
mod units;
mod worksheet;

#[cfg(test)]
mod tests;

pub use error::CalcError;
pub use eval::{Scope, Value, Vector};
pub use format::format_number;
pub use quantity::{BASE_SYMBOLS, Dim, Quantity};
pub use table::{CellNames, TableResult};
pub use units::{auto_unit, constant, display_unit, unit};
pub use worksheet::{Count, Options, RegionKind, RegionResult, Worksheet, evaluate, format_value};

/// Longest statement accepted, in characters; longer text is a [`CalcError::TooLong`].
pub const MAX_INPUT_CHARS: usize = 4096;

/// Deepest nesting of parentheses, signs, roots and exponents in one expression.
pub const MAX_DEPTH: usize = 64;

/// Most nodes (numbers, names, operators, calls) in one expression.
pub const MAX_NODES: usize = 512;

/// Most elements in one vector, range or table column, and most terms in one `∑`/`∏`.
pub const MAX_VALUES: usize = 10_000;

/// Most units of work in one statement, one table or one numeric solve (nodes evaluated plus
/// vector elements touched).
pub const MAX_STEPS: usize = 1_000_000;

/// Most units of work in one worksheet (a whole document's recalculation).
pub const MAX_SHEET_STEPS: usize = 20_000_000;
