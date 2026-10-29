//! `save_document` writes Word and the other editable formats only for a document without Strix
//! objects; with any (a calculation) it refuses, naming them and proposing `.xmath`, and writes
//! nothing. Static exports stay allowed.

use serde_json::{Value, json};
use strix_mcp::{Headless, Server};

/// `tools/call` -> (isError, the first text block).
fn call(server: &mut Server, name: &str, args: Value) -> (bool, String) {
    let line = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": args}}).to_string();
    let reply: Value = serde_json::from_str(&server.handle_line(&line).unwrap()).unwrap();
    let result = &reply["result"];
    let text = result["content"].as_array().unwrap().iter().find_map(|c| c["text"].as_str()).unwrap_or_default().to_string();
    (result["isError"] == true, text)
}

#[test]
fn save_document_refuses_lossy_formats_for_strix_objects() {
    let dir = std::env::temp_dir().join(format!("strix-mcp-objects-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = |name: &str| dir.join(name).to_string_lossy().into_owned();
    let mut server = Server::new(Box::new(Headless::default()));
    let (error, text) = call(&mut server, "type_text", json!({"text": "Beam"}));
    assert!(!error, "{text}");
    // No Strix objects: .docx is written as the reference word processor writes it.
    let (error, text) = call(&mut server, "save_document", json!({"path": path("plain.docx")}));
    assert!(!error && dir.join("plain.docx").is_file(), "{text}");

    let (error, text) = call(&mut server, "execute", json!({"command": "calc.insert", "params": {"text": "L = 4.5 m"}}));
    assert!(!error, "{text}");
    let (error, text) = call(&mut server, "save_document", json!({"path": path("beam.docx")}));
    assert!(error, "{text}");
    assert!(text.contains("1 calculation") && text.contains("beam.xmath"), "{text}");
    assert!(!dir.join("beam.docx").exists());
    // Saving in place the .docx it is now would lose the calculation too.
    let (error, text) = call(&mut server, "save_document", json!({}));
    assert!(error && text.contains("1 calculation"), "{text}");
    for ok in ["beam.xmath", "beam.pdf"] {
        let (error, text) = call(&mut server, "save_document", json!({"path": path(ok)}));
        assert!(!error && dir.join(ok).is_file(), "{ok}: {text}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
