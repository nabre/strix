//! Strix: where a long equation breaks. Lines are filled at the operators as Word does, but a
//! line that has to break goes back to its last relation (`=`, `<`, `≈`…) when that keeps it at
//! least half full, so a chain of equalities breaks between its steps rather than inside one;
//! and in a Strix calculation (`calc`) the lines after the first line line up under the first
//! relation instead of the fixed wrap indent, the way a worked solution is written by hand, and a
//! line that would break inside a product (before a `·`) goes back to its last `+` or `−` when
//! that keeps it at least half full, so a term such as `F_2·d_(2,A)` stays whole; once a
//! calculation has had to break, each later relation starts a line of its own, so a step that
//! took more than one line is never followed by the next `=` at the end of its last line.

/// One stretch of a row between two break opportunities.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Chunk {
    /// Width of the chunk's atoms with the spacing between them.
    pub w: f32,
    /// Spacing before the chunk when it follows another one on the same line.
    pub lead: f32,
    /// Width the chunk adds when it starts a line (a repeated operator and its spacing).
    pub copy_w: f32,
    /// A manual break (`m:brk`) starts a line here.
    pub forced: bool,
    /// The chunk starts at a relation: the best place to break.
    pub rel: bool,
    /// The chunk starts inside a product (at a `·` of a calculation): the worst place to break.
    pub weak: bool,
}

/// A line must be at least this full (of its width) to break at its last relation rather than
/// at the operator where it runs out of room.
const REL_BREAK_FILL: f32 = 0.5;

/// Lines (`[first, end)` chunk ranges) for `chunks` in `first` points on the first line and
/// `rest` on the others, and whether a line broke for lack of room. With `steps` (a calculation),
/// after the first line that broke for lack of room every relation starts a line.
pub(super) fn fill_lines(chunks: &[Chunk], first: f32, rest: f32, steps: bool) -> (Vec<(usize, usize)>, bool) {
    let n = chunks.len();
    let mut lines = Vec::new();
    let mut auto = false;
    let mut c = 0usize;
    while c < n {
        let avail = if lines.is_empty() { first } else { rest };
        let start = c;
        let mut w = chunks.get(c).map_or(0.0, |k| if c > 0 { k.copy_w + k.w } else { k.w });
        c += 1;
        // The last relation on this line, its last break outside a product, and the line's
        // width before each.
        let mut last_rel: Option<(usize, f32)> = None;
        let mut last_strong: Option<(usize, f32)> = None;
        while let Some(k) = chunks.get(c) {
            if k.forced || (steps && auto && k.rel) || w + k.lead + k.w > avail + 0.01 {
                break;
            }
            if k.rel {
                last_rel = Some((c, w));
            }
            if !k.weak {
                last_strong = Some((c, w));
            }
            w += k.lead + k.w;
            c += 1;
        }
        if let Some(k) = chunks.get(c)
            && !k.forced
            && !(steps && auto && k.rel)
        {
            auto = true;
            if !k.rel
                && let Some((j, before)) = last_rel
                && before >= avail * REL_BREAK_FILL
            {
                c = j;
            } else if k.weak
                && let Some((j, before)) = last_strong
                && before >= avail * REL_BREAK_FILL
            {
                c = j;
            }
        }
        lines.push((start, c));
    }
    (lines, auto)
}

/// `c` multiplies: breaking at it splits a term (`F_2·d_(2,A)`).
pub(super) fn is_product(c: char) -> bool {
    matches!(c, '·' | '⋅' | '∙' | '×' | '*')
}

/// The indent of a calculation's continuation lines: under its first relation `rel_x` points from
/// the left, when that is no more than half of `avail` (otherwise `indent`, the wrap indent).
pub(super) fn relation_indent(rel_x: Option<f32>, avail: f32, indent: f32) -> f32 {
    rel_x.filter(|x| x.is_finite() && *x > 0.0 && *x <= avail * 0.5).unwrap_or(indent)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(w: f32, rel: bool) -> Chunk {
        Chunk { w, lead: 2.0, copy_w: 0.0, forced: false, rel, weak: false }
    }

    #[test]
    fn a_line_breaks_at_its_last_relation_when_half_full() {
        // a | = b | − c | = d | − e | − f: room for 50.
        let cs = [chunk(10.0, false), chunk(10.0, true), chunk(10.0, false), chunk(10.0, true), chunk(10.0, false), chunk(10.0, false)];
        let (lines, auto) = fill_lines(&cs, 50.0, 50.0, false);
        assert!(auto);
        // Greedy would put four chunks (46) on the first line; the relation at 3 is past half.
        assert_eq!(lines, vec![(0, 3), (3, 6)]);
    }

    #[test]
    fn a_relation_too_early_leaves_the_greedy_break() {
        let cs = [chunk(10.0, false), chunk(10.0, true), chunk(10.0, false), chunk(10.0, false), chunk(10.0, false), chunk(10.0, false)];
        let (lines, _) = fill_lines(&cs, 50.0, 50.0, false);
        assert_eq!(lines.first(), Some(&(0, 4)));
    }

    /// A calculation's line that runs out of room inside a product goes back to its last `+`.
    #[test]
    fn a_line_does_not_break_inside_a_product_when_half_full() {
        // a | + b | · c | + d | · e: room for 40; greedy breaks before `· e`, splitting `d · e`.
        let weak = |w: f32| Chunk { weak: true, ..chunk(w, false) };
        let cs = [chunk(10.0, false), chunk(10.0, false), weak(5.0), chunk(8.0, false), weak(10.0)];
        let (lines, _) = fill_lines(&cs, 40.0, 40.0, false);
        assert_eq!(lines, vec![(0, 3), (3, 5)]);
        // No `+` past half the line: the greedy break stays.
        let cs = [chunk(30.0, false), weak(5.0), weak(10.0)];
        assert_eq!(fill_lines(&cs, 40.0, 40.0, false).0.first(), Some(&(0, 2)));
    }

    /// Once a calculation has broken, each later relation starts a line: a step that took two
    /// lines is not followed by the next `=` at the end of its second one.
    #[test]
    fn after_a_break_a_calculations_relations_start_lines() {
        // a | = b | − c | − d | = e | = f: room for 40.
        let cs = [chunk(10.0, false), chunk(10.0, true), chunk(10.0, false), chunk(8.0, false), chunk(5.0, true), chunk(5.0, true)];
        // Greedy fills `− d = e = f` on the second line (8 + 2 + 5 + 2 + 5).
        assert_eq!(fill_lines(&cs, 40.0, 40.0, false).0, vec![(0, 3), (3, 6)]);
        assert_eq!(fill_lines(&cs, 40.0, 40.0, true).0, vec![(0, 3), (3, 4), (4, 5), (5, 6)]);
        // An equation that fits keeps its relations on one line.
        assert_eq!(fill_lines(&cs, 100.0, 100.0, true).0, vec![(0, 6)]);
    }

    #[test]
    fn manual_breaks_and_wide_chunks_still_make_progress() {
        let mut cs = vec![chunk(80.0, false), chunk(80.0, true), chunk(5.0, false)];
        if let Some(k) = cs.get_mut(2) {
            k.forced = true;
        }
        let (lines, _) = fill_lines(&cs, 50.0, 50.0, false);
        assert_eq!(lines, vec![(0, 1), (1, 2), (2, 3)]);
        assert_eq!(fill_lines(&[], 50.0, 50.0, false).0, Vec::<(usize, usize)>::new());
        assert!(!fill_lines(&cs, f32::NAN, -1.0, false).0.is_empty());
    }

    #[test]
    fn continuation_lines_go_under_an_early_relation() {
        assert_eq!(relation_indent(Some(30.0), 200.0, 72.0), 30.0);
        assert_eq!(relation_indent(Some(150.0), 200.0, 72.0), 72.0);
        assert_eq!(relation_indent(None, 200.0, 72.0), 72.0);
        assert_eq!(relation_indent(Some(f32::NAN), 200.0, 72.0), 72.0);
    }
}
