//! Evaluation of expression trees against a scope.

use crate::CalcError;
use crate::parser::{BinOp, Expr, Func};
use crate::quantity::{Quantity, finite};
use crate::units::{constant, dim_name, unit};

/// The parameters defined so far in a worksheet, in definition order. Redefining a name updates
/// its value in place (it keeps its original position).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scope {
    entries: Vec<(String, Quantity)>,
}

impl Scope {
    /// An empty scope.
    pub fn new() -> Self {
        Scope::default()
    }

    /// The value of parameter `name`, if defined.
    pub fn get(&self, name: &str) -> Option<Quantity> {
        self.entries.iter().find(|(n, _)| n == name).map(|&(_, q)| q)
    }

    /// Defines or redefines `name`.
    pub fn set(&mut self, name: &str, value: Quantity) {
        match self.entries.iter_mut().find(|(n, _)| n == name) {
            Some(entry) => entry.1 = value,
            None => self.entries.push((name.to_string(), value)),
        }
    }

    /// Undefines `name`, returning its old value.
    pub fn remove(&mut self, name: &str) -> Option<Quantity> {
        let index = self.entries.iter().position(|(n, _)| n == name)?;
        Some(self.entries.remove(index).1)
    }

    /// The parameters in definition order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, Quantity)> {
        self.entries.iter().map(|(n, q)| (n.as_str(), *q))
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
    /// Parameter, then constant, then unit.
    Worksheet,
    /// Units only (the display unit after `=`).
    UnitsOnly,
}

/// Evaluates `expr`. Recursion is bounded by the parser's node and depth limits.
pub(crate) fn eval(expr: &Expr, scope: &Scope, names: Names) -> Result<Quantity, CalcError> {
    match expr {
        Expr::Num(v) => Ok(Quantity::number(finite(*v)?)),
        Expr::Name(name) => resolve(name, scope, names),
        Expr::Neg(inner) => Ok(eval(inner, scope, names)?.negate()),
        Expr::Bin(op, left, right) => {
            let a = eval(left, scope, names)?;
            let b = eval(right, scope, names)?;
            match op {
                BinOp::Add => a.try_add(b),
                BinOp::Sub => a.try_sub(b),
                BinOp::Mul => a.try_mul(b),
                BinOp::Div => a.try_div(b),
                BinOp::Pow => a.try_pow(b),
            }
        }
        Expr::Call(func, args) => {
            let values = args.iter().map(|a| eval(a, scope, names)).collect::<Result<Vec<_>, _>>()?;
            call(*func, &values)
        }
    }
}

fn resolve(name: &str, scope: &Scope, names: Names) -> Result<Quantity, CalcError> {
    match names {
        Names::Worksheet => scope.get(name).or_else(|| constant(name)).or_else(|| unit(name)).ok_or_else(|| match Func::from_name(name) {
            Some(_) => CalcError::FunctionNeedsArguments(name.to_string()),
            None => CalcError::UnknownName(name.to_string()),
        }),
        Names::UnitsOnly => unit(name).ok_or_else(|| CalcError::UnknownUnit(name.to_string())),
    }
}

fn call(func: Func, args: &[Quantity]) -> Result<Quantity, CalcError> {
    let function = func.name();
    match func {
        Func::Min | Func::Max => {
            let (first, rest) = args.split_first().ok_or(CalcError::Arity { function: function.to_string(), expected: "at least 1 argument" })?;
            let mut best = *first;
            for &q in rest {
                best.same_dim(q, "compare")?;
                let better = if func == Func::Min { q.value < best.value } else { q.value > best.value };
                if better {
                    best = q;
                }
            }
            Ok(best)
        }
        Func::Round => {
            let (x, digits) = match args {
                [x] => (*x, 0.0),
                [x, n] => {
                    plain(function, *n)?;
                    (*x, n.value.round().clamp(-15.0, 15.0))
                }
                _ => return Err(CalcError::Arity { function: function.to_string(), expected: "1 or 2 arguments" }),
            };
            let v = plain(function, x)?;
            let scale = 10f64.powf(digits);
            Ok(Quantity::number(finite((v * scale).round() / scale)?))
        }
        _ => {
            let [x] = args else {
                return Err(CalcError::Arity { function: function.to_string(), expected: "1 argument" });
            };
            let x = *x;
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
    }
}

/// The value of a dimensionless argument, else an error naming `function`.
fn plain(function: &str, q: Quantity) -> Result<f64, CalcError> {
    if q.dim.is_dimensionless() { Ok(q.value) } else { Err(CalcError::NeedsDimensionless { function: function.to_string(), found: dim_name(q.dim) }) }
}

fn domain(function: &str) -> CalcError {
    CalcError::Domain { function: function.to_string() }
}
