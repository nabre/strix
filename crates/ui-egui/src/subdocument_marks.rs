//! Subdocuments on the page (docs/master-documents.md): every linked file of a master document is
//! always marked, wherever the caret is, so a file brought in is seen as one: a bar in the left
//! margin along its text, a light frame around it, and a tab above it with the file's name and its
//! state. A click on the tab puts the caret in it, so Unlink, Reload and Open act on that one.
//! Drawn in code; everything ends in a command.

use egui::{Align2, Rect, Stroke, pos2, vec2};
use serde_json::json;
use wordcraft_doc::{Pos, StoryRef};
use wordcraft_layout::DocLayout;

use crate::WordApp;
use crate::theme::{Tokens, regular};

/// Most subdocuments marked at once.
const MAX_MARKED: usize = 500;

/// A tab shown this frame: where it is and where the subdocument's content starts.
#[derive(Clone)]
struct Tab {
    rect: Rect,
    at: Pos,
}

fn tabs_id() -> egui::Id {
    egui::Id::unique("strix_subdocument_tabs")
}

/// Whether a content control is a subdocument (this module draws those; the generic frame doesn't).
pub(crate) fn is_subdocument(tag: &str) -> bool {
    wordcraft_engine::cmd::master::linked_file(tag).is_some()
}

/// The subdocument's content on screen, one rect per page it is on.
fn frames(app: &WordApp, layout: &DocLayout, rects: &[Rect], scale: f32, from: &Pos, to: &Pos) -> Vec<Rect> {
    let mut out: Vec<(usize, Rect)> = Vec::new();
    for (pi, sr) in layout.selection_rects(&app.session.doc, from, to, app.session.page_hint) {
        let Some(pr) = rects.get(pi) else { continue };
        let s = Rect::from_min_size(pos2(pr.min.x + sr.x * scale, pr.min.y + sr.y * scale), vec2(sr.w * scale, sr.h * scale));
        match out.iter_mut().find(|(p, _)| *p == pi) {
            Some((_, u)) => *u = u.union(s),
            None => out.push((pi, s)),
        }
    }
    out.into_iter().map(|(_, r)| r.expand2(vec2(4.0, 2.0))).collect()
}

/// Draw the marks (inside the canvas viewport, with the content controls' frames).
pub(crate) fn paint(app: &WordApp, painter: &egui::Painter, t: &Tokens, layout: &DocLayout, rects: &[Rect], scale: f32) {
    let ctx = painter.ctx();
    let mut tabs = Vec::new();
    let focus = &app.session.sel.focus;
    let subs: Vec<_> = app
        .session
        .doc
        .control_ranges(StoryRef::Body)
        .into_iter()
        .filter(|r| r.depth == 0 && r.control.block && is_subdocument(&r.control.tag))
        .take(MAX_MARKED)
        .collect();
    let hover = ctx.input(|i| i.pointer.hover_pos());
    for r in &subs {
        let current = r.contains(focus);
        let fr = frames(app, layout, rects, scale, &r.content_start(), &r.end);
        let colour = if current { t.accent } else { t.accent.gamma_multiply(0.45) };
        for f in &fr {
            painter.rect_stroke(*f, 2.0, Stroke::new(1.0, colour), egui::StrokeKind::Outside);
            // The bar in the margin, clear of an exercise's grip.
            let bar = Rect::from_min_max(pos2(f.min.x - 28.0, f.min.y), pos2(f.min.x - 25.0, f.max.y));
            painter.rect_filled(bar, 1.0, colour);
        }
        let Some(first) = fr.first() else { continue };
        let path = wordcraft_engine::cmd::master::linked_file(&r.control.tag).unwrap_or_default();
        let name = path.file_name().map_or_else(|| path.to_string_lossy().into_owned(), |n| n.to_string_lossy().into_owned());
        let font = regular(9.5);
        let w = (painter.layout_no_wrap(name.clone(), font.clone(), t.accent_text).size().x + 26.0).min(320.0);
        let tab = Rect::from_min_size(pos2(first.min.x - 28.0, first.min.y - 16.0), vec2(w, 15.0));
        let hovered = hover.is_some_and(|p| tab.contains(p));
        let (fill, ink) = if current || hovered { (t.accent, t.on_accent) } else { (t.checked, t.accent_text) };
        painter.rect_filled(tab, egui::CornerRadius { nw: 3, ne: 3, sw: 0, se: 0 }, fill);
        crate::icons::paint(painter, Rect::from_min_size(pos2(tab.min.x + 3.0, tab.min.y + 1.0), vec2(13.0, 13.0)), "subdocOpen", ink, ink);
        painter.with_clip_rect(tab).text(pos2(tab.min.x + 19.0, tab.center().y), Align2::LEFT_CENTER, &name, font, ink);
        if hovered {
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            egui::Tooltip::always_open(ctx.clone(), painter.layer_id(), egui::Id::unique("strix_subdocument_tip"), egui::PopupAnchor::Pointer).show(
                |ui| {
                    ui.label(egui::RichText::new(tl!("Subdocument")).strong());
                    ui.label(path.to_string_lossy());
                    ui.label(tl!("Click to put the cursor in it: Unlink, Reload and Open act on it."));
                },
            );
        }
        tabs.push(Tab { rect: tab, at: r.content_start() });
    }
    ctx.data_mut(|d| d.insert_temp(tabs_id(), tabs));
}

/// A click on a tab puts the caret at the start of its subdocument. Whether it took the click.
pub(crate) fn click(app: &mut WordApp, ctx: &egui::Context, p: egui::Pos2) -> bool {
    let tabs = ctx.data(|d| d.get_temp::<Vec<Tab>>(tabs_id())).unwrap_or_default();
    let Some(tab) = tabs.into_iter().find(|t| t.rect.contains(p)) else { return false };
    let at = serde_json::to_value(&tab.at).unwrap_or_default();
    let _ = app.run("select.range", json!({"anchor": at, "focus": at}));
    true
}
