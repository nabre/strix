//! Parameters shown as their values: a datum the reader only ever saw as a value (`2 m` in a
//! sentence, never `h = 2 m`) is written as that value where a calculation reads it, so the
//! statement reads `A = (2 m)·r` instead of `A = h·r` (docs/calc.md, "Data shown as values").

use super::{Worksheet, positions};
use crate::MAX_INPUT_CHARS;
use crate::eval::Value;
use crate::lexer::{Tok, Token, lex};
use crate::parser::{Func, parse_names};

/// At most this many names are written as values in one line.
const MAX_REPLACED: usize = 256;

impl Worksheet {
    /// `text` (a statement, or a line of its working) with every parameter `value_only` accepts
    /// written as its value, the way a substitution writes it (`(2 m)`, `3`): `A = h·r` becomes
    /// `A = (2 m)·r`. Only where the name is read as that parameter: not alone on a side (`h =`,
    /// `h = 3 m`, `h ≤ 3 m`), not what `:=` defines, not a unit after a number (`3 h`). Parts
    /// after `⇒` are worked the same. `None` when nothing changes.
    pub fn with_values(&self, text: &str, value_only: &dyn Fn(&str) -> bool) -> Option<String> {
        if text.chars().count() > MAX_INPUT_CHARS {
            return None;
        }
        let mut out = String::with_capacity(text.len().saturating_add(16));
        let mut changed = false;
        for (k, piece) in text.split('⇒').enumerate() {
            if k > 0 {
                out.push('⇒');
            }
            match self.piece_with_values(piece, value_only) {
                Some(p) => {
                    out.push_str(&p);
                    changed = true;
                }
                None => out.push_str(piece),
            }
        }
        changed.then_some(out)
    }

    /// [`Self::with_values`] for a text without `⇒`.
    fn piece_with_values(&self, piece: &str, value_only: &dyn Fn(&str) -> bool) -> Option<String> {
        let tokens = lex(piece, self.opts.decimal_comma).ok()?;
        let len = piece.chars().count();
        let cuts = positions(&tokens, |t| matches!(t, Tok::Equals | Tok::Define | Tok::Cmp(_) | Tok::Sep));
        // `f(x) = …` defines a function: its parameters may shadow any name; left as written.
        let head_is_function = matches!(
            (tokens.first().map(|t| &t.tok), tokens.get(1).map(|t| &t.tok)),
            (Some(Tok::Ident(n)), Some(Tok::LParen)) if Func::from_name(n).is_none() && self.scope.function(n).is_none()
        );
        if head_is_function {
            return None;
        }
        let mut replace: Vec<(usize, usize, String)> = Vec::new();
        let mut start = 0usize;
        for end in cuts.iter().copied().chain(std::iter::once(tokens.len())) {
            let seg: &[Token] = tokens.get(start..end).unwrap_or_default();
            start = end.saturating_add(1);
            let defined = tokens.get(end).is_some_and(|t| t.tok == Tok::Define);
            if seg.len() <= 1 || defined {
                continue;
            }
            let end_pos = tokens.get(end).map_or(len, |t| t.pos);
            let is_func = |n: &str| self.scope.function(n).is_some();
            let is_vec = |n: &str| matches!(self.scope.get(n), Some(Value::Vector(_)));
            let Ok((_, names)) = parse_names(seg, end_pos, &is_func, &is_vec) else { continue };
            for i in names {
                let Some(Token { tok: Tok::Ident(name), byte, .. }) = seg.get(i) else { continue };
                if !value_only(name) || replace.len() >= MAX_REPLACED {
                    continue;
                }
                let Some(value) = self.substitute(name, None) else { continue };
                let to = byte.saturating_add(name.len());
                // The name as written (the lexer reads it unchanged); anything else is left alone.
                if piece.get(*byte..to) != Some(name.as_str()) {
                    return None;
                }
                replace.push((*byte, to, value));
            }
        }
        if replace.is_empty() {
            return None;
        }
        replace.sort_by_key(|(from, _, _)| *from);
        let mut out = String::with_capacity(piece.len().saturating_add(16 * replace.len()));
        let mut at = 0usize;
        for (from, to, value) in replace {
            out.push_str(piece.get(at..from)?);
            out.push_str(&value);
            at = to;
        }
        out.push_str(piece.get(at..)?);
        Some(out)
    }
}
