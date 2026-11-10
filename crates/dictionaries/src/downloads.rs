//! Where the dictionaries Strix downloads go, and how proofing finds them (File › Options ›
//! Proofing, the status bar's language). Proofing is `craft-proof` (storytold/craft-libs), which
//! searches one dictionaries folder before the system's: the person's own folder when they set one
//! (File › Options › Proofing › Dictionaries folder), else Strix's `dictionaries` folder. Downloads
//! go to that same folder, so a downloaded dictionary is read at once.

use std::path::PathBuf;
use std::sync::{PoisonError, RwLock};

use wordcraft_proof::ProofLang;

/// The folder proofing searches and downloads go to now ([`use_folder`]); `None` until it is set.
static FOLDER: RwLock<Option<PathBuf>> = RwLock::new(None);

/// The Hunspell dictionary (`it_IT`, `es_MX`) the proofing language of `lang` (a BCP 47 tag; none:
/// the default, English) reads, whether installed or not. `None` for languages that use none
/// (English has its own word list) or that Strix can't download one for.
pub fn hunspell_name(lang: Option<&str>) -> Option<&'static str> {
    match ProofLang::from_tag(lang) {
        ProofLang::Pl => Some("pl_PL"),
        ProofLang::Pt => Some("pt_BR"),
        ProofLang::It => Some("it_IT"),
        ProofLang::Hr => Some("hr_HR"),
        ProofLang::Tr => Some("tr_TR"),
        ProofLang::Ru => Some("ru_RU"),
        ProofLang::Cs => Some("cs_CZ"),
        ProofLang::Id => Some("id_ID"),
        ProofLang::Es => Some("es_ES"),
        ProofLang::EsMx => Some("es_MX"),
        ProofLang::Es419 => Some("es_419"),
        ProofLang::EsUy => Some("es_UY"),
        _ => None,
    }
}

/// Strix's own folder for downloaded dictionaries: `dictionaries` in the portable data folder of
/// a portable run, else in the user's application data — `%LOCALAPPDATA%\Strix` on Windows,
/// `~/Library/Application Support/Strix` on macOS, `$XDG_DATA_HOME/strix` (or
/// `~/.local/share/strix`) elsewhere. `None` on the web or without a home folder.
pub fn user_dictionaries_dir() -> Option<PathBuf> {
    if cfg!(target_arch = "wasm32") {
        return None;
    }
    if let Some(d) = wordcraft_control_key::portable_dir() {
        return Some(d.join("dictionaries"));
    }
    let var = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let base = if cfg!(windows) {
        var("LOCALAPPDATA").or_else(|| var("APPDATA"))?.join("Strix")
    } else if cfg!(target_os = "macos") {
        var("HOME")?.join("Library/Application Support/Strix")
    } else {
        var("XDG_DATA_HOME").filter(|p| p.is_absolute()).or_else(|| var("HOME").map(|h| h.join(".local/share")))?.join("strix")
    };
    Some(base.join("dictionaries"))
}

/// The folder proofing searches first: the person's own `folder` (an empty one counts as unset),
/// else Strix's ([`user_dictionaries_dir`]).
pub fn search_folder(folder: Option<&str>) -> Option<PathBuf> {
    folder.filter(|f| !f.is_empty()).map(PathBuf::from).or_else(user_dictionaries_dir)
}

/// Make proofing search [`search_folder`]`(folder)` from now on (the saved setting, applied when
/// the app or the CLI starts and when it changes), and read every dictionary again.
pub fn use_folder(folder: Option<&str>) {
    let dir = search_folder(folder);
    *FOLDER.write().unwrap_or_else(PoisonError::into_inner) = dir.clone();
    wordcraft_proof::set_dictionaries_folder(dir);
}

/// The folder a download goes to: the one proofing searches ([`use_folder`]), or Strix's when
/// none was set yet.
pub fn download_dir() -> Option<PathBuf> {
    FOLDER.read().unwrap_or_else(PoisonError::into_inner).clone().or_else(user_dictionaries_dir)
}

/// Read every language's dictionary again (one was just downloaded), and check the text again.
pub fn reload_dictionaries() {
    let dir = download_dir();
    wordcraft_proof::set_dictionaries_folder(dir);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_hunspell_language_names_its_dictionary() {
        for (tag, name) in [
            ("pl-PL", "pl_PL"),
            ("pt-BR", "pt_BR"),
            ("pt-PT", "pt_BR"),
            ("it-IT", "it_IT"),
            ("hr-HR", "hr_HR"),
            ("tr-TR", "tr_TR"),
            ("ru-RU", "ru_RU"),
            ("cs-CZ", "cs_CZ"),
            ("id-ID", "id_ID"),
            ("es-ES", "es_ES"),
            ("es-MX", "es_MX"),
            ("es-AR", "es_419"),
            ("es-UY", "es_UY"),
        ] {
            assert_eq!(hunspell_name(Some(tag)), Some(name), "{tag}");
        }
        for tag in [None, Some("en-US"), Some("en-GB"), Some("bg-BG"), Some("ja-JP"), Some(""), Some("x\u{0}")] {
            assert_eq!(hunspell_name(tag), None, "{tag:?}");
        }
    }

    #[test]
    fn downloads_go_to_a_folder_of_their_own() {
        // The portable folder or the user's data folder: always a `dictionaries` folder.
        if let Some(d) = user_dictionaries_dir() {
            assert!(d.ends_with("dictionaries"), "{d:?}");
        }
    }

    #[test]
    fn the_persons_folder_comes_before_strixs() {
        assert_eq!(search_folder(Some("/somewhere/dicts")), Some(PathBuf::from("/somewhere/dicts")));
        assert_eq!(search_folder(Some("")), user_dictionaries_dir());
        assert_eq!(search_folder(None), user_dictionaries_dir());
    }
}
