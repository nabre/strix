//! Evaluation of expression trees against a scope.

use crate::parser::{BinOp, Expr, Func, Nary};
use crate::quantity::{Dim, Quantity, finite};
use crate::units::{constant, dim_name, unit};
use crate::{CalcError, MAX_SHEET_STEPS, MAX_STEPS, MAX_VALUES};

/// A list of magnitudes (coherent SI units) sharing one dimension: a range, a vector literal or
/// a table column. At most [`MAX_VALUES`] long; every value is finite.
#[derive(Debug, Clone, PartialEq)]
pub struct Vector {
    /// Magnitudes in coherent SI units.
    pub values: Vec<f64>,
    /// The dimension of every element.
    pub dim: Dim,
}

impl Vector {
    /// A vector from quantities that must all have the same dimension. An empty list is a
    /// dimensionless empty vector.
    pub fn from_quantities(items: &[Quantity]) -> Result<Vector, CalcError> {
        let dim = items.first().map_or(Dim::NONE, |q| q.dim);
        if items.len() > MAX_VALUES {
            return Err(CalcError::TooManyValues { limit: MAX_VALUES });
        }
        let mut values = Vec::with_capacity(items.len());
        for q in items {
            if q.dim != dim {
                return Err(CalcError::MixedVector { first: dim_name(dim), other: dim_name(q.dim) });
            }
            values.push(finite(q.value)?);
        }
        Ok(Vector { values, dim })
    }

    /// The elements as quantities.
    pub fn iter(&self) -> impl Iterator<Item = Quantity> + '_ {
        self.values.iter().map(|&v| Quantity::new(v, self.dim))
    }

    /// Number of elements.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// True when the vector has no elements.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// A worksheet value: one quantity, or a [`Vector`] of them.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// One quantity.
    Scalar(Quantity),
    /// Several quantities of one dimension.
    Vector(Vector),
}

impl Value {
    /// The quantity of a scalar value.
    pub fn scalar(&self) -> Option<Quantity> {
        match self {
            Value::Scalar(q) => Some(*q),
            Value::Vector(_) => None,
        }
    }

    /// The dimension of the value (of every element of a vector).
    pub fn dim(&self) -> Dim {
        match self {
            Value::Scalar(q) => q.dim,
            Value::Vector(v) => v.dim,
        }
    }

    /// The elements: one for a scalar.
    pub fn quantities(&self) -> Vec<Quantity> {
        match self {
            Value::Scalar(q) => vec![*q],
            Value::Vector(v) => v.iter().collect(),
        }
    }

    /// The quantity of a scalar value, else [`CalcError::NotAScalar`].
    pub(crate) fn expect_scalar(&self) -> Result<Quantity, CalcError> {
        match self {
            Value::Scalar(q) => Ok(*q),
            Value::Vector(v) => Err(CalcError::NotAScalar { len: v.len() }),
        }
    }
}

impl From<Quantity> for Value {
    fn from(q: Quantity) -> Self {
        Value::Scalar(q)
    }
}

/// The parameters defined so far in a worksheet, in definition order. Redefining a name updates
/// its value in place (it keeps its original position).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scope {
    entries: Vec<(String, Value)>,
    /// Work done by every evaluation against this scope (one worksheet), capped by
    /// [`MAX_SHEET_STEPS`]: however many regions and cells a document has, recalculating it ends.
    spent: std::cell::Cell<usize>,
}

impl Scope {
    /// An empty scope.
    pub fn new() -> Self {
        Scope::default()
    }

    /// The value of parameter `name`, if defined.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.entries.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    /// Defines or redefines `name`.
    pub fn set(&mut self, name: &str, value: impl Into<Value>) {
        let value = value.into();
        match self.entries.iter_mut().find(|(n, _)| n == name) {
            Some(entry) => entry.1 = value,
            None => self.entries.push((name.to_string(), value)),
        }
    }

    /// Undefines `name`, returning its old value.
    pub fn remove(&mut self, name: &str) -> Option<Value> {
        let index = self.entries.iter().position(|(n, _)| n == name)?;
        Some(self.entries.remove(index).1)
    }

    /// The work done so far by evaluations against this scope (see [`MAX_SHEET_STEPS`]).
    pub fn spent(&self) -> usize {
        self.spent.get()
    }

    /// Sets the work done so far: several worksheets of one document share one budget.
    pub fn set_spent(&self, steps: usize) {
        self.spent.set(steps);
    }

    /// The parameters in definition order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.entries.iter().map(|(n, v)| (n.as_str(), v))
    }

    /// Number of parameters.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when nothing is defined.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// How names resolve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Names {
    /// Local (a bound `∑` variable, a table column), parameter, constant, then unit.
    Worksheet,
    /// Units only (the display unit after `=`).
    UnitsOnly,
}

/// Evaluates `expr` with no locals.
pub(crate) fn eval(expr: &Expr, scope: &Scope, names: Names) -> Result<Value, CalcError> {
    Ctx::new(scope, names, Vec::new()).eval(expr)
}

/// Evaluates `expr`, which must give a single quantity.
pub(crate) fn eval_scalar(expr: &Expr, scope: &Scope, names: Names) -> Result<Quantity, CalcError> {
    eval(expr, scope, names)?.expect_scalar()
}

/// An evaluation: the scope, local names that shadow it, and the work done so far (capped by
/// [`MAX_STEPS`], so nested sums over long ranges stop instead of hanging). Recursion is bounded
/// by the parser's node and depth limits.
pub(crate) struct Ctx<'a> {
    scope: &'a Scope,
    names: Names,
    locals: Vec<(String, Value)>,
    steps: usize,
}

impl<'a> Ctx<'a> {
    pub(crate) fn new(scope: &'a Scope, names: Names, locals: Vec<(String, Value)>) -> Self {
        Ctx { scope, names, locals, steps: 0 }
    }

    /// Counts `n` units of work.
    fn charge(&mut self, n: usize) -> Result<(), CalcError> {
        let n = n.max(1);
        self.steps = self.steps.saturating_add(n);
        let spent = self.scope.spent.get().saturating_add(n);
        self.scope.spent.set(spent);
        if self.steps > MAX_STEPS || spent > MAX_SHEET_STEPS { Err(CalcError::TooMuchWork) } else { Ok(()) }
    }

    /// Starts the step count at `steps`, so several evaluations share one [`MAX_STEPS`] budget.
    pub(crate) fn with_steps(mut self, steps: usize) -> Self {
        self.steps = steps;
        self
    }

    /// The steps counted so far.
    pub(crate) fn steps(&self) -> usize {
        self.steps
    }

    pub(crate) fn eval(&mut self, expr: &Expr) -> Result<Value, CalcError> {
        self.charge(1)?;
        match expr {
            Expr::Num(v) => Ok(Quantity::number(finite(*v)?).into()),
            Expr::Name(name) => self.resolve(name),
            Expr::Unit(name) => unit(name).map(Value::Scalar).ok_or_else(|| CalcError::UnknownUnit(name.clone())),
            Expr::Neg(inner) => Ok(match self.eval(inner)? {
                Value::Scalar(q) => Value::Scalar(q.negate()),
                Value::Vector(mut v) => {
                    v.values.iter_mut().for_each(|x| *x = -*x);
                    Value::Vector(v)
                }
            }),
            Expr::Bin(op, left, right) => {
                let a = self.eval(left)?;
                let b = self.eval(right)?;
                self.binary(*op, a, b)
            }
            Expr::Call(func, args) => {
                let values = args.iter().map(|a| self.eval(a)).collect::<Result<Vec<_>, _>>()?;
                self.call(*func, &values)
            }
            Expr::Vector(items) => {
                let mut all = Vec::new();
                for item in items {
                    let value = self.eval(item)?;
                    self.charge(all.len())?;
                    all.extend(value.quantities());
                    if all.len() > MAX_VALUES {
                        return Err(CalcError::TooManyValues { limit: MAX_VALUES });
                    }
                }
                Ok(Value::Vector(Vector::from_quantities(&all)?))
            }
            Expr::Index(base, index) => {
                let base = self.eval(base)?;
                let index = self.eval(index)?;
                let items = base.quantities();
                let pick = |i: Quantity| -> Result<Quantity, CalcError> {
                    let k = whole(i, "an index")?;
                    let found = usize::try_from(k).ok().and_then(|k| k.checked_sub(1)).and_then(|k| items.get(k));
                    found.copied().ok_or(CalcError::IndexOutOfRange { index: k, len: items.len() })
                };
                match index {
                    Value::Scalar(i) => Ok(pick(i)?.into()),
                    Value::Vector(v) => {
                        self.charge(v.len())?;
                        let picked = v.iter().map(pick).collect::<Result<Vec<_>, _>>()?;
                        Ok(Value::Vector(Vector::from_quantities(&picked)?))
                    }
                }
            }
            Expr::Range(from, to) => {
                let from = self.eval(from)?.expect_scalar()?;
                let to = self.eval(to)?.expect_scalar()?;
                let (a, b) = (plain("a range", from)?, plain("a range", to)?);
                let span = (b - a).abs().floor();
                if span >= MAX_VALUES as f64 {
                    return Err(CalcError::TooManyValues { limit: MAX_VALUES });
                }
                let count = span as usize + 1;
                self.charge(count)?;
                let step = if b < a { -1.0 } else { 1.0 };
                let values = (0..count).map(|k| a + step * k as f64).collect();
                Ok(Value::Vector(Vector { values, dim: Dim::NONE }))
            }
            Expr::Nary(nary) => {
                let mark = self.locals.len();
                let result = self.nary(nary);
                self.locals.truncate(mark);
                result
            }
        }
    }

    fn resolve(&self, name: &str) -> Result<Value, CalcError> {
        match self.names {
            Names::Worksheet => {
                if let Some((_, v)) = self.locals.iter().rev().find(|(n, _)| n == name) {
                    return Ok(v.clone());
                }
                if let Some(v) = self.scope.get(name) {
                    return Ok(v.clone());
                }
                constant(name).or_else(|| unit(name)).map(Value::Scalar).ok_or_else(|| match Func::from_name(name) {
                    Some(_) => CalcError::FunctionNeedsArguments(name.to_string()),
                    None => CalcError::UnknownName(name.to_string()),
                })
            }
            Names::UnitsOnly => unit(name).map(Value::Scalar).ok_or_else(|| CalcError::UnknownUnit(name.to_string())),
        }
    }

    /// `a op b`, element by element when either side is a vector (vectors must be as long).
    fn binary(&mut self, op: BinOp, a: Value, b: Value) -> Result<Value, CalcError> {
        let apply = |x: Quantity, y: Quantity| match op {
            BinOp::Add => x.try_add(y),
            BinOp::Sub => x.try_sub(y),
            BinOp::Mul => x.try_mul(y),
            BinOp::Div => x.try_div(y),
            BinOp::Pow => x.try_pow(y),
        };
        let items: Vec<Quantity> = match (a, b) {
            (Value::Scalar(x), Value::Scalar(y)) => return Ok(apply(x, y)?.into()),
            (Value::Vector(u), Value::Scalar(y)) => {
                self.charge(u.len())?;
                u.iter().map(|x| apply(x, y)).collect::<Result<_, _>>()?
            }
            (Value::Scalar(x), Value::Vector(v)) => {
                self.charge(v.len())?;
                v.iter().map(|y| apply(x, y)).collect::<Result<_, _>>()?
            }
            (Value::Vector(u), Value::Vector(v)) => {
                if u.len() != v.len() {
                    return Err(CalcError::LengthMismatch { left: u.len(), right: v.len() });
                }
                self.charge(u.len())?;
                u.iter().zip(v.iter()).map(|(x, y)| apply(x, y)).collect::<Result<_, _>>()?
            }
        };
        Ok(Value::Vector(Vector::from_quantities(&items)?))
    }

    /// `∑`/`∏`: over `var = from..to`, over the values of `var`, or over the operand's elements.
    fn nary(&mut self, n: &Nary) -> Result<Value, CalcError> {
        let sum = n.op == '∑';
        let mut acc: Option<Quantity> = None;
        let add = |acc: &mut Option<Quantity>, q: Quantity| -> Result<(), CalcError> {
            *acc = Some(match *acc {
                None => q,
                Some(a) if sum => a.try_add(q)?,
                Some(a) => a.try_mul(q)?,
            });
            Ok(())
        };
        match (&n.var, &n.from, &n.to) {
            (Some(var), Some(from), Some(to)) => {
                let what = if sum { "a limit of ∑" } else { "a limit of ∏" };
                let a = whole(self.eval(from)?.expect_scalar()?, what)?;
                let b = whole(self.eval(to)?.expect_scalar()?, what)?;
                if b >= a && b.saturating_sub(a) >= MAX_VALUES as i64 {
                    return Err(CalcError::TooManyValues { limit: MAX_VALUES });
                }
                let mut i = a;
                while i <= b {
                    self.locals.push((var.clone(), Quantity::number(i as f64).into()));
                    let q = self.eval(&n.body)?.expect_scalar()?;
                    self.locals.pop();
                    add(&mut acc, q)?;
                    i = i.saturating_add(1);
                }
            }
            (Some(var), _, _) => {
                let values = self.resolve(var)?.quantities();
                for v in values {
                    self.locals.push((var.clone(), v.into()));
                    let q = self.eval(&n.body)?.expect_scalar()?;
                    self.locals.pop();
                    add(&mut acc, q)?;
                }
            }
            (None, _, _) => {
                for q in self.eval(&n.body)?.quantities() {
                    add(&mut acc, q)?;
                }
            }
        }
        Ok(acc.unwrap_or(Quantity::number(if sum { 0.0 } else { 1.0 })).into())
    }

    fn call(&mut self, func: Func, args: &[Value]) -> Result<Value, CalcError> {
        let function = func.name();
        match func {
            Func::Min | Func::Max | Func::Sum | Func::Mean | Func::Len => {
                let all: Vec<Quantity> = args.iter().flat_map(Value::quantities).collect();
                self.charge(all.len())?;
                if func == Func::Len {
                    return Ok(Quantity::number(all.len() as f64).into());
                }
                let (first, rest) = all.split_first().ok_or(CalcError::Arity { function: function.to_string(), expected: "at least 1 value" })?;
                let mut acc = *first;
                for &q in rest {
                    acc = match func {
                        Func::Sum | Func::Mean => acc.try_add(q)?,
                        _ => {
                            acc.same_dim(q, "compare")?;
                            let better = if func == Func::Min { q.value < acc.value } else { q.value > acc.value };
                            if better { q } else { acc }
                        }
                    };
                }
                if func == Func::Mean {
                    acc = acc.try_div(Quantity::number(all.len() as f64))?;
                }
                Ok(acc.into())
            }
            Func::Round => {
                let (x, digits) = match args {
                    [x] => (x, 0.0),
                    [x, n] => {
                        let n = plain(function, n.expect_scalar()?)?;
                        (x, n.round().clamp(-15.0, 15.0))
                    }
                    _ => return Err(CalcError::Arity { function: function.to_string(), expected: "1 or 2 arguments" }),
                };
                let scale = 10f64.powf(digits);
                self.map(x, |q| Ok(Quantity::number(finite((plain(function, q)? * scale).round() / scale)?)))
            }
            _ => {
                let [x] = args else {
                    return Err(CalcError::Arity { function: function.to_string(), expected: "1 argument" });
                };
                self.map(x, |q| unary(func, q))
            }
        }
    }

    /// `f` applied to a scalar, or to each element of a vector.
    fn map(&mut self, x: &Value, f: impl Fn(Quantity) -> Result<Quantity, CalcError>) -> Result<Value, CalcError> {
        match x {
            Value::Scalar(q) => Ok(f(*q)?.into()),
            Value::Vector(v) => {
                self.charge(v.len())?;
                let items = v.iter().map(f).collect::<Result<Vec<_>, _>>()?;
                Ok(Value::Vector(Vector::from_quantities(&items)?))
            }
        }
    }
}

/// A one-argument function of a quantity.
fn unary(func: Func, x: Quantity) -> Result<Quantity, CalcError> {
    let function = func.name();
    match func {
        Func::Sqrt => x.try_pow(Quantity::number(0.5)),
        Func::Abs => Ok(Quantity::new(x.value.abs(), x.dim)),
        _ => {
            let v = plain(function, x)?;
            let out = match func {
                Func::Exp => v.exp(),
                Func::Ln | Func::Log if v <= 0.0 => return Err(domain(function)),
                Func::Ln => v.ln(),
                Func::Log => v.log10(),
                Func::Sin => v.sin(),
                Func::Cos => v.cos(),
                Func::Tan => v.tan(),
                Func::Asin | Func::Acos if !(-1.0..=1.0).contains(&v) => return Err(domain(function)),
                Func::Asin => v.asin(),
                Func::Acos => v.acos(),
                _ => v.atan(),
            };
            Ok(Quantity::number(finite(out)?))
        }
    }
}

/// The value of a dimensionless argument, else an error naming `function`.
fn plain(function: &str, q: Quantity) -> Result<f64, CalcError> {
    if q.dim.is_dimensionless() { Ok(q.value) } else { Err(CalcError::NeedsDimensionless { function: function.to_string(), found: dim_name(q.dim) }) }
}

/// A dimensionless whole number (an index, a limit), else an error naming `what`.
fn whole(q: Quantity, what: &str) -> Result<i64, CalcError> {
    let v = plain(what, q)?;
    if v.fract() != 0.0 || v.abs() > 1e15 {
        return Err(CalcError::NotWhole { what: what.to_string() });
    }
    Ok(v as i64)
}

fn domain(function: &str) -> CalcError {
    CalcError::Domain { function: function.to_string() }
}
