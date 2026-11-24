//! The lever arms of one moment equation of the reactions' working ([`View::Arms`](crate::View)):
//! the free body of the part balanced about the point ([`Structure::pivot`](crate::Structure)),
//! the point ringed, and for each force of the part that turns about it its arm, as the working
//! writes the equation: a vertical force (or part) by its horizontal distance from the point, in
//! rows below the drawing; a horizontal one by its vertical distance, in columns at its right; an
//! inclined reaction by its distance from the point to its line of action. A load's arm is named
//! as the working names it (`d_(1,A)`, `d_(2,v,A)`), and so is a reaction's the dimensions give
//! only as a sum (`d_(D,A)`); any other reaction's (and an unnamed load's) is written by its
//! length (`6 m`). The shortest arms nearest the drawing. Each arm runs from a small ring
//! at the point to an arrowhead at the force's line.

use super::annotate::{DIM, EXT_OVER, reading_angle};
use super::equilibrium::node_sub;
use super::{BLACK, Ctx, Frame, GREY, LOAD, MEMBER_W, P, REACTION, Rgb, TEXT, add, lighter, mul, unit};
use crate::model::{Load, SupportKind};
use crate::names::load_names;
use crate::pivot::Pivot;
use crate::working::{ResultantKind, resultants};

/// A distance below this share of the structure's size is none (the force passes the point).
const TOL: f64 = 1e-9;
/// From what is drawn to the first arm's line, and between two lines (paper mm).
const CLEAR: f64 = 5.0;
const STEP: f64 = 6.5;
/// Between what is drawn and the start of an extension line.
const EXT_GAP: f64 = 1.0;
/// A subscript's height, as a share of the text's (as [`super::Pen::subscripted`] writes it).
const SUB: f64 = 0.72;
/// At most this many arms are drawn (a hostile structure's loads are capped anyway).
const MAX_ARMS: usize = 48;
/// An arm's ends: a small ring at the point, an arrowhead at the force's line (paper mm).
const RING: f64 = 0.55;
const HEAD: f64 = 1.5;
const HEAD_HALF: f64 = 0.3;
/// The paper under the drawing, inside an arm's ring.
const PAPER: Rgb = [0xFF, 0xFF, 0xFF];

/// One arm: measured along `axis` (0: a horizontal distance, 1: a vertical one) from the point
/// to `to` (the world coordinate of the force's line along that axis), written `base` with the
/// subscript `sub`.
struct Arm {
    axis: usize,
    to: f64,
    base: String,
    sub: String,
    color: Rgb,
}

/// The paper width of `base` with its subscript `sub`.
fn text_width(base: &str, sub: &str) -> f64 {
    let w = |s: &str| s.chars().map(|c| if c.is_uppercase() { 0.72 } else { 0.55 }).sum::<f64>();
    w(base) * TEXT + 0.5 + w(sub) * TEXT * SUB
}

impl Ctx<'_> {
    /// The lever arms about [`Structure::pivot`](crate::Structure) (the free body alone without one).
    pub(super) fn arms(&mut self, f: &Frame) {
        let st = self.st;
        let pivot = st.pivot.clone().unwrap_or_default();
        let Some(node) = st.node(&pivot.node) else {
            self.equilibrium(f);
            return;
        };
        let o = (node.x.value, node.y.value);

        // The part's members in full, the rest faint.
        let part_only = !pivot.members.is_empty();
        self.members(f, if part_only { lighter(GREY, 0.45) } else { BLACK }, MEMBER_W, true);
        if part_only {
            for m in st.members.iter().filter(|m| pivot.has_member(&m.id)) {
                let (Some(a), Some(b)) = (self.node_p(f, &m.a), self.node_p(f, &m.b)) else { continue };
                self.pen.line(a, b, BLACK, MEMBER_W);
            }
        }
        self.supports_where(f, lighter(GREY, 0.45), &|id| pivot.has_node(st, id));
        let (_, mut taken) = self.resultants_named(f, &|l| pivot.has_load(st, l));
        taken.extend(self.unknowns_where(f, &|id| pivot.has_node(st, id)));
        self.node_names_clear_of(f, &|id| pivot.has_node(st, id) || id == pivot.node, &taken);
        // The point the moments are taken about.
        let po = f.p(o.0, o.1);
        self.pen.arc(po, 1.9, 0.0, std::f64::consts::TAU, BLACK, 0.3);

        let (straight, inclined) = self.arms_of(&pivot, o);
        self.straight_arms(f, o, straight);
        for (foot_world, through, text) in inclined {
            self.inclined_arm(po, f.p(foot_world.0, foot_world.1), f.p(through.0, through.1), text);
        }
    }

    /// The arms about `o` of the part `pivot` balances: those along an axis, and the inclined
    /// reactions' (the foot of the perpendicular, the reaction's node, its length as text).
    #[allow(clippy::type_complexity)]
    fn arms_of(&self, pivot: &Pivot, o: (f64, f64)) -> (Vec<Arm>, Vec<((f64, f64), (f64, f64), String)>) {
        let st = self.st;
        let scale = 1.0 + st.nodes.iter().map(|n| n.x.value.abs().max(n.y.value.abs())).fold(0.0_f64, f64::max);
        let tol = TOL * scale;
        let lu = st.length_unit.symbol();
        let length = |cx: &Self, d: f64| format!("{} {lu}", cx.num(d.abs()));
        let pv = node_sub(&pivot.node, 0);
        let mut straight = Vec::new();
        let mut inclined = Vec::new();

        // The loads' resultants, part by part as the equation writes them.
        let names = load_names(st);
        for r in resultants(st) {
            if !st.loads.get(r.load).is_some_and(|l| pivot.has_load(st, l)) {
                continue;
            }
            let value = r.value();
            if value == 0.0 || !value.is_finite() || matches!(r.kind, ResultantKind::Couple { .. }) {
                continue;
            }
            let askew = r.dir.0.abs() > TOL && r.dir.1.abs() > TOL;
            let idx: Option<Vec<String>> = names.get(r.load).copied().flatten().map(|name| {
                let n = name.number.to_string();
                match (r.kind, st.loads.get(r.load)) {
                    (ResultantKind::Spread { triangle, .. }, Some(Load::Distributed { q1, q2, .. })) => {
                        let (a, b) = (q1.value, q2.as_ref().map_or(q1.value, |q| q.value));
                        if a != 0.0 && b != a { vec![n, if triangle { "2" } else { "1" }.to_string()] } else { vec![n] }
                    }
                    _ => vec![n],
                }
            });
            // The vertical part turns by the horizontal distance, the horizontal by the vertical.
            for (axis, part, comp) in [(0, "v", r.dir.1), (1, "h", r.dir.0)] {
                let (to, from) = if axis == 0 { (r.at.0, o.0) } else { (r.at.1, o.1) };
                if comp.abs() <= TOL || (to - from).abs() <= tol {
                    continue;
                }
                let (base, sub) = match &idx {
                    Some(i) => {
                        let mut i = i.clone();
                        if askew {
                            i.push(part.to_string());
                        }
                        i.push(pv.clone());
                        ("d".to_string(), i.join(","))
                    }
                    None => (length(self, to - from), String::new()),
                };
                straight.push(Arm { axis, to, base, sub, color: LOAD });
            }
        }

        // The reactions as unknowns, along the axes they count positive on; an arm the chains
        // give as a sum of distances by its name, as the working works it out (`d_D,A`; a fixed
        // end's or pin's parts `d_D,v,A`), a single distance by its length.
        let chains = crate::chains(st);
        for (k, r) in self.an.reactions.iter().enumerate().filter(|(_, r)| pivot.has_node(st, &r.node)) {
            let Some(n) = st.node(&r.node) else { continue };
            let both = matches!(r.kind, SupportKind::Fixed | SupportKind::Pin);
            let arm = |axis: usize, to: f64, from: f64, part: &str| -> (String, String) {
                if chains.segments(axis, from, to).is_some_and(|s| s.len() > 1) {
                    let mut i = vec![node_sub(&r.node, k + 1)];
                    if both {
                        i.push(part.to_string());
                    }
                    i.push(pv.clone());
                    ("d".to_string(), i.join(","))
                } else {
                    (length(self, to - from), String::new())
                }
            };
            let at = (n.x.value, n.y.value);
            let phi = r.angle.to_radians();
            let normal = (-phi.sin(), phi.cos());
            let along_x = matches!(r.kind, SupportKind::Fixed | SupportKind::Pin) || normal.1.abs() < TOL;
            let along_y = matches!(r.kind, SupportKind::Fixed | SupportKind::Pin) || normal.0.abs() < TOL;
            if along_y && (at.0 - o.0).abs() > tol {
                let (base, sub) = arm(0, at.0, o.0, "v");
                straight.push(Arm { axis: 0, to: at.0, base, sub, color: REACTION });
            }
            if along_x && (at.1 - o.1).abs() > tol {
                let (base, sub) = arm(1, at.1, o.1, "h");
                straight.push(Arm { axis: 1, to: at.1, base, sub, color: REACTION });
            }
            if !along_x && !along_y {
                // Its line through the node along the normal: the foot of the perpendicular.
                let rel = (o.0 - at.0, o.1 - at.1);
                let t = rel.0 * normal.0 + rel.1 * normal.1;
                let foot = (at.0 + normal.0 * t, at.1 + normal.1 * t);
                let d = (o.0 - foot.0).hypot(o.1 - foot.1);
                if d > tol {
                    inclined.push((foot, at, length(self, d)));
                }
            }
        }
        straight.truncate(MAX_ARMS);
        inclined.truncate(MAX_ARMS);
        (straight, inclined)
    }

    /// One arm's line from the point `a` to the force's line at `b`: a ring at `a`, an
    /// arrowhead at `b` (the line alone when it is too short for the head).
    fn arm_line(&mut self, a: P, b: P) {
        let len = (b.0 - a.0).hypot(b.1 - a.1);
        if !len.is_finite() {
            return;
        }
        self.pen.line(a, b, DIM, 0.2);
        if len > HEAD + RING {
            self.pen.head(b, unit((b.0 - a.0, b.1 - a.1)), HEAD, HEAD_HALF, DIM);
        }
        self.pen.dot(a, RING, PAPER, Some((DIM, 0.2)));
    }

    /// The arms along the axes from `o`: horizontal distances in rows below what is drawn,
    /// vertical ones in columns at its right, the shortest nearest; each with its extension lines
    /// from what it measures, a ring at the point, an arrowhead at the force and its name over
    /// its middle.
    fn straight_arms(&mut self, f: &Frame, o: (f64, f64), mut arms: Vec<Arm>) {
        arms.sort_by(|a, b| {
            let (da, db) = ((a.to - if a.axis == 0 { o.0 } else { o.1 }).abs(), (b.to - if b.axis == 0 { o.0 } else { o.1 }).abs());
            da.total_cmp(&db)
        });
        let po = f.p(o.0, o.1);
        let drawn: Vec<[f64; 4]> = self.pen.prims.iter().filter_map(super::prim_box).collect();
        let bottom_at = |px: f64| drawn.iter().filter(|b| b[0] <= px + 0.2 && b[2] >= px - 0.2).map(|b| b[3]).fold(po.1, f64::max);
        let right_at = |py: f64| drawn.iter().filter(|b| b[1] <= py + 0.2 && b[3] >= py - 0.2).map(|b| b[2]).fold(po.0, f64::max);
        let bottom = drawn.iter().map(|b| b[3]).fold(po.1, f64::max);
        let right = drawn.iter().map(|b| b[2]).fold(po.0, f64::max);

        // Horizontal distances, one row each below everything.
        let rows: Vec<&Arm> = arms.iter().filter(|a| a.axis == 0).collect();
        let mut y = bottom + CLEAR;
        for (k, a) in rows.iter().enumerate() {
            if k > 0 {
                y += STEP;
            }
            let pa = po.0;
            let pb = f.p(a.to, 0.0).0;
            let from_b = bottom_at(pb) + EXT_GAP;
            self.pen.line((pb, from_b.min(y - 0.5)), (pb, y + EXT_OVER), GREY, 0.15);
            self.arm_line((pa, y), (pb, y));
            let (lo, hi) = (pa.min(pb), pa.max(pb));
            let w = text_width(&a.base, &a.sub);
            // Over its middle when it fits; beyond its far end otherwise.
            if w + 1.0 < hi - lo {
                self.pen.subscripted(((lo + hi) / 2.0, y - 0.4), (0.0, -1.0), &a.base, &a.sub, a.color);
            } else {
                let dir = if pb >= pa { (1.0, 0.0) } else { (-1.0, 0.0) };
                self.pen.subscripted((pb + dir.0 * 1.5, y - TEXT * 0.4 - 0.2), dir, &a.base, &a.sub, a.color);
            }
        }
        if !rows.is_empty() {
            // The point's extension line, down to the last row.
            let from = bottom_at(po.0) + EXT_GAP;
            self.pen.line((po.0, from.min(y - 0.5)), (po.0, y + EXT_OVER), GREY, 0.15);
        }

        // Vertical distances, one column each at the right, wide enough for its name.
        let cols: Vec<&Arm> = arms.iter().filter(|a| a.axis == 1).collect();
        let mut x = right + CLEAR;
        let mut last_x = x;
        for a in &cols {
            let pa = po.1;
            let pb = f.p(0.0, a.to).1;
            let from_b = right_at(pb) + EXT_GAP;
            self.pen.line((from_b.min(x - 0.5), pb), (x + EXT_OVER, pb), GREY, 0.15);
            self.arm_line((x, pa), (x, pb));
            let mid = (pa + pb) / 2.0;
            self.pen.subscripted((x + 0.3, mid), (1.0, 0.0), &a.base, &a.sub, a.color);
            last_x = x;
            x += text_width(&a.base, &a.sub) + 4.0;
        }
        if !cols.is_empty() {
            let from = right_at(po.1) + EXT_GAP;
            self.pen.line((from.min(last_x - 0.5), po.1), (last_x + EXT_OVER, po.1), GREY, 0.15);
        }
    }

    /// An inclined reaction's arm: its line of action through `through` dashed out to the foot
    /// `foot` of the perpendicular from the point `po`, and the distance between them, written
    /// along it.
    fn inclined_arm(&mut self, po: P, foot: P, through: P, text: String) {
        let d = unit((foot.0 - po.0, foot.1 - po.1));
        let len = (foot.0 - po.0).hypot(foot.1 - po.1);
        if len < 1.0 || !len.is_finite() {
            return;
        }
        self.pen.dashed(through, foot, lighter(REACTION, 0.35), 0.2);
        let (a, b) = (add(po, mul(d, 2.2)), foot);
        self.arm_line(a, b);
        let n = (-d.1, d.0);
        let side = if n.1 <= 0.0 { n } else { mul(n, -1.0) };
        let mid = add(mul(add(a, b), 0.5), mul(side, 1.8));
        self.pen.turned(mid, text, TEXT, reading_angle(d), REACTION);
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{Load, LoadAxes, LoadPer, Member, Node, Num, Structure, Support, SupportKind, View};
    use crate::pivot::Pivot;

    /// The owner's beam: 6 m, 10 kN/m over its first 2.5 m, 10 kN at −45° at 4 m.
    fn beam(pivot: Option<Pivot>) -> Structure {
        let node = |id: &str, x: f64| Node { id: id.into(), x: x.into(), y: 0.0.into(), hinge: false };
        let support = |n: &str, kind| Support { node: n.into(), kind, angle: Num::default() };
        Structure {
            nodes: vec![node("A", 0.0), node("B", 6.0)],
            members: vec![Member { id: "1".into(), a: "A".into(), b: "B".into(), ..Member::default() }],
            supports: vec![support("A", SupportKind::Pin), support("B", SupportKind::Roller)],
            loads: vec![
                Load::Distributed {
                    then: Vec::new(),
                    member: "1".into(),
                    q1: 10.0.into(),
                    q2: None,
                    from: Num::default(),
                    to: Some(2.5.into()),
                    angle: Num::new(-90.0),
                    axes: LoadAxes::Global,
                    per: LoadPer::Length,
                },
                Load::Force { node: None, member: Some("1".into()), at: 4.0.into(), value: 10.0.into(), angle: Num::new(-45.0) },
            ],
            view: View::Arms,
            pivot,
            ..Structure::default()
        }
    }

    /// The texts drawn, a letter and its subscript joined (`d_1,A`).
    fn names(st: &Structure) -> Vec<String> {
        let t: Vec<String> = st
            .render(None)
            .plot
            .prims
            .iter()
            .filter_map(|p| match p {
                strix_plot::PlotPrim::Text { text, .. } | strix_plot::PlotPrim::TextAt { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect();
        let mut out: Vec<String> = t.windows(2).map(|w| format!("{}_{}", w[0], w[1])).collect();
        out.extend(t);
        out
    }

    /// A reaction's arm the chains give as a sum (A–B–C, D beyond: `2 m + 4 m`) is named as the working
    /// works it out (`d_C,A`), not drawn by its length.
    #[test]
    fn a_reactions_arm_from_a_sum_is_named() {
        let mut st = beam(Some(Pivot::whole("A")));
        st.nodes = vec![
            Node { id: "A".into(), x: 0.0.into(), y: 0.0.into(), hinge: false },
            Node { id: "B".into(), x: 2.0.into(), y: 0.0.into(), hinge: false },
            Node { id: "C".into(), x: 6.0.into(), y: 0.0.into(), hinge: false },
            Node { id: "D".into(), x: 8.0.into(), y: 0.0.into(), hinge: false },
        ];
        st.members = vec![
            Member { id: "1".into(), a: "A".into(), b: "B".into(), ..Member::default() },
            Member { id: "2".into(), a: "B".into(), b: "C".into(), ..Member::default() },
            Member { id: "3".into(), a: "C".into(), b: "D".into(), ..Member::default() },
        ];
        st.supports = vec![
            Support { node: "A".into(), kind: SupportKind::Pin, angle: Num::default() },
            Support { node: "C".into(), kind: SupportKind::Roller, angle: Num::default() },
        ];
        st.loads = vec![Load::Force { node: Some("B".into()), member: None, at: Num::default(), value: 10.0.into(), angle: Num::new(-90.0) }];
        let t = names(&st);
        assert!(t.iter().any(|p| p == "d_C,A") && t.iter().any(|p| p == "d_1,A"), "{t:?}");
    }

    /// About A: the resultant's and the vertical part's arms by name, V_B's by its length; none
    /// for the horizontal part (on A's level) or for A's reactions. Saved with its point.
    #[test]
    fn the_arms_about_a_support_are_named_as_the_working_names_them() {
        let st = beam(Some(Pivot::whole("A")));
        let t = names(&st);
        for name in ["d_1,A", "d_2,v,A", "6 m", "F_1", "F_2,v", "F_2,h", "V_B", "H_A", "V_A"] {
            assert!(t.iter().any(|p| p == name), "{name}: {t:?}");
        }
        assert!(!t.iter().any(|p| p == "d_2,h,A" || p == "d_2,A"), "{t:?}");
        let back = Structure::from_json(&st.to_json()).unwrap();
        assert_eq!((back.view, back.pivot), (View::Arms, Some(Pivot::whole("A"))));
    }

    /// Each arm starts with a ring at the point and ends with an arrowhead: about A, three arms
    /// (`d_1,A`, `d_2,v,A`, `6 m`), so three rings on A's extension line.
    #[test]
    fn an_arm_runs_from_a_ring_at_the_point_to_an_arrowhead() {
        let plot = beam(Some(Pivot::whole("A"))).render(None).plot;
        let rings: Vec<(f64, f64)> = plot
            .prims
            .iter()
            .filter_map(|p| match p {
                strix_plot::PlotPrim::Dot { at, r, fill, .. } if (*r - super::RING).abs() < 1e-9 && *fill == super::PAPER => Some(*at),
                _ => None,
            })
            .collect();
        assert_eq!(rings.len(), 3, "{rings:?}");
        assert!(rings.iter().all(|p| (p.0 - rings[0].0).abs() < 1e-9), "{rings:?}");
    }

    /// About B: every arm measured from B (V_A's is the span).
    #[test]
    fn the_arms_about_the_other_support() {
        let t = names(&beam(Some(Pivot::whole("B"))));
        for name in ["d_1,B", "d_2,v,B", "6 m"] {
            assert!(t.iter().any(|p| p == name), "{name}: {t:?}");
        }
    }

    /// A part about a hinge: only its members' loads and its nodes' reactions.
    #[test]
    fn a_part_keeps_its_own_forces() {
        let mut st = beam(None);
        st.nodes.insert(1, Node { id: "C".into(), x: 3.0.into(), y: 0.0.into(), hinge: true });
        st.members = vec![
            Member { id: "1".into(), a: "A".into(), b: "C".into(), ..Member::default() },
            Member { id: "2".into(), a: "C".into(), b: "B".into(), ..Member::default() },
        ];
        if let Some(Load::Force { member, at, .. }) = st.loads.get_mut(1) {
            *member = Some("2".into());
            *at = 1.0.into();
        }
        st.supports[0].kind = SupportKind::Fixed;
        let p = Pivot { node: "C".into(), members: vec!["2".into()] };
        assert!(p.has_node(&st, "B") && p.has_node(&st, "C") && !p.has_node(&st, "A"));
        assert!(!p.has_load(&st, &st.loads[0]) && p.has_load(&st, &st.loads[1]));
        st.pivot = Some(p);
        let t = names(&st);
        assert!(t.iter().any(|p| p == "d_2,v,C") && t.iter().any(|p| p == "V_B"), "{t:?}");
        assert!(!t.iter().any(|p| p == "d_1,C" || p == "V_A"), "{t:?}");
    }

    /// Junk in the point never panics: an unknown node draws the free body alone.
    #[test]
    fn an_unknown_point_draws_the_free_body() {
        let t = names(&beam(Some(Pivot { node: "Z\u{0}".repeat(40), members: vec!["?".into(); 200] })));
        assert!(t.iter().any(|p| p == "V_B") && !t.iter().any(|p| p.starts_with("d_")), "{t:?}");
    }
}
