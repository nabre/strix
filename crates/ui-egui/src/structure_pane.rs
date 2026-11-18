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

mod edits;
mod grips;
mod menu;

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

/// The tools as `ui.structureTool` names them.
const TOOL_NAMES: [(Tool, &str); 13] = [
    (Tool::Select, "select"),
    (Tool::Node, "node"),
    (Tool::Member, "member"),
    (Tool::Support(SupportKind::Fixed), "fixed"),
    (Tool::Support(SupportKind::Pin), "pin"),
    (Tool::Support(SupportKind::Roller), "roller"),
    (Tool::Support(SupportKind::Link), "link"),
    (Tool::Support(SupportKind::Slider), "slider"),
    (Tool::Hinge, "hinge"),
    (Tool::Force, "force"),
    (Tool::Couple, "couple"),
    (Tool::Distributed, "distributed"),
    (Tool::Erase, "erase"),
];

impl Tool {
    fn name(self) -> &'static str {
        TOOL_NAMES.iter().find(|(t, _)| *t == self).map_or("select", |(_, n)| n)
    }

    fn from_name(name: &str) -> Option<Tool> {
        TOOL_NAMES.iter().find(|(_, n)| n.eq_ignore_ascii_case(name.trim())).map(|(t, _)| *t)
    }
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
    /// Moving the selected nodes (and the ends of the selected members): the world point grabbed,
    /// where it is now, and the node grabbed (its position), which lands on the grid when snapping.
    Move { from: (f64, f64), to: (f64, f64), anchor: Option<(f64, f64)> },
    /// Sliding a load along its member: its index, where along the member it started and where
    /// it is now.
    Slide { load: usize, start: f64, at: f64 },
    /// A selection window, screen corners.
    Window { from: Pos2, to: Pos2 },
    /// A grip dragged (`grips.rs`), with where the pointer is.
    Grip { grip: grips::Grip, to: Pos2 },
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
    /// Dimensions, slopes and angles drawn on the sheet (the editor's view: the document draws
    /// them as Structures › Dimensions says).
    dims: bool,
    /// The coordinates typed.
    typed: String,
    /// How many loads the structure had when last seen: loads are selected by index, so a
    /// change in their number (an undo, a table, a command) drops them from the selection.
    loads: Option<usize>,
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
            dims: true,
            typed: String::new(),
            loads: None,
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

    /// What no longer exists leaves the selection; loads too when their number changed, since
    /// an index may then name another load.
    fn prune(&mut self, st: &Structure) {
        let n = st.loads.len();
        let renumbered = self.loads.is_some_and(|k| k != n);
        self.loads = Some(n);
        self.sel.retain(|s| match s {
            Sel::Node(id) => st.node(id).is_some(),
            Sel::Member(id) => st.member(id).is_some(),
            Sel::Load(k) => !renumbered && *k < n,
        });
        if let Some(Gesture::Slide { load, .. }) = &self.gesture
            && (renumbered || *load >= n)
        {
            self.gesture = None;
        }
        if let Some(Gesture::Grip { grip: grips::Grip::Point(k) | grips::Grip::Tail(k) | grips::Grip::From(k) | grips::Grip::To(k), .. }) =
            &self.gesture
            && (renumbered || *k >= n)
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
    sheet(app, ui, &src, &st, &mut state, h, false);
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
    // The page is not drawn meanwhile, so its focus flag (set only while it is) would stay on
    // and keep the program's shortcuts (Ctrl+Z, Ctrl+S…) from running.
    app.canvas.focused = false;
    let mut state = ui.data(|d| d.get_temp::<State>(state_id())).unwrap_or_default();
    state.prune(&st);
    let full = ui.available_rect_before_wrap();
    let side_w = 360.0_f32.min(full.width() * 0.4);
    let main = Rect::from_min_max(full.min, pos2(full.max.x - side_w, full.max.y));
    let side = Rect::from_min_max(pos2(full.max.x - side_w, full.min.y), full.max);
    let mut close_now = false;
    let mut esc_free = false;
    ui.scope_builder(egui::UiBuilder::new().max_rect(main.shrink(8.0)).layout(egui::Layout::top_down(egui::Align::Min)), |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(ts("Structure Editor")).heading().strong());
            ui.add_space(12.0);
            status_line(ui, &info);
        });
        // Its tools, Write Working and Close Editor are on its ribbon tab (EDITOR_TAB).
        let h = ui.available_height() - 34.0;
        esc_free = sheet(app, ui, &src, &st, &mut state, h, true);
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
    // Esc with nothing to cancel or deselect leaves the editor (the sheet's keys took it
    // otherwise, one step at a time).
    if esc_free && !egui::Popup::is_any_open(ui.ctx()) {
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
    app.ui.tab = EDITOR_TAB.to_string();
    Ok(())
}

/// Back to the page, on the tab that was open (unless another was chosen meanwhile), the page
/// taking the keyboard again.
pub(crate) fn close(app: &mut WordApp) {
    if let Some(tab) = app.structure_edit.take() {
        if app.ui.tab == EDITOR_TAB {
            app.ui.tab = tab;
        }
        app.canvas.want_focus = true;
    }
}

/// The ribbon tab that comes up with the full-page editor, holding its tools.
pub(crate) const EDITOR_TAB: &str = "Structure Editor";

/// The editor's tools and view, as its ribbon tab shows them.
pub(crate) struct Tools {
    /// The tool, as `ui.structureTool` names it.
    pub tool: &'static str,
    pub snap: bool,
    pub grid: f64,
    pub dims: bool,
}

/// The editor's tools and view now.
pub(crate) fn tools(ctx: &egui::Context) -> Tools {
    let s = ctx.data(|d| d.get_temp::<State>(state_id())).unwrap_or_default();
    Tools { tool: s.tool.name(), snap: s.snap, grid: s.grid, dims: s.dims }
}

/// Changes the editor's state kept between frames (the ribbon's commands).
fn with_state(app: &WordApp, f: impl FnOnce(&mut State)) -> Result<Value, String> {
    let ctx = app.ctx.as_ref().ok_or("no window")?;
    let s = ctx.data_mut(|d| {
        let mut s = d.get_temp::<State>(state_id()).unwrap_or_default();
        f(&mut s);
        d.insert_temp(state_id(), s.clone());
        s
    });
    ctx.request_repaint();
    Ok(json!({"tool": s.tool.name(), "snap": s.snap, "grid": s.grid, "dimensions": s.dims}))
}

/// UI commands (`ui.structure*`): `ui.structureEdit` opens the full-page editor,
/// `ui.structureClose` closes it; the editor's tab: `ui.structureTool` (`{"tool": "select" |
/// "node" | "member" | "fixed" | "pin" | "roller" | "link" | "slider" | "hinge" | "force" |
/// "couple" | "distributed" | "erase"}`), `ui.structureSnap` and `ui.structureDimensions`
/// (`{"on"?: bool}`, without it they toggle), `ui.structureGrid` (`{"size": number}`) and
/// `ui.structureFit`.
pub(crate) fn command(app: &mut WordApp, id: &str, p: &Value) -> Result<Value, String> {
    let on = p.get("on").and_then(Value::as_bool);
    match id {
        "ui.structureEdit" => {
            open(app)?;
            Ok(json!({"open": true}))
        }
        "ui.structureClose" => {
            close(app);
            Ok(json!({"open": false}))
        }
        "ui.structureTool" => {
            let name = p.get("tool").and_then(Value::as_str).unwrap_or_default();
            let tool = Tool::from_name(name).ok_or_else(|| format!("unknown tool {name:?}"))?;
            with_state(app, |s| {
                s.tool = tool;
                s.start = None;
                s.gesture = None;
            })
        }
        "ui.structureSnap" => with_state(app, |s| s.snap = on.unwrap_or(!s.snap)),
        "ui.structureDimensions" => with_state(app, |s| s.dims = on.unwrap_or(!s.dims)),
        "ui.structureGrid" => {
            let size = p.get("size").and_then(Value::as_f64).filter(|v| v.is_finite() && *v > 0.0).ok_or("size: a positive number")?;
            with_state(app, |s| s.grid = size.clamp(0.01, 100.0))
        }
        "ui.structureFit" => with_state(app, |s| s.view = None),
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
        "loads": ts("Loads"),
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

/// What the editor's Dimensions switch does.
pub(crate) const DIMS_TIP: &str = "Show on the sheet the dimensions, the slopes of inclined members and the angles of inclined forces (the document draws them as Structures › Dimensions says)";

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
        ui.checkbox(&mut state.dims, ts("Dimensions")).on_hover_text(ts(DIMS_TIP));
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
    // Room round it for the supports, the loads and the chains of dimensions.
    let margin = 100.0;
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

/// The sheet `h` points high, in the pane or (`full`) in the editor in place of the page. Returns
/// whether Esc was pressed with nothing on the sheet to cancel or deselect.
fn sheet(app: &mut WordApp, ui: &mut Ui, src: &Structure, st: &Structure, state: &mut State, h: f32, full: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h.max(120.0)), Sense::click_and_drag());
    // A click or a drag on the sheet gives it the keyboard (from the page, in the pane), with the
    // arrows and Esc kept for it instead of moving the focus.
    if resp.clicked() || resp.drag_started() || resp.secondary_clicked() {
        resp.request_focus();
    }
    if resp.has_focus() {
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(resp.id, egui::EventFilter { tab: false, horizontal_arrows: true, vertical_arrows: true, escape: true })
        });
    }
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
    menu::context(app, &resp, src, st, v, state);
    if resp.secondary_clicked() {
        state.start = None;
    }
    if resp.double_clicked()
        && state.tool == Tool::Select
        && let Some(hit) = resp.interact_pointer_pos().and_then(|p| pick(st, v, p))
    {
        grips::select_connected(st, state, &hit);
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
    // Keys while no box is being typed in and nothing else has the keyboard: in the full-page
    // editor always, in the pane when the sheet has the focus or the pointer is over it.
    let focused = ui.ctx().memory(|m| m.focused());
    let other = focused.is_some_and(|id| id != resp.id);
    if !other && (full || resp.has_focus() || (resp.hovered() && focused.is_none())) {
        return keys(app, ui, src, st, state);
    }
    false
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
            let grip = grips::hit(&grips::grips(st, v, &state.sel), p);
            state.gesture = match (grip, pick(st, v, p)) {
                (Some(grip), _) => Some(Gesture::Grip { grip, to: p }),
                (None, Some(Sel::Load(k))) => {
                    if !state.has(&Sel::Load(k)) {
                        state.select(Sel::Load(k));
                    }
                    // On a member it slides along it; at a node it goes to another (or a member).
                    match load_on_member(st, k) {
                        Some((_, at)) => Some(Gesture::Slide { load: k, start: at, at }),
                        None if !matches!(st.loads.get(k), Some(Load::Distributed { .. })) => {
                            Some(Gesture::Grip { grip: grips::Grip::Point(k), to: p })
                        }
                        None => None,
                    }
                }
                (None, Some(hit)) => {
                    let w = v.to_world(p);
                    let anchor = grabbed(st, &hit, w);
                    if !state.has(&hit) {
                        if adding {
                            state.sel.push(hit);
                        } else {
                            state.select(hit);
                        }
                    }
                    Some(Gesture::Move { from: w, to: w, anchor })
                }
                (None, None) => {
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
                Some(Gesture::Slide { load, at, .. }) => {
                    if let Some((a, len)) = along(st, *load, v, p) {
                        let r = if snap_on && grid > 0.0 { (a / grid).round() * grid } else { (a * 1000.0).round() / 1000.0 };
                        *at = r.clamp(0.0, len);
                    }
                }
                Some(Gesture::Window { to, .. } | Gesture::Grip { to, .. }) => *to = p,
                None => {}
            }
        }
        if resp.drag_stopped_by(egui::PointerButton::Primary)
            && let Some(g) = state.gesture.take()
        {
            match g {
                Gesture::Move { from, to, anchor } => {
                    let (dx, dy) = step(from, to, anchor, state);
                    if dx != 0.0 || dy != 0.0 {
                        let moved = moved(src, &moved_nodes(st, &state.sel), dx, dy);
                        let _ = app.run("structure.set", json!({"structure": moved}));
                    }
                }
                Gesture::Slide { load, start, at } => {
                    if let Some(lj) = slid(src, load, start, at) {
                        let _ = app.run("structure.load", json!({"index": load, "load": lj}));
                    }
                }
                Gesture::Grip { grip, to } => {
                    let shift = ui.input(|i| i.modifiers.shift);
                    grips::commit(app, src, st, v, state, &grip, to, shift);
                }
                Gesture::Window { from, to } => {
                    let r = Rect::from_two_pos(from, to);
                    // Right to left: what it crosses; left to right: what it holds.
                    let taken = if to.x < from.x { grips::crossing(st, v, r) } else { inside(st, v, r) };
                    for s in taken {
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
    let shift = ui.input(|i| i.modifiers.shift);
    // The structure as the document draws it, with the drag under way.
    let mut shown = st.clone();
    match &state.gesture {
        Some(Gesture::Move { from, to, anchor }) => {
            let (dx, dy) = step(*from, *to, *anchor, state);
            let ids = moved_nodes(st, &state.sel);
            for n in shown.nodes.iter_mut().filter(|n| ids.contains(&n.id)) {
                n.x = Num::new(n.x.value + dx);
                n.y = Num::new(n.y.value + dy);
            }
        }
        Some(Gesture::Slide { load, at, .. }) => {
            if let Some(Load::Force { at: a, .. } | Load::Moment { at: a, .. }) = shown.loads.get_mut(*load) {
                *a = Num::new(*at);
            }
        }
        Some(Gesture::Grip { grip, to }) => shown = grips::preview(st, v, state, grip, *to, shift),
        _ => {}
    }
    shown.diagrams.dimensions = state.dims;
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
    // The grips of the selection, the one dragged and where it would land.
    let handles = grips::grips(&shown, v, &state.sel);
    let dragged = if let Some(Gesture::Grip { grip, to }) = &state.gesture { Some((grip, *to, shift)) } else { None };
    grips::paint(&painter, st, v, state, &handles, hover, dragged);
    let over_grip = state.tool == Tool::Select && hover.is_some_and(|p| grips::hit(&handles, p).is_some());
    grips::cursor(ui.ctx(), over_grip, matches!(state.gesture, Some(Gesture::Grip { .. } | Gesture::Move { .. } | Gesture::Slide { .. })));
    if let Some(Gesture::Window { from, to }) = &state.gesture {
        grips::paint_window(&painter, *from, *to);
    }
    // What the pointer is over, named.
    if let (Some(h), Some(p)) = (&hovered, hover)
        && !over_grip
        && let Some(text) = grips::tag(&shown, h)
    {
        grips::paint_tag(&painter, rect, p, &text);
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
        Some(Gesture::Move { from, to, anchor }) => {
            let (dx, dy) = step(*from, *to, *anchor, state);
            Some(format!("Δx = {} {unit}   Δy = {} {unit}", short(dx), short(dy)))
        }
        Some(Gesture::Slide { at, .. }) => Some(format!("{} = {} {unit}", ts("at"), short(*at))),
        Some(Gesture::Grip { grip, to }) => Some(grips::readout(st, v, state, grip, *to, shift)),
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

/// Esc, Delete, the arrows and Select All on the sheet, each taken (consumed) when it does
/// something, so neither the page nor the program's shortcuts see it too. Esc goes one step at a
/// time: the drag or the member being drawn, then back to Select, then the selection. Returns
/// whether Esc was pressed with none of these left to undo.
fn keys(app: &mut WordApp, ui: &Ui, src: &Structure, st: &Structure, state: &mut State) -> bool {
    use egui::{Key, Modifiers};
    let busy = state.gesture.is_some() || state.start.is_some() || state.tool != Tool::Select || !state.sel.is_empty();
    let selected = !state.sel.is_empty();
    let (esc, esc_free, del, all, arrows, shift) = ui.input_mut(|i| {
        let esc = busy && i.consume_key(Modifiers::NONE, Key::Escape);
        let esc_free = !busy && i.key_pressed(Key::Escape);
        let del = selected && (i.consume_key(Modifiers::NONE, Key::Delete) || i.consume_key(Modifiers::NONE, Key::Backspace));
        let all = i.consume_key(Modifiers::COMMAND, Key::A);
        let mut count = |k: Key| if selected { f64::from(u32::try_from(i.count_and_consume_key(Modifiers::NONE, k)).unwrap_or(0)) } else { 0.0 };
        let dir = (count(Key::ArrowRight) - count(Key::ArrowLeft), count(Key::ArrowUp) - count(Key::ArrowDown));
        (esc, esc_free, del, all, dir, i.modifiers.shift)
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
        menu::select_all(st, state);
    }
    if del {
        let kept = without(src, &state.sel);
        if app.run("structure.set", json!({"structure": kept})).is_ok() {
            state.sel.clear();
        }
    }
    if arrows != (0.0, 0.0) {
        let g = if state.grid > 0.0 { state.grid } else { 0.1 };
        let k = if shift { 10.0 } else { 1.0 };
        let ids = moved_nodes(st, &state.sel);
        if !ids.is_empty() {
            let moved = moved(src, &ids, arrows.0 * g * k, arrows.1 * g * k);
            let _ = app.run("structure.set", json!({"structure": moved}));
        }
    }
    esc_free
}

/// The move from `from` to `to`. When snapping, the node grabbed (`anchor`) lands on the grid;
/// without one, the move is a whole number of grid steps.
fn step(from: (f64, f64), to: (f64, f64), anchor: Option<(f64, f64)>, state: &State) -> (f64, f64) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    if !(state.snap && state.grid > 0.0) {
        return (round(dx), round(dy));
    }
    let g = state.grid;
    let on = |v: f64| (v / g).round() * g;
    match anchor {
        Some((ax, ay)) => (round(on(ax + dx) - ax), round(on(ay + dy) - ay)),
        None => (round(on(dx)), round(on(dy))),
    }
}

/// The node a drag that starts on `hit` at world point `w` carries: the node itself, or the
/// member's end nearest the point grabbed.
fn grabbed(st: &Structure, hit: &Sel, w: (f64, f64)) -> Option<(f64, f64)> {
    let at = |id: &str| st.node(id).map(|n| (n.x.value, n.y.value));
    match hit {
        Sel::Node(id) => at(id),
        Sel::Member(id) => {
            let m = st.member(id)?;
            let (a, b) = (at(&m.a)?, at(&m.b)?);
            let d = |p: (f64, f64)| (p.0 - w.0).hypot(p.1 - w.1);
            Some(if d(b) < d(a) { b } else { a })
        }
        Sel::Load(_) => None,
    }
}

/// Load `k` of `src` slid along its member from `start` to `at` (resolved values), as
/// `structure.load` takes it; `None` when it did not move. An `at` written as an expression stays
/// one (`L/2` → `L/2 + 0.5`).
fn slid(src: &Structure, k: usize, start: f64, at: f64) -> Option<Value> {
    if !(at - start).is_finite() || (at - start).abs() <= 1e-9 {
        return None;
    }
    let l = src.loads.get(k)?;
    let (Load::Force { at: current, .. } | Load::Moment { at: current, .. }) = l else { return None };
    let new = shifted(current, at - start);
    let mut lj = serde_json::to_value(l).ok()?;
    if !lj.is_object() {
        return None;
    }
    lj["at"] = serde_json::to_value(new).ok()?;
    Some(lj)
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
/// A number moved by `d`: an expression stays one (`L` → `L + 0.5`), a step already added to it
/// taking the new one (`L + 0.5` → `L + 1`, back to `L` at zero).
fn shifted(n: &Num, d: f64) -> Num {
    if d == 0.0 || !d.is_finite() {
        return n.clone();
    }
    let Some(e) = &n.expr else { return Num::new(round(n.value + d)) };
    let (base, k) = split_step(e);
    let k = round(k + d);
    if k == 0.0 { Num::expr(base) } else { Num::expr(&format!("{base} {} {}", if k < 0.0 { "-" } else { "+" }, round(k.abs()))) }
}

/// An expression and the plain number added at its end (` + 0.5`, ` - 2`) as a move writes it:
/// (`L`, 0.5); (the whole, 0) without one.
fn split_step(e: &str) -> (&str, f64) {
    for (op, sign) in [(" + ", 1.0), (" - ", -1.0)] {
        if let Some(i) = e.rfind(op)
            && let Ok(v) = e[i + op.len()..].trim().parse::<f64>()
            && v.is_finite()
            && !e[..i].trim().is_empty()
        {
            // Not when the other operator comes later (`L + 1 - x`): the number must end it.
            return (e[..i].trim_end(), sign * v);
        }
    }
    (e, 0.0)
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

/// A section of the properties: its title over a thin rule.
fn section(ui: &mut Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(title).strong());
    ui.separator();
}

/// Labels and fields in columns that line up: label, field, unit.
fn props(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, add: impl FnOnce(&mut Ui)) {
    egui::Grid::new(("structure_props", id)).num_columns(3).min_col_width(64.0).spacing(vec2(8.0, 4.0)).show(ui, add);
}

/// A row's label, weaker than its value.
fn prop_label(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).weak());
}

/// The angle (degrees counter-clockwise from the horizontal) of the line from `a` to `b`.
fn slope_deg(a: (f64, f64), b: (f64, f64)) -> f64 {
    let d = (b.1 - a.1).atan2(b.0 - a.0).to_degrees();
    if d.is_finite() { d } else { 0.0 }
}

/// The properties of what is selected; with nothing selected, the structure's.
fn selection(app: &mut WordApp, ui: &mut Ui, src: &Structure, st: &Structure, state: &mut State) {
    let lu = st.length_unit.symbol();
    let fu = st.force_unit.symbol();
    if state.sel.len() > 1 {
        let n = state.sel.len().to_string();
        section(ui, &crate::i18n::fmt(ts("{n} items selected"), &[("n", &n)]));
        let count = |f: fn(&Sel) -> bool| state.sel.iter().filter(|s| f(s)).count().to_string();
        props(ui, "multi", |ui| {
            for (label, k) in [
                ("Nodes", count(|s| matches!(s, Sel::Node(_)))),
                ("Members", count(|s| matches!(s, Sel::Member(_)))),
                ("Loads", count(|s| matches!(s, Sel::Load(_)))),
            ] {
                prop_label(ui, ts(label));
                ui.label(k);
                ui.end_row();
            }
        });
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
            section(ui, &crate::i18n::fmt(ts("Node {n}"), &[("n", &id)]));
            props(ui, ("node", &id), |ui| {
                prop_label(ui, ts("Name"));
                if let Some(t) = edit_box(ui, ("node_name", &id), &id, 90.0) {
                    let _ = app.run("structure.node", json!({"id": id, "rename": t}));
                    state.select(Sel::Node(t.trim().to_string()));
                }
                ui.label("");
                ui.end_row();
                for (axis, value) in [("x", &n.x), ("y", &n.y)] {
                    prop_label(ui, axis);
                    if let Some(t) = edit_box(ui, ("node", &id, axis), &num_text(value), 120.0) {
                        let _ = app.run("structure.node", json!({"id": id, axis: num_json(&t)}));
                    }
                    ui.label(RichText::new(lu).weak());
                    ui.end_row();
                }
                prop_label(ui, ts("Hinge"));
                let mut hinge = n.hinge;
                if ui.checkbox(&mut hinge, ts("Internal hinge")).changed() {
                    let _ = app.run("structure.node", json!({"id": id, "hinge": hinge}));
                }
                ui.label("");
                ui.end_row();
                let joined: Vec<&str> = src.members.iter().filter(|m| m.a == id || m.b == id).map(|m| m.id.as_str()).collect();
                prop_label(ui, ts("Members"));
                ui.label(if joined.is_empty() { "—".to_string() } else { joined.join(", ") });
                ui.label("");
                ui.end_row();
            });
            section(ui, ts("Support"));
            let support = src.supports.iter().find(|s| s.node == id);
            props(ui, ("node_support", &id), |ui| {
                prop_label(ui, ts("Kind"));
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
                ui.label("");
                ui.end_row();
                if let Some(s) = support {
                    prop_label(ui, ts("Angle"));
                    if let Some(t) = edit_box(ui, ("support_angle", &id), &num_text(&s.angle), 80.0) {
                        let _ = app.run("structure.support", json!({"node": id, "angle": num_json(&t)}));
                    }
                    ui.label(RichText::new("°").weak());
                    ui.end_row();
                }
            });
            node_loads(app, ui, src, st, state, &id);
            ui.add_space(6.0);
            if ui.button(ts("Remove Node")).on_hover_text(ts("Remove the node with its members, support and loads")).clicked() {
                let _ = app.run("structure.node", json!({"id": id, "remove": true}));
                state.sel.clear();
            }
        }
        Some(Sel::Member(id)) => {
            let Some(m) = src.member(&id) else { return };
            let len = st.member(&id).and_then(|m| st.length(m)).unwrap_or(0.0);
            let ends = st
                .member(&id)
                .and_then(|m| Some(((st.node(&m.a)?.x.value, st.node(&m.a)?.y.value), (st.node(&m.b)?.x.value, st.node(&m.b)?.y.value))));
            section(ui, &crate::i18n::fmt(ts("Member {n}"), &[("n", &id)]));
            props(ui, ("member", &id), |ui| {
                for (key, label, current) in [("a", "From", &m.a), ("b", "To", &m.b)] {
                    prop_label(ui, ts(label));
                    craft_ui::ComboBox::from_id_salt(("structure_member_prop_end", &id, key)).width(90.0).selected_text(current.as_str()).show_ui(
                        ui,
                        |ui| {
                            for n in &src.nodes {
                                if ui.selectable_label(*current == n.id, &n.id).clicked() && *current != n.id {
                                    let _ = app.run("structure.member", json!({"id": id, key: n.id}));
                                }
                            }
                        },
                    );
                    let mut hinge = if key == "a" { m.hinge_a } else { m.hinge_b };
                    let field = if key == "a" { "hingeA" } else { "hingeB" };
                    if ui.checkbox(&mut hinge, ts("hinged")).on_hover_text(ts("A hinge at this end of the member")).changed() {
                        let _ = app.run("structure.member", json!({"id": id, field: hinge}));
                    }
                    ui.end_row();
                }
                prop_label(ui, ts("Length"));
                ui.label(short(len));
                ui.label(RichText::new(lu).weak());
                ui.end_row();
                if let Some((pa, pb)) = ends {
                    prop_label(ui, ts("Slope"));
                    ui.label(short(slope_deg(pa, pb)));
                    ui.label(RichText::new("°").weak());
                    ui.end_row();
                }
            });
            section(ui, ts("Section"));
            props(ui, ("member_section", &id), |ui| {
                for (key, label, value, unit) in
                    [("e", "E", &m.e, format!("{fu}/{lu}²")), ("i", "I", &m.i, format!("{lu}⁴")), ("area", "A", &m.area, format!("{lu}²"))]
                {
                    prop_label(ui, label);
                    let text = value.as_ref().map(num_text).unwrap_or_default();
                    if let Some(t) = edit_box(ui, ("member", &id, key), &text, 120.0) {
                        let v = if t.trim().is_empty() { Value::Null } else { num_json(&t) };
                        let _ = app.run("structure.member", json!({"id": id, key: v}));
                    }
                    ui.label(RichText::new(unit).weak());
                    ui.end_row();
                }
            });
            ui.label(RichText::new(ts("Without E, I, A: EI is the same for every member and members do not stretch.")).small().weak());
            member_loads(app, ui, src, st, state, &id);
            ui.add_space(6.0);
            if ui.button(ts("Remove Member")).on_hover_text(ts("Remove the member with its loads")).clicked() {
                let _ = app.run("structure.member", json!({"id": id, "remove": true}));
                state.sel.clear();
            }
        }
        Some(Sel::Load(k)) => {
            section(ui, ts("Load"));
            loads_table(app, ui, src, st, Some(k));
        }
        None => structure_props(app, ui, src),
    }
}

/// The loads acting on node `id`, each a link selecting it.
fn node_loads(app: &mut WordApp, ui: &mut Ui, src: &Structure, st: &Structure, state: &mut State, id: &str) {
    let on: Vec<usize> = src
        .loads
        .iter()
        .enumerate()
        .filter(|(_, l)| matches!(l, Load::Force { node: Some(n), .. } | Load::Moment { node: Some(n), .. } if n == id))
        .map(|(k, _)| k)
        .collect();
    load_links(app, ui, src, st, state, &on, json!({"node": id}));
}

/// The loads acting on member `id`, each a link selecting it.
fn member_loads(app: &mut WordApp, ui: &mut Ui, src: &Structure, st: &Structure, state: &mut State, id: &str) {
    let on: Vec<usize> = src
        .loads
        .iter()
        .enumerate()
        .filter(|(_, l)| match l {
            Load::Force { member: Some(m), .. } | Load::Moment { member: Some(m), .. } => m == id,
            Load::Distributed { member, .. } => member == id,
            _ => false,
        })
        .map(|(k, _)| k)
        .collect();
    load_links(app, ui, src, st, state, &on, json!({"member": id}));
}

/// The Loads section of a node or a member: its loads (a click selects one) and buttons adding
/// one there (`place`: `{"node": …}` or `{"member": …}`).
fn load_links(app: &mut WordApp, ui: &mut Ui, src: &Structure, st: &Structure, state: &mut State, on: &[usize], place: Value) {
    section(ui, ts("Loads"));
    let fu = st.force_unit.symbol();
    let lu = st.length_unit.symbol();
    for &k in on {
        let Some(l) = src.loads.get(k) else { continue };
        let text = match l {
            Load::Force { value, angle, .. } => format!("{} {}: F = {} {fu}, {}°", k + 1, ts("Force"), num_text(value), num_text(angle)),
            Load::Moment { value, .. } => format!("{} {}: C = {} {fu}·{lu}", k + 1, ts("Couple"), num_text(value)),
            Load::Distributed { q1, q2, .. } => {
                let q = match q2 {
                    Some(q2) => format!("{} → {}", num_text(q1), num_text(q2)),
                    None => num_text(q1),
                };
                format!("{} {}: q = {q} {fu}/{lu}", k + 1, ts("Distributed"))
            }
        };
        if ui.selectable_label(false, text).on_hover_text(ts("Select the load to edit it")).clicked() {
            state.select(Sel::Load(k));
        }
    }
    if on.is_empty() {
        ui.label(RichText::new("—").weak());
    }
    ui.horizontal_wrapped(|ui| {
        let mut add = |kind: &str, extra: Value| {
            let mut load = json!({"type": kind});
            for (key, v) in place.as_object().into_iter().flatten().chain(extra.as_object().into_iter().flatten()) {
                load[key] = v.clone();
            }
            if app.run("structure.load", json!({"load": load})).is_ok() {
                state.select(Sel::Load(src.loads.len()));
                state.loads = Some(src.loads.len() + 1);
            }
        };
        if ui.small_button(ts("Add Force")).clicked() {
            add("force", json!({"value": 10.0}));
        }
        if ui.small_button(ts("Add Couple")).clicked() {
            add("moment", json!({"value": 10.0}));
        }
        if place.get("member").is_some() && ui.small_button(ts("Add Distributed Load")).clicked() {
            add("distributed", json!({"q1": 10.0}));
        }
    });
}

/// With nothing selected: the structure's title and units, what it is made of.
fn structure_props(app: &mut WordApp, ui: &mut Ui, src: &Structure) {
    section(ui, ts("Structure"));
    props(ui, "structure", |ui| {
        prop_label(ui, ts("Title"));
        if let Some(t) = edit_box(ui, "structure_title", &src.title, 160.0) {
            let _ = app.run("structure.set", json!({"title": t.trim()}));
        }
        ui.label("");
        ui.end_row();
        prop_label(ui, ts("Lengths"));
        let lu = src.length_unit.symbol();
        craft_ui::ComboBox::from_id_salt("structure_prop_lu").width(70.0).selected_text(lu).show_ui(ui, |ui| {
            for u in ["m", "cm", "mm"] {
                if ui.selectable_label(lu == u, u).clicked() && lu != u {
                    let _ = app.run("structure.set", json!({"lengthUnit": u}));
                }
            }
        });
        ui.label("");
        ui.end_row();
        prop_label(ui, ts("Forces"));
        let fu = src.force_unit.symbol();
        craft_ui::ComboBox::from_id_salt("structure_prop_fu").width(70.0).selected_text(fu).show_ui(ui, |ui| {
            for u in ["kN", "daN", "N"] {
                if ui.selectable_label(fu == u, u).clicked() && fu != u {
                    let _ = app.run("structure.set", json!({"forceUnit": u}));
                }
            }
        });
        ui.label("");
        ui.end_row();
    });
    section(ui, ts("Made of"));
    props(ui, "structure_counts", |ui| {
        for (label, n) in [("Nodes", src.nodes.len()), ("Members", src.members.len()), ("Supports", src.supports.len()), ("Loads", src.loads.len())] {
            prop_label(ui, ts(label));
            ui.label(n.to_string());
            ui.end_row();
        }
    });
    ui.add_space(4.0);
    ui.label(
        RichText::new(ts("Click a node, a member or a load to see its properties; Shift+click or a window adds to the selection.")).small().weak(),
    );
}

fn tables(app: &mut WordApp, ui: &mut Ui, src: &Structure, st: &Structure, state: &mut State) {
    section(ui, ts("Tables"));
    let title = |name: &str, n: usize| RichText::new(format!("{} ({n})", ts(name))).strong();
    egui::CollapsingHeader::new(title("Nodes", src.nodes.len()))
        .id_salt("structure_nodes")
        .default_open(false)
        .show(ui, |ui| nodes_table(app, ui, src, state));
    egui::CollapsingHeader::new(title("Members", src.members.len()))
        .id_salt("structure_members")
        .default_open(false)
        .show(ui, |ui| members_table(app, ui, src, state));
    egui::CollapsingHeader::new(title("Supports", src.supports.len()))
        .id_salt("structure_supports")
        .default_open(false)
        .show(ui, |ui| supports_table(app, ui, src));
    egui::CollapsingHeader::new(title("Loads", src.loads.len()))
        .id_salt("structure_loads")
        .default_open(false)
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
        egui::Frame::group(ui.style()).inner_margin(6.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let title = match l {
                    Load::Force { .. } => ts("Force"),
                    Load::Moment { .. } => ts("Couple"),
                    Load::Distributed { .. } => ts("Distributed"),
                };
                ui.label(RichText::new(format!("{} {}", k + 1, title)).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("×").on_hover_text(ts("Remove the load")).clicked() {
                        remove = true;
                    }
                });
            });
            // One field per row: label, box, unit.
            let mut edits: Vec<(&'static str, Value)> = Vec::new();
            let mut field = |ui: &mut Ui, label: &str, key: &'static str, current: String, unit: &str, optional: bool| {
                prop_label(ui, label);
                if let Some(t) = edit_box(ui, (format!("load_{key}"), k), &current, 90.0) {
                    edits.push((key, if optional && t.trim().is_empty() { Value::Null } else { num_json(&t) }));
                }
                ui.label(RichText::new(unit).weak());
                ui.end_row();
            };
            props(ui, ("load", k), |ui| {
                // Where: a node or a member.
                prop_label(ui, ts("On"));
                match l {
                    Load::Force { node, member, .. } | Load::Moment { node, member, .. } => {
                        let place = node
                            .clone()
                            .map(|n| format!("{} {n}", ts("node")))
                            .or_else(|| member.clone().map(|m| format!("{} {m}", ts("member"))))
                            .unwrap_or_default();
                        craft_ui::ComboBox::from_id_salt(("structure_load_place", k)).width(90.0).selected_text(place).show_ui(ui, |ui| {
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
                    }
                    Load::Distributed { member, .. } => {
                        craft_ui::ComboBox::from_id_salt(("structure_load_member", k))
                            .width(90.0)
                            .selected_text(format!("{} {member}", ts("member")))
                            .show_ui(ui, |ui| {
                                for m in &src.members {
                                    if ui.selectable_label(*member == m.id, format!("{} {}", ts("member"), m.id)).clicked() {
                                        lj["member"] = json!(m.id);
                                        changed = true;
                                    }
                                }
                            });
                    }
                }
                ui.label("");
                ui.end_row();
                match l {
                    Load::Force { member, at, value, angle, .. } => {
                        if member.is_some() {
                            field(ui, ts("at"), "at", num_text(at), lu, false);
                        }
                        field(ui, "F", "value", num_text(value), fu, false);
                        field(ui, ts("direction"), "angle", num_text(angle), "°", false);
                    }
                    Load::Moment { member, at, value, .. } => {
                        if member.is_some() {
                            field(ui, ts("at"), "at", num_text(at), lu, false);
                        }
                        field(ui, "C", "value", num_text(value), &format!("{fu}·{lu}"), false);
                    }
                    Load::Distributed { q1, q2, from, to, angle, .. } => {
                        let q = format!("{fu}/{lu}");
                        field(ui, "q1", "q1", num_text(q1), &q, false);
                        field(ui, "q2", "q2", q2.as_ref().map(num_text).unwrap_or_default(), &q, true);
                        field(ui, ts("from"), "from", num_text(from), lu, false);
                        field(ui, ts("to"), "to", to.as_ref().map(num_text).unwrap_or_default(), lu, true);
                        field(ui, ts("direction"), "angle", num_text(angle), "°", false);
                    }
                }
            });
            for (key, v) in edits {
                lj[key] = v;
                changed = true;
            }
            match l {
                Load::Moment { .. } => {
                    ui.label(RichText::new(ts("counter-clockwise positive")).small().weak());
                }
                Load::Distributed { axes, per, .. } => {
                    let mut local = *axes == strix_structure::LoadAxes::Local;
                    if ui
                        .checkbox(&mut local, ts("from the member"))
                        .on_hover_text(ts(
                            "The direction measured from the member's axis (−90: towards the reference fibres), not from the horizontal",
                        ))
                        .changed()
                    {
                        lj["axes"] = json!(if local { "local" } else { "global" });
                        changed = true;
                    }
                    let mut proj = *per == strix_structure::LoadPer::Projection;
                    if ui
                        .checkbox(&mut proj, ts("per projection"))
                        .on_hover_text(ts(
                            "The intensity per unit of the member's projection across the load (snow on a roof), not per unit of its length",
                        ))
                        .changed()
                    {
                        lj["per"] = json!(if proj { "projection" } else { "length" });
                        changed = true;
                    }
                }
                Load::Force { .. } => {}
            }
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

    /// Moves keep an expression one without piling up steps; a slid load keeps its `at`'s
    /// expression and writes nothing when it did not move; a grabbed node lands on the grid.
    #[test]
    fn moves_keep_expressions_without_piling_up_steps() {
        let l = Num::expr("L");
        let once = shifted(&l, 0.5);
        assert_eq!(once.expr.as_deref(), Some("L + 0.5"));
        let twice = shifted(&once, 0.5);
        assert_eq!(twice.expr.as_deref(), Some("L + 1"));
        let back = shifted(&shifted(&twice, -0.5), -0.5);
        assert_eq!(back.expr.as_deref(), Some("L"), "back to the expression itself");
        assert_eq!(shifted(&back, -0.25).expr.as_deref(), Some("L - 0.25"));
        assert_eq!(shifted(&Num::expr("L - 0.25"), 1.0).expr.as_deref(), Some("L + 0.75"));
        // A number that is not a move's step stays inside the expression.
        assert_eq!(split_step("L/2 + 1 - x"), ("L/2 + 1 - x", 0.0));
        assert_eq!(split_step("2 * (a + 1)"), ("2 * (a + 1)", 0.0));
        assert_eq!(split_step("L/2 - 0.5"), ("L/2", -0.5));
        assert_eq!(shifted(&Num::new(2.0), 0.5).value, 2.5);
        assert_eq!(shifted(&l, f64::NAN).expr.as_deref(), Some("L"));
        // Sliding a load along its member.
        let st = Structure {
            nodes: vec![
                strix_structure::Node { id: "A".into(), ..Default::default() },
                strix_structure::Node { id: "B".into(), x: Num::new(6.0), ..Default::default() },
            ],
            members: vec![strix_structure::Member { id: "1".into(), a: "A".into(), b: "B".into(), ..Default::default() }],
            loads: vec![
                Load::Force { node: None, member: Some("1".into()), at: Num::expr("L/2"), value: Num::new(10.0), angle: Num::new(-90.0) },
                Load::Moment { node: None, member: Some("1".into()), at: Num::new(2.0), value: Num::new(5.0) },
            ],
            ..Structure::default()
        };
        assert!(slid(&st, 0, 3.0, 3.0).is_none(), "no move, no change");
        let lj = slid(&st, 0, 3.0, 3.5).unwrap();
        assert_eq!(lj["at"], json!("L/2 + 0.5"), "{lj}");
        assert_eq!(lj["value"], json!(10.0), "{lj}");
        let lj = slid(&st, 1, 2.0, 1.5).unwrap();
        assert_eq!(lj["at"], json!(1.5), "{lj}");
        assert!(slid(&st, 7, 0.0, 1.0).is_none());
        // Dragging with the grid on: the node grabbed lands on it.
        let state = State::default();
        assert_eq!(step((0.0, 0.0), (0.6, 0.1), Some((0.3, 0.0)), &state), (0.7, 0.0));
        assert_eq!(step((0.0, 0.0), (0.6, 0.1), None, &state), (0.5, 0.0));
        assert_eq!(grabbed(&st, &Sel::Member("1".into()), (5.0, 0.0)), Some((6.0, 0.0)), "the nearer end");
        let free = State { snap: false, ..State::default() };
        assert_eq!(step((0.0, 0.0), (0.6, 0.1), Some((0.3, 0.0)), &free), (0.6, 0.1));
    }

    /// Loads are selected by index: when their number changes, they leave the selection.
    #[test]
    fn selected_loads_go_when_the_loads_are_renumbered() {
        let mut app = WordApp::new(Session::new(wordcraft_doc::Document::new()), crate::Services::default());
        app.run("structure.insert", json!({"template": "frame"})).unwrap();
        let mut st = resolved(&mut app);
        let mut state = State { sel: vec![Sel::Load(0), Sel::Node("A".into())], ..State::default() };
        state.prune(&st);
        assert_eq!(state.sel.len(), 2, "first look: kept");
        state.gesture = Some(Gesture::Slide { load: 0, start: 0.0, at: 1.0 });
        st.loads.remove(0);
        state.prune(&st);
        assert_eq!(state.sel, vec![Sel::Node("A".into())]);
        assert!(state.gesture.is_none());
    }

    /// The full-page editor: Esc goes one step at a time and leaves only with nothing selected;
    /// the arrows move the selection; Ctrl+A selects on the sheet, not in the document; Ctrl+Z
    /// undoes there although the page had the keyboard when the editor opened.
    #[test]
    fn the_editor_takes_its_keys_one_step_at_a_time() {
        let mut a = WordApp::new(Session::new(wordcraft_doc::Document::new()), crate::Services::default());
        a.run("view.commentsPane", json!({"value": false})).unwrap();
        a.run("structure.insert", json!({"template": "beam"})).unwrap();
        a.run("ui.structureEdit", json!({})).unwrap();
        // As after a double-click on the page: its focus flag is still on.
        a.canvas.focused = true;
        // The tables' boxes have no accessible names yet (as in the pane): keys only here.
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1440.0, 1000.0))
            .with_step_dt(1.0 / 60.0)
            .with_accessibility_check(false)
            .build_ui_state(
                |ui, app: &mut WordApp| {
                    let ctx = ui.ctx().clone();
                    app.logic(&ctx);
                    app.ui(ui);
                },
                a,
            );
        h.run_steps(4);
        assert!(h.state().structure_edit.is_some());
        let xb = |h: &mut egui_kittest::Harness<'_, WordApp>| resolved(h.state_mut()).node("B").unwrap().x.value;
        let x0 = xb(&mut h);
        // Ctrl+A selects the structure's nodes and members: the editor stays open.
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        h.run_steps(2);
        assert!(h.state().structure_edit.is_some(), "Ctrl+A stays in the editor");
        // The arrow moves the selection by the grid.
        h.key_press(egui::Key::ArrowRight);
        h.run_steps(2);
        assert!((xb(&mut h) - x0 - 0.5).abs() < 1e-9, "moved by the grid");
        // Esc clears the selection only.
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        assert!(h.state().structure_edit.is_some(), "the first Esc deselects");
        // Ctrl+Z undoes the move in the editor.
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
        h.run_steps(2);
        assert!((xb(&mut h) - x0).abs() < 1e-9, "undone");
        assert!(h.state().structure_edit.is_some(), "the structure stays selected");
        // Nothing left to cancel: Esc goes back to the page.
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        assert!(h.state().structure_edit.is_none(), "the second Esc leaves");
        assert!(h.state().canvas.focused, "the page takes the keyboard again");
    }

    /// The CAD editor does not open over the structure editor.
    #[test]
    fn one_editor_at_a_time() {
        let mut a = WordApp::new(Session::new(wordcraft_doc::Document::new()), crate::Services::default());
        a.run("structure.insert", json!({"template": "beam"})).unwrap();
        a.run("ui.structureEdit", json!({})).unwrap();
        assert!(crate::cad_editor::open(&mut a).is_err());
        assert!(a.cad.is_none());
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
        assert_eq!(h.state().ui.tab, EDITOR_TAB, "its tab of tools comes up");
        for _ in 0..3 {
            h.step();
        }
        h.state_mut().run("ui.structureClose", json!({})).unwrap();
        h.step();
        assert!(h.state().structure_edit.is_none());
        assert_ne!(h.state().ui.tab, EDITOR_TAB);
    }

    /// The editor's tab drives its tools, snapping, grid, view and dimensions by command.
    #[test]
    fn the_editor_tab_commands_set_the_tools() {
        let mut a = WordApp::new(Session::new(wordcraft_doc::Document::new()), crate::Services::default());
        let ctx = egui::Context::default();
        a.ctx = Some(ctx.clone());
        a.run("structure.insert", json!({"template": "beam"})).unwrap();
        a.run("ui.structureEdit", json!({})).unwrap();
        assert_eq!(a.ui.tab, EDITOR_TAB);
        assert_eq!(tools(&ctx).tool, "select");
        for name in ["node", "member", "fixed", "pin", "roller", "link", "slider", "hinge", "force", "couple", "distributed", "erase", "select"] {
            a.run("ui.structureTool", json!({"tool": name})).unwrap();
            assert_eq!(tools(&ctx).tool, name);
        }
        assert!(a.run("ui.structureTool", json!({"tool": "nope"})).is_err());
        assert!(tools(&ctx).snap && tools(&ctx).dims);
        a.run("ui.structureSnap", json!({})).unwrap();
        a.run("ui.structureDimensions", json!({"on": false})).unwrap();
        assert!(!tools(&ctx).snap && !tools(&ctx).dims);
        a.run("ui.structureGrid", json!({"size": 0.25})).unwrap();
        assert!((tools(&ctx).grid - 0.25).abs() < 1e-12);
        for bad in [json!({"size": -1.0}), json!({"size": "x"}), json!({})] {
            assert!(a.run("ui.structureGrid", bad).is_err());
        }
        a.run("ui.structureFit", json!({})).unwrap();
        a.run("ui.structureClose", json!({})).unwrap();
        assert_ne!(a.ui.tab, EDITOR_TAB);
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
