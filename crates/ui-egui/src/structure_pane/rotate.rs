//! Graphical vector inclination & support orientation (`Tool::Rotate`, protractor gizmo on the
//! sheet, smart snapping to 15° steps and member parallel/perpendicular axes, and the interactive
//! compass dial + vector component breakdown in the Properties panel).

use egui::{Align2, Color32, FontId, Pos2, RichText, Sense, Shape, Stroke, Ui, pos2, vec2};
use serde_json::json;
use strix_structure::{Load, LoadAxes, Num, Structure};

use super::edits;
use super::grips::run_load;
use super::{PICK, Sel, State, Tool, View, WordApp, load_shape, pick_load, pick_node, round, shifted, short, ts};

/// Radius (px) of the on-sheet protractor gizmo.
pub(super) const PROTRACTOR_R: f32 = 58.0;
/// Angular capture tolerance (degrees) for snapping to a connected member's axis or normal.
const MEMBER_SNAP_DEG: f64 = 5.0;

/// Something on the sheet whose inclination / orientation can be rotated graphically.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RotTarget {
    /// A concentrated force (`Load::Force`), by load index.
    Force(usize),
    /// A distributed load (`Load::Distributed`), by load index.
    Distributed(usize),
    /// A support on this node (`Support`), by node id.
    Support(String),
}

/// Structural alignment matched when snapping an inclination.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SnapRef {
    /// Perpendicular (`±90°`) to this member.
    Perp(String),
    /// Parallel (`0°` / `180°`) to this member.
    Parallel(String),
}

/// Wraps any finite angle in degrees into `(−180, 180]`, rounded cleanly.
pub(super) fn norm_deg(mut a: f64) -> f64 {
    if !a.is_finite() {
        return 0.0;
    }
    a %= 360.0;
    if a > 180.0 {
        a -= 360.0;
    } else if a <= -180.0 {
        a += 360.0;
    }
    let r = round(a);
    if r == -180.0 { 180.0 } else { r }
}

/// Signed shortest angular difference `a - b` in `(−180, 180]`.
pub(super) fn ang_diff(a: f64, b: f64) -> f64 {
    norm_deg(a - b)
}

/// Updates an angle `Num` from resolved `old_deg` to `new_deg`, keeping an expression intact when
/// rotated by a step (`alpha` → `alpha + 15`).
pub(super) fn shifted_angle(src: &Num, old_deg: f64, new_deg: f64) -> Num {
    let target = norm_deg(new_deg);
    if src.expr.is_some() {
        let d = ang_diff(target, norm_deg(old_deg));
        shifted(src, d)
    } else {
        Num::new(target)
    }
}

/// World direction angle (degrees CCW from `+x`) from screen point `center` to screen point `p`.
pub(super) fn screen_angle(center: Pos2, p: Pos2) -> Option<f64> {
    let (dx, dy) = (f64::from(p.x - center.x), f64::from(center.y - p.y));
    if !(dx.is_finite() && dy.is_finite()) || dx.hypot(dy) < 2.0 {
        return None;
    }
    Some(norm_deg(dy.atan2(dx).to_degrees()))
}

/// Screen point at `radius` px from `center` along world angle `deg` (CCW from `+x`).
pub(super) fn ring_pos(center: Pos2, radius: f32, deg: f64) -> Pos2 {
    let r = deg.to_radians();
    pos2(center.x + radius * (r.cos() as f32), center.y - radius * (r.sin() as f32))
}

/// The selection entry corresponding to `target`.
pub(super) fn sel_of(target: &RotTarget) -> Sel {
    match target {
        RotTarget::Force(k) | RotTarget::Distributed(k) => Sel::Load(*k),
        RotTarget::Support(id) => Sel::Node(id.clone()),
    }
}

/// Resolves the active rotatable target from the current selection (preferring the most recently
/// selected rotatable item).
pub(super) fn active_target(st: &Structure, sel: &[Sel]) -> Option<RotTarget> {
    for s in sel.iter().rev() {
        match s {
            Sel::Load(k) => match st.loads.get(*k) {
                Some(Load::Force { .. }) => return Some(RotTarget::Force(*k)),
                Some(Load::Distributed { .. }) => return Some(RotTarget::Distributed(*k)),
                _ => {}
            },
            Sel::Node(id) => {
                if st.supports.iter().any(|sup| sup.node == *id) {
                    return Some(RotTarget::Support(id.clone()));
                }
                if let Some((k, _)) =
                    st.loads.iter().enumerate().find(|(_, l)| matches!(l, Load::Force { node: Some(n), member: None, .. } if n == id))
                {
                    return Some(RotTarget::Force(k));
                }
            }
            Sel::Member(_) => {}
        }
    }
    None
}

/// Picks a rotatable element (Force, Distributed load, Support symbol, or Node with a support /
/// force) at screen point `p`.
pub(super) fn pick_rotatable(st: &Structure, v: View, p: Pos2) -> Option<RotTarget> {
    if let Some(k) = pick_load(st, v, p) {
        match st.loads.get(k) {
            Some(Load::Force { .. }) => return Some(RotTarget::Force(k)),
            Some(Load::Distributed { .. }) => return Some(RotTarget::Distributed(k)),
            _ => {}
        }
    }
    if let Some(id) = super::pick_support_node(st, v, p).or_else(|| pick_node(st, v, p)) {
        if st.supports.iter().any(|s| s.node == id) {
            return Some(RotTarget::Support(id));
        }
        if let Some((k, _)) = st.loads.iter().enumerate().find(|(_, l)| matches!(l, Load::Force { node: Some(n), member: None, .. } if *n == id)) {
            return Some(RotTarget::Force(k));
        }
    }
    // Also check if `p` is near the tail/arrow of a force even very close to its node.
    for (k, l) in st.loads.iter().enumerate().rev() {
        if matches!(l, Load::Force { .. })
            && let Some([a, b]) = load_shape(st, v, k).as_deref()
            && super::seg_dist(p, *a, *b) <= PICK
        {
            return Some(RotTarget::Force(k));
        }
    }
    None
}

/// World coordinates `(x, y)` of the rotation pivot of `target`.
pub(super) fn pivot(st: &Structure, target: &RotTarget) -> Option<(f64, f64)> {
    match target {
        RotTarget::Force(k) => match st.loads.get(*k)? {
            Load::Force { node, member, at, .. } => match (member, node) {
                (Some(m), _) => edits::point_on(st, m, at.value),
                (None, Some(n)) => st.node(n).map(|n| (n.x.value, n.y.value)),
                (None, None) => None,
            },
            _ => None,
        },
        RotTarget::Distributed(k) => match st.loads.get(*k)? {
            l @ Load::Distributed { from, to, .. } => {
                // The middle of its stretch, along its member or its aligned members.
                let line = st.load_line(l)?;
                let f = from.value.clamp(0.0, line.len);
                let t = to.as_ref().map_or(line.len, |t| t.value).clamp(0.0, line.len);
                Some(line.point(0.5 * (f + t)))
            }
            _ => None,
        },
        RotTarget::Support(id) => st.node(id).map(|n| (n.x.value, n.y.value)),
    }
}

/// Angle (degrees CCW from `+x`) of member `id` from `a` to `b`.
pub(super) fn member_deg(st: &Structure, id: &str) -> Option<f64> {
    let m = st.member(id)?;
    let (a, b) = (st.node(&m.a)?, st.node(&m.b)?);
    let (dx, dy) = (b.x.value - a.x.value, b.y.value - a.y.value);
    if dx.hypot(dy) <= 1e-9 {
        return None;
    }
    let d = dy.atan2(dx).to_degrees();
    d.is_finite().then_some(norm_deg(d))
}

/// Angle (degrees CCW from `+x`) of the line distributed load `k` lies along: its member's from
/// `a` to `b`, or that of the aligned members it goes on over (its local axes').
pub(super) fn line_deg(st: &Structure, k: usize) -> Option<f64> {
    let line = st.load_line(st.loads.get(k)?)?;
    let d = line.dir.1.atan2(line.dir.0).to_degrees();
    d.is_finite().then_some(norm_deg(d))
}

/// Connected members and their directions `(member_id, deg)` at `target`'s location.
pub(super) fn member_angles(st: &Structure, target: &RotTarget) -> Vec<(String, f64)> {
    let at_node = |id: &str| -> Vec<(String, f64)> {
        let Some(n) = st.node(id) else { return Vec::new() };
        st.members
            .iter()
            .filter_map(|m| {
                let other = if m.a == id {
                    st.node(&m.b)
                } else if m.b == id {
                    st.node(&m.a)
                } else {
                    None
                }?;
                let (dx, dy) = (other.x.value - n.x.value, other.y.value - n.y.value);
                (dx.hypot(dy) > 1e-9).then(|| (m.id.clone(), norm_deg(dy.atan2(dx).to_degrees())))
            })
            .collect()
    };
    match target {
        RotTarget::Force(k) => match st.loads.get(*k) {
            Some(Load::Force { member: Some(m), .. }) => member_deg(st, m).into_iter().map(|d| (m.clone(), d)).collect(),
            Some(Load::Force { node: Some(n), .. }) => at_node(n),
            _ => Vec::new(),
        },
        RotTarget::Distributed(k) => match st.loads.get(*k) {
            Some(Load::Distributed { member, .. }) => line_deg(st, *k).into_iter().map(|d| (member.clone(), d)).collect(),
            _ => Vec::new(),
        },
        RotTarget::Support(id) => at_node(id),
    }
}

/// World visual direction (degrees CCW from `+x`) of `target`:
/// - Force / Distributed: the direction in which the load's arrows push;
/// - Support: the direction from the node towards its ground/wall (`angle − 90°`).
pub(super) fn visual_angle(st: &Structure, target: &RotTarget) -> Option<f64> {
    match target {
        RotTarget::Force(k) => match st.loads.get(*k)? {
            Load::Force { value, angle, .. } => Some(if value.value < 0.0 { norm_deg(angle.value + 180.0) } else { norm_deg(angle.value) }),
            _ => None,
        },
        RotTarget::Distributed(k) => match st.loads.get(*k)? {
            Load::Distributed { q1, q2, angle, axes, .. } => {
                let base = match axes {
                    LoadAxes::Global => angle.value,
                    LoadAxes::Local => line_deg(st, *k).unwrap_or(0.0) + angle.value,
                };
                let q = if q1.value != 0.0 { q1.value } else { q2.as_ref().map_or(1.0, |q| q.value) };
                Some(if q < 0.0 { norm_deg(base + 180.0) } else { norm_deg(base) })
            }
            _ => None,
        },
        RotTarget::Support(id) => {
            let s = st.supports.iter().find(|s| s.node == *id)?;
            Some(norm_deg(s.angle.value - 90.0))
        }
    }
}

/// Stored `angle` parameter (resolved degrees) corresponding to world visual direction `vis_deg`.
pub(super) fn stored_angle_for_visual(st: &Structure, target: &RotTarget, vis_deg: f64) -> Option<f64> {
    match target {
        RotTarget::Force(k) => match st.loads.get(*k)? {
            Load::Force { value, .. } => Some(if value.value < 0.0 { norm_deg(vis_deg + 180.0) } else { norm_deg(vis_deg) }),
            _ => None,
        },
        RotTarget::Distributed(k) => match st.loads.get(*k)? {
            Load::Distributed { q1, q2, axes, .. } => {
                let q = if q1.value != 0.0 { q1.value } else { q2.as_ref().map_or(1.0, |q| q.value) };
                let base = if q < 0.0 { vis_deg + 180.0 } else { vis_deg };
                Some(match axes {
                    LoadAxes::Global => norm_deg(base),
                    LoadAxes::Local => norm_deg(base - line_deg(st, *k).unwrap_or(0.0)),
                })
            }
            _ => None,
        },
        RotTarget::Support(id) => {
            let _ = st.supports.iter().find(|s| s.node == *id)?;
            Some(norm_deg(vis_deg + 90.0))
        }
    }
}

/// World visual direction corresponding to a stored `angle` parameter `stored_deg`.
pub(super) fn visual_for_stored(st: &Structure, target: &RotTarget, stored_deg: f64) -> Option<f64> {
    match target {
        RotTarget::Force(k) => match st.loads.get(*k)? {
            Load::Force { value, .. } => Some(if value.value < 0.0 { norm_deg(stored_deg + 180.0) } else { norm_deg(stored_deg) }),
            _ => None,
        },
        RotTarget::Distributed(k) => match st.loads.get(*k)? {
            Load::Distributed { q1, q2, axes, .. } => {
                let base = match axes {
                    LoadAxes::Global => stored_deg,
                    LoadAxes::Local => line_deg(st, *k).unwrap_or(0.0) + stored_deg,
                };
                let q = if q1.value != 0.0 { q1.value } else { q2.as_ref().map_or(1.0, |q| q.value) };
                Some(if q < 0.0 { norm_deg(base + 180.0) } else { norm_deg(base) })
            }
            _ => None,
        },
        RotTarget::Support(_) => Some(norm_deg(stored_deg - 90.0)),
    }
}

/// Snaps `raw_vis_deg` to connected members' perpendicular/parallel directions (within 5°) and
/// 15° global increments when `snap_on` is true, or 1° steps when false.
pub(super) fn snap_visual_angle(st: &Structure, target: &RotTarget, raw_vis_deg: f64, snap_on: bool) -> (f64, Option<SnapRef>) {
    if !snap_on {
        return (norm_deg(raw_vis_deg.round()), None);
    }
    let members = member_angles(st, target);
    let mut best_ref: Option<(f64, f64, SnapRef)> = None;
    for (id, th) in &members {
        for cand in [norm_deg(th - 90.0), norm_deg(th + 90.0)] {
            let d = ang_diff(raw_vis_deg, cand).abs();
            if d <= MEMBER_SNAP_DEG && best_ref.as_ref().is_none_or(|b| d < b.0) {
                best_ref = Some((d, cand, SnapRef::Perp(id.clone())));
            }
        }
        for cand in [norm_deg(*th), norm_deg(th + 180.0)] {
            let d = ang_diff(raw_vis_deg, cand).abs();
            if d <= MEMBER_SNAP_DEG && best_ref.as_ref().is_none_or(|b| d < b.0) {
                best_ref = Some((d, cand, SnapRef::Parallel(id.clone())));
            }
        }
    }
    let step15 = norm_deg((raw_vis_deg / 15.0).round() * 15.0);
    if let Some((d_ref, ang, kind)) = best_ref {
        // Prefer an inclined member's exact ∟ / ‖ unless a 15° tick is much closer.
        let d_15 = ang_diff(raw_vis_deg, step15).abs();
        if (ang - step15).abs() <= 1e-6 || d_ref <= d_15 + 1.5 {
            return (ang, Some(kind));
        }
    }
    // Check if the 15° tick itself is perpendicular or parallel to a connected member.
    let tag = members.iter().find_map(|(id, th)| {
        let diff = ang_diff(step15, *th).abs();
        if (diff - 90.0).abs() <= 0.5 {
            Some(SnapRef::Perp(id.clone()))
        } else if diff <= 0.5 || (diff - 180.0).abs() <= 0.5 {
            Some(SnapRef::Parallel(id.clone()))
        } else {
            None
        }
    });
    (step15, tag)
}

/// Applies `new_stored_deg` to `target` on `st` in memory (for live preview on the sheet).
pub(super) fn preview_target(st: &Structure, target: &RotTarget, new_stored_deg: f64) -> Structure {
    let mut out = st.clone();
    match target {
        RotTarget::Force(k) => {
            if let Some(Load::Force { angle, .. }) = out.loads.get_mut(*k) {
                *angle = Num::new(norm_deg(new_stored_deg));
            }
        }
        RotTarget::Distributed(k) => {
            if let Some(Load::Distributed { angle, .. }) = out.loads.get_mut(*k) {
                *angle = Num::new(norm_deg(new_stored_deg));
            }
        }
        RotTarget::Support(id) => {
            if let Some(s) = out.supports.iter_mut().find(|s| s.node == *id) {
                s.angle = Num::new(norm_deg(new_stored_deg));
            }
        }
    }
    out
}

/// Commits `new_stored_deg` to `target` as a single undoable command (`structure.load` or
/// `structure.support`), preserving expressions when `preserve_expr` is true.
pub(super) fn commit_target(app: &mut WordApp, src: &Structure, st: &Structure, target: &RotTarget, new_stored_deg: f64, preserve_expr: bool) {
    let deg = norm_deg(new_stored_deg);
    match target {
        RotTarget::Force(k) => {
            let Some(Load::Force { angle: cur, .. }) = st.loads.get(*k) else { return };
            if ang_diff(cur.value, deg).abs() <= 1e-9 {
                return;
            }
            if let Some(mut l) = src.loads.get(*k).cloned() {
                if let Load::Force { angle, .. } = &mut l {
                    *angle = if preserve_expr { shifted_angle(angle, cur.value, deg) } else { Num::new(deg) };
                }
                run_load(app, *k, Some(l));
            }
        }
        RotTarget::Distributed(k) => {
            let Some(Load::Distributed { angle: cur, .. }) = st.loads.get(*k) else { return };
            if ang_diff(cur.value, deg).abs() <= 1e-9 {
                return;
            }
            if let Some(mut l) = src.loads.get(*k).cloned() {
                if let Load::Distributed { angle, .. } = &mut l {
                    *angle = if preserve_expr { shifted_angle(angle, cur.value, deg) } else { Num::new(deg) };
                }
                run_load(app, *k, Some(l));
            }
        }
        RotTarget::Support(id) => {
            let Some(cur) = st.supports.iter().find(|s| s.node == *id) else { return };
            if ang_diff(cur.angle.value, deg).abs() <= 1e-9 {
                return;
            }
            let next = match src.supports.iter().find(|s| s.node == *id) {
                Some(src_s) if preserve_expr => shifted_angle(&src_s.angle, cur.angle.value, deg),
                _ => Num::new(deg),
            };
            if let Ok(v) = serde_json::to_value(next) {
                let _ = app.run("structure.support", json!({"node": id, "angle": v}));
            }
        }
    }
}

/// Readout string for a rotation state (`α = −60° · ∟ Member 1 · Fx = …, Fy = …`).
pub(super) fn readout_for(st: &Structure, target: &RotTarget, vis_deg: f64, snap_ref: Option<&SnapRef>) -> String {
    let stored = stored_angle_for_visual(st, target, vis_deg).unwrap_or(vis_deg);
    let mut parts = vec![format!("α = {}°", short(stored))];
    match snap_ref {
        Some(SnapRef::Perp(m)) => parts.push(format!("∟ {}", crate::i18n::fmt(ts("Member {n}"), &[("n", m)]))),
        Some(SnapRef::Parallel(m)) => parts.push(format!("‖ {}", crate::i18n::fmt(ts("Member {n}"), &[("n", m)]))),
        None => {
            if let Some((m, th)) = member_angles(st, target).first() {
                let rel = acute_member_angle(vis_deg, *th);
                parts.push(format!("{}° ({})", short(rel), crate::i18n::fmt(ts("Member {n}"), &[("n", m)])));
            }
        }
    }
    if let RotTarget::Force(k) = target
        && let Some(Load::Force { value, .. }) = st.loads.get(*k)
    {
        let f = value.value.abs();
        let r = vis_deg.to_radians();
        let fu = ts(st.force_unit.symbol());
        parts.push(format!("F_x = {} {fu}, F_y = {} {fu}", short(f * r.cos()), short(f * r.sin())));
    }
    parts.join("   ·   ")
}

/// Acute-or-obtuse angle in `[0, 180]` between a vector along `vis_deg` and a member along `th`.
pub(super) fn acute_member_angle(vis_deg: f64, member_th: f64) -> f64 {
    let d = ang_diff(vis_deg, member_th).abs();
    round(d.min(180.0 - d))
}

/// What the protractor shows: the vector's visual angle and how it got there.
#[derive(Clone, Copy)]
pub(super) struct Dial<'a> {
    /// The vector's current visual angle (degrees, screen convention).
    pub vis_deg: f64,
    /// When dragging, the visual angle before the drag started.
    pub start_vis: Option<f64>,
    /// The member the angle is snapped to, if any.
    pub snap_ref: Option<&'a SnapRef>,
    /// A faint preview (hover) instead of the active protractor.
    pub faint: bool,
}

/// Draws the interactive graphical protractor on `painter` around `target`'s pivot.
pub(super) fn paint_protractor(painter: &egui::Painter, st: &Structure, v: View, target: &RotTarget, dial: Dial) {
    let Dial { vis_deg, start_vis, snap_ref, faint: faint_preview } = dial;
    let Some((wx, wy)) = pivot(st, target) else { return };
    let center = v.to_screen(wx, wy);
    let r = PROTRACTOR_R;
    let alpha_mul = if faint_preview { 0.45 } else { 1.0 };
    let slate = Color32::from_rgb(0x47, 0x55, 0x69).linear_multiply(alpha_mul);
    let accent = Color32::from_rgb(0xE1, 0x1D, 0x48).linear_multiply(alpha_mul);
    let teal = Color32::from_rgb(0x0D, 0x94, 0x88).linear_multiply(alpha_mul);

    // Soft translucent backing disc so ticks & labels stand out cleanly over the grid.
    painter.circle_filled(center, r, Color32::from_rgba_unmultiplied(255, 255, 255, if faint_preview { 110 } else { 212 }));
    painter.circle_stroke(center, r, Stroke::new(1.2, slate));

    // Faint reference crosshairs through the pivot.
    let cross = Stroke::new(0.8, Color32::from_gray(205).linear_multiply(alpha_mul));
    painter.line_segment([pos2(center.x - r, center.y), pos2(center.x + r, center.y)], cross);
    painter.line_segment([pos2(center.x, center.y - r), pos2(center.x, center.y + r)], cross);

    // Connected members' parallel and perpendicular ticks on the rim.
    for (_, th) in member_angles(st, target) {
        for (ang, is_perp) in [(th, false), (th + 180.0, false), (th - 90.0, true), (th + 90.0, true)] {
            let p_in = ring_pos(center, if is_perp { r - 9.0 } else { r - 6.0 }, ang);
            let p_out = ring_pos(center, r + 3.0, ang);
            let active = matches!(
                (snap_ref, is_perp),
                (Some(SnapRef::Perp(_)), true) | (Some(SnapRef::Parallel(_)), false) if ang_diff(vis_deg, ang).abs() <= 1.0
            );
            painter.line_segment([p_in, p_out], Stroke::new(if active { 2.2 } else { 1.2 }, teal));
        }
    }

    // 15° graduations around the 360° circle.
    let mut i = -180i32;
    while i < 180 {
        let deg = f64::from(i);
        let len = if i % 90 == 0 {
            8.0
        } else if i % 45 == 0 {
            5.5
        } else {
            3.2
        };
        painter.line_segment([ring_pos(center, r - len, deg), ring_pos(center, r, deg)], Stroke::new(if i % 90 == 0 { 1.2 } else { 0.8 }, slate));
        i += 15;
    }

    // Cardinal degree labels matching the object's stored `angle` convention.
    if !faint_preview {
        let ref_zero_vis = visual_for_stored(st, target, 0.0).unwrap_or(0.0);
        for (stored_lbl, vis_at) in [
            ("0°", ref_zero_vis),
            ("90°", norm_deg(ref_zero_vis + 90.0)),
            ("180°", norm_deg(ref_zero_vis + 180.0)),
            ("−90°", norm_deg(ref_zero_vis - 90.0)),
        ] {
            let lp = ring_pos(center, r + 11.0, vis_at);
            painter.text(lp, Align2::CENTER_CENTER, stored_lbl, FontId::proportional(9.5), slate);
        }

        // Shaded sector + arc from `0°` stored reference to current `vis_deg`.
        let sweep = ang_diff(vis_deg, ref_zero_vis);
        if sweep.abs() > 0.5 {
            let steps = ((sweep.abs() / 5.0).ceil() as usize).clamp(3, 72);
            let arc_r = r * 0.58;
            let mut poly = Vec::with_capacity(steps + 2);
            poly.push(center);
            let mut arc_pts = Vec::with_capacity(steps + 1);
            for k in 0..=steps {
                let a = ref_zero_vis + sweep * (k as f64 / steps as f64);
                let pt = ring_pos(center, arc_r, a);
                poly.push(pt);
                arc_pts.push(pt);
            }
            for tri in arc_pts.windows(2) {
                if let [p1, p2] = tri {
                    painter.add(Shape::convex_polygon(vec![center, *p1, *p2], Color32::from_rgba_unmultiplied(0xE1, 0x1D, 0x48, 28), Stroke::NONE));
                }
            }
            painter.add(Shape::line(arc_pts, Stroke::new(1.5, accent)));
        }
    }

    // Ghost ray showing where the angle started before dragging.
    if let Some(s_vis) = start_vis
        && ang_diff(vis_deg, s_vis).abs() > 0.5
    {
        let ghost_end = ring_pos(center, r - 2.0, s_vis);
        painter.extend(Shape::dashed_line(&[center, ghost_end], Stroke::new(1.0, slate), 4.0, 3.0));
    }

    // Active direction ray + opposite tail dashed guide (for loads whose arrow sits on the tail side).
    let dir_end = ring_pos(center, r, vis_deg);
    if matches!(target, RotTarget::Force(_) | RotTarget::Distributed(_)) {
        let tail_end = ring_pos(center, r, vis_deg + 180.0);
        painter.extend(Shape::dashed_line(&[center, tail_end], Stroke::new(1.2, accent.linear_multiply(0.75)), 4.0, 3.0));
        painter.circle_filled(tail_end, 4.0, Color32::WHITE);
        painter.circle_stroke(tail_end, 4.0, Stroke::new(1.5, accent));
    }
    painter.line_segment([center, dir_end], Stroke::new(2.0, accent));
    // Small directional arrowhead near `dir_end`.
    let rad = vis_deg.to_radians();
    let ux = rad.cos() as f32;
    let uy = -(rad.sin() as f32);
    let nx = -uy;
    let ny = ux;
    let base = ring_pos(center, r - 8.5, vis_deg);
    let tip = ring_pos(center, r - 1.5, vis_deg);
    painter.add(Shape::convex_polygon(vec![tip, base + vec2(nx * 3.5, ny * 3.5), base - vec2(nx * 3.5, ny * 3.5)], accent, Stroke::NONE));

    // Prominent draggable knob on the protractor rim.
    painter.circle_filled(dir_end, 5.5, accent);
    painter.circle_stroke(dir_end, 5.5, Stroke::new(1.5, Color32::WHITE));
    painter.circle_filled(center, 3.0, slate);

    // Floating badge with exact angle right next to the protractor when active.
    if !faint_preview {
        let stored = stored_angle_for_visual(st, target, vis_deg).unwrap_or(vis_deg);
        let badge = match snap_ref {
            Some(SnapRef::Perp(_)) => format!("{}° (∟)", short(stored)),
            Some(SnapRef::Parallel(_)) => format!("{}° (‖)", short(stored)),
            None => format!("{}°", short(stored)),
        };
        super::grips::paint_tag(painter, v.rect, pos2(center.x + r * 0.72, center.y - r * 0.92), &badge);
    }
}

/// Interactive compass dial + quick inclination presets + component breakdown for a Force or
/// Distributed load in the Properties panel. Returns `Some(new_stored_angle)` when clicked/dragged
/// or a preset button is pressed.
pub(super) fn load_inclination_ui(app: &mut WordApp, ui: &mut Ui, src: &Structure, st: &Structure, state: &mut State, k: usize) {
    let Some(load) = st.loads.get(k) else { return };
    let target = match load {
        Load::Force { .. } => RotTarget::Force(k),
        Load::Distributed { .. } => RotTarget::Distributed(k),
        Load::Moment { .. } => return,
    };
    let Some(vis_deg) = visual_angle(st, &target) else { return };
    let stored_deg = stored_angle_for_visual(st, &target, vis_deg).unwrap_or(vis_deg);
    let members = member_angles(st, &target);

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        // Interactive 64×64 vector compass dial.
        let dial_id = egui::Id::unique(("structure_load_dial", k));
        let size = vec2(64.0, 64.0);
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
        let center = rect.center();
        let dr = 26.0;
        let shift = ui.input(|i| i.modifiers.shift);
        let snap_on = state.snap != shift;

        let mut live_vis = vis_deg;
        if (resp.dragged() || resp.clicked())
            && let Some(p) = resp.interact_pointer_pos()
            && let Some(raw) = screen_angle(center, p)
        {
            let (snapped, _) = snap_visual_angle(st, &target, raw, snap_on);
            live_vis = snapped;
            ui.data_mut(|d| d.insert_temp::<f64>(dial_id, snapped));
        } else if let Some(saved) = ui.data(|d| d.get_temp::<f64>(dial_id)) {
            live_vis = saved;
        }
        if (resp.drag_stopped() || resp.clicked())
            && let Some(fin) = ui.data_mut(|d| d.remove_temp::<f64>(dial_id))
            && let Some(new_st) = stored_angle_for_visual(st, &target, fin)
        {
            commit_target(app, src, st, &target, new_st, false);
            return;
        }
        let resp = resp.on_hover_text(ts("Drag or click on the dial to incline the vector (Shift: 1° precision)"));
        if resp.hovered() || resp.dragged() {
            ui.ctx().set_cursor_icon(if resp.dragged() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
        }

        let p = ui.painter_at(rect);
        p.circle_filled(center, dr, Color32::WHITE);
        p.circle_stroke(center, dr, Stroke::new(1.0, Color32::from_gray(180)));
        p.line_segment([pos2(center.x - dr, center.y), pos2(center.x + dr, center.y)], Stroke::new(0.7, Color32::from_gray(220)));
        p.line_segment([pos2(center.x, center.y - dr), pos2(center.x, center.y + dr)], Stroke::new(0.7, Color32::from_gray(220)));
        for (_, th) in &members {
            let a = ring_pos(center, dr - 2.0, *th);
            let b = ring_pos(center, dr - 2.0, th + 180.0);
            p.line_segment([a, b], Stroke::new(1.5, Color32::from_rgb(0x0D, 0x94, 0x88)));
        }
        let tip = ring_pos(center, dr - 2.0, live_vis);
        let col = Color32::from_rgb(0xB0, 0x3A, 0x2E);
        p.line_segment([center, tip], Stroke::new(2.0, col));
        p.circle_filled(tip, 3.5, col);
        p.circle_filled(center, 2.2, Color32::DARK_GRAY);

        // Quick preset & step buttons beside the dial.
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
            ui.horizontal_wrapped(|ui| {
                for (lbl, tip_txt, vis_preset) in [
                    ("↓ −90°", "Down (−90°)", -90.0),
                    ("↑ 90°", "Up (+90°)", 90.0),
                    ("→ 0°", "Right (0°)", 0.0),
                    ("← 180°", "Left (180°)", 180.0),
                    ("−45°", "At −45°", -45.0),
                    ("+45°", "At +45°", 45.0),
                ] {
                    let active = ang_diff(vis_deg, vis_preset).abs() <= 0.5;
                    if ui.selectable_label(active, lbl).on_hover_text(ts(tip_txt)).clicked()
                        && let Some(st_a) = stored_angle_for_visual(st, &target, vis_preset)
                    {
                        commit_target(app, src, st, &target, st_a, false);
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                if ui.small_button("−15°").on_hover_text(ts("Turn 15° clockwise")).clicked() {
                    commit_target(app, src, st, &target, stored_deg - 15.0, true);
                }
                if ui.small_button("+15°").on_hover_text(ts("Turn 15° counter-clockwise")).clicked() {
                    commit_target(app, src, st, &target, stored_deg + 15.0, true);
                }
                if let Some((_, th)) = members.first() {
                    let perp_vis = norm_deg(th - 90.0);
                    let par_vis = norm_deg(*th);
                    if ui.small_button("∟").on_hover_text(ts("Perpendicular to the member")).clicked()
                        && let Some(st_a) = stored_angle_for_visual(st, &target, perp_vis)
                    {
                        commit_target(app, src, st, &target, st_a, false);
                    }
                    if ui.small_button("‖").on_hover_text(ts("Parallel to the member")).clicked()
                        && let Some(st_a) = stored_angle_for_visual(st, &target, par_vis)
                    {
                        commit_target(app, src, st, &target, st_a, false);
                    }
                }
                if ui.small_button("↔").on_hover_text(ts("Reverse")).clicked() {
                    run_load(app, k, edits::reversed(src, k));
                }
                let rot_on = state.tool == Tool::Rotate && state.has(&Sel::Load(k));
                if ui.selectable_label(rot_on, ts("Rotate")).on_hover_text(ts("Open the graphical protractor on the sheet (R)")).clicked() {
                    state.select(Sel::Load(k));
                    state.tool = if rot_on { Tool::Select } else { Tool::Rotate };
                    state.start = None;
                }
            });
        });
    });

    // Angle with member & vector components summary.
    let rad = vis_deg.to_radians();
    match load {
        Load::Force { value, .. } => {
            let f = value.value.abs();
            let fu = ts(st.force_unit.symbol());
            let (fx, fy) = (f * rad.cos(), f * rad.sin());
            ui.label(RichText::new(format!("F_x = {} {fu}   ·   F_y = {} {fu}", short(fx), short(fy))).small().weak());
            if let Some((m_id, th)) = members.first() {
                let rel = acute_member_angle(vis_deg, *th);
                let d_rad = (vis_deg - th).to_radians();
                let (f_par, f_perp) = (f * d_rad.cos(), -f * d_rad.sin());
                ui.label(
                    RichText::new(format!(
                        "{}: {}°   (F‖ = {} {fu}, F∟ = {} {fu})",
                        crate::i18n::fmt(ts("Member {n}"), &[("n", m_id)]),
                        short(rel),
                        short(f_par),
                        short(f_perp)
                    ))
                    .small()
                    .weak(),
                );
            }
        }
        Load::Distributed { q1, q2, .. } => {
            let q = q1.value.abs().max(q2.as_ref().map_or(0.0, |q| q.value.abs()));
            let fu = ts(st.force_unit.symbol());
            let lu = ts(st.length_unit.symbol());
            let (qx, qy) = (q * rad.cos(), q * rad.sin());
            ui.label(RichText::new(format!("q_x = {} {fu}/{lu}   ·   q_y = {} {fu}/{lu}", short(qx), short(qy))).small().weak());
            if let Some((m_id, th)) = members.first() {
                let rel = acute_member_angle(vis_deg, *th);
                ui.label(RichText::new(format!("{}: {}°", crate::i18n::fmt(ts("Member {n}"), &[("n", m_id)]), short(rel))).small().weak());
            }
        }
        Load::Moment { .. } => {}
    }
}

/// Quick orientation presets & Rotate button for a Support in Node Properties.
pub(super) fn support_orientation_ui(app: &mut WordApp, ui: &mut Ui, src: &Structure, st: &Structure, state: &mut State, node_id: &str) {
    let target = RotTarget::Support(node_id.to_string());
    let Some(cur) = st.supports.iter().find(|s| s.node == node_id) else { return };
    let stored = norm_deg(cur.angle.value);
    let members = member_angles(st, &target);

    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
        for (lbl, tip_txt, deg) in [
            ("0°", "On horizontal ground below (0°)", 0.0),
            ("−90°", "On a wall to the left (−90°)", -90.0),
            ("90°", "On a wall to the right (+90°)", 90.0),
            ("180°", "Above the node (180°)", 180.0),
        ] {
            if ui.selectable_label(ang_diff(stored, deg).abs() <= 0.5, lbl).on_hover_text(ts(tip_txt)).clicked() {
                commit_target(app, src, st, &target, deg, false);
            }
        }
        if ui.small_button("−15°").on_hover_text(ts("Turn 15° clockwise")).clicked() {
            commit_target(app, src, st, &target, stored - 15.0, true);
        }
        if ui.small_button("+15°").on_hover_text(ts("Turn 15° counter-clockwise")).clicked() {
            commit_target(app, src, st, &target, stored + 15.0, true);
        }
        if let Some((_, th)) = members.first() {
            // Ground parallel to member: support angle = member angle `th`.
            let par_ground = norm_deg(*th);
            if ui.small_button("‖").on_hover_text(ts("Parallel to the member")).clicked() {
                commit_target(app, src, st, &target, par_ground, false);
            }
            let perp_ground = norm_deg(th + 90.0);
            if ui.small_button("∟").on_hover_text(ts("Perpendicular to the member")).clicked() {
                commit_target(app, src, st, &target, perp_ground, false);
            }
        }
        let rot_on = state.tool == Tool::Rotate && state.has(&Sel::Node(node_id.to_string()));
        if ui.selectable_label(rot_on, ts("Rotate")).on_hover_text(ts("Open the graphical protractor on the sheet (R)")).clicked() {
            state.select(Sel::Node(node_id.to_string()));
            state.tool = if rot_on { Tool::Select } else { Tool::Rotate };
            state.start = None;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visual_and_stored_angles_round_trip_for_forces_distributed_and_supports() {
        let st = Structure::from_json(
            r#"{"nodes":[{"id":"A","x":0,"y":0},{"id":"B","x":3,"y":4}],
                "members":[{"id":"1","a":"A","b":"B"}],
                "supports":[{"node":"A","kind":"roller","angle":-90}],
                "loads":[
                    {"type":"force","node":"B","value":10,"angle":-60},
                    {"type":"force","node":"B","value":-10,"angle":-60},
                    {"type":"distributed","member":"1","q1":10,"axes":"local","angle":-90}
                ]}"#,
        )
        .unwrap();

        for t in [RotTarget::Force(0), RotTarget::Force(1), RotTarget::Distributed(2), RotTarget::Support("A".into())] {
            let vis = visual_angle(&st, &t).unwrap();
            let back = stored_angle_for_visual(&st, &t, vis).unwrap();
            let vis2 = visual_for_stored(&st, &t, back).unwrap();
            assert!(ang_diff(vis, vis2).abs() < 1e-6, "{t:?}: {vis} -> {back} -> {vis2}");
        }

        // Member 1 has slope atan2(4, 3) ≈ 53.130102°; perpendicular is −36.869898°.
        let f0 = RotTarget::Force(0);
        let (snapped, kind) = snap_visual_angle(&st, &f0, -35.0, true);
        assert_eq!(kind, Some(SnapRef::Perp("1".into())));
        assert!((snapped - (-36.869898)).abs() < 1e-4, "{snapped}");

        // Outside member tolerance, snaps to 15° steps.
        let (snapped15, _) = snap_visual_angle(&st, &f0, -17.0, true);
        assert_eq!(snapped15, -15.0);

        // Expression angle preserved when rotated by delta.
        let expr = Num { value: 30.0, expr: Some("alpha".into()) };
        assert_eq!(shifted_angle(&expr, 30.0, 45.0).expr.as_deref(), Some("alpha + 15"));
        assert_eq!(shifted_angle(&Num::new(170.0), 170.0, 190.0).value, -170.0);
    }
}
