//! Strix's own file formats: `.xmath` (document) and `.xmathm` (template). They hold the same
//! OOXML package as `.docx`/`.dotx`; only they are saved in place by a plain Save, every other
//! format is imported (Open) and exported (Save As).

use serde_json::json;
use strix_doc::StoryRef;

use crate::{CmdError, Session, io};

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("strix-native-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn session(text: &str) -> Session {
    Session::new(strix_doc::Document::from_text(text))
}

fn body(s: &Session) -> String {
    s.doc.plain_text(StoryRef::Body)
}

/// The content type `[Content_Types].xml` gives the main document part of a package.
fn main_content_type(bytes: &[u8]) -> String {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    zip.by_name("[Content_Types].xml").unwrap().read_to_string(&mut xml).unwrap();
    let at = xml.find("PartName=\"/word/document.xml\"").expect("main part override");
    let rest = &xml[at..];
    let start = rest.find("ContentType=\"").unwrap() + "ContentType=\"".len();
    let end = rest[start..].find('"').unwrap();
    rest[start..start + end].to_string()
}

#[test]
fn native_names_and_constants() {
    assert_eq!((io::NATIVE_DOC_EXT, io::NATIVE_TEMPLATE_EXT), ("xmath", "xmathm"));
    assert_eq!(&io::OPEN_EXTS[..2], ["xmath", "xmathm"], "Strix's own formats open first");
    assert_eq!(&io::SAVE_EXTS[..2], ["xmath", "xmathm"], "and save first");
    for yes in ["xmath", "XMATH", ".xmath", "xmathm", "a.xmath", "/d/a.XMATHM", r"C:\d\Report.xmath", "a.docx.xmath"] {
        assert!(io::is_native(yes), "{yes}");
    }
    for no in ["", ".", "docx", "a.docx", "a.xmath.docx", "xmathx", "a.xmat", "/d.xmath/notes", "x/", "é", "\u{0}"] {
        assert!(!io::is_native(no), "{no:?}");
    }
    assert!(io::is_native_template("a.xmathm") && !io::is_native_template("a.xmath"));
    let p = std::path::Path::new;
    assert_eq!(io::native_path(p("/d/report.docx")), p("/d/report.xmath"));
    assert_eq!(io::native_path(p("/d/letter.DOTX")), p("/d/letter.xmathm"));
    assert_eq!(io::native_path(p("/d/old.doc")), p("/d/old.xmath"));
    assert_eq!(io::native_path(p("/d/notes")), p("/d/notes.xmath"));
    assert_eq!(io::native_path(p("/d/kept.xmathm")), p("/d/kept.xmathm"));
    assert_eq!(strix_docx::Flavor::from_ext("xmath"), Some(strix_docx::Flavor::Document));
    assert_eq!(strix_docx::Flavor::from_ext("XMATHM"), Some(strix_docx::Flavor::Template));
    assert!(io::can_encrypt("a.xmath") && io::can_encrypt("b.xmathm"));
}

/// `.xmath` and `.xmathm` are the OOXML package `.docx` and `.dotx` hold: they round-trip, and
/// the template's main part has the template content type, so a renamed copy opens in Word.
#[test]
fn xmath_and_xmathm_round_trip_as_ooxml_packages() {
    let s = session("Strix native\nSecond paragraph");
    for (name, flavor) in [("a.xmath", strix_docx::Flavor::Document), ("a.xmathm", strix_docx::Flavor::Template)] {
        let bytes = io::save_bytes(name, &s.doc).unwrap();
        assert!(bytes.starts_with(b"PK\x03\x04"), "{name} is a zip package");
        assert_eq!(main_content_type(&bytes), flavor.main_content_type(), "{name}");
        let back = io::open_bytes(name, &bytes).unwrap();
        assert_eq!(back.plain_text(StoryRef::Body), body(&s), "{name}");
        // Renamed to Word's extension, the same bytes open as Word's format.
        let word = if flavor == strix_docx::Flavor::Template { "a.dotx" } else { "a.docx" };
        assert_eq!(io::open_bytes(word, &bytes).unwrap().plain_text(StoryRef::Body), body(&s));
    }
    assert_eq!(main_content_type(&io::save_bytes("t.xmathm", &s.doc).unwrap()), main_content_type(&io::save_bytes("t.dotx", &s.doc).unwrap()));
    assert_eq!(main_content_type(&io::save_bytes("d.xmath", &s.doc).unwrap()), main_content_type(&io::save_bytes("d.docx", &s.doc).unwrap()));
}

/// A `.xmath` holding something else (a renamed RTF, a legacy Word file) opens by what it holds;
/// an encrypted one asks for its password.
#[test]
fn xmath_is_sniffed_like_a_word_package() {
    let rtf = b"{\\rtf1\\ansi Renamed rich text\\par}";
    assert!(io::open_bytes("x.xmath", rtf).unwrap().plain_text(StoryRef::Body).contains("Renamed rich text"));
    let s = session("Secret");
    let plain = io::save_bytes("x.xmath", &s.doc).unwrap();
    let locked = io::save_bytes_with("x.xmath", &s.doc, Some("pw"), &io::ExportOptions::default()).unwrap();
    assert!(strix_docx::is_encrypted(&locked) && !strix_docx::is_encrypted(&plain));
    assert!(io::needs_password(&io::open_bytes("x.xmath", &locked).unwrap_err()));
    let (doc, encrypted) = io::open_bytes_with("x.xmathm", &locked, Some("pw")).unwrap();
    assert!(encrypted && doc.plain_text(StoryRef::Body).contains("Secret"));
    // Garbage under the native name is an error, never a panic.
    for junk in [&b""[..], b"PK\x03\x04garbage", b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1", &[0xFF; 64]] {
        let _ = io::open_bytes("junk.xmath", junk);
        let _ = io::open_bytes("junk.xmathm", junk);
    }
}

/// Save writes in place only a Strix file. A document opened from another format (here a
/// `.docx`) isn't overwritten by a plain Save: the engine refuses with what to do instead
/// (programmatic calls never open dialogs), and Save As to a `.xmath` makes that its file.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn save_on_an_imported_docx_asks_for_save_as() {
    let dir = scratch("imported");
    let docx = dir.join("report.docx");
    io::save_path(&docx, &session("Imported text").doc).unwrap();
    let original = std::fs::read(&docx).unwrap();
    let mut s = session("");
    s.run("file.open", &json!({"path": docx.to_string_lossy()})).unwrap();
    s.run("text.insert", &json!({"text": "Edited "})).unwrap();
    let e = s.run("file.save", &json!({})).unwrap_err();
    assert!(matches!(e, CmdError::Failed(_)), "{e:?}");
    let msg = e.to_string();
    assert!(msg.contains("report.docx") && msg.contains("file.saveAs") && msg.contains("report.xmath"), "{msg}");
    assert!(s.dirty, "nothing saved");
    assert_eq!(std::fs::read(&docx).unwrap(), original, "the imported file is untouched");
    assert!(s.ui_requests.iter().all(|q| q["open"] != "saveAs"), "no dialog asked for");

    let native = dir.join("report.xmath");
    let r = s.run("file.saveAs", &json!({"path": native.to_string_lossy()})).unwrap();
    assert_eq!(r["saved"], true);
    assert_eq!(s.path.as_deref(), Some(native.as_path()));
    assert!(!s.dirty);
    s.run("text.insert", &json!({"text": "Again "})).unwrap();
    assert_eq!(s.run("file.save", &json!({})).unwrap()["saved"], true, "a Strix file saves in place");
    let saved = io::open_path(&native).unwrap().plain_text(StoryRef::Body);
    assert!(saved.contains("Again") && saved.contains("Edited") && saved.contains("Imported text"), "{saved}");
    assert_eq!(std::fs::read(&docx).unwrap(), original);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Save As to a name without an extension writes a Strix document; a `.xmathm` is a template and
/// Save as Template makes one by default.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn save_as_without_an_extension_writes_xmath() {
    let dir = scratch("no-ext");
    let mut s = session("Plain name");
    for (asked, saved) in [("Notes", "Notes.xmath"), ("Dotted.", "Dotted.xmath")] {
        let r = s.run("file.saveAs", &json!({"path": dir.join(asked).to_string_lossy()})).unwrap();
        let expected = dir.join(saved);
        assert_eq!(r["path"], json!(expected.to_string_lossy()), "{asked}");
        assert_eq!(s.path.as_deref(), Some(expected.as_path()));
        assert!(std::fs::read(&expected).unwrap().starts_with(b"PK"), "{saved} is a package");
    }
    let template = dir.join("Memo.xmathm");
    s.run("file.saveAs", &json!({"path": template.to_string_lossy()})).unwrap();
    assert_eq!(main_content_type(&std::fs::read(&template).unwrap()), strix_docx::Flavor::Template.main_content_type());
    let r = s.run("file.saveTemplate", &json!({"path": dir.join("Letter").to_string_lossy()})).unwrap();
    assert_eq!(r["path"], json!(dir.join("Letter.xmathm").to_string_lossy()));
    let listed = crate::cmd::user_templates::list_templates(&dir.to_string_lossy()).unwrap();
    let labels: Vec<&str> = listed.templates.iter().map(|t| t.label.as_str()).collect();
    assert_eq!(labels, ["Letter", "Memo"], "the personal templates folder lists .xmathm");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Export to another format writes a copy; a Strix document keeps its own file.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn exports_leave_the_strix_document_its_file() {
    let dir = scratch("export");
    let native = dir.join("book.xmath");
    let mut s = session("Chapter one");
    s.run("file.saveAs", &json!({"path": native.to_string_lossy()})).unwrap();
    s.run("text.insert", &json!({"text": "!"})).unwrap();
    for copy in ["book.md", "book.pdf", "book.html"] {
        s.run("file.saveAs", &json!({"path": dir.join(copy).to_string_lossy()})).unwrap();
        assert!(dir.join(copy).is_file());
        assert_eq!(s.path.as_deref(), Some(native.as_path()), "{copy} is a copy");
        assert!(s.dirty, "{copy} leaves the document unsaved");
    }
    // Import-only formats are never written.
    for refused in ["book.txt", "book.docx", "book.odt"] {
        let e = s.run("file.saveAs", &json!({"path": dir.join(refused).to_string_lossy()})).unwrap_err().to_string();
        assert!(e.contains("only opens it"), "{refused}: {e}");
        assert!(!dir.join(refused).exists(), "{refused} written");
        assert_eq!(s.path.as_deref(), Some(native.as_path()));
        assert!(s.dirty);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Hostile names never panic: Save and Save As with odd paths give a result or an error. The
/// paths are inside a folder that doesn't exist, so nothing is written anywhere.
#[test]
fn hostile_save_names_never_panic() {
    let missing = std::env::temp_dir().join(format!("strix-native-missing-{}", std::process::id())).join("nope");
    let mut s = session("x");
    let long = "x".repeat(5000);
    let names = ["", ".", "..", ".xmath", "a.", "é", "名前", "a\u{0}b.xmath", "a.xmath.", "...", "\u{202e}htam.x", long.as_str()];
    for name in names {
        let _ = io::is_native(name);
        let _ = io::is_native_template(name);
        let _ = io::native_path(std::path::Path::new(name));
        let path = format!("{}/{name}", missing.to_string_lossy());
        for id in ["file.save", "file.saveAs", "file.saveTemplate"] {
            let _ = s.run(id, &json!({"path": path}));
        }
        s.path = Some(std::path::PathBuf::from(&path));
        let _ = s.run("file.save", &json!({}));
    }
    for junk in [json!({"path": 7}), json!({"path": null}), json!({"path": []}), json!([]), json!(null)] {
        s.path = None;
        let _ = s.run("file.save", &junk);
        let _ = s.run("file.saveAs", &junk);
    }
    assert!(!missing.exists());
}
