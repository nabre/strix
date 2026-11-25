//! The result of one calculation (`calc.result`, `calc.resultInfo`; Math › Number Display ›
//! Result): the unit it is shown in, chosen among the units of its quantity, and its own
//! rounding (decimal places, a rounding step and which way it rounds), whatever the document's
//! format is (docs/calc.md, "The result of one calculation").
//!
//! The unit is the one asked for after the result's `=` (`V = a·b·c = dm³`), written into the
//! statement as the user would type it, so it reads back and saves as any unit asked for; the
//! rounding is the equation's own format options (`Math::calc_opts`, as `calc.options` with
//! `"scope": "equation"` sets them).

use serde_json::{Value, json};
use strix_calc::{Dim, Worksheet, compatible_units};
use wordcraft_doc::math::{Math, parse_linear, to_linear};
use wordcraft_doc::{InlineObject, Pos, StoryRef};

use super::{CalcOverrides, Item, Outcome, item_positions, parse_eq_overrides, run_sheet, session_items, values};
use crate::{CmdError, CmdResult, Selection, Session, p};

/// Why `calc.result` can't run at the caret.
const NO_CALC: &str = "the equation at the caret isn't a calculation";

/// The result of the calculation at the caret, as Math › Number Display › Result shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct CaretResult {
    /// The unit written after the result's `=` (a result shown reads back in its unit, as one
    /// asked for), `None` while there is none.
    pub unit: Option<String>,
    /// The unit the result is shown in now (empty for a plain number).
    pub shown: String,
    /// The units of the result's quantity to choose from (the shown one among them).
    pub units: Vec<String>,
    /// The equation's own decimal places (`None`: the document's format).
    pub decimals: Option<u8>,
    /// The equation's own rounding step (`"0.05"`, or `"none"` to take the document's off).
    pub step: Option<String>,
    /// Which way the equation's own step rounds (`nearest`, `up`, `down`).
    pub step_mode: Option<String>,
}

/// The calculation at the caret: where its equation is, its statement as the calculations read
/// it, its options and its value's dimension (`None` while it has no value).
struct AtCaret {
    at: Pos,
    text: String,
    opts: Option<CalcOverrides>,
    dim: Option<Dim>,
    shown: Option<String>,
}

/// The calculation at the caret, worked out with the document as it stands.
fn at_caret(s: &Session) -> Option<AtCaret> {
    let at = super::super::equation::equation_at_caret(s)?;
    if at.story != StoryRef::Body {
        return None;
    }
    let para = s.doc.para(at.story, &at.path)?;
    let Some(InlineObject::Equation { math, .. }) = para.object_at(at.off) else { return None };
    if !math.calc {
        return None;
    }
    let found = session_items(s);
    let index = item_positions(&s.doc.body).iter().position(|p| p.path == at.path && p.off == at.off)?;
    let Some(Item::Equation { text: Some(text), opts, .. }) = found.get(index) else { return None };
    let (text, opts) = (text.clone(), parse_eq_overrides(opts.as_deref()));
    let zones = super::super::exercise::zones(s);
    let ex_calc = super::super::exercise::exercise_calc(s);
    let (_, outcomes) =
        run_sheet(&found, &super::options_now(s), &zones, &super::super::exercise::seeds(s), &ex_calc, &CalcOverrides::default(), None, &[]);
    let (dim, shown) = match outcomes.get(index) {
        // The result is the last line under the statement when the steps are on lines.
        Some(Outcome::Equation(Some(r), _, _)) if r.error.is_none() => {
            (r.value.as_ref().map(strix_calc::Value::dim), r.steps.as_ref().and_then(|l| l.last()).cloned().or_else(|| r.display.clone()))
        }
        _ => (None, None),
    };
    Some(AtCaret { at, text, opts, dim, shown })
}

/// The result of the calculation at the caret and what can be chosen for it; `None` when the
/// caret is at no calculation showing a result.
pub fn caret_result(s: &Session) -> Option<CaretResult> {
    let found = at_caret(s)?;
    let ws = Worksheet::new(super::options_now(s));
    // A calculation with no result after `=` (a datum, a check, an ordinary equation): nothing
    // to choose.
    ws.with_result_unit(&found.text, None)?;
    let unit = ws.result_unit(&found.text);
    // The line shown is read as a solved unknown's result (`a = 2.5 m/s²` is no datum here).
    let shown = found.shown.as_deref().and_then(|d| ws.result_unit(&format!("⇒ {d}"))).or_else(|| unit.clone()).unwrap_or_default();
    let mut units = found.dim.map(compatible_units).unwrap_or_default();
    for u in [&shown, unit.as_ref().unwrap_or(&shown)] {
        if !units.contains(u) && !(u.is_empty() && found.dim.is_some_and(|d| !d.is_dimensionless())) {
            units.insert(0, u.clone());
        }
    }
    let own = found.opts.unwrap_or_default();
    let decimals = own.digits.filter(|_| own.count.as_deref() == Some("decimals"));
    Some(CaretResult { unit, shown, units, decimals, step: own.step, step_mode: own.step_mode })
}

/// Why `calc.result` can't run: no calculation at the caret.
pub(super) fn disabled(s: &Session) -> Option<&'static str> {
    if super::super::equation::equation_at_caret(s).is_none() {
        return Some("no equation at the caret");
    }
    // Cheap, as the ribbon asks every frame: whether the result shows is the command's to find.
    let calc = super::super::equation::equation_at_caret(s)
        .and_then(|at| s.doc.para(at.story, &at.path).map(|p| matches!(p.object_at(at.off), Some(InlineObject::Equation { math, .. }) if math.calc)));
    (calc != Some(true)).then_some(NO_CALC)
}

/// `calc.result`: the unit, decimal places and rounding step of the result at the caret.
pub(super) fn command(s: &mut Session, v: &Value) -> CmdResult {
    let unit = match v.get("unit") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(u)) if matches!(u.trim(), "" | "auto") => Some(None),
        Some(Value::String(u)) => Some(Some(u.trim().to_string())),
        Some(_) => return Err(CmdError::Params("unit is a unit as written after = (\"dm³\", \"kN·m\"), or \"auto\"".into())),
    };
    let decimals = match v.get("decimals") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(d)) if d.trim() == "auto" => Some(None),
        Some(d) => Some(Some(
            d.as_u64().map(|n| u8::try_from(n.min(15)).unwrap_or(15)).ok_or_else(|| CmdError::Params("decimals is 0–15 or \"auto\"".into()))?,
        )),
    };
    let step = super::step_param(v)?;
    let step_mode = match p::str(v, "stepMode") {
        Some(m) => Some(strix_calc::StepMode::from_name(m).ok_or_else(|| CmdError::Params("stepMode is nearest, up or down".into()))?),
        None => None,
    };
    let reset = p::bool(v, "reset").unwrap_or(false);
    // The equation is rewritten: an editing caret inside it would point at what was there.
    if let Some(m) = s.math.take() {
        let off = m.at.off + wordcraft_doc::para::OBJ.len_utf8();
        s.sel = Selection::caret(Pos { off, ..m.at });
    }
    let found = at_caret(s).ok_or_else(|| CmdError::Disabled(NO_CALC.into()))?;
    let ws = Worksheet::new(super::options_now(s));
    if ws.with_result_unit(&found.text, None).is_none() {
        return Err(CmdError::Disabled("the calculation at the caret shows no result after =".into()));
    }
    if reset || decimals.is_some() || step.is_some() || step_mode.is_some() {
        super::apply_scoped_change(
            s,
            &json!({"scope": "equation"}),
            |c, _| {
                if reset {
                    (c.digits, c.count, c.zeros, c.step, c.step_mode) = (None, None, None, None, None);
                }
                match decimals {
                    Some(Some(d)) => (c.digits, c.count) = (Some(d), Some("decimals".into())),
                    Some(None) => (c.digits, c.count) = (None, None),
                    None => {}
                }
                if let Some(st) = &step {
                    c.step = Some(st.clone());
                }
                if let Some(m) = step_mode {
                    c.step_mode = Some(m.name().to_string());
                }
            },
            |_, _| {},
        )?;
    }
    if let Some(unit) = unit {
        set_unit(s, &found, &ws, unit.as_deref())?;
    }
    let report = super::recalculate_now(s);
    let state = caret_result(s);
    Ok(json!({
        "unit": state.as_ref().and_then(|r| r.unit.clone()),
        "shown": state.as_ref().map(|r| r.shown.clone()),
        "units": state.as_ref().map(|r| r.units.clone()).unwrap_or_default(),
        "decimals": state.as_ref().and_then(|r| r.decimals),
        "step": state.as_ref().and_then(|r| r.step.clone()),
        "stepMode": state.as_ref().and_then(|r| r.step_mode.clone()),
        "recalculated": report
    }))
}

/// `calc.resultInfo`: [`caret_result`] as JSON.
pub(super) fn info(s: &mut Session, _: &Value) -> CmdResult {
    let r = caret_result(s).ok_or_else(|| CmdError::Disabled(NO_CALC.into()))?;
    Ok(json!({
        "unit": r.unit,
        "shown": r.shown,
        "units": r.units,
        "decimals": r.decimals,
        "step": r.step,
        "stepMode": r.step_mode
    }))
}

/// Writes `unit` (`None`: the automatic one) after the result's `=` in the equation at the
/// caret, once it is a unit of the result's quantity.
fn set_unit(s: &mut Session, found: &AtCaret, ws: &Worksheet, unit: Option<&str>) -> Result<(), CmdError> {
    if let Some(u) = unit {
        let dim = found.dim.ok_or_else(|| CmdError::Disabled("the calculation has no result to show in another unit".into()))?;
        let unit_dim = ws.eval_expr(u).map(|v| v.dim()).map_err(|e| CmdError::Params(format!("{u}: {e}")))?;
        if unit_dim != dim {
            let shown = strix_calc::display_unit(dim);
            let what = if shown.is_empty() { "a plain number".to_string() } else { format!("{shown} (or a unit of the same quantity)") };
            return Err(CmdError::Params(format!("{u} is not a unit of this result: it is in {what}")));
        }
    }
    let text = ws
        .with_result_unit(&found.text, unit)
        .ok_or_else(|| CmdError::Params(format!("{} is not a unit as written after = (\"dm³\", \"kN·m\", \"m/s²\")", unit.unwrap_or_default())))?;
    let para = s.doc.para_mut(found.at.story, &found.at.path).map_err(|e| CmdError::Failed(e.to_string()))?;
    let Some(InlineObject::Equation { linear, math, .. }) = para.object_at_mut(found.at.off) else {
        return Err(CmdError::Failed("no equation there".into()));
    };
    // The statement as typed again (any data shown as values are written by the calculation):
    // the next calculation works it out and lays it out with its steps.
    let nodes = parse_linear(&text);
    let calc_opts = values::stamped(None, linear, math);
    *math = Math { nodes, jc: math.jc, calc: true, answer: math.answer, calc_opts, ..Math::default() };
    *linear = to_linear(&math.nodes);
    para.touch();
    s.touch();
    Ok(())
}

#[cfg(test)]
mod tests;
