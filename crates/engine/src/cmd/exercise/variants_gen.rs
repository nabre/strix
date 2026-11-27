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

/// `values` without repeats, the first of each kept in its place: a value listed twice is one
/// value, so no two variants are given the same data.
fn distinct(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values.into_iter().filter(|v| seen.insert(v.clone())).collect()
}

fn parse_one_domain(symbol: &str, spec: &Value) -> Result<DomainSpec, CmdError> {
    let sym = clip(symbol);
    if sym.is_empty() {
        return Err(CmdError::Params("domains: empty symbol".into()));
    }
    match spec {
        Value::Array(arr) => {
            let values = distinct(arr.iter().take(MAX_DOMAIN_VALUES).filter_map(value_as_domain_entry).collect());
            if values.is_empty() {
                return Err(CmdError::Params(format!("domains.{sym}: empty value list")));
            }
            Ok(DomainSpec { symbol: sym, unit: None, description: None, values })
        }
        Value::Object(map) => {
            let unit = map.get("unit").and_then(Value::as_str).map(clip);
            let description = map.get("description").and_then(Value::as_str).map(clip);
            if let Some(arr) = map.get("values").and_then(Value::as_array) {
                let values = distinct(arr.iter().take(MAX_DOMAIN_VALUES).filter_map(value_as_domain_entry).collect());
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
            // A step finer than a value is written with gives the same text again.
            Ok(DomainSpec { symbol: sym, unit, description, values: distinct(values) })
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
            // The generated values share one unit: variant A's, unless the domain names one.
            updated.units.clear();
            if let Some(desc) = dom.description {
                updated.description = desc;
            }
            out.push((updated, dom.values));
        } else {
            // Values in different units of one quantity (`450 cm`, `4.5 m`) are drawn with their
            // unit: the kept item's `units` then holds the unit of each domain value, by index,
            // never split again from text.
            let mut kept = item.clone();
            kept.units.clear();
            let mut pairs: Vec<(String, String)> = Vec::new();
            for (k, v) in item.values.iter().enumerate() {
                let pair = (clip(v), item.unit_in(k).to_string());
                if !pair.0.is_empty() && !pairs.contains(&pair) {
                    pairs.push(pair);
                }
            }
            if pairs.is_empty() {
                pairs.push(("1".into(), item.unit.clone()));
            }
            let (vals, units): (Vec<String>, Vec<String>) = pairs.into_iter().unzip();
            if !item.units.is_empty() {
                kept.units = units;
            }
            out.push((kept, vals));
        }
    }
    for dom in parsed {
        if out.len() >= MAX_DATA {
            break;
        }
        let item = DataItem {
            symbol: dom.symbol,
            description: dom.description.unwrap_or_default(),
            unit: dom.unit.unwrap_or_default(),
            ..DataItem::default()
        };
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

    let mut chosen: Vec<Vec<(String, String)>> = Vec::with_capacity(count);
    let mut seen_finals: BTreeSet<String> = BTreeSet::new();
    let mut evaluated = 0usize;
    let mut last_rejection = String::from("domain exhausted");

    for tuple in candidates.into_iter().take(MAX_CANDIDATE_EVALS) {
        evaluated += 1;
        // Each datum's value and unit (a datum given in several units has one per domain value).
        let values_vec: Vec<(String, String)> = tuple
            .iter()
            .zip(&item_domains)
            .map(|(&vi, (it, vals))| (vals.get(vi).cloned().unwrap_or_default(), it.unit_in(vi).to_string()))
            .collect();
        let stmts: Vec<String> =
            item_domains.iter().zip(&values_vec).map(|((it, _), (val, unit))| format!("{} = {val} {unit}", it.symbol).trim().to_string()).collect();
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

    let mut generated = item_domains.into_iter().enumerate().map(|(item_idx, (mut it, _))| {
        let (values, units): (Vec<String>, Vec<String>) = chosen.iter().map(|row| row.get(item_idx).cloned().unwrap_or_default()).unzip();
        it.values = values;
        // A datum drawn with its units gives each variant the unit of its value, exactly as validated.
        if !it.units.is_empty() {
            it.units = units;
        }
        it
    });
    // A datum without a symbol (a line of the data part that is no equation yet) is nothing to
    // vary: it stays where it was, with its active value in every variant. The data with a
    // symbol come in the order they had, the new ones after them (`build_item_domains`).
    let mut new_items: Vec<DataItem> = Vec::with_capacity(e.data.items.len());
    for old in &e.data.items {
        if old.symbol.is_empty() {
            let value = old.values.get(e.data.active).cloned().unwrap_or_default();
            new_items.push(DataItem { values: vec![value; count], ..old.clone() });
        } else if let Some(it) = generated.next() {
            new_items.push(it);
        }
    }
    new_items.extend(generated);
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

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wordcraft_doc::Document;

    use crate::Session;

    fn session() -> Session {
        let mut s = Session::new(Document::new());
        s.run("document.setText", &json!({"text": ""})).unwrap();
        std::sync::Arc::make_mut(&mut s.doc.styles).default_chr.lang = Some("it-IT".into());
        s.run("exercise.insert", &json!({"title": "Rettangolo", "questions": ["Area"]})).unwrap();
        s
    }

    /// The values of each datum across the variants generated.
    fn rows(r: &serde_json::Value) -> Vec<Vec<String>> {
        let items = r["data"]["items"].as_array().unwrap();
        let n = r["generated"].as_u64().unwrap() as usize;
        (0..n).map(|v| items.iter().map(|i| i["values"][v].as_str().unwrap().to_string()).collect()).collect()
    }

    /// A value listed twice in a domain is one value: no two variants get the same data, and a
    /// domain with fewer distinct values than variants asked is refused, as a short one is.
    #[test]
    fn a_value_listed_twice_gives_no_twin_variants() {
        let mut s = session();
        let r = s.run("exercise.generateVariants", &json!({"count": 3, "domains": {"L": [2, 2, 3, 3, 4, 4, 2, "3"], "b": [5, 5]}})).unwrap();
        let mut got = rows(&r);
        got.sort();
        assert_eq!(got, [["2", "5"], ["3", "5"], ["4", "5"]], "{r}");
        let e = s.run("exercise.generateVariants", &json!({"count": 3, "domains": {"L": [2, 2, 2, 3]}})).unwrap_err().to_string();
        assert!(e.contains("2/3"), "{e}");
        // The values a datum already has, when no domain is given for it.
        let mut s = session();
        s.run("exercise.generateVariants", &json!({"count": 2, "domains": {"L": [2, 3], "b": [7]}})).unwrap();
        s.run("exercise.setData", &json!({"data": {"items": [{"symbol": "L", "values": ["2", "3", "2", "3"]}], "variants": ["A", "B", "C", "D"]}}))
            .unwrap();
        let e = s.run("exercise.generateVariants", &json!({"count": 3})).unwrap_err().to_string();
        assert!(e.contains("2/3"), "{e}");
    }

    /// A step too fine to write gives the same text again and again: one value, not three
    /// variants with the same data.
    #[test]
    fn a_step_too_fine_to_write_gives_no_twin_variants() {
        let mut s = session();
        let e =
            s.run("exercise.generateVariants", &json!({"count": 3, "domains": {"L": {"min": 1, "max": 2, "step": 1e-9}}})).unwrap_err().to_string();
        assert!(e.contains("1/3"), "{e}");
    }

    /// A datum without a symbol (a line of the data part that is no equation yet) is nothing to
    /// vary, and stays where it is.
    #[test]
    fn a_datum_without_a_symbol_is_kept() {
        let mut s = session();
        s.run(
            "exercise.setData",
            &json!({"data": {"items": [{"symbol": "L", "unit": "m", "values": ["2"]}, {"description": "acciaio S235", "values": ["x"]}, {"symbol": "b", "values": ["3"]}]}}),
        )
        .unwrap();
        let before = s.run("exercise.data", &json!({})).unwrap()["data"]["items"].as_array().unwrap().len();
        let r = s.run("exercise.generateVariants", &json!({"count": 2, "domains": {"L": [2, 4]}})).unwrap();
        let items = r["data"]["items"].as_array().unwrap();
        assert_eq!(items.len(), before, "{r}");
        if before == 3 {
            assert_eq!(items[1]["description"], json!("acciaio S235"), "{r}");
            assert_eq!(items[1]["values"], json!(["x", "x"]), "{r}");
        }
    }

    #[test]
    fn hostile_domains_never_panic() {
        let mut s = session();
        for v in [
            json!({"domains": {"L": {"min": -1e308, "max": 1e308, "step": 1e-300}}}),
            json!({"domains": {"L": {"min": 5, "max": 1}}}),
            json!({"domains": {"L": {"min": "x", "max": 1}}}),
            json!({"domains": {"": [1]}}),
            json!({"domains": [{"symbol": "L", "values": []}]}),
            json!({"domains": {"L": [1, 2]}, "count": 0}),
            json!({"domains": {"L": [1, 2]}, "count": 1e30}),
            json!({"domains": {"L": [1, 2]}, "seed": -1, "variants": [1, null, "", "A"]}),
            json!({"domains": {"L": {"min": 0, "max": 1e300, "step": 1e298}}, "count": 26, "constraints": {"maxDecimals": 1e9, "extra": ["L >", "= 1", "<<", ""]}}),
        ] {
            let _ = s.run("exercise.generateVariants", &v);
        }
    }
}
