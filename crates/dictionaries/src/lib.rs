//! Spelling dictionaries on demand. Strix doesn't ship the Hunspell dictionaries its proofing
//! languages read (they are large, and several are GPL or LGPL): this crate knows where each one
//! is published — the LibreOffice dictionaries repository, at a fixed revision — and downloads
//! the `.aff`/`.dic` pair into the user's dictionaries folder
//! ([`wordcraft_proof::downloads::user_dictionaries_dir`]), which proofing searches. The user's
//! computer fetches the files from their publisher; Strix doesn't redistribute them.
//!
//! Only the dictionaries listed in [`SOURCES`] can be fetched (never a URL from a caller), each
//! file is size-capped and checked to be a Hunspell file, and a pair is written whole or not at
//! all. On the web there is nothing to download into.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::path::{Path, PathBuf};

/// The repository the dictionaries come from, and the revision fetched (fixed, so what a
/// language downloads doesn't change under it).
pub const REPOSITORY: &str = "https://github.com/LibreOffice/dictionaries";
pub const REVISION: &str = "c0865a85eb67dc377f2cf8e8fe2c4fa9e53f0dcb";

/// Largest `.aff` or `.dic` downloaded: proofing reads none larger (Turkish, the largest, is ~36 MB).
pub const MAX_FILE: usize = 64 << 20;

/// A dictionary Strix can download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Source {
    /// The Hunspell name a proofing language asks for (`it_IT`; see
    /// [`wordcraft_proof::downloads::hunspell_name`]).
    pub name: &'static str,
    /// Its folder in the repository and its file stem there, which is also the name it is saved
    /// under (Latin American Spanish has no dictionary of its own: it reads the Mexican one,
    /// which it also looks for).
    pub folder: &'static str,
    pub file: &'static str,
    /// The licence(s) the publisher gives it under.
    pub licence: &'static str,
}

/// Every dictionary a proofing language reads, as the publisher licenses it (from each folder's
/// README or LICENSE at [`REVISION`]).
pub const SOURCES: &[Source] = &[
    Source { name: "pl_PL", folder: "pl_PL", file: "pl_PL", licence: "GPL-2.0, LGPL-2.1, MPL-1.1, Apache-2.0 or CC-BY-4.0" },
    Source { name: "pt_BR", folder: "pt_BR", file: "pt_BR", licence: "LGPL-3.0 or MPL" },
    Source { name: "it_IT", folder: "it_IT", file: "it_IT", licence: "GPL-3.0" },
    Source { name: "hr_HR", folder: "hr_HR", file: "hr_HR", licence: "GPL-2.0, LGPL-2.1 or MPL-1.1" },
    Source { name: "tr_TR", folder: "tr_TR", file: "tr_TR", licence: "MPL-2.0" },
    Source { name: "ru_RU", folder: "ru_RU", file: "ru_RU", licence: "BSD-style" },
    Source { name: "cs_CZ", folder: "cs_CZ", file: "cs_CZ", licence: "GPL-2.0" },
    Source { name: "id_ID", folder: "id", file: "id_ID", licence: "LGPL-3.0" },
    Source { name: "es_ES", folder: "es", file: "es_ES", licence: "GPL-3.0+, LGPL-3.0+ or MPL-1.1+" },
    Source { name: "es_MX", folder: "es", file: "es_MX", licence: "GPL-3.0+, LGPL-3.0+ or MPL-1.1+" },
    Source { name: "es_419", folder: "es", file: "es_MX", licence: "GPL-3.0+, LGPL-3.0+ or MPL-1.1+" },
    Source { name: "es_UY", folder: "es", file: "es_UY", licence: "GPL-3.0+, LGPL-3.0+ or MPL-1.1+" },
];

/// The dictionary named `name` (`it_IT`), if Strix can download it.
pub fn source(name: &str) -> Option<&'static Source> {
    SOURCES.iter().find(|s| s.name == name)
}

/// The dictionary the proofing language of `lang` (a BCP 47 tag) reads, if Strix can download it.
pub fn source_for(lang: Option<&str>) -> Option<&'static Source> {
    wordcraft_proof::downloads::hunspell_name(lang).and_then(source)
}

impl Source {
    /// The web page of the dictionary's folder (where its licence and authors are).
    pub fn page(&self) -> String {
        format!("{REPOSITORY}/tree/{REVISION}/{}", self.folder)
    }
}

/// The address files are fetched from.
fn base_url() -> String {
    format!("https://raw.githubusercontent.com/LibreOffice/dictionaries/{REVISION}/")
}

/// Download `src` into `dir` (created if needed) and return the `.dic` written. Proofing reads it
/// once its dictionaries are loaded again ([`wordcraft_proof::downloads::reload_dictionaries`]).
/// Blocks while it downloads (seconds to a minute): call it off the interface's thread.
pub fn download(src: &Source, dir: &Path) -> Result<PathBuf, String> {
    download_from(&base_url(), src, dir, MAX_FILE)
}

/// [`download`] from `base` (tests serve the files locally), with files of at most `max` bytes.
fn download_from(base: &str, src: &Source, dir: &Path, max: usize) -> Result<PathBuf, String> {
    let fetch = |ext: &str| -> Result<Vec<u8>, String> {
        let url = format!("{base}{}/{}.{ext}", src.folder, src.file);
        let bytes = net::get(&url, max)?;
        check(ext, &bytes).map_err(|e| format!("{}.{ext}: {e}", src.file))?;
        Ok(bytes)
    };
    let aff = fetch("aff")?;
    let dic = fetch("dic")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    // Both files land, or neither: written aside, then renamed into place.
    let path = |ext: &str| dir.join(format!("{}.{ext}", src.file));
    let part = |ext: &str| dir.join(format!(".{}.{ext}.part", src.file));
    let write = |ext: &str, bytes: &[u8]| std::fs::write(part(ext), bytes).map_err(|e| format!("{}: {e}", part(ext).display()));
    let result = write("aff", &aff).and_then(|()| write("dic", &dic)).and_then(|()| {
        for ext in ["aff", "dic"] {
            std::fs::rename(part(ext), path(ext)).map_err(|e| format!("{}: {e}", path(ext).display()))?;
        }
        Ok(())
    });
    for ext in ["aff", "dic"] {
        let _ = std::fs::remove_file(part(ext));
    }
    result?;
    // Where it came from and under which licence, beside it.
    let note = format!(
        "{}.aff and {}.dic: Hunspell dictionary downloaded by Strix from\n{}\nLicence: {}\nSee that folder's README or LICENSE for its authors and the licence text.\n",
        src.file,
        src.file,
        src.page(),
        src.licence
    );
    let _ = std::fs::write(dir.join(format!("{}.source.txt", src.file)), note);
    Ok(path("dic"))
}

/// Is this a Hunspell file, not an error page? An `.aff` is text; a `.dic` starts with its word
/// count.
fn check(ext: &str, bytes: &[u8]) -> Result<(), String> {
    let head = String::from_utf8_lossy(bytes.get(..bytes.len().min(512)).unwrap_or_default()).to_string();
    let first = head.trim_start_matches('\u{feff}').lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default().to_string();
    if first.is_empty() {
        return Err("empty".into());
    }
    if first.starts_with('<') {
        return Err("not a dictionary (a web page)".into());
    }
    if ext == "dic" && !first.split_whitespace().next().is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())) {
        return Err("not a Hunspell word list (no word count)".into());
    }
    if bytes.contains(&0) {
        return Err("not a text file".into());
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
mod net {
    use std::io::Read;
    use std::time::Duration;

    /// GET `url`: its body, at most `max` bytes once decoded.
    pub fn get(url: &str, max: usize) -> Result<Vec<u8>, String> {
        let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .tls_config(tls)
            .timeout_connect(Some(Duration::from_secs(20)))
            .timeout_global(Some(Duration::from_secs(600)))
            .http_status_as_error(false)
            .user_agent(concat!("Strix/", env!("CARGO_PKG_VERSION")))
            .build()
            .new_agent();
        let mut resp = agent.get(url).call().map_err(|e| format!("{url}: {e}"))?;
        let status = resp.status().as_u16();
        if status != 200 {
            return Err(format!("{url}: the server answered {status}"));
        }
        // Capped once decoded (a small gzip answer can grow far past a cap on the wire).
        let mut reader = resp.body_mut().with_config().reader().take((max as u64).saturating_add(1));
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).map_err(|e| format!("{url}: {e}"))?;
        if bytes.len() > max {
            return Err(format!("{url}: larger than {max} bytes"));
        }
        Ok(bytes)
    }
}

#[cfg(target_arch = "wasm32")]
mod net {
    pub fn get(_url: &str, _max: usize) -> Result<Vec<u8>, String> {
        Err("Dictionaries are downloaded by the desktop app".into())
    }
}

#[cfg(test)]
mod tests;
