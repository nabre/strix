//! Blocks in the CAD editor (docs/cad.md, Blocks): Create Block (the ribbon's Block group, the
//! selection's context menu, `B` at the command line) asks for a name and whether the block is
//! annotative, then for its base point; Insert Block (`I`) lists the blocks, proposes the scale
//! that keeps an annotative block as large on paper as it was drawn (`N / N₀`), then asks for
//! the insertion point. Every change runs `ui.cadBlock`, so scripts and the MCP do the same
//! without a window.

use egui::{Color32, Pos2, Shape, Stroke, Ui};
use serde_json::{Value, json};
use strix_cad::{Drawing, Pt};

use super::{CadEditor, FIELD_W, View, commit, field_grid};
use crate::WordApp;

/// Where a new block's base point is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Base {
    /// Clicked in the drawing after the window closes.
    #[default]
    Pick,
    /// The middle of the selection's box.
    Centre,
    /// The selection's box's lower left corner.
    LowerLeft,
}

/// The block window open.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Dialog {
    Create {
        name: String,
        annotative: bool,
        replace: bool,
        base: Base,
    },
    /// `scale`: the scale typed (proposed from the block), `angle` degrees.
    Insert {
        name: String,
        scale: String,
        angle: String,
    },
}

/// A point the editor waits for.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Pending {
    Create { name: String, annotative: bool, replace: bool, selection: Vec<usize> },
    Insert { name: String, factor: f64, angle: f64 },
}

/// The editor's block state: the window open, the point waited for.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct BlockState {
    pub dialog: Option<Dialog>,
    pub pick: Option<Pending>,
}

/// A number as typed (a comma as the decimal point too), with a ratio allowed: `2`, `0,5`, `1:2`.
fn number(s: &str) -> Option<f64> {
    let s = s.trim().replace(',', ".");
    let v = match s.split_once([':', '/']) {
        Some((a, b)) => a.trim().parse::<f64>().ok()? / b.trim().parse::<f64>().ok()?,
        None => s.parse::<f64>().ok()?,
    };
    v.is_finite().then_some(v)
}

/// A factor as it is shown: up to four decimals, no trailing zeros.
fn shown(v: f64) -> String {
    let s = format!("{:.4}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" { "0".into() } else { s.to_string() }
}

/// A new block's name: `Block 1`, `Block 2`… one the drawing has not.
fn fresh_name(d: &Drawing) -> String {
    let mut n = 1usize;
    loop {
        let name = format!("{} {n}", tl!("Block"));
        if d.block(&name).is_none() {
            return name;
        }
        n += 1;
    }
}

/// Opens the Create Block window (`insert`: the Insert Block one).
pub(super) fn open_dialog(ed: &mut CadEditor, insert: bool) {
    ed.block.pick = None;
    if insert {
        let Some(first) = ed.drawing.blocks.first().map(|b| b.name.clone()) else {
            ed.message = tl!("No blocks yet: select objects and use Create Block").to_string();
            return;
        };
        let scale = ed.drawing.proposed_factor(&first).unwrap_or(1.0);
        ed.block.dialog = Some(Dialog::Insert { name: first, scale: shown(scale), angle: "0".into() });
    } else {
        if ed.selection.is_empty() {
            ed.message = tl!("Create Block: select the objects first").to_string();
            return;
        }
        ed.block.dialog = Some(Dialog::Create { name: fresh_name(&ed.drawing), annotative: true, replace: true, base: Base::Pick });
    }
}

/// `B`/`block` or `I`/`insert` typed at the idle prompt. True when it was one.
pub(super) fn typed(ed: &mut CadEditor, typed: &str) -> bool {
    if ed.tool != super::Tool::Select || !ed.points.is_empty() {
        return false;
    }
    match typed.trim().to_lowercase().as_str() {
        "b" | "block" | "blocco" => open_dialog(ed, false),
        "i" | "insert" | "inserisci" => open_dialog(ed, true),
        _ => return false,
    }
    true
}

/// The prompt while a block's point is waited for.
pub(super) fn prompt(ed: &CadEditor) -> Option<String> {
    Some(match ed.block.pick.as_ref()? {
        Pending::Create { name, .. } => format!("{} {name}: {}", tl!("Create Block"), tl!("base point (Esc cancels)")),
        Pending::Insert { name, factor, .. } => {
            format!("{} {name} ({} {}): {}", tl!("Insert Block"), tl!("scale"), shown(*factor), tl!("insertion point (Esc cancels)"))
        }
    })
}

/// A click while a block's point is waited for: the block made or inserted there. None when no
/// point is waited for (the click goes to the command).
pub(super) fn feed(ed: &mut CadEditor, p: Pt) -> Option<Option<Drawing>> {
    let pending = ed.block.pick.take()?;
    ed.message.clear();
    let mut d = ed.drawing.clone();
    let result = match pending {
        Pending::Create { name, annotative, replace, selection } => {
            d.create_block(&name, &selection, p, annotative, replace).map(|made| (name, made, true))
        }
        Pending::Insert { name, factor, angle } => d.insert_block(&name, p, factor, angle).map(|made| (name, made, false)),
    };
    Some(match result {
        Ok((name, made, created)) => {
            ed.message = if created { format!("{} {name}", tl!("Block created:")) } else { format!("{} {name}", tl!("Block inserted:")) };
            ed.selection = made;
            Some(d)
        }
        Err(e) => {
            ed.message = tl!(&e).to_string();
            None
        }
    })
}

/// The block being inserted, drawn dashed where the next click would put it.
pub(super) fn paint_preview(ed: &CadEditor, painter: &egui::Painter, view: View, p: Pt, accent: Color32) {
    let Some(Pending::Insert { name, factor, angle }) = &ed.block.pick else { return };
    let Some(b) = ed.drawing.block(name) else { return };
    let mut ghost = Drawing { unit: ed.drawing.unit, scale: ed.drawing.scale, blocks: vec![b.clone()], ..Drawing::default() };
    let Ok(made) = ghost.insert_block(name, p, *factor, *angle) else { return };
    for e in made.iter().filter_map(|&i| ghost.entities.get(i)) {
        for line in ghost.shape(e) {
            let screen: Vec<Pos2> = line.iter().map(|&q| view.to_screen(q)).collect();
            if screen.len() > 1 {
                painter.add(Shape::dashed_line(&screen, Stroke::new(1.2, accent), 5.0, 3.0));
            }
        }
    }
}

/// `ui.cadBlock`: {"action": "create", "name", "annotative"?: bool (true), "base"?: [x, y]
/// (else picked in the drawing), "replace"?: bool (true)} of the selection;
/// {"action": "insert", "name", "at"?: [x, y] (else picked), "scale"?: n (the proposed one),
/// "angle"?: degrees}; {"action": "delete", "name"}; {"action": "list"};
/// {"action": "dialog", "which": "create" | "insert" | "close"}.
pub(super) fn command(app: &mut WordApp, p: &Value) -> Result<Value, String> {
    let Some(ed) = app.cad.as_mut() else { return Err("no CAD drawing is open".into()) };
    let s = |k: &str| p.get(k).and_then(Value::as_str).map(str::to_string);
    let point = |k: &str| -> Result<Option<Pt>, String> {
        match p.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => {
                let xy = v.as_array().filter(|a| a.len() == 2).and_then(|a| Some(Pt::new(a.first()?.as_f64()?, a.get(1)?.as_f64()?)));
                xy.filter(|q| q.x.is_finite() && q.y.is_finite()).map(Some).ok_or_else(|| format!("{k}: [x, y] in drawing units"))
            }
        }
    };
    match s("action").as_deref().unwrap_or("list") {
        "list" => {
            let list: Vec<Value> = ed
                .drawing
                .block_infos()
                .into_iter()
                .map(|b| {
                    json!({"name": b.name, "annotative": b.annotative, "scale": b.scale, "objects": b.entities,
                    "size": [b.size.0, b.size.1], "paperMm": [b.paper.0, b.paper.1], "inserted": b.inserted,
                    "proposedScale": ed.drawing.proposed_factor(&b.name)})
                })
                .collect();
            Ok(json!({"blocks": list, "drawingScale": ed.drawing.scale}))
        }
        "dialog" => {
            match s("which").as_deref() {
                Some("create") => open_dialog(ed, false),
                Some("insert") => open_dialog(ed, true),
                Some("close") | None => ed.block.dialog = None,
                Some(other) => return Err(format!("which: create, insert or close, not {other}")),
            }
            Ok(json!({"open": ed.block.dialog.is_some()}))
        }
        "create" => {
            let name = s("name").ok_or("name: the block's name")?;
            let annotative = p.get("annotative").and_then(Value::as_bool).unwrap_or(true);
            let replace = p.get("replace").and_then(Value::as_bool).unwrap_or(true);
            if ed.selection.is_empty() {
                return Err("Select the objects first".into());
            }
            if ed.drawing.block(name.trim()).is_some() {
                return Err(format!("There is already a block called {}", name.trim()));
            }
            let selection = ed.selection.clone();
            match point("base")? {
                None => {
                    ed.block.dialog = None;
                    ed.block.pick = Some(Pending::Create { name, annotative, replace, selection });
                    Ok(json!({"waiting": "base point"}))
                }
                Some(base) => {
                    let mut d = ed.drawing.clone();
                    let made = d.create_block(&name, &selection, base, annotative, replace)?;
                    ed.selection = made;
                    ed.block.dialog = None;
                    let scale = d.scale;
                    commit(app, &d);
                    Ok(json!({"created": name.trim(), "annotative": annotative, "scale": scale}))
                }
            }
        }
        "insert" => {
            let name = s("name").ok_or("name: the block's name")?;
            let proposed = ed.drawing.proposed_factor(&name).ok_or_else(|| format!("There is no block called {name}"))?;
            let factor = match p.get("scale") {
                None | Some(Value::Null) => proposed,
                Some(v) => v.as_f64().or_else(|| v.as_str().and_then(number)).filter(|f| *f > 0.0).ok_or("scale: a number more than 0")?,
            };
            let angle = p.get("angle").and_then(Value::as_f64).unwrap_or(0.0);
            // The dialog's scale is the whole one; the annotative part is kept apart so a later
            // change of the drawing's scale rescales it.
            let extra = factor / proposed.max(1e-12);
            match point("at")? {
                None => {
                    ed.block.dialog = None;
                    ed.block.pick = Some(Pending::Insert { name, factor: extra, angle });
                    Ok(json!({"waiting": "insertion point", "scale": factor}))
                }
                Some(at) => {
                    let mut d = ed.drawing.clone();
                    let made = d.insert_block(&name, at, extra, angle)?;
                    ed.selection = made;
                    ed.block.dialog = None;
                    commit(app, &d);
                    Ok(json!({"inserted": name, "scale": factor}))
                }
            }
        }
        "delete" => {
            let name = s("name").ok_or("name: the block's name")?;
            let mut d = ed.drawing.clone();
            d.delete_block(&name)?;
            ed.selection.clear();
            commit(app, &d);
            Ok(json!({"deleted": name}))
        }
        other => Err(format!("action: create, insert, delete, list or dialog, not {other}")),
    }
}

/// The Home tab's Block group: Create Block and Insert Block.
pub(super) fn group(app: &mut WordApp, ui: &mut Ui) {
    use crate::widgets::{big, group};
    group(ui, "Block", None, app, |ui, app| {
        big(ui, app, "cadBlockCreate", "Create\nBlock", "ui.cadBlock", json!({"action": "dialog", "which": "create"}), false);
        big(ui, app, "cadBlockInsert", "Insert\nBlock", "ui.cadBlock", json!({"action": "dialog", "which": "insert"}), false);
    });
}

/// What the window asks to do when it closes.
enum Outcome {
    Cancel,
    Run(Value),
}

/// The Create Block or Insert Block window.
pub(super) fn show(app: &mut WordApp, ctx: &egui::Context) {
    let Some(dialog) = app.cad.as_ref().and_then(|e| e.block.dialog.clone()) else { return };
    let insert = matches!(dialog, Dialog::Insert { .. });
    let mut outcome = None;
    let resp = egui::Modal::new(egui::Id::unique(("cad_block_dialog", insert))).show(ctx, |ui| {
        ui.set_min_width(400.0);
        ui.set_max_width(460.0);
        let Some(ed) = app.cad.as_mut() else { return };
        let mut dialog = dialog.clone();
        match &mut dialog {
            Dialog::Create { name, annotative, replace, base } => {
                ui.heading(tl!("Create Block"));
                ui.add_space(6.0);
                outcome = create_body(ed, ui, (name, annotative, replace, base));
            }
            Dialog::Insert { name, scale, angle } => {
                ui.heading(tl!("Insert Block"));
                ui.add_space(6.0);
                outcome = insert_body(ed, ui, (name, scale, angle));
            }
        }
        if ed.block.dialog.is_some() {
            ed.block.dialog = Some(dialog);
        }
    });
    if outcome.is_none() && resp.should_close() {
        outcome = Some(Outcome::Cancel);
    }
    match outcome {
        Some(Outcome::Cancel) => {
            if let Some(ed) = app.cad.as_mut() {
                ed.block.dialog = None;
            }
        }
        Some(Outcome::Run(p)) => {
            if let Err(e) = app.run("ui.cadBlock", p)
                && let Some(ed) = app.cad.as_mut()
            {
                ed.message = tl!(&e).to_string();
                // The window stays open to put it right.
            }
        }
        None => {}
    }
}

/// OK and Cancel at the window's foot; true for OK.
fn buttons(ui: &mut Ui, ok_enabled: bool) -> Option<bool> {
    ui.add_space(10.0);
    ui.separator();
    // A row of its own height: a layout that may grow would take the window's whole height.
    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 28.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if ui.add_enabled(ok_enabled, crate::widgets::primary_button(ui, tl!("OK"))).clicked() {
            return Some(true);
        }
        ui.button(tl!("Cancel")).clicked().then_some(false)
    })
    .inner
}

fn create_body(
    ed: &mut CadEditor,
    ui: &mut Ui,
    (name, annotative, replace, base): (&mut String, &mut bool, &mut bool, &mut Base),
) -> Option<Outcome> {
    let d = &ed.drawing;
    let scale = strix_cad::scale_text(d.scale);
    let bounds = d.bounds_of(&d.with_groups(&ed.selection));
    let taken = d.block(name.trim()).is_some();
    field_grid(ui, "cad_block_create", |ui| {
        ui.label(tl!("Name"));
        ui.add(egui::TextEdit::singleline(name).desired_width(FIELD_W + 160.0));
        ui.end_row();
    });
    ui.add_space(4.0);
    ui.label(tl!("Base point"));
    ui.radio_value(base, Base::Pick, tl!("Pick it in the drawing"));
    ui.radio_value(base, Base::Centre, tl!("Centre of the selection"));
    ui.radio_value(base, Base::LowerLeft, tl!("Lower left corner of the selection"));
    ui.add_space(6.0);
    ui.checkbox(annotative, tl!("Annotative: the same size on paper at every scale"));
    let note = if *annotative {
        format!(
            "{} {scale}. {}",
            tl!("Drawn at the drawing's scale,"),
            tl!("Inserted at another scale 1:N it is scaled N / (this N) about its insertion point, and again whenever the drawing's scale changes.")
        )
    } else {
        tl!("Plain: the same size in the drawing at every scale (inserted with scale 1).").to_string()
    };
    ui.label(egui::RichText::new(note).small().weak());
    if let Some((lo, hi)) = bounds {
        let k = d.unit.mm() / d.scale.max(1e-9);
        let (w, h) = (hi.x - lo.x, hi.y - lo.y);
        ui.label(
            egui::RichText::new(format!(
                "{} {} × {} {} · {} {scale}: {} × {} mm",
                tl!("Size:"),
                shown(w),
                shown(h),
                d.unit.symbol(),
                tl!("on paper at"),
                shown(w * k),
                shown(h * k),
            ))
            .small(),
        );
    }
    ui.checkbox(replace, tl!("Turn the selected objects into the block"));
    if taken {
        ui.colored_label(ui.visuals().error_fg_color, tl!("There is already a block with this name"));
    }
    let ok = buttons(ui, !name.trim().is_empty() && !taken)?;
    if !ok {
        return Some(Outcome::Cancel);
    }
    let base_pt = match (*base, bounds) {
        (Base::Centre, Some((lo, hi))) => Some(Pt::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0)),
        (Base::LowerLeft, Some((lo, _))) => Some(lo),
        _ => None,
    };
    let mut p = json!({"action": "create", "name": name.trim(), "annotative": *annotative, "replace": *replace});
    if let Some(b) = base_pt {
        p["base"] = json!([b.x, b.y]);
    }
    Some(Outcome::Run(p))
}

fn insert_body(ed: &mut CadEditor, ui: &mut Ui, (name, scale, angle): (&mut String, &mut String, &mut String)) -> Option<Outcome> {
    let infos = ed.drawing.block_infos();
    let now = ed.drawing.scale;
    field_grid(ui, "cad_block_insert", |ui| {
        ui.label(tl!("Block"));
        let mut chosen = None;
        craft_ui::ComboBox::from_id_salt("cad_block_name").selected_text(name.as_str()).width(FIELD_W + 160.0).show_ui(ui, |ui| {
            for b in &infos {
                let tag = if b.annotative { format!("  ({} {})", tl!("annotative,"), strix_cad::scale_text(b.scale)) } else { String::new() };
                if ui.selectable_label(*name == b.name, format!("{}{tag}", b.name)).clicked() {
                    chosen = Some(b.name.clone());
                }
            }
        });
        if let Some(c) = chosen {
            // Another block: its own proposed scale.
            *scale = shown(ed.drawing.proposed_factor(&c).unwrap_or(1.0));
            *name = c;
        }
        ui.end_row();
        ui.label(tl!("Scale"));
        ui.add(egui::TextEdit::singleline(scale).desired_width(FIELD_W));
        ui.end_row();
        ui.label(tl!("Angle"));
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(angle).desired_width(FIELD_W));
            ui.label("°");
        });
        ui.end_row();
    });
    let info = infos.iter().find(|b| b.name == *name);
    let proposed = ed.drawing.proposed_factor(name).unwrap_or(1.0);
    let factor = number(scale).filter(|f| *f > 0.0);
    if let Some(b) = info {
        let note = if b.annotative {
            format!(
                "{} {} {} {}: {} {} / {} = {}. {}",
                tl!("Annotative, drawn at"),
                strix_cad::scale_text(b.scale),
                tl!("; the drawing is at"),
                strix_cad::scale_text(now),
                tl!("proposed scale"),
                shown(now),
                shown(b.scale),
                shown(proposed),
                tl!("It follows the drawing's scale from now on."),
            )
        } else {
            tl!("Plain block: scale 1 keeps the size it was made with.").to_string()
        };
        ui.add_space(4.0);
        ui.label(egui::RichText::new(note).small().weak());
        if let Some(f) = factor {
            let k = ed.drawing.unit.mm() / now.max(1e-9) * f;
            ui.label(egui::RichText::new(format!("{} {} × {} mm", tl!("On paper:"), shown(b.size.0 * k), shown(b.size.1 * k))).small());
        }
        if ui.small_button(tl!("Use the proposed scale")).clicked() {
            *scale = shown(proposed);
        }
    }
    let angle_v = number(angle);
    if factor.is_none() || angle_v.is_none() {
        ui.colored_label(ui.visuals().error_fg_color, tl!("The scale is a number more than 0 (or a ratio, 1:2), the angle a number"));
    }
    let ok = buttons(ui, factor.is_some() && angle_v.is_some() && info.is_some())?;
    if !ok {
        return Some(Outcome::Cancel);
    }
    Some(Outcome::Run(json!({"action": "insert", "name": name, "scale": factor, "angle": angle_v})))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_with_drawing() -> WordApp {
        let mut app = WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::new()), crate::Services::default());
        app.session.run("document.setText", &json!({"text": ""})).unwrap();
        app.session.run("insert.cad", &json!({})).unwrap();
        app.run("ui.cadEdit", json!({})).unwrap();
        app
    }

    /// The owner: objects drawn at the drawing format's scale become a block, annotative or not;
    /// inserting an annotative one at another scale proposes `N / N₀` and scales it about its
    /// insertion point; the base point is clicked after the window.
    #[test]
    fn create_and_insert_an_annotative_block_in_the_editor() {
        let mut app = app_with_drawing();
        let n = app.cad.as_ref().unwrap().drawing.entities.len();
        assert!(n > 0, "the starter drawing");
        app.run("ui.cadScale", json!({"scale": 50})).unwrap();
        let ed = app.cad.as_mut().unwrap();
        ed.selection = (0..n).collect();
        // The window: B at the command line opens it; a base point to click after it.
        assert!(typed(ed, "b"));
        assert!(matches!(ed.block.dialog, Some(Dialog::Create { annotative: true, .. })));
        app.run("ui.cadBlock", json!({"action": "create", "name": "Tag", "annotative": true})).unwrap();
        let ed = app.cad.as_mut().unwrap();
        assert!(ed.block.dialog.is_none() && ed.block.pick.is_some());
        assert!(!ed.selecting(), "the click is a point, not a pick");
        assert!(ed.prompt().contains("Tag"));
        let d = super::super::feed(ed, Pt::new(0.0, 0.0), None).expect("block made");
        commit(&mut app, &d);
        let ed = app.cad.as_ref().unwrap();
        assert_eq!(ed.drawing.blocks.len(), 1);
        assert_eq!(ed.drawing.inserts.len(), 1);
        assert_eq!(ed.drawing.block("Tag").map(|b| (b.annotative, b.scale)), Some((true, 50.0)));
        // At 1:100 the proposed scale is 2, and the insertion is twice as large in the drawing.
        app.run("ui.cadScale", json!({"scale": 100})).unwrap();
        let list = app.run("ui.cadBlock", json!({"action": "list"})).unwrap();
        assert_eq!(list["blocks"][0]["proposedScale"], json!(2.0));
        let size = |app: &WordApp, sel: &[usize]| {
            let d = &app.cad.as_ref().unwrap().drawing;
            let (lo, hi) = d.bounds_of(sel).unwrap();
            hi.x - lo.x
        };
        let first: Vec<usize> =
            app.cad.as_ref().unwrap().drawing.entities.iter().enumerate().filter(|(_, e)| e.group.is_some()).map(|(i, _)| i).collect();
        let w100 = size(&app, &first);
        app.run("ui.cadBlock", json!({"action": "insert", "name": "Tag", "at": [1000.0, 0.0]})).unwrap();
        let sel = app.cad.as_ref().unwrap().selection.clone();
        assert!((size(&app, &sel) - w100).abs() < 1e-6, "{} vs {w100}", size(&app, &sel));
        // Back to 1:50: both halve.
        app.run("ui.cadScale", json!({"scale": 50})).unwrap();
        assert!((size(&app, &first) - w100 / 2.0).abs() < 1e-6);
        // The Insert window proposes the scale.
        let ed = app.cad.as_mut().unwrap();
        open_dialog(ed, true);
        assert!(matches!(&ed.block.dialog, Some(Dialog::Insert { scale, .. }) if scale == "1"));
        assert!(app.run("ui.cadBlock", json!({"action": "insert", "name": "Nope", "at": [0, 0]})).is_err());
        assert!(app.run("ui.cadBlock", json!({"action": "insert", "name": "Tag", "scale": -1})).is_err());
    }
}
