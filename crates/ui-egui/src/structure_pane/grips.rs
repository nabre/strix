//! The sheet's grips: small handles on what is selected that edit it by dragging, as in a CAD
//! program. A node's (at a selected member's ends too, with one node or member selected) moves
//! that node alone and, dropped on another node, joins the two; a point load's point carries it
//! onto any node or member; a force's tail turns it (by 15° steps while snapping, Shift the other
//! way); a distributed load's ends set where it starts and stops along its member. Also here: the
//! selection window that takes what it crosses (dragged right to left), the tag naming what the
//! pointer is over, and the selection of a whole connected part (a double click).

use egui::{Color32, CursorIcon, FontId, Pos2, Rect, Stroke, pos2, vec2};
use serde_json::json;
use strix_structure::{Load, Num, Structure};

use super::edits::{self, Place};
use super::{PICK, Sel, State, View, WordApp, load_shape, moved, pick_member, round, round_at, short, ts};

/// Half a grip's side (px).
const HALF: f32 = 4.0;
/// How near (px) the pointer takes a grip.
const HIT: f32 = 7.0;
/// No grips beyond this many things selected (they would hide the drawing).
const MAX_SEL: usize = 40;

/// A grip, by what it edits.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Grip {
    /// A node, moved alone.
    Node(String),
    /// A point load's point (a force or a couple), by the load's index.
    Point(usize),
    /// A force's tail.
    Tail(usize),
    /// Where a distributed load starts.
    From(usize),
    /// Where a distributed load stops.
    To(usize),
}

/// Where a grip dragged to a point puts what it edits.
#[derive(Clone, Debug, PartialEq)]
enum Target {
    /// A node at this point.
    At(f64, f64),
    /// A node joined onto this one.
    Onto(String),
    /// A point load placed here.
    Place(Place),
    /// A force turned to this angle.
    Angle(f64),
    /// A distributed load spread over this stretch of its member.
    Spread(f64, f64),
    /// Nothing changes.
    Stay,
}

/// The grips of the selection on structure `st`: where each is drawn.
pub(super) fn grips(st: &Structure, v: View, sel: &[Sel]) -> Vec<(Grip, Pos2)> {
    let mut out: Vec<(Grip, Pos2)> = Vec::new();
    if sel.len() > MAX_SEL {
        return out;
    }
    let node = |out: &mut Vec<(Grip, Pos2)>, id: &str| {
        if let Some(n) = st.node(id)
            && !out.iter().any(|(g, _)| *g == Grip::Node(id.to_string()))
        {
            out.push((Grip::Node(id.to_string()), v.to_screen(n.x.value, n.y.value)));
        }
    };
    // Node grips with one node or member selected: with several, dragging one moves them all.
    let one = sel.iter().filter(|s| matches!(s, Sel::Node(_) | Sel::Member(_))).count() <= 1;
    for s in sel {
        match s {
            Sel::Node(_) | Sel::Member(_) if !one => {}
            Sel::Node(id) => node(&mut out, id),
            Sel::Member(id) => {
                if let Some(m) = st.member(id) {
                    node(&mut out, &m.a);
                    node(&mut out, &m.b);
                }
            }
            Sel::Load(k) => match (st.loads.get(*k), load_shape(st, v, *k).as_deref()) {
                (Some(Load::Force { .. }), Some([p, tail])) => {
                    out.push((Grip::Point(*k), *p));
                    out.push((Grip::Tail(*k), *tail));
                }
                (Some(Load::Moment { .. }), Some([p])) => out.push((Grip::Point(*k), *p)),
                (Some(Load::Distributed { .. }), Some([p0, p1, _, _])) => {
                    out.push((Grip::From(*k), *p0));
                    out.push((Grip::To(*k), *p1));
                }
                _ => {}
            },
        }
    }
    out
}

/// The grip under `p`, the nearest.
pub(super) fn hit(grips: &[(Grip, Pos2)], p: Pos2) -> Option<Grip> {
    grips.iter().map(|(g, at)| (g, (*at - p).length())).filter(|(_, d)| *d <= HIT).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(g, _)| g.clone())
}

/// Where point load `k` acts (world).
fn load_point(st: &Structure, k: usize) -> Option<(f64, f64)> {
    match st.loads.get(k)? {
        Load::Force { node, member, at, .. } | Load::Moment { node, member, at, .. } => match (member, node) {
            (Some(m), _) => edits::point_on(st, m, at.value),
            (None, Some(n)) => st.node(n).map(|n| (n.x.value, n.y.value)),
            (None, None) => None,
        },
        Load::Distributed { .. } => None,
    }
}

/// Where along member `id` (from its `a` end, within it) the world point `w` falls.
fn along_member(st: &Structure, id: &str, w: (f64, f64)) -> Option<(f64, f64)> {
    let m = st.member(id)?;
    let (a, b) = (st.node(&m.a)?, st.node(&m.b)?);
    let len = st.length(m).filter(|l| *l > 0.0)?;
    let (ux, uy) = ((b.x.value - a.x.value) / len, (b.y.value - a.y.value) / len);
    let s = (w.0 - a.x.value) * ux + (w.1 - a.y.value) * uy;
    s.is_finite().then_some((s.clamp(0.0, len), len))
}

/// What grip `g` dragged to `p` does. `shift`: a node keeps its row or column, a force turns
/// freely while snapping (by 15° steps when not).
fn target(st: &Structure, v: View, state: &State, g: &Grip, p: Pos2, shift: bool) -> Target {
    let w = v.to_world(p);
    let grid = state.snap && state.grid > 0.0;
    match g {
        Grip::Node(id) => {
            let Some(n) = st.node(id) else { return Target::Stay };
            // Onto another node within reach.
            if let Some(o) = st
                .nodes
                .iter()
                .filter(|o| o.id != *id)
                .map(|o| (o, (v.to_screen(o.x.value, o.y.value) - p).length()))
                .filter(|(_, d)| *d <= PICK)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(o, _)| o)
            {
                return Target::Onto(o.id.clone());
            }
            let on = |x: f64| if grid { (x / state.grid).round() * state.grid } else { x };
            let (mut x, mut y) = (round(on(w.0)), round(on(w.1)));
            if shift {
                if (x - n.x.value).abs() >= (y - n.y.value).abs() {
                    y = n.y.value;
                } else {
                    x = n.x.value;
                }
            }
            if (x - n.x.value).abs() <= 1e-9 && (y - n.y.value).abs() <= 1e-9 { Target::Stay } else { Target::At(x, y) }
        }
        Grip::Point(_) => {
            if let Some(id) = super::pick_node(st, v, p) {
                return Target::Place(Place::Node(id));
            }
            match pick_member(st, v, p) {
                Some((id, at)) => {
                    let len = st.member(&id).and_then(|m| st.length(m)).unwrap_or(0.0);
                    Target::Place(Place::Member(id, round(round_at(at, len, state))))
                }
                None => Target::Stay,
            }
        }
        Grip::Tail(k) => {
            let (Some(pt), Some(Load::Force { value, .. })) = (load_point(st, *k), st.loads.get(*k)) else { return Target::Stay };
            let step = if state.snap != shift { 15.0 } else { 1.0 };
            edits::force_angle(pt.0 - w.0, pt.1 - w.1, value.value, step).map_or(Target::Stay, Target::Angle)
        }
        Grip::From(k) | Grip::To(k) => {
            let Some(Load::Distributed { member, from, to, .. }) = st.loads.get(*k) else { return Target::Stay };
            let Some((s, len)) = along_member(st, member, w) else { return Target::Stay };
            let s = round(round_at(s, len, state));
            let (f, t) = (from.value, to.as_ref().map_or(len, |t| t.value));
            if matches!(g, Grip::From(_)) { Target::Spread(s, t) } else { Target::Spread(f, s) }
        }
    }
}

/// The shortest a distributed load is made by its grips.
fn gap(state: &State) -> f64 {
    if state.snap && state.grid > 0.0 { state.grid } else { 0.01 }
}

/// `st` as it would be with grip `g` dropped at `p` (resolved values: the sheet's preview).
pub(super) fn preview(st: &Structure, v: View, state: &State, g: &Grip, p: Pos2, shift: bool) -> Structure {
    let mut out = st.clone();
    match (g, target(st, v, state, g, p, shift)) {
        (Grip::Node(id), Target::At(x, y)) => {
            if let Some(n) = out.nodes.iter_mut().find(|n| n.id == *id) {
                n.x = Num::new(x);
                n.y = Num::new(y);
            }
        }
        (Grip::Node(id), Target::Onto(o)) => {
            if let Some((x, y)) = st.node(&o).map(|o| (o.x.value, o.y.value))
                && let Some(n) = out.nodes.iter_mut().find(|n| n.id == *id)
            {
                n.x = Num::new(x);
                n.y = Num::new(y);
            }
        }
        (Grip::Point(k), Target::Place(place)) => {
            if let Some(l) = edits::placed(st, st, *k, &place)
                && let Some(slot) = out.loads.get_mut(*k)
            {
                *slot = l;
            }
        }
        (Grip::Tail(k), Target::Angle(a)) => {
            if let Some(Load::Force { angle, .. }) = out.loads.get_mut(*k) {
                *angle = Num::new(a);
            }
        }
        (Grip::From(k) | Grip::To(k), Target::Spread(f, t)) => {
            if let Some(l) = edits::spread(st, st, *k, f, t, gap(state))
                && let Some(slot) = out.loads.get_mut(*k)
            {
                *slot = l;
            }
        }
        _ => {}
    }
    out
}

/// Drops grip `g` at `p`: one undoable change of the structure, nothing when it stayed.
#[allow(clippy::too_many_arguments)]
pub(super) fn commit(app: &mut WordApp, src: &Structure, st: &Structure, v: View, state: &mut State, g: &Grip, p: Pos2, shift: bool) {
    match (g, target(st, v, state, g, p, shift)) {
        (Grip::Node(id), Target::At(x, y)) => {
            if let Some(n) = st.node(id) {
                let out = moved(src, std::slice::from_ref(id), x - n.x.value, y - n.y.value);
                let _ = app.run("structure.set", json!({"structure": out}));
            }
        }
        (Grip::Node(id), Target::Onto(o)) => {
            if let Some(out) = edits::merge_nodes(src, id, &o)
                && let Ok(j) = serde_json::to_value(&out)
                && app.run("structure.set", json!({"structure": j})).is_ok()
            {
                state.sel.retain(|s| *s != Sel::Node(id.clone()));
                if !state.has(&Sel::Node(o.clone())) {
                    state.sel.push(Sel::Node(o));
                }
            }
        }
        (Grip::Point(k), Target::Place(place)) => run_load(app, *k, edits::placed(src, st, *k, &place)),
        (Grip::Tail(k), Target::Angle(a)) => {
            let changed = matches!(st.loads.get(*k), Some(Load::Force { angle, .. }) if (angle.value - a).abs() > 1e-9);
            if changed && let Some(mut l) = src.loads.get(*k).cloned() {
                if let Load::Force { angle, .. } = &mut l {
                    *angle = Num::new(a);
                }
                run_load(app, *k, Some(l));
            }
        }
        (Grip::From(k) | Grip::To(k), Target::Spread(f, t)) => run_load(app, *k, edits::spread(src, st, *k, f, t, gap(state))),
        _ => {}
    }
}

/// Replaces load `k` with `l` (when there is one).
pub(super) fn run_load(app: &mut WordApp, k: usize, l: Option<Load>) {
    if let Some(l) = l
        && let Ok(lj) = serde_json::to_value(&l)
    {
        let _ = app.run("structure.load", json!({"index": k, "load": lj}));
    }
}

/// What the drag of grip `g` to `p` reads out under the sheet.
pub(super) fn readout(st: &Structure, v: View, state: &State, g: &Grip, p: Pos2, shift: bool) -> String {
    let unit = st.length_unit.symbol();
    match target(st, v, state, g, p, shift) {
        Target::At(x, y) => format!("x = {} {unit}   y = {} {unit}", short(x), short(y)),
        Target::Onto(o) => crate::i18n::fmt(ts("Join onto node {n}"), &[("n", &o)]),
        Target::Place(Place::Node(n)) => crate::i18n::fmt(ts("At node {n}"), &[("n", &n)]),
        Target::Place(Place::Member(m, at)) => {
            format!("{}   {} = {} {unit}", crate::i18n::fmt(ts("Member {n}"), &[("n", &m)]), ts("at"), short(at))
        }
        Target::Angle(a) => format!("α = {}°", short(a)),
        Target::Spread(f, t) => format!("{} {} → {} {unit}", ts("from"), short(f), short(t)),
        Target::Stay => String::new(),
    }
}

/// Paints the grips: the one under the pointer lighter, the one dragged red, and the node a node
/// grip would join (a ring).
pub(super) fn paint(
    painter: &egui::Painter,
    st: &Structure,
    v: View,
    state: &State,
    grips: &[(Grip, Pos2)],
    hover: Option<Pos2>,
    dragged: Option<(&Grip, Pos2, bool)>,
) {
    let under = hover.and_then(|h| hit(grips, h));
    for (g, at) in grips {
        let hot = dragged.is_some_and(|(d, _, _)| d == g);
        let fill = if hot {
            Color32::from_rgb(0xE1, 0x1D, 0x48)
        } else if under.as_ref() == Some(g) {
            Color32::from_rgb(0x2D, 0xD4, 0xBF)
        } else {
            Color32::from_rgb(0x0F, 0x76, 0x6E)
        };
        match g {
            // A force's tail turns it: a round grip.
            Grip::Tail(_) => {
                painter.circle_filled(*at, HALF + 0.5, fill);
                painter.circle_stroke(*at, HALF + 0.5, Stroke::new(1.0, Color32::WHITE));
            }
            // A distributed load's ends slide along the member: diamonds.
            Grip::From(_) | Grip::To(_) => {
                let d = HALF + 1.0;
                let pts = vec![*at + vec2(0.0, -d), *at + vec2(d, 0.0), *at + vec2(0.0, d), *at + vec2(-d, 0.0)];
                painter.add(egui::Shape::convex_polygon(pts, fill, Stroke::new(1.0, Color32::WHITE)));
            }
            Grip::Node(_) | Grip::Point(_) => {
                let r = Rect::from_center_size(*at, vec2(2.0 * HALF, 2.0 * HALF));
                painter.rect_filled(r, 0.0, fill);
                painter.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::WHITE), egui::StrokeKind::Outside);
            }
        }
    }
    if let Some((g, p, shift)) = dragged {
        let ring = |painter: &egui::Painter, at: Pos2| painter.circle_stroke(at, 9.0, Stroke::new(2.0, Color32::from_rgb(0xE1, 0x1D, 0x48)));
        match target(st, v, state, g, p, shift) {
            Target::Onto(o) | Target::Place(Place::Node(o)) => {
                if let Some(n) = st.node(&o) {
                    ring(painter, v.to_screen(n.x.value, n.y.value));
                }
            }
            Target::Place(Place::Member(m, at)) => {
                if let Some((x, y)) = edits::point_on(st, &m, at) {
                    ring(painter, v.to_screen(x, y));
                }
            }
            _ => {}
        }
    }
}

/// The pointer's shape over the sheet: a hand over a grip, the drag under way's.
pub(super) fn cursor(ctx: &egui::Context, over_grip: bool, dragging: bool) {
    if dragging {
        ctx.set_cursor_icon(CursorIcon::Grabbing);
    } else if over_grip {
        ctx.set_cursor_icon(CursorIcon::Grab);
    }
}

/// What a selection window dragged right to left takes: anything it touches (a member crossing
/// it, a load with a point in it), besides what it holds.
pub(super) fn crossing(st: &Structure, v: View, r: Rect) -> Vec<Sel> {
    let mut out: Vec<Sel> = st.nodes.iter().filter(|n| r.contains(v.to_screen(n.x.value, n.y.value))).map(|n| Sel::Node(n.id.clone())).collect();
    for m in &st.members {
        if let (Some(a), Some(b)) = (st.node(&m.a), st.node(&m.b))
            && seg_meets_rect(v.to_screen(a.x.value, a.y.value), v.to_screen(b.x.value, b.y.value), r)
        {
            out.push(Sel::Member(m.id.clone()));
        }
    }
    for k in 0..st.loads.len() {
        if let Some(pts) = load_shape(st, v, k) {
            let touches = match pts.as_slice() {
                [a, b] => seg_meets_rect(*a, *b, r),
                [c] => r.expand(4.0 * super::MM).contains(*c),
                ps => ps.iter().any(|p| r.contains(*p)) || ps.windows(2).any(|w| matches!(w, [a, b] if seg_meets_rect(*a, *b, r))),
            };
            if touches {
                out.push(Sel::Load(k));
            }
        }
    }
    out
}

/// Whether segment `a`–`b` has a point in `r`.
fn seg_meets_rect(a: Pos2, b: Pos2, r: Rect) -> bool {
    if r.contains(a) || r.contains(b) {
        return true;
    }
    let corners = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()];
    (0..4).any(|i| {
        let (c, d) = (corners[i], corners[(i + 1) % 4]);
        segments_cross(a, b, c, d)
    })
}

fn segments_cross(a: Pos2, b: Pos2, c: Pos2, d: Pos2) -> bool {
    let cross = |o: Pos2, p: Pos2, q: Pos2| (p.x - o.x) * (q.y - o.y) - (p.y - o.y) * (q.x - o.x);
    let (d1, d2) = (cross(c, d, a), cross(c, d, b));
    let (d3, d4) = (cross(a, b, c), cross(a, b, d));
    (d1 * d2 < 0.0) && (d3 * d4 < 0.0)
}

/// Paints a selection window: blue and solid when it takes what it holds (dragged left to
/// right), green and dashed when it takes what it crosses.
pub(super) fn paint_window(painter: &egui::Painter, from: Pos2, to: Pos2) {
    let r = Rect::from_two_pos(from, to);
    if to.x < from.x {
        let c = Color32::from_rgb(0x16, 0xA3, 0x4A);
        painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(0x16, 0xA3, 0x4A, 24));
        let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
        painter.extend(egui::Shape::dashed_line(&pts, Stroke::new(1.0, c), 5.0, 3.0));
    } else {
        painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(0x25, 0x63, 0xEB, 24));
        painter.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::from_rgb(0x25, 0x63, 0xEB)), egui::StrokeKind::Middle);
    }
}

/// The name of what `s` is, with its main figure: `Node A (0; 3 m)`, `Member 1 · 6 m`,
/// `Force · 10 kN`.
pub(super) fn tag(st: &Structure, s: &Sel) -> Option<String> {
    let (lu, fu) = (st.length_unit.symbol(), st.force_unit.symbol());
    let fmt = crate::i18n::fmt;
    Some(match s {
        Sel::Node(id) => {
            let n = st.node(id)?;
            format!("{} ({}; {} {lu})", fmt(ts("Node {n}"), &[("n", id)]), short(n.x.value), short(n.y.value))
        }
        Sel::Member(id) => {
            let m = st.member(id)?;
            format!("{} · {} {lu}", fmt(ts("Member {n}"), &[("n", id)]), short(st.length(m)?))
        }
        Sel::Load(k) => match st.loads.get(*k)? {
            Load::Force { value, angle, .. } => format!("{} · {} {fu} · {}°", ts("Force"), short(value.value), short(angle.value)),
            Load::Moment { value, .. } => format!("{} · {} {fu}·{lu}", ts("Couple"), short(value.value)),
            Load::Distributed { q1, q2, .. } => match q2 {
                Some(q2) => format!("{} · {} → {} {fu}/{lu}", ts("Distributed load"), short(q1.value), short(q2.value)),
                None => format!("{} · {} {fu}/{lu}", ts("Distributed load"), short(q1.value)),
            },
        },
    })
}

/// Paints `text` in a small box by the pointer at `p`, within `clip`.
pub(super) fn paint_tag(painter: &egui::Painter, clip: Rect, p: Pos2, text: &str) {
    let font = FontId::proportional(11.0);
    let galley = painter.layout_no_wrap(text.to_string(), font, Color32::from_gray(0x30));
    let size = galley.size() + vec2(8.0, 4.0);
    let mut at = p + vec2(14.0, 16.0);
    at.x = at.x.min(clip.right() - size.x - 2.0).max(clip.left() + 2.0);
    at.y = at.y.min(clip.bottom() - size.y - 22.0).max(clip.top() + 2.0);
    let r = Rect::from_min_size(at, size);
    painter.rect_filled(r, 3.0, Color32::from_rgba_unmultiplied(0xFF, 0xFB, 0xEB, 235));
    painter.rect_stroke(r, 3.0, Stroke::new(1.0, Color32::from_gray(0xC8)), egui::StrokeKind::Inside);
    painter.galley(pos2(r.min.x + 4.0, r.min.y + 2.0), galley, Color32::from_gray(0x30));
}

/// The whole connected part `hit` is in, selected: its nodes, members and their loads.
pub(super) fn select_connected(st: &Structure, state: &mut State, hit: &Sel) {
    let start: Vec<String> = match hit {
        Sel::Node(id) => vec![id.clone()],
        Sel::Member(id) => st.member(id).map(|m| vec![m.a.clone(), m.b.clone()]).unwrap_or_default(),
        Sel::Load(k) => match st.loads.get(*k) {
            Some(Load::Force { node: Some(n), member: None, .. } | Load::Moment { node: Some(n), member: None, .. }) => vec![n.clone()],
            Some(Load::Force { member: Some(m), .. } | Load::Moment { member: Some(m), .. } | Load::Distributed { member: m, .. }) => {
                st.member(m).map(|m| vec![m.a.clone(), m.b.clone()]).unwrap_or_default()
            }
            _ => Vec::new(),
        },
    };
    if start.is_empty() {
        return;
    }
    let (nodes, members) = edits::connected(st, &start);
    state.sel = nodes.iter().map(|n| Sel::Node(n.clone())).chain(members.iter().map(|m| Sel::Member(m.clone()))).collect();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crossing_window_takes_what_it_touches() {
        let a = pos2(0.0, 0.0);
        let b = pos2(100.0, 0.0);
        assert!(seg_meets_rect(a, b, Rect::from_min_max(pos2(40.0, -5.0), pos2(60.0, 5.0))), "crossed through");
        assert!(!seg_meets_rect(a, b, Rect::from_min_max(pos2(40.0, 5.0), pos2(60.0, 15.0))), "beside it");
        assert!(seg_meets_rect(a, b, Rect::from_min_max(pos2(90.0, -5.0), pos2(120.0, 5.0))), "an end in it");
    }
}
