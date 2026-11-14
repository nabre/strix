//! The data an exercise already shows to the student: a datum recalled in the statement or the
//! questions by an inline value (`{{R}}`, `{{R -> cm}}`) or in a drawing there by an annotation
//! (`{{R}}`, `{{R:value}}`, `{{R:both}}`) needs no line in the data part (docs/exercise.md).

use std::collections::BTreeSet;

use super::{Layout, Part};
use crate::Session;

/// At most this many recalls are read from one drawing.
const MAX_RECALLS: usize = 256;

/// For each exercise (in [`super::scan`] order), the expressions its statement, title and
/// questions show the value of.
pub(super) fn shown(s: &Session, layout: &Layout) -> Vec<BTreeSet<String>> {
    let mut out = vec![BTreeSet::new(); layout.exercises.len()];
    let mut note = |top: usize, name: &str| {
        if let Some((Some(ex), part)) = layout.parts.get(top)
            && matches!(part, Part::Title | Part::Text | Part::Questions)
            && let Some(set) = out.get_mut(*ex)
        {
            set.insert(name.trim().to_string());
        }
    };
    for (top, formula) in super::expr::collect_exprs(&s.doc.body) {
        note(top, super::expr::shown_expr(&formula));
    }
    for (top, source) in super::super::calc::cad_sources(s) {
        for name in recalls(&source) {
            note(top, &name);
        }
    }
    out
}

/// The parameters a drawing's annotations show the value of: `{{L}}`, `{{L:value}}` and
/// `{{L:both}}` (not `{{L:symbol}}`, the symbol alone).
fn recalls(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = source;
    while out.len() < MAX_RECALLS {
        let Some(i) = rest.find("{{") else { break };
        let Some(j) = rest.get(i + 2..).and_then(|r| r.find("}}")).map(|j| i + 2 + j) else { break };
        let inner = rest.get(i + 2..j).unwrap_or_default().trim();
        let (name, mode) = inner.split_once(':').map_or((inner, "value"), |(n, m)| (n.trim(), m.trim()));
        if !name.is_empty() && mode != "symbol" {
            out.push(name.to_string());
        }
        rest = rest.get(j + 2..).unwrap_or_default();
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn recalls_skip_the_symbol_alone() {
        assert_eq!(super::recalls(r#"{"value":"r = {{R}}, {{r_1:both}} {{L:symbol}} {{h:value}} {{"#), vec!["R", "r_1", "h"]);
    }
}
