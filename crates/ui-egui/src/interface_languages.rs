//! The languages Strix offers its interface in: English (the source language) and Italian,
//! German and French (docs/localization-parity.md). The other catalogs WordCraft and craft-i18n
//! bring stay embedded, so upstream merges and their tests are untouched, but Strix doesn't offer
//! them: the Options › Language picker lists only these, the system language picks one of these
//! (else English), and the missing-translation report covers only these. `ui.language` still
//! accepts any embedded code, for scripts and tests.

use crate::i18n::{Lang, lang_from_tag};

/// The interface languages Strix offers, by craft-i18n code.
pub const OFFERED: [&str; 4] = ["en", "it", "de", "fr"];

/// Whether Strix offers `lang` as its interface language.
pub fn offered(lang: Lang) -> bool {
    OFFERED.contains(&lang.code())
}

/// The Options › Language picker's languages: English first, then the others by their own name.
pub fn picker() -> Vec<Lang> {
    Lang::for_picker().into_iter().filter(|l| offered(*l)).collect()
}

/// The first of the system's preferred languages (BCP 47 tags, in the user's order) that Strix
/// offers: someone who prefers Spanish, then Italian, gets Italian.
pub fn first_offered<'a>(tags: impl IntoIterator<Item = &'a str>) -> Option<Lang> {
    tags.into_iter().find_map(|t| lang_from_tag(t).filter(|l| offered(*l)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picker_lists_english_italian_german_and_french() {
        let codes: Vec<&str> = picker().into_iter().map(Lang::code).collect();
        assert_eq!(codes, ["en", "de", "fr", "it"]);
    }

    #[test]
    fn the_system_language_is_one_strix_offers() {
        let code = |tags: &[&str]| first_offered(tags.iter().copied()).map(Lang::code);
        assert_eq!(code(&["es-ES", "it-CH", "en-US"]), Some("it"));
        assert_eq!(code(&["de-CH"]), Some("de"));
        assert_eq!(code(&["fr-CA"]), Some("fr"));
        assert_eq!(code(&["pl-PL", "ja-JP"]), None);
        assert_eq!(code(&["ru-RU", "en-GB"]), Some("en"));
    }
}
