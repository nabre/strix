//! Strix: the room a line keeps around inline math.
//!
//! A line with an equation is as tall as the equation reaches, and a little more: subscripts,
//! fractions and the parentheses around a substituted value (`(10 kN/m)`) never touch the line
//! above or below, as they did when the line took only the math's own height. With automatic or
//! at-least spacing every line holding math keeps `MATH_LEADING` of its font size free above and
//! below the math; exact spacing still fixes the line box.

use super::model::StyleRun;

/// The room kept above and below inline math, as a share of the run's font size.
const MATH_LEADING: f32 = 0.25;

/// The ascent and descent a line takes for an inline equation `obj_h` above and `obj_d` below
/// its baseline, set in `st`: never less than the font's own line, and the math plus its leading.
pub(super) fn math_line_extent(st: &StyleRun, obj_h: f32, obj_d: f32) -> (f32, f32) {
    let (a, d) = st.line_extent();
    let size = if st.rc.size.is_finite() { st.rc.size.clamp(0.0, 1638.0) } else { 0.0 };
    let pad = size * MATH_LEADING;
    (a.max(obj_h + pad), d.max(obj_d + pad))
}
