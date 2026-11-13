//! Controlled variant generator (`F3` — `exercise.generateVariants`).
//!
//! Generates `1..=26` consistent variants (`A..Z`) from discrete lists or `[min, max, step]`
//! intervals on initial data items while enforcing zero `strix-calc` / `strix-cad` / `strix:expr`
//! errors and user constraints (`maxDecimals`, `positiveResults`, `distinctFinalResults`, `extra`
//! comparison predicates) within a strict budget (`MAX_CANDIDATE_EVALS = 5_000`).

use std::collections::BTreeSet;

use serde_json::{Value, json};
use wordcraft_doc::InlineObject;

use super::{DataItem, MAX_DATA, MAX_VARIANTS, clip, scan, target, with_data};
use crate::{CmdError, CmdResult, Session, p};

pub const MAX_CANDIDATE_EVALS: usize = 5_000;
const MAX_DOMAIN_VALUES: usize = 500;
const MAX_EXTRA_PREDICATES: usize = 32;

#[derive(Debug, Clone, Default)]
struct Constraints {
    max_decimals: Option<u8>,
    positive_results: bool,
    distinct_final_results: bool,
    extra: Vec<String>,
}

#[derive(Debug, Clone)]
struct DomainSpec {
    symbol: String,
    unit: Option<String>,
    description: Option<String>,
    values: Vec<String>,
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

fn parse_constraints(v: &Value) -> Constraints {
    let sub = v.get("constraints").unwrap_or(v);
    let max_decimals = p::u64(sub, "maxDecimals").or_else(|| p::u64(v, "maxDecimals")).map(|d| u8::try_from(d.min(12)).unwrap_or(12));
    let positive_results = p::bool(sub, "positiveResults").or_else(|| p::bool(v, "positiveResults")).unwrap_or(false);
    let distinct_final_results = p::bool(sub, "distinctFinalResults").or_else(|| p::bool(v, "distinctFinalResults")).unwrap_or(false);
    let extra_val = sub.get("extra").or_else(|| v.get("extra"));
    let extra = match extra_val {
        Some(Value::Array(arr)) => {
            arr.iter().take(MAX_EXTRA_PREDICATES).filter_map(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect()
        }
        Some(Value::String(s)) => {
            s.split([';', '\n']).take(MAX_EXTRA_PREDICATES).map(str::trim).filter(|p| !p.is_empty()).map(str::to_string).collect()
        }
        _ => Vec::new(),
    };
    Constraints { max_decimals, positive_results, distinct_final_results, extra }
}

fn format_domain_number(val: f64, step: f64) -> String {
    if !val.is_finite() {
        return "0".to_string();
    }
    let clean = if val.abs() < 1e-12 { 0.0 } else { val };
    let decimals = step_decimals(step);
    let formatted = format!("{clean:.decimals$}");
    if decimals == 0 {
        formatted
    } else {
        let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
        if trimmed.is_empty() || trimmed == "-" { "0".to_string() } else { trimmed.to_string() }
    }
}

fn step_decimals(step: f64) -> usize {
    let s = step.abs();
    for d in 0..=8usize {
        let pow = 10_f64.powi(i32::try_from(d).unwrap_or(8));
        if ((s * pow) - (s * pow).round()).abs() < 1e-7 {
            return d;
        }
    }
    6
}

fn value_as_f64(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim().replace(',', ".").parse::<f64>().ok())).filter(|x| x.is_finite())
}

fn value_as_domain_entry(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => {
            let c = clip(s);
            (!c.is_empty()).then_some(c)
        }
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn parse_one_domain(symbol: &str, spec: &Value) -> Result<DomainSpec, CmdError> {
    let sym = clip(symbol);
    if sym.is_empty() {
        return Err(CmdError::Params("domains: empty symbol".into()));
    }
    match spec {
        Value::Array(arr) => {
            let values: Vec<String> = arr.iter().take(MAX_DOMAIN_VALUES).filter_map(value_as_domain_entry).collect();
            if values.is_empty() {
                return Err(CmdError::Params(format!("domains.{sym}: empty value list")));
            }
            Ok(DomainSpec { symbol: sym, unit: None, description: None, values })
        }
        Value::Object(map) => {
            let unit = map.get("unit").and_then(Value::as_str).map(clip);
            let description = map.get("description").and_then(Value::as_str).map(clip);
            if let Some(arr) = map.get("values").and_then(Value::as_array) {
                let values: Vec<String> = arr.iter().take(MAX_DOMAIN_VALUES).filter_map(value_as_domain_entry).collect();
                if values.is_empty() {
                    return Err(CmdError::Params(format!("domains.{sym}: empty values list")));
                }
                return Ok(DomainSpec { symbol: sym, unit, description, values });
            }
            let min = map.get("min").and_then(value_as_f64).ok_or_else(|| CmdError::Params(format!("domains.{sym}: expected min/max or values")))?;
            let max = map.get("max").and_then(value_as_f64).ok_or_else(|| CmdError::Params(format!("domains.{sym}: expected max")))?;
            if max < min {
                return Err(CmdError::Params(format!("domains.{sym}: max must be >= min")));
            }
            let raw_step = map.get("step").and_then(value_as_f64).unwrap_or(1.0);
            let step = if raw_step > 1e-12 { raw_step } else { 1.0 };
            let mut values = Vec::new();
            let mut k = 0usize;
            while k < MAX_DOMAIN_VALUES {
                let cur = min + (k as f64) * step;
                if cur > max + step * 1e-7 {
                    break;
                }
                values.push(format_domain_number(cur.min(max), step));
                if (max - min).abs() <= 1e-12 {
                    break;
                }
                k += 1;
            }
            if values.is_empty() {
                values.push(format_domain_number(min, step));
            }
            Ok(DomainSpec { symbol: sym, unit, description, values })
        }
        _ => {
            let single =
                value_as_domain_entry(spec).ok_or_else(|| CmdError::Params(format!("domains.{sym}: expected array or {{min, max, step}}")))?;
            Ok(DomainSpec { symbol: sym, unit: None, description: None, values: vec![single] })
        }
    }
}

fn build_item_domains(existing: &[DataItem], v: &Value) -> Result<Vec<(DataItem, Vec<String>)>, CmdError> {
    let mut parsed: Vec<DomainSpec> = Vec::new();
    match v.get("domains") {
        Some(Value::Object(obj)) => {
            for (k, spec) in obj.iter().take(MAX_DATA) {
                parsed.push(parse_one_domain(k, spec)?);
            }
        }
        Some(Value::Array(arr)) => {
            for spec in arr.iter().take(MAX_DATA) {
                let sym = spec.get("symbol").and_then(Value::as_str).ok_or_else(|| CmdError::Params("domains[]: missing symbol".into()))?;
                parsed.push(parse_one_domain(sym, spec)?);
            }
        }
        _ => {}
    }
    let mut out: Vec<(DataItem, Vec<String>)> = Vec::new();
    for item in existing {
        if item.symbol.is_empty() {
            continue;
        }
        if let Some(pos) = parsed.iter().position(|d| d.symbol == item.symbol) {
            let dom = parsed.remove(pos);
            let mut updated = item.clone();
            if let Some(u) = dom.unit {
                updated.unit = u;
            }
            if let Some(desc) = dom.description {
                updated.description = desc;
            }
            out.push((updated, dom.values));
        } else {
            let mut vals: Vec<String> = item.values.iter().map(|s| clip(s)).filter(|s| !s.is_empty()).collect();
            vals.dedup();
            if vals.is_empty() {
                vals.push("1".into());
            }
            out.push((item.clone(), vals));
        }
    }
    for dom in parsed {
        if out.len() >= MAX_DATA {
            break;
        }
        let item =
            DataItem { symbol: dom.symbol, description: dom.description.unwrap_or_default(), unit: dom.unit.unwrap_or_default(), values: Vec::new() };
        out.push((item, dom.values));
    }
    if out.is_empty() {
        return Err(CmdError::Params("no exercise data items or domains to generate variants from".into()));
    }
    Ok(out)
}

fn variant_names(v: &Value, count: usize) -> Vec<String> {
    if let Some(arr) = v.get("variants").and_then(Value::as_array) {
        let custom: Vec<String> = arr.iter().take(MAX_VARIANTS).filter_map(Value::as_str).map(clip).filter(|s| !s.is_empty()).collect();
        if !custom.is_empty() {
            return custom;
        }
    }
    (0..count).map(|i| char::from(b'A' + u8::try_from(i.min(25)).unwrap_or(25)).to_string()).collect()
}

/// Checks extra comparison predicates (`L > b`, `M <= 50 kN*m`) in `ws`.
fn check_extra_predicate(ws: &strix_calc::Worksheet, pred: &str) -> Result<(), String> {
    let ops = ["<=", ">=", "!=", "==", "≤", "≥", "≠", "<", ">", "="];
    let Some((op, lhs, rhs)) = ops.iter().find_map(|&op| pred.split_once(op).map(|(l, r)| (op, l.trim(), r.trim()))) else {
        return Err(format!("invalid predicate '{pred}'"));
    };
    let lv = ws.eval_expr(lhs).map_err(|e| format!("{pred}: {e}"))?;
    let rv = ws.eval_expr(rhs).map_err(|e| format!("{pred}: {e}"))?;
    let (strix_calc::Value::Scalar(lq), strix_calc::Value::Scalar(rq)) = (lv, rv) else {
        return Err(format!("{pred}: vector comparison unsupported"));
    };
    if lq.dim != rq.dim {
        return Err(format!("{pred}: incompatible dimensions"));
    }
    let diff = lq.value - rq.value;
    let ok = match op {
        "<=" | "≤" => diff <= 1e-12,
        ">=" | "≥" => diff >= -1e-12,
        "!=" | "≠" => diff.abs() > 1e-9,
        "==" | "=" => diff.abs() <= 1e-9,
        "<" => diff < -1e-12,
        ">" => diff > 1e-12,
        _ => false,
    };
    if ok { Ok(()) } else { Err(format!("constraint failed: {pred}")) }
}

fn check_max_decimals(ws: &strix_calc::Worksheet, q: strix_calc::Quantity, shown: &str, max_d: u8) -> bool {
    if !q.value.is_finite() {
        return false;
    }
    let unit_str = extract_trailing_unit(shown);
    let mag = match unit_str {
        Some(u) => match ws.eval_expr(&format!("1 {u}")) {
            Ok(strix_calc::Value::Scalar(uq)) if uq.dim == q.dim && uq.value.abs() > 0.0 => q.value / uq.value,
            _ => q.value,
        },
        None => q.value,
    };
    let pow = 10_f64.powi(i32::from(max_d));
    let scaled = mag * pow;
    scaled.is_finite() && (scaled - scaled.round()).abs() <= 1e-6
}

fn extract_trailing_unit(shown: &str) -> Option<&str> {
    let rhs = shown.rsplit_once(" = ").map_or(shown, |(_, v)| v).trim();
    let idx = rhs.find(|c: char| c.is_alphabetic() && c != 'e' && c != 'E')?;
    let u = rhs.get(idx..)?.trim();
    (!u.is_empty()).then_some(u)
}

fn check_choice_collisions(s: &Session, layout: &super::Layout, ex: usize, ws: &strix_calc::Worksheet) -> Result<(), String> {
    for (q_idx, it) in super::items::items(s, layout, ex).into_iter().enumerate() {
        if it.settings.mode != super::items::Mode::Choice {
            continue;
        }
        let Some((a, b)) = it.answer else { continue };
        let mut seen = BTreeSet::new();
        for i in a..=b {
            let Some(p) = s.doc.body.get(i).and_then(|bl| bl.as_para()) else { continue };
            if p.props.style.as_deref() != Some(super::items::STYLE_CHOICE) {
                continue;
            }
            let rendered = super::expr::render_choice_with_sheet(p, ws);
            if !rendered.is_empty() && !seen.insert(rendered.clone()) {
                return Err(format!("question {} has duplicate option '{rendered}'", q_idx + 1));
            }
        }
    }
    Ok(())
}

fn final_signature(eval: &crate::cmd::calc::VariantEval) -> String {
    if !eval.answers.is_empty() {
        eval.answers
            .iter()
            .map(|(qn, list)| {
                let last = list.last().map(|(s, _)| s.as_str()).unwrap_or("");
                format!("{qn}:{last}")
            })
            .collect::<Vec<_>>()
            .join("|")
    } else {
        eval.scalars.iter().map(|(n, _, s)| format!("{n}={s}")).collect::<Vec<_>>().join("|")
    }
}

fn validate_candidate(
    s: &Session,
    layout: &super::Layout,
    ex: usize,
    stmts: &[String],
    c: &Constraints,
    seen_finals: &BTreeSet<String>,
) -> Result<String, String> {
    let eval = crate::cmd::calc::eval_exercise_variant(s, ex, stmts);
    if let Some((_, err)) = eval.errors.first() {
        return Err(err.clone());
    }
    check_choice_collisions(s, layout, ex, &eval.ws)?;
    if c.positive_results {
        for (name, q, _) in &eval.scalars {
            if q.value <= 1e-12 {
                return Err(format!("{name} is not positive"));
            }
        }
        for list in eval.answers.values() {
            for (txt, q_opt) in list {
                if q_opt.is_some_and(|q| q.value <= 1e-12) {
                    return Err(format!("{txt} is not positive"));
                }
            }
        }
    }
    if let Some(max_d) = c.max_decimals {
        let targets: Vec<(strix_calc::Quantity, &str)> = if !eval.answers.is_empty() {
            eval.answers.values().filter_map(|l| l.last()).filter_map(|(txt, q)| q.map(|qq| (qq, txt.as_str()))).collect()
        } else {
            eval.scalars.iter().map(|(_, q, shown)| (*q, shown.as_str())).collect()
        };
        for (q, shown) in targets {
            if !check_max_decimals(&eval.ws, q, shown, max_d) {
                return Err(format!("'{shown}' exceeds maxDecimals={max_d}"));
            }
        }
    }
    for pred in &c.extra {
        check_extra_predicate(&eval.ws, pred)?;
    }
    let sig = final_signature(&eval);
    if c.distinct_final_results && !sig.is_empty() && seen_finals.contains(&sig) {
        return Err(format!("duplicate final result ({sig})"));
    }
    Ok(sig)
}

fn candidate_order(lengths: &[usize], seed: u64) -> Vec<Vec<usize>> {
    let total = lengths.iter().try_fold(1usize, |acc, &n| acc.checked_mul(n.max(1)));
    let mut rng = seed ^ 0x517c_c1b7_2722_0a95;
    if let Some(prod) = total
        && prod <= MAX_CANDIDATE_EVALS
    {
        let mut all: Vec<Vec<usize>> = Vec::with_capacity(prod);
        for flat in 0..prod {
            let mut rem = flat;
            let mut tuple = vec![0usize; lengths.len()];
            for (d, &len) in lengths.iter().enumerate().rev() {
                let l = len.max(1);
                tuple[d] = rem % l;
                rem /= l;
            }
            all.push(tuple);
        }
        for i in (1..all.len()).rev() {
            let j = (splitmix64(&mut rng) as usize) % (i + 1);
            all.swap(i, j);
        }
        return all;
    }
    let mut out: Vec<Vec<usize>> = Vec::with_capacity(MAX_CANDIDATE_EVALS);
    let mut seen: BTreeSet<Vec<usize>> = BTreeSet::new();
    let max_draws = MAX_CANDIDATE_EVALS.saturating_mul(4);
    for _ in 0..max_draws {
        if out.len() >= MAX_CANDIDATE_EVALS {
            break;
        }
        let tuple: Vec<usize> = lengths.iter().map(|&len| if len <= 1 { 0 } else { (splitmix64(&mut rng) as usize) % len }).collect();
        if seen.insert(tuple.clone()) {
            out.push(tuple);
        }
    }
    out
}

/// `exercise.generateVariants`: generates controlled multi-variant initial data for an exercise.
pub(crate) fn command(s: &mut Session, v: &Value) -> CmdResult {
    let ex = target(s, v)?;
    let layout = scan(s);
    let e = layout.exercises.get(ex).ok_or_else(|| CmdError::Params("no such exercise".into()))?;
    let default_count = v.get("variants").and_then(Value::as_array).map(|a| a.len().clamp(1, MAX_VARIANTS)).unwrap_or(4);
    let raw_count = p::u64(v, "count").map(|n| usize::try_from(n).unwrap_or(MAX_VARIANTS)).unwrap_or(default_count);
    if raw_count == 0 || raw_count > MAX_VARIANTS {
        return Err(CmdError::Params(format!("count: 1..={MAX_VARIANTS}")));
    }
    let names = variant_names(v, raw_count);
    let count = names.len().clamp(1, MAX_VARIANTS);
    let seed = p::u64(v, "seed").unwrap_or(42);
    let constraints = parse_constraints(v);
    let item_domains = build_item_domains(&e.data.items, v)?;
    let lengths: Vec<usize> = item_domains.iter().map(|(_, vals)| vals.len()).collect();
    let candidates = candidate_order(&lengths, seed);

    let mut chosen: Vec<Vec<String>> = Vec::with_capacity(count);
    let mut seen_finals: BTreeSet<String> = BTreeSet::new();
    let mut evaluated = 0usize;
    let mut last_rejection = String::from("domain exhausted");

    for tuple in candidates.into_iter().take(MAX_CANDIDATE_EVALS) {
        evaluated += 1;
        let values_vec: Vec<String> = tuple.iter().enumerate().map(|(idx, &vi)| item_domains[idx].1.get(vi).cloned().unwrap_or_default()).collect();
        let stmts: Vec<String> =
            item_domains.iter().zip(&values_vec).map(|((it, _), val)| format!("{} = {val} {}", it.symbol, it.unit).trim().to_string()).collect();
        match validate_candidate(s, &layout, ex, &stmts, &constraints, &seen_finals) {
            Ok(sig) => {
                if !sig.is_empty() {
                    seen_finals.insert(sig);
                }
                chosen.push(values_vec);
                if chosen.len() >= count {
                    break;
                }
            }
            Err(why) => last_rejection = why,
        }
    }

    if chosen.len() < count {
        return Err(CmdError::Failed(format!(
            "generated only {}/{count} valid variants after {evaluated} evaluations (last rejection: {last_rejection})",
            chosen.len()
        )));
    }

    let mut new_items: Vec<DataItem> = item_domains
        .into_iter()
        .enumerate()
        .map(|(item_idx, (mut it, _))| {
            it.values = chosen.iter().map(|row| row.get(item_idx).cloned().unwrap_or_default()).collect();
            it
        })
        .collect();
    let final_names: Vec<String> = names.into_iter().take(count).collect();

    // Ensure the exercise exposes `strix:exercise.data` even if previously empty.
    let _ = InlineObject::ControlEnd;
    with_data(s, ex, |d| {
        d.items.clear();
        d.items.append(&mut new_items);
        d.variants.clone_from(&final_names);
        d.active = 0;
    })?;

    let updated_layout = scan(s);
    let out_data = updated_layout.exercises.get(ex).map(|x| x.data.clone()).unwrap_or_default();
    Ok(json!({
        "exercise": ex + 1,
        "generated": count,
        "evaluated": evaluated,
        "variants": final_names,
        "data": out_data,
    }))
}
