//! Calculated tables: columns of quantities, column formulas and totals (see `docs/calc.md`).
//!
//! The first row is the header. A header cell is a column name with an optional unit in square
//! brackets or parentheses and an optional formula: `L [m]`, `q (kN/m)`, `M [kN·m] = q·L²/8`
//! (`:=` works too). A formula needs no name and its unit may close it: `M = q·L²/8 [kN·m]`,
//! `= q·L²/8 [kN·m]` (a column without a name is not defined after the table).
//! Any other header (`Load case`, empty) makes a label column, which is never read.
//!
//! Every later row is a data row, unless one of its cells is a total: `Σ =`, `sum =`, `total =`,
//! `mean =`, `min =` or `max =` (with the stale result after `=`). A total is written back with
//! the column's sum, mean, minimum or maximum over the data rows.
//!
//! Any cell holding `=` (or a comparison) is a statement of its own, read like a calculation
//! region with the row's values as parameters: `q·L²/8 =` becomes `q·L²/8 = 30.375 kN·m`,
//! `= q·L²/8` reads as `q·L²/8 =`, `x = 5 m` defines `x`, `F = m·a` solves. Its value is the
//! column's value in that row (for totals and formulas to its right); in a formula column it
//! replaces the column formula for that cell.
//!
//! In a data row, an input column's cell is an expression (usually a number, in the column's
//! unit; a cell with its own unit of the same kind is converted). A formula column's cell is
//! overwritten with the formula's value for that row; the formula sees the row's values by
//! column name, then the worksheet's parameters. A row whose input cells are all empty is
//! skipped. After the table, every column whose data cells all have values is a vector in the
//! worksheet scope, so later regions can use `sum(L)` or `∑_i▒L[i]`.

use crate::eval::{Ctx, Names, Value, Vector, eval_scalar};
use crate::lexer::{Tok, Token, lex};
use crate::parser::{Expr, Func, parse};
use crate::quantity::Quantity;
use crate::solve::free_names;
use crate::worksheet::{Worksheet, in_unit_text, in_unit_text_as};
use crate::{CalcError, MAX_INPUT_CHARS, MAX_VALUES};

/// Most columns of a calculated table that are read.
const MAX_TABLE_COLS: usize = 64;
/// Most cells (columns × rows) of a calculated table that are evaluated.
const MAX_TABLE_CELLS: usize = 100_000;

/// The outcome of evaluating a calculated table.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TableResult {
    /// Cells whose text changes: row, column, new text.
    pub updates: Vec<(usize, usize, String)>,
    /// Problems: row, column, error.
    pub errors: Vec<(usize, usize, CalcError)>,
    /// The columns now defined in the worksheet scope, left to right.
    pub columns: Vec<String>,
    /// With cell names, the parameters each data cell now defines (`F_A`), row by row.
    pub cells: Vec<String>,
}

/// How a data table names its cells: the column's name and the row's label joined by a
/// subscript, one way or the other (docs/calc.md, Data tables).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellNames {
    /// `F_A`: the column's name, the row's label under it.
    ColumnRow,
    /// `A_F`: the row's label, the column's name under it.
    RowColumn,
}

/// A row label or column name as part of a parameter name: its letters and digits.
fn name_part(text: &str) -> String {
    text.chars().filter(|c| c.is_alphanumeric()).take(24).collect()
}

/// A column's unit: the quantity of one unit and its text as written in the header.
#[derive(Clone)]
struct Unit {
    q: Quantity,
    text: String,
}

/// What a header cell makes of its column.
enum Column {
    Label,
    Input { name: String, unit: Option<Unit> },
    Formula { name: String, unit: Option<Unit>, expr: Expr },
}

impl Column {
    fn name(&self) -> Option<&str> {
        match self {
            Column::Label => None,
            Column::Input { name, .. } | Column::Formula { name, .. } => Some(name.as_str()).filter(|n| !n.is_empty()),
        }
    }
}

/// The aggregate a total cell asks for.
#[derive(Clone, Copy)]
enum Total {
    Sum,
    Mean,
    Min,
    Max,
}

/// A total cell: what it computes and the keyword as typed (`Σ`, `Total`).
fn total(text: &str) -> Option<(Total, &str)> {
    let (key, _) = text.split_once('=')?;
    let key = key.trim();
    let kind = match key.to_lowercase().as_str() {
        "σ" | "∑" | "sum" | "total" => Total::Sum,
        "mean" | "avg" | "average" => Total::Mean,
        "min" => Total::Min,
        "max" => Total::Max,
        _ => return None,
    };
    Some((kind, key))
}

/// The ordinal number of each row of a calculated table (row 0 is the header): the data rows
/// with something written in them count 1, 2, 3…; the header, totals rows (`Σ =`) and empty
/// rows have none. A table's ordinal column (column "0") shows these.
pub fn ordinals(rows: &[Vec<String>]) -> Vec<Option<usize>> {
    let mut n = 0usize;
    rows.iter()
        .enumerate()
        .map(|(r, row)| {
            let data = r > 0 && !row.iter().any(|t| total(t).is_some()) && row.iter().any(|t| !t.trim().is_empty());
            data.then(|| {
                n += 1;
                n
            })
        })
        .collect()
}

/// Values read so far, by column then row (`None`: empty, or not a value).
type Values = Vec<Vec<Option<Quantity>>>;

/// What the table's last passes share once the cells are read.
#[derive(Clone, Copy)]
struct Grid<'a> {
    columns: &'a [Column],
    /// Whether each column is a label column (never a value).
    label: &'a [bool],
    values: &'a Values,
    /// Whether each row is a data row with at least one input.
    active: &'a [bool],
}

impl Worksheet {
    /// Evaluates a calculated table given as plain cell texts, row by row (row 0 is the
    /// header), and defines its complete columns in the scope. See the module docs.
    pub fn eval_table(&mut self, rows: &[Vec<String>]) -> TableResult {
        self.eval_table_named(rows, None)
    }

    /// [`Self::eval_table`]; with `names`, every data cell also defines a parameter named from
    /// its column and its row's label (the first column's text): `F_A` or `A_F`.
    pub fn eval_table_named(&mut self, rows: &[Vec<String>], names: Option<CellNames>) -> TableResult {
        let mut out = TableResult::default();
        let Some((header, body)) = rows.split_first() else {
            return out;
        };
        // The grid evaluated is bounded, not just the cells read: columns × rows ≤ MAX_TABLE_CELLS.
        let header = header.get(..MAX_TABLE_COLS.min(header.len())).unwrap_or_default();
        // The columns' names, so a trailing `[…]` that reads one is an index, not a unit.
        let col_words: Vec<String> = header.iter().map(|t| t.trim().chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect()).collect();
        let columns: Vec<Column> = header
            .iter()
            .enumerate()
            .map(|(c, text)| match self.header(text, &col_words) {
                Ok(col) => col,
                Err(err) => {
                    out.errors.push((0, c, err));
                    Column::Label
                }
            })
            .collect();
        if columns.iter().all(|c| matches!(c, Column::Label)) {
            return out;
        }
        // A column's unit is a written unit: later results of its kind show in it.
        let written: Vec<(Quantity, String)> = columns
            .iter()
            .filter_map(|c| match c {
                Column::Input { unit: Some(u), .. } | Column::Formula { unit: Some(u), .. } if !u.q.dim.is_dimensionless() => {
                    Some((u.q, u.text.clone()))
                }
                _ => None,
            })
            .collect();
        for (q, text) in written {
            self.remember(q, text);
        }
        let max_rows = MAX_VALUES.min(MAX_TABLE_CELLS / columns.len().max(1));
        let body = body.get(..max_rows.min(body.len())).unwrap_or_default();
        let cell = |r: usize, c: usize| body.get(r).and_then(|row| row.get(c)).map_or("", |s| s.trim());

        // Input columns first: a bare name whose cells are not all values is a label column.
        let data_row: Vec<bool> = body.iter().map(|row| !row.iter().any(|t| total(t).is_some())).collect();
        let (mut values, label, input_errors) = self.read_inputs(&columns, body, &data_row);
        out.errors.extend(input_errors);
        let is_input = |c: usize| matches!(columns.get(c), Some(Column::Input { .. })) && label.get(c) == Some(&false);
        // A row is skipped when all its input cells are empty.
        let active: Vec<bool> = (0..body.len())
            .map(|r| data_row.get(r) == Some(&true) && (0..columns.len()).any(|c| is_input(c) && !cell(r, c).is_empty() && !is_statement(cell(r, c))))
            .collect();

        // Columns in a dimensionless unit (`p [%]`) show formulas of them in it.
        let hinted: Vec<(String, Option<String>)> = columns
            .iter()
            .filter_map(|c| match c {
                Column::Input { name, unit } => Some((name.clone(), unit.as_ref().filter(|u| u.q.dim.is_dimensionless()).map(|u| u.text.clone()))),
                _ => None,
            })
            .collect();
        for (name, hint) in hinted {
            self.set_hint(&name, hint);
        }

        // Row by row, left to right: a cell with `=` is a statement of its own (data, formula or
        // equation, worked out like a region), else a formula column computes the cell. One step
        // budget covers the table's formulas.
        let col_names: Vec<&str> = columns.iter().zip(&label).filter(|(_, l)| !**l).filter_map(|(c, _)| c.name()).collect();
        let formulas: Vec<Option<(Option<String>, Vec<String>)>> = columns
            .iter()
            .map(|col| match col {
                Column::Formula { expr, .. } => {
                    // Names read in the row; a column only indexed by row (`x[i+1]`) is not.
                    let mut used = Vec::new();
                    free_names(expr, &mut used);
                    let bare = bare_names(expr, &col_names);
                    used.retain(|u| bare.contains(u) || !col_names.contains(&u.as_str()));
                    Some((self.hint(expr), used))
                }
                _ => None,
            })
            .collect();
        // Which columns hold absolute temperatures or their differences, left to right.
        let mut kinds: Vec<(String, Option<bool>)> = Vec::new();
        let mut temps: Vec<Option<bool>> = Vec::new();
        for col in &columns {
            let t = match col {
                Column::Input { unit: Some(u), .. } => crate::units::affine(&u.text).map(|_| true),
                Column::Formula { expr, .. } => temp_kind(expr, &kinds, 0),
                _ => None,
            };
            if let Some(name) = col.name() {
                kinds.push((name.to_string(), t));
            }
            temps.push(t);
        }
        // Whether each formula indexes a column by row, and the data rows in order: `i` is a
        // row's place among them (1, 2, 3…), `n` how many there are.
        let indexed: Vec<bool> = columns.iter().map(|col| matches!(col, Column::Formula { expr, .. } if indexes(expr, &col_names))).collect();
        let order: Vec<usize> = (0..body.len()).filter(|&r| active.get(r) == Some(&true)).collect();
        let mut steps = 0usize;
        for r in 0..body.len() {
            if data_row.get(r) != Some(&true) {
                continue;
            }
            for (c, col) in columns.iter().enumerate() {
                let text = cell(r, c);
                let named = col.name().filter(|_| label.get(c) == Some(&false)).map(str::to_string);
                // A formula column without a name (`= q·L²/8`) still holds the row's value.
                let valued = label.get(c) == Some(&false) && !matches!(col, Column::Label);
                if is_statement(text) {
                    let row = row_values(&columns, &label, &values, r);
                    let result = self.statement_cell(text, &row);
                    if let Some(e) = result.error {
                        out.errors.push((r + 1, c, e));
                    }
                    if let Some(d) = result.display.filter(|d| d.as_str() != text) {
                        out.updates.push((r + 1, c, d));
                    }
                    // A result, or the column's own name defined, is the column's value here;
                    // `x = 5 m` only defines `x`.
                    let own = result.name.is_none() || result.name == named;
                    if valued
                        && own
                        && let Some(q) = result.value.as_ref().and_then(Value::scalar)
                        && let Some(slot) = values.get_mut(c).and_then(|v| v.get_mut(r))
                    {
                        *slot = Some(q);
                    }
                    continue;
                }
                let (Column::Formula { unit, expr, .. }, Some(Some((hint, used)))) = (col, formulas.get(c)) else { continue };
                if active.get(r) != Some(&true) {
                    if !text.is_empty() {
                        out.updates.push((r + 1, c, String::new()));
                    }
                    continue;
                }
                let mut locals = row_values(&columns, &label, &values, r);
                // The row's number and the count, unless a column has that name.
                for (name, v) in [("i", order.iter().position(|&x| x == r).map_or(0, |p| p + 1)), ("n", order.len())] {
                    if used.iter().any(|u| u == name) && !col_names.contains(&name) {
                        locals.push((name.to_string(), Value::Scalar(Quantity::number(v as f64))));
                    }
                }
                let missing = used.iter().find(|u| {
                    columns
                        .iter()
                        .enumerate()
                        .any(|(k, other)| other.name() == Some(u.as_str()) && label.get(k) == Some(&false) && !locals.iter().any(|(n, _)| n == *u))
                });
                let row_work = order.len().saturating_mul(col_names.len());
                let result = match missing {
                    Some(name) => Err(CalcError::MissingValue(name.clone())),
                    // Past the budget an indexed formula isn't even set up: copying the columns
                    // for every cell of a long table would take minutes before it fails.
                    None if indexed.get(c) == Some(&true) && steps.saturating_add(row_work) > crate::MAX_STEPS => {
                        steps = crate::MAX_STEPS.saturating_add(1);
                        Err(CalcError::TooMuchWork)
                    }
                    None => {
                        // Indexing by row reads the columns over the data rows (counted as work).
                        let by_row = if indexed.get(c) == Some(&true) {
                            steps = steps.saturating_add(row_work);
                            columns
                                .iter()
                                .enumerate()
                                .filter(|(k, _)| label.get(*k) == Some(&false))
                                .filter_map(|(k, col)| {
                                    let v = values.get(k)?;
                                    Some((col.name()?.to_string(), order.iter().map(|&row| v.get(row).copied().flatten()).collect()))
                                })
                                .collect()
                        } else {
                            Vec::new()
                        };
                        let mut ctx = Ctx::new(&self.scope, Names::Worksheet, locals).with_steps(steps).with_columns(by_row);
                        let value = ctx.eval(expr).and_then(|v| v.expect_scalar());
                        steps = ctx.steps();
                        value
                    }
                };
                let absolute = temps.get(c).copied().flatten() != Some(false);
                let shown = match result.and_then(|q| Ok((q, self.cell_text_as(q, unit.as_ref(), hint.as_deref(), absolute)?))) {
                    Ok((q, shown)) => {
                        if let Some(slot) = values.get_mut(c).and_then(|v| v.get_mut(r)) {
                            *slot = Some(q);
                        }
                        shown
                    }
                    Err(err) => {
                        out.errors.push((r + 1, c, err));
                        "⚠".to_string()
                    }
                };
                if shown != text {
                    out.updates.push((r + 1, c, shown));
                }
            }
        }

        let grid = Grid { columns: &columns, label: &label, values: &values, active: &active };
        self.fill_totals(body, &grid, &mut out);
        // Complete columns become vectors in the scope.
        self.define_columns(&grid, &mut out);
        // A data table: each value is a parameter of its own, named by its column and its row.
        if let Some(order) = names
            && label.first() == Some(&true)
        {
            self.define_cells(order, body, &grid, &mut out);
        }
        out
    }

    /// A data table's cells as parameters named by column and row (`F_A`, or `A_F`).
    fn define_cells(&mut self, order: CellNames, body: &[Vec<String>], grid: &Grid<'_>, out: &mut TableResult) {
        let Grid { columns, label, values, active } = *grid;
        {
            for (r, cells) in body.iter().enumerate().filter(|(r, _)| active.get(*r) == Some(&true)) {
                let row = name_part(cells.first().map(String::as_str).unwrap_or_default());
                for (c, col) in columns.iter().enumerate().filter(|(c, _)| label.get(*c) == Some(&false)) {
                    let (Some(column), Some(q)) = (col.name().map(name_part), values.get(c).and_then(|v| v.get(r)).copied().flatten()) else {
                        continue;
                    };
                    let (base, sub) = match order {
                        CellNames::ColumnRow => (column, row.clone()),
                        CellNames::RowColumn => (row.clone(), column),
                    };
                    // A name starts with a letter (a row labelled `1` names `F_1`, never `1_F`).
                    if sub.is_empty() || !base.chars().next().is_some_and(char::is_alphabetic) {
                        continue;
                    }
                    let name = format!("{base}_{sub}");
                    self.scope.set(&name, Value::Scalar(q));
                    let hint = match col {
                        Column::Input { unit: Some(u), .. } | Column::Formula { unit: Some(u), .. } if u.q.dim.is_dimensionless() => {
                            Some(u.text.clone())
                        }
                        _ => None,
                    };
                    self.set_hint(&name, hint);
                    out.cells.push(name);
                }
            }
        }
    }

    /// Reads the input columns' cells as values. A bare name whose cells are not all values is
    /// a label column; returns the values by column, which columns are labels, and the errors.
    fn read_inputs(&self, columns: &[Column], body: &[Vec<String>], data_row: &[bool]) -> (Values, Vec<bool>, Vec<(usize, usize, CalcError)>) {
        let cell = |r: usize, c: usize| body.get(r).and_then(|row| row.get(c)).map_or("", |s| s.trim());
        let mut values: Values = vec![vec![None; body.len()]; columns.len()];
        let mut label = vec![false; columns.len()];
        let mut input_errors = Vec::new();
        for (c, col) in columns.iter().enumerate() {
            let Column::Input { unit, .. } = col else {
                if let Some(l) = label.get_mut(c) {
                    *l = matches!(col, Column::Label);
                }
                continue;
            };
            let mut errors = Vec::new();
            for r in (0..body.len()).filter(|&r| data_row.get(r) == Some(&true)) {
                let text = cell(r, c);
                if text.is_empty() || is_statement(text) {
                    continue;
                }
                match self.input_cell(text, unit.as_ref()) {
                    Ok(q) => {
                        if let Some(slot) = values.get_mut(c).and_then(|v| v.get_mut(r)) {
                            *slot = Some(q);
                        }
                    }
                    Err(err) => errors.push((r + 1, c, err)),
                }
            }
            if unit.is_none() && !errors.is_empty() {
                if let Some(l) = label.get_mut(c) {
                    *l = true;
                }
                if let Some(v) = values.get_mut(c) {
                    v.iter_mut().for_each(|x| *x = None);
                }
            } else {
                input_errors.extend(errors);
            }
        }
        (values, label, input_errors)
    }

    /// Total cells (`Σ =`, `mean =`...): their column's values aggregated.
    fn fill_totals(&self, body: &[Vec<String>], grid: &Grid<'_>, out: &mut TableResult) {
        let Grid { columns, label, values, .. } = *grid;
        for (r, row) in body.iter().enumerate() {
            for (c, text) in row.iter().enumerate() {
                let Some((kind, key)) = total(text) else { continue };
                let col = columns.get(c);
                let column_values: Vec<Quantity> = match col {
                    Some(Column::Input { .. } | Column::Formula { .. }) if label.get(c) == Some(&false) => {
                        values.get(c).map(|v| v.iter().flatten().copied().collect()).unwrap_or_default()
                    }
                    _ => continue,
                };
                let unit = match col {
                    Some(Column::Input { unit, .. } | Column::Formula { unit, .. }) => unit.as_ref(),
                    _ => None,
                };
                let shown = aggregate(kind, &column_values).and_then(|q| self.cell_text(q, unit, None));
                let new = match shown {
                    Ok(value) => format!("{key} = {value}"),
                    Err(err) => {
                        out.errors.push((r + 1, c, err));
                        format!("{key} = ⚠")
                    }
                };
                if new != text.trim() {
                    out.updates.push((r + 1, c, new));
                }
            }
        }
    }

    /// Complete columns (a value in every active row) become vectors in the scope; the others
    /// leave it.
    fn define_columns(&mut self, grid: &Grid<'_>, out: &mut TableResult) {
        let Grid { columns, label, values, active } = *grid;
        for (c, col) in columns.iter().enumerate() {
            let Some(name) = col.name().filter(|_| label.get(c) == Some(&false)) else { continue };
            let column: Option<Vec<Quantity>> = (0..active.len())
                .filter(|&r| active.get(r) == Some(&true))
                .map(|r| values.get(c).and_then(|v| v.get(r)).copied().flatten())
                .collect();
            match column.map(|items| Vector::from_quantities(&items)) {
                Some(Ok(v)) if !v.is_empty() => {
                    self.scope.set(name, Value::Vector(v));
                    let hint = match col {
                        Column::Input { unit: Some(u), .. } | Column::Formula { unit: Some(u), .. } if u.q.dim.is_dimensionless() => {
                            Some(u.text.clone())
                        }
                        Column::Formula { unit: None, expr, .. } => self.hint(expr),
                        _ => None,
                    };
                    self.set_hint(name, hint);
                    out.columns.push(name.to_string());
                }
                _ => {
                    self.scope.remove(name);
                }
            }
        }
    }

    /// A statement in a cell, evaluated like a region with the row's values as parameters:
    /// `q·L²/8 =` shows its value, `x = 5 m` defines `x` for the rest of the document. A cell
    /// starting with `=` (`= q·L²/8`) reads as `q·L²/8 =`.
    fn statement_cell(&mut self, text: &str, row: &[(String, Value)]) -> crate::RegionResult {
        let statement = match text.strip_prefix('=') {
            Some(rest) => format!("{} =", rest.trim()),
            None => text.to_string(),
        };
        // The row's values stand in for parameters of the same names while the cell is read.
        let saved: Vec<(String, Option<Value>)> = row.iter().map(|(n, _)| (n.clone(), self.scope.get(n).cloned())).collect();
        for (n, v) in row {
            self.scope.set(n, v.clone());
        }
        // A cell shows its result, never solution steps: those are for the page.
        let opts = self.opts;
        self.opts.steps = false;
        self.opts.layout = crate::Layout::Inline;
        let result = self.eval_statement(&statement);
        self.opts = opts;
        for (n, old) in saved {
            match old {
                Some(v) => self.scope.set(&n, v),
                None => {
                    self.scope.remove(&n);
                }
            }
        }
        result
    }

    /// Parses a header cell.
    fn header(&self, text: &str, names: &[String]) -> Result<Column, CalcError> {
        let text = text.trim();
        if text.is_empty() || text.chars().count() > MAX_INPUT_CHARS {
            return Ok(Column::Label);
        }
        let Ok(tokens) = lex(text, self.opts.decimal_comma) else {
            return Ok(Column::Label);
        };
        // A unit may close a formula: `M = q·L²/8 [kN·m]`, `= q·L²/8 [kN·m]`.
        let (tokens, end_unit) = self.trailing_unit(text, &tokens, names)?;
        let tok = |i: usize| tokens.get(i).map(|t| &t.tok);
        let end_pos = tokens.last().map_or(0, |t| t.pos + 1);
        // A formula needs no name: `= γ·q·L²/8` (spreadsheet style).
        let anonymous = |body: &[Token]| -> Result<Column, CalcError> {
            let expr = self.parse_expr(body, end_pos)?;
            Ok(Column::Formula { name: String::new(), unit: end_unit.clone(), expr })
        };
        if matches!(tok(0), Some(Tok::Define | Tok::Equals)) {
            // A bare `=` (being typed, or a header that is only that) is words, not an error.
            let body = tokens.get(1..).unwrap_or_default();
            if body.is_empty() && end_unit.is_none() {
                return Ok(Column::Label);
            }
            return anonymous(body);
        }
        let name = match tok(0) {
            Some(Tok::Ident(name)) if name.chars().next().is_some_and(char::is_alphabetic) && Func::from_name(name).is_none() => name.clone(),
            _ => return Ok(Column::Label),
        };
        let mut i = 1;
        let mut unit = None;
        if let Some(open @ (Tok::LBracket | Tok::LParen)) = tok(1) {
            let close = if *open == Tok::LBracket { Tok::RBracket } else { Tok::RParen };
            let Some(end) = tokens.iter().position(|t| t.tok == close) else {
                return Ok(Column::Label);
            };
            let inner = tokens.get(2..end).unwrap_or_default();
            let close_pos = tokens.get(end).map_or(0, |t| t.pos);
            // Round brackets that hold no unit are words (`Load (case A)`): a label column. A
            // unit in square brackets must be one (a typo there is reported).
            let read = parse(inner, close_pos).and_then(|expr| eval_scalar(&expr, &self.scope, Names::UnitsOnly));
            let q = match read {
                Ok(q) => q,
                Err(_) if *open == Tok::LParen => return Ok(Column::Label),
                Err(e) => return Err(e),
            };
            let span = |t: Option<&Token>| t.map_or(0, |t| t.byte);
            let raw = text.get(span(tokens.get(2))..span(tokens.get(end))).unwrap_or_default().trim();
            unit = Some(Unit { q, text: raw.to_string() });
            i = end + 1;
        }
        match tok(i) {
            None if end_unit.is_none() => Ok(Column::Input { name, unit }),
            Some(Tok::Define | Tok::Equals) => {
                if unit.is_some() && end_unit.is_some() {
                    return Err(CalcError::BadDefinition("a column has one unit: before the = or at the end".to_string()));
                }
                let body = tokens.get(i + 1..).unwrap_or_default();
                let expr = self.parse_expr(body, end_pos)?;
                Ok(Column::Formula { name, unit: unit.or(end_unit), expr })
            }
            Some(_) if unit.is_none() => Ok(Column::Label),
            _ => Err(CalcError::BadDefinition("a column header is a name, an optional [unit] and an optional = formula".to_string())),
        }
    }

    /// A unit in square brackets closing a header's formula (`= q·L²/8 [kN·m]`): the tokens
    /// before it and the unit. Only after an `=`, and only when the brackets hold a unit and
    /// are not an index: written against a name (`x[i+1]`), brackets that read a column (in
    /// `names`), `i`, `n` or a parameter index it.
    fn trailing_unit(&self, text: &str, tokens: &[Token], names: &[String]) -> Result<(Vec<Token>, Option<Unit>), CalcError> {
        let keep = || Ok((tokens.to_vec(), None));
        let Some((last, _)) = tokens.split_last() else { return keep() };
        if last.tok != Tok::RBracket {
            return keep();
        }
        let mut depth = 0usize;
        let mut open = None;
        for (k, t) in tokens.iter().enumerate().rev() {
            match t.tok {
                Tok::RBracket => depth = depth.saturating_add(1),
                Tok::LBracket => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        open = Some(k);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(k) = open.filter(|&k| k > 0) else { return keep() };
        let before = tokens.get(..k).unwrap_or_default();
        if !before.iter().any(|t| matches!(t.tok, Tok::Define | Tok::Equals)) {
            return keep();
        }
        let Some(lb) = tokens.get(k) else { return keep() };
        let inner = tokens.get(k + 1..tokens.len() - 1).unwrap_or_default();
        // Written against a name with a row's name inside, the brackets index it (`x[i+1]`); a
        // built-up equation's linear text drops the space before a unit (`q·L[kN]`).
        let indexable = matches!(before.last().map(|t| &t.tok), Some(Tok::Ident(_) | Tok::RParen | Tok::RBracket));
        let spaced = text.get(..lb.byte).is_some_and(|t| t.ends_with(char::is_whitespace));
        let reads_row = inner
            .iter()
            .any(|t| matches!(&t.tok, Tok::Ident(n) if n == "i" || n == "n" || names.iter().any(|c| c == n) || self.scope.get(n).is_some()));
        if indexable && !spaced && reads_row {
            return keep();
        }
        let Ok(q) = parse(inner, last.pos).and_then(|expr| eval_scalar(&expr, &self.scope, Names::UnitsOnly)) else {
            return keep();
        };
        let raw = text.get(lb.byte + 1..last.byte).unwrap_or_default().trim();
        Ok((before.to_vec(), Some(Unit { q, text: raw.to_string() })))
    }

    /// The value of an input cell in a column with `unit`.
    fn input_cell(&self, text: &str, unit: Option<&Unit>) -> Result<Quantity, CalcError> {
        if text.chars().count() > MAX_INPUT_CHARS {
            return Err(CalcError::TooLong { limit: MAX_INPUT_CHARS });
        }
        let tokens = lex(text, self.opts.decimal_comma)?;
        let expr = self.parse_expr(&tokens, text.chars().count())?;
        let q = eval_scalar(&expr, &self.scope, Names::Worksheet)?;
        // A cell that writes its own unit (`15 %` in a `[%]` column) is not scaled again.
        let own_unit = tokens.iter().any(|t| {
            matches!(&t.tok, crate::lexer::Tok::Ident(n) if self.scope.get(n).is_none() && crate::units::constant(n).is_none() && crate::units::unit(n).is_some())
        });
        match unit {
            // A number in a °C or °F column is an absolute temperature.
            Some(u)
                if q.dim.is_dimensionless()
                    && !own_unit
                    && let Some((scale, offset)) = crate::units::affine(&u.text) =>
            {
                Ok(Quantity::new(crate::quantity::finite(q.value * scale + offset)?, u.q.dim))
            }
            Some(u) if q.dim.is_dimensionless() && !own_unit => q.try_mul(u.q),
            Some(u) if q.dim != u.q.dim => Err(CalcError::WrongUnit { result: crate::units::dim_name(q.dim), requested: u.text.clone() }),
            _ => Ok(q),
        }
    }

    /// A computed cell: the number in the column's unit, or the number and its automatic unit.
    fn cell_text(&self, q: Quantity, unit: Option<&Unit>, hint: Option<&str>) -> Result<String, CalcError> {
        self.cell_text_as(q, unit, hint, true)
    }

    /// [`Self::cell_text`]; without `absolute`, a `[°C]`/`[°F]` value is a difference (no offset).
    fn cell_text_as(&self, q: Quantity, unit: Option<&Unit>, hint: Option<&str>, absolute: bool) -> Result<String, CalcError> {
        match unit {
            Some(u) if !absolute => in_unit_text_as(&Value::Scalar(q), u.q, &u.text, &self.opts, false),
            Some(u) => in_unit_text(&Value::Scalar(q), u.q, &u.text, &self.opts),
            None => {
                let (number, unit) = self.auto_hinted(&Value::Scalar(q), hint);
                Ok(crate::worksheet::with_unit(&number, &unit))
            }
        }
    }
}

/// Whether a column holds absolute temperatures (`Some(true)`: written in `[°C]`/`[°F]`, or
/// worked out from them), differences of them (`Some(false)`) or neither, by how its formula is
/// written, as statements are (docs/calc.md, Temperatures): `T_2 - T_1` is a difference, shown
/// in a `[°C]` column without the scale's offset.
fn temp_kind(e: &Expr, kinds: &[(String, Option<bool>)], depth: usize) -> Option<bool> {
    if depth > crate::MAX_DEPTH {
        return None;
    }
    let kind = |x: &Expr| temp_kind(x, kinds, depth + 1);
    match e {
        Expr::Name(n) => kinds.iter().find(|(k, _)| k == n).and_then(|(_, t)| *t),
        Expr::Neg(a) => kind(a).filter(|k| !k),
        Expr::Bin(crate::parser::BinOp::Sub, a, b) => match (kind(a), kind(b)) {
            (Some(true), Some(true)) => Some(false),
            (Some(true), _) => Some(true),
            (Some(false), _) | (None, Some(false)) => Some(false),
            _ => None,
        },
        Expr::Bin(crate::parser::BinOp::Add, a, b) => match (kind(a), kind(b)) {
            (Some(true), Some(true)) => None,
            (Some(true), _) | (_, Some(true)) => Some(true),
            (Some(false), _) | (_, Some(false)) => Some(false),
            _ => None,
        },
        Expr::Bin(crate::parser::BinOp::Mul, a, b) => match (kind(a), kind(b)) {
            (Some(false), None) | (None, Some(false)) => Some(false),
            _ => None,
        },
        Expr::Bin(crate::parser::BinOp::Div, a, b) => match (kind(a), kind(b)) {
            (Some(false), None) => Some(false),
            _ => None,
        },
        Expr::Call(f, args) if f.is_rounding() || matches!(f, Func::Min | Func::Max | Func::Mean | Func::Abs) => {
            let mut found = None;
            for a in args.iter().take(1) {
                found = kind(a);
            }
            found
        }
        _ => None,
    }
}

/// The sum, mean, minimum or maximum of `values` (0 for the sum of none).
fn aggregate(kind: Total, values: &[Quantity]) -> Result<Quantity, CalcError> {
    let Some((first, rest)) = values.split_first() else {
        return match kind {
            Total::Sum => Ok(Quantity::number(0.0)),
            _ => Err(CalcError::MissingValue("the column".to_string())),
        };
    };
    let mut acc = *first;
    for &q in rest {
        acc = match kind {
            Total::Sum | Total::Mean => acc.try_add(q)?,
            Total::Min | Total::Max => {
                acc.same_dim(q, "compare")?;
                let better = if matches!(kind, Total::Min) { q.value < acc.value } else { q.value > acc.value };
                if better { q } else { acc }
            }
        };
    }
    if matches!(kind, Total::Mean) {
        acc = acc.try_div(Quantity::number(values.len() as f64))?;
    }
    Ok(acc)
}

/// True for a cell that holds a statement (`q·L²/8 =`, `= q·L²/8`, `x = 5 m`, `σ ≤ f_yd`) rather
/// than a value. Totals (`Σ =`) are recognised before this.
fn is_statement(text: &str) -> bool {
    text.contains(['=', '≔', '≤', '≥', '≠', '<', '>'])
}

/// The row's known values, by column name.
/// Whether `expr` indexes one of the table's `columns` by row (`x[i+1]`).
fn indexes(expr: &Expr, columns: &[&str]) -> bool {
    let mut found = false;
    walk_names(expr, columns, &mut |_, indexed| found |= indexed);
    found
}

/// The names `expr` reads other than as a column indexed by row.
fn bare_names(expr: &Expr, columns: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    walk_names(expr, columns, &mut |name, indexed| {
        if !indexed && !out.iter().any(|n| n == name) {
            out.push(name.to_string());
        }
    });
    out
}

/// Calls `f` with every name `expr` reads and whether it is a column indexed by row.
fn walk_names(expr: &Expr, columns: &[&str], f: &mut dyn FnMut(&str, bool)) {
    match expr {
        Expr::Num(_) | Expr::Unit(_) => {}
        Expr::Name(n) => f(n, false),
        Expr::Index(base, index) => {
            match &**base {
                Expr::Name(n) if columns.contains(&n.as_str()) => f(n, true),
                other => walk_names(other, columns, f),
            }
            walk_names(index, columns, f);
        }
        Expr::Neg(e) => walk_names(e, columns, f),
        Expr::Bin(_, a, b) | Expr::Range(a, b) => {
            walk_names(a, columns, f);
            walk_names(b, columns, f);
        }
        Expr::Call(g, args) if g.is_rounding() => args.iter().take(2).for_each(|a| walk_names(a, columns, f)),
        Expr::Call(_, args) | Expr::Apply(_, args) | Expr::Vector(args) => args.iter().for_each(|a| walk_names(a, columns, f)),
        Expr::Nary(n) => {
            for e in [&n.from, &n.to].into_iter().flatten() {
                walk_names(e, columns, f);
            }
            walk_names(&n.body, columns, f);
        }
    }
}

fn row_values(columns: &[Column], label: &[bool], values: &[Vec<Option<Quantity>>], r: usize) -> Vec<(String, Value)> {
    columns
        .iter()
        .enumerate()
        .filter(|(k, _)| label.get(*k) == Some(&false))
        .filter_map(|(k, col)| Some((col.name()?.to_string(), Value::Scalar(values.get(k)?.get(r).copied().flatten()?))))
        .collect()
}
