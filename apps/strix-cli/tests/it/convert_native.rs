//! `.xmath` (Strix document) and `.xmathm` (Strix template) convert like any other format: in
//! from Word, out to Word, and between each other.
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
fn xmath_converts_to_and_from_word() {
    let dir = std::env::temp_dir().join(format!("strix-cli-native-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let txt = dir.join("in.txt");
    std::fs::write(&txt, "Native round trip\n").unwrap();
    let docx = dir.join("a.docx");
    let xmath = dir.join("b.xmath");
    let xmathm = dir.join("c.xmathm");
    let back = dir.join("d.docx");
    convert(&txt, &docx);
    convert(&docx, &xmath);
    convert(&xmath, &xmathm);
    convert(&xmathm, &back);
    for p in [&xmath, &xmathm, &back] {
        assert!(std::fs::read(p).unwrap().starts_with(b"PK"), "{} is a package", p.display());
        assert!(text(p).contains("Native round trip"), "{}", p.display());
    }
    let _ = std::fs::remove_dir_all(&dir);
}
