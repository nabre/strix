//! Strix: a calculation's fraction wider than its line breaks inside. Word breaks a long equation
//! only at its top-level operators, so a fraction is one block: a structure's working
//! substituting every load and arm into `V_E = (F₃·d₃ + F₂·d₂ + … − (…))/(3 m + 3 m)` ran past
//! the margin with half of it hidden. In a Strix calculation (`calc`) a top-level fraction whose
//! numerator or denominator is wider than the room it has after the first relation is laid out
//! with that part broken at its own operators, its lines over the bar, so every term shows.

use super::bx::{Bx, Cls, St, classes, gaps_between};
use super::{Ctx, FracKind, MNode, MathProps};

/// The width the numerator and denominator of the fractions of one equation row may take.
#[derive(Clone, Debug)]
pub(super) struct FracWrap {
    /// Widest numerator or denominator, points.
    cap: f32,
    /// How their lines break.
    mp: MathProps,
    /// Length of the row's path: only its own fractions (not nested ones) are broken.
    depth: usize,
}

/// A part's continuation lines are indented this many ems.
const PART_INDENT_EM: f32 = 1.0;

impl Ctx {
    /// The atoms of display row `a` for [`Ctx::wrapped`] in `avail` points: a calculation's
    /// fractions that would not fit come with their numerator or denominator broken.
    pub(super) fn wrapped_atoms(&mut self, a: &[MNode], st: St, avail: f32, mp: &MathProps) -> Vec<Bx> {
        let budget = self.nodes;
        let atoms: Vec<Bx> = self.row(a, st, 0, 0).into_iter().map(|(b, _)| b).collect();
        let has_frac = a.iter().any(|n| matches!(n, MNode::Frac { kind: FracKind::Bar, .. }));
        if !self.align_relations || !st.display || !has_frac || !avail.is_finite() || avail <= 0.0 {
            return atoms;
        }
        let em = self.em(st);
        let gaps = gaps_between(&atoms, st, em);
        let total: f32 = atoms.iter().map(|b| b.w).sum::<f32>() + gaps.iter().sum::<f32>();
        let room = room_after_relation(&atoms, &gaps, em, avail, mp);
        if total <= avail + 0.01 || !atoms.iter().any(|b| b.glyph.is_none() && b.w > room + 0.01) {
            return atoms;
        }
        // The part's lines and the bar's padding inside the room.
        let cap = (room - em * 0.2).max(avail * 0.3);
        let outer = self.frac_wrap.replace(FracWrap { cap, mp: mp.clone(), depth: self.path.len() });
        self.nodes = budget;
        let atoms = self.row(a, st, 0, 0).into_iter().map(|(b, _)| b).collect();
        self.frac_wrap = outer;
        atoms
    }

    /// Numerator `n` and denominator `dn` (laid out from `num` in `ns` and `den` in `ds`) of a
    /// fraction, each broken into lines when it is a row's fraction too wide for it
    /// ([`Ctx::wrapped_atoms`]).
    pub(super) fn wrap_frac_parts(&mut self, num: &[MNode], den: &[MNode], parts: (Bx, Bx), styles: (St, St)) -> (Bx, Bx) {
        let (n, dn) = parts;
        let Some(fw) = self.frac_wrap.take_if(|f| f.depth == self.path.len()) else { return (n, dn) };
        let em = self.em(styles.0);
        let mp = MathProps { wrap_indent: em * PART_INDENT_EM, wrap_right: false, ..fw.mp.clone() };
        let me = self.node_stack.last().copied().unwrap_or(0);
        let part = |cx: &mut Ctx, c: usize, a: &[MNode], b: Bx, s: St| -> Bx {
            if b.w <= fw.cap || a.is_empty() {
                return b;
            }
            cx.path.push((me, c));
            let w = cx.wrapped(a, s, fw.cap, &mp);
            cx.path.pop();
            w
        };
        let n = part(self, 0, num, n, styles.0);
        let dn = part(self, 1, den, dn, styles.1);
        self.frac_wrap = Some(fw);
        (n, dn)
    }
}

/// The room a fraction of `atoms` (a row `avail` points wide, `gaps` between them) has: what is
/// left after the first relation and its space, where the fraction sits on the first line and
/// under which the continuation lines start; after the wrap indent when there is no relation in
/// the first half.
fn room_after_relation(atoms: &[Bx], gaps: &[f32], em: f32, avail: f32, mp: &MathProps) -> f32 {
    let cls = classes(atoms);
    let rel = (1..atoms.len()).find(|&i| atoms.get(i).is_some_and(|b| b.glyph.is_some()) && cls.get(i).is_some_and(|c| c.0 == Cls::Rel));
    let after_rel = rel.map(|r| {
        let through: f32 = (0..=r).map(|i| atoms.get(i).map_or(0.0, |b| b.w) + if i > 0 { gaps.get(i).copied().unwrap_or(0.0) } else { 0.0 }).sum();
        through + gaps.get(r + 1).copied().unwrap_or(0.0)
    });
    let indent = if mp.wrap_indent.is_finite() { mp.wrap_indent.clamp(0.0, 1584.0) } else { 0.0 };
    let lead = after_rel.filter(|x| *x <= avail * 0.5 + em).unwrap_or(indent.min(avail * 0.5));
    avail - lead
}

#[cfg(test)]
mod tests {
    use crate::math::{MathLayout, layout_equation};
    use wordcraft_doc::math::{Math, MathProps, parse_linear};

    const LONG: &str = "V_E=((30 kN)·(7.5 m)+(30 kN)·(4.5 m)+(30 kN)·(1.5 m)-((33 kN)·3 m+(12 kN)·9 m))/(3 m+3 m)";

    fn lay(lin: &str, calc: bool, avail: f32) -> MathLayout {
        crate::test_fonts();
        let rc = wordcraft_doc::styles::StyleSheet::default().resolve_char(None, &Default::default());
        let math = Math { nodes: parse_linear(lin), calc, ..Default::default() };
        layout_equation(&math, lin, &rc, true, avail, &MathProps::default(), &mut 0)
    }

    fn offs(m: &MathLayout) -> Vec<(Vec<(usize, usize)>, usize)> {
        let mut v: Vec<_> = m.slots.iter().map(|s| (s.path.clone(), s.off)).collect();
        v.sort();
        v.dedup();
        v
    }

    /// A calculation's fraction wider than the line breaks its numerator over the bar, so the
    /// whole equation fits; every caret position stays.
    #[test]
    fn a_calculations_wide_fraction_breaks_its_numerator() {
        let full = lay(LONG, true, 1e4);
        let avail = full.width * 0.55;
        let calc = lay(LONG, true, avail);
        assert!(calc.width <= avail + 1.0, "fits the line: {} > {avail}", calc.width);
        assert!(calc.ascent > full.ascent + 5.0, "the numerator takes more lines: {} vs {}", calc.ascent, full.ascent);
        assert_eq!(offs(&calc), offs(&full));
        // An ordinary equation keeps Word's behaviour: the fraction is one block.
        let plain = lay(LONG, false, avail);
        assert!(plain.width > avail + 1.0, "{} <= {avail}", plain.width);
    }

    /// A fraction that fits, and a calculation's line with room, are laid out as before.
    #[test]
    fn a_fraction_that_fits_is_unchanged() {
        let full = lay(LONG, true, 1e4);
        let roomy = lay(LONG, true, full.width + 1.0);
        assert!((roomy.width - full.width).abs() < 0.01 && (roomy.ascent - full.ascent).abs() < 0.01);
    }

    #[test]
    fn hostile_widths_never_panic() {
        for avail in [0.0, -5.0, f32::NAN, f32::INFINITY, 1.0, 20.0] {
            let _ = lay(LONG, true, avail);
            let _ = lay("x=(a/(b+c+d+e+f+g))/(h+i)", true, avail);
        }
    }
}
