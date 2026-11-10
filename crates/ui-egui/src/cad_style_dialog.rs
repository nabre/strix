//! The CAD editor's style windows (docs/cad.md, Annotations): Dimension Style, Level Style and
//! Text Style, each a modal window with the style list (the current style, its name to rename,
//! New, Delete) and that style's settings. The CAD Annotate tab's Styles group opens them
//! (`ui.cadStyleDialog`); every change runs `ui.cadStyles`, `ui.cadDimStyle` or `ui.cadTextStyle`
//! (one undo step each), so scripts and the MCP set the same things without a window.

use egui::{Ui, vec2};
use serde_json::json;
use strix_cad::StyleKind;

use crate::WordApp;
use crate::cad_editor::{FIELD_W, field_grid};

/// Which style window is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleDialog {
    /// Linear and angular dimensions: marks, sizes, text, units.
    Dim,
    /// Level marks (elevations): their text and triangle heights and unit. They take the
    /// dimension styles, so the list is the dimension styles'.
    Level,
    /// Texts: font, height, bold, italic.
    Text,
}

impl StyleDialog {
    /// From `ui.cadStyleDialog`'s `which`.
    pub fn parse(s: &str) -> Option<StyleDialog> {
        match s.trim() {
            "dim" | "dimension" => Some(StyleDialog::Dim),
            "level" | "elevation" => Some(StyleDialog::Level),
            "text" => Some(StyleDialog::Text),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            StyleDialog::Dim => "dim",
            StyleDialog::Level => "level",
            StyleDialog::Text => "text",
        }
    }

    fn title(self) -> &'static str {
        match self {
            StyleDialog::Dim => "Dimension Style",
            StyleDialog::Level => "Level Style",
            StyleDialog::Text => "Text Style",
        }
    }
}

/// The CAD Annotate tab's Styles group: a button for each style window, and the current
/// dimension and text styles (chosen with objects selected, they take it too).
pub(crate) fn styles_group(app: &mut WordApp, ui: &mut Ui) {
    use crate::widgets::{big, group};
    let Some((dims, dim_now, texts, text_now)) = app.cad.as_ref().map(|e| {
        let d = &e.drawing;
        let standard = || strix_cad::STANDARD.to_string();
        (
            d.styles(StyleKind::Dim),
            d.current_dim_style.clone().unwrap_or_else(standard),
            d.styles(StyleKind::Text),
            d.current_text_style.clone().unwrap_or_else(standard),
        )
    }) else {
        return;
    };
    group(ui, "Styles", None, app, |ui, app| {
        big(ui, app, "cadDim", "Dimension\nStyle", "ui.cadStyleDialog", json!({"which": "dim"}), false)
            .on_hover_text(tl!("Dimension Style: marks, sizes, text and units of the dimensions"));
        big(ui, app, "cadLevel", "Level\nStyle", "ui.cadStyleDialog", json!({"which": "level"}), false)
            .on_hover_text(tl!("Level Style: text, triangle and unit of the level marks"));
        big(ui, app, "cadText", "Text\nStyle", "ui.cadStyleDialog", json!({"which": "text"}), false)
            .on_hover_text(tl!("Text Style: font, height, bold and italic of the texts"));
        field_grid(ui, "cad_current_styles", |ui| {
            for (label, kind, styles, now) in [("Dimensions", "dim", &dims, &dim_now), ("Texts", "text", &texts, &text_now)] {
                ui.label(tl!(label));
                let mut pick = None;
                craft_ui::ComboBox::from_id_salt(("cad_style_now", kind))
                    .selected_text(tl!(now.as_str()))
                    .width(FIELD_W + 40.0)
                    .show_ui(ui, |ui| {
                        for name in styles.iter() {
                            if ui.selectable_label(name == now, tl!(name.as_str())).clicked() {
                                pick = Some(name.clone());
                            }
                        }
                    })
                    .response
                    .on_hover_text(tl!("The style new objects take; chosen with objects selected, they take it too"));
                if let Some(name) = pick {
                    run(app, "ui.cadStyles", json!({"kind": kind, "current": name}));
                }
                ui.end_row();
            }
        });
    });
}

/// Runs a style command; an error goes to the status bar.
fn run(app: &mut WordApp, id: &str, params: serde_json::Value) {
    if let Err(e) = app.run(id, params) {
        app.status(e);
    }
}

/// The open style window, if any, as a modal window over the editor; Close or Esc closes it.
pub(crate) fn show(app: &mut WordApp, ctx: &egui::Context) {
    let Some(which) = app.cad.as_ref().and_then(|e| e.style_dialog) else { return };
    let resp = egui::Modal::new(egui::Id::unique(("cad_style_dialog", which.name()))).show(ctx, |ui| {
        ui.set_min_width(380.0);
        ui.set_max_width(420.0);
        ui.heading(tl!(which.title()));
        ui.add_space(6.0);
        match which {
            StyleDialog::Dim => dim_body(app, ui),
            StyleDialog::Level => level_body(app, ui),
            StyleDialog::Text => text_body(app, ui),
        }
        ui.add_space(10.0);
        ui.separator();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.add(crate::widgets::primary_button(ui, tl!("Close"))).clicked())
            .inner
    });
    if (resp.inner || resp.should_close())
        && let Some(ed) = app.cad.as_mut()
    {
        ed.style_dialog = None;
    }
}

/// The style list: the current style (choosing one with objects selected gives it to them), its
/// name to rename (Standard keeps its name), New (a copy, made current) and Delete.
fn style_list(app: &mut WordApp, ui: &mut Ui, kind: StyleKind) {
    let Some((styles, current)) = app.cad.as_ref().map(|e| {
        let d = &e.drawing;
        let now = match kind {
            StyleKind::Dim => d.current_dim_style.clone(),
            StyleKind::Text => d.current_text_style.clone(),
        };
        (d.styles(kind), now.unwrap_or_else(|| strix_cad::STANDARD.to_string()))
    }) else {
        return;
    };
    let k = match kind {
        StyleKind::Dim => "dim",
        StyleKind::Text => "text",
    };
    field_grid(ui, &format!("cad_style_list_{k}"), |ui| {
        ui.label(tl!("Style"));
        let mut pick = None;
        craft_ui::ComboBox::from_id_salt(("cad_style_current", k)).selected_text(tl!(current.as_str())).width(FIELD_W + 80.0).show_ui(ui, |ui| {
            for name in &styles {
                if ui.selectable_label(*name == current, tl!(name.as_str())).clicked() {
                    pick = Some(name.clone());
                }
            }
        });
        if let Some(name) = pick {
            run(app, "ui.cadStyles", json!({"kind": k, "current": name}));
        }
        ui.end_row();
        ui.label(tl!("Name"));
        let standard = current == strix_cad::STANDARD;
        ui.scope(|ui| {
            ui.set_width(FIELD_W + 80.0);
            if let Some(to) = crate::a11y_pane::draft_box(ui, ("cad_style_name", k, current.as_str()), &current, !standard, false) {
                run(app, "ui.cadStyles", json!({"kind": k, "rename": current, "to": to}));
            }
        });
        ui.end_row();
        ui.label("");
        ui.horizontal(|ui| {
            if ui.button(tl!("New")).on_hover_text(tl!("A new style, a copy of this one, made current")).clicked() {
                let base = match kind {
                    StyleKind::Dim => tl!("Dimension style"),
                    StyleKind::Text => tl!("Text style"),
                };
                run(app, "ui.cadStyles", json!({"kind": k, "new": base}));
            }
            if ui
                .add_enabled(!standard, egui::Button::new(tl!("Delete")))
                .on_hover_text(tl!("Delete this style: its objects take Standard"))
                .clicked()
            {
                run(app, "ui.cadStyles", json!({"kind": k, "delete": current}));
            }
        });
        ui.end_row();
    });
}

/// The current dimension style, its name and its settings.
fn current_dim(app: &WordApp) -> Option<(strix_cad::DimStyle, String, Vec<String>)> {
    app.cad.as_ref().map(|e| {
        let d = &e.drawing;
        let current = d.current_dim_style.clone().unwrap_or_else(|| strix_cad::STANDARD.to_string());
        (d.current_dim().clone(), current, d.styles(StyleKind::Text))
    })
}

/// A size of the current dimension style, mm, applied once set (one undo step).
fn size_row(ui: &mut Ui, app: &mut WordApp, (label, hint): (&str, &str), key: &str, current: &str, now: f64, max: f64) {
    let l = ui.label(tl!(label));
    if !hint.is_empty() {
        l.on_hover_text(tl!(hint));
    }
    if let Some(v) =
        crate::widgets::value_field(ui, (key, current), vec2(FIELD_W, 18.0), now, |d| d.range(0.0..=max).speed(0.05).max_decimals(2).suffix(" mm"))
    {
        run(app, "ui.cadDimStyle", json!({ key: v }));
    }
    ui.end_row();
}

fn dim_body(app: &mut WordApp, ui: &mut Ui) {
    style_list(app, ui, StyleKind::Dim);
    let Some((style, current, text_styles)) = current_dim(app) else { return };
    ui.add_space(8.0);
    ui.label(egui::RichText::new(tl!("Marks")).strong());
    let mark_combo = |ui: &mut Ui, app: &mut WordApp, key: &str, now: Option<strix_cad::Mark>| {
        // The end mark offers "As the start" (none of its own).
        let shown = now.map_or(tl!("As the start"), |m| tl!(crate::cad_panel::choice_label(m.name())));
        craft_ui::ComboBox::from_id_salt(("cad_dim_mark", key)).selected_text(shown).width(FIELD_W + 24.0).show_ui(ui, |ui| {
            if key == "markEnd" && ui.selectable_label(now.is_none(), tl!("As the start")).clicked() {
                run(app, "ui.cadDimStyle", json!({key: "start"}));
            }
            for (m, name) in strix_cad::Mark::ALL {
                if ui.selectable_label(now == Some(m), tl!(crate::cad_panel::choice_label(name))).clicked() {
                    run(app, "ui.cadDimStyle", json!({key: name}));
                }
            }
        });
    };
    field_grid(ui, "cad_dim_style_marks", |ui| {
        ui.label(tl!("Start mark"));
        mark_combo(ui, app, "mark", Some(style.mark));
        ui.end_row();
        ui.label(tl!("End mark"));
        mark_combo(ui, app, "markEnd", style.mark_end);
        ui.end_row();
        size_row(ui, app, ("Mark size", ""), "markSize", &current, style.mark_size, 20.0);
        let hint = "The width of the marks' lines; 0: automatic (a tick heavier than the line, an arrow as thin)";
        size_row(ui, app, ("Mark width", hint), "markWidth", &current, style.mark_width, 2.0);
    });
    ui.add_space(8.0);
    ui.label(egui::RichText::new(tl!("Lines")).strong());
    field_grid(ui, "cad_dim_style_lines", |ui| {
        size_row(ui, app, ("Line width", ""), "lineWidth", &current, style.line_width, 2.0);
        size_row(ui, app, ("Line beyond", ""), "lineOver", &current, style.line_over, 20.0);
        size_row(ui, app, ("Extension gap", ""), "extGap", &current, style.ext_gap, 20.0);
        size_row(ui, app, ("Extension beyond", ""), "extOver", &current, style.ext_over, 20.0);
    });
    ui.add_space(8.0);
    ui.label(egui::RichText::new(tl!("Text")).strong());
    field_grid(ui, "cad_dim_style_text", |ui| {
        size_row(ui, app, ("Text height", ""), "text", &current, style.text, 50.0);
        size_row(ui, app, ("Text gap", ""), "textGap", &current, style.text_gap, 20.0);
        ui.label(tl!("Text style"));
        let shown = style.text_style.clone().unwrap_or_else(|| strix_cad::STANDARD.to_string());
        craft_ui::ComboBox::from_id_salt("cad_dim_text_style").selected_text(tl!(shown.as_str())).width(FIELD_W + 24.0).show_ui(ui, |ui| {
            for name in &text_styles {
                if ui.selectable_label(*name == shown, tl!(name.as_str())).clicked() {
                    run(app, "ui.cadDimStyle", json!({"textStyle": name}));
                }
            }
        });
        ui.end_row();
        ui.label(tl!("Dimension units"));
        craft_ui::ComboBox::from_id_salt("cad_dim_units").selected_text(tl!(style.units.label())).width(FIELD_W + 60.0).show_ui(ui, |ui| {
            for (u, name, label) in strix_cad::DimUnits::ALL {
                if ui.selectable_label(style.units == u, tl!(label)).clicked() {
                    run(app, "ui.cadDimStyle", json!({"units": name}));
                }
            }
        });
        ui.end_row();
    });
    let mut show_unit = style.show_unit;
    if ui.checkbox(&mut show_unit, tl!("Show the unit")).on_hover_text(tl!("Sizes end with their unit (3.50 m); off, the number alone")).changed() {
        run(app, "ui.cadDimStyle", json!({"showUnit": show_unit}));
    }
    let mut aligned = style.aligned;
    if ui.checkbox(&mut aligned, tl!("Aligned texts")).on_hover_text(tl!("Dimension texts along their line (never upside down), not level")).changed()
    {
        run(app, "ui.cadDimStyle", json!({"aligned": aligned}));
    }
    let mut scaled = style.scaled;
    if ui
        .checkbox(&mut scaled, tl!("Scaled distances"))
        .on_hover_text(tl!(
            "Dimension lines and angle arcs placed with a click or a grip keep their distance in the drawing (they follow its size and scale); off, on paper (the same in print)"
        ))
        .changed()
    {
        run(app, "ui.cadDimStyle", json!({"scaled": scaled}));
    }
}

fn level_body(app: &mut WordApp, ui: &mut Ui) {
    ui.label(egui::RichText::new(tl!("Level marks take the dimension styles: these settings are the style's.")).weak());
    style_list(app, ui, StyleKind::Dim);
    let Some((style, current, _)) = current_dim(app) else { return };
    ui.add_space(8.0);
    field_grid(ui, "cad_level_style", |ui| {
        size_row(ui, app, ("Text height", "0: the dimension text height"), "levelText", &current, style.level_text, 50.0);
        size_row(ui, app, ("Triangle height", "0: automatic (a text and a fifth)"), "levelMark", &current, style.level_mark, 50.0);
        size_row(ui, app, ("Text gap", ""), "textGap", &current, style.text_gap, 20.0);
    });
    let mut unit = style.level_unit;
    if ui.checkbox(&mut unit, tl!("Show the unit")).on_hover_text(tl!("Elevations end with their unit (+3.00 m); off, the number alone")).changed() {
        run(app, "ui.cadDimStyle", json!({"levelUnit": unit}));
    }
}

fn text_body(app: &mut WordApp, ui: &mut Ui) {
    style_list(app, ui, StyleKind::Text);
    let Some((style, current)) = app.cad.as_ref().map(|e| {
        let d = &e.drawing;
        let current = d.current_text_style.clone().unwrap_or_else(|| strix_cad::STANDARD.to_string());
        (d.text_style_named(d.current_text_style.as_deref()).cloned(), current)
    }) else {
        return;
    };
    ui.add_space(8.0);
    let set = |app: &mut WordApp, params: serde_json::Value| run(app, "ui.cadTextStyle", params);
    let fields = ui.add_enabled_ui(style.is_some(), |ui| {
        let st = style.clone().unwrap_or_default();
        field_grid(ui, "cad_text_style", |ui| {
            ui.label(tl!("Font"));
            let shown = st.font.clone().map_or_else(|| tl!("Document font").to_string(), |f| app.previews.font_shown(&f));
            let fams = app.previews.families();
            if let Some(f) = crate::widgets::font_combo(ui, "cad_text_font", FIELD_W + 80.0, &shown, &fams, None)
                && f != crate::widgets::ADD_FONT
                && f != crate::widgets::LOAD_SYSTEM_FONTS
            {
                let f = app.previews.font_stored(&f);
                set(app, json!({"font": f}));
            }
            ui.end_row();
            ui.label(tl!("Height"));
            if let Some(v) = crate::widgets::value_field(ui, ("cad_text_height", current.as_str()), vec2(FIELD_W, 18.0), st.height, |d| {
                d.range(0.5..=50.0).speed(0.05).max_decimals(2).suffix(" mm")
            }) {
                set(app, json!({"height": v}));
            }
            ui.end_row();
        });
        ui.horizontal(|ui| {
            let mut bold = st.bold;
            if ui.checkbox(&mut bold, tl!("Bold")).changed() {
                set(app, json!({"bold": bold}));
            }
            let mut italic = st.italic;
            if ui.checkbox(&mut italic, tl!("Italic")).changed() {
                set(app, json!({"italic": italic}));
            }
        });
    });
    if style.is_none() {
        fields.response.on_disabled_hover_text(tl!("Standard: the document's font. New makes a style of your own."));
    }
}
