//! The Structure Editor pane (Structures › Editor, `view.structurePane`, `docs/structures.md`):
//! draws the selected structure on a sheet with a grid and lets it be built with the mouse
//! (nodes, members, supports, hinges, loads; snapping to the grid and to nodes, coordinates typed
//! as in the CAD editor: `x, y` or `@dx, dy`), then edited in the selection's properties and in
//! the tables of nodes, members, supports and loads, whose numbers may be calculation parameters.
//! Every change goes through the `structure.*` commands, so it is undoable and scriptable.

use egui::{Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Shape, Stroke, Ui, pos2, vec2};
use serde_json::{Value, json};
use strix_plot::{PlotAnchor, PlotPrim};
use strix_structure::{Load, Num, Structure, SupportKind};

use crate::WordApp;

/// Screen pixels per paper millimetre on the sheet (the symbols' and texts' sizes).
const PX_PER_MM: f32 = 4.0;
/// How near (px) the pointer snaps to a node or picks something.
const PICK: f32 = 10.0;

/// What a click on the sheet does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Tool {
    #[default]
    Select,
    Node,
    Member,
    Support(SupportKind),
    Hinge,
    Force,
    Couple,
    Distributed,
    Erase,
}

/// Something selected on the sheet.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Sel {
    Node(String),
    Member(String),
    /// A load, by its index.
    Load(usize),
}

/// What a drag with the primary button is doing.
#[derive(Clone, Debug, PartialEq)]
enum Gesture {
    /// Moving the selected nodes (and the ends of the selected members): the world point grabbed
    /// and where it is now.
    Move { from: (f64, f64), to: (f64, f64) },
    /// Sliding a load along its member: its index and where along the member it is now.
    Slide { load: usize, at: f64 },
    /// A selection window, screen corners.
    Window { from: Pos2, to: Pos2 },
}

/// The editor's state between frames.
#[derive(Clone, Debug)]
struct State {
    tool: Tool,
    /// The selection, in the order it was made.
    sel: Vec<Sel>,
    /// The drag under way.
    gesture: Option<Gesture>,
    /// The world point at the sheet's centre and pixels per unit; `None`: fit on the next frame.
    view: Option<(f64, f64, f64)>,
    /// The structure the view was fitted to (a new one fits again).
    fitted_for: String,
    /// A member being drawn: its first point.
    start: Option<(f64, f64)>,
    grid: f64,
    snap: bool,
    /// The coordinates typed.
    typed: String,
}

impl Default for State {
    fn default() -> Self {
        State {
            tool: Tool::Select,
            sel: Vec::new(),
            gesture: None,
            view: None,
            fitted_for: String::new(),
            start: None,
            grid: 0.5,
            snap: true,
            typed: String::new(),
        }
    }
}

impl State {
    /// The one thing selected, when only one is.
    fn primary(&self) -> Option<&Sel> {
        match self.sel.as_slice() {
            [one] => Some(one),
            _ => None,
        }
    }

    fn select(&mut self, s: Sel) {
        self.sel = vec![s];
    }

    fn has(&self, s: &Sel) -> bool {
        self.sel.contains(s)
    }

    /// Adds `s` to the selection, or takes it out (Shift or Ctrl and a click).
    fn toggle(&mut self, s: Sel) {
        match self.sel.iter().position(|x| *x == s) {
            Some(i) => {
                self.sel.remove(i);
            }
            None => self.sel.push(s),
        }
    }

    /// What no longer exists leaves the selection.
    fn prune(&mut self, st: &Structure) {
        self.sel.retain(|s| match s {
            Sel::Node(id) => st.node(id).is_some(),
            Sel::Member(id) => st.member(id).is_some(),
            Sel::Load(k) => *k < st.loads.len(),
        });
        if let Some(Gesture::Slide { load, .. }) = &self.gesture
            && *load >= st.loads.len()
        {
            self.gesture = None;
        }
    }
}

fn state_id() -> egui::Id {
    egui::Id::unique("structure_editor_state")
}

/// `structure.get`, worked out once per document change and selection.
pub(crate) fn info(app: &mut WordApp, ui: &Ui) -> Option<Value> {
    let key = (app.session.document_id(), app.session.rev(), format!("{:?}", app.session.sel));
    let id = egui::Id::unique("structure_editor_info");
    type Cached = (u64, u64, String, Option<Value>);
    if let Some((d, r, s, v)) = ui.data(|d| d.get_temp::<Cached>(id))
        && (d, r, s.as_str()) == (key.0, key.1, key.2.as_str())
    {
        return v;
    }
    let v = app.session.run("structure.get", &json!({})).ok();
    ui.data_mut(|d| d.insert_temp::<Cached>(id, (key.0, key.1, key.2, v.clone())));
    v
}

pub(crate) fn pane(app: &mut WordApp, ui: &mut Ui) {
    let Some(info) = info(app, ui) else {
        empty_state(app, ui);
        return;
    };
    let (Ok(src), Ok(st)) =
        (serde_json::from_value::<Structure>(info["structure"].clone()), serde_json::from_value::<Structure>(info["resolved"].clone()))
    else {
        ui.label(RichText::new(ts("The structure can't be read.")).weak());
        return;
    };
    let mut state = ui.data(|d| d.get_temp::<State>(state_id())).unwrap_or_default();
    state.prune(&st);
    status_line(ui, &info);
    if ui
        .button(ts("Edit in Full Page"))
        .on_hover_text(ts("Open the structure in its editor, in place of the page (or double-click the structure)"))
        .clicked()
    {
        ui.data_mut(|d| d.insert_temp(state_id(), state.clone()));
        let _ = app.run("ui.structureEdit", json!({}));
    }
    toolbar(ui, &mut state);
    let h = (ui.available_height() * 0.45).clamp(220.0, 460.0);
    sheet(app, ui, &src, &st, &mut state, h);
    typed_point(app, ui, &st, &mut state);
    egui::ScrollArea::vertical().id_salt("structure_pane_scroll").show(ui, |ui| {
        if ui
            .button(ts("Write Working"))
            .on_hover_text(ts("Write after the structure its solution as live calculations: the equilibrium equations giving the reactions, then each member's laws N(x), T(x), M(x) with their values"))
            .clicked()
        {
            let _ = app.run("structure.working", working_labels());
        }
        selection(app, ui, &src, &st, &mut state);
        ui.add_space(4.0);
        tables(app, ui, &src, &st, &mut state);
    });
    ui.data_mut(|d| d.insert_temp(state_id(), state));
}

/// The structure editor in place of the page (double-click a structure, Structures › Edit
/// Structure, `ui.structureEdit`): the sheet as large as the window, the selection's properties
/// and the tables at the right. Esc with nothing selected, or Close Editor, goes back to the page.
pub(crate) fn editor(app: &mut WordApp, ui: &mut Ui) {
    let Some(info) = info(app, ui) else {
        // The structure is gone (undone, deleted) or no longer selected.
        close(app);
        return;
    };
    let (Ok(src), Ok(st)) =
        (serde_json::from_value::<Structure>(info["structure"].clone()), serde_json::from_value::<Structure>(info["resolved"].clone()))
    else {
        close(app);
        return;
    };
    let mut state = ui.data(|d| d.get_temp::<State>(state_id())).unwrap_or_default();
    state.prune(&st);
    let full = ui.available_rect_before_wrap();
    let side_w = 360.0_f32.min(full.width() * 0.4);
    let main = Rect::from_min_max(full.min, pos2(full.max.x - side_w, full.max.y));
    let side = Rect::from_min_max(pos2(full.max.x - side_w, full.min.y), full.max);
    let mut close_now = false;
    ui.scope_builder(egui::UiBuilder::new().max_rect(main.shrink(8.0)).layout(egui::Layout::top_down(egui::Align::Min)), |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(ts("Structure Editor")).heading().strong());
            ui.add_space(12.0);
            status_line(ui, &info);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(ts("Close Editor")).on_hover_text(ts("Back to the page (Esc with nothing selected)")).clicked() {
                    close_now = true;
                }
                if ui.button(ts("Write Working")).clicked() {
                    close_now = true;
                    let _ = app.run("structure.working", working_labels());
                }
            });
        });
        toolbar(ui, &mut state);
        let h = ui.available_height() - 34.0;
        sheet(app, ui, &src, &st, &mut state, h);
        typed_point(app, ui, &st, &mut state);
    });
    ui.painter().vline(side.min.x, side.y_range(), Stroke::new(1.0, Color32::from_gray(200)));
    ui.scope_builder(egui::UiBuilder::new().max_rect(side.shrink(8.0)).layout(egui::Layout::top_down(egui::Align::Min)), |ui| {
        egui::ScrollArea::vertical().id_salt("structure_editor_side").show(ui, |ui| {
            selection(app, ui, &src, &st, &mut state);
            ui.add_space(6.0);
            tables(app, ui, &src, &st, &mut state);
        });
    });
    // Esc with nothing to cancel or deselect leaves the editor.
    let idle = state.sel.is_empty() && state.gesture.is_none() && state.start.is_none() && state.tool == Tool::Select;
    let typing = ui.ctx().memory(|m| m.focused().is_some());
    if idle && !typing && ui.input(|i| i.key_pressed(egui::Key::Escape)) && !egui::Popup::is_any_open(ui.ctx()) {
        close_now = true;
    }
    ui.data_mut(|d| d.insert_temp(state_id(), state));
    if close_now {
        close(app);
    }
}

/// Opens the full-page editor on the selected structure.
pub(crate) fn open(app: &mut WordApp) -> Result<(), String> {
    if !wordcraft_engine::cmd::structure::is_selected(&app.session) {
        return Err("select a structure first".into());
    }
    if app.cad.is_some() {
        return Err("close the CAD drawing first".into());
    }
    if app.structure_edit.is_none() {
        app.structure_edit = Some(app.ui.tab.clone());
    }
    app.ui.tab = "Structures".to_string();
    Ok(())
}

/// Back to the page, on the tab that was open.
pub(crate) fn close(app: &mut WordApp) {
    if let Some(tab) = app.structure_edit.take() {
        app.ui.tab = tab;
    }
}

/// UI commands (`ui.structure*`): `ui.structureEdit` opens the full-page editor,
/// `ui.structureClose` closes it.
pub(crate) fn command(app: &mut WordApp, id: &str) -> Result<Value, String> {
    match id {
        "ui.structureEdit" => {
            open(app)?;
            Ok(json!({"open": true}))
        }
        "ui.structureClose" => {
            close(app);
            Ok(json!({"open": false}))
        }
        _ => Err(format!("unknown command {id}")),
    }
}

fn empty_state(app: &mut WordApp, ui: &mut Ui) {
    ui.label(RichText::new(ts("Select a structure in the document to draw and edit it, or insert one.")).weak());
    ui.add_space(8.0);
    for (template, label) in TEMPLATES {
        if ui.button(ts(label)).clicked() {
            let _ = app.run("structure.insert", json!({"template": template, "openPane": true}));
        }
    }
}

/// `structure.working`'s words in the interface's language.
pub(crate) fn working_labels() -> Value {
    json!({"labels": {
        "heading": ts("Working"),
        "reactions": ts("Reactions"),
        "signs": ts("Reactions are positive along x, along y and counter-clockwise: a negative one acts the other way."),
        "hyperstatic": ts("Hyperstatic: from the solution by the stiffness method"),
        "together": ts("From the equations solved together"),
        "forces": ts("Internal forces"),
        "conventions": ts("x from the member's first node; N positive in tension, T positive turning clockwise, M positive stretching the reference fibres."),
        "member": ts("Member"),
        "axial": ts("Axial forces"),
        "hinge": ts("part about the hinge"),
        "range": ts("x from {a} to {b}"),
    }})
}

/// The ready-made structures, as `structure.insert` names them.
pub(crate) const TEMPLATES: [(&str, &str); 5] = [
    ("beam", "Simply Supported Beam"),
    ("cantilever", "Cantilever"),
    ("continuous", "Continuous Beam"),
    ("frame", "Portal Frame"),
    ("truss", "Truss"),
];

fn status_line(ui: &mut Ui, info: &Value) {
    let status = info["status"].as_str().unwrap_or_default();
    let degree = info["degree"].as_u64().unwrap_or(0).to_string();
    let (text, color) = match status {
        "isostatic" => (ts("Isostatic").to_string(), Color32::from_rgb(0x11, 0x7A, 0x65)),
        "hyperstatic" => (crate::i18n::fmt(ts("Hyperstatic, degree {n}"), &[("n", &degree)]), Color32::from_rgb(0x1B, 0x4F, 0x72)),
        "labile" => (crate::i18n::fmt(ts("Labile: {n} degree(s) of freedom"), &[("n", &degree)]), Color32::from_rgb(0xC0, 0x1C, 0x1C)),
        "empty" => (ts("No members yet: draw them with Member").to_string(), Color32::GRAY),
        _ => (info["message"].as_str().unwrap_or_default().to_string(), Color32::from_rgb(0xC0, 0x1C, 0x1C)),
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new(text).strong().color(color));
        if info["truss"].as_bool().unwrap_or(false) {
            ui.label(RichText::new(ts("(truss)")).weak());
        }
    });
    for e in info["errors"].as_array().into_iter().flatten().filter_map(Value::as_str).take(4) {
        ui.label(RichText::new(e).small().color(Color32::from_rgb(0xC0, 0x1C, 0x1C)));
    }
}

fn toolbar(ui: &mut Ui, state: &mut State) {
    let tools: [(Tool, &str, &str); 13] = [
        (Tool::Select, "Select", "Select nodes, members and loads (Shift+click, a window); drag to move them, a load slides along its member"),
        (Tool::Node, "Node", "Click to add a node"),
        (Tool::Member, "Member", "Click two points to draw a member; it goes on from its end (Esc or a right click stops)"),
        (Tool::Support(SupportKind::Fixed), "Fixed", "Click a node to fix it (incastro)"),
        (Tool::Support(SupportKind::Pin), "Pin", "Click a node to pin it (cerniera)"),
        (Tool::Support(SupportKind::Roller), "Roller", "Click a node to put it on a roller (carrello)"),
        (Tool::Support(SupportKind::Link), "Link", "Click a node to hold it with a link (pendolo)"),
        (Tool::Support(SupportKind::Slider), "Slider", "Click a node to hold it with a slider (glifo)"),
        (Tool::Hinge, "Internal hinge", "Click a node to make it an internal hinge, or not"),
        (Tool::Force, "Force", "Click a node or a point of a member to put a force there"),
        (Tool::Couple, "Couple", "Click a node or a point of a member to put a couple there"),
        (Tool::Distributed, "Distributed", "Click a member to load it along its length"),
        (Tool::Erase, "Erase", "Click a node or a member to remove it (a node with what uses it)"),
    ];
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
        for (tool, label, tip) in tools {
            if ui.selectable_label(state.tool == tool, ts(label)).on_hover_text(ts(tip)).clicked() {
                state.tool = tool;
                state.start = None;
            }
        }
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut state.snap, ts("Snap")).on_hover_text(ts("Snap to the grid (always to the nodes)"));
        ui.label(ts("Grid"));
        ui.add(egui::DragValue::new(&mut state.grid).range(0.01..=100.0).speed(0.05).max_decimals(3));
        if ui.button(ts("Fit")).on_hover_text(ts("Show the whole structure")).clicked() {
            state.view = None;
        }
    });
}

/// World ↔ screen on the sheet.
#[derive(Clone, Copy)]
struct View {
    rect: Rect,
    cx: f64,
    cy: f64,
    s: f64,
}

impl View {
    fn to_screen(self, x: f64, y: f64) -> Pos2 {
        let c = self.rect.center();
        pos2(c.x + ((x - self.cx) * self.s) as f32, c.y - ((y - self.cy) * self.s) as f32)
    }

    fn to_world(self, p: Pos2) -> (f64, f64) {
        let c = self.rect.center();
        (self.cx + f64::from(p.x - c.x) / self.s, self.cy - f64::from(p.y - c.y) / self.s)
    }
}

/// The view that shows the whole structure in `rect`.
fn fit(st: &Structure, rect: Rect) -> (f64, f64, f64) {
    let pts: Vec<(f64, f64)> = st.nodes.iter().map(|n| (n.x.value, n.y.value)).collect();
    if pts.is_empty() {
        return (2.0, 1.0, f64::from(rect.width()) / 8.0);
    }
    let (x0, x1) = pts.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
    let (y0, y1) = pts.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
    let (w, h) = ((x1 - x0).max(1e-6), (y1 - y0).max(1e-6));
    let margin = 70.0;
    let sx = (f64::from(rect.width()) - 2.0 * margin).max(20.0) / w;
    let sy = (f64::from(rect.height()) - 2.0 * margin).max(20.0) / h;
    let s = sx.min(sy).clamp(1e-3, 1e6);
    ((x0 + x1) / 2.0, (y0 + y1) / 2.0, s)
}

/// The point the pointer snaps to: a node within reach, else the grid.
fn snap(st: &Structure, v: View, p: Pos2, state: &State) -> ((f64, f64), Option<String>) {
    if let Some(n) = st.nodes.iter().min_by(|a, b| dist(v.to_screen(a.x.value, a.y.value), p).total_cmp(&dist(v.to_screen(b.x.value, b.y.value), p)))
        && dist(v.to_screen(n.x.value, n.y.value), p) <= f64::from(PICK)
    {
        return ((n.x.value, n.y.value), Some(n.id.clone()));
    }
    let (x, y) = v.to_world(p);
    if state.snap && state.grid > 0.0 {
        let g = state.grid;
        return (((x / g).round() * g, (y / g).round() * g), None);
    }
    ((x, y), None)
}

fn dist(a: Pos2, b: Pos2) -> f64 {
    f64::from((a - b).length())
}

/// The member nearest `p` within reach, with the distance along it (units) to the point picked.
fn pick_member(st: &Structure, v: View, p: Pos2) -> Option<(String, f64)> {
    let mut best: Option<(String, f64, f64)> = None;
    for m in &st.members {
        let (Some(a), Some(b)) = (st.node(&m.a), st.node(&m.b)) else { continue };
        let (pa, pb) = (v.to_screen(a.x.value, a.y.value), v.to_screen(b.x.value, b.y.value));
        let ab = pb - pa;
        let len2 = ab.length_sq();
        if len2 <= 0.0 {
            continue;
        }
        let t = ((p - pa).dot(ab) / len2).clamp(0.0, 1.0);
        let d = f64::from((pa + ab * t - p).length());
        if d <= f64::from(PICK) && best.as_ref().is_none_or(|b| d < b.2) {
            let len = st.length(m).unwrap_or(0.0);
            best = Some((m.id.clone(), f64::from(t) * len, d));
        }
    }
    best.map(|(id, at, _)| (id, at))
}

fn pick_node(st: &Structure, v: View, p: Pos2) -> Option<String> {
    st.nodes
        .iter()
        .map(|n| (n, dist(v.to_screen(n.x.value, n.y.value), p)))
        .filter(|(_, d)| *d <= f64::from(PICK))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(n, _)| n.id.clone())
}

/// A position along a member rounded to the grid (when snapping), within it.
fn round_at(at: f64, len: f64, state: &State) -> f64 {
    let r = if state.snap && state.grid > 0.0 { (at / state.grid).round() * state.grid } else { (at * 1000.0).round() / 1000.0 };
    r.clamp(0.0, len)
}

fn sheet(app: &mut WordApp, ui: &mut Ui, src: &Structure, st: &Structure, state: &mut State, h: f32) {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h.max(120.0)), Sense::click_and_drag());
    let key = st.nodes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>().join(",");
    if state.view.is_none() || (state.fitted_for.is_empty() && !key.is_empty()) {
        state.view = Some(fit(st, rect));
        state.fitted_for = key;
    }
    let (cx, cy, s) = state.view.unwrap_or((0.0, 0.0, 50.0));
    let mut v = View { rect, cx, cy, s };
    // Zoom about the pointer, pan with the middle or right button.
    if let Some(p) = resp.hover_pos() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            let before = v.to_world(p);
            v.s = (v.s * f64::from((scroll / 200.0).exp())).clamp(1e-3, 1e6);
            let after = v.to_world(p);
            v.cx += before.0 - after.0;
            v.cy += before.1 - after.1;
        }
    }
    if resp.dragged_by(egui::PointerButton::Middle) || resp.dragged_by(egui::PointerButton::Secondary) {
        let d = resp.drag_delta();
        v.cx -= f64::from(d.x) / v.s;
        v.cy += f64::from(d.y) / v.s;
    }
    state.view = Some((v.cx, v.cy, v.s));
    let mods = ui.input(|i| i.modifiers);
    let adding = mods.shift || mods.command;

    drag(app, ui, &resp, src, st, v, state, adding);
    paint_sheet(ui, &resp, st, v, state);
    if resp.secondary_clicked() {
        state.start = None;
    }
    if resp.clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        if state.tool == Tool::Select {
            match pick(st, v, p) {
                Some(hit) if adding => state.toggle(hit),
                Some(hit) => state.select(hit),
                None if adding => {}
                None => state.sel.clear(),
            }
        } else {
            click(app, st, v, p, state);
        }
    }
    // Keys, while the pointer is over the sheet and no box is being typed in.
    let typing = ui.ctx().memory(|m| m.focused().is_some());
    if (resp.hovered() || resp.has_focus()) && !typing {
        keys(app, ui, src, st, state);
    }
}

/// A drag with the primary button and the Select tool: moves the selection, slides a load along
/// its member, or draws a selection window.
#[allow(clippy::too_many_arguments)]
fn drag(app: &mut WordApp, ui: &Ui, resp: &egui::Response, src: &Structure, st: &Structure, v: View, state: &mut State, adding: bool) {
    // Dragging with Select: move the selection, slide a load, or draw a selection window.
    if state.tool == Tool::Select {
        if resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(p) = resp.interact_pointer_pos()
        {
            // The press, not where the pointer is once the drag is recognised.
            let p = ui.input(|i| i.pointer.press_origin()).unwrap_or(p);
            state.gesture = match pick(st, v, p) {
                Some(Sel::Load(k)) => {
                    if !state.has(&Sel::Load(k)) {
                        state.select(Sel::Load(k));
                    }
                    load_on_member(st, k).map(|(_, at)| Gesture::Slide { load: k, at })
                }
                Some(hit) => {
                    if !state.has(&hit) {
                        if adding {
                            state.sel.push(hit);
                        } else {
                            state.select(hit);
                        }
                    }
                    let w = v.to_world(p);
                    Some(Gesture::Move { from: w, to: w })
                }
                None => {
                    if !adding {
                        state.sel.clear();
                    }
                    Some(Gesture::Window { from: p, to: p })
                }
            };
        }
        if resp.dragged_by(egui::PointerButton::Primary)
            && let Some(p) = resp.interact_pointer_pos()
        {
            let (snap_on, grid) = (state.snap, state.grid);
            match &mut state.gesture {
                Some(Gesture::Move { to, .. }) => *to = v.to_world(p),
                Some(Gesture::Slide { load, at }) => {
                    if let Some((a, len)) = along(st, *load, v, p) {
                        let r = if snap_on && grid > 0.0 { (a / grid).round() * grid } else { (a * 1000.0).round() / 1000.0 };
                        *at = r.clamp(0.0, len);
                    }
                }
                Some(Gesture::Window { to, .. }) => *to = p,
                None => {}
            }
        }
        if resp.drag_stopped_by(egui::PointerButton::Primary)
            && let Some(g) = state.gesture.take()
        {
            match g {
                Gesture::Move { from, to } => {
                    let (dx, dy) = step(from, to, state);
                    if dx != 0.0 || dy != 0.0 {
                        let moved = moved(src, &moved_nodes(st, &state.sel), dx, dy);
                        let _ = app.run("structure.set", json!({"structure": moved}));
                    }
                }
                Gesture::Slide { load, at } => {
                    if let Some(l) = src.loads.get(load) {
                        let mut lj = serde_json::to_value(l).unwrap_or(Value::Null);
                        if lj.is_object() {
                            lj["at"] = json!(round(at));
                            let _ = app.run("structure.load", json!({"index": load, "load": lj}));
                        }
                    }
                }
                Gesture::Window { from, to } => {
                    let r = Rect::from_two_pos(from, to);
                    for s in inside(st, v, r) {
                        if !state.has(&s) {
                            state.sel.push(s);
                        }
                    }
                }
            }
        }
    } else {
        state.gesture = None;
    }
}

/// The sheet: grid, structure (with the drag under way), selection, hover, selection window,
/// the member being drawn and the readouts.
fn paint_sheet(ui: &Ui, resp: &egui::Response, st: &Structure, v: View, state: &State) {
    let rect = v.rect;
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, Color32::WHITE);
    grid_lines(&painter, v, state.grid);
    // The structure as the document draws it, with the drag under way.
    let mut shown = st.clone();
    match &state.gesture {
        Some(Gesture::Move { from, to }) => {
            let (dx, dy) = step(*from, *to, state);
            let ids = moved_nodes(st, &state.sel);
            for n in shown.nodes.iter_mut().filter(|n| ids.contains(&n.id)) {
                n.x = Num::new(n.x.value + dx);
                n.y = Num::new(n.y.value + dy);
            }
        }
        Some(Gesture::Slide { load, at }) => {
            if let Some(Load::Force { at: a, .. } | Load::Moment { at: a, .. }) = shown.loads.get_mut(*load) {
                *a = Num::new(*at);
            }
        }
        _ => {}
    }
    let origin = v.to_world(rect.min);
    let prims = shown.schema_prims(v.s / f64::from(PX_PER_MM), origin, strix_calc::Options::default());
    paint_prims(&painter, rect.min, &prims);
    // The selection, and what the pointer is over.
    let accent = Color32::from_rgb(0xE0, 0x7B, 0x1F);
    let hover = resp.hover_pos();
    let hovered = hover.filter(|_| state.tool == Tool::Select && state.gesture.is_none()).and_then(|p| pick(&shown, v, p));
    if let Some(h) = &hovered
        && !state.has(h)
    {
        highlight(&painter, &shown, v, h, accent.linear_multiply(0.35));
    }
    for s in &state.sel {
        highlight(&painter, &shown, v, s, accent);
    }
    if let Some(Gesture::Window { from, to }) = &state.gesture {
        let r = Rect::from_two_pos(*from, *to);
        painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(0x25, 0x63, 0xEB, 24));
        painter.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::from_rgb(0x25, 0x63, 0xEB)), egui::StrokeKind::Middle);
    }
    let snapped = hover.map(|p| snap(st, v, p, state));
    // The member being drawn.
    if let (Some((x0, y0)), Some(((x, y), _))) = (state.start, &snapped) {
        painter.line_segment([v.to_screen(x0, y0), v.to_screen(*x, *y)], Stroke::new(1.5, accent));
    }
    if let Some(((x, y), node)) = &snapped
        && matches!(state.tool, Tool::Node | Tool::Member)
    {
        let p = v.to_screen(*x, *y);
        let c = if node.is_some() { accent } else { Color32::from_rgb(0x25, 0x63, 0xEB) };
        painter.line_segment([p - vec2(6.0, 0.0), p + vec2(6.0, 0.0)], Stroke::new(1.0, c));
        painter.line_segment([p - vec2(0.0, 6.0), p + vec2(0.0, 6.0)], Stroke::new(1.0, c));
    }
    let unit = st.length_unit.symbol();
    let readout = match &state.gesture {
        Some(Gesture::Move { from, to }) => {
            let (dx, dy) = step(*from, *to, state);
            Some(format!("Δx = {} {unit}   Δy = {} {unit}", short(dx), short(dy)))
        }
        Some(Gesture::Slide { at, .. }) => Some(format!("{} = {} {unit}", ts("at"), short(*at))),
        _ => snapped.as_ref().map(|((x, y), _)| format!("x = {} {unit}   y = {} {unit}", short(*x), short(*y))),
    };
    if let Some(text) = readout {
        painter.text(rect.left_bottom() + vec2(6.0, -6.0), Align2::LEFT_BOTTOM, text, FontId::proportional(11.0), Color32::DARK_GRAY);
    }
    if !state.sel.is_empty() {
        let n = state.sel.len().to_string();
        painter.text(
            rect.right_bottom() + vec2(-6.0, -6.0),
            Align2::RIGHT_BOTTOM,
            crate::i18n::fmt(ts("{n} selected · Del removes · arrows move"), &[("n", &n)]),
            FontId::proportional(11.0),
            Color32::DARK_GRAY,
        );
    }
}

/// Esc, Delete, the arrows and Select All on the sheet.
fn keys(app: &mut WordApp, ui: &Ui, src: &Structure, st: &Structure, state: &mut State) {
    let (esc, del, all, arrows, shift) = ui.input(|i| {
        let a = |k: egui::Key| i.key_pressed(k);
        let dir = (
            f64::from(i32::from(a(egui::Key::ArrowRight)) - i32::from(a(egui::Key::ArrowLeft))),
            f64::from(i32::from(a(egui::Key::ArrowUp)) - i32::from(a(egui::Key::ArrowDown))),
        );
        (a(egui::Key::Escape), a(egui::Key::Delete) || a(egui::Key::Backspace), i.modifiers.command && a(egui::Key::A), dir, i.modifiers.shift)
    });
    if esc {
        if state.gesture.take().is_some() || state.start.take().is_some() {
        } else if state.tool != Tool::Select {
            state.tool = Tool::Select;
        } else {
            state.sel.clear();
        }
    }
    if all {
        state.sel = st.nodes.iter().map(|n| Sel::Node(n.id.clone())).chain(st.members.iter().map(|m| Sel::Member(m.id.clone()))).collect();
    }
    if del && !state.sel.is_empty() {
        let kept = without(src, &state.sel);
        if app.run("structure.set", json!({"structure": kept})).is_ok() {
            state.sel.clear();
        }
    }
    if arrows != (0.0, 0.0) && !state.sel.is_empty() {
        let g = if state.grid > 0.0 { state.grid } else { 0.1 };
        let k = if shift { 10.0 } else { 1.0 };
        let ids = moved_nodes(st, &state.sel);
        if !ids.is_empty() {
            let moved = moved(src, &ids, arrows.0 * g * k, arrows.1 * g * k);
            let _ = app.run("structure.set", json!({"structure": moved}));
        }
    }
}

/// The move from `from` to `to`, on the grid when snapping.
fn step(from: (f64, f64), to: (f64, f64), state: &State) -> (f64, f64) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    if state.snap && state.grid > 0.0 {
        let g = state.grid;
        ((dx / g).round() * g, (dy / g).round() * g)
    } else {
        (round(dx), round(dy))
    }
}

/// The nodes a move takes: those selected and the ends of the selected members.
fn moved_nodes(st: &Structure, sel: &[Sel]) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    let mut push = |id: &str| {
        if !ids.iter().any(|x| x == id) {
            ids.push(id.to_string());
        }
    };
    for s in sel {
        match s {
            Sel::Node(id) => push(id),
            Sel::Member(id) => {
                if let Some(m) = st.member(id) {
                    push(&m.a);
                    push(&m.b);
                }
            }
            Sel::Load(_) => {}
        }
    }
    ids
}

/// A number moved by `d`: an expression stays one (`L` → `L + 0.5`).
fn shifted(n: &Num, d: f64) -> Num {
    if d == 0.0 {
        return n.clone();
    }
    match &n.expr {
        Some(e) => Num::expr(&format!("{e} {} {}", if d < 0.0 { "-" } else { "+" }, short(d.abs()))),
        None => Num::new(round(n.value + d)),
    }
}

/// `src` with nodes `ids` moved by `(dx, dy)`, as `structure.set` takes it.
fn moved(src: &Structure, ids: &[String], dx: f64, dy: f64) -> Value {
    let mut out = src.clone();
    for n in out.nodes.iter_mut().filter(|n| ids.contains(&n.id)) {
        n.x = shifted(&n.x, dx);
        n.y = shifted(&n.y, dy);
    }
    serde_json::to_value(&out).unwrap_or(Value::Null)
}

/// `src` without the selection: a node goes with its members, support and loads, a member with
/// its loads.
fn without(src: &Structure, sel: &[Sel]) -> Value {
    let nodes: Vec<&str> = sel.iter().filter_map(|s| if let Sel::Node(id) = s { Some(id.as_str()) } else { None }).collect();
    let mut members: Vec<String> = sel.iter().filter_map(|s| if let Sel::Member(id) = s { Some(id.clone()) } else { None }).collect();
    members.extend(src.members.iter().filter(|m| nodes.contains(&m.a.as_str()) || nodes.contains(&m.b.as_str())).map(|m| m.id.clone()));
    let loads: Vec<usize> = sel.iter().filter_map(|s| if let Sel::Load(k) = s { Some(*k) } else { None }).collect();
    let mut out = src.clone();
    out.nodes.retain(|n| !nodes.contains(&n.id.as_str()));
    out.members.retain(|m| !members.contains(&m.id));
    out.supports.retain(|s| !nodes.contains(&s.node.as_str()));
    let mut k = 0;
    out.loads.retain(|l| {
        let i = k;
        k += 1;
        let (node, member) = match l {
            Load::Force { node, member, .. } | Load::Moment { node, member, .. } => (node.as_deref(), member.as_deref()),
            Load::Distributed { member, .. } => (None, Some(member.as_str())),
        };
        !loads.contains(&i) && !node.is_some_and(|n| nodes.contains(&n)) && !member.is_some_and(|m| members.iter().any(|x| x == m))
    });
    serde_json::to_value(&out).unwrap_or(Value::Null)
}

/// A load placed on a member at a point (a force or a couple): the member and where.
fn load_on_member(st: &Structure, k: usize) -> Option<(String, f64)> {
    match st.loads.get(k)? {
        Load::Force { member: Some(m), at, .. } | Load::Moment { member: Some(m), at, .. } => Some((m.clone(), at.value)),
        _ => None,
    }
}

/// Where along its member the pointer puts load `k`, and the member's length.
fn along(st: &Structure, k: usize, v: View, p: Pos2) -> Option<(f64, f64)> {
    let (id, _) = load_on_member(st, k)?;
    let m = st.member(&id)?;
    let (a, b) = (st.node(&m.a)?, st.node(&m.b)?);
    let (pa, pb) = (v.to_screen(a.x.value, a.y.value), v.to_screen(b.x.value, b.y.value));
    let ab = pb - pa;
    let len2 = ab.length_sq();
    if len2 <= 0.0 {
        return None;
    }
    let len = st.length(m)?;
    let t = ((p - pa).dot(ab) / len2).clamp(0.0, 1.0);
    Some((f64::from(t) * len, len))
}

/// Screen pixels of a paper millimetre on the sheet's drawing.
const MM: f32 = PX_PER_MM;

/// Where load `k` is drawn on the sheet: its point and its arrow's tail (a force), its point (a
/// couple), or the corners of its band (a distributed load).
fn load_shape(st: &Structure, v: View, k: usize) -> Option<Vec<Pos2>> {
    let at_point = |node: &Option<String>, member: &Option<String>, at: f64| -> Option<Pos2> {
        if let Some(id) = member {
            let m = st.member(id)?;
            let (a, b) = (st.node(&m.a)?, st.node(&m.b)?);
            let len = st.length(m)?;
            let t = if len > 0.0 { (at / len).clamp(0.0, 1.0) } else { 0.0 };
            return Some(v.to_screen(a.x.value + (b.x.value - a.x.value) * t, a.y.value + (b.y.value - a.y.value) * t));
        }
        let n = st.node(node.as_deref()?)?;
        Some(v.to_screen(n.x.value, n.y.value))
    };
    match st.loads.get(k)? {
        Load::Force { node, member, at, value, angle } => {
            let p = at_point(node, member, at.value)?;
            let a = angle.value.to_radians();
            let s = if value.value < 0.0 { -1.0 } else { 1.0 };
            let d = vec2((a.cos() * s) as f32, (-a.sin() * s) as f32);
            Some(vec![p, p - d * 11.0 * MM])
        }
        Load::Moment { node, member, at, .. } => Some(vec![at_point(node, member, at.value)?]),
        Load::Distributed { member, q1, q2, from, to, angle, axes, .. } => {
            let m = st.member(member)?;
            let (a, b) = (st.node(&m.a)?, st.node(&m.b)?);
            let len = st.length(m).filter(|l| *l > 0.0)?;
            let (c, s) = ((b.x.value - a.x.value) / len, (b.y.value - a.y.value) / len);
            let (x0, x1) = (from.value.clamp(0.0, len), to.as_ref().map_or(len, |t| t.value).clamp(0.0, len));
            let th = angle.value.to_radians();
            let g = match axes {
                strix_structure::LoadAxes::Global => (th.cos(), th.sin()),
                strix_structure::LoadAxes::Local => (c * th.cos() - s * th.sin(), s * th.cos() + c * th.sin()),
            };
            let q = if q1.value != 0.0 { q1.value } else { q2.as_ref().map_or(1.0, |q| q.value) };
            let sign = if q < 0.0 { -1.0 } else { 1.0 };
            let off = vec2((-g.0 * sign) as f32, (g.1 * sign) as f32) * 8.0 * MM;
            let p0 = v.to_screen(a.x.value + c * x0, a.y.value + s * x0);
            let p1 = v.to_screen(a.x.value + c * x1, a.y.value + s * x1);
            Some(vec![p0, p1, p1 + off, p0 + off])
        }
    }
}

fn seg_dist(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    let t = if len2 > 0.0 { ((p - a).dot(ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
    (a + ab * t - p).length()
}

/// The load the pointer is on, the last drawn first.
fn pick_load(st: &Structure, v: View, p: Pos2) -> Option<usize> {
    (0..st.loads.len()).rev().find(|&k| match load_shape(st, v, k).as_deref() {
        Some([a, b]) => seg_dist(p, *a, *b) <= PICK && (p - *a).length() > PICK * 0.6,
        Some([c]) => ((p - *c).length() - 4.0 * MM).abs() <= PICK * 0.8,
        Some([p0, p1, p2, p3]) => {
            // Inside the band (with some slack), not on the member itself.
            let poly = [*p0, *p1, *p2, *p3];
            let inside = (0..4).all(|i| {
                let (a, b) = (poly[i], poly[(i + 1) % 4]);
                let cross = (b - a).x * (p - a).y - (b - a).y * (p - a).x;
                let area = (p1.x - p0.x) * (p2.y - p0.y) - (p1.y - p0.y) * (p2.x - p0.x);
                cross * area.signum() >= -PICK * (b - a).length()
            });
            inside && seg_dist(p, *p0, *p1) > PICK * 0.5
        }
        _ => false,
    })
}

/// What the pointer is on: a node, else a load, else a member.
fn pick(st: &Structure, v: View, p: Pos2) -> Option<Sel> {
    pick_node(st, v, p).map(Sel::Node).or_else(|| pick_load(st, v, p).map(Sel::Load)).or_else(|| pick_member(st, v, p).map(|(id, _)| Sel::Member(id)))
}

/// What a selection window takes: the nodes in it, the members with both ends in it, the loads
/// whose point (or band) is in it.
fn inside(st: &Structure, v: View, r: Rect) -> Vec<Sel> {
    let has = |x: f64, y: f64| r.contains(v.to_screen(x, y));
    let mut out: Vec<Sel> = st.nodes.iter().filter(|n| has(n.x.value, n.y.value)).map(|n| Sel::Node(n.id.clone())).collect();
    for m in &st.members {
        if let (Some(a), Some(b)) = (st.node(&m.a), st.node(&m.b))
            && has(a.x.value, a.y.value)
            && has(b.x.value, b.y.value)
        {
            out.push(Sel::Member(m.id.clone()));
        }
    }
    for k in 0..st.loads.len() {
        if load_shape(st, v, k).is_some_and(|pts| pts.first().is_some_and(|p| r.contains(*p)) && pts.iter().all(|p| r.expand(PICK).contains(*p))) {
            out.push(Sel::Load(k));
        }
    }
    out
}

/// Marks `s` on the sheet in `c`.
fn highlight(p: &egui::Painter, st: &Structure, v: View, s: &Sel, c: Color32) {
    match s {
        Sel::Node(id) => {
            if let Some(n) = st.node(id) {
                p.circle_stroke(v.to_screen(n.x.value, n.y.value), 6.0, Stroke::new(2.0, c));
            }
        }
        Sel::Member(id) => {
            if let Some(m) = st.member(id)
                && let (Some(a), Some(b)) = (st.node(&m.a), st.node(&m.b))
            {
                p.line_segment([v.to_screen(a.x.value, a.y.value), v.to_screen(b.x.value, b.y.value)], Stroke::new(5.0, c.linear_multiply(0.6)));
            }
        }
        Sel::Load(k) => match load_shape(st, v, *k).as_deref() {
            Some([a, b]) => {
                p.line_segment([*a, *b], Stroke::new(5.0, c.linear_multiply(0.6)));
            }
            Some([o]) => {
                p.circle_stroke(*o, 4.0 * MM + 2.0, Stroke::new(3.0, c.linear_multiply(0.6)));
            }
            Some(pts) if pts.len() == 4 => {
                p.add(Shape::closed_line(pts.to_vec(), Stroke::new(2.0, c)));
            }
            _ => {}
        },
    }
}

/// A number as typed back: rounded to a micro-unit, no binary noise.
fn round(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}

fn short(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    if r == 0.0 { "0".to_string() } else { format!("{r}") }
}

fn grid_lines(p: &egui::Painter, v: View, grid: f64) {
    if !(grid.is_finite() && grid > 0.0) {
        return;
    }
    let mut g = grid;
    while g * v.s < 8.0 {
        g *= 5.0;
    }
    let (x0, y1) = v.to_world(v.rect.min);
    let (x1, y0) = v.to_world(v.rect.max);
    let faint = Color32::from_gray(232);
    let axis = Color32::from_gray(200);
    let mut x = (x0 / g).floor() * g;
    let mut guard = 0;
    while x <= x1 && guard < 400 {
        let sx = v.to_screen(x, 0.0).x;
        p.line_segment([pos2(sx, v.rect.top()), pos2(sx, v.rect.bottom())], Stroke::new(1.0, if x.abs() < g * 1e-6 { axis } else { faint }));
        x += g;
        guard += 1;
    }
    let mut y = (y0 / g).floor() * g;
    guard = 0;
    while y <= y1 && guard < 400 {
        let sy = v.to_screen(0.0, y).y;
        p.line_segment([pos2(v.rect.left(), sy), pos2(v.rect.right(), sy)], Stroke::new(1.0, if y.abs() < g * 1e-6 { axis } else { faint }));
        y += g;
        guard += 1;
    }
}

/// Draws `strix-plot` primitives (paper mm, y down) with their origin at `at`.
fn paint_prims(p: &egui::Painter, at: Pos2, prims: &[PlotPrim]) {
    let q = |x: f64, y: f64| at + vec2(x as f32 * PX_PER_MM, y as f32 * PX_PER_MM);
    let rgb = |c: [u8; 3]| Color32::from_rgb(c[0], c[1], c[2]);
    let w = |mm: f64| (mm as f32 * PX_PER_MM).max(1.0);
    for prim in prims {
        match prim {
            PlotPrim::Path { pts, closed, stroke, fill, dash } => {
                let points: Vec<Pos2> = pts.iter().map(|&(x, y)| q(x, y)).collect();
                if points.len() < 2 {
                    continue;
                }
                if let (Some(f), true) = (fill, *closed) {
                    let stroke = stroke.map_or(Stroke::NONE, |(c, sw)| Stroke::new(w(sw), rgb(c)));
                    p.add(Shape::convex_polygon(points, rgb(*f), stroke));
                } else if let Some((c, sw)) = stroke {
                    let st = Stroke::new(w(*sw), rgb(*c));
                    if dash.is_empty() {
                        p.add(if *closed { Shape::closed_line(points, st) } else { Shape::line(points, st) });
                    } else {
                        p.extend(Shape::dashed_line(&points, st, 5.0, 3.0));
                    }
                }
            }
            PlotPrim::Dot { at: (x, y), r, fill, stroke } => {
                let c = q(*x, *y);
                p.circle(c, (*r as f32 * PX_PER_MM).max(1.5), rgb(*fill), stroke.map_or(Stroke::NONE, |(sc, sw)| Stroke::new(w(sw), rgb(sc))));
            }
            PlotPrim::Rect { rect: [x, y, rw, rh], fill, stroke } => {
                let r = Rect::from_min_max(q(*x, *y), q(x + rw, y + rh));
                if let Some(f) = fill {
                    p.rect_filled(r, 0.0, rgb(*f));
                }
                if let Some((c, sw)) = stroke {
                    p.rect_stroke(r, 0.0, Stroke::new(w(*sw), rgb(*c)), egui::StrokeKind::Middle);
                }
            }
            PlotPrim::TextAt { at: (x, y), text, height, angle, color } => {
                let size = (*height as f32 * PX_PER_MM / 0.7).clamp(6.0, 40.0);
                let galley = p.layout_no_wrap(text.clone(), FontId::proportional(size), rgb(*color));
                let rot = -(*angle as f32).to_radians();
                let rot = if rot.is_finite() { rot } else { 0.0 };
                // The galley's top left so that its centre lands on `at` once turned.
                let half = galley.size() * 0.5;
                let (s, c) = rot.sin_cos();
                let off = vec2(c * half.x - s * half.y, s * half.x + c * half.y);
                p.add(egui::epaint::TextShape::new(q(*x, *y) - off, galley, rgb(*color)).with_angle(rot));
            }
            PlotPrim::Text { at: (x, y), text, height, bold: _, anchor, color } => {
                let size = (*height as f32 * PX_PER_MM / 0.7).clamp(6.0, 40.0);
                let align = match anchor {
                    PlotAnchor::Left => Align2::LEFT_BOTTOM,
                    PlotAnchor::Center => Align2::CENTER_BOTTOM,
                    PlotAnchor::Right => Align2::RIGHT_BOTTOM,
                    PlotAnchor::TopCenter => Align2::CENTER_TOP,
                    PlotAnchor::MiddleRight => Align2::RIGHT_CENTER,
                };
                // The baseline is at `at`; a bottom-aligned galley ends a little below it.
                p.text(q(*x, *y) + vec2(0.0, size * 0.22), align, text, FontId::proportional(size), rgb(*color));
            }
        }
    }
}

/// A node at `(x, y)`: the one there, or a new one. Returns its name.
fn node_at(app: &mut WordApp, st: &Structure, (x, y): (f64, f64)) -> Option<String> {
    let close = |n: &strix_structure::Node| (n.x.value - x).abs() < 1e-9 && (n.y.value - y).abs() < 1e-9;
    if let Some(n) = st.nodes.iter().find(|n| close(n)) {
        return Some(n.id.clone());
    }
    let id = st.next_node_id();
    app.run("structure.node", json!({"id": id, "x": round(x), "y": round(y)})).ok()?;
    Some(id)
}

/// A click on the sheet with the current tool.
fn click(app: &mut WordApp, st: &Structure, v: View, p: Pos2, state: &mut State) {
    let (point, on_node) = snap(st, v, p, state);
    match state.tool {
        Tool::Select => match pick(st, v, p) {
            Some(hit) => state.select(hit),
            None => state.sel.clear(),
        },
        Tool::Node => {
            if on_node.is_none()
                && let Some(id) = node_at(app, st, point)
            {
                state.select(Sel::Node(id));
            }
        }
        Tool::Member => member_point(app, st, point, state),
        Tool::Support(kind) => {
            if let Some(id) = pick_node(st, v, p) {
                let _ = app.run("structure.support", json!({"node": id, "kind": kind.name()}));
                state.select(Sel::Node(id));
            }
        }
        Tool::Hinge => {
            if let Some(id) = pick_node(st, v, p) {
                let on = st.node(&id).is_some_and(|n| n.hinge);
                let _ = app.run("structure.node", json!({"id": id, "hinge": !on}));
            }
        }
        Tool::Force | Tool::Couple => {
            let kind = if state.tool == Tool::Force { "force" } else { "moment" };
            let value = 10.0;
            if let Some(id) = pick_node(st, v, p) {
                let _ = app.run("structure.load", json!({"load": {"type": kind, "node": id, "value": value}}));
            } else if let Some((id, at)) = pick_member(st, v, p) {
                let len = st.member(&id).and_then(|m| st.length(m)).unwrap_or(0.0);
                let _ =
                    app.run("structure.load", json!({"load": {"type": kind, "member": id, "at": round(round_at(at, len, state)), "value": value}}));
            }
        }
        Tool::Distributed => {
            if let Some((id, _)) = pick_member(st, v, p) {
                let _ = app.run("structure.load", json!({"load": {"type": "distributed", "member": id, "q1": 10.0}}));
            }
        }
        Tool::Erase => {
            if let Some(id) = pick_node(st, v, p) {
                let _ = app.run("structure.node", json!({"id": id, "remove": true}));
            } else if let Some((id, _)) = pick_member(st, v, p) {
                let _ = app.run("structure.member", json!({"id": id, "remove": true}));
            }
        }
    }
}

/// A point of the member being drawn: its first, or its last (then the next one starts there).
fn member_point(app: &mut WordApp, st: &Structure, point: (f64, f64), state: &mut State) {
    match state.start {
        None => state.start = Some(point),
        Some(start) if (start.0 - point.0).abs() < 1e-9 && (start.1 - point.1).abs() < 1e-9 => {}
        Some(start) => {
            let Some(a) = node_at(app, st, start) else { return };
            // The structure changed if a node was added: read it again for the second one.
            let now = app.session.run("structure.get", &json!({})).ok().and_then(|i| serde_json::from_value::<Structure>(i["resolved"].clone()).ok());
            let Some(b) = node_at(app, now.as_ref().unwrap_or(st), point) else { return };
            if app.run("structure.member", json!({"a": a, "b": b})).is_ok() {
                state.start = Some(point);
            }
        }
    }
}

/// The box for typed coordinates: `x, y` (or `x; y` with decimal commas) or `@dx, dy` from the
/// last point.
fn typed_point(app: &mut WordApp, ui: &mut Ui, st: &Structure, state: &mut State) {
    ui.horizontal(|ui| {
        ui.label(ts("Point"));
        let r = ui.add(egui::TextEdit::singleline(&mut state.typed).desired_width(140.0).hint_text("x, y  ·  @dx, dy"));
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.label(RichText::new(ts("Node or Member tool: Enter places the point")).small().weak());
        if !enter {
            return;
        }
        let text = state.typed.trim().to_string();
        let (relative, body) = match text.strip_prefix('@') {
            Some(rest) => (true, rest.to_string()),
            None => (false, text.clone()),
        };
        let parts: Vec<String> = if body.contains(';') {
            body.split(';').map(|s| s.trim().replace(',', ".")).collect()
        } else {
            body.split(',').map(|s| s.trim().to_string()).collect()
        };
        let nums: Vec<f64> = parts.iter().filter_map(|s| s.parse::<f64>().ok()).filter(|v| v.is_finite()).collect();
        let [x, y] = nums.as_slice() else { return };
        let base = state.start.or_else(|| match state.primary() {
            Some(Sel::Node(id)) => st.node(id).map(|n| (n.x.value, n.y.value)),
            _ => None,
        });
        let point = if relative { base.map_or((*x, *y), |(bx, by)| (bx + x, by + y)) } else { (*x, *y) };
        match state.tool {
            Tool::Member => member_point(app, st, point, state),
            _ => {
                state.tool = if state.tool == Tool::Member { Tool::Member } else { Tool::Node };
                if let Some(id) = node_at(app, st, point) {
                    state.select(Sel::Node(id));
                }
            }
        }
        state.typed.clear();
        r.request_focus();
    });
}

/// A number's text as shown in a box: its expression, or its value.
fn num_text(n: &Num) -> String {
    match &n.expr {
        Some(e) => e.clone(),
        None => short(n.value),
    }
}

/// A typed number: a plain number (a decimal comma too) or an expression.
fn num_json(text: &str) -> Value {
    let t = text.trim();
    match t.replace(',', ".").parse::<f64>() {
        Ok(v) if v.is_finite() => json!(v),
        _ => json!(t),
    }
}

/// A box committed when it loses focus or on Enter; returns the new text when it changed.
fn edit_box(ui: &mut Ui, key: impl std::hash::Hash + std::fmt::Debug, current: &str, width: f32) -> Option<String> {
    let id = egui::Id::unique(("structure_box", key));
    let mut text = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| current.to_string());
    let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(width));
    if r.changed() {
        ui.data_mut(|d| d.insert_temp(id, text.clone()));
    }
    if r.lost_focus() {
        ui.data_mut(|d| d.remove::<String>(id));
        return (text != current).then_some(text);
    }
    if !r.has_focus() {
        ui.data_mut(|d| d.remove::<String>(id));
    }
    None
}

const KINDS: [(Option<SupportKind>, &str); 6] = [
    (None, "None"),
    (Some(SupportKind::Fixed), "Fixed"),
    (Some(SupportKind::Pin), "Pin"),
    (Some(SupportKind::Roller), "Roller"),
    (Some(SupportKind::Link), "Link"),
    (Some(SupportKind::Slider), "Slider"),
];

fn kind_label(k: Option<SupportKind>) -> &'static str {
    KINDS.iter().find(|(x, _)| *x == k).map_or("None", |(_, l)| l)
}

/// The selected node's or member's properties.
fn selection(app: &mut WordApp, ui: &mut Ui, src: &Structure, st: &Structure, state: &mut State) {
    if state.sel.len() > 1 {
        let n = state.sel.len().to_string();
        ui.label(RichText::new(crate::i18n::fmt(ts("{n} items selected"), &[("n", &n)])).strong());
        ui.label(
            RichText::new(ts(
                "Drag one of them to move them together; the arrow keys move them by the grid (Shift: ten times); Delete removes them.",
            ))
            .small()
            .weak(),
        );
        if ui.button(ts("Remove Selected")).clicked() {
            let kept = without(src, &state.sel);
            if app.run("structure.set", json!({"structure": kept})).is_ok() {
                state.sel.clear();
            }
        }
        return;
    }
    match state.primary().cloned() {
        Some(Sel::Node(id)) => {
            let Some(n) = src.node(&id) else { return };
            ui.label(RichText::new(crate::i18n::fmt(ts("Node {n}"), &[("n", &id)])).strong());
            egui::Grid::new("structure_node_props").num_columns(2).spacing(vec2(8.0, 4.0)).show(ui, |ui| {
                ui.label(ts("Name"));
                if let Some(t) = edit_box(ui, ("node_name", &id), &id, 80.0) {
                    let _ = app.run("structure.node", json!({"id": id, "rename": t}));
                    state.select(Sel::Node(t.trim().to_string()));
                }
                ui.end_row();
                for (axis, value) in [("x", &n.x), ("y", &n.y)] {
                    ui.label(axis);
                    if let Some(t) = edit_box(ui, ("node", &id, axis), &num_text(value), 120.0) {
                        let _ = app.run("structure.node", json!({"id": id, axis: num_json(&t)}));
                    }
                    ui.end_row();
                }
                ui.label(ts("Hinge"));
                let mut hinge = n.hinge;
                if ui.checkbox(&mut hinge, ts("Internal hinge")).changed() {
                    let _ = app.run("structure.node", json!({"id": id, "hinge": hinge}));
                }
                ui.end_row();
                let support = src.supports.iter().find(|s| s.node == id);
                ui.label(ts("Support"));
                let current = support.map(|s| s.kind);
                craft_ui::ComboBox::from_id_salt(("structure_support_kind", &id)).selected_text(ts(kind_label(current))).show_ui(ui, |ui| {
                    for (k, label) in KINDS {
                        if ui.selectable_label(current == k, ts(label)).clicked() {
                            let _ = match k {
                                Some(k) => app.run("structure.support", json!({"node": id, "kind": k.name()})),
                                None => app.run("structure.support", json!({"node": id, "remove": true})),
                            };
                        }
                    }
                });
                ui.end_row();
                if let Some(s) = support {
                    ui.label(ts("Angle (°)"));
                    if let Some(t) = edit_box(ui, ("support_angle", &id), &num_text(&s.angle), 80.0) {
                        let _ = app.run("structure.support", json!({"node": id, "angle": num_json(&t)}));
                    }
                    ui.end_row();
                }
            });
            if ui.button(ts("Remove Node")).clicked() {
                let _ = app.run("structure.node", json!({"id": id, "remove": true}));
                state.sel.clear();
            }
        }
        Some(Sel::Member(id)) => {
            let Some(m) = src.member(&id) else { return };
            let len = st.member(&id).and_then(|m| st.length(m)).unwrap_or(0.0);
            ui.label(RichText::new(crate::i18n::fmt(ts("Member {n}"), &[("n", &id)])).strong());
            ui.label(RichText::new(format!("{} → {}   L = {} {}", m.a, m.b, short(len), st.length_unit.symbol())).weak());
            egui::Grid::new("structure_member_props").num_columns(2).spacing(vec2(8.0, 4.0)).show(ui, |ui| {
                ui.label(ts("Hinges"));
                ui.horizontal(|ui| {
                    let (mut ha, mut hb) = (m.hinge_a, m.hinge_b);
                    if ui.checkbox(&mut ha, crate::i18n::fmt(ts("at {n}"), &[("n", &m.a)])).changed() {
                        let _ = app.run("structure.member", json!({"id": id, "hingeA": ha}));
                    }
                    if ui.checkbox(&mut hb, crate::i18n::fmt(ts("at {n}"), &[("n", &m.b)])).changed() {
                        let _ = app.run("structure.member", json!({"id": id, "hingeB": hb}));
                    }
                });
                ui.end_row();
                for (key, label, value) in [("e", "E", &m.e), ("i", "I", &m.i), ("area", "A", &m.area)] {
                    ui.label(label);
                    let text = value.as_ref().map(num_text).unwrap_or_default();
                    if let Some(t) = edit_box(ui, ("member", &id, key), &text, 120.0) {
                        let v = if t.trim().is_empty() { Value::Null } else { num_json(&t) };
                        let _ = app.run("structure.member", json!({"id": id, key: v}));
                    }
                    ui.end_row();
                }
            });
            ui.label(RichText::new(ts("Without E, I, A: EI is the same for every member and members do not stretch.")).small().weak());
            if ui.button(ts("Remove Member")).clicked() {
                let _ = app.run("structure.member", json!({"id": id, "remove": true}));
                state.sel.clear();
            }
        }
        Some(Sel::Load(k)) => {
            loads_table(app, ui, src, st, Some(k));
        }
        None => {
            ui.label(
                RichText::new(ts("Click a node, a member or a load to see its properties; Shift+click or a window adds to the selection.")).weak(),
            );
        }
    }
}

fn tables(app: &mut WordApp, ui: &mut Ui, src: &Structure, st: &Structure, state: &mut State) {
    egui::CollapsingHeader::new(RichText::new(ts("Nodes")).strong())
        .id_salt("structure_nodes")
        .default_open(false)
        .show(ui, |ui| nodes_table(app, ui, src, state));
    egui::CollapsingHeader::new(RichText::new(ts("Members")).strong())
        .id_salt("structure_members")
        .default_open(false)
        .show(ui, |ui| members_table(app, ui, src, state));
    egui::CollapsingHeader::new(RichText::new(ts("Supports")).strong())
        .id_salt("structure_supports")
        .default_open(false)
        .show(ui, |ui| supports_table(app, ui, src));
    egui::CollapsingHeader::new(RichText::new(ts("Loads")).strong())
        .id_salt("structure_loads")
        .default_open(true)
        .show(ui, |ui| loads_table(app, ui, src, st, None));
}

fn remove_button(ui: &mut Ui, tip: &str) -> bool {
    ui.small_button("×").on_hover_text(ts(tip)).clicked()
}

fn nodes_table(app: &mut WordApp, ui: &mut Ui, src: &Structure, state: &mut State) {
    egui::Grid::new("structure_nodes_grid").num_columns(5).striped(true).spacing(vec2(6.0, 3.0)).show(ui, |ui| {
        for h in ["", "x", "y", "Hinge", ""] {
            ui.label(RichText::new(ts(h)).small().strong());
        }
        ui.end_row();
        for n in &src.nodes {
            if ui.selectable_label(state.has(&Sel::Node(n.id.clone())), RichText::new(&n.id).strong()).clicked() {
                state.select(Sel::Node(n.id.clone()));
            }
            for (axis, value) in [("x", &n.x), ("y", &n.y)] {
                if let Some(t) = edit_box(ui, ("tnode", &n.id, axis), &num_text(value), 70.0) {
                    let _ = app.run("structure.node", json!({"id": n.id, axis: num_json(&t)}));
                }
            }
            let mut hinge = n.hinge;
            if ui.checkbox(&mut hinge, "").changed() {
                let _ = app.run("structure.node", json!({"id": n.id, "hinge": hinge}));
            }
            if remove_button(ui, "Remove the node with its members, support and loads") {
                let _ = app.run("structure.node", json!({"id": n.id, "remove": true}));
            }
            ui.end_row();
        }
    });
    if ui.button(ts("Add Node")).clicked() {
        let x = src.nodes.iter().map(|n| n.x.value).fold(0.0_f64, f64::max) + 1.0;
        let _ = app.run("structure.node", json!({"x": round(x), "y": 0.0}));
    }
}

fn members_table(app: &mut WordApp, ui: &mut Ui, src: &Structure, state: &mut State) {
    egui::Grid::new("structure_members_grid").num_columns(8).striped(true).spacing(vec2(6.0, 3.0)).show(ui, |ui| {
        for h in ["", "From", "To", "Hinges", "E", "I", "A", ""] {
            ui.label(RichText::new(ts(h)).small().strong());
        }
        ui.end_row();
        for m in &src.members {
            if ui.selectable_label(state.has(&Sel::Member(m.id.clone())), RichText::new(&m.id).strong()).clicked() {
                state.select(Sel::Member(m.id.clone()));
            }
            for (key, current) in [("a", &m.a), ("b", &m.b)] {
                craft_ui::ComboBox::from_id_salt(("structure_member_end", &m.id, key)).width(44.0).selected_text(current.as_str()).show_ui(
                    ui,
                    |ui| {
                        for n in &src.nodes {
                            if ui.selectable_label(*current == n.id, &n.id).clicked() && *current != n.id {
                                let _ = app.run("structure.member", json!({"id": m.id, key: n.id}));
                            }
                        }
                    },
                );
            }
            ui.horizontal(|ui| {
                let (mut ha, mut hb) = (m.hinge_a, m.hinge_b);
                if ui.checkbox(&mut ha, "").on_hover_text(ts("Hinge at its first node")).changed() {
                    let _ = app.run("structure.member", json!({"id": m.id, "hingeA": ha}));
                }
                if ui.checkbox(&mut hb, "").on_hover_text(ts("Hinge at its second node")).changed() {
                    let _ = app.run("structure.member", json!({"id": m.id, "hingeB": hb}));
                }
            });
            for (key, value) in [("e", &m.e), ("i", &m.i), ("area", &m.area)] {
                let text = value.as_ref().map(num_text).unwrap_or_default();
                if let Some(t) = edit_box(ui, ("tmember", &m.id, key), &text, 54.0) {
                    let v = if t.trim().is_empty() { Value::Null } else { num_json(&t) };
                    let _ = app.run("structure.member", json!({"id": m.id, key: v}));
                }
            }
            if remove_button(ui, "Remove the member with its loads") {
                let _ = app.run("structure.member", json!({"id": m.id, "remove": true}));
            }
            ui.end_row();
        }
    });
    // A new member: between the two nodes chosen.
    let id = egui::Id::unique("structure_new_member");
    let (mut a, mut b) = ui.data(|d| d.get_temp::<(String, String)>(id)).unwrap_or_default();
    ui.horizontal(|ui| {
        for (which, value) in [("new_a", &mut a), ("new_b", &mut b)] {
            craft_ui::ComboBox::from_id_salt(("structure_new_member", which)).width(50.0).selected_text(value.as_str()).show_ui(ui, |ui| {
                for n in &src.nodes {
                    ui.selectable_value(value, n.id.clone(), &n.id);
                }
            });
        }
        if ui.add_enabled(!a.is_empty() && !b.is_empty() && a != b, egui::Button::new(ts("Add Member"))).clicked() {
            let _ = app.run("structure.member", json!({"a": a, "b": b}));
        }
    });
    ui.data_mut(|d| d.insert_temp(id, (a, b)));
}

fn supports_table(app: &mut WordApp, ui: &mut Ui, src: &Structure) {
    egui::Grid::new("structure_supports_grid").num_columns(4).striped(true).spacing(vec2(6.0, 3.0)).show(ui, |ui| {
        for h in ["Node", "Support", "Angle (°)", ""] {
            ui.label(RichText::new(ts(h)).small().strong());
        }
        ui.end_row();
        for s in &src.supports {
            ui.label(RichText::new(&s.node).strong());
            craft_ui::ComboBox::from_id_salt(("structure_support_row", &s.node)).selected_text(ts(kind_label(Some(s.kind)))).show_ui(ui, |ui| {
                for (k, label) in KINDS.iter().skip(1) {
                    if ui.selectable_label(Some(s.kind) == *k, ts(label)).clicked()
                        && let Some(k) = k
                    {
                        let _ = app.run("structure.support", json!({"node": s.node, "kind": k.name()}));
                    }
                }
            });
            if let Some(t) = edit_box(ui, ("tsupport", &s.node), &num_text(&s.angle), 60.0) {
                let _ = app.run("structure.support", json!({"node": s.node, "angle": num_json(&t)}));
            }
            if remove_button(ui, "Remove the support") {
                let _ = app.run("structure.support", json!({"node": s.node, "remove": true}));
            }
            ui.end_row();
        }
    });
    ui.label(
        RichText::new(ts(
            "Angle: the support turned counter-clockwise; 0 stands on the ground, 90 on a wall to the right, −90 on a wall to the left.",
        ))
        .small()
        .weak(),
    );
}

/// The loads' editors (`only`: just that load's, without the buttons adding loads).
fn loads_table(app: &mut WordApp, ui: &mut Ui, src: &Structure, st: &Structure, only: Option<usize>) {
    let fu = st.force_unit.symbol();
    let lu = st.length_unit.symbol();
    for (k, l) in src.loads.iter().enumerate() {
        if only.is_some_and(|o| o != k) {
            continue;
        }
        let mut lj = serde_json::to_value(l).unwrap_or(Value::Null);
        let mut changed = false;
        let mut remove = false;
        egui::Frame::group(ui.style()).inner_margin(4.0).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let title = match l {
                    Load::Force { .. } => ts("Force"),
                    Load::Moment { .. } => ts("Couple"),
                    Load::Distributed { .. } => ts("Distributed"),
                };
                ui.label(RichText::new(format!("{} {}", k + 1, title)).strong());
                // Where: a node or a member.
                match l {
                    Load::Force { node, member, at, .. } | Load::Moment { node, member, at, .. } => {
                        let place = node
                            .clone()
                            .map(|n| format!("{} {n}", ts("node")))
                            .or_else(|| member.clone().map(|m| format!("{} {m}", ts("member"))))
                            .unwrap_or_default();
                        craft_ui::ComboBox::from_id_salt(("structure_load_place", k)).selected_text(place).show_ui(ui, |ui| {
                            for n in &src.nodes {
                                if ui.selectable_label(node.as_deref() == Some(&n.id), format!("{} {}", ts("node"), n.id)).clicked() {
                                    lj["node"] = json!(n.id);
                                    lj.as_object_mut().map(|o| o.remove("member"));
                                    lj.as_object_mut().map(|o| o.remove("at"));
                                    changed = true;
                                }
                            }
                            for m in &src.members {
                                if ui.selectable_label(member.as_deref() == Some(&m.id), format!("{} {}", ts("member"), m.id)).clicked() {
                                    lj["member"] = json!(m.id);
                                    lj.as_object_mut().map(|o| o.remove("node"));
                                    changed = true;
                                }
                            }
                        });
                        if member.is_some() {
                            ui.label(ts("at"));
                            if let Some(t) = edit_box(ui, ("load_at", k), &num_text(at), 46.0) {
                                lj["at"] = num_json(&t);
                                changed = true;
                            }
                            ui.label(lu);
                        }
                    }
                    Load::Distributed { member, .. } => {
                        craft_ui::ComboBox::from_id_salt(("structure_load_member", k)).selected_text(format!("{} {member}", ts("member"))).show_ui(
                            ui,
                            |ui| {
                                for m in &src.members {
                                    if ui.selectable_label(*member == m.id, format!("{} {}", ts("member"), m.id)).clicked() {
                                        lj["member"] = json!(m.id);
                                        changed = true;
                                    }
                                }
                            },
                        );
                    }
                }
                if ui.small_button("×").on_hover_text(ts("Remove the load")).clicked() {
                    remove = true;
                }
            });
            ui.horizontal_wrapped(|ui| match l {
                Load::Force { value, angle, .. } => {
                    ui.label("F");
                    if let Some(t) = edit_box(ui, ("load_value", k), &num_text(value), 60.0) {
                        lj["value"] = num_json(&t);
                        changed = true;
                    }
                    ui.label(fu);
                    ui.label(ts("direction (°)"));
                    if let Some(t) = edit_box(ui, ("load_angle", k), &num_text(angle), 46.0) {
                        lj["angle"] = num_json(&t);
                        changed = true;
                    }
                }
                Load::Moment { value, .. } => {
                    ui.label("C");
                    if let Some(t) = edit_box(ui, ("load_value", k), &num_text(value), 60.0) {
                        lj["value"] = num_json(&t);
                        changed = true;
                    }
                    ui.label(format!("{fu}·{lu}"));
                    ui.label(RichText::new(ts("counter-clockwise positive")).small().weak());
                }
                Load::Distributed { q1, q2, from, to, angle, axes, per, .. } => {
                    ui.label("q1");
                    if let Some(t) = edit_box(ui, ("load_q1", k), &num_text(q1), 50.0) {
                        lj["q1"] = num_json(&t);
                        changed = true;
                    }
                    ui.label("q2");
                    let q2_text = q2.as_ref().map(num_text).unwrap_or_default();
                    if let Some(t) = edit_box(ui, ("load_q2", k), &q2_text, 50.0) {
                        lj["q2"] = if t.trim().is_empty() { Value::Null } else { num_json(&t) };
                        changed = true;
                    }
                    ui.label(format!("{fu}/{lu}"));
                    ui.label(ts("from"));
                    if let Some(t) = edit_box(ui, ("load_from", k), &num_text(from), 40.0) {
                        lj["from"] = num_json(&t);
                        changed = true;
                    }
                    ui.label(ts("to"));
                    let to_text = to.as_ref().map(num_text).unwrap_or_default();
                    if let Some(t) = edit_box(ui, ("load_to", k), &to_text, 40.0) {
                        lj["to"] = if t.trim().is_empty() { Value::Null } else { num_json(&t) };
                        changed = true;
                    }
                    ui.label(ts("direction (°)"));
                    if let Some(t) = edit_box(ui, ("load_angle", k), &num_text(angle), 40.0) {
                        lj["angle"] = num_json(&t);
                        changed = true;
                    }
                    let local = *axes == strix_structure::LoadAxes::Local;
                    let mut l2 = local;
                    if ui
                        .checkbox(&mut l2, ts("from the member"))
                        .on_hover_text(ts(
                            "The direction measured from the member's axis (−90: towards the reference fibres), not from the horizontal",
                        ))
                        .changed()
                    {
                        lj["axes"] = json!(if l2 { "local" } else { "global" });
                        changed = true;
                    }
                    let proj = *per == strix_structure::LoadPer::Projection;
                    let mut p2 = proj;
                    if ui
                        .checkbox(&mut p2, ts("per projection"))
                        .on_hover_text(ts(
                            "The intensity per unit of the member's projection across the load (snow on a roof), not per unit of its length",
                        ))
                        .changed()
                    {
                        lj["per"] = json!(if p2 { "projection" } else { "length" });
                        changed = true;
                    }
                }
            });
        });
        if remove {
            let _ = app.run("structure.load", json!({"index": k, "remove": true}));
            return;
        }
        if changed {
            // `null` means "absent" for optional numbers.
            if let Some(o) = lj.as_object_mut() {
                o.retain(|_, v| !v.is_null());
            }
            let _ = app.run("structure.load", json!({"index": k, "load": lj}));
            return;
        }
    }
    if only.is_some() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        let first_node = src.nodes.first().map(|n| n.id.clone());
        let first_member = src.members.first().map(|m| m.id.clone());
        if ui.add_enabled(first_node.is_some(), egui::Button::new(ts("Add Force"))).clicked() {
            let _ = app.run("structure.load", json!({"load": {"type": "force", "node": first_node, "value": 10.0}}));
        }
        if ui.add_enabled(first_node.is_some(), egui::Button::new(ts("Add Couple"))).clicked() {
            let _ = app.run("structure.load", json!({"load": {"type": "moment", "node": first_node, "value": 10.0}}));
        }
        if ui.add_enabled(first_member.is_some(), egui::Button::new(ts("Add Distributed Load"))).clicked() {
            let _ = app.run("structure.load", json!({"load": {"type": "distributed", "member": first_member, "q1": 10.0}}));
        }
    });
    ui.label(
        RichText::new(ts(
            "Numbers may be calculation parameters or expressions (L/2, q). Direction: degrees from the horizontal, counter-clockwise; −90 is down.",
        ))
        .small()
        .weak(),
    );
}

/// `s` in the interface's language as the structure editor words it.
fn ts(s: &str) -> &str {
    crate::i18n::tc("structure", s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable;
    use wordcraft_engine::Session;

    fn resolved(app: &mut WordApp) -> Structure {
        let info = app.session.run("structure.get", &json!({})).unwrap();
        serde_json::from_value(info["resolved"].clone()).unwrap()
    }

    /// The empty pane inserts a structure; the pane then shows it and its classification.
    #[test]
    fn the_pane_inserts_and_shows_a_structure() {
        let app = WordApp::new(Session::new(wordcraft_doc::Document::new()), crate::Services::default());
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(440.0, 1400.0)).build_ui_state(|ui, app: &mut WordApp| pane(app, ui), app);
        h.run_steps(2);
        h.get_by_label(ts("Simply Supported Beam")).click();
        h.run_steps(3);
        assert!(h.state().session.view.structure_pane);
        h.get_by_label(ts("Isostatic"));
        h.get_by_label(ts("Loads"));
    }

    /// The selection: a window takes what is inside it, a group moves together keeping
    /// expressions, the arrows move it by the grid, Delete removes it with what uses it, and a
    /// load on a member is picked before the member.
    #[test]
    fn the_selection_moves_and_removes_together() {
        let mut app = WordApp::new(Session::new(wordcraft_doc::Document::new()), crate::Services::default());
        app.run("structure.insert", json!({"template": "frame"})).unwrap();
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(600.0, 500.0));
        let st = resolved(&mut app);
        let (cx, cy, s) = fit(&st, rect);
        let v = View { rect, cx, cy, s };
        // The window round the top of the frame takes B, C and the beam between them.
        let (b, c) = (st.node("B").unwrap(), st.node("C").unwrap());
        let r = Rect::from_two_pos(v.to_screen(b.x.value - 0.3, b.y.value + 0.3), v.to_screen(c.x.value + 0.3, c.y.value - 0.3));
        let got = inside(&st, v, r);
        assert!(got.contains(&Sel::Node("B".into())) && got.contains(&Sel::Node("C".into())), "{got:?}");
        assert!(got.iter().any(|s| matches!(s, Sel::Member(_))), "{got:?}");
        assert!(!got.contains(&Sel::Node("A".into())), "{got:?}");
        // Moving the beam moves both its nodes; an expression stays one.
        let beam = got.iter().find(|s| matches!(s, Sel::Member(_))).cloned().unwrap();
        assert_eq!(moved_nodes(&st, std::slice::from_ref(&beam)), vec!["B".to_string(), "C".to_string()]);
        let mut src = st.clone();
        src.nodes[1].y = Num::expr("H");
        let out: Structure = serde_json::from_value(moved(&src, &["B".into()], 0.0, 0.5)).unwrap();
        assert_eq!(out.node("B").unwrap().y.expr.as_deref(), Some("H + 0.5"));
        let out: Structure = serde_json::from_value(moved(&src, &["C".into()], -0.25, 0.0)).unwrap();
        assert!((out.node("C").unwrap().x.value - (c.x.value - 0.25)).abs() < 1e-9);
        // The document takes the move as one change.
        let ys = |app: &mut WordApp| {
            let st = resolved(app);
            (st.node("B").unwrap().y.value, st.node("C").unwrap().y.value)
        };
        let before = ys(&mut app);
        app.run("structure.set", json!({"structure": moved(&st, &moved_nodes(&st, &[beam]), 0.0, 1.0)})).unwrap();
        let after = ys(&mut app);
        assert!((after.0 - before.0 - 1.0).abs() < 1e-9 && (after.1 - before.1 - 1.0).abs() < 1e-9, "{before:?} → {after:?}");
        // Removing B takes its members, and the force on it.
        let st = resolved(&mut app);
        let kept: Structure = serde_json::from_value(without(&st, &[Sel::Node("B".into())])).unwrap();
        assert!(kept.node("B").is_none() && kept.members.iter().all(|m| m.a != "B" && m.b != "B"), "{:?}", kept.members);
        assert!(kept.loads.len() < st.loads.len(), "{:?}", kept.loads);
        // The distributed load on the beam is picked inside its band, the beam on its line.
        let (cx, cy, s) = fit(&st, rect);
        let v = View { rect, cx, cy, s };
        let (b, c) = (st.node("B").unwrap(), st.node("C").unwrap());
        let mid = v.to_screen((b.x.value + c.x.value) / 2.0, b.y.value);
        assert!(matches!(pick(&st, v, mid), Some(Sel::Member(_))), "{:?}", pick(&st, v, mid));
        assert!(matches!(pick(&st, v, mid - vec2(0.0, 4.0 * MM)), Some(Sel::Load(_))), "{:?}", pick(&st, v, mid - vec2(0.0, 4.0 * MM)));
        // Hostile picks never panic.
        let _ = pick(&st, v, pos2(f32::NAN, f32::INFINITY));
        let _ = inside(&st, v, Rect::NOTHING);
    }

    /// A double-click on a structure opens its editor in place of the page; Close Editor, or
    /// the command, comes back to the tab that was open.
    #[test]
    fn double_click_on_a_structure_opens_the_editor() {
        use egui::{Event, PointerButton};
        let mut a = WordApp::new(Session::new(wordcraft_doc::Document::new()), crate::Services::default());
        a.run("view.commentsPane", json!({"value": false})).unwrap();
        a.run("structure.insert", json!({"template": "beam"})).unwrap();
        // The new structure is selected: it starts at the selection's anchor.
        let at_obj = a.session.sel.anchor.clone();
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 2200.0)).with_step_dt(1.0 / 60.0).build_ui_state(
            |ui, app: &mut WordApp| {
                let ctx = ui.ctx().clone();
                app.logic(&ctx);
                app.ui(ui);
            },
            a,
        );
        for _ in 0..6 {
            h.step();
        }
        let hint = h.state().session.page_hint;
        let obj = h.state_mut().session.layout().object(&at_obj, hint).expect("structure laid out");
        let (x, y) = (obj.rect.x + obj.rect.w / 2.0, obj.rect.y + obj.rect.h / 2.0);
        let at = crate::canvas::page_to_screen(h.state_mut(), obj.page, x, y).expect("structure on screen");
        h.event(Event::PointerMoved(at));
        for _ in 0..2 {
            h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Default::default() });
            h.step();
            h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Default::default() });
            h.step();
        }
        assert!(h.state().structure_edit.is_some(), "the editor is open");
        assert_eq!(h.state().ui.tab, "Structures");
        for _ in 0..3 {
            h.step();
        }
        h.state_mut().run("ui.structureClose", json!({})).unwrap();
        h.step();
        assert!(h.state().structure_edit.is_none());
        assert_ne!(h.state().ui.tab, "Structures");
    }

    /// Clicks on the sheet with each tool change the structure as they say.
    #[test]
    fn tools_edit_the_structure() {
        let mut app = WordApp::new(Session::new(wordcraft_doc::Document::new()), crate::Services::default());
        app.run("structure.insert", json!({"template": "beam"})).unwrap();
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 300.0));
        let st = resolved(&mut app);
        let (cx, cy, s) = fit(&st, rect);
        let v = View { rect, cx, cy, s };
        // A member from B (6, 0) to (8, 2): a new node C.
        let mut state = State { tool: Tool::Member, ..State::default() };
        click(&mut app, &st, v, v.to_screen(6.0, 0.0), &mut state);
        click(&mut app, &st, v, v.to_screen(8.0, 2.0), &mut state);
        let st = resolved(&mut app);
        assert_eq!(st.members.len(), 2, "{:?}", st.members);
        assert!(st.nodes.iter().any(|n| n.id == "C" && n.x.value == 8.0 && n.y.value == 2.0), "{:?}", st.nodes);
        // A fixed support at A, a force on member 1 at 3 m.
        state.tool = Tool::Support(SupportKind::Fixed);
        click(&mut app, &st, v, v.to_screen(0.0, 0.0), &mut state);
        state.tool = Tool::Force;
        click(&mut app, &st, v, v.to_screen(3.0, 0.0), &mut state);
        let st = resolved(&mut app);
        assert_eq!(st.supports.iter().find(|s| s.node == "A").map(|s| s.kind), Some(SupportKind::Fixed));
        assert!(
            st.loads.iter().any(|l| matches!(l, Load::Force { member: Some(m), at, .. } if m == "1" && (at.value - 3.0).abs() < 1e-9)),
            "{:?}",
            st.loads
        );
        // Erase removes the new node with its member.
        state.tool = Tool::Erase;
        click(&mut app, &st, v, v.to_screen(8.0, 2.0), &mut state);
        let st = resolved(&mut app);
        assert_eq!(st.members.len(), 1);
        // Typed numbers: plain numbers (decimal commas too) or expressions.
        assert_eq!(num_json("2,5"), json!(2.5));
        assert_eq!(num_json("L/2"), json!("L/2"));
    }
}
