//! The Subdocuments pane (Outlining › Master Document, docs/master-documents.md): the files the
//! master document links, each with its state — in step, edited here (saved with the master),
//! changed on both sides, file not found — and its actions: go to it, open its file, reload it,
//! unlink it. Every action is a `master.*` command, so it is undoable where it edits.

use egui::{RichText, Ui};
use serde_json::{Value, json};

use crate::WordApp;
use crate::pane_kit::{self, Status};
use crate::theme::Tokens;

pub fn pane(app: &mut WordApp, ui: &mut Ui) {
    let subs = app.session.run("master.subdocuments", &json!({})).ok().and_then(|v| v["subdocuments"].as_array().cloned()).unwrap_or_default();
    egui::ScrollArea::vertical().show(ui, |ui| {
        pane_kit::section(ui, "01", tl!("Linked files"));
        if subs.is_empty() {
            pane_kit::note(ui, tl!("This document links no other documents yet. Insert one, or select paragraphs and create one from them: its text shows here and is saved back to its own file."));
        }
        for (i, sub) in subs.iter().enumerate() {
            row(app, ui, i, sub);
            ui.add_space(6.0);
        }
        pane_kit::section(ui, "02", tl!("Add"));
        ui.horizontal_wrapped(|ui| {
            if ui.button(tl!("Insert Subdocument…")).on_hover_text(tl!("Link a document at the cursor")).clicked() {
                run(app, "master.insertSubdocument", json!({}));
            }
            if ui.button(tl!("Create Subdocument…")).on_hover_text(tl!("Save the selected paragraphs as a new file, linked in their place")).clicked() {
                run(app, "master.createSubdocument", json!({}));
            }
        });
        if !subs.is_empty() {
            pane_kit::section(ui, "03", tl!("All"));
            ui.horizontal_wrapped(|ui| {
                if ui.button(tl!("Save Subdocuments")).on_hover_text(tl!("Write the edited subdocuments to their files now (also done by saving the document)")).clicked() {
                    run(app, "master.saveSubdocuments", json!({}));
                }
                if ui.button(tl!("Reload Subdocuments")).on_hover_text(tl!("Show every file as it is on disk, discarding edits made here")).clicked() {
                    run(app, "master.refresh", json!({}));
                }
            });
            ui.add_space(6.0);
            pane_kit::note(ui, tl!("Saving the document saves every subdocument edited here. Opening it shows each file as it is now."));
        }
    });
}

/// One linked file: its name, folder, state, first words and actions.
fn row(app: &mut WordApp, ui: &mut Ui, i: usize, sub: &Value) {
    let t = Tokens::get(ui.ctx());
    let path = sub["path"].as_str().unwrap_or_default().to_string();
    let file = std::path::Path::new(&path);
    let name = file.file_name().map_or_else(|| path.clone(), |n| n.to_string_lossy().into_owned());
    let folder = file.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let (label, status) = if sub["conflict"].as_bool() == Some(true) {
        (tl!("conflict"), Status::Error)
    } else if sub["exists"].as_bool() != Some(true) {
        (tl!("not found"), Status::Warning)
    } else if sub["edited"].as_bool() == Some(true) {
        (tl!("edited"), Status::Changed)
    } else {
        (tl!("in step"), Status::Ok)
    };
    pane_kit::card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{:02}", i + 1)).font(pane_kit::mono(10.5)).color(t.text_dim));
            let title = ui.add(egui::Label::new(RichText::new(&name).strong()).truncate().sense(egui::Sense::click()));
            if title.on_hover_text(tl!("Go to it")).clicked()
                && let Some(start) = sub.get("start")
            {
                let _ = app.run("select.range", json!({"anchor": start, "focus": start}));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| pane_kit::badge(ui, label, status));
        });
        if !folder.is_empty() {
            ui.add(egui::Label::new(RichText::new(&folder).font(pane_kit::mono(10.0)).color(t.text_dim)).truncate()).on_hover_text(&path);
        }
        let text = sub["text"].as_str().unwrap_or_default();
        if !text.is_empty() {
            ui.add(egui::Label::new(RichText::new(text).size(11.0).color(t.text_dim)).truncate());
        }
        if status == Status::Error {
            pane_kit::note(ui, tl!("Changed here and in the file: keep one of the two."));
        }
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let p = json!({"path": path});
            if status == Status::Error {
                if ui.small_button(tl!("Keep this version")).on_hover_text(tl!("Overwrite the file with the text shown here")).clicked() {
                    run(app, "master.saveSubdocuments", json!({"force": true}));
                }
                if ui.small_button(tl!("Keep the file")).on_hover_text(tl!("Replace the text shown here with the file's")).clicked() {
                    run(app, "master.refresh", p.clone());
                }
            } else if ui.small_button(tl!("Reload")).on_hover_text(tl!("Show the file as it is on disk, discarding edits made here")).clicked() {
                run(app, "master.refresh", p.clone());
            }
            if ui.small_button(tl!("Open")).on_hover_text(tl!("Open the file in a window of its own")).clicked() {
                run(app, "master.openSubdocument", p.clone());
            }
            if ui
                .small_button(tl!("Unlink"))
                .on_hover_text(tl!("Keep the text in this document, without the link; the file stays as it is"))
                .clicked()
            {
                run(app, "master.unlinkSubdocument", p);
            }
        });
    });
}

fn run(app: &mut WordApp, id: &str, params: Value) {
    if let Err(e) = app.run(id, params) {
        app.status(e);
    }
}
