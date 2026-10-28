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
use strix_cad::{
    Anchor, Dash, Drawing, Entity, Focus, Grip, Kind, Num, Order, Orient, P, Prim, Pt, Rendered, SelectOption, SnapKind, Snaps, StrokeEdit, kernel,
    parse_point,
};
use strix_doc::InlineObject;

use crate::WordApp;

/// How a circle is given (AutoCAD's Circle options).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircleMode {
    /// Centre, then the radius.
    Radius,
    /// Centre, then the diameter.
    Diameter,
    /// The two ends of a diameter.
    TwoPoints,
    /// Three points on it.
    ThreePoints,
    /// Touching two objects, with a radius.
    TanTanRadius,
    /// Touching three objects.
    TanTanTan,
}

/// The sheet the drawing is drawn on: paper white in both themes, like the page. Line samples
/// in the panes are drawn on it too, so a dark line reads in the dark theme.
pub(crate) const SHEET: Color32 = Color32::WHITE;

/// What a click does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Select,
    Line,
    /// A line touching two circles or arcs.
    LineTan,
    Polyline,
    Rect,
    Circle(CircleMode),
    Arc,
    Point,
    Text,
    Dim(Orient),
    /// An angular dimension: the vertex, a point on each ray, where the arc goes.
    DimAngle,
    /// Continued dimensions: each click measures on from the last dimension's second point, on
    /// its line (the selected linear dimension, else the newest).
    DimContinue,
    /// Baseline dimensions: each click measures from the base's first point, a row further out.
    DimBaseline,
    /// A level (elevation) mark: a click on the level.
    Level,
    /// A hatched strip: the line or polyline it follows, then a point on its side.
    Strip,
    /// A hatch: a click inside the region it fills.
    Hatch,
    Move,
    Copy,
    /// Scale the selection: a base point, then the factor (typed, or a point that far away).
    Scale,
    /// Rotate the selection: a base point, then the angle (typed, or a point in that direction).
    Rotate,
    /// Mirror the selection (a mirrored copy): two points of the axis.
    Mirror,
    /// Offset: the distance (typed; the last one kept), the object, then the side.
    Offset,
    /// Trim: each pick takes away the piece between the nearest crossings (quick mode).
    Trim,
    /// Extend: each pick runs the nearer end to the next object.
    Extend,
    /// Fillet: the radius (typed; the last one kept), then the two lines.
    Fillet,
    /// Chamfer: the distance (typed; the last one kept), then the two lines.
    Chamfer,
    /// Explode the selected polylines into lines and arcs.
    Explode,
    /// Join the selected lines, arcs and polylines that meet end to end.
    Join,
    /// Match Properties: the source object, then the objects that take its look.
    MatchProp,
    /// Stretch: a crossing window, then a base point and a second point.
    Stretch,
    /// A rectangular array of the selection: columns × rows (typed), a base point, the spacing.
    ArrayRect,
    /// A polar array of the selection: how many items (typed), then the centre.
    ArrayPolar,
}

impl Tool {
    /// The commands that work on the selection (and ask for objects when there is none).
    fn takes_selection(self) -> bool {
        matches!(
            self,
            Tool::Move | Tool::Copy | Tool::Scale | Tool::Rotate | Tool::Mirror | Tool::Explode | Tool::Join | Tool::ArrayRect | Tool::ArrayPolar
        )
    }

    /// The commands that act on the selection at once, without points.
    fn immediate(self) -> bool {
        matches!(self, Tool::Explode | Tool::Join)
    }

    /// The next click picks an object rather than a point: no snaps move it.
    fn picks_object(self) -> bool {
        matches!(self, Tool::Trim | Tool::Extend | Tool::Fillet | Tool::Chamfer | Tool::MatchProp)
    }

    pub fn parse(name: &str) -> Option<Tool> {
        Some(match name {
            "select" => Tool::Select,
            "line" | "l" | "linea" => Tool::Line,
            "lineTan" | "linetan" | "lt" => Tool::LineTan,
            "polyline" | "pl" | "polilinea" => Tool::Polyline,
            "rect" | "rec" | "rettangolo" => Tool::Rect,
            "circle" | "c" | "cerchio" => Tool::Circle(CircleMode::Radius),
            "circleDiameter" | "circlediameter" | "cd" => Tool::Circle(CircleMode::Diameter),
            "circle2p" | "2p" => Tool::Circle(CircleMode::TwoPoints),
            "circle3p" | "3p" => Tool::Circle(CircleMode::ThreePoints),
            "circleTtr" | "ttr" => Tool::Circle(CircleMode::TanTanRadius),
            "circleTtt" | "ttt" => Tool::Circle(CircleMode::TanTanTan),
            "arc" | "a" | "arco" => Tool::Arc,
            "point" | "po" | "punto" => Tool::Point,
            "text" | "t" | "testo" => Tool::Text,
            "dim" | "dimAligned" | "quota" => Tool::Dim(Orient::Aligned),
            "dimHorizontal" => Tool::Dim(Orient::Horizontal),
            "dimVertical" => Tool::Dim(Orient::Vertical),
            "dimAngular" | "dan" | "quotaAngolare" => Tool::DimAngle,
            "dimContinue" | "dco" | "dimcontinue" | "quotaContinua" => Tool::DimContinue,
            "dimBaseline" | "dba" | "dimbaseline" | "quotaBase" => Tool::DimBaseline,
            "level" | "liv" | "quotaLivello" => Tool::Level,
            "strip" | "fascia" => Tool::Strip,
            "hatch" | "h" | "bh" | "retino" => Tool::Hatch,
            "move" | "m" | "sposta" => Tool::Move,
            "copy" | "co" | "copia" => Tool::Copy,
            "scale" | "sc" | "scala" => Tool::Scale,
            "rotate" | "ro" | "ruota" => Tool::Rotate,
            "mirror" | "mi" | "specchia" => Tool::Mirror,
            "offset" | "o" | "of" => Tool::Offset,
            "trim" | "tr" | "taglia" => Tool::Trim,
            "extend" | "ex" | "estendi" => Tool::Extend,
            "fillet" | "f" | "raccorda" => Tool::Fillet,
            "chamfer" | "cha" | "cima" => Tool::Chamfer,
            "explode" | "x" | "esplodi" => Tool::Explode,
            "join" | "j" | "unisci" => Tool::Join,
            "matchProp" | "matchprop" | "ma" => Tool::MatchProp,
            "stretch" | "s" | "stira" => Tool::Stretch,
            "arrayRect" | "arrayrect" | "ar" | "serie" => Tool::ArrayRect,
            "arrayPolar" | "arraypolar" | "arp" | "seriePolare" => Tool::ArrayPolar,
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
            (Tool::LineTan, 0) => "Tangent line: pick the first circle or arc",
            (Tool::LineTan, _) => "Tangent line: pick the second circle or arc",
            (Tool::Circle(CircleMode::Radius), 0) => "Circle: centre (or 2p, 3p, ttr, ttt)",
            (Tool::Circle(CircleMode::Radius), _) => "Circle: radius (a point or a number; d for the diameter)",
            (Tool::Circle(CircleMode::Diameter), 0) => "Circle: centre",
            (Tool::Circle(CircleMode::Diameter), _) => "Circle: diameter (a point or a number)",
            (Tool::Circle(CircleMode::TwoPoints), 0) => "Circle 2 points: first end of the diameter",
            (Tool::Circle(CircleMode::TwoPoints), _) => "Circle 2 points: second end of the diameter",
            (Tool::Circle(CircleMode::ThreePoints), 0) => "Circle 3 points: first point",
            (Tool::Circle(CircleMode::ThreePoints), 1) => "Circle 3 points: second point",
            (Tool::Circle(CircleMode::ThreePoints), _) => "Circle 3 points: third point",
            (Tool::Circle(CircleMode::TanTanRadius), 0) => "Tan, tan, radius: pick the first object",
            (Tool::Circle(CircleMode::TanTanRadius), 1) => "Tan, tan, radius: pick the second object",
            (Tool::Circle(CircleMode::TanTanRadius), _) => "Tan, tan, radius: radius (a number, or a point from the second pick)",
            (Tool::Circle(CircleMode::TanTanTan), 0) => "Tan, tan, tan: pick the first object",
            (Tool::Circle(CircleMode::TanTanTan), 1) => "Tan, tan, tan: pick the second object",
            (Tool::Circle(CircleMode::TanTanTan), _) => "Tan, tan, tan: pick the third object",
            (Tool::Arc, 0) => "Arc: centre",
            (Tool::Arc, 1) => "Arc: start point",
            (Tool::Arc, _) => "Arc: end point (counter-clockwise)",
            (Tool::Point, _) => "Point: location",
            (Tool::Text, 0) => "Text: position",
            (Tool::Text, _) => "Text: type the text and press Enter",
            (Tool::Dim(_), 0) => "Dimension: first point",
            (Tool::Dim(_), 1) => "Dimension: second point",
            (Tool::Dim(_), _) => "Dimension: where the dimension line goes",
            (Tool::DimAngle, 0) => "Angular dimension: the vertex",
            (Tool::DimAngle, 1) => "Angular dimension: a point on the first side",
            (Tool::DimAngle, 2) => "Angular dimension: a point on the second side",
            (Tool::DimAngle, _) => "Angular dimension: where the arc goes",
            (Tool::DimContinue, _) => "Continue: next point (Enter or Esc to end)",
            (Tool::DimBaseline, _) => "Baseline: next point (Enter or Esc to end)",
            (Tool::Level, _) => "Level mark: the point on the level (Enter or Esc to end)",
            (Tool::Strip, 0) => "Hatched strip: pick the line or polyline it follows",
            (Tool::Strip, _) => "Hatched strip: click on the side it goes",
            (Tool::Hatch, _) => "Hatch: click inside the region to fill (islands are left out)",
            (Tool::Move | Tool::Copy, 0) => "Base point (select the objects first)",
            (Tool::Move | Tool::Copy, _) => "Second point",
            (Tool::Scale, 0) => "Scale: base point",
            (Tool::Scale, _) => "Scale: factor (type it, or a point that many units from the base)",
            (Tool::Rotate, 0) => "Rotate: base point",
            (Tool::Rotate, _) => "Rotate: angle in degrees (type it, or a point in that direction)",
            (Tool::Mirror, 0) => "Mirror: first point of the axis",
            (Tool::Mirror, _) => "Mirror: second point of the axis (a mirrored copy is made)",
            (Tool::Offset, 0) => "Offset: pick the object (type a number to change the distance)",
            (Tool::Offset, _) => "Offset: click on the side to offset to",
            (Tool::Trim, _) => "Trim: pick the piece to take away (the other objects cut it)",
            (Tool::Extend, _) => "Extend: pick near the end to extend (to the next object)",
            (Tool::Fillet, 0) => "Fillet: pick the first line (type a number to change the radius)",
            (Tool::Fillet, _) => "Fillet: pick the second line",
            (Tool::Chamfer, 0) => "Chamfer: pick the first line (type a number to change the distance)",
            (Tool::Chamfer, _) => "Chamfer: pick the second line",
            (Tool::Explode | Tool::Join, _) => "Select the objects, then Enter",
            (Tool::MatchProp, 0) => "Match Properties: pick the source object",
            (Tool::MatchProp, _) => "Match Properties: pick the objects that take its look (Enter to end)",
            (Tool::Stretch, 0) => "Stretch: first corner of the crossing window",
            (Tool::Stretch, 1) => "Stretch: other corner (the points inside it move)",
            (Tool::Stretch, 2) => "Stretch: base point",
            (Tool::Stretch, _) => "Stretch: second point",
            (Tool::ArrayRect, 0) => "Rectangular array: base point (type columns x rows, e.g. 4x3, to change the counts)",
            (Tool::ArrayRect, _) => "Rectangular array: a point one column and one row away (the spacing)",
            (Tool::ArrayPolar, _) => "Polar array: centre (type a number to change how many items)",
        }
    }
}

/// The editor's state while a drawing is open.
pub struct CadEditor {
    /// The drawing's place in the document.
    pos: strix_doc::Pos,
    /// The source it was read from: a change (undo, recalculation) is read back.
    source: Option<Arc<str>>,
    pub(crate) drawing: Drawing,
    rendered: Rendered,
    /// The calculation parameters defined where the drawing sits (for annotations).
    pub names: Vec<String>,
    pub tool: Tool,
    points: Vec<Pt>,
    pub selection: Vec<usize>,
    pub snap: bool,
    /// Which object snaps are on.
    pub snaps: Snaps,
    pub ortho: bool,
    pub grid: bool,
    /// Show the picture's frame, the area outside it shaded.
    pub frame: bool,
    /// The grip taken (AutoCAD's hot grip): entity, grip, where it was. The next point (a click,
    /// a typed point, the end of a drag) takes it there.
    pub grip: Option<(usize, Grip, Pt)>,
    /// The hot grip is being dragged (released, it is placed).
    grip_drag: bool,
    command: String,
    pub(crate) message: String,
    /// The drawing point at the centre of the view, and screen points per drawing unit.
    center: Pt,
    zoom: f64,
    fitted: bool,
    /// The tab shown before the editor opened.
    pub prev_tab: Option<String>,
    /// The selection of the last command (`P`), and the steps of this one (`U`).
    pub(crate) previous: Vec<usize>,
    sel_steps: Vec<Vec<usize>>,
    /// Clicks remove from the selection (`R`; `A` adds again). Shift does the same per click.
    pub remove_mode: bool,
    /// A two-click window: its first corner.
    win: Option<Pt>,
    /// A fence or a polygon window in progress, with its vertices.
    poly: Option<(SelectOption, Vec<Pt>)>,
    /// A lasso being dragged (screen points).
    lasso: Vec<Pos2>,
    /// Overlapping objects under the pointer: which one a click takes (Shift+Space steps).
    cycle: usize,
    cycle_at: Option<Pos2>,
    /// The last click that picked an object: where, which, and whether it added it.
    last_click: Option<(Pos2, usize, bool)>,
    /// What the last selection step found ("3 found · 5 selected").
    found: String,
    /// Move or Copy started without a selection: clicks select, Enter confirms.
    pub picking: bool,
    /// An object snap for the next point only (`Some(None)`: no snap), from Shift+right-click.
    pub snap_once: Option<Option<SnapKind>>,
    /// The last drawing command (Enter at the idle prompt repeats it).
    pub last_tool: Option<Tool>,
    /// What the Properties pane works on.
    pub focus: Focus,
    /// The Properties pane is shown.
    pub panel: bool,
    /// The Layer Manager is shown (docked under the sheet).
    pub layers_panel: bool,
    /// The Properties pane, the Layer Manager float in their own windows (detached).
    pub panel_float: bool,
    pub layers_float: bool,
    /// The layer new objects go on.
    pub current_layer: String,
    /// The polyline vertex the Properties pane shows.
    pub(crate) vertex: usize,
    /// Where the context menu was opened (drawing point), and whether it is the point menu
    /// (Shift+right-click).
    menu_at: Option<Pt>,
    menu_point: bool,
    /// A field of the Properties pane has the keyboard: the editor's keys stay out of it.
    pub(crate) panel_typing: bool,
    /// The distance Offset uses (drawing units; a number typed changes it).
    pub offset_dist: f64,
    /// Fillet's radius and Chamfer's distance (drawing units; a number typed changes them).
    pub fillet_radius: f64,
    pub chamfer_dist: f64,
    /// The arrays' counts: columns and rows, items round.
    pub array_cols: usize,
    pub array_rows: usize,
    pub array_count: usize,
    /// The linear dimension Continue and Baseline go on from.
    pub(crate) chain_base: Option<usize>,
}

/// The editor's settings that outlive a drawing: kept by the app while it runs, so the next
/// drawing opens as the last one was left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CadSettings {
    pub snap: bool,
    pub snaps: Snaps,
    pub ortho: bool,
    pub grid: bool,
    pub frame: bool,
    pub panel: bool,
    pub layers_panel: bool,
    pub panel_float: bool,
    pub layers_float: bool,
    pub offset_dist: f64,
    pub fillet_radius: f64,
    pub chamfer_dist: f64,
    pub array_cols: usize,
    pub array_rows: usize,
    pub array_count: usize,
}

impl Default for CadSettings {
    fn default() -> Self {
        CadSettings {
            snap: true,
            snaps: Snaps::default(),
            ortho: false,
            grid: true,
            frame: false,
            panel: true,
            layers_panel: false,
            panel_float: false,
            layers_float: false,
            offset_dist: 10.0,
            fillet_radius: 0.0,
            chamfer_dist: 0.0,
            array_cols: 3,
            array_rows: 2,
            array_count: 6,
        }
    }
}

impl CadEditor {
    fn settings(&self) -> CadSettings {
        CadSettings {
            snap: self.snap,
            snaps: self.snaps,
            ortho: self.ortho,
            grid: self.grid,
            frame: self.frame,
            panel: self.panel,
            layers_panel: self.layers_panel,
            panel_float: self.panel_float,
            layers_float: self.layers_float,
            offset_dist: self.offset_dist,
            fillet_radius: self.fillet_radius,
            chamfer_dist: self.chamfer_dist,
            array_cols: self.array_cols,
            array_rows: self.array_rows,
            array_count: self.array_count,
        }
    }
}

/// Opens the selected CAD drawing in the editor.
pub fn open(app: &mut WordApp) -> Result<(), String> {
    let kept = app.cad_settings;
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
        snap: kept.snap,
        snaps: kept.snaps,
        ortho: kept.ortho,
        grid: kept.grid,
        frame: kept.frame,
        grip: None,
        grip_drag: false,
        command: String::new(),
        message: String::new(),
        center: Pt::default(),
        zoom: 1.0,
        fitted: false,
        prev_tab: None,
        previous: Vec::new(),
        sel_steps: Vec::new(),
        remove_mode: false,
        win: None,
        poly: None,
        lasso: Vec::new(),
        cycle: 0,
        cycle_at: None,
        last_click: None,
        found: String::new(),
        picking: false,
        snap_once: None,
        last_tool: None,
        focus: Focus::default(),
        panel: kept.panel,
        layers_panel: kept.layers_panel,
        panel_float: kept.panel_float,
        layers_float: kept.layers_float,
        offset_dist: kept.offset_dist,
        fillet_radius: kept.fillet_radius,
        chamfer_dist: kept.chamfer_dist,
        array_cols: kept.array_cols,
        array_rows: kept.array_rows,
        array_count: kept.array_count,
        current_layer: "0".into(),
        vertex: 0,
        menu_at: None,
        menu_point: false,
        panel_typing: false,
        chain_base: None,
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
        let n = ed.drawing.entities.len();
        ed.selection.retain(|&i| i < n);
        ed.previous.retain(|&i| i < n);
        ed.sel_steps.clear();
        if !ed.drawing.layers.iter().any(|l| l.name == ed.current_layer) {
            ed.current_layer = "0".into();
        }
        if ed.grip.is_some_and(|(i, _, _)| i >= ed.drawing.entities.len()) {
            ed.grip = None;
        }
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
        app.cad_settings = ed.settings();
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
                .ok_or("tool: select, line, lineTan, polyline, rect, circle, circleDiameter, circle2p, circle3p, circleTtr, circleTtt, arc, point, text, dimAligned, dimHorizontal, dimVertical, dimAngular, dimContinue, dimBaseline, level, strip, hatch, move, copy, scale, rotate, mirror, offset, trim, extend, fillet, chamfer, explode, join, matchProp, stretch, arrayRect, arrayPolar")?;
            if let Some(ed) = app.cad.as_mut() {
                ed.set_tool(tool);
                if let Some(d) = immediate(ed) {
                    commit(app, &d);
                }
            }
        }
        "ui.cadToggle" => {
            let what = p.get("what").and_then(serde_json::Value::as_str).unwrap_or_default();
            if let Some(ed) = app.cad.as_mut() {
                let s = &mut ed.snaps;
                match what {
                    "snap" => ed.snap = !ed.snap,
                    "ortho" => ed.ortho = !ed.ortho,
                    "grid" => ed.grid = !ed.grid,
                    "frame" => ed.frame = !ed.frame,
                    "props" => ed.panel = !ed.panel,
                    "layers" => ed.layers_panel = !ed.layers_panel,
                    "props.float" => ed.panel_float = !ed.panel_float,
                    "layers.float" => ed.layers_float = !ed.layers_float,
                    "snap.end" => s.end = !s.end,
                    "snap.mid" => s.mid = !s.mid,
                    "snap.center" => s.center = !s.center,
                    "snap.node" => s.node = !s.node,
                    "snap.quadrant" => s.quadrant = !s.quadrant,
                    "snap.intersection" => s.intersection = !s.intersection,
                    "snap.perpendicular" => s.perpendicular = !s.perpendicular,
                    "snap.tangent" => s.tangent = !s.tangent,
                    "snap.nearest" => s.nearest = !s.nearest,
                    _ => {
                        return Err(
                            "what: snap, ortho, grid, frame, props, layers or snap.<end|mid|center|node|quadrant|intersection|perpendicular|tangent|nearest>"
                                .into(),
                        );
                    }
                }
            }
        }
        "ui.cadDelete" => delete_selection(app),
        "ui.cadSelect" => {
            // {"all": true}, or {"ids": ["e1", …]}: the entities by id (Ctrl+A selects all).
            let Some(ed) = app.cad.as_mut() else { return Ok(json!({})) };
            let flag = |k: &str| p.get(k).and_then(serde_json::Value::as_bool) == Some(true);
            if let Some(word) = p.get("option").and_then(serde_json::Value::as_str) {
                // A typed selection option: all, previous, undo, remove, add (a fence and the
                // polygons need the pointer).
                let opt = SelectOption::parse(word).ok_or("option: all, previous, undo, remove, add")?;
                ed.run_option(opt);
                return Ok(json!({"selected": ed.selection.len(), "found": ed.found}));
            }
            ed.selection = if flag("all") {
                (0..ed.drawing.entities.len()).filter(|&i| ed.drawing.selectable(i)).collect()
            } else if flag("similar") {
                ed.drawing.similar(&ed.selection)
            } else if flag("none") {
                Vec::new()
            } else {
                let ids: Vec<&str> = p
                    .get("ids")
                    .and_then(serde_json::Value::as_array)
                    .map(|a| a.iter().filter_map(serde_json::Value::as_str).collect())
                    .unwrap_or_default();
                ed.drawing.entities.iter().enumerate().filter(|(_, e)| ids.contains(&e.id.as_str())).map(|(i, _)| i).collect()
            };
            ed.grip = None;
            return Ok(json!({"selected": ed.selection.len()}));
        }
        "ui.cadPadding" => {
            // The picture's padding, paper mm: one number, or [top, right, bottom, left].
            let pad = strix_engine::cmd::cad::padding_param(p)?;
            let Some(mut d) = app.cad.as_ref().map(|e| e.drawing.clone()) else { return Ok(json!({})) };
            d.padding = pad;
            commit(app, &d);
        }
        "ui.cadDimStyle" => {
            // The drawing's annotation style: {"text"?, "markSize"?, "extGap"?, "extOver"?,
            // "textGap"? (mm), "mark"? (tick, arrow, dot, none), "aligned"? (texts along their line)}.
            let Some(mut d) = app.cad.as_ref().map(|e| e.drawing.clone()) else { return Ok(json!({})) };
            let st = &mut d.dim_style;
            for (key, slot) in [
                ("text", &mut st.text),
                ("markSize", &mut st.mark_size),
                ("extGap", &mut st.ext_gap),
                ("extOver", &mut st.ext_over),
                ("textGap", &mut st.text_gap),
                ("lineWidth", &mut st.line_width),
                ("lineOver", &mut st.line_over),
            ] {
                if let Some(v) = p.get(key) {
                    *slot = v.as_f64().ok_or(format!("{key}: a size in mm"))?;
                }
            }
            if let Some(m) = p.get("mark") {
                st.mark = m.as_str().and_then(strix_cad::Mark::parse).ok_or("mark: tick, arrow, dot or none")?;
            }
            if let Some(u) = p.get("units") {
                st.units = u.as_str().and_then(strix_cad::DimUnits::parse).ok_or("units: drawing, m, cm, mm or sia")?;
            }
            if let Some(v) = p.get("aligned") {
                st.aligned = v.as_bool().ok_or("aligned: true or false")?;
            }
            if let Some(v) = p.get("scaled") {
                st.scaled = v.as_bool().ok_or("scaled: true or false")?;
            }
            d.dim_style = d.dim_style.sanitized();
            commit(app, &d);
            return Ok(serde_json::to_value(app.cad.as_ref().map(|e| e.drawing.dim_style)).unwrap_or_default());
        }
        "ui.cadScale" => {
            // The drawing's scale and maximum size, as `cad.scale` sets them in the document:
            // {"max"?: true (the page) | false | {"width"?, "height"?} mm, "auto"?: true,
            // "scale"?: n}.
            let Some(mut d) = app.cad.as_ref().map(|e| e.drawing.clone()) else { return Ok(json!({})) };
            let page = || {
                let f = strix_engine::cmd::cad::page_space(&app.session);
                (f.width, f.height)
            };
            match p.get("max") {
                None => {}
                Some(serde_json::Value::Null | serde_json::Value::Bool(false)) => d.set_max_size(None, None),
                Some(serde_json::Value::Bool(true)) => d.set_max_size(Some(page()), None),
                Some(m @ serde_json::Value::Object(_)) => {
                    let num = |k: &str| m.get(k).and_then(serde_json::Value::as_f64).unwrap_or(0.0);
                    d.set_max_size(Some((num("width"), num("height"))), None);
                }
                Some(_) => return Err("max: true, false or {\"width\", \"height\"} in mm".into()),
            }
            if p.get("auto").and_then(serde_json::Value::as_bool) == Some(true) {
                if d.fit.is_none() {
                    d.set_max_size(Some(page()), None);
                }
                d.fit_scale(None)?;
            }
            if let Some(n) = p.get("scale") {
                d.set_fixed_scale(n.as_f64().unwrap_or(f64::NAN), None)?;
            }
            commit(app, &d);
            return Ok(json!({"scale": d.scale, "fit": d.fit}));
        }
        "ui.cadFit" => {
            if let Some(ed) = app.cad.as_mut() {
                ed.fitted = false;
            }
        }
        "ui.cadOrder" => {
            let order = match p.get("order").and_then(serde_json::Value::as_str) {
                Some("front") => Order::Front,
                Some("back") => Order::Back,
                Some("forward") => Order::Forward,
                Some("backward") => Order::Backward,
                _ => return Err("order: front, back, forward or backward".into()),
            };
            reorder(app, order);
        }
        "ui.cadFlip" => {
            // Flips the selected dimensions and level marks.
            let Some((mut d, sel)) = app.cad.as_ref().map(|e| (e.drawing.clone(), e.selection.clone())) else { return Ok(json!({})) };
            let n = d.flip_dims(&sel);
            if n == 0 {
                return Err("select the dimensions or level marks to flip".into());
            }
            commit(app, &d);
            return Ok(json!({"flipped": n}));
        }
        "ui.cadLayer" => {
            // Moves the selection to a layer (made if missing).
            let name = p.get("layer").and_then(serde_json::Value::as_str).ok_or("layer: a layer name")?;
            let Some((mut d, sel)) = app.cad.as_ref().map(|e| (e.drawing.clone(), e.selection.clone())) else { return Ok(json!({})) };
            d.set_layer(&sel, name)?;
            commit(app, &d);
        }
        "ui.cadCurrentLayer" => {
            // The layer new objects go on (made if missing).
            let name = p.get("layer").and_then(serde_json::Value::as_str).ok_or("layer: a layer name")?.trim().to_string();
            let Some(mut d) = app.cad.as_ref().map(|e| e.drawing.clone()) else { return Ok(json!({})) };
            if !d.layers.iter().any(|l| l.name == name) {
                d.ensure_layer(&name)?;
                commit(app, &d);
            }
            if let Some(ed) = app.cad.as_mut() {
                ed.current_layer = name;
            }
        }
        "ui.cadStroke" => {
            // The selection's stroke: "color" "#rrggbb", "width" mm, "dash" solid, dashed,
            // dotted or dashDotted; each may be "byLayer".
            let by = |k: &str| p.get(k).and_then(serde_json::Value::as_str) == Some("byLayer");
            let mut edit = StrokeEdit::default();
            if by("color") {
                edit.color = Some(None);
            } else if let Some(c) = p.get("color").and_then(serde_json::Value::as_str) {
                edit.color = Some(Some(crate::cad_panel::parse_hex(c).ok_or("color: #rrggbb or byLayer")?));
            }
            if by("width") {
                edit.width = Some(None);
            } else if let Some(w) = p.get("width") {
                edit.width = Some(Some(w.as_f64().ok_or("width: mm or byLayer")?));
            }
            if by("dash") {
                edit.dash = Some(None);
            } else if let Some(d) = p.get("dash").and_then(serde_json::Value::as_str) {
                edit.dash = Some(Some(parse_dash(d).ok_or("dash: solid, dashed, dotted, dashDotted or byLayer")?));
            }
            let Some((mut d, sel)) = app.cad.as_ref().map(|e| (e.drawing.clone(), e.selection.clone())) else { return Ok(json!({})) };
            if sel.is_empty() {
                return Err("select objects first".into());
            }
            d.set_stroke(&sel, edit);
            commit(app, &d);
        }
        "ui.cadProps" => {
            // A property of the selected object: {"field", "value", "vertex"?}; with no field,
            // the object's fields.
            let Some(ed) = app.cad.as_ref() else { return Ok(json!({})) };
            let &[i] = ed.selection.as_slice() else { return Err("select one object".into()) };
            let vertex = p.get("vertex").and_then(serde_json::Value::as_u64).map_or(ed.vertex, |v| v.min(1 << 20) as usize);
            let Some(field) = p.get("field").and_then(serde_json::Value::as_str) else {
                let fields: Vec<serde_json::Value> =
                    ed.drawing.fields(i, vertex).into_iter().map(|f| json!({"field": f.key, "label": f.label, "value": f.text})).collect();
                return Ok(json!({"fields": fields}));
            };
            let value = match p.get("value") {
                Some(serde_json::Value::String(v)) => v.clone(),
                Some(serde_json::Value::Number(n)) => n.to_string(),
                Some(serde_json::Value::Bool(b)) => b.to_string(),
                _ => return Err("value: a number, an expression or a text".into()),
            };
            let mut d = ed.drawing.clone();
            d.set_field(i, vertex, field, &value)?;
            commit(app, &d);
        }
        "ui.cadSnapOnce" => {
            // An object snap for the next point only: end, mid, center, node, quadrant,
            // intersection, perpendicular, tangent, nearest, or none.
            let kind = p.get("snap").and_then(serde_json::Value::as_str).unwrap_or_default();
            let k = match kind {
                "none" => None,
                _ => Some(
                    SNAP_KINDS
                        .iter()
                        .find(|(w, _)| *w == kind)
                        .map(|(_, k)| *k)
                        .ok_or("snap: end, mid, center, node, quadrant, intersection, perpendicular, tangent, nearest or none")?,
                ),
            };
            if let Some(ed) = app.cad.as_mut() {
                ed.snap_once = Some(k);
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
    ed.previous = std::mem::take(&mut ed.selection);
    ed.focus = Focus::default();
    commit(app, &d);
}

/// Moves the selection in the draw order; the selection follows it.
fn reorder(app: &mut WordApp, order: Order) {
    let Some(ed) = app.cad.as_mut() else { return };
    if ed.selection.is_empty() {
        return;
    }
    let mut d = ed.drawing.clone();
    ed.selection = d.reorder(&ed.selection, order);
    ed.focus = Focus::default();
    commit(app, &d);
}

/// The object snaps by name (`ui.cadToggle`'s `snap.<name>`, `ui.cadSnapOnce`).
pub(crate) const SNAP_KINDS: [(&str, SnapKind); 9] = [
    ("end", SnapKind::End),
    ("mid", SnapKind::Mid),
    ("center", SnapKind::Center),
    ("node", SnapKind::Node),
    ("quadrant", SnapKind::Quadrant),
    ("intersection", SnapKind::Intersection),
    ("perpendicular", SnapKind::Perpendicular),
    ("tangent", SnapKind::Tangent),
    ("nearest", SnapKind::Nearest),
];

/// Only snap `k` on.
fn only(k: SnapKind) -> Snaps {
    let mut s = Snaps::default();
    for (_, kind) in SNAP_KINDS {
        set_snap(&mut s, kind, kind == k);
    }
    s
}

pub(crate) fn set_snap(s: &mut Snaps, k: SnapKind, on: bool) {
    match k {
        SnapKind::End => s.end = on,
        SnapKind::Mid => s.mid = on,
        SnapKind::Center => s.center = on,
        SnapKind::Node => s.node = on,
        SnapKind::Quadrant => s.quadrant = on,
        SnapKind::Intersection => s.intersection = on,
        SnapKind::Perpendicular => s.perpendicular = on,
        SnapKind::Tangent => s.tangent = on,
        SnapKind::Nearest => s.nearest = on,
    }
}

fn parse_dash(d: &str) -> Option<Dash> {
    Some(match d {
        "solid" => Dash::Solid,
        "dashed" => Dash::Dashed,
        "dotted" => Dash::Dotted,
        "dashDotted" => Dash::DashDotted,
        "dashDoubleDotted" => Dash::DashDoubleDotted,
        _ => return None,
    })
}

impl CadEditor {
    /// A click picks objects: the selection prompt is up (no command, or a command asking for
    /// objects) and no point is being given.
    fn selecting(&self) -> bool {
        (self.tool == Tool::Select || self.picking) && self.points.is_empty() && self.grip.is_none()
    }

    /// Starts `tool`. With objects selected, Move and Copy take them (noun-verb); without, they
    /// ask for objects first.
    pub(crate) fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
        self.points.clear();
        self.message.clear();
        self.win = None;
        self.poly = None;
        self.lasso.clear();
        self.remove_mode = false;
        self.grip = None;
        self.picking = tool.takes_selection() && self.selection.is_empty();
        if matches!(tool, Tool::DimContinue | Tool::DimBaseline) {
            // From the one linear dimension selected, else the newest.
            let picked = match self.selection.as_slice() {
                [i] if matches!(self.drawing.entities.get(*i).map(|e| &e.kind), Some(Kind::Dim { .. })) => Some(*i),
                _ => None,
            };
            self.chain_base = picked.or_else(|| self.drawing.last_linear_dim());
            if self.chain_base.is_none() {
                self.message = "Draw a linear dimension first, or select the one to go on from".into();
            }
        }
        if tool != Tool::Select {
            self.last_tool = Some(tool);
            if !self.selection.is_empty() {
                self.previous = self.selection.clone();
            }
        }
    }

    /// Adds the objects to the selection (or removes them); a click never replaces it.
    fn pick_into(&mut self, ids: Vec<usize>, remove: bool) {
        let mut ok: Vec<usize> = ids.into_iter().filter(|&i| self.drawing.selectable(i)).collect();
        ok.sort_unstable();
        ok.dedup();
        let mut next = self.selection.clone();
        let mut changed = 0;
        for &i in &ok {
            match next.iter().position(|&s| s == i) {
                Some(at) if remove => {
                    next.remove(at);
                    changed += 1;
                }
                None if !remove => {
                    next.push(i);
                    changed += 1;
                }
                _ => {}
            }
        }
        let (n, total) = (ok.len(), next.len());
        self.found = if remove {
            format!("{n} {} · {changed} {} · {total} {}", tl!("found"), tl!("removed"), tl!("selected"))
        } else if changed < n {
            format!("{n} {} · {} {} · {total} {}", tl!("found"), n - changed, tl!("already selected"), tl!("selected"))
        } else {
            format!("{n} {} · {total} {}", tl!("found"), tl!("selected"))
        };
        if changed > 0 {
            self.step_to(next);
        }
    }

    /// A selection step (`U` undoes it).
    fn step_to(&mut self, next: Vec<usize>) {
        if self.sel_steps.len() >= 50 {
            self.sel_steps.remove(0);
        }
        self.sel_steps.push(std::mem::replace(&mut self.selection, next));
        self.grip = None;
    }

    /// Replaces the selection (the Properties pane's Only These): the old one becomes Previous.
    pub(crate) fn replace_selection(&mut self, next: Vec<usize>) {
        self.previous = std::mem::replace(&mut self.selection, next);
        self.focus = Focus::default();
        self.grip = None;
    }

    pub(crate) fn clear_selection(&mut self) {
        if !self.selection.is_empty() {
            self.step_to(Vec::new());
        }
        self.found.clear();
        self.focus = Focus::default();
    }

    /// A typed selection option.
    pub(crate) fn run_option(&mut self, opt: SelectOption) {
        self.message.clear();
        match opt {
            SelectOption::All => self.pick_into((0..self.drawing.entities.len()).collect(), false),
            SelectOption::Fence | SelectOption::WPolygon | SelectOption::CPolygon => {
                self.win = None;
                self.poly = Some((opt, Vec::new()));
            }
            SelectOption::Remove => self.remove_mode = true,
            SelectOption::Add => self.remove_mode = false,
            SelectOption::Previous => {
                let prev = self.previous.clone();
                if prev.is_empty() {
                    self.message = tl!("No previous selection").to_string();
                } else {
                    self.pick_into(prev, self.remove_mode);
                }
            }
            SelectOption::Undo => match self.sel_steps.pop() {
                Some(last) => {
                    self.selection = last;
                    self.found = format!("{} {}", self.selection.len(), tl!("selected"));
                }
                None => self.message = tl!("Nothing to undo in the selection").to_string(),
            },
        }
    }

    /// Closes the fence or polygon in progress and picks what it takes.
    fn finish_poly(&mut self, remove: bool) {
        let Some((mode, pts)) = self.poly.take() else { return };
        let fence = mode == SelectOption::Fence;
        if pts.len() < if fence { 2 } else { 3 } {
            self.message = tl!(if fence { "A fence needs at least two points" } else { "A polygon needs at least three points" }).to_string();
            return;
        }
        let ids = if fence { self.drawing.crossing_fence(&pts) } else { self.drawing.in_polygon(&pts, mode == SelectOption::CPolygon) };
        self.pick_into(ids, remove);
    }

    /// A click on objects at `raw` (screen `at`): adds the one under the pointer (the cycle's
    /// pick among overlapping ones); a second click in the same place swaps the one just added
    /// for the next one under it. False when nothing is there.
    fn click_select(&mut self, raw: Pt, at: Pos2, tol: f64, remove: bool) -> bool {
        let list = self.drawing.hits(raw, tol);
        let Some(&first) = list.first() else { return false };
        if let Some((pos, last, true)) = self.last_click
            && !remove
            && pos.distance(at) <= 4.0
            && list.len() > 1
            && let Some(k) = list.iter().position(|&x| x == last)
        {
            let next_k = (k + 1) % list.len();
            let i = list.get(next_k).copied().unwrap_or(first);
            let mut next = self.selection.clone();
            next.retain(|&s| s != last);
            if !next.contains(&i) {
                next.push(i);
            }
            self.step_to(next);
            self.found = format!("{} {} {} {} · {} {}", tl!("cycle:"), next_k + 1, tl!("of"), list.len(), self.selection.len(), tl!("selected"));
            self.last_click = Some((at, i, true));
            return true;
        }
        let i = list.get(self.cycle % list.len()).copied().unwrap_or(first);
        let added = !remove && !self.selection.contains(&i);
        self.pick_into(vec![i], remove);
        self.last_click = Some((at, i, added));
        true
    }

    /// Esc: the innermost step first (a polygon, a window, Remove mode, a grip, the points, the
    /// command), then the selection. Returns a polyline in progress, kept.
    fn escape(&mut self) -> Option<Drawing> {
        if self.poly.take().is_some() || self.win.take().is_some() {
            return None;
        }
        if self.remove_mode {
            self.remove_mode = false;
        } else if self.grip.is_some() {
            self.grip = None;
            self.grip_drag = false;
        } else if !self.points.is_empty() {
            let kept = finish(self);
            self.points.clear();
            return kept;
        } else if self.tool != Tool::Select {
            self.set_tool(Tool::Select);
        } else {
            self.clear_selection();
        }
        None
    }

    /// The prompt at the command line.
    fn prompt(&self) -> String {
        if let Some((mode, _)) = &self.poly {
            let what = match mode {
                SelectOption::Fence => tl!("Fence"),
                SelectOption::WPolygon => tl!("Window polygon"),
                _ => tl!("Crossing polygon"),
            };
            return format!("{what}: {}", tl!("click the vertices; Enter or right-click closes, Esc cancels"));
        }
        if self.win.is_some() {
            return tl!("Other corner of the window (to the right: whole objects; to the left: touched too)").to_string();
        }
        if self.grip.is_some() {
            return tl!("Grip: the new point (click, or type it; Esc puts it back)").to_string();
        }
        if self.picking || (self.tool == Tool::Select && self.points.is_empty()) {
            let name = match self.tool {
                Tool::Move => tl!("Move"),
                Tool::Copy => tl!("Copy"),
                _ => tl!("Select objects"),
            };
            let how = if self.remove_mode { tl!("(remove) click removes") } else { tl!("click adds, Shift+click removes") };
            let end = if self.picking { format!(" · {}", tl!("Enter confirms")) } else { String::new() };
            return format!(
                "{name}: {how}; {}{end} · {}",
                tl!("click on empty space for a window, drag for a lasso"),
                tl!("All · F fence · WP/CP polygon · R remove · A add · P previous · U undo · Shift+Space cycle")
            );
        }
        tl!(self.tool.prompt(self.points.len())).to_string()
    }
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

/// The marker of a snap kind at `s` (AutoCAD's symbols).
fn snap_marker(painter: &egui::Painter, s: Pos2, kind: SnapKind) {
    let mark = Stroke::new(1.8, Color32::from_rgb(0xD9, 0x77, 0x06));
    let h = 6.0;
    let sq = Rect::from_center_size(s, vec2(2.0 * h, 2.0 * h));
    match kind {
        SnapKind::End => {
            painter.rect_stroke(sq, 0.0, mark, egui::StrokeKind::Middle);
        }
        SnapKind::Mid => {
            painter.add(Shape::closed_line(vec![pos2(s.x, s.y - h), pos2(s.x + h, s.y + h * 0.8), pos2(s.x - h, s.y + h * 0.8)], mark));
        }
        SnapKind::Center => {
            painter.circle_stroke(s, h, mark);
        }
        SnapKind::Node => {
            painter.circle_stroke(s, h, mark);
            painter.line_segment([pos2(s.x - h * 0.7, s.y - h * 0.7), pos2(s.x + h * 0.7, s.y + h * 0.7)], mark);
            painter.line_segment([pos2(s.x - h * 0.7, s.y + h * 0.7), pos2(s.x + h * 0.7, s.y - h * 0.7)], mark);
        }
        SnapKind::Quadrant => {
            painter.add(Shape::closed_line(vec![pos2(s.x, s.y - h), pos2(s.x + h, s.y), pos2(s.x, s.y + h), pos2(s.x - h, s.y)], mark));
        }
        SnapKind::Intersection => {
            painter.line_segment([pos2(s.x - h, s.y - h), pos2(s.x + h, s.y + h)], mark);
            painter.line_segment([pos2(s.x - h, s.y + h), pos2(s.x + h, s.y - h)], mark);
        }
        SnapKind::Perpendicular => {
            painter.line_segment([pos2(s.x - h, s.y + h), pos2(s.x + h, s.y + h)], mark);
            painter.line_segment([pos2(s.x - h, s.y + h), pos2(s.x - h, s.y - h)], mark);
            painter.line_segment([pos2(s.x - h, s.y), pos2(s.x, s.y)], mark);
            painter.line_segment([pos2(s.x, s.y), pos2(s.x, s.y + h)], mark);
        }
        SnapKind::Tangent => {
            painter.circle_stroke(pos2(s.x, s.y + h * 0.3), h * 0.7, mark);
            painter.line_segment([pos2(s.x - h, s.y - h * 0.4), pos2(s.x + h, s.y - h * 0.4)], mark);
        }
        SnapKind::Nearest => {
            painter
                .add(Shape::closed_line(vec![pos2(s.x - h, s.y - h), pos2(s.x + h, s.y - h), pos2(s.x - h, s.y + h), pos2(s.x + h, s.y + h)], mark));
        }
    }
}

/// A snap kind's name (the marker's label).
fn snap_name(kind: SnapKind) -> &'static str {
    match kind {
        SnapKind::End => "Endpoint",
        SnapKind::Mid => "Midpoint",
        SnapKind::Center => "Centre",
        SnapKind::Node => "Node",
        SnapKind::Quadrant => "Quadrant",
        SnapKind::Intersection => "Intersection",
        SnapKind::Perpendicular => "Perpendicular",
        SnapKind::Tangent => "Tangent",
        SnapKind::Nearest => "Nearest",
    }
}

/// Grip squares: half side, and how near the pointer takes one, in screen points.
const GRIP_HALF: f32 = 4.0;
const GRIP_HIT: f32 = 7.0;
/// Grips are shown for at most this many selected objects.
const MAX_GRIPPED: usize = 50;

/// The grips of the selection, with their screen points.
fn shown_grips(ed: &CadEditor, view: View) -> Vec<(usize, Grip, Pt, Pos2)> {
    if ed.tool != Tool::Select || !ed.points.is_empty() || ed.selection.len() > MAX_GRIPPED {
        return Vec::new();
    }
    ed.selection
        .iter()
        .flat_map(|&i| ed.drawing.grips(i).into_iter().map(move |(g, p)| (i, g, p)))
        .map(|(i, g, p)| (i, g, p, view.to_screen(p)))
        .collect()
}

fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// What a context-menu item does; run once the menu has read the editor's state.
#[derive(Debug, Clone, PartialEq)]
enum Act {
    Enter,
    Escape,
    Tool(Tool),
    Delete,
    Order(Order),
    Layer(String),
    Flip,
    Similar,
    Deselect,
    Option(SelectOption),
    SnapOnce(Option<SnapKind>),
    Toggle(&'static str),
    Fit,
    Undo,
    Redo,
    Close,
    ClosePolyline,
    UndoVertex,
}

/// A command's name, for the menu's title and Repeat.
fn tool_name(tool: Tool) -> &'static str {
    match tool {
        Tool::Select => "Select",
        Tool::Line => "Line",
        Tool::LineTan => "Tangent line",
        Tool::Polyline => "Polyline",
        Tool::Rect => "Rectangle",
        Tool::Circle(CircleMode::Radius) => "Circle",
        Tool::Circle(CircleMode::Diameter) => "Circle, centre and diameter",
        Tool::Circle(CircleMode::TwoPoints) => "Circle, 2 points",
        Tool::Circle(CircleMode::ThreePoints) => "Circle, 3 points",
        Tool::Circle(CircleMode::TanTanRadius) => "Circle, tan tan radius",
        Tool::Circle(CircleMode::TanTanTan) => "Circle, tan tan tan",
        Tool::Arc => "Arc",
        Tool::Point => "Point",
        Tool::Text => "Text",
        Tool::Dim(Orient::Aligned) => "Aligned dimension",
        Tool::Dim(Orient::Horizontal) => "Horizontal dimension",
        Tool::Dim(Orient::Vertical) => "Vertical dimension",
        Tool::DimAngle => "Angular dimension",
        Tool::DimContinue => "Continue dimension",
        Tool::DimBaseline => "Baseline dimension",
        Tool::Level => "Level mark",
        Tool::Strip => "Hatched strip",
        Tool::Hatch => "Hatch",
        Tool::Move => "Move",
        Tool::Copy => "Copy",
        Tool::Scale => "Scale",
        Tool::Rotate => "Rotate",
        Tool::Mirror => "Mirror",
        Tool::Offset => "Offset",
        Tool::Trim => "Trim",
        Tool::Extend => "Extend",
        Tool::Fillet => "Fillet",
        Tool::Chamfer => "Chamfer",
        Tool::Explode => "Explode",
        Tool::Join => "Join",
        Tool::MatchProp => "Match Properties",
        Tool::Stretch => "Stretch",
        Tool::ArrayRect => "Rectangular Array",
        Tool::ArrayPolar => "Polar Array",
    }
}

type Item<'a> = dyn Fn(&mut Ui, &str, &str, bool, Act) + 'a;

/// One snap for the next point, no snap, and the snap settings (AutoCAD's Shift+right-click).
fn point_items(ed: &CadEditor, ui: &mut Ui, item: &Item) {
    for (_, k) in SNAP_KINDS {
        item(ui, snap_name(k), "", ed.snap_once == Some(Some(k)), Act::SnapOnce(Some(k)));
    }
    item(ui, "None", "", ed.snap_once == Some(None), Act::SnapOnce(None));
    ui.separator();
    ui.menu_button(tl!("Object snap settings"), |ui| {
        for (word, k) in SNAP_KINDS {
            let what: &'static str = match word {
                "end" => "snap.end",
                "mid" => "snap.mid",
                "center" => "snap.center",
                "node" => "snap.node",
                "quadrant" => "snap.quadrant",
                "intersection" => "snap.intersection",
                "perpendicular" => "snap.perpendicular",
                "tangent" => "snap.tangent",
                _ => "snap.nearest",
            };
            item(ui, snap_name(k), "", ed.snaps.on(k), Act::Toggle(what));
        }
    });
    item(ui, "Ortho", "F8", ed.ortho, Act::Toggle("ortho"));
}

/// The selection options (typed at the command line, or from here).
fn selection_items(ed: &CadEditor, ui: &mut Ui, item: &Item) {
    ui.menu_button(tl!("Selection options"), |ui| {
        item(ui, "All", "T", false, Act::Option(SelectOption::All));
        item(ui, "Fence", "F", false, Act::Option(SelectOption::Fence));
        item(ui, "Window polygon", "WP", false, Act::Option(SelectOption::WPolygon));
        item(ui, "Crossing polygon", "CP", false, Act::Option(SelectOption::CPolygon));
        item(ui, "Previous selection", "P", false, Act::Option(SelectOption::Previous));
        item(ui, "Undo the last step", "U", false, Act::Option(SelectOption::Undo));
        ui.separator();
        item(ui, "Remove (a click removes)", "R", ed.remove_mode, Act::Option(SelectOption::Remove));
        item(ui, "Add (a click adds)", "A", !ed.remove_mode, Act::Option(SelectOption::Add));
    });
}

/// The canvas's context menu, AutoCAD-style: a taken grip; a command (Enter, cancel, its
/// options, the snaps); objects selected (what to do with them); nothing (the drawing).
fn menu(ed: &CadEditor, ui: &mut Ui) -> Option<Act> {
    ui.set_min_width(230.0);
    let act = std::cell::RefCell::new(None);
    let pick = |ui: &mut Ui, a: Act| {
        act.replace(Some(a));
        ui.close();
    };
    let item = |ui: &mut Ui, label: &str, keys: &str, checked: bool, a: Act| {
        if ui.add(egui::Button::new(tl!(label)).shortcut_text(keys).selected(checked)).clicked() {
            pick(ui, a);
        }
    };
    let title = |ui: &mut Ui, text: String| {
        ui.label(egui::RichText::new(text).strong().small());
    };
    if ed.menu_point {
        title(ui, tl!("Snap for the next point").to_string());
        point_items(ed, ui, &item);
    } else if ed.grip.is_some() {
        title(ui, tl!("Grip").to_string());
        item(ui, "Put the grip back", "Esc", false, Act::Escape);
        ui.separator();
        ui.menu_button(tl!("Snap for the next point (Shift+right-click)"), |ui| point_items(ed, ui, &item));
    } else if ed.tool != Tool::Select {
        let name = tl!(tool_name(ed.tool)).to_string();
        title(ui, name.clone());
        let n = ed.points.len();
        if ed.picking {
            if !ed.selection.is_empty() {
                item(ui, "Confirm the objects", "Enter", false, Act::Enter);
            }
        } else if n > 0 {
            item(ui, "Enter", "Enter", false, Act::Enter);
        }
        let cancel = if n > 0 { tl!("Cancel the step").to_string() } else { format!("{} {name}", tl!("Exit")) };
        if ui.add(egui::Button::new(cancel).shortcut_text("Esc")).clicked() {
            pick(ui, Act::Escape);
        }
        if ed.tool == Tool::Polyline && n > 0 {
            ui.separator();
            if n >= 3 {
                item(ui, "Close", "C", false, Act::ClosePolyline);
            }
            item(ui, "Undo the last vertex", "", false, Act::UndoVertex);
        }
        ui.separator();
        if ed.picking {
            selection_items(ed, ui, &item);
        } else {
            ui.menu_button(tl!("Snap for the next point (Shift+right-click)"), |ui| point_items(ed, ui, &item));
            item(ui, "Ortho", "F8", ed.ortho, Act::Toggle("ortho"));
        }
    } else if !ed.selection.is_empty() {
        let n = ed.selection.len();
        title(ui, if n == 1 { tl!("1 object selected").to_string() } else { format!("{n} {}", tl!("objects selected")) });
        item(ui, "Delete", "Del", false, Act::Delete);
        ui.separator();
        item(ui, "Move", "M", false, Act::Tool(Tool::Move));
        item(ui, "Copy", "CO", false, Act::Tool(Tool::Copy));
        let annotations = ed
            .selection
            .iter()
            .any(|&i| matches!(ed.drawing.entities.get(i).map(|e| &e.kind), Some(Kind::Dim { .. } | Kind::Angle { .. } | Kind::Level { .. })));
        if annotations {
            item(ui, "Flip", "", false, Act::Flip);
        }
        ui.separator();
        ui.menu_button(tl!("Draw order"), |ui| {
            item(ui, "Bring to Front", "", false, Act::Order(Order::Front));
            item(ui, "Send to Back", "", false, Act::Order(Order::Back));
            item(ui, "Bring Forward", "", false, Act::Order(Order::Forward));
            item(ui, "Send Backward", "", false, Act::Order(Order::Backward));
        });
        ui.menu_button(tl!("Move to layer"), |ui| {
            for l in &ed.drawing.layers {
                let on = ed.selection.iter().all(|&i| ed.drawing.entities.get(i).is_some_and(|e| e.layer == l.name));
                if ui.add(egui::Button::new(&l.name).selected(on)).clicked() {
                    pick(ui, Act::Layer(l.name.clone()));
                }
            }
        });
        ui.separator();
        item(ui, "Select Similar", "", false, Act::Similar);
        item(ui, "Deselect All", "Esc", false, Act::Deselect);
        item(ui, "Properties", "", ed.panel, Act::Toggle("props"));
    } else {
        if let Some(last) = ed.last_tool {
            let label = format!("{} {}", tl!("Repeat"), tl!(tool_name(last)));
            if ui.add(egui::Button::new(label).shortcut_text("Enter")).clicked() {
                pick(ui, Act::Tool(last));
            }
        }
        item(ui, "Undo", "Ctrl+Z", false, Act::Undo);
        item(ui, "Redo", "Ctrl+Y", false, Act::Redo);
        ui.separator();
        item(ui, "Select All", "Ctrl+A", false, Act::Option(SelectOption::All));
        selection_items(ed, ui, &item);
        ui.separator();
        item(ui, "Zoom Fit", "", false, Act::Fit);
        ui.menu_button(tl!("Object snap settings"), |ui| {
            for (word, k) in SNAP_KINDS {
                let what: &'static str = match word {
                    "end" => "snap.end",
                    "mid" => "snap.mid",
                    "center" => "snap.center",
                    "node" => "snap.node",
                    "quadrant" => "snap.quadrant",
                    "intersection" => "snap.intersection",
                    "perpendicular" => "snap.perpendicular",
                    "tangent" => "snap.tangent",
                    _ => "snap.nearest",
                };
                item(ui, snap_name(k), "", ed.snaps.on(k), Act::Toggle(what));
            }
        });
        item(ui, "Snap", "", ed.snap, Act::Toggle("snap"));
        item(ui, "Ortho", "F8", ed.ortho, Act::Toggle("ortho"));
        item(ui, "Grid", "", ed.grid, Act::Toggle("grid"));
        item(ui, "Properties", "", ed.panel, Act::Toggle("props"));
        ui.separator();
        item(ui, "Close Drawing", "", false, Act::Close);
    }
    act.into_inner()
}

/// Runs a context-menu choice.
fn apply(app: &mut WordApp, a: Act) {
    let mut done = None;
    match a {
        Act::Delete => delete_selection(app),
        Act::Order(o) => reorder(app, o),
        Act::Layer(name) => {
            let _ = app.run("ui.cadLayer", json!({"layer": name}));
        }
        Act::Flip => {
            let _ = app.run("ui.cadFlip", json!({}));
        }
        Act::Undo => {
            let _ = app.run("edit.undo", json!({}));
        }
        Act::Redo => {
            let _ = app.run("edit.redo", json!({}));
        }
        Act::Close => close(app),
        Act::Toggle(what) => {
            let _ = app.run("ui.cadToggle", json!({"what": what}));
        }
        other => {
            let Some(ed) = app.cad.as_mut() else { return };
            match other {
                Act::Enter => done = ed.enter(),
                Act::Escape => done = ed.escape(),
                Act::Tool(t) => ed.set_tool(t),
                Act::Similar => {
                    let ids = ed.drawing.similar(&ed.selection);
                    ed.pick_into(ids, false);
                }
                Act::Deselect => ed.clear_selection(),
                Act::Option(o) => ed.run_option(o),
                Act::SnapOnce(k) => ed.snap_once = Some(k),
                Act::Fit => ed.fitted = false,
                Act::ClosePolyline => done = close_polyline(ed),
                Act::UndoVertex => {
                    ed.points.pop();
                }
                _ => {}
            }
        }
    }
    if let Some(d) = done {
        commit(app, &d);
    }
}

impl CadEditor {
    /// Enter with nothing typed (or Space): closes a polygon, confirms the objects, ends the
    /// command (a polyline in progress is kept), or at the idle prompt repeats the last command.
    fn enter(&mut self) -> Option<Drawing> {
        if self.poly.is_some() {
            self.finish_poly(self.remove_mode);
            return None;
        }
        if self.picking {
            if self.selection.is_empty() {
                self.set_tool(Tool::Select);
            } else {
                self.picking = false;
                self.previous = self.selection.clone();
                self.remove_mode = false;
            }
            return None;
        }
        if !self.points.is_empty() {
            let kept = finish(self);
            self.points.clear();
            return kept;
        }
        match (self.tool, self.last_tool) {
            (Tool::Select, Some(last)) => self.set_tool(last),
            (Tool::Select, None) => {}
            _ => self.set_tool(Tool::Select),
        }
        None
    }
}

/// The editor's sheet, Properties pane, status line and command line, in place of the page.
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
    let mut act: Option<Act> = None;
    let mut close_now = false;
    {
        let Some(ed) = app.cad.as_mut() else { return };
        let bottom = 58.0;
        let full = ui.available_rect_before_wrap();
        let panel_w = if ed.panel && !ed.panel_float { 300.0_f32.min(full.width() * 0.4) } else { 0.0 };
        // The Layer Manager takes a band under the sheet, above the command line (docked).
        let layers_h = if ed.layers_panel && !ed.layers_float { 220.0_f32.min(full.height() * 0.35) } else { 0.0 };
        let sheet = Rect::from_min_max(full.min, pos2(full.max.x - panel_w, (full.max.y - bottom - layers_h).max(full.min.y + 40.0)));
        let (resp, painter) = ui.allocate_painter(sheet.size(), Sense::click_and_drag());
        let rect = resp.rect;
        painter.rect_filled(rect, 0.0, SHEET);
        // Fit the drawing on first show (and on Zoom Fit).
        if !ed.fitted {
            let k = if ed.rendered.scale > 0.0 { ed.rendered.scale } else { 1.0 };
            let (w, h) = (ed.rendered.size.0 / k, ed.rendered.size.1 / k);
            ed.center = paper_to_drawing(&ed.rendered, (ed.rendered.size.0 / 2.0, ed.rendered.size.1 / 2.0));
            ed.zoom = (f64::from(rect.width()) * 0.8 / w.max(1e-9)).min(f64::from(rect.height()) * 0.8 / h.max(1e-9)).clamp(1e-6, 1e9);
            ed.fitted = true;
        }
        let menu_open = egui::Popup::is_any_open(ui.ctx());
        let input = ui.input(|i| (i.pointer.hover_pos(), i.smooth_scroll_delta.y, i.pointer.middle_down(), i.modifiers, i.pointer.delta()));
        let (hover, scroll, middle, mods, delta) = input;
        let hovering = hover.filter(|p| rect.contains(*p)).filter(|_| !menu_open);
        // Zoom about the pointer; pan with the middle button or Alt+drag.
        if let Some(p) = hovering
            && scroll != 0.0
        {
            let before = View { rect, center: ed.center, zoom: ed.zoom }.to_drawing(p);
            ed.zoom = (ed.zoom * f64::from((scroll / 200.0).exp())).clamp(1e-6, 1e9);
            let after = View { rect, center: ed.center, zoom: ed.zoom }.to_drawing(p);
            ed.center = ed.center.plus(before.minus(after));
        }
        let panning = hovering.is_some() && (middle || (mods.alt && resp.dragged()));
        if panning {
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
        // The picture's frame (the drawing's size with its padding); the area outside it shaded.
        if ed.frame {
            let a = view.to_screen(paper_to_drawing(&ed.rendered, (0.0, 0.0)));
            let b = view.to_screen(paper_to_drawing(&ed.rendered, ed.rendered.size));
            let frame = Rect::from_two_pos(a, b);
            let shade = Color32::from_rgba_unmultiplied(60, 60, 60, 26);
            for r in [
                Rect::from_min_max(rect.min, pos2(rect.max.x, frame.min.y.max(rect.min.y))),
                Rect::from_min_max(pos2(rect.min.x, frame.max.y.min(rect.max.y)), rect.max),
                Rect::from_min_max(pos2(rect.min.x, frame.min.y.max(rect.min.y)), pos2(frame.min.x.max(rect.min.x), frame.max.y.min(rect.max.y))),
                Rect::from_min_max(pos2(frame.max.x.min(rect.max.x), frame.min.y.max(rect.min.y)), pos2(rect.max.x, frame.max.y.min(rect.max.y))),
            ] {
                if r.is_positive() {
                    painter.rect_filled(r, 0.0, shade);
                }
            }
            painter.rect_stroke(frame, 0.0, Stroke::new(1.0, Color32::from_gray(150)), egui::StrokeKind::Middle);
        }
        // The drawing.
        let k = if ed.rendered.scale > 0.0 { ed.rendered.scale } else { 1.0 };
        for prim in &ed.rendered.prims {
            match prim {
                Prim::Path { pts, color, width, fill, .. } => {
                    let to_screen = |q: &(f64, f64)| view.to_screen(paper_to_drawing(&ed.rendered, *q));
                    let stroke = Stroke::new((*width as f32 * 3.0).clamp(1.0, 4.0), rgb(*color));
                    if *fill {
                        painter.add(Shape::convex_polygon(pts.iter().map(to_screen).collect(), rgb(*color), stroke));
                    } else {
                        // Its linetype's dashes, as on the page (a solid path is one piece).
                        for piece in prim.pieces() {
                            painter.add(Shape::line(piece.iter().map(to_screen).collect(), stroke));
                        }
                    }
                }
                Prim::Dot { at, r, color } => {
                    painter.circle_filled(
                        view.to_screen(paper_to_drawing(&ed.rendered, *at)),
                        (*r * ed.zoom / k).clamp(2.0, 12.0) as f32,
                        rgb(*color),
                    );
                }
                Prim::Text { at, text, height, anchor, color, angle } => {
                    let size = ((*height / 0.7) * ed.zoom / k).clamp(6.0, 200.0) as f32;
                    let align = match anchor {
                        Anchor::BottomCenter | Anchor::BaselineCenter => Align2::CENTER_BOTTOM,
                        Anchor::TopCenter => Align2::CENTER_TOP,
                        Anchor::BaselineLeft => Align2::LEFT_BOTTOM,
                        Anchor::BaselineRight => Align2::RIGHT_BOTTOM,
                        Anchor::MiddleLeft => Align2::LEFT_CENTER,
                        Anchor::MiddleRight => Align2::RIGHT_CENTER,
                    };
                    let pos = view.to_screen(paper_to_drawing(&ed.rendered, *at));
                    let galley = painter.layout_job(text_job(text, size, rgb(*color)));
                    // Turned about its anchor: counter-clockwise on paper is clockwise on the
                    // screen (y down).
                    let turn = if angle.is_finite() { -(angle.to_radians() as f32) } else { 0.0 };
                    let anchor_in = vec2(galley.size().x * align.x().to_factor(), galley.size().y * align.y().to_factor());
                    let rot = egui::emath::Rot2::from_angle(turn);
                    painter.add(egui::epaint::TextShape::new(pos - rot * anchor_in, galley, rgb(*color)).with_angle(rot.angle()));
                }
            }
        }
        // The selection, highlighted; the objects the Properties pane works on (when not all of
        // the selection) in orange.
        let (_, scope) = ed.focus.scope(&ed.drawing, &ed.selection);
        let partial = scope.len() < ed.selection.len();
        let orange = Color32::from_rgb(0xEA, 0x58, 0x0C);
        for &i in &ed.selection {
            let color = if partial && scope.contains(&i) { orange } else { t.accent };
            if let Some(e) = ed.drawing.entities.get(i) {
                for line in ed.drawing.shape(e) {
                    let screen: Vec<Pos2> = line.iter().map(|&p| view.to_screen(p)).collect();
                    if let [only] = screen.as_slice() {
                        painter.circle_stroke(*only, 5.0, Stroke::new(2.0, color));
                    } else {
                        if partial && scope.contains(&i) {
                            painter.add(Shape::line(screen.clone(), Stroke::new(6.0, color.gamma_multiply(0.3))));
                        }
                        painter.add(Shape::dashed_line(&screen, Stroke::new(2.5, color), 6.0, 3.0));
                    }
                }
            }
        }
        // Grips of the selection: idle, under the pointer, or taken (hot).
        let grips = shown_grips(ed, view);
        for &(i, g, _, at) in &grips {
            let hot = ed.grip.is_some_and(|(hi, hg, _)| hi == i && hg == g);
            let under = hovering.is_some_and(|h| at.distance(h) <= GRIP_HIT);
            let fill = if hot {
                Color32::from_rgb(0xE1, 0x1D, 0x48)
            } else if under {
                Color32::from_rgb(0x2D, 0xD4, 0xBF)
            } else {
                Color32::from_rgb(0x0F, 0x76, 0x6E)
            };
            let r = Rect::from_center_size(at, vec2(2.0 * GRIP_HALF, 2.0 * GRIP_HALF));
            painter.rect_filled(r, 0.0, fill);
            painter.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::WHITE), egui::StrokeKind::Outside);
        }
        let hover_grip = hovering
            .and_then(|h| grips.iter().filter(|g| g.3.distance(h) <= GRIP_HIT).min_by(|a, b| a.3.distance(h).total_cmp(&b.3.distance(h))).copied());
        // Overlapping objects: the cycle starts again once the pointer moves away.
        if let (Some(at), Some(h)) = (ed.cycle_at, hovering)
            && at.distance(h) > 4.0
        {
            ed.cycle = 0;
            ed.cycle_at = None;
        }
        let selecting = ed.selecting();
        // An object pick (Trim, Extend, Fillet…): the pointer as it is, like a selection.
        let free = selecting || ed.tool.picks_object();
        let removing = mods.shift || ed.remove_mode;
        let tol = 6.0 / ed.zoom;
        // The object a click would take, haloed (red when the click would remove it).
        let candidate =
            hovering.filter(|_| selecting && hover_grip.is_none() && ed.lasso.is_empty() && ed.win.is_none() && ed.poly.is_none()).and_then(|h| {
                let list = ed.drawing.hits(view.to_drawing(h), tol);
                list.get(ed.cycle % list.len().max(1)).copied()
            });
        if let Some(e) = candidate.and_then(|i| ed.drawing.entities.get(i)) {
            let halo =
                if removing { Color32::from_rgba_unmultiplied(0xE1, 0x1D, 0x48, 90) } else { Color32::from_rgba_unmultiplied(0x10, 0xB9, 0x81, 90) };
            for line in ed.drawing.shape(e) {
                let screen: Vec<Pos2> = line.iter().map(|&p| view.to_screen(p)).collect();
                if let [only] = screen.as_slice() {
                    painter.circle_filled(*only, 6.0, halo);
                } else {
                    painter.add(Shape::line(screen, Stroke::new(7.0, halo)));
                }
            }
        }
        // The pointer: snapped (only when a point is asked for), then held level or plumb by Ortho.
        let mut snapped: Option<(Pt, SnapKind)> = None;
        let mut cursor: Option<Pt> = None;
        // The point the next one is measured from: a hot grip's place, or the last point.
        let base = ed.grip.map(|g| g.2).or_else(|| ed.points.last().copied());
        if let Some(h) = hovering {
            let raw = view.to_drawing(h);
            let snaps = match ed.snap_once {
                Some(Some(k)) => Some(only(k)),
                Some(None) => None,
                None => ed.snap.then_some(ed.snaps),
            };
            if !free && let Some(on) = snaps {
                // Exact snaps from the geometry kernel: ends, midpoints, centres, nodes,
                // quadrants, intersections, perpendiculars and tangents from the base point.
                snapped = ed.drawing.snap(raw, 10.0 / ed.zoom, base, &on);
            }
            let mut p = snapped.map_or(raw, |(p, _)| p);
            if !free && snapped.is_none() && ed.grid && ed.snap && ed.snap_once.is_none() {
                let step = grid_step(ed.zoom);
                let g = Pt::new((raw.x / step).round() * step, (raw.y / step).round() * step);
                if view.to_screen(g).distance(h) <= 6.0 {
                    p = g;
                }
            }
            // Ortho holds the point level or plumb from the base, unless an object snap holds it.
            if !free
                && ed.ortho
                && snapped.is_none()
                && let Some(last) = base
            {
                let d = p.minus(last);
                p = if d.x.abs() >= d.y.abs() { Pt::new(p.x, last.y) } else { Pt::new(last.x, p.y) };
            }
            cursor = Some(p);
            // The cursor tells the state, as in AutoCAD: idle, the crosshair with the pickbox;
            // asking for a point, the crosshair; asking for objects, the pickbox.
            let s = view.to_screen(p);
            let hair = Stroke::new(1.0, Color32::from_gray(90));
            if !ed.picking || !selecting {
                painter.line_segment([pos2(s.x - 14.0, s.y), pos2(s.x + 14.0, s.y)], hair);
                painter.line_segment([pos2(s.x, s.y - 14.0), pos2(s.x, s.y + 14.0)], hair);
            }
            if selecting {
                let pick = if candidate.is_some() || hover_grip.is_some() { Stroke::new(1.4, t.accent) } else { hair };
                painter.rect_stroke(Rect::from_center_size(s, vec2(8.0, 8.0)), 0.0, pick, egui::StrokeKind::Middle);
            }
            if let Some((_, kind)) = snapped {
                snap_marker(&painter, s, kind);
                painter.text(
                    pos2(s.x + 10.0, s.y - 10.0),
                    Align2::LEFT_BOTTOM,
                    tl!(snap_name(kind)),
                    FontId::proportional(11.0),
                    Color32::from_rgb(0xB4, 0x53, 0x09),
                );
            }
            // A hot grip: the band from where it was, and the object as it would become.
            if let Some((i, g, from)) = ed.grip {
                let band = Stroke::new(1.2, t.accent);
                painter.line_segment([view.to_screen(from), s], band);
                let mut ghost = ed.drawing.clone();
                if ghost.move_grip(i, g, p)
                    && let Some(e) = ghost.entities.get(i)
                {
                    for line in ed.drawing.shape(e) {
                        let screen: Vec<Pos2> = line.iter().map(|&q| view.to_screen(q)).collect();
                        if screen.len() > 1 {
                            painter.add(Shape::dashed_line(&screen, Stroke::new(1.5, t.accent), 5.0, 3.0));
                        }
                    }
                }
            }
            // Rubber band from the last point.
            if let Some(&last) = ed.points.last() {
                let band = Stroke::new(1.2, t.accent);
                let a = view.to_screen(last);
                match ed.tool {
                    Tool::Rect | Tool::Stretch if ed.points.len() == 1 => {
                        let r = Rect::from_two_pos(a, s);
                        painter.rect_stroke(r, 0.0, band, egui::StrokeKind::Middle);
                    }
                    // The circle or line the next click would make.
                    Tool::Circle(mode) => {
                        let mut pts = ed.points.clone();
                        pts.push(p);
                        match circle_of(&ed.drawing, mode, &pts) {
                            Ok(Some((c, r))) => {
                                painter.circle_stroke(view.to_screen(c), (r * ed.zoom) as f32, band);
                            }
                            _ => {
                                painter.line_segment([a, s], band);
                            }
                        }
                    }
                    Tool::LineTan => {
                        if let Ok((t1, t2)) = tangent_line(&ed.drawing, last, p) {
                            painter.line_segment([view.to_screen(t1), view.to_screen(t2)], band);
                        }
                    }
                    _ => {
                        painter.line_segment([a, s], band);
                    }
                }
            }
            // A window (blue, whole objects) to the right; a crossing (green, dashed) to the left.
            let window_look = |crossing: bool| {
                if crossing {
                    (Color32::from_rgba_unmultiplied(40, 160, 80, 30), Color32::from_rgb(0x16, 0xA3, 0x4A))
                } else {
                    (Color32::from_rgba_unmultiplied(40, 90, 200, 30), Color32::from_rgb(0x25, 0x63, 0xEB))
                }
            };
            if let Some(w) = ed.win {
                let from = view.to_screen(w);
                let crossing = h.x < from.x;
                let (fill, edge) = window_look(crossing);
                let r = Rect::from_two_pos(from, h);
                painter.rect_filled(r, 0.0, fill);
                let corners = vec![r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
                if crossing {
                    painter.add(Shape::dashed_line(&corners, Stroke::new(1.0, edge), 5.0, 3.0));
                } else {
                    painter.add(Shape::line(corners, Stroke::new(1.0, edge)));
                }
            }
            if let Some(&first) = ed.lasso.first() {
                let crossing = ed.lasso.iter().find(|q| (q.x - first.x).abs() > 2.0).is_some_and(|q| q.x < first.x);
                let (_, edge) = window_look(crossing);
                let mut path = ed.lasso.clone();
                path.push(h);
                painter.add(Shape::closed_line(path, Stroke::new(1.0, edge)));
            }
            if let Some((mode, pts)) = &ed.poly {
                let mut path: Vec<Pos2> = pts.iter().map(|&q| view.to_screen(q)).collect();
                path.push(h);
                let (_, edge) = window_look(*mode == SelectOption::CPolygon);
                if *mode == SelectOption::Fence {
                    painter.add(Shape::dashed_line(&path, Stroke::new(1.2, edge), 5.0, 3.0));
                } else {
                    painter.add(Shape::closed_line(path, Stroke::new(1.2, edge)));
                }
            }
        }
        if hovering.is_some() && !panning {
            ui.ctx().set_cursor_icon(egui::CursorIcon::None);
        } else if panning {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        // Grips: a click takes one (hot), the next point places it; a drag from one places it
        // where it is released. Like AutoCAD and Allplan, the point snaps and follows Ortho.
        let mut grip_done: Option<Drawing> = None;
        if resp.drag_started_by(egui::PointerButton::Primary)
            && !mods.alt
            && ed.grip.is_none()
            && let Some((i, g, at, _)) = hover_grip
        {
            ed.grip = Some((i, g, at));
            ed.grip_drag = true;
        }
        if ed.grip_drag && resp.drag_stopped() {
            ed.grip_drag = false;
            if let (Some((i, g, _)), Some(p)) = (ed.grip.take(), cursor) {
                let mut d = ed.drawing.clone();
                if d.move_grip(i, g, p) {
                    grip_done = Some(d);
                }
            }
        }
        let grip_click = resp.clicked_by(egui::PointerButton::Primary) && (ed.grip.is_some() || hover_grip.is_some());
        if grip_click {
            match (ed.grip.take(), cursor) {
                (Some((i, g, _)), Some(p)) => {
                    let mut d = ed.drawing.clone();
                    if d.move_grip(i, g, p) {
                        grip_done = Some(d);
                    } else {
                        ed.message = tl!("That would make the object degenerate").to_string();
                    }
                }
                (None, _) => ed.grip = hover_grip.map(|(i, g, at, _)| (i, g, at)),
                _ => {}
            }
        }
        if let Some(d) = grip_done {
            ed.snap_once = None;
            finished = Some(d);
        }
        // Selecting: a click adds (Shift removes), a click on empty space starts a two-click
        // window, a drag from empty space is a lasso, a fence or polygon takes clicked vertices.
        if selecting && !grip_click && !ed.grip_drag && !mods.alt {
            if resp.drag_started_by(egui::PointerButton::Primary)
                && hover_grip.is_none()
                && ed.poly.is_none()
                && ed.win.is_none()
                && let Some(p) = resp.interact_pointer_pos()
            {
                ed.lasso = vec![p];
            }
            if !ed.lasso.is_empty()
                && resp.dragged_by(egui::PointerButton::Primary)
                && let Some(h) = hover
                && ed.lasso.last().is_some_and(|l| l.distance(h) > 3.0)
                && ed.lasso.len() < 4000
            {
                ed.lasso.push(h);
            }
            if resp.drag_stopped() && !ed.lasso.is_empty() {
                let path = std::mem::take(&mut ed.lasso);
                // The first move decides: to the right whole objects, to the left touched too.
                if let Some(&first) = path.first()
                    && path.len() >= 3
                {
                    let crossing = path.iter().find(|q| (q.x - first.x).abs() > 2.0).is_some_and(|q| q.x < first.x);
                    let poly: Vec<Pt> = path.iter().map(|&q| view.to_drawing(q)).collect();
                    let ids = ed.drawing.in_polygon(&poly, crossing);
                    ed.pick_into(ids, removing);
                }
            }
            if resp.clicked_by(egui::PointerButton::Primary)
                && let Some(h) = hover
            {
                let raw = view.to_drawing(h);
                if let Some((_, pts)) = ed.poly.as_mut() {
                    if pts.len() < 500 {
                        pts.push(raw);
                    }
                } else if let Some(w) = ed.win.take() {
                    let ids = ed.drawing.in_box(w, raw, raw.x < w.x);
                    ed.pick_into(ids, removing);
                } else if !ed.click_select(raw, h, tol, removing) {
                    ed.win = Some(raw);
                    ed.found.clear();
                }
            }
        } else if resp.clicked_by(egui::PointerButton::Primary)
            && !grip_click
            && let Some(p) = cursor
        {
            finished = feed(ed, p, None);
            ed.snap_once = None;
        }
        // The right button: the context menu (with Shift while a point is asked for, the snap
        // menu); with a polygon in progress, it closes the polygon.
        let mut closed_poly = false;
        if resp.secondary_clicked() {
            if ed.poly.is_some() {
                ed.finish_poly(removing);
                closed_poly = true;
            } else {
                ed.menu_at = hover.map(|h| view.to_drawing(h));
                ed.menu_point = mods.shift && !selecting;
                // With nothing selected, a right-click on an object selects it (AutoCAD).
                if ed.tool == Tool::Select
                    && ed.selection.is_empty()
                    && ed.grip.is_none()
                    && let Some(h) = hover
                {
                    let list = ed.drawing.hits(view.to_drawing(h), tol);
                    if let Some(&i) = list.get(ed.cycle % list.len().max(1)) {
                        ed.pick_into(vec![i], false);
                    }
                }
            }
        }
        if !closed_poly {
            resp.context_menu(|ui| {
                if let Some(a) = menu(ed, ui) {
                    act = Some(a);
                }
            });
        }

        // The Properties pane and the Layer Manager: docked beside and under the sheet, or
        // floating in their own windows; each closes with its X and detaches or docks again.
        if ed.panel {
            let docked = Rect::from_min_max(pos2(sheet.max.x, full.min.y), pos2(full.max.x, sheet.max.y));
            if let Some(d) = pane(ed, ui, &t, Pane::Properties, docked, vec2(300.0, 460.0)) {
                finished = Some(d);
            }
        }
        if ed.layers_panel {
            let docked = Rect::from_min_max(pos2(full.min.x, sheet.max.y), pos2(sheet.max.x, sheet.max.y + layers_h));
            if let Some(d) = pane(ed, ui, &t, Pane::Layers, docked, vec2(720.0, 240.0)) {
                finished = Some(d);
            }
            // The status and command lines go under it.
            ui.add_space(layers_h);
        }

        // Keys: not while a menu is open or a field of the pane has the keyboard. Esc cancels,
        // Enter or Space ends (or repeats), F8 Ortho, Delete removes the selection, Ctrl+A
        // selects all, Shift+Space steps through overlapping objects.
        let keys_free = !menu_open && !ed.panel_typing;
        let typing_text = ed.tool == Tool::Text && ed.points.len() == 1;
        let (shift_space, space) = if keys_free && ed.command.trim().is_empty() && !typing_text {
            ui.input_mut(|i| {
                let s = i.consume_key(egui::Modifiers::SHIFT, egui::Key::Space);
                let n = i.consume_key(egui::Modifiers::NONE, egui::Key::Space);
                if s || n {
                    i.events.retain(|e| !matches!(e, egui::Event::Text(t) if t.trim().is_empty()));
                }
                (s, n)
            })
        } else {
            (false, false)
        };
        let (esc, enter, f8, del) = ui.input(|i| {
            (i.key_pressed(egui::Key::Escape), i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::F8), i.key_pressed(egui::Key::Delete))
        });
        let (esc, enter, f8, del) = if keys_free { (esc, enter || space, f8, del) } else { (false, false, false, false) };
        if shift_space && let Some(h) = hovering {
            let list = ed.drawing.hits(view.to_drawing(h), tol);
            if list.len() > 1 {
                ed.cycle += 1;
                ed.cycle_at = Some(h);
                ed.found = format!("{} {} {} {} ({})", tl!("cycle:"), ed.cycle % list.len() + 1, tl!("of"), list.len(), tl!("click to pick it"));
            }
        }
        if f8 {
            ed.ortho = !ed.ortho;
        }
        if keys_free && ui.input(|i| i.key_pressed(egui::Key::A) && i.modifiers.command) && ed.command.is_empty() {
            ed.pick_into((0..ed.drawing.entities.len()).collect(), false);
        }
        if esc {
            if let Some(kept) = ed.escape() {
                finished = Some(kept);
            }
            ed.command.clear();
        }
        if del && ed.command.is_empty() {
            act = Some(Act::Delete);
        }

        // Status and command line.
        let mut prompt = if ed.message.is_empty() { ed.prompt() } else { tl!(&ed.message).to_string() };
        if !ed.found.is_empty() && ed.message.is_empty() {
            prompt = format!("{}   [{}]", prompt, ed.found);
        }
        let hover_prompt = prompt.clone();
        let mut coords = cursor.map(|p| format!("{:.3}, {:.3}", p.x, p.y)).unwrap_or_default();
        // From the base point: distance, angle (exact, from the kernel), Δx, Δy.
        if let (Some(b), Some(p)) = (base, cursor) {
            let v = p.minus(b);
            coords = format!(
                "{} {:.4}  {} {:.4}°  Δx {:.4}  Δy {:.4}   {coords}",
                tl!("Distance"),
                v.len(),
                tl!("Angle"),
                kernel::deg(kernel::direction(v)),
                v.x,
                v.y
            );
        }
        // The flags at the right; the prompt in the space left, cut short if it must be.
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let flag = |on: bool, name: &str| if on { format!("{} ✓", tl!(name)) } else { tl!(name).to_string() };
                let once = match ed.snap_once {
                    Some(Some(k)) => format!("{}: {}   ", tl!("Next point"), tl!(snap_name(k))),
                    Some(None) => format!("{}: {}   ", tl!("Next point"), tl!("no snap")),
                    None => String::new(),
                };
                ui.label(
                    egui::RichText::new(format!(
                        "{once}{coords}   {}   {}   {}   {} {}   {} {}",
                        flag(ed.snap, "Snap"),
                        flag(ed.grid, "Grid"),
                        flag(ed.ortho, "Ortho (F8)"),
                        tl!("Layer:"),
                        ed.current_layer,
                        tl!("Unit:"),
                        ed.drawing.unit.symbol()
                    ))
                    .weak(),
                );
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(prompt).strong()).truncate()).on_hover_text(&hover_prompt);
                });
            });
        });
        let field = ui.add(
            egui::TextEdit::singleline(&mut ed.command)
                .id_salt("cad_command_line")
                .hint_text(tl!("Type a point (x,y  @dx,dy  @d<a), a distance, a command (l, pl, rec, c, a, t, m, co), an option or press Enter"))
                .desired_width(f32::INFINITY),
        );
        if !field.has_focus() && hovering.is_some() && !ed.panel_typing {
            field.request_focus();
        }
        if enter && finished.is_none() {
            let typed = std::mem::take(&mut ed.command);
            let typed = typed.trim();
            let option = SelectOption::parse(typed);
            if let Some((i, g, from)) = ed.grip.filter(|_| !typed.is_empty()) {
                // A typed point places the hot grip; `@dx,dy` is from where it was.
                match parse_point(typed, Some(from), cursor) {
                    Ok(p) => {
                        let mut d = ed.drawing.clone();
                        ed.grip = None;
                        if d.move_grip(i, g, p) {
                            ed.snap_once = None;
                            finished = Some(d);
                        }
                    }
                    Err(e) => ed.message = e.to_string(),
                }
            } else if typed.is_empty() {
                finished = ed.enter();
            } else if ed.picking || ed.poly.is_some() {
                // A command asking for objects takes the selection options.
                match option {
                    Some(o) => ed.run_option(o),
                    None => ed.message = format!("{} {typed}", tl!("Unknown selection option:")),
                }
            } else if let Some(tool) = Tool::parse(&typed.to_lowercase()).filter(|_| ed.points.is_empty() || ed.tool == Tool::Select) {
                ed.set_tool(tool);
            } else if let Some(o) = option.filter(|_| ed.tool == Tool::Select && ed.points.is_empty()) {
                // At the idle prompt U undoes the last command, as in AutoCAD.
                if o == SelectOption::Undo { act = Some(Act::Undo) } else { ed.run_option(o) }
            } else if typing_text {
                finished = add_text(ed, typed);
            } else if ed.tool == Tool::Polyline && typed.eq_ignore_ascii_case("c") {
                finished = close_polyline(ed);
            } else if ed.tool == Tool::Circle(CircleMode::Radius) && ed.points.len() == 1 && typed.eq_ignore_ascii_case("d") {
                // `d` after the centre: the diameter instead of the radius.
                ed.tool = Tool::Circle(CircleMode::Diameter);
            } else if typed_counts(ed, typed) {
                // Columns × rows for a rectangular array.
            } else if let Some(result) = typed.replace(',', ".").parse::<f64>().ok().and_then(|v| typed_value(ed, v)) {
                finished = result;
            } else if let Some(v) = typed.parse::<f64>().ok().filter(|_| takes_size(ed))
                && let Some(&last) = ed.points.last()
            {
                // A radius or diameter typed: the point that far from the last one.
                finished = feed(ed, last.plus(Pt::new(v.abs(), 0.0)), None);
                ed.snap_once = None;
            } else {
                match parse_point(typed, ed.points.last().copied(), cursor) {
                    Ok(p) => {
                        finished = feed(ed, p, None);
                        ed.snap_once = None;
                    }
                    Err(e) => ed.message = e.to_string(),
                }
            }
        }
        if ui.input(|i| i.key_pressed(egui::Key::Q) && i.modifiers.command) {
            close_now = true;
        }
        if finished.is_none() {
            finished = immediate(ed);
        }
    }
    if let Some(d) = finished {
        commit(app, &d);
    }
    if let Some(a) = act {
        apply(app, a);
    }
    if close_now {
        close(app);
    }
}

/// The drawing with `e` added.
fn with(ed: &CadEditor, mut e: Entity) -> Option<Drawing> {
    let mut d = ed.drawing.clone();
    // New objects go on the current layer.
    if d.layers.iter().any(|l| l.name == ed.current_layer) {
        e.layer = ed.current_layer.clone();
    }
    // Hatches go behind everything else, as they fill around it.
    let hatch = matches!(e.kind, Kind::Strip { .. } | Kind::Hatch { .. });
    d.add(e).ok()?;
    if hatch {
        d.reorder(&[d.entities.len().saturating_sub(1)], Order::Back);
    }
    Some(d)
}

fn pt(p: Pt) -> P {
    P(Num::new(p.x), Num::new(p.y))
}

/// A drawing text laid out for the canvas: a parameter's symbol in italics, its subscript
/// smaller and lowered, a missing parameter in red (`strix_cad::spans`).
fn text_job(text: &str, size: f32, color: Color32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    for p in strix_cad::spans(text) {
        let sub = p.sub;
        let font = FontId::proportional(if sub { size * 0.7 } else { size });
        let format = egui::TextFormat {
            font_id: font,
            color: if p.kind == strix_cad::SpanKind::Missing { Color32::from_rgb(0xC0, 0x1C, 0x1C) } else { color },
            italics: p.symbol,
            // A subscript's line, shorter than the text's, sits at the bottom: the subscript drops.
            line_height: sub.then_some(size * 0.6),
            ..Default::default()
        };
        job.append(&p.text, 0.0, format);
    }
    job
}

/// The editor's two panes.
#[derive(Clone, Copy, PartialEq)]
enum Pane {
    Properties,
    Layers,
}

impl Pane {
    fn title(self) -> &'static str {
        match self {
            Pane::Properties => "Properties",
            Pane::Layers => "Layer Manager",
        }
    }

    /// Shown, floating.
    fn state(self, ed: &mut CadEditor) -> (&mut bool, &mut bool) {
        match self {
            Pane::Properties => (&mut ed.panel, &mut ed.panel_float),
            Pane::Layers => (&mut ed.layers_panel, &mut ed.layers_float),
        }
    }
}

/// Pane `which`, in `docked` or (detached) in a window of about `size`, under a header with its
/// title, Detach/Dock and Close. The drawing to store when something changed.
fn pane(ed: &mut CadEditor, ui: &mut egui::Ui, t: &crate::theme::Tokens, which: Pane, docked: Rect, size: egui::Vec2) -> Option<Drawing> {
    let body = |ed: &mut CadEditor, ui: &mut egui::Ui| -> Option<Drawing> {
        let floating = *which.state(ed).1;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(tl!(which.title())).strong().size(14.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if crate::widgets::icon_button(ui, "close", tl!("Close")).clicked() {
                    *which.state(ed).0 = false;
                }
                let (icon, tip) = if floating { ("pin", tl!("Dock")) } else { ("launcher", tl!("Detach: a window of its own")) };
                if crate::widgets::icon_button(ui, icon, tip).clicked() {
                    *which.state(ed).1 = !floating;
                }
            });
        });
        match which {
            Pane::Properties => crate::cad_panel::show(ed, ui),
            Pane::Layers => crate::cad_panel::layer_manager(ed, ui),
        }
    };
    if !*which.state(ed).1 {
        ui.painter().rect_filled(docked, 0.0, t.panel);
        let edge = if which == Pane::Properties { [docked.left_top(), docked.left_bottom()] } else { [docked.left_top(), docked.right_top()] };
        ui.painter().line_segment(edge, Stroke::new(1.0, t.border));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(docked.shrink2(vec2(8.0, 6.0))));
        return body(ed, &mut child);
    }
    // Detached: a window that opens over the sheet's corner, moved by its background.
    let start =
        if which == Pane::Properties { docked.right_top() - vec2(size.x + 12.0, -12.0) } else { docked.left_top() + vec2(12.0, -size.y - 12.0) };
    egui::Window::new(which.title())
        .id(egui::Id::unique(if which == Pane::Properties { "cad_pane_properties" } else { "cad_pane_layers" }))
        .title_bar(false)
        .resizable(true)
        .default_size(size)
        .default_pos(start)
        .constrain(true)
        .show(ui.ctx(), |ui| body(ed, ui))
        .and_then(|r| r.inner)
        .flatten()
}

/// The selection transformed by `t` (a copy of it when `copy`): one undo step.
fn transformed(ed: &mut CadEditor, t: &strix_cad::Affine, copy: bool) -> Option<Drawing> {
    let mut d = ed.drawing.clone();
    let targets = if copy { d.copy(&ed.selection, Pt::default()) } else { ed.selection.clone() };
    d.transform(&targets, t);
    if copy {
        ed.selection = targets;
    }
    ed.points.clear();
    Some(d)
}

/// A number typed where a command takes a value rather than a point: Scale's factor, Rotate's
/// angle after the base, Offset's distance. The drawing when it completes the command.
fn typed_value(ed: &mut CadEditor, v: f64) -> Option<Option<Drawing>> {
    match (ed.tool, ed.points.as_slice()) {
        (Tool::Scale, [base]) if !ed.selection.is_empty() => {
            if v == 0.0 || !v.is_finite() {
                ed.message = "A scale factor is not 0".into();
                return Some(None);
            }
            let t = strix_cad::Affine::scaling(*base, v);
            Some(transformed(ed, &t, false))
        }
        (Tool::Rotate, [base]) if !ed.selection.is_empty() => {
            let t = strix_cad::Affine::rotation(*base, v);
            Some(transformed(ed, &t, false))
        }
        (Tool::Offset, []) => {
            if v > 0.0 && v.is_finite() {
                ed.offset_dist = v;
            } else {
                ed.message = "An offset distance is more than 0".into();
            }
            Some(None)
        }
        (Tool::ArrayPolar, []) if !ed.picking => {
            if v.fract() == 0.0 && (2.0..=strix_cad::MAX_ARRAY as f64).contains(&v) {
                ed.array_count = v as usize;
                ed.message = format!("{} {}", tl!("Items:"), ed.array_count);
            } else {
                ed.message = format!("{} 2–{}", tl!("A polar array has"), strix_cad::MAX_ARRAY);
            }
            Some(None)
        }
        (Tool::Fillet | Tool::Chamfer, []) => {
            if v >= 0.0 && v.is_finite() {
                *(if ed.tool == Tool::Fillet { &mut ed.fillet_radius } else { &mut ed.chamfer_dist }) = v;
            } else {
                ed.message = "A fillet radius or chamfer distance is 0 or more".into();
            }
            Some(None)
        }
        _ => None,
    }
}

/// Explode and Join act on the selection as soon as there is one (noun-verb, or after Enter
/// confirms the objects); then the editor is idle again.
fn immediate(ed: &mut CadEditor) -> Option<Drawing> {
    if !ed.tool.immediate() || ed.picking || ed.selection.is_empty() {
        return None;
    }
    let tool = ed.tool;
    let selection = ed.selection.clone();
    ed.set_tool(Tool::Select);
    let mut d = ed.drawing.clone();
    if tool == Tool::Explode {
        let made = d.explode(&selection);
        if made.is_empty() {
            ed.message = "Nothing to explode: select polylines or rectangles".into();
            return None;
        }
        ed.selection = made;
    } else {
        match d.join(&selection) {
            Ok(i) => ed.selection = vec![i],
            Err(e) => {
                ed.message = e;
                return None;
            }
        }
    }
    Some(d)
}

/// `4x3` (or `4×3`, `4*3`) typed for a rectangular array: its columns and rows.
fn typed_counts(ed: &mut CadEditor, typed: &str) -> bool {
    if ed.tool != Tool::ArrayRect || !ed.points.is_empty() || ed.picking {
        return false;
    }
    let Some((c, r)) = typed.split_once(['x', 'X', '×', '*']) else { return false };
    let (Ok(c), Ok(r)) = (c.trim().parse::<usize>(), r.trim().parse::<usize>()) else { return false };
    if c == 0 || r == 0 || c.saturating_mul(r) > strix_cad::MAX_ARRAY {
        ed.message = format!("{} 1–{}", tl!("An array has"), strix_cad::MAX_ARRAY);
    } else {
        (ed.array_cols, ed.array_rows) = (c, r);
        ed.message = format!("{} {c} × {r}", tl!("Columns × rows:"));
    }
    true
}

/// The drawing with `edit` done to a copy of it, or None with the reason it cannot be.
fn edited(ed: &mut CadEditor, edit: impl FnOnce(&mut Drawing) -> Result<(), String>) -> Option<Drawing> {
    let mut d = ed.drawing.clone();
    match edit(&mut d) {
        Ok(()) => {
            // The indices change: what was selected may be gone.
            ed.selection.clear();
            Some(d)
        }
        Err(e) => {
            ed.message = e;
            None
        }
    }
}

/// The line, circle, arc or polyline picked at `p` (within the pickbox), if any.
fn picked_path(ed: &CadEditor, p: Pt) -> Option<usize> {
    let tol = 8.0 / ed.zoom.max(1e-9);
    ed.drawing.nearest_path(p).filter(|&(_, d)| d <= tol).map(|(i, _)| i)
}

/// The next input is a size (a circle's radius or diameter): a number typed is that distance.
fn takes_size(ed: &CadEditor) -> bool {
    matches!((ed.tool, ed.points.len()), (Tool::Circle(CircleMode::Radius | CircleMode::Diameter), 1) | (Tool::Circle(CircleMode::TanTanRadius), 2))
}

/// The curve picked by a click at `p` for a tangent (the nearest one shown).
fn picked_curve(d: &Drawing, p: Pt) -> Result<kernel::Curve, &'static str> {
    kernel::curve_near(d, p).map(|(c, _)| c).ok_or("There is nothing to be tangent to")
}

/// The circle `mode` makes from the points given so far: None while it needs more of them. The
/// tangent modes read the objects nearest their picks; Tan, Tan, Radius takes the radius from
/// its third point (its distance from the second pick).
fn circle_of(d: &Drawing, mode: CircleMode, pts: &[Pt]) -> Result<Option<(Pt, f64)>, &'static str> {
    let made = match (mode, pts) {
        (CircleMode::Radius, [c, q, ..]) => Some((*c, q.minus(*c).len())),
        (CircleMode::Diameter, [c, q, ..]) => Some((*c, q.minus(*c).len() / 2.0)),
        (CircleMode::TwoPoints, [a, b, ..]) => Some(kernel::circle_2p(*a, *b).ok_or("The two points are the same")?),
        (CircleMode::ThreePoints, [a, b, c, ..]) => Some(kernel::circle_3p(*a, *b, *c).ok_or("The three points are in line")?),
        (CircleMode::TanTanRadius, [p1, p2, q, ..]) => {
            let (c1, c2) = (picked_curve(d, *p1)?, picked_curve(d, *p2)?);
            Some(kernel::circle_ttr(&c1, *p1, &c2, *p2, q.minus(*p2).len()).ok_or("No circle of that radius touches both")?)
        }
        (CircleMode::TanTanTan, [p1, p2, p3, ..]) => {
            let (c1, c2, c3) = (picked_curve(d, *p1)?, picked_curve(d, *p2)?, picked_curve(d, *p3)?);
            Some(kernel::circle_ttt([(&c1, *p1), (&c2, *p2), (&c3, *p3)]).ok_or("No circle touches the three objects")?)
        }
        _ => None,
    };
    match made {
        Some((_, r)) if !(r > kernel::RESOLUTION && r.is_finite()) => Err("The circle has no size"),
        other => Ok(other),
    }
}

/// The line touching the circles or arcs picked at `p1` and `p2`: its two touching points.
fn tangent_line(d: &Drawing, p1: Pt, p2: Pt) -> Result<(Pt, Pt), &'static str> {
    let (c1, c2) = (picked_curve(d, p1)?, picked_curve(d, p2)?);
    kernel::line_tan_tan(&c1, p1, &c2, p2).ok_or("Pick two circles or arcs apart from each other")
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
        Tool::LineTan if n >= 2 => {
            let made = tangent_line(&ed.drawing, pts[0], pts[1]);
            done(ed);
            match made {
                Ok((a, b)) => with(ed, Entity::new(Kind::Line { a: pt(a), b: pt(b) })),
                Err(e) => {
                    ed.message = e.into();
                    None
                }
            }
        }
        Tool::Circle(mode) => match circle_of(&ed.drawing, mode, &pts) {
            Ok(None) => None,
            Ok(Some((c, r))) => {
                done(ed);
                with(ed, Entity::new(Kind::Circle { c: pt(c), r: Num::new(r) }))
            }
            Err(e) => {
                done(ed);
                ed.message = e.into();
                None
            }
        },
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
            // In the drawing (scaled) or on paper (annotative), as the style places it.
            let (offset, offset_units) = if ed.drawing.dim_style.scaled { (0.0, offset_units) } else { (offset_units * k, 0.0) };
            with(ed, Entity::new(Kind::Dim { a: pt(a), b: pt(b), offset, offset_units, orient, text: None, below: None, mark: None }))
        }
        Tool::Hatch => {
            done(ed);
            let Some(boundary) = ed.drawing.hatch_boundary_at(pts[0]) else {
                ed.message = "No closed object around that point: draw a closed polyline, a rectangle or a circle".into();
                return None;
            };
            with(
                ed,
                Entity::new(Kind::Hatch {
                    boundary,
                    pattern: strix_cad::Pattern::Lines,
                    angle: 45.0,
                    spacing: (ed.drawing.dim_style.text * 0.8).max(0.5),
                }),
            )
        }
        Tool::Strip if n == 1 => {
            // The path first: something it can follow must be there.
            if ed.drawing.nearest_path(pts[0]).is_none() {
                done(ed);
                ed.message = "There is no line, polyline, circle or arc to follow".into();
            }
            None
        }
        Tool::Strip if n >= 2 => {
            let picked = ed.drawing.nearest_path(pts[0]).map(|(i, _)| i);
            done(ed);
            let i = picked?;
            let along = ed.drawing.entities.get(i)?.id.clone();
            let side = ed.drawing.side_of(i, pts[1]);
            // Sized like the drawing's annotations.
            let style = ed.drawing.dim_style;
            with(
                ed,
                Entity::new(Kind::Strip {
                    along,
                    width: (style.text * 1.2).max(1.0),
                    side,
                    pattern: strix_cad::Pattern::Lines,
                    angle: 45.0,
                    spacing: (style.text * 0.5).max(0.5),
                }),
            )
        }
        Tool::DimAngle if n >= 4 => {
            let (c, mut a, mut b, at) = (pts[0], pts[1], pts[2], pts[3]);
            // The angle measured is the side the arc is placed in.
            let (ua, ub, ut) = (a.minus(c), b.minus(c), at.minus(c));
            if kernel::angle_ccw(ua, ut) > kernel::angle_ccw(ua, ub) {
                std::mem::swap(&mut a, &mut b);
            }
            let r = ut.len();
            done(ed);
            if r <= kernel::RESOLUTION || ua.len() <= kernel::RESOLUTION || ub.len() <= kernel::RESOLUTION {
                ed.message = "An angle needs two sides and an arc away from the vertex".into();
                return None;
            }
            // The arc in the drawing (scaled) or on paper (annotative), as the style places it.
            let k = if ed.rendered.scale > 0.0 { ed.rendered.scale } else { 1.0 };
            let (r, r_paper) = if ed.drawing.dim_style.scaled { (r, 0.0) } else { (0.0, (r * k).min(500.0)) };
            with(ed, Entity::new(Kind::Angle { c: pt(c), a: pt(a), b: pt(b), r, r_paper, text: None, mark: None }))
        }
        Tool::DimContinue | Tool::DimBaseline => {
            done(ed);
            let Some(base) = ed.chain_base.filter(|&i| matches!(ed.drawing.entities.get(i).map(|e| &e.kind), Some(Kind::Dim { .. }))) else {
                ed.message = "Draw a linear dimension first, or select the one to go on from".into();
                return None;
            };
            let made = if ed.tool == Tool::DimContinue { ed.drawing.dim_continue(base, p) } else { ed.drawing.dim_baseline(base, p) };
            let Some(e) = made else {
                ed.message = "Pick a point away from where the dimension starts".into();
                return None;
            };
            // The next one goes on from this one.
            let next = ed.drawing.entities.len();
            let d = with(ed, e)?;
            ed.chain_base = Some(next);
            Some(d)
        }
        Tool::Level => {
            done(ed);
            // Like the last level mark: the same datum, side and way up.
            let like = ed.drawing.entities.iter().rev().find_map(|e| match &e.kind {
                Kind::Level { datum, flip, left, .. } => Some((datum.clone(), *flip, *left)),
                _ => None,
            });
            let (datum, flip, left) = like.unwrap_or_default();
            with(ed, Entity::new(Kind::Level { p: pt(p), datum, text: None, flip, left }))
        }
        Tool::Offset if n >= 2 => {
            // The object picked first, offset towards the second click; the command goes on.
            let made = ed.drawing.nearest_path(pts[0]).and_then(|(i, _)| ed.drawing.offset_entity(i, ed.offset_dist, pts[1]));
            done(ed);
            match made {
                Some(e) => with(ed, e),
                None => {
                    ed.message = "Pick a line, circle, arc or polyline, and a side it can be offset to".into();
                    None
                }
            }
        }
        Tool::Trim | Tool::Extend => {
            // Each pick edits one object; the command goes on.
            done(ed);
            let Some(i) = picked_path(ed, p) else {
                ed.message = "Pick a line, circle, arc or polyline".into();
                return None;
            };
            let trim = ed.tool == Tool::Trim;
            edited(ed, |d| if trim { d.trim(i, p) } else { d.extend(i, p) })
        }
        Tool::Fillet | Tool::Chamfer if n == 1 => {
            if picked_path(ed, p).is_none() {
                done(ed);
                ed.message = "Pick a line".into();
            }
            None
        }
        Tool::Fillet | Tool::Chamfer if n >= 2 => {
            done(ed);
            let (Some(i), Some(j)) = (picked_path(ed, pts[0]), picked_path(ed, pts[1])) else {
                ed.message = "Pick a line".into();
                return None;
            };
            let (r, dist, fillet) = (ed.fillet_radius, ed.chamfer_dist, ed.tool == Tool::Fillet);
            edited(ed, |d| if fillet { d.fillet(i, pts[0], j, pts[1], r) } else { d.chamfer(i, pts[0], j, pts[1], dist, dist) })
        }
        Tool::MatchProp => {
            let tol = 8.0 / ed.zoom.max(1e-9);
            let Some(i) = ed.drawing.hit(p, tol).filter(|&i| ed.drawing.selectable(i)) else {
                ed.points.pop();
                ed.message = "Pick an object".into();
                return None;
            };
            if n == 1 {
                return None;
            }
            // The source stays: the next pick takes its look too.
            ed.points = vec![pts[0]];
            let Some(src) = ed.drawing.hit(pts[0], tol) else {
                done(ed);
                return None;
            };
            let mut d = ed.drawing.clone();
            d.match_properties(src, &[i]);
            Some(d)
        }
        Tool::Stretch if n >= 4 => {
            done(ed);
            let mut d = ed.drawing.clone();
            if d.stretch(pts[0], pts[1], pts[3].minus(pts[2])) == 0 {
                ed.message = "Nothing to stretch: the window takes no points".into();
                return None;
            }
            Some(d)
        }
        Tool::Scale | Tool::Rotate | Tool::Mirror | Tool::ArrayRect | Tool::ArrayPolar if ed.selection.is_empty() => {
            done(ed);
            ed.picking = true;
            None
        }
        Tool::ArrayRect if n >= 2 => {
            done(ed);
            let d = pts[1].minus(pts[0]);
            let (cols, rows, sel) = (ed.array_cols, ed.array_rows, ed.selection.clone());
            edited(ed, |dr| dr.array_rect(&sel, cols, rows, d.x, d.y).map(|_| ()))
        }
        Tool::ArrayPolar => {
            done(ed);
            let (count, sel) = (ed.array_count, ed.selection.clone());
            edited(ed, |dr| dr.array_polar(&sel, p, count, 360.0).map(|_| ()))
        }
        Tool::Scale if n >= 2 => {
            let k = pts[1].minus(pts[0]).len();
            transformed(ed, &strix_cad::Affine::scaling(pts[0], k), false)
        }
        Tool::Rotate if n >= 2 => {
            let deg = kernel::deg(kernel::direction(pts[1].minus(pts[0])));
            transformed(ed, &strix_cad::Affine::rotation(pts[0], deg), false)
        }
        Tool::Mirror if n >= 2 => match strix_cad::Affine::mirror(pts[0], pts[1]) {
            Some(t) => transformed(ed, &t, true),
            None => {
                done(ed);
                ed.message = "The axis needs two different points".into();
                None
            }
        },
        Tool::Move | Tool::Copy if ed.selection.is_empty() => {
            // No objects yet: the command asks for them first.
            done(ed);
            ed.picking = true;
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
    // New texts take the drawing's annotation text height.
    let height = ed.drawing.dim_style.text;
    with(ed, Entity::new(Kind::Text { p: pt(p), value: text.to_string(), height, angle: 0.0, align: strix_cad::Align::Left }))
}

/// The drawing editor's tabs, in order: drawing and modifying (with the layers), annotations,
/// snaps and view, the picture in the document. While the editor is open the document's tabs are
/// disabled.
pub const TABS: [&str; 4] = ["CAD Drawing", "CAD Annotate", "CAD View", "CAD Output"];

/// The drawing editor's tab shown (`app.ui.tab`, one of [`TABS`]).
pub fn ribbon(app: &mut WordApp, ui: &mut Ui) {
    match app.ui.tab.as_str() {
        "CAD Annotate" => annotate_tab(app, ui),
        "CAD View" => view_tab(app, ui),
        "CAD Output" => output_tab(app, ui),
        _ => home_tab(app, ui),
    }
    crate::widgets::group(ui, "Close", None, app, |ui, app| {
        crate::widgets::big(ui, app, "cadClose", "Close\nDrawing", "ui.cadClose", json!({}), false);
    });
}

/// The width of a field in a ribbon form (a size, a choice), points.
pub(crate) const FIELD_W: f32 = 84.0;

/// Labels and fields in two columns, rows three high as ribbon groups are: every label lines up
/// with the others, every field starts at the same place.
pub(crate) fn field_grid(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui)) {
    egui::Grid::new(id).num_columns(2).spacing(vec2(6.0, 3.0)).min_row_height(20.0).show(ui, add);
}

/// Buttons stacked three high in a group.
fn stack(ui: &mut Ui, add: &mut dyn FnMut(&mut Ui)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(1.0, 0.0);
        add(ui);
    });
}

fn tool_of(app: &WordApp) -> Tool {
    app.cad.as_ref().map_or(Tool::Select, |e| e.tool)
}

/// Draw, Modify, Layers.
fn home_tab(app: &mut WordApp, ui: &mut Ui) {
    use crate::widgets::{big_checked, group, small};
    let tool = tool_of(app);
    let t = |name: &str| json!({"tool": name});
    group(ui, "Draw", None, app, |ui, app| {
        big_checked(ui, app, "cadLine", "Line", "ui.cadTool", t("line"), tool == Tool::Line);
        // Circle: its ways, as AutoCAD's Circle button lists them.
        crate::widgets::menu_button(
            ui,
            app,
            "cadCircle",
            Some("Circle"),
            "Circle: centre and radius, or another way",
            true,
            "ui.cadTool",
            |ui, app| {
                for (name, label) in [
                    ("circle", "Centre, Radius"),
                    ("circleDiameter", "Centre, Diameter"),
                    ("circle2p", "2 Points"),
                    ("circle3p", "3 Points"),
                    ("circleTtr", "Tan, Tan, Radius"),
                    ("circleTtt", "Tan, Tan, Tan"),
                ] {
                    let on = Tool::parse(name).is_some_and(|x| x == tool);
                    if ui.selectable_label(on, tl!(label)).clicked() {
                        let _ = app.run("ui.cadTool", json!({"tool": name}));
                        ui.close();
                    }
                }
            },
        );
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
            small(
                ui,
                app,
                "cadLine",
                Some("Tangent Line"),
                "A line touching two circles or arcs: pick each near where it should touch",
                "ui.cadTool",
                t("lineTan"),
                tool == Tool::LineTan,
            );
            small(ui, app, "cadPoint", Some("Point"), "Point", "ui.cadTool", t("point"), tool == Tool::Point);
            small(ui, app, "cadText", Some("Text"), "Text: a position, then type it", "ui.cadTool", t("text"), tool == Tool::Text);
        });
    });
    group(ui, "Modify", None, app, |ui, app| {
        big_checked(ui, app, "cadSelect", "Select", "ui.cadTool", t("select"), tool == Tool::Select);
        stack(ui, &mut |ui| {
            small(ui, app, "cadMove", Some("Move"), "Move the selection: base point, second point", "ui.cadTool", t("move"), tool == Tool::Move);
            small(ui, app, "cadCopy", Some("Copy"), "Copy the selection: base point, second point", "ui.cadTool", t("copy"), tool == Tool::Copy);
        });
        stack(ui, &mut |ui| {
            small(
                ui,
                app,
                "cadRotate",
                Some("Rotate"),
                "Rotate the selection: base point, then the angle",
                "ui.cadTool",
                t("rotate"),
                tool == Tool::Rotate,
            );
            small(
                ui,
                app,
                "cadScale",
                Some("Scale"),
                "Scale the selection: base point, then the factor",
                "ui.cadTool",
                t("scale"),
                tool == Tool::Scale,
            );
            small(
                ui,
                app,
                "cadMirror",
                Some("Mirror"),
                "A mirrored copy of the selection: two points of the axis",
                "ui.cadTool",
                t("mirror"),
                tool == Tool::Mirror,
            );
        });
        stack(ui, &mut |ui| {
            small(
                ui,
                app,
                "cadOffset",
                Some("Offset"),
                "Offset: the distance (type it), the object, the side",
                "ui.cadTool",
                t("offset"),
                tool == Tool::Offset,
            );
            small(ui, app, "cadTrim", Some("Trim"), "Trim: pick the pieces to take away", "ui.cadTool", t("trim"), tool == Tool::Trim);
            small(ui, app, "cadExtend", Some("Extend"), "Extend: pick near the ends to extend", "ui.cadTool", t("extend"), tool == Tool::Extend);
        });
        stack(ui, &mut |ui| {
            small(
                ui,
                app,
                "cadFillet",
                Some("Fillet"),
                "Fillet two lines with an arc (type the radius; 0 makes the corner)",
                "ui.cadTool",
                t("fillet"),
                tool == Tool::Fillet,
            );
            small(
                ui,
                app,
                "cadChamfer",
                Some("Chamfer"),
                "Chamfer two lines (type the distance; 0 makes the corner)",
                "ui.cadTool",
                t("chamfer"),
                tool == Tool::Chamfer,
            );
            small(ui, app, "cadDelete", Some("Delete"), "Delete the selection (Del)", "ui.cadDelete", json!({}), false);
        });
        stack(ui, &mut |ui| {
            small(
                ui,
                app,
                "cadStretch",
                Some("Stretch"),
                "Stretch: a crossing window, then a base point and a second point",
                "ui.cadTool",
                t("stretch"),
                tool == Tool::Stretch,
            );
            small(ui, app, "cadExplode", Some("Explode"), "Explode polylines into lines and arcs", "ui.cadTool", t("explode"), tool == Tool::Explode);
            small(ui, app, "cadJoin", Some("Join"), "Join objects that meet end to end into a polyline", "ui.cadTool", t("join"), tool == Tool::Join);
        });
        stack(ui, &mut |ui| {
            small(
                ui,
                app,
                "cadMatch",
                Some("Match"),
                "Match Properties: the source, then the objects that take its look",
                "ui.cadTool",
                t("matchProp"),
                tool == Tool::MatchProp,
            );
            small(
                ui,
                app,
                "cadArrayRect",
                Some("Array"),
                "Rectangular array of the selection (type columns x rows)",
                "ui.cadTool",
                t("arrayRect"),
                tool == Tool::ArrayRect,
            );
            small(
                ui,
                app,
                "cadArrayPolar",
                Some("Polar"),
                "Polar array of the selection round a centre (type how many items)",
                "ui.cadTool",
                t("arrayPolar"),
                tool == Tool::ArrayPolar,
            );
        });
        stack(ui, &mut |ui| {
            small(ui, app, "undo", Some("Undo"), "Undo", "edit.undo", json!({}), false);
            small(ui, app, "redo", Some("Redo"), "Redo", "edit.redo", json!({}), false);
        });
        stack(ui, &mut |ui| {
            let order = |o: &str| json!({"order": o});
            small(
                ui,
                app,
                "bringForward",
                Some("Bring to Front"),
                "Draw the selection above every other object",
                "ui.cadOrder",
                order("front"),
                false,
            );
            small(ui, app, "sendBackward", Some("Send to Back"), "Draw the selection under every other object", "ui.cadOrder", order("back"), false);
            small(
                ui,
                app,
                "cadSelect",
                Some("Select Similar"),
                "Add the objects of the same type on the same layer",
                "ui.cadSelect",
                json!({"similar": true}),
                false,
            );
        });
    });
    layers_group(app, ui);
}

/// Layers: the Layer Manager, Properties, the current layer.
fn layers_group(app: &mut WordApp, ui: &mut Ui) {
    use crate::widgets::{big_checked, group};
    let (panel, manager, layers, current) = app
        .cad
        .as_ref()
        .map(|e| (e.panel, e.layers_panel, e.drawing.layers.iter().map(|l| l.name.clone()).collect::<Vec<_>>(), e.current_layer.clone()))
        .unwrap_or_default();
    group(ui, "Layers", None, app, |ui, app| {
        big_checked(ui, app, "cadLayers", "Layer\nManager", "ui.cadToggle", json!({"what": "layers"}), manager);
        big_checked(ui, app, "properties", "Properties", "ui.cadToggle", json!({"what": "props"}), panel);
        ui.vertical(|ui| {
            ui.label(tl!("Current layer"));
            let mut pick = None;
            egui::ComboBox::from_id_salt("cad_current_layer").selected_text(&current).width(120.0).show_ui(ui, |ui| {
                for name in &layers {
                    if ui.selectable_label(*name == current, name).clicked() {
                        pick = Some(name.clone());
                    }
                }
            });
            if let Some(name) = pick {
                let _ = app.run("ui.cadCurrentLayer", json!({"layer": name}));
            }
        });
    });
}

/// Dimensions and texts.
fn annotate_tab(app: &mut WordApp, ui: &mut Ui) {
    use crate::widgets::{big_checked, group, small};
    let tool = tool_of(app);
    let t = |name: &str| json!({"tool": name});
    group(ui, "Dimensions", None, app, |ui, app| {
        big_checked(ui, app, "cadDim", "Aligned", "ui.cadTool", t("dimAligned"), tool == Tool::Dim(Orient::Aligned));
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
            small(
                ui,
                app,
                "cadArc",
                Some("Angular"),
                "Angular dimension: the vertex, a point on each side, where the arc goes",
                "ui.cadTool",
                t("dimAngular"),
                tool == Tool::DimAngle,
            );
        });
        stack(ui, &mut |ui| {
            small(
                ui,
                app,
                "cadDimCont",
                Some("Continue"),
                "Continue: dimensions in a chain, each from the last one's end, on its line",
                "ui.cadTool",
                t("dimContinue"),
                tool == Tool::DimContinue,
            );
            small(
                ui,
                app,
                "cadDimBase",
                Some("Baseline"),
                "Baseline: dimensions from the same first point, each a row further out",
                "ui.cadTool",
                t("dimBaseline"),
                tool == Tool::DimBaseline,
            );
            small(
                ui,
                app,
                "cadLevel",
                Some("Level"),
                "Level mark: the elevation of a point above the datum (±0.00), in metres",
                "ui.cadTool",
                t("level"),
                tool == Tool::Level,
            );
        });
        small(
            ui,
            app,
            "cadFlip",
            Some("Flip"),
            "Flip the selected dimensions: the line to the other side, the other side of an angle, a level mark under its level",
            "ui.cadFlip",
            json!({}),
            false,
        );
    });
    // The drawing's annotation style: one place for how every dimension and new text looks.
    let Some(style) = app.cad.as_ref().map(|e| e.drawing.dim_style) else { return };
    group(ui, "Dimension Style", None, app, |ui, app| {
        // Labels and fields in columns: each label in its column, each field the same width.
        let size = |ui: &mut Ui, app: &mut WordApp, label: &str, key: &str, now: f64| {
            ui.label(tl!(label));
            let mut v = now;
            let r = ui.add_sized([FIELD_W, 18.0], egui::DragValue::new(&mut v).range(0.0..=50.0).speed(0.05).max_decimals(2).suffix(" mm"));
            // Applied once the value is set: one undo step.
            if (r.drag_stopped() || r.lost_focus()) && v != now {
                let _ = app.run("ui.cadDimStyle", json!({ key: v }));
            }
            ui.end_row();
        };
        field_grid(ui, "cad_dim_style_a", |ui| {
            size(ui, app, "Text height", "text", style.text);
            size(ui, app, "Mark size", "markSize", style.mark_size);
            ui.label(tl!("End mark"));
            egui::ComboBox::from_id_salt("cad_dim_mark")
                .selected_text(tl!(crate::cad_panel::choice_label(style.mark.name())))
                .width(FIELD_W + 24.0)
                .show_ui(ui, |ui| {
                    for (m, name) in strix_cad::Mark::ALL {
                        if ui.selectable_label(style.mark == m, tl!(crate::cad_panel::choice_label(name))).clicked() {
                            let _ = app.run("ui.cadDimStyle", json!({"mark": name}));
                        }
                    }
                });
            ui.end_row();
        });
        ui.add_space(10.0);
        field_grid(ui, "cad_dim_style_b", |ui| {
            size(ui, app, "Extension gap", "extGap", style.ext_gap);
            size(ui, app, "Extension beyond", "extOver", style.ext_over);
            size(ui, app, "Text gap", "textGap", style.text_gap);
        });
        ui.add_space(10.0);
        field_grid(ui, "cad_dim_style_c", |ui| {
            size(ui, app, "Line width", "lineWidth", style.line_width);
            size(ui, app, "Line beyond", "lineOver", style.line_over);
        });
        ui.add_space(10.0);
        ui.vertical(|ui| {
            ui.label(tl!("Dimension units"));
            egui::ComboBox::from_id_salt("cad_dim_units").selected_text(tl!(style.units.label())).width(FIELD_W + 60.0).show_ui(ui, |ui| {
                for (u, name, label) in strix_cad::DimUnits::ALL {
                    if ui.selectable_label(style.units == u, tl!(label)).clicked() {
                        let _ = app.run("ui.cadDimStyle", json!({"units": name}));
                    }
                }
            });
        });
        ui.add_space(10.0);
        let mut aligned = style.aligned;
        if ui
            .checkbox(&mut aligned, tl!("Aligned texts"))
            .on_hover_text(tl!("Dimension texts along their line (never upside down), not level"))
            .changed()
        {
            let _ = app.run("ui.cadDimStyle", json!({"aligned": aligned}));
        }
        let mut scaled = style.scaled;
        if ui
            .checkbox(&mut scaled, tl!("Scaled distances"))
            .on_hover_text(tl!(
                "Dimension lines and angle arcs placed with a click or a grip keep their distance in the drawing (they follow its size and scale); off, on paper (the same in print)"
            ))
            .changed()
        {
            let _ = app.run("ui.cadDimStyle", json!({"scaled": scaled}));
        }
    });
    group(ui, "Hatch", None, app, |ui, app| {
        big_checked(ui, app, "cadHatch", "Hatch", "ui.cadTool", t("hatch"), tool == Tool::Hatch);
        big_checked(ui, app, "cadStrip", "Hatched\nStrip", "ui.cadTool", t("strip"), tool == Tool::Strip);
    });
    group(ui, "Text", None, app, |ui, app| {
        big_checked(ui, app, "cadText", "Text", "ui.cadTool", t("text"), tool == Tool::Text);
    });
    layers_group(app, ui);
}

/// Precision, object snaps, zoom and the panes.
fn view_tab(app: &mut WordApp, ui: &mut Ui) {
    use crate::widgets::{big, group, small};
    let Some((snap, ortho, grid, snaps)) = app.cad.as_ref().map(|e| (e.snap, e.ortho, e.grid, e.snaps)) else { return };
    group(ui, "Precision", None, app, |ui, app| {
        stack(ui, &mut |ui| {
            small(ui, app, "cadSnap", Some("Snap"), "Snap to ends, midpoints, centres and the grid", "ui.cadToggle", json!({"what": "snap"}), snap);
            small(ui, app, "cadOrtho", Some("Ortho"), "Hold lines level or plumb (F8)", "ui.cadToggle", json!({"what": "ortho"}), ortho);
            small(ui, app, "cadGrid", Some("Grid"), "Show the grid", "ui.cadToggle", json!({"what": "grid"}), grid);
        });
        big(ui, app, "cadFit", "Zoom\nFit", "ui.cadFit", json!({}), false);
    });
    group(ui, "Object Snaps", None, app, |ui, app| {
        let toggle = |what: &str| json!({"what": format!("snap.{what}")});
        stack(ui, &mut |ui| {
            small(ui, app, "cadSnapEnd", Some("Endpoint"), "Snap to ends", "ui.cadToggle", toggle("end"), snaps.end);
            small(ui, app, "cadSnapMid", Some("Midpoint"), "Snap to midpoints", "ui.cadToggle", toggle("mid"), snaps.mid);
            small(ui, app, "cadSnapCenter", Some("Centre"), "Snap to centres", "ui.cadToggle", toggle("center"), snaps.center);
        });
        stack(ui, &mut |ui| {
            small(
                ui,
                app,
                "cadSnapInt",
                Some("Intersection"),
                "Snap to intersections (exact)",
                "ui.cadToggle",
                toggle("intersection"),
                snaps.intersection,
            );
            small(
                ui,
                app,
                "cadSnapPerp",
                Some("Perpendicular"),
                "Snap to the foot of the perpendicular from the last point",
                "ui.cadToggle",
                toggle("perpendicular"),
                snaps.perpendicular,
            );
            small(
                ui,
                app,
                "cadSnapTan",
                Some("Tangent"),
                "Snap to tangent points from the last point",
                "ui.cadToggle",
                toggle("tangent"),
                snaps.tangent,
            );
        });
        stack(ui, &mut |ui| {
            small(ui, app, "cadSnapQuad", Some("Quadrant"), "Snap to quadrant points", "ui.cadToggle", toggle("quadrant"), snaps.quadrant);
            small(ui, app, "cadSnapNode", Some("Node"), "Snap to points", "ui.cadToggle", toggle("node"), snaps.node);
            small(
                ui,
                app,
                "cadSnapNear",
                Some("Nearest"),
                "Snap to the nearest point of an object",
                "ui.cadToggle",
                toggle("nearest"),
                snaps.nearest,
            );
        });
    });
    layers_group(app, ui);
}

/// The picture in the document: its frame and padding.
/// The scale and maximum size of drawing `d` in a ribbon group: the scale list offers Fitted to
/// Frame (with a maximum size) and the standard scales at which `drawn` (the drawing with its
/// sizes worked out) fits; the maximum size, on or off, its width and height. `command` applies a
/// change (`cad.scale` in the document, `ui.cadScale` in the editor; the same params).
pub(crate) fn scale_fields(ui: &mut Ui, app: &mut WordApp, id: &str, d: &Drawing, drawn: &Drawing, command: &str) {
    let fitted = d.fit.is_some_and(|f| !f.fixed);
    field_grid(ui, &format!("{id}_scale_grid"), |ui| {
        ui.label(tl!("Scale"));
        let shown = if fitted { format!("{} ({})", strix_cad::scale_text(d.scale), tl!("fitted to frame")) } else { strix_cad::scale_text(d.scale) };
        egui::ComboBox::from_id_salt(format!("{id}_scale")).selected_text(shown).width(FIELD_W + 24.0).show_ui(ui, |ui| {
            if d.fit.is_some() && ui.selectable_label(fitted, tl!("Fitted to Frame")).clicked() {
                let _ = app.run(command, json!({"auto": true}));
            }
            // Only the scales at which the drawing fits its maximum size (all without one).
            let mut probe = drawn.clone();
            probe.fit = d.fit;
            for n in probe.fitting_scales(None) {
                if ui.selectable_label(!fitted && d.scale == n, strix_cad::scale_text(n)).clicked() {
                    let _ = app.run(command, json!({"scale": n}));
                }
            }
        });
        ui.end_row();
        ui.label(tl!("Maximum size"));
        let mut on = d.fit.is_some();
        if ui
            .checkbox(&mut on, "")
            .on_hover_text(tl!("Limit the drawing's size on paper (from the page's text area): the scales offered are those at which it fits"))
            .changed()
        {
            let _ = app.run(command, json!({"max": on}));
        }
        ui.end_row();
        // The maximum size, mm (0: that side free); applied once set.
        let fit = d.fit.unwrap_or_default();
        for (label, key, now) in [("Width", "width", fit.width), ("Height", "height", fit.height)] {
            ui.label(tl!(label));
            let mut v = now;
            let r = ui
                .add_enabled_ui(d.fit.is_some(), |ui| {
                    ui.add_sized([FIELD_W + 32.0, 18.0], egui::DragValue::new(&mut v).range(0.0..=5000.0).speed(1.0).max_decimals(1).suffix(" mm"))
                })
                .inner;
            if (r.drag_stopped() || r.lost_focus()) && v != now {
                let mut space = json!({"width": fit.width, "height": fit.height});
                space[key] = json!(v);
                let _ = app.run(command, json!({"max": space}));
            }
            ui.end_row();
        }
    });
}

fn output_tab(app: &mut WordApp, ui: &mut Ui) {
    use crate::widgets::{big_checked, group};
    let Some((frame, padding)) = app.cad.as_ref().map(|e| (e.frame, e.drawing.padding)) else { return };
    if let Some(d) = app.cad.as_ref().map(|e| e.drawing.clone()) {
        group(ui, "Scale", None, app, |ui, app| scale_fields(ui, app, "cad_output", &d, &d, "ui.cadScale"));
    }
    group(ui, "Picture", None, app, |ui, app| {
        big_checked(ui, app, "cadFrame", "Frame", "ui.cadToggle", json!({"what": "frame"}), frame);
        field_grid(ui, "cad_output_grid", |ui| {
            ui.label(tl!("Padding"));
            let mut v = padding.top;
            let resp = ui.add_sized(
                [FIELD_W, 18.0],
                egui::DragValue::new(&mut v).range(0.0..=strix_cad::MAX_PADDING).speed(0.1).max_decimals(2).suffix(" mm"),
            );
            if (resp.drag_stopped() || resp.lost_focus()) && v != padding.top {
                let _ = app.run("ui.cadPadding", json!({"value": v}));
            }
            ui.end_row();
        });
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
        ed.tool = Tool::Circle(CircleMode::Radius);
        feed(ed, Pt::new(5.0, 5.0), None);
        let d = feed(ed, Pt::new(8.0, 9.0), None).unwrap();
        assert!(matches!(&d.entities.last().unwrap().kind, Kind::Circle { r, .. } if (r.value - 5.0).abs() < 1e-9));
        ed.tool = Tool::Dim(Orient::Horizontal);
        feed(ed, Pt::new(0.0, 0.0), None);
        feed(ed, Pt::new(30.0, 0.0), None);
        let d = feed(ed, Pt::new(15.0, 12.0), None).unwrap();
        assert!(matches!(&d.entities.last().unwrap().kind, Kind::Dim { offset, .. } if *offset > 0.0));
        // Move without a selection asks for objects first.
        ed.selection.clear();
        ed.set_tool(Tool::Move);
        assert!(ed.picking && ed.selecting());
    }

    /// A dimension line or an angle's arc placed with a click goes on paper (annotative) by
    /// default, in the drawing (scaled) with the style's Scaled distances.
    #[test]
    fn placed_distances_follow_the_style() {
        let mut app = app_with_drawing();
        app.run("ui.cadEdit", json!({})).unwrap();
        let place = |app: &mut WordApp| {
            let ed = app.cad.as_mut().unwrap();
            ed.set_tool(Tool::Dim(Orient::Horizontal));
            feed(ed, Pt::new(0.0, 0.0), None);
            feed(ed, Pt::new(30.0, 0.0), None);
            let dim = feed(ed, Pt::new(15.0, 12.0), None).unwrap().entities.last().unwrap().kind.clone();
            ed.set_tool(Tool::DimAngle);
            for p in [Pt::new(0.0, 0.0), Pt::new(10.0, 0.0), Pt::new(0.0, 10.0)] {
                feed(ed, p, None);
            }
            let angle = feed(ed, Pt::new(4.0, 4.0), None).unwrap().entities.last().unwrap().kind.clone();
            (dim, angle)
        };
        let (dim, angle) = place(&mut app);
        assert!(matches!(dim, Kind::Dim { offset, offset_units, .. } if offset > 0.0 && offset_units == 0.0), "{dim:?}");
        assert!(matches!(angle, Kind::Angle { r, r_paper, .. } if r == 0.0 && r_paper > 0.0), "{angle:?}");
        assert_eq!(app.run("ui.cadDimStyle", json!({"scaled": true})).unwrap()["scaled"], json!(true));
        let (dim, angle) = place(&mut app);
        assert!(matches!(dim, Kind::Dim { offset, offset_units, .. } if offset == 0.0 && (offset_units - 12.0).abs() < 1e-9), "{dim:?}");
        assert!(matches!(angle, Kind::Angle { r, r_paper, .. } if (r - 32f64.sqrt()).abs() < 1e-9 && r_paper == 0.0), "{angle:?}");
    }

    /// The level tool places a mark per click, each like the last one (datum, side, way up).
    #[test]
    fn level_marks_click_by_click() {
        let mut app = app_with_drawing();
        app.run("ui.cadEdit", json!({})).unwrap();
        app.run("ui.cadTool", json!({"tool": "quotaLivello"})).unwrap();
        let ed = app.cad.as_mut().unwrap();
        assert_eq!(ed.tool, Tool::Level);
        let d = feed(ed, Pt::new(0.0, 3000.0), None).expect("a level mark");
        commit(&mut app, &d);
        let mut d = app.cad.as_ref().unwrap().drawing.clone();
        let i = d.entities.len() - 1;
        d.set_field(i, 0, "datum", "500").unwrap();
        d.set_field(i, 0, "flip", "true").unwrap();
        commit(&mut app, &d);
        let ed = app.cad.as_mut().unwrap();
        let d = feed(ed, Pt::new(10.0, 0.0), None).expect("a second level mark");
        assert!(matches!(&d.entities.last().unwrap().kind, Kind::Level { datum, flip: true, left: false, .. } if datum.value == 500.0));
        assert_eq!(ed.tool, Tool::Level, "the command goes on");
        // Flip works on the selected annotations, one undo step; nothing to flip is an error.
        let d = feed(ed, Pt::new(20.0, 0.0), None).unwrap();
        commit(&mut app, &d);
        let ed = app.cad.as_mut().unwrap();
        let last = ed.drawing.entities.len() - 1;
        ed.selection = vec![last];
        assert_eq!(app.run("ui.cadFlip", json!({})).unwrap(), json!({"flipped": 1}));
        assert!(matches!(app.cad.as_ref().unwrap().drawing.entities[last].kind, Kind::Level { flip: false, .. }));
        app.cad.as_mut().unwrap().selection.clear();
        assert!(app.run("ui.cadFlip", json!({})).is_err());
    }

    /// Continue and Baseline go on from the newest linear dimension (or the one selected), one
    /// dimension per click, each the base of the next; with none to go on from they say so.
    #[test]
    fn continue_and_baseline_dimensions_chain_click_by_click() {
        let mut app = app_with_drawing();
        app.run("ui.cadEdit", json!({})).unwrap();
        let dims = |app: &WordApp| app.cad.as_ref().unwrap().drawing.entities.iter().filter(|e| matches!(e.kind, Kind::Dim { .. })).count();
        // Without a linear dimension there is nothing to go on from.
        let mut d = app.cad.as_ref().unwrap().drawing.clone();
        d.entities.retain(|e| !matches!(e.kind, Kind::Dim { .. }));
        commit(&mut app, &d);
        app.run("ui.cadTool", json!({"tool": "dco"})).unwrap();
        let ed = app.cad.as_mut().unwrap();
        assert_eq!(ed.tool, Tool::DimContinue);
        assert!(!ed.message.is_empty() && feed(ed, Pt::new(1.0, 1.0), None).is_none());
        // A horizontal dimension, then two continued ones and a baseline one.
        ed.set_tool(Tool::Dim(Orient::Horizontal));
        feed(ed, Pt::new(0.0, 0.0), None);
        feed(ed, Pt::new(30.0, 0.0), None);
        let d = feed(ed, Pt::new(15.0, 12.0), None).unwrap();
        commit(&mut app, &d);
        app.run("ui.cadTool", json!({"tool": "dimContinue"})).unwrap();
        for x in [50.0, 80.0] {
            let ed = app.cad.as_mut().unwrap();
            let d = feed(ed, Pt::new(x, -4.0), None).expect("a continued dimension");
            commit(&mut app, &d);
        }
        assert_eq!(dims(&app), 3);
        let ed = app.cad.as_ref().unwrap();
        let ends: Vec<(f64, f64)> = ed
            .drawing
            .entities
            .iter()
            .filter_map(|e| if let Kind::Dim { a, b, .. } = &e.kind { Some((a.0.value, b.0.value)) } else { None })
            .collect();
        assert_eq!(ends, vec![(0.0, 30.0), (30.0, 50.0), (50.0, 80.0)], "each from the last one's end");
        // Baseline from the first dimension, selected.
        let first = ed.drawing.entities.iter().position(|e| matches!(e.kind, Kind::Dim { .. })).unwrap();
        app.cad.as_mut().unwrap().selection = vec![first];
        app.run("ui.cadTool", json!({"tool": "quotaBase"})).unwrap();
        let ed = app.cad.as_mut().unwrap();
        assert_eq!(ed.chain_base, Some(first));
        let d = feed(ed, Pt::new(80.0, 0.0), None).expect("a baseline dimension");
        assert!(matches!(&d.entities.last().unwrap().kind, Kind::Dim { a, b, .. } if a.0.value == 0.0 && b.0.value == 80.0));
        commit(&mut app, &d);
        assert_eq!(dims(&app), 4);
    }

    #[test]
    fn selection_adds_removes_cycles_and_undoes_like_autocad() {
        let mut app = app_with_drawing();
        app.run("ui.cadEdit", json!({})).unwrap();
        let ed = app.cad.as_mut().unwrap();
        let n = ed.drawing.entities.len();
        // Two lines on top of each other.
        let mut d = ed.drawing.clone();
        d.add(Entity::new(Kind::Line { a: P::new(-100.0, -100.0), b: P::new(-50.0, -100.0) })).unwrap();
        d.add(Entity::new(Kind::Line { a: P::new(-100.0, -100.0), b: P::new(-50.0, -100.0) })).unwrap();
        commit(&mut app, &d);
        let ed = app.cad.as_mut().unwrap();
        let at = Pt::new(-75.0, -100.0);
        let screen = pos2(10.0, 10.0);
        // A click adds the topmost; a second click in the same place swaps it for the one under.
        assert!(ed.click_select(at, screen, 1.0, false));
        assert_eq!(ed.selection, [n + 1]);
        assert!(ed.click_select(at, screen, 1.0, false));
        assert_eq!(ed.selection, [n]);
        // Elsewhere, a click adds; Shift (remove) takes it out.
        ed.last_click = None;
        assert!(ed.click_select(at, pos2(50.0, 50.0), 1.0, false));
        assert_eq!(ed.selection.len(), 2);
        assert!(ed.click_select(at, pos2(90.0, 90.0), 1.0, true));
        assert_eq!(ed.selection.len(), 1);
        // Empty space: no pick.
        assert!(!ed.click_select(Pt::new(1e6, 1e6), screen, 1.0, false));
        // U undoes the steps; All, Previous, Remove/Add.
        ed.run_option(SelectOption::Undo);
        assert_eq!(ed.selection.len(), 2);
        ed.run_option(SelectOption::All);
        assert_eq!(ed.selection.len(), n + 2);
        ed.set_tool(Tool::Move);
        assert!(!ed.picking, "noun-verb: the selection is taken");
        assert_eq!(ed.previous.len(), n + 2);
        ed.set_tool(Tool::Select);
        ed.clear_selection();
        ed.run_option(SelectOption::Previous);
        assert_eq!(ed.selection.len(), n + 2);
        ed.run_option(SelectOption::Remove);
        assert!(ed.remove_mode);
        // A fence across both overlapping lines, then a polygon with too few points.
        ed.clear_selection();
        ed.remove_mode = false;
        ed.run_option(SelectOption::Fence);
        if let Some((_, pts)) = ed.poly.as_mut() {
            pts.extend([Pt::new(-75.0, -90.0), Pt::new(-75.0, -110.0)]);
        }
        assert!(ed.enter().is_none());
        assert_eq!(ed.selection, [n, n + 1]);
        ed.clear_selection();
        ed.run_option(SelectOption::CPolygon);
        if let Some((_, pts)) = ed.poly.as_mut() {
            pts.push(Pt::new(0.0, 0.0));
        }
        ed.enter();
        assert!(!ed.message.is_empty(), "a polygon needs three points");
        // Esc: the selection last, after commands and modes.
        ed.run_option(SelectOption::All);
        ed.set_tool(Tool::Line);
        assert!(ed.escape().is_none());
        assert_eq!(ed.tool, Tool::Select);
        assert!(!ed.selection.is_empty());
        ed.escape();
        assert!(ed.selection.is_empty());
        // Enter at the idle prompt repeats the last command.
        ed.enter();
        assert_eq!(ed.tool, Tool::Line);
    }

    #[test]
    fn menu_actions_order_layers_strokes_and_properties() {
        let mut app = app_with_drawing();
        app.run("ui.cadEdit", json!({})).unwrap();
        app.run("ui.cadSelect", json!({"ids": ["e1"]})).unwrap();
        // Draw order: the selection follows the object.
        apply(&mut app, Act::Order(Order::Front));
        let ed = app.cad.as_ref().unwrap();
        let last = ed.drawing.entities.len() - 1;
        assert_eq!(ed.drawing.entities[last].id, "e1");
        assert_eq!(ed.selection, [last]);
        // Layers: move the selection to a new one; the current layer takes new objects.
        app.run("ui.cadLayer", json!({"layer": "Axes"})).unwrap();
        assert_eq!(app.cad.as_ref().unwrap().drawing.entities[last].layer, "Axes");
        app.run("ui.cadCurrentLayer", json!({"layer": "Hidden lines"})).unwrap();
        let ed = app.cad.as_mut().unwrap();
        ed.tool = Tool::Point;
        let d = feed(ed, Pt::new(1.0, 1.0), None).unwrap();
        assert_eq!(d.entities.last().unwrap().layer, "Hidden lines");
        ed.tool = Tool::Select;
        // Stroke of the selection.
        app.run("ui.cadStroke", json!({"color": "#ff0000", "width": 0.5, "dash": "dashed"})).unwrap();
        let ed = app.cad.as_ref().unwrap();
        assert_eq!(ed.drawing.effective_stroke(last), Some(([255, 0, 0], 0.5, Dash::Dashed)));
        app.run("ui.cadStroke", json!({"color": "byLayer"})).unwrap();
        assert_eq!(app.cad.as_ref().unwrap().drawing.entities[last].stroke.color, None);
        assert!(app.run("ui.cadStroke", json!({"color": "red"})).is_err());
        // Properties: read the fields, set one (one undo step).
        let fields = app.run("ui.cadProps", json!({})).unwrap();
        assert!(fields["fields"].as_array().is_some_and(|f| !f.is_empty()), "{fields}");
        let key = fields["fields"][0]["field"].as_str().unwrap().to_string();
        app.run("ui.cadProps", json!({"field": key, "value": 12.5})).unwrap();
        let shown = app.run("ui.cadProps", json!({})).unwrap();
        assert_eq!(shown["fields"][0]["value"], "12.5");
        assert!(app.run("ui.cadProps", json!({"field": "nope", "value": 1})).is_err());
        // Select Similar, Deselect, snap for the next point.
        apply(&mut app, Act::Similar);
        apply(&mut app, Act::Deselect);
        assert!(app.cad.as_ref().unwrap().selection.is_empty());
        assert!(app.run("ui.cadProps", json!({})).is_err(), "one object");
        app.run("ui.cadSnapOnce", json!({"snap": "mid"})).unwrap();
        assert_eq!(app.cad.as_ref().unwrap().snap_once, Some(Some(SnapKind::Mid)));
        assert!(app.run("ui.cadSnapOnce", json!({"snap": "x"})).is_err());
        assert!(app.run("ui.cadOrder", json!({"order": "up"})).is_err());
        app.run("ui.cadToggle", json!({"what": "props"})).unwrap();
        assert!(!app.cad.as_ref().unwrap().panel);
        // Delete all.
        app.run("ui.cadSelect", json!({"all": true})).unwrap();
        apply(&mut app, Act::Delete);
        let ed = app.cad.as_ref().unwrap();
        assert!(ed.drawing.entities.is_empty());
        assert!(ed.selection.is_empty());
    }

    #[test]
    fn hostile_cad_params_never_panic() {
        let mut app = app_with_drawing();
        app.run("ui.cadEdit", json!({})).unwrap();
        let junk = [
            json!(null),
            json!({}),
            json!({"layer": ""}),
            json!({"layer": "x".repeat(500)}),
            json!({"field": 1, "value": [1]}),
            json!({"vertex": u64::MAX, "field": "vx", "value": "1"}),
            json!({"color": "#zzzzzz", "width": "x", "dash": 3}),
            json!({"option": "zz"}),
            json!({"ids": "e1"}),
            json!({"order": 7}),
            json!({"snap": null}),
        ];
        for id in [
            "ui.cadLayer",
            "ui.cadCurrentLayer",
            "ui.cadStroke",
            "ui.cadProps",
            "ui.cadSelect",
            "ui.cadOrder",
            "ui.cadSnapOnce",
            "ui.cadToggle",
            "ui.cadTool",
        ] {
            for p in &junk {
                let _ = app.run(id, p.clone());
            }
            app.run("ui.cadSelect", json!({"all": true})).unwrap();
            for p in &junk {
                let _ = app.run(id, p.clone());
            }
        }
        assert!(app.cad.is_some());
    }

    #[test]
    fn frame_snaps_and_padding_are_editor_commands() {
        let mut app = app_with_drawing();
        app.run("ui.cadEdit", json!({})).unwrap();
        app.run("ui.cadToggle", json!({"what": "frame"})).unwrap();
        app.run("ui.cadToggle", json!({"what": "snap.quadrant"})).unwrap();
        let ed = app.cad.as_ref().unwrap();
        assert!(ed.frame && ed.snaps.quadrant);
        assert!(app.run("ui.cadToggle", json!({"what": "nope"})).is_err());
        // The padding is the drawing's: stored in the document, one undo step.
        let before = app.cad.as_ref().unwrap().rendered.size;
        app.run("ui.cadPadding", json!({"value": 10})).unwrap();
        let ed = app.cad.as_ref().unwrap();
        assert_eq!(ed.drawing.padding, strix_cad::Padding::all(10.0));
        assert!((ed.rendered.size.0 - before.0 - 16.0).abs() < 1e-6, "{:?} {before:?}", ed.rendered.size);
        app.run("ui.cadPadding", json!({"value": [1, 2, 3, 4]})).unwrap();
        assert_eq!(app.cad.as_ref().unwrap().drawing.padding.left, 4.0);
        assert!(app.run("ui.cadPadding", json!({"value": "x"})).is_err());
        app.run("ui.cadPadding", json!({"value": 1e9})).unwrap();
        assert_eq!(app.cad.as_ref().unwrap().drawing.padding.top, strix_cad::MAX_PADDING);
        app.run("edit.undo", json!({})).unwrap();
        reload(&mut app);
        assert_eq!(app.cad.as_ref().unwrap().drawing.padding.left, 4.0);
    }

    /// The Circle ways and the tangent line, through the same steps as clicks.
    #[test]
    fn circle_ways_and_the_tangent_line() {
        let mut app = app_with_drawing();
        app.run("ui.cadEdit", json!({})).unwrap();
        let radius = |d: &Drawing| match &d.entities.last().unwrap().kind {
            Kind::Circle { c, r } => (c.pt(), r.value),
            k => panic!("{k:?}"),
        };
        let ed = app.cad.as_mut().unwrap();
        ed.set_tool(Tool::parse("3p").unwrap());
        feed(ed, Pt::new(200.0, 0.0), None);
        feed(ed, Pt::new(210.0, 10.0), None);
        let d = feed(ed, Pt::new(220.0, 0.0), None).expect("three points");
        let (c, r) = radius(&d);
        assert!((c.x - 210.0).abs() < 1e-9 && c.y.abs() < 1e-9 && (r - 10.0).abs() < 1e-9);
        ed.drawing = d;
        // Two circles side by side, then the line over their tops.
        ed.set_tool(Tool::parse("2p").unwrap());
        feed(ed, Pt::new(240.0, 0.0), None);
        ed.drawing = feed(ed, Pt::new(260.0, 0.0), None).expect("two points");
        ed.set_tool(Tool::LineTan);
        feed(ed, Pt::new(210.0, 10.0), None);
        let d = feed(ed, Pt::new(250.0, 10.0), None).expect("tangent line");
        let Kind::Line { a, b } = &d.entities.last().unwrap().kind else { panic!() };
        assert!((a.1.value - 10.0).abs() < 1e-9 && (b.1.value - 10.0).abs() < 1e-9, "{a:?} {b:?}");
        // Tan, tan, radius: a typed radius is the third input.
        ed.drawing = d;
        ed.set_tool(Tool::parse("ttr").unwrap());
        feed(ed, Pt::new(210.0, -10.0), None);
        feed(ed, Pt::new(250.0, -10.0), None);
        assert!(takes_size(ed));
        let d = feed(ed, Pt::new(250.0 + 30.0, -10.0), None).expect("ttr");
        assert!((radius(&d).1 - 30.0).abs() < 1e-9);
        // Nothing to touch: a message, no circle.
        ed.drawing = Drawing::default();
        ed.set_tool(Tool::parse("ttt").unwrap());
        for p in [Pt::new(0.0, 0.0), Pt::new(1.0, 0.0)] {
            feed(ed, p, None);
        }
        assert!(feed(ed, Pt::new(0.0, 1.0), None).is_none() && !ed.message.is_empty());
    }

    /// The angular dimension measures the side its arc is placed in; the annotation style is
    /// one undo step and clamps what it is given.
    #[test]
    fn angular_dimensions_and_the_dimension_style() {
        let mut app = app_with_drawing();
        app.run("ui.cadEdit", json!({})).unwrap();
        let ed = app.cad.as_mut().unwrap();
        ed.set_tool(Tool::parse("dimAngular").unwrap());
        for p in [Pt::new(0.0, 0.0), Pt::new(10.0, 0.0), Pt::new(0.0, 10.0)] {
            assert!(feed(ed, p, None).is_none());
        }
        // The arc placed in the other sector: 270°, the rays swapped.
        let d = feed(ed, Pt::new(-5.0, -5.0), None).expect("angular dimension");
        let Kind::Angle { a, b, .. } = &d.entities.last().unwrap().kind else { panic!() };
        assert_eq!((a.pt(), b.pt()), (Pt::new(0.0, 10.0), Pt::new(10.0, 0.0)));
        app.run("ui.cadDimStyle", json!({"text": 3.5, "mark": "arrow"})).unwrap();
        app.run("ui.cadDimStyle", json!({"aligned": true})).unwrap();
        assert!(app.cad.as_ref().unwrap().drawing.dim_style.aligned);
        assert!(app.run("ui.cadDimStyle", json!({"aligned": 3})).is_err());
        assert_eq!(parse_dash("dashDoubleDotted"), Some(Dash::DashDoubleDotted));
        let st = app.cad.as_ref().unwrap().drawing.dim_style;
        assert_eq!((st.text, st.mark), (3.5, strix_cad::Mark::Arrow));
        assert!(app.run("ui.cadDimStyle", json!({"mark": "star"})).is_err());
        app.run("ui.cadDimStyle", json!({"markSize": -4})).unwrap();
        assert_eq!(app.cad.as_ref().unwrap().drawing.dim_style.mark_size, 0.0);
        app.run("edit.undo", json!({})).unwrap();
        reload(&mut app);
        assert_eq!(app.cad.as_ref().unwrap().drawing.dim_style.mark_size, 2.5);
    }

    /// Hatched strip: a click on the line it follows, a click on its side.
    #[test]
    fn a_hatched_strip_along_a_line() {
        let mut app = app_with_drawing();
        app.run("ui.cadEdit", json!({})).unwrap();
        let ed = app.cad.as_mut().unwrap();
        ed.set_tool(Tool::parse("strip").unwrap());
        // The starter rectangle's bottom edge (y = 0), the strip below it (outside).
        assert!(feed(ed, Pt::new(60.0, 0.5), None).is_none());
        let d = feed(ed, Pt::new(60.0, -10.0), None).expect("a strip");
        // Hatches go behind everything else: the strip is drawn first.
        let Some(Kind::Strip { along, side, .. }) = d.entities.first().map(|e| &e.kind) else { panic!() };
        // The rectangle runs counter-clockwise: below its bottom edge is the edge's right.
        assert_eq!((along.as_str(), *side), ("e1", strix_cad::Side::Right));
    }

    /// The editor's settings outlive the drawing: the next one opens as the last was left.
    #[test]
    fn settings_are_kept_while_the_program_runs() {
        let mut app = app_with_drawing();
        app.run("ui.cadEdit", json!({})).unwrap();
        for what in ["ortho", "grid", "layers", "props", "snap.quadrant", "layers.float"] {
            app.run("ui.cadToggle", json!({"what": what})).unwrap();
        }
        app.run("ui.cadClose", json!({})).unwrap();
        app.run("ui.cadEdit", json!({})).unwrap();
        let ed = app.cad.as_ref().unwrap();
        assert!(ed.ortho && !ed.grid && ed.layers_panel && !ed.panel && ed.snaps.quadrant);
        // A detached pane opens detached again.
        assert!(ed.layers_float && !ed.panel_float);
    }

    /// Hatch: a click inside the region; it goes behind everything else.
    #[test]
    fn a_hatch_fills_the_region_clicked() {
        let mut app = app_with_drawing();
        app.run("ui.cadEdit", json!({})).unwrap();
        let ed = app.cad.as_mut().unwrap();
        ed.set_tool(Tool::parse("hatch").unwrap());
        let d = feed(ed, Pt::new(60.0, 30.0), None).expect("a hatch");
        assert!(matches!(&d.entities[0].kind, Kind::Hatch { boundary, .. } if boundary == &["e1".to_string()]));
        // Outside every closed object: a message, nothing made.
        ed.set_tool(Tool::Hatch);
        assert!(feed(ed, Pt::new(-500.0, -500.0), None).is_none() && !ed.message.is_empty());
    }

    /// Scale, Rotate and Mirror work on the selection (typed values or points); Offset keeps its
    /// distance and goes on.
    #[test]
    fn scale_rotate_mirror_and_offset_in_the_editor() {
        let mut app = app_with_drawing();
        app.run("ui.cadEdit", json!({})).unwrap();
        let ed = app.cad.as_mut().unwrap();
        // The starter rectangle alone, scaled by 2 about its corner.
        ed.selection = vec![0];
        ed.set_tool(Tool::parse("sc").unwrap());
        assert!(feed(ed, Pt::new(0.0, 0.0), None).is_none());
        let d = typed_value(ed, 2.0).unwrap().expect("scaled");
        assert!(matches!(&d.entities[0].kind, Kind::Polyline { pts, .. } if pts[2].pt() == Pt::new(240.0, 120.0)));
        ed.drawing = d;
        // Rotated a quarter turn by a point straight up.
        ed.selection = vec![0];
        ed.set_tool(Tool::Rotate);
        feed(ed, Pt::new(0.0, 0.0), None);
        let d = feed(ed, Pt::new(0.0, 5.0), None).expect("rotated");
        assert!(matches!(&d.entities[0].kind, Kind::Polyline { pts, .. } if pts[1].pt().minus(Pt::new(0.0, 240.0)).len() < 1e-9));
        ed.drawing = d;
        // Mirrored: a copy.
        let before = ed.drawing.entities.len();
        ed.selection = vec![0];
        ed.set_tool(Tool::Mirror);
        feed(ed, Pt::new(10.0, 0.0), None);
        let d = feed(ed, Pt::new(10.0, 10.0), None).expect("mirrored");
        assert_eq!(d.entities.len(), before + 1);
        // Mirror with nothing selected asks for objects first.
        ed.selection.clear();
        ed.set_tool(Tool::Mirror);
        assert!(ed.picking);
        // Offset: the distance typed, then the object and the side; kept for the next drawing.
        ed.drawing = d;
        ed.set_tool(Tool::Offset);
        assert!(typed_value(ed, 5.0).is_some());
        feed(ed, Pt::new(-120.0, 100.0), None);
        let d = feed(ed, Pt::new(-200.0, 100.0), None).expect("offset");
        assert_eq!(d.entities.len(), before + 2);
        assert_eq!(ed.tool, Tool::Offset, "it goes on");
        app.run("ui.cadClose", json!({})).unwrap();
        assert_eq!(app.cad_settings.offset_dist, 5.0);
    }

    /// Trim and Extend edit the object picked and go on; Fillet and Chamfer take a typed size
    /// and two lines, kept for the next drawing; a pick on nothing says so.
    #[test]
    fn trim_extend_fillet_and_chamfer_in_the_editor() {
        let mut app = app_with_drawing();
        app.run("ui.cadEdit", json!({})).unwrap();
        let ed = app.cad.as_mut().unwrap();
        let cross = r#"{"entities":[{"type":"line","a":[0,0],"b":[10,0]},{"type":"line","a":[5,-5],"b":[5,5]},
            {"type":"line","a":[20,-5],"b":[20,5]}]}"#;
        ed.drawing = Drawing::from_json(cross).unwrap();
        ed.zoom = 10.0;
        assert!(!Tool::Select.picks_object());
        ed.set_tool(Tool::parse("tr").unwrap());
        assert!(ed.tool.picks_object());
        let d = feed(ed, Pt::new(8.0, 0.1), None).expect("trimmed");
        assert!(matches!(&d.entities[0].kind, Kind::Line { b, .. } if b.pt() == Pt::new(5.0, 0.0)));
        assert_eq!(ed.tool, Tool::Trim, "it goes on");
        ed.drawing = d;
        assert!(feed(ed, Pt::new(50.0, 50.0), None).is_none() && !ed.message.is_empty(), "nothing there");
        ed.set_tool(Tool::parse("ex").unwrap());
        let d = feed(ed, Pt::new(4.5, 0.0), None).expect("extended");
        assert!(matches!(&d.entities[0].kind, Kind::Line { b, .. } if b.pt() == Pt::new(20.0, 0.0)));
        ed.drawing = d;
        // Fillet with radius 2 typed: the two lines and an arc.
        ed.set_tool(Tool::parse("f").unwrap());
        assert!(typed_value(ed, 2.0).is_some());
        assert!(feed(ed, Pt::new(1.0, 0.0), None).is_none());
        let d = feed(ed, Pt::new(5.0, 4.0), None).expect("filleted");
        assert!(d.entities.iter().any(|e| matches!(&e.kind, Kind::Arc { r, .. } if r.value == 2.0)));
        // Chamfer 1 on a fresh corner.
        ed.drawing = Drawing::from_json(cross).unwrap();
        ed.set_tool(Tool::Chamfer);
        typed_value(ed, 1.0);
        feed(ed, Pt::new(1.0, 0.0), None);
        let d = feed(ed, Pt::new(5.0, 4.0), None).expect("chamfered");
        assert_eq!(d.entities.len(), 4);
        // Parallel lines: a message, nothing changed.
        ed.drawing = Drawing::from_json(r#"{"entities":[{"type":"line","a":[0,0],"b":[10,0]},{"type":"line","a":[0,1],"b":[10,1]}]}"#).unwrap();
        feed(ed, Pt::new(1.0, 0.0), None);
        assert!(feed(ed, Pt::new(1.0, 1.0), None).is_none() && !ed.message.is_empty());
        app.run("ui.cadClose", json!({})).unwrap();
        assert_eq!((app.cad_settings.fillet_radius, app.cad_settings.chamfer_dist), (2.0, 1.0));
    }

    /// Explode and Join act on the selection at once; Match Properties goes on from its
    /// source; Stretch takes a window and two points; the arrays take typed counts.
    #[test]
    fn explode_join_match_stretch_and_arrays_in_the_editor() {
        let mut app = app_with_drawing();
        app.run("ui.cadEdit", json!({})).unwrap();
        let ed = app.cad.as_mut().unwrap();
        let rect = r#"{"layers":[{"name":"0"},{"name":"A"}],"entities":[
            {"type":"polyline","pts":[[0,0],[10,0],[10,5],[0,5]],"closed":true},
            {"type":"line","layer":"A","a":[20,0],"b":[30,0]}]}"#;
        ed.drawing = Drawing::from_json(rect).unwrap();
        ed.zoom = 10.0;
        // Explode, noun-verb: four lines selected.
        ed.selection = vec![0];
        ed.set_tool(Tool::parse("x").unwrap());
        let d = immediate(ed).expect("exploded");
        assert_eq!((d.entities.len(), ed.selection.len(), ed.tool), (5, 4, Tool::Select));
        ed.drawing = d;
        // Join, verb-noun: pick, Enter, joined back into one closed polyline.
        ed.selection.clear();
        ed.set_tool(Tool::Join);
        assert!(ed.picking && immediate(ed).is_none());
        ed.selection = vec![0, 1, 2, 3];
        ed.enter();
        let d = immediate(ed).expect("joined");
        assert!(matches!(&d.entities[0].kind, Kind::Polyline { closed: true, .. }));
        ed.drawing = d;
        // Match Properties: the line on layer A gives the rectangle its layer.
        ed.set_tool(Tool::parse("ma").unwrap());
        assert!(feed(ed, Pt::new(25.0, 0.0), None).is_none());
        let d = feed(ed, Pt::new(5.0, 0.0), None).expect("matched");
        assert_eq!(d.entities[0].layer, "A");
        assert_eq!(ed.points.len(), 1, "it goes on from the source");
        ed.drawing = d;
        // Stretch the rectangle's right side by 5.
        ed.set_tool(Tool::parse("s").unwrap());
        for q in [(8.0, -1.0), (12.0, 6.0), (0.0, 0.0)] {
            assert!(feed(ed, Pt::new(q.0, q.1), None).is_none());
        }
        let d = feed(ed, Pt::new(5.0, 0.0), None).expect("stretched");
        assert!(matches!(&d.entities[0].kind, Kind::Polyline { pts, .. } if pts[1].pt() == Pt::new(15.0, 0.0)));
        ed.drawing = d;
        // A 2×2 rectangular array of the line, then a polar one of 4.
        ed.selection = vec![1];
        ed.set_tool(Tool::parse("ar").unwrap());
        assert!(typed_counts(ed, "2x2"));
        assert_eq!((ed.array_cols, ed.array_rows), (2, 2));
        feed(ed, Pt::new(0.0, 0.0), None);
        let d = feed(ed, Pt::new(0.0, 10.0), None).expect("rectangular array");
        assert_eq!(d.entities.len(), 5);
        ed.drawing = d;
        ed.selection = vec![1];
        ed.set_tool(Tool::ArrayPolar);
        assert!(typed_value(ed, 4.0).is_some());
        let d = feed(ed, Pt::new(0.0, 0.0), None).expect("polar array");
        assert_eq!(d.entities.len(), 8);
        assert!(typed_value(ed, 1.0).is_some() && ed.array_count == 4, "too few items: kept");
        app.run("ui.cadClose", json!({})).unwrap();
        assert_eq!((app.cad_settings.array_cols, app.cad_settings.array_count), (2, 4));
    }

    /// CAD Output › Scale: a maximum size from the page, the scale fitted to it, a scale picked
    /// among those that fit, one that does not refused, no limit again.
    #[test]
    fn the_editor_sets_the_maximum_size_and_scale() {
        let mut app = app_with_drawing();
        app.run("ui.cadEdit", json!({})).unwrap();
        let r = app.run("ui.cadScale", json!({"max": {"width": 60}})).unwrap();
        // The 120 mm starter rectangle with its dimensions in 60 mm: the first scale that fits.
        let d = app.cad.as_ref().unwrap().drawing.clone();
        let offered = d.fitting_scales(None);
        let first = offered[0];
        assert_eq!(r["scale"], json!(first), "{r}");
        assert!(first > 2.0 && !offered.contains(&1.0));
        app.run("ui.cadScale", json!({"scale": offered[1]})).unwrap();
        assert!(app.cad.as_ref().unwrap().drawing.fit.unwrap().fixed);
        assert!(app.run("ui.cadScale", json!({"scale": 1})).is_err());
        app.run("ui.cadScale", json!({"auto": true})).unwrap();
        assert_eq!(app.cad.as_ref().unwrap().drawing.scale, first);
        app.run("ui.cadScale", json!({"max": false})).unwrap();
        assert!(app.cad.as_ref().unwrap().drawing.fit.is_none());
        app.run("ui.cadScale", json!({"scale": 1})).unwrap();
        assert!(app.run("ui.cadScale", json!({"max": "x"})).is_err());
    }

    /// While a drawing is open only its own tabs (and File) can be chosen; closing it opens the
    /// document's tabs again. The Layer Manager toggles.
    #[test]
    fn the_editor_has_its_own_tabs_and_disables_the_others() {
        let mut app = app_with_drawing();
        app.run("ui.cadEdit", json!({})).unwrap();
        for tab in TABS {
            app.run("ui.tab", json!({"tab": tab})).unwrap();
            assert_eq!(app.ui.tab, tab);
        }
        assert!(app.run("ui.tab", json!({"tab": "Home"})).is_err(), "the document's tabs are off");
        assert_eq!(app.ui.tab, "CAD Output");
        app.run("ui.cadToggle", json!({"what": "layers"})).unwrap();
        assert!(app.cad.as_ref().unwrap().layers_panel);
        app.run("ui.cadClose", json!({})).unwrap();
        app.run("ui.tab", json!({"tab": "Home"})).unwrap();
        assert_eq!(app.ui.tab, "Home");
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
