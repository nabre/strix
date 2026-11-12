//! Download Dictionary: when the language at the caret has no spelling dictionary (the status
//! bar says "(no dictionary)"), its menu and File › Options › Language offer to download it
//! (`proof.downloadDictionary`). The download runs off the interface's thread; when it lands the
//! text is checked with it at once and the status bar says so.

use std::sync::{Arc, Mutex};

use egui::Ui;

use crate::WordApp;

/// A download in progress (by dictionary name) and how it ended, kept in the context's memory.
#[derive(Clone, Default)]
struct Job(Arc<Mutex<Option<(&'static str, Option<Result<std::path::PathBuf, String>>)>>>);

fn job(ctx: &egui::Context) -> Job {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Job>(egui::Id::unique("dictionary_download")).clone())
}

/// The dictionary the caret's language lacks and Strix can download, if any.
fn wanted(app: &WordApp) -> Option<&'static wordcraft_engine::cmd::dictionaries::Source> {
    let lang = wordcraft_engine::cmd::dictionaries::caret_lang(&app.session);
    let missing = wordcraft_engine::proof::missing_dictionary(lang.as_deref()).is_some()
        || wordcraft_engine::proof::unreadable_dictionary(lang.as_deref()).is_some();
    if missing { wordcraft_engine::cmd::dictionaries::source_for(lang.as_deref()).ok() } else { None }
}

/// The name of the dictionary being downloaded, if one is.
fn running(ctx: &egui::Context) -> Option<&'static str> {
    job(ctx).0.lock().ok().and_then(|g| g.as_ref().filter(|(_, done)| done.is_none()).map(|(n, _)| *n))
}

/// The offer, as a menu item or a button: Download the {name} dictionary (its licence and
/// source in the tooltip), or "Downloading…" while it runs. Nothing when the caret's language
/// has its dictionary, needs none, or Strix has none for it — and on the web. Says whether it
/// showed something.
pub(crate) fn offer(ui: &mut Ui, app: &mut WordApp) -> bool {
    if cfg!(target_arch = "wasm32") {
        return false;
    }
    if let Some(name) = running(ui.ctx()) {
        ui.add_enabled(false, egui::Button::new(crate::i18n::fmt(tl!("Downloading the {name} dictionary…"), &[("name", name)])));
        return true;
    }
    let Some(src) = wanted(app) else { return false };
    let label = crate::i18n::fmt(tl!("Download the {name} spelling dictionary"), &[("name", src.name)]);
    let tip = crate::i18n::fmt(
        tl!(
            "Downloads the Hunspell dictionary {name} from LibreOffice's dictionaries ({source}), licensed {licence}, into your dictionaries folder, and checks spelling with it at once."
        ),
        &[("name", src.name), ("source", &src.page()), ("licence", src.licence)],
    );
    if ui.button(label).on_hover_text(tip).clicked() {
        start(ui.ctx(), src);
        ui.close();
    }
    true
}

fn start(ctx: &egui::Context, src: &'static wordcraft_engine::cmd::dictionaries::Source) {
    let job = job(ctx);
    if let Ok(mut g) = job.0.lock() {
        *g = Some((src.name, None));
    }
    let worker = job.clone();
    let ctx = ctx.clone();
    let spawned = std::thread::Builder::new().name("dictionary-download".into()).spawn(move || {
        let result = wordcraft_engine::cmd::dictionaries::download(src).map_err(|e| e.to_string());
        if let Ok(mut g) = worker.0.lock() {
            *g = Some((src.name, Some(result)));
        }
        ctx.request_repaint();
    });
    if let Err(e) = spawned
        && let Ok(mut g) = job.0.lock()
    {
        *g = Some((src.name, Some(Err(e.to_string()))));
    }
}

/// A finished download: say how it went in the status bar (each frame; cheap).
pub(crate) fn poll(ctx: &egui::Context, app: &mut WordApp) {
    let job = job(ctx);
    let Ok(mut g) = job.0.lock() else { return };
    if !matches!(&*g, Some((_, Some(_)))) {
        return;
    }
    let Some((name, Some(result))) = g.take() else { return };
    drop(g);
    match result {
        Ok(_) => {
            let lang = crate::ribbon::caret_language(app);
            app.status(crate::i18n::fmt(
                tl!("The {name} dictionary is installed: spelling in {language} is checked now."),
                &[("name", name), ("language", &lang)],
            ));
            app.session.relayout();
        }
        Err(e) => app.status(crate::i18n::fmt(tl!("The {name} dictionary couldn't be downloaded: {reason}"), &[("name", name), ("reason", &e)])),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn app(lang: &str) -> WordApp {
        let mut app = WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::new()), crate::Services::default());
        app.session.run("document.setText", &json!({"text": "Testo"})).unwrap();
        std::sync::Arc::make_mut(&mut app.session.doc.styles).default_chr.lang = Some(lang.into());
        app
    }

    /// The owner asked for the dictionaries to be found and downloaded from the program: the
    /// caret's language without one is offered its own, with its licence.
    #[test]
    fn the_caret_language_without_a_dictionary_is_offered_one() {
        // English needs none; Japanese has none to offer.
        assert_eq!(wanted(&app("en-US")), None);
        assert_eq!(wanted(&app("ja-JP")), None);
        // Croatian (unless this machine has it installed).
        let hr = app("hr-HR");
        if wordcraft_engine::proof::missing_dictionary(Some("hr-HR")).is_some() {
            let src = wanted(&hr).unwrap();
            assert_eq!((src.name, src.licence), ("hr_HR", "GPL-2.0, LGPL-2.1 or MPL-1.1"));
        } else {
            assert_eq!(wanted(&hr), None, "installed: nothing to offer");
        }
    }

    #[test]
    fn a_finished_download_is_reported_in_the_status_bar() {
        let ctx = egui::Context::default();
        let mut a = app("hr-HR");
        if let Ok(mut g) = job(&ctx).0.lock() {
            *g = Some(("hr_HR", None));
        }
        assert_eq!(running(&ctx), Some("hr_HR"));
        poll(&ctx, &mut a);
        assert!(a.status_msg.is_none(), "still running: nothing to say");
        assert_eq!(running(&ctx), Some("hr_HR"), "and it is still shown as running");
        if let Ok(mut g) = job(&ctx).0.lock() {
            *g = Some(("hr_HR", Some(Err("offline".into()))));
        }
        poll(&ctx, &mut a);
        let msg = a.status_msg.clone().map(|(m, _)| m).unwrap_or_default();
        assert!(msg.contains("hr_HR") && msg.contains("offline"), "{msg}");
        assert_eq!(running(&ctx), None);
    }
}
