//! Calculated tables: columns of quantities, column formulas and totals (see `docs/calc.md`).
//!
//! The first row is the header. A header cell is a column name with an optional unit in square
//! brackets or parentheses and an optional formula: `L [m]`, `q (kN/m)`, `M [kN·m] = q·L²/8`
//! (`:=` works too).
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
use crate::worksheet::{Worksheet, in_unit_text};
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
}

/// A column's unit: the quantity of one unit and its text as written in the header.
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
            Column::Input { name, .. } | Column::Formula { name, .. } => Some(name),
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

impl Worksheet {
    /// Evaluates a calculated table given as plain cell texts, row by row (row 0 is the
    /// header), and defines its complete columns in the scope. See the module docs.
    pub fn eval_table(&mut self, rows: &[Vec<String>]) -> TableResult {
        let mut out = TableResult::default();
        let Some((header, body)) = rows.split_first() else {
            return out;
        };
        // The grid evaluated is bounded, not just the cells read: columns × rows ≤ MAX_TABLE_CELLS.
        let header = header.get(..MAX_TABLE_COLS.min(header.len())).unwrap_or_default();
        let columns: Vec<Column> = header
            .iter()
            .enumerate()
            .map(|(c, text)| match self.header(text) {
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
        let mut values: Vec<Vec<Option<Quantity>>> = vec![vec![None; body.len()]; columns.len()];
        let mut label = vec![false; columns.len()];
        let data_row: Vec<bool> = body.iter().map(|row| !row.iter().any(|t| total(t).is_some())).collect();
        let mut input_errors = Vec::new();
        for (c, col) in columns.iter().enumerate() {
            let Column::Input { unit, .. } = col else {
                label[c] = matches!(col, Column::Label);
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
                label[c] = true;
                if let Some(v) = values.get_mut(c) {
                    v.iter_mut().for_each(|x| *x = None);
                }
            } else {
                input_errors.extend(errors);
            }
        }
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
        let formulas: Vec<Option<(Option<String>, Vec<String>)>> = columns
            .iter()
            .map(|col| match col {
                Column::Formula { expr, .. } => {
                    let mut used = Vec::new();
                    free_names(expr, &mut used);
                    Some((self.hint(expr), used))
                }
                _ => None,
            })
            .collect();
        let mut steps = 0usize;
        for r in 0..body.len() {
            if data_row.get(r) != Some(&true) {
                continue;
            }
            for (c, col) in columns.iter().enumerate() {
                let text = cell(r, c);
                let named = col.name().filter(|_| label.get(c) == Some(&false)).map(str::to_string);
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
                    if named.is_some()
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
                let locals = row_values(&columns, &label, &values, r);
                let missing = used.iter().find(|u| {
                    columns
                        .iter()
                        .enumerate()
                        .any(|(k, other)| other.name() == Some(u.as_str()) && label.get(k) == Some(&false) && !locals.iter().any(|(n, _)| n == *u))
                });
                let result = match missing {
                    Some(name) => Err(CalcError::MissingValue(name.clone())),
                    None => {
                        let mut ctx = Ctx::new(&self.scope, Names::Worksheet, locals).with_steps(steps);
                        let value = ctx.eval(expr).and_then(|v| v.expect_scalar());
                        steps = ctx.steps();
                        value
                    }
                };
                let shown = match result.and_then(|q| Ok((q, self.cell_text(q, unit.as_ref(), hint.as_deref())?))) {
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

        // Totals.
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

        // Complete columns become vectors in the scope.
        for (c, col) in columns.iter().enumerate() {
            let Some(name) = col.name().filter(|_| label.get(c) == Some(&false)) else { continue };
            let column: Option<Vec<Quantity>> =
                (0..body.len()).filter(|&r| active.get(r) == Some(&true)).map(|r| values.get(c).and_then(|v| v.get(r)).copied().flatten()).collect();
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
        out
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
        let result = self.eval_statement(&statement);
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
    fn header(&self, text: &str) -> Result<Column, CalcError> {
        let text = text.trim();
        if text.is_empty() || text.chars().count() > MAX_INPUT_CHARS {
            return Ok(Column::Label);
        }
        let Ok(tokens) = lex(text, self.opts.decimal_comma) else {
            return Ok(Column::Label);
        };
        let tok = |i: usize| tokens.get(i).map(|t| &t.tok);
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
            let end_pos = tokens.get(end).map_or(0, |t| t.pos);
            let expr = parse(inner, end_pos)?;
            let q = eval_scalar(&expr, &self.scope, Names::UnitsOnly)?;
            let span = |t: Option<&Token>| t.map_or(0, |t| t.byte);
            let raw = text.get(span(tokens.get(2))..span(tokens.get(end))).unwrap_or_default().trim();
            unit = Some(Unit { q, text: raw.to_string() });
            i = end + 1;
        }
        match tok(i) {
            None => Ok(Column::Input { name, unit }),
            Some(Tok::Define | Tok::Equals) => {
                let body = tokens.get(i + 1..).unwrap_or_default();
                let expr = parse(body, text.chars().count())?;
                Ok(Column::Formula { name, unit, expr })
            }
            Some(_) if unit.is_none() => Ok(Column::Label),
            Some(_) => Err(CalcError::BadDefinition("a column header is a name, an optional [unit] and an optional = formula".to_string())),
        }
    }

    /// The value of an input cell in a column with `unit`.
    fn input_cell(&self, text: &str, unit: Option<&Unit>) -> Result<Quantity, CalcError> {
        if text.chars().count() > MAX_INPUT_CHARS {
            return Err(CalcError::TooLong { limit: MAX_INPUT_CHARS });
        }
        let tokens = lex(text, self.opts.decimal_comma)?;
        let expr = parse(&tokens, text.chars().count())?;
        let q = eval_scalar(&expr, &self.scope, Names::Worksheet)?;
        // A cell that writes its own unit (`15 %` in a `[%]` column) is not scaled again.
        let own_unit = tokens.iter().any(|t| {
            matches!(&t.tok, crate::lexer::Tok::Ident(n) if self.scope.get(n).is_none() && crate::units::constant(n).is_none() && crate::units::unit(n).is_some())
        });
        match unit {
            Some(u) if q.dim.is_dimensionless() && !own_unit => q.try_mul(u.q),
            Some(u) if q.dim != u.q.dim => Err(CalcError::WrongUnit { result: crate::units::dim_name(q.dim), requested: u.text.clone() }),
            _ => Ok(q),
        }
    }

    /// A computed cell: the number in the column's unit, or the number and its automatic unit.
    fn cell_text(&self, q: Quantity, unit: Option<&Unit>, hint: Option<&str>) -> Result<String, CalcError> {
        match unit {
            Some(u) => in_unit_text(&Value::Scalar(q), u.q, &u.text, &self.opts),
            None => {
                let (number, unit) = self.auto_hinted(&Value::Scalar(q), hint);
                Ok(crate::worksheet::with_unit(&number, &unit))
            }
        }
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
fn row_values(columns: &[Column], label: &[bool], values: &[Vec<Option<Quantity>>], r: usize) -> Vec<(String, Value)> {
    columns
        .iter()
        .enumerate()
        .filter(|(k, _)| label.get(*k) == Some(&false))
        .filter_map(|(k, col)| Some((col.name()?.to_string(), Value::Scalar(values.get(k)?.get(r).copied().flatten()?))))
        .collect()
}
