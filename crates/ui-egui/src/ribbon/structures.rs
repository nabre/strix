//! The Structures tab (Strutture, between Math and Teaching; `docs/structures.md`): insert a
//! structure, open its editor, and set its diagrams, display, units and student copy.

use super::common::stack;
use super::language::mi_check;
use super::{Ui, Value, WordApp, group, json, menu_button, small};

pub(super) fn structures(app: &mut WordApp, ui: &mut Ui) {
    let info = crate::structure_pane::info(app, ui);
    let st = info.as_ref().map(|i| i["structure"].clone()).unwrap_or(Value::Null);
    group(ui, "Structure", None, app, |ui, app| {
        menu_button(
            ui,
            app,
            crate::widgets::MenuButtonOptions::new(
                "structure",
                Some("Structure"),
                "Insert a beam, frame or truss with its reactions and diagrams",
                true,
                "structure.insert",
            ),
            |ui, app| {
                for (template, label) in crate::structure_pane::TEMPLATES {
                    super::mi(ui, app, label, "ui.structureNew", json!({"template": template}));
                }
            },
        );
        menu_button(
            ui,
            app,
            crate::widgets::MenuButtonOptions::new(
                "exercise",
                Some("Structure Exercise"),
                "A new exercise on a structure: the structure in the statement, the reactions and the diagrams asked, the working and the drawings in the solution",
                true,
                "structure.exercise",
            ),
            |ui, app| {
                for (template, label) in crate::structure_pane::TEMPLATES {
                    super::mi(ui, app, label, "structure.exercise", exercise_labels(template));
                }
            },
        );
        // One editor: the full-page one (also a double-click on the structure).
        let editing = app.structure_edit.is_some();
        let (cmd, label) = if editing { ("ui.structureClose", "Close\nEditor") } else { ("ui.structureEdit", "Structure\nEditor") };
        super::big(ui, app, "structureEdit", label, cmd, json!({}), false);
        // Report ⇄ exercise: the label says where the structure goes.
        let label = if wordcraft_engine::cmd::structure::in_exercise(&app.session) { "To\nReport" } else { "To\nExercise" };
        super::big(ui, app, "structureMode", label, "structure.mode", mode_labels(), false);
    });
    let d = &st["diagrams"];
    let shown = |k: &str| d[k]["show"].as_bool().unwrap_or(true);
    // Absent flags are on, but dimensions and stations, which a structure saved before them
    // lacks (off: its drawing stays as it was; a new one writes them).
    let flag = |k: &str| d[k].as_bool().unwrap_or(!matches!(k, "dimensions" | "stations"));
    let shear = st["titles"]["v"].as_str().unwrap_or("T").to_string();
    group(ui, "Diagrams", None, app, |ui, app| {
        stack(ui, |ui| {
            let on = flag("reactions");
            small(
                ui,
                app,
                "structureReactions",
                Some("Reactions"),
                "Draw the reactions on the structure, in a panel of their own",
                crate::widgets::ButtonAction::new("structure.set", json!({"diagrams": {"reactions": !on}}), on),
            );
            let on = shown("n");
            small(
                ui,
                app,
                "diagramN",
                Some("Axial Force N"),
                "Draw the axial force diagram (a truss: each member's force)",
                crate::widgets::ButtonAction::new("structure.set", json!({"diagrams": {"n": {"show": !on}}}), on),
            );
            let on = shown("v");
            let label = format!("{} {shear}", ts("Shear"));
            small(
                ui,
                app,
                "diagramT",
                Some(&label),
                "Draw the shear diagram",
                crate::widgets::ButtonAction::new("structure.set", json!({"diagrams": {"v": {"show": !on}}}), on),
            );
        });
        stack(ui, |ui| {
            let on = shown("m");
            small(
                ui,
                app,
                "diagramM",
                Some("Moment M"),
                "Draw the bending moment diagram",
                crate::widgets::ButtonAction::new("structure.set", json!({"diagrams": {"m": {"show": !on}}}), on),
            );
            menu_button(
                ui,
                app,
                crate::widgets::MenuButtonOptions::new(
                    "diagramSide",
                    Some("Positive Side"),
                    "Which side of each member a diagram's positive values are drawn on",
                    false,
                    "structure.set",
                ),
                |ui, app| positive_menu(ui, app, d, &shear),
            );
            small(
                ui,
                app,
                "structureItalian",
                Some("Italian Convention"),
                "N and T positive up, M on the stretched fibres (positive down)",
                crate::widgets::ButtonAction::new("structure.set", json!({"convention": "italian"}), false),
            );
        });
    });
    group(ui, "Show", None, app, |ui, app| {
        stack(ui, |ui| {
            let on = flag("values");
            small(
                ui,
                app,
                "numberFormat",
                Some("Values"),
                "Write the notable values: at the ends of each stretch, the jumps, the maxima",
                crate::widgets::ButtonAction::new("structure.set", json!({"diagrams": {"values": !on}}), on),
            );
            let on = flag("hatch");
            small(
                ui,
                app,
                "structureHatch",
                Some("Hatching"),
                "Hatch the diagrams' areas across the members",
                crate::widgets::ButtonAction::new("structure.set", json!({"diagrams": {"hatch": !on}}), on),
            );
            let on = flag("signs");
            small(
                ui,
                app,
                "structureSigns",
                Some("Signs"),
                "Mark each area of the diagrams with its sign",
                crate::widgets::ButtonAction::new("structure.set", json!({"diagrams": {"signs": !on}}), on),
            );
        });
        stack(ui, |ui| {
            let on = flag("dimensions");
            small(
                ui,
                app,
                "structureDimensions",
                Some("Dimensions"),
                "Write the structure's dimensions, the slopes of inclined members and the angles inclined forces make with their members",
                crate::widgets::ButtonAction::new("structure.set", json!({"diagrams": {"dimensions": !on}}), on),
            );
            let on = flag("stations");
            small(
                ui,
                app,
                "structureStations",
                Some("Stations"),
                "Mark on each diagram's axis where its notable values are: a load's position, a maximum, a zero, with the distance from the member's start",
                crate::widgets::ButtonAction::new("structure.set", json!({"diagrams": {"stations": !on}}), on),
            );
        });
        stack(ui, |ui| {
            let layout = st["layout"].as_str().unwrap_or("auto").to_string();
            menu_button(
                ui,
                app,
                crate::widgets::MenuButtonOptions::new(
                    "columns",
                    Some("Panel Layout"),
                    "How the panels are laid out: one under the other, or two by two",
                    false,
                    "structure.set",
                ),
                |ui, app| {
                    for (value, label) in [("auto", "Automatic (Frames in Two Columns)"), ("column", "One Column"), ("grid", "Two Columns")] {
                        mi_check(ui, app, label, layout == value, "structure.set", json!({"layout": value}));
                    }
                },
            );
            let lu = st["lengthUnit"].as_str().unwrap_or("m").to_string();
            let fu = st["forceUnit"].as_str().unwrap_or("kN").to_string();
            menu_button(
                ui,
                app,
                crate::widgets::MenuButtonOptions::new(
                    "ruler",
                    Some("Units"),
                    "The units of lengths and forces (moments are their product)",
                    false,
                    "structure.set",
                ),
                |ui, app| {
                    for u in ["m", "cm", "mm"] {
                        mi_check(ui, app, u, lu == u, "structure.set", json!({"lengthUnit": u}));
                    }
                    ui.separator();
                    for u in ["kN", "daN", "N"] {
                        mi_check(ui, app, u, fu == u, "structure.set", json!({"forceUnit": u}));
                    }
                },
            );
        });
    });
    group(ui, "Teaching", None, app, |ui, app| {
        super::big(ui, app, "steps", "Write\nWorking", "structure.working", crate::structure_pane::working_labels(), false);
        let answer = st["answer"].as_bool().unwrap_or(true);
        crate::widgets::big_checked(
            ui,
            app,
            "studentPreview",
            "Solution\nOnly",
            "structure.set",
            json!({"answer": !answer}),
            answer && info.is_some(),
        );
    });
}

/// The full-page structure editor's tab (`structure_pane::EDITOR_TAB`): its tools, the sheet's
/// snapping, grid and view, and the way back to the page.
pub(super) fn structure_editor(app: &mut WordApp, ui: &mut Ui) {
    let tools = crate::structure_pane::tools(ui.ctx());
    let tool = |name: &str| crate::widgets::ButtonAction::new("ui.structureTool", json!({"tool": name}), tools.tool == name);
    let big_tool = |ui: &mut Ui, app: &mut WordApp, icon: &str, label: &str, name: &str| {
        crate::widgets::big_checked(ui, app, icon, label, "ui.structureTool", json!({"tool": name}), tools.tool == name);
    };
    group(ui, "Selection", None, app, |ui, app| {
        big_tool(ui, app, "cadSelect", "Select", "select");
        stack(ui, |ui| {
            small(ui, app, "cadDelete", Some("Erase"), "Click a node or a member to remove it (a node with what uses it)", tool("erase"));
        });
    });
    group(ui, "Draw", None, app, |ui, app| {
        big_tool(ui, app, "structureNode", "Node", "node");
        big_tool(ui, app, "structureMember", "Member", "member");
    });
    group(ui, "Supports", None, app, |ui, app| {
        stack(ui, |ui| {
            small(ui, app, "structureFixed", Some("Fixed"), "Click a node to fix it (incastro)", tool("fixed"));
            small(ui, app, "structurePin", Some("Pin"), "Click a node to pin it (cerniera)", tool("pin"));
            small(ui, app, "structureRoller", Some("Roller"), "Click a node to put it on a roller (carrello)", tool("roller"));
        });
        stack(ui, |ui| {
            small(ui, app, "structureLink", Some("Link"), "Click a node to hold it with a link (pendolo)", tool("link"));
            small(ui, app, "structureSlider", Some("Slider"), "Click a node to hold it with a slider (glifo)", tool("slider"));
            small(ui, app, "structureHinge", Some("Internal hinge"), "Click a node to make it an internal hinge, or not", tool("hinge"));
        });
    });
    group(ui, "Loads", None, app, |ui, app| {
        stack(ui, |ui| {
            small(ui, app, "structureForce", Some("Force"), "Click a node or a point of a member to put a force there", tool("force"));
            small(ui, app, "structureCouple", Some("Couple"), "Click a node or a point of a member to put a couple there", tool("couple"));
            small(ui, app, "structureDistributed", Some("Distributed"), "Click a member to load it along its length", tool("distributed"));
        });
    });
    group(ui, "Sheet", None, app, |ui, app| {
        stack(ui, |ui| {
            small(
                ui,
                app,
                "cadSnap",
                Some("Snap"),
                "Snap to the grid (always to the nodes)",
                crate::widgets::ButtonAction::new("ui.structureSnap", json!({}), tools.snap),
            );
            ui.horizontal(|ui| {
                ui.add_space(4.0);
                ui.label(tl!("Grid"));
                let mut grid = tools.grid;
                let r = ui.add(egui::DragValue::new(&mut grid).range(0.01..=100.0).speed(0.05).max_decimals(3));
                if r.on_hover_text(tl!("The grid's step, in the structure's length unit")).changed() {
                    let _ = app.run("ui.structureGrid", json!({"size": grid}));
                }
            });
            small(ui, app, "cadFit", Some("Fit"), "Show the whole structure", crate::widgets::ButtonAction::new("ui.structureFit", json!({}), false));
        });
        stack(ui, |ui| {
            small(
                ui,
                app,
                "structureDimensions",
                Some("Dimensions"),
                crate::structure_pane::DIMS_TIP,
                crate::widgets::ButtonAction::new("ui.structureDimensions", json!({}), tools.dims),
            );
        });
    });
    group(ui, "Teaching", None, app, |ui, app| {
        super::big(ui, app, "steps", "Write\nWorking", "structure.working", crate::structure_pane::working_labels(), false);
    });
    group(ui, "Close", None, app, |ui, app| {
        super::big(ui, app, "cadClose", "Close\nEditor", "ui.structureClose", json!({}), false);
    });
}

/// Positive Side: up or down for each diagram.
fn positive_menu(ui: &mut Ui, app: &mut WordApp, d: &Value, shear: &str) {
    let rows = [("n", "N".to_string()), ("v", shear.to_string()), ("m", "M".to_string())];
    for (k, name) in rows {
        let side = d[k]["positive"].as_str().unwrap_or(if k == "m" { "down" } else { "up" }).to_string();
        for (value, label) in [("up", "Positive Up"), ("down", "Positive Down")] {
            let text = format!("{name}: {}", ts(label));
            mi_check(ui, app, &text, side == value, "structure.set", json!({"diagrams": {k: {"positive": value}}}));
        }
        if k != "m" {
            ui.separator();
        }
    }
}

/// `s` in the interface's language as the structure editor words it.
fn ts(s: &str) -> &str {
    crate::i18n::tc("structure", s)
}

/// `structure.mode`'s params: the exercise's words and the working's in the interface's language.
fn mode_labels() -> Value {
    let mut v = exercise_labels("");
    if let Some(o) = v.as_object_mut() {
        o.remove("template");
    }
    v
}

/// `structure.exercise`'s params for `template`: the exercise's words and the working's in the
/// interface's language.
fn exercise_labels(template: &str) -> Value {
    let mut v = crate::structure_pane::working_labels();
    if let Some(l) = v.get_mut("labels").and_then(Value::as_object_mut) {
        l.insert("title".into(), json!(ts("Structure")));
        l.insert("text".into(), json!(ts("For the structure in the figure, loaded as shown:")));
        l.insert("jump".into(), json!(ts("{T} changes sign under a concentrated load")));
        l.insert("nozero".into(), json!(ts("{T} never vanishes nor changes sign: the greatest moment is at an end of a stretch.")));
    }
    if let Some(o) = v.as_object_mut() {
        o.insert(
            "questions".into(),
            json!({
                "reactions": ts("Find the support reactions."),
                "axial": ts("Work out and draw the diagram of the axial force N."),
                "shear": ts("Work out and draw the diagram of the shear force {T}."),
                "zeros": ts("Find the points where the shear force {T} is zero."),
                "maxima": ts("Work out the greatest bending moment at those points ({T}(x) = 0)."),
                "moments": ts("Work out and draw the diagram of the bending moment M."),
                "trussAxial": ts("Find the axial force in every member and draw it."),
            }),
        );
        o.insert("template".into(), json!(template));
    }
    v
}
