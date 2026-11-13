//! Archive properties and metadata-only exercise bank, kept per open document.
use crate::WordApp;
use egui::{Id, Ui};
use serde_json::{Value, json};

#[derive(Clone, Default)]
struct Bank {
    #[cfg(not(target_arch = "wasm32"))]
    folder: String,
    query: String,
    entries: Vec<Value>,
    error: String,
}
fn key(app: &WordApp) -> Id {
    Id::unique(("strix_archive", app.session.document_id()))
}

pub(crate) fn import(app: &mut WordApp, params: Value) -> Result<Value, String> {
    #[cfg(not(target_arch = "wasm32"))]
    let params = {
        let mut params = params;
        if params.get("data").is_none()
            && let Some(path) = params["path"].as_str()
        {
            let bytes = wordcraft_engine::io::read_file_capped(path, 64 * 1024 * 1024, "exercise bundle")?;
            params["data"] = json!(wordcraft_engine::cmd::insert::base64_encode(&bytes));
        }
        params
    };
    if params.get("data").is_none() {
        if let Some(pick) = &app.services.open_async {
            pick("archive");
        } else {
            app.ask_file(
                crate::FileDialogRequest::Open { purpose: "archive".into() },
                crate::file_dialogs::AfterPick::WithPath { command: "archive.importBundle" },
            );
        }
        return Ok(json!({"pending": true}));
    }
    let result = app.session.run("archive.importBundle", &params).map_err(|e| e.to_string())?;
    if let Some(ctx) = &app.ctx {
        let k = key(app);
        ctx.data_mut(|d| {
            let mut state = d.get_temp::<Bank>(k).unwrap_or_default();
            state.entries = result["entries"].as_array().cloned().unwrap_or_default();
            state.error =
                result["errors"].as_array().filter(|a| !a.is_empty()).map(|a| serde_json::to_string(a).unwrap_or_default()).unwrap_or_default();
            d.insert_temp(k, state);
        });
    }
    app.session.view.archive_pane = true;
    Ok(result)
}

pub(crate) fn pane(app: &mut WordApp, ui: &mut Ui) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.heading(tl!("Archive Properties"));
        let props = app.session.run("archive.properties", &json!({})).unwrap_or_default();
        for (field, label) in [
            ("subject", tl!("School Subject")),
            ("grade", tl!("Grade")),
            ("profession", tl!("Profession")),
            ("section", tl!("Section")),
            ("teacher", tl!("Teacher")),
            ("duration", tl!("Duration (min)")),
            ("tags", tl!("Topics / Tags")),
        ] {
            let mut text = props[field]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| if props[field].is_null() { String::new() } else { props[field].to_string() });
            ui.label(label);
            let response = ui.text_edit_singleline(&mut text);
            if response.changed() {
                let mut update = json!({});
                update[field] = if text.trim().is_empty() { Value::Null } else { json!(text) };
                let _ = app.run("archive.properties", update);
            }
        }
        let score = if props["score"].is_null() { "—".to_string() } else { props["score"].to_string() };
        ui.label(format!("{}: {score}", tl!("Score")));
        ui.separator();
        ui.heading(tl!("Exercise Bank"));
        let k = key(app);
        let mut state = ui.data(|d| d.get_temp::<Bank>(k)).unwrap_or_default();
        #[cfg(not(target_arch = "wasm32"))]
        {
            ui.label(tl!("Folder"));
            ui.text_edit_singleline(&mut state.folder);
            if ui.button(tl!("Scan Folder")).clicked() {
                match app.session.run("archive.scan", &json!({"path": state.folder})) {
                    Ok(r) => {
                        state.entries = r["entries"].as_array().cloned().unwrap_or_default();
                        state.error = r["errors"]
                            .as_array()
                            .filter(|a| !a.is_empty())
                            .map(|a| serde_json::to_string(a).unwrap_or_default())
                            .unwrap_or_default();
                    }
                    Err(e) => state.error = e.to_string(),
                }
            }
        }
        if ui.button(tl!("Import Exercise Bundle…")).clicked() {
            let _ = app.run("archive.importBundle", json!({}));
        }
        ui.label(tl!("Search"));
        ui.text_edit_singleline(&mut state.query);
        if !state.error.is_empty() {
            ui.label(&state.error);
        }
        let query = state.query.to_lowercase();
        for entry in &state.entries {
            if !query.is_empty() && !format!("{} {}", entry["name"], entry["properties"]).to_lowercase().contains(&query) {
                continue;
            }
            ui.group(|ui| {
                ui.label(entry["name"].as_str().unwrap_or_default());
                let props = &entry["properties"];
                ui.label(format!(
                    "{} · {} · {}",
                    props["subject"].as_str().unwrap_or_default(),
                    props["grade"],
                    props["tags"].as_str().unwrap_or_default()
                ));
                ui.horizontal_wrapped(|ui| {
                    if ui.button(tl!("Insert Copy")).clicked() {
                        let mut v = json!({"path": entry["path"]});
                        if entry.get("data").is_some() {
                            v["data"] = entry["data"].clone();
                        }
                        let _ = app.run("archive.insertCopy", v);
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    if entry.get("data").is_none() && ui.button(tl!("Link Subdocument")).clicked() {
                        let _ = app.run("master.insertSubdocument", json!({"path": entry["path"]}));
                    }
                });
            });
        }
        ui.data_mut(|d| d.insert_temp(k, state));
    });
}
