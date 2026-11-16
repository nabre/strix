//! The structures of the Equation tab read as calculations, the way a built-up equation writes
//! them in linear format:
//!
//! ```text
//! primary  = "|" full "|"                        absolute value (a built-up one: `├|x|┤`)
//!          | "‖" full "‖"                        the length of a vector
//!          | "|" matrix "|"                      a determinant
//!          | "⌊" full "⌋" | "⌈" full "⌉"         floor, ceiling
//!          | "{" full "}"                        braces group like parentheses
//!          | "√" "(" full "&" full ")"           a root with its degree, degree first
//!          | "∛" postfix | "∜" postfix
//!          | "log_" base "⁡"? argument            a logarithm with its base
//!          | "lim" "┬" "(" name "→" limit side? ")" "⁡"? term
//!          | matrix | cases
//! matrix   = "■" "(" full ("&" full)* ("@" full ("&" full)*)* ")"
//! cases    = "{"? "█" "(" case ("@" case)* ")" "}"?
//! case     = expr (","? "&"? condition)?
//! condition= expr (cmp expr)+ | "otherwise"
//! side     = "^" ("+" | "-")
//! ```
//!
//! A matrix of one row or one column is a vector; any other is only read inside the bars of a
//! determinant. Everything here builds ordinary [`Expr`]s (calls and n-ary nodes), so solving,
//! steps and user functions take them as they take any other.

use crate::CalcError;
use crate::lexer::{Side, Tok};
use crate::parser::{Expr, Func, Nary, Parser};

type R<T> = Result<T, CalcError>;

/// The n-ary operators of a limit: from both sides, from the right (`x→a⁺`), from the left.
pub(crate) const LIMIT: char = '→';
pub(crate) const LIMIT_RIGHT: char = '↘';
pub(crate) const LIMIT_LEFT: char = '↗';

/// True for the operator of a limit.
pub(crate) fn is_limit(op: char) -> bool {
    matches!(op, LIMIT | LIMIT_RIGHT | LIMIT_LEFT)
}

/// Words that start a condition (skipped) and words that stand for "in every other case".
const IF_WORDS: &[&str] = &["if", "for", "when", "se", "per", "quando"];
const ELSE_WORDS: &[&str] = &["otherwise", "else", "elsewhere", "altrimenti", "altrove"];

/// The most rows and columns of a matrix (a determinant of 8×8 is 64 cells).
pub(crate) const MAX_MATRIX: usize = 8;

impl Parser<'_> {
    /// True when the next token opens one of the structures (so it multiplies what is before
    /// it: `2├|x|┤`, `3⌊x⌋`). A bar typed alone could close as well, so it never does.
    pub(crate) fn opens_structure(&self) -> bool {
        matches!(self.peek(), Some(Tok::LBrace | Tok::LFloor(_) | Tok::Root(_) | Tok::Matrix | Tok::Stack | Tok::Bar { side: Side::Open, .. }))
    }

    /// A structure that starts with a name already read: `log_2⁡x`, `log_(a+1)⁡x`,
    /// `lim┬(x→0)⁡…`. `None` when `name` starts none.
    pub(crate) fn named_structure(&mut self, name: &str) -> R<Option<Expr>> {
        let limit = (name == "lim" && matches!(self.peek(), Some(Tok::Below))) || (name == "lim_" && matches!(self.peek(), Some(Tok::LParen)));
        if limit {
            if matches!(self.peek(), Some(Tok::Below)) {
                self.bump();
            }
            return self.nested(Self::limit_of).map(Some);
        }
        let Some(base) = name.strip_prefix("log_") else {
            return Ok(None);
        };
        let base = if base.is_empty() {
            if !matches!(self.peek(), Some(Tok::LParen)) {
                return Ok(None);
            }
            self.bump();
            let inner = self.nested(Self::full)?;
            self.expect(Tok::RParen, ")")?;
            inner
        } else {
            if !matches!(self.peek(), Some(Tok::LParen | Tok::Operand)) {
                return Ok(None);
            }
            match base.parse::<f64>() {
                Ok(v) if v.is_finite() => self.node(Expr::Num(v))?,
                _ => self.node(Expr::Name(base.to_string()))?,
            }
        };
        let pos = self.pos();
        let mut args = self.function_args()?;
        if args.len() != 1 {
            return Err(CalcError::syntax(pos, "a logarithm takes one argument after its base"));
        }
        args.push(base);
        self.node(Expr::Call(Func::Log, args)).map(Some)
    }

    /// After `lim┬`: `(x→a)`, an optional `⁡` and the expression (it spans `·` and `/`, like the
    /// operand of a sum).
    fn limit_of(&mut self) -> R<Expr> {
        const HOW: &str = "write the limit as lim┬(x→a), with the variable and the value it tends to";
        self.expect(Tok::LParen, "(").map_err(|_| CalcError::syntax(self.pos(), HOW))?;
        let Some(Tok::Ident(var)) = self.peek().cloned() else {
            return Err(CalcError::syntax(self.pos(), HOW));
        };
        self.bump();
        if !matches!(self.peek(), Some(Tok::Tends)) {
            return Err(CalcError::syntax(self.pos(), HOW));
        }
        self.bump();
        let mut to = self.limit()?;
        // A value with its unit: `h→0 s`.
        if crate::format::is_numeric(&to) && self.unit_at(self.i) {
            let unit = self.unit_chain()?;
            to = self.node(Expr::Bin(crate::parser::BinOp::Mul, Box::new(to), Box::new(unit)))?;
        }
        let mut op = LIMIT;
        if matches!(self.peek(), Some(Tok::Caret)) {
            op = match self.toks.get(self.i.saturating_add(1)).map(|t| &t.tok) {
                Some(Tok::Plus) => LIMIT_RIGHT,
                Some(Tok::Minus) => LIMIT_LEFT,
                _ => return Err(CalcError::syntax(self.pos(), "after the value a limit takes only its side: ^+ or ^-")),
            };
            self.bump();
            self.bump();
        }
        self.expect(Tok::RParen, ")")?;
        if matches!(self.peek(), Some(Tok::Operand)) {
            self.bump();
        }
        let body = self.term()?;
        // The value tended to is the lower limit: with one, the variable is bound wherever names are read.
        self.node(Expr::Nary(Nary { op, var: Some(var), from: Some(Box::new(to)), to: None, body: Box::new(body) }))
    }

    /// After `√`: `(n&x)`, the root of degree `n`. `None` (nothing read) for a plain square root.
    pub(crate) fn root_with_degree(&mut self) -> R<Option<Expr>> {
        if !matches!(self.peek(), Some(Tok::LParen)) {
            return Ok(None);
        }
        // The `&` of this bracket, when it has one.
        let mut depth = 0usize;
        let mut amp = false;
        for t in self.toks.get(self.i..).unwrap_or_default() {
            match t.tok {
                Tok::LParen => depth = depth.saturating_add(1),
                Tok::RParen => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        break;
                    }
                }
                Tok::Amp if depth == 1 => {
                    amp = true;
                    break;
                }
                _ => {}
            }
        }
        if !amp {
            return Ok(None);
        }
        self.bump();
        let degree = self.nested(Self::full)?;
        self.expect(Tok::Amp, "&")?;
        let radicand = self.nested(Self::full)?;
        self.expect(Tok::RParen, ")")?;
        self.node(Expr::Call(Func::Root, vec![radicand, degree])).map(Some)
    }

    /// A structure that starts with its own sign: bars, floor and ceiling, braces, `∛`, a matrix,
    /// cases.
    pub(crate) fn structure(&mut self) -> R<Expr> {
        let pos = self.pos();
        let Some(tok) = self.peek().cloned() else {
            return Err(CalcError::syntax(pos, "expected a value"));
        };
        self.bump();
        match tok {
            Tok::Bar { side: Side::Close, .. } => Err(CalcError::syntax(pos, "expected a value, found a closing bar")),
            Tok::Bar { double, .. } => {
                let inner = self.full()?;
                match self.peek() {
                    Some(&Tok::Bar { double: d, side }) if d == double && side != Side::Open => self.bump(),
                    _ => return Err(CalcError::syntax(self.pos(), if double { "missing the closing ‖" } else { "missing the closing |" })),
                }
                match inner {
                    Expr::Call(Func::Matrix, cells) if !double => self.node(Expr::Call(Func::Det, cells)),
                    inner => self.node(Expr::Call(if double { Func::Norm } else { Func::Abs }, vec![inner])),
                }
            }
            Tok::LFloor(up) => {
                let inner = self.full()?;
                match self.peek() {
                    Some(&Tok::RFloor(u)) if u == up => self.bump(),
                    _ => return Err(CalcError::syntax(self.pos(), if up { "missing ⌉" } else { "missing ⌋" })),
                }
                self.node(Expr::Call(if up { Func::Ceil } else { Func::Floor }, vec![inner]))
            }
            Tok::Root(n) => {
                let arg = self.postfix()?;
                if n == 3 {
                    return self.node(Expr::Call(Func::Cbrt, vec![arg]));
                }
                let degree = self.node(Expr::Num(f64::from(n)))?;
                self.node(Expr::Call(Func::Root, vec![arg, degree]))
            }
            Tok::LBrace => {
                if matches!(self.peek(), Some(Tok::Stack)) {
                    self.bump();
                    let cases = self.cases()?;
                    if matches!(self.peek(), Some(Tok::RBrace)) {
                        self.bump();
                    }
                    return Ok(cases);
                }
                let inner = self.full()?;
                self.expect(Tok::RBrace, "}")?;
                Ok(inner)
            }
            Tok::Stack => self.cases(),
            Tok::Matrix => self.matrix(),
            other => Err(CalcError::syntax(pos, format!("expected a value, found {}", crate::lexer::describe(&other)))),
        }
    }

    /// After `■`: `(a&b@c&d)`. One row or one column is a vector.
    fn matrix(&mut self) -> R<Expr> {
        let pos = self.pos();
        self.expect(Tok::LParen, "( after ■")?;
        let mut rows: Vec<Vec<Expr>> = vec![Vec::new()];
        loop {
            let cell = self.full()?;
            if let Some(row) = rows.last_mut() {
                row.push(cell);
            }
            match self.peek() {
                Some(Tok::Amp) => self.bump(),
                Some(Tok::At) => {
                    self.bump();
                    rows.push(Vec::new());
                }
                Some(Tok::RParen) => {
                    self.bump();
                    break;
                }
                _ => return Err(CalcError::syntax(self.pos(), "expected &, @ or ) in the matrix")),
            }
            if rows.len() > MAX_MATRIX || rows.last().is_some_and(|r| r.len() >= MAX_MATRIX) {
                return Err(CalcError::syntax(pos, format!("a matrix has at most {MAX_MATRIX} rows and columns")));
            }
        }
        let columns = rows.first().map_or(0, Vec::len);
        if rows.iter().any(|r| r.len() != columns) {
            return Err(CalcError::syntax(pos, "the rows of the matrix have different lengths"));
        }
        if rows.len() == 1 || columns == 1 {
            return self.node(Expr::Vector(rows.into_iter().flatten().collect()));
        }
        let mut cells = vec![self.node(Expr::Num(columns as f64))?];
        cells.extend(rows.into_iter().flatten());
        self.node(Expr::Call(Func::Matrix, cells))
    }

    /// After `█`: `(value,&condition@value,&condition)`, the rows of cases.
    fn cases(&mut self) -> R<Expr> {
        let pos = self.pos();
        self.expect(Tok::LParen, "( after █")?;
        let mut args = Vec::new();
        let mut conditions = 0usize;
        loop {
            args.push(self.expr()?);
            if matches!(self.peek(), Some(Tok::Sep)) {
                self.bump();
            }
            if matches!(self.peek(), Some(Tok::Amp)) {
                self.bump();
            }
            if matches!(self.peek(), Some(Tok::At | Tok::RParen)) {
                args.push(self.node(Expr::Num(1.0))?);
            } else {
                let condition = self.condition()?;
                conditions += 1;
                args.push(condition);
            }
            match self.peek() {
                Some(Tok::At) => self.bump(),
                Some(Tok::RParen) => {
                    self.bump();
                    break;
                }
                _ => return Err(CalcError::syntax(self.pos(), "expected @ or ) after the case")),
            }
        }
        if conditions == 0 {
            return Err(CalcError::syntax(pos, "give each case its condition, like x<0, after a comma"));
        }
        self.node(Expr::Call(Func::Cases, args))
    }

    /// The condition of a case: `x<0`, `0≤x≤1`, `if x ≥ 0`, `otherwise`.
    fn condition(&mut self) -> R<Expr> {
        if let Some(Tok::Ident(word)) = self.peek() {
            if ELSE_WORDS.contains(&word.as_str()) {
                self.bump();
                return self.node(Expr::Num(1.0));
            }
            if IF_WORDS.contains(&word.as_str()) {
                self.bump();
            }
        }
        let mut left = self.expr()?;
        let mut all = Vec::new();
        loop {
            let func = match self.peek() {
                Some(Tok::Cmp('<')) => Func::Lt,
                Some(Tok::Cmp('≤')) => Func::Le,
                Some(Tok::Cmp('>')) => Func::Gt,
                Some(Tok::Cmp('≥')) => Func::Ge,
                Some(Tok::Cmp(_)) => Func::Ne,
                Some(Tok::Equals) => Func::Eq,
                _ => break,
            };
            self.bump();
            let right = self.expr()?;
            all.push(self.node(Expr::Call(func, vec![left, right.clone()]))?);
            left = right;
        }
        match all.len() {
            0 => Err(CalcError::syntax(self.pos(), "the condition of a case compares two values, like x<0")),
            1 => Ok(all.swap_remove(0)),
            _ => self.node(Expr::Call(Func::All, all)),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{CalcError, Options, evaluate};

    /// The display text of the last statement.
    fn last(statements: &[&str]) -> String {
        evaluate(statements, &Options::default()).pop().and_then(|r| r.display).unwrap_or_default()
    }

    fn last_error(statements: &[&str]) -> Option<CalcError> {
        evaluate(statements, &Options::default()).pop().and_then(|r| r.error)
    }

    fn shows(statement: &str, expected: &str) {
        assert_eq!(last(&[statement]), expected, "{statement}");
        assert_eq!(last_error(&[statement]), None, "{statement}");
    }

    #[test]
    fn fractions_and_radicals() {
        shows("6⁄3 =", "6⁄3 = 2");
        shows("6∕3 =", "6∕3 = 2");
        shows("√(3&27) =", "√(3&27) = 3");
        shows("√(4&16) =", "√(4&16) = 2");
        shows("√(2&9) =", "√(2&9) = 3");
        shows("∛27 =", "∛27 = 3");
        shows("∜16 =", "∜16 = 2");
        shows("√(3&−8) =", "√(3&−8) = -2");
        shows("√(a^2+b^2) =".replace("a", "3").replace("b", "4").as_str(), "√(3^2+4^2) = 5");
        // The degree may be an expression, the radicand a quantity.
        assert_eq!(last(&["V = 8 m³", "√(3&V) ="]), "√(3&V) = 2 m");
        assert_eq!(last(&["n = 3", "√(n&64) ="]), "√(n&64) = 4");
        // A square root in brackets is still one.
        shows("√(16) =", "√(16) = 4");
    }

    #[test]
    fn brackets_group_and_bars_measure() {
        shows("{[(2+3)·4−1]·2+2}/8 =", "{[(2+3)·4−1]·2+2}/8 = 5");
        shows("[6] =", "[6] = 6");
        shows("├|−6|┤ =", "├|−6|┤ = 6");
        shows("|−6| =", "|−6| = 6");
        shows("2├|−6|┤ =", "2├|−6|┤ = 12");
        shows("|2−5|·|−2| =", "|2−5|·|−2| = 6");
        shows("⌊2.7⌋ =", "⌊2.7⌋ = 2");
        shows("⌈2.1⌉ =", "⌈2.1⌉ = 3");
        shows("⌊−2.5⌋ =", "⌊−2.5⌋ = -3");
        shows("├‖[3; 4]‖┤ =", "├‖[3; 4]‖┤ = 5");
        shows("‖−7‖ =", "‖−7‖ = 7");
        assert_eq!(last(&["F = [3; 4] kN", "‖F‖ ="]), "‖F‖ = 5 kN");
        assert_eq!(last(&["x = −3 m", "├|x|┤ ="]), "├|x|┤ = 3 m");
        assert!(last_error(&["|3 ="]).is_some());
        assert!(last_error(&["⌊3⌉ ="]).is_some());
        assert!(last_error(&["{3) ="]).is_some());
    }

    #[test]
    fn reciprocal_functions_and_their_inverses() {
        shows("csc⁡(30°) =", "csc\u{2061}(30°) = 2");
        shows("sec⁡(60°) =", "sec\u{2061}(60°) = 2");
        shows("cot⁡(45°) =", "cot\u{2061}(45°) = 1");
        shows("csc^(−1)⁡2 =", "csc^(−1)\u{2061}2 = 30°");
        shows("sec^(−1)⁡2 =", "sec^(−1)\u{2061}2 = 60°");
        shows("cot^(−1)⁡1 =", "cot^(−1)\u{2061}1 = 45°");
        shows("cot^(−1)⁡0 =", "cot^(−1)\u{2061}0 = 90°");
        shows("cot⁻¹(−1) =", "cot⁻¹(−1) = 135°");
        shows("arccot(1) =", "arccot(1) = 45°");
        shows("arcsec(2) =", "arcsec(2) = 60°");
        shows("arccsc(2) =", "arccsc(2) = 30°");
        shows("csch⁡(ln(2)) =", "csch\u{2061}(ln(2)) = 1.333");
        shows("sech⁡0 =", "sech\u{2061}0 = 1");
        shows("coth⁡(ln(2)) =", "coth\u{2061}(ln(2)) = 1.667");
        shows("csch^(−1)⁡(4/3) =", "csch^(−1)\u{2061}(4/3) = 0.693");
        shows("sech^(−1)⁡1 =", "sech^(−1)\u{2061}1 = 0");
        shows("coth^(−1)⁡(5/3) =", "coth^(−1)\u{2061}(5/3) = 0.693");
        // The Italian names.
        shows("sen(30°) =", "sen(30°) = 0.5");
        shows("tg(45°) =", "tg(45°) = 1");
        shows("cotg(45°) =", "cotg(45°) = 1");
        shows("arcsen(0.5) =", "arcsen(0.5) = 30°");
        shows("arctg(1) =", "arctg(1) = 45°");
        // No value where the function has a pole or is not defined.
        assert_eq!(last_error(&["cot(0) ="]), Some(CalcError::DivisionByZero));
        assert_eq!(last_error(&["csc(180°) ="]), Some(CalcError::DivisionByZero));
        assert_eq!(last_error(&["sec^(−1)⁡(0.5) ="]), Some(CalcError::Domain { function: "sec⁻¹".into() }));
        assert_eq!(last_error(&["coth^(−1)⁡(0.5) ="]), Some(CalcError::Domain { function: "coth⁻¹".into() }));
    }

    #[test]
    fn inverses_are_written_as_the_gallery_writes_them() {
        let o = Options { steps: true, ..Options::default() };
        let run = |s: &[&str]| evaluate(s, &o).pop().and_then(|r| r.display).unwrap_or_default();
        // Solving turns a function into its inverse, shown as a power on the name.
        assert_eq!(run(&["y = 0.5", "y = sin(α)"]), "y = sin(α) ⇒ α = sin^(−1)\u{2061}(y) = sin^(−1)\u{2061}(0.5) = 30°");
        assert_eq!(run(&["y = 2", "y = sec(α)"]), "y = sec(α) ⇒ α = sec^(−1)\u{2061}(y) = sec^(−1)\u{2061}(2) = 60°");
        assert_eq!(run(&["y = 1", "y = cot(α)"]), "y = cot(α) ⇒ α = cot^(−1)\u{2061}(y) = cot^(−1)\u{2061}(1) = 45°");
        assert_eq!(run(&["α = 30°", "α = csc^(−1)⁡x"]), "α = csc^(−1)\u{2061}x ⇒ x = csc(α) = csc((30°)) = 2");
        // What is written back reads as the same calculation.
        assert_eq!(run(&["sin^(−1)\u{2061}(0.5) ="]), "sin^(−1)\u{2061}(0.5) = 30°");
    }

    /// A function applied the way an equation applies it (U+2061) takes what is written against
    /// it, as the linear format does.
    #[test]
    fn an_applied_function_takes_its_whole_operand() {
        assert_eq!(last(&["x = 0.5", "cos\u{2061}2x ="]), "cos\u{2061}2x = 0.54");
        assert_eq!(last(&["θ = 30°", "cos\u{2061}2θ ="]), "cos\u{2061}2θ = 0.5");
        shows("sin\u{2061}30° =", "sin\u{2061}30° = 0.5");
        assert_eq!(last(&["α = 30°", "β = 60°", "2 sin\u{2061}α cos\u{2061}β ="]), "2 sin\u{2061}α cos\u{2061}β = 0.5");
        assert_eq!(last(&["x = 2", "sin\u{2061}x^2 ="]), "sin\u{2061}x^2 = -0.757");
        assert_eq!(last(&["x = 2", "ln\u{2061}x + 1 ="]), "ln\u{2061}x + 1 = 1.693");
        assert_eq!(last(&["x = 2", "ln\u{2061}x·3 ="]), "ln\u{2061}x·3 = 2.079");
        // The differential of an integral is not part of the operand.
        shows("∫_0^π▒〖sin\u{2061}x dx〗 =", "∫_0^π▒〖sin\u{2061}x dx〗 = 2");
        // With brackets the argument is what they hold.
        assert_eq!(last(&["x = 0.5", "cos\u{2061}(2)x ="]), "cos\u{2061}(2)x = -0.208");
    }

    #[test]
    fn logarithms_with_a_base() {
        shows("log_2⁡8 =", "log_2\u{2061}8 = 3");
        shows("log_2⁡(8) =", "log_2\u{2061}(8) = 3");
        shows("log_10⁡1000 =", "log_10\u{2061}1000 = 3");
        shows("log_(1+1)⁡16 =", "log_(1+1)\u{2061}16 = 4");
        shows("log_3(81) =", "log_3(81) = 4");
        assert_eq!(last(&["a = 5", "log_a⁡125 ="]), "log_a\u{2061}125 = 3");
        assert_eq!(last(&["3 = log_2⁡x"]), "3 = log_2\u{2061}x ⇒ x = 8");
        assert!(last_error(&["log_1⁡8 ="]).is_some());
    }

    #[test]
    fn limits() {
        shows("lim┬(n→∞)⁡〖(1+1/n)^n〗 =", "lim┬(n→∞)\u{2061}〖(1+1/n)^n〗 = 2.718");
        shows("lim┬(x→0)⁡〖sin(x)/x〗 =", "lim┬(x→0)\u{2061}〖sin(x)/x〗 = 1");
        shows("lim┬(x→0)⁡〖(1−cos(x))/x^2〗 =", "lim┬(x→0)\u{2061}〖(1−cos(x))/x^2〗 = 0.5");
        shows("lim┬(x→2)⁡〖(x^2−4)/(x−2)〗 =", "lim┬(x→2)\u{2061}〖(x^2−4)/(x−2)〗 = 4");
        shows("lim┬(x→∞)⁡〖(3x^2+1)/(x^2−x)〗 =", "lim┬(x→∞)\u{2061}〖(3x^2+1)/(x^2−x)〗 = 3");
        shows("lim┬(x→−∞)⁡〖e^x〗 =", "lim┬(x→−∞)\u{2061}〖e^x〗 = 0");
        shows("lim┬(x→∞)⁡〖e^(−x)〗 =", "lim┬(x→∞)\u{2061}〖e^(−x)〗 = 0");
        shows("lim┬(x→0^+)⁡〖x·ln(x)〗 =", "lim┬(x→0^+)\u{2061}〖x·ln(x)〗 = 0");
        shows("lim┬(x→0)⁡〖√x〗 =", "lim┬(x→0)\u{2061}〖√x〗 = 0");
        shows("lim┬(x→0^+)⁡〖|x|/x〗 =", "lim┬(x→0^+)\u{2061}〖|x|/x〗 = 1");
        shows("lim┬(x→0^-)⁡〖|x|/x〗 =", "lim┬(x→0^-)\u{2061}〖|x|/x〗 = -1");
        shows("lim┬(x→0⁻)⁡〖|x|/x〗 =", "lim┬(x→0⁻)\u{2061}〖|x|/x〗 = -1");
        shows("lim┬(x→1)⁡x^2 =", "lim┬(x→1)\u{2061}x^2 = 1");
        shows("lim_(x→3)⁡〖2·x〗 =", "lim_(x→3)\u{2061}〖2·x〗 = 6");
        // A derivative as a limit, with units.
        assert_eq!(last(&["s(t) = (3 m/s²)·t^2", "lim┬(h→0 s)⁡〖(s(2 s+h)−s(2 s))/h〗 ="]), "lim┬(h→0 s)\u{2061}〖(s(2 s+h)−s(2 s))/h〗 = 12 m/s");
        // No limit is an error, never a number.
        let no = |s: &str| matches!(last_error(&[s]), Some(CalcError::Structure(_)));
        assert!(no("lim┬(x→0)⁡〖|x|/x〗 ="));
        assert!(no("lim┬(x→0)⁡〖1/x^2〗 ="));
        assert!(no("lim┬(x→0^+)⁡〖ln(x)〗 ="));
        assert!(no("lim┬(x→0)⁡〖sin(1/x)〗 ="));
        assert!(no("lim┬(x→∞)⁡〖sin(x)〗 ="));
        assert!(no("lim┬(x→∞)⁡〖x^2〗 ="));
        assert!(last_error(&["lim┬6⁡3 ="]).is_some());
        // The variable is the limit's own.
        assert_eq!(last(&["x = 7", "lim┬(x→1)⁡〖x+1〗 ="]), "lim┬(x→1)\u{2061}〖x+1〗 = 2");
        assert_eq!(last(&["L = lim┬(x→0)⁡〖sin(2x)/x〗", "L ="]), "L = 2");
    }

    #[test]
    fn matrices_vectors_and_determinants() {
        shows("■(6@3@2) =", "■(6@3@2) = [6; 3; 2]");
        shows("(■(1&2&3)) =", "(■(1&2&3)) = [1; 2; 3]");
        shows("[■(1@2)] =", "[■(1@2)] = [1; 2]");
        shows("├|■(6&3@2&5)|┤ =", "├|■(6&3@2&5)|┤ = 24");
        shows("|■(1&2@3&4)| =", "|■(1&2@3&4)| = -2");
        shows("├|■(2&0&1@1&3&2@1&1&1)|┤ =", "├|■(2&0&1@1&3&2@1&1&1)|┤ = 0");
        shows("├|■(1&0&0@0&1&0@0&0&1)|┤ =", "├|■(1&0&0@0&1&0@0&0&1)|┤ = 1");
        shows("├|■(0&1@1&0)|┤ =", "├|■(0&1@1&0)|┤ = -1");
        shows("det(■(1&2@3&4)) =", "det(■(1&2@3&4)) = -2");
        assert_eq!(last(&["a = 2 m", "├|■(a&1 m@0 m&3 m)|┤ ="]), "├|■(a&1 m@0 m&3 m)|┤ = 6 m²");
        assert_eq!(evaluate(&["v = ■(3@4) m"], &Options::default()).pop().and_then(|r| r.error), None);
        assert_eq!(last(&["v = ■(3@4) m", "‖v‖ ="]), "‖v‖ = 5 m");
        // A definition that starts with a structure is still a definition.
        assert_eq!(last(&["a = ├|−3 m|┤", "a ="]), "a = 3 m");
        assert_eq!(last(&["a = ⌊3.5⌋", "a ="]), "a = 3");
        assert_eq!(last(&["a = {2+1}·2", "a ="]), "a = 6");
        assert_eq!(last(&["a = ∛8", "a ="]), "a = 2");
        let err = |s: &str| last_error(&[s]);
        assert!(matches!(err("■(1&2@3&4) ="), Some(CalcError::Structure(_))));
        assert!(matches!(err("├|■(1&2&3@4&5&6)|┤ ="), Some(CalcError::Structure(_))));
        assert!(err("■(1&2@3) =").is_some());
        assert!(err("├|■(1 m&2@3&4)|┤ =").is_some());
    }

    #[test]
    fn cases_choose_by_their_conditions() {
        let f = "f(x)=├{█(−x,&x<0@x,&x≥0)┤";
        assert_eq!(last(&[f, "f(−3) ="]), "f(−3) = 3");
        assert_eq!(last(&[f, "f(4) ="]), "f(4) = 4");
        assert_eq!(last(&[f, "f(0) ="]), "f(0) = 0");
        let g = "g(x) = {█(0,&x<0@x^2,&0≤x≤2@4,&otherwise)┤";
        assert_eq!(last(&[g, "g(−1) ="]), "g(−1) = 0");
        assert_eq!(last(&[g, "g(1.5) ="]), "g(1.5) = 2.25");
        assert_eq!(last(&[g, "g(9) ="]), "g(9) = 4");
        // A case is not evaluated where it does not apply.
        let h = "h(x) = {█(ln(x),&x>0@0,&x≤0)┤";
        assert_eq!(last(&[h, "h(−5) ="]), "h(−5) = 0");
        assert_eq!(last(&[h, "h(1) ="]), "h(1) = 0");
        // Units, and a value chosen in place.
        assert_eq!(last(&["t = 3 s", "v = {█(2 m/s,&t<2 s@5 m/s,&t≥2 s)┤", "v ="]), "v = 5 m/s");
        assert_eq!(last(&["x = 2", "{█(1,&x=2@0,&x≠2)┤ ="]), "{█(1,&x=2@0,&x≠2)┤ = 1");
        // Its integral and its limit are taken like any function's.
        assert_eq!(last(&[f, "∫_(−1)^2▒〖f(x) dx〗 ="]), "∫_(−1)^2▒〖f(x) dx〗 = 2.5");
        assert!(matches!(last_error(&["p(x) = {█(1,&x<0@2,&x>0)┤", "p(0) ="]), Some(CalcError::Structure(_))));
        assert!(last_error(&["{█(1@2)┤ ="]).is_some());
        assert!(last_error(&["t = 3 s", "{█(1,&t<2@2,&t≥2)┤ ="]).is_some());
    }

    #[test]
    fn accents_belong_to_names_and_subscripts_index_vectors() {
        assert_eq!(last(&["x = [2; 4; 9]", "x\u{305} = mean(x)", "x\u{305} ="]), "x\u{305} = 5");
        assert_eq!(last(&["v\u{20D7} = [3; 4] m", "‖v\u{20D7}‖ ="]), "‖v\u{20D7}‖ = 5 m");
        assert_eq!(last(&["x\u{307} = 3 m/s", "2·x\u{307} ="]), "2·x\u{307} = 6 m/s");
        // `x_i` under a sum over `i` is the element; so is `x_2` when no such name is defined.
        assert_eq!(last(&["x = [2; 4; 9]", "n = 3", "∑_(i=1)^n▒x_i ="]), "∑_(i=1)^n▒x_i = 15");
        assert_eq!(last(&["A = [2; 4; 9]", "n = 3", "∏_(k=1)^n▒(A_k) ="]), "∏_(k=1)^n▒(A_k) = 72");
        assert_eq!(last(&["x = [2; 4; 9]", "n = 3", "m = (∑_(i=1)^n▒x_i)/n", "m ="]), "m = 5");
        assert_eq!(last(&["x = [2; 4; 9] m", "x_2 ="]), "x_2 = 4 m");
        // A name of its own wins, and can still be defined beside the vector.
        assert_eq!(last(&["F = [1; 2] kN", "F_1 = 7 kN", "F_1 ="]), "F_1 = 7 kN");
        assert_eq!(last(&["v = [3; 4] m/s", "v_x = 3 m/s", "v_x ="]), "v_x = 3 m/s");
        assert!(last_error(&["x = [2; 4; 9]", "∑_(i=1)^5▒x_i ="]).is_some());
    }

    #[test]
    fn hostile_structures_never_panic() {
        for s in [
            "|", "||", "|||", "‖", "├|", "|┤", "├┤", "{", "}", "{}", "{{{{", "⌊", "⌋", "⌈⌉", "√(&)", "√(3&)", "√(&3)", "∛", "∜", "■", "■(", "■()", "■(&)",
            "■(@)", "■(1&2@3)", "█", "█(", "█()", "█(@@&&", "{█(,&)┤", "{█(1,&x<)┤", "lim", "lim┬", "lim┬(", "lim┬(x", "lim┬(x→", "lim┬(x→)", "lim┬(x→0",
            "lim┬(x→0)", "lim┬(x→0^)", "lim┬(x→0^+", "lim┬(x→∞)⁡〖1/0〗", "lim┬(x→x)⁡x", "log_", "log_⁡", "log_()⁡3", "log_2", "log_2⁡", "log_0⁡3",
            "├|■(1&2@3&4)|", "|■()|", "det()", "det(1)", "det(■(1&2@3&4); 2)", "norm()", "norm(1; 2)", "x_", "x_0", "x_99999999999", "⁺", "⁻", "2⁺",
            "csc", "csc^(−1)", "sec⁡", "cot^(−2)⁡x", "x\u{305}\u{305}\u{305}", "\u{305}", "6⁄0", "√(0&5)", "∛−8",
        ] {
            for tail in ["", " =", " = 3"] {
                let text = format!("{s}{tail}");
                let _ = evaluate(&["x = [1; 2; 3]", text.as_str()], &Options::default());
                let _ = evaluate(&[text.as_str()], &Options { steps: true, ..Options::default() });
            }
        }
        // Deep nesting ends with an error.
        let deep = format!("{}1{} =", "|".repeat(200), "|".repeat(200));
        assert!(last_error(&[deep.as_str()]).is_some());
        let deep = format!("{}1{} =", "{".repeat(200), "}".repeat(200));
        assert!(last_error(&[deep.as_str()]).is_some());
        let big = format!("├|■({})|┤ =", vec![vec!["1"; 9].join("&"); 9].join("@"));
        assert!(last_error(&[big.as_str()]).is_some());
    }
}
