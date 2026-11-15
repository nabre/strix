use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;

use super::*;

/// A one-thread HTTP server answering `GET /<path>` from `files` (404 otherwise), for `n` requests.
fn serve(files: HashMap<String, Vec<u8>>, n: usize) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}/", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming().take(n) {
            let Ok(mut s) = stream else { continue };
            let mut buf = [0u8; 4096];
            let len = s.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..len]).to_string();
            let path = req.split_whitespace().nth(1).unwrap_or("/").trim_start_matches('/').to_string();
            let (status, body) = match files.get(&path) {
                Some(b) => ("200 OK", b.clone()),
                None => ("404 Not Found", b"<html>404</html>".to_vec()),
            };
            let head = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
            let _ = s.write_all(head.as_bytes());
            let _ = s.write_all(&body);
        }
    });
    base
}

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("strix-dictionaries-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

const IT: Source = Source { kind: Kind::Spelling, name: "it_IT", folder: "it_IT", file: "it_IT", licence: "GPL-3.0", notices: &[] };

#[test]
fn every_proofing_language_can_download_its_dictionary() {
    for tag in ["pl-PL", "pt-BR", "pt-PT", "it-IT", "it-CH", "hr-HR", "tr-TR", "ru-RU", "cs-CZ", "id-ID", "es-ES", "es-MX", "es-AR", "es-UY", "es-US"]
    {
        let src = source_for(Some(tag)).unwrap_or_else(|| panic!("{tag}: no source"));
        assert!(!src.licence.is_empty() && src.page().contains(REVISION), "{tag}");
    }
    // English needs none; unchecked languages have none.
    assert_eq!(source_for(Some("en-US")), None);
    assert_eq!(source_for(Some("ja-JP")), None);
    assert_eq!(source("../../etc/passwd"), None);
    // Latin American Spanish reads the Mexican dictionary, which it also looks for.
    assert_eq!(source_for(Some("es-AR")).map(|s| s.file), Some("es_MX"));
}

#[test]
fn downloads_a_dictionary_whole_and_proofing_reads_it() {
    let aff = b"SET UTF-8\nTRY aeio\n".to_vec();
    let dic = b"2\nciao\nmondo\n".to_vec();
    let base = serve(HashMap::from([("it_IT/it_IT.aff".to_string(), aff.clone()), ("it_IT/it_IT.dic".to_string(), dic.clone())]), 2);
    let dir = temp_dir("ok");
    let file = download_from(&base, &IT, &dir, MAX_FILE).unwrap();
    assert_eq!(file, dir.join("it_IT.dic"));
    assert_eq!(std::fs::read(dir.join("it_IT.aff")).unwrap(), aff);
    assert_eq!(std::fs::read(&file).unwrap(), dic);
    let note = std::fs::read_to_string(dir.join("it_IT.source.txt")).unwrap();
    assert!(note.contains("GPL-3.0") && note.contains(REVISION), "{note}");
    // No leftovers; the pair is one proofing finds and reads.
    let names: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).collect();
    assert!(!names.iter().any(|n| n.ends_with(".part")), "{names:?}");
    let (a, d) = wordcraft_proof::hunspell::find_in(&dir, &["it_IT"]).unwrap();
    assert!(wordcraft_proof::hunspell::load(&a, &d).is_ok());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_download_writes_nothing() {
    // The word list is missing (404 page): no `.aff` alone is left behind.
    let base = serve(HashMap::from([("it_IT/it_IT.aff".to_string(), b"SET UTF-8\n".to_vec())]), 2);
    let dir = temp_dir("missing");
    let e = download_from(&base, &IT, &dir, MAX_FILE).unwrap_err();
    assert!(e.contains("404"), "{e}");
    assert!(!dir.join("it_IT.aff").exists());
    // An error page served as 200, a word list without its count, a file over the cap.
    for (aff, dic, max, want) in [
        (b"<!DOCTYPE html>".to_vec(), b"1\na\n".to_vec(), MAX_FILE, "web page"),
        (b"SET UTF-8\n".to_vec(), b"ciao\n".to_vec(), MAX_FILE, "word count"),
        (b"SET UTF-8\n".to_vec(), vec![b'1'; 100], 50, "larger"),
        (Vec::new(), b"1\na\n".to_vec(), MAX_FILE, "empty"),
    ] {
        let base = serve(HashMap::from([("it_IT/it_IT.aff".to_string(), aff), ("it_IT/it_IT.dic".to_string(), dic)]), 2);
        let e = download_from(&base, &IT, &dir, max).unwrap_err();
        assert!(e.contains(want), "{want}: {e}");
        assert!(!dir.join("it_IT.aff").exists() && !dir.join("it_IT.dic").exists(), "{want}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_unreachable_server_is_an_error() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}/", listener.local_addr().unwrap());
    drop(listener);
    assert!(download_from(&base, &IT, &temp_dir("down"), MAX_FILE).is_err());
}

/// LO1.1: every thesaurus Strix offers says under which licence its publisher gives it and where
/// that is written, at the revision fetched.
#[test]
fn every_thesaurus_has_its_licence_and_page() {
    let thesauri: Vec<_> = SOURCES.iter().filter(|s| s.kind == Kind::Thesaurus).collect();
    assert!(thesauri.len() >= 8, "{}", thesauri.len());
    for s in &thesauri {
        assert!(!s.licence.trim().is_empty(), "{}", s.name);
        assert!(s.page().contains(REVISION) && s.page().ends_with(s.folder), "{}", s.name);
        assert!(!s.notices.is_empty(), "{}: the files that state its licence", s.name);
        assert!(s.file_name().ends_with(".dat"), "{}", s.name);
        // Names and files come from this table only: nothing a caller could point elsewhere.
        for part in [s.folder, s.file].iter().chain(s.notices) {
            assert!(!part.contains("..") && !part.contains('/') && !part.contains('\\'), "{part}");
        }
        assert_eq!(thesaurus(s.name), Some(*s));
    }
    // Spelling and thesaurus rows of the same language are told apart.
    assert_eq!(source("it_IT").map(|s| s.kind), Some(Kind::Spelling));
    assert_eq!(thesaurus("it_IT").map(|s| s.kind), Some(Kind::Thesaurus));
    assert_eq!(thesaurus("pt_BR"), None, "its folder states no licence for the thesaurus");
    assert_eq!(thesaurus("../it_IT"), None);
}

const TH_IT: Source =
    Source { kind: Kind::Thesaurus, name: "it_IT", folder: "it_IT", file: "th_it_IT_v2", licence: "GPL-3.0", notices: &["README_th_it_IT.txt"] };

#[test]
fn downloads_a_thesaurus_with_its_licence_notice() {
    let dat = "UTF-8\ncasa|1\n(s.f.)|abitazione|dimora\n".as_bytes().to_vec();
    let readme = b"Italian Thesaurus\nLicense: GNU GPL 3\n".to_vec();
    let base =
        serve(HashMap::from([("it_IT/th_it_IT_v2.dat".to_string(), dat.clone()), ("it_IT/README_th_it_IT.txt".to_string(), readme.clone())]), 2);
    let dir = temp_dir("thesaurus");
    let file = download_from(&base, &TH_IT, &dir, crate::mythes::MAX_FILE).unwrap();
    assert_eq!(file, dir.join("th_it_IT_v2.dat"));
    assert_eq!(std::fs::read(&file).unwrap(), dat);
    assert_eq!(std::fs::read(dir.join("th_it_IT_v2.README_th_it_IT.txt")).unwrap(), readme);
    let note = std::fs::read_to_string(dir.join("th_it_IT_v2.source.txt")).unwrap();
    assert!(note.contains("GPL-3.0") && note.contains(REVISION) && note.contains("th_it_IT_v2.README_th_it_IT.txt"), "{note}");
    let names: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).collect();
    assert!(!names.iter().any(|n| n.ends_with(".part")), "{names:?}");
    let t = crate::mythes::load(&file, crate::mythes::MAX_FILE).unwrap();
    assert_eq!(t.lookup("Case", Some("it-IT")).map(|(w, m)| (w, m.len())), Some(("casa".to_string(), 1)));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_thesaurus_that_cant_be_read_is_not_kept() {
    let dir = temp_dir("bad-thesaurus");
    for (dat, want) in [
        (b"UTF-8\ncasa|3\n(s.f.)|abitazione\n".to_vec(), "declares 3 meanings"),
        (b"<!DOCTYPE html><html>".to_vec(), "web page"),
        (b"EBCDIC\ncasa|1\n-|dimora\n".to_vec(), "unknown encoding"),
        (Vec::new(), "empty"),
    ] {
        let base =
            serve(HashMap::from([("it_IT/th_it_IT_v2.dat".to_string(), dat), ("it_IT/README_th_it_IT.txt".to_string(), b"GPL 3\n".to_vec())]), 2);
        let e = download_from(&base, &TH_IT, &dir, crate::mythes::MAX_FILE).unwrap_err();
        assert!(e.contains(want), "{want}: {e}");
        assert!(!dir.join("th_it_IT_v2.dat").exists() && !dir.join("th_it_IT_v2.README_th_it_IT.txt").exists(), "{want}");
    }
    // A missing licence notice: nothing is kept either.
    let base = serve(HashMap::from([("it_IT/th_it_IT_v2.dat".to_string(), b"UTF-8\na|1\n-|b\n".to_vec())]), 1);
    let e = download_from(&base, &TH_IT, &dir, crate::mythes::MAX_FILE).unwrap_err();
    assert!(e.contains("404"), "{e}");
    assert!(!dir.join("th_it_IT_v2.dat").exists());
    let _ = std::fs::remove_dir_all(&dir);
}
