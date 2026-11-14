//! Live calculations (docs/calc.md): calc regions are equations marked as calculated
//! (`Math::calc`, the Calculate button on an equation) whose linear text is a `strix-calc`
//! statement (`L = 4.5 m`, `M = q·L²/8`, `F = m·a`), and calculated tables (`TableProps::calc`,
//! Table Layout › Data › Calculated Table): named columns with units, column formulas and
//! totals. Both are evaluated in reading order with one shared set of parameters, and results
//! are written back into the equations and cells. Other equations and tables are never read or
//! changed.

use std::sync::Arc;

use serde_json::{Value, json};
use strix_calc::{Count, Layout, Options, RegionKind, RegionResult, TableResult, Worksheet};
use wordcraft_doc::math::{Math, parse_linear, to_linear};
use wordcraft_doc::props::{Rgb, TextColor};
use wordcraft_doc::{Block, InlineObject, Paragraph, Table};

use crate::{CmdError, CmdResult, CommandSpec, Session, p};

/// Deepest table nesting searched for regions.
const MAX_DEPTH: usize = 16;
/// Most regions evaluated in one document; later equations are left as they are.
const MAX_REGIONS: usize = 10_000;
/// Most cells read from one calculated table.
const MAX_TABLE_CELLS: usize = 100_000;

/// Builds the calculation command table: insert a region, recalculate, list the parameters.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("calc.insert", "Calculation", "Math › Calculations", insert)
            .params(r#"{"text": string (a statement: "L = 4.5 m", "M = q*L^2/8 =", "F = m*a"), "display"?: bool}"#),
        CommandSpec::new("calc.recalculate", "Recalculate", "Math › Calculations", |s, v| {
            let report = recalculate_with(s, v);
            Ok(report)
        })
        .params(r#"{"digits"?: n, "count"?: "auto|decimals|significant", "zeros"?: bool, "decimalComma"?: bool, "steps"?: bool, "layout"?: "inline|lines"} → {"regions": n, "tables": n, "changed": n, "errors": [{"region"|"table": n, "row"?, "col"?, "text", "error"}]}"#),
        CommandSpec::new("equation.calc", "Calculate", "Equation › Tools", toggle_equation)
            .when(|s| if super::equation::equation_at_caret(s).is_some() { None } else { Some("no equation at the caret") })
            .params(r#"{"value"?: bool (default: toggle)} → {"calc": bool, "recalculated"}: the equation at the caret is a calculation region"#),
        CommandSpec::new("calc.newEquations", "Calculate New Equations", "Math › Calculations", |s, v| {
            s.prefs.calc_new_equations = p::bool(v, "value").unwrap_or(!s.prefs.calc_new_equations);
            Ok(json!({"calcNewEquations": s.prefs.calc_new_equations}))
        })
        .pure()
        .params(r#"{"value"?: bool (default: toggle)}: new equations are calculation regions (a preference)"#),
        CommandSpec::new("calc.steps", "Show Steps", "Math › Results", set_steps)
            .params(r#"{"value"?: bool (default: toggle), "scope"?: "equation"|"exercise"|"document"|"prefs", "exercise"?: 1-based, "reset"?: bool} → formulas, solved equations and checks show their solution steps, laid out as calc.layout says"#),
        CommandSpec::new("calc.layout", "Result Layout", "Math › Results", set_layout)
            .params(r#"{"value"?: "inline" | "lines" (default: toggle), "scope"?: "equation"|"exercise"|"document"|"prefs", "exercise"?: 1-based, "reset"?: bool} → where results and steps go: inline (after ⇒ and = on the statement's line) or lines (one line each under the statement aligned at =); then recalculates"#),
        CommandSpec::new("calc.options", "Number Format", "Math › Results", set_options)
            .params(r#"{"digits"?: 0–15, "count"?: "auto|decimals|significant", "zeros"?: bool, "steps"?: bool, "layout"?: "inline|lines", "uncertainty"?: "max|rss", "scope"?: "equation"|"exercise"|"document"|"prefs", "exercise"?: 1-based, "reset"?: bool} → the format: how results are rounded and laid out (resolved as equation > exercise > document > prefs), then recalculates"#),
        CommandSpec::new("table.calc", "Calculated Table", "Table Layout › Data", toggle_table)
            .when(|s| if s.sel.focus.path.cell().is_some() { None } else { Some("the cursor isn't in a table") })
            .params(r#"{"value"?: bool (default: toggle)} → {"calc": bool, "recalculated"}: the table at the caret is calculated: header cells name its columns (`L [m]`, `M [kN·m] = q·L^2/8`), `Σ =` cells total them"#),
        CommandSpec::new("table.calcNames", "Data Table", "Table Layout › Data", cell_names)
            .when(|s| if s.sel.focus.path.cell().is_some() { None } else { Some("the cursor isn't in a table") })
            .params(r#"{"value": "columnRow" | "rowColumn" | "none"} → {"names", "recalculated"}: the table at the caret is a data table (made calculated): every value is a parameter named by its column and its row's label (the first column), joined by a subscript: `F_A` (columnRow) or `A_F` (rowColumn)"#),
        CommandSpec::new("table.mathCells", "Cells as Equations", "Table Layout › Data", math_cells)
            .when(|s| if s.sel.focus.path.cell().is_some() { None } else { Some("the cursor isn't in a table") })
            .params(r#"{"value"?: bool (default: toggle)} → {"math", "recalculated"}: the table at the caret (made calculated) shows every cell as an equation: names in italics, subscripts, units in the Unit style; off, the cells are text again"#),
        CommandSpec::new("table.calcIndex", "Ordinal Column", "Table Layout › Data", calc_index)
            .when(|s| if s.sel.focus.path.cell().is_some() { None } else { Some("the cursor isn't in a table") })
            .params(r#"{"value"?: bool (default: toggle)} → {"index": bool, "recalculated"}: the table at the caret (made calculated) gets a first column, column "0", headed "i", with its data rows' ordinal numbers (1, 2, 3…) written by the calculations and never read by them; off, that column goes"#),
        CommandSpec::new("table.calcTotal", "Column Total", "Table Layout › Data", calc_total)
            .when(|s| if s.sel.focus.path.cell().is_some() { None } else { Some("the cursor isn't in a table") })
            .params(r#"{"value"?: bool (default: toggle)} → {"total": bool, "recalculated"}: the column at the caret is summed in the total row of its table (made calculated); on, the table gets its total row (Table Design › Total Row)"#),
        CommandSpec::new("calc.insertTable", "Calculated Table", "Math › Tables", insert_table)
            .params(r#"{"rows"?: n (default 4), "cols"?: n (default 3)} → inserts a table and makes it calculated (see table.calc)"#),
        CommandSpec::new("calc.tableInfo", "Calculated Table Info", "Math › Tables", |s, _| {
            let table = s.sel.focus.path.cell().and_then(|(tp, _, _)| s.doc.table(s.sel.focus.story, &tp));
            let calc = table.is_some_and(|t| t.props.calc);
            let names = table.and_then(|t| t.props.calc_names).map_or("none", |n| n.name());
            let math = table.is_some_and(|t| t.props.calc_math);
            let index = table.is_some_and(|t| t.props.calc_index);
            let total = table.is_some_and(|t| t.props.look.total_row);
            let column_total = column_header(s).is_some_and(|(_, c)| !c.props.calc_no_total);
            Ok(json!({"calc": calc, "names": names, "math": math, "index": index, "total": total, "columnTotal": column_total}))
        })
        .pure()
        .params(r#"→ {"calc": bool, "names": "columnRow"|"rowColumn"|"none", "math": bool, "index": bool, "total": bool, "columnTotal": bool}: whether the table at the caret is calculated, how a data table names its cells, whether its cells are shown as equations, whether it has an ordinal column, whether its last row is the total row (Table Design › Total Row), whether the caret's column is summed there (table.calcTotal)"#),
        CommandSpec::new("calc.variables", "Variables", "Math › Results", variables)
            .pure()
            .params(r#"{"digits"?: n, "units"?: {"name" | "document:name" | "exercise:1:name": "unit"}} → {"variables": [{"name", "value", "text", "unit", "si", "shown", "region", "pos", "compatibleUnits", "selectedUnit", "convertedText", "converted"}], "exercises": [{"exercise": 1-based, "title", "variant", "variables": [...]}]}"#),
    ]
}

/// `Document::passthrough` key holding the document's calculation options (JSON [`CalcOverrides`]).
pub const DOC_CALC_OPTIONS: &str = "strix:calc.options";

/// Calculation and number-format overrides at equation, exercise or document scope
/// (`Equazione > Esercizio > Documento > Prefs`, `docs/calc.md`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CalcOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digits: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zeros: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steps: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uncertainty: Option<String>,
}

impl CalcOverrides {
    pub fn is_empty(&self) -> bool {
        self.digits.is_none()
            && self.count.is_none()
            && self.zeros.is_none()
            && self.steps.is_none()
            && self.layout.is_none()
            && self.uncertainty.is_none()
    }

    pub fn sanitize(&mut self) {
        if self.uncertainty.as_deref().is_some_and(|u| strix_calc::UncertaintyMode::from_name(u).is_none()) {
            self.uncertainty = None;
        }
        if let Some(d) = &mut self.digits {
            *d = (*d).min(15);
        }
        if self.count.as_deref().is_some_and(|c| !matches!(c, "auto" | "decimals" | "significant")) {
            self.count = None;
        }
        if self.layout.as_deref().is_some_and(|l| Layout::from_name(l).is_none()) {
            self.layout = None;
        }
    }

    pub fn from_prefs(prefs: &crate::Prefs) -> Self {
        Self {
            uncertainty: Some(strix_calc::UncertaintyMode::from_name(&prefs.calc_uncertainty).unwrap_or_default().name().into()),
            digits: Some(prefs.calc_digits.min(15)),
            count: Some(match prefs.calc_count.as_str() {
                "decimals" | "significant" => prefs.calc_count.clone(),
                _ => "auto".to_string(),
            }),
            zeros: Some(prefs.calc_zeros),
            steps: Some(prefs.calc_steps),
            layout: Some(Layout::from_name(&prefs.calc_layout).unwrap_or_default().name().to_string()),
        }
    }

    pub fn matches_default_options(&self) -> bool {
        self.digits.unwrap_or(3) == 3
            && self.count.as_deref().unwrap_or("auto") == "auto"
            && !self.zeros.unwrap_or(false)
            && !self.steps.unwrap_or(false)
            && self.layout.as_deref().unwrap_or("inline") == "inline"
            && self.uncertainty.as_deref().unwrap_or("max") == "max"
    }

    pub fn apply_to(&self, o: &mut Options) {
        if let Some(u) = self.uncertainty.as_deref().and_then(strix_calc::UncertaintyMode::from_name) {
            o.uncertainty = u;
        }
        if let Some(d) = self.digits {
            o.digits = d.min(15);
        }
        if let Some(c) = &self.count {
            o.count = count(c);
        }
        if let Some(z) = self.zeros {
            o.zeros = z;
        }
        if let Some(st) = self.steps {
            o.steps = st;
        }
        if let Some(l) = self.layout.as_deref().and_then(Layout::from_name) {
            o.layout = l;
        }
    }
}

pub(crate) fn doc_overrides(doc: &wordcraft_doc::Document) -> Option<CalcOverrides> {
    let bytes = doc.passthrough.get(DOC_CALC_OPTIONS)?;
    let mut c: CalcOverrides = serde_json::from_slice(bytes).ok()?;
    c.sanitize();
    (!c.is_empty()).then_some(c)
}

pub(crate) fn set_doc_overrides(doc: &mut wordcraft_doc::Document, c: &CalcOverrides) {
    let mut clean = c.clone();
    clean.sanitize();
    if clean.is_empty() {
        doc.passthrough.remove(DOC_CALC_OPTIONS);
    } else if let Ok(bytes) = serde_json::to_vec(&clean) {
        doc.passthrough.insert(DOC_CALC_OPTIONS.into(), Arc::new(bytes));
    }
}

/// Stamps a newly created document with the current calculation preferences.
pub(crate) fn stamp_new_document(doc: &mut wordcraft_doc::Document, prefs: &crate::Prefs) {
    set_doc_overrides(doc, &CalcOverrides::from_prefs(prefs));
}

fn parse_eq_overrides(raw: Option<&str>) -> Option<CalcOverrides> {
    let mut c: CalcOverrides = serde_json::from_str(raw?).ok()?;
    c.sanitize();
    (!c.is_empty()).then_some(c)
}

fn prefs_options(s: &Session) -> Options {
    Options {
        uncertainty: strix_calc::UncertaintyMode::from_name(&s.prefs.calc_uncertainty).unwrap_or_default(),
        digits: s.prefs.calc_digits.min(15),
        count: count(&s.prefs.calc_count),
        zeros: s.prefs.calc_zeros,
        steps: s.prefs.calc_steps,
        layout: layout_pref(s),
        ..Options::default()
    }
}

/// Document-level resolved options (`Document > Prefs`).
fn doc_options(s: &Session) -> Options {
    let mut o = prefs_options(s);
    if let Some(c) = doc_overrides(&s.doc) {
        c.apply_to(&mut o);
    }
    o
}

/// Effective calculation options at the caret (`Equazione > Esercizio > Documento > Prefs`).
pub fn resolved_options(s: &Session) -> Options {
    let mut o = doc_options(s);
    if let Some(ex) = super::exercise::exercise_at_caret(s)
        && let Some(c) = super::exercise::exercise_calc(s).get(&ex)
    {
        c.apply_to(&mut o);
    }
    if let Some(at) = super::equation::equation_at_caret(s)
        && let Some(para) = s.doc.para(at.story, &at.path)
        && let Some(InlineObject::Equation { math, .. }) = para.object_at(at.off)
        && let Some(c) = parse_eq_overrides(math.calc_opts.as_deref())
    {
        c.apply_to(&mut o);
    }
    o
}

/// Stable identifier of a [`Count`] mode (`auto`, `decimals`, `significant`).
pub fn count_name(c: Count) -> &'static str {
    match c {
        Count::Auto => "auto",
        Count::Decimals => "decimals",
        Count::Significant => "significant",
    }
}

fn call_overrides(v: &Value) -> CalcOverrides {
    CalcOverrides {
        uncertainty: p::str(v, "uncertainty").and_then(strix_calc::UncertaintyMode::from_name).map(|u| u.name().into()),
        digits: p::u64(v, "digits").map(|d| u8::try_from(d.min(15)).unwrap_or(3)),
        count: p::str(v, "count").map(str::to_string),
        zeros: p::bool(v, "zeros"),
        steps: p::bool(v, "steps"),
        layout: p::str(v, "layout").and_then(Layout::from_name).map(|l| l.name().to_string()),
    }
}

/// The number format: the document/preferences, overridden by the call's parameters.
fn options(s: &Session, v: &Value) -> Options {
    let mut o = doc_options(s);
    call_overrides(v).apply_to(&mut o);
    if let Some(c) = p::bool(v, "decimalComma") {
        o.decimal_comma = c;
    }
    o
}

/// The layout preference (`calc.layout`); anything but `lines` is inline.
fn layout_pref(s: &Session) -> Layout {
    Layout::from_name(&s.prefs.calc_layout).unwrap_or_default()
}

fn count(name: &str) -> Count {
    match name {
        "decimals" => Count::Decimals,
        "significant" => Count::Significant,
        _ => Count::Auto,
    }
}

fn apply_scoped_change(
    s: &mut Session,
    v: &Value,
    mutate: impl FnOnce(&mut CalcOverrides, Options),
    mutate_prefs: impl FnOnce(&mut crate::Prefs, Options),
) -> Result<&'static str, CmdError> {
    let reset = p::bool(v, "reset").unwrap_or(false);
    let current = resolved_options(s);
    match p::str(v, "scope") {
        Some("equation") => {
            let at = super::equation::equation_at_caret(s).ok_or_else(|| CmdError::Disabled("no equation at the caret".into()))?;
            let para = s.doc.para_mut(at.story, &at.path).map_err(|e| CmdError::Failed(e.to_string()))?;
            let Some(InlineObject::Equation { math, .. }) = para.object_at_mut(at.off) else {
                return Err(CmdError::Failed("no equation there".into()));
            };
            if reset {
                math.calc_opts = None;
            } else {
                let mut c = parse_eq_overrides(math.calc_opts.as_deref()).unwrap_or_default();
                mutate(&mut c, current);
                c.sanitize();
                math.calc_opts = (!c.is_empty()).then(|| serde_json::to_string(&c).unwrap_or_default());
            }
            math.omml.clear();
            para.touch();
            Ok("equation")
        }
        Some("exercise") => {
            super::exercise::set_exercise_calc(s, v, |c| {
                if reset {
                    *c = CalcOverrides::default();
                } else {
                    mutate(c, current);
                }
            })?;
            Ok("exercise")
        }
        Some("prefs") => {
            if reset {
                let d = crate::Prefs::default();
                s.prefs.calc_digits = d.calc_digits;
                s.prefs.calc_count = d.calc_count;
                s.prefs.calc_zeros = d.calc_zeros;
                s.prefs.calc_steps = d.calc_steps;
                s.prefs.calc_layout = d.calc_layout;
                s.prefs.calc_uncertainty = d.calc_uncertainty;
            } else {
                let base = prefs_options(s);
                mutate_prefs(&mut s.prefs, base);
            }
            Ok("prefs")
        }
        Some("document") => {
            if reset {
                s.doc.passthrough.remove(DOC_CALC_OPTIONS);
            } else {
                let base = doc_options(s);
                let mut c = doc_overrides(&s.doc).unwrap_or_else(|| CalcOverrides::from_prefs(&s.prefs));
                mutate(&mut c, base);
                set_doc_overrides(&mut s.doc, &c);
            }
            Ok("document")
        }
        None => {
            if reset {
                s.doc.passthrough.remove(DOC_CALC_OPTIONS);
            } else {
                let base = doc_options(s);
                let mut c = doc_overrides(&s.doc).unwrap_or_else(|| CalcOverrides::from_prefs(&s.prefs));
                mutate(&mut c, base);
                set_doc_overrides(&mut s.doc, &c);
                mutate_prefs(&mut s.prefs, base);
            }
            Ok("document")
        }
        Some(_) => Err(CmdError::Params("scope is equation, exercise, document or prefs".into())),
    }
}

fn set_steps(s: &mut Session, v: &Value) -> CmdResult {
    let explicit = p::bool(v, "value");
    let scope = apply_scoped_change(
        s,
        v,
        |c, base| c.steps = Some(explicit.unwrap_or(!base.steps)),
        |pr, base| pr.calc_steps = explicit.unwrap_or(!base.steps),
    )?;
    let report = recalculate_now(s);
    let eff = resolved_options(s);
    Ok(json!({"steps": eff.steps, "scope": scope, "recalculated": report}))
}

fn set_layout(s: &mut Session, v: &Value) -> CmdResult {
    let explicit = match p::str(v, "value") {
        Some(name) => Some(Layout::from_name(name).ok_or_else(|| CmdError::Params("value is inline or lines".into()))?),
        None => None,
    };
    let pick = |base: Options| match explicit {
        Some(l) => l,
        None if base.layout == Layout::Lines => Layout::Inline,
        None => Layout::Lines,
    };
    let scope = apply_scoped_change(
        s,
        v,
        |c, base| c.layout = Some(pick(base).name().to_string()),
        |pr, base| pr.calc_layout = pick(base).name().to_string(),
    )?;
    let report = recalculate_now(s);
    let eff = resolved_options(s);
    Ok(json!({"layout": eff.layout.name(), "scope": scope, "recalculated": report}))
}

fn set_options(s: &mut Session, v: &Value) -> CmdResult {
    let uncertainty = match v.get("uncertainty") {
        None => None,
        Some(value) => Some(
            value.as_str().and_then(strix_calc::UncertaintyMode::from_name).ok_or_else(|| CmdError::Params("uncertainty is max or rss".into()))?,
        ),
    };
    let digits = p::u64(v, "digits").map(|d| u8::try_from(d.min(15)).unwrap_or(3));
    let count_str = match p::str(v, "count") {
        Some(c) if matches!(c, "auto" | "decimals" | "significant") => Some(c.to_string()),
        Some(_) => return Err(CmdError::Params("count is auto, decimals or significant".into())),
        None => None,
    };
    let zeros = p::bool(v, "zeros");
    let steps = p::bool(v, "steps");
    let layout = match p::str(v, "layout") {
        Some(name) => Some(Layout::from_name(name).ok_or_else(|| CmdError::Params("layout is inline or lines".into()))?),
        None => None,
    };
    let scope = apply_scoped_change(
        s,
        v,
        |c, _| {
            if let Some(u) = uncertainty {
                c.uncertainty = Some(u.name().into());
            }
            if let Some(d) = digits {
                c.digits = Some(d);
            }
            if let Some(cnt) = &count_str {
                c.count = Some(cnt.clone());
            }
            if let Some(z) = zeros {
                c.zeros = Some(z);
            }
            if let Some(st) = steps {
                c.steps = Some(st);
            }
            if let Some(l) = layout {
                c.layout = Some(l.name().to_string());
            }
        },
        |pr, _| {
            if let Some(u) = uncertainty {
                pr.calc_uncertainty = u.name().into();
            }
            if let Some(d) = digits {
                pr.calc_digits = d;
            }
            if let Some(cnt) = &count_str {
                pr.calc_count.clone_from(cnt);
            }
            if let Some(z) = zeros {
                pr.calc_zeros = z;
            }
            if let Some(st) = steps {
                pr.calc_steps = st;
            }
            if let Some(l) = layout {
                pr.calc_layout = l.name().to_string();
            }
        },
    )?;
    let report = recalculate_now(s);
    let eff = resolved_options(s);
    Ok(json!({
        "uncertainty": eff.uncertainty.name(),
        "digits": eff.digits,
        "count": count_name(eff.count),
        "zeros": eff.zeros,
        "steps": eff.steps,
        "layout": eff.layout.name(),
        "scope": scope,
        "recalculated": report
    }))
}

/// The linear text of an equation, as the calc engine reads it.
pub(crate) fn equation_text(linear: &str, math: &Math) -> String {
    // An equation showing its result and steps on lines of their own is rows aligned at `&`:
    // the statement is the first, the result the last (read for the unit it is in). Only arrays
    // Strix made are read so: an array the user wrote is the statement as a whole.
    if math.stepped
        && let [wordcraft_doc::math::MNode::EqArr { rows }] = math.nodes.as_slice()
        && let Some(first) = rows.first()
    {
        let row = |r: &[wordcraft_doc::math::MNode]| to_linear(&without_notes(r)).replace('&', "").trim().to_string();
        let first = row(first);
        return match rows.get(1..).and_then(<[_]>::last).map(|r| row(r)) {
            // `= 30.375 kN·m` under a formula: its result, as if after it on its line.
            Some(last) if last.starts_with('=') => format!("{first} {last}"),
            // `a = 2.5 m/s²` under a solved equation, a check's values: after `⇒`.
            Some(last) if !last.is_empty() => format!("{first} ⇒ {last}"),
            _ => first,
        };
    }
    if math.nodes.is_empty() || math.linear {
        linear.to_string()
    } else if math.stepped {
        to_linear(&without_notes(&math.nodes))
    } else {
        to_linear(&math.nodes)
    }
}

/// `row` without the notes Strix wrote into it (normal text: "(numerical solution)").
fn without_notes(row: &[wordcraft_doc::math::MNode]) -> Vec<wordcraft_doc::math::MNode> {
    row.iter().filter(|n| !matches!(n, wordcraft_doc::math::MNode::Run(r) if r.nor)).cloned().collect()
}

/// The equation for `text` (built up from its linear format), with `note` after its result in
/// normal text.
fn shown_nodes(text: &str, note: Option<&str>) -> wordcraft_doc::math::Arg {
    use wordcraft_doc::math::{MNode, MRun, MSty};
    let mut nodes = parse_linear(text);
    // A check's mark stands apart from the unit before it (the linear format drops the space).
    let spaced = |row: &mut wordcraft_doc::math::Arg| {
        for n in row.iter_mut() {
            if let MNode::Run(r) = n
                && r.text.contains(['✓', '✗'])
            {
                r.text = r.text.replace('✓', "\u{2002}✓").replace('✗', "\u{2002}✗");
            }
        }
    };
    match nodes.as_mut_slice() {
        [MNode::EqArr { rows }] => rows.iter_mut().for_each(spaced),
        _ => spaced(&mut nodes),
    }
    if let Some(note) = note {
        let run = MNode::Run(MRun { text: format!(" {note}"), nor: true, sty: Some(MSty::Plain), ..Default::default() });
        match nodes.as_mut_slice() {
            [MNode::EqArr { rows }] => {
                if let Some(last) = rows.last_mut() {
                    last.push(run);
                }
            }
            _ => nodes.push(run),
        }
    }
    nodes
}

/// The equation's whole linear text (all its rows).
fn equation_full(linear: &str, math: &Math) -> String {
    if math.nodes.is_empty() || math.linear { linear.to_string() } else { to_linear(&math.nodes) }
}

/// `line` with the alignment mark before its first `=` (or comparison) outside brackets:
/// `a &= F/m`, `30 MPa &≤ 355 MPa ✓`.
fn aligned(line: &str) -> String {
    let mut depth = 0i32;
    for (i, c) in line.char_indices() {
        match c {
            '(' | '[' | '{' | '〖' => depth += 1,
            ')' | ']' | '}' | '〗' => depth -= 1,
            '=' | '≤' | '≥' | '<' | '>' | '≠' if depth == 0 => {
                return format!("{}&{}", line.get(..i).unwrap_or_default(), line.get(i..).unwrap_or_default());
            }
            _ => {}
        }
    }
    line.to_string()
}

/// The equation a statement and its steps show: the statement alone, or the statement and the
/// steps as rows aligned at `=`, one under the other.
fn with_steps(statement: &str, steps: Option<&Vec<String>>) -> String {
    match steps {
        Some(lines) if !lines.is_empty() => {
            let rows: Vec<String> = std::iter::once(statement).chain(lines.iter().map(String::as_str)).map(aligned).collect();
            format!("█({})", rows.join("@"))
        }
        _ => statement.to_string(),
    }
}

/// What the worksheet reads, in reading order.
#[derive(Debug, Clone, PartialEq)]
enum Item {
    /// An equation in top-level block `top`: its text if it is a calc region, `None` otherwise,
    /// the question it answers (`Math::answer`, 0 for none), and per-equation calc overrides.
    Equation { top: usize, text: Option<String>, answer: u32, full: String, opts: Option<String> },
    /// A calculated table (top-level block `top`): its cells' text, row by row, and how its
    /// cells are named when it is a data table.
    /// `index`: its first column is the ordinal column, written here and never read. `total`:
    /// its last row is the totals row (Table Design › Total Row); `no_total`: the columns it
    /// doesn't sum (`CellProps::calc_no_total` on their header cells).
    Table {
        top: usize,
        cells: Vec<Vec<String>>,
        names: Option<wordcraft_doc::props::CellNames>,
        math: bool,
        index: bool,
        total: bool,
        no_total: Vec<bool>,
    },
    /// A CAD drawing (docs/cad.md) in top-level block `top`: its source.
    Cad { top: usize, source: Arc<str> },
    /// A mathematical/experimental plot (`docs/plot.md`) in top-level block `top`: its source.
    Plot { top: usize, source: Arc<str> },
}

/// A cell's text as the calc engine reads it: its first paragraph.
fn cell_text(blocks: &[Arc<Block>]) -> String {
    match blocks.first().map(|b| &**b) {
        // A cell shown as an equation: its linear text.
        Some(Block::Para(p)) if let Some(text) = cell_equation(p) => text,
        Some(Block::Para(p)) => p.plain_text().trim().to_string(),
        _ => String::new(),
    }
}

/// The linear text of a paragraph that is one equation and nothing else.
fn cell_equation(p: &Paragraph) -> Option<String> {
    let obj = wordcraft_doc::para::OBJ.to_string();
    // A header formula written spreadsheet style keeps its `=` as text before the equation.
    let lead = match p.text.trim() {
        t if t == obj => "",
        t if t == format!("={obj}") => "= ",
        _ => return None,
    };
    match p.objects.as_slice() {
        [InlineObject::Equation { linear, math, .. }] if math.nodes.is_empty() => Some(format!("{lead}{}", linear.trim())),
        [InlineObject::Equation { math, .. }] => Some(format!("{lead}{}", formula_linear(&math.nodes, !lead.is_empty()).trim())),
        _ => None,
    }
}

/// An equation's linear text, a header formula's closing unit (`= q·L/2 [kN]`) set apart by a
/// space: the linear format writes `L/2[kN]`, which reads back with the unit in the fraction.
fn formula_linear(nodes: &[wordcraft_doc::math::MNode], header: bool) -> String {
    use wordcraft_doc::math::MNode;
    match nodes.split_last() {
        Some((last @ MNode::Delim { beg: Some('['), .. }, rest)) if !rest.is_empty() && (header || to_linear(rest).contains('=')) => {
            format!("{} {}", to_linear(rest), to_linear(std::slice::from_ref(last)))
        }
        _ => to_linear(nodes),
    }
}

/// How many bytes of a header cell's text are the leading `=` of a formula written spreadsheet
/// style (`= q·L²/8 [kN·m]`), with the spaces around it: hidden text, shown only while the cell
/// is edited ([`revealed_header`]). 0 for any other header.
fn formula_equals(text: &str) -> usize {
    let rest = text.trim_start();
    match rest.strip_prefix('=') {
        // A formula after it: a bare `=` is a label and shows.
        Some(after) if !after.starts_with('=') && !after.trim().is_empty() => text.len() - after.trim_start().len(),
        _ => 0,
    }
}

/// Hides the leading `=` of a header cell's formula written spreadsheet style and shows the rest
/// (text typed after the `=` takes its formatting, hidden too). A header without one is left as
/// it is (text hidden on purpose stays hidden), unless all of it is hidden: what was typed over
/// a hidden `=`. Returns whether it changed.
fn hide_formula_equals(p: &mut Paragraph) -> bool {
    let cut = formula_equals(&p.text);
    let hidden = |props: &wordcraft_doc::props::CharProps| props.hidden == Some(true);
    if cut == 0 && (p.text.trim().is_empty() || !p.run_ranges().all(|(_, props)| hidden(props))) {
        return false;
    }
    let right = p.run_ranges().all(|(r, props)| {
        if r.end <= cut {
            hidden(props)
        } else if r.start >= cut {
            !hidden(props)
        } else {
            false
        }
    });
    if right {
        return false;
    }
    let len = p.text.len();
    let hide = p.format(0, cut, &|c| c.hidden = Some(true)).is_ok();
    let show = p
        .format(cut, len, &|c| {
            if c.hidden == Some(true) {
                c.hidden = None;
            }
        })
        .is_ok();
    hide || show
}

/// The header row of the calculated table at `top`, its formulas' `=` hidden
/// ([`hide_formula_equals`]). Returns how many cells changed.
fn hide_header_equals(s: &mut Session, top: usize) -> usize {
    let Some(block) = s.doc.body.get_mut(top) else { return 0 };
    let Block::Table(t) = &**block else { return 0 };
    let Some(row) = t.rows.first() else { return 0 };
    // Only what changes is touched (a stable table stays as it is).
    let wrong: Vec<usize> = row
        .cells
        .iter()
        .enumerate()
        .filter(
            |(_, c)| matches!(c.blocks.first().map(|b| &**b), Some(Block::Para(p)) if p.objects.is_empty() && hide_formula_equals(&mut p.clone())),
        )
        .map(|(k, _)| k)
        .collect();
    if wrong.is_empty() {
        return 0;
    }
    let Block::Table(t) = Arc::make_mut(block) else { return 0 };
    for k in &wrong {
        if let Some(Block::Para(p)) = t.rows.first_mut().and_then(|r| r.cells.get_mut(*k)).and_then(|c| c.blocks.first_mut()).map(Arc::make_mut) {
            hide_formula_equals(p);
        }
    }
    wrong.len()
}

/// A table that stops calculating shows its header formulas' `=` again.
fn show_header_equals(t: &mut Table) {
    let Some(row) = t.rows.first_mut() else { return };
    for cell in &mut row.cells {
        if let Some(Block::Para(p)) = cell.blocks.first_mut().map(Arc::make_mut)
            && formula_equals(&p.text) > 0
        {
            let len = p.text.len();
            let _ = p.format(0, len, &|c| {
                if c.hidden == Some(true) {
                    c.hidden = None;
                }
            });
        }
    }
}

/// The paragraph of the calculated table's header cell the caret is in, when it holds hidden
/// text (a formula's `=`, [`hide_formula_equals`]): by its revision, so the layout shows the `=`
/// while the formula is edited.
pub(crate) fn revealed_header(s: &Session) -> Option<u64> {
    let (tp, row, _) = s.sel.focus.path.cell()?;
    if row != 0 || !s.doc.table(s.sel.focus.story, &tp)?.props.calc {
        return None;
    }
    let p = s.doc.para_at(&s.sel.focus)?;
    p.run_ranges().any(|(_, props)| props.hidden == Some(true)).then_some(p.rev)
}

fn table_cells(t: &Table) -> Vec<Vec<String>> {
    let mut budget = MAX_TABLE_CELLS;
    t.rows
        .iter()
        .map(|row| {
            let take = row.cells.len().min(budget);
            budget -= take;
            row.cells.iter().take(take).map(|c| cell_text(&c.blocks)).collect()
        })
        .collect()
}

/// Every paragraph of a block, tables included, exactly as deep as [`items`] reads: the
/// writers count regions as they meet them, so a region deeper than the reader goes would make
/// every later one take the next one's result.
fn each_para(b: &mut Block, depth: usize, f: &mut dyn FnMut(&mut Paragraph)) {
    if depth > MAX_DEPTH {
        return;
    }
    match b {
        Block::Para(p) => f(p),
        Block::Table(t) => {
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    for cb in &mut cell.blocks {
                        each_para(Arc::make_mut(cb), depth + 1, f);
                    }
                }
            }
        }
    }
}

/// Every equation and calculated table in reading order (body top to bottom, tables cell by
/// cell). A calculated table is read as a whole; only top-level ones are calculated.
fn items(blocks: &[Arc<Block>]) -> Vec<Item> {
    fn walk(b: &Block, top: usize, depth: usize, out: &mut Vec<Item>) {
        if depth > MAX_DEPTH || out.len() >= MAX_REGIONS {
            return;
        }
        match b {
            Block::Para(p) => {
                for o in &p.objects {
                    if out.len() >= MAX_REGIONS {
                        return;
                    }
                    if let InlineObject::Graphic { graphic, .. } = o
                        && graphic.kind == wordcraft_doc::graphic::GraphicKind::Cad
                    {
                        out.push(Item::Cad { top, source: graphic.cad.clone().unwrap_or_else(|| Arc::from("")) });
                        continue;
                    }
                    if let InlineObject::Graphic { graphic, .. } = o
                        && graphic.kind == wordcraft_doc::graphic::GraphicKind::Plot
                    {
                        out.push(Item::Plot { top, source: graphic.plot.clone().unwrap_or_else(|| Arc::from("")) });
                        continue;
                    }
                    if let InlineObject::Equation { linear, math, .. } = o {
                        out.push(Item::Equation {
                            top,
                            // A unit written inline (`kN∕m`, see `style_parts`) reads as typed.
                            text: math.calc.then(|| equation_text(linear, math).replace('∕', "/")),
                            answer: math.answer,
                            full: equation_full(linear, math),
                            opts: math.calc_opts.clone(),
                        });
                    }
                }
            }
            Block::Table(t) if depth == 0 && t.props.calc => out.push(Item::Table {
                top,
                cells: table_cells(t),
                names: t.props.calc_names,
                math: t.props.calc_math,
                index: t.props.calc_index,
                total: t.props.look.total_row,
                no_total: t.rows.first().map(|row| row.cells.iter().map(|c| c.props.calc_no_total).collect()).unwrap_or_default(),
            }),
            Block::Table(t) => {
                for row in &t.rows {
                    for cell in &row.cells {
                        for cb in &cell.blocks {
                            walk(cb, top, depth + 1, out);
                        }
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    for (i, b) in blocks.iter().enumerate() {
        walk(b, i, 0, &mut out);
    }
    out
}

/// The equations only (`Item::Equation`), as (top-level block, text) pairs.
#[cfg(test)]
fn regions(blocks: &[Arc<Block>]) -> Vec<(usize, Option<String>)> {
    items(blocks)
        .into_iter()
        .filter_map(|i| match i {
            Item::Equation { top, text, .. } => Some((top, text)),
            Item::Table { .. } | Item::Cad { .. } | Item::Plot { .. } => None,
        })
        .collect()
}

/// The units of a row of a calculation, part by part: between its `=`, `⇒` and comparisons
/// (`F = m·a ⇒ a = 2.5 m/s²`: none, none, none, `m` and `s`).
type Parts = Vec<Vec<String>>;

/// What evaluating an item gave.
enum Outcome {
    /// The result, the units of each row the equation shows (the statement, then its steps),
    /// part by part (between `=`, `⇒` and comparisons) to style, and the formatted value shown
    /// for its defined name under the equation's resolved options.
    Equation(Option<RegionResult>, Vec<Parts>, Option<String>),
    /// The table's result and, for a table shown as equations, every cell's final text with
    /// the units in it (to style).
    Table(TableResult, Option<Vec<Vec<(String, Vec<String>)>>>),
    /// A drawing as rendered, the expressions that failed, and the drawing resolved (its
    /// coordinates' values filled in, for the editor).
    /// The rendering for the page, the failed expressions, and for the editor the drawing, the
    /// parameters it can show and its sheet's rendering when that differs (layers not printed).
    #[allow(clippy::type_complexity)]
    Cad(Option<strix_cad::Rendered>, Vec<(String, String)>, Option<Box<(strix_cad::Drawing, Vec<String>, Option<strix_cad::Rendered>)>>),
    /// A plot as rendered and any series evaluation errors.
    Plot(Option<strix_plot::PlotRendered>, Vec<String>),
}

/// The document's worksheets after evaluating it: the main one, and one per exercise (each
/// exercise is a sandbox: what it defines stays in it, and it sees nothing from outside).
struct Sheets {
    main: Worksheet,
    exercises: std::collections::BTreeMap<usize, Worksheet>,
}

/// Evaluates the items in order, each in its zone's worksheet (`zones`: the exercise of each
/// top-level block, `None` outside exercises). One step budget covers them all.
#[allow(clippy::too_many_arguments)]
fn run_sheet(
    found: &[Item],
    opts: &Options,
    zones: &[Option<usize>],
    seeds: &std::collections::BTreeMap<usize, Vec<String>>,
    ex_calc: &std::collections::BTreeMap<usize, CalcOverrides>,
    call_opts: &CalcOverrides,
    hidden_solutions: Option<&(std::collections::BTreeSet<usize>, bool)>,
) -> (Sheets, Vec<Outcome>) {
    let zone_options = |ex: Option<usize>| -> Options {
        let mut o = *opts;
        if let Some(e) = ex
            && let Some(c) = ex_calc.get(&e)
        {
            c.apply_to(&mut o);
        }
        call_opts.apply_to(&mut o);
        o
    };
    let main_opts = zone_options(None);
    let mut sheets = Sheets { main: Worksheet::new(main_opts), exercises: Default::default() };
    // Each exercise's sandbox starts with its data (the active variant's values).
    // The data count against the same step budget as everything else.
    let mut spent = 0usize;
    for (e, statements) in seeds {
        let z_opts = zone_options(Some(*e));
        let ws = sheets.exercises.entry(*e).or_insert_with(|| Worksheet::new(z_opts));
        for statement in statements {
            ws.scope().set_spent(spent);
            ws.eval_statement(statement);
            spent = ws.scope().spent();
        }
    }
    let outcomes = found
        .iter()
        .map(|item| {
            let top = match item {
                Item::Equation { top, .. } | Item::Table { top, .. } | Item::Cad { top, .. } | Item::Plot { top, .. } => *top,
            };
            let zone = zones.get(top).copied().flatten();
            let show_sol = match (hidden_solutions, zone) {
                (None, _) => true,
                (Some((set, _)), Some(e)) => !set.contains(&e),
                (Some((_, all_hidden)), None) => !*all_hidden,
            };
            let z_opts = zone_options(zone);
            let ws = match zone {
                Some(e) => sheets.exercises.entry(e).or_insert_with(|| Worksheet::new(z_opts)),
                None => &mut sheets.main,
            };
            ws.set_options(z_opts);
            ws.scope().set_spent(spent);
            let outcome = match item {
                Item::Equation { text: Some(text), opts: eq_opts, .. } => {
                    let mut item_opts = z_opts;
                    if let Some(c) = parse_eq_overrides(eq_opts.as_deref()) {
                        c.apply_to(&mut item_opts);
                    }
                    call_opts.apply_to(&mut item_opts);
                    ws.set_options(item_opts);
                    let r = ws.eval_statement(text);
                    let shown = r.name.as_deref().and_then(|n| ws.shown(n));
                    ws.set_options(z_opts);
                    let first = r.display.clone().unwrap_or_else(|| text.clone());
                    let units = std::iter::once(&first).chain(r.steps.iter().flatten()).map(|row| ws.units_by_part(row)).collect();
                    Outcome::Equation(Some(r), units, shown)
                }
                Item::Equation { text: None, .. } => Outcome::Equation(None, Vec::new(), None),
                Item::Table { cells, names, math, index, total, no_total, .. } => {
                    let result = eval_calc_table(ws, cells, *names, *index, *total, no_total);
                    // Shown as equations: every cell's final text, and the units in it to style.
                    let shown = math.then(|| {
                        let mut grid = cells.clone();
                        for (r, c, text) in &result.updates {
                            if let Some(cell) = grid.get_mut(*r).and_then(|row| row.get_mut(*c)) {
                                cell.clone_from(text);
                            }
                        }
                        grid.into_iter()
                            .map(|row| {
                                row.into_iter()
                                    .map(|t| {
                                        // Units only where a quantity or a column's unit is.
                                        let quantity = t.chars().any(|c| c.is_ascii_digit() || c == '[' || c == '(');
                                        let units = if quantity { ws.units_in(&t) } else { Vec::new() };
                                        (t, units)
                                    })
                                    .collect()
                            })
                            .collect()
                    });
                    Outcome::Table(result, shown)
                }
                // A drawing reads the parameters defined before it.
                Item::Cad { source, .. } => match strix_cad::Drawing::from_json(source) {
                    Ok(mut d) => {
                        let errors = d.resolve(ws);
                        // Fitted to its space: the scale follows the sizes just worked out.
                        d.apply_fit(Some(ws));
                        let rendered = d.render_for(Some(ws), false, show_sol);
                        // The editor's sheet shows the layers not printed too.
                        let screen = d.has_unprinted().then(|| d.render_for(Some(ws), true, true));
                        // The parameters it can show, for the editor's Annotation box.
                        let names = ws.scope().iter().map(|(n, _)| n.to_string()).collect();
                        Outcome::Cad(Some(rendered), errors, Some(Box::new((d, names, screen))))
                    }
                    Err(e) => Outcome::Cad(None, vec![(String::new(), e)], None),
                },
                Item::Plot { source, .. } => match strix_plot::Plot::from_json(source) {
                    Ok(p) => {
                        let rendered = p.render_for(Some(ws), show_sol);
                        let errs = rendered.errors.clone();
                        Outcome::Plot(Some(rendered), errs)
                    }
                    Err(e) => Outcome::Plot(None, vec![e]),
                },
            };
            spent = ws.scope().spent();
            outcome
        })
        .collect();
    (sheets, outcomes)
}

/// The sign the total row shows in its first column: the row's label (`docs/calc.md`).
const TOTAL_SIGN: &str = "Σ";

/// A total row's cell left from an earlier calculation, which the next one writes over: empty,
/// a sum (`7`, `12 kN`, `Σ = 7`, `Σ`), no value (`–`) or an error (`⚠`). Anything else (`mean =`,
/// a label) is the user's and stays.
fn stale_total(text: &str) -> bool {
    let t = text.trim();
    strix_calc::is_blank(t)
        || t == "⚠"
        || t == TOTAL_SIGN
        || t == "∑"
        || t.strip_prefix(TOTAL_SIGN).or_else(|| t.strip_prefix('∑')).is_some_and(|rest| rest.trim_start().starts_with('='))
        || (t.starts_with(|c: char| c.is_ascii_digit() || matches!(c, '-' | '−' | '+' | '.')) && !t.contains('='))
}

/// Whether the table at the caret is calculated and has its total row (`None`: not calculated).
pub(crate) fn caret_total_row(s: &Session) -> Option<bool> {
    let (tp, _, _) = s.sel.focus.path.cell()?;
    s.doc.table(s.sel.focus.story, &tp).filter(|t| t.props.calc).map(|t| t.props.look.total_row)
}

/// After Table Style Options: a calculated table whose total row was turned on or off is worked
/// out at once, though the caret is in it (it waits for the caret to leave only while typing).
pub(crate) fn total_row_changed(s: &mut Session, before: Option<bool>) {
    if before.is_some() && caret_total_row(s) != before {
        let opts = options_now(s);
        recalculate(s, &opts);
    }
}

/// A calculated table's total row turned off: what the calculations wrote in its last row (the
/// sums, `Σ`, `–`, `⚠`) goes, so it doesn't stay behind as a row of data; text written there
/// stays. Its cells keep their look.
pub(crate) fn clear_total_row(t: &mut Table) {
    if !t.props.calc || t.rows.len() < 2 {
        return;
    }
    let Some(row) = t.rows.last_mut() else { return };
    for cell in &mut row.cells {
        let text = cell_text(&cell.blocks);
        if text.trim().is_empty() || !stale_total(&text) {
            continue;
        }
        let Some(first) = cell.blocks.first_mut() else { continue };
        let Block::Para(p) = &**first else { continue };
        let mut new = Paragraph::with_text("", p.runs.first().map_or_else(|| p.mark.clone(), |r| r.props.clone()));
        new.props = p.props.clone();
        new.mark = p.mark.clone();
        *Arc::make_mut(first) = Block::Para(new);
    }
}

/// Evaluates calculated table `cells` (row by row, as the document holds them) in `ws`: the
/// ordinal column (`index`, column 0) is not read and gets the data rows' numbers; with `total`
/// the last row is the total row: each column's cell shows its sum alone (`7`), the first
/// column's cell `Σ` (or `Σ = 7` when that column is summed itself), and the columns in
/// `no_total` (by the table's columns) nothing. A cell written there keeps its own text
/// (`mean =`, a label). Returns the result in the table's own columns.
fn eval_calc_table(
    ws: &mut Worksheet,
    cells: &[Vec<String>],
    names: Option<wordcraft_doc::props::CellNames>,
    index: bool,
    total: bool,
    no_total: &[bool],
) -> TableResult {
    let skip = usize::from(index);
    // The ordinal column is not read: the calculations see the table without it.
    let mut read: Vec<Vec<String>> = cells.iter().map(|row| row.iter().skip(skip).cloned().collect()).collect();
    // The total row: every cell left from an earlier calculation sums its column (label columns
    // get none), even a column not summed: the row is then never a data row.
    let last = read.len().saturating_sub(1);
    let total = total && read.len() >= 2;
    let mut sums: Vec<bool> = Vec::new();
    if total && let Some(row) = read.last_mut() {
        for cell in row.iter_mut() {
            let stale = stale_total(cell);
            if stale {
                *cell = "Σ =".to_string();
            }
            sums.push(stale);
        }
    }
    let mut result = ws.eval_table_named(
        &read,
        names.map(|n| match n {
            wordcraft_doc::props::CellNames::ColumnRow => strix_calc::CellNames::ColumnRow,
            wordcraft_doc::props::CellNames::RowColumn => strix_calc::CellNames::RowColumn,
        }),
    );
    if total {
        let summed = |c: usize| no_total.get(c + skip) != Some(&true);
        // A column not summed reports no error there.
        result.errors.retain(|(r, c, _)| *r != last || sums.get(*c) != Some(&true) || summed(*c));
        let mut updates: Vec<(usize, usize, String)> =
            result.updates.iter().filter(|u| u.0 != last || sums.get(u.1) != Some(&true)).cloned().collect();
        for (c, _) in sums.iter().enumerate().filter(|(_, s)| **s) {
            let first = c == 0 && !index;
            let worked = result.updates.iter().find(|u| u.0 == last && u.1 == c).map(|u| u.2.as_str());
            let want = match worked {
                Some(text) if summed(c) && first => text.to_string(),
                Some(text) if summed(c) => text.strip_prefix("Σ =").map_or(text, str::trim).to_string(),
                _ if first => TOTAL_SIGN.to_string(),
                _ => String::new(),
            };
            if cells.get(last).and_then(|row| row.get(c + skip)).map_or("", |t| t.trim()) != want {
                updates.push((last, c, want));
            }
        }
        result.updates = updates;
    }
    if index {
        for u in &mut result.updates {
            u.1 += 1;
        }
        for e in &mut result.errors {
            e.1 += 1;
        }
        // Each data row's ordinal number; the total row's cell is its sign; other rows' are
        // cleared (not the header).
        for (r, n) in strix_calc::ordinals(&read).into_iter().enumerate().skip(1) {
            let want = match n {
                Some(n) => n.to_string(),
                None if total && r == last => TOTAL_SIGN.to_string(),
                None => String::new(),
            };
            if cells.get(r).and_then(|row| row.first()).map_or("", |t| t.trim()) != want {
                result.updates.push((r, 0, want));
            }
        }
    }
    result
}

fn recalculate_with(s: &mut Session, v: &Value) -> Value {
    let base = options(s, v);
    let call_c = call_overrides(v);
    recalculate_scoped(s, &base, &call_c)
}

/// Evaluates every region and calculated table and writes changed results back. Returns the
/// report JSON.
pub(crate) fn recalculate(s: &mut Session, opts: &Options) -> Value {
    recalculate_scoped(s, opts, &CalcOverrides::default())
}

fn recalculate_scoped(s: &mut Session, opts: &Options, call_opts: &CalcOverrides) -> Value {
    let converted_templates = super::exercise::expr::convert_all_templates(s);
    let found = items(&s.doc.body);
    let zones = super::exercise::zones(s);
    let ex_calc = super::exercise::exercise_calc(s);
    let hidden = super::exercise::hidden_solutions(s);
    let (sheets, outcomes) = run_sheet(&found, opts, &zones, &super::exercise::seeds(s), &ex_calc, call_opts, Some(&hidden));
    let mut updates: Vec<Option<(String, Option<&'static str>)>> = Vec::new();
    // The Unit style is added only to a document that has something to style: recalculating a
    // document with no calculation leaves it as it was.
    let live = found.iter().any(|i| !matches!(i, Item::Equation { text: None, .. }));
    let unit = if live { unit_color(s) } else { unit_color_of(s) };
    let mut units: Vec<Vec<Parts>> = Vec::new();
    let mut stepped: Vec<bool> = Vec::new();
    let mut cads: Vec<Option<Redrawn>> = Vec::new();
    let mut plots: Vec<Option<RedrawnPlot>> = Vec::new();
    let mut cell_updates: Vec<(usize, Vec<(usize, usize, String)>)> = Vec::new();
    let mut math_cells: Vec<(usize, Vec<Vec<(String, Vec<String>)>>)> = Vec::new();
    let mut plain_tables: Vec<usize> = Vec::new();
    let mut errors = Vec::new();
    let (mut calc_regions, mut tables) = (0usize, 0usize);
    // The drawings by id, resolved: the underlays printed with a drawing are drawn from them.
    let by_id: std::collections::HashMap<String, &strix_cad::Drawing> = outcomes
        .iter()
        .filter_map(|o| match o {
            Outcome::Cad(_, _, Some(r)) => r.0.id.clone().map(|id| (id, &r.0)),
            _ => None,
        })
        .collect();
    for (i, (item, outcome)) in found.iter().zip(&outcomes).enumerate() {
        match (item, outcome) {
            (Item::Equation { top, text: Some(text), full, .. }, Outcome::Equation(Some(r), found_units, _)) => {
                units.push(found_units.clone());
                if r.kind != RegionKind::NotCalc {
                    calc_regions += 1;
                }
                if let Some(e) = &r.error {
                    errors.push(json!({"region": i, "text": text, "error": e.to_string()}));
                }
                // What the equation should show: the statement (rewritten or as typed) and its steps,
                // and after a result found numerically, a note saying so.
                let statement = r.display.clone().unwrap_or_else(|| text.clone());
                let desired = with_steps(&statement, r.steps.as_ref());
                let noted = r.numeric.then(|| super::exercise::numeric_note_at(s, *top));
                // Compared as the equation would be built and styled (the linear format of a
                // built-up equation drops spaces, writes `(a·b)/8` for `a·b/8`): only a real change
                // rewrites it.
                let squash = |t: &str| t.replace(char::is_whitespace, "");
                let mut built = shown_nodes(&desired, noted);
                style_rows(&mut built, found_units, unit);
                let built = to_linear(&built);
                updates.push((squash(&built) != squash(full)).then_some((desired, noted)));
                stepped.push(r.steps.as_ref().is_some_and(|l| !l.is_empty()) || r.numeric);
            }
            (Item::Equation { .. }, _) => {
                updates.push(None);
                stepped.push(false);
                units.push(Vec::new());
            }
            (Item::Table { top, cells, .. }, Outcome::Table(t, shown)) => {
                tables += 1;
                for (row, col, e) in &t.errors {
                    let text = cells.get(*row).and_then(|r| r.get(*col)).cloned().unwrap_or_default();
                    errors.push(json!({"table": i, "row": row, "col": col, "text": text, "error": e.to_string()}));
                }
                if let Some(grid) = shown {
                    math_cells.push((*top, grid.clone()));
                } else {
                    plain_tables.push(*top);
                    if !t.updates.is_empty() {
                        cell_updates.push((*top, t.updates.clone()));
                    }
                }
            }
            (Item::Cad { .. }, Outcome::Cad(drawn, failed, resolved)) => {
                for (expr, why) in failed {
                    errors.push(json!({"drawing": i, "text": expr, "error": why}));
                }
                let fitted = resolved.as_deref().filter(|(d, ..)| d.fit.is_some()).map(|(d, ..)| d.to_json());
                let body_font = s.doc.settings.minor_font.clone();
                let drawn = with_printed_underlay(drawn.clone(), resolved.as_deref().map(|r| &r.0), &by_id);
                cads.push(drawn.as_ref().map(|r| {
                    let (w, h, items) = super::cad::items_of(r, unit, &body_font);
                    (w, h, items, r.clone(), unit, fitted.clone())
                }));
            }
            (Item::Plot { .. }, Outcome::Plot(drawn, failed)) => {
                errors.extend(failed.iter().map(|why| json!({"plot": i, "error": why})));
                let body_font = s.doc.settings.minor_font.clone();
                plots.push(drawn.as_ref().map(|r| {
                    let (w, h, items) = super::plot::items_of(r, &body_font);
                    (w, h, items, r.clone())
                }));
            }
            (Item::Table { .. } | Item::Cad { .. } | Item::Plot { .. }, _) => {}
        }
    }
    let mut changed = updates.iter().filter(|u| u.is_some()).count() + cell_updates.iter().map(|(_, u)| u.len()).sum::<usize>();
    let equations: Vec<(usize, Option<String>)> = found
        .iter()
        .filter_map(|i| match i {
            Item::Equation { top, text, .. } => Some((*top, text.clone())),
            Item::Table { .. } | Item::Cad { .. } | Item::Plot { .. } => None,
        })
        .collect();
    let restyled = write_back(s, &equations, &updates, &stepped, &units, unit);
    changed += restyled + redraw(s, &cads) + redraw_plots(s, &plots);
    for (top, grid) in &math_cells {
        changed += write_math_cells(s, *top, grid, unit);
    }
    for (top, list) in &cell_updates {
        write_cells(s, *top, list);
    }
    for top in plain_tables {
        changed += hide_header_equals(s, top);
    }
    super::exercise::stem_items::table_blanks(s);
    let (expr_changed, expr_errors) = super::exercise::expr::apply_sheets(s, &zones, &sheets.main, &sheets.exercises);
    changed += usize::from(converted_templates) + expr_changed;
    errors.extend(expr_errors);
    json!({"regions": calc_regions, "tables": tables, "changed": changed, "errors": errors})
}

/// Drawing `d`'s rendering `drawn` with its underlay beneath, when the underlay prints and shows
/// (its source found by id; one level deep, never itself).
fn with_printed_underlay(
    drawn: Option<strix_cad::Rendered>,
    d: Option<&strix_cad::Drawing>,
    by_id: &std::collections::HashMap<String, &strix_cad::Drawing>,
) -> Option<strix_cad::Rendered> {
    let (Some(r), Some(d)) = (drawn.as_ref(), d) else { return drawn };
    let Some(u) = d.underlay.as_ref().filter(|u| u.print && u.show && d.id.as_ref() != Some(&u.source)) else { return drawn };
    let Some(src) = by_id.get(&u.source) else { return drawn };
    Some(r.clone().with_under(d.underlay_view(src).render(None)))
}

/// A hash of what the calculations read: every item's text, in order.
fn fingerprint(found: &[Item]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for item in found {
        match item {
            Item::Equation { text, answer, full, opts, .. } => (text, answer, full, opts).hash(&mut h),
            Item::Table { cells, names, math, index, total, .. } => {
                cells.hash(&mut h);
                names.map(|n| n.name()).hash(&mut h);
                math.hash(&mut h);
                index.hash(&mut h);
                total.hash(&mut h);
            }
            Item::Cad { source, .. } | Item::Plot { source, .. } => source.hash(&mut h),
        }
    }
    h.finish()
}

fn calc_state_hash(s: &Session) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.doc.styles.get(UNIT_STYLE).map(|st| format!("{:?}", st.chr.color)).hash(&mut h);
    super::default_lang(s).hash(&mut h);
    doc_overrides(&s.doc).hash(&mut h);
    (s.prefs.calc_digits, &s.prefs.calc_count, s.prefs.calc_zeros, s.prefs.calc_steps, &s.prefs.calc_layout, &s.prefs.calc_uncertainty).hash(&mut h);
    h.finish()
}

/// After a top-level command: when a calc region or a calculated table changed since the last
/// calculation, recalculate. While an equation is being edited, or the caret is in a cell of a
/// calculated table, it waits until the caret leaves (so typing is never rewritten under the
/// caret), like a spreadsheet.
pub(crate) fn auto_recalculate(s: &mut Session) {
    if s.math.is_some() || caret_in_calc_table(s) {
        return;
    }
    let found = items(&s.doc.body);
    let expr_fp = super::exercise::expr::fingerprint(&s.doc.body);
    let key = fingerprint(&found) ^ super::exercise::signature(s) ^ calc_state_hash(s) ^ expr_fp;
    if s.calc_seen == Some(key) {
        return;
    }
    // Drawings alone count once the document is open: a drawing whose calculations were just
    // deleted shows its parameters as missing, not their old values (on opening, a document
    // with drawings and no calculations is left as saved).
    let drawings = s.calc_seen.is_some() && found.iter().any(|i| matches!(i, Item::Cad { .. } | Item::Plot { .. }));
    let has_expr = super::exercise::expr::has_any_expr_or_template(&s.doc.body);
    if drawings || has_expr || found.iter().any(|i| matches!(i, Item::Equation { text: Some(_), .. } | Item::Table { .. })) {
        let report = recalculate_now(s);
        if report["changed"].as_u64().unwrap_or(0) > 0 {
            s.touch();
        }
    }
    // Exercises show the results: their automatic parts follow.
    super::exercise::update(s);
    let expr_fp_after = super::exercise::expr::fingerprint(&s.doc.body);
    s.calc_seen = Some(fingerprint(&items(&s.doc.body)) ^ super::exercise::signature(s) ^ calc_state_hash(s) ^ expr_fp_after);
}

/// The number format of the preferences.
pub(crate) fn options_now(s: &Session) -> Options {
    options(s, &json!({}))
}

/// The result equations (`Math::answer`) with their top-level block, question and shown value
/// (`M = 30.375 kN·m`), evaluated as the document stands.
pub(crate) fn answers(s: &Session, opts: &Options) -> Vec<(usize, u32, String)> {
    let found = items(&s.doc.body);
    if !found.iter().any(|i| matches!(i, Item::Equation { answer, .. } if *answer > 0)) {
        return Vec::new();
    }
    let zones = super::exercise::zones(s);
    let ex_calc = super::exercise::exercise_calc(s);
    let (sheets, outcomes) = run_sheet(&found, opts, &zones, &super::exercise::seeds(s), &ex_calc, &CalcOverrides::default(), None);
    found
        .iter()
        .zip(&outcomes)
        .filter_map(|(item, outcome)| {
            let Item::Equation { top, answer, text, .. } = item else { return None };
            if *answer == 0 {
                return None;
            }
            let shown = match outcome {
                Outcome::Equation(Some(r), _, eq_shown) => match (&r.name, &r.value, &r.error) {
                    (_, _, Some(e)) => format!("⚠ {e}"),
                    (Some(name), Some(_), None) => {
                        let val = eq_shown.clone().unwrap_or_else(|| {
                            let ws = zones.get(*top).copied().flatten().and_then(|e| sheets.exercises.get(&e)).unwrap_or(&sheets.main);
                            ws.shown(name).unwrap_or_default()
                        });
                        format!("{name} = {val}")
                    }
                    _ => r.display.clone().or_else(|| text.clone()).unwrap_or_default(),
                },
                // An ordinary equation: its text as written.
                _ => text.clone().unwrap_or_default(),
            };
            Some((*top, *answer, shown))
        })
        .collect()
}

/// In-memory evaluation of one exercise under candidate initial-data `statements` (`D4`, `F3`, `F9`).
pub(crate) struct VariantEval {
    pub ws: Worksheet,
    pub errors: Vec<(usize, String)>,
    pub answers: std::collections::BTreeMap<u32, Vec<(String, Option<strix_calc::Quantity>)>>,
    pub scalars: Vec<(String, strix_calc::Quantity, String)>,
}

/// Evaluates exercise `ex` in memory with `statements` (`["L = 4.5 m", ...]`) without mutating `s`.
pub(crate) fn eval_exercise_variant(s: &Session, ex: usize, statements: &[String]) -> VariantEval {
    let zones = super::exercise::zones(s);
    let start_top = zones.iter().position(|z| *z == Some(ex)).unwrap_or(0);
    let mut z_opts = options_now(s);
    if let Some(c) = super::exercise::exercise_calc(s).get(&ex) {
        c.apply_to(&mut z_opts);
    }
    let mut ws = Worksheet::new(z_opts);
    let mut errors: Vec<(usize, String)> = Vec::new();
    for stmt in statements {
        let r = ws.eval_statement(stmt);
        if let Some(e) = r.error {
            errors.push((start_top, format!("{stmt}: {e}")));
        }
    }
    let seed_names: std::collections::BTreeSet<String> = ws.scope().iter().map(|(k, _)| k.to_string()).collect();
    let mut answers: std::collections::BTreeMap<u32, Vec<(String, Option<strix_calc::Quantity>)>> = Default::default();
    let mut scalars: Vec<(String, strix_calc::Quantity, String)> = Vec::new();

    for item in items(&s.doc.body) {
        let top = match &item {
            Item::Equation { top, .. } | Item::Table { top, .. } | Item::Cad { top, .. } | Item::Plot { top, .. } => *top,
        };
        if zones.get(top).copied().flatten() != Some(ex) {
            continue;
        }
        eval_variant_item(&item, top, z_opts, &mut ws, &seed_names, &mut errors, &mut answers, &mut scalars);
    }

    for (top, formula) in super::exercise::expr::collect_exprs(&s.doc.body) {
        if zones.get(top).copied().flatten() == Some(ex)
            && let Err(e) = super::exercise::expr::eval_in_sheet(&ws, &formula)
        {
            errors.push((top, format!("{{{{{formula}}}}}: {e}")));
        }
    }

    VariantEval { ws, errors, answers, scalars }
}

#[allow(clippy::too_many_arguments)]
fn eval_variant_item(
    item: &Item,
    top: usize,
    z_opts: Options,
    ws: &mut Worksheet,
    seed_names: &std::collections::BTreeSet<String>,
    errors: &mut Vec<(usize, String)>,
    answers: &mut std::collections::BTreeMap<u32, Vec<(String, Option<strix_calc::Quantity>)>>,
    scalars: &mut Vec<(String, strix_calc::Quantity, String)>,
) {
    match item {
        Item::Equation { text: Some(text), answer, opts: eq_opts, .. } => {
            let mut item_opts = z_opts;
            if let Some(c) = parse_eq_overrides(eq_opts.as_deref()) {
                c.apply_to(&mut item_opts);
            }
            ws.set_options(item_opts);
            let r = ws.eval_statement(text);
            let shown_opt = r.name.as_deref().and_then(|n| ws.shown(n));
            ws.set_options(z_opts);
            if let Some(e) = &r.error {
                errors.push((top, format!("{text}: {e}")));
            }
            let qty = match &r.value {
                Some(strix_calc::Value::Scalar(q)) => Some(*q),
                _ => None,
            };
            if let (Some(name), Some(q)) = (&r.name, qty)
                && !seed_names.contains(name)
            {
                let disp = shown_opt.clone().unwrap_or_default();
                scalars.push((name.clone(), q, disp));
            }
            if *answer > 0 {
                let ans_str = match (&r.name, &r.value, &r.error) {
                    (_, _, Some(e)) => format!("⚠ {e}"),
                    (Some(name), Some(_), None) => format!("{name} = {}", shown_opt.unwrap_or_default()),
                    _ => r.display.clone().unwrap_or_else(|| text.clone()),
                };
                answers.entry(*answer).or_default().push((ans_str, qty));
            }
        }
        Item::Equation { text: None, answer, full, .. } if *answer > 0 => {
            answers.entry(*answer).or_default().push((full.clone(), None));
        }
        Item::Equation { text: None, .. } => {}
        Item::Table { cells, names, index, total, no_total, .. } => {
            let res = eval_calc_table(ws, cells, *names, *index, *total, no_total);
            for (r, c, e) in res.errors {
                errors.push((top, format!("table ({r},{c}): {e}")));
            }
        }
        Item::Cad { source, .. } => match strix_cad::Drawing::from_json(source) {
            Ok(mut d) => {
                for (expr, why) in d.resolve(ws) {
                    errors.push((top, format!("CAD {expr}: {why}")));
                }
                if let Some(deg) = degenerate_cad_reason(&d) {
                    errors.push((top, format!("CAD: {deg}")));
                }
            }
            Err(e) => errors.push((top, format!("CAD: {e}"))),
        },
        Item::Plot { source, .. } => match strix_plot::Plot::from_json(source) {
            Ok(p) => {
                for why in p.render(Some(ws)).errors {
                    errors.push((top, format!("Plot: {why}")));
                }
            }
            Err(e) => errors.push((top, format!("Plot: {e}"))),
        },
    }
}

fn degenerate_cad_reason(d: &strix_cad::Drawing) -> Option<String> {
    use strix_cad::{Kind, P};
    let is_param_pt = |p: &P| p.0.expr.is_some() || p.1.expr.is_some();
    for e in &d.entities {
        match &e.kind {
            Kind::Circle { r, .. } | Kind::Arc { r, .. } => {
                if !r.value.is_finite() || r.value <= 1e-9 {
                    let label = r.expr.as_deref().unwrap_or(&e.id);
                    return Some(format!("non-positive radius ({label} = {:.4})", r.value));
                }
            }
            Kind::Line { a, b } | Kind::Dim { a, b, .. } if is_param_pt(a) || is_param_pt(b) => {
                let len = (a.0.value - b.0.value).hypot(a.1.value - b.1.value);
                if !len.is_finite() || len <= 1e-9 {
                    return Some(format!("zero-length parametric segment ({})", e.id));
                }
            }
            Kind::Polyline { pts, .. } if pts.iter().any(|v| v.0.expr.is_some() || v.1.expr.is_some()) && pts.len() >= 2 => {
                let first = pts[0].pt();
                if pts.iter().all(|v| (v.0.value - first.x).hypot(v.1.value - first.y) <= 1e-9) {
                    return Some(format!("collapsed parametric polyline ({})", e.id));
                }
            }
            _ => {}
        }
    }
    None
}

/// All formula/expression texts referenced inside exercise `ex` (equations, calculated tables, CAD
/// expressions, and inline `strix:expr` controls), used by `exercise.check` to detect unused data.
pub(crate) fn exercise_formula_texts(s: &Session, ex: usize) -> Vec<String> {
    let zones = super::exercise::zones(s);
    let mut out = Vec::new();
    for item in items(&s.doc.body) {
        let top = match &item {
            Item::Equation { top, .. } | Item::Table { top, .. } | Item::Cad { top, .. } | Item::Plot { top, .. } => *top,
        };
        if zones.get(top).copied().flatten() != Some(ex) {
            continue;
        }
        match item {
            Item::Equation { text: Some(t), .. } => out.push(t),
            Item::Equation { full, .. } => out.push(full),
            Item::Table { cells, .. } => out.extend(cells.into_iter().flatten()),
            Item::Cad { source, .. } | Item::Plot { source, .. } => out.push(source.to_string()),
        }
    }
    for (top, f) in super::exercise::expr::collect_exprs(&s.doc.body) {
        if zones.get(top).copied().flatten() == Some(ex) {
            out.push(f);
        }
    }
    out
}

/// Every drawing's source, with the top-level block holding it.
pub(crate) fn cad_sources(s: &Session) -> Vec<(usize, Arc<str>)> {
    items(&s.doc.body)
        .into_iter()
        .filter_map(|i| match i {
            Item::Cad { top, source } => Some((top, source)),
            _ => None,
        })
        .collect()
}

fn caret_in_calc_table(s: &Session) -> bool {
    s.sel.focus.path.cell().and_then(|(tp, _, _)| s.doc.table(s.sel.focus.story, &tp)).is_some_and(|t| t.props.calc)
}

/// The drawing whose source is `source` (the same `Arc`), resolved where it sits in the document,
/// and rendered, in its own unit.
///
/// The ribbon asks for it every frame while a drawing is selected, so the answer is kept until
/// something it reads changes (the calculations before the drawing, the exercises, the number
/// format): working it out evaluates the document up to the drawing and renders it.
pub(crate) fn resolved_cad(s: &Session, source: &Arc<str>) -> Option<ResolvedCad> {
    resolved_cad_as(s, source, false)
}

/// [`resolved_cad`] in millimetres, the unit the drawing editor draws in: a drawing in
/// centimetres or metres is converted ([`strix_cad::Drawing::to_mm`]: its real size, dimension
/// texts and expressions kept) without changing the document, which takes the converted drawing
/// with the editor's first change.
pub(crate) fn resolved_cad_in_mm(s: &Session, source: &Arc<str>) -> Option<ResolvedCad> {
    resolved_cad_as(s, source, true)
}

fn resolved_cad_as(s: &Session, source: &Arc<str>, in_mm: bool) -> Option<ResolvedCad> {
    use std::hash::{Hash, Hasher};
    let mut found = items(&s.doc.body);
    let at = found.iter().position(|i| matches!(i, Item::Cad { source: src, .. } if Arc::ptr_eq(src, source)))?;
    // What comes after the drawing does not change it.
    found.truncate(at + 1);
    let opts = options_now(s);
    let (zones, seeds, ex_calc) = (super::exercise::zones(s), super::exercise::seeds(s), super::exercise::exercise_calc(s));
    let mut h = std::collections::hash_map::DefaultHasher::new();
    fingerprint(&found).hash(&mut h);
    found
        .iter()
        .map(|i| match i {
            Item::Equation { top, .. } | Item::Table { top, .. } | Item::Cad { top, .. } | Item::Plot { top, .. } => *top,
        })
        .for_each(|t| t.hash(&mut h));
    (&zones, &seeds, &ex_calc, format!("{opts:?}")).hash(&mut h);
    let key = h.finish();
    let slot = usize::from(in_mm);
    if let Some(hit) = CAD_MEMO.with(|m| m.borrow().get(slot).and_then(Option::as_ref).filter(|(k, _)| *k == key).map(|(_, v)| v.clone())) {
        return hit;
    }
    let (mut sheets, outcomes) = run_sheet(&found, &opts, &zones, &seeds, &ex_calc, &CalcOverrides::default(), None);
    let resolved = match (found.last(), outcomes.into_iter().last()) {
        (Some(Item::Cad { top, source }), Some(Outcome::Cad(Some(r), _, Some(resolved)))) => {
            let (d, names, screen) = *resolved;
            if in_mm && d.unit != strix_cad::Unit::Mm {
                // Worked out again from the source converted, in the worksheet it was resolved in.
                let ws = match zones.get(*top).copied().flatten() {
                    Some(e) => sheets.exercises.entry(e).or_insert_with(|| Worksheet::new(opts)),
                    None => &mut sheets.main,
                };
                strix_cad::Drawing::from_json(source).ok().map(|mut d| {
                    d.to_mm();
                    d.resolve(ws);
                    d.apply_fit(Some(ws));
                    let r = if d.has_unprinted() { d.render_layers(Some(ws), true) } else { d.render(Some(ws)) };
                    std::rc::Rc::new((d, r, names))
                })
            } else {
                Some(std::rc::Rc::new((d, screen.unwrap_or(r), names)))
            }
        }
        _ => None,
    };
    CAD_MEMO.with(|m| {
        if let Some(cell) = m.borrow_mut().get_mut(slot) {
            *cell = Some((key, resolved.clone()));
        }
    });
    resolved
}

/// The expressions of drawing `source` that fail where it sits (the parameter or expression and
/// why), as the recalculation reports them; `None` when the drawing isn't in the body.
pub(crate) fn cad_errors(s: &Session, source: &Arc<str>) -> Option<Vec<(String, String)>> {
    let mut found = items(&s.doc.body);
    let at = found.iter().position(|i| matches!(i, Item::Cad { source: src, .. } if Arc::ptr_eq(src, source)))?;
    found.truncate(at + 1);
    let (zones, seeds, ex_calc) = (super::exercise::zones(s), super::exercise::seeds(s), super::exercise::exercise_calc(s));
    let (_, outcomes) = run_sheet(&found, &options_now(s), &zones, &seeds, &ex_calc, &CalcOverrides::default(), None);
    match outcomes.into_iter().last() {
        Some(Outcome::Cad(_, errors, _)) => Some(errors),
        _ => None,
    }
}

/// Evaluates plot `source` where it sits in reading order (against its main or exercise worksheet).
pub(crate) fn resolved_plot(s: &Session, source: &Arc<str>) -> Option<strix_plot::PlotRendered> {
    let mut found = items(&s.doc.body);
    let at = found.iter().position(|i| matches!(i, Item::Plot { source: src, .. } if Arc::ptr_eq(src, source)))?;
    found.truncate(at + 1);
    let (zones, seeds, ex_calc) = (super::exercise::zones(s), super::exercise::seeds(s), super::exercise::exercise_calc(s));
    let (_, outcomes) = run_sheet(&found, &options_now(s), &zones, &seeds, &ex_calc, &CalcOverrides::default(), None);
    match outcomes.into_iter().last() {
        Some(Outcome::Plot(rendered, _)) => rendered,
        _ => None,
    }
}

/// A drawing resolved where it sits: the drawing, how it renders, the parameters it can show.
pub(crate) type ResolvedCad = std::rc::Rc<(strix_cad::Drawing, strix_cad::Rendered, Vec<String>)>;

thread_local! {
    /// The last drawing [`resolved_cad`] worked out, by what it read; then the last one
    /// [`resolved_cad_in_mm`] did.
    static CAD_MEMO: std::cell::RefCell<[Option<(u64, Option<ResolvedCad>)>; 2]> = const { std::cell::RefCell::new([None, None]) };
}

/// Redraws the CAD drawings (in reading order) whose items changed; returns how many.
/// A redrawn drawing: its size, items, the rendering they came from and the Unit colour.
/// With a fitted scale, the source with the scale it was drawn at (so it shows and saves it).
type Redrawn = (f32, f32, Vec<wordcraft_doc::graphic::GraphicItem>, strix_cad::Rendered, Rgb, Option<String>);
type RedrawnPlot = (f32, f32, Vec<wordcraft_doc::graphic::GraphicItem>, strix_plot::PlotRendered);

fn redraw_plots(s: &mut Session, plots: &[Option<RedrawnPlot>]) -> usize {
    if plots.is_empty() {
        return 0;
    }
    let mut index = 0usize;
    let mut changed = 0usize;
    for block in s.doc.body.iter_mut() {
        let mut each = |p: &mut Paragraph| {
            let mut touched = false;
            for o in &mut p.objects {
                let InlineObject::Graphic { w, h, graphic, .. } = o else { continue };
                if graphic.kind != wordcraft_doc::graphic::GraphicKind::Plot {
                    continue;
                }
                if let Some(Some((dw, dh, items, rendered))) = plots.get(index)
                    && (graphic.items != *items || *w != *dw || *h != *dh || graphic.svg.is_none())
                {
                    let g = Arc::make_mut(graphic);
                    g.items = items.clone();
                    g.svg = Some(rendered.to_svg().into());
                    (g.w, g.h) = (*dw, *dh);
                    (*w, *h) = (*dw, *dh);
                    touched = true;
                    changed += 1;
                }
                index += 1;
            }
            if touched {
                p.touch();
            }
        };
        if matches!(&**block, Block::Table(t) if t.props.calc) {
            continue;
        }
        each_para(Arc::make_mut(block), 0, &mut each);
    }
    changed
}

fn redraw(s: &mut Session, cads: &[Option<Redrawn>]) -> usize {
    if cads.is_empty() {
        return 0;
    }
    let mut index = 0usize;
    let mut changed = 0usize;
    for block in s.doc.body.iter_mut() {
        let mut each = |p: &mut Paragraph| {
            let mut touched = false;
            for o in &mut p.objects {
                let InlineObject::Graphic { w, h, graphic, .. } = o else { continue };
                if graphic.kind != wordcraft_doc::graphic::GraphicKind::Cad {
                    continue;
                }
                // The JSON is the drawing; its SVG picture is made again only when what it shows
                // changed (or it has none yet, as read from a file).
                // A fitted drawing keeps the scale it is drawn at in its source.
                if let Some(Some((.., Some(source)))) = cads.get(index)
                    && graphic.cad.as_deref() != Some(source.as_str())
                {
                    Arc::make_mut(graphic).cad = Some(source.as_str().into());
                    touched = true;
                }
                if let Some(Some((dw, dh, items, rendered, unit, _))) = cads.get(index)
                    && (graphic.items != *items || *w != *dw || *h != *dh || graphic.svg.is_none())
                {
                    let g = Arc::make_mut(graphic);
                    g.items = items.clone();
                    g.svg = Some(rendered.to_svg([unit.0, unit.1, unit.2]).into());
                    (g.w, g.h) = (*dw, *dh);
                    (*w, *h) = (*dw, *dh);
                    touched = true;
                    changed += 1;
                }
                index += 1;
            }
            if touched {
                p.touch();
            }
        };
        // Calculated tables are read whole and hold no drawings of their own here.
        if matches!(&**block, Block::Table(t) if t.props.calc) {
            continue;
        }
        each_para(Arc::make_mut(block), 0, &mut each);
    }
    changed
}

/// Recalculates with the preferred number format (F9, Update Fields, leaving an equation).
pub(crate) fn recalculate_now(s: &mut Session) -> Value {
    let opts = options(s, &json!({}));
    recalculate(s, &opts)
}

/// Replaces the text of the regions with an update and styles their units, touching only the
/// blocks that change. Returns how many regions changed only in style.
fn write_back(
    s: &mut Session,
    found: &[(usize, Option<String>)],
    updates: &[Option<(String, Option<&'static str>)>],
    stepped: &[bool],
    units: &[Vec<Parts>],
    color: Rgb,
) -> usize {
    let mut index = 0usize;
    let mut restyled = 0usize;
    let mut tops: Vec<usize> = found.iter().filter(|(_, t)| t.is_some()).map(|(t, _)| *t).collect();
    tops.dedup();
    for (top, block) in s.doc.body.iter_mut().enumerate() {
        // Regions in blocks without updates only advance the counter.
        let count = found.iter().filter(|(t, _)| *t == top).count();
        if !tops.contains(&top) {
            index = index.saturating_add(count);
            continue;
        }
        // A calculated table's cells are not regions: they are skipped like the reader skips them.
        if matches!(&**block, Block::Table(t) if t.props.calc) {
            continue;
        }
        let mut each = |p: &mut Paragraph| {
            let mut touched = false;
            for o in &mut p.objects {
                if let InlineObject::Equation { linear, math, display } = o {
                    if index >= MAX_REGIONS {
                        return;
                    }
                    let found_units = units.get(index).map(Vec::as_slice).unwrap_or_default();
                    let steps_written = stepped.get(index).copied().unwrap_or(false);
                    if let Some(Some((text, note))) = updates.get(index) {
                        let mut nodes = shown_nodes(text, *note);
                        style_rows(&mut nodes, found_units, color);
                        // Steps go one under the other: the equation stands on its own line.
                        if text.starts_with('█') {
                            *display = true;
                        }
                        *math = Math {
                            nodes,
                            jc: math.jc,
                            calc: true,
                            answer: math.answer,
                            stepped: steps_written,
                            calc_opts: math.calc_opts.clone(),
                            ..Math::default()
                        };
                        *linear = to_linear(&math.nodes);
                        touched = true;
                    } else if math.calc {
                        let mut nodes = if math.nodes.is_empty() { parse_linear(linear) } else { math.nodes.clone() };
                        style_rows(&mut nodes, found_units, color);
                        if nodes != math.nodes {
                            math.nodes = nodes;
                            math.omml.clear();
                            touched = true;
                            restyled += 1;
                        }
                    }
                    index = index.saturating_add(1);
                }
            }
            if touched {
                p.touch();
            }
        };
        each_para(Arc::make_mut(block), 0, &mut each);
    }
    restyled
}

/// The character style the units of calculations are shown in: `MathUnit` ("Unit"), added with
/// a teal colour the first time; its colour is the units' colour.
pub(crate) const UNIT_STYLE: &str = "MathUnit";
const UNIT_COLOR: Rgb = Rgb(0x1B, 0x6F, 0x8A);

/// The Unit style's colour, without adding the style when the document has none.
pub(crate) fn unit_color_of(s: &Session) -> Rgb {
    match s.doc.styles.get(UNIT_STYLE).and_then(|st| st.chr.color) {
        Some(TextColor::Rgb(c)) => c,
        _ => UNIT_COLOR,
    }
}

pub(crate) fn unit_color(s: &mut Session) -> Rgb {
    if s.doc.styles.get(UNIT_STYLE).is_none() {
        std::sync::Arc::make_mut(&mut s.doc.styles).styles.push(wordcraft_doc::styles::Style {
            id: UNIT_STYLE.into(),
            name: "Unit".into(),
            kind: wordcraft_doc::styles::StyleKind::Character,
            chr: wordcraft_doc::props::CharProps { color: Some(TextColor::Rgb(UNIT_COLOR)), ..Default::default() },
            priority: Some(30),
            quick: true,
            ..Default::default()
        });
    }
    match s.doc.styles.get(UNIT_STYLE).and_then(|st| st.chr.color) {
        Some(TextColor::Rgb(c)) => c,
        _ => UNIT_COLOR,
    }
}

/// Styles the units of each row with that row's units, part by part: a name that is a unit in
/// the result (`m` in `25 m/s²`) stays a parameter in the formula (`m` in `F = m·a`), on a line
/// of its own or on the statement's line (`F = m·a ⇒ a = 25 m/s²`).
fn style_rows(nodes: &mut wordcraft_doc::math::Arg, rows: &[Parts], color: Rgb) {
    use wordcraft_doc::math::MNode;
    let style = |row: &mut wordcraft_doc::math::Arg, parts: Option<&Parts>, depth: usize| {
        let parts = parts.map(Vec::as_slice).unwrap_or_default();
        // The parts are told apart by the separators at the row's top level; when they don't
        // match what was read (an `=` the text has in a bracket), all the row's units count.
        let separators: usize = row
            .iter()
            .map(|n| match n {
                MNode::Run(r) if !r.nor => r.text.chars().filter(|c| is_separator(*c)).count(),
                _ => 0,
            })
            .sum();
        if separators + 1 == parts.len() {
            let parts: Vec<&[String]> = parts.iter().map(Vec::as_slice).collect();
            style_parts(row, &parts, color, depth);
        } else {
            let mut all: Vec<String> = Vec::new();
            for u in parts.iter().flatten() {
                if !all.contains(u) {
                    all.push(u.clone());
                }
            }
            style_units(row, &all, color, depth);
        }
    };
    match nodes.as_mut_slice() {
        [MNode::EqArr { rows: arr }] => {
            for (k, row) in arr.iter_mut().enumerate() {
                style(row, rows.get(k), 1);
            }
        }
        _ => style(nodes, rows.first(), 0),
    }
}

/// The characters that part a calculation's row ([`Parts`]).
fn is_separator(c: char) -> bool {
    matches!(c, '=' | '⇒' | '≔' | '≤' | '≥' | '<' | '>' | '≠')
}

/// Characters a unit name is made of (letters, `°`, `%`, `‰`, `µ`, `Ω`).
fn unit_char(c: char) -> bool {
    c.is_alphabetic() || matches!(c, '°' | '%' | '‰') || is_superscript(c)
}

/// A superscript power written after a unit (`s²`, `m⁻¹`): styled with it.
fn is_superscript(c: char) -> bool {
    matches!(c, '¹' | '²' | '³' | '⁰' | '⁴'..='⁹' | '⁻')
}

/// Whether `arg` (a fraction's part) is units alone: words that are all units of the row, joined
/// by `·` or spaces (`kN`, `kN·m`, `s²`).
fn unit_arg(arg: &wordcraft_doc::math::Arg, units: &[String]) -> bool {
    use wordcraft_doc::math::MNode;
    !arg.is_empty()
        && arg.iter().all(|n| match n {
            MNode::Run(r) if !r.nor => {
                let words: Vec<&str> = r.text.split(['·', ' ', '\u{2009}']).filter(|w| !w.is_empty()).collect();
                !words.is_empty() && words.iter().all(|w| units.iter().any(|u| u == w.trim_end_matches(is_superscript)))
            }
            // A unit raised to a power (`s²` built up as a superscript).
            MNode::Script { base, sub, sup, .. } => {
                sub.is_empty()
                    && unit_arg(base, units)
                    && sup.iter().all(|n| matches!(n, MNode::Run(r) if r.text.chars().all(|c| c.is_ascii_digit() || c == '-' || c == '−')))
            }
            _ => false,
        })
}

/// When a fraction's numerator is a number then units (`90 km` of `90 km/h`), takes the number
/// out of it and leaves the units alone in `num`. The number comes back as a run with the
/// numerator's own formatting (size, font, colour, a line break before it), only its text cut
/// down to the number; `unit_color` is dropped from it, as a number is never a unit.
fn split_coefficient(num: &mut wordcraft_doc::math::Arg, units: &[String], unit_color: Rgb) -> Option<wordcraft_doc::math::MRun> {
    use wordcraft_doc::math::MNode;
    let MNode::Run(first) = num.first()? else { return None };
    if first.nor {
        return None;
    }
    let coefficient = leading_number(&first.text)?;
    let spaces = |c: char| c == ' ' || c == '\u{2009}';
    let rest = first.text.get(coefficient.len()..)?.trim_start_matches(spaces);
    let mut units_part = num.clone();
    match rest.is_empty() {
        true => {
            units_part.remove(0);
        }
        false => {
            if let Some(MNode::Run(r)) = units_part.first_mut() {
                r.text = rest.to_string();
                // The break, if any, goes before the number, which now comes first.
                r.brk = None;
            }
        }
    }
    if !unit_arg(&units_part, units) {
        return None;
    }
    let mut run = first.clone();
    run.text = coefficient.to_string();
    if run.color == Some(unit_color) {
        run.color = None;
    }
    *num = units_part;
    Some(run)
}

/// The number `text` starts with, if it is a well-formed one: an optional sign, then digits that
/// `.` or `,` may part (`90`, `−9.81`, `2,5`, `1.000,5`), ending on a digit. `None` for text that
/// doesn't start with a number, or whose number runs on into more signs (`3-`, `1,,2`, `2.`).
fn leading_number(text: &str) -> Option<&str> {
    let sign = text.chars().next().filter(|c| matches!(c, '-' | '−' | '+')).map_or(0, char::len_utf8);
    let body = text.get(sign..)?;
    let end = body.find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | ',' | '-' | '−' | '+'))).unwrap_or(body.len());
    let number = body.get(..end)?;
    let well_formed = number.starts_with(|c: char| c.is_ascii_digit())
        && number.ends_with(|c: char| c.is_ascii_digit())
        && !number.contains(['-', '−', '+'])
        && !number.contains("..")
        && !number.contains(",,")
        && !number.contains(".,")
        && !number.contains(",.");
    well_formed.then(|| text.get(..sign + end)).flatten()
}

/// Splits the math runs so each unit is a run of its own, upright in `color`; runs that are no
/// longer units lose that colour. Other formatting is kept.
fn style_units(root: &mut wordcraft_doc::math::Arg, units: &[String], color: Rgb, depth: usize) {
    style_parts(root, &[units], color, depth);
}

/// [`style_units`] with the units of each part of the row (`parts`, see [`Parts`]): the part
/// moves on at each separator at this level.
fn style_parts(root: &mut wordcraft_doc::math::Arg, parts: &[&[String]], color: Rgb, depth: usize) {
    use wordcraft_doc::math::{MNode, MRun, MSty};
    use wordcraft_doc::math_edit::{child_count, child_mut};
    if depth > MAX_DEPTH {
        return;
    }
    let units_of = |k: usize| -> &[String] { parts.get(k).or(parts.last()).copied().unwrap_or_default() };
    let mut part = 0usize;
    let mut out = Vec::with_capacity(root.len());
    for mut n in std::mem::take(root) {
        let units = units_of(part);
        // The linear format makes `50 cm^2` "…c" then `m²`: put the unit back together.
        if let MNode::Script { base, .. } = &mut n
            && let [MNode::Run(b)] = base.as_mut_slice()
            && let Some(MNode::Run(prev)) = out.last_mut()
            && !prev.nor
        {
            let letters: Vec<(usize, char)> = prev.text.char_indices().rev().take_while(|(_, c)| c.is_alphabetic()).take(4).collect();
            for &(at, _) in &letters {
                let candidate = format!("{}{}", prev.text.get(at..).unwrap_or_default(), b.text);
                if units.contains(&candidate) {
                    prev.text.truncate(at);
                    b.text = candidate;
                    break;
                }
            }
            if units.contains(&b.text) && prev.text.ends_with(|c: char| c.is_ascii_digit()) {
                prev.text.push('\u{2009}');
            }
            if prev.text.is_empty() {
                out.pop();
            }
        }
        // A value with a unit quotient (`90 km/h`) is read as `(90 km)/h`: the number is the
        // coefficient of the unit, so it goes before the fraction, which keeps the unit alone.
        if let MNode::Frac { kind, num, den } = &mut n
            && *kind == wordcraft_doc::math::FracKind::Bar
            && unit_arg(den, units)
            && let Some(coefficient) = split_coefficient(num, units, color)
        {
            // It joins the run before only when that run looks the same (`v = ` then `90`).
            match out.last_mut() {
                Some(MNode::Run(prev))
                    if coefficient.brk.is_none()
                        && MRun { text: String::new(), ..prev.clone() } == MRun { text: String::new(), ..coefficient.clone() } =>
                {
                    prev.text.push_str(&coefficient.text)
                }
                _ => out.push(MNode::Run(coefficient)),
            }
        }
        // A unit with a quotient reads inline, as ISO 80000 writes it (`12 kN/m`, `9.81 m/s²`),
        // not stacked like the formula's fractions; a thin space parts it from its number.
        if let MNode::Frac { kind, num, den } = &mut n
            && *kind == wordcraft_doc::math::FracKind::Bar
            && unit_arg(num, units)
            && unit_arg(den, units)
        {
            *kind = wordcraft_doc::math::FracKind::Linear;
            if let Some(MNode::Run(prev)) = out.last_mut()
                && !prev.nor
                && prev.text.ends_with(|c: char| c.is_ascii_digit())
            {
                prev.text.push('\u{2009}');
            }
        }
        let MNode::Run(r) = n else {
            for c in 0..child_count(&n) {
                if let Some(ch) = child_mut(&mut n, c) {
                    style_units(ch, units, color, depth + 1);
                }
            }
            out.push(n);
            continue;
        };
        if r.nor {
            out.push(MNode::Run(r));
            continue;
        }
        // Segments: runs of unit characters, and everything else, each with its part.
        let mut segments: Vec<(bool, String, usize)> = Vec::new();
        for c in r.text.chars() {
            let word = unit_char(c);
            match segments.last_mut() {
                Some((w, t, _)) if *w == word => t.push(c),
                _ => segments.push((word, c.to_string(), part)),
            }
            if parts.len() > 1 && is_separator(c) {
                part += 1;
            }
        }
        let plain = |text: String| {
            let mut p = MRun { text, ..r.clone() };
            if p.color == Some(color) {
                p.color = None;
                p.sty = None;
            }
            p
        };
        let mut pieces: Vec<MRun> = Vec::new();
        for (word, text, k) in segments {
            let piece = if word && units_of(k).iter().any(|u| *u == text.trim_end_matches(is_superscript)) {
                MRun { text, sty: Some(MSty::Plain), color: Some(color), ..r.clone() }
            } else {
                plain(text)
            };
            // ISO 80000: a thin space between a number and its unit.
            if piece.color == Some(color)
                && let Some(last) = pieces.last_mut()
                && last.text.ends_with(|c: char| c.is_ascii_digit())
            {
                last.text.push('\u{2009}');
            }
            match pieces.last_mut() {
                Some(last) if last.sty == piece.sty && last.color == piece.color && last.brk.is_none() && piece.brk.is_none() => {
                    last.text.push_str(&piece.text)
                }
                _ => pieces.push(piece),
            }
        }
        if pieces.is_empty() {
            out.push(MNode::Run(r));
        } else {
            out.extend(pieces.into_iter().map(MNode::Run));
        }
    }
    *root = out;
}

/// While a calculation is typed: the units in the argument the caret is in (`a`, units `off`
/// into it) get the Unit style at once, as the recalculation would give them. Only that
/// argument, not the structures in it, so the caret's path stays valid; the caret moves past a
/// thin space set between a number and its unit. Returns the caret's new offset.
pub(crate) fn style_units_typing(a: &mut wordcraft_doc::math::Arg, off: usize, opts: &Options, color: Rgb) -> usize {
    use wordcraft_doc::math::MNode;
    let flat = |a: &wordcraft_doc::math::Arg| -> Vec<char> {
        a.iter()
            .flat_map(|n| match n {
                MNode::Run(r) => r.text.chars().collect::<Vec<_>>(),
                _ => vec!['\u{FFFC}'],
            })
            .collect()
    };
    // While typing, the parameters aren't known yet: a unit is one that follows a number (`4.5 m`),
    // never a name being defined (`L =`, which is also litres).
    let text = to_linear(a);
    let after_number = |u: &str| {
        text.match_indices(u).any(|(i, _)| {
            let head = text.get(..i).unwrap_or_default().trim_end_matches([' ', '\u{2009}']);
            head.ends_with(|c: char| c.is_ascii_digit()) && !text.get(i + u.len()..).unwrap_or_default().starts_with(char::is_alphanumeric)
        })
    };
    let units: Vec<String> = Worksheet::new(*opts).units_in(&text).into_iter().filter(|u| after_number(u)).collect();
    let before = flat(a);
    // At the depth limit: this argument's runs only.
    style_units(a, &units, color, MAX_DEPTH);
    let after = flat(a);
    // Walk the old text to the caret through the new one, past the thin spaces added.
    let (mut i, mut j) = (0usize, 0usize);
    while i < off.min(before.len()) {
        match (before.get(i), after.get(j)) {
            (Some(x), Some(y)) if x == y => {
                i += 1;
                j += 1;
            }
            (_, Some('\u{2009}')) => j += 1,
            _ => return off.min(after.len()),
        }
    }
    j
}

/// Writes computed cells of the calculated table in top-level block `top`: the cell's first
/// paragraph gets the new text in its first run's formatting (a paragraph holding objects is left
/// alone).
/// A table shown as equations: each cell with text is one equation (its units in the Unit
/// style), an empty cell an empty paragraph; only the cells that change are touched. Returns how
/// many changed.
fn write_math_cells(s: &mut Session, top: usize, grid: &[Vec<(String, Vec<String>)>], color: Rgb) -> usize {
    let Some(block) = s.doc.body.get_mut(top) else { return 0 };
    let Block::Table(t) = Arc::make_mut(block) else { return 0 };
    let mut changed = 0;
    for (r, (row, texts)) in t.rows.iter_mut().zip(grid).enumerate() {
        for (cell, (text, units)) in row.cells.iter_mut().zip(texts) {
            let Some(first) = cell.blocks.first_mut() else { continue };
            let Block::Para(p) = &**first else { continue };
            let mut props = p.runs.first().map_or_else(|| p.mark.clone(), |r| r.props.clone());
            props.hidden = None;
            // A header formula written spreadsheet style (`= q·L²/8`): its `=` stays text,
            // hidden, before the equation ([`hide_formula_equals`]).
            let cut = if r == 0 { formula_equals(text) } else { 0 };
            let body = text.get(cut..).unwrap_or(text);
            let mut nodes = parse_linear(body);
            style_units(&mut nodes, units, color, 0);
            // A label (`Support`, `Beam 2`) stays words: no italics, no units; so do the total
            // row's sign and a cell that found no data.
            let label = cut == 0 && (is_label(text) || text.trim() == TOTAL_SIGN || (!text.trim().is_empty() && strix_calc::is_blank(text)));
            let lead_hidden = p.text.starts_with('=') && p.runs.first().is_some_and(|r| r.props.hidden == Some(true));
            let same = match p.objects.as_slice() {
                [InlineObject::Equation { math, .. }] => !label && math.nodes == nodes && lead_hidden == (cut > 0),
                [] => (text.is_empty() || label) && p.text.trim() == text.as_str(),
                _ => false,
            };
            if same {
                continue;
            }
            let lead = if cut > 0 { "=" } else { "" };
            let mut new = Paragraph::with_text(if label { text.as_str() } else { lead }, props.clone());
            if cut > 0 && new.format(0, lead.len(), &|c| c.hidden = Some(true)).is_err() {
                continue;
            }
            if !body.is_empty() && !label {
                let math = Math { nodes, ..Math::default() };
                let obj = InlineObject::Equation { linear: to_linear(&math.nodes), math, display: false };
                if new.insert_object(lead.len(), obj, &props).is_err() {
                    continue;
                }
            }
            new.props = p.props.clone();
            new.mark = p.mark.clone();
            *Arc::make_mut(first) = Block::Para(new);
            changed += 1;
        }
    }
    changed
}

/// A cell that names rather than computes: a word or words (`Support`, `Beam 2`), with no digit
/// where a quantity would be, no `=`, brackets or subscripts. Single letters (`A`) are symbols.
fn is_label(text: &str) -> bool {
    let t = text.trim();
    t.chars().filter(|c| c.is_alphabetic()).count() >= 2
        && t.split_whitespace().all(|w| w.chars().count() >= 2 || w.chars().all(|c| c.is_ascii_digit()))
        && !t.chars().any(|c| matches!(c, '=' | '[' | ']' | '(' | ')' | '_' | '^' | '/' | '*' | '·' | '+' | '-'))
        && !t.split_whitespace().next().is_some_and(|w| w.chars().all(|c| c.is_ascii_digit() || c == '.' || c == ','))
}

/// A table shown as equations turned back to text: each cell equation becomes its linear text.
fn unmath_cells(t: &mut Table) {
    for row in &mut t.rows {
        for cell in &mut row.cells {
            let Some(first) = cell.blocks.first_mut() else { continue };
            let Block::Para(p) = &**first else { continue };
            let Some(text) = cell_equation(p) else { continue };
            // Not the hidden `=` of a header formula: the calculations hide it again.
            let mut props = p.runs.first().map_or_else(|| p.mark.clone(), |r| r.props.clone());
            props.hidden = None;
            // As typed: the thin space an equation sets before a unit is a plain space in text.
            let mut new = Paragraph::with_text(&text.replace('\u{2009}', " "), props);
            new.props = p.props.clone();
            new.mark = p.mark.clone();
            *Arc::make_mut(first) = Block::Para(new);
        }
    }
}

fn math_cells(s: &mut Session, v: &Value) -> CmdResult {
    let (tp, _, _) = s.sel.focus.path.cell().ok_or_else(|| CmdError::Disabled("the cursor isn't in a table".into()))?;
    let story = s.sel.focus.story;
    let t = s.doc.table_mut(story, &tp)?;
    let on = p::bool(v, "value").unwrap_or(!t.props.calc_math);
    t.props.calc_math = on;
    if on {
        // Shown as equations, the table is calculated.
        t.props.calc = true;
    } else {
        unmath_cells(t);
    }
    super::table::touch_cells(s, &tp)?;
    let report = recalculate(s, &options(s, v));
    Ok(json!({"math": on, "recalculated": report}))
}

fn write_cells(s: &mut Session, top: usize, list: &[(usize, usize, String)]) {
    let Some(block) = s.doc.body.get_mut(top) else { return };
    let Block::Table(t) = Arc::make_mut(block) else { return };
    for (r, c, text) in list {
        let Some(cell) = t.rows.get_mut(*r).and_then(|row| row.cells.get_mut(*c)) else { continue };
        let Some(first) = cell.blocks.first_mut() else { continue };
        let Block::Para(p) = Arc::make_mut(first) else { continue };
        if !p.objects.is_empty() {
            continue;
        }
        let props = p.runs.first().map_or_else(|| p.mark.clone(), |r| r.props.clone());
        let mut new = Paragraph::with_text(text, props);
        new.props = p.props.clone();
        new.mark = p.mark.clone();
        *p = new;
    }
}

fn insert(s: &mut Session, v: &Value) -> CmdResult {
    let text = p::req_str(v, "text")?.trim().to_string();
    if text.is_empty() {
        return Err(CmdError::Params("give the statement as text".into()));
    }
    if text.chars().count() > strix_calc::MAX_INPUT_CHARS {
        return Err(CmdError::Params(format!("a statement is at most {} characters", strix_calc::MAX_INPUT_CHARS)));
    }
    let mut eq = json!({"linear": text, "edit": false, "calc": true});
    if let Some(d) = p::bool(v, "display") {
        eq["display"] = json!(d);
    }
    s.run("insert.equation", &eq).map_err(|e| CmdError::Failed(e.to_string()))?;
    let report = recalculate(s, &options(s, v));
    Ok(json!({"inserted": true, "recalculated": report}))
}

/// Turns the equation at the caret into a calculation region or back, then recalculates.
fn toggle_equation(s: &mut Session, v: &Value) -> CmdResult {
    let at = super::equation::equation_at_caret(s).ok_or_else(|| CmdError::Disabled("no equation at the caret".into()))?;
    let para = s.doc.para_mut(at.story, &at.path).map_err(|e| CmdError::Failed(e.to_string()))?;
    let Some(InlineObject::Equation { math, .. }) = para.object_at_mut(at.off) else {
        return Err(CmdError::Failed("no equation there".into()));
    };
    let on = p::bool(v, "value").unwrap_or(!math.calc);
    if math.calc != on {
        math.calc = on;
        // The kept source XML can't carry the flag: the writer uses the nodes from now on.
        math.omml.clear();
        para.touch();
    }
    let report = recalculate(s, &options(s, v));
    Ok(json!({"calc": on, "recalculated": report}))
}

/// Turns the table at the caret into a calculated table or back, then recalculates.
fn toggle_table(s: &mut Session, v: &Value) -> CmdResult {
    let (tp, _, _) = s.sel.focus.path.cell().ok_or_else(|| CmdError::Disabled("the cursor isn't in a table".into()))?;
    let story = s.sel.focus.story;
    let t = s.doc.table_mut(story, &tp)?;
    let on = p::bool(v, "value").unwrap_or(!t.props.calc);
    let was = t.props.calc;
    t.props.calc = on;
    if !on && was {
        show_header_equals(t);
    }
    if on && !was {
        calc_header(s)?;
    }
    let (tp, _, _) = s.sel.focus.path.cell().ok_or_else(|| CmdError::Disabled("the cursor isn't in a table".into()))?;
    super::table::touch_cells(s, &tp)?;
    let report = recalculate(s, &options(s, v));
    Ok(json!({"calc": on, "recalculated": report}))
}

fn cell_names(s: &mut Session, v: &Value) -> CmdResult {
    let names = match p::str(v, "value") {
        Some("none") => None,
        Some(word) => {
            Some(wordcraft_doc::props::CellNames::parse(word).ok_or_else(|| CmdError::Params("value: columnRow, rowColumn or none".into()))?)
        }
        None => return Err(CmdError::Params("value: columnRow, rowColumn or none".into())),
    };
    let (tp, _, _) = s.sel.focus.path.cell().ok_or_else(|| CmdError::Disabled("the cursor isn't in a table".into()))?;
    let story = s.sel.focus.story;
    let t = s.doc.table_mut(story, &tp)?;
    t.props.calc_names = names;
    // A data table is a calculated table.
    let was = t.props.calc;
    if names.is_some() {
        t.props.calc = true;
        if !was {
            calc_header(s)?;
        }
    }
    let (tp, _, _) = s.sel.focus.path.cell().ok_or_else(|| CmdError::Disabled("the cursor isn't in a table".into()))?;
    super::table::touch_cells(s, &tp)?;
    let report = recalculate(s, &options(s, v));
    Ok(json!({"names": names.map_or("none", |n| n.name()), "recalculated": report}))
}

/// Runs table command `id` with the caret in cell (`row`, `col`) of the table at the caret, then
/// puts the caret back where it was, moved with its cell (`rows`, `cols`: how the command
/// shifts the rows and columns from 0 on: +1 inserted, −1 deleted).
fn at_first_cell(s: &mut Session, id: &str, rows: i32, cols: i32) -> Result<(), CmdError> {
    let (tp, _, _) = s.sel.focus.path.cell().ok_or_else(|| CmdError::Disabled("the cursor isn't in a table".into()))?;
    let keep = s.sel.clone();
    let mut at = tp.0.clone();
    at.extend([0, 0, 0]);
    s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(keep.focus.story, wordcraft_doc::Path(at), 0));
    let done = s.run(id, &json!({}));
    let depth = tp.0.len();
    let shift = |mut p: wordcraft_doc::Pos| {
        if p.path.0.len() > depth + 1 && p.path.0.get(..depth) == Some(&tp.0[..]) {
            let gone = cols < 0 && p.path.0.get(depth + 1) == Some(&0);
            for (k, by) in [(depth, rows), (depth + 1, cols)] {
                if let Some(i) = p.path.0.get_mut(k) {
                    *i = i.saturating_add_signed(by);
                }
            }
            // In the column that went: the start of the first cell left in its row.
            if gone {
                p.path.0.truncate(depth + 2);
                if let Some(c) = p.path.0.get_mut(depth + 1) {
                    *c = 0;
                }
                p.path.0.push(0);
                p.off = 0;
            }
        }
        p
    };
    s.sel = crate::Selection { anchor: shift(keep.anchor), focus: shift(keep.focus) };
    done.map(|_| ()).map_err(|e| CmdError::Failed(e.to_string()))
}

/// The table at the caret just became calculated: its first row is the header the calculations
/// are read from (Table Design › Header Row, repeated at the top of each page). When that row
/// holds values (a table of numbers), an empty header row goes above it.
fn calc_header(s: &mut Session) -> Result<(), CmdError> {
    let (tp, _, _) = s.sel.focus.path.cell().ok_or_else(|| CmdError::Disabled("the cursor isn't in a table".into()))?;
    let story = s.sel.focus.story;
    let values = s.doc.table(story, &tp).and_then(|t| t.rows.first()).is_some_and(|row| {
        let texts: Vec<String> = row.cells.iter().map(|c| cell_text(&c.blocks)).collect();
        let value = |t: &str| t.trim().starts_with(|c: char| c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | '−')) && !t.contains('=');
        texts.iter().any(|t| !t.trim().is_empty()) && texts.iter().all(|t| t.trim().is_empty() || value(t))
    });
    if values {
        at_first_cell(s, "table.insertRowAbove", 1, 0)?;
    }
    calc_style(s)?;
    let (tp, _, _) = s.sel.focus.path.cell().ok_or_else(|| CmdError::Disabled("the cursor isn't in a table".into()))?;
    let t = s.doc.table_mut(story, &tp)?;
    t.props.look.header_row = true;
    if let Some(row) = t.rows.first_mut() {
        row.props.header = true;
        // Bold, so it reads as the header whatever the table's style.
        for cell in &mut row.cells {
            for b in &mut cell.blocks {
                if let Block::Para(p) = Arc::make_mut(b) {
                    p.mark.bold = Some(true);
                    for r in &mut p.runs {
                        r.props.bold = Some(true);
                    }
                }
            }
        }
    }
    Ok(())
}

/// What a cell of a table shown as equations holds where the caret is, for typing straight into
/// its equation: the whole cell is the equation's place (`docs/calc.md`, Cells as equations).
#[derive(Clone, Debug, PartialEq)]
pub enum MathCell {
    /// An empty cell: its paragraph's start, where an equation goes.
    Empty(wordcraft_doc::Pos),
    /// A cell that is one equation: the equation object (a header formula's hidden `=` before
    /// it).
    Equation(wordcraft_doc::Pos),
}

/// The cell of a table shown as equations (top-level, calculated) that `pos` is in, when it is
/// empty or one equation and `pos` is in its first paragraph; `None` for any other place.
pub fn math_cell_at(s: &Session, pos: &wordcraft_doc::Pos) -> Option<MathCell> {
    let (tp, _, _) = pos.path.cell()?;
    let t = s.doc.table(pos.story, &tp)?;
    // Only top-level body tables are calculated, and only a cell's first paragraph is read.
    if pos.story != wordcraft_doc::StoryRef::Body
        || tp.0.len() != 1
        || !(t.props.calc && t.props.calc_math)
        || pos.path.0.get(3) != Some(&0)
        || pos.path.0.len() != 4
    {
        return None;
    }
    let p = s.doc.para_at(pos)?;
    let obj = wordcraft_doc::para::OBJ;
    let at = |off: usize| wordcraft_doc::Pos { off, ..pos.clone() };
    match p.objects.as_slice() {
        [] if p.text.trim().is_empty() => Some(MathCell::Empty(at(0))),
        [InlineObject::Equation { .. }] if p.text == obj.to_string() || p.text == format!("={obj}") => {
            p.text.find(obj).map(|off| MathCell::Equation(at(off)))
        }
        _ => None,
    }
}

/// Typing in a cell of a table shown as equations types in its equation: an empty cell gets one
/// first (one undo step with what is typed), a cell that is one equation is edited at its end.
/// `None` when `id`/`v` is not plain typing there (the command then runs as usual).
pub(crate) fn type_in_math_cell(s: &mut Session, id: &str, v: &Value) -> Option<Result<Value, CmdError>> {
    if id != "text.insert" || s.math.is_some() || !s.sel.is_collapsed() || p::bool(v, "raw") == Some(true) || p::bool(v, "paragraphs") == Some(true) {
        return None;
    }
    let text = p::str(v, "text")?.to_string();
    if text.is_empty() || text.contains(['\n', '\r', '\t', '\u{b}', '\u{c}']) {
        return None;
    }
    let cell = math_cell_at(s, &s.sel.focus.clone())?;
    let opened = match cell {
        MathCell::Empty(at) => {
            s.sel = crate::Selection::caret(at);
            s.run("insert.equation", &json!({"linear": "", "display": false, "calc": false, "edit": true}))
        }
        MathCell::Equation(at) => {
            s.sel = crate::Selection::caret(at);
            s.run("equation.edit", &json!({}))
        }
    };
    if let Err(e) = opened {
        return Some(Err(e));
    }
    s.join_next_undo();
    Some(s.run("equation.type", &json!({"text": text})))
}

/// The table style a calculated table takes when it has the default look (Table Grid): a grid of
/// thin lines, the header row's bottom edge and the total row's top edge drawn heavier, so the
/// names and formulas stand apart from the values and the totals from the rows they sum.
pub const CALC_TABLE_STYLE: &str = "Calculated Table";

/// Gives the table at the caret [`CALC_TABLE_STYLE`] (added to the document's styles the first
/// time) when it has no style or Table Grid; a style the user chose stays.
fn calc_style(s: &mut Session) -> Result<(), CmdError> {
    let (tp, _, _) = s.sel.focus.path.cell().ok_or_else(|| CmdError::Disabled("the cursor isn't in a table".into()))?;
    let story = s.sel.focus.story;
    let current = s.doc.table(story, &tp).and_then(|t| t.props.style.clone());
    let grid = super::table_style::table_style_id(s, "TableGrid");
    if current.is_some() && current != grid {
        return Ok(());
    }
    let id = match super::table_style::table_style_id(s, CALC_TABLE_STYLE) {
        Some(id) => id,
        None => {
            let edge = |edges: &str| json!({"edges": edges, "style": "single", "width": 1.5});
            let made = s
                .run(
                    "table.newStyle",
                    &json!({
                        "name": CALC_TABLE_STYLE,
                        "apply": false,
                        "wholeTable": {"borders": {"edges": "all", "style": "single", "width": 0.5}},
                        "headerRow": {"bold": true, "borders": edge("bottom")},
                        "totalRow": {"borders": edge("top")},
                    }),
                )
                .map_err(|e| CmdError::Failed(e.to_string()))?;
            made.get("id").and_then(Value::as_str).map(str::to_string).ok_or_else(|| CmdError::Failed("no table style".into()))?
        }
    };
    let (tp, _, _) = s.sel.focus.path.cell().ok_or_else(|| CmdError::Disabled("the cursor isn't in a table".into()))?;
    s.doc.table_mut(story, &tp)?.props.style = Some(id);
    Ok(())
}

/// The ordinal column (column "0") on or off: a first column headed "i" added, or removed.
fn calc_index(s: &mut Session, v: &Value) -> CmdResult {
    let (tp, _, _) = s.sel.focus.path.cell().ok_or_else(|| CmdError::Disabled("the cursor isn't in a table".into()))?;
    let story = s.sel.focus.story;
    let t = s.doc.table(story, &tp).ok_or_else(|| CmdError::Failed("no table".into()))?;
    let now = t.props.calc && t.props.calc_index;
    // The ordinal column stays when the table stops calculating: its flag says it is there.
    let has_column = t.props.calc_index;
    let on = p::bool(v, "value").unwrap_or(!now);
    if on != now || (!on && has_column) {
        if on && !t.props.calc {
            toggle_table(s, &json!({"value": true}))?;
        }
        if on && has_column {
            // Calculating again: the column is there already.
        } else if on {
            at_first_cell(s, "table.insertColumnLeft", 0, 1)?;
            let (tp, _, _) = s.sel.focus.path.cell().ok_or_else(|| CmdError::Disabled("the cursor isn't in a table".into()))?;
            write_cell_text(s, story, &tp, 0, 0, "i")?;
            // Bold like the rest of the header.
            if let Some(Block::Para(p)) =
                s.doc.table_mut(story, &tp)?.rows.first_mut().and_then(|r| r.cells.first_mut()).and_then(|c| c.blocks.first_mut()).map(Arc::make_mut)
            {
                p.mark.bold = Some(true);
                p.runs.iter_mut().for_each(|r| r.props.bold = Some(true));
            }
        } else {
            at_first_cell(s, "table.deleteColumn", 0, -1)?;
        }
        let (tp, _, _) = s.sel.focus.path.cell().ok_or_else(|| CmdError::Disabled("the cursor isn't in a table".into()))?;
        s.doc.table_mut(story, &tp)?.props.calc_index = on;
        super::table::touch_cells(s, &tp)?;
    }
    let report = recalculate(s, &options(s, v));
    Ok(json!({"index": on, "recalculated": report}))
}

/// The header cell of the column the caret is in (by grid column, so merged cells line up), and
/// its index in the header row.
fn column_header(s: &Session) -> Option<(usize, &wordcraft_doc::table::Cell)> {
    let (tp, row, col) = s.sel.focus.path.cell()?;
    let t = s.doc.table(s.sel.focus.story, &tp)?;
    let h = t.cell_at_grid(0, t.grid_col(row, col))?;
    Some((h, t.rows.first()?.cells.get(h)?))
}

/// The column at the caret summed in the total row or not (`CellProps::calc_no_total` on its
/// header cell). Summing a column turns the total row on; the table is made calculated.
fn calc_total(s: &mut Session, v: &Value) -> CmdResult {
    let (h, cell) = column_header(s).ok_or_else(|| CmdError::Disabled("the cursor isn't in a table".into()))?;
    let on = p::bool(v, "value").unwrap_or(cell.props.calc_no_total);
    let calc = s.sel.focus.path.cell().and_then(|(tp, _, _)| s.doc.table(s.sel.focus.story, &tp)).is_some_and(|t| t.props.calc);
    if !calc {
        toggle_table(s, &json!({"value": true}))?;
    }
    let (tp, _, _) = s.sel.focus.path.cell().ok_or_else(|| CmdError::Disabled("the cursor isn't in a table".into()))?;
    // Making it calculated may have put a header row above: the column's header is found again.
    let h = column_header(s).map_or(h, |(h, _)| h);
    let t = s.doc.table_mut(s.sel.focus.story, &tp)?;
    let cell = t.rows.first_mut().and_then(|r| r.cells.get_mut(h)).ok_or_else(|| CmdError::Failed("no header cell".into()))?;
    cell.props.calc_no_total = !on;
    if on {
        t.props.look.total_row = true;
    }
    super::table::touch_cells(s, &tp)?;
    let report = recalculate(s, &options(s, v));
    Ok(json!({"total": on, "recalculated": report}))
}

/// Sets the text of cell (`row`, `col`) of the table at `tp`: its first paragraph, its look kept.
fn write_cell_text(
    s: &mut Session,
    story: wordcraft_doc::StoryRef,
    tp: &wordcraft_doc::Path,
    row: usize,
    col: usize,
    text: &str,
) -> Result<(), CmdError> {
    let t = s.doc.table_mut(story, tp)?;
    let cell = t.rows.get_mut(row).and_then(|r| r.cells.get_mut(col)).ok_or_else(|| CmdError::Failed("no such cell".into()))?;
    let first = cell.blocks.first_mut().ok_or_else(|| CmdError::Failed("an empty cell".into()))?;
    if let Block::Para(p) = Arc::make_mut(first) {
        let props = p.runs.first().map_or_else(|| p.mark.clone(), |r| r.props.clone());
        let mut new = Paragraph::with_text(text, props);
        new.props = p.props.clone();
        new.mark = p.mark.clone();
        *p = new;
    }
    Ok(())
}

/// Inserts a table and turns it into a calculated table.
fn insert_table(s: &mut Session, v: &Value) -> CmdResult {
    let rows = p::u64(v, "rows").unwrap_or(4).clamp(2, 1000);
    let cols = p::u64(v, "cols").unwrap_or(3).clamp(1, 63);
    s.run("insert.table", &json!({"rows": rows, "cols": cols})).map_err(|e| CmdError::Failed(e.to_string()))?;
    toggle_table(s, &json!({"value": true}))
}

fn item_positions(blocks: &[Arc<Block>]) -> Vec<wordcraft_doc::Pos> {
    use wordcraft_doc::{Path, Pos, StoryRef};
    fn walk(b: &Block, path: &Path, depth: usize, out: &mut Vec<Pos>) {
        if depth > MAX_DEPTH || out.len() >= MAX_REGIONS {
            return;
        }
        match b {
            Block::Para(p) => {
                let offs = p.text.char_indices().filter(|(_, c)| *c == wordcraft_doc::para::OBJ).map(|(off, _)| off);
                for (off, o) in offs.zip(&p.objects) {
                    if out.len() >= MAX_REGIONS {
                        return;
                    }
                    let is_graphic = matches!(
                        o,
                        InlineObject::Graphic { graphic, .. }
                            if matches!(graphic.kind, wordcraft_doc::graphic::GraphicKind::Cad | wordcraft_doc::graphic::GraphicKind::Plot)
                    );
                    if is_graphic || matches!(o, InlineObject::Equation { .. }) {
                        out.push(Pos::new(StoryRef::Body, path.clone(), off));
                    }
                }
            }
            Block::Table(t) if depth == 0 && t.props.calc => {
                let top = path.0.first().copied().unwrap_or(0);
                out.push(Pos::new(StoryRef::Body, Path(vec![top, 0, 0, 0]), 0));
            }
            Block::Table(t) => {
                for (r, row) in t.rows.iter().enumerate() {
                    for (c, cell) in row.cells.iter().enumerate() {
                        for (k, cb) in cell.blocks.iter().enumerate() {
                            let mut next = path.0.clone();
                            next.extend([r as u32, c as u32, k as u32]);
                            walk(cb, &Path(next), depth + 1, out);
                        }
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    for (i, b) in blocks.iter().enumerate() {
        walk(b, &Path::top(i), 0, &mut out);
    }
    out
}

fn defined_regions(found: &[Item], outcomes: &[Outcome], zones: &[Option<usize>], target_zone: Option<usize>) -> Vec<(String, usize)> {
    let mut defined_at: Vec<(String, usize)> = Vec::new();
    for (i, (item, outcome)) in found.iter().zip(outcomes).enumerate() {
        let top = match item {
            Item::Equation { top, .. } | Item::Table { top, .. } | Item::Cad { top, .. } | Item::Plot { top, .. } => *top,
        };
        if zones.get(top).copied().flatten() != target_zone {
            continue;
        }
        let names: Vec<String> = match outcome {
            Outcome::Equation(Some(r), _, _) if r.error.is_none() => r.name.iter().cloned().collect(),
            Outcome::Table(t, _) => t.columns.clone(),
            Outcome::Equation(..) | Outcome::Cad(..) | Outcome::Plot(..) => Vec::new(),
        };
        for name in names {
            defined_at.retain(|(n, _)| *n != name);
            defined_at.push((name, i));
        }
    }
    defined_at
}

#[allow(clippy::too_many_arguments)]
fn variable_json(
    doc: &wordcraft_doc::Document,
    scope_key: &str,
    name: &str,
    value: &strix_calc::Value,
    region: Option<usize>,
    fallback_pos: Option<&wordcraft_doc::Pos>,
    positions: &[wordcraft_doc::Pos],
    opts: &Options,
    unit_map: Option<&serde_json::Map<String, Value>>,
) -> Value {
    let pos = region.and_then(|idx| positions.get(idx)).or(fallback_pos).map(|p| super::pos_json(&doc.clamp(p)));
    let compat = strix_calc::compatible_units(value.dim());
    let shown = strix_calc::format_value(value, opts);
    let requested = unit_map.and_then(|m| m.get(&format!("{scope_key}:{name}")).or_else(|| m.get(name))).and_then(Value::as_str);
    let converted_pair =
        requested.and_then(|u| strix_calc::format_value_in(value, u, opts).ok().map(|(num, full)| (u.trim().to_string(), num, full)));
    match value.scalar() {
        Some(q) => {
            let (number, unit) = strix_calc::auto_unit(q.value, q.dim);
            let text = strix_calc::format_number(number, opts);
            let (sel_u, conv_text, conv) = converted_pair.unwrap_or_else(|| (unit.clone(), text.clone(), shown.clone()));
            json!({
                "name": name,
                "value": number,
                "text": text,
                "unit": unit,
                "si": q.value,
                "shown": shown,
                "region": region,
                "pos": pos,
                "compatibleUnits": compat,
                "selectedUnit": sel_u,
                "convertedText": conv_text,
                "converted": conv
            })
        }
        None => {
            let si: Vec<f64> = value.quantities().iter().map(|q| q.value).collect();
            let unit = strix_calc::display_unit(value.dim());
            let (sel_u, conv_text, conv) = converted_pair.unwrap_or_else(|| (unit.clone(), shown.clone(), shown.clone()));
            json!({
                "name": name,
                "value": si,
                "text": shown,
                "unit": unit,
                "si": si,
                "shown": shown,
                "region": region,
                "pos": pos,
                "compatibleUnits": compat,
                "selectedUnit": sel_u,
                "convertedText": conv_text,
                "converted": conv
            })
        }
    }
}

fn variables(s: &mut Session, v: &Value) -> CmdResult {
    let opts = options(s, v);
    let call_c = call_overrides(v);
    let unit_map = v.get("units").and_then(Value::as_object);
    let found = items(&s.doc.body);
    let positions = item_positions(&s.doc.body);
    let zones = super::exercise::zones(s);
    let ex_calc = super::exercise::exercise_calc(s);
    let meta = super::exercise::sandbox_meta(s);
    let (sheets, outcomes) = run_sheet(&found, &opts, &zones, &super::exercise::seeds(s), &ex_calc, &call_c, None);
    let main_defined = defined_regions(&found, &outcomes, &zones, None);
    let list: Vec<Value> = sheets
        .main
        .scope()
        .iter()
        .map(|(name, value)| {
            let region = main_defined.iter().find(|(n, _)| n == name).map(|(_, i)| *i);
            variable_json(&s.doc, "document", name, value, region, None, &positions, &opts, unit_map)
        })
        .collect();
    let exercises: Vec<Value> = sheets
        .exercises
        .iter()
        .filter(|(_, ws)| ws.scope().iter().next().is_some())
        .map(|(&ex, ws)| {
            let mut z_opts = opts;
            if let Some(c) = ex_calc.get(&ex) {
                c.apply_to(&mut z_opts);
            }
            call_c.apply_to(&mut z_opts);
            let ex_defined = defined_regions(&found, &outcomes, &zones, Some(ex));
            let (title, variant, seed_pos) = meta.get(&ex).cloned().unwrap_or_else(|| (String::new(), None, wordcraft_doc::Pos::body(0, 0)));
            let scope_key = format!("exercise:{}", ex + 1);
            let vars: Vec<Value> = ws
                .scope()
                .iter()
                .map(|(name, value)| {
                    let region = ex_defined.iter().find(|(n, _)| n == name).map(|(_, i)| *i);
                    let fallback = region.is_none().then_some(&seed_pos);
                    variable_json(&s.doc, &scope_key, name, value, region, fallback, &positions, &z_opts, unit_map)
                })
                .collect();
            json!({
                "exercise": ex + 1,
                "title": title,
                "variant": variant,
                "variables": vars
            })
        })
        .collect();
    Ok(json!({"variables": list, "exercises": exercises}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wordcraft_doc::Document;

    fn session() -> Session {
        let mut s = Session::new(Document::new());
        s.run("document.setText", &json!({"text": ""})).unwrap();
        s
    }

    /// The text of every equation in the body, in order, without spaces (the linear format
    /// of a built-up equation drops them).
    fn texts(s: &Session) -> Vec<String> {
        regions(&s.doc.body).into_iter().filter_map(|(_, t)| t.map(|t| t.replace(char::is_whitespace, ""))).collect()
    }

    fn add(s: &mut Session, text: &str) -> Value {
        let r = s.run("calc.insert", &json!({"text": text})).unwrap();
        s.run("text.insert", &json!({"text": " "})).unwrap();
        r
    }

    /// A region nested deeper than the calculations read never takes another region's result:
    /// the writers walk exactly as deep as the reader.
    #[test]
    fn a_region_too_deep_to_read_never_takes_another_regions_result() {
        let eq = |text: &str| {
            let mut s = session();
            add(&mut s, text);
            let has_eq = |b: &&Arc<Block>| matches!(&***b, Block::Para(p) if p.objects.iter().any(|o| matches!(o, InlineObject::Equation { .. })));
            s.doc.body.iter().find(has_eq).cloned().unwrap()
        };
        let mut t = wordcraft_doc::Table::new(1, 1, 100.0);
        t.rows[0].cells[0].blocks = vec![eq("d := 7 m")];
        for _ in 0..19 {
            let mut outer = wordcraft_doc::Table::new(1, 1, 100.0);
            outer.rows[0].cells[0].blocks = vec![Arc::new(Block::Table(t))];
            t = outer;
        }
        t.rows[0].cells[0].blocks.insert(0, eq("a = 1+1 ="));
        let mut s = session();
        s.doc.body = vec![Arc::new(Block::Table(t)), eq("y = a*3 m =")];
        s.run("calc.recalculate", &json!({})).unwrap();
        fn deepest(b: &Block) -> Option<String> {
            match b {
                // The nested table's region, else this cell's own.
                Block::Table(t) => {
                    let blocks = &t.rows.first()?.cells.first()?.blocks;
                    blocks.iter().find(|b| matches!(&***b, Block::Table(_))).or_else(|| blocks.first()).and_then(|b| deepest(b))
                }
                Block::Para(p) => p.objects.iter().find_map(|o| match o {
                    InlineObject::Equation { linear, math, .. } => Some(equation_text(linear, math)),
                    _ => None,
                }),
            }
        }
        let deep = deepest(&s.doc.body[0]).unwrap_or_default();
        assert!(deep.trim_start().starts_with('d'), "the deep region keeps its own text: {deep}");
        let all = texts(&s);
        assert!(all.last().is_some_and(|t| t.contains('6')), "{all:?}");
    }

    #[test]
    fn a_beam_worksheet_remembers_its_parameters() {
        let mut s = session();
        for t in ["L := 4.5 m", "q := 12 kN/m", "M := q*L^2/8", "M = kN*m"] {
            add(&mut s, t);
        }
        let all = texts(&s);
        assert_eq!(all.len(), 4, "{all:?}");
        assert_eq!(all[3], "M=30.375kN·m");
        let vars = s.run("calc.variables", &json!({})).unwrap();
        let names: Vec<&str> = vars["variables"].as_array().unwrap().iter().map(|v| v["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["L", "q", "M"]);
        assert_eq!(vars["variables"][1]["unit"], "kN/m");
        assert_eq!(vars["variables"][2]["region"], 2);
    }

    #[test]
    fn changing_a_parameter_updates_later_results() {
        let mut s = session();
        add(&mut s, "a := 2 m");
        add(&mut s, "a*3 =");
        assert_eq!(texts(&s)[1], "a*3=(2m)·3=6m");
        // Edit the definition through the equation command, then recalculate (F9 does it too).
        s.sel = crate::Selection::caret(wordcraft_doc::Pos { off: 0, ..s.sel.focus.clone() });
        s.run("equation.set", &json!({"linear": "a := 5 m"})).unwrap();
        s.run("equation.exit", &json!({})).ok();
        let r = s.run("references.updateFields", &json!({})).unwrap();
        assert!(r.is_object());
        assert_eq!(texts(&s), ["a:=5m", "a*3=(5m)·3=15m"]);
        let r = s.run("calc.recalculate", &json!({})).unwrap();
        assert_eq!(r["changed"], 0, "recalculating again changes nothing");
    }

    #[test]
    fn errors_are_reported_and_shown_without_stopping_later_regions() {
        let mut s = session();
        add(&mut s, "x := 2 m + 3 s");
        add(&mut s, "y := 4 kN");
        add(&mut s, "y =");
        let r = s.run("calc.recalculate", &json!({})).unwrap();
        assert_eq!(r["errors"].as_array().unwrap().len(), 1, "{r}");
        assert!(r["errors"][0]["error"].as_str().unwrap().contains("cannot add"));
        assert_eq!(texts(&s)[2], "y=4kN");
    }

    #[test]
    fn ordinary_equations_are_left_alone() {
        let mut s = session();
        s.run("insert.equation", &json!({"linear": "a := 2 m", "edit": false, "calc": false})).unwrap();
        s.run("text.insert", &json!({"text": " "})).unwrap();
        s.run("insert.equation", &json!({"linear": "E = mc^2", "edit": false})).unwrap();
        let r = s.run("calc.recalculate", &json!({})).unwrap();
        // The first isn't calculated; the second is, but `mc^2` is no unit, so it stays as typed.
        assert_eq!(r["regions"], 0);
        assert_eq!(texts(&s), ["E=mc^2"]);
    }

    #[test]
    fn the_calculate_button_turns_an_equation_into_a_region_and_back() {
        let mut s = session();
        s.run("calc.newEquations", &json!({"value": false})).unwrap();
        s.run("insert.equation", &json!({"linear": "x := 3 m", "edit": false})).unwrap();
        s.run("text.insert", &json!({"text": " "})).unwrap();
        add(&mut s, "x*2 =");
        assert_eq!(texts(&s), ["x*2=⚠"], "x isn't defined: its equation isn't calculated");
        s.sel = crate::Selection::caret(wordcraft_doc::Pos { off: 0, ..s.sel.focus.clone() });
        let r = s.run("equation.calc", &json!({})).unwrap();
        assert_eq!(r["calc"], true);
        // With steps (on by default) the value substituted.
        assert_eq!(texts(&s), ["x:=3m", "x*2=(3m)·2=6m"]);
        s.run("equation.calc", &json!({"value": false})).unwrap();
        // With x unknown again, the last result reads as an equation in x and gives it back.
        assert_eq!(texts(&s), ["x*2=6m⇒x=6m/2=3m"]);
        // New equations follow the preference.
        assert_eq!(s.run("calc.newEquations", &json!({})).unwrap()["calcNewEquations"], true);
    }

    #[test]
    fn the_flag_survives_saving_as_xmath() {
        let mut s = session();
        add(&mut s, "L := 2 m");
        s.run("insert.equation", &json!({"linear": "y = 1", "edit": false, "calc": false})).unwrap();
        let bytes = crate::io::save_bytes("w.xmath", &s.doc).unwrap();
        let doc = crate::io::open_bytes("w.xmath", &bytes).unwrap();
        let flags: Vec<bool> = regions(&doc.body).into_iter().map(|(_, t)| t.is_some()).collect();
        assert_eq!(flags, [true, false]);
    }

    #[test]
    fn plain_equals_signs_define_solve_and_show() {
        let mut s = session();
        s.run("calc.layout", &json!({"value": "lines"})).unwrap();
        for t in ["F = 10 N", "m = 4 kg", "F = m·a", "a = m/s^2"] {
            add(&mut s, t);
        }
        // Read back, a solved equation is its statement and the result under it.
        assert_eq!(texts(&s), ["F=10N", "m=4kg", "F=m·a⇒a=2.5m/s²", "a=2.5m/s²"]);
        // The solved equation shows its steps under it, aligned at `=`: formula, values, result.
        let fulls: Vec<(String, bool)> = s
            .doc
            .body
            .iter()
            .filter_map(|b| match &**b {
                Block::Para(p) => Some(p.objects.iter().filter_map(|o| match o {
                    InlineObject::Equation { linear, math, display } => {
                        Some((equation_full(linear, math).replace(char::is_whitespace, ""), *display))
                    }
                    _ => None,
                })),
                _ => None,
            })
            .flatten()
            .collect();
        let (steps, display) = &fulls[2];
        assert!(steps.starts_with("█(F&=m·a@a&=F/m@a&="), "{steps}");
        // The unit's quotient inline (a linear fraction).
        assert!(steps.ends_with("@a&=2.5m∕s²)"), "{steps}");
        assert!(*display, "the steps stand on their own lines");
        // The steps are results: a caret put in them (a click, an arrow key) goes back to the
        // statement, the only part that is edited.
        let at = s
            .doc
            .body
            .iter()
            .enumerate()
            .filter_map(|(i, b)| match &**b {
                // The stepped equation's own object marker (others may share the paragraph).
                Block::Para(p) => p
                    .text
                    .match_indices(wordcraft_doc::para::OBJ)
                    .map(|(off, _)| off)
                    .zip(&p.objects)
                    .find(|(_, o)| matches!(o, InlineObject::Equation { math, .. } if math.stepped))
                    .map(|(off, _)| wordcraft_doc::Pos::new(wordcraft_doc::StoryRef::Body, wordcraft_doc::Path(vec![i as u32]), off)),
                _ => None,
            })
            .next()
            .expect("a stepped equation");
        // As a click in the third row does.
        let pos = json!({"path": [[0, 2]], "off": 1});
        s.sel = crate::Selection::caret(at.clone());
        s.run("equation.edit", &json!({"pos": pos})).unwrap();
        assert_eq!(s.math.as_ref().map(|m| m.pos.path.clone()), Some(vec![(0, 0)]));
        // And an arrow key down from the statement.
        s.run("equation.move", &json!({"dir": "down"})).unwrap();
        assert_eq!(s.math.as_ref().map(|m| m.pos.path.clone()), Some(vec![(0, 0)]));
        s.run("equation.exit", &json!({})).unwrap();
        // Turned off, the result alone under the statement; inline, after `⇒` on its line.
        s.run("calc.steps", &json!({"value": false})).unwrap();
        assert_eq!(texts(&s)[2], "F=m·a⇒a=2.5m/s²");
        let full = |s: &Session| -> Vec<String> {
            items(&s.doc.body)
                .into_iter()
                .filter_map(|i| if let Item::Equation { full, .. } = i { Some(full.replace(char::is_whitespace, "")) } else { None })
                .collect()
        };
        assert_eq!(full(&s)[2], "█(F&=m·a@a&=2.5m∕s²)");
        let r = s.run("calc.layout", &json!({})).unwrap();
        assert_eq!(r["layout"], "inline");
        assert_eq!(full(&s)[2], "F=m·a⇒a=2.5m∕s²");
        let vars = s.run("calc.variables", &json!({})).unwrap();
        assert_eq!(vars["variables"][2]["name"], "a");
        assert_eq!(vars["variables"][2]["region"], 2);
    }

    /// `5 m·30 cm =`: worked in one unit, the result under it; read back it is worked again
    /// (not checked), and changed data is worked anew.
    #[test]
    fn mixed_units_are_worked_in_one_unit_in_the_document() {
        let mut s = session();
        add(&mut s, "5 m·30 cm =");
        let full = |s: &Session| -> Vec<String> {
            items(&s.doc.body)
                .into_iter()
                .filter_map(|i| if let Item::Equation { full, .. } = i { Some(full.replace(char::is_whitespace, "")) } else { None })
                .collect()
        };
        let first = full(&s);
        assert_eq!(first.len(), 1);
        assert!(first[0].contains("5m·0.3m") && first[0].contains("1.5m²"), "{first:?}");
        assert_eq!(texts(&s), ["5m·30cm=5m·0.3m=1.5m²"]);
        // Recalculating changes nothing.
        s.run("calc.recalculate", &json!({})).unwrap();
        assert_eq!(full(&s), first);
        // New data: worked anew.
        s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(wordcraft_doc::StoryRef::Body, wordcraft_doc::Path(vec![0]), 0));
        s.run("equation.set", &json!({"linear": "5 m·40 cm = 5 m·0.3 m = 1.5 m²"})).unwrap();
        s.run("equation.exit", &json!({})).ok();
        let changed = full(&s);
        assert!(changed[0].contains("5m·0.4m") && changed[0].contains("2m²"), "{changed:?}");
    }

    /// `A = 3 m · 5 m =` defines `A` and shows its value at the end; recalculating keeps it.
    #[test]
    fn a_definition_is_worked_out_at_once_in_the_document() {
        let mut s = session();
        add(&mut s, "A = 3 m · 5 m =");
        add(&mut s, "B = 2 m * 2 m =");
        add(&mut s, "A + B =");
        let t = texts(&s);
        assert_eq!(t[0], "A=3m·5m=15m²");
        assert!(t[1].ends_with("2m=4m²"), "{t:?}");
        // With steps (on by default), the values substituted.
        assert_eq!(t[2], "A+B=(15m²)+(4m²)=19m²");
        let r = s.run("calc.recalculate", &json!({})).unwrap();
        assert_eq!(r["changed"], 0, "{r}");
        assert_eq!(r["errors"].as_array().map_or(0, Vec::len), 0, "{r}");
        let vars = s.run("calc.variables", &json!({})).unwrap();
        assert_eq!(vars["variables"][0]["name"], "A");
        // Hostile forms are left alone or reported, never a panic.
        for bad in ["A = =", "= 3 m =", "A = 3 m = =", "A = 3 m = 4 m ="] {
            add(&mut s, bad);
        }
        s.run("calc.recalculate", &json!({})).unwrap();
        assert_eq!(texts(&s)[0], "A=3m·5m=15m²");
        // With steps, mixed units: the working, the result under it; stable on recalculation.
        let mut s = session();
        s.run("calc.steps", &json!({"value": true})).unwrap();
        add(&mut s, "A = 3 m·50 cm =");
        assert_eq!(texts(&s), ["A=3m·50cm=3m·0.5m=1.5m²"]);
        assert_eq!(s.run("calc.recalculate", &json!({})).unwrap()["changed"], 0);
        s.run("calc.steps", &json!({"value": false})).unwrap();
        assert_eq!(texts(&s), ["A=3m·50cm=1.5m²"]);
    }

    /// Inserts a table filled with `cells` (straight into the model: typing isn't under test).
    fn with_table(s: &mut Session, cells: &[&[&str]]) {
        let cols = cells.first().map_or(1, |r| r.len());
        s.run("insert.table", &json!({"rows": cells.len(), "cols": cols})).unwrap();
        let block = s.doc.body.iter_mut().find(|b| matches!(&***b, Block::Table(_))).unwrap();
        let Block::Table(t) = Arc::make_mut(block) else { return };
        for (row, texts) in t.rows.iter_mut().zip(cells) {
            for (cell, text) in row.cells.iter_mut().zip(texts.iter()) {
                cell.blocks = vec![wordcraft_doc::para_block(Paragraph::with_text(text, Default::default()))];
            }
        }
    }

    /// The texts of the first table's cells, row by row.
    fn grid(s: &Session) -> Vec<Vec<String>> {
        let t = s.doc.body.iter().find_map(|b| if let Block::Table(t) = &**b { Some(t.clone()) } else { None }).unwrap();
        t.rows.iter().map(|r| r.cells.iter().map(|c| cell_text(&c.blocks)).collect()).collect()
    }

    /// A header formula written spreadsheet style (`= q·L/2 [kN]`) computes its column; its `=`
    /// is hidden text, shown while the caret is in that header cell (as text or as an equation),
    /// and shown again when the table stops calculating.
    #[test]
    fn a_spreadsheet_style_header_hides_its_equals() {
        use wordcraft_doc::{Path, Pos, StoryRef};
        let mut s = session();
        with_table(&mut s, &[&["L [m]", "q [kN/m]", "= q·L/2 [kN]"], &["4", "10", ""], &["2", "8", ""]]);
        let at = |r: u32, c: u32| crate::Selection::caret(Pos::new(StoryRef::Body, Path(vec![0, r, c, 0]), 0));
        s.sel = at(1, 0);
        s.run("table.calc", &json!({"value": true})).unwrap();
        assert_eq!(grid(&s)[1][2], "20");
        assert_eq!(grid(&s)[2][2], "8");
        let header = |s: &Session| -> Paragraph {
            let t = s.doc.body.iter().find_map(|b| if let Block::Table(t) = &**b { Some(t.clone()) } else { None }).unwrap();
            t.rows[0].cells[2].blocks.first().and_then(|b| b.as_para()).unwrap().clone()
        };
        let hidden =
            |p: &Paragraph| -> String { p.run_ranges().filter(|(_, c)| c.hidden == Some(true)).map(|(r, _)| p.text[r].to_string()).collect() };
        assert_eq!(hidden(&header(&s)), "= ", "only the `=` and its space are hidden");
        assert_eq!(grid(&s)[0][2], "= q·L/2 [kN]", "the calculations still read it");
        assert_eq!(s.run("calc.recalculate", &json!({})).unwrap()["changed"], 0, "stable");
        // Shown while the caret is in that cell, not elsewhere.
        assert!(revealed_header(&s).is_none());
        s.sel = at(0, 2);
        assert_eq!(revealed_header(&s), Some(header(&s).rev));
        s.sel = at(0, 1);
        assert!(revealed_header(&s).is_none());
        // Text typed after the `=` takes its formatting: the next calculation shows it.
        let block = s.doc.body.iter_mut().find(|b| matches!(&***b, Block::Table(_))).unwrap();
        if let Block::Table(t) = Arc::make_mut(block)
            && let Some(Block::Para(p)) = t.rows[0].cells[2].blocks.first_mut().map(Arc::make_mut)
        {
            let props = p.runs[0].props.clone();
            p.insert_text(2, "2·", &props).unwrap();
        }
        s.run("calc.recalculate", &json!({})).unwrap();
        assert_eq!(hidden(&header(&s)), "= ");
        assert_eq!(grid(&s)[1][2], "40");
        // As an equation: the `=` stays hidden text before it.
        s.sel = at(1, 0);
        s.run("table.mathCells", &json!({"value": true})).unwrap();
        let p = header(&s);
        assert_eq!(hidden(&p), "=");
        assert!(matches!(p.objects.as_slice(), [InlineObject::Equation { .. }]));
        assert!(grid(&s)[0][2].starts_with("= 2·q·L"), "{:?}", grid(&s)[0][2]);
        assert_eq!(grid(&s)[1][2], "40");
        assert_eq!(s.run("calc.recalculate", &json!({})).unwrap()["changed"], 0, "stable as an equation");
        s.run("table.mathCells", &json!({"value": false})).unwrap();
        assert_eq!(hidden(&header(&s)), "= ");
        // No longer calculated: the `=` shows.
        s.run("table.calc", &json!({"value": false})).unwrap();
        assert_eq!(hidden(&header(&s)), "");
    }

    /// A header without a spreadsheet-style formula keeps text hidden on purpose; a bare `=` is
    /// a label and shows; a header all hidden (typed over a hidden `=`) shows again.
    #[test]
    fn header_hidden_text_is_left_alone() {
        use wordcraft_doc::{Path, Pos, StoryRef};
        let mut s = session();
        with_table(&mut s, &[&["L [m]", "q note", "=", "x"], &["4", "", "", ""]]);
        let cell = |s: &mut Session, c: usize| -> Paragraph {
            let t = s.doc.body.iter().find_map(|b| if let Block::Table(t) = &**b { Some(t.clone()) } else { None }).unwrap();
            t.rows[0].cells[c].blocks.first().and_then(|b| b.as_para()).unwrap().clone()
        };
        let set = |s: &mut Session, c: usize, from: usize, to: usize| {
            let block = s.doc.body.iter_mut().find(|b| matches!(&***b, Block::Table(_))).unwrap();
            if let Block::Table(t) = Arc::make_mut(block)
                && let Some(Block::Para(p)) = t.rows[0].cells[c].blocks.first_mut().map(Arc::make_mut)
            {
                p.format(from, to, &|c| c.hidden = Some(true)).unwrap();
            }
        };
        // "note" hidden on purpose; the last header all hidden, as typed over a hidden `=`.
        set(&mut s, 1, 2, 6);
        set(&mut s, 3, 0, 1);
        s.sel = crate::Selection::caret(Pos::new(StoryRef::Body, Path(vec![0, 1, 0, 0]), 0));
        s.run("table.calc", &json!({"value": true})).unwrap();
        let hidden =
            |p: &Paragraph| -> String { p.run_ranges().filter(|(_, c)| c.hidden == Some(true)).map(|(r, _)| p.text[r].to_string()).collect() };
        assert_eq!(hidden(&cell(&mut s, 1)), "note");
        assert_eq!(hidden(&cell(&mut s, 2)), "", "a bare `=` shows");
        assert_eq!(hidden(&cell(&mut s, 3)), "", "all hidden: shown again");
        assert_eq!(s.run("calc.recalculate", &json!({})).unwrap()["changed"], 0, "stable");
    }

    /// Turned calculated, a table's first row is its header (Header Row, repeated on each page);
    /// a table of numbers gets an empty header row above them.
    #[test]
    fn a_calculated_table_has_a_header_row() {
        let mut s = session();
        with_table(&mut s, &[&["4", "12"], &["6", "10"]]);
        s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(wordcraft_doc::StoryRef::Body, wordcraft_doc::Path(vec![0, 1, 1, 0]), 0));
        s.run("table.calc", &json!({"value": true})).unwrap();
        assert_eq!(grid(&s), vec![vec!["", ""], vec!["4", "12"], vec!["6", "10"]]);
        let t = s.doc.body.iter().find_map(|b| if let Block::Table(t) = &**b { Some(t.clone()) } else { None }).unwrap();
        assert!(t.props.look.header_row && t.rows[0].props.header && !t.rows[1].props.header);
        let bold = |r: usize| t.rows[r].cells.iter().all(|c| matches!(&*c.blocks[0], Block::Para(p) if p.mark.bold == Some(true)));
        assert!(bold(0) && !bold(1), "the header is bold");
        assert_eq!(s.sel.focus.path.0, vec![0, 2, 1, 0], "the caret moved down with its cell");
        // A table with a header keeps its rows; turning it off and on again adds nothing.
        s.run("table.calc", &json!({"value": false})).unwrap();
        s.run("table.calc", &json!({"value": true})).unwrap();
        assert_eq!(grid(&s).len(), 3);
    }

    /// The ordinal column (column "0"): headed "i", it numbers the data rows 1, 2, 3 (not the
    /// header, a totals row or an empty row), the calculations never read it, and off it goes.
    /// The Total Row sums each column in the last row. Both are kept on save.
    #[test]
    fn ordinal_column_and_total_row() {
        let mut s = session();
        with_table(&mut s, &[&["Beam", "L [m]", "M [m^2] = L^2"], &["B1", "2", ""], &["", "", ""], &["B2", "3", ""], &["", "", ""]]);
        s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(wordcraft_doc::StoryRef::Body, wordcraft_doc::Path(vec![0, 3, 1, 0]), 0));
        let r = s.run("table.calcIndex", &json!({})).unwrap();
        assert_eq!(r["index"], true);
        let g = grid(&s);
        let col = |g: &Vec<Vec<String>>, c: usize| g.iter().map(|r| r[c].clone()).collect::<Vec<_>>();
        assert_eq!(col(&g, 0), vec!["i", "1", "", "2", ""]);
        let t = s.doc.body.iter().find_map(|b| if let Block::Table(t) = &**b { Some(t.clone()) } else { None }).unwrap();
        assert!(matches!(&*t.rows[0].cells[0].blocks[0], Block::Para(p) if p.runs.iter().all(|r| r.props.bold == Some(true))));
        assert_eq!(col(&g, 3), vec!["M [m^2] = L^2", "4", "", "9", ""], "the formula sees L in its own column");
        assert_eq!(s.sel.focus.path.0, vec![0, 3, 2, 0], "the caret moved right with its cell");
        // The Total Row: the last row sums each column (the label and ordinal columns have none);
        // the sign is in the first column only, the sums alone under their columns.
        s.run("table.look", &json!({"totalRow": true})).unwrap();
        s.run("calc.recalculate", &json!({})).unwrap();
        let g = grid(&s);
        assert_eq!(g[4], vec!["Σ", "", "5", "13"], "{g:?}");
        assert_eq!(col(&g, 0), vec!["i", "1", "", "2", "Σ"], "the total row has the sign, no number");
        let info = s.run("calc.tableInfo", &json!({})).unwrap();
        assert_eq!((info["index"].as_bool(), info["total"].as_bool(), info["columnTotal"].as_bool()), (Some(true), Some(true), Some(true)));
        // Stable: recalculating again changes nothing.
        assert_eq!(s.run("calc.recalculate", &json!({})).unwrap()["changed"], 0);
        // Column by column: L not summed leaves its cell empty; summed again it comes back.
        s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(wordcraft_doc::StoryRef::Body, wordcraft_doc::Path(vec![0, 1, 2, 0]), 0));
        assert_eq!(s.run("table.calcTotal", &json!({})).unwrap()["total"], false);
        assert_eq!(grid(&s)[4], vec!["Σ", "", "", "13"]);
        assert_eq!(s.run("calc.tableInfo", &json!({})).unwrap()["columnTotal"], false);
        // A total written in a column's cell (`max =`) stays the user's.
        let bytes = crate::io::save_bytes("d.docx", &s.doc).unwrap();
        let back = crate::io::open_bytes("d.docx", &bytes).unwrap();
        let t = back.body.iter().find_map(|b| if let Block::Table(t) = &**b { Some(t.clone()) } else { None }).unwrap();
        assert!(t.rows[0].cells[2].props.calc_no_total && !t.rows[0].cells[3].props.calc_no_total, "kept in a .docx");
        s.run("table.calcTotal", &json!({"value": true})).unwrap();
        assert_eq!(grid(&s)[4], vec!["Σ", "", "5", "13"]);
        // The total row off: its sums go with it (they would be read as a row of data), and on
        // again they come back.
        s.run("table.look", &json!({"totalRow": false})).unwrap();
        let g = grid(&s);
        assert_eq!(g[4], vec!["", "", "", ""], "{g:?}");
        assert_eq!(col(&g, 3), vec!["M [m^2] = L^2", "4", "", "9", ""]);
        s.run("table.look", &json!({"totalRow": true})).unwrap();
        assert_eq!(grid(&s)[4], vec!["Σ", "", "5", "13"]);
        // Saved and opened again.
        let bytes = crate::io::save_bytes("d.xmath", &s.doc).unwrap();
        let back = crate::io::open_bytes("d.xmath", &bytes).unwrap();
        let t = back.body.iter().find_map(|b| if let Block::Table(t) = &**b { Some(t.props.clone()) } else { None }).unwrap();
        assert!(t.calc_index && t.look.total_row);
        // Off: the column goes, the rest stays.
        s.run("table.calcIndex", &json!({"value": false})).unwrap();
        let g = grid(&s);
        assert_eq!(g[0], vec!["Beam", "L [m]", "M [m^2] = L^2"]);
        assert_eq!(s.sel.focus.path.0, vec![0, 1, 1, 0], "the caret moved left with its cell");
    }

    /// A data table: its values are parameters `F_A` (or `A_F`) for what follows; kept on save.
    #[test]
    fn a_data_table_names_its_cells() {
        let mut s = session();
        with_table(&mut s, &[&["", "F [kN]", "a [m]"], &["A", "10", "2"], &["B", "12", "0.5"]]);
        let r = s.run("table.calcNames", &json!({"value": "columnRow"})).unwrap();
        assert_eq!(r["names"], "columnRow");
        assert_eq!(
            s.run("calc.tableInfo", &json!({})).unwrap(),
            json!({"calc": true, "names": "columnRow", "math": false, "index": false, "total": false, "columnTotal": true})
        );
        s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(
            wordcraft_doc::StoryRef::Body,
            wordcraft_doc::Path(vec![s.doc.body.len() as u32 - 1]),
            0,
        ));
        add(&mut s, "F_B·a_A =");
        assert_eq!(texts(&s).last().map(String::as_str), Some("F_B·a_A=(12kN)·(2m)=24kN·m"), "12 kN × 2 m: a moment");
        // The other way round, then saved and opened again.
        s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(wordcraft_doc::StoryRef::Body, wordcraft_doc::Path(vec![0, 1, 1, 0]), 0));
        s.run("table.calcNames", &json!({"value": "rowColumn"})).unwrap();
        let vars = s.run("calc.variables", &json!({})).unwrap();
        let names: Vec<&str> = vars["variables"].as_array().unwrap().iter().filter_map(|v| v["name"].as_str()).collect();
        assert!(names.contains(&"B_F") && !names.contains(&"F_B"), "{names:?}");
        let bytes = crate::io::save_bytes("d.docx", &s.doc).unwrap();
        let back = crate::io::open_bytes("d.docx", &bytes).unwrap();
        let t = back.body.iter().find_map(|b| if let Block::Table(t) = &**b { Some(t.props.calc_names) } else { None });
        assert_eq!(t, Some(Some(wordcraft_doc::props::CellNames::RowColumn)));
        assert!(s.run("table.calcNames", &json!({"value": "diagonal"})).is_err());
    }

    /// Cells as equations: every cell with text becomes one equation (its units styled), a
    /// formula column is written into them, a recalculation leaves them alone, and turning the
    /// option off gives the text back; the option is saved.
    #[test]
    fn table_cells_shown_as_equations() {
        let mut s = session();
        with_table(&mut s, &[&["", "F_k [kN]", "M [kN·m] = F_k·2 m"], &["A", "10", ""], &["B", "12 kN", ""]]);
        s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(wordcraft_doc::StoryRef::Body, wordcraft_doc::Path(vec![0, 1, 1, 0]), 0));
        let r = s.run("table.mathCells", &json!({"value": true})).unwrap();
        assert_eq!(r["math"], true);
        let table =
            |s: &Session| -> Table { s.doc.body.iter().find_map(|b| if let Block::Table(t) = &**b { Some(t.clone()) } else { None }).unwrap() };
        let t = table(&s);
        let equations = t
            .rows
            .iter()
            .flat_map(|r| &r.cells)
            .filter(|c| matches!(c.blocks.first().map(|b| &**b), Some(Block::Para(p)) if cell_equation(p).is_some()))
            .count();
        assert_eq!(equations, 8, "every cell with text (the corner is empty)");
        assert!(is_label("Support") && is_label("Beam 2") && !is_label("A") && !is_label("12 kN") && !is_label("F_k [kN]") && !is_label("q·L"));
        assert_eq!(table_texts(&s)[2][2], "24", "the formula column, in an equation");
        // Units in the Unit style: `kN` of `12 kN`.
        let unit = unit_color(&mut s);
        let styled = |p: &Paragraph| matches!(p.objects.as_slice(), [InlineObject::Equation { math, .. }] if format!("{:?}", math.nodes).contains(&format!("{unit:?}")));
        assert!(matches!(t.rows[2].cells[1].blocks.first().map(|b| &**b), Some(Block::Para(p)) if styled(p)));
        // Stable.
        let again = s.run("calc.recalculate", &json!({})).unwrap();
        assert_eq!(again["changed"], 0, "{again}");
        // Saved and read back.
        let bytes = crate::io::save_bytes("d.docx", &s.doc).unwrap();
        let back = crate::io::open_bytes("d.docx", &bytes).unwrap();
        assert!(back.body.iter().any(|b| matches!(&**b, Block::Table(t) if t.props.calc_math)));
        // Off: text again.
        s.run("table.mathCells", &json!({"value": false})).unwrap();
        let t = table(&s);
        assert!(t.rows.iter().flat_map(|r| &r.cells).all(|c| matches!(c.blocks.first().map(|b| &**b), Some(Block::Para(p)) if p.objects.is_empty())));
        assert_eq!(table_texts(&s)[2][1], "12 kN");
    }

    /// A table shown as equations reads rows by subscript (`y_(i+1)`: `–` where there is no next
    /// row), its total row has the sign in the first column only and the sums alone, and the
    /// whole of a cell is its equation's place: typing in an empty cell or beside a cell's
    /// equation types in an equation, one undo step.
    #[test]
    fn a_table_of_equations_reads_rows_by_subscript_and_types_in_its_cells() {
        let mut s = session();
        with_table(&mut s, &[&["x_i", "y_i", "= x_i·y_(i+1)"], &["3", "3", ""], &["4", "4", ""], &["", "", ""], &["", "", ""]]);
        s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(wordcraft_doc::StoryRef::Body, wordcraft_doc::Path(vec![0, 1, 0, 0]), 0));
        s.run("table.calcIndex", &json!({"value": true})).unwrap();
        s.run("table.look", &json!({"totalRow": true})).unwrap();
        s.run("table.mathCells", &json!({"value": true})).unwrap();
        let t = table_texts(&s);
        assert_eq!(t[1], vec!["1", "3", "3", "12"], "{t:?}");
        assert_eq!(t[2], vec!["2", "4", "4", "–"], "no row 3 to read: {t:?}");
        assert_eq!(t[4], vec!["Σ", "7", "7", "12"], "the sign in the first column, the sums alone: {t:?}");
        let report = s.run("calc.recalculate", &json!({})).unwrap();
        assert_eq!(report["errors"], json!([]), "{report}");
        assert_eq!(report["changed"], 0, "stable: {report}");
        // The sign and the dash are words, not equations.
        let cell = |s: &Session, r: usize, c: usize| -> Paragraph {
            let t = s.doc.body.iter().find_map(|b| if let Block::Table(t) = &**b { Some(t.clone()) } else { None }).unwrap();
            match t.rows[r].cells[c].blocks.first().map(|b| (**b).clone()) {
                Some(Block::Para(p)) => p,
                _ => Paragraph::new(),
            }
        };
        assert!(cell(&s, 4, 0).objects.is_empty() && cell(&s, 2, 3).objects.is_empty());
        assert!(cell(&s, 4, 2).objects.len() == 1, "a sum is an equation");

        // Typing in the empty cell of row 3 types in an equation there.
        let at = |r: u32, c: u32, off: usize| {
            crate::Selection::caret(wordcraft_doc::Pos::new(wordcraft_doc::StoryRef::Body, wordcraft_doc::Path(vec![0, r, c, 0]), off))
        };
        s.sel = at(3, 1, 0);
        assert!(matches!(math_cell_at(&s, &s.sel.focus.clone()), Some(MathCell::Empty(_))));
        s.run("text.insert", &json!({"text": "5"})).unwrap();
        assert!(s.math.is_some(), "editing the cell's equation");
        assert_eq!(cell(&s, 3, 1).objects.len(), 1);
        assert_eq!(cell_equation(&cell(&s, 3, 1)).as_deref(), Some("5"));
        // One undo step takes the equation and what was typed.
        s.run("edit.undo", &json!({})).unwrap();
        assert!(cell(&s, 3, 1).objects.is_empty() && cell(&s, 3, 1).text.is_empty());
        // Beside a cell's equation (after it), typing goes on in the equation.
        s.math = None;
        s.sel = at(1, 1, wordcraft_doc::para::OBJ.len_utf8());
        assert!(matches!(math_cell_at(&s, &s.sel.focus.clone()), Some(MathCell::Equation(_))));
        s.run("text.insert", &json!({"text": "0"})).unwrap();
        assert_eq!(cell_equation(&cell(&s, 1, 1)).as_deref(), Some("30"));
        // Leaving the cell recalculates: x₁ = 30.
        s.run("equation.exit", &json!({})).unwrap();
        s.sel = at(3, 2, 0);
        s.run("calc.recalculate", &json!({})).unwrap();
        assert_eq!(table_texts(&s)[1][3], "120", "30 · 4");
        // Not a table of equations: typing is typing.
        s.run("table.mathCells", &json!({"value": false})).unwrap();
        s.sel = at(3, 1, 0);
        assert_eq!(math_cell_at(&s, &s.sel.focus.clone()), None);
        s.run("text.insert", &json!({"text": "6"})).unwrap();
        assert!(s.math.is_none() && cell(&s, 3, 1).objects.is_empty());
    }

    /// A calculated table with the default look takes the Calculated Table style: the header's
    /// bottom edge and the total row's top edge heavier than the grid; a style chosen stays.
    #[test]
    fn a_calculated_table_takes_its_style() {
        let mut s = session();
        s.run("calc.insertTable", &json!({"rows": 3, "cols": 2})).unwrap();
        let style = |s: &Session| s.doc.body.iter().find_map(|b| if let Block::Table(t) = &**b { t.props.style.clone() } else { None });
        let id = style(&s).unwrap();
        let st = s.doc.styles.table_style(&id).unwrap();
        let part = |p: wordcraft_doc::styles::TablePart| st.part(p).borders;
        let head = part(wordcraft_doc::styles::TablePart::FirstRow).and_then(|b| b.bottom).unwrap();
        let foot = part(wordcraft_doc::styles::TablePart::LastRow).and_then(|b| b.top).unwrap();
        let grid = part(wordcraft_doc::styles::TablePart::WholeTable).and_then(|b| b.between).unwrap();
        assert!(head.width > grid.width && foot.width > grid.width, "{head:?} {foot:?} {grid:?}");
        assert_eq!(s.doc.styles.get(&id).map(|x| x.name.as_str()), Some(CALC_TABLE_STYLE));
        // A second table reuses it; a table with a style of its own keeps it.
        s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(
            wordcraft_doc::StoryRef::Body,
            wordcraft_doc::Path(vec![s.doc.body.len() as u32 - 1]),
            0,
        ));
        s.run("insert.table", &json!({"rows": 2, "cols": 2})).unwrap();
        s.run("table.newStyle", &json!({"name": "Mine"})).unwrap();
        s.run("table.calc", &json!({"value": true})).unwrap();
        let mine = crate::cmd::table_style::table_style_id(&s, "Mine");
        let styles: Vec<Option<String>> =
            s.doc.body.iter().filter_map(|b| if let Block::Table(t) = &**b { Some(t.props.style.clone()) } else { None }).collect();
        assert_eq!(styles, vec![Some(id), mine]);
    }

    fn table_texts(s: &Session) -> Vec<Vec<String>> {
        s.doc
            .body
            .iter()
            .find_map(|b| match &**b {
                Block::Table(t) => Some(table_cells(t)),
                _ => None,
            })
            .unwrap_or_default()
    }

    #[test]
    fn a_calculated_table_is_the_ordinary_table_with_the_flag_on() {
        let mut s = session();
        add(&mut s, "γ = 1.5");
        s.run("text.insert", &json!({"text": "x"})).unwrap();
        s.run("para.split", &json!({})).ok();
        with_table(&mut s, &[&["L [m]", "q [kN/m]", "M [kN·m] = γ·q·L^2/8"], &["4", "10", ""], &["2", "8", ""], &["", "", "Σ ="]]);
        assert_eq!(table_texts(&s)[1][2], "", "an ordinary table is never calculated");
        let r = s.run("table.calc", &json!({})).unwrap();
        assert_eq!(r["calc"], true, "{r}");
        assert_eq!(r["recalculated"]["tables"], 1, "{r}");
        let t = table_texts(&s);
        assert_eq!(t[1][2], "30");
        assert_eq!(t[2][2], "6");
        assert_eq!(t[3][2], "Σ = 36");
        // The columns are vectors for later calculations.
        let vars = s.run("calc.variables", &json!({})).unwrap();
        let names: Vec<&str> = vars["variables"].as_array().unwrap().iter().map(|v| v["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["γ", "L", "q", "M"]);
        // The flag survives saving.
        let bytes = crate::io::save_bytes("w.xmath", &s.doc).unwrap();
        let doc = crate::io::open_bytes("w.xmath", &bytes).unwrap();
        assert!(doc.body.iter().any(|b| matches!(&**b, Block::Table(t) if t.props.calc)));
        // Turned off, the cells keep their last values and are no longer read.
        assert_eq!(s.run("table.calc", &json!({"value": false})).unwrap()["recalculated"]["tables"], 0);
    }

    #[test]
    fn editing_a_calculation_recalculates_by_itself() {
        let mut s = session();
        add(&mut s, "a = 2 m");
        add(&mut s, "a*3 =");
        assert_eq!(texts(&s)[1], "a*3=(2m)·3=6m");
        // Change the definition and leave the equation: no Recalculate needed.
        s.sel = crate::Selection::caret(wordcraft_doc::Pos { off: 0, ..s.sel.focus.clone() });
        s.run("equation.set", &json!({"linear": "a = 5 m"})).unwrap();
        s.run("equation.exit", &json!({})).ok();
        assert_eq!(texts(&s), ["a=5m", "a*3=(5m)·3=15m"]);
    }

    #[test]
    fn a_calculated_table_recalculates_when_the_caret_leaves_the_cell() {
        let mut s = session();
        with_table(&mut s, &[&["L [m]", "A [m²] = L^2"], &["2", ""]]);
        s.run("table.calc", &json!({})).unwrap();
        assert_eq!(table_texts(&s)[1][1], "4");
        // Type a new value into L's cell: nothing is rewritten while the caret is in the table.
        let Some((tp, _, _)) = s.sel.focus.path.cell() else { panic!("in the table") };
        let mut cell = tp.clone();
        cell.0.extend([1, 0, 0]);
        s.sel = crate::Selection::caret(wordcraft_doc::Pos { story: s.sel.focus.story, path: cell, off: 1 });
        s.run("text.insert", &json!({"text": "0"})).unwrap();
        assert_eq!(table_texts(&s)[1][0], "20");
        assert_eq!(table_texts(&s)[1][1], "4", "not while typing in the table");
        // Leave the table: it recalculates.
        s.run("caret.docEnd", &json!({})).unwrap();
        assert_eq!(table_texts(&s)[1][1], "400");
    }

    #[test]
    fn number_format_options_round_every_result() {
        let mut s = session();
        add(&mut s, "x = 2/3 =");
        assert_eq!(texts(&s), ["x=2/3=0.667"]);
        s.run("calc.options", &json!({"digits": 2, "count": "decimals", "zeros": true})).unwrap();
        assert_eq!(texts(&s), ["x=2/3=0.67"]);
        assert!(s.run("calc.options", &json!({"count": "nope"})).is_err());
    }

    /// The runs of the `n`th equation as (text, coloured) pairs.
    fn runs(s: &Session, n: usize) -> Vec<(String, bool)> {
        let mut out = Vec::new();
        let mut k = 0;
        for b in &s.doc.body {
            if let Block::Para(p) = &**b {
                for o in &p.objects {
                    if let InlineObject::Equation { math, .. } = o {
                        if k == n {
                            let mut nodes = math.nodes.clone();
                            wordcraft_doc::math_edit::each_run_mut(&mut nodes, &mut |r| out.push((r.text.clone(), r.color.is_some())));
                        }
                        k += 1;
                    }
                }
            }
        }
        out
    }

    #[test]
    fn units_take_the_unit_style() {
        let mut s = session();
        add(&mut s, "L = 4.5 m");
        add(&mut s, "M = L·2 kN =");
        let coloured = |s: &Session, n| runs(s, n).into_iter().filter(|(_, c)| *c).map(|(t, _)| t).collect::<Vec<_>>();
        assert_eq!(coloured(&s, 0), ["m"]);
        // Also in the substitution (Show Steps): `(4.5 m)·2 kN`.
        assert_eq!(coloured(&s, 1), ["kN", "m", "kN", "kN", "m"], "{:?}", runs(&s, 1));
        // A unit with a power typed as `^2` is styled whole.
        add(&mut s, "A = 50 cm^2");
        assert_eq!(coloured(&s, 2), ["cm"], "{:?}", runs(&s, 2));
        // A parameter spelled like a unit is not a unit.
        add(&mut s, "m = 3 kg");
        add(&mut s, "m*2 =");
        assert_eq!(coloured(&s, 4), ["kg", "kg"], "{:?}", runs(&s, 4));
        // In steps, `m` is the mass in the formula and the metre in the result.
        add(&mut s, "F = 10 kN");
        add(&mut s, "F = m·a");
        let rows = runs(&s, 6);
        assert!(rows.iter().any(|(t, c)| t.contains('m') && !c), "the mass m is not a unit: {rows:?}");
        assert!(rows.iter().any(|(t, c)| t == "m" && *c), "the metre in the result is: {rows:?}");
        // The style's colour is the units' colour.
        let st = std::sync::Arc::make_mut(&mut s.doc.styles).get_mut(UNIT_STYLE).unwrap();
        st.chr.color = Some(TextColor::Rgb(Rgb(200, 0, 0)));
        s.run("calc.recalculate", &json!({})).unwrap();
        let mut colors = Vec::new();
        if let Some(Block::Para(p)) = s.doc.body.first().map(|b| &**b) {
            for o in &p.objects {
                if let InlineObject::Equation { math, .. } = o {
                    let mut nodes = math.nodes.clone();
                    wordcraft_doc::math_edit::each_run_mut(&mut nodes, &mut |r| colors.extend(r.color));
                }
            }
        }
        assert!(colors.contains(&Rgb(200, 0, 0)), "{colors:?}");
        // Styling twice changes nothing.
        assert_eq!(s.run("calc.recalculate", &json!({})).unwrap()["changed"], 0);
    }

    #[test]
    fn a_users_equation_array_is_never_cut_to_its_first_row() {
        let mut s = session();
        s.run("insert.equation", &json!({"linear": "█(x=1@y=2)", "edit": false, "calc": true})).unwrap();
        s.run("calc.recalculate", &json!({})).unwrap();
        let back = Session::new(crate::io::open_bytes("a.xmath", &crate::io::save_bytes("a.xmath", &s.doc).unwrap()).unwrap());
        for doc in [&s.doc, &back.doc] {
            let full: Vec<String> =
                items(&doc.body).into_iter().filter_map(|i| if let Item::Equation { full, .. } = i { Some(full) } else { None }).collect();
            assert!(full[0].contains('@'), "both rows stay: {full:?}");
        }
        // Strix's own steps are marked: read back, the statement is the first row, with the
        // result of the last.
        let mut s = session();
        s.run("calc.layout", &json!({"value": "lines"})).unwrap();
        for t in ["F = 10 N", "m = 4 kg", "F = m·a"] {
            add(&mut s, t);
        }
        let opened = crate::io::open_bytes("b.xmath", &crate::io::save_bytes("b.xmath", &s.doc).unwrap()).unwrap();
        assert_eq!(regions(&opened.body)[2].1.as_deref().map(|t| t.replace(char::is_whitespace, "")).as_deref(), Some("F=m·a⇒a=2.5m/s²"));
    }

    /// The calc equations' maths, in order.
    fn maths(s: &Session) -> Vec<Math> {
        let mut out = Vec::new();
        for b in &s.doc.body {
            if let Block::Para(p) = &**b {
                for o in &p.objects {
                    if let InlineObject::Equation { math, .. } = o
                        && math.calc
                    {
                        out.push(math.clone());
                    }
                }
            }
        }
        out
    }

    /// A unit's quotient (`kN/m`) reads inline in a calculation, its formula's fractions stacked;
    /// recalculating again changes nothing.
    #[test]
    fn a_units_quotient_reads_inline_and_stays_put() {
        let mut s = session();
        s.run("calc.steps", &json!({"value": true})).unwrap();
        for t in ["L = 4.5 m", "q = 12 kN/m", "M = q·L^2/8 ="] {
            s.run("calc.insert", &json!({"text": t})).unwrap();
            s.run("text.newParagraph", &json!({})).unwrap();
        }
        s.run("calc.recalculate", &json!({})).unwrap();
        let all = maths(&s);
        // In the linear text a linear fraction is `∕`, a stacked one `/`.
        let text = to_linear(&all[2].nodes);
        assert!(text.contains("kN∕m"), "kN/m inline: {text}");
        assert!(text.contains(")/8") || text.contains("/8"), "L²/8 stacked: {text}");
        // Recalculated again, nothing changes.
        let again = s.run("calc.recalculate", &json!({})).unwrap();
        assert_eq!(again["changed"], 0, "{again}");
        assert_eq!(maths(&s), all);
    }

    /// A value with a unit quotient (`v = 90 km/h`) keeps its number before the unit, which alone
    /// is the fraction (`90 km∕h`, not `90 km` over `h`); a formula's fraction stays stacked.
    #[test]
    fn a_values_number_stays_out_of_its_units_quotient() {
        let mut s = session();
        for t in ["v_0 = 0 km/h", "v = 90 km/h", "t = 10 s", "a = (v - v_0)/t ="] {
            s.run("calc.insert", &json!({"text": t})).unwrap();
            s.run("text.newParagraph", &json!({})).unwrap();
        }
        s.run("calc.recalculate", &json!({})).unwrap();
        let all = maths(&s);
        let lin = |m: &Math| to_linear(&m.nodes);
        // A thin space parts the number from its unit, as everywhere else in a value.
        assert_eq!(lin(&all[0]), "v_0=0\u{2009}km∕h");
        assert_eq!(lin(&all[1]), "v=90\u{2009}km∕h");
        assert!(lin(&all[3]).starts_with("a=(v-v_0)/t"), "{}", lin(&all[3]));
        let again = s.run("calc.recalculate", &json!({})).unwrap();
        assert_eq!(again["changed"], 0, "{again}");
        assert_eq!(maths(&s), all);
    }

    /// Only a well-formed number is taken out of a unit quotient: a sign at the start, digits
    /// that `.` or `,` may part, ending on a digit.
    #[test]
    fn a_coefficient_is_a_well_formed_number() {
        assert_eq!(leading_number("90 km"), Some("90"));
        assert_eq!(leading_number("−9.81 m"), Some("−9.81"));
        assert_eq!(leading_number("2,5 m"), Some("2,5"));
        assert_eq!(leading_number("1.000,5 m"), Some("1.000,5"));
        assert_eq!(leading_number("90"), Some("90"));
        for junk in ["km", "", "-", "3- km", "1,,2 m", "2. m", ".5 m", "+-3 m", "1−2 m"] {
            assert_eq!(leading_number(junk), None, "{junk:?}");
        }
    }

    /// The number taken out of `90 km/h` keeps the numerator's formatting (size, font, colour, a
    /// break before it), loses the units' colour, and the units left behind lose the break.
    #[test]
    fn a_coefficient_keeps_the_numerators_formatting() {
        use wordcraft_doc::math::{MNode, MRun};
        let units = vec!["km".to_string(), "h".to_string()];
        let unit_color = Rgb(0x20, 0x60, 0xA0);
        let red = Rgb(0xC0, 0, 0);
        let styled =
            MRun { text: "90 km".into(), size: Some(14.0), color: Some(red), font: Some("Cambria Math".into()), brk: Some(0), ..MRun::default() };
        let mut num = vec![MNode::Run(styled.clone())];
        let run = split_coefficient(&mut num, &units, unit_color).expect("a number then units");
        assert_eq!(run, MRun { text: "90".into(), ..styled.clone() });
        assert_eq!(num, vec![MNode::Run(MRun { text: "km".into(), brk: None, ..styled })]);
        // A run in the units' colour gives a plain number.
        let mut num = vec![MNode::Run(MRun { text: "3 km".into(), color: Some(unit_color), ..MRun::default() })];
        assert_eq!(split_coefficient(&mut num, &units, unit_color).map(|r| r.color), Some(None));
        // No units after the number, or something that isn't a unit: left alone.
        for text in ["1", "2 x", "3- km"] {
            let mut num = vec![MNode::Run(MRun::new(text))];
            assert!(split_coefficient(&mut num, &units, unit_color).is_none(), "{text}");
            assert_eq!(num, vec![MNode::Run(MRun::new(text))]);
        }
    }

    /// More values with a unit quotient: decimal commas, negative numbers, a unit squared, a
    /// value used in a formula; `1/s` alone (no unit on top) stays a fraction.
    #[test]
    fn values_with_unit_quotients_in_their_forms() {
        let mut s = session();
        for t in ["u = 2,5 m/s", "g = −9.81 m/s^2", "f = 1/s", "x = 3 m/s · 4 s ="] {
            s.run("calc.insert", &json!({"text": t})).unwrap();
            s.run("text.newParagraph", &json!({})).unwrap();
        }
        s.run("calc.recalculate", &json!({})).unwrap();
        let all = maths(&s);
        let lin: Vec<String> = all.iter().map(|m| to_linear(&m.nodes)).collect();
        assert_eq!(lin[0], "u=2,5\u{2009}m∕s");
        assert_eq!(lin[1], "g=−9.81\u{2009}m∕(s^2)");
        assert_eq!(lin[2], "f=1/s", "no unit over the bar: the fraction stays stacked");
        assert_eq!(lin[3], "x=3\u{2009}m∕s·4\u{2009}s=12\u{2009}m");
        let again = s.run("calc.recalculate", &json!({})).unwrap();
        assert_eq!(again["changed"], 0, "{again}");
    }

    /// The owner's beam, typed as in the app: with Show Steps every formula and check shows its
    /// working, on its line or on lines under it; recalculating, saving and reopening keep it.
    #[test]
    fn a_beam_shows_its_steps_in_either_layout() {
        for layout in ["inline", "lines"] {
            let mut s = session();
            s.run("calc.steps", &json!({"value": true})).unwrap();
            s.run("calc.layout", &json!({"value": layout})).unwrap();
            for t in ["L = 4.5 m", "q = 12 kN/m", "M = q·L^2/8 =", "W = 1000 cm^3", "f_yd = 355 MPa", "σ = M/W =", "σ ≤ f_yd"] {
                s.run("calc.insert", &json!({"text": t})).unwrap();
                s.run("text.newParagraph", &json!({})).unwrap();
            }
            let r = s.run("calc.recalculate", &json!({})).unwrap();
            assert_eq!(r["errors"].as_array().map_or(0, Vec::len), 0, "{r}");
            let all = maths(&s);
            assert_eq!(all.len(), 7);
            let lin = |m: &Math| to_linear(&m.nodes).replace(char::is_whitespace, "");
            for k in [2, 5, 6] {
                let m = &all[k];
                match layout {
                    "inline" => assert!(!m.stepped && !matches!(m.nodes.as_slice(), [wordcraft_doc::math::MNode::EqArr { .. }]), "{}", lin(m)),
                    _ => assert!(m.stepped && matches!(m.nodes.as_slice(), [wordcraft_doc::math::MNode::EqArr { .. }]), "{}", lin(m)),
                }
            }
            let text = lin(&all[2]);
            // The unit's quotient reads inline (a linear fraction), the formula's stacked.
            assert!(text.contains("(12kN∕m)·") && text.contains("(4.5m)²") && text.trim_end_matches(')').ends_with("30.375kN·m"), "{text}");
            assert!(lin(&all[5]).contains("(30.375kN·m)"), "{}", lin(&all[5]));
            assert!(lin(&all[6]).contains("30.375MPa") && lin(&all[6]).contains('✓'), "{}", lin(&all[6]));
            // The check's units are units, on either layout.
            let mut coloured = Vec::new();
            let mut nodes = all[6].nodes.clone();
            wordcraft_doc::math_edit::each_run_mut(&mut nodes, &mut |r| {
                if r.color.is_some() {
                    coloured.push(r.text.clone());
                }
            });
            assert_eq!(coloured, ["MPa", "MPa"], "{layout}");
            // Stable: recalculating, saving and reopening change nothing.
            let again = s.run("calc.recalculate", &json!({})).unwrap()["changed"].clone();
            assert_eq!(maths(&s).iter().map(lin).collect::<Vec<_>>(), all.iter().map(lin).collect::<Vec<_>>());
            assert_eq!(again, 0, "{layout}");
            let bytes = crate::io::save_bytes("beam.xmath", &s.doc).unwrap();
            let opened = crate::io::open_bytes("beam.xmath", &bytes).unwrap();
            let read: Vec<Option<String>> = regions(&opened.body).into_iter().map(|(_, t)| t).collect();
            assert_eq!(read, regions(&s.doc.body).into_iter().map(|(_, t)| t).collect::<Vec<_>>(), "{layout}");
            // A new session recalculates with its own preferences; with these, as it was.
            let mut back = Session::new(opened);
            back.prefs = s.prefs.clone();
            back.run("calc.recalculate", &json!({})).unwrap();
            assert_eq!(back.run("calc.recalculate", &json!({})).unwrap()["changed"], 0, "{layout}");
            assert_eq!(maths(&back).iter().map(lin).collect::<Vec<_>>(), all.iter().map(lin).collect::<Vec<_>>());
            // Switching the layout lays the same out the other way, and back.
            let other = if layout == "inline" { "lines" } else { "inline" };
            back.run("calc.layout", &json!({"value": other})).unwrap();
            assert_ne!(maths(&back).iter().map(lin).collect::<Vec<_>>(), all.iter().map(lin).collect::<Vec<_>>());
            back.run("calc.layout", &json!({"value": layout})).unwrap();
            assert_eq!(maths(&back).iter().map(lin).collect::<Vec<_>>(), all.iter().map(lin).collect::<Vec<_>>());
        }
    }

    /// The algebra of a solved equation, and a note after a result found numerically.
    #[test]
    fn solved_equations_show_their_algebra() {
        let mut s = session();
        s.run("calc.layout", &json!({"value": "lines"})).unwrap();
        for t in ["m = 400 kg", "F = 1 kN", "F = m·a", "k = 10", "x^3 + x = k"] {
            add(&mut s, t);
        }
        let all = maths(&s);
        let rows = |m: &Math| match m.nodes.as_slice() {
            [wordcraft_doc::math::MNode::EqArr { rows }] => rows.iter().map(|r| to_linear(r).replace(char::is_whitespace, "")).collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        assert_eq!(rows(&all[2]).len(), 4, "{:?}", rows(&all[2]));
        assert!(rows(&all[2])[1].starts_with("a&=F/m"), "{:?}", rows(&all[2]));
        let last = rows(&all[4]);
        assert_eq!(last.last().map(String::as_str), Some("x&=2(numericalsolution)"), "{last:?}");
        assert_eq!(s.run("calc.recalculate", &json!({})).unwrap()["changed"], 0);
        // Inline: the note ends the line; read back, it is not part of the statement.
        s.run("calc.layout", &json!({"value": "inline"})).unwrap();
        assert_eq!(texts(&s)[4], "x^3+x=k⇒x³+x=10⇒x=2");
        assert_eq!(s.run("calc.recalculate", &json!({})).unwrap()["changed"], 0);
        let bytes = crate::io::save_bytes("n.xmath", &s.doc).unwrap();
        let opened = crate::io::open_bytes("n.xmath", &bytes).unwrap();
        assert_eq!(regions(&opened.body)[4].1.as_deref().map(|t| t.replace(char::is_whitespace, "")).as_deref(), Some("x^3+x=k⇒x³+x=10⇒x=2"));
        // The layout is a preference, given by name.
        assert!(s.run("calc.layout", &json!({"value": "sideways"})).is_err());
        assert!(s.run("calc.options", &json!({"layout": 3})).is_ok());
        assert_eq!(s.run("calc.options", &json!({"layout": "lines"})).unwrap()["layout"], "lines");
        assert_eq!(s.prefs.calc_layout, "lines");
    }

    #[test]
    fn variables_inspect_document_and_exercise_sandboxes_with_unit_conversion() {
        let mut s = session();
        add(&mut s, "L = 2.5 m");
        s.run("exercise.insert", &json!({"title": "Trave", "questions": ["Momento"]})).unwrap();
        s.run(
            "exercise.setData",
            &json!({"exercise": 1, "data": {"items": [{"symbol": "L", "unit": "m", "values": ["4.5", "6"]}], "variants": ["A", "B"], "active": 0}}),
        )
        .unwrap();
        let out = s.run("calc.variables", &json!({"units": {"document:L": "cm", "exercise:1:L": "mm"}})).unwrap();
        let doc_vars = out["variables"].as_array().unwrap();
        assert_eq!(doc_vars.len(), 1);
        assert_eq!(doc_vars[0]["name"], "L");
        assert_eq!(doc_vars[0]["converted"], "250 cm");
        assert!(doc_vars[0]["compatibleUnits"].as_array().unwrap().iter().any(|u| u == "mm"));
        assert!(doc_vars[0]["pos"].is_object());
        let ex_list = out["exercises"].as_array().unwrap();
        assert_eq!(ex_list.len(), 1);
        assert_eq!(ex_list[0]["exercise"], 1);
        assert_eq!(ex_list[0]["variant"], "A");
        assert_eq!(ex_list[0]["variables"][0]["converted"], "4500 mm");
        assert!(ex_list[0]["variables"][0]["pos"].is_object());
        assert_eq!(s.run("view.variablesPane", &json!({})).unwrap()["value"], true);
        assert!(s.view.variables_pane);
        assert_eq!(s.run("view.variablesPane", &json!({"value": false})).unwrap()["value"], false);
        assert!(!s.view.variables_pane);
    }

    #[test]
    fn hostile_insert_params_never_panic() {
        let mut s = session();
        for v in
            [json!({}), json!({"text": ""}), json!({"text": 5}), json!({"text": "(".repeat(100_000)}), json!({"text": "x := 1", "digits": u64::MAX})]
        {
            let _ = s.run("calc.insert", &v);
        }
        let _ = s.run("calc.recalculate", &json!({"digits": -1, "decimalComma": "yes"}));
        let _ = s.run("calc.variables", &json!({"digits": 0}));
    }

    /// Turning the ordinal column on again after the table stopped calculating reuses the one
    /// it has, never adding a second.
    #[test]
    fn the_ordinal_column_comes_back_once() {
        let mut s = session();
        s.run("calc.insertTable", &json!({})).unwrap();
        s.run("table.calcIndex", &json!({})).unwrap();
        s.run("table.calc", &json!({"value": false})).unwrap();
        s.run("table.calcIndex", &json!({})).unwrap();
        let header = table_texts(&s).first().cloned().unwrap_or_default();
        assert_eq!(header.iter().filter(|c| *c == "i").count(), 1, "{header:?}");
        // Off for good: the column goes, once.
        s.run("table.calcIndex", &json!({"value": false})).unwrap();
        let header = table_texts(&s).first().cloned().unwrap_or_default();
        assert!(!header.iter().any(|c| c == "i"), "{header:?}");
    }

    #[test]
    fn numeric_solution_note_follows_all_four_languages() {
        let mut s = session();
        s.run("calc.layout", &json!({"value": "lines"})).unwrap();
        add(&mut s, "x^3 + x = 10");
        for (lang, want) in
            [("en-US", "(numericalsolution)"), ("it-IT", "(soluzionenumerica)"), ("de-DE", "(numerischeLösung)"), ("fr-FR", "(solutionnumérique)")]
        {
            s.run("review.language", &json!({"lang": lang, "default": true})).unwrap();
            let all = maths(&s);
            let last = match all[0].nodes.as_slice() {
                [wordcraft_doc::math::MNode::EqArr { rows }] => {
                    rows.last().map(|r| to_linear(r).replace(char::is_whitespace, "")).unwrap_or_default()
                }
                _ => String::new(),
            };
            assert!(last.ends_with(want), "{lang}: got {last}");
        }
    }

    #[test]
    fn calc_options_follow_hierarchy_and_persist_in_docx_and_xmath() {
        let mut s = session();
        // Prefs = 1 digit, Document = 2 digits, Exercise = 4 digits, Equation = 5 digits.
        s.run("calc.options", &json!({"digits": 1, "count": "decimals", "zeros": true, "scope": "prefs"})).unwrap();
        s.run("calc.options", &json!({"digits": 2, "count": "decimals", "zeros": true, "scope": "document"})).unwrap();
        add(&mut s, "a = 1/8 =");

        // Insert a problem exercise and put two equations in its solution.
        s.run("exercise.insert", &json!({})).unwrap();
        s.run("calc.options", &json!({"digits": 4, "count": "decimals", "zeros": true, "scope": "exercise", "exercise": 1})).unwrap();
        let sol = s.doc.body.iter().position(|b| b.as_para().is_some_and(|p| p.props.style.as_deref() == Some("ExercisePrompt"))).unwrap() + 1;
        s.sel = crate::Selection::caret(wordcraft_doc::Pos::new(wordcraft_doc::StoryRef::Body, wordcraft_doc::Path(vec![sol as u32]), 0));
        add(&mut s, "b = 1/8 =");
        s.run("calc.insert", &json!({"text": "c = 1/8 ="})).unwrap();
        // Caret right after `calc.insert` is adjacent to equation `c`: override at equation scope.
        s.run("calc.options", &json!({"digits": 5, "scope": "equation"})).unwrap();

        let all = texts(&s);
        assert!(all.iter().any(|t| t == "a=1/8=0.13"), "document scope 2 decimals: {all:?}");
        assert!(all.iter().any(|t| t == "b=1/8=0.1250"), "exercise scope 4 decimals: {all:?}");
        assert!(all.iter().any(|t| t == "c=1/8=0.12500"), "equation scope 5 decimals: {all:?}");

        // Save to both .docx and .xmath, then reopen in a fresh session with completely different Prefs (0 decimals).
        for ext in ["docx", "xmath"] {
            let name = format!("hierarchy.{ext}");
            let bytes = crate::io::save_bytes(&name, &s.doc).unwrap();
            let reopened_doc = crate::io::open_bytes(&name, &bytes).unwrap();
            let mut s2 = Session::new(reopened_doc);
            s2.prefs.calc_digits = 0;
            s2.prefs.calc_count = "decimals".into();
            s2.run("calc.recalculate", &json!({})).unwrap();
            let after = texts(&s2);
            assert!(after.iter().any(|t| t == "a=1/8=0.13"), "{ext}: document options persisted over prefs: {after:?}");
            assert!(after.iter().any(|t| t == "b=1/8=0.1250"), "{ext}: exercise options persisted: {after:?}");
            assert!(after.iter().any(|t| t == "c=1/8=0.12500"), "{ext}: equation options persisted: {after:?}");
        }

        // New documents inherit from Prefs at creation time.
        s.prefs.calc_digits = 3;
        s.prefs.calc_count = "decimals".into();
        s.prefs.calc_zeros = true;
        s.run("file.new", &json!({})).unwrap();
        s.prefs.calc_digits = 0;
        add(&mut s, "d = 1/8 =");
        let new_texts = texts(&s);
        assert!(new_texts.iter().any(|t| t == "d=1/8=0.125"), "new doc inherited 3 decimals from prefs at creation: {new_texts:?}");
    }
}
