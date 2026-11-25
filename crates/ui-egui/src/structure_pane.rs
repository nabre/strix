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

use crate::{WordApp, theme::Tokens};

mod edits;
mod grips;
mod menu;
mod rotate;

/// Screen pixels per paper millimetre on the sheet (the symbols' and texts' sizes).
const PX_PER_MM: f32 = 4.0;
/// How near (px) the pointer snaps to a node or picks something.
const PICK: f32 = 10.0;

/// What a click on the sheet does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Tool {
    #[default]
    Select,
    Rotate,
    Node,
    Member,
    Support(SupportKind),
    Hinge,
    Force,
    Couple,
    Distributed,
    Erase,
}

/// What `Tool::Erase` would delete at the pointer.
#[derive(Clone, Debug, PartialEq, Eq)]
enum EraseTarget {
    Load(usize),
    Support(String),
    Node(String),
    Member(String),
}

/// The tools as `ui.structureTool` names them.
const TOOL_NAMES: [(Tool, &str); 15] = [
    (Tool::Select, "select"),
    (Tool::Rotate, "rotate"),
    (Tool::Node, "node"),
    (Tool::Member, "member"),
    (Tool::Support(SupportKind::Fixed), "fixed"),
    (Tool::Support(SupportKind::Pin), "pin"),
    (Tool::Support(SupportKind::RollerCircle), "rollerCircle"),
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

    fn icon(self) -> &'static str {
        match self {
            Tool::Select => "cadSelect",
            Tool::Rotate => "structureRotate",
            Tool::Node => "structureNode",
            Tool::Member => "structureMember",
            Tool::Support(SupportKind::Fixed) => "structureFixed",
            Tool::Support(SupportKind::Pin) => "structurePin",
            Tool::Support(SupportKind::Roller) => "structureRoller",
            Tool::Support(SupportKind::RollerCircle) => "structureRollerCircle",
            Tool::Support(SupportKind::Link) => "structureLink",
            Tool::Support(SupportKind::Slider) => "structureSlider",
            Tool::Hinge => "structureHinge",
            Tool::Force => "structureForce",
            Tool::Couple => "structureCouple",
            Tool::Distributed => "structureDistributed",
            Tool::Erase => "cadDelete",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Rotate => "Rotate",
            Tool::Node => "Node",
            Tool::Member => "Member",
            Tool::Support(SupportKind::Fixed) => "Fixed",
            Tool::Support(SupportKind::Pin) => "Pin",
            Tool::Support(SupportKind::Roller) => "Roller (triangle)",
            Tool::Support(SupportKind::RollerCircle) => "Roller",
            Tool::Support(SupportKind::Link) => "Link",
            Tool::Support(SupportKind::Slider) => "Slider",
            Tool::Hinge => "Internal hinge",
            Tool::Force => "Force",
            Tool::Couple => "Couple",
            Tool::Distributed => "Distributed",
            Tool::Erase => "Erase",
        }
    }

    /// Step-by-step guidance shown in the active command banner above the sheet.
    fn prompt(self, has_start: bool, has_rot: bool) -> &'static str {
        match self {
            Tool::Select => "Click, Shift+click or drag a window · Tab cycles overlapping items · R rotates vectors",
            Tool::Rotate if has_rot => "Drag on the protractor or click an angle to incline (Shift: 1° precision · arrows: ±15°)",
            Tool::Rotate => "Click a force, distributed load or support to incline it with the protractor",
            Tool::Node => "Click on the sheet (or type x, y below) to add a node",
            Tool::Member if has_start => "Click the end point (Shift: 0°/45°/90° · right-click or Esc finishes the chain)",
            Tool::Member => "Click the first point of the member (or type x, y / @L<α below)",
            Tool::Support(_) => "Click a node to place the support (or drag from the node to orient its angle)",
            Tool::Hinge => "Click a node or near a member's end to toggle an internal hinge",
            Tool::Force => "Click a node or member to add a force (or drag to aim its inclination)",
            Tool::Couple => "Click a node or member to add a couple",
            Tool::Distributed => "Click a member to apply a distributed load",
            Tool::Erase => "Click a load, support, member or node to erase it",
        }
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
    /// Rotating a vector or support on the protractor (`Tool::Rotate`).
    Rotate { target: rotate::RotTarget, grab_deg: f64, start_vis: f64, to: Pos2, moved: bool },
    /// Placing a force with `Tool::Force` and dragging to aim its inclination.
    AimForce { place: edits::Place, from: Pos2, to: Pos2 },
    /// Placing a support with `Tool::Support` and dragging to orient its angle.
    AimSupport { node: String, kind: SupportKind, from: Pos2, to: Pos2 },
}

/// The editor's state between frames.
#[derive(Clone, Debug)]
struct State {
    tool: Tool,
    /// The selection, in the order it was made.
    sel: Vec<Sel>,
    /// Index for cycling through overlapping hits (`Alt+Click` or `Tab`).
    cycle: usize,
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
    /// The structure's length unit when last seen: when it changes the structure's numbers were
    /// converted, and the view and the grid are rescaled to show the same thing.
    unit: Option<strix_structure::LengthUnit>,
}

impl Default for State {
    fn default() -> Self {
        State {
            tool: Tool::Select,
            sel: Vec::new(),
            cycle: 0,
            gesture: None,
            view: None,
            fitted_for: String::new(),
            start: None,
            grid: 0.5,
            snap: true,
            dims: true,
            typed: String::new(),
            loads: None,
            unit: None,
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

    /// Selects a newly appended load at `idx` (`prev_len`) and updates `self.loads` so `prune`
    /// keeps it selected on the next frame.
    fn select_new_load(&mut self, idx: usize) {
        self.loads = Some(idx + 1);
        self.select(Sel::Load(idx));
    }

    /// Updates load indices in the selection after load `removed` was deleted.
    fn on_load_removed(&mut self, removed: usize, prev_len: usize) {
        self.loads = Some(prev_len.saturating_sub(1));
        self.sel.retain_mut(|s| match s {
            Sel::Load(k) if *k == removed => false,
            Sel::Load(k) => {
                if *k > removed {
                    *k -= 1;
                }
                true
            }
            _ => true,
        });
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
        if let Some(old) = self.unit
            && old != st.length_unit
        {
            rescale(self, old.si() / st.length_unit.si());
        }
        self.unit = Some(st.length_unit);
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
        if let Some(Gesture::Grip {
            grip: grips::Grip::Point(k) | grips::Grip::Tail(k) | grips::Grip::DistAngle(k) | grips::Grip::From(k) | grips::Grip::To(k),
            ..
        }) = &self.gesture
            && (renumbered || *k >= n)
        {
            self.gesture = None;
        }
        if let Some(Gesture::Rotate { target, .. }) = &self.gesture {
            let gone = match target {
                rotate::RotTarget::Force(k) | rotate::RotTarget::Distributed(k) => renumbered || *k >= n,
                rotate::RotTarget::Support(id) => !st.supports.iter().any(|s| s.node == *id),
            };
            if gone {
                self.gesture = None;
            }
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
    if ui
        .horizontal_wrapped(|ui| {
            if ui
                .button(RichText::new(ts("Edit in Full Page")).strong())
                .on_hover_text(ts("Open the structure in its editor, in place of the page (or double-click the structure)"))
                .clicked()
            {
                ui.data_mut(|d| d.insert_temp(state_id(), state.clone()));
                let _ = app.run("ui.structureEdit", json!({}));
                return true;
            }
            if ui.button(ts("Write Working")).on_hover_text(ts(WORKING_TIP)).clicked() {
                let _ = app.run("structure.working", working_labels());
            }
            false
        })
        .inner
    {
        return;
    }
    status_line(ui, &info);
    toolbar(ui, &mut state);
    active_command_bar(ui, &st, &mut state);
    let h = (ui.available_height() * 0.48).clamp(120.0, 440.0);
    sheet(app, ui, &src, &st, &mut state, h, false);
    typed_point(app, ui, &src, &st, &mut state);
    egui::ScrollArea::vertical().id_salt("structure_pane_scroll").show(ui, |ui| {
        inspector(app, ui, &info, &src, &st, &mut state);
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
    let side_w = 370.0_f32.min(full.width() * 0.42);
    let main = Rect::from_min_max(full.min, pos2(full.max.x - side_w, full.max.y));
    let side = Rect::from_min_max(pos2(full.max.x - side_w, full.min.y), full.max);
    let mut close_now = false;
    let mut esc_free = false;
    ui.scope_builder(egui::UiBuilder::new().max_rect(main.shrink(8.0)).layout(egui::Layout::top_down(egui::Align::Min)), |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(ts("Structure Editor")).heading().strong());
            ui.add_space(12.0);
            status_line(ui, &info);
        });
        order_hint(app, ui, &st, &mut state);
        active_command_bar(ui, &st, &mut state);
        // Its tools, Write Working and Close Editor are on its ribbon tab (EDITOR_TAB).
        let h = ui.available_height() - 34.0;
        esc_free = sheet(app, ui, &src, &st, &mut state, h, true);
        typed_point(app, ui, &src, &st, &mut state);
    });
    let tokens = Tokens::get(ui.ctx());
    ui.painter().rect_filled(side, 0.0, tokens.panel);
    ui.painter().vline(side.min.x, side.y_range(), Stroke::new(1.0, tokens.border));
    ui.scope_builder(egui::UiBuilder::new().max_rect(side.shrink(8.0)).layout(egui::Layout::top_down(egui::Align::Min)), |ui| {
        egui::ScrollArea::vertical().id_salt("structure_editor_side").show(ui, |ui| {
            inspector(app, ui, &info, &src, &st, &mut state);
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
    // A drawing of the structure's working (a free body, a stretch), which a change writes
    // again, opens the structure itself, which its changes are made to.
    wordcraft_engine::cmd::structure::select_base(&mut app.session);
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

/// UI commands (`ui.structure*`): `ui.structureEdit` opens the full-page editor (`ui.structureNew`,
/// `{"template"?}`: on a new structure),
/// `ui.structureClose` closes it; the editor's tab: `ui.structureTool` (`{"tool": "select" |
/// "node" | "member" | "fixed" | "pin" | "roller" | "rollerCircle" | "link" | "slider" | "hinge" |
/// "force" | "couple" | "distributed" | "erase"}`), `ui.structureSnap` and `ui.structureDimensions`
/// (`{"on"?: bool}`, without it they toggle), `ui.structureGrid` (`{"size": number}`) and
/// `ui.structureFit`.
pub(crate) fn command(app: &mut WordApp, id: &str, p: &Value) -> Result<Value, String> {
    let on = p.get("on").and_then(Value::as_bool);
    match id {
        "ui.structureEdit" => {
            open(app)?;
            Ok(json!({"open": true}))
        }
        "ui.structureNew" => {
            let template = p.get("template").and_then(Value::as_str).unwrap_or("beam");
            app.run("structure.insert", json!({"template": template}))?;
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
            with_state(app, |s| s.grid = size.clamp(MIN_GRID, MAX_GRID))
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
            let _ = app.run("ui.structureNew", json!({"template": template}));
        }
    }
}

/// `structure.working`'s words in the interface's language.
pub(crate) fn working_labels() -> Value {
    json!({"labels": {
        "heading": ts("Working"),
        "loads": ts("Loads"),
        "reactions": ts("Reactions"),
        "signs": ts("Reactions are taken positive the way their arrows point in the free body: a negative one acts the other way."),
        "flipped": ts("From here on the negative reactions are taken the way they act, as the drawings show them, with positive values:"),
        "hyperstatic": ts("Hyperstatic: from the solution by the stiffness method"),
        "together": ts("From the equations solved together"),
        "force1": ts("Hyperstatic structure of degree 1, solved by the force method. Redundant: {X}. Without the constraint that gives it, the structure left (the released structure) is isostatic."),
        "force2": ts("Hyperstatic structure of degree 2, solved by the force method. Redundants: {X}. Without the constraints that give them, the structure left (the released structure) is isostatic."),
        "system0": ts("System 0, the released structure under the loads:"),
        "system": ts("System {i}, the released structure under only"),
        "congruence": ts("Congruence: where a constraint was taken away, the real structure doesn't move along its redundant. The displacements of the released structure by the principle of virtual work,"),
        "eiconst": ts("(EI constant: multiplied by EI; axial strains neglected):"),
        "eaconst": ts("(EA constant: multiplied by EA):"),
        "noaxial": ts("(axial strains neglected):"),
        "cramer": ts("Solved by Cramer's rule (δ_21 = δ_12):"),
        "forces": ts("Internal forces"),
        "conventions": ts("x from the member's first node; N positive in tension, T positive turning clockwise, M positive stretching the reference fibres."),
        "member": ts("Member"),
        "axial": ts("Axial forces"),
        "hinge": ts("part about the hinge"),
        "range": ts("x from {a} to {b}"),
        "stretch": ts("Stretch"),
        "values": ts("Values at the ends of the stretch:"),
        "valuesMax": ts("Values at the ends of the stretch and at M's extremes:"),
        "unsolved": ts("The structure can't be solved: its working will be written again as soon as it can."),
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
    let tokens = Tokens::get(ui.ctx());
    let (text, color) = match status {
        "isostatic" => (ts("Isostatic").to_string(), tokens.accent_text),
        "hyperstatic" => (crate::i18n::fmt(ts("Hyperstatic, degree {n}"), &[("n", &degree)]), tokens.text),
        "labile" => (crate::i18n::fmt(ts("Labile: {n} degree(s) of freedom"), &[("n", &degree)]), ui.visuals().error_fg_color),
        "empty" => (ts("No members yet: draw them with Member").to_string(), tokens.text_dim),
        _ => (info["message"].as_str().unwrap_or_default().to_string(), ui.visuals().error_fg_color),
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new(text).strong().color(color));
        if info["truss"].as_bool().unwrap_or(false) {
            ui.label(RichText::new(ts("(truss)")).weak());
        }
    });
    for e in info["errors"].as_array().into_iter().flatten().filter_map(Value::as_str).take(4) {
        ui.label(RichText::new(e).small().color(ui.visuals().error_fg_color));
    }
}

/// Names that don't follow the structure (members `3`, `1`, `2` from left to right, nodes `D`,
/// `A`, `B`, `C`): a line saying so with one click to rename them in order (`structure.renumber`).
/// The working goes through the members left to right whatever their names.
fn order_hint(app: &mut WordApp, ui: &mut Ui, st: &Structure, state: &mut State) {
    if st.members.len() < 2 || st.names_in_order() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(ts("The names don't follow the structure from left to right.")).color(ui.visuals().warn_fg_color));
        if ui.button(ts("Rename in Order")).on_hover_text(ts(RENUMBER_TIP)).clicked() && app.run("structure.renumber", json!({})).is_ok() {
            // The names change: what was selected by them is let go.
            state.sel.clear();
        }
    });
}

const WORKING_TIP: &str = "Write after the structure its solution as live calculations: the equilibrium equations giving the reactions, then each member's laws N(x), T(x), M(x) with their values";

/// What Rename in Order does.
pub(crate) const RENUMBER_TIP: &str =
    "Name the members 1, 2, 3… from left to right and the nodes A, B, C… in the same order, so the drawing and the working read in order";

/// What the editor's Dimensions switch does.
pub(crate) const DIMS_TIP: &str = "Show on the sheet the dimensions, the slopes of inclined members and the angles of inclined forces (the document draws them as Structures › Dimensions says)";

/// Banner shown above the sheet: highlights the active command with its icon, step prompt, and
/// a one-click return to Select (`Esc`), or shows contextual selection hints in `Select` mode.
fn active_command_bar(ui: &mut Ui, st: &Structure, state: &mut State) {
    let has_rot = rotate::active_target(st, &state.sel).is_some();
    let active = state.tool != Tool::Select;
    let tokens = Tokens::get(ui.ctx());
    let (bg, border, badge_col) = match state.tool {
        Tool::Select => (tokens.panel, tokens.border, tokens.text),
        Tool::Erase => (tokens.panel, ui.visuals().error_fg_color, ui.visuals().error_fg_color),
        _ => (tokens.checked, tokens.accent, tokens.accent_text),
    };
    egui::Frame::NONE
        .fill(bg)
        .stroke(Stroke::new(if active { 1.2 } else { 0.8 }, border))
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                let (ic_rect, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                crate::icons::paint(ui.painter(), ic_rect, state.tool.icon(), badge_col, badge_col);
                let title = if state.tool == Tool::Member {
                    format!("{} ({}/2)", ts(state.tool.label()), if state.start.is_some() { "2" } else { "1" })
                } else {
                    ts(state.tool.label()).to_string()
                };
                ui.label(RichText::new(title).strong().color(badge_col));
                if active && ui.small_button(ts("Select tool")).on_hover_text(ts("Return to the Select tool (Esc)")).clicked() {
                    state.tool = Tool::Select;
                    state.start = None;
                    state.gesture = None;
                }
            });
            ui.label(RichText::new(ts(state.tool.prompt(state.start.is_some(), has_rot))).small().color(tokens.text));
        });
    ui.add_space(2.0);
}

fn toolbar(ui: &mut Ui, state: &mut State) {
    let groups: [(&str, &[Tool]); 4] = [
        ("Selection", &[Tool::Select, Tool::Rotate, Tool::Erase]),
        ("Draw", &[Tool::Node, Tool::Member, Tool::Hinge]),
        (
            "Supports",
            &[
                Tool::Support(SupportKind::Fixed),
                Tool::Support(SupportKind::Pin),
                Tool::Support(SupportKind::RollerCircle),
                Tool::Support(SupportKind::Roller),
                Tool::Support(SupportKind::Link),
                Tool::Support(SupportKind::Slider),
            ],
        ),
        ("Loads", &[Tool::Force, Tool::Couple, Tool::Distributed]),
    ];
    for (heading, tools) in groups {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 3.0);
            ui.label(RichText::new(tl!(heading)).small().weak());
            for &tool in tools {
                let text = RichText::new(ts(tool.label()));
                let text = if tool == Tool::Erase { text.color(ui.visuals().error_fg_color) } else { text };
                if ui.add(egui::Button::new(text).small().selected(state.tool == tool)).on_hover_text(ts(tool.prompt(false, false))).clicked() {
                    state.tool = tool;
                    state.start = None;
                    state.gesture = None;
                }
            }
        });
    }
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut state.snap, ts("Snap")).on_hover_text(ts("Snap to the grid (always to the nodes)"));
        ui.label(ts("Grid"));
        ui.add(egui::DragValue::new(&mut state.grid).range(MIN_GRID..=MAX_GRID).speed(0.05).max_decimals(3));
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

/// Constrains `pt` to 0°/45°/90° from `start` when `shift` is held and not snapped to an existing node.
fn snap_member_end(start: Option<(f64, f64)>, pt: (f64, f64), on_node: bool, shift: bool, state: &State) -> (f64, f64) {
    let Some((x0, y0)) = start.filter(|_| shift && !on_node) else { return pt };
    let (dx, dy) = (pt.0 - x0, pt.1 - y0);
    let len = dx.hypot(dy);
    if len <= 1e-9 {
        return pt;
    }
    let ang = (dy.atan2(dx).to_degrees() / 45.0).round() * 45.0;
    let r = ang.to_radians();
    let (mut x, mut y) = (x0 + len * r.cos(), y0 + len * r.sin());
    if state.snap && state.grid > 0.0 {
        let g = state.grid;
        if r.sin().abs() < 1e-6 {
            x = (x / g).round() * g;
            y = y0;
        } else if r.cos().abs() < 1e-6 {
            x = x0;
            y = (y / g).round() * g;
        } else {
            let step = ((dx.abs().max(dy.abs())) / g).round() * g;
            x = x0 + step * r.cos().signum();
            y = y0 + step * r.sin().signum();
        }
    }
    (round(x), round(y))
}

fn dist(a: Pos2, b: Pos2) -> f64 {
    f64::from((a - b).length())
}

/// Screen bounding box of the support drawn on node `id`, if it has one.
fn support_rect(st: &Structure, v: View, id: &str) -> Option<Rect> {
    let n = st.node(id)?;
    let s = st.supports.iter().find(|s| s.node == id)?;
    let p = v.to_screen(n.x.value, n.y.value);
    let phi = s.angle.value.to_radians();
    let down = vec2(phi.sin() as f32, phi.cos() as f32);
    let along = vec2(phi.cos() as f32, -(phi.sin() as f32));
    let depth = match s.kind {
        SupportKind::Fixed => 2.2,
        SupportKind::Link => 7.0,
        SupportKind::Slider => 6.0,
        _ => 5.0,
    } * MM;
    let half = 4.0 * MM;
    let corners = [p - along * half, p + along * half, p + along * half + down * depth, p - along * half + down * depth];
    let mut r = Rect::NOTHING;
    for c in corners {
        r.extend_with(c);
    }
    Some(r)
}

/// Picks the node whose support symbol lies under screen point `p`.
fn pick_support_node(st: &Structure, v: View, p: Pos2) -> Option<String> {
    for s in st.supports.iter().rev() {
        let Some(n) = st.node(&s.node) else { continue };
        let np = v.to_screen(n.x.value, n.y.value);
        let phi = s.angle.value.to_radians();
        let down = vec2(phi.sin() as f32, phi.cos() as f32);
        let along = vec2(phi.cos() as f32, -(phi.sin() as f32));
        let rel = p - np;
        let u = rel.dot(along);
        let w = rel.dot(down);
        let depth = match s.kind {
            SupportKind::Fixed => 2.6,
            SupportKind::Link => 7.2,
            SupportKind::Slider => 6.2,
            _ => 5.2,
        } * MM;
        if u.abs() <= 4.2 * MM + 4.0 && w >= -2.0 && w <= depth + 5.0 {
            return Some(s.node.clone());
        }
    }
    None
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

/// Checks if `p` is near an end of a member (but outside `PICK` of the node itself), returning
/// `(member_id, is_end_b)` for toggling a member end hinge with `Tool::Hinge`.
fn pick_member_end(st: &Structure, v: View, p: Pos2) -> Option<(String, bool)> {
    if pick_node(st, v, p).is_some() {
        return None;
    }
    let (id, at) = pick_member(st, v, p)?;
    let m = st.member(&id)?;
    let len = st.length(m).filter(|l| *l > 0.0)?;
    let end_zone = (len * 0.25).min(24.0 / v.s.max(1.0));
    if at <= end_zone {
        Some((id, false))
    } else if at >= len - end_zone {
        Some((id, true))
    } else {
        None
    }
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
    if resp.clicked() || resp.drag_started() || resp.secondary_clicked() {
        resp.request_focus();
    }
    if resp.has_focus() {
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(resp.id, egui::EventFilter { tab: resp.hovered(), horizontal_arrows: true, vertical_arrows: true, escape: true })
        });
    }
    let key = st.nodes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>().join(",");
    if state.view.is_none() || (state.fitted_for.is_empty() && !key.is_empty()) {
        state.view = Some(fit(st, rect));
        state.fitted_for = key;
    }
    let (cx, cy, s) = state.view.unwrap_or((0.0, 0.0, 50.0));
    let mut v = View { rect, cx, cy, s };
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
            let candidates = hits(st, v, p);
            let chosen = if mods.alt && !candidates.is_empty() {
                state.cycle = state.cycle.wrapping_add(1);
                candidates.get(state.cycle % candidates.len()).cloned()
            } else {
                state.cycle = 0;
                candidates.into_iter().next()
            };
            match chosen {
                Some(hit) if adding => state.toggle(hit),
                Some(hit) => state.select(hit),
                None if adding => {}
                None => state.sel.clear(),
            }
        } else {
            click_with_mods(app, st, v, p, state, mods.shift);
        }
    }
    let focused = ui.ctx().memory(|m| m.focused());
    let other = focused.is_some_and(|id| id != resp.id);
    if !other && (full || resp.has_focus() || (resp.hovered() && focused.is_none())) {
        return keys(app, ui, &resp, src, st, v, state);
    }
    false
}

/// Primary-button drags across tools (`Select`, `Rotate`, `Force`, `Support`).
#[allow(clippy::too_many_arguments)]
fn drag(app: &mut WordApp, ui: &Ui, resp: &egui::Response, src: &Structure, st: &Structure, v: View, state: &mut State, adding: bool) {
    match state.tool {
        Tool::Select | Tool::Rotate | Tool::Force | Tool::Support(_) => {
            if resp.drag_started_by(egui::PointerButton::Primary)
                && let Some(p) = resp.interact_pointer_pos()
            {
                let origin = ui.input(|i| i.pointer.press_origin()).unwrap_or(p);
                start_gesture(st, v, state, adding, origin, p);
            }
        }
        _ => {
            state.gesture = None;
        }
    }

    if resp.dragged_by(egui::PointerButton::Primary)
        && let Some(p) = resp.interact_pointer_pos()
    {
        update_gesture(st, v, state, p);
    }

    if resp.drag_stopped_by(egui::PointerButton::Primary)
        && let Some(g) = state.gesture.take()
    {
        finish_gesture(app, resp, src, st, v, state, g);
    }
}

/// A primary-button drag starts at `origin` (the press) with the pointer now at `p`.
fn start_gesture(st: &Structure, v: View, state: &mut State, adding: bool, origin: Pos2, p: Pos2) {
    match state.tool {
        Tool::Select => state.gesture = select_gesture(st, v, state, adding, origin),
        Tool::Rotate => {
            let cur_active = rotate::active_target(st, &state.sel);
            let chosen = match &cur_active {
                Some(rt) if rotate::pivot(st, rt).is_some_and(|(wx, wy)| (v.to_screen(wx, wy) - origin).length() <= rotate::PROTRACTOR_R + 24.0) => {
                    Some(rt.clone())
                }
                _ => rotate::pick_rotatable(st, v, origin).or(cur_active),
            };
            if let Some(rt) = chosen
                && let Some((wx, wy)) = rotate::pivot(st, &rt)
                && let Some(start_vis) = rotate::visual_angle(st, &rt)
            {
                state.select(rotate::sel_of(&rt));
                let center = v.to_screen(wx, wy);
                let grab_deg = rotate::screen_angle(center, origin).unwrap_or(start_vis);
                state.gesture = Some(Gesture::Rotate { target: rt, grab_deg, start_vis, to: p, moved: false });
            }
        }
        Tool::Force => {
            let place = pick_node(st, v, origin).map(edits::Place::Node).or_else(|| {
                pick_member(st, v, origin).map(|(id, at)| {
                    let len = st.member(&id).and_then(|m| st.length(m)).unwrap_or(0.0);
                    edits::Place::Member(id, round(round_at(at, len, state)))
                })
            });
            if let Some(place) = place {
                state.gesture = Some(Gesture::AimForce { place, from: origin, to: p });
            }
        }
        Tool::Support(kind) => {
            if let Some(node) = pick_node(st, v, origin) {
                state.gesture = Some(Gesture::AimSupport { node, kind, from: origin, to: p });
            }
        }
        _ => {}
    }
}

/// The Select tool's drag pressed at `p`: a grip, a load slid along its member, a move of what is
/// under the pointer (selecting it first) or a selection window.
fn select_gesture(st: &Structure, v: View, state: &mut State, adding: bool, p: Pos2) -> Option<Gesture> {
    let grip = grips::hit(&grips::grips(st, v, &state.sel), p);
    match (grip, pick(st, v, p)) {
        (Some(grip), _) => Some(Gesture::Grip { grip, to: p }),
        (None, Some(Sel::Load(k))) => {
            if !state.has(&Sel::Load(k)) {
                state.select(Sel::Load(k));
            }
            match load_on_member(st, k) {
                Some((_, at)) => Some(Gesture::Slide { load: k, start: at, at }),
                // A distributed load slides along its run of aligned members.
                None if matches!(st.loads.get(k), Some(Load::Distributed { .. })) => {
                    grips::body(st, v, k, p).map(|grip| Gesture::Grip { grip, to: p })
                }
                None => Some(Gesture::Grip { grip: grips::Grip::Point(k), to: p }),
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
    }
}

/// The gesture under way follows the pointer at `p`.
fn update_gesture(st: &Structure, v: View, state: &mut State, p: Pos2) {
    let (snap_on, grid) = (state.snap, state.grid);
    match &mut state.gesture {
        Some(Gesture::Move { to, .. }) => *to = v.to_world(p),
        Some(Gesture::Slide { load, at, .. }) => {
            if let Some((a, len)) = along(st, *load, v, p) {
                let r = if snap_on && grid > 0.0 { (a / grid).round() * grid } else { (a * 1000.0).round() / 1000.0 };
                *at = r.clamp(0.0, len);
            }
        }
        Some(Gesture::Window { to, .. } | Gesture::Grip { to, .. } | Gesture::AimForce { to, .. } | Gesture::AimSupport { to, .. }) => {
            *to = p;
        }
        Some(Gesture::Rotate { to, moved, .. }) => {
            if (*to - p).length() > 0.5 {
                *moved = true;
            }
            *to = p;
        }
        None => {}
    }
}

/// The drag ended: the gesture `g` is committed to the session.
fn finish_gesture(app: &mut WordApp, resp: &egui::Response, src: &Structure, st: &Structure, v: View, state: &mut State, g: Gesture) {
    let shift = resp.ctx.input(|i| i.modifiers.shift);
    let snap_rot = state.snap != shift;
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
            grips::commit(app, src, st, v, state, &grip, to, shift);
        }
        Gesture::Window { from, to } => {
            let r = Rect::from_two_pos(from, to);
            let taken = if to.x < from.x { grips::crossing(st, v, r) } else { inside(st, v, r) };
            for s in taken {
                if !state.has(&s) {
                    state.sel.push(s);
                }
            }
        }
        Gesture::Rotate { target, grab_deg, start_vis, to, moved } => {
            if moved
                && let Some((wx, wy)) = rotate::pivot(st, &target)
                && let Some(cur_deg) = rotate::screen_angle(v.to_screen(wx, wy), to)
            {
                let raw_vis = rotate::norm_deg(start_vis + rotate::ang_diff(cur_deg, grab_deg));
                let (vis, _) = rotate::snap_visual_angle(st, &target, raw_vis, snap_rot);
                if let Some(stored) = rotate::stored_angle_for_visual(st, &target, vis) {
                    rotate::commit_target(app, src, st, &target, stored, true);
                }
            }
        }
        Gesture::AimForce { place, from, to } => {
            let aimed = (to - from).length() >= 9.0;
            if aimed || !resp.clicked() {
                let deg = if aimed { aimed_force_angle(st, &place, from, to, snap_rot) } else { -90.0 };
                let load = match place {
                    edits::Place::Node(id) => json!({"type": "force", "node": id, "value": 10.0, "angle": round(deg)}),
                    edits::Place::Member(id, at) => json!({"type": "force", "member": id, "at": round(at), "value": 10.0, "angle": round(deg)}),
                };
                let idx = st.loads.len();
                if app.run("structure.load", json!({"load": load})).is_ok() {
                    state.select_new_load(idx);
                }
            }
        }
        Gesture::AimSupport { node, kind, from, to } => {
            let aimed = (to - from).length() >= 10.0;
            if aimed || !resp.clicked() {
                let deg = if aimed { aimed_support_angle(st, &node, kind, from, to, snap_rot) } else { 0.0 };
                if app.run("structure.support", json!({"node": node, "kind": kind.name(), "angle": round(deg)})).is_ok() {
                    state.select(Sel::Node(node));
                }
            }
        }
    }
}

/// The stored angle of a new force at `place` aimed by dragging from `from` to `to`, snapped like
/// the protractor.
fn aimed_force_angle(st: &Structure, place: &edits::Place, from: Pos2, to: Pos2, snap_rot: bool) -> f64 {
    let raw = rotate::screen_angle(from, to).unwrap_or(-90.0);
    let mut temp = st.clone();
    let idx = temp.loads.len();
    temp.loads.push(match place {
        edits::Place::Node(id) => {
            Load::Force { node: Some(id.clone()), member: None, at: Num::default(), value: Num::new(10.0), angle: Num::new(raw) }
        }
        edits::Place::Member(id, at) => {
            Load::Force { node: None, member: Some(id.clone()), at: Num::new(*at), value: Num::new(10.0), angle: Num::new(raw) }
        }
    });
    rotate::snap_visual_angle(&temp, &rotate::RotTarget::Force(idx), raw, snap_rot).0
}

/// The stored angle of a new support of `kind` at `node` aimed by dragging from `from` to `to`,
/// snapped like the protractor.
fn aimed_support_angle(st: &Structure, node: &str, kind: SupportKind, from: Pos2, to: Pos2, snap_rot: bool) -> f64 {
    let raw_vis = rotate::screen_angle(from, to).unwrap_or(-90.0);
    let mut temp = st.clone();
    temp.supports.retain(|s| s.node != node);
    temp.supports.push(strix_structure::Support { node: node.to_string(), kind, angle: Num::new(0.0) });
    let rt = rotate::RotTarget::Support(node.to_string());
    let (vis, _) = rotate::snap_visual_angle(&temp, &rt, raw_vis, snap_rot);
    rotate::norm_deg(vis + 90.0)
}

/// `(preview_structure, target, vis_deg, start_vis, snap_ref)` of a rotation or aim under way.
type RotState = (Structure, rotate::RotTarget, f64, Option<f64>, Option<rotate::SnapRef>);

/// Computes the [`RotState`] for an active rotation or aim gesture.
fn gesture_rotation_state(st: &Structure, v: View, state: &State, shift: bool) -> Option<RotState> {
    let snap_rot = state.snap != shift;
    match &state.gesture {
        Some(Gesture::Rotate { target, grab_deg, start_vis, to, .. }) => {
            let (wx, wy) = rotate::pivot(st, target)?;
            let center = v.to_screen(wx, wy);
            let cur_deg = rotate::screen_angle(center, *to).unwrap_or(*grab_deg);
            let raw_vis = rotate::norm_deg(start_vis + rotate::ang_diff(cur_deg, *grab_deg));
            let (vis, sref) = rotate::snap_visual_angle(st, target, raw_vis, snap_rot);
            let stored = rotate::stored_angle_for_visual(st, target, vis)?;
            Some((rotate::preview_target(st, target, stored), target.clone(), vis, Some(*start_vis), sref))
        }
        Some(Gesture::AimForce { place, from, to }) if (*to - *from).length() >= 9.0 => {
            let raw = rotate::screen_angle(*from, *to).unwrap_or(-90.0);
            let mut out = st.clone();
            let idx = out.loads.len();
            out.loads.push(match place {
                edits::Place::Node(id) => {
                    Load::Force { node: Some(id.clone()), member: None, at: Num::default(), value: Num::new(10.0), angle: Num::new(-90.0) }
                }
                edits::Place::Member(id, at) => {
                    Load::Force { node: None, member: Some(id.clone()), at: Num::new(*at), value: Num::new(10.0), angle: Num::new(-90.0) }
                }
            });
            let rt = rotate::RotTarget::Force(idx);
            let (vis, sref) = rotate::snap_visual_angle(&out, &rt, raw, snap_rot);
            out = rotate::preview_target(&out, &rt, vis);
            Some((out, rt, vis, None, sref))
        }
        Some(Gesture::AimSupport { node, kind, from, to }) if (*to - *from).length() >= 10.0 => {
            let raw = rotate::screen_angle(*from, *to).unwrap_or(-90.0);
            let mut out = st.clone();
            out.supports.retain(|s| s.node != *node);
            out.supports.push(strix_structure::Support { node: node.clone(), kind: *kind, angle: Num::new(0.0) });
            let rt = rotate::RotTarget::Support(node.clone());
            let (vis, sref) = rotate::snap_visual_angle(&out, &rt, raw, snap_rot);
            let stored = rotate::norm_deg(vis + 90.0);
            out = rotate::preview_target(&out, &rt, stored);
            Some((out, rt, vis, None, sref))
        }
        _ => None,
    }
}

/// The sheet: grid, structure (with the drag under way), selection, hover, protractor, tool ghosts,
/// selection window, the member being drawn and the HUD readouts.
fn paint_sheet(ui: &Ui, resp: &egui::Response, st: &Structure, v: View, state: &State) {
    let rect = v.rect;
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, Color32::WHITE);
    grid_lines(&painter, v, state.grid);
    let shift = ui.input(|i| i.modifiers.shift);

    let rot_state = gesture_rotation_state(st, v, state, shift);
    let mut shown = match &rot_state {
        Some((preview_st, ..)) => preview_st.clone(),
        None => gesture_preview(st, v, state, shift),
    };
    shown.diagrams.dimensions = state.dims;
    let origin = v.to_world(rect.min);
    let prims = shown.schema_prims(v.s / f64::from(PX_PER_MM), origin, strix_calc::Options::default());
    paint_prims(&painter, rect.min, &prims);

    let accent = Color32::from_rgb(0xE0, 0x7B, 0x1F);
    let crimson = Color32::from_rgb(0xDC, 0x26, 0x26);
    let hover = resp.hover_pos();

    let hovered_select = hover.filter(|_| state.tool == Tool::Select && state.gesture.is_none()).and_then(|p| pick(&shown, v, p));
    let hovered_erase = hover.filter(|_| state.tool == Tool::Erase && state.gesture.is_none()).and_then(|p| pick_erase(&shown, v, p));
    let hovered_rot = hover.filter(|_| state.tool == Tool::Rotate && state.gesture.is_none()).and_then(|p| rotate::pick_rotatable(&shown, v, p));

    if let Some(h) = &hovered_select
        && !state.has(h)
    {
        highlight(&painter, &shown, v, h, accent.linear_multiply(0.4));
    }
    if let Some(et) = &hovered_erase {
        match et {
            EraseTarget::Load(k) => highlight(&painter, &shown, v, &Sel::Load(*k), crimson),
            EraseTarget::Support(id) => {
                if let Some(sr) = support_rect(&shown, v, id) {
                    painter.rect_filled(sr.expand(2.0), 3.0, crimson.linear_multiply(0.18));
                    painter.rect_stroke(sr.expand(2.0), 3.0, Stroke::new(1.8, crimson), egui::StrokeKind::Outside);
                }
            }
            EraseTarget::Node(id) => highlight(&painter, &shown, v, &Sel::Node(id.clone()), crimson),
            EraseTarget::Member(id) => highlight(&painter, &shown, v, &Sel::Member(id.clone()), crimson),
        }
    }
    for s in &state.sel {
        highlight(&painter, &shown, v, s, accent);
    }
    if let Some(Sel::Member(id)) = state.primary() {
        grips::paint_member_cue(&painter, &shown, v, id);
    }

    paint_rotation(&painter, &shown, v, state, rot_state.as_ref(), hovered_rot.as_ref());

    let handles = if state.tool == Tool::Select { grips::grips(&shown, v, &state.sel) } else { Vec::new() };
    let dragged_grip = if let Some(Gesture::Grip { grip, to }) = &state.gesture { Some((grip, *to, shift)) } else { None };
    if state.tool == Tool::Select {
        grips::paint(&painter, st, v, state, &handles, hover, dragged_grip);
    }

    let mut over_tool_target = hovered_erase.is_some() || hovered_rot.is_some();
    if let Some(p) = hover.filter(|_| state.gesture.is_none()) {
        paint_tool_ghost(&painter, &shown, v, p, state, accent, &mut over_tool_target);
    }

    let over_grip = (state.tool == Tool::Select && hover.is_some_and(|p| grips::hit(&handles, p).is_some()))
        || (state.tool == Tool::Rotate
            && rotate::active_target(&shown, &state.sel).and_then(|rt| rotate::pivot(&shown, &rt)).zip(hover).is_some_and(|((wx, wy), p)| {
                let d = (v.to_screen(wx, wy) - p).length();
                (d - rotate::PROTRACTOR_R).abs() <= 14.0
            }));
    let dragging = matches!(
        state.gesture,
        Some(
            Gesture::Grip { .. }
                | Gesture::Move { .. }
                | Gesture::Slide { .. }
                | Gesture::Rotate { .. }
                | Gesture::AimForce { .. }
                | Gesture::AimSupport { .. }
        )
    );
    let crosshair = matches!(state.tool, Tool::Node | Tool::Member);
    grips::cursor(ui.ctx(), over_grip, dragging, crosshair, over_tool_target);

    if let Some(Gesture::Window { from, to }) = &state.gesture {
        grips::paint_window(&painter, *from, *to);
    }

    if let Some(p) = hover
        && !over_grip
        && state.gesture.is_none()
    {
        if let Some(et) = &hovered_erase
            && let Some(text) = grips::erase_tag(&shown, et)
        {
            grips::paint_tag_styled(
                &painter,
                rect,
                p,
                &text,
                Color32::from_rgba_unmultiplied(0xFE, 0xE2, 0xE2, 244),
                crimson,
                Color32::from_rgb(0x99, 0x1B, 0x1B),
            );
        } else if let Some(h) = &hovered_select
            && let Some(text) = grips::tag(&shown, h)
        {
            grips::paint_tag(&painter, rect, p, &text);
        } else if let Some(hrt) = &hovered_rot
            && let Some(text) = grips::tag(&shown, &rotate::sel_of(hrt))
        {
            grips::paint_tag(&painter, rect, p, &text);
        }
    }

    let snapped = hover.map(|p| {
        let (raw_pt, node) = snap(st, v, p, state);
        let pt = if state.tool == Tool::Member { snap_member_end(state.start, raw_pt, node.is_some(), shift, state) } else { raw_pt };
        (pt, node)
    });

    if let Some(snapped) = &snapped {
        paint_drawing(&painter, st, v, state, snapped, accent);
    }

    let sheet_border = match state.tool {
        Tool::Select => Stroke::new(1.0, Color32::from_gray(210)),
        Tool::Erase => Stroke::new(1.5, crimson.linear_multiply(0.75)),
        Tool::Rotate => Stroke::new(1.5, Color32::from_rgb(0xE1, 0x1D, 0x48).linear_multiply(0.75)),
        _ => Stroke::new(1.5, accent.linear_multiply(0.75)),
    };
    painter.rect_stroke(rect, 2.0, sheet_border, egui::StrokeKind::Inside);

    let unit = ts(st.length_unit.symbol());
    let readout = if let Some((_, ref rt, vis, _, ref sref)) = rot_state {
        Some(rotate::readout_for(&shown, rt, vis, sref.as_ref()))
    } else {
        match &state.gesture {
            Some(Gesture::Move { from, to, anchor }) => {
                let (dx, dy) = step(*from, *to, *anchor, state);
                Some(format!("Δx = {} {unit}   Δy = {} {unit}", short(dx), short(dy)))
            }
            Some(Gesture::Slide { at, .. }) => Some(format!("{} = {} {unit}", ts("at"), short(*at))),
            Some(Gesture::Grip { grip, to }) => Some(grips::readout(st, v, state, grip, *to, shift)),
            _ => snapped.as_ref().map(|((x, y), node)| {
                let snap_tag = match node {
                    Some(id) => format!("   ·   ● {id}"),
                    None if state.snap => format!("   ·   ⊞ {}", short(state.grid)),
                    None => String::new(),
                };
                format!("x = {} {unit}   y = {} {unit}{snap_tag}", short(*x), short(*y))
            }),
        }
    };
    if let Some(text) = readout.filter(|t| !t.is_empty()) {
        paint_hud_pill(&painter, rect.left_bottom() + vec2(6.0, -6.0), Align2::LEFT_BOTTOM, &text);
    }
    if !state.sel.is_empty() {
        let n = state.sel.len().to_string();
        let right_text = crate::i18n::fmt(ts("{n} selected · Del removes · arrows move"), &[("n", &n)]);
        paint_hud_pill(&painter, rect.right_bottom() + vec2(-6.0, -6.0), Align2::RIGHT_BOTTOM, &right_text);
    }
}

/// `st` as shown during a move, a slide or a grip's drag (unchanged otherwise).
fn gesture_preview(st: &Structure, v: View, state: &State, shift: bool) -> Structure {
    let mut s = st.clone();
    match &state.gesture {
        Some(Gesture::Move { from, to, anchor }) => {
            let (dx, dy) = step(*from, *to, *anchor, state);
            let ids = moved_nodes(st, &state.sel);
            for n in s.nodes.iter_mut().filter(|n| ids.contains(&n.id)) {
                n.x = Num::new(n.x.value + dx);
                n.y = Num::new(n.y.value + dy);
            }
        }
        Some(Gesture::Slide { load, at, .. }) => {
            if let Some(Load::Force { at: a, .. } | Load::Moment { at: a, .. }) = s.loads.get_mut(*load) {
                *a = Num::new(*at);
            }
        }
        Some(Gesture::Grip { grip, to }) => s = grips::preview(st, v, state, grip, *to, shift),
        _ => {}
    }
    s
}

/// The protractor: the rotation or aim under way, else (Rotate tool) the selected vector's and a
/// faint one on the vector under the pointer.
fn paint_rotation(
    painter: &egui::Painter,
    shown: &Structure,
    v: View,
    state: &State,
    rot_state: Option<&RotState>,
    hovered_rot: Option<&rotate::RotTarget>,
) {
    if let Some((_, rt, vis, start_vis, sref)) = rot_state {
        rotate::paint_protractor(painter, shown, v, rt, rotate::Dial { vis_deg: *vis, start_vis: *start_vis, snap_ref: sref.as_ref(), faint: false });
        return;
    }
    if state.tool != Tool::Rotate {
        return;
    }
    let active_rt = rotate::active_target(shown, &state.sel);
    if let Some(ref rt) = active_rt
        && let Some(vis) = rotate::visual_angle(shown, rt)
    {
        let (_, sref) = rotate::snap_visual_angle(shown, rt, vis, true);
        rotate::paint_protractor(painter, shown, v, rt, rotate::Dial { vis_deg: vis, start_vis: None, snap_ref: sref.as_ref(), faint: false });
    }
    if let Some(hrt) = hovered_rot
        && active_rt.as_ref() != Some(hrt)
        && let Some(vis) = rotate::visual_angle(shown, hrt)
    {
        let crimson = Color32::from_rgb(0xDC, 0x26, 0x26);
        highlight(painter, shown, v, &rotate::sel_of(hrt), crimson.linear_multiply(0.45));
        rotate::paint_protractor(painter, shown, v, hrt, rotate::Dial { vis_deg: vis, start_vis: None, snap_ref: None, faint: true });
    }
}

/// The member being drawn (from `state.start` to the snapped point, with its length and slope) and
/// the Node and Member tools' crosshair.
fn paint_drawing(painter: &egui::Painter, st: &Structure, v: View, state: &State, snapped: &((f64, f64), Option<String>), accent: Color32) {
    let ((x, y), node) = snapped;
    if let Some((x0, y0)) = state.start {
        let (p0, p1) = (v.to_screen(x0, y0), v.to_screen(*x, *y));
        painter.line_segment([p0, p1], Stroke::new(1.8, accent));
        painter.circle_filled(p0, 3.5, accent);
        painter.circle_filled(p1, 3.5, accent);
        let len = (*x - x0).hypot(*y - y0);
        if len > 1e-6 {
            let deg = (*y - y0).atan2(*x - x0).to_degrees();
            let lu = ts(st.length_unit.symbol());
            let badge = format!("L = {} {lu} · {}°", short(len), short(deg));
            grips::paint_tag(painter, v.rect, pos2(0.5 * (p0.x + p1.x), 0.5 * (p0.y + p1.y) - 18.0), &badge);
        }
    }
    if matches!(state.tool, Tool::Node | Tool::Member) {
        let p = v.to_screen(*x, *y);
        let c = if node.is_some() { accent } else { Color32::from_rgb(0x25, 0x63, 0xEB) };
        painter.line_segment([p - vec2(7.0, 0.0), p + vec2(7.0, 0.0)], Stroke::new(1.0, c));
        painter.line_segment([p - vec2(0.0, 7.0), p + vec2(0.0, 7.0)], Stroke::new(1.0, c));
        if state.tool == Tool::Node && node.is_none() {
            painter.circle_stroke(p, 4.0, Stroke::new(1.4, c));
        }
    }
}

/// Paints a translucent rounded HUD status pill anchored at `anchor_pos`.
fn paint_hud_pill(painter: &egui::Painter, anchor_pos: Pos2, align: Align2, text: &str) {
    let galley = painter.layout_no_wrap(text.to_string(), FontId::proportional(11.0), Color32::from_gray(0x28));
    let pad = vec2(7.0, 3.0);
    let size = galley.size() + pad * 2.0;
    let r = align.anchor_size(anchor_pos, size);
    painter.rect_filled(r, 3.0, Color32::from_rgba_unmultiplied(255, 255, 255, 224));
    painter.rect_stroke(r, 3.0, Stroke::new(0.8, Color32::from_gray(205)), egui::StrokeKind::Inside);
    painter.galley(r.min + pad, galley, Color32::from_gray(0x28));
}

/// Paints live target highlights & ghost previews for `Support`, `Hinge`, `Force`, `Couple`, and
/// `Distributed` tools while hovering at `p`.
fn paint_tool_ghost(painter: &egui::Painter, st: &Structure, v: View, p: Pos2, state: &State, accent: Color32, over_target: &mut bool) {
    let load_col = Color32::from_rgb(0xB0, 0x3A, 0x2E).linear_multiply(0.65);
    match state.tool {
        Tool::Support(kind) => {
            if let Some(id) = pick_node(st, v, p)
                && let Some(n) = st.node(&id)
            {
                *over_target = true;
                let np = v.to_screen(n.x.value, n.y.value);
                painter.circle_stroke(np, 7.5, Stroke::new(2.0, accent));
                let tag = format!("{}: {}", ts(kind_label(Some(kind))), crate::i18n::fmt(ts("Node {n}"), &[("n", &id)]));
                grips::paint_tag(painter, v.rect, p, &tag);
            }
        }
        Tool::Hinge => {
            if let Some(id) = pick_node(st, v, p)
                && let Some(n) = st.node(&id)
            {
                *over_target = true;
                let np = v.to_screen(n.x.value, n.y.value);
                painter.circle(np, 4.5, Color32::WHITE, Stroke::new(1.8, accent));
            } else if let Some((id, end_b)) = pick_member_end(st, v, p)
                && let Some(m) = st.member(&id)
                && let (Some(a), Some(b)) = (st.node(&m.a), st.node(&m.b))
            {
                *over_target = true;
                let (pa, pb) = (v.to_screen(a.x.value, a.y.value), v.to_screen(b.x.value, b.y.value));
                let d = pb - pa;
                let u = if d.length() > 0.0 { d / d.length() } else { vec2(1.0, 0.0) };
                let hp = if end_b { pb - u * 6.5 } else { pa + u * 6.5 };
                painter.circle(hp, 4.0, Color32::WHITE, Stroke::new(1.8, accent));
                let end_name = if end_b { &m.b } else { &m.a };
                grips::paint_tag(painter, v.rect, p, &crate::i18n::fmt(ts("Hinge at end {n}"), &[("n", end_name)]));
            }
        }
        Tool::Force | Tool::Couple => {
            let landing = if let Some(id) = pick_node(st, v, p) {
                st.node(&id).map(|n| (v.to_screen(n.x.value, n.y.value), crate::i18n::fmt(ts("At node {n}"), &[("n", &id)])))
            } else if let Some((id, at)) = pick_member(st, v, p) {
                let len = st.member(&id).and_then(|m| st.length(m)).unwrap_or(0.0);
                let sat = round(round_at(at, len, state));
                edits::point_on(st, &id, sat).map(|(wx, wy)| {
                    let lu = ts(st.length_unit.symbol());
                    (v.to_screen(wx, wy), format!("{} · {} = {} {lu}", crate::i18n::fmt(ts("Member {n}"), &[("n", &id)]), ts("at"), short(sat)))
                })
            } else {
                None
            };
            if let Some((lp, label)) = landing {
                *over_target = true;
                if state.tool == Tool::Force {
                    let tail = lp - vec2(0.0, 11.0 * MM);
                    painter.line_segment([tail, lp - vec2(0.0, 5.0)], Stroke::new(1.8, load_col));
                    painter.add(Shape::convex_polygon(vec![lp, lp + vec2(-3.5, -7.0), lp + vec2(3.5, -7.0)], load_col, Stroke::NONE));
                } else {
                    painter.circle_stroke(lp, 4.0 * MM, Stroke::new(1.8, load_col));
                }
                grips::paint_tag(painter, v.rect, p, &label);
            }
        }
        Tool::Distributed => {
            if let Some((id, _)) = pick_member(st, v, p)
                && let Some(m) = st.member(&id)
                && let (Some(a), Some(b)) = (st.node(&m.a), st.node(&m.b))
            {
                *over_target = true;
                let (pa, pb) = (v.to_screen(a.x.value, a.y.value), v.to_screen(b.x.value, b.y.value));
                let off = vec2(0.0, -7.0 * MM);
                painter.add(Shape::closed_line(vec![pa, pb, pb + off, pa + off], Stroke::new(1.5, load_col)));
                grips::paint_tag(painter, v.rect, p, &crate::i18n::fmt(ts("Member {n}"), &[("n", &id)]));
            }
        }
        _ => {}
    }
}

/// Esc, Delete, the arrows, Tab, tool shortcuts and Select All on the sheet.
fn keys(app: &mut WordApp, ui: &Ui, resp: &egui::Response, src: &Structure, st: &Structure, v: View, state: &mut State) -> bool {
    use egui::{Key, Modifiers};
    let busy = state.gesture.is_some() || state.start.is_some() || state.tool != Tool::Select || !state.sel.is_empty();
    let selected = !state.sel.is_empty();
    let hovered = resp.hover_pos();
    let (esc, esc_free, del, all, tab, tool_key, arrows, shift) = ui.input_mut(|i| {
        let esc = busy && i.consume_key(Modifiers::NONE, Key::Escape);
        let esc_free = !busy && i.key_pressed(Key::Escape);
        let del = selected && (i.consume_key(Modifiers::NONE, Key::Delete) || i.consume_key(Modifiers::NONE, Key::Backspace));
        let all = i.consume_key(Modifiers::COMMAND, Key::A);
        let tab = hovered.is_some() && i.consume_key(Modifiers::NONE, Key::Tab);
        let mut tkey = None;
        for (k, t) in [
            (Key::V, Tool::Select),
            (Key::R, Tool::Rotate),
            (Key::N, Tool::Node),
            (Key::M, Tool::Member),
            (Key::F, Tool::Force),
            (Key::C, Tool::Couple),
            (Key::D, Tool::Distributed),
            (Key::H, Tool::Hinge),
            (Key::E, Tool::Erase),
        ] {
            if i.consume_key(Modifiers::NONE, k) {
                tkey = Some(t);
            }
        }
        let shift = i.modifiers.shift;
        let arrow_mods = if shift { Modifiers::SHIFT } else { Modifiers::NONE };
        let can_arrow = selected || state.tool == Tool::Rotate;
        let mut count = |k: Key| if can_arrow { f64::from(u32::try_from(i.count_and_consume_key(arrow_mods, k)).unwrap_or(0)) } else { 0.0 };
        let dir = (count(Key::ArrowRight) - count(Key::ArrowLeft), count(Key::ArrowUp) - count(Key::ArrowDown));
        (esc, esc_free, del, all, tab, tkey, dir, shift)
    });
    if esc {
        if state.gesture.take().is_some() || state.start.take().is_some() {
        } else if state.tool != Tool::Select {
            state.tool = Tool::Select;
        } else {
            state.sel.clear();
        }
    }
    if let Some(t) = tool_key {
        state.tool = t;
        state.start = None;
        state.gesture = None;
    }
    if tab && let Some(p) = hovered {
        let candidates = hits(st, v, p);
        if !candidates.is_empty() {
            state.cycle = state.cycle.wrapping_add(1);
            if let Some(chosen) = candidates.get(state.cycle % candidates.len()).cloned() {
                state.select(chosen);
            }
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
        if state.tool == Tool::Rotate
            && let Some(rt) = rotate::active_target(st, &state.sel)
            && let Some(vis) = rotate::visual_angle(st, &rt)
            && let Some(cur_stored) = rotate::stored_angle_for_visual(st, &rt, vis)
        {
            let step_deg = if shift { 1.0 } else { 15.0 };
            let delta = (-arrows.0 + arrows.1) * step_deg;
            rotate::commit_target(app, src, st, &rt, cur_stored + delta, true);
        } else {
            let g = if state.grid > 0.0 { state.grid } else { 0.1 };
            let k = if shift { 10.0 } else { 1.0 };
            let ids = moved_nodes(st, &state.sel);
            if !ids.is_empty() {
                let moved = moved(src, &ids, arrows.0 * g * k, arrows.1 * g * k);
                let _ = app.run("structure.set", json!({"structure": moved}));
            } else if let Some(Sel::Load(idx)) = state.primary().cloned()
                && let Some((m_id, cur_at)) = load_on_member(st, idx)
            {
                let len = st.member(&m_id).and_then(|m| st.length(m)).unwrap_or(0.0);
                let step_s = (arrows.0 + arrows.1) * g * k;
                let new_at = (cur_at + step_s).clamp(0.0, len);
                if let Some(lj) = slid(src, idx, cur_at, new_at) {
                    let _ = app.run("structure.load", json!({"index": idx, "load": lj}));
                }
            }
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
            Load::Distributed { member, then, .. } => {
                if then.iter().any(|m| members.iter().any(|x| x == m)) {
                    return false;
                }
                (None, Some(member.as_str()))
            }
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
/// couple), or the actual trapezoid/triangle corners of its band (a distributed load).
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
        Load::Distributed { member, then, q1, q2, from, to, angle, axes, .. } => {
            // Along its member, or the aligned members it goes on over.
            let line = st.spread_line(member, then)?;
            let (len, (c, s)) = (line.len, line.dir);
            let (x0, x1) = (from.value.clamp(0.0, len), to.as_ref().map_or(len, |t| t.value).clamp(0.0, len));
            let th = angle.value.to_radians();
            let g = match axes {
                strix_structure::LoadAxes::Global => (th.cos(), th.sin()),
                strix_structure::LoadAxes::Local => (c * th.cos() - s * th.sin(), s * th.cos() + c * th.sin()),
            };
            let qmax = st
                .loads
                .iter()
                .filter_map(|l| match l {
                    Load::Distributed { q1, q2, .. } => Some(q1.value.abs().max(q2.as_ref().map_or(0.0, |q| q.value.abs()))),
                    _ => None,
                })
                .fold(0.0_f64, f64::max);
            let q_a = q1.value;
            let q_b = q2.as_ref().map_or(q_a, |q| q.value);
            let dom_sign = if q_a != 0.0 {
                q_a.signum()
            } else if q_b != 0.0 {
                q_b.signum()
            } else {
                1.0
            };
            let tail_dir = vec2(-g.0 as f32, g.1 as f32);
            let h_for = |q: f64| -> f32 {
                if qmax <= 0.0 {
                    return 6.0 * MM * dom_sign as f32;
                }
                let raw = (8.0 * MM) * ((q / qmax) as f32);
                if q == 0.0 {
                    0.0
                } else if raw.abs() < 2.2 * MM {
                    2.2 * MM * raw.signum()
                } else {
                    raw
                }
            };
            let ((ax, ay), (bx, by)) = (line.point(x0), line.point(x1));
            let (p0, p1) = (v.to_screen(ax, ay), v.to_screen(bx, by));
            Some(vec![p0, p1, p1 + tail_dir * h_for(q_b), p0 + tail_dir * h_for(q_a)])
        }
    }
}

fn seg_dist(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    let t = if len2 > 0.0 { ((p - a).dot(ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
    (a + ab * t - p).length()
}

/// Checks whether `p` lies inside (with pick slack) the distributed load polygon `pts`.
fn in_load_band(p: Pos2, pts: &[Pos2; 4]) -> bool {
    let [p0, p1, p2, p3] = *pts;
    if seg_dist(p, p0, p1) <= PICK * 0.5 {
        return false;
    }
    let in_tri = |a: Pos2, b: Pos2, c: Pos2| -> bool {
        let area = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
        if area.abs() < 1.0 {
            return seg_dist(p, a, b).min(seg_dist(p, b, c)).min(seg_dist(p, c, a)) <= PICK * 0.8;
        }
        let s = area.signum();
        [(a, b), (b, c), (c, a)].into_iter().all(|(u, w)| {
            let cross = (w.x - u.x) * (p.y - u.y) - (w.y - u.y) * (p.x - u.x);
            cross * s >= -PICK * 0.6 * (w - u).length()
        })
    };
    in_tri(p0, p1, p2) || in_tri(p0, p2, p3)
}

/// The load the pointer is on, the last drawn first.
fn pick_load(st: &Structure, v: View, p: Pos2) -> Option<usize> {
    (0..st.loads.len()).rev().find(|&k| match load_shape(st, v, k).as_deref() {
        Some([a, b]) => seg_dist(p, *a, *b) <= PICK && (p - *a).length() > PICK * 0.6,
        Some([c]) => ((p - *c).length() - 4.0 * MM).abs() <= PICK * 0.8,
        Some([p0, p1, p2, p3]) => in_load_band(p, &[*p0, *p1, *p2, *p3]),
        _ => false,
    })
}

/// All selectable items within reach of `p`, in priority order (for cycling overlapping items).
fn hits(st: &Structure, v: View, p: Pos2) -> Vec<Sel> {
    let mut out: Vec<Sel> = Vec::new();
    let mut push = |s: Sel| {
        if !out.contains(&s) {
            out.push(s);
        }
    };
    for n in &st.nodes {
        if dist(v.to_screen(n.x.value, n.y.value), p) <= f64::from(PICK) {
            push(Sel::Node(n.id.clone()));
        }
    }
    for k in (0..st.loads.len()).rev() {
        let matched = match load_shape(st, v, k).as_deref() {
            Some([a, b]) => seg_dist(p, *a, *b) <= PICK,
            Some([c]) => ((p - *c).length() - 4.0 * MM).abs() <= PICK * 0.8,
            Some([p0, p1, p2, p3]) => in_load_band(p, &[*p0, *p1, *p2, *p3]),
            _ => false,
        };
        if matched {
            push(Sel::Load(k));
        }
    }
    if let Some(id) = pick_support_node(st, v, p) {
        push(Sel::Node(id));
    }
    if let Some((id, _)) = pick_member(st, v, p) {
        push(Sel::Member(id));
    }
    out
}

/// What the pointer is on: a node, else a load, else a support symbol's node, else a member.
fn pick(st: &Structure, v: View, p: Pos2) -> Option<Sel> {
    pick_node(st, v, p)
        .map(Sel::Node)
        .or_else(|| pick_load(st, v, p).map(Sel::Load))
        .or_else(|| pick_support_node(st, v, p).map(Sel::Node))
        .or_else(|| pick_member(st, v, p).map(|(id, _)| Sel::Member(id)))
}

/// What `Tool::Erase` hits at `p`: a load, a node, a support symbol, or a member.
fn pick_erase(st: &Structure, v: View, p: Pos2) -> Option<EraseTarget> {
    pick_load(st, v, p)
        .map(EraseTarget::Load)
        .or_else(|| pick_node(st, v, p).map(EraseTarget::Node))
        .or_else(|| pick_support_node(st, v, p).map(EraseTarget::Support))
        .or_else(|| pick_member(st, v, p).map(|(id, _)| EraseTarget::Member(id)))
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

/// Marks `s` on the sheet in `c` (including a node's support box when present).
fn highlight(p: &egui::Painter, st: &Structure, v: View, s: &Sel, c: Color32) {
    match s {
        Sel::Node(id) => {
            if let Some(n) = st.node(id) {
                let np = v.to_screen(n.x.value, n.y.value);
                p.circle_filled(np, 7.5, c.linear_multiply(0.18));
                p.circle_stroke(np, 6.5, Stroke::new(2.2, c));
                if let Some(sr) = support_rect(st, v, id) {
                    p.rect_stroke(sr.expand(1.5), 2.5, Stroke::new(1.2, c.linear_multiply(0.65)), egui::StrokeKind::Outside);
                }
            }
        }
        Sel::Member(id) => {
            if let Some(m) = st.member(id)
                && let (Some(a), Some(b)) = (st.node(&m.a), st.node(&m.b))
            {
                p.line_segment([v.to_screen(a.x.value, a.y.value), v.to_screen(b.x.value, b.y.value)], Stroke::new(5.5, c.linear_multiply(0.55)));
            }
        }
        Sel::Load(k) => match load_shape(st, v, *k).as_deref() {
            Some([a, b]) => {
                p.line_segment([*a, *b], Stroke::new(5.5, c.linear_multiply(0.55)));
            }
            Some([o]) => {
                p.circle_stroke(*o, 4.0 * MM + 2.0, Stroke::new(3.0, c.linear_multiply(0.6)));
            }
            Some(pts) if pts.len() == 4 => {
                p.add(Shape::closed_line(pts.to_vec(), Stroke::new(2.2, c)));
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

/// A click on the sheet with the current tool (used by unit tests and plain clicks).
#[cfg(test)]
fn click(app: &mut WordApp, st: &Structure, v: View, p: Pos2, state: &mut State) {
    click_with_mods(app, st, v, p, state, false);
}

/// A click on the sheet with the current tool (`shift`: 0°/45°/90° member constraint or 1° protractor precision).
fn click_with_mods(app: &mut WordApp, st: &Structure, v: View, p: Pos2, state: &mut State, shift: bool) {
    let (raw_pt, on_node) = snap(st, v, p, state);
    let point = if state.tool == Tool::Member { snap_member_end(state.start, raw_pt, on_node.is_some(), shift, state) } else { raw_pt };
    match state.tool {
        Tool::Select => match pick(st, v, p) {
            Some(hit) => state.select(hit),
            None => state.sel.clear(),
        },
        Tool::Rotate => {
            let cur = rotate::active_target(st, &state.sel);
            if let Some(ref rt) = cur
                && let Some((wx, wy)) = rotate::pivot(st, rt)
            {
                let center = v.to_screen(wx, wy);
                let d = (p - center).length();
                let picked_other = rotate::pick_rotatable(st, v, p).filter(|o| o != rt);
                if picked_other.is_none()
                    && (14.0..=rotate::PROTRACTOR_R + 18.0).contains(&d)
                    && let Some(raw_vis) = rotate::screen_angle(center, p)
                    && let Some(stored) = rotate::stored_angle_for_visual(st, rt, rotate::snap_visual_angle(st, rt, raw_vis, state.snap != shift).0)
                {
                    let src = app
                        .session
                        .run("structure.get", &json!({}))
                        .ok()
                        .and_then(|i| serde_json::from_value::<Structure>(i["structure"].clone()).ok())
                        .unwrap_or_else(|| st.clone());
                    rotate::commit_target(app, &src, st, rt, stored, false);
                    return;
                }
            }
            match rotate::pick_rotatable(st, v, p) {
                Some(rt) => state.select(rotate::sel_of(&rt)),
                None => state.sel.clear(),
            }
        }
        Tool::Node => {
            if let Some(id) = on_node {
                state.select(Sel::Node(id));
            } else if let Some(id) = node_at(app, st, point) {
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
                if app.run("structure.node", json!({"id": id, "hinge": !on})).is_ok() {
                    state.select(Sel::Node(id));
                }
            } else if let Some((id, end_b)) = pick_member_end(st, v, p)
                && let Some(m) = st.member(&id)
            {
                let (key, cur) = if end_b { ("hingeB", m.hinge_b) } else { ("hingeA", m.hinge_a) };
                if app.run("structure.member", json!({"id": id, key: !cur})).is_ok() {
                    state.select(Sel::Member(id));
                }
            }
        }
        Tool::Force | Tool::Couple => {
            let kind = if state.tool == Tool::Force { "force" } else { "moment" };
            let value = 10.0;
            let idx = st.loads.len();
            let added = if let Some(id) = pick_node(st, v, p) {
                app.run("structure.load", json!({"load": {"type": kind, "node": id, "value": value}})).is_ok()
            } else if let Some((id, at)) = pick_member(st, v, p) {
                let len = st.member(&id).and_then(|m| st.length(m)).unwrap_or(0.0);
                app.run("structure.load", json!({"load": {"type": kind, "member": id, "at": round(round_at(at, len, state)), "value": value}}))
                    .is_ok()
            } else {
                false
            };
            if added {
                state.select_new_load(idx);
            }
        }
        Tool::Distributed => {
            if let Some((id, _)) = pick_member(st, v, p) {
                let idx = st.loads.len();
                if app.run("structure.load", json!({"load": {"type": "distributed", "member": id, "q1": 10.0}})).is_ok() {
                    state.select_new_load(idx);
                }
            }
        }
        Tool::Erase => {
            if let Some(et) = pick_erase(st, v, p) {
                match et {
                    EraseTarget::Load(k) => {
                        let prev = st.loads.len();
                        if app.run("structure.load", json!({"index": k, "remove": true})).is_ok() {
                            state.on_load_removed(k, prev);
                        }
                    }
                    EraseTarget::Support(id) => {
                        let _ = app.run("structure.support", json!({"node": id, "remove": true}));
                    }
                    EraseTarget::Node(id) => {
                        let _ = app.run("structure.node", json!({"id": id, "remove": true}));
                    }
                    EraseTarget::Member(id) => {
                        let _ = app.run("structure.member", json!({"id": id, "remove": true}));
                    }
                }
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

/// Typed coordinates or angles below the sheet:
/// - `x, y`, `x; y`, `@dx, dy` or polar `@L<α` place a node or member point;
/// - in `Rotate` mode with a selected target, typing an angle (`−30`, `45`, `@15` or an expression)
///   sets or increments its inclination on Enter.
fn typed_point(app: &mut WordApp, ui: &mut Ui, src: &Structure, st: &Structure, state: &mut State) {
    let rot_target = (state.tool == Tool::Rotate).then(|| rotate::active_target(st, &state.sel)).flatten();
    ui.horizontal(|ui| {
        let (lbl, hint, help) = if rot_target.is_some() {
            (ts("Angle"), "−30  ·  @15", ts("Enter sets the angle in degrees (@delta turns from the current angle)"))
        } else {
            (ts("Point"), "x, y  ·  @dx, dy  ·  @L<α", ts("Node or Member tool: Enter places the point"))
        };
        ui.label(lbl);
        let r = ui.add(egui::TextEdit::singleline(&mut state.typed).desired_width(160.0).hint_text(hint)).on_hover_text(help);
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.add(egui::Label::new(RichText::new(help).small().weak()).truncate());
        if !enter {
            return;
        }
        let text = state.typed.trim().trim_end_matches('°').trim().to_string();
        if text.is_empty() {
            return;
        }
        if let Some(rt) = &rot_target
            && !text.contains(',')
            && !text.contains(';')
            && !text.contains('<')
        {
            if let Some(rel) = text.strip_prefix('@') {
                if let Ok(d) = rel.trim().replace(',', ".").parse::<f64>()
                    && d.is_finite()
                    && let Some(vis) = rotate::visual_angle(st, rt)
                    && let Some(cur) = rotate::stored_angle_for_visual(st, rt, vis)
                {
                    rotate::commit_target(app, src, st, rt, cur + d, true);
                    state.typed.clear();
                    r.request_focus();
                    return;
                }
            } else if let Ok(deg) = text.replace(',', ".").parse::<f64>()
                && deg.is_finite()
            {
                rotate::commit_target(app, src, st, rt, deg, false);
                state.typed.clear();
                r.request_focus();
                return;
            } else {
                let val = num_json(&text);
                match rt {
                    rotate::RotTarget::Force(k) | rotate::RotTarget::Distributed(k) => {
                        if let Some(l) = src.loads.get(*k)
                            && let Ok(mut lj) = serde_json::to_value(l)
                        {
                            lj["angle"] = val;
                            let _ = app.run("structure.load", json!({"index": *k, "load": lj}));
                        }
                    }
                    rotate::RotTarget::Support(id) => {
                        let _ = app.run("structure.support", json!({"node": id, "angle": val}));
                    }
                }
                state.typed.clear();
                r.request_focus();
                return;
            }
        }
        let (relative, body) = match text.strip_prefix('@') {
            Some(rest) => (true, rest.to_string()),
            None => (false, text.clone()),
        };
        let parsed_delta = if let Some((l_str, a_str)) = body.split_once('<') {
            let l = l_str.trim().replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite());
            let a = a_str.trim().trim_end_matches('°').trim().replace(',', ".").parse::<f64>().ok().filter(|v| v.is_finite());
            l.zip(a).map(|(len, deg)| {
                let rad = deg.to_radians();
                (round(len * rad.cos()), round(len * rad.sin()))
            })
        } else {
            let parts: Vec<String> = if body.contains(';') {
                body.split(';').map(|s| s.trim().replace(',', ".")).collect()
            } else {
                body.split(',').map(|s| s.trim().to_string()).collect()
            };
            let nums: Vec<f64> = parts.iter().filter_map(|s| s.parse::<f64>().ok()).filter(|v| v.is_finite()).collect();
            match nums.as_slice() {
                [x, y] => Some((*x, *y)),
                _ => None,
            }
        };
        let Some((dx, dy)) = parsed_delta else { return };
        let base = state.start.or_else(|| match state.primary() {
            Some(Sel::Node(id)) => st.node(id).map(|n| (n.x.value, n.y.value)),
            _ => None,
        });
        let point = if relative || body.contains('<') { base.map_or((dx, dy), |(bx, by)| (bx + dx, by + dy)) } else { (dx, dy) };
        match state.tool {
            Tool::Member => member_point(app, st, point, state),
            _ => {
                state.tool = Tool::Node;
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
    draft_box(ui, key, current, width, None)
}

/// [`edit_box`] named for screen readers by the label `name` (the Structures tab's boxes).
pub(crate) fn named_box(ui: &mut Ui, key: impl std::hash::Hash + std::fmt::Debug, current: &str, width: f32, name: egui::Id) -> Option<String> {
    draft_box(ui, key, current, width, Some(name))
}

fn draft_box(ui: &mut Ui, key: impl std::hash::Hash + std::fmt::Debug, current: &str, width: f32, name: Option<egui::Id>) -> Option<String> {
    let id = egui::Id::unique(("structure_box", key));
    let mut text = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| current.to_string());
    let mut edit = egui::TextEdit::singleline(&mut text).desired_width(width);
    if name.is_some() {
        // The ribbon's rows: small text, as tall as its buttons.
        edit = edit.font(egui::TextStyle::Small).margin(vec2(4.0, 1.0));
    }
    let mut r = ui.add(edit);
    if let Some(name) = name {
        r = r.labelled_by(name);
    }
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

const KINDS: [(Option<SupportKind>, &str); 7] = [
    (None, "None"),
    (Some(SupportKind::Fixed), "Fixed"),
    (Some(SupportKind::Pin), "Pin"),
    (Some(SupportKind::RollerCircle), "Roller"),
    (Some(SupportKind::Roller), "Roller (triangle)"),
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

/// Shows the unit, and `= resolved` when `src_num` is written as a calculation expression.
fn unit_or_resolved(ui: &mut Ui, src_num: Option<&Num>, resolved_val: Option<f64>, unit: &str) {
    let u = ts(unit);
    match (src_num.and_then(|n| n.expr.as_ref()), resolved_val) {
        (Some(_), Some(v)) => {
            ui.label(RichText::new(format!("= {} {u}", short(v))).small().color(Color32::from_rgb(0x0F, 0x76, 0x6E)));
        }
        _ => {
            ui.label(RichText::new(u).weak());
        }
    }
}

/// Formats a load's short display title using `strix_structure::load_names` (`F₁ · Force`).
fn load_title(st: &Structure, k: usize, l: &Load) -> String {
    let names = strix_structure::load_names(st);
    let kind = match l {
        Load::Force { .. } => ts("Force"),
        Load::Moment { .. } => ts("Couple"),
        Load::Distributed { .. } => ts("Distributed"),
    };
    match names.get(k).copied().flatten() {
        Some(n) => format!("{} · {kind}", n.pretty()),
        None => format!("#{} {kind}", k + 1),
    }
}

/// Renders solved support reactions for `only_node` (or all supported nodes when `None`).
fn reactions_summary_ui(ui: &mut Ui, info: &Value, st: &Structure, only_node: Option<&str>) {
    let Some(arr) = info["reactions"].as_array() else { return };
    let rows: Vec<&Value> = arr.iter().filter(|r| only_node.is_none_or(|id| r["node"].as_str() == Some(id))).collect();
    if rows.is_empty() {
        return;
    }
    let fu = ts(st.force_unit.symbol());
    let lu = ts(st.length_unit.symbol());
    section(ui, ts("Reactions"));
    props(ui, ("reactions_summary", only_node.unwrap_or("all")), |ui| {
        for r in rows {
            let node = r["node"].as_str().unwrap_or_default();
            let fx = r["fx"].as_f64().unwrap_or(0.0);
            let fy = r["fy"].as_f64().unwrap_or(0.0);
            let m = r["m"].as_f64().unwrap_or(0.0);
            if only_node.is_none() {
                ui.label(RichText::new(node).strong());
            } else {
                prop_label(ui, &format!("H_{node}, V_{node}"));
            }
            let mut parts = vec![format!("H = {} {fu}", short(fx)), format!("V = {} {fu}", short(fy))];
            if m.abs() > 1e-9 {
                parts.push(format!("M = {} {fu}·{lu}", short(m)));
            }
            ui.label(RichText::new(parts.join("  ·  ")).small());
            let mag = fx.hypot(fy);
            ui.label(RichText::new(format!("|R| = {} {fu}", short(mag))).small().weak());
            ui.end_row();
        }
    });
}

/// Renders solved internal forces summary (`N`, `T`, `M` min/max, or truss axial force) for `member_id`.
fn member_forces_ui(ui: &mut Ui, info: &Value, st: &Structure, member_id: &str) {
    let Some(m_info) = info["members"].as_array().and_then(|arr| arr.iter().find(|m| m["id"].as_str() == Some(member_id))) else {
        return;
    };
    let vals = |key: &str| -> Vec<f64> {
        m_info[key].as_array().into_iter().flatten().filter_map(|n| n["value"].as_f64()).filter(|v| v.is_finite()).collect()
    };
    let (ns, vs, ms) = (vals("n"), vals("v"), vals("m"));
    if ns.is_empty() && vs.is_empty() && ms.is_empty() {
        return;
    }
    let fu = ts(st.force_unit.symbol());
    let lu = ts(st.length_unit.symbol());
    let truss = info["truss"].as_bool().unwrap_or(false);
    section(ui, if truss { ts("Axial forces") } else { ts("Internal forces") });
    let range_str = |xs: &[f64], u: &str| -> String {
        let lo = xs.iter().copied().fold(f64::MAX, f64::min);
        let hi = xs.iter().copied().fold(f64::MIN, f64::max);
        if (hi - lo).abs() <= 1e-6 { format!("{} {u}", short(lo)) } else { format!("{} … {} {u}", short(lo), short(hi)) }
    };
    props(ui, ("member_forces", member_id), |ui| {
        if !ns.is_empty() {
            prop_label(ui, &st.titles.n);
            ui.label(range_str(&ns, fu));
            if truss && let Some(&n0) = ns.first() {
                let state_lbl = if n0 > 1e-6 {
                    ts("Tension")
                } else if n0 < -1e-6 {
                    ts("Compression")
                } else {
                    "0"
                };
                ui.label(RichText::new(state_lbl).small().weak());
            } else {
                ui.label("");
            }
            ui.end_row();
        }
        if !truss && !vs.is_empty() {
            prop_label(ui, &st.titles.v);
            ui.label(range_str(&vs, fu));
            ui.label("");
            ui.end_row();
        }
        if !truss && !ms.is_empty() {
            prop_label(ui, &st.titles.m);
            ui.label(range_str(&ms, &format!("{fu}·{lu}")));
            ui.label("");
            ui.end_row();
        }
    });
}

/// The properties of what is selected; with nothing selected, the structure's.
fn selection(app: &mut WordApp, ui: &mut Ui, info: &Value, src: &Structure, st: &Structure, state: &mut State) {
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
        // Interactive list of selected items: click one to inspect it alone, `×` to drop it from selection.
        let snapshot = state.sel.clone();
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 3.0);
            for item in &snapshot {
                let chip = match item {
                    Sel::Node(id) => crate::i18n::fmt(ts("Node {n}"), &[("n", id)]),
                    Sel::Member(id) => crate::i18n::fmt(ts("Member {n}"), &[("n", id)]),
                    Sel::Load(k) => st.loads.get(*k).map_or_else(|| format!("#{}", k + 1), |l| load_title(st, *k, l)),
                };
                if ui.small_button(&chip).on_hover_text(ts("Focus this item alone")).clicked() {
                    state.select(item.clone());
                }
            }
        });
        // Contextual bulk actions.
        ui.horizontal_wrapped(|ui| {
            if let [Sel::Member(p), Sel::Member(q)] = state.sel.as_slice()
                && let Some(shared) = edits::shared_node(st, p, q)
            {
                let res = edits::join_members(src, st, &shared);
                let why = res.as_ref().err().copied();
                if why != Some(edits::NoJoin::NotTwo) {
                    let btn = ui.add_enabled(why.is_none(), egui::Button::new(ts("Join the members")));
                    if let Some(w) = why {
                        btn.on_disabled_hover_text(ts(w.reason()));
                    } else if btn.clicked()
                        && let Ok(out) = res
                        && let Ok(j) = serde_json::to_value(&out)
                        && app.run("structure.set", json!({"structure": j})).is_ok()
                    {
                        let kept = st
                            .members
                            .iter()
                            .find(|m| (m.a == shared || m.b == shared) && out.member(&m.id).is_some())
                            .map(|m| Sel::Member(m.id.clone()));
                        state.sel = kept.into_iter().collect();
                        return;
                    }
                }
            }
            let load_indices: Vec<usize> = state.sel.iter().filter_map(|s| if let Sel::Load(k) = s { Some(*k) } else { None }).collect();
            if !load_indices.is_empty() && ui.button(ts("Reverse")).clicked() {
                let mut out = src.clone();
                for k in load_indices {
                    if let Some(rev) = edits::reversed(&out, k)
                        && let Some(slot) = out.loads.get_mut(k)
                    {
                        *slot = rev;
                    }
                }
                if let Ok(j) = serde_json::to_value(&out) {
                    let _ = app.run("structure.set", json!({"structure": j}));
                }
            }
            if ui.button(ts("Deselect")).clicked() {
                state.sel.clear();
                return;
            }
            if ui.add(destructive(ui, "Remove Selected")).clicked() {
                let kept = without(src, &state.sel);
                if app.run("structure.set", json!({"structure": kept})).is_ok() {
                    state.sel.clear();
                }
            }
        });
        ui.label(
            RichText::new(ts(
                "Drag one of them to move them together; the arrow keys move them by the grid (Shift: ten times); Delete removes them.",
            ))
            .small()
            .weak(),
        );
        return;
    }
    match state.primary().cloned() {
        Some(Sel::Node(id)) => {
            let Some(n) = src.node(&id) else { return };
            let rn = st.node(&id);
            section(ui, &crate::i18n::fmt(ts("Node {n}"), &[("n", &id)]));
            props(ui, ("node", &id), |ui| {
                prop_label(ui, ts("Name"));
                if let Some(t) = edit_box(ui, ("node_name", &id), &id, 90.0) {
                    let _ = app.run("structure.node", json!({"id": id, "rename": t}));
                    state.select(Sel::Node(t.trim().to_string()));
                }
                ui.label("");
                ui.end_row();
                for (axis, value, rval) in [("x", &n.x, rn.map(|r| r.x.value)), ("y", &n.y, rn.map(|r| r.y.value))] {
                    prop_label(ui, axis);
                    if let Some(t) = edit_box(ui, ("node", &id, axis), &num_text(value), 110.0) {
                        let _ = app.run("structure.node", json!({"id": id, axis: num_json(&t)}));
                    }
                    unit_or_resolved(ui, Some(value), rval, lu);
                    ui.end_row();
                }
                prop_label(ui, ts("Hinge"));
                let mut hinge = n.hinge;
                if ui.checkbox(&mut hinge, ts("Internal hinge")).changed() {
                    let _ = app.run("structure.node", json!({"id": id, "hinge": hinge}));
                }
                ui.label("");
                ui.end_row();
                let joined: Vec<String> = src.members.iter().filter(|m| m.a == id || m.b == id).map(|m| m.id.clone()).collect();
                prop_label(ui, ts("Members"));
                if joined.is_empty() {
                    ui.label("—");
                } else {
                    ui.horizontal_wrapped(|ui| {
                        for mid in &joined {
                            if ui.small_button(mid).on_hover_text(ts("Select the member")).clicked() {
                                state.select(Sel::Member(mid.clone()));
                            }
                        }
                    });
                }
                ui.label("");
                ui.end_row();
            });
            // Offer Join the members directly in Node Properties when 2 members meet here.
            match edits::join_members(src, st, &id) {
                Ok(joined_st) => {
                    if ui.small_button(ts("Join the members")).clicked()
                        && let Ok(j) = serde_json::to_value(&joined_st)
                        && app.run("structure.set", json!({"structure": j})).is_ok()
                    {
                        let kept = st
                            .members
                            .iter()
                            .find(|m| (m.a == id || m.b == id) && joined_st.member(&m.id).is_some())
                            .map(|m| Sel::Member(m.id.clone()));
                        state.sel = kept.into_iter().collect();
                        return;
                    }
                }
                Err(edits::NoJoin::NotTwo) => {}
                Err(why) => {
                    ui.add_enabled(false, egui::Button::new(ts("Join the members")).small()).on_disabled_hover_text(ts(why.reason()));
                }
            }
            section(ui, ts("Support"));
            let support = src.supports.iter().find(|s| s.node == id);
            let rsupport = st.supports.iter().find(|s| s.node == id);
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
                    unit_or_resolved(ui, Some(&s.angle), rsupport.map(|r| r.angle.value), "°");
                    ui.end_row();
                }
            });
            if support.is_some() {
                rotate::support_orientation_ui(app, ui, src, st, state, &id);
            }
            reactions_summary_ui(ui, info, st, Some(&id));
            node_loads(app, ui, src, st, state, &id);
            ui.add_space(6.0);
            if ui.add(destructive(ui, "Remove Node")).on_hover_text(ts("Remove the node with its members, support and loads")).clicked() {
                let _ = app.run("structure.node", json!({"id": id, "remove": true}));
                state.sel.clear();
            }
        }
        Some(Sel::Member(id)) => {
            let Some(m) = src.member(&id) else { return };
            let rm = st.member(&id);
            let len = rm.and_then(|m| st.length(m)).unwrap_or(0.0);
            let ends = rm.and_then(|m| Some(((st.node(&m.a)?.x.value, st.node(&m.a)?.y.value), (st.node(&m.b)?.x.value, st.node(&m.b)?.y.value))));
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
                ui.label(RichText::new(ts(lu)).weak());
                ui.end_row();
                if let Some((pa, pb)) = ends {
                    prop_label(ui, ts("Slope"));
                    ui.label(short(slope_deg(pa, pb)));
                    ui.label(RichText::new("°").weak());
                    ui.end_row();
                }
            });
            ui.horizontal_wrapped(|ui| {
                if ui.small_button(ts("Swap ends")).on_hover_text(ts("Reverse the member's local axis (a ↔ b), keeping loads in place")).clicked()
                    && let Some(out) = edits::reverse_member(src, st, &id)
                    && let Ok(j) = serde_json::to_value(&out)
                {
                    let _ = app.run("structure.set", json!({"structure": j}));
                }
                if len > 1e-6
                    && ui.small_button(ts("Split in half")).clicked()
                    && let Some((out, node)) = edits::split_member(src, st, &id, round(len / 2.0))
                    && let Ok(j) = serde_json::to_value(&out)
                    && app.run("structure.set", json!({"structure": j})).is_ok()
                {
                    state.select(Sel::Node(node));
                }
            });
            member_forces_ui(ui, info, st, &id);
            section(ui, ts("Section"));
            props(ui, ("member_section", &id), |ui| {
                for (key, label, value, rval, unit) in [
                    ("e", "E", &m.e, rm.and_then(|r| r.e.as_ref().map(|x| x.value)), format!("{}/{lu}²", ts(fu))),
                    ("i", "I", &m.i, rm.and_then(|r| r.i.as_ref().map(|x| x.value)), format!("{}⁴", ts(lu))),
                    ("area", "A", &m.area, rm.and_then(|r| r.area.as_ref().map(|x| x.value)), format!("{}²", ts(lu))),
                ] {
                    prop_label(ui, label);
                    let text = value.as_ref().map(num_text).unwrap_or_default();
                    if let Some(t) = edit_box(ui, ("member", &id, key), &text, 110.0) {
                        let v = if t.trim().is_empty() { Value::Null } else { num_json(&t) };
                        let _ = app.run("structure.member", json!({"id": id, key: v}));
                    }
                    match (value.as_ref().and_then(|n| n.expr.as_ref()), rval) {
                        (Some(_), Some(v)) => {
                            ui.label(RichText::new(format!("= {} {unit}", short(v))).small().color(Color32::from_rgb(0x0F, 0x76, 0x6E)));
                        }
                        _ => {
                            ui.label(RichText::new(unit).weak());
                        }
                    }
                    ui.end_row();
                }
            });
            ui.label(RichText::new(ts("Without E, I, A: EI is the same for every member and members do not stretch.")).small().weak());
            member_loads(app, ui, src, st, state, &id);
            ui.add_space(6.0);
            if ui.add(destructive(ui, "Remove Member")).on_hover_text(ts("Remove the member with its loads")).clicked() {
                let _ = app.run("structure.member", json!({"id": id, "remove": true}));
                state.sel.clear();
            }
        }
        Some(Sel::Load(k)) => {
            let hdr = st.loads.get(k).map_or_else(|| ts("Load").to_string(), |l| load_title(st, k, l));
            section(ui, &hdr);
            loads_table(app, ui, src, st, state, Some(k));
        }
        None => structure_props(ui, info, src, st),
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
            Load::Distributed { member, then, .. } => member == id || then.iter().any(|m| m == id),
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
    let fu = ts(st.force_unit.symbol());
    let lu = ts(st.length_unit.symbol());
    for &k in on {
        let Some(l) = src.loads.get(k) else { continue };
        let head = load_title(st, k, l);
        let text = match l {
            Load::Force { value, angle, .. } => format!("{head}: F = {} {fu}, {}°", num_text(value), num_text(angle)),
            Load::Moment { value, .. } => format!("{head}: C = {} {fu}·{lu}", num_text(value)),
            Load::Distributed { q1, q2, .. } => {
                let q = match q2 {
                    Some(q2) => format!("{} → {}", num_text(q1), num_text(q2)),
                    None => num_text(q1),
                };
                format!("{head}: q = {q} {fu}/{lu}")
            }
        };
        if ui.selectable_label(state.has(&Sel::Load(k)), text).on_hover_text(ts("Select the load to edit it")).clicked() {
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
            let idx = src.loads.len();
            if app.run("structure.load", json!({"load": load})).is_ok() {
                state.select_new_load(idx);
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

/// The editor's view and grid after the structure's lengths were multiplied by `k` (a change of
/// unit): the sheet shows the same thing, the grid is as wide on it.
fn rescale(s: &mut State, k: f64) {
    if !(k.is_finite() && k > 0.0) {
        return;
    }
    s.view = s.view.map(|(cx, cy, px)| (cx * k, cy * k, px / k));
    s.grid = (s.grid * k).clamp(MIN_GRID, MAX_GRID);
    s.start = s.start.map(|(x, y)| (x * k, y * k));
    s.gesture = None;
}

/// The grid's step (in the structure's length unit) is kept within these.
pub(crate) const MIN_GRID: f64 = 0.01;
pub(crate) const MAX_GRID: f64 = 10_000.0;

/// With nothing selected: the structure's counts and solved reactions summary (its title,
/// units, diagram names and positive senses are on the Structures tab).
fn structure_props(ui: &mut Ui, info: &Value, src: &Structure, st: &Structure) {
    section(ui, ts("Made of"));
    props(ui, "structure_counts", |ui| {
        for (label, n) in [("Nodes", src.nodes.len()), ("Members", src.members.len()), ("Supports", src.supports.len()), ("Loads", src.loads.len())] {
            prop_label(ui, ts(label));
            ui.label(n.to_string());
            ui.end_row();
        }
    });
    reactions_summary_ui(ui, info, st, None);
    ui.add_space(4.0);
    ui.label(
        RichText::new(ts("Click a node, a member or a load to see its properties; Shift+click or a window adds to the selection.")).small().weak(),
    );
}

fn inspector(app: &mut WordApp, ui: &mut Ui, info: &Value, src: &Structure, st: &Structure, state: &mut State) {
    let width = ui.available_width();
    egui::ScrollArea::horizontal().id_salt("structure_properties_width").auto_shrink([false, true]).show(ui, |ui| {
        ui.set_max_width(width);
        selection(app, ui, info, src, st, state);
    });
    ui.add_space(6.0);
    tables(app, ui, src, st, state);
}

fn tables(app: &mut WordApp, ui: &mut Ui, src: &Structure, st: &Structure, state: &mut State) {
    section(ui, ts("Tables"));
    let title = |name: &str, n: usize| RichText::new(format!("{} ({n})", ts(name))).strong();
    table_section(ui, title("Nodes", src.nodes.len()), "structure_nodes", |ui| nodes_table(app, ui, src, state));
    table_section(ui, title("Members", src.members.len()), "structure_members", |ui| members_table(app, ui, src, state));
    table_section(ui, title("Supports", src.supports.len()), "structure_supports", |ui| supports_table(app, ui, src, state));
    table_section(ui, title("Loads", src.loads.len()), "structure_loads", |ui| loads_table(app, ui, src, st, state, None));
}

fn table_section(ui: &mut Ui, title: RichText, id: &str, contents: impl FnOnce(&mut Ui)) {
    egui::CollapsingHeader::new(title).id_salt(id).show(ui, |ui| {
        egui::ScrollArea::horizontal().id_salt((id, "width")).auto_shrink([false, true]).show(ui, contents);
    });
}

fn destructive(ui: &Ui, label: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(ts(label).to_owned()).color(ui.visuals().error_fg_color))
}

fn remove_button(ui: &mut Ui, tip: &str) -> bool {
    let response = ui.add(destructive(ui, "×").small()).on_hover_text(ts(tip));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), ts(tip)));
    response.clicked()
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

fn supports_table(app: &mut WordApp, ui: &mut Ui, src: &Structure, state: &mut State) {
    egui::Grid::new("structure_supports_grid").num_columns(4).striped(true).spacing(vec2(6.0, 3.0)).show(ui, |ui| {
        for h in ["Node", "Support", "Angle (°)", ""] {
            ui.label(RichText::new(ts(h)).small().strong());
        }
        ui.end_row();
        for s in &src.supports {
            if ui.selectable_label(state.has(&Sel::Node(s.node.clone())), RichText::new(&s.node).strong()).clicked() {
                state.select(Sel::Node(s.node.clone()));
            }
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

/// The loads' editors (`only`: just that load's, with its interactive inclination dial, without
/// the table's Add buttons).
fn loads_table(app: &mut WordApp, ui: &mut Ui, src: &Structure, st: &Structure, state: &mut State, only: Option<usize>) {
    let fu = st.force_unit.symbol();
    let lu = st.length_unit.symbol();
    for (k, l) in src.loads.iter().enumerate() {
        if only.is_some_and(|o| o != k) {
            continue;
        }
        let rl = st.loads.get(k);
        let mut lj = serde_json::to_value(l).unwrap_or(Value::Null);
        let mut changed = false;
        let mut remove = false;
        egui::Frame::group(ui.style()).inner_margin(6.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let hdr = load_title(st, k, l);
                if only.is_none() {
                    if ui
                        .selectable_label(state.has(&Sel::Load(k)), RichText::new(hdr).strong())
                        .on_hover_text(ts("Select the load to edit it"))
                        .clicked()
                    {
                        state.select(Sel::Load(k));
                    }
                } else {
                    ui.label(RichText::new(hdr).strong());
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("×").on_hover_text(ts("Remove the load")).clicked() {
                        remove = true;
                    }
                });
            });
            let mut edits: Vec<(&'static str, Value)> = Vec::new();
            let mut field = |ui: &mut Ui, label: &str, key: &'static str, src_num: Option<&Num>, rval: Option<f64>, unit: &str, optional: bool| {
                prop_label(ui, label);
                let current = src_num.map(num_text).unwrap_or_default();
                if let Some(t) = edit_box(ui, (format!("load_{key}"), k), &current, 90.0) {
                    edits.push((key, if optional && t.trim().is_empty() { Value::Null } else { num_json(&t) }));
                }
                unit_or_resolved(ui, src_num, rval, unit);
                ui.end_row();
            };
            props(ui, ("load", k), |ui| {
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
                    Load::Distributed { member, then, .. } => {
                        // Over several aligned members: all of them (`1 + 2`).
                        let on = if then.is_empty() {
                            format!("{} {member}", ts("member"))
                        } else {
                            format!("{} {member} + {}", ts("members"), then.join(" + "))
                        };
                        craft_ui::ComboBox::from_id_salt(("structure_load_member", k)).width(90.0).selected_text(on).show_ui(ui, |ui| {
                            for m in &src.members {
                                if ui.selectable_label(*member == m.id && then.is_empty(), format!("{} {}", ts("member"), m.id)).clicked() {
                                    lj["member"] = json!(m.id);
                                    lj.as_object_mut().map(|o| o.remove("then"));
                                    changed = true;
                                }
                            }
                        });
                    }
                }
                ui.label("");
                ui.end_row();
                match (l, rl) {
                    (Load::Force { member, at, value, angle, .. }, Some(Load::Force { at: rat, value: rval, angle: rang, .. })) => {
                        if member.is_some() {
                            field(ui, ts("at"), "at", Some(at), Some(rat.value), lu, false);
                        }
                        field(ui, "F", "value", Some(value), Some(rval.value), fu, false);
                        field(ui, ts("direction"), "angle", Some(angle), Some(rang.value), "°", false);
                    }
                    (Load::Moment { member, at, value, .. }, Some(Load::Moment { at: rat, value: rval, .. })) => {
                        if member.is_some() {
                            field(ui, ts("at"), "at", Some(at), Some(rat.value), lu, false);
                        }
                        field(ui, "C", "value", Some(value), Some(rval.value), &format!("{fu}·{lu}"), false);
                    }
                    (
                        Load::Distributed { q1, q2, from, to, angle, .. },
                        Some(Load::Distributed { q1: rq1, q2: rq2, from: rf, to: rt, angle: rang, .. }),
                    ) => {
                        let q_u = format!("{fu}/{lu}");
                        field(ui, "q1", "q1", Some(q1), Some(rq1.value), &q_u, false);
                        field(ui, "q2", "q2", q2.as_ref(), rq2.as_ref().map(|x| x.value), &q_u, true);
                        field(ui, ts("from"), "from", Some(from), Some(rf.value), lu, false);
                        field(ui, ts("to"), "to", to.as_ref(), rt.as_ref().map(|x| x.value), lu, true);
                        field(ui, ts("direction"), "angle", Some(angle), Some(rang.value), "°", false);
                    }
                    _ => {}
                }
            });
            for (key, v) in edits {
                lj[key] = v;
                changed = true;
            }
            if only.is_some() && matches!(l, Load::Force { .. } | Load::Distributed { .. }) {
                rotate::load_inclination_ui(app, ui, src, st, state, k);
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
            let prev = src.loads.len();
            if app.run("structure.load", json!({"index": k, "remove": true})).is_ok() {
                state.on_load_removed(k, prev);
            }
            return;
        }
        if changed {
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
        let idx = src.loads.len();
        if ui.add_enabled(first_node.is_some(), egui::Button::new(ts("Add Force"))).clicked()
            && app.run("structure.load", json!({"load": {"type": "force", "node": first_node, "value": 10.0}})).is_ok()
        {
            state.select_new_load(idx);
        }
        if ui.add_enabled(first_node.is_some(), egui::Button::new(ts("Add Couple"))).clicked()
            && app.run("structure.load", json!({"load": {"type": "moment", "node": first_node, "value": 10.0}})).is_ok()
        {
            state.select_new_load(idx);
        }
        if ui.add_enabled(first_member.is_some(), egui::Button::new(ts("Add Distributed Load"))).clicked()
            && app.run("structure.load", json!({"load": {"type": "distributed", "member": first_member, "q1": 10.0}})).is_ok()
        {
            state.select_new_load(idx);
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

    /// A change of length unit converts the structure (1 m is 100 cm): the view and the grid
    /// follow, so the sheet shows the same thing.
    #[test]
    fn a_change_of_unit_rescales_the_view_and_the_grid() {
        let mut st = Structure::default();
        let mut state = State { view: Some((3.0, 1.0, 50.0)), grid: 0.5, ..State::default() };
        state.prune(&st);
        assert_eq!(state.view, Some((3.0, 1.0, 50.0)));
        st.length_unit = strix_structure::LengthUnit::Cm;
        state.prune(&st);
        let (cx, cy, px) = state.view.unwrap();
        assert!((cx - 300.0).abs() < 1e-9 && (cy - 100.0).abs() < 1e-9 && (px - 0.5).abs() < 1e-12);
        assert!((state.grid - 50.0).abs() < 1e-9);
        st.length_unit = strix_structure::LengthUnit::M;
        state.prune(&st);
        assert!((state.grid - 0.5).abs() < 1e-12);
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

    /// The empty pane inserts a structure and opens the full-page editor on it; the pane shows it
    /// and its classification.
    #[test]
    fn the_pane_inserts_and_shows_a_structure() {
        let app = WordApp::new(Session::new(wordcraft_doc::Document::new()), crate::Services::default());
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(440.0, 1400.0)).build_ui_state(|ui, app: &mut WordApp| pane(app, ui), app);
        h.run_steps(2);
        h.get_by_label(ts("Simply Supported Beam")).click();
        h.run_steps(3);
        assert!(h.state().structure_edit.is_some(), "the full-page editor is open");
        h.get_by_label(ts("Isostatic"));
        h.get_by_label(&format!("{} (1)", ts("Loads")));
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
        for name in [
            "rotate",
            "node",
            "member",
            "fixed",
            "pin",
            "roller",
            "rollerCircle",
            "link",
            "slider",
            "hinge",
            "force",
            "couple",
            "distributed",
            "erase",
            "select",
        ] {
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

    /// Graphical vector rotation (`Tool::Rotate` protractor + `DistAngle` & `SupportAngle` grips),
    /// `Tool::Erase` on loads and support symbols, `Tool::Hinge` on member ends, and immediate
    /// selection of newly placed loads.
    #[test]
    fn rotate_tool_and_grips_incline_vectors_and_supports() {
        let mut app = WordApp::new(Session::new(wordcraft_doc::Document::new()), crate::Services::default());
        app.run("structure.insert", json!({"template": "beam"})).unwrap();
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(600.0, 400.0));
        let st = resolved(&mut app);
        let (cx, cy, s) = fit(&st, rect);
        let v = View { rect, cx, cy, s };

        // Adding a force with Tool::Force selects the new load right away.
        let mut state = State { tool: Tool::Force, ..State::default() };
        click(&mut app, &st, v, v.to_screen(3.0, 0.0), &mut state);
        let st = resolved(&mut app);
        assert_eq!(state.sel, vec![Sel::Load(1)]);

        // Tool::Rotate: clicking at −45° on the active protractor rim sets the force to −45°.
        state.tool = Tool::Rotate;
        let center = v.to_screen(3.0, 0.0);
        let at_m45 = rotate::ring_pos(center, rotate::PROTRACTOR_R, -45.0);
        click(&mut app, &st, v, at_m45, &mut state);
        let st = resolved(&mut app);
        assert!(matches!(&st.loads[1], Load::Force { angle, .. } if (angle.value - (-45.0)).abs() < 1e-6), "{:?}", st.loads[1]);

        // Distributed load inclination grip (`Grip::DistAngle(0)`): dragging tail up-left inclines arrows down-right (−60°).
        let src: Structure = serde_json::from_value(app.session.run("structure.get", &json!({})).unwrap()["structure"].clone()).unwrap();
        let tail_120 = rotate::ring_pos(v.to_screen(3.0, 0.0), 40.0, 120.0);
        grips::commit(&mut app, &src, &st, v, &mut state, &grips::Grip::DistAngle(0), tail_120, false);
        let st = resolved(&mut app);
        assert!(matches!(&st.loads[0], Load::Distributed { angle, .. } if (angle.value - (-60.0)).abs() < 1e-6), "{:?}", st.loads[0]);

        // Support angle grip (`Grip::SupportAngle("B")`): dragging ground to the right of B sets support angle to +90°.
        let src: Structure = serde_json::from_value(app.session.run("structure.get", &json!({})).unwrap()["structure"].clone()).unwrap();
        let right_of_b = v.to_screen(6.0, 0.0) + vec2(30.0, 0.0);
        grips::commit(&mut app, &src, &st, v, &mut state, &grips::Grip::SupportAngle("B".into()), right_of_b, false);
        let st = resolved(&mut app);
        assert_eq!(st.supports.iter().find(|sup| sup.node == "B").map(|sup| sup.angle.value), Some(90.0));

        // Tool::Hinge near end A of member 1 (away from node A itself) toggles hingeA.
        state.tool = Tool::Hinge;
        let near_a = v.to_screen(0.0, 0.0) + vec2(PICK + 6.0, 0.0);
        click(&mut app, &st, v, near_a, &mut state);
        let st = resolved(&mut app);
        assert!(st.member("1").unwrap().hinge_a);

        // Tool::Erase on the force's shaft removes only that force, keeping member 1 intact.
        state.tool = Tool::Erase;
        let shape = load_shape(&st, v, 1).unwrap();
        let [a, b] = shape.as_slice() else { panic!("a force's shape is its two ends, got {shape:?}") };
        let force_shaft = pos2(0.5 * (a.x + b.x), 0.5 * (a.y + b.y));
        click(&mut app, &st, v, force_shaft, &mut state);
        let st = resolved(&mut app);
        assert_eq!(st.loads.len(), 1);
        assert_eq!(st.members.len(), 1);

        // Tool::Erase on node A's Pin support symbol (below node A) removes only the support, keeping node A.
        let on_pin = v.to_screen(0.0, 0.0) + vec2(0.0, 3.8 * MM);
        click(&mut app, &st, v, on_pin, &mut state);
        let st = resolved(&mut app);
        assert!(st.node("A").is_some(), "node A stays");
        assert!(st.supports.iter().all(|s| s.node != "A"), "pin support removed");
    }
}
