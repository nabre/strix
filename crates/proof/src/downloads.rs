//! Dictionaries Strix downloads for the user (File › Options › Proofing, the status bar's
//! language): the Hunspell name each proofing language needs, the folder downloads go to (searched
//! with the others, see [`crate::hunspell::search_dirs`]) and loading them without a restart.
//! The download itself is `strix-dictionaries`; this crate only reads files.

use std::path::PathBuf;

use crate::lang::ProofLang;

/// The Hunspell dictionary (`it_IT`, `es_MX`) the proofing language of `lang` (a BCP 47 tag; none:
/// the default, English) reads, whether installed or not. `None` for languages that use none
/// (English has its own word list) or that Strix doesn't check.
pub fn hunspell_name(lang: Option<&str>) -> Option<&'static str> {
    crate::dictionary_needed(ProofLang::from_tag(lang), false)
}

/// The folder dictionaries downloaded by Strix go to: `dictionaries` in the portable data folder
/// of a portable run, else in the user's application data — `%LOCALAPPDATA%\Strix` on Windows,
/// `~/Library/Application Support/Strix` on macOS, `$XDG_DATA_HOME/strix` (or
/// `~/.local/share/strix`) elsewhere. `None` on the web or without a home folder.
pub fn user_dictionaries_dir() -> Option<PathBuf> {
    if cfg!(target_arch = "wasm32") {
        return None;
    }
    if let Some(d) = strix_control_key::portable_dir() {
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

/// Read every language's dictionary again (one was just downloaded or removed), and check the
/// text again.
pub fn reload_dictionaries() {
    crate::pl::reload_dictionary();
    crate::pt::reload_dictionary();
    crate::it::reload_dictionary();
    crate::hr::reload_dictionary();
    crate::tr::reload_dictionary();
    crate::ru::reload_dictionary();
    crate::cs::reload_dictionary();
    crate::id::reload_dictionary();
    crate::es::reload_dictionary();
    crate::DICTIONARY_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
}
