//! The CAD drawing editor (docs/cad.md, stage 3): double-clicking a CAD drawing, or Math ›
//! Drawing › Edit Drawing, replaces the page with the drawing's sheet and brings up the CAD
//! Drawing tab. AutoCAD-style: a command line (typed points `x,y`, `@dx,dy`, `@d<a`, a distance
//! towards the pointer), snaps to ends, midpoints and centres, Ortho (F8), a grid; Line chains,
//! Polyline (`c` closes), Rectangle, Circle, Arc, Point, Text, Dimensions; Select (click, Shift,
//! window left to right, crossing right to left), Move, Copy, Delete. Wheel zooms around the
//! pointer, the middle button or Shift+drag pans; Esc cancels the step, then the command.
//!
//! Every finished operation is one `cad.set` (an undo step of its own); the document recalculates
//! and redraws, and the editor reads the drawing back. Coordinates written as expressions of the
//! calculations stay linked when moved (`L` → `L + 0.5`).

use std::sync::Arc;

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};
use serde_json::json;
use strix_cad::{Anchor, Drawing, Entity, Kind, Num, Orient, P, Prim, Pt, Rendered, SnapKind, outline, parse_point};
use strix_doc::InlineObject;

use crate::WordApp;

/// What a click does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Select,
    Line,
    Polyline,
    Rect,
    Circle,
    Arc,
    Point,
    Text,
    Dim(Orient),
    Move,
    Copy,
}

impl Tool {
    pub fn parse(name: &str) -> Option<Tool> {
        Some(match name {
            "select" => Tool::Select,
            "line" | "l" | "linea" => Tool::Line,
            "polyline" | "pl" | "polilinea" => Tool::Polyline,
            "rect" | "rec" | "rettangolo" => Tool::Rect,
            "circle" | "c" | "cerchio" => Tool::Circle,
            "arc" | "a" | "arco" => Tool::Arc,
            "point" | "po" | "punto" => Tool::Point,
            "text" | "t" | "testo" => Tool::Text,
            "dim" | "dimAligned" | "quota" => Tool::Dim(Orient::Aligned),
            "dimHorizontal" => Tool::Dim(Orient::Horizontal),
            "dimVertical" => Tool::Dim(Orient::Vertical),
            "move" | "m" | "sposta" => Tool::Move,
            "copy" | "co" | "copia" => Tool::Copy,
            _ => return None,
        })
    }

    /// What the command line asks for after `n` points.
    fn prompt(self, n: usize) -> &'static str {
        match (self, n) {
            (Tool::Select, _) => "Select objects: click, or drag a window (left to right) or a crossing (right to left)",
            (Tool::Line, 0) => "Line: first point",
            (Tool::Line, _) => "Line: next point (Enter or Esc to end)",
            (Tool::Polyline, 0) => "Polyline: first point",
            (Tool::Polyline, _) => "Polyline: next point (Enter to end, c to close)",
            (Tool::Rect, 0) => "Rectangle: first corner",
            (Tool::Rect, _) => "Rectangle: other corner",
            (Tool::Circle, 0) => "Circle: centre",
            (Tool::Circle, _) => "Circle: radius (a point or a number)",
            (Tool::Arc, 0) => "Arc: centre",
            (Tool::Arc, 1) => "Arc: start point",
            (Tool::Arc, _) => "Arc: end point (counter-clockwise)",
            (Tool::Point, _) => "Point: location",
            (Tool::Text, 0) => "Text: position",
            (Tool::Text, _) => "Text: type the text and press Enter",
            (Tool::Dim(_), 0) => "Dimension: first point",
            (Tool::Dim(_), 1) => "Dimension: second point",
            (Tool::Dim(_), _) => "Dimension: where the dimension line goes",
            (Tool::Move | Tool::Copy, 0) => "Base point (select the objects first)",
            (Tool::Move | Tool::Copy, _) => "Second point",
        }
    }
}

/// The editor's state while a drawing is open.
pub struct CadEditor {
    /// The drawing's place in the document.
    pos: strix_doc::Pos,
    /// The source it was read from: a change (undo, recalculation) is read back.
    source: Option<Arc<str>>,
    drawing: Drawing,
    rendered: Rendered,
    /// The calculation parameters defined where the drawing sits (for annotations).
    pub names: Vec<String>,
    pub tool: Tool,
    points: Vec<Pt>,
    pub selection: Vec<usize>,
    pub snap: bool,
    pub ortho: bool,
    pub grid: bool,
    command: String,
    message: String,
    /// The drawing point at the centre of the view, and screen points per drawing unit.
    center: Pt,
    zoom: f64,
    fitted: bool,
    window_from: Option<Pos2>,
    /// The tab shown before the editor opened.
    pub prev_tab: Option<String>,
}

/// Opens the selected CAD drawing in the editor.
pub fn open(app: &mut WordApp) -> Result<(), String> {
    let (pos, drawing, rendered, names) = strix_engine::cmd::cad::edited(&app.session).ok_or("select a CAD drawing first")?;
    let source = current_source(app, &pos);
    app.cad = Some(CadEditor {
        pos,
        source,
        drawing,
        rendered,
        names,
        tool: Tool::Select,
        points: Vec::new(),
        selection: Vec::new(),
        snap: true,
        ortho: false,
        grid: true,
        command: String::new(),
        message: String::new(),
        center: Pt::default(),
        zoom: 1.0,
        fitted: false,
        window_from: None,
        prev_tab: None,
    });
    Ok(())
}

/// The CAD source of the graphic at `pos`, if it is still one.
fn current_source(app: &WordApp, pos: &strix_doc::Pos) -> Option<Arc<str>> {
    match app.session.doc.para_at(pos).and_then(|p| p.object_at(pos.off)) {
        Some(InlineObject::Graphic { graphic, .. }) if graphic.kind == strix_doc::graphic::GraphicKind::Cad => graphic.cad.clone(),
        _ => None,
    }
}

/// Selects the drawing in the document (the `cad.*` commands act on the selection).
fn select_drawing(app: &mut WordApp, pos: &strix_doc::Pos) {
    let end = strix_doc::Pos { off: pos.off + strix_doc::para::OBJ.len_utf8(), ..pos.clone() };
    app.session.sel = strix_engine::Selection { anchor: pos.clone(), focus: end };
}

/// Reads the drawing back after the document changed (an edit, Undo, a recalculation).
fn reload(app: &mut WordApp) {
    let Some(pos) = app.cad.as_ref().map(|e| e.pos.clone()) else { return };
    let source = current_source(app, &pos);
    if source.is_none() {
        // The drawing is gone (undone or deleted): the editor closes.
        close(app);
        return;
    }
    select_drawing(app, &pos);
    if let Some((_, d, r, names)) = strix_engine::cmd::cad::edited(&app.session)
        && let Some(ed) = app.cad.as_mut()
    {
        ed.drawing = d;
        ed.rendered = r;
        ed.names = names;
        ed.source = source;
        ed.selection.retain(|&i| i < ed.drawing.entities.len());
    }
}

/// Stores the drawing in the document (one undo step) and reads it back.
fn commit(app: &mut WordApp, drawing: &Drawing) {
    let Some(pos) = app.cad.as_ref().map(|e| e.pos.clone()) else { return };
    select_drawing(app, &pos);
    let result = app.run("cad.set", json!({"drawing": drawing.to_json()}));
    if let (Err(e), Some(ed)) = (result, app.cad.as_mut()) {
        ed.message = e;
    }
    reload(app);
}

pub fn close(app: &mut WordApp) {
    if let Some(ed) = app.cad.take() {
        select_drawing(app, &ed.pos);
        if let Some(tab) = ed.prev_tab {
            app.ui.tab = tab;
        }
    }
}

/// UI commands (`ui.cad*`).
pub fn command(app: &mut WordApp, id: &str, p: &serde_json::Value) -> Result<serde_json::Value, String> {
    if id == "ui.cadEdit" {
        open(app)?;
        return Ok(json!({"open": true}));
    }
    if app.cad.is_none() {
        return Err("no CAD drawing is open".into());
    }
    match id {
        "ui.cadTool" => {
            let tool = p
                .get("tool")
                .and_then(serde_json::Value::as_str)
                .and_then(Tool::parse)
                .ok_or("tool: select, line, polyline, rect, circle, arc, point, text, dimAligned, dimHorizontal, dimVertical, move, copy")?;
            if let Some(ed) = app.cad.as_mut() {
                ed.tool = tool;
                ed.points.clear();
                ed.message.clear();
            }
        }
        "ui.cadToggle" => {
            let what = p.get("what").and_then(serde_json::Value::as_str).unwrap_or_default();
            if let Some(ed) = app.cad.as_mut() {
                match what {
                    "snap" => ed.snap = !ed.snap,
                    "ortho" => ed.ortho = !ed.ortho,
                    "grid" => ed.grid = !ed.grid,
                    _ => return Err("what: snap, ortho or grid".into()),
                }
            }
        }
        "ui.cadDelete" => delete_selection(app),
        "ui.cadFit" => {
            if let Some(ed) = app.cad.as_mut() {
                ed.fitted = false;
            }
        }
        "ui.cadClose" => close(app),
        _ => return Err(format!("unknown command {id}")),
    }
    Ok(json!({}))
}

fn delete_selection(app: &mut WordApp) {
    let Some(ed) = app.cad.as_mut() else { return };
    if ed.selection.is_empty() {
        return;
    }
    let mut d = ed.drawing.clone();
    d.remove(&ed.selection);
    ed.selection.clear();
    commit(app, &d);
}

/// Screen ↔ drawing coordinates for the view in `rect`.
#[derive(Clone, Copy)]
struct View {
    rect: Rect,
    center: Pt,
    zoom: f64,
}

impl View {
    fn to_screen(self, p: Pt) -> Pos2 {
        let c = self.rect.center();
        pos2(c.x + ((p.x - self.center.x) * self.zoom) as f32, c.y - ((p.y - self.center.y) * self.zoom) as f32)
    }
    fn to_drawing(self, s: Pos2) -> Pt {
        let c = self.rect.center();
        Pt::new(self.center.x + f64::from(s.x - c.x) / self.zoom, self.center.y - f64::from(s.y - c.y) / self.zoom)
    }
}

/// A primitive's paper point (mm, y down) as a drawing point.
fn paper_to_drawing(r: &Rendered, (x, y): (f64, f64)) -> Pt {
    let k = if r.scale > 0.0 { r.scale } else { 1.0 };
    Pt::new((x + r.origin.0) / k, (r.origin.1 - y) / k)
}

/// A grid step (1, 2 or 5 × 10ⁿ drawing units) at least 18 px apart.
fn grid_step(zoom: f64) -> f64 {
    let raw = 18.0 / zoom.max(1e-9);
    let p = 10f64.powf(raw.log10().floor());
    [1.0, 2.0, 5.0, 10.0].iter().map(|m| m * p).find(|s| *s >= raw).unwrap_or(10.0 * p)
}

fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// The editor's sheet, status line and command line, in place of the page.
pub fn show(app: &mut WordApp, ui: &mut Ui) {
    // Undo, a recalculation or any other change to the drawing in the document: read it back.
    if let Some(ed) = app.cad.as_ref() {
        let now = current_source(app, &ed.pos);
        let same = match (&now, &ed.source) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        };
        if !same {
            reload(app);
        }
    }
    let t = crate::theme::Tokens::get(ui.ctx());
    let mut finished: Option<Drawing> = None;
    let mut close_now = false;
    let mut delete_now = false;
    {
        let Some(ed) = app.cad.as_mut() else { return };
        let bottom = 58.0;
        let full = ui.available_rect_before_wrap();
        let sheet = Rect::from_min_max(full.min, pos2(full.max.x, (full.max.y - bottom).max(full.min.y + 40.0)));
        let (resp, painter) = ui.allocate_painter(sheet.size(), Sense::click_and_drag());
        let rect = resp.rect;
        painter.rect_filled(rect, 0.0, Color32::WHITE);
        // Fit the drawing on first show (and on Zoom Fit).
        if !ed.fitted {
            let k = if ed.rendered.scale > 0.0 { ed.rendered.scale } else { 1.0 };
            let (w, h) = (ed.rendered.size.0 / k, ed.rendered.size.1 / k);
            ed.center = paper_to_drawing(&ed.rendered, (ed.rendered.size.0 / 2.0, ed.rendered.size.1 / 2.0));
            ed.zoom = (f64::from(rect.width()) * 0.8 / w.max(1e-9)).min(f64::from(rect.height()) * 0.8 / h.max(1e-9)).clamp(1e-6, 1e9);
            ed.fitted = true;
        }
        let input = ui.input(|i| (i.pointer.hover_pos(), i.smooth_scroll_delta.y, i.pointer.middle_down(), i.modifiers, i.pointer.delta()));
        let (hover, scroll, middle, mods, delta) = input;
        let hovering = hover.filter(|p| rect.contains(*p));
        // Zoom about the pointer; pan with the middle button or Shift+drag.
        if let Some(p) = hovering
            && scroll != 0.0
        {
            let before = View { rect, center: ed.center, zoom: ed.zoom }.to_drawing(p);
            ed.zoom = (ed.zoom * f64::from((scroll / 200.0).exp())).clamp(1e-6, 1e9);
            let after = View { rect, center: ed.center, zoom: ed.zoom }.to_drawing(p);
            ed.center = ed.center.plus(before.minus(after));
        }
        if hovering.is_some() && (middle || (mods.shift && resp.dragged())) {
            ed.center = ed.center.minus(Pt::new(f64::from(delta.x) / ed.zoom, -f64::from(delta.y) / ed.zoom));
        }
        let view = View { rect, center: ed.center, zoom: ed.zoom };
        // Grid.
        if ed.grid {
            let step = grid_step(ed.zoom);
            let a = view.to_drawing(rect.left_bottom());
            let b = view.to_drawing(rect.right_top());
            let light = Stroke::new(0.5, Color32::from_gray(232));
            let mut x = (a.x / step).floor() * step;
            for _ in 0..400 {
                if x > b.x {
                    break;
                }
                let s = view.to_screen(Pt::new(x, 0.0)).x;
                painter.line_segment([pos2(s, rect.top()), pos2(s, rect.bottom())], light);
                x += step;
            }
            let mut y = (a.y / step).floor() * step;
            for _ in 0..400 {
                if y > b.y {
                    break;
                }
                let s = view.to_screen(Pt::new(0.0, y)).y;
                painter.line_segment([pos2(rect.left(), s), pos2(rect.right(), s)], light);
                y += step;
            }
            let o = view.to_screen(Pt::default());
            painter.line_segment([pos2(o.x, rect.top()), pos2(o.x, rect.bottom())], Stroke::new(0.8, Color32::from_gray(205)));
            painter.line_segment([pos2(rect.left(), o.y), pos2(rect.right(), o.y)], Stroke::new(0.8, Color32::from_gray(205)));
        }
        // The drawing.
        let k = if ed.rendered.scale > 0.0 { ed.rendered.scale } else { 1.0 };
        for prim in &ed.rendered.prims {
            match prim {
                Prim::Path { pts, closed, color, width, .. } => {
                    let screen: Vec<Pos2> = pts.iter().map(|&q| view.to_screen(paper_to_drawing(&ed.rendered, q))).collect();
                    let stroke = Stroke::new((*width as f32 * 3.0).clamp(1.0, 4.0), rgb(*color));
                    painter.add(if *closed { Shape::closed_line(screen, stroke) } else { Shape::line(screen, stroke) });
                }
                Prim::Dot { at, r, color } => {
                    painter.circle_filled(
                        view.to_screen(paper_to_drawing(&ed.rendered, *at)),
                        (*r * ed.zoom / k).clamp(2.0, 12.0) as f32,
                        rgb(*color),
                    );
                }
                Prim::Text { at, text, height, anchor, color, .. } => {
                    let shown: String = text.chars().filter(|&c| !strix_cad::is_marker(c)).collect();
                    let size = ((*height / 0.7) * ed.zoom / k).clamp(6.0, 200.0) as f32;
                    let align = match anchor {
                        Anchor::BottomCenter => Align2::CENTER_BOTTOM,
                        Anchor::BaselineLeft => Align2::LEFT_BOTTOM,
                        Anchor::MiddleLeft => Align2::LEFT_CENTER,
                        Anchor::MiddleRight => Align2::RIGHT_CENTER,
                    };
                    painter.text(view.to_screen(paper_to_drawing(&ed.rendered, *at)), align, shown, FontId::proportional(size), rgb(*color));
                }
            }
        }
        // The selection, highlighted.
        for &i in &ed.selection {
            if let Some(e) = ed.drawing.entities.get(i) {
                for line in outline(e) {
                    let screen: Vec<Pos2> = line.iter().map(|&p| view.to_screen(p)).collect();
                    if let [only] = screen.as_slice() {
                        painter.circle_stroke(*only, 5.0, Stroke::new(2.0, t.accent));
                    } else {
                        painter.add(Shape::dashed_line(&screen, Stroke::new(2.5, t.accent), 6.0, 3.0));
                    }
                }
            }
        }
        // The pointer: snapped, then held level or plumb by Ortho.
        let mut snapped: Option<(Pt, SnapKind)> = None;
        let mut cursor: Option<Pt> = None;
        if let Some(h) = hovering {
            let raw = view.to_drawing(h);
            if ed.snap {
                snapped = ed
                    .drawing
                    .snap_points()
                    .into_iter()
                    .map(|(p, kind)| (p, kind, view.to_screen(p).distance(h)))
                    .filter(|(_, _, d)| *d <= 10.0)
                    .min_by(|a, b| a.2.total_cmp(&b.2))
                    .map(|(p, kind, _)| (p, kind));
            }
            let mut p = snapped.map_or(raw, |(p, _)| p);
            if snapped.is_none() && ed.grid && ed.snap {
                let step = grid_step(ed.zoom);
                let g = Pt::new((raw.x / step).round() * step, (raw.y / step).round() * step);
                if view.to_screen(g).distance(h) <= 6.0 {
                    p = g;
                }
            }
            if ed.ortho
                && let Some(&last) = ed.points.last()
            {
                let d = p.minus(last);
                p = if d.x.abs() >= d.y.abs() { Pt::new(p.x, last.y) } else { Pt::new(last.x, p.y) };
            }
            cursor = Some(p);
            // Crosshair and snap marker.
            let s = view.to_screen(p);
            let hair = Stroke::new(1.0, Color32::from_gray(90));
            painter.line_segment([pos2(s.x - 14.0, s.y), pos2(s.x + 14.0, s.y)], hair);
            painter.line_segment([pos2(s.x, s.y - 14.0), pos2(s.x, s.y + 14.0)], hair);
            if let Some((_, kind)) = snapped {
                let mark = Stroke::new(1.8, Color32::from_rgb(0xE0, 0x7A, 0x00));
                match kind {
                    SnapKind::End => {
                        painter.rect_stroke(Rect::from_center_size(s, vec2(11.0, 11.0)), 0.0, mark, egui::StrokeKind::Middle);
                    }
                    SnapKind::Mid => {
                        painter.add(Shape::closed_line(vec![pos2(s.x, s.y - 7.0), pos2(s.x + 7.0, s.y + 5.0), pos2(s.x - 7.0, s.y + 5.0)], mark));
                    }
                    SnapKind::Center => {
                        painter.circle_stroke(s, 6.0, mark);
                    }
                }
            }
            // Rubber band from the last point.
            if let Some(&last) = ed.points.last() {
                let band = Stroke::new(1.2, t.accent);
                let a = view.to_screen(last);
                match ed.tool {
                    Tool::Rect => {
                        let r = Rect::from_two_pos(a, s);
                        painter.rect_stroke(r, 0.0, band, egui::StrokeKind::Middle);
                    }
                    Tool::Circle => {
                        painter.circle_stroke(a, a.distance(s), band);
                    }
                    _ => {
                        painter.line_segment([a, s], band);
                    }
                }
            }
            if let Some(from) = ed.window_from {
                let r = Rect::from_two_pos(from, h);
                let crossing = h.x < from.x;
                let fill = if crossing { Color32::from_rgba_unmultiplied(40, 160, 80, 30) } else { Color32::from_rgba_unmultiplied(40, 90, 200, 30) };
                painter.rect_filled(r, 0.0, fill);
                painter.rect_stroke(r, 0.0, Stroke::new(1.0, t.accent), egui::StrokeKind::Middle);
            }
        }
        if hovering.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::None);
        }
        // Clicks feed the command; a drag in Select makes a window.
        if ed.tool == Tool::Select && !mods.shift {
            if resp.drag_started_by(egui::PointerButton::Primary) {
                ed.window_from = resp.interact_pointer_pos();
            }
            if resp.drag_stopped()
                && let (Some(from), Some(to)) = (ed.window_from.take(), hover)
            {
                let ids = ed.drawing.in_box(view.to_drawing(from), view.to_drawing(to), to.x < from.x);
                if mods.command || mods.ctrl {
                    ed.selection.extend(ids);
                } else {
                    ed.selection = ids;
                }
            }
        }
        if resp.clicked_by(egui::PointerButton::Primary)
            && let Some(p) = cursor
        {
            if ed.tool == Tool::Select {
                let tol = 6.0 / ed.zoom;
                match ed.drawing.hit(view.to_drawing(hover.unwrap_or_default()), tol) {
                    Some(i) if mods.shift => {
                        if let Some(at) = ed.selection.iter().position(|&s| s == i) {
                            ed.selection.remove(at);
                        } else {
                            ed.selection.push(i);
                        }
                    }
                    Some(i) => ed.selection = vec![i],
                    None if !mods.shift => ed.selection.clear(),
                    None => {}
                }
            } else {
                finished = feed(ed, p, None);
            }
        }
        // Keys: Esc cancels, Enter ends, F8 Ortho, Delete removes the selection.
        let (esc, enter, f8, del) = ui.input(|i| {
            (i.key_pressed(egui::Key::Escape), i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::F8), i.key_pressed(egui::Key::Delete))
        });
        if f8 {
            ed.ortho = !ed.ortho;
        }
        if esc {
            if !ed.points.is_empty() {
                // A polyline in progress keeps what was drawn.
                finished = finish(ed);
                ed.points.clear();
            } else if ed.tool != Tool::Select {
                ed.tool = Tool::Select;
            } else {
                ed.selection.clear();
            }
            ed.command.clear();
        }
        if del && ed.command.is_empty() {
            delete_now = true;
        }

        // Status and command line.
        let prompt = if ed.message.is_empty() { tl!(ed.tool.prompt(ed.points.len())).to_string() } else { ed.message.clone() };
        let coords = cursor.map(|p| format!("{:.3}, {:.3}", p.x, p.y)).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(prompt).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let flag = |on: bool, name: &str| if on { format!("{} ✓", tl!(name)) } else { tl!(name).to_string() };
                ui.label(
                    egui::RichText::new(format!(
                        "{coords}   {}   {}   {}   {} {}",
                        flag(ed.snap, "Snap"),
                        flag(ed.grid, "Grid"),
                        flag(ed.ortho, "Ortho (F8)"),
                        tl!("Unit:"),
                        ed.drawing.unit.symbol()
                    ))
                    .weak(),
                );
            });
        });
        let field = ui.add(
            egui::TextEdit::singleline(&mut ed.command)
                .hint_text(tl!("Type a point (x,y  @dx,dy  @d<a), a distance, a command (l, pl, rec, c, a, t, m, co) or press Enter"))
                .desired_width(f32::INFINITY),
        );
        if !field.has_focus() && hovering.is_some() {
            field.request_focus();
        }
        if enter && finished.is_none() {
            let typed = std::mem::take(&mut ed.command);
            let typed = typed.trim();
            if typed.is_empty() {
                finished = finish(ed);
                ed.points.clear();
            } else if let Some(tool) = Tool::parse(&typed.to_lowercase()).filter(|_| ed.points.is_empty() || ed.tool == Tool::Select) {
                ed.tool = tool;
                ed.points.clear();
            } else if ed.tool == Tool::Text && ed.points.len() == 1 {
                finished = add_text(ed, typed);
            } else if ed.tool == Tool::Polyline && typed.eq_ignore_ascii_case("c") {
                finished = close_polyline(ed);
            } else if ed.tool == Tool::Circle && ed.points.len() == 1 && typed.parse::<f64>().is_ok() {
                let r: f64 = typed.parse().unwrap_or(0.0);
                let c = ed.points[0];
                finished = feed(ed, c.plus(Pt::new(r.abs(), 0.0)), None);
            } else {
                match parse_point(typed, ed.points.last().copied(), cursor) {
                    Ok(p) => finished = feed(ed, p, None),
                    Err(e) => ed.message = e.to_string(),
                }
            }
        }
        if ui.input(|i| i.key_pressed(egui::Key::Q) && i.modifiers.command) {
            close_now = true;
        }
    }
    if let Some(d) = finished {
        commit(app, &d);
    }
    if delete_now {
        delete_selection(app);
    }
    if close_now {
        close(app);
    }
}

/// The drawing with `e` added.
fn with(ed: &CadEditor, e: Entity) -> Option<Drawing> {
    let mut d = ed.drawing.clone();
    d.add(e).ok()?;
    Some(d)
}

fn pt(p: Pt) -> P {
    P(Num::new(p.x), Num::new(p.y))
}

/// Takes point `p` for the current tool; returns the new drawing when an operation completes.
fn feed(ed: &mut CadEditor, p: Pt, _text: Option<&str>) -> Option<Drawing> {
    ed.message.clear();
    ed.points.push(p);
    let n = ed.points.len();
    let pts = ed.points.clone();
    let done = |ed: &mut CadEditor| ed.points.clear();
    match ed.tool {
        Tool::Select => {
            done(ed);
            None
        }
        Tool::Line if n >= 2 => {
            let (a, b) = (pts[n - 2], pts[n - 1]);
            // The line chains on from its end.
            ed.points = vec![b];
            with(ed, Entity::new(Kind::Line { a: pt(a), b: pt(b) }))
        }
        Tool::Rect if n >= 2 => {
            let (a, b) = (pts[0], pts[1]);
            done(ed);
            with(
                ed,
                Entity::new(Kind::Polyline {
                    pts: vec![pt(a), pt(Pt::new(b.x, a.y)), pt(b), pt(Pt::new(a.x, b.y))],
                    bulges: Vec::new(),
                    closed: true,
                }),
            )
        }
        Tool::Circle if n >= 2 => {
            let r = pts[1].minus(pts[0]).len();
            done(ed);
            with(ed, Entity::new(Kind::Circle { c: pt(pts[0]), r: Num::new(r) }))
        }
        Tool::Arc if n >= 3 => {
            let c = pts[0];
            let r = pts[1].minus(c).len();
            let angle = |q: Pt| (q.y - c.y).atan2(q.x - c.x).to_degrees();
            done(ed);
            with(ed, Entity::new(Kind::Arc { c: pt(c), r: Num::new(r), start: Num::new(angle(pts[1])), end: Num::new(angle(pts[2])) }))
        }
        Tool::Point => {
            done(ed);
            with(ed, Entity::new(Kind::Point { p: pt(p) }))
        }
        Tool::Dim(orient) if n >= 3 => {
            let (a, b, at) = (pts[0], pts[1], pts[2]);
            let dir = match orient {
                Orient::Aligned => b.minus(a).unit(),
                Orient::Horizontal => Pt::new(if b.x >= a.x { 1.0 } else { -1.0 }, 0.0),
                Orient::Vertical => Pt::new(0.0, if b.y >= a.y { 1.0 } else { -1.0 }),
            };
            let normal = dir.left();
            let level = |q: Pt| q.x * normal.x + q.y * normal.y;
            let (na, nb, nl) = (level(a), level(b), level(at));
            // Past the farther point along the normal, in paper mm.
            let offset_units = if nl >= na.max(nb) { nl - na.max(nb) } else { nl - na.min(nb) };
            let k = if ed.rendered.scale > 0.0 { ed.rendered.scale } else { 1.0 };
            done(ed);
            with(ed, Entity::new(Kind::Dim { a: pt(a), b: pt(b), offset: offset_units * k, orient, text: None }))
        }
        Tool::Move | Tool::Copy if ed.selection.is_empty() => {
            done(ed);
            ed.message = "Select the objects first (Select tool)".into();
            None
        }
        Tool::Move | Tool::Copy if n >= 2 => {
            let d = pts[1].minus(pts[0]);
            let mut drawing = ed.drawing.clone();
            if ed.tool == Tool::Move {
                drawing.translate(&ed.selection, d);
            } else {
                let copies = drawing.copy(&ed.selection, d);
                ed.selection = copies;
            }
            done(ed);
            Some(drawing)
        }
        _ => None,
    }
}

/// Ends the current command: a polyline in progress is kept.
fn finish(ed: &mut CadEditor) -> Option<Drawing> {
    if ed.tool == Tool::Polyline && ed.points.len() >= 2 {
        let pts = ed.points.iter().map(|&p| pt(p)).collect();
        return with(ed, Entity::new(Kind::Polyline { pts, bulges: Vec::new(), closed: false }));
    }
    None
}

fn close_polyline(ed: &mut CadEditor) -> Option<Drawing> {
    if ed.points.len() < 3 {
        ed.message = "A closed polyline needs three points".into();
        return None;
    }
    let pts = ed.points.iter().map(|&p| pt(p)).collect();
    ed.points.clear();
    with(ed, Entity::new(Kind::Polyline { pts, bulges: Vec::new(), closed: true }))
}

fn add_text(ed: &mut CadEditor, text: &str) -> Option<Drawing> {
    let p = *ed.points.first()?;
    ed.points.clear();
    with(ed, Entity::new(Kind::Text { p: pt(p), value: text.to_string(), height: 3.5, angle: 0.0 }))
}

/// The CAD Drawing contextual tab.
pub fn ribbon(app: &mut WordApp, ui: &mut Ui) {
    use crate::widgets::{big, group, small};
    let Some((tool, snap, ortho, grid, has_sel)) = app.cad.as_ref().map(|e| (e.tool, e.snap, e.ortho, e.grid, !e.selection.is_empty())) else {
        return;
    };
    let stack = |ui: &mut Ui, add: &mut dyn FnMut(&mut Ui)| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing = vec2(1.0, 0.0);
            add(ui);
        });
    };
    let t = |name: &str| json!({"tool": name});
    group(ui, "Draw", None, app, |ui, app| {
        big(ui, app, "cadLine", "Line", "ui.cadTool", t("line"), tool == Tool::Line);
        big(ui, app, "cadCircle", "Circle", "ui.cadTool", t("circle"), tool == Tool::Circle);
        stack(ui, &mut |ui| {
            small(
                ui,
                app,
                "cadPolyline",
                Some("Polyline"),
                "Polyline: points, Enter to end, c to close",
                "ui.cadTool",
                t("polyline"),
                tool == Tool::Polyline,
            );
            small(ui, app, "cadRect", Some("Rectangle"), "Rectangle by two corners", "ui.cadTool", t("rect"), tool == Tool::Rect);
            small(ui, app, "cadArc", Some("Arc"), "Arc: centre, start, end (counter-clockwise)", "ui.cadTool", t("arc"), tool == Tool::Arc);
        });
        stack(ui, &mut |ui| {
            small(ui, app, "cadPoint", Some("Point"), "Point", "ui.cadTool", t("point"), tool == Tool::Point);
            small(ui, app, "cadText", Some("Text"), "Text: a position, then type it", "ui.cadTool", t("text"), tool == Tool::Text);
        });
    });
    group(ui, "Dimensions", None, app, |ui, app| {
        big(ui, app, "cadDim", "Aligned", "ui.cadTool", t("dimAligned"), tool == Tool::Dim(Orient::Aligned));
        stack(ui, &mut |ui| {
            small(
                ui,
                app,
                "cadDimH",
                Some("Horizontal"),
                "Horizontal dimension",
                "ui.cadTool",
                t("dimHorizontal"),
                tool == Tool::Dim(Orient::Horizontal),
            );
            small(ui, app, "cadDimV", Some("Vertical"), "Vertical dimension", "ui.cadTool", t("dimVertical"), tool == Tool::Dim(Orient::Vertical));
        });
    });
    group(ui, "Modify", None, app, |ui, app| {
        big(ui, app, "cadSelect", "Select", "ui.cadTool", t("select"), tool == Tool::Select);
        stack(ui, &mut |ui| {
            small(ui, app, "cadMove", Some("Move"), "Move the selection: base point, second point", "ui.cadTool", t("move"), tool == Tool::Move);
            small(ui, app, "cadCopy", Some("Copy"), "Copy the selection: base point, second point", "ui.cadTool", t("copy"), tool == Tool::Copy);
            let _ = has_sel;
            small(ui, app, "cadDelete", Some("Delete"), "Delete the selection (Del)", "ui.cadDelete", json!({}), false);
        });
        stack(ui, &mut |ui| {
            small(ui, app, "undo", Some("Undo"), "Undo", "edit.undo", json!({}), false);
            small(ui, app, "redo", Some("Redo"), "Redo", "edit.redo", json!({}), false);
        });
    });
    group(ui, "Precision", None, app, |ui, app| {
        stack(ui, &mut |ui| {
            small(ui, app, "cadSnap", Some("Snap"), "Snap to ends, midpoints, centres and the grid", "ui.cadToggle", json!({"what": "snap"}), snap);
            small(ui, app, "cadOrtho", Some("Ortho"), "Hold lines level or plumb (F8)", "ui.cadToggle", json!({"what": "ortho"}), ortho);
            small(ui, app, "cadGrid", Some("Grid"), "Show the grid", "ui.cadToggle", json!({"what": "grid"}), grid);
        });
        big(ui, app, "cadFit", "Zoom\nFit", "ui.cadFit", json!({}), false);
    });
    group(ui, "Close", None, app, |ui, app| {
        big(ui, app, "cadClose", "Close\nDrawing", "ui.cadClose", json!({}), false);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_with_drawing() -> WordApp {
        let mut app = WordApp::new(strix_engine::Session::new(strix_doc::Document::new()), crate::Services::default());
        app.session.run("document.setText", &json!({"text": ""})).unwrap();
        app.session.run("insert.cad", &json!({})).unwrap();
        app
    }

    fn entities(app: &WordApp) -> usize {
        app.cad.as_ref().map_or(0, |e| e.drawing.entities.len())
    }

    #[test]
    fn drawing_in_the_editor_is_saved_and_undone_in_the_document() {
        let mut app = app_with_drawing();
        app.run("ui.cadEdit", json!({})).unwrap();
        assert_eq!(entities(&app), 3, "the starter drawing");
        app.run("ui.cadTool", json!({"tool": "line"})).unwrap();
        // Two points make a line; the line chains on from its end.
        let ed = app.cad.as_mut().unwrap();
        assert!(feed(ed, Pt::new(0.0, 0.0), None).is_none());
        let d = feed(ed, Pt::new(50.0, 20.0), None).expect("a line");
        assert_eq!(ed.points, [Pt::new(50.0, 20.0)]);
        commit(&mut app, &d);
        assert_eq!(entities(&app), 4);
        let saved = strix_engine::cmd::cad::edited(&app.session).map(|(_, d, _, _)| d.entities.len());
        assert_eq!(saved, Some(4), "stored in the document");
        // Undo in the document; the editor reads the drawing back.
        app.run("edit.undo", json!({})).unwrap();
        reload(&mut app);
        assert_eq!(entities(&app), 3);
        // Select, move by typed points, delete.
        app.run("ui.cadTool", json!({"tool": "move"})).unwrap();
        let ed = app.cad.as_mut().unwrap();
        ed.selection = vec![0];
        assert!(feed(ed, Pt::new(0.0, 0.0), None).is_none());
        let moved = feed(ed, parse_point("@10,0", Some(Pt::new(0.0, 0.0)), None).unwrap(), None).expect("moved");
        commit(&mut app, &moved);
        app.run("ui.cadDelete", json!({})).unwrap();
        assert_eq!(entities(&app), 2);
        app.run("ui.cadClose", json!({})).unwrap();
        assert!(app.cad.is_none());
    }

    #[test]
    fn rectangles_circles_and_dimensions() {
        let mut app = app_with_drawing();
        app.run("ui.cadEdit", json!({})).unwrap();
        let ed = app.cad.as_mut().unwrap();
        ed.tool = Tool::Rect;
        feed(ed, Pt::new(0.0, 0.0), None);
        let d = feed(ed, Pt::new(30.0, 10.0), None).unwrap();
        assert!(matches!(&d.entities.last().unwrap().kind, Kind::Polyline { pts, closed: true, .. } if pts.len() == 4));
        ed.tool = Tool::Circle;
        feed(ed, Pt::new(5.0, 5.0), None);
        let d = feed(ed, Pt::new(8.0, 9.0), None).unwrap();
        assert!(matches!(&d.entities.last().unwrap().kind, Kind::Circle { r, .. } if (r.value - 5.0).abs() < 1e-9));
        ed.tool = Tool::Dim(Orient::Horizontal);
        feed(ed, Pt::new(0.0, 0.0), None);
        feed(ed, Pt::new(30.0, 0.0), None);
        let d = feed(ed, Pt::new(15.0, 12.0), None).unwrap();
        assert!(matches!(&d.entities.last().unwrap().kind, Kind::Dim { offset, .. } if *offset > 0.0));
        // Move without a selection only asks for one.
        ed.tool = Tool::Move;
        ed.selection.clear();
        assert!(feed(ed, Pt::new(0.0, 0.0), None).is_none());
        assert!(!ed.message.is_empty());
    }

    #[test]
    fn tools_parse_their_names_and_aliases() {
        assert_eq!(Tool::parse("l"), Some(Tool::Line));
        assert_eq!(Tool::parse("rettangolo"), Some(Tool::Rect));
        assert_eq!(Tool::parse("dimVertical"), Some(Tool::Dim(Orient::Vertical)));
        assert_eq!(Tool::parse("nope"), None);
    }

    #[test]
    fn grid_steps_are_round_numbers() {
        for zoom in [0.001, 0.37, 1.0, 18.0, 250.0, 1e6] {
            let s = grid_step(zoom);
            assert!(s * zoom >= 18.0 - 1e-9, "{zoom} {s}");
            let m = s / 10f64.powf(s.log10().floor());
            assert!([1.0, 2.0, 5.0, 10.0].iter().any(|x| (m - x).abs() < 1e-9), "{s}");
        }
    }
}
