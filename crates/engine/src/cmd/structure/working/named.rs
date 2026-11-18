//! A stretch's laws written with the names of the reactions and the loads (`docs/structures.md`,
//! Working): `T_1(x) = V_A - q_1·x`, `M_1(x) = V_A·x - q_1·x^2/2`, from the equilibrium of the
//! part of the structure before the section (the member's `a` end side). Each law is checked
//! against the solution's; a member in a closed ring, a varying load across the section or a law
//! that disagrees keeps its numbers ([`super::law`]).

use strix_structure::{Action, Load, LoadAxes, LoadPer, MemberResult, Resultant, ResultantKind, Stretch, Structure, resultants};

use super::{Acts, Units, Unknown, arg};

const TOL: f64 = 1e-9;

/// One term of a law: its sign, its text without the sign, its value as a polynomial in `x`.
struct Term {
    sign: f64,
    text: String,
    poly: [f64; 3],
}

/// The laws of N, T, M of member `m`'s stretch `s` with the names (`None` for one that can't be
/// written so).
pub(super) fn laws(st: &Structure, us: &[Unknown], m: &MemberResult, s: &Stretch, u: &Units) -> [Option<String>; 3] {
    let none = [None, None, None];
    let Some(src) = st.member(&m.id) else { return none };
    let (Some(a), Some(b)) = (st.node(&src.a), st.node(&src.b)) else { return none };
    let len = m.length;
    if len <= 0.0 {
        return none;
    }
    let a = (a.x.value, a.y.value);
    let e = ((b.x.value - a.0) / len, (b.y.value - a.1) / len);
    let n = (-e.1, e.0);
    // The part before the section: the nodes reached from `a` without the member.
    let Some(left) = left_nodes(st, &src.id, &src.a, &src.b) else { return none };
    let in_left = |id: &str| left.iter().any(|x| x == id);
    let other_member_left = |id: &str| id != src.id && st.member(id).is_some_and(|x| in_left(&x.a) && in_left(&x.b));
    let eps = 1e-9 * (1.0 + len);

    let mut agents: Vec<Agent> = Vec::new();
    for k in us.iter().filter(|k| in_left(&k.node)) {
        match k.acts {
            Acts::Along(d) => agents.push(Agent::Point { name: k.name.clone(), mag: k.value, at: k.at, dir: d }),
            Acts::Turns => agents.push(Agent::Couple { name: k.name.clone(), mag: k.value, sign: 1.0 }),
        }
    }
    let rs = resultants(st);
    for (li, load) in st.loads.iter().enumerate() {
        let mine: Vec<&Resultant> = rs.iter().filter(|r| r.load == li).collect();
        let whole = match load {
            Load::Force { node: Some(nd), member: None, .. } | Load::Moment { node: Some(nd), member: None, .. } => in_left(nd),
            Load::Force { member: Some(mm), at, .. } | Load::Moment { member: Some(mm), at, .. } => {
                other_member_left(mm) || (*mm == src.id && at.value.clamp(0.0, len) <= s.x0 + eps)
            }
            Load::Distributed { member, from, to, q1, q2, angle, axes, per } => {
                if other_member_left(member) {
                    true
                } else if *member == src.id {
                    let s0 = from.value.clamp(0.0, len);
                    let s1 = to.as_ref().map_or(len, |t| t.value).clamp(0.0, len);
                    let (s0, s1) = if s0 <= s1 { (s0, s1) } else { (s1, s0) };
                    if s1 <= s0 || s0 >= s.x1 - eps {
                        false
                    } else if s1 <= s0 + eps || s1 <= s.x0 + eps {
                        true
                    } else {
                        // Across the section: uniform only, covering the whole stretch.
                        let (qa, qb) = (q1.value, q2.as_ref().map_or(q1.value, |q| q.value));
                        if qa != qb || s0 > s.x0 + eps || s1 < s.x1 - eps {
                            return none;
                        }
                        let th = angle.value.to_radians();
                        let dir = match axes {
                            LoadAxes::Global => (th.cos(), th.sin()),
                            LoadAxes::Local => (e.0 * th.cos() - e.1 * th.sin(), e.1 * th.cos() + e.0 * th.sin()),
                        };
                        let factor = match per {
                            LoadPer::Length => 1.0,
                            LoadPer::Projection => (e.0 * dir.1 - e.1 * dir.0).abs(),
                        };
                        let Some(name) = u.names.get(li).copied().flatten() else { return none };
                        let sign = qa.signum();
                        agents.push(Agent::Spread {
                            name: name.linear_end(1),
                            q: qa.abs() * factor,
                            factor,
                            from: s0,
                            dir: (dir.0 * sign, dir.1 * sign),
                        });
                        false
                    }
                } else {
                    false
                }
            }
            _ => false,
        };
        if !whole {
            continue;
        }
        for r in mine {
            let v = r.value();
            if let ResultantKind::Couple { .. } = r.kind {
                agents.push(Agent::Couple { name: u.resultant(r), mag: v.abs(), sign: v.signum() });
            } else {
                let sg = v.signum();
                agents.push(Agent::Point { name: u.resultant(r), mag: v.abs(), at: r.at, dir: (r.dir.0 * sg, r.dir.1 * sg) });
            }
        }
    }

    let geo = Geometry { a, e, n, u };
    let mut out = [None, None, None];
    for (slot, action) in out.iter_mut().zip([Action::Axial, Action::Shear, Action::Moment]) {
        let terms: Vec<Term> = agents.iter().filter_map(|ag| geo.term(ag, action)).collect();
        let fit = match action {
            Action::Axial => &s.n,
            Action::Shear => &s.v,
            Action::Moment => &s.m,
        };
        if agree(&terms, fit, s) {
            *slot = Some(text(&terms, action, u));
        }
    }
    out
}

/// What acts on the part before the section.
enum Agent {
    /// A force of size `mag` (a reaction: its signed value) along `dir` at `at`, named `name`.
    Point { name: String, mag: f64, at: (f64, f64), dir: (f64, f64) },
    /// A couple of size `mag`, counter-clockwise when `sign` is positive.
    Couple { name: String, mag: f64, sign: f64 },
    /// A uniform load `q` (per unit of the member's length) along `dir`, from `from` up to the
    /// section; `factor`: its projection's share of the length.
    Spread { name: String, q: f64, factor: f64, from: f64, dir: (f64, f64) },
}

struct Geometry<'a> {
    a: (f64, f64),
    e: (f64, f64),
    n: (f64, f64),
    u: &'a Units,
}

fn dot(p: (f64, f64), q: (f64, f64)) -> f64 {
    p.0 * q.0 + p.1 * q.1
}

fn cross(p: (f64, f64), q: (f64, f64)) -> f64 {
    p.0 * q.1 - p.1 * q.0
}

impl Geometry<'_> {
    /// `x - 2 m`, `x + 1 m` or `x`.
    fn shifted(&self, s: f64) -> Option<String> {
        if s.abs() <= TOL * (1.0 + s.abs()) {
            return None;
        }
        Some(format!("x {} {}", if s > 0.0 { "-" } else { "+" }, arg(s.abs(), &self.u.l, self.u.comma)))
    }

    /// The section's internal force `action` from agent `ag` on the part before it (N: tension
    /// positive; T: turning clockwise; M: stretching the fibres on the member's −y side).
    fn term(&self, ag: &Agent, action: Action) -> Option<Term> {
        match (ag, action) {
            (Agent::Point { name, mag, dir, .. }, Action::Axial | Action::Shear) => {
                let g = if action == Action::Axial { -dot(*dir, self.e) } else { dot(*dir, self.n) };
                (g.abs() > TOL).then(|| Term { sign: g.signum(), text: format!("{name}{}", self.u.factor(g)), poly: [g * mag, 0.0, 0.0] })
            }
            (Agent::Point { name, mag, at, dir }, Action::Moment) => {
                let k1 = cross(self.e, *dir);
                let c0 = -cross((at.0 - self.a.0, at.1 - self.a.1), *dir);
                if k1.abs() > TOL {
                    let s = -c0 / k1;
                    let text = match self.shifted(s) {
                        Some(t) => format!("{name}{}·({t})", self.u.factor(k1)),
                        None => format!("{name}{}·x", self.u.factor(k1)),
                    };
                    Some(Term { sign: k1.signum(), text, poly: [mag * c0, mag * k1, 0.0] })
                } else if c0.abs() > TOL {
                    Some(Term { sign: c0.signum(), text: format!("{name}·{}", arg(c0.abs(), &self.u.l, self.u.comma)), poly: [mag * c0, 0.0, 0.0] })
                } else {
                    None
                }
            }
            (Agent::Couple { name, mag, sign }, Action::Moment) => Some(Term { sign: -sign, text: name.clone(), poly: [-sign * mag, 0.0, 0.0] }),
            (Agent::Couple { .. }, _) => None,
            (Agent::Spread { name, q, factor, from, dir }, _) => {
                let g = match action {
                    Action::Axial => -dot(*dir, self.e),
                    Action::Shear => dot(*dir, self.n),
                    Action::Moment => cross(self.e, *dir),
                };
                if g.abs() <= TOL {
                    return None;
                }
                let f = self.u.factor(g * factor);
                let x = self.shifted(*from).map_or_else(|| "x".to_string(), |t| format!("({t})"));
                let s0 = *from;
                Some(if action == Action::Moment {
                    // q·(x − s0)²/2.
                    Term { sign: g.signum(), text: format!("{name}{f}·{x}^2/2"), poly: [g * q * s0 * s0 / 2.0, -g * q * s0, g * q / 2.0] }
                } else {
                    Term { sign: g.signum(), text: format!("{name}{f}·{x}"), poly: [-g * q * s0, g * q, 0.0] }
                })
            }
        }
    }
}

/// The terms add up to the solution's law over the stretch.
fn agree(terms: &[Term], fit: &[f64; 4], s: &Stretch) -> bool {
    let value = |c: &[f64], x: f64| c.iter().rev().fold(0.0, |acc, k| acc * x + k);
    let xs: Vec<f64> = (0..=4).map(|k| s.x0 + (s.x1 - s.x0) * f64::from(k) / 4.0).collect();
    let scale =
        xs.iter().map(|&x| value(fit, x).abs()).chain(terms.iter().flat_map(|t| xs.iter().map(|&x| value(&t.poly, x).abs()))).fold(1e-9, f64::max);
    xs.iter().all(|&x| {
        let sum: f64 = terms.iter().map(|t| value(&t.poly, x)).sum();
        (sum - value(fit, x)).abs() <= 1e-6 * scale
    })
}

/// The law's text: `V_A - q_1·x`; no term at all, `0 kN` (`0 kN·m`).
fn text(terms: &[Term], action: Action, u: &Units) -> String {
    let mut out = String::new();
    for t in terms {
        if out.is_empty() {
            if t.sign < 0.0 {
                out.push('-');
            }
        } else {
            out.push_str(if t.sign < 0.0 { " - " } else { " + " });
        }
        out.push_str(&t.text);
    }
    if out.is_empty() {
        out = if action == Action::Moment { format!("0 {}·{}", u.f, u.l) } else { format!("0 {}", u.f) };
    }
    out
}

/// The nodes reached from `start` without member `cut`; `None` when `end` is among them (the
/// member closes a ring: no part lies before its sections alone).
fn left_nodes(st: &Structure, cut: &str, start: &str, end: &str) -> Option<Vec<String>> {
    let mut seen = vec![start.to_string()];
    let mut frontier = vec![start.to_string()];
    while let Some(v) = frontier.pop() {
        for m in st.members.iter().filter(|m| m.id != cut) {
            let other = if m.a == v {
                &m.b
            } else if m.b == v {
                &m.a
            } else {
                continue;
            };
            if !seen.contains(other) {
                if seen.len() > st.nodes.len() {
                    return None;
                }
                seen.push(other.clone());
                frontier.push(other.clone());
            }
        }
    }
    (!seen.iter().any(|x| x == end)).then_some(seen)
}
