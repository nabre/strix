//! Evaluation of the Equation tab's structures (`crate::structures` reads them): cases and their
//! conditions, determinants, the length of a vector and limits.

use super::{Ctx, Value, domain};
use crate::CalcError;
use crate::limits::From;
use crate::parser::{Expr, Func, Nary};
use crate::quantity::{Quantity, finite};
use crate::structures::{LIMIT_LEFT, LIMIT_RIGHT, MAX_MATRIX};

/// Functions that choose which of their arguments to evaluate, or read them as a grid.
pub(super) fn is_lazy(func: Func) -> bool {
    matches!(func, Func::Cases | Func::Matrix | Func::Det | Func::All | Func::Lt | Func::Le | Func::Gt | Func::Ge | Func::Ne | Func::Eq)
}

impl Ctx<'_> {
    pub(super) fn structure(&mut self, func: Func, args: &[Expr]) -> Result<Value, CalcError> {
        match func {
            // The first case whose condition holds; the others are not evaluated, so a case may
            // be undefined where it does not apply (`ln(x)` for `x > 0`, `0` otherwise).
            Func::Cases => {
                for case in args.chunks(2) {
                    let [value, condition] = case else { break };
                    if self.holds(condition)? {
                        return self.eval(value);
                    }
                }
                Err(CalcError::Structure("none of the cases applies to this value"))
            }
            Func::Matrix => {
                Err(CalcError::Structure("a matrix is not calculated: only its determinant (between bars) or one row or column (a vector)"))
            }
            Func::Det => self.determinant(args),
            _ => {
                let holds = self.holds_call(func, args)?;
                Ok(Quantity::number(if holds { 1.0 } else { 0.0 }).into())
            }
        }
    }

    /// True when the condition `e` of a case holds.
    fn holds(&mut self, e: &Expr) -> Result<bool, CalcError> {
        match e {
            Expr::Call(func, args) if is_lazy(*func) && !matches!(func, Func::Cases | Func::Matrix | Func::Det) => self.holds_call(*func, args),
            _ => Ok(self.eval(e)?.expect_scalar()?.value != 0.0),
        }
    }

    fn holds_call(&mut self, func: Func, args: &[Expr]) -> Result<bool, CalcError> {
        if func == Func::All {
            for a in args {
                if !self.holds(a)? {
                    return Ok(false);
                }
            }
            return Ok(true);
        }
        let [a, b] = args else {
            return Err(CalcError::Arity { function: func.name().to_string(), expected: "2 values" });
        };
        let x = self.eval(a)?.expect_scalar()?;
        let y = self.eval(b)?.expect_scalar()?;
        x.same_dim(y, "compare")?;
        let same = (x.value - y.value).abs() <= 1e-12 * x.value.abs().max(y.value.abs());
        Ok(match func {
            Func::Lt => x.value < y.value && !same,
            Func::Le => x.value <= y.value || same,
            Func::Gt => x.value > y.value && !same,
            Func::Ge => x.value >= y.value || same,
            Func::Ne => !same,
            _ => same,
        })
    }

    /// `|■(a&b@c&d)|`: `args` is the number of columns, then the cells row by row (or one
    /// matrix, from `det(■(…))`). The cells are of one kind; the result is that kind to the
    /// matrix's order.
    fn determinant(&mut self, args: &[Expr]) -> Result<Value, CalcError> {
        const SQUARE: CalcError = CalcError::Structure("a determinant takes a square matrix, as many rows as columns");
        let args = match args {
            [Expr::Call(Func::Matrix, cells)] => cells.as_slice(),
            [_] => return Err(SQUARE),
            other => other,
        };
        let Some((Expr::Num(columns), cells)) = args.split_first() else {
            return Err(SQUARE);
        };
        let n = *columns as usize;
        if n == 0 || n > MAX_MATRIX || cells.len() != n.saturating_mul(n) {
            return Err(SQUARE);
        }
        let mut m: Vec<f64> = Vec::with_capacity(cells.len());
        let mut first: Option<Quantity> = None;
        for cell in cells {
            let q = self.eval(cell)?.expect_scalar()?;
            if q.unc.is_some() {
                return Err(domain("uncertainty propagation: unsupported function; use val() explicitly"));
            }
            match first {
                None => first = Some(q),
                Some(p) => p.same_dim(q, "add")?,
            }
            m.push(q.value);
        }
        let whole = m.iter().all(|v| v.fract() == 0.0);
        let mut dim = crate::quantity::Dim::NONE;
        for _ in 0..n {
            dim = dim.try_mul(first.map(|q| q.dim).unwrap_or_default())?;
        }
        // Gaussian elimination with partial pivoting.
        let at = |m: &[f64], r: usize, c: usize| m.get(r * n + c).copied().unwrap_or(0.0);
        let mut det = 1.0f64;
        for col in 0..n {
            let pivot = (col..n).max_by(|&a, &b| at(&m, a, col).abs().total_cmp(&at(&m, b, col).abs())).unwrap_or(col);
            if at(&m, pivot, col) == 0.0 {
                return Ok(Quantity::new(0.0, dim).into());
            }
            if pivot != col {
                for c in 0..n {
                    m.swap(pivot * n + c, col * n + c);
                }
                det = -det;
            }
            let p = at(&m, col, col);
            det *= p;
            for r in (col + 1)..n {
                let factor = at(&m, r, col) / p;
                for c in col..n {
                    let v = at(&m, r, c) - factor * at(&m, col, c);
                    if let Some(slot) = m.get_mut(r * n + c) {
                        *slot = v;
                    }
                }
            }
        }
        // Whole cells give a whole determinant: drop the elimination's last bits. Any other
        // keeps its value however small it is (cells in micrometres are 10⁻⁶ in SI).
        let rounded = det.round();
        if whole && (det - rounded).abs() <= 1e-9 * det.abs().max(1.0) {
            det = rounded;
        }
        Ok(Quantity::new(finite(det)?, dim).into())
    }

    /// A name that is not defined, read as an element of a vector by its subscript: `x_i` is
    /// `x[i]` and `x_2` is `x[2]` (1-based) when `x` is a vector and the index a whole number.
    /// `None` when `name` is not one.
    pub(super) fn subscripted(&self, name: &str) -> Option<Result<Value, CalcError>> {
        let (base, index) = name.split_once('_')?;
        let known = |n: &str| self.locals.iter().rev().find(|(m, _)| m == n).map(|(_, v)| v).or_else(|| self.scope.get(n));
        let Value::Vector(vector) = known(base)? else {
            return None;
        };
        let index = match index.parse::<u32>() {
            Ok(i) => f64::from(i),
            Err(_) => match known(index)? {
                Value::Scalar(q) if q.dim.is_dimensionless() => q.value,
                _ => return None,
            },
        };
        let len = vector.len();
        if index.fract() != 0.0 || index < 1.0 || index > len as f64 {
            return Some(Err(CalcError::IndexOutOfRange { index: index as i64, len }));
        }
        Some(vector.iter().nth(index as usize - 1).map(Value::Scalar).ok_or(CalcError::IndexOutOfRange { index: index as i64, len }))
    }

    /// `‖v‖`: the length of a vector, the absolute value of a number.
    pub(super) fn norm(&mut self, args: &[Value]) -> Result<Value, CalcError> {
        let [v] = args else {
            return Err(CalcError::Arity { function: "norm".to_string(), expected: "1 argument" });
        };
        let all = v.quantities();
        self.charge(all.len())?;
        if all.iter().any(|q| q.unc.is_some()) {
            return Err(domain("uncertainty propagation: unsupported function; use val() explicitly"));
        }
        let largest = all.iter().fold(0.0f64, |m, q| m.max(q.value.abs()));
        if largest == 0.0 {
            return Ok(Quantity::new(0.0, v.dim()).into());
        }
        // Scaled, so the squares of large values do not overflow.
        let sum: f64 = all.iter().map(|q| (q.value / largest).powi(2)).sum();
        Ok(Quantity::new(finite(largest * sum.sqrt())?, v.dim()).into())
    }

    /// `lim┬(var→a)⁡body`: `a` is the node's lower limit.
    pub(super) fn limit(&mut self, n: &Nary) -> Result<Value, CalcError> {
        let (Some(var), Some(to)) = (&n.var, &n.from) else {
            return Err(CalcError::syntax(0, "write the limit as lim┬(x→a), with the variable and the value it tends to"));
        };
        // At infinity the variable is a plain number.
        let a = match crate::calculus::infinite(to) {
            Some(v) => Quantity::number(v),
            None => self.eval(to)?.expect_scalar()?,
        };
        let from = match n.op {
            LIMIT_RIGHT => From::Right,
            LIMIT_LEFT => From::Left,
            _ => From::Both,
        };
        const WHAT: &str = "lim";
        let mut first: Option<Quantity> = None;
        let mut f = |v: f64| -> Result<f64, CalcError> {
            self.locals.push((var.clone(), Quantity::new(v, a.dim).into()));
            let q = self.eval(&n.body).and_then(|v| v.expect_scalar());
            self.locals.pop();
            let q = q?;
            match first {
                None => first = Some(q),
                Some(p) => p.same_dim(q, WHAT)?,
            }
            Ok(q.value)
        };
        let value = crate::limits::limit(&mut f, a.value, from)?;
        Ok(Quantity::new(finite(value)?, first.map(|q| q.dim).unwrap_or_default()).into())
    }
}
