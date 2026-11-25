//! The unit a calculation's result is asked in: read from the text after its last `=`
//! (`V = a·b·c = 7500 dm³`) and written there, so that a front end can offer the units of the
//! result's quantity and change it without the user retyping the statement (docs/calc.md,
//! "The result of one calculation").

use super::{Worksheet, positions, unit_start, units_only, written_unit};
use crate::MAX_INPUT_CHARS;
use crate::lexer::{Tok, Token, lex};
use crate::parser::parse;

/// The part of `text` after its last `⇒` (a solved unknown's result, a check's values), with the
/// byte where it starts; the whole text when it has none.
fn last_piece(text: &str) -> (usize, &str) {
    match text.rfind('⇒') {
        Some(at) => {
            let from = at.saturating_add('⇒'.len_utf8());
            (from, text.get(from..).unwrap_or_default())
        }
        None => (0, text),
    }
}

/// Where a result stands in a statement: the byte after its last `=` and the byte where the
/// unit asked for starts (after a stale number), both in the whole text, and the tokens of
/// the unit.
struct ResultAt {
    after_equals: usize,
    unit_from: usize,
    unit: Vec<Token>,
}

impl Worksheet {
    /// The result of `text`: after its last `=` outside brackets, nothing, a stale result, a
    /// unit or both (`= 7.5 m³`, `= dm³`, `=`). Not a datum (`L = 4.12 m`: the only `=` after a
    /// name alone is a definition, unless the name is a solved unknown's after `⇒`), not a
    /// check (`a + b = c`).
    fn result_at(&self, text: &str) -> Option<ResultAt> {
        if text.chars().count() > MAX_INPUT_CHARS {
            return None;
        }
        let (head, piece) = last_piece(text);
        let tokens = lex(piece, self.opts.decimal_comma).ok()?;
        let equals = positions(&tokens, |t| matches!(t, Tok::Equals));
        let eq = *equals.last()?;
        let rhs = tokens.get(eq.saturating_add(1)..)?;
        if !units_only(rhs) {
            return None;
        }
        let datum = head == 0 && equals.len() == 1 && matches!(tokens.get(..eq), Some([Token { tok: Tok::Ident(_), .. }]));
        if datum || positions(&tokens, |t| matches!(t, Tok::Define)).iter().any(|&d| d > eq) {
            return None;
        }
        let after_equals = head.saturating_add(tokens.get(eq)?.byte).saturating_add('='.len_utf8());
        let start = unit_start(rhs);
        let unit_from = match rhs.get(start) {
            Some(t) => head.saturating_add(t.byte),
            None => text.len(),
        };
        let unit: Vec<Token> = rhs.get(start..).unwrap_or_default().to_vec();
        Some(ResultAt { after_equals, unit_from, unit })
    }

    /// The unit the result of `text` is asked in (`dm³` in `V = a·b·c = 7500 dm³`), `None`
    /// when it has none or `text` shows no result.
    pub fn result_unit(&self, text: &str) -> Option<String> {
        let at = self.result_at(text)?;
        if at.unit.is_empty() {
            return None;
        }
        let expr = parse(&at.unit, text.chars().count()).ok()?;
        if !written_unit(&expr, 0) {
            return None;
        }
        text.get(at.unit_from..).map(|u| u.trim().to_string()).filter(|u| !u.is_empty())
    }

    /// `text` with its result asked in `unit` (`V = a·b·c = 7.5 m³` → `V = a·b·c = dm³`, worked
    /// again by the next calculation), or with no unit asked for when `unit` is `None` or empty:
    /// the stale number is kept (`= 7.5`), or a bare `=` left. `None` when `text` shows no
    /// result or `unit` is not a unit as written after `=` (`kN·m`, `m/s²`, `1/min`).
    pub fn with_result_unit(&self, text: &str, unit: Option<&str>) -> Option<String> {
        let at = self.result_at(text)?;
        let unit = unit.map(str::trim).filter(|u| !u.is_empty());
        match unit {
            Some(u) => {
                let tokens = lex(u, self.opts.decimal_comma).ok()?;
                if !units_only(&tokens) || unit_start(&tokens) != 0 {
                    return None;
                }
                let expr = parse(&tokens, u.chars().count()).ok()?;
                if !written_unit(&expr, 0) {
                    return None;
                }
                Some(format!("{} {u}", text.get(..at.after_equals)?.trim_end()))
            }
            None => Some(text.get(..at.unit_from)?.trim_end().to_string()),
        }
    }
}
