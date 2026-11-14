//! The data an exercise already shows to the student: a datum recalled in the statement or the
//! questions by an inline value (`{{R}}`, `{{R -> cm}}`) or in a drawing there by an annotation
//! (`{{R}}`, `{{R:value}}`, `{{R:both}}`) needs no line in the data part (docs/exercise.md).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use wordcraft_doc::{Block, InlineObject};

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
    recalls_with_modes(source).into_iter().filter(|(_, mode)| mode != "symbol").map(|(name, _)| name).collect()
}

/// A drawing's annotations' recalls: the parameter and how it is shown (`value` when not said).
fn recalls_with_modes(source: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = source;
    while out.len() < MAX_RECALLS {
        let Some(i) = rest.find("{{") else { break };
        let Some(j) = rest.get(i + 2..).and_then(|r| r.find("}}")).map(|j| i + 2 + j) else { break };
        let inner = rest.get(i + 2..j).unwrap_or_default().trim();
        let (name, mode) = inner.split_once(':').map_or((inner, "value"), |(n, m)| (n.trim(), m.trim()));
        if !name.is_empty() {
            out.push((name.to_string(), mode.to_string()));
        }
        rest = rest.get(j + 2..).unwrap_or_default();
    }
    out
}

/// At most this many inline values are read for [`value_only`].
const MAX_INLINE: usize = 2_000;

/// For each exercise (by index), the data the reader only ever sees as values: a datum whose
/// value its title, statement or questions show — an inline value recalling it shown as value
/// and unit or as the number only (`{{h}}`, `{{h -> cm}}`), a drawing there whose annotation
/// shows its value (`{{h}}`, `{{h:value}}`) — so the data part has no line for it, and whose
/// symbol they never show (an inline value shown as Symbol = Value, a drawing's `{{h:both}}` or
/// `{{h:symbol}}`). Calculations write such a datum as its value (docs/calc.md).
pub(super) fn value_only(s: &Session, layout: &Layout) -> BTreeMap<usize, BTreeSet<String>> {
    let n = layout.exercises.len();
    let (mut values, mut symbols) = (vec![BTreeSet::new(); n], vec![BTreeSet::new(); n]);
    let mut note = |top: usize, name: &str, symbol: bool| {
        if let Some((Some(ex), part)) = layout.parts.get(top)
            && matches!(part, Part::Title | Part::Text | Part::Questions)
            && let (Some(v), Some(sy)) = (values.get_mut(*ex), symbols.get_mut(*ex))
        {
            if symbol { sy } else { v }.insert(name.trim().to_string());
        }
    };
    let mut inline = Vec::new();
    for (top, b) in s.doc.body.iter().enumerate() {
        inline_values(b, top, 0, &mut inline);
    }
    for (top, formula, show) in inline {
        note(top, super::expr::shown_expr(&formula), show == "both");
    }
    for (top, source) in super::super::calc::cad_sources(s) {
        for (name, mode) in recalls_with_modes(&source) {
            note(top, &name, mode != "value");
        }
    }
    let mut out = BTreeMap::new();
    for (k, e) in layout.exercises.iter().enumerate() {
        let (Some(v), Some(sy)) = (values.get(k), symbols.get(k)) else { continue };
        let set: BTreeSet<String> =
            e.data.items.iter().map(|i| i.symbol.trim().to_string()).filter(|sym| !sym.is_empty() && v.contains(sym) && !sy.contains(sym)).collect();
        if !set.is_empty() {
            out.insert(k, set);
        }
    }
    out
}

/// The inline values in `b` (top-level block `top`): their formula and how each is shown.
fn inline_values(b: &Arc<Block>, top: usize, depth: usize, out: &mut Vec<(usize, String, &'static str)>) {
    if depth > 16 || out.len() >= MAX_INLINE {
        return;
    }
    match &**b {
        Block::Para(p) => {
            for (a, _) in super::expr::expr_spans(p) {
                if let Some(InlineObject::ControlStart { control }) = p.object_at(a)
                    && control.tag == super::expr::TAG_EXPR
                    && let Some(f) = super::expr::read_formula(control)
                {
                    out.push((top, f, super::expr::read_show(control)));
                }
            }
        }
        Block::Table(t) => {
            for cb in t.rows.iter().flat_map(|r| r.cells.iter()).flat_map(|c| c.blocks.iter()) {
                inline_values(cb, top, depth + 1, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn recalls_skip_the_symbol_alone() {
        assert_eq!(super::recalls(r#"{"value":"r = {{R}}, {{r_1:both}} {{L:symbol}} {{h:value}} {{"#), vec!["R", "r_1", "h"]);
        let modes = super::recalls_with_modes(r#"{{R}} {{L:symbol}} {{h:both}}"#);
        assert_eq!(modes, vec![("R".into(), "value".into()), ("L".into(), "symbol".into()), ("h".into(), "both".into())]);
    }
}
