//! Which thesaurus a language reads, where an installed one is found and the thesauri read so far.
//! A downloaded thesaurus is in the user's dictionaries folder ([`crate::downloads::download_dir`]);
//! one installed with LibreOffice or the system's packages (`/usr/share/mythes`, LibreOffice's
//! `dict-xx` extension folders) is read too. A thesaurus is read once and kept while its file
//! doesn't change; one that can't be read is remembered as such, so it isn't read again at every
//! lookup.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::SystemTime;

use crate::mythes::{self, Thesaurus};
use crate::{Kind, Source};

/// The thesaurus (`it_IT`, `es`, `en_US`) the language `lang` (a BCP 47 tag; none: the default,
/// English) reads, whether installed or not. `None` for a language Strix has no thesaurus for.
pub fn thesaurus_name(lang: Option<&str>) -> Option<&'static str> {
    let tag = lang.map(str::trim).filter(|l| !l.is_empty()).unwrap_or("en-US").to_ascii_lowercase();
    let mut parts = tag.split(['-', '_']);
    let primary = parts.next().unwrap_or_default();
    let region = parts.find(|p| p.len() == 2 || p.len() == 3 && p.bytes().all(|b| b.is_ascii_digit()));
    match (primary, region) {
        ("en", _) => Some("en_US"),
        ("it", _) => Some("it_IT"),
        ("es", _) => Some("es"),
        // Brazilian Portuguese has none Strix can download (see `SOURCES`).
        ("pt", Some("pt")) => Some("pt_PT"),
        ("pl", _) => Some("pl_PL"),
        ("ru", _) => Some("ru_RU"),
        ("id", _) => Some("id_ID"),
        ("cs", _) => Some("cs_CZ"),
        _ => None,
    }
}

/// The thesaurus the language `lang` reads, if Strix can download one.
pub fn source_for(lang: Option<&str>) -> Option<&'static Source> {
    thesaurus_name(lang).and_then(crate::thesaurus)
}

/// The folders searched for an installed thesaurus `src`, the user's dictionaries folder first.
fn search_dirs(src: &Source) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = crate::downloads::download_dir().into_iter().collect();
    if cfg!(target_arch = "wasm32") {
        return dirs;
    }
    let primary = src.name.split('_').next().unwrap_or(src.name);
    dirs.extend(wordcraft_proof::hunspell::search_dirs(Some(&[format!("dict-{primary}")])));
    if !cfg!(windows) {
        dirs.push(PathBuf::from("/usr/share/mythes"));
        dirs.push(PathBuf::from("/usr/local/share/mythes"));
    }
    dirs.dedup();
    dirs
}

/// The installed file of the thesaurus `src`, if there is one.
pub fn installed(src: &Source) -> Option<PathBuf> {
    if src.kind != Kind::Thesaurus {
        return None;
    }
    let name = src.file_name();
    search_dirs(src).into_iter().map(|d| d.join(&name)).find(|p| p.is_file())
}

/// A thesaurus read (or that couldn't be): its file, the file's size and time when read.
struct Read {
    path: PathBuf,
    len: u64,
    modified: Option<SystemTime>,
    result: Result<Arc<Thesaurus>, String>,
}

/// The thesauri read so far (one per language at most; a handful in practice).
static READ: Mutex<Vec<Read>> = Mutex::new(Vec::new());
/// Most thesauri kept in memory at once (the oldest read is dropped).
const KEEP: usize = 4;

fn stamp(path: &Path) -> Option<(u64, Option<SystemTime>)> {
    let m = std::fs::metadata(path).ok()?;
    Some((m.len(), m.modified().ok()))
}

/// The installed thesaurus `src`, read (or taken from those read already). `Ok(None)`: it isn't
/// installed. `Err`: it is, but couldn't be read (why).
pub fn get(src: &Source) -> Result<Option<Arc<Thesaurus>>, String> {
    let Some(path) = installed(src) else { return Ok(None) };
    let Some((len, modified)) = stamp(&path) else { return Ok(None) };
    let mut read = READ.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(r) = read.iter().find(|r| r.path == path && r.len == len && r.modified == modified) {
        return r.result.clone().map(Some);
    }
    // Read outside the lock would let two lookups read the same file twice; a thesaurus reads in
    // well under a second, so lookups wait for it instead.
    let result = mythes::load(&path, mythes::MAX_FILE).map(Arc::new);
    read.retain(|r| r.path != path);
    if read.len() >= KEEP {
        read.remove(0);
    }
    read.push(Read { path, len, modified, result: result.clone() });
    result.map(Some)
}

/// Forget the thesauri read so far (one was just downloaded): the next lookup reads them again.
pub fn forget() {
    READ.lock().unwrap_or_else(PoisonError::into_inner).clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_with_a_thesaurus_names_it() {
        for (tag, name) in [
            (None, "en_US"),
            (Some("en-GB"), "en_US"),
            (Some("it-IT"), "it_IT"),
            (Some("it-CH"), "it_IT"),
            (Some("es-MX"), "es"),
            (Some("es-419"), "es"),
            (Some("pt-PT"), "pt_PT"),
            (Some("pl-PL"), "pl_PL"),
            (Some("ru-RU"), "ru_RU"),
            (Some("id-ID"), "id_ID"),
            (Some("cs-CZ"), "cs_CZ"),
            (Some("IT_it"), "it_IT"),
        ] {
            assert_eq!(thesaurus_name(tag), Some(name), "{tag:?}");
            assert!(source_for(tag).is_some_and(|s| s.kind == Kind::Thesaurus && s.name == name), "{tag:?}");
        }
        for tag in ["pt-BR", "pt", "de-DE", "ja-JP", "x\u{0}", "-", "--"] {
            assert_eq!(source_for(Some(tag)), None, "{tag}");
        }
    }
}
