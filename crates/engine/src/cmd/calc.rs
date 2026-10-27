//! Live calculations (docs/calc.md): calc regions are equations marked as calculated
//! (`Math::calc`, the Calculate button on an equation) whose linear text is a `strix-calc`
//! statement (`L = 4.5 m`, `M = q·L²/8`, `F = m·a`), and calculated tables (`TableProps::calc`,
//! Table Layout › Data › Calculated Table): named columns with units, column formulas and
//! totals. Both are evaluated in reading order with one shared set of parameters, and results
//! are written back into the equations and cells. Other equations and tables are never read or
//! changed.

use std::sync::Arc;

use serde_json::{Value, json};
use strix_calc::{Count, Options, RegionKind, RegionResult, TableResult, Worksheet};
use strix_doc::math::{Math, parse_linear, to_linear};
use strix_doc::{Block, InlineObject, Paragraph, Table};

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
            let report = recalculate(s, &options(s, v));
            Ok(report)
        })
        .params(r#"{"digits"?: n, "count"?: "auto|decimals|significant", "zeros"?: bool, "decimalComma"?: bool} → {"regions": n, "tables": n, "changed": n, "errors": [{"region"|"table": n, "row"?, "col"?, "text", "error"}]}"#),
        CommandSpec::new("equation.calc", "Calculate", "Equation › Tools", toggle_equation)
            .when(|s| if super::equation::equation_at_caret(s).is_some() { None } else { Some("no equation at the caret") })
            .params(r#"{"value"?: bool (default: toggle)} → {"calc": bool, "recalculated"}: the equation at the caret is a calculation region"#),
        CommandSpec::new("calc.newEquations", "Calculate New Equations", "Math › Calculations", |s, v| {
            s.prefs.calc_new_equations = p::bool(v, "value").unwrap_or(!s.prefs.calc_new_equations);
            Ok(json!({"calcNewEquations": s.prefs.calc_new_equations}))
        })
        .pure()
        .params(r#"{"value"?: bool (default: toggle)}: new equations are calculation regions (a preference)"#),
        CommandSpec::new("calc.options", "Number Format", "Math › Results", set_options)
            .params(r#"{"digits"?: 0–15, "count"?: "auto|decimals|significant", "zeros"?: bool} → the format: how results are rounded (a preference), then recalculates"#),
        CommandSpec::new("table.calc", "Calculated Table", "Table Layout › Data", toggle_table)
            .when(|s| if s.sel.focus.path.cell().is_some() { None } else { Some("the cursor isn't in a table") })
            .params(r#"{"value"?: bool (default: toggle)} → {"calc": bool, "recalculated"}: the table at the caret is calculated: header cells name its columns (`L [m]`, `M [kN·m] = q·L^2/8`), `Σ =` cells total them"#),
        CommandSpec::new("calc.insertTable", "Calculated Table", "Math › Tables", insert_table)
            .params(r#"{"rows"?: n (default 4), "cols"?: n (default 3)} → inserts a table and makes it calculated (see table.calc)"#),
        CommandSpec::new("calc.tableInfo", "Calculated Table Info", "Math › Tables", |s, _| {
            let calc = s.sel.focus.path.cell().and_then(|(tp, _, _)| s.doc.table(s.sel.focus.story, &tp)).is_some_and(|t| t.props.calc);
            Ok(json!({"calc": calc}))
        })
        .pure()
        .params(r#"→ {"calc": bool}: whether the table at the caret is calculated"#),
        CommandSpec::new("calc.variables", "Variables", "Math › Results", variables)
            .pure()
            .params(r#"{"digits"?: n} → {"variables": [{"name", "value" (number in the unit shown, or an array), "text", "unit", "si" (SI magnitude, or an array), "region" (index of the defining item)}]}"#),
    ]
}

/// The number format: the preferences, overridden by the call's parameters.
fn options(s: &Session, v: &Value) -> Options {
    let mut o = Options { digits: s.prefs.calc_digits.min(15), count: count(&s.prefs.calc_count), zeros: s.prefs.calc_zeros, ..Options::default() };
    if let Some(d) = p::u64(v, "digits") {
        o.digits = u8::try_from(d.min(15)).unwrap_or(3);
    }
    if let Some(c) = p::str(v, "count") {
        o.count = count(c);
    }
    if let Some(z) = p::bool(v, "zeros") {
        o.zeros = z;
    }
    if let Some(c) = p::bool(v, "decimalComma") {
        o.decimal_comma = c;
    }
    o
}

fn count(name: &str) -> Count {
    match name {
        "decimals" => Count::Decimals,
        "significant" => Count::Significant,
        _ => Count::Auto,
    }
}

fn set_options(s: &mut Session, v: &Value) -> CmdResult {
    if let Some(d) = p::u64(v, "digits") {
        s.prefs.calc_digits = u8::try_from(d.min(15)).unwrap_or(3);
    }
    if let Some(c) = p::str(v, "count") {
        if !matches!(c, "auto" | "decimals" | "significant") {
            return Err(CmdError::Params("count is auto, decimals or significant".into()));
        }
        s.prefs.calc_count = c.to_string();
    }
    if let Some(z) = p::bool(v, "zeros") {
        s.prefs.calc_zeros = z;
    }
    let report = recalculate(s, &options(s, &json!({})));
    Ok(json!({"digits": s.prefs.calc_digits, "count": s.prefs.calc_count, "zeros": s.prefs.calc_zeros, "recalculated": report}))
}

/// The linear text of an equation, as the calc engine reads it.
fn equation_text(linear: &str, math: &Math) -> String {
    if math.nodes.is_empty() || math.linear { linear.to_string() } else { to_linear(&math.nodes) }
}

/// What the worksheet reads, in reading order.
#[derive(Debug, Clone, PartialEq)]
enum Item {
    /// An equation in top-level block `top`: its text if it is a calc region, `None` otherwise.
    Equation { top: usize, text: Option<String> },
    /// A calculated table (top-level block `top`): its cells' text, row by row.
    Table { top: usize, cells: Vec<Vec<String>> },
}

/// A cell's text as the calc engine reads it: its first paragraph.
fn cell_text(blocks: &[Arc<Block>]) -> String {
    match blocks.first().map(|b| &**b) {
        Some(Block::Para(p)) => p.plain_text().trim().to_string(),
        _ => String::new(),
    }
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
                    if let InlineObject::Equation { linear, math, .. } = o {
                        out.push(Item::Equation { top, text: math.calc.then(|| equation_text(linear, math)) });
                    }
                }
            }
            Block::Table(t) if depth == 0 && t.props.calc => out.push(Item::Table { top, cells: table_cells(t) }),
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
            Item::Equation { top, text } => Some((top, text)),
            Item::Table { .. } => None,
        })
        .collect()
}

/// What evaluating an item gave.
enum Outcome {
    Equation(Option<RegionResult>),
    Table(TableResult),
}

/// Evaluates the items in order with one worksheet.
fn run_sheet(found: &[Item], opts: &Options) -> (Worksheet, Vec<Outcome>) {
    let mut ws = Worksheet::new(*opts);
    let outcomes = found
        .iter()
        .map(|item| match item {
            Item::Equation { text: Some(text), .. } => Outcome::Equation(Some(ws.eval_statement(text))),
            Item::Equation { text: None, .. } => Outcome::Equation(None),
            Item::Table { cells, .. } => Outcome::Table(ws.eval_table(cells)),
        })
        .collect();
    (ws, outcomes)
}

/// Evaluates every region and calculated table and writes changed results back. Returns the
/// report JSON.
pub(crate) fn recalculate(s: &mut Session, opts: &Options) -> Value {
    let found = items(&s.doc.body);
    let (_, outcomes) = run_sheet(&found, opts);
    let mut updates: Vec<Option<String>> = Vec::new();
    let mut cell_updates: Vec<(usize, Vec<(usize, usize, String)>)> = Vec::new();
    let mut errors = Vec::new();
    let (mut calc_regions, mut tables) = (0usize, 0usize);
    for (i, (item, outcome)) in found.iter().zip(&outcomes).enumerate() {
        match (item, outcome) {
            (Item::Equation { text: Some(text), .. }, Outcome::Equation(Some(r))) => {
                if r.kind != RegionKind::NotCalc {
                    calc_regions += 1;
                }
                if let Some(e) = &r.error {
                    errors.push(json!({"region": i, "text": text, "error": e.to_string()}));
                }
                // The linear format of a built-up equation drops spaces: only a real change rewrites it.
                let same = |d: &String| {
                    d.split_whitespace().eq(text.split_whitespace()) || d.replace(char::is_whitespace, "") == text.replace(char::is_whitespace, "")
                };
                updates.push(r.display.clone().filter(|d| !same(d)));
            }
            (Item::Equation { .. }, _) => updates.push(None),
            (Item::Table { top, cells }, Outcome::Table(t)) => {
                tables += 1;
                for (row, col, e) in &t.errors {
                    let text = cells.get(*row).and_then(|r| r.get(*col)).cloned().unwrap_or_default();
                    errors.push(json!({"table": i, "row": row, "col": col, "text": text, "error": e.to_string()}));
                }
                if !t.updates.is_empty() {
                    cell_updates.push((*top, t.updates.clone()));
                }
            }
            (Item::Table { .. }, Outcome::Equation(_)) => {}
        }
    }
    let changed = updates.iter().filter(|u| u.is_some()).count() + cell_updates.iter().map(|(_, u)| u.len()).sum::<usize>();
    let equations: Vec<(usize, Option<String>)> = found
        .iter()
        .filter_map(|i| match i {
            Item::Equation { top, text } => Some((*top, text.clone())),
            Item::Table { .. } => None,
        })
        .collect();
    if updates.iter().any(Option::is_some) {
        write_back(s, &equations, &updates);
    }
    for (top, list) in &cell_updates {
        write_cells(s, *top, list);
    }
    json!({"regions": calc_regions, "tables": tables, "changed": changed, "errors": errors})
}

/// Recalculates with the preferred number format (F9, Update Fields, leaving an equation).
pub(crate) fn recalculate_now(s: &mut Session) -> Value {
    let opts = options(s, &json!({}));
    recalculate(s, &opts)
}

/// Replaces the text of the regions with an update, touching only the blocks that hold one.
fn write_back(s: &mut Session, found: &[(usize, Option<String>)], updates: &[Option<String>]) {
    let mut index = 0usize;
    let mut tops: Vec<usize> = found.iter().zip(updates).filter(|(_, u)| u.is_some()).map(|((t, _), _)| *t).collect();
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
                if let InlineObject::Equation { linear, math, .. } = o {
                    if index >= MAX_REGIONS {
                        return;
                    }
                    if let Some(Some(text)) = updates.get(index) {
                        *math = Math { nodes: parse_linear(text), jc: math.jc, calc: true, ..Math::default() };
                        *linear = to_linear(&math.nodes);
                        touched = true;
                    }
                    index = index.saturating_add(1);
                }
            }
            if touched {
                p.touch();
            }
        };
        super::paste::each_para(Arc::make_mut(block), 0, &mut each);
    }
}

/// Writes computed cells of the calculated table in top-level block `top`: the cell's first
/// paragraph gets the new text in its first run's formatting (a paragraph holding objects is left
/// alone).
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
    t.props.calc = on;
    super::table::touch_cells(s, &tp)?;
    let report = recalculate(s, &options(s, v));
    Ok(json!({"calc": on, "recalculated": report}))
}

/// Inserts a table and turns it into a calculated table.
fn insert_table(s: &mut Session, v: &Value) -> CmdResult {
    let rows = p::u64(v, "rows").unwrap_or(4).clamp(2, 1000);
    let cols = p::u64(v, "cols").unwrap_or(3).clamp(1, 63);
    s.run("insert.table", &json!({"rows": rows, "cols": cols})).map_err(|e| CmdError::Failed(e.to_string()))?;
    toggle_table(s, &json!({"value": true}))
}

fn variables(s: &mut Session, v: &Value) -> CmdResult {
    let opts = options(s, v);
    let found = items(&s.doc.body);
    let (ws, outcomes) = run_sheet(&found, &opts);
    let mut defined_at: Vec<(String, usize)> = Vec::new();
    for (i, outcome) in outcomes.iter().enumerate() {
        let names: Vec<String> = match outcome {
            Outcome::Equation(Some(r)) if r.error.is_none() => r.name.iter().cloned().collect(),
            Outcome::Table(t) => t.columns.clone(),
            Outcome::Equation(_) => Vec::new(),
        };
        for name in names {
            defined_at.retain(|(n, _)| *n != name);
            defined_at.push((name, i));
        }
    }
    let list: Vec<Value> = ws
        .scope()
        .iter()
        .map(|(name, value)| {
            let region = defined_at.iter().find(|(n, _)| n == name).map(|(_, i)| *i);
            let shown = strix_calc::format_value(value, &opts);
            match value.scalar() {
                Some(q) => {
                    let (number, unit) = strix_calc::auto_unit(q.value, q.dim);
                    json!({"name": name, "value": number, "text": strix_calc::format_number(number, &opts), "unit": unit, "si": q.value, "shown": shown, "region": region})
                }
                None => {
                    let si: Vec<f64> = value.quantities().iter().map(|q| q.value).collect();
                    json!({"name": name, "value": si, "text": shown, "unit": strix_calc::display_unit(value.dim()), "si": si, "shown": shown, "region": region})
                }
            }
        })
        .collect();
    Ok(json!({"variables": list}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use strix_doc::Document;

    fn session() -> Session {
        let mut s = Session::new(Document::new());
        s.run("document.setText", &json!({"text": ""})).unwrap();
        s
    }

    /// The text of every equation in the body, in order, without spaces (the linear format
    /// of a built-up equation drops them).
    fn texts(s: &Session) -> Vec<String> {
        regions(&s.doc.body).into_iter().filter_map(|(_, t)| t.map(|t| t.replace(' ', ""))).collect()
    }

    fn add(s: &mut Session, text: &str) -> Value {
        let r = s.run("calc.insert", &json!({"text": text})).unwrap();
        s.run("text.insert", &json!({"text": " "})).unwrap();
        r
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
        assert_eq!(texts(&s)[1], "a*3=6m");
        // Edit the definition through the equation command, then recalculate (F9 does it too).
        s.sel = crate::Selection::caret(strix_doc::Pos { off: 0, ..s.sel.focus.clone() });
        s.run("equation.set", &json!({"linear": "a := 5 m"})).unwrap();
        s.run("equation.exit", &json!({})).ok();
        let r = s.run("references.updateFields", &json!({})).unwrap();
        assert!(r.is_object());
        assert_eq!(texts(&s), ["a:=5m", "a*3=15m"]);
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
        s.sel = crate::Selection::caret(strix_doc::Pos { off: 0, ..s.sel.focus.clone() });
        let r = s.run("equation.calc", &json!({})).unwrap();
        assert_eq!(r["calc"], true);
        assert_eq!(texts(&s), ["x:=3m", "x*2=6m"]);
        s.run("equation.calc", &json!({"value": false})).unwrap();
        // With x unknown again, the last result reads as an equation in x and gives it back.
        assert_eq!(texts(&s), ["x*2=6m⇒x=3m"]);
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
        for t in ["F = 10 N", "m = 4 kg", "F = m·a", "a = m/s^2"] {
            add(&mut s, t);
        }
        assert_eq!(texts(&s), ["F=10N", "m=4kg", "F=m·a⇒a=2.5m/s²", "a=2.5m/s²"]);
        let vars = s.run("calc.variables", &json!({})).unwrap();
        assert_eq!(vars["variables"][2]["name"], "a");
        assert_eq!(vars["variables"][2]["region"], 2);
    }

    /// Inserts a table filled with `cells` (straight into the model: typing isn't under test).
    fn with_table(s: &mut Session, cells: &[&[&str]]) {
        let cols = cells.first().map_or(1, |r| r.len());
        s.run("insert.table", &json!({"rows": cells.len(), "cols": cols})).unwrap();
        let block = s.doc.body.iter_mut().find(|b| matches!(&***b, Block::Table(_))).unwrap();
        let Block::Table(t) = Arc::make_mut(block) else { return };
        for (row, texts) in t.rows.iter_mut().zip(cells) {
            for (cell, text) in row.cells.iter_mut().zip(texts.iter()) {
                cell.blocks = vec![strix_doc::para_block(Paragraph::with_text(text, Default::default()))];
            }
        }
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
    fn number_format_options_round_every_result() {
        let mut s = session();
        add(&mut s, "x = 2/3 =");
        assert_eq!(texts(&s), ["x=2/3=0.667"]);
        s.run("calc.options", &json!({"digits": 2, "count": "decimals", "zeros": true})).unwrap();
        assert_eq!(texts(&s), ["x=2/3=0.67"]);
        assert!(s.run("calc.options", &json!({"count": "nope"})).is_err());
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
}
