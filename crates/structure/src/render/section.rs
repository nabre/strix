//! One stretch of a written working ([`View::Section`](crate::View)): the stretch marked on the
//! structure by a pale band, the member cut half way along it, and the part before the cut (the
//! member's first end's side) drawn in full — its members, its loads with their names, its
//! reactions as unknowns (`V_A`) — with the internal forces the rest of the structure puts on
//! the cut, `N`, `T`, `M` with the stretch's tag (`N_1a`), pointing the way they count positive
//! (`N` pulling, `T` turning the part clockwise, `M` stretching the reference fibres); the rest of
//! the structure faded beyond a gap, and the section's distance `x` from the member's first node
//! dimensioned. Its laws are the equilibrium of what is drawn in full.

use super::{BLACK, COLOR_M, COLOR_N, COLOR_V, Ctx, Frame, GREY, LOAD, MEMBER_W, P, add, lighter, mul, pd};
use crate::cut::part_before;
use crate::model::{Load, Num, Structure};

/// The band marking the stretch: a pale amber, under the members.
const BAND: [u8; 3] = [0xF7, 0xDB, 0x91];
const BAND_W: f64 = 3.2;
/// The gap left after the cut before the faded rest of the member (paper mm).
const GAP: f64 = 14.0;
/// The internal forces' arrows and the moment's arc (paper mm).
const N_LEN: f64 = 7.5;
const T_LEN: f64 = 6.0;
const M_R: f64 = 4.5;
/// The dimension of `x`: its distance from the member (paper mm).
const DIM_OFF: f64 = 11.5;

impl Ctx<'_> {
    /// The drawing of the stretch [`Structure::cut`] names (the structure alone without one).
    pub(super) fn section(&mut self, f: &Frame) {
        let st = self.st;
        let Some(geo) = Geo::of(st) else {
            self.members(f, BLACK, MEMBER_W, true);
            self.supports(f, BLACK);
            self.loads(f);
            self.node_names(f);
            return;
        };
        let at = |s: f64| f.p(geo.a.0 + geo.e.0 * s, geo.a.1 + geo.e.1 * s);
        let (pe, pn) = (pd(geo.e), pd(geo.n));
        let ps = at(geo.cut);
        let left = part_before(st, &geo.member, &geo.start, &geo.end);
        let in_left = |id: &str| left.as_ref().is_some_and(|l| l.iter().any(|x| x == id));
        let faded = lighter(GREY, 0.45);

        // The stretch, marked under the members, with a tick across each end.
        self.pen.line(at(geo.from), at(geo.to), BAND, BAND_W);
        let tick = lighter([0xC8, 0x8A, 0x10], 0.2);
        for s in [geo.from, geo.to] {
            let p = at(s);
            self.pen.line(add(p, mul(pn, BAND_W * 0.8)), add(p, mul(pn, -BAND_W * 0.8)), tick, 0.3);
        }

        let Some(left_nodes) = &left else {
            // A member in a closed ring: no part lies before the cut alone.
            self.members(f, BLACK, MEMBER_W, true);
            self.supports(f, GREY);
            self.loads(f);
            self.node_names(f);
            return;
        };

        // Everything faded, then the part before the cut over it.
        self.members(f, faded, MEMBER_W, true);
        for m in &st.members {
            let whole = m.id != geo.member && left_nodes.contains(&m.a) && left_nodes.contains(&m.b);
            if whole && let (Some(a), Some(b)) = (self.node_p(f, &m.a), self.node_p(f, &m.b)) {
                self.pen.line(a, b, BLACK, MEMBER_W);
            }
        }
        // The gap after the cut (as far as the member goes), then the member up to the cut.
        let room = ((geo.len - geo.cut) * f.k).max(0.0);
        self.pen.line(add(ps, mul(pe, 0.3)), add(ps, mul(pe, GAP.min(room))), [255, 255, 255], MEMBER_W + 0.6);
        self.pen.line(at(0.0), ps, BLACK, MEMBER_W);
        self.supports(f, lighter(GREY, 0.45));

        // The loads: all faded, then those on the part before the cut, a distributed load the
        // cut crosses up to it (named unless it varies: its far end's name would be wrong).
        self.loads_as(f, lighter(LOAD, 0.65), false, &|_| true);
        let part = geo.loads_before(st, &in_left);
        let mine = Structure { loads: part.iter().map(|(l, _)| l.clone()).collect(), ..st.clone() };
        let picked: Vec<(bool, bool)> = part.iter().map(|(_, k)| (k.0, k.1)).collect();
        let pen = std::mem::take(&mut self.pen);
        let mut cx = Ctx { st: &mine, an: self.an, opts: self.opts, pen, amp_cap: self.amp_cap, placed: Vec::new() };
        let pick = |l: &Load, named: bool| {
            let k = mine.loads.iter().position(|x| std::ptr::eq(x, l));
            k.and_then(|k| picked.get(k)).is_some_and(|&(on, plain)| on && (plain == named))
        };
        cx.loads_as(f, LOAD, true, &|l| pick(l, true));
        cx.loads_as(f, LOAD, false, &|l| pick(l, false));
        self.pen = cx.pen;

        self.unknowns_where(f, &in_left);
        self.internal_forces(ps, pe, pn, &geo.tag);
        self.cut_dimension(at(0.0), ps, pn);
        self.node_names(f);
    }

    /// `N`, `T`, `M` on the cut at `ps` of a member running along paper direction `pe` (`pn` its
    /// normal, the member's local y), each with the stretch's tag.
    fn internal_forces(&mut self, ps: P, pe: P, pn: P, tag: &str) {
        let shear: String = self.st.titles.v.chars().filter(|c| c.is_alphabetic()).take(2).collect();
        let shear = if shear.is_empty() { "T".to_string() } else { shear };
        // N pulling the part (tension positive).
        let tip = add(ps, mul(pe, N_LEN));
        self.pen.arrow(add(ps, mul(pe, 0.4)), tip, COLOR_N, 0.45);
        self.pen.subscripted(add(tip, mul(pe, 0.6)), pe, "N", tag, COLOR_N);
        // T against the member's y on the part before the cut: the pair turns clockwise.
        let tail = add(ps, mul(pn, -0.8));
        let tip = add(ps, mul(pn, -(0.8 + T_LEN)));
        self.pen.arrow(tail, tip, COLOR_V, 0.45);
        // Named on the part's side, clear of the dimension's extension line under the cut.
        self.pen.subscripted(add(add(tip, mul(pn, 1.6)), mul(pe, -0.6)), mul(pe, -1.0), &shear, tag, COLOR_V);
        // M counter-clockwise on the part's end (from the member's −y round past the cut to its
        // +y), stretching the reference fibres; its head at the +y side.
        let (t0, t1) = ((-70.0_f64).to_radians(), 70.0_f64.to_radians());
        let n = 16;
        let pts: Vec<P> = (0..=n)
            .map(|k| {
                let t = t0 + (t1 - t0) * f64::from(k) / f64::from(n);
                add(ps, add(mul(pe, M_R * t.cos()), mul(pn, M_R * t.sin())))
            })
            .collect();
        let (Some(&end), Some(&before)) = (pts.last(), pts.get(pts.len().saturating_sub(4))) else { return };
        self.pen.poly(pts.get(..pts.len().saturating_sub(3)).map(<[P]>::to_vec).unwrap_or_default(), COLOR_M, 0.45);
        self.pen.arrow(before, end, COLOR_M, 0.45);
        let label = add(ps, add(mul(pe, M_R * 0.55), mul(pn, M_R + 1.6)));
        self.pen.subscripted(label, pe, "M", tag, COLOR_M);
    }

    /// The section's distance `x` from the member's first node `pa` to the cut `ps`, on the
    /// member's −y side.
    fn cut_dimension(&mut self, pa: P, ps: P, pn: P) {
        if (ps.0 - pa.0).hypot(ps.1 - pa.1) < 2.0 {
            return;
        }
        let out = mul(pn, -1.0);
        let (a, b) = (add(pa, mul(out, DIM_OFF)), add(ps, mul(out, DIM_OFF)));
        for (from, to) in [(pa, a), (ps, b)] {
            self.pen.line(add(from, mul(out, DIM_OFF - 2.5)), add(to, mul(out, 1.2)), BLACK, 0.18);
        }
        self.pen.line(a, b, BLACK, 0.22);
        let d = super::unit((b.0 - a.0, b.1 - a.1));
        for (p, s) in [(a, 1.0), (b, -1.0)] {
            self.pen.arrow(add(p, mul(d, s * 2.0)), p, BLACK, 0.22);
        }
        let mid = mul(add(a, b), 0.5);
        self.pen.beside(add(mid, mul(out, 0.4)), out, "x".to_string(), BLACK);
    }
}

/// Where the stretch lies: its member's first node `a` (world), its direction `e` and normal `n`,
/// its length; the stretch and the cut along it (clamped to the member).
struct Geo {
    member: String,
    start: String,
    end: String,
    tag: String,
    a: P,
    e: P,
    n: P,
    len: f64,
    from: f64,
    to: f64,
    cut: f64,
}

impl Geo {
    fn of(st: &Structure) -> Option<Geo> {
        let cut = st.cut.as_ref()?;
        let m = st.member(&cut.member)?;
        let (na, nb) = (st.node(&m.a)?, st.node(&m.b)?);
        let len = st.length(m).filter(|l| *l > 0.0 && l.is_finite())?;
        let e = ((nb.x.value - na.x.value) / len, (nb.y.value - na.y.value) / len);
        let (from, to) = (cut.from.clamp(0.0, len), cut.to.clamp(0.0, len));
        Some(Geo {
            member: m.id.clone(),
            start: m.a.clone(),
            end: m.b.clone(),
            tag: cut.tag.clone(),
            a: (na.x.value, na.y.value),
            e,
            n: (-e.1, e.0),
            len,
            from,
            to,
            cut: (from + to) / 2.0,
        })
    }

    /// Every load, each with whether it acts on the part before the cut and whether it is drawn
    /// with its name (`(on, named)`): a distributed load the cut crosses ends at the cut.
    fn loads_before(&self, st: &Structure, in_left: &dyn Fn(&str) -> bool) -> Vec<(Load, (bool, bool))> {
        let eps = 1e-9 * (1.0 + self.len);
        let on_left_member = |id: &str| id != self.member && st.member(id).is_some_and(|m| in_left(&m.a) && in_left(&m.b));
        st.loads
            .iter()
            .map(|l| {
                let mut l = l.clone();
                let on = match &mut l {
                    Load::Force { node: Some(n), member: None, .. } | Load::Moment { node: Some(n), member: None, .. } => in_left(n),
                    Load::Force { member: Some(m), at, .. } | Load::Moment { member: Some(m), at, .. } => {
                        on_left_member(m) || (*m == self.member && at.value.clamp(0.0, self.len) <= self.cut + eps)
                    }
                    Load::Distributed { member, from, to, q1, q2, .. } => {
                        if on_left_member(member) {
                            true
                        } else if *member == self.member {
                            let s0 = from.value.clamp(0.0, self.len);
                            let s1 = to.as_ref().map_or(self.len, |t| t.value).clamp(0.0, self.len);
                            let (lo, hi) = if s0 <= s1 { (s0, s1) } else { (s1, s0) };
                            if lo >= self.cut - eps {
                                false
                            } else if hi <= self.cut + eps {
                                true
                            } else {
                                // Up to the cut, its intensity there as the load varies.
                                let (qa, qb) = (q1.value, q2.as_ref().map_or(q1.value, |q| q.value));
                                let q_at = |s: f64| if (s1 - s0).abs() > eps { qa + (qb - qa) * (s - s0) / (s1 - s0) } else { qa };
                                let (ql, qc) = (q_at(lo), q_at(self.cut));
                                *from = Num::from(lo);
                                *to = Some(Num::from(self.cut));
                                *q1 = Num::from(ql);
                                *q2 = Some(Num::from(qc));
                                let named = (qa - qb).abs() <= 1e-12 * qa.abs().max(qb.abs());
                                return (l, (true, named));
                            }
                        } else {
                            false
                        }
                    }
                    _ => false,
                };
                (l, (on, true))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::cut::Cut;
    use crate::model::{Load, LoadAxes, LoadPer, Member, Node, Num, Structure, Support, SupportKind, View};

    fn beam(cut: Option<Cut>) -> Structure {
        let node = |id: &str, x: f64| Node { id: id.into(), x: x.into(), y: 0.0.into(), hinge: false };
        let support = |n: &str, kind| Support { node: n.into(), kind, angle: Num::default() };
        Structure {
            nodes: vec![node("A", 0.0), node("B", 6.0)],
            members: vec![Member { id: "1".into(), a: "A".into(), b: "B".into(), ..Member::default() }],
            supports: vec![support("A", SupportKind::Pin), support("B", SupportKind::Roller)],
            loads: vec![Load::Distributed {
                member: "1".into(),
                q1: 10.0.into(),
                q2: None,
                from: Num::default(),
                to: None,
                angle: Num::new(-90.0),
                axes: LoadAxes::Global,
                per: LoadPer::Length,
            }],
            view: View::Section,
            cut,
            ..Structure::default()
        }
    }

    /// The texts drawn, a letter and its subscript joined (`V_A`).
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

    /// A stretch's drawing puts N, T, M with its tag on the cut, names the reactions of the part
    /// before it only, its loads, and the section's distance x; saved with its cut.
    #[test]
    fn a_stretch_shows_the_part_before_the_cut_and_the_internal_forces() {
        let st = beam(Some(Cut { member: "1".into(), from: 0.0, to: 6.0, tag: "1a".into() }));
        let t = names(&st);
        for name in ["N_1a", "T_1a", "M_1a", "H_A", "V_A", "x", "q₁ = 10 kN/m"] {
            assert!(t.iter().any(|p| p == name), "{name}: {t:?}");
        }
        assert!(!t.iter().any(|p| p == "V_B"), "{t:?}");
        let back = Structure::from_json(&st.to_json()).unwrap();
        assert_eq!((back.view, back.cut.as_ref().map(|c| c.tag.as_str())), (View::Section, Some("1a")));
    }

    /// The shear's letter follows the diagram's title.
    #[test]
    fn a_stretch_names_the_shear_by_its_title() {
        let mut st = beam(Some(Cut { member: "1".into(), from: 0.0, to: 3.0, tag: "1".into() }));
        st.titles.v = "V".into();
        assert!(names(&st).iter().any(|p| p == "V_1"));
    }

    /// A cut naming nothing, or with junk numbers, draws the structure and never panics.
    #[test]
    fn hostile_cuts_never_panic() {
        for cut in [
            Cut { member: "9".into(), from: 0.0, to: 1.0, tag: "x".into() },
            Cut { member: "1".into(), from: f64::NAN, to: f64::INFINITY, tag: String::new() },
            Cut { member: "1".into(), from: 9.0, to: -4.0, tag: "1".repeat(100) },
            Cut { member: "1".into(), from: 6.0, to: 6.0, tag: "1".into() },
        ] {
            let st = beam(Some(cut)).sanitized();
            assert!(!st.render(None).plot.prims.is_empty());
        }
        assert!(!beam(None).render(None).plot.prims.is_empty());
    }
}
