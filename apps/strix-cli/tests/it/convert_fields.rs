// Modified by the Strix contributors (fork of WordCraft); see NOTICE.
//! Explicit field refresh for headless conversions; ordinary conversion keeps cached results.
use std::process::Command;
use strix_doc::para::InlineObject;
use strix_doc::{Document, Path, Pos, StoryRef};

#[test]
fn convert_refresh_is_opt_in() {
    let dir = std::env::temp_dir().join(format!("wc-convert-fields-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut doc = Document::from_text("Contents\nTarget");
    doc.para_mut(StoryRef::Body, &Path::top(1)).unwrap().props.page_break_before = Some(true);
    doc.insert_object(&Pos::body(1, 0), InlineObject::BookmarkStart { name: "target".into() }, &Default::default()).unwrap();
    for locked in [true, false] {
        doc.insert_object(
            &Pos::body(0, 0),
            InlineObject::Field { instr: "PAGEREF target".into(), result: "104".into(), locked, dirty: true, code: false, form: String::new() },
            &Default::default(),
        )
        .unwrap();
    }
    let input = dir.join("in.json");
    std::fs::write(&input, serde_json::to_vec(&doc).unwrap()).unwrap();
    let docx = dir.join("in.xmath");
    let make = Command::new(env!("CARGO_BIN_EXE_strix-cli")).arg("convert").arg(&input).arg(&docx).output().unwrap();
    assert!(make.status.success(), "{}", String::from_utf8_lossy(&make.stderr));
    for refresh in [false, true] {
        let output = dir.join(if refresh { "refreshed.xmath" } else { "cached.xmath" });
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_strix-cli"));
        cmd.arg("convert").arg(&docx).arg(&output);
        if refresh {
            cmd.arg("--update-fields");
        }
        let run = cmd.output().unwrap();
        assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
        let out: Document = strix_engine::io::open_path(&output).unwrap();
        let results: Vec<_> = out
            .para(StoryRef::Body, &Path::top(0))
            .unwrap()
            .objects
            .iter()
            .filter_map(|o| match o {
                InlineObject::Field { result, .. } => Some(result.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(results, if refresh { vec!["2", "104"] } else { vec!["104", "104"] });
    }
    std::fs::remove_dir_all(dir).unwrap();
}
