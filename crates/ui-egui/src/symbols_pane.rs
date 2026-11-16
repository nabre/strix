//! The Symbols pane (Equation › Symbols, the group's launcher; `view.symbolsPane`): every symbol
//! set of the Equation tab's gallery under its title, beside the document. It stays open while
//! writing; a click puts the symbol in the equation at the cursor (`equation.insertSymbol`).

use egui::{Ui, vec2};
use serde_json::json;
use wordcraft_engine::math_gallery::SYMBOL_SETS;

use crate::WordApp;
use crate::theme::Tokens;

/// The Symbols pane: each set's title over its symbols.
pub(crate) fn pane(app: &mut WordApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        for (name, chars) in SYMBOL_SETS {
            crate::pane_kit::section(ui, "", tl!(name));
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(1.0, 1.0);
                for c in chars.chars() {
                    if crate::equation_tab::symbol_button(ui, app, c, &t) {
                        let _ = app.run("equation.insertSymbol", json!({"char": c.to_string()}));
                        app.canvas.want_focus = true;
                    }
                }
            });
        }
        ui.add_space(8.0);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable;
    use wordcraft_engine::Session;

    /// The owner: the Symbols group's launcher shows the symbols in a pane beside the document,
    /// every set under its title. `view.symbolsPane` shows and hides it.
    #[test]
    fn symbols_are_a_pane_with_every_set_titled() {
        let mut app = WordApp::new(Session::new(wordcraft_doc::Document::from_text("Intro")), crate::Services::default());
        assert!(!crate::panes::open(&app).contains(&crate::panes::Pane::Symbols));
        app.run("view.symbolsPane", json!({})).unwrap();
        assert!(app.session.view.symbols_pane);
        assert!(crate::panes::open(&app).contains(&crate::panes::Pane::Symbols));
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(300.0, 2400.0)).build_ui_state(|ui, app: &mut WordApp| pane(app, ui), app);
        h.run_steps(2);
        for (name, _) in SYMBOL_SETS {
            h.get_by_label(&tl!(name).to_uppercase());
        }
        h.state_mut().run("view.symbolsPane", json!({})).unwrap();
        assert!(!crate::panes::open(h.state()).contains(&crate::panes::Pane::Symbols));
    }
}
