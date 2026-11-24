//! One stretch of a written working ([`View::Section`](crate::View)): the stretch marked by a pale
//! band of uniform intensity across both sides of the cut, the member cut half way along it with a
//! zigzag cut face, the part before the cut (the member's first end's side) drawn in full — its
//! members, its loads with their names, its reactions by name (`H_A`, `V_A`) pointing the way they
//! count positive, as the laws use them, even one that came out negative — and on
//! the cut the internal force(s) the rest of the structure puts on it (`N`, `T`, `M` together, or the single
//! force [`Cut::action`](crate::Cut::action) asks for), pointing the way they count positive; any
//! loads strictly inside the remainder of the stretch past the cut drawn faded without labels; the
//! rest of the member (and any remaining skeleton) faintly dashed with its end node and support so
//! the whole member is seen without loads past the stretch or reactions past the cut; and the
//! section's distance `x` from the member's first node as a progressive clear of the reactions,
//! and below it the stretch's start and end as progressives: an extension line at each with its
//! distance from the member's first node written beside it (`2.5 m`, `4.5 m`), no dimension line
//! or arrows.

use std::ops::Range;

use super::annotate::{DIM, EXT_OVER, reading_angle};
use super::{BLACK, COLOR_M, COLOR_N, COLOR_V, Ctx, Frame, GREY, LOAD, MEMBER_W, P, add, lighter, max_spread, mul, pd, prim_box};
use crate::cut::part_before;
use crate::diagram::Action;
use crate::model::{Load, Num, Structure};

/// The band marking the stretch: a pale amber, under the members.
const BAND: [u8; 3] = [0xF7, 0xDB, 0x91];
const BAND_W: f64 = 3.2;
/// Minimum separation at the section cut before the faint continuation of the member (paper mm).
const CUT_GAP: f64 = 2.0;
/// The internal forces' arrows and the moment's arc (paper mm).
const N_LEN: f64 = 7.5;
const T_LEN: f64 = 6.0;
const M_R: f64 = 4.5;
/// Least distance of the progressive `x` from the member (paper mm).
const MIN_DIM_OFF: f64 = 10.5;
/// The progressive `x`: its origin's circle and its arrow's head (paper mm).
const X_ORIGIN_R: f64 = 0.8;
const X_HEAD: f64 = 2.8;
/// The progressives of the stretch's ends: between their labels and `x`'s line (paper mm).
const PROG_STEP: f64 = 6.5;

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

        let Some(left_nodes) = &left else {
            // A member in a closed ring: no part lies before the cut alone. The stretch is marked
            // on the structure drawn whole.
            self.band(at(geo.from), at(geo.to), pn);
            self.members(f, BLACK, MEMBER_W, true);
            self.supports(f, GREY);
            self.loads(f);
            self.node_names(f);
            return;
        };

        let rem_mm = ((geo.len - geo.cut) * f.k).max(0.0);
        let resume_mm = Self::resume_gap(geo.action, rem_mm);
        let cut_after = geo.cut + resume_mm / f.k.max(1e-9);

        // 1. The stretch's band: uniform intensity on both sides of the cut gap up to the
        //    stretch's end so the whole stretch [from, to] is seen without crossing N, T or M.
        if geo.cut > geo.from {
            let b0 = at(geo.from);
            self.pen.line(b0, ps, BAND, BAND_W);
            self.band_tick(b0, pn);
        }
        if geo.to > cut_after + 1e-9 {
            let (b1, b2) = (at(cut_after), at(geo.to));
            self.pen.line(b1, b2, BAND, BAND_W);
            self.band_tick(b2, pn);
        }

        // 2. The rest of the member (and any other members past the cut) faintly dashed so the
        //    whole member A–B is seen in context, plus any loads strictly inside the remainder of
        //    the stretch [cut_after, to] drawn faded without labels.
        let ghost_member = lighter(GREY, 0.25);
        let ghost_other = lighter(GREY, 0.52);
        self.ghost_rest(f, &geo, left_nodes, ps, pe, pn, rem_mm, resume_mm, ghost_member, ghost_other);
        let qmax = max_spread(st);
        let after = geo.loads_after(st, cut_after, f.k);
        if !after.is_empty() {
            let faded = Structure { loads: after, ..st.clone() };
            let pen = std::mem::take(&mut self.pen);
            let mut cx = Ctx { st: &faded, an: self.an, opts: self.opts, pen, amp_cap: self.amp_cap, placed: Vec::new() };
            cx.loads_with_max(f, lighter(LOAD, 0.52), false, qmax, &|_, _| true);
            self.pen = cx.pen;
        }

        // 3. The part before the cut in full: the cut member up to `ps` with a zigzag cut face
        //    (and its dashed reference fibres when `M` is shown, matching the M sign convention),
        //    the other members before the cut, its supports faded under the reactions, its loads,
        //    its reactions, and the internal force(s) on the cut.
        self.pen.line(at(0.0), ps, BLACK, MEMBER_W);
        if matches!(geo.action, None | Some(Action::Moment)) && geo.cut * f.k > 4.0 {
            let inset = 1.4_f64.min(geo.cut * f.k * 0.25);
            let f0 = add(add(at(0.0), mul(pe, inset)), mul(pn, -1.15));
            let f1 = add(add(ps, mul(pe, -0.6)), mul(pn, -1.15));
            self.pen.dashed(f0, f1, lighter(BLACK, 0.25), 0.24);
        }
        self.cut_face(ps, pe, pn, lighter(BLACK, 0.25), 0.3);
        if let Some(m) = st.member(&geo.member) {
            let node_hinged = |id: &str| st.node(id).is_some_and(|n| n.hinge);
            if m.hinge_a && !node_hinged(&m.a) {
                self.pen.dot(add(at(0.0), mul(pe, 1.6)), 0.8, [255, 255, 255], Some((BLACK, 0.3)));
            }
        }
        let part = Structure {
            nodes: st.nodes.iter().filter(|n| left_nodes.contains(&n.id)).cloned().collect(),
            members: st.members.iter().filter(|m| m.id != geo.member && left_nodes.contains(&m.a) && left_nodes.contains(&m.b)).cloned().collect(),
            ..st.clone()
        };
        let pen = std::mem::take(&mut self.pen);
        let mut cx = Ctx { st: &part, an: self.an, opts: self.opts, pen, amp_cap: self.amp_cap, placed: Vec::new() };
        cx.members(f, BLACK, MEMBER_W, true);
        self.pen = cx.pen;
        // Its supports faded: their reactions stand for them.
        self.supports_where(f, lighter(GREY, 0.42), &in_left);

        // Its loads, a distributed load the cut crosses up to it (named unless it varies: its far
        // end's name would be wrong).
        let part = geo.loads_before(st, &in_left);
        let mine = Structure { loads: part.iter().map(|(l, _)| l.clone()).collect(), ..st.clone() };
        let picked: Vec<(bool, bool)> = part.iter().map(|(_, k)| (k.0, k.1)).collect();
        let pen = std::mem::take(&mut self.pen);
        let mut cx = Ctx { st: &mine, an: self.an, opts: self.opts, pen, amp_cap: self.amp_cap, placed: Vec::new() };
        // `mine.loads` and `picked` are in the same order: the load's index picks it.
        let pick = |k: usize, named: bool| picked.get(k).is_some_and(|&(on, plain)| on && (plain == named));
        cx.loads_with_max(f, LOAD, true, qmax, &|k, _| pick(k, true));
        cx.loads_with_max(f, LOAD, false, qmax, &|k, _| pick(k, false));
        self.pen = cx.pen;

        self.unknowns_where(f, &in_left);
        self.internal_forces(ps, pe, pn, &geo.tag, geo.action);
        self.node_names_where(f, &in_left);

        // 4. The progressive `x`, measured below everything drawn on the active part before the cut
        //    so it stays clear of `V_A`, supports, upward loads and `T`.
        let x_prims = self.pen.prims.len();
        let x_off = self.cut_dimension(at(0.0), ps, pn);
        let x_prims = x_prims..self.pen.prims.len();

        // 5. Faint supports and node names past the cut (anchoring the far end `B` of the member).
        self.supports_where(f, lighter(GREY, 0.56), &|id| !in_left(id));
        self.node_names_styled(f, lighter(GREY, 0.15), false, &|id| !in_left(id));

        // 6. The stretch's start and end as progressives, below `x`.
        let stops: Vec<(P, f64)> = [geo.from, geo.to].into_iter().map(|s| (at(s), s)).collect();
        self.progressives(&stops, pe, pn, x_off, x_prims);
    }

    /// Paper distance (mm) from the cut `ps` to where the faint continuation of the member (and
    /// any remaining stretch band) resumes, leaving clean white room for the force(s) on the cut.
    fn resume_gap(action: Option<Action>, rem_mm: f64) -> f64 {
        let want = match action {
            Some(Action::Shear) => 2.8,
            Some(Action::Moment) => 6.2,
            Some(Action::Axial) | None => N_LEN + 2.2,
        };
        if rem_mm > want + 2.2 { want } else { CUT_GAP.min(rem_mm * 0.35) }
    }

    /// Faint dashed continuation of the cut member up to its second node, plus any other members
    /// beyond `left_nodes`, so the whole member is seen without competing with the free body.
    #[allow(clippy::too_many_arguments)]
    fn ghost_rest(
        &mut self,
        f: &Frame,
        geo: &Geo,
        left_nodes: &[String],
        ps: P,
        pe: P,
        pn: P,
        rem_mm: f64,
        start_mm: f64,
        ghost_member: [u8; 3],
        ghost_other: [u8; 3],
    ) {
        let st = self.st;
        let at = |s: f64| f.p(geo.a.0 + geo.e.0 * s, geo.a.1 + geo.e.1 * s);
        let node_hinged = |id: &str| st.node(id).is_some_and(|n| n.hinge);

        // Other members not wholly in the part before the cut.
        for m in &st.members {
            if m.id == geo.member || (left_nodes.contains(&m.a) && left_nodes.contains(&m.b)) {
                continue;
            }
            let (Some(a), Some(b)) = (self.node_p(f, &m.a), self.node_p(f, &m.b)) else { continue };
            self.pen.dashed(a, b, ghost_other, 0.32);
            let d = super::unit((b.0 - a.0, b.1 - a.1));
            if m.hinge_a && !node_hinged(&m.a) {
                self.pen.dot(add(a, mul(d, 1.6)), 0.75, [255, 255, 255], Some((ghost_other, 0.25)));
            }
            if m.hinge_b && !node_hinged(&m.b) {
                self.pen.dot(add(b, mul(d, -1.6)), 0.75, [255, 255, 255], Some((ghost_other, 0.25)));
            }
        }

        // The rest of the cut member itself, from `start_mm` past the cut to its far node `B`.
        if rem_mm > 0.4 {
            let pb = at(geo.len);
            let p_start = add(ps, mul(pe, start_mm));
            if rem_mm > start_mm + 0.3 {
                if rem_mm - start_mm > 2.5 {
                    self.cut_face(p_start, pe, pn, ghost_member, 0.24);
                }
                self.pen.dashed(p_start, pb, ghost_member, 0.42);
            }
            if let Some(m) = st.member(&geo.member)
                && m.hinge_b
                && !node_hinged(&m.b)
            {
                self.pen.dot(add(pb, mul(pe, -1.6)), 0.8, [255, 255, 255], Some((ghost_member, 0.28)));
            }
            // Mark the far node `B` subtly when nothing else (support, hinge or member) sits there.
            let bare_end = !node_hinged(&geo.end)
                && !st.supports.iter().any(|s| s.node == geo.end)
                && !st.members.iter().any(|m| m.id != geo.member && (m.a == geo.end || m.b == geo.end));
            if bare_end {
                self.pen.dot(pb, 0.55, [255, 255, 255], Some((ghost_member, 0.28)));
            }
        }

        for n in st.nodes.iter().filter(|n| n.hinge && !left_nodes.contains(&n.id)) {
            self.pen.dot(f.p(n.x.value, n.y.value), 0.85, [255, 255, 255], Some((ghost_member, 0.28)));
        }
    }

    /// The pale band marking a stretch from `a` to `b` (paper), with a tick across each end.
    fn band(&mut self, a: P, b: P, pn: P) {
        self.pen.line(a, b, BAND, BAND_W);
        for p in [a, b] {
            self.band_tick(p, pn);
        }
    }

    fn band_tick(&mut self, p: P, pn: P) {
        let tick = lighter([0xC8, 0x8A, 0x10], 0.2);
        self.pen.line(add(p, mul(pn, BAND_W * 0.8)), add(p, mul(pn, -BAND_W * 0.8)), tick, 0.3);
    }

    /// A short zigzag section-cut line across the member at `p` (`pe` along, `pn` across).
    fn cut_face(&mut self, p: P, pe: P, pn: P, c: [u8; 3], w: f64) {
        let h = 2.0;
        let q = |u: f64, v: f64| add(p, add(mul(pe, u), mul(pn, v)));
        self.pen.poly(vec![q(0.0, -h), q(-0.42, -0.65), q(0.42, 0.65), q(0.0, h)], c, w);
    }

    /// Internal force(s) on the cut at `ps` of a member running along paper direction `pe` (`pn`
    /// its normal, the member's local y), with the stretch's tag: all three when `action` is
    /// `None`, or the single corresponding internal force (`N`, `T`, `M`) alone, oriented by the
    /// positive sign convention (`conventions.rs`: `N` pulling in tension, `T` turning the part
    /// clockwise across the cut face, `M` stretching the `−y` reference fibres).
    fn internal_forces(&mut self, ps: P, pe: P, pn: P, tag: &str, action: Option<Action>) {
        let want_n = matches!(action, None | Some(Action::Axial));
        let want_v = matches!(action, None | Some(Action::Shear));
        let want_m = matches!(action, None | Some(Action::Moment));

        if want_n {
            // N pulling the part (tension positive); named above its arrow when alone, or below its
            // tip when M's arc takes the space above, staying inside the cut gap.
            let tail = add(ps, mul(pe, 0.7));
            let tip = add(ps, mul(pe, 0.7 + N_LEN));
            self.pen.arrow(tail, tip, COLOR_N, 0.45);
            let (label_at, dir) = if want_m {
                (add(ps, add(mul(pe, 0.7 + N_LEN * 0.72), mul(pn, -0.75))), mul(pn, -1.0))
            } else {
                (add(ps, add(mul(pe, 0.7 + N_LEN * 0.52), mul(pn, 0.95))), pn)
            };
            self.pen.subscripted(label_at, dir, "N", tag, COLOR_N);
        }

        if want_v {
            let shear: String = self.st.titles.v.chars().filter(|c| c.is_alphabetic()).take(2).collect();
            let shear = if shear.is_empty() { "T".to_string() } else { shear };
            // T against the member's y on the part before the cut: turns the piece clockwise, just
            // as on the right face of `Pen::convention`. When T stands alone, it is centred across
            // the cut face; with N and M present too, it starts just below `ps` clear of N's shaft.
            let (tail, tip, label) = if action == Some(Action::Shear) {
                let x_off = 1.35;
                let half = 3.5;
                let tail = add(ps, add(mul(pe, x_off), mul(pn, half)));
                let tip = add(ps, add(mul(pe, x_off), mul(pn, -half)));
                // Below the stretch's band (`BAND_W / 2 = 1.6 mm`), beside T's arrowhead.
                let label = add(add(tip, mul(pn, 0.4)), mul(pe, -(x_off + 0.5)));
                (tail, tip, label)
            } else {
                let tail = add(ps, mul(pn, -0.8));
                let tip = add(ps, mul(pn, -(0.8 + T_LEN)));
                let label = add(add(tip, mul(pn, 1.6)), mul(pe, -0.6));
                (tail, tip, label)
            };
            self.pen.arrow(tail, tip, COLOR_V, 0.45);
            self.pen.subscripted(label, mul(pe, -1.0), &shear, tag, COLOR_V);
        }

        if want_m {
            // Half a turn outside the cut face from the `−y` reference fibres round `+pe` to the
            // `+y` fibres, its head at `+y` pointing back (`−pe`) over the piece, just as on the
            // right face of `Pen::convention`: stretches the dashed `−y` reference fibres.
            let (cx_off, r) = if action == Some(Action::Moment) { (1.5, 3.3) } else { (1.2, M_R) };
            let center = add(ps, mul(pe, cx_off));
            let n = 16;
            let pts: Vec<P> = (0..=n)
                .map(|k| {
                    let t = -std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * f64::from(k) / f64::from(n);
                    add(center, add(mul(pe, r * t.cos()), mul(pn, r * t.sin())))
                })
                .collect();
            let (Some(&tip), Some(&before)) = (pts.last(), pts.get(pts.len().saturating_sub(4))) else { return };
            let mut arc = pts;
            arc.truncate(arc.len().saturating_sub(3));
            self.pen.poly(arc, COLOR_M, 0.45);
            self.pen.arrow(before, add(tip, mul(pe, -0.5)), COLOR_M, 0.45);
            let label = if action == Some(Action::Moment) {
                add(ps, add(mul(pe, 3.1), mul(pn, r + 0.55)))
            } else {
                add(ps, add(mul(pe, 3.5), mul(pn, r + 0.55)))
            };
            self.pen.subscripted(label, pn, "M", tag, COLOR_M);
        }
    }

    /// The section's distance `x` from the member's first node `pa` to the cut `ps`, on the
    /// member's −y side, as a progressive (a small circle at its origin, an arrow at the cut),
    /// placed clear of supports, reactions (`V_A`), loads and `T`. Its distance from the member
    /// (paper mm), when drawn.
    fn cut_dimension(&mut self, pa: P, ps: P, pn: P) -> Option<f64> {
        let cut_len = (ps.0 - pa.0).hypot(ps.1 - pa.1);
        if cut_len < 2.0 {
            return None;
        }
        let d = super::unit((ps.0 - pa.0, ps.1 - pa.1));
        let out = mul(pn, -1.0);
        // Project already-drawn boxes onto `(d, out)` relative to `pa` so `x` clears everything
        // between `pa` and `ps`, and each extension line starts below what lies across it.
        let mut span_w = 0.0_f64;
        let mut w_pa = 1.6_f64;
        let mut w_ps = 1.6_f64;
        for b in self.pen.prims.iter().filter_map(prim_box) {
            let corners = [(b[0], b[1]), (b[2], b[1]), (b[2], b[3]), (b[0], b[3])];
            let (mut s0, mut s1, mut w1) = (f64::MAX, f64::MIN, f64::MIN);
            for c in corners {
                let rel = (c.0 - pa.0, c.1 - pa.1);
                let s = rel.0 * d.0 + rel.1 * d.1;
                let w = rel.0 * out.0 + rel.1 * out.1;
                s0 = s0.min(s);
                s1 = s1.max(s);
                w1 = w1.max(w);
            }
            if w1 <= 0.0 || w1 > 40.0 {
                continue;
            }
            if s1 >= -3.5 && s0 <= cut_len + 3.5 {
                span_w = span_w.max(w1);
            }
            if s0 <= 1.8 && s1 >= -1.8 {
                w_pa = w_pa.max(w1);
            }
            if s0 <= cut_len + 1.8 && s1 >= cut_len - 1.8 {
                w_ps = w_ps.max(w1);
            }
        }
        let off = (span_w + 3.5).max(MIN_DIM_OFF);
        let (a, b) = (add(pa, mul(out, off)), add(ps, mul(out, off)));
        for (origin, w_obs, to) in [(pa, w_pa, a), (ps, w_ps, b)] {
            let start = (w_obs + 1.0).clamp(2.2, (off - 1.5).max(2.2));
            self.pen.line(add(origin, mul(out, start)), add(to, mul(out, EXT_OVER)), BLACK, 0.18);
        }
        // The line from the circle's rim to the arrow's head, the head larger than a force's.
        let base = add(b, mul(d, -X_HEAD));
        self.pen.line(add(a, mul(d, X_ORIGIN_R)), base, BLACK, 0.22);
        let n = (-d.1, d.0);
        self.pen.fill(vec![b, add(base, mul(n, X_HEAD * 0.34)), add(base, mul(n, -X_HEAD * 0.34))], BLACK, None);
        self.pen.dot(a, X_ORIGIN_R, [255, 255, 255], Some((BLACK, 0.25)));
        let mid = mul(add(a, b), 0.5);
        self.pen.beside(add(mid, mul(out, 0.4)), out, "x".to_string(), BLACK);
        Some(off)
    }

    /// Progressives along the member (`pe` its direction on paper) on its −y side (`pn` its +y):
    /// at each of `stops` (paper point, distance from the member's first node) an extension line
    /// with the distance written beside its end (`0` at the first node) — no dimension line, no
    /// arrows. The labels sit below the `x` dimension (`x_off` from the member, its primitives
    /// `x_prims`) and clear of everything else drawn.
    fn progressives(&mut self, stops: &[(P, f64)], pe: P, pn: P, x_off: Option<f64>, x_prims: Range<usize>) {
        let mut stops: Vec<(P, f64)> = stops.iter().copied().filter(|(p, s)| s.is_finite() && p.0.is_finite() && p.1.is_finite()).collect();
        stops.sort_by(|a, b| a.1.total_cmp(&b.1));
        let tol = 1e-6 * (1.0 + stops.iter().fold(0.0_f64, |m, s| m.max(s.1.abs())));
        stops.dedup_by(|b, a| (b.1 - a.1).abs() <= tol);
        let (Some(&(pa, _)), Some(&(pz, _))) = (stops.first(), stops.last()) else { return };
        let full = (pz.0 - pa.0).hypot(pz.1 - pa.1);
        let d = super::unit(pe);
        if !(d.0.is_finite() && d.1.is_finite()) {
            return;
        }
        let out = mul(pn, -1.0);
        let along = |p: P| (p.0 - pa.0) * d.0 + (p.1 - pa.1) * d.1;
        let ss: Vec<f64> = stops.iter().map(|(p, _)| along(*p)).collect();

        // What is drawn, projected onto `(d, out)` from `pa` (the `x` dimension aside: the
        // extension lines cross it): the line clears all of it, each extension line starts below
        // what lies across it.
        let mut span_w = 0.0_f64;
        let mut w_at = vec![1.6_f64; ss.len()];
        for b in self.pen.prims.iter().enumerate().filter(|(i, _)| !x_prims.contains(i)).filter_map(|(_, p)| prim_box(p)) {
            let corners = [(b[0], b[1]), (b[2], b[1]), (b[2], b[3]), (b[0], b[3])];
            let (mut s0, mut s1, mut w1) = (f64::MAX, f64::MIN, f64::MIN);
            for c in corners {
                let rel = (c.0 - pa.0, c.1 - pa.1);
                s0 = s0.min(rel.0 * d.0 + rel.1 * d.1);
                s1 = s1.max(rel.0 * d.0 + rel.1 * d.1);
                w1 = w1.max(rel.0 * out.0 + rel.1 * out.1);
            }
            if w1 <= 0.0 || w1 > 40.0 {
                continue;
            }
            if s1 >= -3.5 && s0 <= full + 3.5 {
                span_w = span_w.max(w1);
            }
            for (w, &s) in w_at.iter_mut().zip(&ss) {
                if s0 <= s + 1.8 && s1 >= s - 1.8 {
                    *w = w.max(w1);
                }
            }
        }
        let off = x_off.map_or(MIN_DIM_OFF, |x| x + PROG_STEP).max(span_w + 3.5);
        let on = |s: f64, w: f64| add(add(pa, mul(d, s)), mul(out, w));

        for (&s, &w_obs) in ss.iter().zip(&w_at) {
            let start = (w_obs + 1.0).clamp(2.2, (off - 1.5).max(2.2));
            self.pen.line(on(s, start), on(s, off + EXT_OVER), DIM, 0.15);
        }
        let angle = reading_angle(d);
        for (&s, &(_, value)) in ss.iter().zip(&stops) {
            let text = if value.abs() <= tol { "0".to_string() } else { format!("{} {}", self.num(value), self.st.length_unit.symbol()) };
            let width = text.chars().count() as f64 * super::TEXT * 0.55;
            self.pen.turned(on(s + width / 2.0 + 1.0, off + 2.0), text, super::TEXT, angle, DIM);
        }
    }
}

/// Where the stretch lies: its member's first node `a` (world), its direction `e` and normal `n`,
/// its length; the stretch, the cut along it (clamped to the member) and which force is drawn.
struct Geo {
    member: String,
    start: String,
    end: String,
    tag: String,
    action: Option<Action>,
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
            action: cut.action,
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

    /// Loads (or parts of distributed loads) acting strictly inside the remainder of the stretch
    /// after the cut gap (`(cut_after, to)`), drawn faded without labels so every load in the
    /// stretch is seen while only the part before the cut enters equilibrium.
    fn loads_after(&self, st: &Structure, cut_after: f64, k: f64) -> Vec<Load> {
        let eps = 1e-9 * (1.0 + self.len);
        if self.to <= cut_after + eps {
            return Vec::new();
        }
        st.loads
            .iter()
            .filter_map(|l| {
                let mut l = l.clone();
                match &mut l {
                    Load::Force { member: Some(m), at, .. } | Load::Moment { member: Some(m), at, .. } => {
                        let s = at.value.clamp(0.0, self.len);
                        (*m == self.member && s > self.cut + eps && s >= cut_after && s < self.to - eps).then_some(l)
                    }
                    Load::Distributed { member, from, to, q1, q2, .. } if *member == self.member => {
                        let s0 = from.value.clamp(0.0, self.len);
                        let s1 = to.as_ref().map_or(self.len, |t| t.value).clamp(0.0, self.len);
                        let (lo, hi) = if s0 <= s1 { (s0, s1) } else { (s1, s0) };
                        let (b0, b1) = (lo.max(cut_after), hi.min(self.to));
                        if b1 <= b0 + eps || (b1 - b0) * k < 1.8 {
                            return None;
                        }
                        let (qa, qb) = (q1.value, q2.as_ref().map_or(q1.value, |q| q.value));
                        let q_at = |s: f64| if (s1 - s0).abs() > eps { qa + (qb - qa) * (s - s0) / (s1 - s0) } else { qa };
                        let (qa0, qa1) = (q_at(b0), q_at(b1));
                        *from = Num::from(b0);
                        *to = Some(Num::from(b1));
                        *q1 = Num::from(qa0);
                        *q2 = Some(Num::from(qa1));
                        Some(l)
                    }
                    _ => None,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::cut::Cut;
    use crate::diagram::Action;
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

    /// A stretch's drawing puts N, T, M with its tag on the cut when no single action is asked,
    /// names the reactions of the part before it only, its loads, and the section's distance x;
    /// saved with its cut.
    #[test]
    fn a_stretch_shows_the_part_before_the_cut_and_the_internal_forces() {
        let st = beam(Some(Cut { member: "1".into(), from: 0.0, to: 6.0, tag: "1a".into(), action: None }));
        let t = names(&st);
        for name in ["N_1a", "T_1a", "M_1a", "H_A", "V_A", "x", "q₁ = 10 kN/m", "A", "B"] {
            assert!(t.iter().any(|p| p == name), "{name}: {t:?}");
        }
        assert!(!t.iter().any(|p| p == "V_B"), "{t:?}");
        let back = Structure::from_json(&st.to_json()).unwrap();
        assert_eq!(
            (back.view, back.cut.as_ref().map(|c| c.tag.as_str()), back.cut.as_ref().and_then(|c| c.action)),
            (View::Section, Some("1a"), None)
        );
    }

    /// Each diagram's cut puts only its own internal force on the section and not the others, and
    /// round-trips its `action` through JSON.
    #[test]
    fn a_stretch_for_one_diagram_draws_only_its_internal_force() {
        for (action, shown, hidden) in
            [(Action::Axial, "N_1b", ["T_1b", "M_1b"]), (Action::Shear, "T_1b", ["N_1b", "M_1b"]), (Action::Moment, "M_1b", ["N_1b", "T_1b"])]
        {
            let st = beam(Some(Cut { member: "1".into(), from: 2.5, to: 6.0, tag: "1b".into(), action: Some(action) }));
            let t = names(&st);
            assert!(t.iter().any(|p| p == shown), "{action:?} missing {shown}: {t:?}");
            for other in hidden {
                assert!(!t.iter().any(|p| p == other), "{action:?} drew {other}: {t:?}");
            }
            let back = Structure::from_json(&st.to_json()).unwrap();
            assert_eq!(back.cut.and_then(|c| c.action), Some(action));
        }
    }

    /// Each internal force on the cut points in the direction of its positive sign convention:
    /// `N` outward (tension), `T` clockwise on the part before the cut, `M` stretching the dashed
    /// `−y` reference fibres (from `−y` round the cut to `+y`, head pointing back over the part).
    #[test]
    fn internal_forces_follow_the_positive_sign_convention() {
        let head_dir = |st: &Structure, color: [u8; 3]| -> (f64, f64) {
            let prims = st.render(None).plot.prims;
            // Filled triangle arrowhead of `color`: `pts[0]` is tip, `pts[1]` and `pts[2]` base.
            let tri = prims
                .iter()
                .find_map(|p| match p {
                    strix_plot::PlotPrim::Path { pts, closed: true, fill: Some(c), .. } if *c == color && pts.len() == 3 => Some(pts.clone()),
                    _ => None,
                })
                .unwrap();
            let mid = ((tri[1].0 + tri[2].0) / 2.0, (tri[1].1 + tri[2].1) / 2.0);
            crate::render::unit((tri[0].0 - mid.0, tri[0].1 - mid.1))
        };
        // Left-to-right beam (part before cut is on the left, cut on its right face):
        // N points right (+1, 0), T points down (0, +1) on paper, M's head at top points left.
        let n_dir =
            head_dir(&beam(Some(Cut { member: "1".into(), from: 0.0, to: 4.0, tag: "1".into(), action: Some(Action::Axial) })), super::COLOR_N);
        assert!(n_dir.0 > 0.99 && n_dir.1.abs() < 0.05, "{n_dir:?}");
        let v_dir =
            head_dir(&beam(Some(Cut { member: "1".into(), from: 0.0, to: 4.0, tag: "1".into(), action: Some(Action::Shear) })), super::COLOR_V);
        assert!(v_dir.0.abs() < 0.05 && v_dir.1 > 0.99, "{v_dir:?}");
        let m_st = beam(Some(Cut { member: "1".into(), from: 0.0, to: 4.0, tag: "1".into(), action: Some(Action::Moment) }));
        let m_dir = head_dir(&m_st, super::COLOR_M);
        assert!(m_dir.0 < -0.7, "M head points back over the piece: {m_dir:?}");
        // Reference fibres are drawn dashed on the lower (`−y`) side of the active member.
        let has_ref_fibres = m_st
            .render(None)
            .plot
            .prims
            .iter()
            .any(|p| matches!(p, strix_plot::PlotPrim::Path { stroke: Some((_, w)), dash, .. } if !dash.is_empty() && (*w - 0.24).abs() < 1e-6));
        assert!(has_ref_fibres);

        // A member drawn right to left runs left to right as the structure's axes do: the part
        // before the cut is still on the left, T still points down.
        let mut rev = beam(Some(Cut { member: "1".into(), from: 0.0, to: 4.0, tag: "1".into(), action: Some(Action::Shear) }));
        rev.members[0] = Member { id: "1".into(), a: "B".into(), b: "A".into(), ..Member::default() };
        let v_rev = head_dir(&rev, super::COLOR_V);
        assert!(v_rev.0.abs() < 0.05 && v_rev.1 > 0.99, "{v_rev:?}");
    }

    /// Beyond the cut, the rest of the member is faintly dashed up to `B`, the stretch's yellow
    /// band keeps one intensity on both sides of the cut, loads inside the remainder of the
    /// stretch are drawn faded up to `to`, while concentrated loads on `to` or past the stretch
    /// and reactions past the cut are left out; `x` clears `V_A`.
    #[test]
    fn a_stretch_faintly_shows_the_whole_member_and_all_loads_in_the_stretch() {
        let faded_load = super::lighter(super::LOAD, 0.52);
        let mut st = beam(Some(Cut { member: "1".into(), from: 0.0, to: 2.5, tag: "1a".into(), action: Some(Action::Shear) }));
        // Concentrated load right on the end of the stretch (`2.5`) and one past it (`5.0`): neither drawn.
        st.loads.push(Load::Force { node: None, member: Some("1".into()), at: 2.5.into(), value: 12.0.into(), angle: Num::new(-90.0) });
        st.loads.push(Load::Force { node: None, member: Some("1".into()), at: 5.0.into(), value: 10.0.into(), angle: Num::new(-90.0) });
        let t = names(&st);
        assert!(t.iter().any(|p| p == "A") && t.iter().any(|p| p == "B"), "{t:?}");
        assert!(!t.iter().any(|p| p == "V_B" || p.starts_with("F")), "{t:?}");
        // `q₁ = 10 kN/m` is written once, over the active part before the cut.
        assert_eq!(t.iter().filter(|p| p.as_str() == "q₁ = 10 kN/m").count(), 1, "{t:?}");
        let prims = st.render(None).plot.prims;

        // Both halves of the stretch's yellow band have the exact same full `BAND` colour.
        let band_segs: Vec<(f64, f64)> = prims
            .iter()
            .filter_map(|p| match p {
                strix_plot::PlotPrim::Path { pts, stroke: Some((c, w)), .. } if *c == super::BAND && (*w - super::BAND_W).abs() < 1e-6 => {
                    Some((pts.iter().map(|q| q.0).fold(f64::MAX, f64::min), pts.iter().map(|q| q.0).fold(f64::MIN, f64::max)))
                }
                _ => None,
            })
            .collect();
        assert_eq!(band_segs.len(), 2, "expected two equal-intensity band segments around the cut: {band_segs:?}");
        let band_end = band_segs[1].1;

        // Solid black member stops at the cut; a dashed ghosted path continues to B.
        let solid_end = prims
            .iter()
            .filter_map(|p| match p {
                strix_plot::PlotPrim::Path { pts, stroke: Some((c, w)), dash, .. }
                    if *c == super::BLACK && *w == super::MEMBER_W && dash.is_empty() =>
                {
                    Some(pts.iter().map(|q| q.0).fold(f64::MIN, f64::max))
                }
                _ => None,
            })
            .fold(f64::MIN, f64::max);
        let dashed_end = prims
            .iter()
            .filter_map(|p| match p {
                strix_plot::PlotPrim::Path { pts, dash, .. } if !dash.is_empty() => Some(pts.iter().map(|q| q.0).fold(f64::MIN, f64::max)),
                _ => None,
            })
            .fold(f64::MIN, f64::max);
        assert!(dashed_end > solid_end + 20.0, "solid={solid_end} dashed={dashed_end}");

        // Faded distributed-load arrows cover the second half of the stretch up to `band_end` (`to`),
        // and stop there (even though `q₁` spans the whole 6 m beam), with no concentrated-force arrow
        // at `to` (`ARROW = 11.0` taller than `SPREAD = 8.0`).
        let faded_xs: Vec<f64> = prims
            .iter()
            .filter_map(|p| match p {
                strix_plot::PlotPrim::Path { pts, stroke: Some((c, _)), .. } if *c == faded_load => {
                    Some(pts.iter().map(|q| q.0).fold(f64::MIN, f64::max))
                }
                _ => None,
            })
            .collect();
        assert!(!faded_xs.is_empty(), "expected faded load arrows in the second half of the stretch");
        let faded_max_x = faded_xs.iter().copied().fold(f64::MIN, f64::max);
        assert!((faded_max_x - band_end).abs() < 0.6, "faded_max_x={faded_max_x} band_end={band_end}");

        // `x`'s origin circle sits below `V_A`'s arrow, never on its shaft.
        let origin_y = prims
            .iter()
            .find_map(|p| match p {
                strix_plot::PlotPrim::Dot { at, r, .. } if (*r - super::X_ORIGIN_R).abs() < 1e-6 => Some(at.1),
                _ => None,
            })
            .unwrap();
        let va_bottom = prims
            .iter()
            .filter_map(|p| match p {
                strix_plot::PlotPrim::Text { at, text, .. } if text == "V" => Some(at.1),
                _ => None,
            })
            .fold(f64::MIN, f64::max);
        assert!(origin_y > va_bottom + 1.5, "origin_y={origin_y} va_bottom={va_bottom}");

        // In a joined N stretch `[0, 6]`, a transverse force strictly inside `(cut, 6)` (at 4.5 m)
        // is drawn faded without a label, while one at the end `6.0` is not.
        let mut axial = beam(Some(Cut { member: "1".into(), from: 0.0, to: 6.0, tag: "1".into(), action: Some(Action::Axial) }));
        axial.loads = vec![
            Load::Force { node: None, member: Some("1".into()), at: 4.5.into(), value: 12.0.into(), angle: Num::new(-90.0) },
            Load::Force { node: None, member: Some("1".into()), at: 6.0.into(), value: 12.0.into(), angle: Num::new(-90.0) },
        ];
        let axial_prims = axial.render(None).plot.prims;
        let faded_heads = axial_prims
            .iter()
            .filter(|p| matches!(p, strix_plot::PlotPrim::Path { closed: true, fill: Some(c), pts, .. } if *c == faded_load && pts.len() == 3))
            .count();
        assert_eq!(faded_heads, 1, "expected only the interior force at 4.5 m to be drawn faded");
        assert!(!names(&axial).iter().any(|p| p.starts_with("F")));
    }

    /// Below `x`, only the stretch's start and end as progressives (their distances from the
    /// member's first node): no `0` origin, no member's far end, no dimension line with arrows.
    #[test]
    fn a_stretch_shows_its_progressives_below_x() {
        let mid = beam(Some(Cut { member: "1".into(), from: 2.5, to: 4.5, tag: "1b".into(), action: Some(Action::Moment) }));
        let t = names(&mid);
        for v in ["2.5 m", "4.5 m"] {
            assert_eq!(t.iter().filter(|p| p.as_str() == v).count(), 1, "{v}: {t:?}");
        }
        assert!(!t.iter().any(|p| p == "0" || p == "6 m"), "{t:?}");
        // No filled triangle in the dimensions' grey (`x`'s head is black).
        let dim_heads = mid
            .render(None)
            .plot
            .prims
            .iter()
            .filter(|p| matches!(p, strix_plot::PlotPrim::Path { closed: true, fill: Some(c), pts, .. } if *c == super::DIM && pts.len() == 3))
            .count();
        assert_eq!(dim_heads, 0, "no arrowheads on the progressives");
        let st = beam(Some(Cut { member: "1".into(), from: 2.5, to: 6.0, tag: "1b".into(), action: Some(Action::Moment) }));
        let prims = st.render(None).plot.prims;
        let text_y = |want: &str| {
            prims.iter().find_map(|p| match p {
                strix_plot::PlotPrim::Text { at, text, .. } | strix_plot::PlotPrim::TextAt { at, text, .. } if text == want => Some(at.1),
                _ => None,
            })
        };
        let (x, a) = (text_y("x").unwrap(), text_y("2.5 m").unwrap());
        assert!(a > x + 2.0, "progressives below x: x={x} 2.5 m={a}");
        // A stretch over the whole member has two stops, `0` and its length.
        let whole = names(&beam(Some(Cut { member: "1".into(), from: 0.0, to: 6.0, tag: "1".into(), action: None })));
        assert!(whole.iter().any(|p| p == "0") && whole.iter().filter(|p| p.as_str() == "6 m").count() == 1, "{whole:?}");
    }

    /// The shear's letter follows the diagram's title.
    #[test]
    fn a_stretch_names_the_shear_by_its_title() {
        let mut st = beam(Some(Cut { member: "1".into(), from: 0.0, to: 3.0, tag: "1".into(), action: Some(Action::Shear) }));
        st.titles.v = "V".into();
        let t = names(&st);
        assert!(t.iter().any(|p| p == "V_1") && !t.iter().any(|p| p == "N_1" || p == "M_1"), "{t:?}");
    }

    /// The tag keeps the member's id as the working writes it (`1-2a`), not only its letters.
    #[test]
    fn a_stretch_tag_keeps_the_members_id() {
        let mut st = beam(None);
        st.members[0].id = "1-2".into();
        if let Some(Load::Distributed { member, .. }) = st.loads.first_mut() {
            *member = "1-2".into();
        }
        st.cut = Some(Cut { member: "1-2".into(), from: 0.0, to: 6.0, tag: "1-2a".into(), action: None });
        let st = st.sanitized();
        assert_eq!(st.cut.as_ref().map(|c| c.tag.as_str()), Some("1-2a"));
        let t = names(&st);
        assert!(t.iter().any(|p| p == "N_1-2a"), "{t:?}");
    }

    /// A member drawn right to left runs left to right as the structure's axes do: the part before
    /// the cut is the left one, as for the member drawn the other way.
    #[test]
    fn a_reversed_member_is_cut_from_its_left_end() {
        let forward = names(&beam(Some(Cut { member: "1".into(), from: 0.0, to: 2.0, tag: "1".into(), action: None })));
        let mut st = beam(Some(Cut { member: "1".into(), from: 0.0, to: 2.0, tag: "1".into(), action: None }));
        st.members[0] = Member { id: "1".into(), a: "B".into(), b: "A".into(), ..Member::default() };
        let t = names(&st);
        assert!(t.iter().any(|p| p == "V_A") && t.iter().any(|p| p == "M_1"), "{t:?}");
        assert!(!t.iter().any(|p| p == "V_B"), "{t:?}");
        assert_eq!(t, forward);
    }

    /// A column fixed at its foot: its reactions, the couple among them, and the forces on the cut;
    /// a negative reaction drawn the way it acts.
    #[test]
    fn a_vertical_member_is_cut() {
        let node = |id: &str, y: f64| Node { id: id.into(), x: 0.0.into(), y: y.into(), hinge: false };
        let st = Structure {
            nodes: vec![node("A", 0.0), node("B", 4.0)],
            members: vec![Member { id: "1".into(), a: "A".into(), b: "B".into(), ..Member::default() }],
            supports: vec![Support { node: "A".into(), kind: SupportKind::Fixed, angle: Num::default() }],
            loads: vec![Load::Force { node: Some("B".into()), member: None, at: Num::default(), value: 10.0.into(), angle: Num::new(0.0) }],
            view: View::Section,
            cut: Some(Cut { member: "1".into(), from: 0.0, to: 4.0, tag: "1".into(), action: None }),
            ..Structure::default()
        };
        let t = names(&st);
        // H_A, taken positive along +x, came out −10 kN: still named, as the laws use it
        // (`N_1(x) = −H_A`), never by its size beside its name's arrow.
        for name in ["H_A", "V_A", "M_A", "N_1", "T_1", "M_1", "x"] {
            assert!(t.iter().any(|p| p == name), "{name}: {t:?}");
        }
        assert!(!t.iter().any(|p| p.ends_with("kN")), "{t:?}");
    }

    /// A varying distributed load the cut crosses is drawn up to the cut without its name (its far
    /// end's would be wrong there).
    #[test]
    fn a_varying_load_across_the_cut_goes_unnamed() {
        let mut st = beam(Some(Cut { member: "1".into(), from: 0.0, to: 6.0, tag: "1".into(), action: None }));
        if let Some(Load::Distributed { q2, .. }) = st.loads.first_mut() {
            *q2 = Some(20.0.into());
        }
        let t = names(&st);
        assert!(t.iter().any(|p| p == "N_1"), "{t:?}");
        assert!(!t.iter().any(|p| p.starts_with("q₁ =") || p.starts_with("q₂ =")), "{t:?}");
    }

    /// A member closing a ring has no part before its cut alone: the stretch is marked on the
    /// structure drawn whole, without the internal forces.
    #[test]
    fn a_member_in_a_ring_draws_no_internal_forces() {
        let node = |id: &str, x: f64, y: f64| Node { id: id.into(), x: x.into(), y: y.into(), hinge: false };
        let member = |id: &str, a: &str, b: &str| Member { id: id.into(), a: a.into(), b: b.into(), ..Member::default() };
        let mut st = beam(Some(Cut { member: "1".into(), from: 0.0, to: 6.0, tag: "1".into(), action: None }));
        st.nodes.push(node("C", 3.0, 3.0));
        st.members.extend([member("2", "B", "C"), member("3", "C", "A")]);
        let t = names(&st);
        assert!(!t.iter().any(|p| p == "N_1" || p == "M_1"), "{t:?}");
    }

    /// A cut naming nothing, or with junk numbers, draws the structure and never panics.
    #[test]
    fn hostile_cuts_never_panic() {
        for cut in [
            Cut { member: "9".into(), from: 0.0, to: 1.0, tag: "x".into(), action: None },
            Cut { member: "1".into(), from: f64::NAN, to: f64::INFINITY, tag: String::new(), action: Some(Action::Axial) },
            Cut { member: "1".into(), from: 9.0, to: -4.0, tag: "1".repeat(100), action: Some(Action::Shear) },
            Cut { member: "1".into(), from: 6.0, to: 6.0, tag: "1".into(), action: Some(Action::Moment) },
        ] {
            let st = beam(Some(cut)).sanitized();
            assert!(!st.render(None).plot.prims.is_empty());
        }
        assert!(!beam(None).render(None).plot.prims.is_empty());
    }
}
