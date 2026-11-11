//! Strix objects edited in another program (docs/word-round-trip.md): a Word document Strix wrote
//! comes back with the fingerprint of what each object showed (`crate::word_envelope`). An
//! object whose content changed (a result typed over, a picture replaced) stays as it is, static,
//! until the person decides: recalculate it from its Strix source, or keep the change.

use serde_json::{Value, json};

use crate::word_envelope::{Resolution, pending, resolve};
use crate::{CmdError, CmdResult, CommandSpec, Session};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("file.externalEdits", "Objects Changed in Another Program", "File › Info", run)
            .params(
                r#"{"action"?: "list" | "recalculate" | "keep", "ids"?: [n] (default: all)} → {"pending": [{"id", "kind"}], "resolved"?: n}: the Strix objects whose content was changed in another program since Strix saved the Word document (they are static until decided): recalculate makes them live again from their Strix source (the change is replaced), keep keeps the change and drops their source"#,
            ),
    ]
}

fn list(s: &Session) -> Value {
    json!(pending(&s.doc).iter().map(|e| json!({"id": e.id, "kind": e.source.kind_id()})).collect::<Vec<_>>())
}

fn run(s: &mut Session, v: &Value) -> CmdResult {
    let how = match v.get("action").and_then(Value::as_str) {
        None | Some("list") => return Ok(json!({"pending": list(s)})),
        Some("recalculate") => Resolution::Recalculate,
        Some("keep") => Resolution::Keep,
        Some(_) => return Err(CmdError::Params("action: list, recalculate or keep".into())),
    };
    let ids: Option<Vec<u32>> = match v.get("ids") {
        None | Some(Value::Null) => None,
        Some(Value::Array(a)) => Some(a.iter().filter_map(Value::as_u64).filter_map(|n| u32::try_from(n).ok()).take(100_000).collect()),
        Some(_) => return Err(CmdError::Params("ids: an array of object numbers".into())),
    };
    if pending(&s.doc).is_empty() {
        return Err(CmdError::Disabled("no Strix object was changed in another program".into()));
    }
    let resolved = resolve(&mut s.doc, ids.as_deref(), how);
    if resolved > 0 {
        s.touch();
    }
    Ok(json!({"pending": list(s), "resolved": resolved}))
}

/// After a document is opened: say on the status line how many Strix objects were changed in
/// another program, if any.
pub(crate) fn after_open(s: &mut Session) {
    let n = pending(&s.doc).len();
    if n > 0 {
        s.status = format!(
            "{n} Strix object(s) were changed in another program and are static for now: recalculate them from Strix or keep the changes (file.externalEdits)"
        );
    }
}
