//! Strix CAD drawings (see `docs/cad.md`).
//!
//! A [`Drawing`] is a 2D drafting model in real-world units (millimetres, centimetres or metres,
//! y up, angles in degrees counter-clockwise) drawn at a scale `1:N` on paper. It holds layers and
//! entities: lines, circles, arcs, polylines, points, texts and dimensions. Every coordinate,
//! radius and angle is a [`Num`]: a number, or an expression of the document's calculation
//! parameters (`"L"`, `"b/2 + 10 mm"`), so a drawing follows the results of the calculations
//! before it ([`Drawing::resolve`]). Dimension labels show the measured size (`<>`) or a
//! parameter (`{{L}}`).
//!
//! [`Drawing::render`] turns the drawing into [`Prim`]itives in paper millimetres (y down), the
//! one geometry that the page, the PDF and the editor all draw. Stroke widths, text heights,
//! dimension offsets and ticks are paper sizes and do not scale.
//!
//! Hostile input never panics: sizes are clamped, counts capped, non-finite numbers rejected.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod edit;
mod geom;
mod input;
pub mod kernel;
mod model;
mod props;
mod render;
mod select;
mod svg;

#[cfg(test)]
mod tests;

pub use edit::{Grip, SnapKind, Snaps, dim_offset, outline};
pub use geom::{Pt, bulge_points};
pub use input::{InputError, parse_point};
pub use model::{Align, Dash, DimStyle, Drawing, Entity, Kind, Layer, MAX_PADDING, Mark, Num, Orient, P, Padding, Stroke, Unit, fill};
pub use props::{ALIGNS, Field, FieldKind, MARKS, ORIENTS, num_text};
pub use render::{Anchor, Prim, Rendered};
pub use select::{Family, Focus, Order, SelectOption, StrokeEdit, families, point_in_polygon};
pub use svg::MISSING_COLOR;

/// Marks the start and end of a unit in a text (`L = 4.5 \u{1F}m\u{1E}`), so it can be set in
/// the document's Unit style. Never part of what is shown.
pub const UNIT_START: char = '\u{1F}';
pub const UNIT_END: char = '\u{1E}';
/// Marks a placeholder whose parameter is not defined (`[L?]`), drawn in red.
pub const MISSING_START: char = '\u{1D}';
pub const MISSING_END: char = '\u{1C}';

/// True for the marker characters above (never shown).
pub fn is_marker(c: char) -> bool {
    matches!(c, UNIT_START | UNIT_END | MISSING_START | MISSING_END)
}

/// Most entities in one drawing.
pub const MAX_ENTITIES: usize = 5000;
/// Most layers in one drawing.
pub const MAX_LAYERS: usize = 64;
/// Most vertices in one polyline.
pub const MAX_VERTICES: usize = 10_000;
/// Largest coordinate magnitude, in drawing units.
pub const MAX_COORD: f64 = 1e9;
/// Largest serialized drawing accepted by [`Drawing::from_json`], in bytes.
pub const MAX_JSON_BYTES: usize = 8 << 20;
