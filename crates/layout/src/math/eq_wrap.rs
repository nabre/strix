//! Strix: a display equation array wider than its line wraps row by row. Each row breaks at its
//! operators like any long display equation ([`Ctx::wrapped`]); the rows keep their alignment at
//! `&` as far as the line allows. A Strix calculation's steps (`calc` with `{"layout":"lines"}`)
//! and a structure's written working are such arrays.

use super::bx::{Bx, St, finalize, gaps_between};
use super::{Arg, Ctx, MNode, MathProps};

/// Line spacing of a calculation's steps (an equation marked `stepped`: a solution's algebra,
/// substitution and result, a structure's working), as a multiple of single spacing: wider than
/// an ordinary equation array's, so that one step stands clear of the next.
const STEPS_LINE_SPACING: f32 = 1.5;

/// The line spacing of `math`'s equation-array rows: [`STEPS_LINE_SPACING`] for a calculation's
/// steps, single otherwise.
pub(super) fn row_spacing(math: &wordcraft_doc::math::Math) -> f32 {
    if math.stepped { STEPS_LINE_SPACING } else { 1.0 }
}

impl Ctx {
    /// Least space between an equation array's rows and least baseline-to-baseline distance, in
    /// points at `em`: single spacing is the array's own; wider spacing multiplies its line
    /// (`EQ_ROW_SKIP`) and adds the extra to the clearance between tall rows as well.
    pub(super) fn eq_row_spacing(&self, em: f32) -> (f32, f32) {
        let extra = (self.row_spacing.clamp(1.0, 3.0) - 1.0) * super::rows::EQ_ROW_SKIP;
        (em * (super::rows::EQ_ROW_GAP + extra), em * (super::rows::EQ_ROW_SKIP + extra))
    }

    /// The equation array `rows` (the whole of display equation `nodes`) in `avail` points.
    pub(super) fn eq_arr_wrapped(&mut self, nodes: &[MNode], rows: &[Arg], st: St, avail: f32, mp: &MathProps) -> Bx {
        let budget = self.nodes;
        let whole = self.wrapped(nodes, st, avail, mp);
        if whole.w <= avail + 0.01 || avail <= 0.0 || rows.is_empty() {
            return whole;
        }
        self.nodes = budget;
        let em = self.em(st);
        self.node_stack.push(0);
        // Each aligned row's width before its first `&`.
        let mut leads: Vec<Option<f32>> = Vec::new();
        for (ri, r) in rows.iter().take(1000).enumerate() {
            self.path.push((0, ri));
            let before = self.nodes;
            let atoms = self.row(r, st, 1, 0);
            self.nodes = before;
            self.path.pop();
            let boxes: Vec<Bx> = atoms.iter().map(|(b, _)| b.clone()).collect();
            let gaps = gaps_between(&boxes, st, em);
            let aligned = atoms.iter().any(|(_, s)| *s > 0);
            let w0: f32 = atoms.iter().enumerate().filter(|(_, (_, s))| *s == 0).map(|(i, (b, _))| b.w + gaps.get(i).copied().unwrap_or(0.0)).sum();
            leads.push(aligned.then_some(w0));
        }
        let col = leads.iter().flatten().fold(0.0f32, |a, b| a.max(*b)).min(avail * 0.5);
        let mut placed = Vec::new();
        for (ri, r) in rows.iter().take(1000).enumerate() {
            let x = leads.get(ri).copied().flatten().map_or(0.0, |w0| (col - w0).max(0.0));
            self.path.push((0, ri));
            let b = self.wrapped(r, st, (avail - x).max(avail * 0.5), mp);
            self.path.pop();
            placed.push((b, x));
        }
        self.node_stack.pop();
        let (gap, skip) = self.eq_row_spacing(em);
        let (mut out, _) = self.stack_rows(placed, st, gap, skip);
        // The caret before and after the array, as when it is laid out whole.
        let w = out.w;
        out.slot(&[], 0, 0.0);
        out.slot(&[], 1, w);
        finalize(&mut out, &[], em);
        out
    }
}

#[cfg(test)]
mod tests {
    use crate::math::{MItem, MathLayout, layout_equation};
    use wordcraft_doc::math::{MNode, Math, MathProps, parse_linear};

    fn lay(lin: &str, avail: f32) -> MathLayout {
        crate::test_fonts();
        let rc = wordcraft_doc::styles::StyleSheet::default().resolve_char(None, &Default::default());
        let math = Math { nodes: parse_linear(lin), ..Default::default() };
        layout_equation(&math, lin, &rc, true, avail, &MathProps::default(), &mut 0)
    }

    /// The baselines of the glyph runs, top first.
    fn baselines(m: &MathLayout) -> Vec<f32> {
        let mut ys: Vec<f32> = Vec::new();
        for it in &m.items {
            if let MItem::Glyphs { glyphs, .. } = it
                && let Some(&(_, _, y)) = glyphs.first()
                && !ys.iter().any(|v| (v - y).abs() < 0.5)
            {
                ys.push(y);
            }
        }
        ys
    }

    /// A calculation's steps are spaced one and a half lines apart, wider than an ordinary
    /// equation array, with fractions as well as plain rows.
    #[test]
    fn a_calculations_steps_are_spaced_wider_than_an_equation_array() {
        crate::test_fonts();
        let rc = wordcraft_doc::styles::StyleSheet::default().resolve_char(None, &Default::default());
        for lin in ["█(F&=m·a@a&=F/m@a&=2.5)", "█(d^2&=(4·A)/π@d&=√((4·A)/π)@d&=(1 kN)/(400 kg))"] {
            let gaps = |stepped: bool| -> Vec<f32> {
                let math = Math { nodes: parse_linear(lin), stepped, ..Default::default() };
                let m = layout_equation(&math, lin, &rc, true, 1e4, &MathProps::default(), &mut 0);
                let rows: Vec<f32> = (0..3).filter_map(|r| m.slots.iter().filter(|s| s.path == [(0, r)]).map(|s| s.y).next()).collect();
                rows.windows(2).map(|w| (w[1] - w[0]).abs()).collect()
            };
            let (plain, steps) = (gaps(false), gaps(true));
            assert_eq!((plain.len(), steps.len()), (2, 2), "{lin}");
            let em = rc.draw_size();
            for (p, s) in plain.iter().zip(&steps) {
                assert!(*s >= p + em * 0.5, "{lin}: steps {steps:?} vs array {plain:?}");
                assert!(*s >= em * 1.4 * 1.5 - 0.5, "{lin}: at least one and a half lines: {steps:?}");
            }
        }
    }

    #[test]
    fn a_wide_equation_array_wraps_each_row_at_its_operators() {
        let lin = "█(0&=a+b+c+d+e+f+g+h+i+j+k+l+m+n+o+p+q+r+s+t+u+v+w@x&=1)";
        assert!(matches!(parse_linear(lin).as_slice(), [MNode::EqArr { .. }]));
        let full = lay(lin, 1e4);
        assert_eq!(baselines(&full).len(), 2, "two rows when there is room");
        let narrow = lay(lin, full.width * 0.45);
        assert!(baselines(&narrow).len() >= 3, "the long row wraps: {:?}", baselines(&narrow));
        assert!(narrow.width <= full.width * 0.45 + 1.0, "fits the line: {} > {}", narrow.width, full.width * 0.45);
        // Caret slots before and after the array stay.
        assert!(narrow.slots.iter().any(|s| s.path.is_empty() && s.off == 1));
    }

    /// Rows with several alignment points wrap too, inside the line, and keep every caret
    /// position of every row, so the caret moves and clicks land in a wrapped array as in a
    /// whole one.
    #[test]
    fn a_wrapped_array_with_several_columns_keeps_its_caret_positions() {
        let lin = "█(x&=a+b+c+d+e+f+g+h+i+j&=k+l+m+n+o+p+q+r+s+t@y&=1&=2)";
        let full = lay(lin, 1e4);
        let avail = full.width * 0.4;
        let narrow = lay(lin, avail);
        assert!(narrow.width <= avail + 1.0, "fits the line: {} > {avail}", narrow.width);
        assert!(baselines(&narrow).len() > baselines(&full).len(), "it wraps: {:?}", baselines(&narrow));
        let offs = |m: &MathLayout, row: usize| -> Vec<usize> {
            let mut v: Vec<usize> = m.slots.iter().filter(|s| s.path == [(0, row)]).map(|s| s.off).collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        for row in 0..2 {
            assert!(!offs(&full, row).is_empty(), "row {row} has caret positions");
            assert_eq!(offs(&narrow, row), offs(&full, row), "row {row}");
        }
        // A click on the wrapped part of the first row lands in it.
        let lowest = narrow.slots.iter().filter(|s| s.path == [(0, 0)]).map(|s| s.y).fold(f32::MAX, f32::min);
        let hit = narrow.hit(avail * 0.5, lowest).expect("a caret position");
        assert_eq!(hit.path, [(0, 0)]);
    }

    #[test]
    fn hostile_widths_never_panic() {
        for avail in [0.0, -5.0, f32::NAN, f32::INFINITY, 1.0] {
            let _ = lay("█(a&=b+c@d&=e+f+g)", avail);
        }
        let _ = lay("█()", 1.0);
    }
}
