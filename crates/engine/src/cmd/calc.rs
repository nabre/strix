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
use strix_doc::props::{Rgb, TextColor};
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
        CommandSpec::new("calc.steps", "Show Steps", "Math › Results", |s, v| {
            s.prefs.calc_steps = p::bool(v, "value").unwrap_or(!s.prefs.calc_steps);
            let report = recalculate_now(s);
            Ok(json!({"steps": s.prefs.calc_steps, "recalculated": report}))
        })
        .params(r#"{"value"?: bool (default: toggle)} → formulas and solved equations show their solution under them: the formula solved for the unknown, the values substituted, the result (a preference)"#),
        CommandSpec::new("calc.options", "Number Format", "Math › Results", set_options)
            .params(r#"{"digits"?: 0–15, "count"?: "auto|decimals|significant", "zeros"?: bool} → the format: how results are rounded (a preference), then recalculates"#),
        CommandSpec::new("table.calc", "Calculated Table", "Table Layout › Data", toggle_table)
            .when(|s| if s.sel.focus.path.cell().is_some() { None } else { Some("the cursor isn't in a table") })
            .params(r#"{"value"?: bool (default: toggle)} → {"calc": bool, "recalculated"}: the table at the caret is calculated: header cells name its columns (`L [m]`, `M [kN·m] = q·L^2/8`), `Σ =` cells total them"#),
        CommandSpec::new("table.calcNames", "Data Table", "Table Layout › Data", cell_names)
            .when(|s| if s.sel.focus.path.cell().is_some() { None } else { Some("the cursor isn't in a table") })
            .params(r#"{"value": "columnRow" | "rowColumn" | "none"} → {"names", "recalculated"}: the table at the caret is a data table (made calculated): every value is a parameter named by its column and its row's label (the first column), joined by a subscript: `F_A` (columnRow) or `A_F` (rowColumn)"#),
        CommandSpec::new("table.mathCells", "Cells as Equations", "Table Layout › Data", math_cells)
            .when(|s| if s.sel.focus.path.cell().is_some() { None } else { Some("the cursor isn't in a table") })
            .params(r#"{"value"?: bool (default: toggle)} → {"math", "recalculated"}: the table at the caret (made calculated) shows every cell as an equation: names in italics, subscripts, units in the Unit style; off, the cells are text again"#),
        CommandSpec::new("calc.insertTable", "Calculated Table", "Math › Tables", insert_table)
            .params(r#"{"rows"?: n (default 4), "cols"?: n (default 3)} → inserts a table and makes it calculated (see table.calc)"#),
        CommandSpec::new("calc.tableInfo", "Calculated Table Info", "Math › Tables", |s, _| {
            let table = s.sel.focus.path.cell().and_then(|(tp, _, _)| s.doc.table(s.sel.focus.story, &tp));
            let calc = table.is_some_and(|t| t.props.calc);
            let names = table.and_then(|t| t.props.calc_names).map_or("none", |n| n.name());
            let math = table.is_some_and(|t| t.props.calc_math);
            Ok(json!({"calc": calc, "names": names, "math": math}))
        })
        .pure()
        .params(r#"→ {"calc": bool, "names": "columnRow"|"rowColumn"|"none", "math": bool}: whether the table at the caret is calculated, how a data table names its cells, whether its cells are shown as equations"#),
        CommandSpec::new("calc.variables", "Variables", "Math › Results", variables)
            .pure()
            .params(r#"{"digits"?: n} → {"variables": [{"name", "value" (number in the unit shown, or an array), "text", "unit", "si" (SI magnitude, or an array), "region" (index of the defining item)}]}"#),
    ]
}

/// The number format: the preferences, overridden by the call's parameters.
fn options(s: &Session, v: &Value) -> Options {
    let mut o = Options {
        digits: s.prefs.calc_digits.min(15),
        count: count(&s.prefs.calc_count),
        zeros: s.prefs.calc_zeros,
        steps: s.prefs.calc_steps,
        ..Options::default()
    };
    if let Some(steps) = p::bool(v, "steps") {
        o.steps = steps;
    }
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
    // An equation showing its solution steps is rows aligned at `&`: the statement is the first.
    // Only arrays Strix made are read so: an array the user wrote is the statement as a whole.
    if math.stepped
        && let [strix_doc::math::MNode::EqArr { rows }] = math.nodes.as_slice()
        && let Some(first) = rows.first()
    {
        return to_linear(first).replace('&', "");
    }
    if math.nodes.is_empty() || math.linear { linear.to_string() } else { to_linear(&math.nodes) }
}

/// The equation's whole linear text (all its rows).
fn equation_full(linear: &str, math: &Math) -> String {
    if math.nodes.is_empty() || math.linear { linear.to_string() } else { to_linear(&math.nodes) }
}

/// `line` with the alignment mark before its first `=` outside brackets: `a &= F/m`.
fn aligned(line: &str) -> String {
    let mut depth = 0i32;
    for (i, c) in line.char_indices() {
        match c {
            '(' | '[' | '{' | '〖' => depth += 1,
            ')' | ']' | '}' | '〗' => depth -= 1,
            '=' if depth == 0 => return format!("{}&{}", line.get(..i).unwrap_or_default(), line.get(i..).unwrap_or_default()),
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
    /// and the question it answers (`Math::answer`, 0 for none).
    Equation { top: usize, text: Option<String>, answer: u32, full: String },
    /// A calculated table (top-level block `top`): its cells' text, row by row, and how its
    /// cells are named when it is a data table.
    Table { top: usize, cells: Vec<Vec<String>>, names: Option<strix_doc::props::CellNames>, math: bool },
    /// A CAD drawing (docs/cad.md) in top-level block `top`: its source.
    Cad { top: usize, source: Arc<str> },
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
    match p.objects.as_slice() {
        [InlineObject::Equation { linear, math, .. }] if p.text.trim() == strix_doc::para::OBJ.to_string() => {
            Some(if math.nodes.is_empty() { linear.clone() } else { to_linear(&math.nodes) }.trim().to_string())
        }
        _ => None,
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
                    if let InlineObject::Graphic { graphic, .. } = o
                        && graphic.kind == strix_doc::graphic::GraphicKind::Cad
                    {
                        out.push(Item::Cad { top, source: graphic.cad.clone().unwrap_or_else(|| Arc::from("")) });
                        continue;
                    }
                    if let InlineObject::Equation { linear, math, .. } = o {
                        out.push(Item::Equation {
                            top,
                            text: math.calc.then(|| equation_text(linear, math)),
                            answer: math.answer,
                            full: equation_full(linear, math),
                        });
                    }
                }
            }
            Block::Table(t) if depth == 0 && t.props.calc => {
                out.push(Item::Table { top, cells: table_cells(t), names: t.props.calc_names, math: t.props.calc_math })
            }
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
            Item::Table { .. } | Item::Cad { .. } => None,
        })
        .collect()
}

/// What evaluating an item gave.
enum Outcome {
    /// The result and the units of each row the equation shows (the statement, then its steps),
    /// to style.
    Equation(Option<RegionResult>, Vec<Vec<String>>),
    /// The table's result and, for a table shown as equations, every cell's final text with
    /// the units in it (to style).
    Table(TableResult, Option<Vec<Vec<(String, Vec<String>)>>>),
    /// A drawing as rendered, the expressions that failed, and the drawing resolved (its
    /// coordinates' values filled in, for the editor).
    /// The rendering for the page, the failed expressions, and for the editor the drawing, the
    /// parameters it can show and its sheet's rendering when that differs (layers not printed).
    Cad(Option<strix_cad::Rendered>, Vec<(String, String)>, Option<(strix_cad::Drawing, Vec<String>, Option<strix_cad::Rendered>)>),
}

/// The document's worksheets after evaluating it: the main one, and one per exercise (each
/// exercise is a sandbox: what it defines stays in it, and it sees nothing from outside).
struct Sheets {
    main: Worksheet,
    exercises: std::collections::BTreeMap<usize, Worksheet>,
}

/// Evaluates the items in order, each in its zone's worksheet (`zones`: the exercise of each
/// top-level block, `None` outside exercises). One step budget covers them all.
fn run_sheet(
    found: &[Item],
    opts: &Options,
    zones: &[Option<usize>],
    seeds: &std::collections::BTreeMap<usize, Vec<String>>,
) -> (Sheets, Vec<Outcome>) {
    let mut sheets = Sheets { main: Worksheet::new(*opts), exercises: Default::default() };
    // Each exercise's sandbox starts with its data (the active variant's values).
    // The data count against the same step budget as everything else.
    let mut spent = 0usize;
    for (e, statements) in seeds {
        let ws = sheets.exercises.entry(*e).or_insert_with(|| Worksheet::new(*opts));
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
                Item::Equation { top, .. } | Item::Table { top, .. } | Item::Cad { top, .. } => *top,
            };
            let ws = match zones.get(top).copied().flatten() {
                Some(e) => sheets.exercises.entry(e).or_insert_with(|| Worksheet::new(*opts)),
                None => &mut sheets.main,
            };
            ws.scope().set_spent(spent);
            let outcome = match item {
                Item::Equation { text: Some(text), .. } => {
                    let r = ws.eval_statement(text);
                    let first = r.display.clone().unwrap_or_else(|| text.clone());
                    let units = std::iter::once(&first).chain(r.steps.iter().flatten()).map(|row| ws.units_in(row)).collect();
                    Outcome::Equation(Some(r), units)
                }
                Item::Equation { text: None, .. } => Outcome::Equation(None, Vec::new()),
                Item::Table { cells, names, math, .. } => {
                    let result = ws.eval_table_named(
                        cells,
                        names.map(|n| match n {
                            strix_doc::props::CellNames::ColumnRow => strix_calc::CellNames::ColumnRow,
                            strix_doc::props::CellNames::RowColumn => strix_calc::CellNames::RowColumn,
                        }),
                    );
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
                        let rendered = d.render(Some(ws));
                        // The editor's sheet shows the layers not printed too.
                        let screen = d.has_unprinted().then(|| d.render_layers(Some(ws), true));
                        // The parameters it can show, for the editor's Annotation box.
                        let names = ws.scope().iter().map(|(n, _)| n.to_string()).collect();
                        Outcome::Cad(Some(rendered), errors, Some((d, names, screen)))
                    }
                    Err(e) => Outcome::Cad(None, vec![(String::new(), e)], None),
                },
            };
            spent = ws.scope().spent();
            outcome
        })
        .collect();
    (sheets, outcomes)
}

/// Evaluates every region and calculated table and writes changed results back. Returns the
/// report JSON.
pub(crate) fn recalculate(s: &mut Session, opts: &Options) -> Value {
    let found = items(&s.doc.body);
    let (_, outcomes) = run_sheet(&found, opts, &super::exercise::zones(s), &super::exercise::seeds(s));
    let mut updates: Vec<Option<String>> = Vec::new();
    let mut units: Vec<Vec<Vec<String>>> = Vec::new();
    let mut stepped: Vec<bool> = Vec::new();
    let mut cads: Vec<Option<Redrawn>> = Vec::new();
    let mut cell_updates: Vec<(usize, Vec<(usize, usize, String)>)> = Vec::new();
    let mut math_cells: Vec<(usize, Vec<Vec<(String, Vec<String>)>>)> = Vec::new();
    let mut errors = Vec::new();
    let (mut calc_regions, mut tables) = (0usize, 0usize);
    for (i, (item, outcome)) in found.iter().zip(&outcomes).enumerate() {
        match (item, outcome) {
            (Item::Equation { text: Some(text), full, .. }, Outcome::Equation(Some(r), found_units)) => {
                units.push(found_units.clone());
                if r.kind != RegionKind::NotCalc {
                    calc_regions += 1;
                }
                if let Some(e) = &r.error {
                    errors.push(json!({"region": i, "text": text, "error": e.to_string()}));
                }
                // What the equation should show: the statement (rewritten or as typed) and its steps.
                let statement = r.display.clone().unwrap_or_else(|| text.clone());
                let desired = with_steps(&statement, r.steps.as_ref());
                // The linear format of a built-up equation drops spaces: only a real change rewrites it.
                let squash = |t: &str| t.replace(char::is_whitespace, "");
                updates.push((squash(&desired) != squash(full)).then_some(desired));
                stepped.push(r.steps.as_ref().is_some_and(|l| !l.is_empty()));
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
                } else if !t.updates.is_empty() {
                    cell_updates.push((*top, t.updates.clone()));
                }
            }
            (Item::Cad { .. }, Outcome::Cad(drawn, failed, resolved)) => {
                for (expr, why) in failed {
                    errors.push(json!({"drawing": i, "text": expr, "error": why}));
                }
                let unit = unit_color(s);
                let fitted = resolved.as_ref().filter(|(d, ..)| d.fit.is_some()).map(|(d, ..)| d.to_json());
                cads.push(drawn.as_ref().map(|r| {
                    let (w, h, items) = super::cad::items_of(r, unit);
                    (w, h, items, r.clone(), unit, fitted.clone())
                }));
            }
            (Item::Table { .. } | Item::Cad { .. }, _) => {}
        }
    }
    let mut changed = updates.iter().filter(|u| u.is_some()).count() + cell_updates.iter().map(|(_, u)| u.len()).sum::<usize>();
    let equations: Vec<(usize, Option<String>)> = found
        .iter()
        .filter_map(|i| match i {
            Item::Equation { top, text, .. } => Some((*top, text.clone())),
            Item::Table { .. } | Item::Cad { .. } => None,
        })
        .collect();
    let color = unit_color(s);
    let restyled = write_back(s, &equations, &updates, &stepped, &units, color);
    changed += restyled + redraw(s, &cads);
    let unit = unit_color(s);
    for (top, grid) in &math_cells {
        changed += write_math_cells(s, *top, grid, unit);
    }
    for (top, list) in &cell_updates {
        write_cells(s, *top, list);
    }
    json!({"regions": calc_regions, "tables": tables, "changed": changed, "errors": errors})
}

/// A hash of what the calculations read: every item's text, in order.
fn fingerprint(found: &[Item]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for item in found {
        match item {
            Item::Equation { text, answer, full, .. } => (text, answer, full).hash(&mut h),
            Item::Table { cells, names, math, .. } => {
                cells.hash(&mut h);
                names.map(|n| n.name()).hash(&mut h);
                math.hash(&mut h);
            }
            Item::Cad { source, .. } => source.hash(&mut h),
        }
    }
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
    let unit = s.doc.styles.get(UNIT_STYLE).map(|st| format!("{:?}", st.chr.color)).unwrap_or_default();
    let key = fingerprint(&found) ^ super::exercise::signature(s) ^ {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        unit.hash(&mut h);
        h.finish()
    };
    if s.calc_seen == Some(key) {
        return;
    }
    if found.iter().any(|i| matches!(i, Item::Equation { text: Some(_), .. } | Item::Table { .. })) {
        let report = recalculate_now(s);
        if report["changed"].as_u64().unwrap_or(0) > 0 {
            s.touch();
        }
    }
    // Exercises show the results: their automatic parts follow.
    super::exercise::update(s);
    let unit = s.doc.styles.get(UNIT_STYLE).map(|st| format!("{:?}", st.chr.color)).unwrap_or_default();
    s.calc_seen = Some(
        fingerprint(&items(&s.doc.body)) ^ super::exercise::signature(s) ^ {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            unit.hash(&mut h);
            h.finish()
        },
    );
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
    let (sheets, outcomes) = run_sheet(&found, opts, &zones, &super::exercise::seeds(s));
    found
        .iter()
        .zip(&outcomes)
        .filter_map(|(item, outcome)| {
            let Item::Equation { top, answer, text, .. } = item else { return None };
            if *answer == 0 {
                return None;
            }
            let shown = match outcome {
                Outcome::Equation(Some(r), _) => match (&r.name, &r.value, &r.error) {
                    (_, _, Some(e)) => format!("⚠ {e}"),
                    (Some(name), Some(_), None) => {
                        let ws = zones.get(*top).copied().flatten().and_then(|e| sheets.exercises.get(&e)).unwrap_or(&sheets.main);
                        format!("{name} = {}", ws.shown(name).unwrap_or_default())
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

fn caret_in_calc_table(s: &Session) -> bool {
    s.sel.focus.path.cell().and_then(|(tp, _, _)| s.doc.table(s.sel.focus.story, &tp)).is_some_and(|t| t.props.calc)
}

/// The drawing whose source is `source` (the same `Arc`), resolved where it sits in the document,
/// and rendered: for the drawing editor.
pub(crate) fn resolved_cad(s: &Session, source: &Arc<str>) -> Option<(strix_cad::Drawing, strix_cad::Rendered, Vec<String>)> {
    let found = items(&s.doc.body);
    let opts = options_now(s);
    let (_, outcomes) = run_sheet(&found, &opts, &super::exercise::zones(s), &super::exercise::seeds(s));
    found.iter().zip(outcomes).find_map(|(item, outcome)| match (item, outcome) {
        (Item::Cad { source: src, .. }, Outcome::Cad(Some(r), _, Some((d, names, screen)))) if Arc::ptr_eq(src, source) => {
            Some((d, screen.unwrap_or(r), names))
        }
        _ => None,
    })
}

/// Redraws the CAD drawings (in reading order) whose items changed; returns how many.
/// A redrawn drawing: its size, items, the rendering they came from and the Unit colour.
/// With a fitted scale, the source with the scale it was drawn at (so it shows and saves it).
type Redrawn = (f32, f32, Vec<strix_doc::graphic::GraphicItem>, strix_cad::Rendered, Rgb, Option<String>);

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
                if graphic.kind != strix_doc::graphic::GraphicKind::Cad {
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
        super::paste::each_para(Arc::make_mut(block), 0, &mut each);
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
    updates: &[Option<String>],
    stepped: &[bool],
    units: &[Vec<Vec<String>>],
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
                    if let Some(Some(text)) = updates.get(index) {
                        let mut nodes = parse_linear(text);
                        style_rows(&mut nodes, found_units, color);
                        // Steps go one under the other: the equation stands on its own line.
                        if text.starts_with('█') {
                            *display = true;
                        }
                        *math = Math { nodes, jc: math.jc, calc: true, answer: math.answer, stepped: steps_written, ..Math::default() };
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
        super::paste::each_para(Arc::make_mut(block), 0, &mut each);
    }
    restyled
}

/// The character style the units of calculations are shown in: `MathUnit` ("Unit"), added with
/// a teal colour the first time; its colour is the units' colour.
pub(crate) const UNIT_STYLE: &str = "MathUnit";
const UNIT_COLOR: Rgb = Rgb(0x1B, 0x6F, 0x8A);

pub(crate) fn unit_color(s: &mut Session) -> Rgb {
    if s.doc.styles.get(UNIT_STYLE).is_none() {
        s.doc.styles.styles.push(strix_doc::styles::Style {
            id: UNIT_STYLE.into(),
            name: "Unit".into(),
            kind: strix_doc::styles::StyleKind::Character,
            chr: strix_doc::props::CharProps { color: Some(TextColor::Rgb(UNIT_COLOR)), ..Default::default() },
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

/// Styles the units of each row with that row's units: a name that is a unit in the result
/// (`m` in `25 m/s²`) stays a parameter in the formula (`m` in `F = m·a`).
fn style_rows(nodes: &mut strix_doc::math::Arg, rows: &[Vec<String>], color: Rgb) {
    use strix_doc::math::MNode;
    match nodes.as_mut_slice() {
        [MNode::EqArr { rows: arr }] => {
            for (k, row) in arr.iter_mut().enumerate() {
                style_units(row, rows.get(k).map(Vec::as_slice).unwrap_or_default(), color, 1);
            }
        }
        _ => style_units(nodes, rows.first().map(Vec::as_slice).unwrap_or_default(), color, 0),
    }
}

/// Characters a unit name is made of (letters, `°`, `%`, `‰`, `µ`, `Ω`).
fn unit_char(c: char) -> bool {
    c.is_alphabetic() || matches!(c, '°' | '%' | '‰') || is_superscript(c)
}

/// A superscript power written after a unit (`s²`, `m⁻¹`): styled with it.
fn is_superscript(c: char) -> bool {
    matches!(c, '¹' | '²' | '³' | '⁰' | '⁴'..='⁹' | '⁻')
}

/// Splits the math runs so each unit is a run of its own, upright in `color`; runs that are no
/// longer units lose that colour. Other formatting is kept.
fn style_units(root: &mut strix_doc::math::Arg, units: &[String], color: Rgb, depth: usize) {
    use strix_doc::math::{MNode, MRun, MSty};
    use strix_doc::math_edit::{child_count, child_mut};
    if depth > MAX_DEPTH {
        return;
    }
    let mut out = Vec::with_capacity(root.len());
    for mut n in std::mem::take(root) {
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
        // Segments: runs of unit characters, and everything else.
        let mut segments: Vec<(bool, String)> = Vec::new();
        for c in r.text.chars() {
            let word = unit_char(c);
            match segments.last_mut() {
                Some((w, t)) if *w == word => t.push(c),
                _ => segments.push((word, c.to_string())),
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
        for (word, text) in segments {
            let piece = if word && units.iter().any(|u| *u == text.trim_end_matches(is_superscript)) {
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
    for (row, texts) in t.rows.iter_mut().zip(grid) {
        for (cell, (text, units)) in row.cells.iter_mut().zip(texts) {
            let Some(first) = cell.blocks.first_mut() else { continue };
            let Block::Para(p) = &**first else { continue };
            let props = p.runs.first().map_or_else(|| p.mark.clone(), |r| r.props.clone());
            let mut nodes = parse_linear(text);
            style_units(&mut nodes, units, color, 0);
            // A label (`Support`, `Beam 2`) stays words: no italics, no units.
            let label = is_label(text);
            let same = match p.objects.as_slice() {
                [InlineObject::Equation { math, .. }] => !label && math.nodes == nodes,
                [] => (text.is_empty() || label) && p.text.trim() == text.as_str(),
                _ => false,
            };
            if same {
                continue;
            }
            let mut new = Paragraph::with_text(if label { text.as_str() } else { "" }, props.clone());
            if !text.is_empty() && !label {
                let math = Math { nodes, ..Math::default() };
                let obj = InlineObject::Equation { linear: to_linear(&math.nodes), math, display: false };
                if new.insert_object(0, obj, &props).is_err() {
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
            let props = p.runs.first().map_or_else(|| p.mark.clone(), |r| r.props.clone());
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
    t.props.calc = on;
    super::table::touch_cells(s, &tp)?;
    let report = recalculate(s, &options(s, v));
    Ok(json!({"calc": on, "recalculated": report}))
}

fn cell_names(s: &mut Session, v: &Value) -> CmdResult {
    let names = match p::str(v, "value") {
        Some("none") => None,
        Some(word) => Some(strix_doc::props::CellNames::parse(word).ok_or_else(|| CmdError::Params("value: columnRow, rowColumn or none".into()))?),
        None => return Err(CmdError::Params("value: columnRow, rowColumn or none".into())),
    };
    let (tp, _, _) = s.sel.focus.path.cell().ok_or_else(|| CmdError::Disabled("the cursor isn't in a table".into()))?;
    let story = s.sel.focus.story;
    let t = s.doc.table_mut(story, &tp)?;
    t.props.calc_names = names;
    // A data table is a calculated table.
    if names.is_some() {
        t.props.calc = true;
    }
    super::table::touch_cells(s, &tp)?;
    let report = recalculate(s, &options(s, v));
    Ok(json!({"names": names.map_or("none", |n| n.name()), "recalculated": report}))
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
    let (sheets, outcomes) = run_sheet(&found, &opts, &super::exercise::zones(s), &super::exercise::seeds(s));
    let ws = &sheets.main;
    let mut defined_at: Vec<(String, usize)> = Vec::new();
    for (i, outcome) in outcomes.iter().enumerate() {
        let names: Vec<String> = match outcome {
            Outcome::Equation(Some(r), _) if r.error.is_none() => r.name.iter().cloned().collect(),
            Outcome::Table(t, _) => t.columns.clone(),
            Outcome::Equation(..) | Outcome::Cad(..) => Vec::new(),
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
        regions(&s.doc.body).into_iter().filter_map(|(_, t)| t.map(|t| t.replace(char::is_whitespace, ""))).collect()
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
        assert_eq!(texts(&s), ["F=10N", "m=4kg", "F=m·a", "a=2.5m/s²"]);
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
        assert!(steps.ends_with("@a&=2.5m/s²)"), "{steps}");
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
                    .match_indices(strix_doc::para::OBJ)
                    .map(|(off, _)| off)
                    .zip(&p.objects)
                    .find(|(_, o)| matches!(o, InlineObject::Equation { math, .. } if math.stepped))
                    .map(|(off, _)| strix_doc::Pos::new(strix_doc::StoryRef::Body, strix_doc::Path(vec![i as u32]), off)),
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
        // Turned off, the result goes back on the equation's line.
        s.run("calc.steps", &json!({"value": false})).unwrap();
        assert_eq!(texts(&s)[2], "F=m·a⇒a=2.5m/s²");
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
        assert_eq!(texts(&s), ["5m·30cm=5m·0.3m"]);
        // Recalculating changes nothing.
        s.run("calc.recalculate", &json!({})).unwrap();
        assert_eq!(full(&s), first);
        // New data: worked anew.
        s.sel = crate::Selection::caret(strix_doc::Pos::new(strix_doc::StoryRef::Body, strix_doc::Path(vec![0]), 0));
        s.run("equation.set", &json!({"linear": "5 m·40 cm = 5 m·0.3 m"})).unwrap();
        s.run("equation.exit", &json!({})).ok();
        let changed = full(&s);
        assert!(changed[0].contains("5m·0.4m") && changed[0].contains("2m²"), "{changed:?}");
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

    /// A data table: its values are parameters `F_A` (or `A_F`) for what follows; kept on save.
    #[test]
    fn a_data_table_names_its_cells() {
        let mut s = session();
        with_table(&mut s, &[&["", "F [kN]", "a [m]"], &["A", "10", "2"], &["B", "12", "0.5"]]);
        let r = s.run("table.calcNames", &json!({"value": "columnRow"})).unwrap();
        assert_eq!(r["names"], "columnRow");
        assert_eq!(s.run("calc.tableInfo", &json!({})).unwrap(), json!({"calc": true, "names": "columnRow", "math": false}));
        s.sel = crate::Selection::caret(strix_doc::Pos::new(strix_doc::StoryRef::Body, strix_doc::Path(vec![s.doc.body.len() as u32 - 1]), 0));
        add(&mut s, "F_B·a_A =");
        assert_eq!(texts(&s).last().map(String::as_str), Some("F_B·a_A=24kN·m"), "12 kN × 2 m: a moment");
        // The other way round, then saved and opened again.
        s.sel = crate::Selection::caret(strix_doc::Pos::new(strix_doc::StoryRef::Body, strix_doc::Path(vec![0, 1, 1, 0]), 0));
        s.run("table.calcNames", &json!({"value": "rowColumn"})).unwrap();
        let vars = s.run("calc.variables", &json!({})).unwrap();
        let names: Vec<&str> = vars["variables"].as_array().unwrap().iter().filter_map(|v| v["name"].as_str()).collect();
        assert!(names.contains(&"B_F") && !names.contains(&"F_B"), "{names:?}");
        let bytes = crate::io::save_bytes("d.docx", &s.doc).unwrap();
        let back = crate::io::open_bytes("d.docx", &bytes).unwrap();
        let t = back.body.iter().find_map(|b| if let Block::Table(t) = &**b { Some(t.props.calc_names) } else { None });
        assert_eq!(t, Some(Some(strix_doc::props::CellNames::RowColumn)));
        assert!(s.run("table.calcNames", &json!({"value": "diagonal"})).is_err());
    }

    /// Cells as equations: every cell with text becomes one equation (its units styled), a
    /// formula column is written into them, a recalculation leaves them alone, and turning the
    /// option off gives the text back; the option is saved.
    #[test]
    fn table_cells_shown_as_equations() {
        let mut s = session();
        with_table(&mut s, &[&["", "F_k [kN]", "M [kN·m] = F_k·2 m"], &["A", "10", ""], &["B", "12 kN", ""]]);
        s.sel = crate::Selection::caret(strix_doc::Pos::new(strix_doc::StoryRef::Body, strix_doc::Path(vec![0, 1, 1, 0]), 0));
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
        assert_eq!(texts(&s)[1], "a*3=6m");
        // Change the definition and leave the equation: no Recalculate needed.
        s.sel = crate::Selection::caret(strix_doc::Pos { off: 0, ..s.sel.focus.clone() });
        s.run("equation.set", &json!({"linear": "a = 5 m"})).unwrap();
        s.run("equation.exit", &json!({})).ok();
        assert_eq!(texts(&s), ["a=5m", "a*3=15m"]);
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
        s.sel = crate::Selection::caret(strix_doc::Pos { story: s.sel.focus.story, path: cell, off: 1 });
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
                            strix_doc::math_edit::each_run_mut(&mut nodes, &mut |r| out.push((r.text.clone(), r.color.is_some())));
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
        assert_eq!(coloured(&s, 1), ["kN", "kN", "m"], "{:?}", runs(&s, 1));
        // A unit with a power typed as `^2` is styled whole.
        add(&mut s, "A = 50 cm^2");
        assert_eq!(coloured(&s, 2), ["cm"], "{:?}", runs(&s, 2));
        // A parameter spelled like a unit is not a unit.
        add(&mut s, "m = 3 kg");
        add(&mut s, "m*2 =");
        assert_eq!(coloured(&s, 4), ["kg"], "{:?}", runs(&s, 4));
        // In steps, `m` is the mass in the formula and the metre in the result.
        add(&mut s, "F = 10 kN");
        add(&mut s, "F = m·a");
        let rows = runs(&s, 6);
        assert!(rows.iter().any(|(t, c)| t.contains('m') && !c), "the mass m is not a unit: {rows:?}");
        assert!(rows.iter().any(|(t, c)| t == "m" && *c), "the metre in the result is: {rows:?}");
        // The style's colour is the units' colour.
        let st = s.doc.styles.get_mut(UNIT_STYLE).unwrap();
        st.chr.color = Some(TextColor::Rgb(Rgb(200, 0, 0)));
        s.run("calc.recalculate", &json!({})).unwrap();
        let mut colors = Vec::new();
        if let Some(Block::Para(p)) = s.doc.body.first().map(|b| &**b) {
            for o in &p.objects {
                if let InlineObject::Equation { math, .. } = o {
                    let mut nodes = math.nodes.clone();
                    strix_doc::math_edit::each_run_mut(&mut nodes, &mut |r| colors.extend(r.color));
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
        // Strix's own steps are marked: read back, the statement is the first row only.
        let mut s = session();
        for t in ["F = 10 N", "m = 4 kg", "F = m·a"] {
            add(&mut s, t);
        }
        let back = Session::new(crate::io::open_bytes("b.xmath", &crate::io::save_bytes("b.xmath", &s.doc).unwrap()).unwrap());
        assert_eq!(texts(&back)[2], "F=m·a");
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
