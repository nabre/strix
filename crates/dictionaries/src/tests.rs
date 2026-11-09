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

const IT: Source = Source { name: "it_IT", folder: "it_IT", file: "it_IT", licence: "GPL-3.0" };

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
