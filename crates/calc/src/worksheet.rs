//! Statements and worksheet evaluation.

use crate::eval::{Names, Scope, Value, eval, eval_scalar};
use crate::format::{format_number, render};
use crate::lexer::{Tok, Token, lex};
use crate::parser::{Expr, Func, parse};
use crate::quantity::{Dim, Quantity, finite};
use crate::solve::{free_names, isolate, numeric};
use crate::units::{auto_unit, constant, dim_name, unit};
use crate::{CalcError, MAX_INPUT_CHARS};

/// Evaluation and number-format options (`docs/calc.md`, Number format).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Digits shown in results (default 3, at most 15): decimal places or significant digits, as
    /// [`Options::count`] says.
    pub digits: u8,
    /// What [`Options::digits`] counts.
    pub count: Count,
    /// Keep trailing zeros (`2.500`).
    pub zeros: bool,
    /// Decimal comma: `1,5` is one and a half (input and output) and `;` separates function
    /// arguments.
    pub decimal_comma: bool,
}

/// What [`Options::digits`] counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Count {
    /// Decimal places for |x| ≥ 1, significant digits below 1.
    #[default]
    Auto,
    /// Decimal places.
    Decimals,
    /// Significant digits.
    Significant,
}

impl Default for Options {
    fn default() -> Self {
        Options { digits: 3, count: Count::Auto, zeros: false, decimal_comma: false }
    }
}

/// What a statement is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    /// `name = 4.5 m`, `name = expr` with `name` new, or `name := expr`.
    Definition,
    /// An equation with one unknown that is not alone on a side (`F = m·a` with `a` unknown):
    /// solved for it, which is then defined; its value is shown after `⇒`.
    Solved,
    /// An equation whose names are all known: the two sides must agree.
    Check,
    /// `expr =` or `expr = <number> <unit>`: show the value.
    Evaluation,
    /// `name := expr =` (optionally with a number and unit after `=`): define and show.
    DefineAndShow,
    /// Not a calc statement (no `:=`, no evaluation `=`); left untouched.
    NotCalc,
}

/// The outcome of one statement.
#[derive(Debug, Clone, PartialEq)]
pub struct RegionResult {
    /// What the statement is.
    pub kind: RegionKind,
    /// The name defined (definitions only).
    pub name: Option<String>,
    /// The computed value, when evaluation succeeded.
    pub value: Option<Value>,
    /// The new linear text for the equation, or `None` to leave it unchanged (definitions and
    /// non-calc text). For an evaluation it is `<left> = <number> <unit>`, or `<left> = ⚠ <unit>`
    /// on error (the requested unit is kept when it parsed, so fixing the inputs restores it).
    pub display: Option<String>,
    /// Why the statement failed.
    pub error: Option<CalcError>,
}

impl RegionResult {
    fn not_calc() -> Self {
        RegionResult { kind: RegionKind::NotCalc, name: None, value: None, display: None, error: None }
    }
}

/// A worksheet: statements evaluated in order against one shared [`Scope`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Worksheet {
    pub(crate) scope: Scope,
    pub(crate) opts: Options,
    /// Working units: the last unit written for each dimension (`b = 300 mm` makes lengths show
    /// in mm), as its quantity and text.
    working: Vec<(Dim, Quantity, String)>,
}

/// Evaluates `statements` in order with one shared scope (see [`Worksheet::eval_statement`]).
pub fn evaluate(statements: &[&str], opts: &Options) -> Vec<RegionResult> {
    let mut sheet = Worksheet::new(*opts);
    statements.iter().map(|s| sheet.eval_statement(s)).collect()
}

impl Worksheet {
    /// An empty worksheet.
    pub fn new(opts: Options) -> Self {
        Worksheet { scope: Scope::new(), opts, working: Vec::new() }
    }

    /// Evaluates an expression (`"L/2 + 10 mm"`) with the parameters defined so far, without
    /// changing the worksheet.
    pub fn eval_expr(&self, text: &str) -> Result<Value, CalcError> {
        if text.chars().count() > MAX_INPUT_CHARS {
            return Err(CalcError::TooLong { limit: MAX_INPUT_CHARS });
        }
        let tokens = lex(text, self.opts.decimal_comma)?;
        let expr = parse(&tokens, text.chars().count())?;
        eval(&expr, &self.scope, Names::Worksheet)
    }

    /// The working unit for `dim`: the last unit written for it.
    pub fn working_unit(&self, dim: Dim) -> Option<&str> {
        self.working.iter().find(|(d, _, _)| *d == dim).map(|(_, _, t)| t.as_str())
    }

    /// Remembers the unit written in `tokens` (`4.5 m`, `= kN·m`), if any, as the working unit
    /// of its dimension. Plain numbers and angles don't count.
    fn remember_unit(&mut self, tokens: &[Token], len: usize) {
        let Ok(Some(expr)) = requested_unit(tokens, len) else { return };
        let Ok(q) = eval_scalar(&expr, &self.scope, Names::UnitsOnly) else { return };
        if q.dim.is_dimensionless() || q.value == 0.0 || !q.value.is_finite() {
            return;
        }
        self.remember(q, render(&expr));
    }

    pub(crate) fn remember(&mut self, q: Quantity, text: String) {
        self.working.retain(|(d, _, _)| *d != q.dim);
        self.working.push((q.dim, q, text));
    }

    /// The parameters defined so far.
    pub fn scope(&self) -> &Scope {
        &self.scope
    }

    /// The options.
    pub fn options(&self) -> &Options {
        &self.opts
    }

    /// Evaluates one statement, updating the scope.
    ///
    /// - `name := expr` defines `name` (a failed definition undefines it, so later uses error
    ///   instead of silently using a stale value).
    /// - `expr =` shows the value in an automatic unit; `expr = unit` or `expr = 1.2 unit`
    ///   shows it in `unit` (a stale number after `=`, including our own `1.2·10^9` form or
    ///   `⚠`, is replaced). The text after `=` must consist of units only; otherwise (`E = mc²`)
    ///   the statement is not a calc statement.
    /// - `name := expr =` defines and shows.
    pub fn eval_statement(&mut self, text: &str) -> RegionResult {
        if text.chars().count() > MAX_INPUT_CHARS {
            return self.raw_failure(text, CalcError::TooLong { limit: MAX_INPUT_CHARS });
        }
        // A solved unknown is shown after `⇒`: that tail is our own output, read for its unit.
        let (text, tail) = match text.find('⇒') {
            Some(b) => (text.get(..b).unwrap_or(text).trim_end(), text.get(b + '⇒'.len_utf8()..)),
            None => (text, None),
        };
        let len = text.chars().count();
        let tokens = match lex(text, self.opts.decimal_comma) {
            Ok(tokens) => tokens,
            Err(err) => return self.raw_failure(text, err),
        };
        // `=` inside brackets is part of an expression (the lower limit of `∑_(i=1)`).
        let comparisons: Vec<usize> = positions(&tokens, |t| matches!(t, Tok::Cmp(_)));
        if let [cmp] = comparisons.as_slice()
            && !tokens.iter().any(|t| matches!(t.tok, Tok::Define | Tok::Equals))
        {
            return self.comparison(text, &tokens, *cmp, len);
        }
        let defines: Vec<usize> = positions(&tokens, |t| matches!(t, Tok::Define));
        let equals: Vec<usize> = positions(&tokens, |t| matches!(t, Tok::Equals));
        match (defines.as_slice(), equals.as_slice()) {
            ([], []) => RegionResult::not_calc(),
            ([], _) => self.equation(text, &tokens, &equals, tail, len),
            ([def], []) => self.definition(text, &tokens, *def, None, len),
            ([def], [eq]) if def < eq => self.definition(text, &tokens, *def, Some(*eq), len),
            _ => {
                let at = equals.iter().chain(defines.iter().skip(1)).min().and_then(|&i| tokens.get(i));
                let err = CalcError::syntax(at.map_or(0, |t| t.pos), "a statement takes one := and at most one = after it");
                self.raw_failure(text, err)
            }
        }
    }

    /// A statement with `=` only: `x = 4.5 m` (define), `expr =` / `expr = unit` (show), or an
    /// equation `a = b` (optionally followed by `= unit` to show the result) that defines its one
    /// unknown.
    fn equation(&mut self, text: &str, tokens: &[Token], equals: &[usize], tail: Option<&str>, len: usize) -> RegionResult {
        let mut parts = Vec::with_capacity(equals.len() + 1);
        let mut start = 0;
        for &e in equals {
            parts.push(start..e);
            start = e + 1;
        }
        parts.push(start..tokens.len());
        let part = |i: usize| parts.get(i).and_then(|r| tokens.get(r.clone())).unwrap_or_default();
        let last = part(parts.len() - 1);
        let shows = parts.len() >= 2 && units_only(last);
        match parts.len() - usize::from(shows) {
            1 => {
                // `L = 4.5 m`, `x^2 + x = 6`, `x^2 + x·1 m = A`: a number or a parameter after `=`
                // with an unknown before it makes an equation; with every name known it is a
                // result to refresh, and units alone ask for a display unit.
                let reads_parameter = last.iter().any(|t| matches!(&t.tok, Tok::Ident(n) if self.scope.get(n).is_some()));
                if (starts_with_number(last) || reads_parameter) && self.has_unknown(part(0), last, len) {
                    return self.solve(text, tokens, &parts, false, tail, len);
                }
                match equals.first() {
                    Some(&eq) => self.evaluation(text, tokens, eq, len),
                    None => RegionResult::not_calc(),
                }
            }
            2 => self.solve(text, tokens, &parts, shows, tail, len),
            _ => {
                let pos = equals.get(1).and_then(|&i| tokens.get(i)).map_or(0, |t| t.pos);
                let err = CalcError::syntax(pos, "an equation has two sides; a last = only shows the result");
                RegionResult { error: Some(err), ..RegionResult::not_calc() }
            }
        }
    }

    /// True when `left = right` (both parsed) has an unknown name.
    fn has_unknown(&self, left: &[Token], right: &[Token], len: usize) -> bool {
        match (parse(left, len), parse(right, len)) {
            (Ok(l), Ok(r)) => !self.unknowns(&l, &r).is_empty(),
            _ => false,
        }
    }

    /// A check `a ≤ b` (`≥ < > ≠`): both sides must have the same kind; it is shown as
    /// `a ≤ b ⇒ 5.4 MPa ≤ 355 MPa ✓` (`✗` when it fails, which is also reported as an error).
    /// Nearly equal sides count as equal.
    fn comparison(&mut self, text: &str, tokens: &[Token], cmp: usize, len: usize) -> RegionResult {
        let op = match tokens.get(cmp).map(|t| &t.tok) {
            Some(Tok::Cmp(c)) => *c,
            _ => return RegionResult::not_calc(),
        };
        let cmp_pos = tokens.get(cmp).map_or(len, |t| t.pos);
        let left = tokens.get(..cmp).unwrap_or_default();
        let right = tokens.get(cmp + 1..).unwrap_or_default();
        let (Ok(l), Ok(r)) = (parse(left, cmp_pos), parse(right, len)) else {
            return RegionResult::not_calc();
        };
        if !self.unknowns(&l, &r).is_empty() {
            return RegionResult::not_calc();
        }
        let values = eval(&l, &self.scope, Names::Worksheet)
            .and_then(|a| Ok((a.expect_scalar()?, eval(&r, &self.scope, Names::Worksheet)?.expect_scalar()?)))
            .and_then(|(a, b)| {
                a.same_dim(b, "compare")?;
                Ok((a, b))
            });
        let result = values.and_then(|(a, b)| {
            let close = (a.value - b.value).abs() <= 1e-9 * a.value.abs().max(b.value.abs());
            let holds = match op {
                '≤' => a.value <= b.value || close,
                '≥' => a.value >= b.value || close,
                '<' => a.value < b.value && !close,
                '>' => a.value > b.value && !close,
                _ => !close,
            };
            // Both sides in the left side's unit, so they read side by side.
            let (na, unit) = self.auto(&Value::Scalar(a));
            let u = lex(&unit, false).ok().and_then(|t| parse(&t, unit.chars().count()).ok());
            let scale = u.and_then(|e| eval_scalar(&e, &self.scope, Names::UnitsOnly).ok()).unwrap_or(Quantity::number(1.0));
            let nb = in_unit_text(&Value::Scalar(b), scale, &unit, &self.opts)?;
            let shown = format!("{} {op} {}", with_unit(&na, &unit), with_unit(&nb, &unit));
            Ok((holds, shown))
        });
        let text = text.trim();
        match result {
            Ok((holds, shown)) => RegionResult {
                kind: RegionKind::Check,
                name: None,
                value: None,
                display: Some(format!("{text} ⇒ {shown} {}", if holds { '✓' } else { '✗' })),
                error: (!holds).then_some(CalcError::CheckFailed(shown)),
            },
            Err(err) => RegionResult { kind: RegionKind::Check, name: None, value: None, display: Some(format!("{text} ⇒ ⚠")), error: Some(err) },
        }
    }

    /// Defines `name` as `value` (undefines it on an error).
    fn assign(&mut self, kind: RegionKind, name: &str, value: Result<Value, CalcError>, display: Option<String>) -> RegionResult {
        match value {
            Ok(v) => {
                self.scope.set(name, v.clone());
                RegionResult { kind, name: Some(name.to_string()), value: Some(v), display, error: None }
            }
            Err(err) => {
                self.scope.remove(name);
                RegionResult { kind, name: Some(name.to_string()), value: None, display, error: Some(err) }
            }
        }
    }

    /// The names of an equation that have no value: not parameters, constants or units — except
    /// that a name alone on a side is unknown whenever it is not a parameter (`m = 3 kg` defines
    /// `m` although `m` is also the metre).
    fn unknowns(&self, lhs: &Expr, rhs: &Expr) -> Vec<String> {
        let mut names = Vec::new();
        free_names(lhs, &mut names);
        free_names(rhs, &mut names);
        let alone = |e: &Expr, n: &str| matches!(e, Expr::Name(m) if m == n);
        names.retain(|n| {
            self.scope.get(n).is_none()
                && Func::from_name(n).is_none()
                && (alone(lhs, n) || alone(rhs, n) || (constant(n).is_none() && unit(n).is_none()))
        });
        names
    }

    /// An equation `lhs = rhs`, with `parts` the token ranges between its `=` signs.
    fn solve(&mut self, text: &str, tokens: &[Token], parts: &[std::ops::Range<usize>], shows: bool, tail: Option<&str>, len: usize) -> RegionResult {
        let range = |i: usize| parts.get(i).cloned().unwrap_or_default();
        let part = |i: usize| tokens.get(range(i)).unwrap_or_default();
        let end_pos = |i: usize| tokens.get(range(i).end).map_or(len, |t| t.pos);
        let (p0, p1) = (part(0), part(1));
        if p0.is_empty() || p1.is_empty() {
            return RegionResult::not_calc();
        }
        for side in [p0, p1] {
            if starts_with_number(side) && units_only(side) {
                self.remember_unit(side, len);
            }
        }
        if shows {
            self.remember_unit(part(2), len);
        }
        let (Ok(lhs), Ok(rhs)) = (parse(p0, end_pos(0)), parse(p1, end_pos(1))) else {
            return RegionResult::not_calc();
        };
        // `<a> = <b>` as typed, before the `=` that asks for the result.
        let shown_left = || {
            let byte = tokens.get(range(1).end).map_or(text.len(), |t| t.byte);
            text.get(..byte).unwrap_or(text).trim().to_string()
        };
        let unknowns = self.unknowns(&lhs, &rhs);
        match unknowns.as_slice() {
            [] => {
                let a = eval(&lhs, &self.scope, Names::Worksheet);
                let b = eval(&rhs, &self.scope, Names::Worksheet);
                let check = match (&a, &b) {
                    (Ok(a), Ok(b)) => agree(a, b, &self.opts),
                    (Err(e), _) | (_, Err(e)) => Err(e.clone()),
                };
                let (value, display, shown_err) = if shows {
                    let (v, d, e) = self.show_value(&shown_left(), b, part(2), len);
                    (v, Some(d), e)
                } else {
                    (b.ok(), None, None)
                };
                RegionResult { kind: RegionKind::Check, name: None, value, display, error: check.err().or(shown_err) }
            }
            [x] => {
                let alone = matches!(&lhs, Expr::Name(n) if n == x) || matches!(&rhs, Expr::Name(n) if n == x);
                let value = self.solve_for(&lhs, &rhs, x);
                let kind = if alone { RegionKind::Definition } else { RegionKind::Solved };
                let mut result = self.assign(kind, x, value.clone(), None);
                if shows {
                    let (_, d, e) = self.show_value(&shown_left(), value, part(2), len);
                    result.kind = if alone { RegionKind::DefineAndShow } else { RegionKind::Solved };
                    result.display = Some(d);
                    result.error = result.error.or(e);
                } else if !alone {
                    // The unit of a stale `⇒ x = 2.5 m/s²` is kept.
                    let tail_tokens = tail.and_then(|t| lex(t, self.opts.decimal_comma).ok()).unwrap_or_default();
                    let unit = match tail_tokens.as_slice() {
                        [Token { tok: Tok::Ident(n), .. }, Token { tok: Tok::Equals, .. }, rest @ ..] if n == x => rest,
                        _ => &[],
                    };
                    let (_, d, e) = self.show_value(&format!("{} ⇒ {x}", text.trim()), value, unit, len);
                    result.display = Some(d);
                    result.error = result.error.or(e);
                }
                result
            }
            _ => RegionResult { error: Some(CalcError::Underdetermined(unknowns.clone())), ..RegionResult::not_calc() },
        }
    }

    /// The value of `x` that makes `lhs = rhs` true.
    fn solve_for(&self, lhs: &Expr, rhs: &Expr, x: &str) -> Result<Value, CalcError> {
        let direct = match (lhs, rhs) {
            (Expr::Name(n), other) | (other, Expr::Name(n)) if n == x && !reads(other, x) => Some(other.clone()),
            _ => isolate(lhs, rhs, x),
        };
        if let Some(expr) = direct
            && let Ok(v) = eval(&expr, &self.scope, Names::Worksheet)
        {
            // An inverse function gives one value; keep it only if it satisfies the equation.
            let mut scope = self.scope.clone();
            scope.set(x, v.clone());
            let ok = match (eval(lhs, &scope, Names::Worksheet), eval(rhs, &scope, Names::Worksheet)) {
                (Ok(a), Ok(b)) => agree(&a, &b, &self.opts).is_ok(),
                _ => false,
            };
            if ok {
                return Ok(v);
            }
        }
        numeric(lhs, rhs, x, &self.scope).map(Value::Scalar)
    }

    fn evaluation(&mut self, text: &str, tokens: &[Token], eq: usize, len: usize) -> RegionResult {
        let left = tokens.get(..eq).unwrap_or_default();
        let rhs = tokens.get(eq.saturating_add(1)..).unwrap_or_default();
        if left.is_empty() || !units_only(rhs) {
            return RegionResult::not_calc();
        }
        self.remember_unit(rhs, len);
        let Some(eq_token) = tokens.get(eq) else {
            return RegionResult::not_calc();
        };
        let left_text = text.get(..eq_token.byte).unwrap_or(text).trim();
        let (value, display, error) = self.show(left_text, left, eq_token.pos, rhs, len);
        RegionResult { kind: RegionKind::Evaluation, name: None, value, display: Some(display), error }
    }

    fn definition(&mut self, text: &str, tokens: &[Token], def: usize, eq: Option<usize>, len: usize) -> RegionResult {
        let kind = if eq.is_some() { RegionKind::DefineAndShow } else { RegionKind::Definition };
        let eq_token = eq.and_then(|i| tokens.get(i));
        let left_text = eq_token.map(|t| text.get(..t.byte).unwrap_or(text).trim());
        let failed = |name: Option<String>, err: CalcError| RegionResult {
            kind,
            name,
            value: None,
            display: left_text.map(|l| format!("{l} = ⚠")),
            error: Some(err),
        };
        let name = match tokens.get(..def) {
            Some([Token { tok: Tok::Ident(name), .. }]) if name.chars().next().is_some_and(char::is_alphabetic) => name.clone(),
            _ => {
                let pos = tokens.get(def).map_or(0, |t| t.pos);
                return failed(None, CalcError::syntax(pos, "the left side of := must be a single name"));
            }
        };
        if Func::from_name(&name).is_some() {
            return failed(Some(name.clone()), CalcError::BadDefinition(format!("{name} is a function and cannot be redefined")));
        }
        let body_end = eq.unwrap_or(tokens.len());
        let body = tokens.get(def.saturating_add(1)..body_end).unwrap_or_default();
        let body_end_pos = eq_token.map_or(len, |t| t.pos);
        if starts_with_number(body) && units_only(body) {
            self.remember_unit(body, len);
        }
        if let Some(e) = eq {
            self.remember_unit(tokens.get(e.saturating_add(1)..).unwrap_or_default(), len);
        }
        let (value, display, error) = match (eq_token, left_text) {
            (Some(eq_token), Some(left_text)) => {
                let rhs = tokens.get(body_end.saturating_add(1)..).unwrap_or_default();
                let (value, display, error) = self.show(left_text, body, eq_token.pos, rhs, len);
                (value, Some(display), error)
            }
            _ => match parse(body, body_end_pos).and_then(|e| eval(&e, &self.scope, Names::Worksheet)) {
                Ok(v) => (Some(v), None, None),
                Err(err) => (None, None, Some(err)),
            },
        };
        match &value {
            Some(v) => self.scope.set(&name, v.clone()),
            None => {
                self.scope.remove(&name);
            }
        }
        RegionResult { kind, name: Some(name), value, display, error }
    }

    /// Evaluates `expr` (ending at character `expr_end`) and formats `<left> = <value>` with
    /// the unit requested in `rhs`. Returns the value (even if only the display failed), the
    /// display text and the error.
    fn show(&self, left_text: &str, expr: &[Token], expr_end: usize, rhs: &[Token], len: usize) -> (Option<Value>, String, Option<CalcError>) {
        let value = parse(expr, expr_end).and_then(|e| eval(&e, &self.scope, Names::Worksheet));
        self.show_value(left_text, value, rhs, len)
    }

    /// `<left> = <value>` in the unit requested in `rhs`, like [`Self::show`].
    fn show_value(&self, left_text: &str, value: Result<Value, CalcError>, rhs: &[Token], len: usize) -> (Option<Value>, String, Option<CalcError>) {
        let request = requested_unit(rhs, len);
        let unit_text = match &request {
            Ok(Some(e)) => render(e),
            _ => String::new(),
        };
        let shown = value.clone().and_then(|v| {
            let request = request?;
            self.in_unit(&v, request.as_ref(), &unit_text)
        });
        match shown {
            Ok((number, unit)) => (value.ok(), format!("{left_text} = {}", with_unit(&number, &unit)), None),
            Err(err) => (value.ok(), format!("{left_text} = {}", with_unit("⚠", &unit_text)), Some(err)),
        }
    }

    /// `v`'s numbers and unit when none is asked for: the working unit of its dimension, else
    /// the automatic one.
    pub(crate) fn auto(&self, v: &Value) -> (String, String) {
        if let Some((_, q, text)) = self.working.iter().find(|(d, _, _)| *d == v.dim())
            && let Ok(number) = in_unit_text(v, *q, text, &self.opts)
        {
            return (number, text.clone());
        }
        auto_text(v, &self.opts)
    }

    /// `v` as formatted numbers and unit text, in the requested unit or the automatic one.
    fn in_unit(&self, v: &Value, request: Option<&Expr>, unit_text: &str) -> Result<(String, String), CalcError> {
        let Some(request) = request else {
            return Ok(self.auto(v));
        };
        let u = eval_scalar(request, &self.scope, Names::UnitsOnly)?;
        let number = in_unit_text(v, u, unit_text, &self.opts)?;
        Ok((number, unit_text.to_string()))
    }

    /// Classifies text that could not be tokenised (or is too long) from its raw characters and
    /// reports `err` for calc statements; other text is left alone.
    fn raw_failure(&mut self, text: &str, err: CalcError) -> RegionResult {
        let define = text.find(":=").map(|b| (b, 2)).or_else(|| text.find('≔').map(|b| (b, '≔'.len_utf8())));
        let mut prev = None;
        let mut first_eq = None;
        for (b, c) in text.char_indices() {
            if c == '=' && prev != Some(':') && define.is_none_or(|(d, _)| b > d) {
                first_eq = Some(b);
                break;
            }
            prev = Some(c);
        }
        let left = |b: usize| text.get(..b).unwrap_or_default().trim().to_string();
        let warn = |b: usize| Some(format!("{} = ⚠", left(b)));
        match define {
            Some((d, _)) => {
                let name = left(d);
                let valid =
                    name.chars().next().is_some_and(char::is_alphabetic) && name.chars().all(|c| c.is_alphabetic() || c.is_ascii_digit() || c == '_');
                if valid {
                    self.scope.remove(&name);
                }
                let kind = if first_eq.is_some() { RegionKind::DefineAndShow } else { RegionKind::Definition };
                RegionResult { kind, name: valid.then_some(name), value: None, display: first_eq.and_then(warn), error: Some(err) }
            }
            None => match first_eq {
                Some(b) if text.trim_end().ends_with('=') => {
                    RegionResult { kind: RegionKind::Evaluation, name: None, value: None, display: warn(b), error: Some(err) }
                }
                _ => RegionResult::not_calc(),
            },
        }
    }
}

/// Indices of the tokens outside any brackets that match `pred` (all of them when the brackets
/// don't balance, so `(2 =` is still an evaluation with a syntax error).
fn positions(tokens: &[Token], pred: impl Fn(&Tok) -> bool) -> Vec<usize> {
    let mut depth = 0usize;
    let mut out = Vec::new();
    for (i, t) in tokens.iter().enumerate() {
        match t.tok {
            Tok::LParen | Tok::LBracket => depth = depth.saturating_add(1),
            Tok::RParen | Tok::RBracket => depth = depth.saturating_sub(1),
            _ if depth == 0 && pred(&t.tok) => out.push(i),
            _ => {}
        }
    }
    if depth != 0 {
        return tokens.iter().enumerate().filter(|(_, t)| pred(&t.tok)).map(|(i, _)| i).collect();
    }
    out
}

/// Elements shown in full; a longer vector shows its first [`SHOWN_HEAD`], `…` and its last.
const SHOWN_ALL: usize = 10;
const SHOWN_HEAD: usize = 8;

/// The numbers of `v` (already scaled to the display unit) as text: `12.5` or `[1; 2; 3]`.
fn numbers_text(v: &Value, scale: f64, opts: &Options) -> String {
    match v {
        Value::Scalar(q) => format_number(q.value * scale, opts),
        Value::Vector(vec) => {
            let n = vec.len();
            let shown: Vec<String> = vec
                .values
                .iter()
                .enumerate()
                .filter(|(i, _)| n <= SHOWN_ALL || *i < SHOWN_HEAD || *i + 1 == n)
                .map(|(_, x)| format_number(x * scale, opts))
                .collect();
            let mut parts = shown;
            if n > SHOWN_ALL {
                parts.insert(SHOWN_HEAD, "…".to_string());
            }
            format!("[{}]", parts.join("; "))
        }
    }
}

/// `v`'s numbers and its automatic unit; a vector takes the unit of its largest element.
fn auto_text(v: &Value, opts: &Options) -> (String, String) {
    let largest = v.quantities().iter().map(|q| q.value.abs()).fold(0.0, f64::max);
    let (scaled, unit) = auto_unit(largest, v.dim());
    let scale = if largest == 0.0 { 1.0 } else { scaled / largest };
    (numbers_text(v, scale, opts), unit)
}

/// `v`'s numbers in unit `u` (spelled `unit_text` in messages).
pub(crate) fn in_unit_text(v: &Value, u: Quantity, unit_text: &str, opts: &Options) -> Result<String, CalcError> {
    if u.dim != v.dim() {
        return Err(CalcError::WrongUnit { result: dim_name(v.dim()), requested: unit_text.to_string() });
    }
    if u.value == 0.0 {
        return Err(CalcError::DivisionByZero);
    }
    finite(1.0 / u.value)?;
    for q in v.quantities() {
        finite(q.value / u.value)?;
    }
    Ok(numbers_text(v, 1.0 / u.value, opts))
}

/// `v` as it is shown after `=`, in its automatic unit: `30.38 kN`, `[1; 2; 3] m`.
pub fn format_value(v: &Value, opts: &Options) -> String {
    let (number, unit) = auto_text(v, opts);
    with_unit(&number, &unit)
}

/// `number` followed by `unit`: `30.38 kN·m`, `45°`, `0.5`.
pub(crate) fn with_unit(number: &str, unit: &str) -> String {
    match unit {
        "" => number.to_string(),
        "°" => format!("{number}°"),
        _ => format!("{number} {unit}"),
    }
}

/// True when the text after `=` can be a number and a unit: every name in it is a unit. A
/// leading `[…]` is a stale vector result.
fn units_only(rhs: &[Token]) -> bool {
    rhs.get(stale_vector(rhs)..).unwrap_or_default().iter().all(|t| match &t.tok {
        Tok::Ident(name) => unit(name).is_some(),
        Tok::Sqrt
        | Tok::Sep
        | Tok::Define
        | Tok::Equals
        | Tok::LBracket
        | Tok::RBracket
        | Tok::Range
        | Tok::Under
        | Tok::Operand
        | Tok::Nary(_)
        | Tok::Cmp(_) => false,
        _ => true,
    })
}

/// The unit expression after `=`, skipping a stale result in front of it: a vector `[…]`, an
/// optional sign and number, optionally in our scientific form (`1.2·10^9`, `1.2·10^-5`), or `⚠`. A number
/// directly followed by `/` is part of the unit (`= 1/s`).
fn requested_unit(rhs: &[Token], len: usize) -> Result<Option<Expr>, CalcError> {
    let tok = |i: usize| rhs.get(i).map(|t| &t.tok);
    let mut start = stale_vector(rhs);
    if start > 0 {
        // A stale vector result: the unit follows it.
    } else if matches!(tok(0), Some(Tok::Warn)) {
        start = 1;
    } else {
        let mut j = usize::from(matches!(tok(0), Some(Tok::Minus | Tok::Plus)));
        if matches!(tok(j), Some(Tok::Num(_))) && !matches!(tok(j + 1), Some(Tok::Slash)) {
            j += 1;
            if matches!(tok(j), Some(Tok::Star)) && matches!(tok(j + 1), Some(Tok::Num(v)) if *v == 10.0) && matches!(tok(j + 2), Some(Tok::Caret)) {
                let k = j + 3 + usize::from(matches!(tok(j + 3), Some(Tok::Minus | Tok::Plus)));
                if matches!(tok(k), Some(Tok::Num(_))) {
                    j = k + 1;
                }
            }
            start = j;
        }
    }
    let unit = rhs.get(start..).unwrap_or_default();
    if unit.is_empty() { Ok(None) } else { parse(unit, len).map(Some) }
}

/// The number of tokens of a stale vector result `[…]` at the start of `rhs` (0 if none).
fn stale_vector(rhs: &[Token]) -> usize {
    if !matches!(rhs.first().map(|t| &t.tok), Some(Tok::LBracket)) {
        return 0;
    }
    rhs.iter().position(|t| t.tok == Tok::RBracket).map_or(0, |i| i + 1)
}

/// True when the tokens start with a number (optionally signed) or a vector: `4.5 m`, `-3`.
fn starts_with_number(tokens: &[Token]) -> bool {
    let skip = usize::from(matches!(tokens.first().map(|t| &t.tok), Some(Tok::Minus | Tok::Plus)));
    matches!(tokens.get(skip).map(|t| &t.tok), Some(Tok::Num(_) | Tok::LBracket))
}

/// True when `expr` reads `x`.
fn reads(expr: &Expr, x: &str) -> bool {
    let mut names = Vec::new();
    free_names(expr, &mut names);
    names.iter().any(|n| n == x)
}

/// `Ok` when `a` and `b` are the same quantity (or vector) to within rounding.
fn agree(a: &Value, b: &Value, opts: &Options) -> Result<(), CalcError> {
    let (qa, qb) = (a.quantities(), b.quantities());
    let close = |x: f64, y: f64| (x - y).abs() <= 1e-9 * x.abs().max(y.abs()).max(1e-300);
    if a.dim() == b.dim() && qa.len() == qb.len() && qa.iter().zip(&qb).all(|(x, y)| close(x.value, y.value)) {
        return Ok(());
    }
    Err(CalcError::NotEqual { left: format_value(a, opts), right: format_value(b, opts) })
}
