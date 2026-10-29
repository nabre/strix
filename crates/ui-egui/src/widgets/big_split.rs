//! A large split button: the top part (the icon) runs the command in the way chosen last, the
//! bottom part (the label and the drop-down marker) opens the menu of its ways. A CAD ribbon's
//! Circle button works so (`cad_editor.rs`).

use super::buttons::{BIG_ICON_Y, BIG_LABEL_Y, ButtonAction, CONTENT_H, LABEL_PITCH, bg, enabled, label_line, run_from_ribbon};
use super::groups::command_menu;
use super::tooltip::{keytip_badge, show_hint, tooltip, tooltip_lines};
use super::{CornerRadius, Rect, Response, Sense, Stroke, Tokens, Ui, WordApp, icons, pos2, regular, vec2};

/// Where the top part ends, from the button's top: just under the 32 px icon.
const BIG_SPLIT_Y: f32 = BIG_ICON_Y + 17.0;

/// A large split button: `icon` and `tip` show the way the top part runs (`action`), `label` names
/// the command under it.
pub struct BigSplit<'a> {
    pub icon: &'a str,
    pub label: &'a str,
    pub tip: &'a str,
    pub action: ButtonAction<'a>,
}

/// Draws the button; the bottom part opens `menu`. Returns the top part's response.
pub fn big_split(ui: &mut Ui, app: &mut WordApp, button: BigSplit<'_>, menu: impl FnOnce(&mut Ui, &mut WordApp)) -> Response {
    let BigSplit { icon, label, tip, action } = button;
    let ButtonAction { id, params, checked } = action;
    let (label, tip) = (tl!(label), tl!(tip));
    let t = Tokens::get(ui.ctx());
    let galley_w = ui.ctx().fonts_mut(|f| f.layout_no_wrap(label.to_string(), regular(11.5), t.text).size().x);
    let w = (galley_w + 22.0).max(44.0);
    let (r, _) = ui.allocate_exact_size(vec2(w, CONTENT_H), Sense::hover());
    let top = Rect::from_min_max(r.min, pos2(r.max.x, r.min.y + BIG_SPLIT_Y));
    let bottom = Rect::from_min_max(pos2(r.min.x, top.max.y), r.max);
    let base = ui.make_persistent_id(("bigSplit", id, label));
    let resp = ui.interact(top, base.with("run"), Sense::click());
    let aresp = ui.interact(bottom, base.with("menu"), Sense::click());
    keytip_badge(ui, top, id, tip);
    let on = enabled(app, id);
    // Hovering one part outlines the whole button, the part under the pointer filled.
    if on && (resp.hovered() || aresp.hovered()) {
        ui.painter().rect_stroke(r, CornerRadius::same(4), Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    }
    bg(ui, top, &resp, checked, &t);
    bg(ui, bottom, &aresp, false, &t);
    let (c, a, text) = if on { (t.icon, t.accent, t.text) } else { (t.text_disabled, t.text_disabled, t.text_disabled) };
    icons::paint(ui.painter(), Rect::from_center_size(pos2(r.center().x, r.min.y + BIG_ICON_Y), vec2(32.0, 32.0)), icon, c, a);
    let lines: Vec<&str> = label.split('\n').collect();
    let mut y = r.min.y + BIG_LABEL_Y;
    for (i, l) in lines.iter().enumerate() {
        label_line(ui, r.center().x, y, l, regular(11.5), text, i + 1 == lines.len());
        y += LABEL_PITCH;
    }
    let resp = tooltip(app, resp, tip, id);
    command_menu(&resp, app, id);
    if resp.clicked() && on {
        run_from_ribbon(app, id, params);
    }
    let (arrow_title, _) = tooltip_lines(&crate::i18n::fmt(tl!("{name} options"), &[("name", &label.replace('\n', " "))]), "", "", None);
    show_hint(&aresp, &arrow_title, &[]);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, on, tip));
    aresp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, on, &arrow_title));
    if on {
        egui::Popup::menu(&aresp).show(|ui| {
            ui.set_min_width(200.0);
            menu(ui, app);
        });
    }
    resp
}
