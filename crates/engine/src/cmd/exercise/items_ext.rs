//! Extended quiz item helpers (`F1` calculation tolerances, `F2` choice rationales and
//! deterministic per-variant option shuffling).

use std::sync::Arc;

use wordcraft_doc::control::{ContentControl, ControlKind, ControlLock};
use wordcraft_doc::para::OBJ;
use wordcraft_doc::props::{CharProps, Rgb, TextColor};
use wordcraft_doc::{Block, InlineObject, Paragraph};

use crate::Session;

pub const TAG_RATIONALE: &str = "strix:exercise.rationale";
const RATIONALE_EL: &str = "strix:rationale";
const CHOICE_IDX_EL: &str = "strix:choiceIdx";
const MAX_RATIONALE_CHARS: usize = 500;
const MAX_TOLERANCE_CHARS: usize = 64;

pub(super) fn sanitize_tolerance(raw: Option<&str>) -> Option<String> {
    let s = raw?.chars().filter(|&c| !c.is_control() && c != OBJ).take(MAX_TOLERANCE_CHARS).collect::<String>().trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn clean_rationale(raw: &str) -> String {
    raw.chars().filter(|&c| !c.is_control() && c != OBJ).take(MAX_RATIONALE_CHARS).collect::<String>().trim().to_string()
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn unescape_xml(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

pub(super) fn read_rationale(ctl: &ContentControl) -> Option<String> {
    let open = format!("<{RATIONALE_EL}>");
    let close = format!("</{RATIONALE_EL}>");
    let raw = ctl.extra.iter().find_map(|x| x.strip_prefix(&open)?.strip_suffix(&close))?;
    let s = clean_rationale(&unescape_xml(raw));
    (!s.is_empty()).then_some(s)
}

pub(super) fn write_rationale(ctl: &mut ContentControl, rationale: Option<&str>) {
    let open = format!("<{RATIONALE_EL}>");
    ctl.extra.retain(|x| !x.starts_with(&open));
    if let Some(r) = rationale.map(clean_rationale).filter(|s| !s.is_empty()) {
        ctl.extra.push(format!("{open}{}</{RATIONALE_EL}>", escape_xml(&r)));
    }
}

pub(super) fn read_choice_idx(ctl: &ContentControl) -> Option<usize> {
    let open = format!("<{CHOICE_IDX_EL} ");
    let el = ctl.extra.iter().find(|x| x.starts_with(&open))?;
    let (_, rest) = el.split_once("val=\"")?;
    let (num, _) = rest.split_once('"')?;
    num.parse::<usize>().ok()
}

pub(super) fn write_choice_idx(ctl: &mut ContentControl, idx: usize) {
    let open = format!("<{CHOICE_IDX_EL} ");
    ctl.extra.retain(|x| !x.starts_with(&open));
    ctl.extra.push(format!("<{CHOICE_IDX_EL} val=\"{idx}\"/>"));
}

/// Formats the acceptable interval suffix `(±tol: lo – hi unit)` for a `Mode::Calc` teacher result line.
pub(super) fn tolerance_suffix(found: &str, tol: &str, opts: &strix_calc::Options, unit_of: fn(&str) -> Option<&str>) -> Option<String> {
    let tol = tol.trim();
    if tol.is_empty() || found.starts_with('⚠') {
        return None;
    }
    let rhs = found.rsplit_once(" = ").map_or(found, |(_, v)| v).trim();
    let unit = unit_of(rhs);
    let ws = strix_calc::Worksheet::new(*opts);
    let strix_calc::Value::Scalar(q) = ws.eval_expr(rhs).ok()? else { return None };
    let delta_si = if let Some(pct_str) = tol.strip_suffix('%') {
        let pct: f64 = pct_str.trim().replace(',', ".").parse().ok()?;
        if !pct.is_finite() || pct < 0.0 {
            return None;
        }
        q.value.abs() * (pct / 100.0)
    } else {
        let strix_calc::Value::Scalar(dq) = ws.eval_expr(tol).ok()? else { return None };
        if dq.dim != q.dim {
            return None;
        }
        dq.value.abs()
    };
    let q_lo = strix_calc::Quantity::new(q.value - delta_si, q.dim);
    let q_hi = strix_calc::Quantity::new(q.value + delta_si, q.dim);
    let lo_str = match unit {
        Some(u) => strix_calc::format_value_in(&strix_calc::Value::Scalar(q_lo), u, opts).ok()?.1,
        None => strix_calc::format_value(&strix_calc::Value::Scalar(q_lo), opts),
    };
    let hi_str = match unit {
        Some(u) => strix_calc::format_value_in(&strix_calc::Value::Scalar(q_hi), u, opts).ok()?.1,
        None => strix_calc::format_value(&strix_calc::Value::Scalar(q_hi), opts),
    };
    let lo_num = match unit {
        Some(u) => lo_str.strip_suffix(u).map(str::trim).unwrap_or(lo_str.as_str()),
        None => lo_str.as_str(),
    };
    Some(format!(" (±{tol}: {lo_num} – {hi_str})"))
}

/// Synchronizes the inline `TAG_RATIONALE` diagnostic note at the end of a choice paragraph `p`.
pub(super) fn sync_rationale_para(
    p: &mut Paragraph,
    rationale: Option<&str>,
    shown: bool,
    span_fn: fn(&Paragraph, &str) -> Option<(usize, usize)>,
    trailing_ends_fn: fn(&Paragraph) -> usize,
) -> bool {
    let clean = rationale.map(clean_rationale).filter(|s| !s.is_empty());
    match (span_fn(p, TAG_RATIONALE), clean) {
        (None, None) => false,
        (Some((a, b)), None) => {
            if p.delete(a, b + OBJ.len_utf8()).is_ok() {
                p.touch();
                true
            } else {
                false
            }
        }
        (Some((a, b)), Some(r)) => {
            let text = format!(" — {r}");
            let want_hidden = (!shown).then_some(true);
            let cur_text = p.text.get(a + OBJ.len_utf8()..b);
            let cur_hidden = p.props_of_char(a + OBJ.len_utf8()).hidden;
            if cur_text == Some(text.as_str()) && cur_hidden == want_hidden {
                return false;
            }
            let props = rationale_props(p, a, shown);
            if b > a + OBJ.len_utf8() && p.delete(a + OBJ.len_utf8(), b).is_err() {
                return false;
            }
            let _ = p.insert_text(a + OBJ.len_utf8(), &text, &props);
            let _ = p.format(a, a + OBJ.len_utf8() + text.len() + OBJ.len_utf8(), &|cp| cp.hidden = want_hidden);
            p.touch();
            true
        }
        (None, Some(r)) => {
            let at = p.text.len().saturating_sub(trailing_ends_fn(p) * OBJ.len_utf8());
            let props = rationale_props(p, at.saturating_sub(1), shown);
            let mut ctl = ContentControl::new(ControlKind::RichText);
            ctl.tag = TAG_RATIONALE.into();
            ctl.title = "Rationale".into();
            ctl.block = false;
            ctl.lock = ControlLock::SdtContentLocked;
            let text = format!(" — {r}");
            if p.insert_object(at, InlineObject::ControlStart { control: Box::new(ctl) }, &props).is_err() {
                return false;
            }
            let inner = at + OBJ.len_utf8();
            let _ = p.insert_text(inner, &text, &props);
            let _ = p.insert_object(inner + text.len(), InlineObject::ControlEnd, &props);
            p.touch();
            true
        }
    }
}

fn rationale_props(p: &Paragraph, near: usize, shown: bool) -> CharProps {
    let mut base = if p.text.is_empty() { CharProps::default() } else { p.props_of_char(near.min(p.text.len().saturating_sub(1))).clone() };
    base.italic = Some(true);
    base.color = Some(TextColor::Rgb(Rgb(0x5A, 0x62, 0x70)));
    base.hidden = (!shown).then_some(true);
    base
}

fn fnv1a(ex: usize, q: usize, variant: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in ex.to_le_bytes().into_iter().chain(q.to_le_bytes()).chain(variant.as_bytes().iter().copied()) {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Deterministic permutation of `0..count` for `(ex, q, variant_name)`.
/// Variant 0 (the base variant, e.g. `A`) stays in canonical author order `0..count`; subsequent
/// variants (`B..Z`) receive a deterministic Fisher-Yates permutation guaranteed to differ from `0..count`.
pub(super) fn choice_permutation(
    ex: usize,
    q: usize,
    variant_name: &str,
    variant_idx: usize,
    _first_variant: &str,
    count: usize,
    shuffle: bool,
) -> Vec<usize> {
    let mut perm: Vec<usize> = (0..count).collect();
    if !shuffle || count <= 1 || variant_idx == 0 {
        return perm;
    }
    let mut rng = fnv1a(ex, q, variant_name);
    for i in (1..count).rev() {
        let j = (splitmix64(&mut rng) as usize) % (i + 1);
        perm.swap(i, j);
    }
    if perm.iter().copied().eq(0..count) {
        let shift = 1 + (variant_idx - 1) % (count - 1);
        perm.rotate_left(shift);
    }
    perm
}

/// Ensures every `TAG_BOX` in `a..=b` has a canonical `<strix:choiceIdx>` and reorders the
/// `STYLE_CHOICE` paragraphs inside `a..=b` to match the active variant's deterministic order
/// (or canonical `0..n` order when `shuffle == false`).
#[allow(clippy::too_many_arguments)]
pub(super) fn reorder_choices_for_variant(
    s: &mut Session,
    (a, b): (usize, usize),
    ex: usize,
    q: usize,
    variant_name: &str,
    variant_idx: usize,
    first_variant: &str,
    shuffle: bool,
    style_choice: &str,
    tag_answer: &str,
    tag_box: &str,
    span_fn: fn(&Paragraph, &str) -> Option<(usize, usize)>,
    outer_ends_fn: fn(&Session, usize, usize) -> usize,
) -> bool {
    let indices: Vec<usize> =
        (a..=b).filter(|&i| s.doc.body.get(i).and_then(|bl| bl.as_para()).is_some_and(|p| p.props.style.as_deref() == Some(style_choice))).collect();
    // Only contiguous pure-choice answer ranges are reordered.
    if indices.len() <= 1 || indices.len() != (b - a + 1) {
        return false;
    }
    let mut changed = ensure_canonical_indices(s, &indices, tag_box, span_fn);
    let current_ids: Vec<usize> = indices
        .iter()
        .enumerate()
        .map(|(fallback, &i)| {
            s.doc
                .body
                .get(i)
                .and_then(|bl| bl.as_para())
                .and_then(|p| {
                    let (at, _) = span_fn(p, tag_box)?;
                    match p.object_at(at) {
                        Some(InlineObject::ControlStart { control }) => read_choice_idx(control),
                        _ => None,
                    }
                })
                .unwrap_or(fallback)
        })
        .collect();
    let mut canonical_sorted = current_ids.clone();
    canonical_sorted.sort_unstable();
    canonical_sorted.dedup();
    if canonical_sorted.len() != indices.len() {
        // Deduplicate corrupted indices if any.
        assign_sequential_indices(s, &indices, tag_box, span_fn);
        return true;
    }
    let perm = choice_permutation(ex, q, variant_name, variant_idx, first_variant, canonical_sorted.len(), shuffle);
    let desired_ids: Vec<usize> = perm.into_iter().filter_map(|p_i| canonical_sorted.get(p_i).copied()).collect();
    if current_ids == desired_ids {
        return changed;
    }
    let outer = outer_ends_fn(s, a, b);
    let Some(answer_ctl) = take_leading_control(s, a, tag_answer) else { return changed };
    if strip_trailing_ends(s, b, 1 + outer).is_err() {
        put_leading_control(s, a, answer_ctl);
        return changed;
    }
    let old_blocks: Vec<(usize, Arc<Block>)> = current_ids.into_iter().zip(indices.iter().filter_map(|&i| s.doc.body.get(i).cloned())).collect();
    let mut reordered: Vec<Arc<Block>> = Vec::with_capacity(desired_ids.len());
    for want_id in &desired_ids {
        if let Some((_, bl)) = old_blocks.iter().find(|(cid, _)| cid == want_id) {
            reordered.push(bl.clone());
        }
    }
    if reordered.len() == indices.len() {
        s.doc.body.splice(a..=b, reordered);
        put_leading_control(s, a, answer_ctl);
        append_trailing_ends(s, b, 1 + outer);
        changed = true;
    } else {
        put_leading_control(s, a, answer_ctl);
        append_trailing_ends(s, b, 1 + outer);
    }
    changed
}

fn ensure_canonical_indices(s: &mut Session, indices: &[usize], tag_box: &str, span_fn: fn(&Paragraph, &str) -> Option<(usize, usize)>) -> bool {
    let any_missing = indices.iter().any(|&i| {
        s.doc.body.get(i).and_then(|bl| bl.as_para()).is_some_and(|p| {
            span_fn(p, tag_box)
                .and_then(|(at, _)| match p.object_at(at) {
                    Some(InlineObject::ControlStart { control }) => read_choice_idx(control),
                    _ => None,
                })
                .is_none()
        })
    });
    if !any_missing {
        return false;
    }
    assign_sequential_indices(s, indices, tag_box, span_fn);
    true
}

fn assign_sequential_indices(s: &mut Session, indices: &[usize], tag_box: &str, span_fn: fn(&Paragraph, &str) -> Option<(usize, usize)>) {
    for (k, &i) in indices.iter().enumerate() {
        let Some(block) = s.doc.body.get_mut(i) else { continue };
        let Block::Para(p) = Arc::make_mut(block) else { continue };
        if let Some((at, _)) = span_fn(p, tag_box)
            && let Some(InlineObject::ControlStart { control }) = p.object_at_mut(at)
        {
            write_choice_idx(control, k);
            p.touch();
        }
    }
}

fn take_leading_control(s: &mut Session, block_idx: usize, tag: &str) -> Option<ContentControl> {
    let block = s.doc.body.get_mut(block_idx)?;
    let Block::Para(p) = Arc::make_mut(block) else { return None };
    let (at, ctl) = p.objects_with_offsets().find_map(|(off, o)| match o {
        InlineObject::ControlStart { control } if control.tag == tag => Some((off, (**control).clone())),
        _ => None,
    })?;
    if p.delete(at, at + OBJ.len_utf8()).is_ok() {
        p.touch();
        Some(ctl)
    } else {
        None
    }
}

fn put_leading_control(s: &mut Session, block_idx: usize, ctl: ContentControl) {
    if let Some(block) = s.doc.body.get_mut(block_idx)
        && let Block::Para(p) = Arc::make_mut(block)
    {
        let props = if p.text.is_empty() { CharProps::default() } else { p.props_of_char(0).clone() };
        let _ = p.insert_object(0, InlineObject::ControlStart { control: Box::new(ctl) }, &props);
        p.touch();
    }
}

fn strip_trailing_ends(s: &mut Session, block_idx: usize, count: usize) -> Result<(), ()> {
    let block = s.doc.body.get_mut(block_idx).ok_or(())?;
    let Block::Para(p) = Arc::make_mut(block) else { return Err(()) };
    for _ in 0..count {
        if super::take_last_end(p).is_err() {
            return Err(());
        }
    }
    Ok(())
}

fn append_trailing_ends(s: &mut Session, block_idx: usize, count: usize) {
    if let Some(block) = s.doc.body.get_mut(block_idx)
        && let Block::Para(p) = Arc::make_mut(block)
    {
        let props = CharProps::default();
        for _ in 0..count {
            let end = p.text.len();
            let _ = p.insert_object(end, InlineObject::ControlEnd, &props);
        }
        p.touch();
    }
}
