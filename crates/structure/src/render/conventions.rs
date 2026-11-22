//! The sign conventions a diagram's values follow, drawn beside its title as the textbooks draw
//! them on a short piece of member: N positive pulling (tension), T positive turning the piece
//! clockwise, M positive stretching the reference fibres (dashed, along the lower edge of a member drawn left to
//! right). And a reaction that came out negative: the way it was taken positive, faded and
//! crossed out, beside the arrow the way it really acts.

use strix_plot::PlotAnchor;

use super::{BLACK, Ctx, ERROR, P, Pen, Rgb, TEXT, add, lighter, mul, pd, unit};
use crate::diagram::Action;
use crate::model::Positive;

/// The room between the title and the convention.
pub(super) const AFTER_TITLE: f64 = 4.0;

impl Pen {
    /// Draw the positive internal action on a short member bounded by two cut faces.
    /// `at` is the left edge of the symbol at mid-height, in paper millimetres.
    pub(super) fn convention(&mut self, at: P, action: Action, c: Rgb) {
        let (left, right) = (4.6, 14.6);
        let h = 2.0;
        let outline = vec![
            (left, -h),
            (right, -h),
            (right - 0.5, -0.7),
            (right + 0.5, 0.7),
            (right, h),
            (left, h),
            (left + 0.5, 0.7),
            (left - 0.5, -0.7),
            (left, -h),
        ];
        self.poly(outline.into_iter().map(|p| add(at, p)).collect(), BLACK, 0.5);

        let w = 0.4;
        match action {
            Action::Axial => {
                self.arrow(add(at, (left - 0.7, 0.0)), add(at, (0.6, 0.0)), c, w);
                self.arrow(add(at, (right + 0.7, 0.0)), add(at, (18.6, 0.0)), c, w);
            }
            Action::Shear => {
                // Up on the left face, down on the right: a clockwise pair.
                let (l, r) = (left - 1.7, right + 1.7);
                self.arrow(add(at, (l, h)), add(at, (l, -h)), c, w);
                self.arrow(add(at, (r, -h)), add(at, (r, h)), c, w);
            }
            Action::Moment => {
                self.dashed(add(at, (left + 0.9, 1.15)), add(at, (right - 0.9, 1.15)), BLACK, 0.25);
                self.end_couple(add(at, (left - 1.6, 0.0)), 2.2, true, c, w);
                self.end_couple(add(at, (right + 1.6, 0.0)), 2.2, false, c, w);
            }
        }
    }

    /// The senses the reactions' equations count positive, as the textbooks draw them: an arrow
    /// along x and one along y from a common origin, each the way its axis counts positive and
    /// named at its tip, and a curved arrow marked `+` the way a rotation counts positive, in
    /// the quadrant between the two arrows, its opening towards their origin; in the title row
    /// whose top left corner is `at` (`TITLE_ROW` high). Bold heads, readable at a glance.
    pub(super) fn positive(&mut self, at: P, pos: Positive) {
        let (w, len, head, half) = (0.45, 7.5, 2.4, 0.42);
        let (sx, sy) = (pos.sx(), pos.sy());
        // The origin at the corner the arrows leave from (paper y grows downwards).
        let small = TEXT * 0.85;
        // With y downwards, room above the x arrow for its name.
        let o = add(at, (if sx > 0.0 { 1.0 } else { 1.0 + len }, if sy > 0.0 { 1.0 + len } else { 1.5 + small }));
        let tx = add(o, (sx * len, 0.0));
        let ty = add(o, (0.0, -sy * len));
        self.arrow_sized(o, tx, BLACK, w, head, half);
        self.arrow_sized(o, ty, BLACK, w, head, half);
        self.dot(o, 0.4, BLACK, None);
        // `x` by its tip and `y` beside its tip, both outside the quadrant the rotation takes.
        let xy = if sy > 0.0 { small * 1.15 } else { -small * 0.65 };
        self.text(add(tx, (0.0, xy)), "x".to_string(), small, false, PlotAnchor::Center, BLACK);
        let (yx, anchor) = if sx > 0.0 { (-1.1, PlotAnchor::Right) } else { (1.1, PlotAnchor::Left) };
        self.text(add(ty, (yx, small * 0.75)), "y".to_string(), small, false, anchor, BLACK);
        // The rotation in the quadrant between the arrows (x and y both positive), open towards
        // the origin (the angles counter-clockwise from the right, y up).
        let q = add(o, (sx * 4.5, -sy * 4.5));
        let gap = (-sy).atan2(-sx).to_degrees();
        self.turn(q, 2.5, pos.sr() > 0.0, (gap + 40.0, gap + 320.0), (w, head, half));
        self.text(add(q, (0.0, small * 0.38)), "+".to_string(), small, false, PlotAnchor::Center, BLACK);
    }

    /// The curved arrow of [`Pen::positive`] round `q`, radius `r`, over the angles `span`
    /// (degrees, counter-clockwise from the right, y up), turning counter-clockwise when `ccw`:
    /// the same stroke and the same head (`style`: width, head length, half width) as the axes'
    /// arrows, the head wrapped on the arc's last `head` mm so it keeps its full size.
    fn turn(&mut self, q: P, r: f64, ccw: bool, span: (f64, f64), style: (f64, f64, f64)) {
        let (w, head, half) = style;
        let (a0, a1) = (span.0.to_radians(), span.1.to_radians());
        let (from, to) = if ccw { (a0, a1) } else { (a1, a0) };
        let at = |t: f64| (q.0 + r * t.cos(), q.1 - r * t.sin());
        // The head's base on the arc, `head` mm (along the arc) before its point.
        let back = (head / r).min((to - from).abs() * 0.5);
        let base = to - back * (to - from).signum();
        let n = 24;
        let arc: Vec<P> = (0..=n).map(|k| at(from + (base - from) * f64::from(k) / f64::from(n))).collect();
        self.poly(arc, BLACK, w);
        let (tip, b) = (at(to), at(base));
        self.head(tip, unit((tip.0 - b.0, tip.1 - b.1)), head, half, BLACK);
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

    /// A couple the way it was taken positive (counter-clockwise when `ccw`), faded and crossed
    /// out.
    pub(super) fn crossed_couple(&mut self, p: P, r: f64, ccw: bool, c: Rgb) {
        self.couple(p, r, ccw, lighter(c, 0.55), 0.3);
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
    fn the_rotation_has_the_same_head_as_the_axes() {
        use crate::model::{Positive, SenseTurn, SenseX, SenseY};
        let area = |t: &[(f64, f64)]| ((t[1].0 - t[0].0) * (t[2].1 - t[0].1) - (t[2].0 - t[0].0) * (t[1].1 - t[0].1)).abs() / 2.0;
        for pos in [Positive::default(), Positive { x: SenseX::Left, y: SenseY::Down, rotation: SenseTurn::Cw }] {
            let mut pen = super::Pen::default();
            pen.positive((0.0, 0.0), pos);
            let heads: Vec<f64> = pen
                .prims
                .iter()
                .filter_map(|p| match p {
                    PlotPrim::Path { pts, fill: Some(_), .. } if pts.len() == 3 => Some(area(pts)),
                    _ => None,
                })
                .collect();
            // The x arrow's, the y arrow's and the rotation's: all three the same size.
            assert_eq!(heads.len(), 3, "{pos:?}");
            assert!(heads.iter().all(|a| (a - heads[0]).abs() < 1e-9), "{heads:?}");
        }
    }

    #[test]
    fn each_diagram_title_has_its_sign_convention() {
        // Each legend has a closed outline with two zigzag cut faces.
        let pieces = |st: &Structure| {
            let r = st.render(None);
            r.plot
                .prims
                .iter()
                .filter(|p| {
                    matches!(p, PlotPrim::Path { pts, stroke: Some((c, w)), dash, .. }
                        if *c == BLACK && *w == 0.5 && dash.is_empty() && pts.len() == 9 && pts.first() == pts.last())
                })
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
