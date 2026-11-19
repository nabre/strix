//! A hyperstatic structure's redundants by the force method (`docs/structures.md`, Working):
//! the redundant reactions chosen so the equilibrium equations give every other one, their
//! constraints taken away (the released structure, isostatic), its bending moments under the
//! loads (system 0, `M_(0,1)(x)`) and under each redundant alone, equal to 1 (system i,
//! `m_(i,1)(x)`), the coefficients of the congruence equations by the principle of virtual work
//! (`δ_11 = ∫_(0 m)^(6 m) m_(1,1)(x)^2 dx`, `δ_10 = ∫ M_(0,1)(x)·m_(1,1)(x) dx`), then the equations
//! themselves, which calc solves (`0 kN·m^3 = δ_10 + V_B·δ_11`; two redundants by Cramer's rule).
//!
//! Without sections (`e`, `i`) `EI` is constant: the coefficients are multiplied by it and the
//! axial strains neglected, as the stiffness solution assumes; a truss uses its axial forces
//! (`EA` constant). Sections given for every member enter as `EI_1 = E·I` (and `EA_1 = E·A`
//! where an area is given). Anything else — three redundants or more, an internal redundant (a
//! closed frame), sections given for some members only — keeps the stiffness solution's values.

use strix_structure::{Analysis, Load, Num, Status, Stretch, Structure, SupportKind, analyse_released, stretches};

use super::{Acts, Equation, Labels, Line, Units, Unknown, at, law, precise, qty, stretch_header, stretch_tag, sub};

/// The most redundants solved this way (Cramer's rule beyond two is no working to read).
const MAX_DEGREE: usize = 2;

/// The force method's lines and the redundants they solve (indices into the unknowns), or
/// `None` when the structure isn't one it handles.
pub(super) fn section(
    st: &Structure,
    an: &Analysis,
    us: &[Unknown],
    eqs: &[Equation],
    u: &Units,
    labels: &Labels,
) -> Option<(Vec<Line>, Vec<usize>)> {
    let Status::Hyperstatic { degree } = an.status else { return None };
    if degree == 0 || degree > MAX_DEGREE {
        return None;
    }
    let reds = choose(eqs, us, vec![false; us.len()], degree)?;
    if reds.len() != degree {
        return None;
    }
    let mode = Mode::of(st, an.truss)?;
    if an.truss && reds.iter().any(|&i| us.get(i).is_some_and(|k| k.acts == Acts::Turns)) {
        return None;
    }
    let released: Vec<(usize, usize)> = reds.iter().filter_map(|&i| us.get(i).map(|k| (k.support, k.place))).collect();
    let a0 = analyse_released(st, &released);
    if a0.status != Status::Isostatic {
        return None;
    }
    let mut units = Vec::new();
    for &i in &reds {
        let k = us.get(i)?;
        let a = analyse_released(&unit_structure(st, k), &released);
        if a.status != Status::Isostatic {
            return None;
        }
        units.push(a);
    }
    let w = Writer { st, u, labels, mode, truss: an.truss, reds: reds.iter().filter_map(|&i| us.get(i)).collect() };
    Some((w.lines(&a0, &units), reds))
}

/// The redundants: unknowns no equation isolates, chosen so that, once they are known, the
/// equations give all the others; a roller's or link's reaction first, then a fixed end's couple,
/// then the rest (the textbook's released structures: a cantilever, a simply supported beam).
fn choose(eqs: &[Equation], us: &[Unknown], known: Vec<bool>, budget: usize) -> Option<Vec<usize>> {
    let known = closure(eqs, known);
    if known.iter().all(|k| *k) {
        return Some(Vec::new());
    }
    if budget == 0 {
        return None;
    }
    let mut open: Vec<usize> = (0..us.len()).filter(|&i| !known.get(i).copied().unwrap_or(true)).collect();
    open.sort_by_key(|&i| us.get(i).map_or(usize::MAX, rank));
    for i in open {
        let mut next = known.clone();
        if let Some(slot) = next.get_mut(i) {
            *slot = true;
        }
        if let Some(mut rest) = choose(eqs, us, next, budget - 1) {
            rest.insert(0, i);
            return Some(rest);
        }
    }
    None
}

/// What the equations solve one by one, once `known` are.
fn closure(eqs: &[Equation], mut known: Vec<bool>) -> Vec<bool> {
    while let Some(i) = eqs.iter().find_map(|e| match e.open(&known).as_slice() {
        [one] => Some(*one),
        _ => None,
    }) {
        if let Some(slot) = known.get_mut(i) {
            *slot = true;
        }
    }
    known
}

/// How readily an unknown is taken as a redundant (lower first).
fn rank(k: &Unknown) -> usize {
    match (k.kind, k.acts) {
        (SupportKind::Roller | SupportKind::RollerCircle | SupportKind::Link, Acts::Along(_)) => 0,
        (_, Acts::Turns) => 1,
        (SupportKind::Slider, _) => 2,
        (SupportKind::Pin, _) => 3,
        (SupportKind::Fixed, _) => 4,
    }
}

/// The structure with no loads but the redundant `k` equal to 1, acting as it counts positive.
fn unit_structure(st: &Structure, k: &Unknown) -> Structure {
    let load = match k.acts {
        Acts::Along(d) => Load::Force {
            node: Some(k.node.clone()),
            member: None,
            at: Num::new(0.0),
            value: Num::new(1.0),
            angle: Num::new(d.1.atan2(d.0).to_degrees()),
        },
        Acts::Turns => Load::Moment { node: Some(k.node.clone()), member: None, at: Num::new(0.0), value: Num::new(1.0) },
    };
    Structure { loads: vec![load], ..st.clone() }
}

/// How the members' stiffnesses enter the coefficients.
#[derive(Debug, Clone, PartialEq)]
enum Mode {
    /// The same `EI` (`EA` for a truss) everywhere: the coefficients are multiplied by it.
    Uniform,
    /// Each member's `EI_k = E·I`, and `EA_k = E·A` for those with an area (`axial[k]`).
    Explicit { axial: Vec<bool> },
}

impl Mode {
    fn of(st: &Structure, truss: bool) -> Option<Mode> {
        // As the solver takes it.
        let ei = |m: &strix_structure::Member| match (&m.e, &m.i) {
            (None, None) => 1.0,
            (e, i) => e.as_ref().map_or(1.0, |n| n.value) * i.as_ref().map_or(1.0, |n| n.value),
        };
        let first = st.members.first().map(ei)?;
        let same = st.members.iter().all(|m| (ei(m) - first).abs() <= 1e-9 * first.abs());
        let no_area = st.members.iter().all(|m| m.area.is_none());
        if same && no_area {
            return Some(Mode::Uniform);
        }
        if truss {
            return st.members.iter().all(|m| m.e.is_some() && m.area.is_some()).then(|| Mode::Explicit { axial: vec![true; st.members.len()] });
        }
        st.members
            .iter()
            .all(|m| m.e.is_some() && m.i.is_some())
            .then(|| Mode::Explicit { axial: st.members.iter().map(|m| m.area.is_some()).collect() })
    }
}

/// A stretch of a member as the coefficients integrate it: its system 0 stretch and each
/// system's laws there.
struct Piece<'a> {
    member: usize,
    stretch: &'a Stretch,
    tag: String,
    /// System 0's laws `[n, m]`, then each redundant's.
    laws: Vec<[[f64; 4]; 2]>,
}

/// The laws' kinds: axial force, bending moment.
const AXIAL: usize = 0;
const BENDING: usize = 1;

fn zero(c: &[f64; 4]) -> bool {
    let big = c.iter().fold(0.0_f64, |a, v| a.max(v.abs()));
    big < 1e-12
}

struct Writer<'a> {
    st: &'a Structure,
    u: &'a Units,
    labels: &'a Labels,
    mode: Mode,
    truss: bool,
    reds: Vec<&'a Unknown>,
}

impl Writer<'_> {
    fn lines(&self, a0: &Analysis, units: &[Analysis]) -> Vec<Line> {
        let u = self.u;
        let mut out = Vec::new();
        let names: Vec<&str> = self.reds.iter().map(|k| k.name.as_str()).collect();
        let words = if self.reds.len() == 1 { &self.labels.force1 } else { &self.labels.force2 };
        let (before, after) = words.split_once("{X}").unwrap_or((words.as_str(), ""));
        out.push(Line::noted(before.to_string(), names.join(", "), after.to_string()));
        // Each member's stretches under the loads, with every system's laws there.
        let all: Vec<(usize, Vec<Stretch>)> =
            a0.members.iter().enumerate().filter_map(|(k, m)| self.st.member(&m.id).map(|_| (k, stretches(m)))).collect();
        let mut pieces: Vec<Piece> = Vec::new();
        for (k, parts) in &all {
            let Some(m0) = a0.members.get(*k) else { continue };
            for (j, s) in parts.iter().enumerate() {
                let mid = (s.x0 + s.x1) / 2.0;
                let mut laws = vec![[s.n, s.m]];
                for a in units {
                    let law = a
                        .members
                        .iter()
                        .find(|m| m.id == m0.id)
                        .map(stretches)
                        .and_then(|ss| ss.into_iter().find(|t| t.x0 <= mid && mid <= t.x1).map(|t| [t.n, t.m]));
                    laws.push(law.unwrap_or([[0.0; 4]; 2]));
                }
                pieces.push(Piece { member: *k, stretch: s, tag: stretch_tag(&sub(&m0.id, 0), j, parts.len()), laws });
            }
        }
        let kinds: Vec<usize> = self.kinds_used();
        // A piece counts where some redundant's law isn't zero.
        let counts = |p: &Piece, kind: usize| self.enters(p.member, kind) && p.laws.iter().skip(1).any(|l| !zero(&l[kind]));
        let pieces: Vec<Piece> = pieces.into_iter().filter(|p| kinds.iter().any(|&kind| counts(p, kind))).collect();
        // System 0, then each redundant's.
        for s in 0..=self.reds.len() {
            if s == 0 {
                out.push(Line::words(self.labels.system0.clone()));
            } else if let Some(k) = self.reds.get(s - 1) {
                out.push(Line::noted(format!("{} ", self.labels.system.replace("{i}", &s.to_string())), format!("{} = 1", k.name), ":".to_string()));
            }
            for p in &pieces {
                let Some(m0) = a0.members.get(p.member) else { continue };
                let Some(src) = self.st.member(&m0.id) else { continue };
                let eqs: Vec<String> = kinds
                    .iter()
                    .filter(|&&kind| counts(p, kind))
                    .filter_map(|&kind| {
                        let c = p.laws.get(s)?[kind];
                        (s == 0 || !zero(&c)).then(|| format!("{}(x) = {}", self.law_name(s, kind, &p.tag), self.law_text(s, kind, &c)))
                    })
                    .collect();
                if eqs.is_empty() {
                    continue;
                }
                let mut line = stretch_header(src, p.stretch, u, self.labels);
                line.equations = eqs;
                line.keep = false;
                out.push(line);
            }
        }
        // The congruence equations' coefficients: the formula, how the stiffnesses enter.
        let kinds_used = self.kinds_used();
        let stiff = |p: &str, e: &str| if self.mode == Mode::Uniform { p.to_string() } else { format!("({p})/{e}") };
        let formula: Vec<String> = kinds_used
            .iter()
            .map(|&kind| {
                if kind == BENDING {
                    format!("∫▒〖{} dx〗", stiff("m_i·m_j", "EI"))
                } else {
                    format!("∫▒〖{} dx〗", stiff("n_i·n_j", "EA"))
                }
            })
            .collect();
        let after = match (&self.mode, self.truss) {
            (Mode::Uniform, false) => format!(" {}", self.labels.eiconst),
            (Mode::Uniform, true) => format!(" {}", self.labels.eaconst),
            (Mode::Explicit { axial }, false) if axial.iter().all(|a| !a) => format!(" {}", self.labels.noaxial),
            _ => ":".to_string(),
        };
        out.push(Line::noted(format!("{} ", self.labels.congruence), format!("δ_ij = {}", formula.join(" + ")), after));
        if let Mode::Explicit { axial } = &self.mode {
            let mut defs = Vec::new();
            let mut seen: Vec<usize> = Vec::new();
            for p in &pieces {
                if seen.contains(&p.member) {
                    continue;
                }
                seen.push(p.member);
                let Some(src) = a0.members.get(p.member).and_then(|m| self.st.member(&m.id)) else { continue };
                let id = sub(&src.id, p.member + 1);
                let e = src.e.as_ref().map_or(1.0, |n| n.value);
                let modulus = format!("({} {}/{}^2)", precise(e, u.comma), u.f, u.l);
                if !self.truss
                    && let Some(i) = &src.i
                {
                    let inertia = format!("({} {}^4)", precise(i.value, u.comma), u.l);
                    defs.push(format!("EI_{id} = {modulus}·{inertia} = {}·{}^2", u.f, u.l));
                }
                if axial.get(p.member).copied().unwrap_or(false)
                    && let Some(a) = &src.area
                {
                    let area = format!("({} {}^2)", precise(a.value, u.comma), u.l);
                    defs.push(format!("EA_{id} = {modulus}·{area} = {}", u.f));
                }
            }
            for chunk in defs.chunks(2) {
                out.push(Line::eqs(chunk.to_vec(), false));
            }
        }
        let n = self.reds.len();
        for i in 1..=n {
            for j in i..=n {
                out.push(Line::eqs(vec![self.coefficient(i, j, &pieces, a0)], false));
            }
        }
        for i in 1..=n {
            out.push(Line::eqs(vec![self.coefficient(i, 0, &pieces, a0)], false));
        }
        // The equations: one solved by calc, two by Cramer's rule.
        let x: Vec<&str> = names;
        match x.as_slice() {
            [x1] => {
                let zero = self.zero(1);
                out.push(Line { stepped: true, ..Line::eqs(vec![format!("{zero} = δ_10 + {x1}·δ_11")], true) });
            }
            [x1, x2] => {
                out.push(Line::noted(String::new(), format!("δ_10 + {x1}·δ_11 + {x2}·δ_12 = 0"), String::new()));
                out.push(Line::noted(String::new(), format!("δ_20 + {x1}·δ_12 + {x2}·δ_22 = 0"), String::new()));
                out.push(Line::words(self.labels.cramer.clone()));
                out.push(Line::eqs(vec![format!("{x1} = (δ_12·δ_20 - δ_10·δ_22)/(δ_11·δ_22 - δ_12^2) =")], true));
                out.push(Line::eqs(vec![format!("{x2} = (δ_12·δ_10 - δ_11·δ_20)/(δ_11·δ_22 - δ_12^2) =")], true));
            }
            _ => {}
        }
        out
    }

    /// The laws' kinds the coefficients use: bending moments, axial forces in a truss or where an
    /// area is given.
    fn kinds_used(&self) -> Vec<usize> {
        match (&self.mode, self.truss) {
            (_, true) => vec![AXIAL],
            (Mode::Explicit { axial }, false) if axial.iter().any(|a| *a) => vec![BENDING, AXIAL],
            _ => vec![BENDING],
        }
    }

    /// Does member `k`'s law of this kind enter the coefficients?
    fn enters(&self, k: usize, kind: usize) -> bool {
        match (kind, &self.mode) {
            (BENDING, _) => !self.truss,
            (_, Mode::Uniform) => self.truss,
            (_, Mode::Explicit { axial }) => axial.get(k).copied().unwrap_or(false),
        }
    }

    /// `M_(0,1a)`, `m_(2,1)`, `N_(0,3)`, `n_(1,3)`.
    fn law_name(&self, s: usize, kind: usize, tag: &str) -> String {
        let letter = match (kind, s) {
            (BENDING, 0) => "M",
            (BENDING, _) => "m",
            (_, 0) => "N",
            _ => "n",
        };
        format!("{letter}_({s},{tag})")
    }

    /// A law as calc reads it: system 0's in the forces' units, a redundant's per unit of it
    /// (a moment per unit force is a length).
    fn law_text(&self, s: usize, kind: usize, c: &[f64; 4]) -> String {
        let (f, l) = (self.u.f.as_str(), self.u.l.as_str());
        if s == 0 {
            let fl = format!("{f}/{l}");
            let fl2 = format!("{f}/{l}^2");
            let fl3 = format!("{f}/{l}^3");
            let fm = format!("{f}·{l}");
            return if kind == BENDING { law(c, [&fm, f, &fl, &fl2], self.u.comma) } else { law(c, [f, &fl, &fl2, &fl3], self.u.comma) };
        }
        // The power of the length in the law's constant term.
        let couple = self.reds.get(s - 1).is_some_and(|k| k.acts == Acts::Turns);
        let p0 = match (kind, couple) {
            (BENDING, false) => 1,
            (BENDING, true) | (_, false) => 0,
            (_, true) => -1,
        };
        unit_law(c, p0, l, self.u.comma)
    }

    /// `δ_ij = ∫ m_i·m_j/EI dx + …` (`j` 0: the loads), with the unit it comes out in.
    fn coefficient(&self, i: usize, j: usize, pieces: &[Piece], a0: &Analysis) -> String {
        let (lo, hi) = if j == 0 { (0, i) } else { (i, j) };
        let mut terms = Vec::new();
        for p in pieces {
            for kind in self.kinds_used() {
                if !self.enters(p.member, kind) {
                    continue;
                }
                let (Some(a), Some(b)) = (p.laws.get(lo), p.laws.get(hi)) else { continue };
                if zero(&a[kind]) || zero(&b[kind]) {
                    continue;
                }
                let (na, nb) = (self.law_name(lo, kind, &p.tag), self.law_name(hi, kind, &p.tag));
                let product = if lo == hi { format!("{na}(x)^2") } else { format!("{na}(x)·{nb}(x)") };
                let integrand = match &self.mode {
                    Mode::Uniform => product,
                    Mode::Explicit { .. } => {
                        let id = a0.members.get(p.member).map_or_else(|| (p.member + 1).to_string(), |m| sub(&m.id, p.member + 1));
                        format!("({product})/E{}_{id}", if kind == BENDING { "I" } else { "A" })
                    }
                };
                let (x0, x1) = (at(p.stretch.x0, &self.u.l, self.u.comma), at(p.stretch.x1, &self.u.l, self.u.comma));
                terms.push(format!("∫_({x0})^({x1})▒〖{integrand} dx〗"));
            }
        }
        let name = if j == 0 { format!("δ_{i}0") } else { format!("δ_{i}{j}") };
        let unit = self.unit(lo, hi);
        if terms.is_empty() {
            return if unit.is_empty() { format!("{name} = 0") } else { format!("{name} = (0 {unit})") };
        }
        format!("{name} = {} = {unit}", terms.join(" + ")).trim_end().to_string()
    }

    /// The `0` an equation of congruence starts with, in its terms' unit.
    fn zero(&self, i: usize) -> String {
        let unit = self.unit(0, i);
        if unit.is_empty() { "0".to_string() } else { format!("0 {unit}") }
    }

    /// The unit of `δ_ab` (system 0 is the loads').
    fn unit(&self, a: usize, b: usize) -> String {
        let bending = !self.truss;
        // (force, length) powers of a system's law.
        let law = |s: usize| -> (i32, i32) {
            let couple = s > 0 && self.reds.get(s - 1).is_some_and(|k| k.acts == Acts::Turns);
            match (bending, s == 0, couple) {
                (true, true, _) => (1, 1),
                (true, false, false) => (0, 1),
                (true, false, true) => (0, 0),
                (false, true, _) => (1, 0),
                (false, false, false) => (0, 0),
                (false, false, true) => (0, -1),
            }
        };
        let (fa, la) = law(a);
        let (fb, lb) = law(b);
        let (mut f, mut l) = (fa + fb, la + lb + 1);
        if matches!(self.mode, Mode::Explicit { .. }) {
            f -= 1;
            if bending {
                l -= 2;
            }
        }
        unit_text(f, l, &self.u.f, &self.u.l)
    }
}

/// A redundant's law `c0 + c1·x + …` per unit of it, its constant term's unit the length to the
/// power `p0`: `(6 m) - x`, `(-2)/3·x`... written `-1 + x/(6 m)` when the length divides.
fn unit_law(c: &[f64; 4], p0: i32, l: &str, comma: bool) -> String {
    let power = |p: i32| if p == 1 { l.to_string() } else { format!("{l}^{p}") };
    let mut out = String::new();
    for (k, &v) in c.iter().enumerate() {
        if zero(&[v, 0.0, 0.0, 0.0]) || v.abs() < 1e-12 * c.iter().fold(0.0_f64, |a, x| a.max(x.abs())) {
            continue;
        }
        let pw = p0 - k as i32;
        let x = match k {
            0 => String::new(),
            1 => "x".to_string(),
            _ => format!("x^{k}"),
        };
        // |v| as p/q, or a decimal.
        let (p, q) = fraction(v.abs(), comma);
        let term = if pw > 0 {
            let coef = if q == "1" { format!("({p} {})", power(pw)) } else { format!("({p} {})/{q}", power(pw)) };
            if x.is_empty() { coef } else { format!("{coef}·{x}") }
        } else if pw == 0 {
            match (x.is_empty(), p.as_str(), q.as_str()) {
                (true, _, "1") => p.clone(),
                (true, _, _) => format!("{p}/{q}"),
                (false, "1", "1") => x.clone(),
                (false, _, "1") => format!("{p}·{x}"),
                (false, _, _) => format!("{p}/{q}·{x}"),
            }
        } else {
            let top = match (x.is_empty(), p.as_str()) {
                (true, _) => p.clone(),
                (false, "1") => x.clone(),
                (false, _) => format!("{p}·{x}"),
            };
            let den = if q == "1" { power(-pw) } else { format!("{q} {}", power(-pw)) };
            format!("{top}/({den})")
        };
        if out.is_empty() {
            // Its sign inside a quantity's parentheses (`(-8 m)/3`), as calc reads a number.
            if v < 0.0 && term.starts_with('(') {
                out.push_str(&term.replacen('(', "(-", 1));
                continue;
            }
            if v < 0.0 {
                out.push('-');
            }
        } else {
            out.push_str(if v < 0.0 { " - " } else { " + " });
        }
        out.push_str(&term);
    }
    if out.is_empty() { "0".to_string() } else { out }
}

/// A positive number as `(p, q)`, a simple fraction's terms, or `(decimal, "1")`.
fn fraction(a: f64, comma: bool) -> (String, String) {
    let t = qty(a, "", comma);
    let inner = |t: &str| t.trim_start_matches('(').trim_end_matches(')').trim().to_string();
    match t.split_once(")/") {
        Some((p, q)) => (inner(p), q.to_string()),
        None => (inner(&t), "1".to_string()),
    }
}

/// A unit from its powers of force and length: `kN·m^3`, `m/kN`, `m^3`, `kN^-1`, nothing.
fn unit_text(f: i32, l: i32, fu: &str, lu: &str) -> String {
    let power = |u: &str, p: i32| if p == 1 { u.to_string() } else { format!("{u}^{p}") };
    let mut num = Vec::new();
    let mut den = Vec::new();
    for (u, p) in [(fu, f), (lu, l)] {
        if p > 0 {
            num.push(power(u, p));
        } else if p < 0 {
            den.push(power(u, -p));
        }
    }
    match (num.is_empty(), den.is_empty()) {
        (_, true) => num.join("·"),
        (false, false) => format!("{}/{}", num.join("·"), den.join("·")),
        (true, false) => [(fu, f), (lu, l)].iter().filter(|(_, p)| *p < 0).map(|(u, p)| power(u, *p)).collect::<Vec<_>>().join("·"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_and_unit_laws() {
        assert_eq!(unit_text(1, 3, "kN", "m"), "kN·m^3");
        assert_eq!(unit_text(-1, 1, "kN", "m"), "m/kN");
        assert_eq!(unit_text(0, 3, "kN", "m"), "m^3");
        assert_eq!(unit_text(-1, 0, "kN", "m"), "kN^-1");
        assert_eq!(unit_text(0, 0, "kN", "m"), "");
        assert_eq!(unit_law(&[6.0, -1.0, 0.0, 0.0], 1, "m", false), "(6 m) - x");
        assert_eq!(unit_law(&[0.0, -2.0 / 3.0, 0.0, 0.0], 1, "m", false), "-2/3·x");
        assert_eq!(unit_law(&[-8.0 / 3.0, 1.0 / 3.0, 0.0, 0.0], 1, "m", false), "(-8 m)/3 + 1/3·x");
        assert_eq!(unit_law(&[-1.0, 1.0 / 6.0, 0.0, 0.0], 0, "m", false), "-1 + x/(6 m)");
        assert_eq!(unit_law(&[0.0, 0.0, 0.0, 0.0], 0, "m", false), "0");
        assert_eq!(unit_law(&[0.5, 0.0, 0.0, 0.0], -1, "m", false), "1/(2 m)");
    }
}
