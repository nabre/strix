//! Tokens of the linear math text.

use crate::CalcError;
use crate::quantity::finite;

/// A token kind.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Tok {
    Num(f64),
    /// A name: parameter, constant, unit or function (also `°` and `%`).
    Ident(String),
    Plus,
    Minus,
    /// `*`, `·`, `×`, `⋅`, `∙`.
    Star,
    /// `/`, `÷`, `∕`.
    Slash,
    Caret,
    /// A superscript power `¹ ² ³`.
    Sup(u8),
    LParen,
    RParen,
    /// Function argument separator: `;`, and `,` unless decimal commas are on.
    Sep,
    /// `:=` or `≔`.
    Define,
    Equals,
    /// `√`.
    Sqrt,
    /// `⚠`, the error marker written back by a failed evaluation.
    Warn,
    /// `[`: a vector literal or an index.
    LBracket,
    /// `]`.
    RBracket,
    /// `..` or `…`: a range.
    Range,
    /// `_` after an n-ary operator: its lower limit.
    Under,
    /// `▒`: the operand of an n-ary operator follows (Word's linear format).
    Operand,
    /// An n-ary operator: `∑` (sum) or `∏` (product).
    Nary(char),
    /// A comparison of a check: `≤` (also `<=`), `≥` (`>=`), `<`, `>` or `≠` (`!=`).
    Cmp(char),
}

/// A token with its 0-based character position and byte offset in the statement.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Token {
    pub tok: Tok,
    pub pos: usize,
    pub byte: usize,
}

/// A short description of a token for messages.
pub(crate) fn describe(tok: &Tok) -> String {
    match tok {
        Tok::Num(_) => "number".to_string(),
        Tok::Ident(name) => name.clone(),
        Tok::Plus => "+".to_string(),
        Tok::Minus => "-".to_string(),
        Tok::Star => "·".to_string(),
        Tok::Slash => "/".to_string(),
        Tok::Caret => "^".to_string(),
        Tok::Sup(n) => format!("^{n}"),
        Tok::LParen => "(".to_string(),
        Tok::RParen => ")".to_string(),
        Tok::Sep => "an argument separator".to_string(),
        Tok::Define => ":=".to_string(),
        Tok::Equals => "=".to_string(),
        Tok::Sqrt => "√".to_string(),
        Tok::Warn => "⚠".to_string(),
        Tok::LBracket => "[".to_string(),
        Tok::RBracket => "]".to_string(),
        Tok::Range => "..".to_string(),
        Tok::Under => "_".to_string(),
        Tok::Operand => "▒".to_string(),
        Tok::Nary(c) | Tok::Cmp(c) => c.to_string(),
    }
}

/// Splits `text` into tokens. With `decimal_comma`, `1,5` is one and a half and only `;`
/// separates function arguments.
pub(crate) fn lex(text: &str, decimal_comma: bool) -> Result<Vec<Token>, CalcError> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let ch = |i: usize| chars.get(i).map(|&(_, c)| c);
    let mut out = Vec::new();
    let mut i = 0usize;
    while let Some(&(byte, c)) = chars.get(i) {
        let pos = i;
        let next = ch(i.saturating_add(1));
        let next_is_digit = next.is_some_and(|n| n.is_ascii_digit());
        let tok = match c {
            c if c.is_whitespace() || c == '\u{FE0F}' || c == '\u{200B}' => {
                i += 1;
                continue;
            }
            '0'..='9' => {
                let (tok, end) = number(&chars, i, decimal_comma)?;
                out.push(Token { tok, pos, byte });
                i = end;
                continue;
            }
            '.' if next == Some('.') => {
                i += 1;
                Tok::Range
            }
            '…' => Tok::Range,
            '.' if next_is_digit => {
                let (tok, end) = number(&chars, i, decimal_comma)?;
                out.push(Token { tok, pos, byte });
                i = end;
                continue;
            }
            // A Greek capital sigma before limits or an operand is the summation sign.
            'Σ' if matches!(next, Some('_' | '^' | '▒')) => {
                out.push(Token { tok: Tok::Nary('∑'), pos, byte });
                i += 1;
                continue;
            }
            c if c.is_alphabetic() => {
                let mut name = String::new();
                while let Some(c) = ch(i).filter(|&c| c.is_alphabetic() || c.is_ascii_digit() || c == '_') {
                    name.push(c);
                    i += 1;
                }
                out.push(Token { tok: Tok::Ident(name), pos, byte });
                continue;
            }
            // `°C` and `°F` are one unit (also `℃`, `℉`); `°` alone is the degree of angle.
            '°' if matches!(next, Some('C' | 'F')) && !ch(i.saturating_add(2)).is_some_and(|c| c.is_alphanumeric() || c == '_') => {
                i += 1;
                Tok::Ident(if next == Some('C') { "°C" } else { "°F" }.to_string())
            }
            '\u{2103}' => Tok::Ident("°C".to_string()),
            '\u{2109}' => Tok::Ident("°F".to_string()),
            '°' | '%' | '‰' => Tok::Ident(c.to_string()),
            '+' => Tok::Plus,
            '-' | '\u{2212}' | '\u{2013}' => Tok::Minus,
            '*' | '·' | '×' | '\u{22C5}' | '\u{2219}' => Tok::Star,
            '/' | '÷' | '\u{2215}' => Tok::Slash,
            '^' => Tok::Caret,
            '¹' => Tok::Sup(1),
            '²' => Tok::Sup(2),
            '³' => Tok::Sup(3),
            // Word's invisible grouping brackets `〖…〗` group like parentheses.
            '(' | '〖' => Tok::LParen,
            ')' | '〗' => Tok::RParen,
            '[' => Tok::LBracket,
            ']' => Tok::RBracket,
            '_' => Tok::Under,
            '▒' => Tok::Operand,
            '∑' => Tok::Nary('∑'),
            '∏' => Tok::Nary('∏'),
            ',' if !decimal_comma => Tok::Sep,
            ';' => Tok::Sep,
            ':' if next == Some('=') => {
                i += 1;
                Tok::Define
            }
            '≔' => Tok::Define,
            '<' | '>' | '!' if next == Some('=') => {
                i += 1;
                Tok::Cmp(match c {
                    '<' => '≤',
                    '>' => '≥',
                    _ => '≠',
                })
            }
            '<' | '>' | '≤' | '≥' | '≠' => Tok::Cmp(c),
            '⩽' => Tok::Cmp('≤'),
            '⩾' => Tok::Cmp('≥'),
            '=' => Tok::Equals,
            '√' => Tok::Sqrt,
            '⚠' => Tok::Warn,
            other => return Err(CalcError::syntax(pos, format!("unexpected character {other}"))),
        };
        out.push(Token { tok, pos, byte });
        i += 1;
    }
    Ok(out)
}

/// Reads a number starting at character `start`; returns the token and the index after it.
fn number(chars: &[(usize, char)], start: usize, decimal_comma: bool) -> Result<(Tok, usize), CalcError> {
    let ch = |i: usize| chars.get(i).map(|&(_, c)| c);
    let is_digit = |i: usize| ch(i).is_some_and(|c| c.is_ascii_digit());
    let mut text = String::new();
    let mut i = start;
    while let Some(c) = ch(i).filter(char::is_ascii_digit) {
        text.push(c);
        i += 1;
    }
    let separator = ch(i) == Some('.') || (decimal_comma && ch(i) == Some(','));
    if separator && is_digit(i + 1) {
        text.push('.');
        i += 1;
        while let Some(c) = ch(i).filter(char::is_ascii_digit) {
            text.push(c);
            i += 1;
        }
    }
    if matches!(ch(i), Some('e' | 'E')) {
        let mut j = i + 1;
        let mut exponent = String::from("e");
        if let Some(sign @ ('+' | '-')) = ch(j) {
            exponent.push(sign);
            j += 1;
        }
        if is_digit(j) {
            while let Some(c) = ch(j).filter(char::is_ascii_digit) {
                exponent.push(c);
                j += 1;
            }
            text.push_str(&exponent);
            i = j;
        }
    }
    if text.starts_with('.') {
        text.insert(0, '0');
    }
    let value: f64 = text.parse().map_err(|_| CalcError::syntax(start, "malformed number"))?;
    Ok((Tok::Num(finite(value)?), i))
}
