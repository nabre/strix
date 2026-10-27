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
        let body = body.get(..MAX_VALUES.min(body.len())).unwrap_or_default();
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
                if text.is_empty() {
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
        let active: Vec<bool> =
            (0..body.len()).map(|r| data_row.get(r) == Some(&true) && (0..columns.len()).any(|c| is_input(c) && !cell(r, c).is_empty())).collect();

        // Formula columns, left to right, row by row.
        for (c, col) in columns.iter().enumerate() {
            let Column::Formula { unit, expr, .. } = col else { continue };
            let mut used = Vec::new();
            free_names(expr, &mut used);
            for r in 0..body.len() {
                if data_row.get(r) != Some(&true) {
                    continue;
                }
                if active.get(r) != Some(&true) {
                    if !cell(r, c).is_empty() {
                        out.updates.push((r + 1, c, String::new()));
                    }
                    continue;
                }
                let mut locals = Vec::new();
                let mut missing = None;
                for (k, other) in columns.iter().enumerate() {
                    let Some(name) = other.name().filter(|_| label.get(k) == Some(&false)) else { continue };
                    match values.get(k).and_then(|v| v.get(r)).copied().flatten() {
                        Some(q) => locals.push((name.to_string(), Value::Scalar(q))),
                        None if used.iter().any(|u| u == name) && missing.is_none() => missing = Some(name.to_string()),
                        None => {}
                    }
                }
                let result = match missing {
                    Some(name) => Err(CalcError::MissingValue(name)),
                    None => Ctx::new(&self.scope, Names::Worksheet, locals).eval(expr).and_then(|v| v.expect_scalar()),
                };
                let text = match result.and_then(|q| Ok((q, self.cell_text(q, unit.as_ref())?))) {
                    Ok((q, text)) => {
                        if let Some(slot) = values.get_mut(c).and_then(|v| v.get_mut(r)) {
                            *slot = Some(q);
                        }
                        text
                    }
                    Err(err) => {
                        out.errors.push((r + 1, c, err));
                        "⚠".to_string()
                    }
                };
                if text != cell(r, c) {
                    out.updates.push((r + 1, c, text));
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
                let shown = aggregate(kind, &column_values).and_then(|q| self.cell_text(q, unit));
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
                    out.columns.push(name.to_string());
                }
                _ => {
                    self.scope.remove(name);
                }
            }
        }
        out
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
        match unit {
            Some(u) if q.dim.is_dimensionless() => q.try_mul(u.q),
            Some(u) if q.dim != u.q.dim => Err(CalcError::WrongUnit { result: crate::units::dim_name(q.dim), requested: u.text.clone() }),
            _ => Ok(q),
        }
    }

    /// A computed cell: the number in the column's unit, or the number and its automatic unit.
    fn cell_text(&self, q: Quantity, unit: Option<&Unit>) -> Result<String, CalcError> {
        match unit {
            Some(u) => in_unit_text(&Value::Scalar(q), u.q, &u.text, &self.opts),
            None => {
                let (number, unit) = self.auto(&Value::Scalar(q));
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
