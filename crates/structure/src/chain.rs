//! Distributed loads over several members: a load on `member` that goes on (`then`) over the
//! members aligned with it, joined end to end, lies along one straight line. Its `from` and `to`
//! are measured along that line from its start, its local axes are the line's. The drawing, the
//! names and the resultants take it whole ([`Structure::spread_line`]); the solver and what works
//! member by member take it in one piece per member ([`Structure::spread_pieces`]).

use crate::model::{Load, LoadAxes, Num, Structure};

/// How far from parallel (the sine of the angle) two members still count as aligned.
const ALIGNED: f64 = 1e-6;
/// The most members one load goes over.
pub(crate) const MAX_CHAIN: usize = 64;

/// One member of a load's line.
#[derive(Debug, Clone, PartialEq)]
pub struct LinePart {
    pub member: String,
    /// Where along the line the member starts.
    pub at: f64,
    pub len: f64,
    /// The member runs against the line (its `a` end is the line's far one).
    pub reversed: bool,
}

/// The straight line a distributed load lies along: from `start`, in direction `dir` (a unit
/// vector), `len` long, made of `parts`.
#[derive(Debug, Clone, PartialEq)]
pub struct SpreadLine {
    pub start: (f64, f64),
    pub dir: (f64, f64),
    pub len: f64,
    pub parts: Vec<LinePart>,
}

impl SpreadLine {
    /// The world point `s` along the line (within it).
    pub fn point(&self, s: f64) -> (f64, f64) {
        let s = s.clamp(0.0, self.len);
        (self.start.0 + self.dir.0 * s, self.start.1 + self.dir.1 * s)
    }

    /// The member `s` along the line falls on, and where along it from its `a` end.
    pub fn locate(&self, s: f64) -> Option<(&str, f64)> {
        let s = s.clamp(0.0, self.len);
        let p = self.parts.iter().find(|p| s <= p.at + p.len + 1e-9 * (1.0 + self.len)).or(self.parts.last())?;
        let u = (s - p.at).clamp(0.0, p.len);
        Some((p.member.as_str(), if p.reversed { p.len - u } else { u }))
    }

    /// Where along the line the world point `w` falls (within it).
    pub fn along(&self, w: (f64, f64)) -> f64 {
        let s = (w.0 - self.start.0) * self.dir.0 + (w.1 - self.start.1) * self.dir.1;
        if s.is_finite() { s.clamp(0.0, self.len) } else { 0.0 }
    }
}

impl Structure {
    /// Member `id`'s ends (resolved values) and length; `None` when it is missing or has none.
    fn ends(&self, id: &str) -> Option<(String, (f64, f64), String, (f64, f64), f64)> {
        let m = self.member(id)?;
        let (a, b) = (self.node(&m.a)?, self.node(&m.b)?);
        let len = self.length(m).filter(|l| l.is_finite() && *l > 0.0)?;
        Some((m.a.clone(), (a.x.value, a.y.value), m.b.clone(), (b.x.value, b.y.value), len))
    }

    /// The line of a load on `member` going on over `then`: it starts at the end of `member`
    /// away from `then`'s first (its `a` end when there is none) and takes the members of `then`
    /// while each joins the last one's far end and runs on in the same direction; the first that
    /// does not ends it there. `None` when `member` is missing or has no length.
    pub fn spread_line(&self, member: &str, then: &[String]) -> Option<SpreadLine> {
        let (a, pa, b, pb, len) = self.ends(member)?;
        // Which way the first member runs: towards the one that follows it.
        let reversed = then.first().and_then(|n| self.ends(n)).is_some_and(|(na, _, nb, _, _)| (na == a || nb == a) && na != b && nb != b);
        let (start, mut end_node, mut end) = if reversed { (pb, a, pa) } else { (pa, b, pb) };
        let dir = ((end.0 - start.0) / len, (end.1 - start.1) / len);
        let mut parts = vec![LinePart { member: member.to_string(), at: 0.0, len, reversed }];
        let mut total = len;
        for id in then.iter().take(MAX_CHAIN) {
            if parts.iter().any(|p| p.member == *id) {
                break;
            }
            let Some((na, pna, nb, pnb, l)) = self.ends(id) else { break };
            let (rev, far_node, far) = if na == end_node {
                (false, nb, pnb)
            } else if nb == end_node {
                (true, na, pna)
            } else {
                break;
            };
            let d = ((far.0 - end.0) / l, (far.1 - end.1) / l);
            let cross = dir.0 * d.1 - dir.1 * d.0;
            let dot = dir.0 * d.0 + dir.1 * d.1;
            if !(cross.abs() <= ALIGNED && dot > 0.0) {
                break;
            }
            parts.push(LinePart { member: id.clone(), at: total, len: l, reversed: rev });
            total += l;
            end_node = far_node;
            end = far;
        }
        Some(SpreadLine { start, dir, len: total, parts })
    }

    /// The line of distributed load `l`; `None` for other loads.
    pub fn load_line(&self, l: &Load) -> Option<SpreadLine> {
        match l {
            Load::Distributed { member, then, .. } => self.spread_line(member, then),
            _ => None,
        }
    }

    /// Load `l` one member at a time: a distributed load over several members as one load on
    /// each it covers (its intensity at their ends, its local direction turned for a member
    /// running against the line), measured from each one's `a` end; any other load as it is.
    pub fn spread_pieces(&self, l: &Load) -> Vec<Load> {
        let Load::Distributed { member, then, q1, q2, from, to, angle, axes, per } = l else { return vec![l.clone()] };
        if then.is_empty() {
            return vec![l.clone()];
        }
        let Some(line) = self.spread_line(member, then) else { return Vec::new() };
        let s0 = from.value.clamp(0.0, line.len);
        let s1 = to.as_ref().map_or(line.len, |t| t.value).clamp(0.0, line.len);
        let (lo, hi) = if s0 <= s1 { (s0, s1) } else { (s1, s0) };
        let (qa, qb) = (q1.value, q2.as_ref().map_or(q1.value, |q| q.value));
        // The solver puts `q1` at the nearer end of a stretch.
        let q_at = |s: f64| if hi - lo > 0.0 { qa + (qb - qa) * (s - lo) / (hi - lo) } else { qa };
        let eps = 1e-9 * (1.0 + line.len);
        let mut out = Vec::new();
        for p in &line.parts {
            let (u0, u1) = (lo.max(p.at), hi.min(p.at + p.len));
            if u1 - u0 <= eps {
                continue;
            }
            let (qs, qe) = (q_at(u0), q_at(u1));
            let (f, t, qf, qt) = if p.reversed { (p.len - (u1 - p.at), p.len - (u0 - p.at), qe, qs) } else { (u0 - p.at, u1 - p.at, qs, qe) };
            let mut ang = angle.clone();
            if p.reversed && *axes == LoadAxes::Local {
                let a = angle.value + 180.0;
                ang = Num::new(if a > 180.0 { a - 360.0 } else { a });
            }
            out.push(Load::Distributed {
                member: p.member.clone(),
                then: Vec::new(),
                q1: Num::new(qf),
                q2: q2.as_ref().map(|_| Num::new(qt)),
                from: Num::new(f.max(0.0)),
                to: if (p.len - t).abs() <= eps { None } else { Some(Num::new(t)) },
                angle: ang,
                axes: *axes,
                per: *per,
            });
        }
        out
    }

    /// The structure with each distributed load over several members in pieces, one per member
    /// ([`Structure::spread_pieces`]), with, for each load, the index of the one it comes from.
    pub fn split_chains(&self) -> (Structure, Vec<usize>) {
        let mut out = self.clone();
        let mut from = Vec::with_capacity(self.loads.len());
        out.loads.clear();
        for (k, l) in self.loads.iter().enumerate() {
            for piece in self.spread_pieces(l) {
                out.loads.push(piece);
                from.push(k);
            }
        }
        (out, from)
    }

    /// The members load `l` lies on.
    pub fn load_members<'a>(&self, l: &'a Load) -> Vec<&'a str> {
        match l {
            Load::Force { member, .. } | Load::Moment { member, .. } => member.as_deref().into_iter().collect(),
            Load::Distributed { member, then, .. } => std::iter::once(member.as_str()).chain(then.iter().map(String::as_str)).collect(),
        }
    }

    /// The straight run of aligned members through member `id`, as a load line read in `id`'s
    /// direction (from `a` to `b`, or against it with `reversed`): every member joined end to end
    /// to it that runs on in the same direction, through nodes where exactly one such member goes
    /// on. Returns the run's first member, those after it, and where `id` starts along it.
    pub fn straight_run(&self, id: &str, reversed: bool) -> Option<(String, Vec<String>, f64)> {
        let (a, pa, b, pb, len) = self.ends(id)?;
        let (start_node, ps, end_node, pe) = if reversed { (b, pb, a, pa) } else { (a, pa, b, pb) };
        let dir = ((pe.0 - ps.0) / len, (pe.1 - ps.1) / len);
        // The aligned member going on from node `n` (at `p`) in direction `d`, not in `seen`.
        let next = |n: &str, p: (f64, f64), d: (f64, f64), seen: &[String]| -> Option<(String, String, (f64, f64), f64)> {
            let mut found = None;
            for m in self.members.iter().filter(|m| (m.a == n || m.b == n) && !seen.contains(&m.id)) {
                let Some((ma, pma, mb, pmb, l)) = self.ends(&m.id) else { continue };
                let (far_node, far) = if ma == n { (mb, pmb) } else { (ma, pma) };
                let e = ((far.0 - p.0) / l, (far.1 - p.1) / l);
                if (d.0 * e.1 - d.1 * e.0).abs() <= ALIGNED && d.0 * e.0 + d.1 * e.1 > 0.0 {
                    if found.is_some() {
                        return None;
                    }
                    found = Some((m.id.clone(), far_node, far, l));
                }
            }
            found
        };
        let mut seen = vec![id.to_string()];
        let mut after = Vec::new();
        let (mut n, mut p) = (end_node, pe);
        while seen.len() < MAX_CHAIN {
            let Some((m, far_node, far, _)) = next(&n, p, dir, &seen) else { break };
            seen.push(m.clone());
            after.push(m);
            n = far_node;
            p = far;
        }
        let mut before: Vec<String> = Vec::new();
        let mut offset = 0.0;
        let (mut n, mut p) = (start_node, ps);
        let back = (-dir.0, -dir.1);
        while seen.len() < MAX_CHAIN {
            let Some((m, far_node, far, l)) = next(&n, p, back, &seen) else { break };
            seen.push(m.clone());
            before.push(m);
            offset += l;
            n = far_node;
            p = far;
        }
        before.reverse();
        let mut all = before;
        all.push(id.to_string());
        all.extend(after);
        let mut it = all.into_iter();
        let first = it.next()?;
        Some((first, it.collect(), offset))
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{Load, LoadAxes, Structure};

    /// A → B → C along x (4 + 2), member 2 drawn C → B; D above B on a post.
    fn beam() -> Structure {
        Structure::from_json(
            r#"{"nodes":[{"id":"A","x":0,"y":0},{"id":"B","x":4,"y":0},{"id":"C","x":6,"y":0},{"id":"D","x":4,"y":3}],
                "members":[{"id":"1","a":"A","b":"B"},{"id":"2","a":"C","b":"B"},{"id":"3","a":"B","b":"D"}],
                "loads":[{"type":"distributed","member":"1","then":["2"],"q1":2,"q2":8,"axes":"local","angle":-90}]}"#,
        )
        .unwrap()
    }

    #[test]
    fn a_load_over_two_members_lies_along_one_line() {
        let st = beam();
        let line = st.load_line(&st.loads[0]).unwrap();
        assert_eq!((line.start, line.dir, line.len), ((0.0, 0.0), (1.0, 0.0), 6.0));
        assert_eq!(line.parts.len(), 2);
        assert!(line.parts[1].reversed);
        assert_eq!(line.locate(5.0), Some(("2", 1.0)));
        assert_eq!(line.locate(2.0), Some(("1", 2.0)));
        // Starting from 2's far end: the line runs right to left.
        let back = st.spread_line("2", &["1".to_string()]).unwrap();
        assert_eq!((back.start, back.dir, back.len), ((6.0, 0.0), (-1.0, 0.0), 6.0));
        // A member that does not go on straight ends the line.
        let bent = st.spread_line("1", &["3".to_string()]).unwrap();
        assert_eq!(bent.len, 4.0);
        assert!(st.spread_line("9", &[]).is_none());
    }

    #[test]
    fn its_pieces_meet_at_the_value_it_has_at_the_node() {
        let st = beam();
        let pieces = st.spread_pieces(&st.loads[0]);
        assert_eq!(pieces.len(), 2);
        // From 0 to 6, 2 → 8: 6 at B (s = 4).
        match (&pieces[0], &pieces[1]) {
            (
                Load::Distributed { member: m1, q1: a1, q2: b1, from: f1, to: t1, .. },
                Load::Distributed { member: m2, q1: a2, q2: b2, from: f2, to: t2, angle, axes, .. },
            ) => {
                assert_eq!((m1.as_str(), a1.value, b1.as_ref().map(|q| q.value), f1.value, t1.is_none()), ("1", 2.0, Some(6.0), 0.0, true));
                // Member 2 runs C → B: its `a` end carries 8, the local direction is turned round.
                assert_eq!((m2.as_str(), a2.value, b2.as_ref().map(|q| q.value), f2.value, t2.is_none()), ("2", 8.0, Some(6.0), 0.0, true));
                assert_eq!((angle.value, *axes), (90.0, LoadAxes::Local));
            }
            other => panic!("{other:?}"),
        }
        let (split, from) = st.split_chains();
        assert_eq!((split.loads.len(), from), (2, vec![0, 0]));
    }

    #[test]
    fn the_straight_run_goes_through_aligned_members_only() {
        let st = beam();
        let (first, then, at) = st.straight_run("2", true).unwrap();
        assert_eq!((first.as_str(), then, at), ("1", vec!["2".to_string()], 4.0));
        let (first, then, at) = st.straight_run("3", false).unwrap();
        assert_eq!((first.as_str(), then.len(), at), ("3", 0, 0.0));
    }

    /// The owner's beam: pin at A, roller at B, 10 kN/m from 1 m to C over 1 (A–B, 4.5 m) and
    /// 2 (B–C, 1 m) — as one load, or as two.
    fn owners_beam(one: bool) -> Structure {
        let loads = if one {
            r#"[{"type":"distributed","member":"1","then":["2"],"q1":10,"from":1}]"#
        } else {
            r#"[{"type":"distributed","member":"1","q1":10,"from":1},{"type":"distributed","member":"2","q1":10}]"#
        };
        Structure::from_json(&format!(
            r#"{{"nodes":[{{"id":"A","x":0,"y":0}},{{"id":"B","x":4.5,"y":0}},{{"id":"C","x":5.5,"y":0}}],
                "members":[{{"id":"1","a":"A","b":"B"}},{{"id":"2","a":"B","b":"C"}}],
                "supports":[{{"node":"A","kind":"pin"}},{{"node":"B","kind":"roller"}}],
                "loads":{loads}}}"#
        ))
        .unwrap()
    }

    #[test]
    fn one_load_over_two_members_solves_as_two_and_is_named_once() {
        let (one, two) = (crate::analyse(&owners_beam(true)), crate::analyse(&owners_beam(false)));
        assert!(one.warnings.is_empty(), "{:?}", one.warnings);
        assert_eq!(one.reactions.len(), two.reactions.len());
        for (a, b) in one.reactions.iter().zip(&two.reactions) {
            assert!((a.fx - b.fx).abs() < 1e-9 && (a.fy - b.fy).abs() < 1e-9, "{a:?} {b:?}");
        }
        // 45 kN at 3.25 m: V_B = 45·3.25/4.5.
        let vb = one.reactions.iter().find(|r| r.node == "B").map(|r| r.fy).unwrap();
        assert!((vb - 45.0 * 3.25 / 4.5).abs() < 1e-9, "{vb}");
        let st = owners_beam(true);
        let names = crate::load_names(&st);
        assert_eq!(names.iter().flatten().count(), 1);
        // One resultant: 45 kN in the middle of its stretch.
        let rs = crate::resultants(&st);
        assert_eq!(rs.len(), 1);
        assert!((rs[0].value().abs() - 45.0).abs() < 1e-9 && (rs[0].at.0 - 3.25).abs() < 1e-9, "{:?}", rs[0]);
        // Drawn with one label.
        let texts = st
            .render(None)
            .plot
            .prims
            .iter()
            .filter(|p| matches!(p, strix_plot::PlotPrim::Text { text, .. } | strix_plot::PlotPrim::TextAt { text, .. } if text.starts_with('q')))
            .count();
        assert_eq!(texts, 1);
    }

    #[test]
    fn a_load_over_members_drawn_right_to_left_turns_with_them() {
        // 2 then 1, read from C to A: oriented, it runs from A.
        let mut st = owners_beam(true);
        st.loads = vec![Load::Distributed {
            member: "2".into(),
            then: vec!["1".into()],
            q1: 4.0.into(),
            q2: Some(0.0.into()),
            from: 0.0.into(),
            to: Some(2.0.into()),
            angle: crate::Num::new(-90.0),
            axes: LoadAxes::Global,
            per: crate::LoadPer::Length,
        }];
        let o = st.clone().oriented();
        match &o.loads[0] {
            Load::Distributed { member, then, q1, q2, from, to, .. } => {
                assert_eq!((member.as_str(), then.as_slice()), ("1", ["2".to_string()].as_slice()));
                assert_eq!((q1.value, q2.as_ref().map(|q| q.value), from.value, to.as_ref().map(|t| t.value)), (0.0, Some(4.0), 3.5, Some(5.5)));
            }
            other => panic!("{other:?}"),
        }
        let (a, b) = (crate::analyse(&st), crate::analyse(&o));
        for (x, y) in a.reactions.iter().zip(&b.reactions) {
            assert!((x.fy - y.fy).abs() < 1e-9);
        }
    }

    #[test]
    fn hostile_chains_never_panic() {
        let st = Structure::from_json(
            r#"{"nodes":[{"id":"A","x":0,"y":0},{"id":"B","x":0,"y":0}],
                "members":[{"id":"1","a":"A","b":"B"},{"id":"2","a":"A","b":"A"}],
                "loads":[{"type":"distributed","member":"1","then":["1","2","x","1"],"q1":1,"from":-5,"to":1e9}]}"#,
        )
        .unwrap();
        assert!(st.spread_pieces(&st.loads[0]).is_empty());
        assert!(st.straight_run("1", false).is_none());
        let _ = st.split_chains();
    }
}
