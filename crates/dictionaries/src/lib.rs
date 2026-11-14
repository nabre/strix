//! Spelling dictionaries and thesauri on demand. Strix doesn't ship the Hunspell dictionaries its
//! proofing languages read, nor the thesauri its Thesaurus reads (they are large, and several are
//! GPL or LGPL): this crate knows where each one is published — the LibreOffice dictionaries
//! repository, at a fixed revision — and downloads a dictionary's `.aff`/`.dic` pair, or a
//! thesaurus's `.dat` file ([`mythes`]) with its licence notices, into the user's dictionaries
//! folder ([`crate::downloads::download_dir`]), which proofing and the Thesaurus search
//! ([`thesauri`]). The user's computer fetches the files from their publisher; Strix doesn't
//! redistribute them, and they stay separate works under their own licences, which are written
//! beside them.
//!
//! Only the files listed in [`SOURCES`] can be fetched (never a URL from a caller), each file is
//! size-capped and checked to be what it should be (a thesaurus is read whole before it is
//! kept), and a download is written whole or not at all. On the web there is nothing to download
//! into.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::path::{Path, PathBuf};

/// The repository the dictionaries come from, and the revision fetched (fixed, so what a
/// language downloads doesn't change under it).
pub const REPOSITORY: &str = "https://github.com/LibreOffice/dictionaries";
pub const REVISION: &str = "c0865a85eb67dc377f2cf8e8fe2c4fa9e53f0dcb";

/// Largest `.aff` or `.dic` downloaded: proofing reads none larger (Turkish, the largest, is ~36 MB).
pub const MAX_FILE: usize = 64 << 20;

/// Largest licence notice (a README or LICENSE file) downloaded beside a thesaurus.
pub const MAX_NOTICE: usize = 1 << 20;

/// What a download is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A Hunspell dictionary (`<file>.aff` and `<file>.dic`) proofing reads.
    Spelling,
    /// A MyThes thesaurus (`<file>.dat`) the Thesaurus reads ([`mythes`]).
    Thesaurus,
}

/// A dictionary or thesaurus Strix can download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Source {
    pub kind: Kind,
    /// A dictionary: the Hunspell name a proofing language asks for (`it_IT`; see
    /// [`crate::downloads::hunspell_name`]). A thesaurus: the name of the language it is for
    /// ([`crate::thesauri::thesaurus_name`]).
    pub name: &'static str,
    /// Its folder in the repository and its file stem there, which is also the name it is saved
    /// under (Latin American Spanish has no dictionary of its own: it reads the Mexican one,
    /// which it also looks for).
    pub folder: &'static str,
    pub file: &'static str,
    /// The licence(s) the publisher gives it under.
    pub licence: &'static str,
    /// Files of its folder that state its licence and authors, downloaded with it and kept
    /// beside it (`<file>.<notice>`).
    pub notices: &'static [&'static str],
}

/// Every dictionary a proofing language reads and every thesaurus the Thesaurus reads, as the
/// publisher licenses it (from each folder's README or LICENSE at [`REVISION`]). Brazilian
/// Portuguese's thesaurus is left out: its folder states no licence for the thesaurus's data.
pub const SOURCES: &[Source] = &[
    Source {
        kind: Kind::Spelling,
        name: "pl_PL",
        folder: "pl_PL",
        file: "pl_PL",
        licence: "GPL-2.0, LGPL-2.1, MPL-1.1, Apache-2.0 or CC-BY-4.0",
        notices: &[],
    },
    Source { kind: Kind::Spelling, name: "pt_BR", folder: "pt_BR", file: "pt_BR", licence: "LGPL-3.0 or MPL", notices: &[] },
    Source { kind: Kind::Spelling, name: "pt_PT", folder: "pt_PT", file: "pt_PT", licence: "GPL-2.0, LGPL-2.1 or MPL-1.1", notices: &[] },
    Source { kind: Kind::Spelling, name: "it_IT", folder: "it_IT", file: "it_IT", licence: "GPL-3.0", notices: &[] },
    Source { kind: Kind::Spelling, name: "hr_HR", folder: "hr_HR", file: "hr_HR", licence: "GPL-2.0, LGPL-2.1 or MPL-1.1", notices: &[] },
    Source { kind: Kind::Spelling, name: "tr_TR", folder: "tr_TR", file: "tr_TR", licence: "MPL-2.0", notices: &[] },
    Source { kind: Kind::Spelling, name: "ru_RU", folder: "ru_RU", file: "ru_RU", licence: "BSD-style", notices: &[] },
    Source { kind: Kind::Spelling, name: "cs_CZ", folder: "cs_CZ", file: "cs_CZ", licence: "GPL-2.0", notices: &[] },
    Source { kind: Kind::Spelling, name: "id_ID", folder: "id", file: "id_ID", licence: "LGPL-3.0", notices: &[] },
    Source { kind: Kind::Spelling, name: "es_ES", folder: "es", file: "es_ES", licence: "GPL-3.0+, LGPL-3.0+ or MPL-1.1+", notices: &[] },
    Source { kind: Kind::Spelling, name: "es_MX", folder: "es", file: "es_MX", licence: "GPL-3.0+, LGPL-3.0+ or MPL-1.1+", notices: &[] },
    Source { kind: Kind::Spelling, name: "es_419", folder: "es", file: "es_MX", licence: "GPL-3.0+, LGPL-3.0+ or MPL-1.1+", notices: &[] },
    Source { kind: Kind::Spelling, name: "es_UY", folder: "es", file: "es_UY", licence: "GPL-3.0+, LGPL-3.0+ or MPL-1.1+", notices: &[] },
    Source { kind: Kind::Thesaurus, name: "it_IT", folder: "it_IT", file: "th_it_IT_v2", licence: "GPL-3.0", notices: &["README_th_it_IT.txt"] },
    Source { kind: Kind::Thesaurus, name: "es", folder: "es", file: "th_es_v2", licence: "LGPL-2.1", notices: &["README_th_es.txt", "LGPLv2.1.txt"] },
    // OpenThesaurus's Portuguese snapshot (GPL-2.0 or later) merged with Onto.PT (CC BY 3.0).
    Source {
        kind: Kind::Thesaurus,
        name: "pt_PT",
        folder: "pt_PT",
        file: "th_pt_PT",
        licence: "GPL-2.0+ and CC-BY-3.0",
        notices: &["README_th_pt_PT.txt", "LICENSES.txt"],
    },
    Source { kind: Kind::Thesaurus, name: "pl_PL", folder: "pl_PL", file: "th_pl_PL_v2", licence: "LGPL-2.1", notices: &["README_en.txt"] },
    Source {
        kind: Kind::Thesaurus,
        name: "ru_RU",
        folder: "ru_RU",
        file: "th_ru_RU_M_aot_and_v2",
        licence: "LGPL-2.1",
        notices: &["README_thes_ru_RU_M_aot_and_v2.txt"],
    },
    Source {
        kind: Kind::Thesaurus,
        name: "id_ID",
        folder: "id",
        file: "th_id_ID_v2",
        licence: "LGPL-2.1",
        notices: &["README-thes", "LICENSE-thes"],
    },
    Source { kind: Kind::Thesaurus, name: "cs_CZ", folder: "cs_CZ", file: "thes_cs_CZ", licence: "GFDL-1.1+", notices: &["README_en.txt"] },
    // WordNet, which every English variant reads (the folder's README_en_GB_thes.txt says so).
    Source {
        kind: Kind::Thesaurus,
        name: "en_US",
        folder: "en",
        file: "th_en_US_v2",
        licence: "WordNet 2.1 licence (Princeton University)",
        notices: &["WordNet_license.txt"],
    },
];

/// The dictionary named `name` (`it_IT`), if Strix can download it.
pub fn source(name: &str) -> Option<&'static Source> {
    SOURCES.iter().find(|s| s.kind == Kind::Spelling && s.name == name)
}

/// The thesaurus named `name` (`it_IT`, `es`), if Strix can download it.
pub fn thesaurus(name: &str) -> Option<&'static Source> {
    SOURCES.iter().find(|s| s.kind == Kind::Thesaurus && s.name == name)
}

/// The dictionary the proofing language of `lang` (a BCP 47 tag) reads, if Strix can download it.
pub fn source_for(lang: Option<&str>) -> Option<&'static Source> {
    crate::downloads::hunspell_name(lang).and_then(source)
}

impl Source {
    /// The web page of the dictionary's folder (where its licence and authors are).
    pub fn page(&self) -> String {
        format!("{REPOSITORY}/tree/{REVISION}/{}", self.folder)
    }

    /// The name of the file a download keeps: the `.dic` of a dictionary, the `.dat` of a
    /// thesaurus.
    pub fn file_name(&self) -> String {
        match self.kind {
            Kind::Spelling => format!("{}.dic", self.file),
            Kind::Thesaurus => format!("{}.dat", self.file),
        }
    }
}

/// The address files are fetched from.
fn base_url() -> String {
    format!("https://raw.githubusercontent.com/LibreOffice/dictionaries/{REVISION}/")
}

/// Download `src` into `dir` (created if needed) and return the file written (the `.dic` of a
/// dictionary, the `.dat` of a thesaurus). Proofing reads a dictionary once its dictionaries are
/// loaded again ([`crate::downloads::reload_dictionaries`]), the Thesaurus a thesaurus at its next
/// lookup. Blocks while it downloads (seconds to a minute): call it off the interface's thread.
pub fn download(src: &Source, dir: &Path) -> Result<PathBuf, String> {
    let max = match src.kind {
        Kind::Spelling => MAX_FILE,
        Kind::Thesaurus => mythes::MAX_FILE,
    };
    download_from(&base_url(), src, dir, max)
}

/// [`download`] from `base` (tests serve the files locally), with files of at most `max` bytes.
fn download_from(base: &str, src: &Source, dir: &Path, max: usize) -> Result<PathBuf, String> {
    let fetch = |remote: &str, cap: usize, check: &dyn Fn(&[u8]) -> Result<(), String>| -> Result<Vec<u8>, String> {
        let url = format!("{base}{}/{remote}", src.folder);
        let bytes = net::get(&url, cap)?;
        check(&bytes).map_err(|e| format!("{remote}: {e}"))?;
        Ok(bytes)
    };
    // (the name it is kept under, its bytes), the main file last.
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    match src.kind {
        Kind::Spelling => {
            for ext in ["aff", "dic"] {
                let name = format!("{}.{ext}", src.file);
                let bytes = fetch(&name, max, &|b| check(ext, b))?;
                files.push((name, bytes));
            }
        }
        Kind::Thesaurus => {
            for notice in src.notices {
                let bytes = fetch(notice, MAX_NOTICE, &check_notice)?;
                files.push((format!("{}.{notice}", src.file), bytes));
            }
            let name = src.file_name();
            // Read whole before it is kept: a file the Thesaurus couldn't read is never written.
            let bytes = fetch(&name, max, &|b| mythes::parse(b).map(|_| ()))?;
            files.push((name, bytes));
        }
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    // Every file lands, or none: written aside, then renamed into place.
    let part = |name: &str| dir.join(format!(".{name}.part"));
    let result = files
        .iter()
        .try_for_each(|(name, bytes)| std::fs::write(part(name), bytes).map_err(|e| format!("{}: {e}", part(name).display())))
        .and_then(|()| {
            files.iter().try_for_each(|(name, _)| {
                let path = dir.join(name);
                std::fs::rename(part(name), &path).map_err(|e| format!("{}: {e}", path.display()))
            })
        });
    for (name, _) in &files {
        let _ = std::fs::remove_file(part(name));
    }
    result?;
    // Where it came from and under which licence, beside it.
    let kept: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
    let what = match src.kind {
        Kind::Spelling => "Hunspell dictionary",
        Kind::Thesaurus => "MyThes thesaurus",
    };
    let note = format!(
        "{}: {what} downloaded by Strix from\n{}\nLicence: {}\n{}\n",
        kept.join(", "),
        src.page(),
        src.licence,
        if src.notices.is_empty() {
            "See that folder's README or LICENSE for its authors and the licence text.".to_string()
        } else {
            format!(
                "Its authors and licence, as that folder states them, are in {}.",
                src.notices.iter().map(|n| format!("{}.{n}", src.file)).collect::<Vec<_>>().join(" and ")
            )
        }
    );
    let _ = std::fs::write(dir.join(format!("{}.source.txt", src.file)), note);
    Ok(dir.join(src.file_name()))
}

/// Is this a licence notice, not an error page? Text, not empty.
fn check_notice(bytes: &[u8]) -> Result<(), String> {
    let head = String::from_utf8_lossy(bytes.get(..bytes.len().min(512)).unwrap_or_default()).to_string();
    let first = head.trim_start_matches('\u{feff}').trim_start();
    if first.is_empty() {
        return Err("empty".into());
    }
    if first.starts_with("<!") || first.to_ascii_lowercase().starts_with("<html") {
        return Err("not a licence notice (a web page)".into());
    }
    if bytes.contains(&0) {
        return Err("not a text file".into());
    }
    Ok(())
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

pub mod downloads;
pub mod mythes;
pub mod thesauri;
