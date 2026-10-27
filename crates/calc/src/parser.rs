//! Expression parser (precedence climbing over [`Tok`]s).
//!
//! Grammar, loosest first:
//!
//! ```text
//! full     = expr (".." expr)?                   a range, in a definition, brackets or arguments
//! expr     = term (("+" | "-") term)*
//! term     = unary (("*" | "/") unary)*          explicit, left to right
//! unary    = ("-" | "+") unary | implicit
//! implicit = power power*                        juxtaposition; the right operand must not
//!                                                start with a number
//! power    = postfix ("^" exponent)?             right to left
//! exponent = ("-" | "+") exponent | power
//! postfix  = primary ("¹" | "²" | "³" | "[" full "]")*
//! primary  = number | name | function "(" args ")" | "(" full ")" | "[" full (";" full)* "]"
//!          | "√" postfix | nary
//! nary     = ("∑" | "∏") ("_" lower)? ("^" postfix)? "▒"? term      the operand spans · and /
//! lower    = "(" name "=" full ")" | name
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
    Sum,
    Mean,
    Len,
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
            "sum" => Func::Sum,
            "mean" => Func::Mean,
            "len" => Func::Len,
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
            Func::Sum => "sum",
            Func::Mean => "mean",
            Func::Len => "len",
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
    /// A unit written after a number (`4.5 m`, `12 kN/m`): always the unit, even where a
    /// parameter has the same name (ISO 80000: a quantity is a number times a unit).
    Unit(String),
    Neg(Box<Expr>),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    Call(Func, Vec<Expr>),
    /// `[a; b; c]`.
    Vector(Vec<Expr>),
    /// `v[i]`, 1-based.
    Index(Box<Expr>, Box<Expr>),
    /// `a..b`: whole steps from `a` towards `b`.
    Range(Box<Expr>, Box<Expr>),
    /// `∑`/`∏` over `var` from `from` to `to`, over the values of `var` (no limits), or over the
    /// elements of the operand (no variable).
    Nary(Nary),
}

/// An n-ary sum or product.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Nary {
    /// `∑` or `∏`.
    pub op: char,
    /// The bound variable.
    pub var: Option<String>,
    /// The lower limit, `i = from`.
    pub from: Option<Box<Expr>>,
    /// The upper limit.
    pub to: Option<Box<Expr>>,
    /// The operand.
    pub body: Box<Expr>,
}

/// Parses all of `tokens` as one expression. `end_pos` is the 0-based character position just
/// after the tokens, for "expected a value" at the end.
pub(crate) fn parse(tokens: &[Token], end_pos: usize) -> Result<Expr, CalcError> {
    let mut parser = Parser { toks: tokens, i: 0, depth: 0, nodes: 0, end_pos };
    let expr = parser.full()?;
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

    fn expect(&mut self, tok: Tok, what: &str) -> R<()> {
        if self.peek() == Some(&tok) {
            self.bump();
            Ok(())
        } else {
            Err(CalcError::syntax(self.pos(), format!("missing {what}")))
        }
    }

    fn full(&mut self) -> R<Expr> {
        let from = self.expr()?;
        if !matches!(self.peek(), Some(Tok::Range)) {
            return Ok(from);
        }
        self.bump();
        let to = self.expr()?;
        self.node(Expr::Range(Box::new(from), Box::new(to)))
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
        // A number followed by units: the units are read as such (`4.5 m`, `9.81 m/s²`).
        if matches!(left, Expr::Num(_)) && self.unit_at(self.i) {
            let unit = self.unit_chain()?;
            left = self.bin(BinOp::Mul, left, unit)?;
        }
        while matches!(self.peek(), Some(Tok::Ident(_) | Tok::LParen | Tok::Sqrt | Tok::Nary(_))) {
            let right = self.power()?;
            left = self.bin(BinOp::Mul, left, right)?;
        }
        Ok(left)
    }

    /// True when token `i` is a unit name (and not a function call).
    fn unit_at(&self, i: usize) -> bool {
        match self.toks.get(i).map(|t| &t.tok) {
            Some(Tok::Ident(name)) => {
                crate::units::unit(name).is_some() && !matches!(self.toks.get(i + 1).map(|t| &t.tok), Some(Tok::LParen) | Some(Tok::LBracket))
            }
            _ => false,
        }
    }

    /// Units multiplied, divided and raised to powers: `kN·m`, `m/s²`, `kg m^-3`. Stops at
    /// anything that isn't a unit (`2 m·a` is two metres times `a`).
    fn unit_chain(&mut self) -> R<Expr> {
        let mut e = self.unit_factor()?;
        loop {
            match self.peek() {
                Some(Tok::Star) if self.unit_at(self.i + 1) => {
                    self.bump();
                    let f = self.unit_factor()?;
                    e = self.bin(BinOp::Mul, e, f)?;
                }
                Some(Tok::Slash) if self.unit_at(self.i + 1) => {
                    self.bump();
                    let f = self.unit_factor()?;
                    e = self.bin(BinOp::Div, e, f)?;
                }
                Some(Tok::Ident(_)) if self.unit_at(self.i) => {
                    let f = self.unit_factor()?;
                    e = self.bin(BinOp::Mul, e, f)?;
                }
                _ => return Ok(e),
            }
        }
    }

    /// One unit with its power: `m`, `m²`, `s^-2`.
    fn unit_factor(&mut self) -> R<Expr> {
        let Some(Tok::Ident(name)) = self.peek().cloned() else {
            return Err(CalcError::syntax(self.pos(), "expected a unit"));
        };
        self.bump();
        let mut e = self.node(Expr::Unit(name))?;
        loop {
            match self.peek() {
                Some(&Tok::Sup(n)) => {
                    self.bump();
                    let exponent = self.node(Expr::Num(f64::from(n)))?;
                    e = self.bin(BinOp::Pow, e, exponent)?;
                }
                Some(Tok::Caret) => {
                    self.bump();
                    let exponent = self.nested(Self::exponent)?;
                    e = self.bin(BinOp::Pow, e, exponent)?;
                }
                _ => return Ok(e),
            }
        }
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
        loop {
            match self.peek() {
                Some(&Tok::Sup(n)) => {
                    self.bump();
                    let exponent = self.node(Expr::Num(f64::from(n)))?;
                    expr = self.bin(BinOp::Pow, expr, exponent)?;
                }
                Some(Tok::LBracket) => {
                    self.bump();
                    let index = self.nested(Self::full)?;
                    self.expect(Tok::RBracket, "]")?;
                    expr = self.node(Expr::Index(Box::new(expr), Box::new(index)))?;
                }
                _ => return Ok(expr),
            }
        }
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
                let inner = self.nested(Self::full)?;
                self.expect(Tok::RParen, ")")?;
                Ok(inner)
            }
            Tok::LBracket => {
                self.bump();
                let items = self.nested(Self::items)?;
                self.node(Expr::Vector(items))
            }
            &Tok::Nary(op) => {
                self.bump();
                self.nested(|p| p.nary(op))
            }
            Tok::Sqrt => {
                self.bump();
                let arg = self.nested(Self::postfix)?;
                self.node(Expr::Call(Func::Sqrt, vec![arg]))
            }
            other => Err(CalcError::syntax(pos, format!("expected a value, found {}", describe(other)))),
        }
    }

    /// Vector elements after `[` up to and including `]`.
    fn items(&mut self) -> R<Vec<Expr>> {
        let mut items = Vec::new();
        loop {
            items.push(self.full()?);
            match self.peek() {
                Some(Tok::Sep) => self.bump(),
                Some(Tok::RBracket) => {
                    self.bump();
                    return Ok(items);
                }
                _ => return Err(CalcError::syntax(self.pos(), "expected ; or ]")),
            }
        }
    }

    /// An n-ary operator after its sign: limits, an optional `▒` and the operand.
    fn nary(&mut self, op: char) -> R<Expr> {
        let (mut var, mut from) = (None, None);
        if matches!(self.peek(), Some(Tok::Under)) {
            self.bump();
            let pos = self.pos();
            let toks = self.toks;
            match (toks.get(self.i).map(|t| &t.tok), toks.get(self.i + 1).map(|t| &t.tok)) {
                (Some(Tok::LParen), Some(Tok::Ident(name))) if matches!(toks.get(self.i + 2).map(|t| &t.tok), Some(Tok::Equals)) => {
                    var = Some(name.clone());
                    self.i += 3;
                    from = Some(Box::new(self.full()?));
                    self.expect(Tok::RParen, ")")?;
                }
                (Some(Tok::Ident(name)), _) => {
                    var = Some(name.clone());
                    self.bump();
                }
                _ => return Err(CalcError::syntax(pos, "the lower limit is a variable, like i or (i=1)")),
            }
        }
        let mut to = None;
        if matches!(self.peek(), Some(Tok::Caret)) {
            self.bump();
            to = Some(Box::new(self.postfix()?));
        }
        if to.is_some() != from.is_some() {
            return Err(CalcError::syntax(self.pos(), "give both limits, like (i=1)^n, or a range variable alone"));
        }
        if matches!(self.peek(), Some(Tok::Operand)) {
            self.bump();
        }
        let body = Box::new(self.term()?);
        self.node(Expr::Nary(Nary { op, var, from, to, body }))
    }

    /// Arguments after `(` up to and including `)`.
    fn args(&mut self) -> R<Vec<Expr>> {
        let mut args = Vec::new();
        if matches!(self.peek(), Some(Tok::RParen)) {
            self.bump();
            return Ok(args);
        }
        loop {
            args.push(self.full()?);
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
