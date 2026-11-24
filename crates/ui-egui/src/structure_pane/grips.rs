//! The sheet's grips: small handles on what is selected that edit it by dragging, as in a CAD
//! program. A node's (at a selected member's ends too, with one node or member selected) moves
//! that node alone and, dropped on another node, joins the two; a point load's point carries it
//! onto any node or member; a force's tail, a distributed load's envelope midpoint and a support's
//! ground handle turn them with a graphical protractor (by 15° steps and member ∥/⊥ snaps while
//! snapping, Shift the other way); a distributed load's ends set where it starts and stops, and
//! its body slides it, along its straight run of aligned members (past a node onto the next one). Also here: the crossing window, the hover/erase tags, member local-axis cues, and
//! connected-part selection.

use egui::{Color32, CursorIcon, FontId, Pos2, Rect, Shape, Stroke, pos2, vec2};
use serde_json::json;
use strix_structure::{Load, Num, Structure};

use super::edits::{self, Place};
use super::rotate::{self, RotTarget, SnapRef};
use super::{EraseTarget, MM, PICK, Sel, State, View, WordApp, kind_label, load_shape, moved, pick_member, round, round_at, short, ts};

/// Half a grip's side (px).
const HALF: f32 = 4.0;
/// How near (px) the pointer takes a grip.
const HIT: f32 = 8.0;
/// No grips beyond this many things selected (they would hide the drawing).
const MAX_SEL: usize = 40;

/// A grip, by what it edits.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Grip {
    /// A node, moved alone.
    Node(String),
    /// A point load's point (a force or a couple), by the load's index.
    Point(usize),
    /// A force's tail (rotates its inclination).
    Tail(usize),
    /// A distributed load's envelope midpoint (rotates its inclination).
    DistAngle(usize),
    /// A support's ground handle on this node (rotates the support).
    SupportAngle(String),
    /// Where a distributed load starts.
    From(usize),
    /// Where a distributed load stops.
    To(usize),
    /// A distributed load's body, slid along its run of aligned members: the load's index and
    /// how far from its start (along the run) it was grabbed.
    Body(usize, f64),
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
    /// A vector or support turned to `(stored_deg, visual_deg, snap_ref)`.
    Turn(RotTarget, f64, f64, Option<SnapRef>),
    /// A distributed load spread over this stretch of its run of aligned members (from, to,
    /// along the run).
    Spread(f64, f64),
    /// Nothing changes.
    Stay,
}

/// Screen position of the ground rotation handle of node `id`'s support.
pub(super) fn support_handle_pos(st: &Structure, v: View, id: &str) -> Option<Pos2> {
    let n = st.node(id)?;
    let s = st.supports.iter().find(|s| s.node == id)?;
    let p = v.to_screen(n.x.value, n.y.value);
    let phi = s.angle.value.to_radians();
    // Along the ground normal away from the node (`(sin φ, cos φ)` in screen coords).
    let d = vec2(phi.sin() as f32, phi.cos() as f32);
    Some(p + d * 5.8 * MM)
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
            Sel::Node(id) => {
                node(&mut out, id);
                if let Some(sp) = support_handle_pos(st, v, id) {
                    out.push((Grip::SupportAngle(id.clone()), sp));
                }
            }
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
                (Some(Load::Distributed { .. }), Some([p0, p1, t1, t0])) => {
                    out.push((Grip::From(*k), *p0));
                    out.push((Grip::To(*k), *p1));
                    let mid_tail = pos2(0.5 * (t0.x + t1.x), 0.5 * (t0.y + t1.y));
                    out.push((Grip::DistAngle(*k), mid_tail));
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

/// A position `s` along a run of members rounded to the grid (when snapping) from the start of
/// the member it falls on, within the run.
fn round_on_run(run: &strix_structure::SpreadLine, s: f64, state: &State) -> f64 {
    let s = s.clamp(0.0, run.len);
    let eps = 1e-9 * (1.0 + run.len);
    match run.parts.iter().find(|p| s <= p.at + p.len + eps) {
        Some(p) => round(p.at + round_at(s - p.at, p.len, state)),
        None => round(s),
    }
}

/// The body grip of distributed load `k` grabbed at screen point `p`: how far from the load's
/// start along its run.
pub(super) fn body(st: &Structure, v: View, k: usize, p: Pos2) -> Option<Grip> {
    let (run, f, _) = edits::on_run(st, k)?;
    Some(Grip::Body(k, run.along(v.to_world(p)) - f))
}

/// What grip `g` dragged to `p` does. `shift`: a node keeps its row or column, a rotation turns
/// freely while snapping (by 15° + member ∥/⊥ steps when not).
fn target(st: &Structure, v: View, state: &State, g: &Grip, p: Pos2, shift: bool) -> Target {
    let w = v.to_world(p);
    let grid = state.snap && state.grid > 0.0;
    let snap_rot = state.snap != shift;
    match g {
        Grip::Node(id) => {
            let Some(n) = st.node(id) else { return Target::Stay };
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
            let rt = RotTarget::Force(*k);
            let Some((px, py)) = rotate::pivot(st, &rt) else { return Target::Stay };
            // Dragging the tail puts the tail at `w`, so the force pushes towards `(px - w.0, py - w.1)`.
            let (dx, dy) = (px - w.0, py - w.1);
            if !(dx.is_finite() && dy.is_finite()) || dx.hypot(dy) <= 1e-9 {
                return Target::Stay;
            }
            let raw_vis = rotate::norm_deg(dy.atan2(dx).to_degrees());
            let (vis, sref) = rotate::snap_visual_angle(st, &rt, raw_vis, snap_rot);
            let Some(stored) = rotate::stored_angle_for_visual(st, &rt, vis) else { return Target::Stay };
            Target::Turn(rt, stored, vis, sref)
        }
        Grip::DistAngle(k) => {
            let rt = RotTarget::Distributed(*k);
            let Some((px, py)) = rotate::pivot(st, &rt) else { return Target::Stay };
            // Dragging the envelope midpoint puts the tails at `w`, so arrows push towards `(px - w.0, py - w.1)`.
            let (dx, dy) = (px - w.0, py - w.1);
            if !(dx.is_finite() && dy.is_finite()) || dx.hypot(dy) <= 1e-9 {
                return Target::Stay;
            }
            let raw_vis = rotate::norm_deg(dy.atan2(dx).to_degrees());
            let (vis, sref) = rotate::snap_visual_angle(st, &rt, raw_vis, snap_rot);
            let Some(stored) = rotate::stored_angle_for_visual(st, &rt, vis) else { return Target::Stay };
            Target::Turn(rt, stored, vis, sref)
        }
        Grip::SupportAngle(id) => {
            let rt = RotTarget::Support(id.clone());
            let Some((px, py)) = rotate::pivot(st, &rt) else { return Target::Stay };
            // Dragging the ground handle puts the ground at `(w.0 - px, w.1 - py)`.
            let (dx, dy) = (w.0 - px, w.1 - py);
            if !(dx.is_finite() && dy.is_finite()) || dx.hypot(dy) <= 1e-9 {
                return Target::Stay;
            }
            let raw_vis = rotate::norm_deg(dy.atan2(dx).to_degrees());
            let (vis, sref) = rotate::snap_visual_angle(st, &rt, raw_vis, snap_rot);
            let Some(stored) = rotate::stored_angle_for_visual(st, &rt, vis) else { return Target::Stay };
            Target::Turn(rt, stored, vis, sref)
        }
        Grip::From(k) | Grip::To(k) => {
            // Along the run: past a node, onto the aligned member that goes on from it.
            let Some((run, f, t)) = edits::on_run(st, *k) else { return Target::Stay };
            let s = round_on_run(&run, run.along(w), state);
            if matches!(g, Grip::From(_)) { Target::Spread(s.min(t), t) } else { Target::Spread(f, s.max(f)) }
        }
        Grip::Body(k, grab) => {
            let Some((run, f, t)) = edits::on_run(st, *k) else { return Target::Stay };
            let span = t - f;
            let start = (run.along(w) - grab).clamp(0.0, (run.len - span).max(0.0));
            let start = round_on_run(&run, start, state).clamp(0.0, (run.len - span).max(0.0));
            if (start - f).abs() <= 1e-9 { Target::Stay } else { Target::Spread(start, start + span) }
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
        (_, Target::Turn(rt, stored, _, _)) => {
            return rotate::preview_target(st, &rt, stored);
        }
        (Grip::From(k) | Grip::To(k) | Grip::Body(k, _), Target::Spread(f, t)) => {
            if let Some(l) = edits::respread(st, st, *k, f, t, gap(state))
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
        (_, Target::Turn(rt, stored, _, _)) => {
            rotate::commit_target(app, src, st, &rt, stored, true);
        }
        (Grip::From(k) | Grip::To(k) | Grip::Body(k, _), Target::Spread(f, t)) => run_load(app, *k, edits::respread(src, st, *k, f, t, gap(state))),
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
    let unit = ts(st.length_unit.symbol());
    match target(st, v, state, g, p, shift) {
        Target::At(x, y) => format!("x = {} {unit}   y = {} {unit}", short(x), short(y)),
        Target::Onto(o) => crate::i18n::fmt(ts("Join onto node {n}"), &[("n", &o)]),
        Target::Place(Place::Node(n)) => crate::i18n::fmt(ts("At node {n}"), &[("n", &n)]),
        Target::Place(Place::Member(m, at)) => {
            format!("{}   {} = {} {unit}", crate::i18n::fmt(ts("Member {n}"), &[("n", &m)]), ts("at"), short(at))
        }
        Target::Turn(rt, _, vis, sref) => rotate::readout_for(st, &rt, vis, sref.as_ref()),
        Target::Spread(f, t) => match g {
            Grip::From(k) | Grip::To(k) | Grip::Body(k, _) => spread_readout(st, *k, f, t, unit),
            _ => String::new(),
        },
        Target::Stay => String::new(),
    }
}

/// What a distributed load laid from `f` to `t` along its run reads out: the members it goes
/// over, and where it starts and stops on the first and the last.
fn spread_readout(st: &Structure, k: usize, f: f64, t: f64, unit: &str) -> String {
    let Some((run, _)) = edits::run_of(st, k) else { return String::new() };
    let eps = 1e-9 * (1.0 + run.len);
    let ids: Vec<&str> = run.parts.iter().filter(|p| t.min(p.at + p.len) - f.max(p.at) > eps).map(|p| p.member.as_str()).collect();
    let base = run.parts.iter().find(|p| ids.first() == Some(&p.member.as_str())).map_or(0.0, |p| p.at);
    let members = if ids.len() > 1 {
        crate::i18n::fmt(ts("Members {n}"), &[("n", &ids.join(" + "))])
    } else {
        crate::i18n::fmt(ts("Member {n}"), &[("n", &ids.join(""))])
    };
    format!("{members}   {} {} → {} {unit}", ts("from"), short(f - base), short(t - base))
}

/// Draws a local-axis arrow (`a → b`) and dashed reference-fibre cue on a selected single member.
pub(super) fn paint_member_cue(painter: &egui::Painter, st: &Structure, v: View, id: &str) {
    let Some(m) = st.member(id) else { return };
    let (Some(a), Some(b)) = (st.node(&m.a), st.node(&m.b)) else { return };
    let (pa, pb) = (v.to_screen(a.x.value, a.y.value), v.to_screen(b.x.value, b.y.value));
    let d = pb - pa;
    let len = d.length();
    if len < 36.0 {
        return;
    }
    let u = d / len;
    // Reference fibres sit on the member's −y side (right-hand side from `a` to `b` in world,
    // which in screen y-down coordinates is `(-u.y, u.x)`).
    let ref_side = vec2(-u.y, u.x);
    let mid = pos2(0.5 * (pa.x + pb.x), 0.5 * (pa.y + pb.y));
    let teal = Color32::from_rgb(0x0F, 0x76, 0x6E);
    // Small direction chevron along `a → b` slightly above the member.
    let c = mid - ref_side * 8.0;
    let tip = c + u * 8.0;
    let tail = c - u * 8.0;
    painter.line_segment([tail, tip], Stroke::new(1.3, teal));
    painter.add(Shape::convex_polygon(vec![tip, tip - u * 4.5 + ref_side * 2.6, tip - u * 4.5 - ref_side * 2.6], teal, Stroke::NONE));
    // Short dashed reference-fibre line on the −y side.
    let rf0 = mid + ref_side * 6.5 - u * 12.0;
    let rf1 = mid + ref_side * 6.5 + u * 12.0;
    painter.extend(Shape::dashed_line(&[rf0, rf1], Stroke::new(1.0, teal.linear_multiply(0.75)), 3.5, 2.5));
}

/// Paints the grips: the one under the pointer lighter, the one dragged red, the protractor while
/// a rotation grip is dragged, and the node/point a grip would join or land on.
pub(super) fn paint(
    painter: &egui::Painter,
    st: &Structure,
    v: View,
    state: &State,
    grips: &[(Grip, Pos2)],
    hover: Option<Pos2>,
    dragged: Option<(&Grip, Pos2, bool)>,
) {
    // Draw protractor underneath the grips when dragging a rotation grip.
    if let Some((g, p, shift)) = dragged
        && let Target::Turn(rt, _, vis, sref) = target(st, v, state, g, p, shift)
    {
        let start_vis = rotate::visual_angle(st, &rt);
        rotate::paint_protractor(painter, st, v, &rt, rotate::Dial { vis_deg: vis, start_vis, snap_ref: sref.as_ref(), faint: false });
    }

    let under = hover.and_then(|h| hit(grips, h));
    for (g, at) in grips {
        let hot = dragged.is_some_and(|(d, _, _)| d == g);
        let hovered = under.as_ref() == Some(g);
        let fill = if hot {
            Color32::from_rgb(0xE1, 0x1D, 0x48)
        } else if hovered {
            Color32::from_rgb(0x2D, 0xD4, 0xBF)
        } else {
            Color32::from_rgb(0x0F, 0x76, 0x6E)
        };
        match g {
            // Rotation grips: round knob with a subtle outer rotation arc halo.
            Grip::Tail(_) | Grip::DistAngle(_) | Grip::SupportAngle(_) => {
                if hovered || hot {
                    painter.circle_stroke(*at, HALF + 4.0, Stroke::new(1.2, fill.linear_multiply(0.65)));
                } else {
                    painter.circle_stroke(*at, HALF + 2.8, Stroke::new(0.9, fill.linear_multiply(0.4)));
                }
                painter.circle_filled(*at, HALF + 0.8, fill);
                painter.circle_stroke(*at, HALF + 0.8, Stroke::new(1.1, Color32::WHITE));
            }
            // A distributed load's ends slide along the member: diamonds.
            Grip::From(_) | Grip::To(_) => {
                let d = HALF + 1.0;
                let pts = vec![*at + vec2(0.0, -d), *at + vec2(d, 0.0), *at + vec2(0.0, d), *at + vec2(-d, 0.0)];
                painter.add(egui::Shape::convex_polygon(pts, fill, Stroke::new(1.0, Color32::WHITE)));
            }
            // Taken by dragging the load itself: no handle of its own.
            Grip::Body(..) => {}
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

/// The pointer's shape over the sheet: reflects grips, active drags, and the active tool.
pub(super) fn cursor(ctx: &egui::Context, over_grip: bool, dragging: bool, tool_crosshair: bool, over_target: bool) {
    if dragging {
        ctx.set_cursor_icon(CursorIcon::Grabbing);
    } else if over_grip {
        ctx.set_cursor_icon(CursorIcon::Grab);
    } else if tool_crosshair {
        ctx.set_cursor_icon(CursorIcon::Crosshair);
    } else if over_target {
        ctx.set_cursor_icon(CursorIcon::PointingHand);
    }
}

/// What a selection window dragged right to left takes: anything it touches (a member crossing
/// it, a load or support in it), besides what it holds.
pub(super) fn crossing(st: &Structure, v: View, r: Rect) -> Vec<Sel> {
    let mut out: Vec<Sel> = st
        .nodes
        .iter()
        .filter(|n| r.contains(v.to_screen(n.x.value, n.y.value)) || super::support_rect(st, v, &n.id).is_some_and(|sr| sr.intersects(r)))
        .map(|n| Sel::Node(n.id.clone()))
        .collect();
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
                ps => {
                    ps.iter().any(|p| r.contains(*p))
                        || ps.windows(2).any(|w| matches!(w, [a, b] if seg_meets_rect(*a, *b, r)))
                        || matches!((ps.last(), ps.first()), (Some(a), Some(b)) if seg_meets_rect(*a, *b, r))
                }
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

/// The name of what `s` is, with its symbol and main figures:
/// `Node A (0; 3 m) · Pin`, `Member 1 (A → B) · 6 m`, `F₁ · Force · 10 kN · −90°`.
pub(super) fn tag(st: &Structure, s: &Sel) -> Option<String> {
    let (lu, fu) = (ts(st.length_unit.symbol()), ts(st.force_unit.symbol()));
    let fmt = crate::i18n::fmt;
    let names = strix_structure::load_names(st);
    Some(match s {
        Sel::Node(id) => {
            let n = st.node(id)?;
            let mut base = format!("{} ({}; {} {lu})", fmt(ts("Node {n}"), &[("n", id)]), short(n.x.value), short(n.y.value));
            if let Some(sup) = st.supports.iter().find(|sup| sup.node == *id) {
                base.push_str(&format!(" · {}", ts(kind_label(Some(sup.kind)))));
                if sup.angle.value.abs() > 1e-6 {
                    base.push_str(&format!(" ({}°)", short(sup.angle.value)));
                }
            }
            if n.hinge {
                base.push_str(&format!(" · {}", ts("Internal hinge")));
            }
            base
        }
        Sel::Member(id) => {
            let m = st.member(id)?;
            let slope = rotate::member_deg(st, id).unwrap_or(0.0);
            format!("{} ({} → {}) · {} {lu} · {}°", fmt(ts("Member {n}"), &[("n", id)]), m.a, m.b, short(st.length(m)?), short(slope))
        }
        Sel::Load(k) => {
            let prefix = names.get(*k).copied().flatten().map(|n| format!("{} · ", n.pretty())).unwrap_or_default();
            match st.loads.get(*k)? {
                Load::Force { value, angle, .. } => {
                    format!("{prefix}{} · {} {fu} · {}°", ts("Force"), short(value.value), short(angle.value))
                }
                Load::Moment { value, .. } => format!("{prefix}{} · {} {fu}·{lu}", ts("Couple"), short(value.value)),
                Load::Distributed { q1, q2, angle, .. } => match q2 {
                    Some(q2) => {
                        format!("{prefix}{} · {} → {} {fu}/{lu} · {}°", ts("Distributed load"), short(q1.value), short(q2.value), short(angle.value))
                    }
                    None => {
                        format!("{prefix}{} · {} {fu}/{lu} · {}°", ts("Distributed load"), short(q1.value), short(angle.value))
                    }
                },
            }
        }
    })
}

/// Warning tag shown while hovering with `Tool::Erase`, naming what will be removed.
pub(super) fn erase_tag(st: &Structure, t: &EraseTarget) -> Option<String> {
    let fmt = crate::i18n::fmt;
    match t {
        EraseTarget::Load(k) => Some(format!("{}: {}", ts("Erase"), tag(st, &Sel::Load(*k))?)),
        EraseTarget::Support(id) => {
            let sup = st.supports.iter().find(|s| s.node == *id)?;
            Some(format!("{}: {} ({})", ts("Erase"), ts(kind_label(Some(sup.kind))), fmt(ts("Node {n}"), &[("n", id)])))
        }
        EraseTarget::Node(id) => {
            let m_cnt = st.members.iter().filter(|m| m.a == *id || m.b == *id).count();
            let base = fmt(ts("Node {n}"), &[("n", id)]);
            if m_cnt > 0 { Some(format!("{}: {base} (+ {m_cnt} {})", ts("Erase"), ts("Members"))) } else { Some(format!("{}: {base}", ts("Erase"))) }
        }
        EraseTarget::Member(id) => Some(format!("{}: {}", ts("Erase"), fmt(ts("Member {n}"), &[("n", id)]))),
    }
}

/// Paints `text` in a small box by the pointer at `p`, within `clip`.
pub(super) fn paint_tag(painter: &egui::Painter, clip: Rect, p: Pos2, text: &str) {
    paint_tag_styled(
        painter,
        clip,
        p,
        text,
        Color32::from_rgba_unmultiplied(0xFF, 0xFB, 0xEB, 238),
        Color32::from_gray(0xC8),
        Color32::from_gray(0x28),
    );
}

/// Paints `text` in a small styled tag box by `p`, within `clip`.
pub(super) fn paint_tag_styled(painter: &egui::Painter, clip: Rect, p: Pos2, text: &str, bg: Color32, border: Color32, fg: Color32) {
    let font = FontId::proportional(11.0);
    let galley = painter.layout_no_wrap(text.to_string(), font, fg);
    let size = galley.size() + vec2(10.0, 5.0);
    let mut at = p + vec2(14.0, 16.0);
    at.x = at.x.min(clip.right() - size.x - 2.0).max(clip.left() + 2.0);
    at.y = at.y.min(clip.bottom() - size.y - 26.0).max(clip.top() + 2.0);
    let r = Rect::from_min_size(at, size);
    painter.rect_filled(r, 3.0, bg);
    painter.rect_stroke(r, 3.0, Stroke::new(1.0, border), egui::StrokeKind::Inside);
    painter.galley(pos2(r.min.x + 5.0, r.min.y + 2.5), galley, fg);
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
