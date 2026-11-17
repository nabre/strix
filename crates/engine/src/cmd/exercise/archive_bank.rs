//! Metadata-only exercise bank. Bundles are inspected in memory, never extracted.
use crate::{CmdError, CmdResult, Session, p};
use serde_json::{Value, json};
use std::io::{Cursor, Read};

const MAX_FILES: usize = 1000;
const MAX_FILE: u64 = 32 * 1024 * 1024;
const MAX_BUNDLE: usize = 64 * 1024 * 1024;

/// The properties a free-text query looks in: what they say, never their names.
const SEARCHED: [&str; 9] = ["title", "subject", "tags", "teacher", "profession", "section", "grade", "duration", "score"];

fn matches(props: &Value, v: &Value) -> bool {
    let text = |x: &Value| x.as_str().map(str::to_string).unwrap_or_else(|| x.to_string()).trim().to_lowercase();
    let have = |k: &str| props.get(k).filter(|x| !x.is_null()).map(text);
    let query = p::str(v, "query").unwrap_or("").trim().to_lowercase();
    if !query.is_empty() && !SEARCHED.iter().any(|k| have(k).is_some_and(|t| t.contains(&query))) {
        return false;
    }
    ["subject", "grade", "tags", "section", "profession", "teacher"].iter().all(|k| {
        let Some(want) = v.get(*k).filter(|x| !x.is_null()).map(text).filter(|w| !w.is_empty()) else { return true };
        match have(k) {
            // A grade is a number: 1 is not found in 10 to 19.
            Some(t) if *k == "grade" => match (t.parse::<f64>(), want.parse::<f64>()) {
                (Ok(a), Ok(b)) => a == b,
                _ => t == want,
            },
            Some(t) => t.contains(&want),
            None => false,
        }
    })
}

fn properties<R: Read + std::io::Seek>(reader: R) -> Result<Value, String> {
    let doc = wordcraft_docx::read_archive_metadata(reader).map_err(|e| e.to_string())?;
    let title = doc.core.title.clone();
    let mut props = super::archive::state(&Session::new(doc));
    props["title"] = json!(title);
    Ok(props)
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn scan(_: &mut Session, v: &Value) -> CmdResult {
    let path = p::req_str(v, "path")?;
    // Through `io::list_dir`, which checks the automation read root for the folder and each entry.
    let (listing, truncated) = crate::io::list_dir(std::path::Path::new(path), MAX_FILES).map_err(CmdError::Failed)?;
    let mut files: Vec<_> =
        listing.into_iter().filter(|e| !e.dir && e.path.extension().is_some_and(|x| x.eq_ignore_ascii_case("docx"))).map(|e| e.path).collect();
    files.sort();
    let (mut entries, mut errors) = (Vec::new(), Vec::new());
    for path in files {
        let result = (|| {
            crate::file_access::check_read(&path.to_string_lossy())?;
            let (file, metadata) = crate::io::open_regular_file(&path)?;
            if metadata.len() > MAX_FILE {
                return Err("document exceeds archive size limit".into());
            }
            properties(file)
        })();
        match result {
            Ok(props) if matches(&props, v) => {
                entries.push(json!({"path": path, "name": path.file_name().map(|n| n.to_string_lossy()), "properties": props}))
            }
            Ok(_) => {}
            Err(error) => errors.push(json!({"path": path, "error": error})),
        }
    }
    Ok(json!({"entries": entries, "errors": errors, "truncated": truncated}))
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn scan(_: &mut Session, _: &Value) -> CmdResult {
    Err(CmdError::Disabled("Use archive.importBundle on the web".into()))
}

pub(crate) fn bundle(_: &mut Session, v: &Value) -> CmdResult {
    let encoded = p::req_str(v, "data")?;
    if encoded.len() > MAX_BUNDLE.saturating_mul(2) {
        return Err(CmdError::Params("bundle too large".into()));
    }
    let bytes = crate::cmd::insert::base64_decode(encoded)
        .filter(|b| b.len() <= MAX_BUNDLE)
        .ok_or_else(|| CmdError::Params("invalid or oversized bundle".into()))?;
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| CmdError::Params(e.to_string()))?;
    let (mut entries, mut errors, mut total) = (Vec::new(), Vec::new(), 0usize);
    for i in 0..zip.len().min(MAX_FILES) {
        let file = zip.by_index(i).map_err(|e| CmdError::Params(e.to_string()))?;
        let name = file.name().map_err(|e| CmdError::Params(e.to_string()))?.into_owned();
        if file.is_dir() || !name.to_ascii_lowercase().ends_with(".docx") {
            continue;
        }
        let result = (|| -> Result<Vec<u8>, String> {
            if file.size() > MAX_FILE {
                return Err("file too large".into());
            }
            let mut bytes = Vec::new();
            file.take(MAX_FILE + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            total = total.saturating_add(bytes.len());
            if bytes.len() as u64 > MAX_FILE || total > MAX_BUNDLE {
                return Err("bundle expansion limit exceeded".into());
            }
            Ok(bytes)
        })();
        match result.and_then(|bytes| properties(Cursor::new(&bytes)).map(|props| (bytes, props))) {
            Ok((bytes, props)) if matches(&props, v) => {
                entries.push(json!({"name": name, "path": name, "data": crate::cmd::insert::base64_encode(&bytes), "properties": props}))
            }
            Ok(_) => {}
            Err(error) => errors.push(json!({"name": name, "error": error})),
        }
        if total > MAX_BUNDLE {
            break;
        }
    }
    Ok(json!({"entries": entries, "errors": errors, "truncated": zip.len() > MAX_FILES || total > MAX_BUNDLE}))
}

pub(crate) fn insert(s: &mut Session, v: &Value) -> CmdResult {
    let path = p::req_str(v, "path")?;
    let doc = if let Some(encoded) = p::str(v, "data") {
        if encoded.len() > (MAX_FILE as usize).saturating_mul(2) {
            return Err(CmdError::Params("file too large".into()));
        }
        let bytes = crate::cmd::insert::base64_decode(encoded).ok_or_else(|| CmdError::Params("invalid base64".into()))?;
        crate::io::open_bytes(path, &bytes).map_err(CmdError::Failed)?
    } else {
        crate::io::open_path(std::path::Path::new(path)).map_err(CmdError::Failed)?
    };
    let mut frag = crate::cmd::paste::adopt(s, doc);
    crate::cmd::paste::unlink_sections(&mut frag.blocks);
    // Never inside another exercise: right after it.
    super::outside_for(s, &frag.blocks);
    let at = crate::cmd::delete_selection(s)?;
    let end = s.doc.insert_fragment(&at, &frag)?;
    s.sel = crate::Selection::caret(end);
    Ok(json!({"inserted": true}))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    /// A grade is a number: grade 1 is not found in grades 10 to 19, nor 2 in 12 and 20.
    #[test]
    fn a_grade_filter_matches_that_grade_only() {
        let props = |g: u64| json!({"grade": g, "subject": "Fisica", "section": "B", "tags": "misure, laboratorio"});
        assert!(super::matches(&props(1), &json!({"grade": 1})));
        assert!(super::matches(&props(1), &json!({"grade": "1"})));
        assert!(super::matches(&props(12), &json!({"grade": " 12 "})));
        for g in [10, 11, 12, 19] {
            assert!(!super::matches(&props(g), &json!({"grade": 1})), "{g}");
        }
        assert!(!super::matches(&props(12), &json!({"grade": 2})));
        assert!(!super::matches(&json!({"grade": null}), &json!({"grade": 2})));
        // Text properties are still searched within, case aside.
        assert!(super::matches(&props(3), &json!({"subject": "fis", "tags": "MISURE"})));
        assert!(!super::matches(&props(3), &json!({"subject": "chimica"})));
    }

    /// The free-text query looks in what the properties say, not in their names: "sezione" or
    /// "grade" doesn't find every file.
    #[test]
    fn a_query_searches_the_values_not_the_property_names() {
        let mut s = crate::Session::new(wordcraft_doc::Document::new());
        s.run("review.language", &json!({"lang": "it-CH", "default": true})).unwrap();
        s.run("archive.properties", &json!({"subject": "Fisica", "tags": "misure", "grade": 3, "section": "B", "teacher": "M. Rossi"})).unwrap();
        let mut props = super::super::archive::state(&s);
        props["title"] = json!("Laboratorio");
        for q in ["fisica", "MISURE", "rossi", "laboratorio", "3"] {
            assert!(super::matches(&props, &json!({"query": q})), "{q}");
        }
        for q in ["sezione", "classe", "punteggio", "grade", "names", "null", "subject", "chimica"] {
            assert!(!super::matches(&props, &json!({"query": q})), "{q}");
        }
    }
}
