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
//! primary  = number | name | function "(" args ")" | user "(" args ")" | "(" full ")" | "[" full (";" full)* "]"
//!          | "(" full "¦" full ")"               a binomial coefficient, n over k
//!          | "√" postfix | nary
//! nary     = ("∑" | "∏") ("_" lower)? ("^" postfix)? "▒"? term      the operand spans · and /
//!          | "∫" "_" limit "^" limit "▒"? term (("+" | "-") term)*   up to the differential `dx`;
//!                                                a limit may be `∞` or `-∞`
//! lower    = "(" name "=" full ")" | name
//! ```
//!
//! The structures of the Equation tab are read as they are written (`structures.rs`): `|x|`,
//! `‖v‖`, `⌊x⌋`, `⌈x⌉`, `{…}`, `√(n&x)`, `log_b⁡x`, `lim┬(x→a)⁡…`, `■(a&b@c&d)` and the cases
//! `{█(a,&x<0@b,&x≥0)┤`.
//!
//! Implicit multiplication binds tighter than `/` so `12 kN/m` is `(12·kN)/m` (see the crate
//! docs). Nesting is capped by [`MAX_DEPTH`] and size by [`MAX_NODES`].

use crate::lexer::{Tok, Token, describe};
use crate::{CalcError, MAX_DEPTH, MAX_NODES};

/// A built-in function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Func {
    Measure,
    RelativeMeasure,
    Val,
    Unc,
    Relunc,
    Sqrt,
    Cbrt,
    Root,
    Abs,
    Sign,
    Mod,
    Exp,
    Ln,
    Log,
    Log2,
    Sin,
    Cos,
    Tan,
    Csc,
    Sec,
    Cot,
    Asin,
    Acos,
    Atan,
    Acsc,
    Asec,
    Acot,
    Atan2,
    Sinh,
    Cosh,
    Tanh,
    Csch,
    Sech,
    Coth,
    Asinh,
    Acosh,
    Atanh,
    Acsch,
    Asech,
    Acoth,
    /// `‖v‖`: the length of a vector (of a number, its absolute value).
    Norm,
    /// A matrix as written, `■(a&b@c&d)`: the number of columns, then the cells row by row.
    Matrix,
    /// `|■(a&b@c&d)|`: the determinant; arguments as [`Func::Matrix`].
    Det,
    /// Cases, `{█(a,&x<0@b,&x≥0)┤`: each value followed by its condition.
    Cases,
    /// The comparisons of a case's condition (1 when it holds, else 0) and several at once
    /// (`0≤x≤1`).
    Lt,
    Le,
    Gt,
    Ge,
    Ne,
    Eq,
    All,
    Gcd,
    Lcm,
    Fact,
    Binom,
    /// [`Func::Binom`] written as a coefficient, `(n¦k)`: the same value, shown as written.
    Choose,
    Min,
    Max,
    Round,
    Floor,
    Ceil,
    Trunc,
    Sum,
    Mean,
    Median,
    Var,
    Varp,
    Stdev,
    Stdevp,
    Slope,
    Intercept,
    R2,
    Len,
}

impl Func {
    pub(crate) fn from_name(name: &str) -> Option<Func> {
        Some(match name {
            "measure" => Func::Measure,
            "relativeMeasure" => Func::RelativeMeasure,
            "val" => Func::Val,
            "unc" => Func::Unc,
            "relunc" => Func::Relunc,
            "sqrt" => Func::Sqrt,
            "cbrt" => Func::Cbrt,
            "root" => Func::Root,
            "abs" => Func::Abs,
            "sign" => Func::Sign,
            "mod" => Func::Mod,
            "exp" => Func::Exp,
            "ln" => Func::Ln,
            "log" => Func::Log,
            "log2" => Func::Log2,
            // The Italian names too: `sen`, `tg`, `cotg`, `cosec`.
            "sin" | "sen" => Func::Sin,
            "cos" => Func::Cos,
            "tan" | "tg" => Func::Tan,
            "csc" | "cosec" => Func::Csc,
            "sec" => Func::Sec,
            "cot" | "cotg" | "ctg" => Func::Cot,
            "asin" | "arcsin" | "arcsen" => Func::Asin,
            "acos" | "arccos" => Func::Acos,
            "atan" | "arctan" | "arctg" => Func::Atan,
            "acsc" | "arccsc" | "arccosec" => Func::Acsc,
            "asec" | "arcsec" => Func::Asec,
            "acot" | "arccot" | "arccotg" | "arcctg" => Func::Acot,
            "atan2" => Func::Atan2,
            "sinh" | "senh" => Func::Sinh,
            "cosh" => Func::Cosh,
            "tanh" | "tgh" => Func::Tanh,
            "csch" => Func::Csch,
            "sech" => Func::Sech,
            "coth" => Func::Coth,
            "asinh" | "arsinh" | "arcsinh" => Func::Asinh,
            "acosh" | "arcosh" | "arccosh" => Func::Acosh,
            "atanh" | "artanh" | "arctanh" => Func::Atanh,
            "acsch" | "arcsch" | "arccsch" => Func::Acsch,
            "asech" | "arsech" | "arcsech" => Func::Asech,
            "acoth" | "arcoth" | "arccoth" => Func::Acoth,
            "norm" => Func::Norm,
            "det" => Func::Det,
            "gcd" | "mcd" => Func::Gcd,
            "lcm" | "mcm" => Func::Lcm,
            "fact" => Func::Fact,
            "binom" => Func::Binom,
            "min" => Func::Min,
            "max" => Func::Max,
            "round" => Func::Round,
            "floor" => Func::Floor,
            "ceil" => Func::Ceil,
            "trunc" => Func::Trunc,
            "sum" => Func::Sum,
            "mean" => Func::Mean,
            "median" => Func::Median,
            "var" => Func::Var,
            "varp" => Func::Varp,
            "stdev" | "std" => Func::Stdev,
            "stdevp" => Func::Stdevp,
            "slope" => Func::Slope,
            "intercept" => Func::Intercept,
            "r2" => Func::R2,
            "len" => Func::Len,
            _ => return None,
        })
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Func::Measure => "measure",
            Func::RelativeMeasure => "relativeMeasure",
            Func::Val => "val",
            Func::Unc => "unc",
            Func::Relunc => "relunc",
            Func::Sqrt => "sqrt",
            Func::Cbrt => "cbrt",
            Func::Root => "root",
            Func::Abs => "abs",
            Func::Sign => "sign",
            Func::Mod => "mod",
            Func::Exp => "exp",
            Func::Ln => "ln",
            Func::Log => "log",
            Func::Log2 => "log2",
            Func::Sin => "sin",
            Func::Cos => "cos",
            Func::Tan => "tan",
            Func::Csc => "csc",
            Func::Sec => "sec",
            Func::Cot => "cot",
            // An inverse is named with an `a` before the function (`atan`), as the owner wants it.
            Func::Asin => "asin",
            Func::Acos => "acos",
            Func::Atan => "atan",
            Func::Acsc => "acsc",
            Func::Asec => "asec",
            Func::Acot => "acot",
            Func::Atan2 => "atan2",
            Func::Sinh => "sinh",
            Func::Cosh => "cosh",
            Func::Tanh => "tanh",
            Func::Csch => "csch",
            Func::Sech => "sech",
            Func::Coth => "coth",
            Func::Asinh => "asinh",
            Func::Acosh => "acosh",
            Func::Atanh => "atanh",
            Func::Acsch => "acsch",
            Func::Asech => "asech",
            Func::Acoth => "acoth",
            Func::Norm => "norm",
            Func::Matrix => "matrix",
            Func::Det => "det",
            Func::Cases => "cases",
            Func::Lt => "<",
            Func::Le => "≤",
            Func::Gt => ">",
            Func::Ge => "≥",
            Func::Ne => "≠",
            Func::Eq => "=",
            Func::All => "and",
            Func::Gcd => "gcd",
            Func::Lcm => "lcm",
            Func::Fact => "fact",
            Func::Binom | Func::Choose => "binom",
            Func::Min => "min",
            Func::Max => "max",
            Func::Round => "round",
            Func::Floor => "floor",
            Func::Ceil => "ceil",
            Func::Trunc => "trunc",
            Func::Sum => "sum",
            Func::Mean => "mean",
            Func::Median => "median",
            Func::Var => "var",
            Func::Varp => "varp",
            Func::Stdev => "stdev",
            Func::Stdevp => "stdevp",
            Func::Slope => "slope",
            Func::Intercept => "intercept",
            Func::R2 => "r2",
            Func::Len => "len",
        }
    }

    /// The inverse of a trigonometric or hyperbolic function (`sin⁻¹` is `asin`).
    pub(crate) fn inverse(self) -> Option<Func> {
        Some(match self {
            Func::Sin => Func::Asin,
            Func::Cos => Func::Acos,
            Func::Tan => Func::Atan,
            Func::Csc => Func::Acsc,
            Func::Sec => Func::Asec,
            Func::Cot => Func::Acot,
            Func::Sinh => Func::Asinh,
            Func::Cosh => Func::Acosh,
            Func::Tanh => Func::Atanh,
            Func::Csch => Func::Acsch,
            Func::Sech => Func::Asech,
            Func::Coth => Func::Acoth,
            _ => return None,
        })
    }

    /// The function an inverse undoes, when `self` is one (`sin` for `sin⁻¹`).
    pub(crate) fn inverse_of(self) -> Option<Func> {
        Some(match self {
            Func::Asin => Func::Sin,
            Func::Acos => Func::Cos,
            Func::Atan => Func::Tan,
            Func::Acsc => Func::Csc,
            Func::Asec => Func::Sec,
            Func::Acot => Func::Cot,
            Func::Asinh => Func::Sinh,
            Func::Acosh => Func::Cosh,
            Func::Atanh => Func::Tanh,
            Func::Acsch => Func::Csch,
            Func::Asech => Func::Sech,
            Func::Acoth => Func::Coth,
            _ => return None,
        })
    }

    /// An inverse trigonometric function: its result is an angle.
    pub(crate) fn gives_angle(self) -> bool {
        matches!(self, Func::Asin | Func::Acos | Func::Atan | Func::Acsc | Func::Asec | Func::Acot | Func::Atan2)
    }

    pub(crate) fn is_rounding(self) -> bool {
        matches!(self, Func::Round | Func::Floor | Func::Ceil | Func::Trunc)
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
    /// A call of a user function defined in the worksheet (`f(3)`, `A(2 m, 3 m)`).
    Apply(String, Vec<Expr>),
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
    parse_env(tokens, end_pos, &|_| false, &|_| false)
}

/// [`parse`], where a name `is_func` accepts followed by `(` is a call of that user function
/// (`f(3)`), not a product (`a(b + c)`); also distinguishing defined vector parameters (`F = [1; 2; 3] kN` in `2 F =`).
pub(crate) fn parse_env(tokens: &[Token], end_pos: usize, is_func: &dyn Fn(&str) -> bool, is_vec: &dyn Fn(&str) -> bool) -> Result<Expr, CalcError> {
    parse_names(tokens, end_pos, is_func, is_vec, &|_| false).map(|(expr, _)| expr)
}

/// [`parse_env`], also knowing the defined parameters (`is_param`): after a unit and `·`, a
/// parameter is the parameter (`2 m·s` with `s = 3 m` is 2 m times s), and `0.0002/K` is per
/// kelvin unless `K` is a parameter.
pub(crate) fn parse_scope(
    tokens: &[Token],
    end_pos: usize,
    is_func: &dyn Fn(&str) -> bool,
    is_vec: &dyn Fn(&str) -> bool,
    is_param: &dyn Fn(&str) -> bool,
) -> Result<Expr, CalcError> {
    parse_names(tokens, end_pos, is_func, is_vec, is_param).map(|(expr, _)| expr)
}

/// [`parse_env`], with the indices of the tokens read as names (parameters, constants, units
/// spelled alone: `h` in `h·r`, not the unit `h` in `3 h`).
pub(crate) fn parse_names(
    tokens: &[Token],
    end_pos: usize,
    is_func: &dyn Fn(&str) -> bool,
    is_vec: &dyn Fn(&str) -> bool,
    is_param: &dyn Fn(&str) -> bool,
) -> Result<(Expr, Vec<usize>), CalcError> {
    let mut parser = Parser { toks: tokens, i: 0, depth: 0, nodes: 0, end_pos, is_func, is_vec, is_param, unit_group: 0, names: Vec::new() };
    let expr = parser.full()?;
    if let Some(token) = tokens.get(parser.i) {
        return Err(CalcError::syntax(token.pos, format!("unexpected {}", describe(&token.tok))));
    }
    Ok((expr, parser.names))
}

type R<T> = Result<T, CalcError>;

/// True for one value as a vector lists it: a number, a name, either with a sign, a unit or an
/// index (`5`, `x`, `−3`, `5 kN`, `F[2]`), not an expression to work out (`2 + 4`).
fn one_value(expr: &Expr) -> bool {
    match expr {
        Expr::Num(_) | Expr::Name(_) => true,
        Expr::Neg(inner) => one_value(inner),
        Expr::Index(base, _) => one_value(base),
        Expr::Bin(BinOp::Mul | BinOp::Div, value, unit) => one_value(value) && units_only(unit),
        _ => false,
    }
}

/// True when `expr` is made of units alone (`kN`, `m/s²`).
fn units_only(expr: &Expr) -> bool {
    match expr {
        Expr::Unit(_) => true,
        Expr::Bin(BinOp::Mul | BinOp::Div, a, b) => units_only(a) && units_only(b),
        Expr::Bin(BinOp::Pow, base, exponent) => units_only(base) && crate::format::is_numeric(exponent),
        _ => false,
    }
}

pub(crate) struct Parser<'a> {
    pub(crate) toks: &'a [Token],
    pub(crate) i: usize,
    depth: usize,
    pub(crate) nodes: usize,
    end_pos: usize,
    is_func: &'a dyn Fn(&str) -> bool,
    is_vec: &'a dyn Fn(&str) -> bool,
    /// A defined parameter ([`parse_scope`]).
    is_param: &'a dyn Fn(&str) -> bool,
    /// How deep in brackets of units (`(kg·m)`) the parser is: there every name is a unit.
    unit_group: usize,
    /// The indices of the tokens read as names ([`parse_names`]).
    names: Vec<usize>,
}

impl Parser<'_> {
    pub(crate) fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.i).map(|t| &t.tok)
    }

    pub(crate) fn pos(&self) -> usize {
        self.toks.get(self.i).map_or(self.end_pos, |t| t.pos)
    }

    pub(crate) fn bump(&mut self) {
        self.i = self.i.saturating_add(1);
    }

    pub(crate) fn node(&mut self, expr: Expr) -> R<Expr> {
        self.nodes = self.nodes.saturating_add(1);
        if self.nodes > MAX_NODES { Err(CalcError::TooComplex) } else { Ok(expr) }
    }

    fn bin(&mut self, op: BinOp, left: Expr, right: Expr) -> R<Expr> {
        self.node(Expr::Bin(op, Box::new(left), Box::new(right)))
    }

    /// Runs `f` one nesting level deeper.
    pub(crate) fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> R<T>) -> R<T> {
        self.depth = self.depth.saturating_add(1);
        if self.depth > MAX_DEPTH {
            return Err(CalcError::TooComplex);
        }
        let result = f(self);
        self.depth = self.depth.saturating_sub(1);
        result
    }

    pub(crate) fn expect(&mut self, tok: Tok, what: &str) -> R<()> {
        if self.peek() == Some(&tok) {
            self.bump();
            Ok(())
        } else {
            Err(CalcError::syntax(self.pos(), format!("missing {what}")))
        }
    }

    pub(crate) fn full(&mut self) -> R<Expr> {
        let from = self.expr()?;
        if !matches!(self.peek(), Some(Tok::Range)) {
            return Ok(from);
        }
        self.bump();
        let to = self.expr()?;
        self.node(Expr::Range(Box::new(from), Box::new(to)))
    }

    pub(crate) fn expr(&mut self) -> R<Expr> {
        let mut left = self.term()?;
        loop {
            if matches!(self.peek(), Some(Tok::PlusMinus)) {
                self.bump();
                let mut right = self.term()?;
                let mut func = Func::Measure;
                // A trailing unit belongs to both sides: `12.4 ± 0.1 cm`.
                if let Expr::Bin(BinOp::Mul, n, units) = &right
                    && crate::format::is_units(units)
                {
                    let mut shared = (**units).clone();
                    if let Some(u) = without_leading_percent(&shared) {
                        right = self.bin(BinOp::Mul, (**n).clone(), Expr::Unit("%".into()))?;
                        shared = u;
                        func = Func::RelativeMeasure;
                    }
                    if crate::format::is_numeric(&left) {
                        left = self.bin(BinOp::Mul, left, shared)?;
                    }
                }
                if matches!(&right, Expr::Bin(BinOp::Mul, _, u) if matches!(&**u, Expr::Unit(name) if name == "%")) {
                    func = Func::RelativeMeasure;
                }
                left = self.node(Expr::Call(func, vec![left, right]))?;
                continue;
            }
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

    pub(crate) fn term(&mut self) -> R<Expr> {
        let mut left = self.unary()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Star) => BinOp::Mul,
                Some(Tok::Slash) => BinOp::Div,
                _ => return Ok(left),
            };
            self.bump();
            // A written quantity divided by units: `(12 kN)/m` (a built-up fraction's linear
            // text) is per metre, even with a parameter named `m`.
            let right = if op == BinOp::Div && (crate::format::is_quantity(&left) || crate::format::is_units(&left)) && self.unit_at(self.i) {
                self.unit_chain()?
            } else {
                self.unary()?
            };
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
        // So are units after numbers alone: `[4; 5; 6] m`, `10^3 m`.
        if crate::format::is_numeric(&left) && self.unit_at(self.i) {
            let unit = self.unit_chain()?;
            left = self.bin(BinOp::Mul, left, unit)?;
        } else if crate::format::is_numeric(&left)
            && matches!(self.peek(), Some(Tok::Num(v)) if *v == 1.0)
            && matches!(self.toks.get(self.i + 1).map(|t| &t.tok), Some(Tok::Slash))
            && self.unit_at(self.i + 2)
        {
            // A reciprocal unit written after a number: `0.0002 1/K`.
            let one = self.node(Expr::Num(1.0))?;
            self.bump();
            self.bump();
            let f = self.unit_factor()?;
            let first = self.bin(BinOp::Div, one, f)?;
            let unit = self.unit_chain_from(first)?;
            left = self.bin(BinOp::Mul, left, unit)?;
        }
        while matches!(self.peek(), Some(Tok::Ident(_) | Tok::LParen | Tok::Sqrt | Tok::Nary(_))) || self.opens_structure() {
            let right = self.power()?;
            left = self.bin(BinOp::Mul, left, right)?;
        }
        Ok(left)
    }

    /// True when token `i` starts a unit factor (a unit name, or a parenthesised compound unit
    /// like `(kg·°C)` or `(m²)`).
    pub(crate) fn unit_at(&self, i: usize) -> bool {
        match self.toks.get(i).map(|t| &t.tok) {
            Some(Tok::Ident(name)) => {
                let next = self.toks.get(i + 1).map(|t| &t.tok);
                let vec_alone = (self.is_vec)(name) && !matches!(next, Some(Tok::Slash | Tok::Caret | Tok::Sup(_)));
                crate::units::unit(name).is_some() && !vec_alone && !matches!(next, Some(Tok::LParen) | Some(Tok::LBracket))
            }
            Some(Tok::LParen) => self.paren_unit_at(i),
            _ => false,
        }
    }

    /// True when `(` at `start` encloses only units, `*`, `/`, powers and nested parentheses, with
    /// at least one unit name (`(kg·°C)`, `(m²)`).
    fn paren_unit_at(&self, start: usize) -> bool {
        let mut depth = 0usize;
        let mut has_unit = false;
        let mut i = start;
        while let Some(t) = self.toks.get(i) {
            match &t.tok {
                Tok::LParen => depth += 1,
                Tok::RParen => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return has_unit;
                    }
                }
                Tok::Ident(name) => {
                    let next = self.toks.get(i + 1).map(|n| &n.tok);
                    if crate::units::unit(name).is_none() || matches!(next, Some(Tok::LParen | Tok::LBracket)) {
                        return false;
                    }
                    has_unit = true;
                }
                Tok::Star | Tok::Slash | Tok::Caret | Tok::Sup(_) => {}
                Tok::Minus | Tok::Plus if i > start && matches!(self.toks.get(i - 1).map(|p| &p.tok), Some(Tok::Caret)) => {}
                Tok::Num(_) if i > start && matches!(self.toks.get(i - 1).map(|p| &p.tok), Some(Tok::Caret | Tok::Minus | Tok::Plus)) => {}
                _ => return false,
            }
            i += 1;
        }
        false
    }

    /// Units multiplied, divided and raised to powers: `kN·m`, `m/s²`, `kg m^-3`. Stops at
    /// anything that isn't a unit (`2 m·a` is two metres times `a`).
    pub(crate) fn unit_chain(&mut self) -> R<Expr> {
        let e = self.unit_factor()?;
        self.unit_chain_from(e)
    }

    /// True when token `i` is a defined parameter that a unit chain `chain` stops before: after
    /// `·` or a space (`2 m·s` with `s = 3 m`), outside brackets of units (`(kN·m)` is always the
    /// unit). The unit stays the unit when it is raised or divided (`m·s^-1`, `m·s²`) or when it
    /// makes a named quantity with the units before it (`kN·m`, `kW·h`, `A·h`, `N·s`), whatever
    /// parameters `m`, `h` or `s` there are.
    fn param_at(&self, i: usize, chain: &Expr) -> bool {
        let Some(Tok::Ident(name)) = self.toks.get(i).map(|t| &t.tok) else { return false };
        if self.unit_group > 0 || !(self.is_param)(name) {
            return false;
        }
        if matches!(self.toks.get(i + 1).map(|t| &t.tok), Some(Tok::Sup(_) | Tok::Caret | Tok::Slash)) {
            return false;
        }
        let named = units_dim(chain)
            .zip(crate::units::unit(name).map(|u| u.dim))
            .and_then(|(a, b)| a.try_mul(b).ok())
            .is_some_and(crate::units::named_product);
        !named
    }

    /// [`Self::unit_chain`] after its first factor `e`.
    fn unit_chain_from(&mut self, mut e: Expr) -> R<Expr> {
        loop {
            match self.peek() {
                Some(Tok::Star) if self.unit_at(self.i + 1) && !self.param_at(self.i + 1, &e) => {
                    self.bump();
                    let f = self.unit_factor()?;
                    e = self.bin(BinOp::Mul, e, f)?;
                }
                Some(Tok::Slash) if self.unit_at(self.i + 1) => {
                    self.bump();
                    let f = self.unit_factor()?;
                    e = self.bin(BinOp::Div, e, f)?;
                }
                Some(Tok::Ident(_)) if self.unit_at(self.i) && !self.param_at(self.i, &e) => {
                    let f = self.unit_factor()?;
                    e = self.bin(BinOp::Mul, e, f)?;
                }
                _ => return Ok(e),
            }
        }
    }

    fn sup_node(&mut self, n: i32) -> R<Expr> {
        if n < 0 {
            let inner = self.node(Expr::Num(f64::from(-n)))?;
            self.node(Expr::Neg(Box::new(inner)))
        } else {
            self.node(Expr::Num(f64::from(n)))
        }
    }

    /// One unit (or parenthesised unit group) with its power: `m`, `m²`, `s^-2`, `s⁻²`, `(kg·°C)`.
    fn unit_factor(&mut self) -> R<Expr> {
        let mut e = if matches!(self.peek(), Some(Tok::LParen)) {
            self.bump();
            self.unit_group += 1;
            let inner = self.nested(Self::unit_chain);
            self.unit_group = self.unit_group.saturating_sub(1);
            let inner = inner?;
            self.expect(Tok::RParen, ")")?;
            inner
        } else {
            let Some(Tok::Ident(name)) = self.peek().cloned() else {
                return Err(CalcError::syntax(self.pos(), "expected a unit"));
            };
            self.bump();
            self.node(Expr::Unit(name))?
        };
        loop {
            match self.peek() {
                Some(&Tok::Sup(n)) => {
                    self.bump();
                    let exponent = self.sup_node(n)?;
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

    pub(crate) fn power(&mut self) -> R<Expr> {
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

    pub(crate) fn postfix(&mut self) -> R<Expr> {
        let mut expr = self.primary()?;
        loop {
            match self.peek() {
                Some(&Tok::Sup(n)) => {
                    self.bump();
                    let exponent = self.sup_node(n)?;
                    expr = self.bin(BinOp::Pow, expr, exponent)?;
                }
                Some(Tok::Bang) => {
                    self.bump();
                    expr = self.node(Expr::Call(Func::Fact, vec![expr]))?;
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
                if let Some(expr) = self.named_structure(name)? {
                    return Ok(expr);
                }
                // `sin⁻¹(x)`, `sin^(-1)⁡x`: the inverse function (`asin`); `sin²(x)`: `sin(x)²`.
                if let Some(func) = Func::from_name(name).filter(|f| f.inverse().is_some())
                    && let Some((power, after)) = self.function_power()
                    && matches!(toks.get(after).map(|t| &t.tok), Some(Tok::LParen | Tok::Operand))
                {
                    self.i = after;
                    let args = self.function_args()?;
                    return match power {
                        -1 => match func.inverse() {
                            Some(inverse) => self.node(Expr::Call(inverse, args)),
                            None => Err(CalcError::syntax(pos, format!("{name} has no inverse function"))),
                        },
                        1 => self.node(Expr::Call(func, args)),
                        n if n >= 2 => {
                            let call = self.node(Expr::Call(func, args))?;
                            let exponent = self.node(Expr::Num(f64::from(n)))?;
                            self.bin(BinOp::Pow, call, exponent)
                        }
                        _ => Err(CalcError::syntax(pos, format!("write the power after the brackets: ({name}(x))^{power}"))),
                    };
                }
                // `sin⁡x`, `sin⁡(x)` (Word's function application): the argument follows.
                if let Some(func) = Func::from_name(name)
                    && matches!(self.peek(), Some(Tok::Operand))
                {
                    let args = self.function_args()?;
                    return self.node(Expr::Call(func, args));
                }
                // A function name is a call only when `(` follows, so `min` alone is minutes.
                let func = Func::from_name(name).filter(|_| matches!(self.peek(), Some(Tok::LParen)));
                let Some(func) = func else {
                    // `f′(2)`: a derivative of the user function `f`.
                    let (base, _) = crate::calculus::split_primes(name);
                    if matches!(self.peek(), Some(Tok::LParen)) && (self.is_func)(base) {
                        self.bump();
                        let args = self.nested(Self::args)?;
                        return self.node(Expr::Apply(name.clone(), args));
                    }
                    self.names.push(self.i.saturating_sub(1));
                    return self.node(Expr::Name(name.clone()));
                };
                self.bump();
                let args = self.nested(Self::args)?;
                self.node(Expr::Call(func, args))
            }
            Tok::LParen => {
                self.bump();
                let inner = self.nested(Self::full)?;
                // `(n¦k)`: a binomial coefficient, as an equation writes it.
                if matches!(self.peek(), Some(Tok::Choose)) {
                    self.bump();
                    let lower = self.nested(Self::full)?;
                    self.expect(Tok::RParen, ")")?;
                    return self.node(Expr::Call(Func::Choose, vec![inner, lower]));
                }
                self.expect(Tok::RParen, ")")?;
                Ok(inner)
            }
            Tok::LBracket => {
                self.bump();
                let mut items = self.nested(Self::items)?;
                // Square brackets round an expression group like round ones (`[(2 + 3)·4 − 1]·2`),
                // and so do those round a vector (`[■(1@2)]`); one value alone is a vector of one
                // element (`[5]`, `[5 kN]`, `[x]`, `[−3]`).
                if let [only] = items.as_slice()
                    && !matches!(only, Expr::Range(..))
                    && !one_value(only)
                {
                    return Ok(items.swap_remove(0));
                }
                self.node(Expr::Vector(items))
            }
            &Tok::Nary(op) => {
                self.bump();
                self.nested(|p| p.nary(op))
            }
            Tok::Sqrt => {
                self.bump();
                if let Some(root) = self.root_with_degree()? {
                    return Ok(root);
                }
                let arg = self.nested(Self::postfix)?;
                self.node(Expr::Call(Func::Sqrt, vec![arg]))
            }
            Tok::LBrace | Tok::Bar { .. } | Tok::LFloor(_) | Tok::Root(_) | Tok::Matrix | Tok::Stack => self.nested(Self::structure),
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

    /// The `▒` before the operand of an n-ary operator, if any. An equation built from text
    /// whose operand follows a space (`∫_0^1 x^2 + 1 dx`, `∑_(i=1)^3 i²`) has an empty operand
    /// and reads `∫_0^1▒()x^2+1dx` in linear format: the operand is what follows.
    fn operand_mark(&mut self) {
        if matches!(self.peek(), Some(Tok::Operand)) {
            self.bump();
            if matches!((self.peek(), self.toks.get(self.i + 1).map(|t| &t.tok)), (Some(Tok::LParen), Some(Tok::RParen))) {
                self.i += 2;
            }
        }
    }

    /// An n-ary operator after its sign: limits, an optional `▒` and the operand.
    fn nary(&mut self, op: char) -> R<Expr> {
        if op == '∫' {
            return self.integral();
        }
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
        self.operand_mark();
        let body = Box::new(self.term()?);
        self.node(Expr::Nary(Nary { op, var, from, to, body }))
    }

    /// A definite integral after `∫`: `_a^b` (either order), an optional `▒` and the integrand up
    /// to its differential (`x² + 1 dx`, `〖x² dx〗`).
    fn integral(&mut self) -> R<Expr> {
        let (mut from, mut to) = (None, None);
        loop {
            match self.peek() {
                Some(Tok::Under) if from.is_none() => {
                    self.bump();
                    from = Some(Box::new(self.limit()?));
                }
                Some(Tok::Caret) if to.is_none() => {
                    self.bump();
                    to = Some(Box::new(self.limit()?));
                }
                _ => break,
            }
        }
        if from.is_none() || to.is_none() {
            return Err(CalcError::syntax(self.pos(), "give both limits of the integral, like ∫_0^1"));
        }
        self.operand_mark();
        let mut body: Option<Expr> = None;
        let mut negate = false;
        loop {
            let term = self.term()?;
            let term = if negate { self.node(Expr::Neg(Box::new(term)))? } else { term };
            let (term, var) = match crate::calculus::split_differential(term) {
                Ok((term, var)) => (term, Some(var)),
                Err(term) => (term, None),
            };
            let joined = match body.take() {
                None => term,
                Some(left) => self.bin(BinOp::Add, left, term)?,
            };
            if let Some(var) = var {
                return self.node(Expr::Nary(Nary { op: '∫', var: Some(var), from, to, body: Box::new(joined) }));
            }
            body = Some(joined);
            negate = match self.peek() {
                Some(Tok::Plus) => false,
                Some(Tok::Minus) => true,
                _ => return Err(CalcError::syntax(self.pos(), "end the integral with its variable, like dx")),
            };
            self.bump();
        }
    }

    /// A limit of an integral: a signed postfix (`0`, `-1`, `π`, `(a+b)`), a number with the unit
    /// written against it (`∫_0m^6m`: the linear format of a limit `0 m` typed in the equation,
    /// whose script runs to the next operator, so the unit takes no power here).
    pub(crate) fn limit(&mut self) -> R<Expr> {
        let e = match self.peek() {
            Some(Tok::Minus) => {
                self.bump();
                let inner = self.nested(Self::postfix)?;
                self.node(Expr::Neg(Box::new(inner)))?
            }
            Some(Tok::Plus) => {
                self.bump();
                self.nested(Self::postfix)?
            }
            _ => self.nested(Self::postfix)?,
        };
        if crate::format::is_numeric(&e)
            && self.unit_at(self.i)
            && let Some(Tok::Ident(name)) = self.peek().cloned()
        {
            self.bump();
            let unit = self.node(Expr::Unit(name))?;
            return self.bin(BinOp::Mul, e, unit);
        }
        Ok(e)
    }

    /// The integer power written on a function name before its argument: `⁻¹`, `²`, `^-1`,
    /// `^(-1)`, `^2`; with the index of the token after it. `None` when there is none.
    fn function_power(&self) -> Option<(i32, usize)> {
        let tok = |i: usize| self.toks.get(i).map(|t| &t.tok);
        let at = self.i;
        let int = |v: f64| (v.fract() == 0.0 && (1.0..=9.0).contains(&v)).then_some(v as i32);
        match tok(at)? {
            Tok::Sup(n) => Some((*n, at + 1)),
            Tok::Caret => match (tok(at + 1)?, tok(at + 2), tok(at + 3), tok(at + 4)) {
                (Tok::Num(v), ..) => Some((int(*v)?, at + 2)),
                (Tok::Minus, Some(Tok::Num(v)), ..) => Some((-int(*v)?, at + 3)),
                (Tok::LParen, Some(Tok::Num(v)), Some(Tok::RParen), _) => Some((int(*v)?, at + 4)),
                (Tok::LParen, Some(Tok::Minus), Some(Tok::Num(v)), Some(Tok::RParen)) => Some((-int(*v)?, at + 5)),
                _ => None,
            },
            _ => None,
        }
    }

    /// A function's arguments: `(a; b)`, or after Word's function application `⁡(a)` or `⁡x`
    /// (an operand up to its power: `sin⁡x²` is `sin(x²)`).
    pub(crate) fn function_args(&mut self) -> R<Vec<Expr>> {
        if matches!(self.peek(), Some(Tok::Operand)) {
            self.bump();
            if !matches!(self.peek(), Some(Tok::LParen)) {
                // A number takes what is written against it, as the linear format has it: its
                // unit (`sin⁡30°`) or the one factor it is the coefficient of (`cos⁡2x` is
                // `cos(2·x)`). A name is the whole operand: `sin⁡α L` is `sin(α)·L`, as the
                // built-up equation shows it.
                let mut arg = self.nested(Self::power)?;
                if crate::format::is_numeric(&arg) {
                    if self.unit_at(self.i) {
                        let unit = self.unit_chain()?;
                        arg = self.bin(BinOp::Mul, arg, unit)?;
                    } else if let Some(Tok::Ident(name)) = self.peek()
                        && Func::from_name(name).is_none()
                        && !(self.is_func)(name)
                        && !name.starts_with("log_")
                        && !name.starts_with("lim")
                    {
                        let right = self.nested(Self::power)?;
                        arg = self.bin(BinOp::Mul, arg, right)?;
                    }
                }
                return Ok(vec![arg]);
            }
        }
        match self.peek() {
            Some(Tok::LParen) => {
                self.bump();
                self.nested(Self::args)
            }
            _ => Err(CalcError::syntax(self.pos(), "expected the function's argument")),
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

/// A leading percent is a relative uncertainty marker, including in compound units
/// such as `% kg/m³`; remove only that factor, retaining the physical unit.
fn without_leading_percent(e: &Expr) -> Option<Expr> {
    match e {
        Expr::Unit(n) if n == "%" => Some(Expr::Num(1.0)),
        Expr::Bin(BinOp::Mul, l, r) if matches!(&**l, Expr::Unit(n) if n == "%") => Some((**r).clone()),
        Expr::Bin(op @ (BinOp::Mul | BinOp::Div), l, r) => without_leading_percent(l).map(|u| Expr::Bin(*op, Box::new(u), r.clone())),
        _ => None,
    }
}

/// The dimension of a chain of units (`kN·m`, `m/s²`), `None` when a part isn't a unit.
fn units_dim(e: &Expr) -> Option<crate::quantity::Dim> {
    match e {
        Expr::Unit(n) | Expr::Name(n) => crate::units::unit(n).map(|u| u.dim),
        Expr::Bin(BinOp::Mul, a, b) => units_dim(a)?.try_mul(units_dim(b)?).ok(),
        Expr::Bin(BinOp::Div, a, b) => units_dim(a)?.try_div(units_dim(b)?).ok(),
        Expr::Bin(BinOp::Pow, a, p) => {
            let p = match &**p {
                Expr::Num(v) => *v,
                Expr::Neg(inner) => match &**inner {
                    Expr::Num(v) => -*v,
                    _ => return None,
                },
                _ => return None,
            };
            units_dim(a)?.try_pow(p).ok()
        }
        _ => None,
    }
}
