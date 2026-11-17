//! Download Dictionary and Download Thesaurus: when the language at the caret has no spelling
//! dictionary (the status bar says "(no dictionary)") or no thesaurus, its menu and File › Options
//! › Language offer to download it (`proof.downloadDictionary`, `proof.downloadThesaurus`), and
//! the Thesaurus pane offers the thesaurus. A download runs off the interface's thread; when it
//! lands, the text is checked with the dictionary at once (the Thesaurus reads the thesaurus at
//! its next lookup) and the status bar says so.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use egui::Ui;
use wordcraft_engine::cmd::dictionaries::Source;

use crate::WordApp;

/// What a download fetches.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum What {
    Dictionary,
    Thesaurus,
}

/// A download in progress (by name) and how it ended, kept in the context's memory, one per kind.
#[derive(Clone, Default)]
struct Job(Arc<Mutex<Option<(&'static str, Option<Result<std::path::PathBuf, String>>)>>>);

fn job(ctx: &egui::Context, what: What) -> Job {
    let id = match what {
        What::Dictionary => egui::Id::unique("dictionary_download"),
        What::Thesaurus => egui::Id::unique("thesaurus_download"),
    };
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Job>(id).clone())
}

/// Thesauri downloaded this run: the Thesaurus pane looks its word up again when it changes.
static THESAURI_DOWNLOADED: AtomicU64 = AtomicU64::new(0);

/// How many thesauri were downloaded this run.
pub(crate) fn thesauri_downloaded() -> u64 {
    THESAURI_DOWNLOADED.load(Ordering::Relaxed)
}

/// The dictionary the caret's language lacks and Strix can download, if any.
fn wanted(app: &WordApp) -> Option<&'static Source> {
    let lang = wordcraft_engine::cmd::dictionaries::caret_lang(&app.session);
    let missing = wordcraft_engine::proof::missing_dictionary(lang.as_deref()).is_some()
        || wordcraft_engine::proof::unreadable_dictionary(lang.as_deref()).is_some();
    if missing { wordcraft_engine::cmd::dictionaries::source_for(lang.as_deref()).ok() } else { None }
}

/// The thesaurus the caret's language lacks and Strix can download, if any.
fn wanted_thesaurus(app: &WordApp) -> Option<&'static Source> {
    wordcraft_engine::cmd::thesaurus::missing(wordcraft_engine::cmd::dictionaries::caret_lang(&app.session).as_deref())
}

/// The name of the download of this kind in progress, if one is.
fn running(ctx: &egui::Context, what: What) -> Option<&'static str> {
    job(ctx, what).0.lock().ok().and_then(|g| g.as_ref().filter(|(_, done)| done.is_none()).map(|(n, _)| *n))
}

/// The offers, as menu items or buttons: Download the {name} spelling dictionary, and Download the
/// {name} thesaurus (each with its licence and source in the tooltip), or "Downloading…" while one
/// runs. Nothing for what the caret's language has, needs none of, or Strix has none for — and on
/// the web. Says whether it showed something.
pub(crate) fn offer(ui: &mut Ui, app: &mut WordApp) -> bool {
    let dictionary = offer_dictionary(ui, app);
    let thesaurus = offer_thesaurus(ui, app);
    dictionary || thesaurus
}

fn offer_dictionary(ui: &mut Ui, app: &mut WordApp) -> bool {
    if cfg!(target_arch = "wasm32") {
        return false;
    }
    if let Some(name) = running(ui.ctx(), What::Dictionary) {
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
        start(ui.ctx(), What::Dictionary, src);
        ui.close();
    }
    true
}

/// The thesaurus offer alone (the Thesaurus pane shows it where it has nothing better to say).
pub(crate) fn offer_thesaurus(ui: &mut Ui, app: &mut WordApp) -> bool {
    if cfg!(target_arch = "wasm32") {
        return false;
    }
    if let Some(name) = running(ui.ctx(), What::Thesaurus) {
        ui.add_enabled(false, egui::Button::new(crate::i18n::fmt(tl!("Downloading the {name} thesaurus…"), &[("name", name)])));
        return true;
    }
    let Some(src) = wanted_thesaurus(app) else { return false };
    let label = crate::i18n::fmt(tl!("Download the {name} thesaurus"), &[("name", src.name)]);
    let tip = crate::i18n::fmt(
        tl!(
            "Downloads the thesaurus {name} from LibreOffice's dictionaries ({source}), licensed {licence}, with the files that state its licence, into your dictionaries folder. The Thesaurus and the Synonyms menu use it at once."
        ),
        &[("name", src.name), ("source", &src.page()), ("licence", src.licence)],
    );
    if ui.button(label).on_hover_text(tip).clicked() {
        start(ui.ctx(), What::Thesaurus, src);
        ui.close();
    }
    true
}

fn start(ctx: &egui::Context, what: What, src: &'static Source) {
    let job = job(ctx, what);
    if let Ok(mut g) = job.0.lock() {
        *g = Some((src.name, None));
    }
    let worker = job.clone();
    let ctx = ctx.clone();
    let thread = match what {
        What::Dictionary => "dictionary-download",
        What::Thesaurus => "thesaurus-download",
    };
    let spawned = std::thread::Builder::new().name(thread.into()).spawn(move || {
        let result = match what {
            What::Dictionary => wordcraft_engine::cmd::dictionaries::download(src),
            What::Thesaurus => wordcraft_engine::cmd::thesaurus::download(src),
        }
        .map_err(|e| e.to_string());
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

/// Finished downloads: say how they went in the status bar (each frame; cheap).
pub(crate) fn poll(ctx: &egui::Context, app: &mut WordApp) {
    for what in [What::Dictionary, What::Thesaurus] {
        let job = job(ctx, what);
        let Ok(mut g) = job.0.lock() else { continue };
        if !matches!(&*g, Some((_, Some(_)))) {
            continue;
        }
        let Some((name, Some(result))) = g.take() else { continue };
        drop(g);
        let lang = crate::ribbon::caret_language(app);
        match (what, result) {
            (What::Dictionary, Ok(_)) => {
                app.status(crate::i18n::fmt(
                    tl!("The {name} dictionary is installed: spelling in {language} is checked now."),
                    &[("name", name), ("language", &lang)],
                ));
                app.session.relayout();
            }
            (What::Dictionary, Err(e)) => {
                app.status(crate::i18n::fmt(tl!("The {name} dictionary couldn't be downloaded: {reason}"), &[("name", name), ("reason", &e)]))
            }
            (What::Thesaurus, Ok(_)) => {
                THESAURI_DOWNLOADED.fetch_add(1, Ordering::Relaxed);
                app.status(crate::i18n::fmt(
                    tl!("The {name} thesaurus is installed: synonyms in {language} come from it now."),
                    &[("name", name), ("language", &lang)],
                ));
            }
            (What::Thesaurus, Err(e)) => {
                app.status(crate::i18n::fmt(tl!("The {name} thesaurus couldn't be downloaded: {reason}"), &[("name", name), ("reason", &e)]))
            }
        }
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
        // Italian's thesaurus (unless this machine has it), with its licence; none for Japanese.
        let it = app("it-IT");
        match wanted_thesaurus(&it) {
            Some(src) => assert_eq!((src.name, src.licence), ("it_IT", "GPL-3.0")),
            None => assert!(wordcraft_engine::cmd::thesaurus::missing(Some("it-IT")).is_none()),
        }
        assert_eq!(wanted_thesaurus(&app("ja-JP")), None);
    }

    #[test]
    fn a_finished_download_is_reported_in_the_status_bar() {
        let ctx = egui::Context::default();
        let mut a = app("hr-HR");
        if let Ok(mut g) = job(&ctx, What::Dictionary).0.lock() {
            *g = Some(("hr_HR", None));
        }
        assert_eq!(running(&ctx, What::Dictionary), Some("hr_HR"));
        assert_eq!(running(&ctx, What::Thesaurus), None, "each kind has its own download");
        poll(&ctx, &mut a);
        assert!(a.status_msg.is_none(), "still running: nothing to say");
        assert_eq!(running(&ctx, What::Dictionary), Some("hr_HR"), "and it is still shown as running");
        if let Ok(mut g) = job(&ctx, What::Dictionary).0.lock() {
            *g = Some(("hr_HR", Some(Err("offline".into()))));
        }
        poll(&ctx, &mut a);
        let msg = a.status_msg.clone().map(|(m, _)| m).unwrap_or_default();
        assert!(msg.contains("hr_HR") && msg.contains("offline"), "{msg}");
        assert_eq!(running(&ctx, What::Dictionary), None);
    }

    #[test]
    fn a_finished_thesaurus_download_is_reported_and_looked_up_again() {
        let ctx = egui::Context::default();
        let mut a = app("it-IT");
        let before = thesauri_downloaded();
        if let Ok(mut g) = job(&ctx, What::Thesaurus).0.lock() {
            *g = Some(("it_IT", Some(Ok(std::path::PathBuf::from("th_it_IT_v2.dat")))));
        }
        poll(&ctx, &mut a);
        let msg = a.status_msg.clone().map(|(m, _)| m).unwrap_or_default();
        assert!(msg.contains("it_IT") && msg.contains("thesaurus"), "{msg}");
        assert!(thesauri_downloaded() > before);
        if let Ok(mut g) = job(&ctx, What::Thesaurus).0.lock() {
            *g = Some(("it_IT", Some(Err("offline".into()))));
        }
        poll(&ctx, &mut a);
        let msg = a.status_msg.clone().map(|(m, _)| m).unwrap_or_default();
        assert!(msg.contains("couldn't be downloaded") && msg.contains("offline"), "{msg}");
    }
}
