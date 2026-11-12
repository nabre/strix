//! Fonts downloaded on demand from the suite's font store (craft-libs `craft-font-store`: Google
//! Fonts and Noto, pinned and verified by sha256), for the typefaces Strix offers without bundling
//! them: Atkinson Hyperlegible and Lexend in Design › Fonts (docs/dsa.md).
//!
//! The store is the Language packs' one (the same folder and the same single consent setting,
//! [`crate::packs`]): downloading must be on, which only a person in the app turns on (an Install
//! click, or a click on a font to download here). The command never turns it on. A downloaded
//! family is added to the font database at once and kept in the user font folder for next time.

use std::sync::{Arc, Mutex, PoisonError};

use serde_json::{Value, json};

use crate::{CmdError, CmdResult, CommandSpec, Session, p};

/// The families Design › Fonts offers to download, (family, what it is for).
pub const OFFERED: [(&str, &str); 2] = [
    ("Atkinson Hyperlegible", "letters told apart at a glance, for low vision and dyslexia"),
    ("Lexend", "wide letters and spacing, for reading fluency"),
];

/// Longest family name taken, in bytes.
const MAX_FAMILY_LEN: usize = 128;
/// Most font files of one family added to the font database.
const MAX_FILES: usize = 64;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("font.download", "Download Font", "Design › Document Formatting", download)
            .params(r#"{"family": string (a family of the font store's sources, Google Fonts and Noto, e.g. "Lexend", "Atkinson Hyperlegible")} — downloads it, verified by sha256, and adds it to the fonts at once (kept for the next start); a family already installed is left as is. Needs downloading on (only the person in the app can turn it on). Opens nothing. In the app it runs in the background and returns {"started": true}: follow it with {"family"} again ("downloading" while it runs, "installed"; a failed download starts again, the failure logged); elsewhere it returns when installed → {"family", "installed": bool, "files": n}"#)
            .pure()
            .when(|_| if cfg!(target_arch = "wasm32") { Some("downloading fonts isn't available in the browser") } else { None }),
        CommandSpec::new("file.embedFonts", "Embed Fonts in the File", "Design › Document Formatting", embed_fonts)
            .params(r#"{"on"?: bool} → saving as a Word document puts in it the fonts you added or downloaded (Atkinson Hyperlegible, Lexend…) that the document uses, so Word shows them on another computer (Word's Embed fonts in the file; the system's fonts aren't embedded); without params, says → {"on", "fonts": [family] (embedded on the next save)}"#),
    ]
}

fn embed_fonts(s: &mut Session, v: &Value) -> CmdResult {
    if let Some(on) = v.get("on") {
        let on = on.as_bool().ok_or_else(|| CmdError::Params("`on` must be true or false".into()))?;
        s.doc.settings.embed_fonts = on;
    }
    let mut fonts: Vec<String> = Vec::new();
    for f in crate::font_embed::fonts_to_embed(&s.doc).into_iter().filter(|f| wordcraft_docx::embeddable(&f.data)) {
        if !fonts.iter().any(|x| x.eq_ignore_ascii_case(&f.family)) {
            fonts.push(f.family);
        }
    }
    Ok(json!({"on": s.doc.settings.embed_fonts, "fonts": fonts}))
}

/// A download running or ended in the background.
#[derive(Clone, Debug, PartialEq)]
pub enum Job {
    Downloading,
    Failed(String),
}

static JOBS: Mutex<Vec<(String, Job)>> = Mutex::new(Vec::new());
type Notify = Arc<dyn Fn() + Send + Sync>;
static NOTIFY: Mutex<Option<Notify>> = Mutex::new(None);

fn jobs() -> std::sync::MutexGuard<'static, Vec<(String, Job)>> {
    JOBS.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Called as each background download ends (the window repaints). Set by the app.
pub fn set_notify(notify: Notify) {
    *NOTIFY.lock().unwrap_or_else(PoisonError::into_inner) = Some(notify);
}

/// The background download of `family`, running or failed; `None` when there is none.
pub fn job(family: &str) -> Option<Job> {
    jobs().iter().find(|(f, _)| f.eq_ignore_ascii_case(family)).map(|(_, j)| j.clone())
}

/// Whether the fonts can draw `family` now.
pub fn installed(family: &str) -> bool {
    wordcraft_fonts::FontDb::global().has_family(family)
}

/// Whether there is a font store to download into, and downloading is on.
pub fn network_on() -> bool {
    crate::packs::backend().is_some_and(|b| b.network_on())
}

fn family_param(v: &Value) -> Result<String, CmdError> {
    let f = p::str(v, "family").map(str::trim).unwrap_or("");
    if f.is_empty() || f.len() > MAX_FAMILY_LEN || f.chars().any(char::is_control) {
        return Err(CmdError::Params("`family` (a font family name like Lexend) is required".into()));
    }
    Ok(f.to_string())
}

fn download(_: &mut Session, v: &Value) -> CmdResult {
    let family = family_param(v)?;
    if installed(&family) {
        jobs().retain(|(f, _)| !f.eq_ignore_ascii_case(&family));
        return Ok(json!({"family": family, "installed": true, "files": 0}));
    }
    match job(&family) {
        Some(Job::Downloading) => return Ok(json!({"family": family, "installed": false, "downloading": true})),
        Some(Job::Failed(_)) | None => {}
    }
    if crate::packs::runs_in_background() {
        start(&family).map_err(CmdError::Failed)?;
        return Ok(json!({"family": family, "started": true}));
    }
    let files = install(&family).map_err(CmdError::Failed)?;
    Ok(json!({"family": family, "installed": true, "files": files}))
}

/// Download `family` on a thread (the app): [`job`] follows it. Downloading must be on.
pub fn start(family: &str) -> Result<(), String> {
    check(family)?;
    {
        let mut j = jobs();
        if j.iter().any(|(f, s)| f.eq_ignore_ascii_case(family) && *s == Job::Downloading) {
            return Ok(());
        }
        j.retain(|(f, _)| !f.eq_ignore_ascii_case(family));
        j.push((family.to_string(), Job::Downloading));
    }
    let family = family.to_string();
    let run = move || {
        let result = install(&family);
        {
            let mut j = jobs();
            j.retain(|(f, _)| !f.eq_ignore_ascii_case(&family));
            if let Err(e) = result {
                log::warn!("font download of {family:?} failed: {e}");
                j.push((family, Job::Failed(e)));
            }
        }
        if let Some(n) = NOTIFY.lock().unwrap_or_else(PoisonError::into_inner).clone() {
            n();
        }
    };
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::thread::Builder::new().name("font-download".into()).spawn(run).map(|_| ()).map_err(|e| e.to_string())
    }
    #[cfg(target_arch = "wasm32")]
    {
        drop(run);
        Err("downloading fonts isn't available in the browser".into())
    }
}

/// What stops a download before it starts.
fn check(_family: &str) -> Result<(), String> {
    let b = crate::packs::backend().ok_or("there is no font store to download into")?;
    if !b.network_on() {
        return Err("downloading is turned off: turn it on in Strix (click the font in Design › Fonts, or File › Options › Language)".into());
    }
    if let Some(dir) = b.packs_dir() {
        crate::file_access::check_write(&dir.to_string_lossy())?;
    }
    Ok(())
}

/// Download `family` and add its files to the font database; returns how many files were added.
#[cfg(not(target_arch = "wasm32"))]
fn install(family: &str) -> Result<usize, String> {
    use craft_font_store::{Caller, Cancel, FontStore, PINNED_INDEX};
    check(family)?;
    let b = crate::packs::backend().ok_or("there is no font store to download into")?;
    // The packs live in `<font store>/language-packs`; the fonts in the store itself.
    let dir = b.packs_dir().and_then(|d| d.parent().map(std::path::Path::to_path_buf)).ok_or("the font store has no folder")?;
    // An agent's store never turns downloading on: the person did, before this ran.
    let store = FontStore::new(dir, Caller::Agent);
    let cancel = Cancel::new();
    let index = match store.cached_indexes(&PINNED_INDEX) {
        Some(i) if i.family(family).is_some() => i,
        _ => store.fetch_indexes(&PINNED_INDEX, &cancel).map_err(|e| e.to_string())?,
    };
    let fam = index.family(family).ok_or_else(|| format!("the font store has no family named {family}"))?;
    let done = store.install_family(&index, fam, &cancel, &mut |_| {}).map_err(|e| e.to_string())?;
    let db = wordcraft_fonts::FontDb::global();
    let mut faces = 0usize;
    for file in done.files.iter().take(MAX_FILES) {
        let path = store.dir().join(&done.dir).join(&file.name);
        let bytes = crate::io::read_file_capped(&path.to_string_lossy(), wordcraft_fonts::MAX_FONT_FILE_BYTES as u64, "font")?;
        match db.install_font(&file.name, &bytes) {
            Ok(_) => faces = faces.saturating_add(1),
            // A face already available (another copy) is no failure.
            Err(e) => log::info!("font {}: {e}", file.name),
        }
    }
    if !installed(family) {
        return Err(format!("{family} was downloaded but its fonts couldn't be added"));
    }
    Ok(faces)
}

#[cfg(target_arch = "wasm32")]
fn install(_family: &str) -> Result<usize, String> {
    Err("downloading fonts isn't available in the browser".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_a_family_and_a_store() {
        let mut s = Session::new(wordcraft_doc::Document::new());
        for bad in [json!({}), json!({"family": ""}), json!({"family": "a\nb"}), json!({"family": "x".repeat(300)}), json!({"family": 3})] {
            assert!(s.run("font.download", &bad).is_err(), "{bad}");
        }
        // Tests have no font store: nothing is downloaded.
        assert!(s.run("font.download", &json!({"family": "Lexend Zetta Test"})).is_err());
    }

    /// A real download from the pinned sources into a temporary store (network: run by hand with
    /// `--ignored`).
    #[test]
    #[ignore]
    fn downloads_the_offered_fonts() {
        let dir = std::env::temp_dir().join(format!("strix-font-store-{}", std::process::id()));
        crate::packs::use_font_store_in(dir.clone(), true);
        crate::packs::backend().unwrap().enable_network().unwrap();
        for (family, _) in OFFERED {
            let n = install(family).unwrap();
            assert!(n > 0 && installed(family), "{family}");
        }
        // Chosen in Design › Fonts and saved as .docx with Embed Fonts in the File: Word gets them.
        let mut s = Session::new(wordcraft_doc::Document::from_text("Testo"));
        s.run("design.themeFonts", &json!({"heading": "Lexend", "body": "Atkinson Hyperlegible"})).unwrap();
        let r = s.run("file.embedFonts", &json!({"on": true})).unwrap();
        assert_eq!(r["fonts"].as_array().map(Vec::len), Some(2), "{r}");
        let bytes = crate::io::save_bytes("t.docx", &s.doc).unwrap();
        let mut names: Vec<String> = wordcraft_docx::embedded_fonts(&wordcraft_docx::read(&bytes).unwrap()).into_iter().map(|f| f.name).collect();
        names.dedup();
        assert_eq!(names, ["Atkinson Hyperlegible", "Lexend"]);
        std::fs::write(std::env::temp_dir().join("strix-embedded-fonts.docx"), &bytes).unwrap();
        crate::packs::set_backend(None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
