//! Thesauri on demand: `review.thesaurus` reads the downloaded thesaurus of the word's language
//! (the language of the run at the caret, else the document's), with WordCraft's small built-in
//! list as the fallback; `proof.downloadThesaurus` downloads that thesaurus (LibreOffice's
//! dictionaries, at a fixed revision) with its licence notices. The reading and the download are
//! `strix-dictionaries` (`mythes`, `thesauri`); docs/thesaurus.md.

use serde_json::{Value, json};

use crate::{CmdError, CmdResult, CommandSpec, Session};

pub use strix_dictionaries::Source;

/// `review.thesaurus`'s parameters and answer.
pub const PARAMS: &str = r#"{"word"?: string (default: the word at the caret), "lang"?: "it-IT|en-US|…" (BCP 47; default: the language of the run at the caret, else the document's)} → {"word", "synonyms": [string] (every meaning's, in order, without repeats), "meanings": [{"category": string|null ("noun", "s.f."…), "synonyms": [string]}], "entry": string|null (the form the thesaurus lists: "casa" for "case"), "language", "thesaurus": "it_IT"|null (the downloaded thesaurus that answered; null: the built-in list, or no synonyms), "download": "it_IT"|null (a thesaurus for this language that proof.downloadThesaurus can download)}"#;

/// Most synonyms in the answer's flat list.
const MAX_LISTED: usize = 80;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("proof.downloadThesaurus", "Download Thesaurus", "Review › Proofing", download_command).params(
            r#"{"lang"?: "it-IT|es-ES|en-US|…" (BCP 47; default: the language at the caret, as the status bar shows it), "force"?: bool (download again even if one is installed)} → downloads the thesaurus of that language (LibreOffice's dictionaries, at a fixed revision; MyThes format) and the files stating its licence into the user's dictionaries folder; review.thesaurus reads it at once. Waits for the download (up to a minute). → {"thesaurus": "it_IT", "language", "licence", "source": url, "file": path, "downloaded": bool (false: one was installed already)}"#,
        ),
    ]
}

/// The language a lookup reads: `lang` when given, else the caret's (the run's language, else the
/// document's).
fn lookup_lang(s: &Session, v: &Value) -> Option<String> {
    v.get("lang").and_then(Value::as_str).map(str::to_string).or_else(|| super::dictionaries::caret_lang(s))
}

/// A synonym as it is inserted: without the notes some thesauri put around it ("(generic term)",
/// "(colloquially)"), unless that leaves nothing.
fn plain(synonym: &str) -> String {
    let mut t = synonym.trim();
    loop {
        let before = t;
        if t.starts_with('(')
            && let Some(close) = t.find(')')
        {
            t = t.get(close.saturating_add(1)..).unwrap_or_default().trim_start();
        }
        if t.ends_with(')')
            && let Some(open) = t.rfind('(')
        {
            t = t.get(..open).unwrap_or_default().trim_end();
        }
        if t == before {
            break;
        }
    }
    if t.is_empty() { synonym.trim().to_string() } else { t.to_string() }
}

/// `review.thesaurus`'s answer for `word` (lower case): the downloaded thesaurus of its language
/// when it lists the word, else the built-in list's `builtin` synonyms.
pub(crate) fn answer(s: &Session, v: &Value, word: String, builtin: Vec<&str>) -> CmdResult {
    if v.get("lang").is_some_and(|l| !l.is_string() && !l.is_null()) {
        return Err(CmdError::Params("lang: a language tag such as it-IT".into()));
    }
    let lang = lookup_lang(s, v);
    let src = strix_dictionaries::thesauri::source_for(lang.as_deref());
    let read = src.map(read);
    let found = match &read {
        Some(Ok(Some(t))) => t.lookup(&word, lang.as_deref()),
        _ => None,
    };
    let download = match &read {
        Some(Ok(None)) => src.map(|x| x.name),
        _ => None,
    };
    let mut out = json!({"word": word, "language": lang, "download": download});
    if let Some(Err(e)) = &read {
        out["thesaurusError"] = json!(e);
    }
    let (entry, meanings) = match found {
        Some((entry, m)) => {
            let same = |x: &str| x.to_lowercase() == word || x.to_lowercase() == entry;
            let meanings: Vec<(Option<String>, Vec<String>)> = m
                .into_iter()
                .map(|m| {
                    let mut syn: Vec<String> = Vec::new();
                    for x in m.synonyms.iter().map(|x| plain(x)) {
                        if !same(&x) && !syn.contains(&x) {
                            syn.push(x);
                        }
                    }
                    (m.category, syn)
                })
                .filter(|(_, syn)| !syn.is_empty())
                .collect();
            (Some(entry), meanings)
        }
        None if builtin.is_empty() => (None, Vec::new()),
        None => (None, vec![(None, builtin.iter().map(|x| x.to_string()).collect())]),
    };
    let mut flat: Vec<&str> = Vec::new();
    for x in meanings.iter().flat_map(|(_, syn)| syn) {
        if flat.len() < MAX_LISTED && !flat.contains(&x.as_str()) {
            flat.push(x);
        }
    }
    out["synonyms"] = json!(flat);
    out["meanings"] = meanings.iter().map(|(c, syn)| json!({"category": c, "synonyms": syn})).collect();
    let answered = entry.is_some() && !meanings.is_empty();
    out["entry"] = json!(entry.filter(|_| answered));
    out["thesaurus"] = json!(src.map(|x| x.name).filter(|_| answered));
    Ok(out)
}

/// The installed thesaurus `src`, read.
#[cfg(not(test))]
fn read(src: &Source) -> Result<Option<std::sync::Arc<strix_dictionaries::mythes::Thesaurus>>, String> {
    strix_dictionaries::thesauri::get(src)
}

/// Tests read a small Italian thesaurus of our own, and no other (never this machine's).
#[cfg(test)]
fn read(src: &Source) -> Result<Option<std::sync::Arc<strix_dictionaries::mythes::Thesaurus>>, String> {
    if src.name == "it_IT" { strix_dictionaries::mythes::parse(tests::ITALIAN.as_bytes()).map(|t| Some(std::sync::Arc::new(t))) } else { Ok(None) }
}

/// The thesaurus a download for `lang` would fetch, or why there is none.
pub fn source_for(lang: Option<&str>) -> Result<&'static Source, CmdError> {
    strix_dictionaries::thesauri::source_for(lang).ok_or_else(|| {
        let shown = lang.filter(|l| !l.is_empty()).unwrap_or("en-US");
        CmdError::Failed(format!("{shown}: Strix has no thesaurus to download for this language"))
    })
}

/// The thesaurus `lang` reads when one can be downloaded and none is installed yet.
pub fn missing(lang: Option<&str>) -> Option<&'static Source> {
    strix_dictionaries::thesauri::source_for(lang).filter(|src| strix_dictionaries::thesauri::installed(src).is_none())
}

/// Download `src` into the user's dictionaries folder; the next lookup reads it. Blocks.
pub fn download(src: &Source) -> Result<std::path::PathBuf, CmdError> {
    let dir = strix_dictionaries::downloads::download_dir().ok_or_else(|| CmdError::Failed("no folder to save thesauri in".into()))?;
    let file = fetch(src, &dir).map_err(CmdError::Failed)?;
    strix_dictionaries::thesauri::forget();
    Ok(file)
}

#[cfg(not(test))]
fn fetch(src: &Source, dir: &std::path::Path) -> Result<std::path::PathBuf, String> {
    strix_dictionaries::download(src, dir)
}

/// Tests never reach the network (the command is fuzzed with every other one).
#[cfg(test)]
fn fetch(_: &Source, _: &std::path::Path) -> Result<std::path::PathBuf, String> {
    Err("no downloads in tests".into())
}

fn download_command(s: &mut Session, v: &Value) -> CmdResult {
    let lang = match v.get("lang") {
        None | Some(Value::Null) => super::dictionaries::caret_lang(s),
        Some(Value::String(l)) => Some(l.clone()),
        Some(_) => return Err(CmdError::Params("lang: a language tag such as it-IT".into())),
    };
    let force = match v.get("force") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err(CmdError::Params("force: true or false".into())),
    };
    let src = source_for(lang.as_deref())?;
    let installed = strix_dictionaries::thesauri::installed(src);
    let (file, downloaded) = match installed {
        Some(f) if !force => (f, false),
        _ => (download(src)?, true),
    };
    Ok(json!({
        "thesaurus": src.name,
        "language": lang,
        "licence": src.licence,
        "source": src.page(),
        "file": file.display().to_string(),
        "downloaded": downloaded,
    }))
}

#[cfg(test)]
#[path = "thesaurus_tests.rs"]
mod tests;
