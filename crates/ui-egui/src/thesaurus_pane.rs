//! The Thesaurus pane (Review › Proofing › Thesaurus, Shift+F7; docs/thesaurus.md): the word at
//! the caret, or one typed in its box, with its synonyms grouped by meaning from the thesaurus of
//! the caret's language (`review.thesaurus`). A synonym looks itself up when clicked; Insert puts
//! it in place of the word at the caret. When the language has a thesaurus Strix can download and
//! none is installed, the pane offers it (`proof.downloadThesaurus`).

use egui::Ui;
use serde_json::{Value, json};

use crate::WordApp;

/// What the pane shows, kept in the context's memory: the word looked up, the caret's word it last
/// followed, and the answer with what it was asked for (word, language, thesauri downloaded).
#[derive(Clone, Default)]
struct State {
    query: String,
    followed: String,
    answer: Option<(String, Option<String>, u64, Value)>,
}

fn id() -> egui::Id {
    egui::Id::unique("thesaurus_pane")
}

/// The thesaurus's own words the pane drew last (the word, its entry, categories and synonyms):
/// content, not interface wording (the translation audit leaves them alone).
#[cfg(test)]
static SHOWN: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// What [`SHOWN`] holds.
#[cfg(test)]
pub(crate) fn content_text() -> Vec<String> {
    SHOWN.lock().map(|g| g.clone()).unwrap_or_default()
}

/// The word at the caret, lower case (empty when there is none).
fn caret_word(app: &WordApp) -> String {
    let f = &app.session.sel.focus;
    let Some(p) = app.session.doc.para_at(f) else { return String::new() };
    let (a, b) = p.word_at(f.off);
    p.text.get(a..b).unwrap_or("").trim().to_lowercase()
}

/// `review.thesaurus` for `word`, asked again only when the word, the caret's language or the
/// thesauri installed change.
fn answer(app: &mut WordApp, st: &mut State, word: &str) -> Value {
    let lang = wordcraft_engine::cmd::dictionaries::caret_lang(&app.session);
    let epoch = crate::dictionary_download::thesauri_downloaded();
    if let Some((w, l, e, v)) = &st.answer
        && w == word
        && *l == lang
        && *e == epoch
    {
        return v.clone();
    }
    let v = app.session.run("review.thesaurus", &json!({"word": word})).unwrap_or(Value::Null);
    st.answer = Some((word.to_string(), lang, epoch, v.clone()));
    v
}

/// Put `w` in place of the word at the caret (as the context menu's Synonyms do).
fn insert(app: &mut WordApp, w: &str) {
    let _ = app.run("select.word", json!({}));
    let (a, b) = app.session.sel.ordered();
    let trimmed = app.session.doc.para_at(&a).and_then(|p| p.text.get(a.off..b.off)).map(|t| t.trim_end().len()).unwrap_or(0);
    let end = wordcraft_doc::Pos { off: a.off.saturating_add(trimmed), ..b };
    app.session.sel = wordcraft_engine::Selection { anchor: a, focus: end };
    if let Err(e) = app.run("text.insert", json!({"text": w, "raw": true})) {
        app.status(e);
    }
}

pub fn pane(app: &mut WordApp, ui: &mut Ui) {
    let mut st: State = ui.ctx().data_mut(|d| d.get_temp::<State>(id())).unwrap_or_default();
    // Follow the caret: a new word there is looked up (a word typed in the box stays until then).
    let at_caret = caret_word(app);
    if at_caret != st.followed {
        st.followed = at_caret.clone();
        if !at_caret.is_empty() {
            st.query = at_caret;
        }
    }
    let mut look_up: Option<String> = None;
    let mut to_insert: Option<String> = None;
    ui.horizontal(|ui| {
        let field = ui.add(egui::TextEdit::singleline(&mut st.query).hint_text(tl!("Search for a word")).desired_width(ui.available_width() - 34.0));
        if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            look_up = Some(st.query.trim().to_lowercase());
        }
        if ui.button("🔍").on_hover_text(tl!("Look up")).clicked() {
            look_up = Some(st.query.trim().to_lowercase());
        }
    });
    if let Some(w) = look_up.take() {
        st.query = w;
    }
    let word = st.query.trim().to_lowercase();
    let v = if word.is_empty() { Value::Null } else { answer(app, &mut st, &word) };
    #[cfg(test)]
    {
        let mut shown: Vec<String> = vec![st.query.clone(), word.clone()];
        shown.extend(v.get("entry").and_then(Value::as_str).map(str::to_string));
        for m in v.get("meanings").and_then(Value::as_array).into_iter().flatten() {
            if let Some(c) = m.get("category").and_then(Value::as_str) {
                shown.extend([c.to_string(), c.to_uppercase()]);
            }
            shown.extend(m.get("synonyms").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string));
        }
        if let Ok(mut g) = SHOWN.lock() {
            *g = shown;
        }
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        if word.is_empty() {
            crate::pane_kit::note(ui, tl!("Put the insertion point in a word, or type one above, to see its synonyms."));
        } else {
            let meanings = v.get("meanings").and_then(Value::as_array).cloned().unwrap_or_default();
            if meanings.is_empty() {
                crate::pane_kit::note(ui, &crate::i18n::fmt(tl!("No synonyms found for “{word}”."), &[("word", &word)]));
            }
            if let Some(entry) = v.get("entry").and_then(Value::as_str).filter(|e| *e != word) {
                crate::pane_kit::note(ui, &crate::i18n::fmt(tl!("Showing “{entry}”."), &[("entry", entry)]));
            }
            for (i, m) in meanings.iter().enumerate() {
                let category = m.get("category").and_then(Value::as_str).map_or_else(|| tl!("Synonyms").to_string(), str::to_string);
                crate::pane_kit::section(ui, &format!("{:02}", i.saturating_add(1)), &category);
                for s in m.get("synonyms").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
                    ui.horizontal(|ui| {
                        if ui.link(s).on_hover_text(tl!("Look up this word")).clicked() {
                            look_up = Some(s.to_lowercase());
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button(tl!("Insert")).on_hover_text(tl!("Replace the word at the insertion point with this one")).clicked() {
                                to_insert = Some(s.to_string());
                            }
                        });
                    });
                }
            }
            ui.add_space(8.0);
            // Where the synonyms come from.
            match v.get("thesaurus").and_then(Value::as_str) {
                Some(name) => crate::pane_kit::note(ui, &crate::i18n::fmt(tl!("From the {name} thesaurus."), &[("name", name)])),
                None if !meanings.is_empty() => crate::pane_kit::note(ui, tl!("From Strix's short built-in list.")),
                None => {}
            }
            if let Some(e) = v.get("thesaurusError").and_then(Value::as_str) {
                crate::pane_kit::note(ui, &crate::i18n::fmt(tl!("The thesaurus couldn't be read: {reason}"), &[("reason", e)]));
            }
        }
        // The caret's language has a thesaurus to download: offer it.
        if v.get("download").is_some_and(|d| !d.is_null()) || word.is_empty() {
            ui.add_space(4.0);
            let lang = crate::ribbon::caret_language(app);
            let mut shown = false;
            ui.scope(|ui| {
                shown = crate::dictionary_download::offer_thesaurus(ui, app);
            });
            if shown {
                crate::pane_kit::note(ui, &crate::i18n::fmt(tl!("No thesaurus for {language} is installed yet."), &[("language", &lang)]));
            }
        }
    });
    if let Some(w) = look_up {
        st.query = w;
    }
    if let Some(w) = to_insert {
        insert(app, &w);
        st.followed = caret_word(app);
    }
    ui.ctx().data_mut(|d| d.insert_temp(id(), st));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(text: &str, lang: &str) -> WordApp {
        let mut app = WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::new()), crate::Services::default());
        app.session.run("document.setText", &json!({"text": text})).unwrap();
        std::sync::Arc::make_mut(&mut app.session.doc.styles).default_chr.lang = Some(lang.into());
        app
    }

    #[test]
    fn the_ribbon_command_opens_the_pane() {
        let mut a = app("good", "en-US");
        assert!(!a.session.view.thesaurus_pane);
        a.run("review.thesaurus", json!({})).unwrap();
        assert!(a.session.view.thesaurus_pane);
        assert!(crate::panes::open(&a).contains(&crate::panes::Pane::Thesaurus));
        a.run("view.thesaurusPane", json!({"value": false})).unwrap();
        assert!(!a.session.view.thesaurus_pane);
    }

    #[test]
    fn insert_replaces_the_word_at_the_caret() {
        let mut a = app("a good day", "en-US");
        a.run("caret.docStart", json!({})).unwrap();
        for _ in 0..3 {
            a.run("caret.right", json!({})).unwrap();
        }
        assert_eq!(caret_word(&a), "good");
        insert(&mut a, "fine");
        assert_eq!(a.session.doc.plain_text(wordcraft_doc::StoryRef::Body).trim(), "a fine day");
    }

    #[test]
    fn the_answer_is_asked_again_only_when_something_changes() {
        let mut a = app("good", "en-US");
        let mut st = State::default();
        let v = answer(&mut a, &mut st, "good");
        assert_eq!(v["synonyms"][0], json!("fine"));
        let key = st.answer.as_ref().map(|(w, l, e, _)| (w.clone(), l.clone(), *e));
        assert_eq!(answer(&mut a, &mut st, "good"), v);
        assert_eq!(st.answer.as_ref().map(|(w, l, e, _)| (w.clone(), l.clone(), *e)), key);
        assert_ne!(answer(&mut a, &mut st, "bad"), v);
    }
}
