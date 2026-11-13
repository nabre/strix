//! Strix Cartesian mathematical and experimental plots (`docs/plot.md`).
//!
//! A [`Plot`] holds 2D Cartesian axes (`[x_min, x_max] × [y_min, y_max]`), grid settings,
//! student/teacher visibility (`answer`) and a list of [`Series`]: adaptive `y = f(x)` functions
//! (with discontinuity/pole splitting, roots, extrema, tangents and shaded definite integrals),
//! parametric curves `(x(t), y(t))`, and experimental datasets from `calc.table` vectors or inline
//! points with `±Δx, ±Δy` error bars and linear/quadratic least-squares regression.
//!
//! [`Plot::render_for`] turns a plot into [`PlotPrim`] primitives in paper millimetres (y down),
//! guaranteeing identical bounding boxes, axes and grids between student (`show_answers = false`)
//! and teacher (`show_answers = true`) copies.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod features;
mod model;
mod render;
mod sample;
mod svg;

#[cfg(test)]
mod tests;

pub use features::{IntegralInfo, NotableKind, NotablePoint, RegressionInfo};
pub use model::{DEFAULT_COLORS, DataPoint, Plot, RegressionKind, Series, SeriesKind};
pub use render::{PlotAnchor, PlotPrim, PlotRendered};

/// Maximum series in a single plot.
pub(crate) const MAX_SERIES: usize = 32;
/// Maximum adaptive function/parametric sample evaluations per series.
pub(crate) const MAX_SAMPLES: usize = 2_048;
/// Maximum data points per experimental series.
pub(crate) const MAX_DATA_POINTS: usize = 2_048;
/// Largest serialized JSON accepted by [`Plot::from_json`], in bytes.
pub(crate) const MAX_JSON_BYTES: usize = 2 << 20;
