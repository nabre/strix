//! The working of a structure exercise split by its questions (`docs/structures.md`, Exercise):
//! the reactions; the laws of N; of T; where T vanishes (its law solved within the stretch) or changes sign
//! under a concentrated load; the moment there; the laws of M. Each part is written under its
//! question in the solution, in reading order, so a law is defined before what uses it (the
//! moment's laws of the stretches where T vanishes come with the moment there, and again with
//! the diagram of M).

use serde_json::Value;
use strix_structure::{Action, MemberResult, NotableKind, Stretch, Structure, notables, stretches};
use wordcraft_doc::InlineObject;
use wordcraft_doc::para::Paragraph;

use super::{
    Figure, Figures, Labels, Line, Units, lines, num, paragraphs, paragraphs_with, shear_name, signed, stretch_header, stretch_tag, sub, units,
    unknowns,
};
use crate::{CmdError, Session};

/// The solution's parts, one per question (a truss has only the reactions and the axial forces:
/// the others are empty).
pub(in crate::cmd::structure) struct Parts {
    pub truss: bool,
    pub reactions: Vec<Paragraph>,
    pub axial: Vec<Paragraph>,
    pub shear: Vec<Paragraph>,
    pub zeros: Vec<Paragraph>,
    pub maxima: Vec<Paragraph>,
    pub moments: Vec<Paragraph>,
}

/// The questions' numbers the results belong to (the exercise's results summary).
const Q_REACTIONS: u32 = 1;
const Q_ZEROS: u32 = 4;
const Q_MAXIMA: u32 = 5;

/// The working of structure `src` split by the exercise's questions.
pub(in crate::cmd::structure) fn parts(
    s: &Session,
    src: &std::sync::Arc<str>,
    v: &Value,
    props: &wordcraft_doc::CharProps,
) -> Result<Parts, CmdError> {
    let st = Structure::from_json(src).map_err(|e| CmdError::Failed(format!("the structure can't be read: {e}")))?;
    let r = crate::cmd::calc::resolved_structure(s, src)
        .unwrap_or_else(|| st.render(Some(&strix_calc::Worksheet::new(crate::cmd::calc::options_now(s)))));
    if !r.analysis.status.is_solved() {
        return Err(CmdError::Failed(format!("the structure can't be solved: {}", r.analysis.status.describe())));
    }
    // The drawings from the structure as written (its parameters, not their values).
    let written = st.clone();
    let figures = Figures { s, st: &written };
    let (st, an) = (&r.resolved, &r.analysis);
    let comma = crate::cmd::calc::options_now(s).decimal_comma;
    let labels = Labels::from(v);
    let all = lines(st, an, &labels, comma);
    let forces = format!("{}: {}", labels.forces, labels.conventions);
    let axial = format!("{}:", labels.axial);
    let split = all.iter().position(|l| l.note.is_none() && l.equations.is_empty() && (l.label == forces || l.label == axial)).unwrap_or(all.len());
    let (first, rest) = all.split_at(split);
    // Without the working's heading.
    let first = first.get(1..).unwrap_or(&[]);
    let mut reactions = paragraphs_with(first, props, Some(&figures))?;
    // The reactions are the first question's result: the equations solving them, and those
    // giving a redundant its value.
    let names: Vec<String> = unknowns(st, an).into_iter().map(|k| format!("{} = ", k.name)).collect();
    for (line, para) in first.iter().zip(reactions.iter_mut()) {
        if line.stepped || (!line.equations.is_empty() && line.equations.iter().all(|e| names.iter().any(|n| e.starts_with(n.as_str())))) {
            mark(para, Q_REACTIONS);
        }
    }
    if an.truss {
        return Ok(Parts {
            truss: true,
            reactions,
            axial: paragraphs(rest, props)?,
            shear: Vec::new(),
            zeros: Vec::new(),
            maxima: Vec::new(),
            moments: Vec::new(),
        });
    }
    let u = units(st, comma);
    let w = Writer { st, u: &u, labels: &labels, shear: shear_name(st), us: unknowns(st, an) };
    let members: Vec<(&MemberResult, Vec<Stretch>)> = an.members.iter().map(|m| (m, stretches(m))).collect();
    let mut axial_lines = vec![Line::heading(forces)];
    let mut shear_lines = Vec::new();
    let mut moment_lines = Vec::new();
    for (m, parts) in &members {
        w.laws(m, &w.axial_parts(m, parts), Action::Axial, &mut axial_lines);
        w.laws(m, parts, Action::Shear, &mut shear_lines);
        w.laws(m, parts, Action::Moment, &mut moment_lines);
    }
    let points = points(&members);
    let (zero_lines, zero_marked) = w.zeros(&members, &points);
    let (max_lines, max_marked) = w.maxima(&members, &points);
    let marked = |lines: &[Line], marks: &[bool], q: u32| -> Result<Vec<Paragraph>, CmdError> {
        let mut out = paragraphs(lines, props)?;
        for (para, _) in out.iter_mut().zip(marks).filter(|(_, m)| **m) {
            mark(para, q);
        }
        Ok(out)
    };
    Ok(Parts {
        truss: false,
        reactions,
        axial: paragraphs(&cap(axial_lines), props)?,
        shear: paragraphs_with(&cap(shear_lines), props, Some(&figures))?,
        zeros: marked(&zero_lines, &zero_marked, Q_ZEROS)?,
        maxima: marked(&max_lines, &max_marked, Q_MAXIMA)?,
        moments: paragraphs_with(&cap(moment_lines), props, Some(&figures))?,
    })
}

/// At most [`super::MAX_LINES`] lines in a part.
fn cap(mut lines: Vec<Line>) -> Vec<Line> {
    lines.truncate(super::MAX_LINES);
    lines
}

/// Marks every equation of `para` as a result of question `q`.
fn mark(para: &mut Paragraph, q: u32) {
    let offs: Vec<usize> = para.objects_with_offsets().filter(|(_, o)| matches!(o, InlineObject::Equation { .. })).map(|(off, _)| off).collect();
    for off in offs {
        if let Some(InlineObject::Equation { math, .. }) = para.object_at_mut(off) {
            math.answer = q;
        }
    }
}

/// A law's value at `x`.
fn value(c: &[f64; 4], x: f64) -> f64 {
    c.iter().rev().fold(0.0, |acc, k| acc * x + k)
}

/// Where the moment may be greatest: T vanishing inside a stretch, or changing sign from one
/// stretch to the next (a concentrated force).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Point {
    /// Member `m`'s stretch `k`: T vanishes at these places inside it (at most two: T is a
    /// polynomial of degree two at most under the loads Strix knows).
    Zero { m: usize, k: usize, xs: [Option<f64>; 2] },
    /// T changes sign between member `m`'s stretches `k` and `k + 1`, at `x`.
    Jump { m: usize, k: usize, x: f64 },
}

fn points(members: &[(&MemberResult, Vec<Stretch>)]) -> Vec<Point> {
    let mut out = Vec::new();
    for (mi, (m, parts)) in members.iter().enumerate() {
        let eps = 1e-9 * (1.0 + m.length);
        let zeros: Vec<f64> = notables(m, Action::Shear).into_iter().filter(|n| n.kind == NotableKind::Zero).map(|n| n.x).collect();
        let scale = parts.iter().flat_map(|s| [value(&s.v, s.x0).abs(), value(&s.v, s.x1).abs()]).fold(0.0_f64, f64::max);
        let tol = 1e-9 * scale.max(1e-12);
        for (k, s) in parts.iter().enumerate() {
            let inside: Vec<f64> = zeros.iter().copied().filter(|x| *x > s.x0 + eps && *x < s.x1 - eps).take(2).collect();
            if !inside.is_empty() {
                out.push(Point::Zero { m: mi, k, xs: [inside.first().copied(), inside.get(1).copied()] });
            }
            if let Some(next) = parts.get(k + 1) {
                let (before, after) = (value(&s.v, s.x1), value(&next.v, next.x0));
                // A sign change, or T vanishing at the stretch's end from one side only.
                if (before > tol && after < -tol) || (before < -tol && after > tol) {
                    out.push(Point::Jump { m: mi, k, x: s.x1 });
                }
            }
        }
    }
    out
}

struct Writer<'a> {
    st: &'a Structure,
    u: &'a Units,
    labels: &'a Labels,
    shear: String,
    /// The reactions, by their names.
    us: Vec<super::Unknown>,
}

impl Writer<'_> {
    fn names(&self, m: &MemberResult, k: usize, count: usize) -> (String, String) {
        let tag = stretch_tag(&sub(&m.id, 0), k, count);
        let x = format!("x_{tag}");
        (tag, x)
    }

    /// A law's name: `N`, `T`, `M`.
    fn letter(&self, action: Action) -> &str {
        match action {
            Action::Axial => "N",
            Action::Shear => self.shear.as_str(),
            Action::Moment => "M",
        }
    }

    fn unit(&self, action: Action) -> String {
        match action {
            Action::Moment => format!("{}·{}", self.u.f, self.u.l),
            _ => self.u.f.clone(),
        }
    }

    /// `M_1a(x) = …`: stretch `s`'s law of `action` (member `m`'s), with the reactions' and the
    /// loads' names where it can be.
    fn law(&self, m: &MemberResult, tag: &str, s: &Stretch, action: Action) -> String {
        let [n, v, mo] = super::named::laws(self.st, &self.us, m, s, self.u);
        let named = match action {
            Action::Axial => n,
            Action::Shear => v,
            Action::Moment => mo,
        };
        if let Some(body) = named {
            return format!("{}_{tag}(x) = {body}", self.letter(action));
        }
        let (f, l) = (self.u.f.as_str(), self.u.l.as_str());
        let fl = format!("{f}/{l}");
        let fl2 = format!("{f}/{l}^2");
        let fl3 = format!("{f}/{l}^3");
        let fm = format!("{f}·{l}");
        let body = match action {
            Action::Axial => super::law(&s.n, [f, &fl, &fl2, &fl3], self.u.comma),
            Action::Shear => super::law(&s.v, [f, &fl, &fl2, &fl3], self.u.comma),
            Action::Moment => super::law(&s.m, [&fm, f, &fl, &fl2], self.u.comma),
        };
        format!("{}_{tag}(x) = {body}", self.letter(action))
    }

    /// `T_1a(3 m) = kN`: the law's value at `x`, worked out by calc.
    fn at(&self, tag: &str, action: Action, x: f64) -> String {
        format!("{}_{tag}({}) = {}", self.letter(action), super::at(x, &self.u.l, self.u.comma), self.unit(action))
    }

    /// Member `m`'s stretches for N: neighbours with the same law of N joined, so N's
    /// intervals end only where an axial load (or a load's axial part) acts, not at every
    /// load across the member.
    fn axial_parts(&self, m: &MemberResult, parts: &[Stretch]) -> Vec<Stretch> {
        let named = |s: &Stretch| super::named::laws(self.st, &self.us, m, s, self.u)[0].clone();
        let mut out: Vec<(Stretch, Option<String>)> = Vec::new();
        for s in parts {
            let name = named(s);
            if let Some((last, last_name)) = out.last_mut() {
                let scale = last.n.iter().chain(s.n.iter()).fold(1e-12_f64, |a, c| a.max(c.abs()));
                let xm = s.x1.abs().max(1.0);
                let same = (0..4).all(|k| {
                    let (a, b) = (last.n.get(k).copied().unwrap_or(0.0), s.n.get(k).copied().unwrap_or(0.0));
                    ((a - b) * xm.powi(k as i32)).abs() <= 1e-9 * scale * xm.powi(3)
                });
                if same && *last_name == name && (last.x1 - s.x0).abs() <= 1e-9 * (1.0 + m.length) {
                    last.x1 = s.x1;
                    continue;
                }
            }
            out.push((*s, name));
        }
        out.into_iter().map(|(s, _)| s).collect()
    }

    /// Member `m`'s laws of `action`, stretch by stretch, with their values at the stretch's ends
    /// (a constant N: its law alone).
    fn laws(&self, m: &MemberResult, parts: &[Stretch], action: Action, out: &mut Vec<Line>) {
        let Some(src) = self.st.member(&m.id) else { return };
        for (k, s) in parts.iter().enumerate() {
            if out.len() >= super::MAX_LINES {
                return;
            }
            let (tag, _) = self.names(m, k, parts.len());
            out.push(stretch_header(src, s, &tag, self.u, self.labels));
            if matches!(action, Action::Shear | Action::Moment) {
                // Where the stretch lies and the part whose equilibrium gives its laws, with T's
                // and with M's (N's stretches may join several of these: drawn with T's).
                out.push(Line::figure(Figure::Stretch(strix_structure::Cut { member: m.id.clone(), from: s.x0, to: s.x1, tag: tag.clone() })));
                out.push(Line::heading(self.labels.cut.replace("{a}", &src.a).replace("{T}", &self.shear)));
            }
            out.push(Line::eqs(vec![self.law(m, &tag, s, action)], true));
            let c = match action {
                Action::Axial => &s.n,
                Action::Shear => &s.v,
                Action::Moment => &s.m,
            };
            if action != Action::Axial || c[1..].iter().any(|c| *c != 0.0) {
                // One value a line.
                out.push(Line::eqs(vec![self.at(&tag, action, s.x0)], false));
                out.push(Line::eqs(vec![self.at(&tag, action, s.x1)], false));
            }
        }
    }

    /// Where T vanishes, stretch by stretch (solved from its law, defined when calc's solver
    /// would give another root), or changes sign; the second value is which lines are results.
    fn zeros(&self, members: &[(&MemberResult, Vec<Stretch>)], points: &[Point]) -> (Vec<Line>, Vec<bool>) {
        let mut out: Vec<(Line, bool)> = Vec::new();
        for p in points {
            match *p {
                Point::Zero { m, k, xs } => {
                    let Some((mr, parts)) = members.get(m) else { continue };
                    let (Some(src), Some(s)) = (self.st.member(&mr.id), parts.get(k)) else { continue };
                    let (tag, x) = self.names(mr, k, parts.len());
                    out.push((stretch_header(src, s, &tag, self.u, self.labels), false));
                    let found: Vec<f64> = xs.iter().flatten().copied().collect();
                    // One zero: T's law solved for it within the stretch (a law of degree two has
                    // another root outside it).
                    if found.len() == 1 {
                        let (a, b) = (super::at(s.x0, &self.u.l, self.u.comma), super::at(s.x1, &self.u.l, self.u.comma));
                        let eq = format!("0 {} = {}_{tag}({x}); {a} ≤ {x} ≤ {b}", self.u.f, self.shear);
                        out.push((Line { stepped: true, ..Line::eqs(vec![eq], true) }, true));
                    } else {
                        for (j, at) in found.iter().enumerate() {
                            let name = if found.len() > 1 { format!("x_({tag},{})", j + 1) } else { x.clone() };
                            let def = format!("{name} = {}", super::bare(&signed(*at, &self.u.l, self.u.comma)));
                            let check = format!("{}_{tag}({name}) = {}", self.shear, self.u.f);
                            out.push((Line::eqs(vec![def, check], false), true));
                        }
                    }
                }
                Point::Jump { m, k, x } => {
                    let Some((mr, parts)) = members.get(m) else { continue };
                    let Some(src) = self.st.member(&mr.id) else { continue };
                    let (before, _) = self.names(mr, k, parts.len());
                    let (after, _) = self.names(mr, k + 1, parts.len());
                    let jump = self.labels.jump.replace("{T}", &self.shear);
                    let place = format!("x = {} {}", num(x, self.u.comma), self.u.l);
                    out.push((Line::words(format!("{} {} ({}–{}), {place}: {jump}:", self.labels.member, src.id, src.a, src.b)), false));
                    out.push((Line::eqs(vec![self.at(&before, Action::Shear, x)], false), true));
                    out.push((Line::eqs(vec![self.at(&after, Action::Shear, x)], false), true));
                }
            }
        }
        if out.is_empty() {
            out.push((Line::words(self.labels.nozero.replace("{T}", &self.shear)), false));
        }
        out.truncate(super::MAX_LINES);
        out.into_iter().unzip()
    }

    /// The moment where T vanishes or changes sign (each stretch's law of M first); where it
    /// does neither, at the stretch's end where it is greatest.
    fn maxima(&self, members: &[(&MemberResult, Vec<Stretch>)], points: &[Point]) -> (Vec<Line>, Vec<bool>) {
        let mut out: Vec<(Line, bool)> = Vec::new();
        let mut defined: Vec<(usize, usize)> = Vec::new();
        let mut define = |m: usize, k: usize, out: &mut Vec<(Line, bool)>| -> Option<String> {
            let (mr, parts) = members.get(m)?;
            let (src, s) = (self.st.member(&mr.id)?, parts.get(k)?);
            let (tag, _) = self.names(mr, k, parts.len());
            if !defined.contains(&(m, k)) {
                defined.push((m, k));
                out.push((stretch_header(src, s, &tag, self.u, self.labels), false));
                out.push((Line::eqs(vec![self.law(mr, &tag, s, Action::Moment)], true), false));
            }
            Some(tag)
        };
        let unit = self.unit(Action::Moment);
        for p in points {
            match *p {
                Point::Zero { m, k, xs } => {
                    let Some(tag) = define(m, k, &mut out) else { continue };
                    let count = members.get(m).map_or(1, |(_, parts)| parts.len());
                    let (_, x) = members.get(m).map(|(mr, _)| self.names(mr, k, count)).unwrap_or_default();
                    let found: Vec<f64> = xs.iter().flatten().copied().collect();
                    let names: Vec<String> = if found.len() > 1 { (1..=found.len()).map(|j| format!("x_({tag},{j})")).collect() } else { vec![x] };
                    let eqs = names.iter().map(|n| format!("M_{tag}({n}) = {unit}")).collect();
                    out.push((Line::eqs(eqs, false), true));
                }
                Point::Jump { m, k, x } => {
                    let Some(tag) = define(m, k, &mut out) else { continue };
                    out.push((Line::eqs(vec![self.at(&tag, Action::Moment, x)], false), true));
                }
            }
        }
        if points.is_empty() {
            // The end of a stretch with the greatest |M|.
            let mut best: Option<(usize, usize, f64, f64)> = None;
            for (mi, (_, parts)) in members.iter().enumerate() {
                for (k, s) in parts.iter().enumerate() {
                    for x in [s.x0, s.x1] {
                        let v = value(&s.m, x).abs();
                        if best.is_none_or(|b| v > b.3 + 1e-9 * v.max(1.0)) {
                            best = Some((mi, k, x, v));
                        }
                    }
                }
            }
            if let Some((m, k, x, _)) = best
                && let Some(tag) = define(m, k, &mut out)
            {
                out.push((Line::eqs(vec![self.at(&tag, Action::Moment, x)], false), true));
            }
        }
        out.truncate(super::MAX_LINES);
        out.into_iter().unzip()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_law_s_value() {
        assert_eq!(value(&[1.0, 2.0, 3.0, 0.0], 2.0), 17.0);
    }
}
