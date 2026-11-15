//! Strix's contextual Exercise and Series tabs (docs/exercise.md): every setting of an exercise
//! and of a series on the ribbon, where the Exercise Properties pane used to hold them. The
//! Exercise tab comes up with the cursor in an exercise; the Series tab whenever the document has
//! more than one. What needs more room than a ribbon row (the questions' settings, the data, the
//! variant generator, the graph paper) opens in a panel under its button that stays open while it
//! is used. Kept apart from the upstream tab files (docs/upstream.md).

use egui::{Rect, Sense, pos2, vec2};

use super::common::{mi_text, stack};
use super::language::mi_check;
use super::{Ui, Value, WordApp, group, json, menu_button, mi, small};
use crate::theme::{Tokens, regular};
use crate::widgets::{BIG_ICON_Y, BIG_LABEL_Y, ButtonAction, CONTENT_H, LABEL_PITCH, MenuButtonOptions};

/// The tab of the exercise at the cursor.
pub(crate) const EXERCISE_TAB: &str = "Exercise";
/// The tab of the series, when the document has more than one exercise.
pub(crate) const SERIES_TAB: &str = "Series";

/// The exercise tabs the document and the cursor call for (pure, tested).
pub(super) fn contextual(s: &wordcraft_engine::Session) -> Vec<&'static str> {
    let (here, count) = wordcraft_engine::cmd::exercise::context(s);
    let mut tabs = Vec::new();
    if here {
        tabs.push(EXERCISE_TAB);
    }
    if count > 1 {
        tabs.push(SERIES_TAB);
    }
    tabs
}

/// Exercise Properties (`view.exercisePane`: the grip's menu, command search, scripts) brings up
/// the Exercise tab, or the Series tab with the cursor outside the exercises; there is no pane.
pub(super) fn follow(app: &mut WordApp) {
    if !app.session.view.exercise_pane {
        return;
    }
    app.session.view.exercise_pane = false;
    let tabs = contextual(&app.session);
    if let Some(tab) = tabs.first() {
        app.ui.tab = (*tab).to_string();
    }
}

/// Whether the document has more than one exercise (the series rules their layout).
fn in_series(app: &WordApp) -> bool {
    wordcraft_engine::cmd::exercise::context(&app.session).1 > 1
}

/// Runs a command, its error in the status bar.
fn run(app: &mut WordApp, id: &str, params: Value) {
    if let Err(e) = app.run(id, params) {
        app.status(e);
    }
}

/// What only some types of question use, on the Exercise tab after what every exercise has (pure,
/// tested): each is on only when the exercise's questions are of a type that uses it (the
/// commands stay on for scripts).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Peculiar {
    /// A quiz: Add Question offers the types of question (a problem's questions have none).
    pub(super) types: bool,
    /// Result Of: a problem's results, or a calculation's or worked example's.
    pub(super) results: bool,
    /// The data and the variants the values are calculated with: those, an inline completion's
    /// calculated blanks, or data already there.
    pub(super) data: bool,
    /// Hide Words: an inline completion question.
    pub(super) hide_words: bool,
}

/// [`Peculiar`] for the exercise `q_st` (`exercise.questions`) describes, `has_data` when it
/// already has data.
pub(super) fn peculiar(q_st: &Value, has_data: bool) -> Peculiar {
    let problem = q_st["kind"].as_str().unwrap_or("problem") == "problem";
    let modes = modes(q_st);
    let any = |m: &[&str]| modes.iter().any(|x| m.contains(&x.as_str()));
    let results = problem || any(&["calc", "worked"]);
    Peculiar { types: !problem, results, data: results || has_data || any(&["cloze"]), hide_words: any(&["cloze"]) }
}

/// The type of every question of the exercise `q_st` describes, in order.
fn modes(q_st: &Value) -> Vec<String> {
    q_st["questions"].as_array().map(|a| a.iter().map(|q| crate::exercise_quiz::mode_of(q).to_string()).collect()).unwrap_or_default()
}

/// The name of the exercise's type (pure, tested): a problem; a quiz whose questions are all of
/// one type, that type's (True or False, Inline Completion…); else its kind's.
pub(super) fn type_name(q_st: &Value) -> &'static str {
    let kind = q_st["kind"].as_str().unwrap_or("problem");
    if kind == "problem" {
        return "Problem";
    }
    let modes = modes(q_st);
    if let Some(first) = modes.first()
        && modes.iter().all(|m| m == first)
        && let Some((_, name)) = crate::exercise_quiz::QUESTION_MODES.iter().find(|(m, _)| m == first)
    {
        return name;
    }
    if kind == "choice" { "Multiple Choice" } else { "Open Answer" }
}

/// The Exercise tab: first what every exercise has (its first line, duration and type, adding
/// questions and notes and their settings, the layout, Select and Delete), then what only some
/// types of question use, on only for them: Result Of, the data and variants, Hide Words.
pub(super) fn exercise_tab(app: &mut WordApp, ui: &mut Ui) {
    let Ok(info) = app.session.run("exercise.data", &json!({})) else { return };
    let ex = info["exercise"].as_u64().unwrap_or(0);
    let q_st = app.session.run("exercise.questions", &json!({"exercise": ex})).ok().unwrap_or_default();
    let has_data = info["data"]["items"].as_array().is_some_and(|a| !a.is_empty());
    let pec = peculiar(&q_st, has_data);
    general_group(app, ui, ex, &info, type_name(&q_st));
    questions_group(app, ui, ex, &q_st, pec.types);
    if in_series(app) {
        group(ui, "Series", None, app, |ui, app| {
            go_to_series(ui, app);
        });
    } else {
        layout_group(app, ui, false);
    }
    exercise_group(app, ui);
    calculation_group(app, ui, ex, &info, pec);
    completion_group(app, ui, pec);
}

/// Exercise › Exercise: Select and Delete.
fn exercise_group(app: &mut WordApp, ui: &mut Ui) {
    group(ui, "Exercise", None, app, |ui, app| {
        stack(ui, |ui| {
            let span = app
                .session
                .doc
                .control_ranges(wordcraft_doc::StoryRef::Body)
                .into_iter()
                .find(|r| r.control.tag == wordcraft_engine::cmd::exercise::TAG_EXERCISE && r.contains(&app.session.sel.focus));
            if let Some(r) = span {
                let end = wordcraft_doc::Pos { off: r.end.off.saturating_add(wordcraft_doc::para::OBJ.len_utf8()), ..r.end.clone() };
                small(
                    ui,
                    app,
                    "select",
                    Some("Select Exercise"),
                    "Select the whole exercise: its statement, questions and solution",
                    ButtonAction::new("select.range", json!({"anchor": r.start, "focus": end}), false),
                );
            }
            small(
                ui,
                app,
                "deleteTable",
                Some("Delete Exercise"),
                "Delete the exercise at the cursor, or the exercises selected, whole: its statement, questions and solution go, its results leave the summary (Undo brings it back)",
                ButtonAction::new("exercise.delete", json!({}), false),
            );
        });
    });
}

/// A button that brings up the Series tab.
fn go_to_series(ui: &mut Ui, app: &mut WordApp) {
    let t = Tokens::get(ui.ctx());
    let label = tl!("Series\nSettings");
    let (r, resp) = big_rect(ui, label);
    crate::widgets::bg(ui, r, &resp, false, &t);
    big_face(ui, r, "exercise", label, false, true);
    let tip = tl!("Page breaks, parts shown, graph paper and numbering are the series': they are in the Series tab");
    let resp = resp.on_hover_text(tip);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tip));
    if resp.clicked() {
        app.ui.tab = SERIES_TAB.into();
    }
}

/// Exercise › Questions: add a question (of a type, in a quiz), a sub-question or a note, and every
/// question's settings in a panel.
fn questions_group(app: &mut WordApp, ui: &mut Ui, ex: u64, q_st: &Value, quiz: bool) {
    group(ui, "Questions", None, app, |ui, app| {
        if quiz {
            crate::widgets::big_split(
                ui,
                app,
                crate::widgets::BigSplit {
                    icon: "addQuestion",
                    label: "Add Question",
                    tip: "Add a question at the end of the exercise; the menu offers the types: open answer, multiple choice, true or false, calculation, CAD drawing…",
                    action: ButtonAction::new("exercise.addQuestion", json!({}), false),
                },
                |ui, app| {
                    for (id, name) in crate::exercise_quiz::QUESTION_MODES {
                        mi(ui, app, name, "exercise.addQuestion", json!({"type": id}));
                    }
                },
            );
        } else {
            super::big(ui, app, "addQuestion", "Add Question", "exercise.addQuestion", json!({}), false)
                .on_hover_text(tl!("Add a question at the end of the exercise"));
        }
        stack(ui, |ui| {
            small(
                ui,
                app,
                "addSubQuestion",
                Some("Add Sub-question"),
                "Add a question nested under the last one (a.1, a.1.1); Tab and Shift+Tab change a question's level",
                ButtonAction::new("exercise.addQuestion", json!({"sub": true}), false),
            );
            small(
                ui,
                app,
                "addNote",
                Some("Add Note"),
                "Add a note between the questions: free text that is not a question",
                ButtonAction::new("exercise.addNote", json!({}), false),
            );
        });
        let st = q_st.clone();
        panel_button(
            ui,
            app,
            PanelButton {
                key: "questions",
                icon: "exerciseProperties",
                label: "Question\nSettings",
                tip: "Every question with its type and points, and the settings of the one chosen: answer space, correct options, tolerance…",
                on: true,
            },
            |ui, app| crate::exercise_quiz::questions(app, ui, ex, &st, quiz),
        );
    });
}

/// Exercise › General: what the first line holds, the duration, the type.
fn general_group(app: &mut WordApp, ui: &mut Ui, ex: u64, info: &Value, type_name: &str) {
    let line = app.session.run("exercise.titleLine", &json!({"exercise": ex})).ok().and_then(|v| v["line"].as_str().map(str::to_string));
    let line = line.unwrap_or_else(|| "title".into());
    group(ui, "General", None, app, |ui, app| {
        stack(ui, |ui| {
            menu_button(
                ui,
                app,
                MenuButtonOptions::new(
                    "addText",
                    Some("First Line"),
                    "What the exercise's first line holds after the number: a title, or the statement's text",
                    false,
                    "exercise.titleLine",
                ),
                |ui, app| {
                    for (id, label) in [("title", "Title"), ("statement", "Text next to the number (no title)")] {
                        mi_check(ui, app, label, line == id, "exercise.titleLine", json!({"exercise": ex, "line": id}));
                    }
                },
            );
            crate::widgets::row(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let name = ui.label(egui::RichText::new(tl!("Duration")).small());
                let dur = info["data"]["duration"].as_u64().map(|d| d.to_string()).unwrap_or_default();
                ui.scope(|ui| {
                    ui.set_width(40.0);
                    if let Some(t) = named_draft_box(ui, ("exercise_duration", ex), &dur, name.id) {
                        let mut data = info["data"].clone();
                        let parsed = t.trim().parse::<u32>().ok().filter(|&n| (1..=600).contains(&n));
                        data["duration"] = parsed.map_or(Value::Null, |n| json!(n));
                        run(app, "exercise.setData", json!({"exercise": ex, "data": data}));
                    }
                });
                ui.label(egui::RichText::new("min").small());
            });
            crate::widgets::row(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(egui::RichText::new(tl!(type_name)).small().weak());
            });
        });
    });
}

/// Exercise › Inline Completion: Hide Words, on only with an inline completion question.
fn completion_group(app: &mut WordApp, ui: &mut Ui, pec: Peculiar) {
    group(ui, "Inline Completion", None, app, |ui, app| {
        stack(ui, |ui| {
            enabled_if(ui, pec.hide_words, |ui| {
                small(
                    ui,
                    app,
                    "hideWords",
                    Some("Hide Words"),
                    "Hide the selected words for a text to complete by hand: the corrected copy shows them, the students' copy a line to write on ([[word]] typed in an inline completion question does the same)",
                    ButtonAction::new("exercise.blank", json!({}), false),
                );
            });
        });
    });
}

/// Exercise › Calculation: Result Of (a problem's, a calculation's), the variant the exercise is
/// calculated with, add and remove one, the data and the generator in panels; each on only when
/// the exercise's type of question uses it.
fn calculation_group(app: &mut WordApp, ui: &mut Ui, ex: u64, info: &Value, pec: Peculiar) {
    let data = info["data"].clone();
    let variants: Vec<String> =
        data["variants"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
    let active = data["active"].as_u64().unwrap_or(0) as usize;
    let has_items = data["items"].as_array().is_some_and(|a| !a.is_empty());
    group(ui, "Calculation", None, app, |ui, app| {
        stack(ui, |ui| {
            enabled_if(ui, pec.results, |ui| super::strix::result_of_menu(ui, app));
        });
        enabled_if(ui, pec.data, |ui| {
            stack(ui, |ui| {
                crate::widgets::row(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let name = ui.label(egui::RichText::new(tl!("Variant")).small());
                    let shown = variants.get(active).cloned().unwrap_or_else(|| "A".into());
                    let errors = &info["variantErrors"];
                    ui.add_enabled_ui(variants.len() > 1, |ui| {
                        let combo = craft_ui::ComboBox::from_id_salt(("exercise_variant", ex)).selected_text(shown).width(56.0).show_ui(ui, |ui| {
                            for (k, name) in variants.iter().enumerate() {
                                let errs: Vec<&str> = errors
                                    .get(name.as_str())
                                    .and_then(Value::as_array)
                                    .map(|a| a.iter().filter_map(Value::as_str).collect())
                                    .unwrap_or_default();
                                let text = if errs.is_empty() { name.clone() } else { format!("{name} ⚠") };
                                let r = ui.selectable_label(k == active, text);
                                let r = if errs.is_empty() { r } else { r.on_hover_text(format!("⚠ {}", errs.join("\n⚠ "))) };
                                if r.clicked() && k != active {
                                    run(app, "exercise.variant", json!({"exercise": ex, "active": k}));
                                }
                            }
                        });
                        combo.response.labelled_by(name.id);
                    });
                });
                let mut added = data.clone();
                crate::exercise_panels::add_variant(&mut added);
                enabled_if(ui, has_items && variants.len() < crate::exercise_panels::MAX_VARIANTS, |ui| {
                    small(
                        ui,
                        app,
                        "plus",
                        Some("Add Variant"),
                        "Add a variant: a copy of the active one's values (add data first)",
                        ButtonAction::new("exercise.setData", json!({"exercise": ex, "data": added}), false),
                    );
                });
                let mut removed = data.clone();
                crate::exercise_panels::remove_variant(&mut removed);
                enabled_if(ui, variants.len() > 1, |ui| {
                    small(
                        ui,
                        app,
                        "minus",
                        Some("Remove Variant"),
                        "Remove the active variant",
                        ButtonAction::new("exercise.setData", json!({"exercise": ex, "data": removed}), false),
                    );
                });
            });
        });
        let panel_info = info.clone();
        panel_button(
            ui,
            app,
            PanelButton {
                key: "data",
                icon: "variables",
                label: "Data",
                tip: "The initial data: an equation per datum (L = 4.5 m) with its description, and the variants",
                on: pec.data,
            },
            |ui, app| crate::exercise_panels::data_panel(app, ui, &panel_info),
        );
        let gen_data = data.clone();
        panel_button(
            ui,
            app,
            PanelButton {
                key: "generate",
                icon: "generateVariants",
                label: "Generate\nVariants",
                tip: "Generate clean variants from each datum's range, checked against the constraints on the results (add data first)",
                on: pec.data && has_items,
            },
            |ui, app| crate::exercise_panels::variant_generator_ui(app, ui, ex, &gen_data),
        );
    });
}

/// Draws `add` greyed out and unclickable unless `on` (the command stays on for scripts).
fn enabled_if(ui: &mut Ui, on: bool, add: impl FnOnce(&mut Ui)) {
    ui.add_enabled_ui(on, |ui| {
        if !on {
            ui.multiply_opacity(0.4);
        }
        add(ui);
    });
}

/// Layout: the parts shown, the page breaks, the frontispiece and the graph paper; of the lone
/// exercise, or (`series`) of every exercise of the series.
fn layout_group(app: &mut WordApp, ui: &mut Ui, series: bool) {
    group(ui, "Layout", None, app, |ui, app| {
        stack(ui, |ui| {
            super::strix::show_menu(ui, app);
            page_breaks_menu(ui, app, series);
            let front = app.session.run("exercise.frontispiece", &json!({})).ok().is_some_and(|v| v["on"].as_bool().unwrap_or(false));
            small(
                ui,
                app,
                "coverPage",
                Some("Frontispiece"),
                "Pages of your own before the exercises (title, class, date, instructions), ended by a Frontispiece Break",
                ButtonAction::new("exercise.frontispiece", json!({"on": !front}), front),
            );
        });
        let grid = app.session.run("exercise.show", &json!({})).ok().map(|st| st["grid"].clone()).unwrap_or(Value::Null);
        let on = !grid.is_null();
        panel_button(
            ui,
            app,
            PanelButton {
                key: "grid",
                icon: "graphPaperProperties",
                label: "Graph\nPaper",
                tip: "The graph paper after the questions, before the solution: its squares, lines and colour (Show › Graph Paper Before the Solution adds it)",
                on,
            },
            |ui, app| {
                let theme = app.session.doc.settings.theme_colors.clone();
                if let Some(change) = crate::graph_pane::editor(ui, &grid, false, &theme) {
                    run(app, "exercise.grid", change);
                }
            },
        );
    });
}

/// Layout › Page Breaks (`exercise.pageBreaks`): before the solution and the results summary, and
/// in a series before every exercise but the first.
fn page_breaks_menu(ui: &mut Ui, app: &mut WordApp, series: bool) {
    let b = app.session.run("exercise.pageBreaks", &json!({})).ok().unwrap_or_default();
    menu_button(
        ui,
        app,
        MenuButtonOptions::new("pageBreak", Some("Page Breaks"), "Where the exercises start a new page", false, "exercise.pageBreaks"),
        |ui, app| {
            let mut items = vec![("beforeSolution", if series { "New page before each solution" } else { "New page before the solution" })];
            if series {
                items.insert(0, ("beforeExercise", "New page before each exercise"));
            }
            items.push(("beforeResults", "New page before the results summary"));
            for (key, label) in items {
                let on = b[key].as_bool().unwrap_or(false);
                mi_check(ui, app, label, on, "exercise.pageBreaks", json!({ key: !on }));
            }
        },
    );
}

/// The Series tab: every exercise to go to or delete, the numbering, and the layout every
/// exercise of the series shares.
pub(super) fn series_tab(app: &mut WordApp, ui: &mut Ui) {
    let info = app.session.run("exercise.info", &json!({})).ok().unwrap_or_default();
    let here = info["exercise"].as_u64();
    let list = info["list"].as_array().cloned().unwrap_or_default();
    let name = |e: &Value| {
        let title: String = e["title"].as_str().unwrap_or_default().chars().take(40).collect();
        format!("{} {title}", e["label"].as_str().unwrap_or_default()).trim().to_string()
    };
    group(ui, "Exercises", None, app, |ui, app| {
        menu_button(
            ui,
            app,
            MenuButtonOptions::new("exercise", Some("Go To\nExercise"), "Put the cursor in an exercise's title", true, "caret.set"),
            |ui, app| {
                for (k, e) in list.iter().enumerate() {
                    let current = here == Some(k as u64 + 1);
                    if ui.add(egui::Button::new(name(e)).selected(current)).clicked() {
                        go_to(app, &e["pos"]);
                        ui.close();
                    }
                }
            },
        );
        stack(ui, |ui| {
            let at = here.map(|n| n.saturating_sub(1) as usize);
            let previous = at.and_then(|i| i.checked_sub(1)).and_then(|i| list.get(i));
            let next = match at {
                Some(i) => list.get(i + 1),
                None => list.first(),
            };
            for (icon, label, tip, target) in [
                ("chevronUp", "Previous Exercise", "Put the cursor in the previous exercise's title", previous),
                ("chevronDown", "Next Exercise", "Put the cursor in the next exercise's title", next),
            ] {
                enabled_if(ui, target.is_some(), |ui| {
                    let pos = target.map(|e| e["pos"].clone()).unwrap_or(Value::Null);
                    small(ui, app, icon, Some(label), tip, ButtonAction::new("caret.set", json!({"pos": pos}), false));
                });
            }
            menu_button(
                ui,
                app,
                MenuButtonOptions::new(
                    "deleteTable",
                    Some("Delete Exercise"),
                    "Delete an exercise whole: its statement, questions and solution (Undo brings it back)",
                    false,
                    "exercise.delete",
                ),
                |ui, app| {
                    for (k, e) in list.iter().enumerate() {
                        mi_text(ui, app, &name(e), "exercise.delete", json!({"exercise": k + 1}));
                    }
                },
            );
        });
    });
    numbering_group(app, ui);
    layout_group(app, ui, true);
}

/// Puts the cursor at `pos` (an exercise's title) and back in the document.
fn go_to(app: &mut WordApp, pos: &Value) {
    run(app, "caret.set", json!({"pos": pos}));
    app.canvas.want_focus = true;
}

/// The number formats offered for exercises: the format's id and how it counts.
const FORMATS: [(&str, &str); 5] =
    [("decimal", "1, 2, 3"), ("upperLetter", "A, B, C"), ("lowerLetter", "a, b, c"), ("upperRoman", "I, II, III"), ("lowerRoman", "i, ii, iii")];

/// The separators offered after an exercise's number (`exercise.numbering`'s ids) and their names.
const SEPARATORS: [(&str, &str); 5] =
    [(".", "Period (1.)"), (":", "Colon (1:)"), ("-", "Dash (1 -)"), (")", "Parenthesis (1))"), (" ", "Space (1 Title)")];

/// Series › Numbering, a document setting (`exercise.numbering`): the prefix, number format,
/// separator and first number of every exercise's label, in the titles and the results summary.
fn numbering_group(app: &mut WordApp, ui: &mut Ui) {
    let Ok(n) = app.session.run("exercise.numbering", &json!({})) else { return };
    let prefix = n["prefix"].as_str().unwrap_or_default().to_string();
    let format = n["format"].as_str().unwrap_or("decimal").to_string();
    let start = n["start"].as_u64().unwrap_or(1);
    let separator = n["separator"].as_str().unwrap_or(".").to_string();
    let default = n["default"].as_bool().unwrap_or(false);
    let example = n["example"].as_str().unwrap_or_default().to_string();
    group(ui, "Numbering", None, app, |ui, app| {
        stack(ui, |ui| {
            crate::widgets::row(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let name = ui.label(egui::RichText::new(tl!("Prefix")).small());
                ui.scope(|ui| {
                    ui.set_width(90.0);
                    if let Some(t) = named_draft_box(ui, "exercise_numbering_prefix", &prefix, name.id) {
                        run(app, "exercise.numbering", json!({"prefix": t}));
                    }
                });
                if ui
                    .add_enabled(!default, egui::Button::new(egui::RichText::new(tl!("Default")).small()))
                    .on_hover_text(tl!("Exercise, in the document's language"))
                    .clicked()
                {
                    run(app, "exercise.numbering", json!({"prefix": null}));
                }
            });
            crate::widgets::row(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let name = ui.label(egui::RichText::new(tl!("Number format")).small());
                let shown = FORMATS.iter().find(|(id, _)| *id == format).map_or("1, 2, 3", |(_, l)| l);
                let combo = craft_ui::ComboBox::from_id_salt("exercise_numbering_format").selected_text(shown).width(90.0).show_ui(ui, |ui| {
                    for (id, label) in FORMATS {
                        if ui.selectable_label(id == format, label).clicked() && id != format {
                            run(app, "exercise.numbering", json!({"format": id}));
                        }
                    }
                });
                combo.response.labelled_by(name.id);
            });
            crate::widgets::row(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let name = ui.label(egui::RichText::new(tl!("Separator")).small());
                let shown = SEPARATORS.iter().find(|(id, _)| *id == separator).map_or(separator.clone(), |(_, l)| tl!(l).to_string());
                let combo = craft_ui::ComboBox::from_id_salt("exercise_numbering_separator").selected_text(shown).width(110.0).show_ui(ui, |ui| {
                    for (id, label) in SEPARATORS {
                        if ui.selectable_label(id == separator, tl!(label)).clicked() && id != separator {
                            run(app, "exercise.numbering", json!({"separator": id}));
                        }
                    }
                });
                combo.response.labelled_by(name.id);
            });
        });
        stack(ui, |ui| {
            crate::widgets::row(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let name = ui.label(egui::RichText::new(tl!("Start at")).small());
                let mut value = start;
                if ui.add(egui::DragValue::new(&mut value).range(1..=32767)).labelled_by(name.id).changed() && value != start {
                    run(app, "exercise.numbering", json!({"start": value}));
                }
            });
            crate::widgets::row(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(egui::RichText::new(crate::i18n::fmt(tl!("First exercise: {label}"), &[("label", &example)])).small().weak());
            });
        });
    });
}

/// A text box editing `current`, named by the label `name` for screen readers; the draft lives
/// in egui memory while it has focus. Returns the new text when editing ends with a change.
fn named_draft_box(ui: &mut Ui, key: impl std::hash::Hash + std::fmt::Debug, current: &str, name: egui::Id) -> Option<String> {
    let key = egui::Id::unique(key);
    let mut text = ui.data(|d| d.get_temp::<String>(key)).unwrap_or_else(|| current.to_string());
    let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(f32::INFINITY)).labelled_by(name);
    if r.changed() {
        ui.data_mut(|d| d.insert_temp(key, text.clone()));
    }
    if r.lost_focus() {
        ui.data_mut(|d| d.remove::<String>(key));
        return (text != current).then_some(text);
    }
    if !r.has_focus() {
        ui.data_mut(|d| d.remove::<String>(key));
    }
    None
}

/// A panel button: what it is, and whether it opens.
struct PanelButton<'a> {
    /// Tells its open state apart from the other panels'.
    key: &'a str,
    icon: &'a str,
    /// English, translated when drawn; `\n` splits it in two lines.
    label: &'a str,
    tip: &'a str,
    on: bool,
}

/// A large ribbon button whose panel (`add`) opens under it and stays open while it is used: only
/// a click outside it (on the ribbon or the document), the button again or Esc closes it. Clicks
/// in the lists its controls open count as inside.
fn panel_button(ui: &mut Ui, app: &mut WordApp, button: PanelButton<'_>, add: impl FnOnce(&mut Ui, &mut WordApp)) {
    let t = Tokens::get(ui.ctx());
    let (label, tip) = (tl!(button.label), tl!(button.tip));
    let (r, resp) = big_rect(ui, label);
    let open_id = egui::Id::unique(("strix_exercise_panel", button.key));
    let was_open = button.on && ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    if button.on {
        crate::widgets::bg(ui, r, &resp, was_open, &t);
    }
    big_face(ui, r, button.icon, label, true, button.on);
    let resp = resp.on_hover_text(tip);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, button.on, tip));
    let mut open = was_open;
    if resp.clicked() && button.on {
        open = !open;
    } else if open && ui.input(|i| i.pointer.any_click()) {
        let layer = ui.input(|i| i.pointer.interact_pos()).and_then(|p| ui.ctx().layer_id_at(p));
        if !layer.is_some_and(|l| matches!(l.order, egui::Order::Foreground | egui::Order::Tooltip)) {
            open = false;
        }
    }
    let child_open = egui::Popup::is_any_open(ui.ctx());
    let mut shown = open;
    if open {
        app.escape_free = false;
        crate::rtl::menu(&resp).open_bool(&mut shown).close_behavior(egui::PopupCloseBehavior::IgnoreClicks).show(|ui| {
            ui.set_min_width(340.0);
            ui.set_max_width(420.0);
            egui::ScrollArea::vertical().max_height(480.0).show(ui, |ui| add(ui, app));
        });
        if !shown && child_open && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            shown = true;
        }
    }
    ui.data_mut(|d| d.insert_temp(open_id, shown));
}

/// The room of a large button labelled `label`.
fn big_rect(ui: &mut Ui, label: &str) -> (Rect, egui::Response) {
    let t = Tokens::get(ui.ctx());
    let galley_w =
        label.split('\n').map(|l| ui.ctx().fonts_mut(|f| f.layout_no_wrap(l.to_string(), regular(11.5), t.text).size().x)).fold(0.0, f32::max);
    ui.allocate_exact_size(vec2((galley_w + 22.0).max(44.0), CONTENT_H), Sense::click())
}

/// A large button's icon and label (`marker`: the drop-down mark after the last line).
fn big_face(ui: &Ui, r: Rect, icon: &str, label: &str, marker: bool, on: bool) {
    let t = Tokens::get(ui.ctx());
    let (c, a, text) = if on { (t.icon, t.accent, t.text) } else { (t.text_disabled, t.text_disabled, t.text_disabled) };
    crate::icons::paint(ui.painter(), Rect::from_center_size(pos2(r.center().x, r.min.y + BIG_ICON_Y), vec2(32.0, 32.0)), icon, c, a);
    let lines: Vec<&str> = label.split('\n').collect();
    let mut y = r.min.y + BIG_LABEL_Y;
    for (i, l) in lines.iter().enumerate() {
        crate::widgets::label_line(ui, r.center().x, y, l, regular(11.5), text, marker && i + 1 == lines.len());
        y += LABEL_PITCH;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wordcraft_engine::Session;

    fn app_with(exercises: usize) -> WordApp {
        let mut app = WordApp::new(Session::new(wordcraft_doc::Document::from_text("Intro")), crate::Services::default());
        for _ in 0..exercises {
            app.run("exercise.insert", json!({"kind": "problem"})).unwrap();
        }
        app
    }

    fn caret_at_start(app: &mut WordApp) {
        app.run("caret.set", json!({"pos": wordcraft_doc::Pos::body(0, 0)})).unwrap();
    }

    /// The Exercise tab comes with the cursor in an exercise; the Series tab with a second one,
    /// wherever the cursor is.
    #[test]
    fn exercise_and_series_tabs_follow_the_cursor_and_the_document() {
        let mut app = app_with(0);
        assert!(contextual(&app.session).is_empty());
        let mut app1 = app_with(1);
        assert_eq!(contextual(&app1.session), vec![EXERCISE_TAB], "a lone exercise has no series");
        caret_at_start(&mut app1);
        assert!(contextual(&app1.session).is_empty(), "outside the exercise");
        app.run("exercise.insert", json!({"kind": "choice"})).unwrap();
        app.run("exercise.insert", json!({"kind": "open"})).unwrap();
        assert_eq!(contextual(&app.session), vec![EXERCISE_TAB, SERIES_TAB]);
        caret_at_start(&mut app);
        assert_eq!(contextual(&app.session), vec![SERIES_TAB], "the series' tab shows with the cursor anywhere");
        let tabs = super::super::contextual_tabs(&app.session);
        assert!(tabs.contains(&SERIES_TAB) && !tabs.contains(&EXERCISE_TAB));
    }

    /// Exercise Properties (the grip's menu, command search) opens the tab, not a pane.
    #[test]
    fn exercise_properties_brings_up_the_tab() {
        let mut app = app_with(2);
        app.run("view.exercisePane", json!({"value": true})).unwrap();
        follow(&mut app);
        assert_eq!(app.ui.tab, EXERCISE_TAB);
        assert!(!app.session.view.exercise_pane, "no pane stays asked for");
        caret_at_start(&mut app);
        app.ui.tab = "Home".into();
        app.run("view.exercisePane", json!({"value": true})).unwrap();
        follow(&mut app);
        assert_eq!(app.ui.tab, SERIES_TAB, "outside the exercises: the series");
    }

    /// Every type of exercise has the common commands; the peculiar ones are on only for the types
    /// of question that use them, and the type shows by name.
    #[test]
    fn peculiar_commands_follow_the_type_of_question() {
        let cases = [
            ("problem", "Problem", Peculiar { types: false, results: true, data: true, hide_words: false }),
            ("open", "Open Answer", Peculiar { types: true, results: false, data: false, hide_words: false }),
            ("choice", "Multiple Choice", Peculiar { types: true, results: false, data: false, hide_words: false }),
            ("trueFalse", "True or False", Peculiar { types: true, results: false, data: false, hide_words: false }),
            ("cloze", "Inline Completion", Peculiar { types: true, results: false, data: true, hide_words: true }),
            ("calc", "Calculation", Peculiar { types: true, results: true, data: true, hide_words: false }),
        ];
        for (kind, name, want) in cases {
            let mut app = app_with(0);
            app.run("exercise.insert", json!({"kind": kind})).unwrap();
            let st = app.session.run("exercise.questions", &json!({})).unwrap();
            assert_eq!(peculiar(&st, false), want, "{kind}: {st}");
            assert_eq!(type_name(&st), name, "{kind}: {st}");
        }
        // Data already there stays reachable; a quiz of mixed types is named after its kind.
        let mut app = app_with(0);
        app.run("exercise.insert", json!({"kind": "open"})).unwrap();
        app.run("exercise.addQuestion", json!({"type": "cloze"})).unwrap();
        let st = app.session.run("exercise.questions", &json!({})).unwrap();
        assert!(peculiar(&st, true).data && peculiar(&st, false).hide_words, "{st}");
        assert_eq!(type_name(&st), "Open Answer");
    }

    /// Both tabs draw every group within the ribbon's height, and their buttons work: Add Variant
    /// adds one, the Series tab's numbering applies.
    #[test]
    fn both_tabs_draw_and_their_buttons_work() {
        use egui_kittest::kittest::Queryable;
        let mut app = app_with(2);
        let datum = json!({"variants": ["A"], "active": 0, "items": [{"symbol": "L", "description": "", "unit": "m", "values": ["4"]}]});
        app.run("exercise.setData", json!({"data": datum})).unwrap();
        let direct = app.session.run("exercise.data", &json!({})).unwrap();
        assert_eq!(direct["data"]["variants"], json!(["A"]), "{direct}");
        app.run("ui.tab", json!({"tab": EXERCISE_TAB})).unwrap();
        let mut h = egui_kittest::Harness::builder().with_size(vec2(2560.0, 900.0)).build_ui_state(
            |ui, app: &mut WordApp| {
                let ctx = ui.ctx().clone();
                app.logic(&ctx);
                app.ui(ui);
            },
            app,
        );
        for tab in [EXERCISE_TAB, SERIES_TAB] {
            h.state_mut().run("ui.tab", json!({"tab": tab})).unwrap();
            h.run_steps(2);
            assert_eq!(h.state().ui.tab, tab);
            crate::widgets::TALL_GROUPS.with(|g| g.borrow_mut().clear());
            h.run_steps(1);
            let tall = crate::widgets::TALL_GROUPS.with(|g| g.take());
            assert!(tall.is_empty(), "{tab}: groups taller than the ribbon: {tall:?}");
            if tab == EXERCISE_TAB {
                h.get_by_label(tl!("Add a variant: a copy of the active one's values (add data first)")).click();
                h.run_steps(2);
                let data = h.state_mut().session.run("exercise.data", &json!({})).unwrap();
                assert_eq!(data["data"]["variants"].as_array().map(Vec::len), Some(2), "{data}");
            }
        }
        let before = h.state_mut().session.run("exercise.numbering", &json!({})).unwrap();
        assert_eq!(before["format"], json!("decimal"));
    }
}
