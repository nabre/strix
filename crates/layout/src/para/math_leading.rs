//! Strix: the room a line keeps around inline math.
//!
//! A line with an equation is as tall as the equation reaches, and a little more: subscripts,
//! fractions and the parentheses around a substituted value (`(10 kN/m)`) never touch the line
//! above or below, as they did when the line took only the math's own height. With automatic or
//! at-least spacing a line whose math is more than plain symbols (a script, a fraction, a root, a
//! bracket or a bar) keeps `MATH_LEADING` of its font size free above and below it; plain symbols
//! on the baseline (`x`, `a + b` in a sentence) leave the line as the text around it, so prose
//! with inline symbols keeps even spacing. Exact spacing still fixes the line box.

use super::model::StyleRun;
use crate::math::{MItem, MathLayout};

/// The room kept above and below inline math, as a share of the run's font size.
const MATH_LEADING: f32 = 0.25;

/// Characters that reach above and below a letter's box.
const BRACKETS: &[char] = &['(', ')', '[', ']', '{', '}', '|', '‖', '⟨', '⟩', '⌊', '⌋', '⌈', '⌉'];

/// Is `math` set as plain text is: glyphs on the baseline at one size, no bracket, no line or bar
/// (a fraction's, a root's)? What the editor draws only on screen doesn't count.
pub(crate) fn plain(math: &MathLayout) -> bool {
    let mut size = None;
    math.items.iter().all(|item| match item {
        MItem::Glyphs { size: s, glyphs, text, .. } => {
            let one = *size.get_or_insert(*s) == *s;
            one && glyphs.iter().all(|g| g.2 == 0.0) && !text.contains(BRACKETS)
        }
        MItem::Rect { .. } | MItem::Line { .. } => false,
        MItem::ScreenOnly(_) => true,
    })
}

/// The ascent and descent a line takes for inline equation `math`, `obj_h` above and `obj_d`
/// below its baseline, set in `st`: never less than the font's own line, and, when the math is
/// more than plain symbols, the math plus its leading.
pub(super) fn math_line_extent(st: &StyleRun, math: Option<&MathLayout>, obj_h: f32, obj_d: f32) -> (f32, f32) {
    let (a, d) = st.line_extent();
    if math.is_some_and(plain) {
        return (a.max(obj_h), d.max(obj_d));
    }
    let size = if st.rc.size.is_finite() { st.rc.size.clamp(0.0, 1638.0) } else { 0.0 };
    let pad = size * MATH_LEADING;
    (a.max(obj_h + pad), d.max(obj_d + pad))
}
