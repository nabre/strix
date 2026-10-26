//! Statements and worksheet evaluation.

use crate::eval::{Names, Scope, eval};
use crate::format::{format_number, render};
use crate::lexer::{Tok, Token, lex};
use crate::parser::{Expr, Func, parse};
use crate::quantity::{Quantity, finite};
use crate::units::{auto_unit, dim_name, unit};
use crate::{CalcError, MAX_INPUT_CHARS};

/// Evaluation options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Significant digits shown in results (default 4, clamped to 1..=15).
    pub sig_digits: u8,
    /// Decimal comma: `1,5` is one and a half (input and output) and `;` separates function
    /// arguments.
    pub decimal_comma: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { sig_digits: 4, decimal_comma: false }
    }
}

/// What a statement is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    /// `name := expr`.
    Definition,
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
    pub value: Option<Quantity>,
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
    scope: Scope,
    opts: Options,
}

/// Evaluates `statements` in order with one shared scope (see [`Worksheet::eval_statement`]).
pub fn evaluate(statements: &[&str], opts: &Options) -> Vec<RegionResult> {
    let mut sheet = Worksheet::new(*opts);
    statements.iter().map(|s| sheet.eval_statement(s)).collect()
}

impl Worksheet {
    /// An empty worksheet.
    pub fn new(opts: Options) -> Self {
        Worksheet { scope: Scope::new(), opts }
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
        let len = text.chars().count();
        if len > MAX_INPUT_CHARS {
            return self.raw_failure(text, CalcError::TooLong { limit: MAX_INPUT_CHARS });
        }
        let tokens = match lex(text, self.opts.decimal_comma) {
            Ok(tokens) => tokens,
            Err(err) => return self.raw_failure(text, err),
        };
        let defines: Vec<usize> = positions(&tokens, |t| matches!(t, Tok::Define));
        let equals: Vec<usize> = positions(&tokens, |t| matches!(t, Tok::Equals));
        match (defines.as_slice(), equals.as_slice()) {
            ([], [eq]) => self.evaluation(text, &tokens, *eq, len),
            ([], _) => RegionResult::not_calc(),
            ([def], []) => self.definition(text, &tokens, *def, None, len),
            ([def], [eq]) if def < eq => self.definition(text, &tokens, *def, Some(*eq), len),
            _ => {
                let at = equals.iter().chain(defines.iter().skip(1)).min().and_then(|&i| tokens.get(i));
                let err = CalcError::syntax(at.map_or(0, |t| t.pos), "a statement takes one := and at most one = after it");
                self.raw_failure(text, err)
            }
        }
    }

    fn evaluation(&mut self, text: &str, tokens: &[Token], eq: usize, len: usize) -> RegionResult {
        let left = tokens.get(..eq).unwrap_or_default();
        let rhs = tokens.get(eq.saturating_add(1)..).unwrap_or_default();
        if left.is_empty() || !units_only(rhs) {
            return RegionResult::not_calc();
        }
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
        let (value, display, error) = match (eq_token, left_text) {
            (Some(eq_token), Some(left_text)) => {
                let rhs = tokens.get(body_end.saturating_add(1)..).unwrap_or_default();
                let (value, display, error) = self.show(left_text, body, eq_token.pos, rhs, len);
                (value, Some(display), error)
            }
            _ => match parse(body, body_end_pos).and_then(|e| eval(&e, &self.scope, Names::Worksheet)) {
                Ok(q) => (Some(q), None, None),
                Err(err) => (None, None, Some(err)),
            },
        };
        match value {
            Some(q) => self.scope.set(&name, q),
            None => {
                self.scope.remove(&name);
            }
        }
        RegionResult { kind, name: Some(name), value, display, error }
    }

    /// Evaluates `expr` (ending at character `expr_end`) and formats `<left> = <value>` with
    /// the unit requested in `rhs`. Returns the value (even if only the display failed), the
    /// display text and the error.
    fn show(&self, left_text: &str, expr: &[Token], expr_end: usize, rhs: &[Token], len: usize) -> (Option<Quantity>, String, Option<CalcError>) {
        let request = requested_unit(rhs, len);
        let unit_text = match &request {
            Ok(Some(e)) => render(e),
            _ => String::new(),
        };
        let value = parse(expr, expr_end).and_then(|e| eval(&e, &self.scope, Names::Worksheet));
        let shown = value.clone().and_then(|q| {
            let request = request?;
            self.in_unit(q, request.as_ref(), &unit_text)
        });
        match shown {
            Ok((number, unit)) => (value.ok(), format!("{left_text} = {}", with_unit(&number, &unit)), None),
            Err(err) => (value.ok(), format!("{left_text} = {}", with_unit("⚠", &unit_text)), Some(err)),
        }
    }

    /// `q` as a formatted number and unit text, in the requested unit or the automatic one.
    fn in_unit(&self, q: Quantity, request: Option<&Expr>, unit_text: &str) -> Result<(String, String), CalcError> {
        let Some(request) = request else {
            let (value, unit) = auto_unit(q.value, q.dim);
            return Ok((format_number(value, &self.opts), unit));
        };
        let u = eval(request, &self.scope, Names::UnitsOnly)?;
        if u.dim != q.dim {
            return Err(CalcError::WrongUnit { result: dim_name(q.dim), requested: unit_text.to_string() });
        }
        if u.value == 0.0 {
            return Err(CalcError::DivisionByZero);
        }
        let number = finite(q.value / u.value)?;
        Ok((format_number(number, &self.opts), unit_text.to_string()))
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

fn positions(tokens: &[Token], pred: impl Fn(&Tok) -> bool) -> Vec<usize> {
    tokens.iter().enumerate().filter(|(_, t)| pred(&t.tok)).map(|(i, _)| i).collect()
}

/// `number` followed by `unit`: `30.38 kN·m`, `45°`, `0.5`.
fn with_unit(number: &str, unit: &str) -> String {
    match unit {
        "" => number.to_string(),
        "°" => format!("{number}°"),
        _ => format!("{number} {unit}"),
    }
}

/// True when the text after `=` can be a number and a unit: every name in it is a unit.
fn units_only(rhs: &[Token]) -> bool {
    rhs.iter().all(|t| match &t.tok {
        Tok::Ident(name) => unit(name).is_some(),
        Tok::Sqrt | Tok::Sep | Tok::Define | Tok::Equals => false,
        _ => true,
    })
}

/// The unit expression after `=`, skipping a stale result in front of it: an optional sign and
/// number, optionally in our scientific form (`1.2·10^9`, `1.2·10^-5`), or `⚠`. A number
/// directly followed by `/` is part of the unit (`= 1/s`).
fn requested_unit(rhs: &[Token], len: usize) -> Result<Option<Expr>, CalcError> {
    let tok = |i: usize| rhs.get(i).map(|t| &t.tok);
    let mut start = 0;
    if matches!(tok(0), Some(Tok::Warn)) {
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
