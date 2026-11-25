//! Data shown as values (docs/calc.md): in an exercise, a datum the reader only ever saw as a
//! value (`2 m` in the statement, never `h = 2 m`) is written as that value where a calculation
//! reads it: `A = h·r` shows `A = (2 m)·r = (2 m)·(0.75 m) = 1.5 m²`.
//!
//! The statement as typed (`A = h·r`) stays the calculation's source: it is kept on the
//! equation (`Math::calc_opts`, keys [`SOURCE`] and [`WRITTEN`]) with a fingerprint of what
//! Strix wrote, and read instead of the equation's text while the equation is still exactly
//! that. So a new variant or a changed datum is worked again from `h`; an equation edited by
//! hand is read as it now stands.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde_json::{Map, Value};
use strix_calc::{RegionKind, RegionResult, Worksheet};
use wordcraft_doc::math::Math;
use wordcraft_doc::{Block, InlineObject, Paragraph};

use super::{MAX_DEPTH, MAX_REGIONS, Parts, equation_full};
use crate::Session;

/// The statement as typed, in `Math::calc_opts`.
const SOURCE: &str = "source";
/// The fingerprint of the equation as Strix wrote it, in `Math::calc_opts`.
const WRITTEN: &str = "written";
/// In `Math::calc_opts`: the calculations made the equation a display equation (its steps on
/// lines); it was inline before and is again when they no longer are.
const WAS_INLINE: &str = "wasInline";
/// The longest statement kept as a source (`calc_opts` is read back up to 2048 bytes); a longer
/// one is shown as typed.
const MAX_SOURCE: usize = 1024;

/// The statement the equation was worked from, when Strix wrote the equation from it and the
/// equation is still exactly as written.
pub(super) fn source(linear: &str, math: &Math) -> Option<String> {
    let opts: Map<String, Value> = serde_json::from_str(math.calc_opts.as_deref()?).ok()?;
    let source = opts.get(SOURCE)?.as_str()?;
    let written = opts.get(WRITTEN)?.as_str()?;
    (source.len() <= MAX_SOURCE && written == fingerprint(&equation_full(linear, math))).then(|| source.to_string())
}

/// FNV-1a of `text` without its spaces (the linear format of a built-up equation drops them), in
/// hexadecimal.
fn fingerprint(text: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.chars().filter(|c| !c.is_whitespace()).collect::<String>().bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// `r` (worked from `text`) with the data `names` written as their values, in the statement and
/// in every line of its working, and the units of its rows found again; `None` when it shows
/// none of them.
pub(super) fn shown_as_values(r: &RegionResult, text: &str, ws: &Worksheet, names: &BTreeSet<String>) -> Option<(RegionResult, Vec<Parts>)> {
    if r.kind == RegionKind::NotCalc || text.len() > MAX_SOURCE || names.is_empty() {
        return None;
    }
    let only = |n: &str| names.contains(n);
    let statement = r.display.clone().unwrap_or_else(|| text.to_string());
    let new_statement = ws.with_values(&statement, &only);
    let new_steps: Option<Vec<Option<String>>> = r.steps.as_ref().map(|rows| rows.iter().map(|row| ws.with_values(row, &only)).collect());
    if new_statement.is_none() && !new_steps.iter().flatten().any(Option::is_some) {
        return None;
    }
    let mut out = r.clone();
    out.display = Some(new_statement.unwrap_or(statement));
    if let (Some(rows), Some(new)) = (out.steps.as_mut(), new_steps) {
        for (row, changed) in rows.iter_mut().zip(new) {
            if let Some(c) = changed {
                *row = c;
            }
        }
    }
    let first = out.display.clone().unwrap_or_default();
    let units = std::iter::once(&first).chain(out.steps.iter().flatten()).map(|row| ws.units_by_part(row)).collect();
    Some((out, units))
}

/// The worksheet and the data shown as values of the exercise holding top-level block `top`.
pub(super) fn zone<'a>(
    top: usize,
    zones: &[Option<usize>],
    sheets: &'a BTreeMap<usize, Worksheet>,
    value_only: &'a BTreeMap<usize, BTreeSet<String>>,
) -> Option<(&'a Worksheet, &'a BTreeSet<String>)> {
    let e = zones.get(top).copied().flatten()?;
    Some((sheets.get(&e)?, value_only.get(&e)?))
}

/// After the equations are written back: each calc equation (`sources`, in reading order) keeps
/// the statement it was worked from when it shows data as values, with the fingerprint of what
/// it now shows; the others drop any they had.
pub(super) fn stamp(s: &mut Session, sources: &[Option<String>]) {
    let mut index = 0usize;
    for block in s.doc.body.iter_mut() {
        // A calculated table's cells are not regions (as the reader and `write_back` skip them).
        if matches!(&**block, Block::Table(t) if t.props.calc) {
            continue;
        }
        let first = index;
        let mut wanted = false;
        each_math(block, 0, &mut index, &mut |i, linear, math| wanted |= stamped(sources.get(i).cloned().flatten(), linear, math) != math.calc_opts);
        if !wanted {
            continue;
        }
        let mut at = first;
        super::each_para(Arc::make_mut(block), 0, &mut |p: &mut Paragraph| {
            let mut touched = false;
            for o in &mut p.objects {
                if let InlineObject::Equation { linear, math, .. } = o {
                    if at >= MAX_REGIONS {
                        return;
                    }
                    let opts = stamped(sources.get(at).cloned().flatten(), linear, math);
                    if opts != math.calc_opts {
                        math.calc_opts = opts;
                        touched = true;
                    }
                    at = at.saturating_add(1);
                }
            }
            if touched {
                p.touch();
            }
        });
    }
}

/// `math`'s calculation options with `source` kept (and the fingerprint of what it shows), or
/// with no source when `source` is `None`.
pub(super) fn stamped(source: Option<String>, linear: &str, math: &Math) -> Option<String> {
    let mut opts: Map<String, Value> = math.calc_opts.as_deref().and_then(|o| serde_json::from_str(o).ok()).unwrap_or_default();
    match source.filter(|s| s.len() <= MAX_SOURCE) {
        Some(src) => {
            opts.insert(SOURCE.into(), Value::String(src));
            opts.insert(WRITTEN.into(), Value::String(fingerprint(&equation_full(linear, math))));
        }
        None => {
            if !opts.contains_key(SOURCE) && !opts.contains_key(WRITTEN) {
                return math.calc_opts.clone();
            }
            opts.remove(SOURCE);
            opts.remove(WRITTEN);
        }
    }
    (!opts.is_empty()).then(|| Value::Object(opts).to_string())
}

/// `new` calculation options (as `calc.options` set them) with the source `old` kept, and its
/// mark of an equation taken out of its line ([`WAS_INLINE`]).
pub(super) fn keep_source(old: Option<&str>, new: Option<String>) -> Option<String> {
    let old: Map<String, Value> = old.and_then(|o| serde_json::from_str(o).ok()).unwrap_or_default();
    let source = old.get(SOURCE).zip(old.get(WRITTEN));
    let inline = old.get(WAS_INLINE);
    if source.is_none() && inline.is_none() {
        return new;
    }
    let mut opts: Map<String, Value> = new.as_deref().and_then(|o| serde_json::from_str(o).ok()).unwrap_or_default();
    if let Some((src, written)) = source {
        opts.insert(SOURCE.into(), src.clone());
        opts.insert(WRITTEN.into(), written.clone());
    }
    if let Some(mark) = inline {
        opts.insert(WAS_INLINE.into(), mark.clone());
    }
    Some(Value::Object(opts).to_string())
}

/// Whether the calculations made this equation a display equation to put its steps on lines
/// (`opts`: its `Math::calc_opts`): it was in its line of text, and goes back there.
pub(super) fn was_inline(opts: Option<&str>) -> bool {
    opts.and_then(|o| serde_json::from_str::<Map<String, Value>>(o).ok()).is_some_and(|m| m.get(WAS_INLINE) == Some(&Value::Bool(true)))
}

/// `opts` with the mark of [`was_inline`] set or taken off.
pub(super) fn set_was_inline(opts: Option<String>, on: bool) -> Option<String> {
    let mut map: Map<String, Value> = opts.as_deref().and_then(|o| serde_json::from_str(o).ok()).unwrap_or_default();
    if on {
        map.insert(WAS_INLINE.into(), Value::Bool(true));
    } else if map.remove(WAS_INLINE).is_none() {
        return opts;
    }
    (!map.is_empty()).then(|| Value::Object(map).to_string())
}

/// Every equation in `b`, in reading order (as `each_para` walks), with its index.
fn each_math(b: &Block, depth: usize, index: &mut usize, f: &mut dyn FnMut(usize, &str, &Math)) {
    if depth > MAX_DEPTH {
        return;
    }
    match b {
        Block::Para(p) => {
            for o in &p.objects {
                if let InlineObject::Equation { linear, math, .. } = o {
                    if *index >= MAX_REGIONS {
                        return;
                    }
                    f(*index, linear, math);
                    *index = index.saturating_add(1);
                }
            }
        }
        Block::Table(t) => {
            for cb in t.rows.iter().flat_map(|r| r.cells.iter()).flat_map(|c| c.blocks.iter()) {
                each_math(cb, depth + 1, index, f);
            }
        }
    }
}

#[cfg(test)]
mod tests;
