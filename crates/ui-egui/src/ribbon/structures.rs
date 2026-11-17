//! The Structures tab (Strutture, between Math and Teaching; `docs/structures.md`): insert a
//! structure, open its editor, and set its diagrams, display, units and student copy.

use super::common::stack;
use super::language::mi_check;
use super::{Ui, Value, WordApp, big_toggle, group, json, menu_button, small};

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
                    super::mi(ui, app, label, "structure.insert", json!({"template": template, "openPane": true}));
                }
            },
        );
        let pane = app.session.view.structure_pane;
        big_toggle(ui, app, "structureEditor", "Structure\nEditor", "view.structurePane", pane);
    });
    let d = &st["diagrams"];
    let shown = |k: &str| d[k]["show"].as_bool().unwrap_or(true);
    let flag = |k: &str| d[k].as_bool().unwrap_or(true);
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
