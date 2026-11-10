//! The exercise's grip (docs/exercise.md, Deleting an exercise): a handle in the margin at the
//! left of the title of the exercise the cursor is in. A click opens its menu: select the whole
//! exercise, its properties, delete it. Drawn in code; everything ends in a command.

use egui::{Color32, Rect, Stroke, pos2, vec2};
use serde_json::json;
use wordcraft_doc::Pos;
use wordcraft_doc::control::ControlRange;

use crate::WordApp;
use crate::theme::Tokens;

/// The grip shown this frame.
#[derive(Clone)]
struct Grip {
    /// Where it is on screen.
    rect: Rect,
    /// The exercise, 1-based (as `exercise.delete` names it).
    number: u64,
    /// The exercise control's start marker and just after its end marker (its whole selection).
    from: Pos,
    to: Pos,
}

/// The grip whose menu is open.
#[derive(Clone)]
struct Menu {
    grip: Grip,
}

fn grip_id() -> egui::Id {
    egui::Id::unique("strix_exercise_grip")
}

fn menu_id() -> egui::Id {
    egui::Id::unique("strix_exercise_grip_menu")
}

/// Forget last frame's grip (the content controls are painted again).
pub(crate) fn clear(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove::<Grip>(grip_id()));
}

/// Draw the grip of exercise `r` at the left of `first`, its frame's first rect on screen.
pub(crate) fn paint(app: &WordApp, painter: &egui::Painter, t: &Tokens, r: &ControlRange, first: Rect) {
    let tag = wordcraft_engine::cmd::exercise::TAG_EXERCISE;
    let mut starts: Vec<Pos> = app.session.doc.control_ranges(r.start.story).into_iter().filter(|c| c.control.tag == tag).map(|c| c.start).collect();
    starts.sort();
    let Some(k) = starts.iter().position(|s| *s == r.start) else { return };
    let rect = Rect::from_min_size(pos2(first.min.x - 18.0, first.min.y + 1.0), vec2(12.0, 18.0));
    let ctx = painter.ctx();
    let hovered = ctx.input(|i| i.pointer.hover_pos()).is_some_and(|p| rect.contains(p));
    let open = ctx.data(|d| d.get_temp::<Menu>(menu_id())).is_some();
    let fill = if hovered || open { t.accent } else { t.checked };
    let ink = if hovered || open { t.on_accent } else { t.accent_text };
    painter.rect_filled(rect, 3.0, fill);
    painter.rect_stroke(rect, 3.0, Stroke::new(1.0, t.accent.gamma_multiply(0.55)), egui::StrokeKind::Inside);
    dots(painter, rect, ink);
    if hovered {
        ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let end = Pos { off: r.end.off + wordcraft_doc::para::OBJ.len_utf8(), ..r.end.clone() };
    let grip = Grip { rect, number: k as u64 + 1, from: r.start.clone(), to: end };
    ctx.data_mut(|d| d.insert_temp(grip_id(), grip));
}

/// Two columns of three dots: the grip's mark.
fn dots(painter: &egui::Painter, rect: Rect, ink: Color32) {
    let c = rect.center();
    for dx in [-2.5, 2.5] {
        for dy in [-4.5, 0.0, 4.5] {
            painter.circle_filled(pos2(c.x + dx, c.y + dy), 1.2, ink);
        }
    }
}

/// A click at `p` on the grip opens (or closes) its menu; whether it was on the grip.
pub(crate) fn click(ctx: &egui::Context, p: egui::Pos2) -> bool {
    let Some(grip) = ctx.data(|d| d.get_temp::<Grip>(grip_id())) else { return false };
    if !grip.rect.contains(p) {
        return false;
    }
    let open = ctx.data(|d| d.get_temp::<Menu>(menu_id())).is_some();
    ctx.data_mut(|d| {
        if open {
            d.remove::<Menu>(menu_id());
        } else {
            d.insert_temp(menu_id(), Menu { grip });
        }
    });
    true
}

/// The grip's menu (outside the canvas viewport, above everything).
pub(crate) fn menu(app: &mut WordApp, ctx: &egui::Context) {
    let Some(Menu { grip }) = ctx.data(|d| d.get_temp::<Menu>(menu_id())) else { return };
    // Its exercise no longer shows its grip (the cursor left it, it went): the menu goes too.
    if ctx.data(|d| d.get_temp::<Grip>(grip_id())).is_none_or(|g| g.from != grip.from || g.number != grip.number) {
        ctx.data_mut(|d| d.remove::<Menu>(menu_id()));
        return;
    }
    let t = Tokens::get(ctx);
    let mut close = false;
    let area = egui::Area::new(egui::Id::unique("strix_exercise_grip_area"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos2(grip.rect.min.x, grip.rect.max.y + 2.0));
    let shown = area.show(ctx, |ui| {
        egui::Frame::popup(ui.style()).fill(t.menu).show(ui, |ui| {
            ui.set_min_width(190.0);
            ui.set_max_width(230.0);
            let item = |ui: &mut egui::Ui, icon: &str, text: &str| crate::widgets::icon_menu_item(ui, icon, text, "", 190.0);
            if item(ui, "select", tl!("Select Exercise")).clicked() {
                let _ = app.run("select.range", json!({"anchor": grip.from, "focus": grip.to}));
                close = true;
            }
            if item(ui, "exerciseProperties", tl!("Exercise Properties")).clicked() {
                let _ = app.run("view.exercisePane", json!({"value": true}));
                close = true;
            }
            ui.separator();
            let delete = item(ui, "deleteTable", tl!("Delete Exercise"))
                .on_hover_text(tl!("The whole exercise goes: its statement, questions and solution (Undo brings it back)"));
            if delete.clicked() {
                if let Err(e) = app.run("exercise.delete", json!({"exercise": grip.number})) {
                    app.status(e);
                }
                close = true;
            }
        });
    });
    let outside = ctx.input(|i| i.pointer.any_pressed())
        && ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| !shown.response.rect.contains(p) && !grip.rect.contains(p));
    if close || outside || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        ctx.data_mut(|d| d.remove::<Menu>(menu_id()));
    }
}
