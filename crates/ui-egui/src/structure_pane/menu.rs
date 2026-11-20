//! The sheet's context menu (a right click): what can be done with what is under the pointer
//! (a node, a member, a load), with the selection, or with the sheet. A right click on something
//! not selected selects it first; a right click while a member is being drawn only ends it.
//! Every item is one undoable `structure.*` command.

use egui::{Pos2, Ui};
use serde_json::json;
use strix_structure::{Load, LoadAxes, Num, Structure, SupportKind};

use super::edits::{self, Place};
use super::grips::{run_load, select_connected};
use super::{KINDS, Sel, State, Tool, View, WordApp, pick, pick_member, round, round_at, snap, ts, without};

/// What the menu was opened on.
#[derive(Clone, Debug, Default)]
struct Target {
    /// What was under the pointer.
    hit: Option<Sel>,
    /// The point clicked, snapped to the grid or a node.
    snapped: (f64, f64),
    /// The member under the pointer and how far along it (its point rounded to the grid).
    member_at: Option<(String, f64)>,
}

/// What an item does.
#[derive(Clone, Debug, PartialEq)]
enum Act {
    Select,
    Rotate,
    Draw((f64, f64)),
    AddNode((f64, f64)),
    Support(String, Option<SupportKind>),
    Hinge(String, bool),
    MemberHinge(String, bool, bool),
    SwapEnds(String),
    Force(Place),
    Couple(Place),
    Distributed(String),
    Split(String, f64),
    /// The two aligned members at this node joined into one, the node removed.
    Join(String),
    Reverse(usize),
    Angle(usize, f64),
    Whole(usize),
    Uniform(usize),
    Triangular(usize),
    Perpendicular(usize),
    Parallel(usize),
    SelectEnds(String),
    SelectLoads,
    Connected(Sel),
    SelectAll,
    Deselect,
    Delete,
    Fit,
    Snap,
    Dimensions,
}

fn target_id() -> egui::Id {
    egui::Id::unique("structure_editor_menu_target")
}

/// Opens the menu on a right click (selecting what is under the pointer) and does what is chosen.
pub(super) fn context(app: &mut WordApp, resp: &egui::Response, src: &Structure, st: &Structure, v: View, state: &mut State) {
    if resp.secondary_clicked() {
        // Drawing a member: a right click ends it, as the CAD editor does.
        if state.start.is_some() {
            return;
        }
        let Some(p) = resp.interact_pointer_pos() else { return };
        let t = aim(st, v, state, p);
        if let Some(h) = &t.hit
            && !state.has(h)
        {
            state.select(h.clone());
        }
        resp.ctx.data_mut(|d| d.insert_temp(target_id(), t));
    }
    let t: Target = resp.ctx.data(|d| d.get_temp(target_id())).unwrap_or_default();
    let mut act = None;
    resp.context_menu(|ui| act = items(ui, st, state, &t));
    if let Some(a) = act {
        apply(app, src, st, state, a);
    }
}

/// What a right click at `p` is on.
fn aim(st: &Structure, v: View, state: &State, p: Pos2) -> Target {
    let member_at = pick_member(st, v, p).map(|(id, at)| {
        let len = st.member(&id).and_then(|m| st.length(m)).unwrap_or(0.0);
        let at = round(round_at(at, len, state));
        (id, at)
    });
    Target { hit: pick(st, v, p), snapped: snap(st, v, p, state).0, member_at }
}

/// The menu's items for target `t`; returns the one chosen.
fn items(ui: &mut Ui, st: &Structure, state: &State, t: &Target) -> Option<Act> {
    ui.set_min_width(220.0);
    let act = std::cell::RefCell::new(None);
    let pick = |ui: &mut Ui, a: Act| {
        act.replace(Some(a));
        ui.close();
    };
    let item = |ui: &mut Ui, label: &str, keys: &str, checked: bool, a: Act| {
        if ui.add(egui::Button::new(ts(label)).shortcut_text(ts(keys)).selected(checked)).clicked() {
            pick(ui, a);
        }
    };
    let title = |ui: &mut Ui, text: String| {
        ui.label(egui::RichText::new(text).strong().small());
    };
    // Join members at `node`: offered where two members meet, disabled (with the reason) when
    // they cannot be joined.
    let join = |ui: &mut Ui, node: &str| {
        let why = match edits::join_members(st, st, node) {
            Ok(_) => None,
            Err(edits::NoJoin::NotTwo) => return,
            Err(e) => Some(e.reason()),
        };
        let resp = ui.add_enabled(why.is_none(), egui::Button::new(ts("Join the members")));
        if let Some(why) = why {
            resp.on_disabled_hover_text(ts(why));
        } else if resp.clicked() {
            pick(ui, Act::Join(node.to_string()));
        }
    };
    if state.tool != Tool::Select {
        item(ui, "Select tool", "Esc", false, Act::Select);
        ui.separator();
    }
    let fmt = crate::i18n::fmt;
    match (state.sel.len(), state.primary().cloned()) {
        (_, Some(Sel::Node(id))) => {
            title(ui, fmt(ts("Node {n}"), &[("n", &id)]));
            let current = st.supports.iter().find(|s| s.node == id).map(|s| s.kind);
            ui.menu_button(ts("Support"), |ui| {
                for (k, label) in KINDS {
                    item(ui, label, "", current == k, Act::Support(id.clone(), k));
                }
            });
            if current.is_some() {
                item(ui, "Rotate", "R", state.tool == Tool::Rotate, Act::Rotate);
            }
            let hinge = st.node(&id).is_some_and(|n| n.hinge);
            item(ui, "Internal hinge", "", hinge, Act::Hinge(id.clone(), !hinge));
            join(ui, &id);
            ui.separator();
            item(ui, "Add a force here", "", false, Act::Force(Place::Node(id.clone())));
            item(ui, "Add a couple here", "", false, Act::Couple(Place::Node(id.clone())));
            if let Some(n) = st.node(&id) {
                item(ui, "Draw a member from here", "", false, Act::Draw((n.x.value, n.y.value)));
            }
            ui.separator();
            item(ui, "Select its loads", "", false, Act::SelectLoads);
            item(ui, "Select the connected part", "Double-click", false, Act::Connected(Sel::Node(id.clone())));
            item(ui, "Delete", "Del", false, Act::Delete);
        }
        (_, Some(Sel::Member(id))) => {
            title(ui, fmt(ts("Member {n}"), &[("n", &id)]));
            let here = t.member_at.as_ref().filter(|(m, _)| *m == id).map_or_else(|| half(st, &id), |(_, at)| *at);
            item(ui, "Add a force here", "", false, Act::Force(Place::Member(id.clone(), here)));
            item(ui, "Add a couple here", "", false, Act::Couple(Place::Member(id.clone(), here)));
            item(ui, "Add a distributed load", "", false, Act::Distributed(id.clone()));
            ui.separator();
            let len = st.member(&id).and_then(|m| st.length(m)).unwrap_or(0.0);
            if here > 1e-9 && here < len - 1e-9 {
                item(ui, "Split here", "", false, Act::Split(id.clone(), here));
            }
            let mid = half(st, &id);
            if (mid - here).abs() > 1e-9 && mid > 0.0 {
                item(ui, "Split in half", "", false, Act::Split(id.clone(), mid));
            }
            if let Some(m) = st.member(&id) {
                item(ui, "Swap ends", "", false, Act::SwapEnds(id.clone()));
                let a = fmt(ts("Hinge at end {n}"), &[("n", &m.a)]);
                let b = fmt(ts("Hinge at end {n}"), &[("n", &m.b)]);
                if ui.add(egui::Button::new(a).selected(m.hinge_a)).clicked() {
                    pick(ui, Act::MemberHinge(id.clone(), false, !m.hinge_a));
                }
                if ui.add(egui::Button::new(b).selected(m.hinge_b)).clicked() {
                    pick(ui, Act::MemberHinge(id.clone(), true, !m.hinge_b));
                }
            }
            ui.separator();
            item(ui, "Select its ends", "", false, Act::SelectEnds(id.clone()));
            item(ui, "Select its loads", "", false, Act::SelectLoads);
            item(ui, "Select the connected part", "Double-click", false, Act::Connected(Sel::Member(id.clone())));
            item(ui, "Delete", "Del", false, Act::Delete);
        }
        (_, Some(Sel::Load(k))) => {
            let names = strix_structure::load_names(st);
            let sym = names.get(k).copied().flatten().map(|n| format!("{} · ", n.pretty())).unwrap_or_default();
            match st.loads.get(k) {
                Some(Load::Force { member, .. }) => {
                    title(ui, format!("{sym}{}", ts("Force")));
                    item(ui, "Rotate", "R", state.tool == Tool::Rotate, Act::Rotate);
                    item(ui, "Reverse", "", false, Act::Reverse(k));
                    ui.menu_button(ts("Direction"), |ui| {
                        item(ui, "Vertical", "", false, Act::Angle(k, -90.0));
                        item(ui, "Up (+90°)", "", false, Act::Angle(k, 90.0));
                        item(ui, "Horizontal", "", false, Act::Angle(k, 0.0));
                        item(ui, "Left (180°)", "", false, Act::Angle(k, 180.0));
                        item(ui, "At 45°", "", false, Act::Angle(k, -45.0));
                        item(ui, "At +45°", "", false, Act::Angle(k, 45.0));
                        if member.is_some() {
                            ui.separator();
                            item(ui, "Perpendicular to the member", "", false, Act::Perpendicular(k));
                            item(ui, "Parallel to the member", "", false, Act::Parallel(k));
                        }
                    });
                }
                Some(Load::Moment { .. }) => {
                    title(ui, format!("{sym}{}", ts("Couple")));
                    item(ui, "Reverse", "", false, Act::Reverse(k));
                }
                Some(Load::Distributed { q2, from, to, angle, axes, .. }) => {
                    title(ui, format!("{sym}{}", ts("Distributed load")));
                    item(ui, "Rotate", "R", state.tool == Tool::Rotate, Act::Rotate);
                    item(ui, "Reverse", "", false, Act::Reverse(k));
                    item(ui, "Uniform", "", q2.is_none(), Act::Uniform(k));
                    item(ui, "Triangular", "", q2.is_some(), Act::Triangular(k));
                    item(ui, "On the whole member", "", from.value == 0.0 && to.is_none(), Act::Whole(k));
                    let perp = *axes == LoadAxes::Local && angle.value == -90.0;
                    item(ui, "Perpendicular to the member", "", perp, Act::Perpendicular(k));
                    ui.menu_button(ts("Direction"), |ui| {
                        item(ui, "Vertical", "", false, Act::Angle(k, -90.0));
                        item(ui, "Horizontal", "", false, Act::Angle(k, 0.0));
                        item(ui, "At 45°", "", false, Act::Angle(k, -45.0));
                        item(ui, "Perpendicular to the member", "", perp, Act::Perpendicular(k));
                    });
                }
                None => {}
            }
            ui.separator();
            item(ui, "Delete", "Del", false, Act::Delete);
        }
        (0, _) => {
            item(ui, "Add a node here", "", false, Act::AddNode(t.snapped));
            item(ui, "Draw a member from here", "", false, Act::Draw(t.snapped));
            ui.separator();
            item(ui, "Select all", "Ctrl+A", false, Act::SelectAll);
            item(ui, "Fit", "", false, Act::Fit);
            item(ui, "Snap to the grid", "", state.snap, Act::Snap);
            item(ui, "Dimensions", "", state.dims, Act::Dimensions);
        }
        (n, _) => {
            title(ui, fmt(ts("{n} items selected"), &[("n", &n.to_string())]));
            // Two members meeting at a node: join them there.
            if let [Sel::Member(p), Sel::Member(q)] = state.sel.as_slice()
                && let Some(node) = edits::shared_node(st, p, q)
            {
                join(ui, &node);
            }
            item(ui, "Select its loads", "", false, Act::SelectLoads);
            item(ui, "Deselect", "Esc", false, Act::Deselect);
            item(ui, "Delete", "Del", false, Act::Delete);
        }
    }
    act.into_inner()
}

/// Half member `id`'s length, rounded.
fn half(st: &Structure, id: &str) -> f64 {
    round(st.member(id).and_then(|m| st.length(m)).unwrap_or(0.0) / 2.0)
}

/// The angle (degrees) of member `id` from its `a` end to its `b` end.
fn member_angle(st: &Structure, id: &str) -> Option<f64> {
    let m = st.member(id)?;
    let (a, b) = (st.node(&m.a)?, st.node(&m.b)?);
    let d = (b.y.value - a.y.value).atan2(b.x.value - a.x.value).to_degrees();
    d.is_finite().then_some(d)
}

/// The loads on what is selected: on its nodes, on its members, and those selected.
fn loads_of(st: &Structure, sel: &[Sel]) -> Vec<Sel> {
    let has = |s: Sel| sel.contains(&s);
    st.loads
        .iter()
        .enumerate()
        .filter(|(k, l)| {
            has(Sel::Load(*k))
                || match l {
                    Load::Force { node, member, .. } | Load::Moment { node, member, .. } => {
                        node.as_ref().is_some_and(|n| has(Sel::Node(n.clone()))) || member.as_ref().is_some_and(|m| has(Sel::Member(m.clone())))
                    }
                    Load::Distributed { member, .. } => has(Sel::Member(member.clone())),
                }
        })
        .map(|(k, _)| Sel::Load(k))
        .collect()
}

/// Does `a`.
fn apply(app: &mut WordApp, src: &Structure, st: &Structure, state: &mut State, a: Act) {
    let set = |app: &mut WordApp, out: &Structure| {
        serde_json::to_value(out).ok().is_some_and(|j| app.run("structure.set", json!({"structure": j})).is_ok())
    };
    let point = |kind: &str, place: &Place| match place {
        Place::Node(id) => json!({"type": kind, "node": id, "value": 10.0}),
        Place::Member(id, at) => json!({"type": kind, "member": id, "at": round(*at), "value": 10.0}),
    };
    let new_load = |app: &mut WordApp, state: &mut State, l: serde_json::Value| {
        if app.run("structure.load", json!({"load": l})).is_ok() {
            // The new load is the last; selected once the count is seen again.
            state.loads = Some(st.loads.len() + 1);
            state.select(Sel::Load(st.loads.len()));
        }
    };
    match a {
        Act::Select => {
            state.tool = Tool::Select;
            state.start = None;
        }
        Act::Rotate => {
            state.tool = if state.tool == Tool::Rotate { Tool::Select } else { Tool::Rotate };
            state.start = None;
        }
        Act::Draw(p) => {
            state.tool = Tool::Member;
            state.start = Some(p);
        }
        Act::AddNode((x, y)) => {
            if let Some(id) = super::node_at(app, st, (x, y)) {
                state.select(Sel::Node(id));
            }
        }
        Act::Support(id, kind) => {
            let p = match kind {
                Some(k) => json!({"node": id, "kind": k.name()}),
                None => json!({"node": id, "remove": true}),
            };
            let _ = app.run("structure.support", p);
        }
        Act::Hinge(id, on) => {
            let _ = app.run("structure.node", json!({"id": id, "hinge": on}));
        }
        Act::MemberHinge(id, b, on) => {
            let key = if b { "hingeB" } else { "hingeA" };
            let _ = app.run("structure.member", json!({"id": id, key: on}));
        }
        Act::SwapEnds(id) => {
            if let Some(out) = edits::reverse_member(src, st, &id) {
                let _ = set(app, &out);
            }
        }
        Act::Force(place) => new_load(app, state, point("force", &place)),
        Act::Couple(place) => new_load(app, state, point("moment", &place)),
        Act::Distributed(id) => new_load(app, state, json!({"type": "distributed", "member": id, "q1": 10.0})),
        Act::Split(id, s) => {
            if let Some((out, node)) = edits::split_member(src, st, &id, s)
                && set(app, &out)
            {
                state.select(Sel::Node(node));
            }
        }
        Act::Join(node) => {
            if let Ok(out) = edits::join_members(src, st, &node) {
                // The member that kept its name: the one now running across where the node was.
                let kept = st.members.iter().find(|m| (m.a == node || m.b == node) && out.member(&m.id).is_some()).map(|m| m.id.clone());
                if set(app, &out) {
                    state.sel = kept.map(Sel::Member).into_iter().collect();
                }
            }
        }
        Act::Reverse(k) => run_load(app, k, edits::reversed(src, k)),
        Act::Angle(k, deg) => {
            let mut l = src.loads.get(k).cloned();
            match &mut l {
                Some(Load::Force { angle, .. }) => *angle = Num::new(deg),
                Some(Load::Distributed { angle, axes, .. }) => {
                    *angle = Num::new(deg);
                    *axes = LoadAxes::Global;
                }
                _ => {}
            }
            run_load(app, k, l);
        }
        Act::Perpendicular(k) => {
            let mut l = src.loads.get(k).cloned();
            match &mut l {
                Some(Load::Force { member: Some(m), angle, .. }) => {
                    let Some(th) = member_angle(st, m) else { return };
                    // The side below the member (left of its direction turned clockwise).
                    let mut d = th - 90.0;
                    if d <= -180.0 {
                        d += 360.0;
                    }
                    *angle = Num::new(round(d));
                }
                Some(Load::Distributed { angle, axes, .. }) => {
                    *angle = Num::new(-90.0);
                    *axes = LoadAxes::Local;
                }
                _ => return,
            }
            run_load(app, k, l);
        }
        Act::Parallel(k) => {
            let mut l = src.loads.get(k).cloned();
            if let Some(Load::Force { member: Some(m), angle, .. }) = &mut l {
                let Some(th) = member_angle(st, m) else { return };
                *angle = Num::new(round(th));
                run_load(app, k, l);
            }
        }
        Act::Whole(k) => {
            let mut l = src.loads.get(k).cloned();
            if let Some(Load::Distributed { from, to, .. }) = &mut l {
                *from = Num::new(0.0);
                *to = None;
            }
            run_load(app, k, l);
        }
        Act::Uniform(k) => {
            let mut l = src.loads.get(k).cloned();
            if let Some(Load::Distributed { q1, q2, .. }) = &mut l {
                // The larger end, so a triangle becomes the band that covers it.
                if let Some(q) = q2.take()
                    && q.value.abs() > q1.value.abs()
                {
                    *q1 = q;
                }
            }
            run_load(app, k, l);
        }
        Act::Triangular(k) => {
            let mut l = src.loads.get(k).cloned();
            if let Some(Load::Distributed { q1, q2, .. }) = &mut l {
                if q2.is_none() {
                    *q2 = Some(q1.clone());
                }
                *q1 = Num::new(0.0);
            }
            run_load(app, k, l);
        }
        Act::SelectEnds(id) => {
            if let Some(m) = st.member(&id) {
                state.sel = vec![Sel::Node(m.a.clone()), Sel::Node(m.b.clone())];
            }
        }
        Act::SelectLoads => {
            let loads = loads_of(st, &state.sel);
            if !loads.is_empty() {
                state.sel = loads;
            }
        }
        Act::Connected(hit) => select_connected(st, state, &hit),
        Act::SelectAll => select_all(st, state),
        Act::Deselect => state.sel.clear(),
        Act::Delete => {
            let kept = without(src, &state.sel);
            if app.run("structure.set", json!({"structure": kept})).is_ok() {
                state.sel.clear();
            }
        }
        Act::Fit => state.view = None,
        Act::Snap => state.snap = !state.snap,
        Act::Dimensions => state.dims = !state.dims,
    }
}

/// Everything selected: nodes, members and loads.
pub(super) fn select_all(st: &Structure, state: &mut State) {
    state.sel = st
        .nodes
        .iter()
        .map(|n| Sel::Node(n.id.clone()))
        .chain(st.members.iter().map(|m| Sel::Member(m.id.clone())))
        .chain((0..st.loads.len()).map(Sel::Load))
        .collect();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_loads_of_a_selection() {
        let st = Structure::from_json(
            r#"{"nodes":[{"id":"A","x":0,"y":0},{"id":"B","x":4,"y":0}],"members":[{"id":"1","a":"A","b":"B"}],
                "loads":[{"type":"force","node":"A","value":1},{"type":"distributed","member":"1","q1":1},{"type":"moment","node":"B","value":1}]}"#,
        )
        .unwrap();
        assert_eq!(loads_of(&st, &[Sel::Node("A".into())]), [Sel::Load(0)]);
        assert_eq!(loads_of(&st, &[Sel::Member("1".into()), Sel::Load(2)]), [Sel::Load(1), Sel::Load(2)]);
        assert_eq!(member_angle(&st, "1"), Some(0.0));
    }

    use super::super::grips::Grip;
    use egui::{Rect, pos2, vec2};
    use wordcraft_engine::Session;

    /// A portal frame (A–B–C–D, a uniform load on 2, a horizontal force at B) as written and as
    /// resolved, and a view of it.
    fn frame() -> (WordApp, View) {
        let mut app = WordApp::new(Session::new(wordcraft_doc::Document::new()), crate::Services::default());
        app.run("structure.insert", json!({"template": "frame"})).unwrap();
        let (_, st) = now(&mut app);
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(600.0, 500.0));
        let (cx, cy, s) = super::super::fit(&st, rect);
        (app, View { rect, cx, cy, s })
    }

    fn now(app: &mut WordApp) -> (Structure, Structure) {
        let info = app.session.run("structure.get", &json!({})).unwrap();
        (serde_json::from_value(info["structure"].clone()).unwrap(), serde_json::from_value(info["resolved"].clone()).unwrap())
    }

    /// The menu's Split in half, Reverse and Support items change the structure in one step each.
    #[test]
    fn menu_items_edit_the_structure() {
        let (mut app, _) = frame();
        let mut state = State::default();
        let (src, st) = now(&mut app);
        apply(&mut app, &src, &st, &mut state, Act::Split("2".into(), 3.0));
        let (src, st) = now(&mut app);
        assert_eq!(st.members.len(), 4);
        let e = st.node("E").unwrap();
        assert_eq!((e.x.value, e.y.value), (3.0, 4.0));
        assert_eq!(state.sel, [Sel::Node("E".into())], "the new node selected");
        // The uniform load went in two pieces, one on each half.
        let on = |m: &str| st.loads.iter().filter(|l| matches!(l, Load::Distributed { member, .. } if member == m)).count();
        assert_eq!((on("2"), on("4")), (1, 1));
        // One undo puts the member back whole.
        app.run("edit.undo", json!({})).unwrap();
        assert_eq!(now(&mut app).1.members.len(), 3);
        let (src2, st2) = now(&mut app);
        let (k, before) =
            st2.loads.iter().enumerate().find_map(|(k, l)| if let Load::Force { value, .. } = l { Some((k, value.value)) } else { None }).unwrap();
        apply(&mut app, &src2, &st2, &mut state, Act::Reverse(k));
        assert!(matches!(&now(&mut app).1.loads[k], Load::Force { value, .. } if value.value == -before));
        apply(&mut app, &src, &st, &mut state, Act::Support("B".into(), Some(SupportKind::Fixed)));
        assert!(now(&mut app).1.supports.iter().any(|s| s.node == "B" && s.kind == SupportKind::Fixed));
    }

    /// Join the members undoes Split: one member again, the node gone, in one undoable step; a
    /// corner (members not aligned) is not joined.
    #[test]
    fn join_the_members_undoes_a_split() {
        let (mut app, _) = frame();
        let mut state = State::default();
        let (src, st) = now(&mut app);
        apply(&mut app, &src, &st, &mut state, Act::Split("2".into(), 3.0));
        let (src, st) = now(&mut app);
        apply(&mut app, &src, &st, &mut state, Act::Join("E".into()));
        let (src, st) = now(&mut app);
        assert!(st.node("E").is_none());
        assert_eq!(st.members.len(), 3);
        assert_eq!(state.sel, [Sel::Member("2".into())]);
        assert_eq!(st.loads.iter().filter(|l| matches!(l, Load::Distributed { member, .. } if member == "2")).count(), 2);
        // A corner of the frame stays.
        apply(&mut app, &src, &st, &mut state, Act::Join("B".into()));
        assert_eq!(now(&mut app).1.members.len(), 3);
        app.run("edit.undo", json!({})).unwrap();
        assert_eq!(now(&mut app).1.members.len(), 4);
    }

    /// Grips: a node dropped on another joins it; a force's tail turns it by 15° steps.
    #[test]
    fn grips_join_nodes_and_turn_forces() {
        let (mut app, v) = frame();
        let mut state = State::default();
        let (src, st) = now(&mut app);
        let k = st.loads.iter().position(|l| matches!(l, Load::Force { .. })).unwrap();
        state.sel = vec![Sel::Load(k), Sel::Member("2".into())];
        let handles = super::super::grips::grips(&st, v, &state.sel);
        assert!(handles.iter().any(|(g, _)| *g == Grip::Tail(k)) && handles.iter().any(|(g, _)| *g == Grip::Node("C".into())), "{handles:?}");
        // The tail pulled straight below the point: the force points up (90°).
        let b = st.node("B").unwrap();
        let below = v.to_screen(b.x.value, b.y.value - 1.0) + vec2(3.0, 0.0);
        super::super::grips::commit(&mut app, &src, &st, v, &mut state, &Grip::Tail(k), below, false);
        let (src, st) = now(&mut app);
        assert!(matches!(&st.loads[k], Load::Force { angle, .. } if angle.value == 90.0), "{:?}", st.loads[k]);
        // B dragged onto C: the beam between them goes, the column A–B now ends at C.
        let c = st.node("C").unwrap();
        let at_c = v.to_screen(c.x.value, c.y.value) + vec2(2.0, 1.0);
        super::super::grips::commit(&mut app, &src, &st, v, &mut state, &Grip::Node("B".into()), at_c, false);
        let (_, st) = now(&mut app);
        assert!(st.node("B").is_none());
        assert_eq!(st.members.len(), 2);
        assert_eq!(st.member("1").map(|m| m.b.as_str()), Some("C"));
        assert!(state.has(&Sel::Node("C".into())));
    }
}
