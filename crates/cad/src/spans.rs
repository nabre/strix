//! A drawing text's pieces, as the page, the PDF, the SVG picture and the editor draw them: plain
//! text, units (the Unit style's colour), missing parameters (red), and parameters set as the
//! equations set them — the symbol in italics with its subscript smaller and lowered, the value
//! upright (docs/cad.md, Annotations).

use crate::{MATH_END, MATH_START, MISSING_END, MISSING_START, SYM_END, SYM_START, UNIT_END, UNIT_START};

/// What a piece of text is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanKind {
    Plain,
    /// A unit: the Unit style's colour.
    Unit,
    /// A placeholder whose parameter is not defined: red.
    Missing,
}

/// One piece of a drawing text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// The characters as written (a symbol's plain letters: each output styles them).
    pub text: String,
    pub kind: SpanKind,
    /// Part of a parameter (`{{L}}`), set as an equation: its renderer's math font.
    pub math: bool,
    /// A parameter's symbol: italic.
    pub symbol: bool,
    /// A symbol's subscript: smaller and lowered.
    pub sub: bool,
}

/// `text`'s pieces, in order, without the markers. Empty pieces are left out.
pub fn spans(text: &str) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    let (mut kind, mut math, mut symbol) = (SpanKind::Plain, false, false);
    let mut cur = String::new();
    let flush = |out: &mut Vec<Span>, cur: &mut String, kind: SpanKind, math: bool, symbol: bool| {
        if cur.is_empty() {
            return;
        }
        let text = std::mem::take(cur);
        if !symbol {
            out.push(Span { text, kind, math, symbol, sub: false });
            return;
        }
        // A symbol `F_k`: the base, then its subscript (`F_{k,1}` and `F_(k)` without brackets).
        let (base, sub) = text.split_once('_').map_or((text.as_str(), ""), |(b, s)| (b, s));
        let sub = sub.trim_start_matches(['{', '(']).trim_end_matches(['}', ')']);
        for (t, is_sub) in [(base, false), (sub, true)] {
            if !t.is_empty() {
                out.push(Span { text: t.to_string(), kind, math, symbol, sub: is_sub });
            }
        }
    };
    for c in text.chars() {
        match c {
            UNIT_START | UNIT_END | MISSING_START | MISSING_END | MATH_START | MATH_END | SYM_START | SYM_END => {
                flush(&mut out, &mut cur, kind, math, symbol);
                match c {
                    UNIT_START => kind = SpanKind::Unit,
                    MISSING_START => kind = SpanKind::Missing,
                    UNIT_END | MISSING_END => kind = SpanKind::Plain,
                    MATH_START => math = true,
                    MATH_END => {
                        math = false;
                        symbol = false;
                    }
                    SYM_START => symbol = true,
                    _ => symbol = false,
                }
            }
            c => cur.push(c),
        }
    }
    flush(&mut out, &mut cur, kind, math, symbol);
    out
}
