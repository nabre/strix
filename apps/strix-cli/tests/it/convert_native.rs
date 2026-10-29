//! `.xmath` (Strix document) and `.xmathm` (Strix template): converted into from Word and other
//! imported formats, and between each other. Word and the other editable formats are written for
//! a document without Strix objects; one with any (a calculation) is never converted to them.
use std::process::Command;

fn convert(from: &std::path::Path, to: &std::path::Path) {
    let run = Command::new(env!("CARGO_BIN_EXE_strix-cli")).arg("convert").arg(from).arg(to).output().unwrap();
    assert!(run.status.success(), "{} -> {}: {}", from.display(), to.display(), String::from_utf8_lossy(&run.stderr));
}

fn text(path: &std::path::Path) -> String {
    let run = Command::new(env!("CARGO_BIN_EXE_strix-cli")).arg("text").arg(path).output().unwrap();
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    String::from_utf8_lossy(&run.stdout).into_owned()
}

#[test]
fn xmath_imports_word_and_writes_it_back_without_strix_objects() {
    let dir = std::env::temp_dir().join(format!("strix-cli-native-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let txt = dir.join("in.txt");
    std::fs::write(&txt, "Native round trip\n").unwrap();
    // A Word file to import, written by the engine's low-level writer.
    let docx = dir.join("a.docx");
    strix_engine::io::save_path(&docx, &strix_doc::Document::from_text("Native round trip")).unwrap();
    let from_txt = dir.join("t.xmath");
    let xmath = dir.join("b.xmath");
    let xmathm = dir.join("c.xmathm");
    let back = dir.join("d.xmath");
    convert(&txt, &from_txt);
    convert(&docx, &xmath);
    convert(&xmath, &xmathm);
    convert(&xmathm, &back);
    for p in [&from_txt, &xmath, &xmathm, &back] {
        assert!(std::fs::read(p).unwrap().starts_with(b"PK"), "{} is a package", p.display());
        assert!(text(p).contains("Native round trip"), "{}", p.display());
    }
    // Without Strix objects, Word and the other editable formats are written.
    for word in ["e.docx", "f.dotx", "g.odt", "h.txt", "i.rtf"] {
        let out = dir.join(word);
        convert(&xmath, &out);
        assert!(text(&out).contains("Native round trip"), "{word}");
    }
    // Word 97-2003 is import-only: converting to it fails and writes nothing.
    let out = dir.join("j.doc");
    let run = Command::new(env!("CARGO_BIN_EXE_strix-cli")).arg("convert").arg(&xmath).arg(&out).output().unwrap();
    assert!(!run.status.success() && !out.exists(), ".doc was accepted");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A document with Strix objects converts to `.xmath` and to static exports (PDF), never to Word
/// or another editable format: the error names the objects and proposes `.xmath`.
#[test]
fn a_document_with_strix_objects_never_converts_to_word() {
    let dir = std::env::temp_dir().join(format!("strix-cli-objects-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let calc = dir.join("calc.xmath");
    let made = Command::new(env!("CARGO_BIN_EXE_strix-cli"))
        .args(["run", "--template", "blank", "--cmd", r#"calc.insert={"text":"L = 4.5 m"}"#, "--save"])
        .arg(&calc)
        .output()
        .unwrap();
    assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
    for word in ["calc.docx", "calc.odt", "calc.rtf", "calc.txt"] {
        let out = dir.join(word);
        let run = Command::new(env!("CARGO_BIN_EXE_strix-cli")).arg("convert").arg(&calc).arg(&out).output().unwrap();
        let err = String::from_utf8_lossy(&run.stderr);
        assert!(!run.status.success(), "{word} was accepted");
        assert!(err.contains("1 calculation") && err.contains("calc.xmath"), "{word}: {err}");
        assert!(!out.exists(), "{word} written");
    }
    convert(&calc, &dir.join("copy.xmath"));
    convert(&calc, &dir.join("calc.pdf"));
    assert!(std::fs::read(dir.join("calc.pdf")).unwrap().starts_with(b"%PDF"));
    let _ = std::fs::remove_dir_all(&dir);
}
