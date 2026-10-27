//! `.xmath` (Strix document) and `.xmathm` (Strix template): converted into from Word and other
//! imported formats, and between each other; Word and other import-only formats are never written.
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
fn xmath_imports_word_and_never_writes_it() {
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
    // Word formats are import-only: converting to one fails and writes nothing.
    for word in ["e.docx", "f.dotx", "g.odt", "h.txt"] {
        let out = dir.join(word);
        let run = Command::new(env!("CARGO_BIN_EXE_strix-cli")).arg("convert").arg(&xmath).arg(&out).output().unwrap();
        assert!(!run.status.success(), "{word} was accepted");
        assert!(!out.exists(), "{word} written");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
