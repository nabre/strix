//! A stretch's laws written with the names of the reactions and the loads (`docs/structures.md`,
//! Working): `T_1(x) = V_A - q_1·x`, `M_1(x) = V_A·x - q_1·x^2/2`, from the equilibrium of the
//! part of the structure before the section (the member's `a` end side). Each law is checked
//! against the solution's; a member in a closed ring, a varying load across the section or a law
//! that disagrees keeps its numbers ([`super::law`]). A reaction that came out negative keeps its
//! name too, taken the way it acts as the stretch's drawing shows it (`N_1(x) = H_A`, after
//! `H_A := -H_A` turned its value positive).

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
    let Some(left) = strix_structure::part_before(st, &src.id, &src.a, &src.b) else { return none };
    let in_left = |id: &str| left.iter().any(|x| x == id);
    let other_member_left = |id: &str| id != src.id && st.member(id).is_some_and(|x| in_left(&x.a) && in_left(&x.b));
    let eps = 1e-9 * (1.0 + len);

    let mut agents: Vec<Agent> = Vec::new();
    // The laws (N, T, M) a load across the section keeps from being named.
    let mut blocked = [false; 3];
    // A reaction by its name, the way it acts as the stretch's drawing shows it: one that came out
    // negative was taken the other way before the internal forces (`H_A := -H_A`).
    for k in us.iter().filter(|k| in_left(&k.node)) {
        let flip = if k.flipped(us) { -1.0 } else { 1.0 };
        match k.acts {
            Acts::Along(d) => agents.push(Agent::Point { name: k.name.clone(), mag: k.value * flip, at: k.at, dir: (d.0 * flip, d.1 * flip) }),
            Acts::Turns(sense) => agents.push(Agent::Couple { name: k.name.clone(), mag: k.value * flip, sign: sense * flip }),
        }
    }
    let whole_rs = resultants(st);
    // A load over several members, taken whole.
    let chains = chained(&Section { st, src, e, n, eps, s, u, other_member_left: &other_member_left }, &whole_rs, &mut agents, &mut blocked);
    // The loads on one member each.
    for (li, load) in st.loads.iter().enumerate() {
        if chains.contains(&li) {
            continue;
        }
        let mine: Vec<&Resultant> = whole_rs.iter().filter(|r| r.load == li).collect();
        let whole = match load {
            Load::Force { node: Some(nd), member: None, .. } | Load::Moment { node: Some(nd), member: None, .. } => in_left(nd),
            Load::Force { member: Some(mm), at, .. } | Load::Moment { member: Some(mm), at, .. } => {
                other_member_left(mm) || (*mm == src.id && at.value.clamp(0.0, len) <= s.x0 + eps)
            }
            Load::Distributed { member, from, to, q1, q2, angle, axes, per, .. } => {
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
                        // Across the section: uniform only, covering the whole stretch; else
                        // the laws it enters keep their numbers (a load across the member
                        // leaves N's law named, one along it T's and M's).
                        let (qa, qb) = (q1.value, q2.as_ref().map_or(q1.value, |q| q.value));
                        let th = angle.value.to_radians();
                        let dir = match axes {
                            LoadAxes::Global => (th.cos(), th.sin()),
                            LoadAxes::Local => (e.0 * th.cos() - e.1 * th.sin(), e.1 * th.cos() + e.0 * th.sin()),
                        };
                        let name = u.names.get(li).copied().flatten();
                        if qa != qb || s0 > s.x0 + eps || s1 < s.x1 - eps || name.is_none() {
                            blocked[0] |= dot(dir, e).abs() > TOL;
                            blocked[1] |= dot(dir, n).abs() > TOL;
                            blocked[2] = blocked[1];
                            continue;
                        }
                        let Some(name) = name else { continue };
                        let factor = match per {
                            LoadPer::Length => 1.0,
                            LoadPer::Projection => (e.0 * dir.1 - e.1 * dir.0).abs(),
                        };
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
    for ((slot, action), blocked) in out.iter_mut().zip([Action::Axial, Action::Shear, Action::Moment]).zip(blocked) {
        if blocked {
            continue;
        }
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

/// What the laws of a stretch are written for: the structure, the member, its axes, the stretch.
struct Section<'a> {
    st: &'a Structure,
    src: &'a strix_structure::Member,
    e: (f64, f64),
    n: (f64, f64),
    eps: f64,
    s: &'a Stretch,
    u: &'a Units,
    /// Is a member (not the one cut) wholly in the part before the section?
    other_member_left: &'a dyn Fn(&str) -> bool,
}

/// The loads over several members, each taken whole: by its resultant's name (`F_1`) when all
/// of it lies before the section; as one load from where it starts along the member's line
/// (`q_1·(x + 3 m)`) when it goes on across the section; not at all when it starts after it. Its
/// pieces, each a share of it, can't take its name. Returns the loads so taken (indices into the
/// structure's loads).
fn chained(sec: &Section, whole_rs: &[Resultant], agents: &mut Vec<Agent>, blocked: &mut [bool; 3]) -> Vec<usize> {
    let Section { st, src, e, n, eps, s, u, other_member_left } = *sec;
    let mut chains: Vec<usize> = Vec::new();
    for (li, load) in st.loads.iter().enumerate() {
        let Load::Distributed { member, then, q1, q2, from, to, angle, axes, per } = load else { continue };
        if then.is_empty() {
            continue;
        }
        chains.push(li);
        let Some(line) = st.spread_line(member, then) else { continue };
        let s0 = from.value.clamp(0.0, line.len);
        let s1 = to.as_ref().map_or(line.len, |t| t.value).clamp(0.0, line.len);
        let (s0, s1) = if s0 <= s1 { (s0, s1) } else { (s1, s0) };
        if s1 - s0 <= eps {
            continue;
        }
        let th = angle.value.to_radians();
        let d = line.dir;
        let dir = match axes {
            LoadAxes::Global => (th.cos(), th.sin()),
            LoadAxes::Local => (d.0 * th.cos() - d.1 * th.sin(), d.1 * th.cos() + d.0 * th.sin()),
        };
        let block = |blocked: &mut [bool; 3]| {
            blocked[0] |= dot(dir, e).abs() > TOL;
            blocked[1] |= dot(dir, n).abs() > TOL;
            blocked[2] = blocked[1];
        };
        let whole_load = |agents: &mut Vec<Agent>| {
            for r in whole_rs.iter().filter(|r| r.load == li) {
                let v = r.value();
                let sg = v.signum();
                agents.push(Agent::Point { name: u.resultant(r), mag: v.abs(), at: r.at, dir: (r.dir.0 * sg, r.dir.1 * sg) });
            }
        };
        let covered: Vec<&strix_structure::LinePart> = line.parts.iter().filter(|p| s0.max(p.at) < s1.min(p.at + p.len) - eps).collect();
        let Some(part) = line.parts.iter().find(|p| p.member == src.id) else {
            // On other members: all of it before the section, or none of it.
            if !covered.is_empty() && covered.iter().all(|p| other_member_left(&p.member)) {
                whole_load(agents);
            } else if covered.iter().any(|p| other_member_left(&p.member)) {
                block(blocked);
            }
            continue;
        };
        // Where the load starts and ends along the member, from its `a` end (beyond its ends
        // on the members before and after it).
        let local = |t: f64| if part.reversed { part.at + part.len - t } else { t - part.at };
        let (c0, c1) = (local(s0).min(local(s1)), local(s0).max(local(s1)));
        if c1 <= s.x0 + eps {
            whole_load(agents);
        } else if c0 >= s.x1 - eps {
            // After the section.
        } else {
            let (qa, qb) = (q1.value, q2.as_ref().map_or(q1.value, |q| q.value));
            let name = u.names.get(li).copied().flatten();
            match name {
                Some(name) if qa == qb && c0 <= s.x0 + eps && c1 >= s.x1 - eps => {
                    let factor = match per {
                        LoadPer::Length => 1.0,
                        LoadPer::Projection => (e.0 * dir.1 - e.1 * dir.0).abs(),
                    };
                    let sign = qa.signum();
                    agents.push(Agent::Spread {
                        name: name.linear_end(1),
                        q: qa.abs() * factor,
                        factor,
                        from: c0,
                        dir: (dir.0 * sign, dir.1 * sign),
                    });
                }
                _ => block(blocked),
            }
        }
    }
    chains
}

/// What acts on the part before the section.
enum Agent {
    /// A force of size `mag` along `dir` at `at`, named `name`.
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
    /// The share of a force along `dir` that acts along the member (`along`) or across it, as a
    /// factor: `·cos(30°)`, `·sin(30°)` of the angle between the force and the member; nothing
    /// for the whole force; by its number when the angle isn't a round one (hundredths).
    fn trig(&self, dir: (f64, f64), along: bool) -> String {
        let (c, s) = (dot(dir, self.e).abs(), cross(self.e, dir).abs());
        let g = if along { c } else { s };
        if (g - 1.0).abs() < 1e-9 {
            return String::new();
        }
        let deg = s.atan2(c).to_degrees();
        let round = (deg * 100.0).round() / 100.0;
        if (deg - round).abs() >= 1e-6 {
            return self.u.factor(g);
        }
        format!("·{}({}°)", if along { "cos" } else { "sin" }, super::precise(round, self.u.comma))
    }

    /// `x - 2 m`, `x - 3.2 m`, `x + 1 m` or `x` (a fraction only with no short decimal).
    fn shifted(&self, s: f64) -> Option<String> {
        if s.abs() <= TOL * (1.0 + s.abs()) {
            return None;
        }
        Some(format!("x {} {}", if s > 0.0 { "-" } else { "+" }, self.u.measure(s.abs())))
    }

    /// The section's internal force `action` from agent `ag` on the part before it (N: tension
    /// positive; T: turning clockwise; M: stretching the fibres on the member's −y side).
    fn term(&self, ag: &Agent, action: Action) -> Option<Term> {
        match (ag, action) {
            (Agent::Point { name, mag, dir, .. }, Action::Axial | Action::Shear) => {
                let g = if action == Action::Axial { -dot(*dir, self.e) } else { dot(*dir, self.n) };
                (g.abs() > TOL).then(|| Term {
                    sign: g.signum(),
                    text: format!("{name}{}", self.trig(*dir, action == Action::Axial)),
                    poly: [g * mag, 0.0, 0.0],
                })
            }
            (Agent::Point { name, mag, at, dir }, Action::Moment) => {
                let k1 = cross(self.e, *dir);
                let c0 = -cross((at.0 - self.a.0, at.1 - self.a.1), *dir);
                if k1.abs() > TOL {
                    let s = -c0 / k1;
                    let text = match self.shifted(s) {
                        Some(t) => format!("{name}{}·({t})", self.trig(*dir, false)),
                        None => format!("{name}{}·x", self.trig(*dir, false)),
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
                let f = if (factor - 1.0).abs() < 1e-9 { self.trig(*dir, action == Action::Axial) } else { self.u.factor(g * factor) };
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
