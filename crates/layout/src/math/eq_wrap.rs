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

    /// Least space between a long equation's wrapped lines and least baseline-to-baseline
    /// distance, in points at `em`: a calculation's lines are spaced as its steps
    /// ([`STEPS_LINE_SPACING`]), so a fraction or a bracket on one line keeps well clear of the
    /// next; other equations keep Word's tighter spacing.
    pub(super) fn wrap_line_spacing(&self, em: f32) -> (f32, f32) {
        if self.align_relations {
            let extra = (STEPS_LINE_SPACING - 1.0) * super::rows::EQ_ROW_SKIP;
            (em * (super::rows::EQ_ROW_GAP + extra), em * (super::rows::EQ_ROW_SKIP + extra))
        } else {
            (em * 0.3, em * 1.3)
        }
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

    /// An equation in a line of text (`nodes`), broken at its operators like a display equation
    /// where it is wider than the paragraph's `avail` points: its first line sits on the text's
    /// baseline, the rest below it.
    pub(super) fn inline_wrapped(&mut self, nodes: &[MNode], st: St, avail: f32, mp: &MathProps) -> Bx {
        let budget = self.nodes;
        let whole = self.arg(nodes, st, 0);
        if whole.w <= avail + 0.01 || avail <= 0.0 {
            return whole;
        }
        self.nodes = budget;
        match nodes {
            [MNode::EqArr { rows }] => self.eq_arr_wrapped(nodes, rows, st, avail, mp),
            _ => self.wrapped(nodes, st, avail, mp),
        }
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

    fn lay_with(lin: &str, display: bool, calc: bool, avail: f32) -> MathLayout {
        crate::test_fonts();
        let rc = wordcraft_doc::styles::StyleSheet::default().resolve_char(None, &Default::default());
        let math = Math { nodes: parse_linear(lin), calc, ..Default::default() };
        layout_equation(&math, lin, &rc, display, avail, &MathProps::default(), &mut 0)
    }

    /// Each line's left edge and baseline, top first: from the caret positions of the equation's
    /// top level, which sit on its lines' baselines (glyph runs also sit on scripts' and
    /// fractions' baselines).
    fn line_starts(m: &MathLayout) -> Vec<(f32, f32)> {
        let mut out: Vec<(f32, f32)> = Vec::new();
        for s in m.slots.iter().filter(|s| s.path.is_empty()) {
            match out.iter_mut().find(|(_, y)| (y - s.y).abs() < 0.5) {
                Some(e) => e.0 = e.0.min(s.x),
                None => out.push((s.x, s.y)),
            }
        }
        out.sort_by(|a, b| b.1.total_cmp(&a.1));
        out
    }

    /// A structure's working writes `T(2.5 m) = … = …` as an equation in a line of text: wider
    /// than the paragraph, it breaks at its operators instead of running past the margin, its
    /// first line on the text's baseline.
    #[test]
    fn an_inline_equation_wider_than_the_line_wraps() {
        let lin = "T_1a (2.5 m)=V_A-q_1·2.5 m=(19.792 kN)-(10 kN/m)·2.5 m=19.792 kN-25 kN=-5.208 kN";
        let full = lay_with(lin, false, true, 1e4);
        assert_eq!(line_starts(&full).len(), 1);
        let avail = full.width * 0.6;
        let narrow = lay_with(lin, false, true, avail);
        assert!(line_starts(&narrow).len() >= 2, "it wraps: {:?}", line_starts(&narrow));
        assert!(narrow.width <= avail + 1.0, "fits the line: {} > {avail}", narrow.width);
        assert!(line_starts(&narrow).first().is_some_and(|(_, y)| y.abs() < 0.01), "first line on the baseline");
        assert!(narrow.ascent <= full.ascent + 0.5);
        assert!(narrow.descent > full.descent + 5.0, "the rest below it");
        // Every caret position is kept.
        let offs = |m: &MathLayout| -> Vec<usize> {
            let mut v: Vec<usize> = m.slots.iter().filter(|s| s.path.is_empty()).map(|s| s.off).collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        assert_eq!(offs(&narrow), offs(&full));
    }

    /// An inline equation that fits is laid out as before.
    #[test]
    fn an_inline_equation_that_fits_stays_on_one_line() {
        let lin = "a+b=c";
        let wide = lay_with(lin, false, false, 1e4);
        let tight = lay_with(lin, false, false, wide.width + 0.5);
        assert_eq!(line_starts(&tight).len(), 1);
        assert!((tight.width - wide.width).abs() < 0.01);
    }

    /// A calculation's continuation lines start under its first `=` and break between the steps
    /// of the chain (at a relation) rather than inside one.
    #[test]
    fn a_calculation_wraps_at_its_relations_under_the_first_one() {
        let lin = "T_1a (2.5 m)=V_A-q_1·2.5 m=(19.792 kN)-(10 kN/m)·2.5 m=19.792 kN-25 kN=-5.208 kN";
        let full = lay_with(lin, true, true, 1e4);
        let avail = full.width * 0.6;
        let calc = lay_with(lin, true, true, avail);
        let starts = line_starts(&calc);
        assert!(starts.len() >= 2, "it wraps: {starts:?}");
        let first_eq = lay_with("T_1a (2.5 m)", true, true, 1e4).width;
        for (x, _) in starts.iter().skip(1) {
            assert!(*x > first_eq * 0.8 && *x < first_eq * 1.6, "under the first relation ({first_eq}): {starts:?}");
        }
        // Not a calculation: the wrap indent (72 pt) as Word does.
        let plain = lay_with(lin, true, false, avail);
        let plain_starts = line_starts(&plain);
        assert!(plain_starts.iter().skip(1).all(|(x, _)| (x - 72.0).abs() < 3.0), "{plain_starts:?}");
    }

    /// Once a calculation has broken, every later `=` starts a line: a step that took two lines
    /// is not followed by the next `=` at the end of its last one.
    #[test]
    fn after_a_break_each_of_a_calculations_steps_starts_a_line() {
        let lin = "M_3b (3 m)=-H_A·1 m+V_A·(3 m+1.5 m)-F_1·(1 m)/2=-(-15 kN)·1 m+(1.38 kN)·(3 m+1.5 m)-(10 kN)·(1 m)/2-(10 kN/m)·2 m=15 kN·m+6.21 kN·m-5 kN·m-20 kN·m=-3.79 kN·m";
        let full = lay_with(lin, true, true, 1e4);
        let calc = lay_with(lin, true, true, full.width * 0.45);
        // Each line's first caret offset, top first.
        let mut lines: Vec<(f32, usize)> = Vec::new();
        for s in calc.slots.iter().filter(|s| s.path.is_empty()) {
            match lines.iter_mut().find(|(y, _)| (y - s.y).abs() < 0.5) {
                Some(e) => e.1 = e.1.min(s.off),
                None => lines.push((s.y, s.off)),
            }
        }
        lines.sort_by(|a, b| b.0.total_cmp(&a.0));
        let starts: Vec<usize> = lines.iter().map(|l| l.1).collect();
        assert!(starts.len() >= 4, "it wraps: {starts:?}");
        // The `=`s' unit offsets at the top level (a run's characters, one per structure).
        let mut eqs = Vec::new();
        let mut u = 0usize;
        for node in parse_linear(lin) {
            match node {
                wordcraft_doc::math::MNode::Run(r) => {
                    for ch in r.text.chars() {
                        if ch == '=' {
                            eqs.push(u);
                        }
                        u += 1;
                    }
                }
                _ => u += 1,
            }
        }
        let eqs: Vec<usize> = eqs.into_iter().skip(1).collect();
        assert_eq!(eqs.len(), 3);
        let first_auto = starts.get(1).copied().unwrap_or(0);
        for e in eqs.iter().filter(|&&e| e >= first_auto) {
            assert!(starts.contains(e), "`=` at {e} starts a line: {starts:?}");
        }
    }

    /// A calculation's wrapped lines are spaced as its steps, one and a half lines apart, wider
    /// than an ordinary long equation's.
    #[test]
    fn a_calculations_wrapped_lines_are_spaced_as_its_steps() {
        let lin = "M=V_A·2.5 m-F_1·(2.5 m-0.875 m)-F_2·sin 75°·(2.5 m-2.5 m)=41.137 kN·m";
        let full = lay_with(lin, true, true, 1e4);
        let skip = |calc: bool| -> f32 {
            let m = lay_with(lin, true, calc, full.width * 0.5);
            let ys: Vec<f32> = line_starts(&m).iter().map(|l| l.1).collect();
            assert!(ys.len() >= 2, "it wraps: {ys:?}");
            ys.windows(2).map(|w| (w[0] - w[1]).abs()).fold(f32::MAX, f32::min)
        };
        let em = wordcraft_doc::styles::StyleSheet::default().resolve_char(None, &Default::default()).draw_size();
        let (calc, plain) = (skip(true), skip(false));
        assert!(calc >= em * 1.4 * 1.5 - 0.5, "one and a half lines: {calc} at {em}");
        assert!(calc > plain + em * 0.5, "wider than an ordinary equation's: {calc} vs {plain}");
    }

    #[test]
    fn hostile_widths_never_panic() {
        for avail in [0.0, -5.0, f32::NAN, f32::INFINITY, 1.0] {
            let _ = lay("█(a&=b+c@d&=e+f+g)", avail);
            let _ = lay_with("a=b+c+d=e-f=g", false, true, avail);
        }
        let _ = lay("█()", 1.0);
    }
}
