//! Spelling dictionaries on demand (`proof.downloadDictionary`): the Hunspell dictionary a
//! proofing language reads, downloaded from its publisher into the user's dictionaries folder
//! and read at once (docs/spelling-dictionaries.md). The download is `strix-dictionaries`; the
//! interface runs [`download`] off its thread, the command (CLI, MCP) waits for it.

use serde_json::{Value, json};

use crate::{CmdError, CmdResult, CommandSpec, Session};

pub use strix_dictionaries::Source;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("proof.downloadDictionary", "Download Dictionary", "Review › Language", run).params(
            r#"{"lang"?: "it-IT|pl-PL|es-MX|…" (BCP 47; default: the language at the caret, as the status bar shows it), "force"?: bool (download again even if one is installed)} → downloads the Hunspell dictionary that language's spelling reads (LibreOffice's dictionaries, at a fixed revision) into the user's dictionaries folder, and checks the text with it at once. Waits for the download (up to a minute). → {"dictionary": "it_IT", "language", "licence", "source": url, "file": path|null, "downloaded": bool (false: one was installed already)}"#,
        ),
    ]
}

/// The proofing language at the caret, as the status bar shows it: the typing formatting
/// resolved through the paragraph's style and the document defaults.
pub fn caret_lang(s: &Session) -> Option<String> {
    let style = s.doc.para_at(&s.sel.focus).and_then(|p| p.props.style.clone());
    s.doc.styles.resolve_char(style.as_deref(), &s.typing_props()).lang.or_else(|| super::default_lang(s))
}

/// What a download of the dictionary for `lang` would fetch, or why there is none.
pub fn source_for(lang: Option<&str>) -> Result<&'static strix_dictionaries::Source, CmdError> {
    strix_dictionaries::source_for(lang).ok_or_else(|| {
        let shown = lang.filter(|l| !l.is_empty()).unwrap_or("en-US");
        if wordcraft_proof::has_dictionary(lang) && strix_dictionaries::downloads::hunspell_name(lang).is_none() {
            CmdError::Failed(format!("{shown}: spelling needs no dictionary to download"))
        } else {
            CmdError::Failed(format!("{shown}: Strix has no spelling dictionary to download for this language"))
        }
    })
}

/// Download `src` into the user's dictionaries folder and read the dictionaries again. Blocks.
pub fn download(src: &strix_dictionaries::Source) -> Result<std::path::PathBuf, CmdError> {
    let dir = strix_dictionaries::downloads::download_dir().ok_or_else(|| CmdError::Failed("no folder to save dictionaries in".into()))?;
    let file = fetch(src, &dir).map_err(CmdError::Failed)?;
    strix_dictionaries::downloads::reload_dictionaries();
    Ok(file)
}

#[cfg(not(test))]
fn fetch(src: &strix_dictionaries::Source, dir: &std::path::Path) -> Result<std::path::PathBuf, String> {
    strix_dictionaries::download(src, dir)
}

/// Tests never reach the network (the command is fuzzed with every other one).
#[cfg(test)]
fn fetch(_: &strix_dictionaries::Source, _: &std::path::Path) -> Result<std::path::PathBuf, String> {
    Err("no downloads in tests".into())
}

fn run(s: &mut Session, v: &Value) -> CmdResult {
    let lang = match v.get("lang") {
        None | Some(Value::Null) => caret_lang(s),
        Some(Value::String(l)) => Some(l.clone()),
        Some(_) => return Err(CmdError::Params("lang: a language tag such as it-IT".into())),
    };
    let force = match v.get("force") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err(CmdError::Params("force: true or false".into())),
    };
    let src = source_for(lang.as_deref())?;
    let base = json!({
        "dictionary": src.name,
        "language": lang,
        "licence": src.licence,
        "source": src.page(),
    });
    let installed =
        wordcraft_proof::missing_dictionary(lang.as_deref()).is_none() && wordcraft_proof::unreadable_dictionary(lang.as_deref()).is_none();
    let (file, downloaded) = if installed && !force { (None, false) } else { (Some(download(src)?), true) };
    let mut out = base;
    out["file"] = json!(file.map(|f| f.display().to_string()));
    out["downloaded"] = json!(downloaded);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::Session;

    #[test]
    fn downloads_the_dictionary_of_the_language_asked_or_at_the_caret() {
        let mut s = Session::new(wordcraft_doc::Document::new());
        s.run("document.setText", &json!({"text": "Ciao"})).unwrap();
        // English needs none; a language Strix doesn't check has none.
        let e = s.run("proof.downloadDictionary", &json!({"lang": "en-US"})).unwrap_err().to_string();
        assert!(e.contains("needs no dictionary"), "{e}");
        let e = s.run("proof.downloadDictionary", &json!({"lang": "ja-JP"})).unwrap_err().to_string();
        assert!(e.contains("no spelling dictionary"), "{e}");
        // The caret's language (the status bar's): Italian here. Tests never download, so the
        // answer is the download's error unless it is installed already.
        std::sync::Arc::make_mut(&mut s.doc.styles).default_chr.lang = Some("it-IT".into());
        assert_eq!(super::caret_lang(&s).as_deref(), Some("it-IT"));
        assert_eq!(super::source_for(Some("it-IT")).map(|x| x.name).ok(), Some("it_IT"));
        match s.run("proof.downloadDictionary", &json!({})) {
            Ok(r) => assert_eq!((r["dictionary"].clone(), r["downloaded"].clone()), (json!("it_IT"), json!(false))),
            Err(e) => assert!(e.to_string().contains("no downloads in tests"), "{e}"),
        }
        for bad in [json!({"lang": 5}), json!({"force": "yes"}), json!({"lang": ["it"]})] {
            assert!(s.run("proof.downloadDictionary", &bad).is_err(), "{bad}");
        }
    }
}
