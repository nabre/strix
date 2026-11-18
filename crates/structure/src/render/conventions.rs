//! The sign conventions a diagram's values follow, drawn beside its title as the textbooks draw
//! them on a short piece of member: N positive pulling (tension), T positive turning the piece
//! clockwise, M positive stretching the reference fibres (dashed, below a member drawn left to
//! right). And a reaction that came out negative: the way it was taken positive, faded and
//! crossed out, beside the arrow the way it really acts.

use super::{BLACK, Ctx, ERROR, P, Pen, Rgb, add, lighter, mul, pd, unit};
use crate::diagram::Action;

/// Half the piece of member's length (paper mm).
const HALF: f64 = 5.0;
/// The arrows' length.
const LEN: f64 = 4.0;
/// The room between the title and the convention.
pub(super) const AFTER_TITLE: f64 = 4.0;

impl Pen {
    /// The sign convention of `action` on a piece of member, its left end `HALF` before `at`'s
    /// centre (`at` is the drawing's left edge, half way up), the forces in colour `c`.
    pub(super) fn convention(&mut self, at: P, action: Action, c: Rgb) {
        let w = 0.35;
        match action {
            Action::Axial => {
                // The piece, and a force at each end pulling it apart.
                let (l, r) = (add(at, (LEN + 0.6, 0.0)), add(at, (LEN + 0.6 + 2.0 * HALF, 0.0)));
                self.line(l, r, BLACK, 0.5);
                self.arrow(l, add(l, (-LEN, 0.0)), c, w);
                self.arrow(r, add(r, (LEN, 0.0)), c, w);
            }
            Action::Shear => {
                // Up on the left face, down on the right one: a clockwise pair.
                let (l, r) = (add(at, (0.6, 0.0)), add(at, (0.6 + 2.0 * HALF, 0.0)));
                self.line(l, r, BLACK, 0.5);
                self.arrow(add(l, (0.0, LEN / 2.0)), add(l, (0.0, -LEN / 2.0)), c, w);
                self.arrow(add(r, (0.0, -LEN / 2.0)), add(r, (0.0, LEN / 2.0)), c, w);
            }
            Action::Moment => {
                // Clockwise on the left face, counter-clockwise on the right: the reference
                // fibres (dashed, below) stretched.
                let r = 2.2;
                let (l, rt) = (add(at, (r + 0.4, 0.0)), add(at, (r + 0.4 + 2.0 * HALF, 0.0)));
                self.line(l, rt, BLACK, 0.5);
                self.dashed(add(l, (0.0, 0.8)), add(rt, (0.0, 0.8)), BLACK, 0.2);
                self.end_couple(l, r, true, c, w);
                self.end_couple(rt, r, false, c, w);
            }
        }
    }

    /// Half a turn round a piece's end `p` on its outer side: on the left end (`left`) from below
    /// round the left to the top (clockwise on the page), on the right end from below round the
    /// right to the top (counter-clockwise); its head at the top, pointing back over the piece.
    fn end_couple(&mut self, p: P, r: f64, left: bool, c: Rgb, w: f64) {
        let n = 12;
        let s = if left { -1.0 } else { 1.0 };
        let pts: Vec<P> = (0..=n)
            .map(|k| {
                let t = std::f64::consts::PI * f64::from(k) / f64::from(n);
                (p.0 + s * r * t.sin(), p.1 + r * t.cos())
            })
            .collect();
        let (Some(&tip), Some(&before)) = (pts.last(), pts.get(pts.len().saturating_sub(3))) else { return };
        let mut arc = pts;
        arc.truncate(arc.len().saturating_sub(2));
        self.poly(arc, c, w);
        self.arrow(before, add(tip, (-s * 0.6, 0.0)), c, w);
    }

    /// A reaction's arrow the way it was taken positive, from `from` to `to`, faded and crossed
    /// out: its value came out negative, it acts the other way.
    pub(super) fn crossed_arrow(&mut self, from: P, to: P, c: Rgb) {
        self.arrow(from, to, lighter(c, 0.55), 0.3);
        self.cross(mul(add(from, to), 0.5), 1.3);
    }

    /// A couple the way it was taken positive (counter-clockwise), faded and crossed out.
    pub(super) fn crossed_couple(&mut self, p: P, r: f64, c: Rgb) {
        self.couple(p, r, true, lighter(c, 0.55), 0.3);
        // On the arc, up and to the right of the centre (the couple's arc starts below left).
        let a = 30.0_f64.to_radians();
        self.cross((p.0 + r * a.cos(), p.1 - r * a.sin()), 1.3);
    }

    /// A ✕ of half-size `h` centred on `p`.
    fn cross(&mut self, p: P, h: f64) {
        let d = mul(unit((1.0, 1.0)), h);
        self.line(add(p, mul(d, -1.0)), add(p, d), ERROR, 0.35);
        self.line(add(p, (-d.0, d.1)), add(p, (d.0, -d.1)), ERROR, 0.35);
    }
}

impl Ctx<'_> {
    /// `dir` (a paper direction) or its opposite, whichever points away from the middle of the
    /// structure as seen from node `id`.
    pub(super) fn outwards(&self, id: &str, dir: P) -> P {
        let st = self.st;
        let n = st.nodes.len().max(1) as f64;
        let mid = st.nodes.iter().fold((0.0, 0.0), |m, q| (m.0 + q.x.value / n, m.1 + q.y.value / n));
        let Some(at) = st.node(id) else { return dir };
        let away = pd((at.x.value - mid.0, at.y.value - mid.1));
        if away.0 * dir.0 + away.1 * dir.1 < 0.0 { mul(dir, -1.0) } else { dir }
    }
}

#[cfg(test)]
mod tests {
    use strix_plot::PlotPrim;

    use super::{BLACK, ERROR};
    use crate::{Load, Member, Node, Num, Structure, Support, SupportKind};

    /// A 3 m cantilever fixed at A with a force at its tip B, upward when `up`.
    fn cantilever(up: bool) -> Structure {
        let node = |id: &str, x: f64| Node { id: id.into(), x: Num::new(x), y: Num::new(0.0), ..Node::default() };
        Structure {
            nodes: vec![node("A", 0.0), node("B", 3.0)],
            members: vec![Member { id: "1".into(), a: "A".into(), b: "B".into(), ..Member::default() }],
            supports: vec![Support { node: "A".into(), kind: SupportKind::Fixed, ..Support::default() }],
            loads: vec![Load::Force {
                node: Some("B".into()),
                member: None,
                at: Num::default(),
                value: Num::new(10.0),
                angle: Num::new(if up { 90.0 } else { -90.0 }),
            }],
            ..Structure::default()
        }
    }

    /// The red strokes: the crosses on the reactions that came out negative.
    fn crosses(st: &Structure) -> usize {
        st.render(None).plot.prims.iter().filter(|p| matches!(p, PlotPrim::Path { stroke: Some((c, _)), .. } if *c == ERROR)).count() / 2
    }

    #[test]
    fn negative_reactions_have_their_positive_way_crossed_out() {
        // Pushed up at the tip: V_A down and M_A clockwise, both negative.
        assert_eq!(crosses(&cantilever(true)), 2);
        // Pushed down: V_A up, M_A counter-clockwise, both positive.
        assert_eq!(crosses(&cantilever(false)), 0);
    }

    #[test]
    fn each_diagram_title_has_its_sign_convention() {
        // The pieces of member the conventions are drawn on (solid black, 0.5 mm).
        let pieces = |st: &Structure| {
            let r = st.render(None);
            r.plot
                .prims
                .iter()
                .filter(|p| matches!(p, PlotPrim::Path { stroke: Some((c, w)), dash, .. } if *c == BLACK && *w == 0.5 && dash.is_empty()))
                .count()
        };
        let mut st = cantilever(false);
        assert_eq!(pieces(&st), 3);
        st.diagrams.v.show = false;
        assert_eq!(pieces(&st), 2);
        st.view = crate::View::Moment;
        assert_eq!(pieces(&st), 1);
        st.view = crate::View::Structure;
        assert_eq!(pieces(&st), 0);
    }
}
