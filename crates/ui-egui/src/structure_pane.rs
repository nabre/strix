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

/// What is selected on the sheet.
#[derive(Clone, Debug, PartialEq)]
enum Sel {
    Node(String),
    Member(String),
}

/// The editor's state between frames.
#[derive(Clone, Debug)]
struct State {
    tool: Tool,
    sel: Option<Sel>,
    /// The world point at the sheet's centre and pixels per unit; `None`: fit on the next frame.
    view: Option<(f64, f64, f64)>,
    /// The structure the view was fitted to (a new one fits again).
    fitted_for: String,
    /// A member being drawn: its first point.
    start: Option<(f64, f64)>,
    /// A node being dragged and where it is now.
    drag: Option<(String, (f64, f64))>,
    grid: f64,
    snap: bool,
    /// The coordinates typed.
    typed: String,
}

impl Default for State {
    fn default() -> Self {
        State {
            tool: Tool::Select,
            sel: None,
            view: None,
            fitted_for: String::new(),
            start: None,
            drag: None,
            grid: 0.5,
            snap: true,
            typed: String::new(),
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
    // A selection that no longer exists goes.
    if let Some(Sel::Node(id)) = &state.sel
        && st.node(id).is_none()
    {
        state.sel = None;
    }
    if let Some(Sel::Member(id)) = &state.sel
        && st.member(id).is_none()
    {
        state.sel = None;
    }
    status_line(ui, &info);
    toolbar(ui, &mut state);
    sheet(app, ui, &st, &mut state);
    typed_point(app, ui, &st, &mut state);
    egui::ScrollArea::vertical().id_salt("structure_pane_scroll").show(ui, |ui| {
        selection(app, ui, &src, &st, &mut state);
        ui.add_space(4.0);
        tables(app, ui, &src, &st, &mut state);
    });
    ui.data_mut(|d| d.insert_temp(state_id(), state));
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
        (Tool::Select, "Select", "Select a node or a member, drag a node to move it"),
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

fn sheet(app: &mut WordApp, ui: &mut Ui, st: &Structure, state: &mut State) {
    let h = (ui.available_height() * 0.45).clamp(220.0, 460.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click_and_drag());
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

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, Color32::WHITE);
    grid_lines(&painter, v, state.grid);
    // The structure as the document draws it (a dragged node where it is now).
    let mut shown = st.clone();
    if let Some((id, (x, y))) = &state.drag
        && let Some(n) = shown.nodes.iter_mut().find(|n| &n.id == id)
    {
        n.x = Num::new(*x);
        n.y = Num::new(*y);
    }
    let origin = v.to_world(rect.min);
    let prims = shown.schema_prims(v.s / f64::from(PX_PER_MM), origin, strix_calc::Options::default());
    paint_prims(&painter, rect.min, &prims);
    // The selection.
    let accent = Color32::from_rgb(0xE0, 0x7B, 0x1F);
    match &state.sel {
        Some(Sel::Node(id)) => {
            if let Some(n) = shown.node(id) {
                painter.circle_stroke(v.to_screen(n.x.value, n.y.value), 6.0, Stroke::new(2.0, accent));
            }
        }
        Some(Sel::Member(id)) => {
            if let Some(m) = shown.member(id)
                && let (Some(a), Some(b)) = (shown.node(&m.a), shown.node(&m.b))
            {
                painter.line_segment(
                    [v.to_screen(a.x.value, a.y.value), v.to_screen(b.x.value, b.y.value)],
                    Stroke::new(4.0, accent.linear_multiply(0.6)),
                );
            }
        }
        None => {}
    }
    let hover = resp.hover_pos();
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
    if let Some(((x, y), _)) = &snapped {
        let unit = st.length_unit.symbol();
        painter.text(
            rect.left_bottom() + vec2(6.0, -6.0),
            Align2::LEFT_BOTTOM,
            format!("x = {} {unit}   y = {} {unit}", short(*x), short(*y)),
            FontId::proportional(11.0),
            Color32::DARK_GRAY,
        );
    }
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) || resp.secondary_clicked() {
        state.start = None;
    }
    // Dragging a node with Select.
    if state.tool == Tool::Select {
        if resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(p) = resp.interact_pointer_pos()
            && let Some(id) = pick_node(st, v, p)
        {
            state.sel = Some(Sel::Node(id.clone()));
            state.drag = Some((id, snap_free(v, p, state)));
        }
        if let (Some((id, _)), Some(p)) = (state.drag.clone(), resp.interact_pointer_pos())
            && resp.dragged_by(egui::PointerButton::Primary)
        {
            state.drag = Some((id, snap_free(v, p, state)));
        }
        if resp.drag_stopped()
            && let Some((id, (x, y))) = state.drag.take()
        {
            let _ = app.run("structure.node", json!({"id": id, "x": round(x), "y": round(y)}));
        }
    }
    if resp.clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        click(app, st, v, p, state);
    }
}

/// Where a dragged node goes: the grid, not the node itself.
fn snap_free(v: View, p: Pos2, state: &State) -> (f64, f64) {
    let (x, y) = v.to_world(p);
    if state.snap && state.grid > 0.0 { ((x / state.grid).round() * state.grid, (y / state.grid).round() * state.grid) } else { (x, y) }
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
        Tool::Select => {
            state.sel = pick_node(st, v, p).map(Sel::Node).or_else(|| pick_member(st, v, p).map(|(id, _)| Sel::Member(id)));
        }
        Tool::Node => {
            if on_node.is_none() {
                let id = node_at(app, st, point);
                state.sel = id.map(Sel::Node);
            }
        }
        Tool::Member => member_point(app, st, point, state),
        Tool::Support(kind) => {
            if let Some(id) = pick_node(st, v, p) {
                let _ = app.run("structure.support", json!({"node": id, "kind": kind.name()}));
                state.sel = Some(Sel::Node(id));
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
        let base = state.start.or_else(|| match &state.sel {
            Some(Sel::Node(id)) => st.node(id).map(|n| (n.x.value, n.y.value)),
            _ => None,
        });
        let point = if relative { base.map_or((*x, *y), |(bx, by)| (bx + x, by + y)) } else { (*x, *y) };
        match state.tool {
            Tool::Member => member_point(app, st, point, state),
            _ => {
                state.tool = if state.tool == Tool::Member { Tool::Member } else { Tool::Node };
                if let Some(id) = node_at(app, st, point) {
                    state.sel = Some(Sel::Node(id));
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
    match state.sel.clone() {
        Some(Sel::Node(id)) => {
            let Some(n) = src.node(&id) else { return };
            ui.label(RichText::new(crate::i18n::fmt(ts("Node {n}"), &[("n", &id)])).strong());
            egui::Grid::new("structure_node_props").num_columns(2).spacing(vec2(8.0, 4.0)).show(ui, |ui| {
                ui.label(ts("Name"));
                if let Some(t) = edit_box(ui, ("node_name", &id), &id, 80.0) {
                    let _ = app.run("structure.node", json!({"id": id, "rename": t}));
                    state.sel = Some(Sel::Node(t.trim().to_string()));
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
                state.sel = None;
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
                state.sel = None;
            }
        }
        None => {
            ui.label(RichText::new(ts("Click a node or a member to see its properties.")).weak());
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
        .show(ui, |ui| loads_table(app, ui, src, st));
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
            if ui.selectable_label(state.sel == Some(Sel::Node(n.id.clone())), RichText::new(&n.id).strong()).clicked() {
                state.sel = Some(Sel::Node(n.id.clone()));
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
            if ui.selectable_label(state.sel == Some(Sel::Member(m.id.clone())), RichText::new(&m.id).strong()).clicked() {
                state.sel = Some(Sel::Member(m.id.clone()));
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

fn loads_table(app: &mut WordApp, ui: &mut Ui, src: &Structure, st: &Structure) {
    let fu = st.force_unit.symbol();
    let lu = st.length_unit.symbol();
    for (k, l) in src.loads.iter().enumerate() {
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

    /// Clicks on the sheet with each tool change the structure as they say.
    #[test]
    fn tools_edit_the_structure() {
        let mut app = WordApp::new(Session::new(wordcraft_doc::Document::new()), crate::Services::default());
        app.run("structure.insert", json!({"template": "beam"})).unwrap();
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 300.0));
        let st = resolved(&mut app);
        let (cx, cy, s) = fit(&st, rect);
        let v = View { rect, cx, cy, s };
        let mut state = State::default();
        // A member from B (6, 0) to (8, 2): a new node C.
        state.tool = Tool::Member;
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
