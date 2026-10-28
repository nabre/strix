//! Strix's own ribbon groups and tabs: the Math tab, graph paper, CAD drawings and calculated
//! tables. Kept apart from the upstream tab files so upstream merges don't touch them
//! (docs/upstream.md); the tab files only call in here.

use super::arrange::{align_menu, group_menu, rotate_menu, wrap_menu};
use super::common::{mi_icon, stack};
use super::language::mi_check;
use super::shape::alt_text_group;
use super::{Ui, Value, WordApp, big, big_toggle, group, json, menu_button, mi, small};

/// Insert › Pages › Graph Paper (docs/graph-paper.md).
pub(super) fn graph_paper_button(ui: &mut Ui, app: &mut WordApp) {
    menu_button(
        ui,
        app,
        crate::widgets::MenuButtonOptions::new(
            "graphPaper",
            Some(
                "Graph
Paper",
            ),
            "Graph Paper",
            true,
            "graph.insert",
        ),
        graph_paper_menu,
    );
}

/// Insert › Pages › Graph Paper (docs/graph-paper.md): the placements, then the pane.
fn graph_paper_menu(ui: &mut Ui, app: &mut WordApp) {
    mi_icon(ui, app, "graphPaper", "To the End of the Page", "graph.insert", json!({"placement": "pageEnd"}));
    mi(ui, app, "Page Background", "graph.insert", json!({"placement": "background"}));
    mi(ui, app, "Until the Next Odd Page", "graph.insert", json!({"placement": "untilOdd"}));
    mi(ui, app, "Until the Next Even Page", "graph.insert", json!({"placement": "untilEven"}));
    mi(ui, app, "For Two Pages", "graph.insert", json!({"placement": "pages", "pages": 2}));
    mi(ui, app, "For a Height of 5 cm", "graph.insert", json!({"placement": "height", "height": 50}));
    ui.separator();
    let pane = app.session.view.graph_pane;
    mi_check(ui, app, "Graph Paper Properties", pane, "view.graphPaperPane", json!({}));
    mi(ui, app, "Remove Graph Paper", "graph.remove", json!({}));
}

/// Insert › Symbols: Equation and Symbol. The Math tab shows the same group.
pub(super) fn symbols_group(app: &mut WordApp, ui: &mut Ui) {
    group(ui, "Symbols", None, app, |ui, app| {
        crate::equation_tab::insert_button(ui, app);
        menu_button(ui, app, crate::widgets::MenuButtonOptions::new("symbol", Some("Symbol"), "Symbol", true, "insert.symbol"), |ui, app| {
            // Recently used symbols first (their font on hover), then common ones.
            let mut items: Vec<(String, String)> = app.ui.recent_symbols.iter().map(|r| (r.ch.clone(), r.font.clone())).collect();
            for c in [
                "©", "®", "™", "§", "¶", "€", "£", "¥", "°", "±", "≠", "≤", "≥", "÷", "×", "∞", "µ", "α", "β", "π", "Ω", "∑", "√", "→", "←", "✓",
                "★", "♥", "—", "…",
            ] {
                if !items.iter().any(|(ch, f)| ch == c && f.is_empty()) {
                    items.push((c.to_string(), String::new()));
                }
            }
            items.truncate(30);
            egui::Grid::new("syms").show(ui, |ui| {
                for (i, (c, font)) in items.iter().enumerate() {
                    let b = ui.button(egui::RichText::new(c).size(16.0));
                    let b = if font.is_empty() { b } else { b.on_hover_text(font) };
                    if b.clicked() {
                        let params = if font.is_empty() { json!({"char": c}) } else { json!({"char": c, "font": font}) };
                        if app.run("insert.symbol", params).is_ok() {
                            crate::dialogs_insert::remember_symbol(app, c, font);
                        }
                        ui.close();
                    }
                    if i % 6 == 5 {
                        ui.end_row();
                    }
                }
            });
            ui.separator();
            mi(ui, app, "More Symbols…", "ui.dialog", json!({"name": "symbol"}));
        });
    });
}

/// Math: every calculation tool (docs/calc.md) in one place.
pub(super) fn math(app: &mut WordApp, ui: &mut Ui) {
    group(ui, "Calculations", None, app, |ui, app| {
        big(
            ui,
            app,
            "calculation",
            "New
Calculation",
            "insert.equation",
            json!({"calc": true}),
            false,
        );
        stack(ui, |ui| {
            let info = app.session.run("equation.get", &json!({})).ok();
            let calc = info.as_ref().and_then(|v| v.get("calc")).and_then(Value::as_bool).unwrap_or(false);
            small(
                ui,
                app,
                "formula",
                Some("Calculate"),
                "Make the equation at the cursor a live calculation, or an ordinary one",
                crate::widgets::ButtonAction::new("equation.calc", json!({}), calc),
            );
            let by_default = app.session.prefs.calc_new_equations;
            small(
                ui,
                app,
                "check",
                Some("Calculate New Equations"),
                "New equations are live calculations",
                crate::widgets::ButtonAction::new("calc.newEquations", json!({}), by_default),
            );
            small(
                ui,
                app,
                "equation",
                Some("Recalculate"),
                "Recalculate every calculation in the document (F9)",
                crate::widgets::ButtonAction::new("calc.recalculate", json!({}), false),
            );
        });
    });
    symbols_group(app, ui);
    group(ui, "Exercise", None, app, |ui, app| {
        big(ui, app, "exercise", "New\nExercise", "exercise.insert", json!({}), false);
        let info = app.session.run("exercise.info", &json!({})).ok().unwrap_or_default();
        stack(ui, |ui| {
            small(
                ui,
                app,
                "addQuestion",
                Some("Add Question"),
                "Add a question at the end of the exercise at the cursor",
                crate::widgets::ButtonAction::new("exercise.addQuestion", json!({}), false),
            );
            small(
                ui,
                app,
                "addSubQuestion",
                Some("Add Sub-question"),
                "Add a question nested under the last one (a.1, a.1.1); Tab and Shift+Tab change a question's level",
                crate::widgets::ButtonAction::new("exercise.addQuestion", json!({"sub": true}), false),
            );
            small(
                ui,
                app,
                "addNote",
                Some("Add Note"),
                "Add a note between the questions: free text that is not a question",
                crate::widgets::ButtonAction::new("exercise.addNote", json!({}), false),
            );
        });
        stack(ui, |ui| {
            let strings = |key: &str| -> Vec<String> {
                info[key].as_array().map(|a| a.iter().filter_map(|q| q.as_str().map(str::to_string)).collect()).unwrap_or_default()
            };
            let (questions, labels) = (strings("questions"), strings("labels"));
            let levels: Vec<u64> = info["levels"].as_array().map(|a| a.iter().filter_map(serde_json::Value::as_u64).collect()).unwrap_or_default();
            let current = info["answer"].as_u64().unwrap_or(0);
            menu_button(
                ui,
                app,
                crate::widgets::MenuButtonOptions::new(
                    "resultOf",
                    Some("Result Of"),
                    "Mark the equation at the cursor as the result of a question: its value goes into the results summary",
                    false,
                    "equation.answer",
                ),
                |ui, app| {
                    for (k, q) in questions.iter().enumerate() {
                        let n = k as u64 + 1;
                        let indent = "    ".repeat(levels.get(k).copied().unwrap_or(0) as usize);
                        let label = format!("{indent}{} {}", labels.get(k).map_or("", String::as_str), q.chars().take(48).collect::<String>());
                        let resp = ui.add_enabled(crate::widgets::enabled(app, "equation.answer"), egui::Button::new(label).selected(current == n));
                        if resp.clicked() {
                            let _ = app.run("equation.answer", json!({"question": n}));
                            ui.close();
                        }
                    }
                    ui.separator();
                    mi_check(ui, app, "Not a Result", current == 0, "equation.answer", json!({"question": 0}));
                },
            );
            small(
                ui,
                app,
                "equation",
                Some("Update Summary"),
                "Rebuild the repeated questions and the results summary",
                crate::widgets::ButtonAction::new("exercise.update", json!({}), false),
            );
            let pane = app.session.view.exercise_pane;
            small(
                ui,
                app,
                "exerciseProperties",
                Some("Properties"),
                "Show the exercise's data and variants",
                crate::widgets::ButtonAction::new("view.exercisePane", json!({}), pane),
            );
        });
    });
    group(ui, "Tables", None, app, |ui, app| {
        big(
            ui,
            app,
            "calcTable",
            "Calculated
Table",
            "calc.insertTable",
            json!({}),
            false,
        );
        stack(ui, |ui| {
            let on = app.session.run("calc.tableInfo", &json!({})).ok().and_then(|v| v.get("calc").and_then(Value::as_bool)).unwrap_or(false);
            small(
                ui,
                app,
                "calcTable",
                Some("Calculate This Table"),
                "The table at the cursor is calculated: name its columns in the first row (L [m], M [kN·m] = q·L²/8) and total them with Σ =",
                crate::widgets::ButtonAction::new("table.calc", json!({}), on),
            );
            calc_table_tools(ui, app);
        });
        stack(ui, |ui| calc_table_rows(ui, app));
    });
    group(ui, "Results", None, app, |ui, app| {
        let digits = u64::from(app.session.prefs.calc_digits);
        stack(ui, |ui| {
            small(
                ui,
                app,
                "moreDigits",
                Some("More Digits"),
                "Show one more digit in results",
                crate::widgets::ButtonAction::new("calc.options", json!({"digits": (digits + 1).min(15)}), false),
            );
            small(
                ui,
                app,
                "fewerDigits",
                Some("Fewer Digits"),
                "Show one digit less in results",
                crate::widgets::ButtonAction::new("calc.options", json!({"digits": digits.saturating_sub(1)}), false),
            );
        });
        let steps = app.session.prefs.calc_steps;
        crate::widgets::big_toggle(ui, app, "steps", "Show\nSteps", "calc.steps", steps);
        menu_button(
            ui,
            app,
            crate::widgets::MenuButtonOptions::new("numberFormat", Some("Number Format"), "How results are rounded", true, "calc.options"),
            |ui, app| {
                let count = app.session.prefs.calc_count.clone();
                mi_check(ui, app, "Automatic (Decimals, Significant Below 1)", count == "auto", "calc.options", json!({"count": "auto"}));
                mi_check(ui, app, "Decimal Places", count == "decimals", "calc.options", json!({"count": "decimals"}));
                mi_check(ui, app, "Significant Digits", count == "significant", "calc.options", json!({"count": "significant"}));
                ui.separator();
                let zeros = app.session.prefs.calc_zeros;
                mi_check(ui, app, "Show Trailing Zeros", zeros, "calc.options", json!({"zeros": !zeros}));
            },
        );
    });
    group(ui, "Drawing", None, app, |ui, app| {
        big(
            ui,
            app,
            "cad",
            "CAD
Drawing",
            "insert.cad",
            json!({}),
            false,
        );
        let editable = strix_engine::cmd::cad::edited(&app.session).is_some();
        ui.add_enabled_ui(editable, |ui| {
            big(
                ui,
                app,
                "cadEdit",
                "Edit
Drawing",
                "ui.cadEdit",
                json!({}),
                false,
            );
        });
    });
}

/// Draws the Picture Format tab's Adjust, Picture Styles, Accessibility (Alt Text), Arrange and
/// Size groups.
/// Arrange for a selected object: Position, Wrap Text, Rotate, Align (and Group).
pub(super) fn arrange_group(ui: &mut Ui, app: &mut WordApp, grouping: bool) {
    group(ui, "Arrange", None, app, |ui, app| {
        big(ui, app, "position", "Position", "arrange.position", json!({}), false);
        wrap_menu(ui, app);
        stack(ui, |ui| {
            rotate_menu(ui, app, Some("Rotate"));
            align_menu(ui, app, Some("Align"));
            if grouping {
                group_menu(ui, app, Some("Group"));
            }
        });
    });
}

/// The selected CAD drawing's picture: edit it, its scale (fixed, or the largest standard one
/// that fits a space, by default the page's), its padding, and what any picture has (alt text,
/// arrange).
pub(super) fn cad_picture_format(app: &mut WordApp, ui: &mut Ui) {
    let Some((_, d)) = strix_engine::cmd::cad::selected_drawing(&app.session) else { return };
    group(ui, "Drawing", None, app, |ui, app| {
        big(ui, app, "cadSelect", "Edit\nDrawing", "ui.cadEdit", json!({}), false);
    });
    group(ui, "Scale", None, app, |ui, app| {
        let fitting = d.fit.is_some_and(|f| !f.fixed);
        crate::widgets::big_checked(
            ui,
            app,
            "cadFit",
            "Fit to
Page",
            "cad.scale",
            json!({"fit": !fitting}),
            fitting,
        );
        // What fits is judged on the drawing as drawn, its sizes worked out by the calculations.
        let drawn = strix_engine::cmd::cad::edited(&app.session).map_or_else(|| d.clone(), |(_, r, ..)| r);
        crate::cad_editor::scale_fields(ui, app, "cad_picture", &d, &drawn, "cad.scale");
    });
    group(ui, "Picture", None, app, |ui, app| {
        crate::cad_editor::field_grid(ui, "cad_picture_grid", |ui| {
            ui.label(tl!("Padding"));
            let now = d.padding.top;
            let mut v = now;
            let r = ui.add_sized(
                [crate::cad_editor::FIELD_W, 18.0],
                egui::DragValue::new(&mut v).range(0.0..=strix_cad::MAX_PADDING).speed(0.1).max_decimals(2).suffix(" mm"),
            );
            if (r.drag_stopped() || r.lost_focus()) && v != now {
                let _ = app.run("cad.padding", json!({"value": v}));
            }
            ui.end_row();
            ui.label(tl!("Unit"));
            ui.label(d.unit.symbol());
            ui.end_row();
        });
    });
    alt_text_group(ui, app);
    arrange_group(ui, app, false);
}

/// A calculated table's ordinal column (column "0", the data rows numbered 1, 2, 3…) and its
/// total row (the last row sums each column: Table Design › Total Row).
fn calc_table_rows(ui: &mut Ui, app: &mut WordApp) {
    let info = app.session.run("calc.tableInfo", &json!({})).ok();
    let flag = |k: &str| info.as_ref().and_then(|v| v.get(k).and_then(Value::as_bool)).unwrap_or(false);
    let (index, total) = (flag("index"), flag("total"));
    small(
        ui,
        app,
        "numbering",
        Some("Ordinal Column"),
        "A first column, column 0, that numbers the data rows 1, 2, 3… (the calculations never read it)",
        crate::widgets::ButtonAction::new("table.calcIndex", json!({}), index),
    );
    small(
        ui,
        app,
        "calcTotal",
        Some("Total Row"),
        "The last row sums each column (Σ =); a cell written there keeps its own text (mean =, max =)",
        crate::widgets::ButtonAction::new("table.look", json!({"totalRow": !total}), total),
    );
}

/// A calculated table's options: Cells as Equations, and Data Table (its values as parameters
/// named by column and row, `F_A` or `A_F`).
fn calc_table_tools(ui: &mut Ui, app: &mut WordApp) {
    let info = app.session.run("calc.tableInfo", &json!({})).ok();
    let names = info.as_ref().and_then(|v| v.get("names").and_then(Value::as_str).map(str::to_string)).unwrap_or_default();
    let math = info.as_ref().and_then(|v| v.get("math").and_then(Value::as_bool)).unwrap_or(false);
    small(
        ui,
        app,
        "equation",
        Some("Cells as Equations"),
        "Show every cell as an equation: names in italics, subscripts, units in the Unit style",
        crate::widgets::ButtonAction::new("table.mathCells", json!({}), math),
    );
    menu_button(
        ui,
        app,
        crate::widgets::MenuButtonOptions::new(
            "calcTable",
            Some("Data Table"),
            "Every value is a parameter named by its column and its row's label (the first column), with a subscript: F_A or A_F",
            false,
            "table.calcNames",
        ),
        |ui, app| {
            for (value, label) in [("none", "Not a Data Table"), ("columnRow", "Column, Row Below (F_A)"), ("rowColumn", "Row, Column Below (A_F)")] {
                if ui.selectable_label(names == value, tl!(label)).clicked() {
                    let _ = app.run("table.calcNames", json!({"value": value}));
                    ui.close();
                }
            }
        },
    );
}

/// Table Layout › Data: Calculated Table and its options (docs/calc.md).
pub(super) fn calc_table_buttons(ui: &mut Ui, app: &mut WordApp) {
    let on = app.session.run("calc.tableInfo", &json!({})).ok().and_then(|v| v.get("calc").and_then(Value::as_bool)).unwrap_or(false);
    big_toggle(
        ui,
        app,
        "calcTable",
        "Calculated
Table",
        "table.calc",
        on,
    );
    stack(ui, |ui| calc_table_tools(ui, app));
    stack(ui, |ui| calc_table_rows(ui, app));
}
