//! Strix's own ribbon groups and tabs: the Math and Teaching tabs, the Header & Footer tab, graph paper, CAD drawings and calculated
//! tables. Kept apart from the upstream tab files so upstream merges don't touch them
//! (docs/upstream.md); the tab files only call in here.

use super::arrange::{align_menu, group_menu, rotate_menu, wrap_menu};
use super::common::{mi_icon, stack};
use super::language::mi_check;
use super::shape::alt_text_group;
use super::{Ui, Value, WordApp, big, big_toggle, group, json, menu_button, mi, small};

/// The Teaching tab (Didattica): the objects for teaching, exercises and their series, the
/// scores table, grading scale, assessment check & pack export, and graph paper.
pub(super) fn teaching(app: &mut WordApp, ui: &mut Ui) {
    exercise_group(app, ui);
    group(ui, "Archive", None, app, |ui, app| {
        big(ui, app, "folder", "Exercise\nBank", "view.archivePane", json!({}), false);
    });
    group(ui, "Scores", None, app, |ui, app| {
        let st = app.session.run("exercise.scores", &json!({})).ok().unwrap_or_default();
        let on = st["on"].as_bool().unwrap_or(false);
        crate::widgets::big_checked(ui, app, "calcTotal", "Scores\nTable", "exercise.scores", json!({"on": !on}), on);
        stack(ui, |ui| {
            ui.add_enabled_ui(on, |ui| {
                small(
                    ui,
                    app,
                    "plus",
                    Some("Add Criterion"),
                    "Add a criterion to the scores table: two columns, one left empty and one for the points (type its name in the header)",
                    crate::widgets::ButtonAction::new("exercise.scores", json!({"add": true}), false),
                );
                let criteria: Vec<String> =
                    st["criteria"].as_array().map(|a| a.iter().filter_map(|c| c.as_str().map(str::to_string)).collect()).unwrap_or_default();
                menu_button(
                    ui,
                    app,
                    crate::widgets::MenuButtonOptions::new(
                        "minus",
                        Some("Remove Criterion"),
                        "Remove a criterion and its points from the scores table",
                        false,
                        "exercise.scores",
                    ),
                    |ui, app| {
                        for (k, c) in criteria.iter().enumerate() {
                            let label = if c.is_empty() { format!("{}", k + 1) } else { c.clone() };
                            mi(ui, app, &label, "exercise.scores", json!({"remove": k + 1}));
                        }
                    },
                );
            });
            grading_scale_menu(ui, app);
        });
    });
    group(ui, "Assessment", None, app, |ui, app| {
        let check_on = app.session.view.check_pane;
        big_toggle(ui, app, "checkAssessment", "Check", "view.checkPane", check_on);
        big(ui, app, "exportPack", "Export\nPack", "file.exportPack", json!({}), false);
    });
    group(ui, "Graph Paper", None, app, |ui, app| {
        graph_paper_button(ui, app);
    });
}

fn grading_scale_menu(ui: &mut Ui, app: &mut WordApp) {
    let gst = app.session.run("exercise.grading", &json!({})).ok().unwrap_or_default();
    let g_on = gst["on"].as_bool().unwrap_or(false);
    let cfg = &gst["config"];
    let preset = cfg["preset"].as_str().unwrap_or("it-10").to_string();
    let curve = cfg["curve"].as_str().unwrap_or("linear").to_string();
    let step = cfg["step"].as_f64().unwrap_or(0.5);
    let vis = cfg["visibility"].as_str().unwrap_or("teacher").to_string();
    menu_button(
        ui,
        app,
        crate::widgets::MenuButtonOptions::new(
            "gradingScale",
            Some("Grading Scale"),
            "Insert or configure the points-to-grade conversion table (1–10, 1–6, 6–1, 0–20, %)",
            false,
            "exercise.grading",
        ),
        |ui, app| {
            mi_check(ui, app, "Show Grading Scale", g_on, "exercise.grading", json!({"on": !g_on}));
            ui.separator();
            for (id, lbl) in [
                ("it-10", "Italy (1–10)"),
                ("ch-6", "Switzerland (1–6)"),
                ("de-6", "Germany (6–1)"),
                ("fr-20", "France (0–20)"),
                ("percent", "Percentage (0–100%)"),
            ] {
                mi_check(ui, app, lbl, g_on && preset == id, "exercise.grading", json!({"on": true, "preset": id}));
            }
            ui.separator();
            mi_check(ui, app, "Linear Curve", curve == "linear", "exercise.grading", json!({"on": true, "curve": "linear"}));
            mi_check(
                ui,
                app,
                "Piecewise Curve (Pass Threshold)",
                curve == "piecewiseLinear",
                "exercise.grading",
                json!({"on": true, "curve": "piecewiseLinear"}),
            );
            ui.separator();
            for (st_val, st_lbl) in [(0.1, "Step 0.1"), (0.25, "Step 0.25"), (0.5, "Step 0.5"), (1.0, "Step 1.0")] {
                mi_check(ui, app, st_lbl, (step - st_val).abs() < 1e-6, "exercise.grading", json!({"on": true, "step": st_val}));
            }
            ui.separator();
            mi_check(ui, app, "Teacher Copy Only", vis == "teacher", "exercise.grading", json!({"on": true, "visibility": "teacher"}));
            mi_check(ui, app, "Student and Teacher Copies", vis == "both", "exercise.grading", json!({"on": true, "visibility": "both"}));
        },
    );
}

/// The Header & Footer tab, while a header or footer is edited (docs/headers-footers.md): go to
/// the header or footer and between sections, Link to Previous for the header and the footer
/// apart, Different First Page (the section's) and Different Odd & Even Pages, and Close.
pub(super) fn header_footer(app: &mut WordApp, ui: &mut Ui) {
    let st = app.session.run("hf.state", &json!({})).ok().unwrap_or_default();
    let flag = |k: &str| st[k].as_bool().unwrap_or(false);
    let editing = st["editing"].as_str().unwrap_or("").to_string();
    let page = app.session.page_hint;
    group(ui, "Navigation", None, app, |ui, app| {
        stack(ui, |ui| {
            small(
                ui,
                app,
                "header",
                Some("Go to Header"),
                "Edit this page's header",
                crate::widgets::ButtonAction::new("insert.editHeader", json!({"page": page}), editing == "header"),
            );
            small(
                ui,
                app,
                "footer",
                Some("Go to Footer"),
                "Edit this page's footer",
                crate::widgets::ButtonAction::new("insert.editFooter", json!({"page": page}), editing == "footer"),
            );
        });
        stack(ui, |ui| {
            small(
                ui,
                app,
                "chevronUp",
                Some("Previous"),
                "The previous section's header or footer",
                crate::widgets::ButtonAction::new("hf.previous", json!({}), false),
            );
            small(
                ui,
                app,
                "chevronDown",
                Some("Next"),
                "The next section's header or footer",
                crate::widgets::ButtonAction::new("hf.next", json!({}), false),
            );
        });
        // The first section has none before it to link to.
        let later = st["section"].as_u64().unwrap_or(1) > 1;
        stack(ui, |ui| {
            if !later {
                ui.disable();
            }
            let (h, f) = (flag("headerLinked"), flag("footerLinked"));
            small(
                ui,
                app,
                "link",
                Some("Link Header to Previous"),
                "On: this section shows the previous section's header. Off: it has its own (a copy to change)",
                crate::widgets::ButtonAction::new("layout.linkToPrevious", json!({"footer": false, "value": !h}), h),
            );
            small(
                ui,
                app,
                "link",
                Some("Link Footer to Previous"),
                "On: this section shows the previous section's footer. Off: it has its own (a copy to change)",
                crate::widgets::ButtonAction::new("layout.linkToPrevious", json!({"footer": true, "value": !f}), f),
            );
        });
    });
    group(ui, "Options", None, app, |ui, app| {
        stack(ui, |ui| {
            let (first, odd_even) = (flag("firstPage"), flag("oddEven"));
            small(
                ui,
                app,
                "check",
                Some("Different First Page"),
                "This section's first page has a header and footer of its own",
                crate::widgets::ButtonAction::new("layout.differentFirstPage", json!({"value": !first}), first),
            );
            small(
                ui,
                app,
                "check",
                Some("Different Odd & Even Pages"),
                "Even pages have a header and footer of their own (the whole document)",
                crate::widgets::ButtonAction::new("layout.differentOddEven", json!({"value": !odd_even}), odd_even),
            );
            let section = format!("{} {} / {}", tl!("Section"), st["section"].as_u64().unwrap_or(1), st["sections"].as_u64().unwrap_or(1));
            ui.label(egui::RichText::new(section).small());
        });
    });
    group(ui, "Close", None, app, |ui, app| {
        big(ui, app, "close", "Close Header\nand Footer", "insert.closeHeader", json!({}), false);
    });
}

/// Editing a header or footer brings up the Header & Footer tab; leaving it goes back to the tab
/// shown before (kept in the context's memory).
pub(super) fn follow_header_footer(app: &mut WordApp, ctx: &egui::Context) {
    let id = egui::Id::unique("strix_hf_prev_tab");
    let prev: Option<String> = ctx.data(|d| d.get_temp(id));
    if super::tabs::is_editing_header_footer(&app.session) {
        if prev.is_none() {
            ctx.data_mut(|d| d.insert_temp(id, app.ui.tab.clone()));
            app.ui.tab = "Header & Footer".into();
        }
    } else if let Some(prev) = prev {
        ctx.data_mut(|d| d.remove_temp::<String>(id));
        if app.ui.tab == "Header & Footer" {
            app.ui.tab = prev;
        }
    }
}

/// Teaching › Exercise (docs/exercise.md).
fn exercise_group(app: &mut WordApp, ui: &mut Ui) {
    group(ui, "Exercise", None, app, |ui, app| {
        new_exercise(ui, app);
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
            show_menu(ui, app);
            let front = app.session.run("exercise.frontispiece", &json!({})).ok().is_some_and(|v| v["on"].as_bool().unwrap_or(false));
            small(
                ui,
                app,
                "coverPage",
                Some("Frontispiece"),
                "Pages of your own before the exercises (title, class, date, instructions), ended by a Frontispiece Break",
                crate::widgets::ButtonAction::new("exercise.frontispiece", json!({"on": !front}), front),
            );
        });
        stack(ui, |ui| {
            let pane = app.session.view.exercise_pane;
            small(
                ui,
                app,
                "exerciseProperties",
                Some("Properties"),
                "Data and variants, page breaks, numbering, parts shown and graph paper, in the Exercise Properties pane",
                crate::widgets::ButtonAction::new("view.exercisePane", json!({}), pane),
            );
            question_type_menu(ui, app);
            inline_expr_menu(ui, app);
            // Only with the cursor in an exercise, or whole ones selected.
            let targets = !wordcraft_engine::cmd::exercise::delete_targets(&app.session).is_empty();
            ui.add_enabled_ui(targets, |ui| {
                // Drawn greyed out like a disabled command: the command itself stays on for
                // scripts naming an exercise.
                if !targets {
                    ui.multiply_opacity(0.4);
                }
                small(
                    ui,
                    app,
                    "deleteTable",
                    Some("Delete Exercise"),
                    "Delete the exercise at the cursor, or the exercises selected, whole: its statement, questions and solution go, its results leave the summary (Undo brings it back)",
                    crate::widgets::ButtonAction::new("exercise.delete", json!({}), false),
                );
            });
        });
    });
}

fn inline_expr_menu(ui: &mut Ui, app: &mut WordApp) {
    menu_button(
        ui,
        app,
        crate::widgets::MenuButtonOptions::new(
            "inlineExpr",
            Some("Inline Expression"),
            "Insert an inline parametric expression evaluated in the exercise's sandbox (e.g. v * t -> m)",
            false,
            "exercise.insertExpr",
        ),
        |ui, app| {
            let id = egui::Id::unique("inline_expr_draft");
            let mut text = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| "v * t -> m".into());
            ui.set_min_width(200.0);
            ui.label(egui::RichText::new(tl!("Parametric expression")).small().weak());
            let edit = ui.text_edit_singleline(&mut text);
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.data_mut(|d| d.insert_temp(id, text.clone()));
            if (enter || ui.button(tl!("Insert")).clicked()) && !text.trim().is_empty() {
                let _ = app.run("exercise.insertExpr", json!({"expr": text.trim()}));
                ui.close();
            }
        },
    );
}

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
    mi(ui, app, "To the End of the Column", "graph.insert", json!({"placement": "columnEnd"}));
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
            crate::rtl::grid(ui, egui::Grid::new("syms"), |ui| {
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
    group(ui, "Calculation", None, app, |ui, app| {
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
                Some("Always Calculate"),
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
        big_toggle(ui, app, "variables", "Variables", "view.variablesPane", app.session.view.variables_pane);
    });
    symbols_group(app, ui);
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
                Some("Calculate Table"),
                "The table at the cursor is calculated: name its columns in the first row (L [m], M [kN·m] = q·L²/8) and total them with Σ =",
                crate::widgets::ButtonAction::new("table.calc", json!({}), on),
            );
            calc_table_tools(ui, app);
        });
        stack(ui, |ui| calc_table_rows(ui, app));
        menu_button(
            ui,
            app,
            crate::widgets::MenuButtonOptions::new(
                "calcTable",
                Some("Student Blanks"),
                "Choose which calculated cells students complete",
                false,
                "exercise.tableFill",
            ),
            |ui, app| {
                mi(ui, app, "Student Blank Cell", "exercise.tableFill", json!({"studentBlank":true}));
                mi(ui, app, "Student Blank Column", "exercise.tableFill", json!({"studentBlank":true,"column":true}));
                mi(ui, app, "Show Cell Value", "exercise.tableFill", json!({"studentBlank":false}));
            },
        );
    });
    group(ui, "Number Display", None, app, |ui, app| {
        let eff = wordcraft_engine::cmd::calc::resolved_options(&app.session);
        let digits = u64::from(eff.digits);
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
            small(
                ui,
                app,
                "steps",
                Some("Show Steps"),
                "Show the substitution steps of every calculation: the formula with its values, then the result",
                crate::widgets::ButtonAction::new("calc.steps", json!({}), eff.steps),
            );
        });
        stack(ui, |ui| {
            menu_button(
                ui,
                app,
                crate::widgets::MenuButtonOptions::new("numberFormat", Some("Rounding"), "How results are rounded", false, "calc.options"),
                |ui, app| {
                    let eff = wordcraft_engine::cmd::calc::resolved_options(&app.session);
                    let count = wordcraft_engine::cmd::calc::count_name(eff.count);
                    mi_check(ui, app, "Automatic (Decimals, Significant Below 1)", count == "auto", "calc.options", json!({"count": "auto"}));
                    mi_check(ui, app, "Decimal Places", count == "decimals", "calc.options", json!({"count": "decimals"}));
                    mi_check(ui, app, "Significant Digits", count == "significant", "calc.options", json!({"count": "significant"}));
                    ui.separator();
                    let zeros = eff.zeros;
                    mi_check(ui, app, "Show Trailing Zeros", zeros, "calc.options", json!({"zeros": !zeros}));
                    ui.separator();
                    mi_check(ui, app, "Maximum Uncertainty", eff.uncertainty.name() == "max", "calc.options", json!({"uncertainty": "max"}));
                    mi_check(
                        ui,
                        app,
                        "Uncertainty in Quadrature (GUM)",
                        eff.uncertainty.name() == "rss",
                        "calc.options",
                        json!({"uncertainty": "rss"}),
                    );
                },
            );
            menu_button(
                ui,
                app,
                crate::widgets::MenuButtonOptions::new(
                    "lineSpacing",
                    Some("Result Layout"),
                    "Where results and steps go: after ⇒ on the statement's line, or one line each under it, aligned at =",
                    false,
                    "calc.layout",
                ),
                |ui, app| {
                    let eff = wordcraft_engine::cmd::calc::resolved_options(&app.session);
                    let lines = eff.layout.name() == "lines";
                    mi_check(ui, app, "Inline (⇒)", !lines, "calc.layout", json!({"value": "inline"}));
                    mi_check(ui, app, "Line by Line", lines, "calc.layout", json!({"value": "lines"}));
                },
            );
        });
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
        let editable = wordcraft_engine::cmd::cad::editable(&app.session);
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
        big(ui, app, "plot", "Plot", "plot.insert", json!({"openPane": true}), false);
        let plot_pane = app.session.view.plot_pane;
        big_toggle(ui, app, "plotProperties", "Plot\nProperties", "view.plotPane", plot_pane);
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
    let Some((_, d)) = wordcraft_engine::cmd::cad::selected_drawing(&app.session) else { return };
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
        let drawn = wordcraft_engine::cmd::cad::edited_drawing(&app.session).unwrap_or_else(|| d.clone());
        let space = wordcraft_engine::cmd::cad::space_here(&mut app.session);
        crate::cad_editor::scale_fields(ui, app, "cad_picture", (&d, &drawn), space, "cad.scale");
    });
    group(ui, "Picture", None, app, |ui, app| {
        crate::cad_editor::field_grid(ui, "cad_picture_grid", |ui| {
            ui.label(tl!("Padding"));
            if let Some(v) = crate::widgets::value_field(ui, "cad_padding", egui::vec2(crate::cad_editor::FIELD_W, 18.0), d.padding.top, |d| {
                d.range(0.0..=strix_cad::MAX_PADDING).speed(0.1).max_decimals(2).suffix(" mm")
            }) {
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

/// Teaching › Exercise › New Exercise: the top part inserts a problem, the menu one of the kinds
/// (docs/exercise.md, Kinds of exercise).
fn new_exercise(ui: &mut Ui, app: &mut WordApp) {
    crate::widgets::big_split(
        ui,
        app,
        crate::widgets::BigSplit {
            icon: "exercise",
            label: "New Exercise",
            tip: "Insert an exercise; the menu offers the kinds: problem, open answer, multiple choice, true or false, calculation, CAD drawing",
            action: crate::widgets::ButtonAction::new("exercise.insert", json!({}), false),
        },
        |ui, app| {
            mi(ui, app, "Problem (Statement, Questions, Solution)", "exercise.insert", json!({"kind": "problem"}));
            mi(ui, app, "Open Answer", "exercise.insert", json!({"kind": "open"}));
            mi(ui, app, "Multiple Choice", "exercise.insert", json!({"kind": "choice"}));
            mi(ui, app, "True or False", "exercise.insert", json!({"kind": "trueFalse"}));
            mi(ui, app, "Calculation", "exercise.insert", json!({"kind": "calc"}));
            mi(ui, app, "Worked Example", "exercise.insert", json!({"kind": "worked"}));
            mi(ui, app, "Inline Completion", "exercise.insert", json!({"kind": "cloze"}));
            mi(ui, app, "Table to Complete", "exercise.insert", json!({"kind": "tableFill"}));
            mi(ui, app, "Matching", "exercise.insert", json!({"kind": "matching"}));
            mi(ui, app, "CAD Drawing Question", "exercise.insert", json!({"kind": "drawing"}));
        },
    );
}

/// Teaching › Exercise › Question Type: adds a question of a mode to the quiz at the cursor
/// (docs/question-modes.md); one quiz mixes them.
fn question_type_menu(ui: &mut Ui, app: &mut WordApp) {
    let tip = "Add a question of a given type to the exercise at the cursor: open answer, multiple choice, true or false, calculation, CAD drawing";
    menu_button(
        ui,
        app,
        crate::widgets::MenuButtonOptions::new("addQuestion", Some("Question Type"), tip, false, "exercise.addQuestion"),
        |ui, app| {
            mi(ui, app, "Open Answer", "exercise.addQuestion", json!({"type": "open"}));
            mi(ui, app, "Multiple Choice", "exercise.addQuestion", json!({"type": "choice"}));
            mi(ui, app, "True or False", "exercise.addQuestion", json!({"type": "trueFalse"}));
            mi(ui, app, "Calculation", "exercise.addQuestion", json!({"type": "calc"}));
            mi(ui, app, "CAD Drawing Question", "exercise.addQuestion", json!({"type": "drawing"}));
            mi(ui, app, "Worked Example", "exercise.addQuestion", json!({"type": "worked"}));
            mi(ui, app, "Inline Completion", "exercise.addQuestion", json!({"type": "cloze"}));
            mi(ui, app, "Table to Complete", "exercise.addQuestion", json!({"type": "tableFill"}));
            mi(ui, app, "Matching", "exercise.addQuestion", json!({"type": "matching"}));
        },
    );
}

/// Teaching › Exercise › Show: the parts the exercise shows (of the one there is, or of every one in a
/// series, which rules) and its graph paper (docs/exercise.md, Parts shown).
fn show_menu(ui: &mut Ui, app: &mut WordApp) {
    let st = app.session.run("exercise.show", &json!({})).ok().unwrap_or_default();
    let flag = |k: &str| st[k].as_bool().unwrap_or(true);
    let (solution, results, grid) = (flag("solution"), flag("results"), !st["grid"].is_null());
    let tip = "Choose what the exercise shows (every exercise in a series): hidden parts stay in the document as hidden text";
    menu_button(ui, app, crate::widgets::MenuButtonOptions::new("showNotes", Some("Show"), tip, false, "exercise.show"), |ui, app| {
        mi_check(ui, app, "Solution", solution, "exercise.show", json!({"solution": !solution}));
        mi_check(ui, app, "Results in the Summary", results, "exercise.show", json!({"results": !results}));
        mi_check(ui, app, "Graph Paper Before the Solution", grid, "exercise.grid", json!({"on": !grid}));
    });
}

/// Design › Document Formatting › DSA (docs/dsa.md): the three versions of `design.dsa` as check
/// boxes, which combine, and None to take them all off.
pub(super) fn dsa_menu(ui: &mut Ui, app: &mut WordApp) {
    stack(ui, |ui| {
        menu_button(
            ui,
            app,
            crate::widgets::MenuButtonOptions::new(
                "accessibility",
                Some("DSA"),
                "DSA formatting: the test laid out for students with specific learning difficulties; the versions combine",
                false,
                "design.dsa",
            ),
            |ui, app| {
                let st = app.session.run("design.dsa", &json!({})).ok().unwrap_or_default();
                for (label, key) in
                    [("Reading (dyslexia)", "reading"), ("Writing (dysgraphia)", "writing"), ("Maths and Memory (dyscalculia)", "math")]
                {
                    let on = st[key].as_bool().unwrap_or(false);
                    mi_check(ui, app, label, on, "design.dsa", json!({ key: !on }));
                }
                ui.separator();
                mi(ui, app, "No DSA Formatting", "design.dsa", json!({"none": true}));
            },
        );
    });
}

/// Design › Fonts: the typefaces Strix offers without bundling them (Atkinson Hyperlegible and
/// Lexend, for DSA formatting: docs/dsa.md), heading and body alike. One not installed yet is
/// downloaded from the font store on the click, which is the person's consent to downloading;
/// the document takes it at once and shows it as soon as it arrives. Choosing one turns on
/// Embed Fonts in the File (checked below the fonts), so Word shows it on another computer.
pub(super) fn offered_fonts(ui: &mut Ui, app: &mut WordApp) {
    use wordcraft_engine::cmd::font_store::{self, Job, OFFERED};
    let ctx = ui.ctx().clone();
    font_store::set_notify(std::sync::Arc::new(move || ctx.request_repaint()));
    ui.separator();
    for (family, why) in OFFERED {
        let installed = font_store::installed(family);
        let label = match (installed, font_store::job(family)) {
            (true, _) => family.to_string(),
            (false, Some(Job::Downloading)) => crate::i18n::fmt(tl!("{font} (downloading…)"), &[("font", family)]),
            (false, Some(Job::Failed(_))) => crate::i18n::fmt(tl!("{font} (download failed: click to try again)"), &[("font", family)]),
            (false, None) => crate::i18n::fmt(tl!("{font} (download)"), &[("font", family)]),
        };
        let tip = if installed {
            tl!(why).to_string()
        } else {
            format!("{}\n{}", tl!(why), tl!("Downloaded from Google Fonts (SIL Open Font License) on the first click"))
        };
        if ui.button(label).on_hover_text(tip).clicked() {
            if !installed {
                download_font(app, ui.ctx(), family);
            }
            crate::widgets::run_from_ribbon(app, "design.themeFonts", json!({"heading": family, "body": family}));
            // Another computer seldom has it: a Word document saved from here carries it.
            if !app.session.doc.settings.embed_fonts {
                crate::widgets::run_from_ribbon(app, "file.embedFonts", json!({"on": true}));
            }
            ui.close();
        }
    }
    let on = app.session.doc.settings.embed_fonts;
    mi_check(ui, app, "Embed Fonts in the File", on, "file.embedFonts", json!({"on": !on}));
}

/// Starts downloading `family`, turning downloading on for the person's own click (a click the
/// control channel made leaves it off and says why).
fn download_font(app: &mut WordApp, ctx: &egui::Context, family: &str) {
    use wordcraft_engine::cmd::font_store;
    let Some(backend) = wordcraft_engine::packs::backend() else {
        app.status(tl!("Downloading fonts isn't available here").to_string());
        return;
    };
    if !backend.network_on() {
        if app.input_from_control(ctx) {
            app.status(tl!("Only you can turn on downloading: click the button yourself. Agents can install once it is on.").to_string());
            return;
        }
        if let Err(e) = backend.enable_network() {
            app.status(crate::i18n::fmt(tl!("Couldn't turn on downloading: {error}"), &[("error", &e)]));
            return;
        }
    }
    if let Err(e) = font_store::start(family) {
        app.status(crate::i18n::fmt(tl!("Couldn't download the font: {error}"), &[("error", &e)]));
    }
}

#[cfg(test)]
mod tests {
    use super::super::vec2;
    use super::*;
    use wordcraft_engine::Session;

    /// Design › Document Formatting › DSA: ticking Reading in the menu turns that version on.
    #[test]
    fn the_dsa_menu_turns_a_version_on() {
        use egui_kittest::kittest::Queryable;
        let a = WordApp::new(Session::new(wordcraft_doc::Document::from_text("Text")), crate::Services::default());
        let mut h = egui_kittest::Harness::builder()
            .with_size(vec2(1600.0, 400.0))
            .with_context_setup(|ctx| ctx.set_fonts(crate::theme::font_definitions(false, false)))
            .build_ui_state(|ui, app: &mut WordApp| super::super::design(app, ui), a);
        h.run_steps(2);
        h.get_by_label("DSA formatting: the test laid out for students with specific learning difficulties; the versions combine").click();
        h.run_steps(2);
        h.get_by_label("Reading (dyslexia)").click();
        h.run_steps(2);
        let st = h.state_mut().session.run("design.dsa", &json!({})).unwrap();
        assert_eq!(st["reading"], true, "{st}");
        assert_eq!(st["writing"], false, "{st}");
    }

    /// Design › Fonts offers the DSA typefaces (to download when missing) and Embed Fonts in the
    /// File, a document setting.
    #[test]
    fn the_fonts_menu_offers_the_dsa_fonts_and_embedding() {
        use egui_kittest::kittest::Queryable;
        let a = WordApp::new(Session::new(wordcraft_doc::Document::from_text("Text")), crate::Services::default());
        let mut h = egui_kittest::Harness::builder()
            .with_size(vec2(1600.0, 900.0))
            .with_context_setup(|ctx| ctx.set_fonts(crate::theme::font_definitions(false, false)))
            .build_ui_state(|ui, app: &mut WordApp| super::super::design(app, ui), a);
        h.run_steps(2);
        h.get_by_label("Theme Fonts").click();
        h.run_steps(2);
        for (family, _) in wordcraft_engine::cmd::font_store::OFFERED {
            assert!(h.query_by_label_contains(family).is_some(), "{family} offered");
        }
        h.get_by_label("Embed Fonts in the File").click();
        h.run_steps(2);
        assert!(h.state().session.doc.settings.embed_fonts);
    }
}
