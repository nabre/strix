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
    PlusMinus,
    Minus,
    /// `*`, `·`, `×`, `⋅`, `∙`.
    Star,
    /// `/`, `÷`, `∕`.
    Slash,
    Caret,
    /// A superscript power `[⁺⁻]?[⁰¹²³⁴⁵⁶⁷⁸⁹]+`.
    Sup(i32),
    /// `!`: postfix factorial.
    Bang,
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
    /// `▒`: the operand of an n-ary operator follows (Word's linear format); also Word's
    /// function application U+2061, the argument of a function follows.
    Operand,
    /// An n-ary operator: `∑` (sum), `∏` (product) or `∫` (definite integral).
    Nary(char),
    /// A comparison of a check: `≤` (also `<=`), `≥` (`>=`), `<`, `>` or `≠` (`!=`, unless the
    /// `!` is written against an operand: `n!=` is a factorial and `=`).
    Cmp(char),
    /// `¦`: the bar-less fraction of a binomial coefficient, `(n¦k)` (Word's linear format).
    Choose,
    /// `{`: a grouping bracket, or the brace of cases.
    LBrace,
    /// `}`.
    RBrace,
    /// `|` (single) or `‖` (double): absolute value, determinant, norm. A built-up equation
    /// says which side it is (`├|` opens, `|┤` closes); typed alone it is either.
    Bar {
        double: bool,
        side: Side,
    },
    /// `⌊` (floor) or `⌈` (ceiling).
    LFloor(bool),
    /// `⌋` or `⌉`.
    RFloor(bool),
    /// `∛` (3) or `∜` (4): a root with its degree.
    Root(u8),
    /// `&`: the degree of a radical before its radicand (`√(3&x)`), a matrix's next column, the
    /// condition of a case.
    Amp,
    /// `@`: the next row of a matrix or of cases.
    At,
    /// `■`: a matrix follows, `■(a&b@c&d)`.
    Matrix,
    /// `█`: stacked rows follow (the rows of cases), `█(a,&x<0@b,&x≥0)`.
    Stack,
    /// `┬`: what follows goes below (`lim┬(x→0)`).
    Below,
    /// `→`: the variable of a limit tends to a value.
    Tends,
}

/// Which side of its content a bar is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Side {
    Open,
    Close,
    Either,
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
        Tok::PlusMinus => "±".to_string(),
        Tok::Plus => "+".to_string(),
        Tok::Minus => "-".to_string(),
        Tok::Star => "·".to_string(),
        Tok::Slash => "/".to_string(),
        Tok::Caret => "^".to_string(),
        Tok::Sup(n) => format!("^{n}"),
        Tok::Bang => "!".to_string(),
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
        Tok::Choose => "¦".to_string(),
        Tok::LBrace => "{".to_string(),
        Tok::RBrace => "}".to_string(),
        Tok::Bar { double, .. } => if *double { "‖" } else { "|" }.to_string(),
        Tok::LFloor(up) => if *up { "⌈" } else { "⌊" }.to_string(),
        Tok::RFloor(up) => if *up { "⌉" } else { "⌋" }.to_string(),
        Tok::Root(n) => if *n == 3 { "∛" } else { "∜" }.to_string(),
        Tok::Amp => "&".to_string(),
        Tok::At => "@".to_string(),
        Tok::Matrix => "■".to_string(),
        Tok::Stack => "█".to_string(),
        Tok::Below => "┬".to_string(),
        Tok::Tends => "→".to_string(),
    }
}

/// Splits `text` into tokens. With `decimal_comma`, `1,5` is one and a half, so between
/// two numbers only `;` separates function arguments; a comma that no number takes still does.
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
                // An accent belongs to its letter: `x̅`, `v⃗`, `ẋ` are names.
                while let Some(c) = ch(i).filter(|&c| c.is_alphabetic() || c.is_ascii_digit() || c == '_' || is_accent(c)) {
                    name.push(c);
                    i += 1;
                }
                i = subscript_list(&chars, i, &mut name);
                i = crate::calculus::primes(&chars, i, &mut name);
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
            '+' if next == Some('-') => {
                i += 1;
                Tok::PlusMinus
            }
            '±' => Tok::PlusMinus,
            '+' => Tok::Plus,
            '-' | '\u{2212}' | '\u{2013}' => Tok::Minus,
            '*' | '·' | '×' | '\u{22C5}' | '\u{2219}' => Tok::Star,
            '/' | '÷' | '\u{2215}' | '\u{2044}' => Tok::Slash,
            '^' => Tok::Caret,
            // A sign alone as a superscript is the side of a limit: `x→0⁺`.
            '⁺' | '⁻' if next.and_then(sup_digit).is_none() => {
                out.push(Token { tok: Tok::Caret, pos, byte });
                if c == '⁺' { Tok::Plus } else { Tok::Minus }
            }
            c if sup_digit(c).is_some() || (matches!(c, '⁺' | '⁻') && next.and_then(sup_digit).is_some()) => {
                let (tok, end) = superscript(&chars, i)?;
                out.push(Token { tok, pos, byte });
                i = end;
                continue;
            }
            // Word's invisible grouping brackets `〖…〗` group like parentheses.
            '(' | '〖' => Tok::LParen,
            ')' | '〗' => Tok::RParen,
            '[' => Tok::LBracket,
            ']' => Tok::RBracket,
            '_' => Tok::Under,
            // Word's function application (`sin⁡x`, U+2061) also says the operand follows.
            '▒' | '\u{2061}' => Tok::Operand,
            '∑' => Tok::Nary('∑'),
            '∏' => Tok::Nary('∏'),
            // Only a definite integral is calculated; `∫ f dx = F` stays an ordinary equation.
            '∫' if matches!(next, Some('_' | '^')) => Tok::Nary('∫'),
            // With decimal commas a number has taken its own (`1,5`): any other separates, as
            // after the value of a case (`█(−x,&x<0@…)`).
            ',' | ';' => Tok::Sep,
            ':' if next == Some('=') => {
                i += 1;
                Tok::Define
            }
            '≔' => Tok::Define,
            // `5!=`, `n!=120`: a `!` written against its operand is the factorial, and the `=`
            // after it the statement's (a built-up equation has no spaces: `c=5!=`). Not equal
            // is `≠`, or `!=` set apart from what is before it (`a != b`).
            '!' if next == Some('=') && i > 0 && ch(i - 1).is_some_and(|p| !p.is_whitespace()) && ends_operand(out.last()) => Tok::Bang,
            '<' | '>' | '!' if next == Some('=') => {
                i += 1;
                Tok::Cmp(match c {
                    '<' => '≤',
                    '>' => '≥',
                    _ => '≠',
                })
            }
            '!' => Tok::Bang,
            '<' | '>' | '≤' | '≥' | '≠' => Tok::Cmp(c),
            '⩽' => Tok::Cmp('≤'),
            '⩾' => Tok::Cmp('≥'),
            '=' => Tok::Equals,
            '√' => Tok::Sqrt,
            '∛' => Tok::Root(3),
            '∜' => Tok::Root(4),
            '¦' => Tok::Choose,
            '{' => Tok::LBrace,
            '}' => Tok::RBrace,
            // A built-up equation marks a bracket that could be either side: `├|x|┤`.
            '├' if matches!(next, Some('|' | '‖')) => {
                i += 1;
                Tok::Bar { double: next == Some('‖'), side: Side::Open }
            }
            '|' | '‖' if next == Some('┤') => {
                i += 1;
                Tok::Bar { double: c == '‖', side: Side::Close }
            }
            '|' | '‖' => Tok::Bar { double: c == '‖', side: Side::Either },
            // The markers alone (`├{█(…)┤`, a brace on one side) only say a bracket follows.
            '├' | '┤' => {
                i += 1;
                continue;
            }
            '⌊' => Tok::LFloor(false),
            '⌈' => Tok::LFloor(true),
            '⌋' => Tok::RFloor(false),
            '⌉' => Tok::RFloor(true),
            '&' => Tok::Amp,
            '@' => Tok::At,
            '■' => Tok::Matrix,
            '█' => Tok::Stack,
            '┬' => Tok::Below,
            '→' => Tok::Tends,
            // Infinity is a number no arithmetic takes: only a limit of an integral reads it.
            '∞' => Tok::Num(f64::INFINITY),
            '⚠' => Tok::Warn,
            other => return Err(CalcError::syntax(pos, format!("unexpected character {other}"))),
        };
        out.push(Token { tok, pos, byte });
        i += 1;
    }
    Ok(nested_subscripts(out))
}

/// A name whose subscript has a subscript, `V_A_pl`, comes back from a built-up equation as
/// `(V_A)_pl`: written against each other, that is the one name `V_A_pl` (nothing else reads a
/// bracketed name followed by `_`).
fn nested_subscripts(tokens: Vec<Token>) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::with_capacity(tokens.len());
    let mut i = 0usize;
    while let Some(t) = tokens.get(i) {
        if let (
            Some(Tok::LParen),
            Some(Token { tok: Tok::Ident(a), pos: pa, .. }),
            Some(Token { tok: Tok::RParen, pos: pr, .. }),
            Some(Token { tok: Tok::Under, pos: pu, .. }),
            Some(Token { tok: Tok::Ident(b), pos: pb, .. }),
        ) = (Some(&t.tok), tokens.get(i + 1), tokens.get(i + 2), tokens.get(i + 3), tokens.get(i + 4))
            && a.contains('_')
            && !a.ends_with('_')
            && *pa == t.pos + 1
            && *pr == pa + a.chars().count()
            && *pu == pr + 1
            && *pb == pu + 1
            && b.chars().all(char::is_alphanumeric)
        {
            out.push(Token { tok: Tok::Ident(format!("{a}_{b}")), pos: t.pos, byte: t.byte });
            i += 5;
            continue;
        }
        out.push(t.clone());
        i += 1;
    }
    out
}

/// A name with a subscript and a superscript of letters written against it (`Z_RT^S`, a
/// section modulus): one name where `defined` says so (a parameter of that name, or the name a
/// statement defines); otherwise `x_1^n` stays a power.
pub(crate) fn scripted_names(tokens: Vec<Token>, defined: &dyn Fn(&str) -> bool) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::with_capacity(tokens.len());
    let mut i = 0usize;
    while let Some(t) = tokens.get(i) {
        if let Some(name) = scripted_at(&tokens, i)
            && defined(&name)
        {
            out.push(Token { tok: Tok::Ident(name), pos: t.pos, byte: t.byte });
            i += 3;
            continue;
        }
        out.push(t.clone());
        i += 1;
    }
    out
}

/// The name `a^b` when tokens `i..i+3` are a subscripted name, `^` and letters, written against
/// each other.
pub(crate) fn scripted_at(tokens: &[Token], i: usize) -> Option<String> {
    let (
        Some(Token { tok: Tok::Ident(a), pos: pa, .. }),
        Some(Token { tok: Tok::Caret, pos: pc, .. }),
        Some(Token { tok: Tok::Ident(b), pos: pb, .. }),
    ) = (tokens.get(i), tokens.get(i + 1), tokens.get(i + 2))
    else {
        return None;
    };
    let joined = a.contains('_')
        && !a.ends_with('_')
        && *pc == pa + a.chars().count()
        && *pb == pc + 1
        && b.chars().count() <= 8
        && b.chars().all(char::is_alphabetic);
    joined.then(|| format!("{a}^{b}"))
}

/// True when a postfix `!` can follow `last`: it ends a value (`5`, `n`, `(n − 1)`, `x²`, `3!`).
fn ends_operand(last: Option<&Token>) -> bool {
    matches!(
        last.map(|t| &t.tok),
        Some(
            Tok::Num(_)
                | Tok::Ident(_)
                | Tok::RParen
                | Tok::RBracket
                | Tok::RBrace
                | Tok::Sup(_)
                | Tok::Bang
                | Tok::RFloor(_)
                | Tok::Bar { side: Side::Close, .. }
        )
    )
}

/// Reads a number starting at character `start`; returns the token and the index after it.
/// A subscript listing several indices, `_(1,2)` (a trapezoidal load's ends `q_(1,1)`,
/// `q_(1,2)`), belongs to the name it follows: `name` ends with `_`, `start` is at `(`. Only
/// letters and digits separated by single commas, so `y_(i+1)` (a table's row) stays as it was.
fn subscript_list(chars: &[(usize, char)], start: usize, name: &mut String) -> usize {
    let ch = |i: usize| chars.get(i).map(|&(_, c)| c);
    if !name.ends_with('_') || ch(start) != Some('(') {
        return start;
    }
    let mut inner = String::new();
    let mut j = start.saturating_add(1);
    while let Some(c) = ch(j).filter(|&c| c.is_alphanumeric() || c == ',') {
        inner.push(c);
        j += 1;
    }
    let good = ch(j) == Some(')') && inner.contains(',') && inner.len() <= 16 && inner.split(',').all(|part| !part.is_empty());
    if !good {
        return start;
    }
    name.push('(');
    name.push_str(&inner);
    name.push(')');
    j.saturating_add(1)
}

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

/// A combining accent of the Equation tab's Accent gallery (dot, bar, hat, arrow…).
fn is_accent(c: char) -> bool {
    matches!(c, '\u{300}'..='\u{36F}' | '\u{20D0}'..='\u{20F0}')
}

fn sup_digit(c: char) -> Option<i32> {
    Some(match c {
        '⁰' => 0,
        '¹' => 1,
        '²' => 2,
        '³' => 3,
        '⁴' => 4,
        '⁵' => 5,
        '⁶' => 6,
        '⁷' => 7,
        '⁸' => 8,
        '⁹' => 9,
        _ => return None,
    })
}

fn superscript(chars: &[(usize, char)], start: usize) -> Result<(Tok, usize), CalcError> {
    let ch = |i: usize| chars.get(i).map(|&(_, c)| c);
    let mut i = start;
    let sign = match ch(i) {
        Some('⁺') => {
            i += 1;
            1i32
        }
        Some('⁻') => {
            i += 1;
            -1i32
        }
        _ => 1i32,
    };
    let mut acc = 0i32;
    while let Some(d) = ch(i).and_then(sup_digit) {
        acc = acc.checked_mul(10).and_then(|v| v.checked_add(d)).filter(|&v| v <= 1_000_000).ok_or(CalcError::Overflow)?;
        i += 1;
    }
    Ok((Tok::Sup(sign * acc), i))
}
