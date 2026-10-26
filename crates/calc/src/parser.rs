//! Expression parser (precedence climbing over [`Tok`]s).
//!
//! Grammar, loosest first:
//!
//! ```text
//! expr     = term (("+" | "-") term)*
//! term     = unary (("*" | "/") unary)*          explicit, left to right
//! unary    = ("-" | "+") unary | implicit
//! implicit = power power*                        juxtaposition; the right operand must not
//!                                                start with a number
//! power    = postfix ("^" exponent)?             right to left
//! exponent = ("-" | "+") exponent | power
//! postfix  = primary ("¹" | "²" | "³")*
//! primary  = number | name | function "(" args ")" | "(" expr ")" | "√" postfix
//! ```
//!
//! Implicit multiplication binds tighter than `/` so `12 kN/m` is `(12·kN)/m` (see the crate
//! docs). Nesting is capped by [`MAX_DEPTH`] and size by [`MAX_NODES`].

use crate::lexer::{Tok, Token, describe};
use crate::{CalcError, MAX_DEPTH, MAX_NODES};

/// A built-in function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Func {
    Sqrt,
    Abs,
    Exp,
    Ln,
    Log,
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Min,
    Max,
    Round,
}

impl Func {
    pub(crate) fn from_name(name: &str) -> Option<Func> {
        Some(match name {
            "sqrt" => Func::Sqrt,
            "abs" => Func::Abs,
            "exp" => Func::Exp,
            "ln" => Func::Ln,
            "log" => Func::Log,
            "sin" => Func::Sin,
            "cos" => Func::Cos,
            "tan" => Func::Tan,
            "asin" => Func::Asin,
            "acos" => Func::Acos,
            "atan" => Func::Atan,
            "min" => Func::Min,
            "max" => Func::Max,
            "round" => Func::Round,
            _ => return None,
        })
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Func::Sqrt => "sqrt",
            Func::Abs => "abs",
            Func::Exp => "exp",
            Func::Ln => "ln",
            Func::Log => "log",
            Func::Sin => "sin",
            Func::Cos => "cos",
            Func::Tan => "tan",
            Func::Asin => "asin",
            Func::Acos => "acos",
            Func::Atan => "atan",
            Func::Min => "min",
            Func::Max => "max",
            Func::Round => "round",
        }
    }
}

/// A binary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

/// An expression tree.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Expr {
    Num(f64),
    Name(String),
    Neg(Box<Expr>),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    Call(Func, Vec<Expr>),
}

/// Parses all of `tokens` as one expression. `end_pos` is the 0-based character position just
/// after the tokens, for "expected a value" at the end.
pub(crate) fn parse(tokens: &[Token], end_pos: usize) -> Result<Expr, CalcError> {
    let mut parser = Parser { toks: tokens, i: 0, depth: 0, nodes: 0, end_pos };
    let expr = parser.expr()?;
    if let Some(token) = tokens.get(parser.i) {
        return Err(CalcError::syntax(token.pos, format!("unexpected {}", describe(&token.tok))));
    }
    Ok(expr)
}

type R<T> = Result<T, CalcError>;

struct Parser<'a> {
    toks: &'a [Token],
    i: usize,
    depth: usize,
    nodes: usize,
    end_pos: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.i).map(|t| &t.tok)
    }

    fn pos(&self) -> usize {
        self.toks.get(self.i).map_or(self.end_pos, |t| t.pos)
    }

    fn bump(&mut self) {
        self.i = self.i.saturating_add(1);
    }

    fn node(&mut self, expr: Expr) -> R<Expr> {
        self.nodes = self.nodes.saturating_add(1);
        if self.nodes > MAX_NODES { Err(CalcError::TooComplex) } else { Ok(expr) }
    }

    fn bin(&mut self, op: BinOp, left: Expr, right: Expr) -> R<Expr> {
        self.node(Expr::Bin(op, Box::new(left), Box::new(right)))
    }

    /// Runs `f` one nesting level deeper.
    fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> R<T>) -> R<T> {
        self.depth = self.depth.saturating_add(1);
        if self.depth > MAX_DEPTH {
            return Err(CalcError::TooComplex);
        }
        let result = f(self);
        self.depth = self.depth.saturating_sub(1);
        result
    }

    fn expr(&mut self) -> R<Expr> {
        let mut left = self.term()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Plus) => BinOp::Add,
                Some(Tok::Minus) => BinOp::Sub,
                _ => return Ok(left),
            };
            self.bump();
            let right = self.term()?;
            left = self.bin(op, left, right)?;
        }
    }

    fn term(&mut self) -> R<Expr> {
        let mut left = self.unary()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Star) => BinOp::Mul,
                Some(Tok::Slash) => BinOp::Div,
                _ => return Ok(left),
            };
            self.bump();
            let right = self.unary()?;
            left = self.bin(op, left, right)?;
        }
    }

    fn unary(&mut self) -> R<Expr> {
        match self.peek() {
            Some(Tok::Minus) => {
                self.bump();
                let inner = self.nested(Self::unary)?;
                self.node(Expr::Neg(Box::new(inner)))
            }
            Some(Tok::Plus) => {
                self.bump();
                self.nested(Self::unary)
            }
            _ => self.implicit(),
        }
    }

    fn implicit(&mut self) -> R<Expr> {
        let mut left = self.power()?;
        while matches!(self.peek(), Some(Tok::Ident(_) | Tok::LParen | Tok::Sqrt)) {
            let right = self.power()?;
            left = self.bin(BinOp::Mul, left, right)?;
        }
        Ok(left)
    }

    fn power(&mut self) -> R<Expr> {
        let base = self.postfix()?;
        if matches!(self.peek(), Some(Tok::Caret)) {
            self.bump();
            let exponent = self.nested(Self::exponent)?;
            return self.bin(BinOp::Pow, base, exponent);
        }
        Ok(base)
    }

    fn exponent(&mut self) -> R<Expr> {
        match self.peek() {
            Some(Tok::Minus) => {
                self.bump();
                let inner = self.nested(Self::exponent)?;
                self.node(Expr::Neg(Box::new(inner)))
            }
            Some(Tok::Plus) => {
                self.bump();
                self.nested(Self::exponent)
            }
            _ => self.power(),
        }
    }

    fn postfix(&mut self) -> R<Expr> {
        let mut expr = self.primary()?;
        while let Some(&Tok::Sup(n)) = self.peek() {
            self.bump();
            let exponent = self.node(Expr::Num(f64::from(n)))?;
            expr = self.bin(BinOp::Pow, expr, exponent)?;
        }
        Ok(expr)
    }

    fn primary(&mut self) -> R<Expr> {
        let pos = self.pos();
        let toks = self.toks;
        let Some(token) = toks.get(self.i) else {
            return Err(CalcError::syntax(pos, "expected a value"));
        };
        match &token.tok {
            Tok::Num(v) => {
                self.bump();
                self.node(Expr::Num(*v))
            }
            Tok::Ident(name) => {
                self.bump();
                // A function name is a call only when `(` follows, so `min` alone is minutes.
                let func = Func::from_name(name).filter(|_| matches!(self.peek(), Some(Tok::LParen)));
                let Some(func) = func else {
                    return self.node(Expr::Name(name.clone()));
                };
                self.bump();
                let args = self.nested(Self::args)?;
                self.node(Expr::Call(func, args))
            }
            Tok::LParen => {
                self.bump();
                let inner = self.nested(Self::expr)?;
                if !matches!(self.peek(), Some(Tok::RParen)) {
                    return Err(CalcError::syntax(self.pos(), "missing )"));
                }
                self.bump();
                Ok(inner)
            }
            Tok::Sqrt => {
                self.bump();
                let arg = self.nested(Self::postfix)?;
                self.node(Expr::Call(Func::Sqrt, vec![arg]))
            }
            other => Err(CalcError::syntax(pos, format!("expected a value, found {}", describe(other)))),
        }
    }

    /// Arguments after `(` up to and including `)`.
    fn args(&mut self) -> R<Vec<Expr>> {
        let mut args = Vec::new();
        if matches!(self.peek(), Some(Tok::RParen)) {
            self.bump();
            return Ok(args);
        }
        loop {
            args.push(self.expr()?);
            match self.peek() {
                Some(Tok::Sep) => self.bump(),
                Some(Tok::RParen) => {
                    self.bump();
                    return Ok(args);
                }
                _ => return Err(CalcError::syntax(self.pos(), "expected an argument separator or )")),
            }
        }
    }
}
