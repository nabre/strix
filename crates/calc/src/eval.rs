//! Evaluation of expression trees against a scope.

use crate::parser::{BinOp, Expr, Func, Nary};
use crate::quantity::{Dim, Quantity, finite};
use crate::units::{TEMPERATURE_DIM, affine, constant, dim_name, unit};
use crate::{CalcError, MAX_CALL_DEPTH, MAX_SHEET_STEPS, MAX_STEPS, MAX_VALUES};

/// The hint of a temperature difference (`T₂ − T₁`): shown in kelvin, never with the offset of
/// a `°C`/`°F` working unit.
pub(crate) const DIFFERENCE: &str = "K";

/// What a temperature expression is (`docs/calc.md`, Temperatures).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Temp {
    /// An absolute temperature written in `°C` or `°F` (the unit), or worked out from one.
    Absolute(String),
    /// A difference of absolute temperatures.
    Difference,
}

/// Deepest nesting of evaluations, counting into function bodies: one expression nests at most
/// [`MAX_NODES`](crate::MAX_NODES) deep, so this only stops functions calling each other with
/// deep bodies, before the stack runs out.
const MAX_EVAL_DEPTH: usize = 640;

/// A user function `f(x, y) = body`: its parameters shadow the worksheet's names in the body;
/// every other name is read from the worksheet when the function is called.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Function {
    pub name: String,
    pub params: Vec<String>,
    pub body: Expr,
}

/// A list of magnitudes (coherent SI units) sharing one dimension: a range, a vector literal or
/// a table column. At most [`MAX_VALUES`] long; every value is finite.
#[derive(Debug, Clone, PartialEq)]
pub struct Vector {
    /// Magnitudes in coherent SI units.
    pub values: Vec<f64>,
    /// Absolute SI uncertainties parallel to `values`; absent for an exact vector.
    pub uncertainties: Option<Vec<Option<f64>>>,
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
        let uncertainties = items.iter().any(|q| q.unc.is_some()).then(|| items.iter().map(|q| q.unc).collect());
        Ok(Vector { values, uncertainties, dim })
    }

    /// The elements as quantities.
    pub fn iter(&self) -> impl Iterator<Item = Quantity> + '_ {
        self.values.iter().enumerate().map(|(i, &v)| Quantity {
            value: v,
            dim: self.dim,
            unc: self.uncertainties.as_ref().and_then(|u| u.get(i)).copied().flatten(),
        })
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

/// Explicit physical domain constraint on an unknown (`t > 0`, `x ≥ 1 m`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DomainConstraint {
    pub var: String,
    pub op: char,
    pub bound: Quantity,
}

impl DomainConstraint {
    pub(crate) fn accepts(&self, q: Quantity) -> Result<bool, CalcError> {
        let b = if self.bound.value == 0.0 && self.bound.dim.is_dimensionless() {
            0.0
        } else {
            q.same_dim(self.bound, "compare")?;
            self.bound.value
        };
        let close = (q.value - b).abs() <= 1e-12 * q.value.abs().max(b.abs());
        Ok(match self.op {
            '≤' => q.value <= b || close,
            '≥' => q.value >= b || close,
            '<' => q.value < b && !close,
            '>' => q.value > b && !close,
            _ => !close,
        })
    }
}

/// The parameters defined so far in a worksheet, in definition order. Redefining a name updates
/// its value in place (it keeps its original position).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scope {
    entries: Vec<(String, Value)>,
    /// Where each name is in `entries`: a data table defines tens of thousands of cells, and
    /// every name an expression reads is looked up.
    index: std::collections::HashMap<String, usize>,
    /// User functions, in definition order.
    funcs: Vec<Function>,
    /// Working units: the last unit written for each dimension (`b = 300 mm` makes lengths show
    /// in mm), as its quantity and text.
    pub(crate) working: Vec<(Dim, Quantity, String)>,
    /// The unit hint each parameter is shown in (`α = 30°` → `°`, `T = 20 °C` → `°C`).
    pub(crate) hints: Vec<(String, String)>,
    /// Physical domain constraints declared before an equation (`t > 0`).
    pub(crate) constraints: Vec<DomainConstraint>,
    /// Work done by every evaluation against this scope (one worksheet), capped by
    /// [`MAX_SHEET_STEPS`]: however many regions and cells a document has, recalculating it ends.
    spent: std::cell::Cell<usize>,
    pub(crate) uncertainty: crate::UncertaintyMode,
}

impl Scope {
    /// An empty scope.
    pub fn new() -> Self {
        Scope::default()
    }

    /// The value of parameter `name`, if defined.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.entries.get(*self.index.get(name)?).map(|(_, v)| v)
    }

    /// Defines or redefines `name`.
    pub fn set(&mut self, name: &str, value: impl Into<Value>) {
        let value = value.into();
        match self.index.get(name).and_then(|&i| self.entries.get_mut(i)) {
            Some(entry) => entry.1 = value,
            None => {
                self.index.insert(name.to_string(), self.entries.len());
                self.entries.push((name.to_string(), value));
            }
        }
    }

    /// Undefines `name`, returning its old value.
    pub fn remove(&mut self, name: &str) -> Option<Value> {
        let index = self.index.remove(name)?;
        if index >= self.entries.len() {
            return None;
        }
        let (_, old) = self.entries.remove(index);
        // The names after it moved down one.
        for (n, _) in self.entries.iter().skip(index) {
            if let Some(i) = self.index.get_mut(n) {
                *i = i.saturating_sub(1);
            }
        }
        Some(old)
    }

    /// Defines or redefines a user function.
    pub(crate) fn set_function(&mut self, f: Function) {
        match self.funcs.iter_mut().find(|g| g.name == f.name) {
            Some(g) => *g = f,
            None => self.funcs.push(f),
        }
    }

    /// Undefines the user function `name`.
    pub(crate) fn remove_function(&mut self, name: &str) {
        self.funcs.retain(|f| f.name != name);
    }

    /// The user function called `name`, if defined.
    pub(crate) fn function(&self, name: &str) -> Option<&Function> {
        self.funcs.iter().find(|f| f.name == name)
    }

    /// The user functions in definition order: name and parameters.
    pub fn functions(&self) -> impl Iterator<Item = (&str, &[String])> {
        self.funcs.iter().map(|f| (f.name.as_str(), f.params.as_slice()))
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

    pub(crate) fn add_constraint(&mut self, c: DomainConstraint) {
        if let Some(existing) = self.constraints.iter_mut().find(|e| e.var == c.var && e.op == c.op) {
            *existing = c;
        } else {
            self.constraints.push(c);
        }
    }

    pub(crate) fn constraints_for<'a>(&'a self, var: &'a str) -> impl Iterator<Item = &'a DomainConstraint> {
        self.constraints.iter().filter(move |c| c.var == var)
    }

    /// Whether `e` is an absolute temperature in `°C`/`°F` or a difference of them, from how it
    /// is written.
    pub(crate) fn temp_kind(&self, e: &Expr, depth: usize) -> Option<Temp> {
        if depth > 64 {
            return None;
        }
        if let Some(u) = self.affine_literal(e) {
            return Some(Temp::Absolute(u));
        }
        match e {
            Expr::Name(n) => {
                if self.get(n).map(Value::dim) != Some(TEMPERATURE_DIM) {
                    return None;
                }
                let (_, h) = self.hints.iter().find(|(m, _)| m == n)?;
                if affine(h).is_some() { Some(Temp::Absolute(h.clone())) } else { (h == DIFFERENCE).then_some(Temp::Difference) }
            }
            Expr::Neg(a) if self.affine_literal(a).is_some() => self.temp_kind(a, depth + 1),
            Expr::Neg(a) => self.temp_kind(a, depth + 1).filter(|k| *k == Temp::Difference),
            Expr::Bin(op, a, b) => self.temp_bin(*op, a, b, depth + 1),
            Expr::Index(a, _) => self.temp_kind(a, depth + 1),
            Expr::Vector(items) => self.common_temp(items, depth + 1),
            Expr::Call(f, args) if f.is_rounding() => self.rounding_temp(args, depth + 1),
            Expr::Call(Func::Measure | Func::RelativeMeasure | Func::Val | Func::Min | Func::Max | Func::Mean | Func::Median | Func::Abs, args) => {
                self.common_temp(args.get(..1).unwrap_or_default(), depth + 1)
            }
            Expr::Call(Func::Stdev | Func::Stdevp, args) => self.common_temp(args, depth + 1).map(|_| Temp::Difference),
            _ => None,
        }
    }

    fn affine_literal(&self, x: &Expr) -> Option<String> {
        match x {
            Expr::Bin(BinOp::Mul, _, u) => match &**u {
                Expr::Unit(n) => affine(n).map(|_| n.clone()),
                Expr::Name(n) if self.get(n).is_none() => affine(n).map(|_| n.clone()),
                _ => None,
            },
            _ => None,
        }
    }

    fn temp_bin(&self, op: BinOp, a: &Expr, b: &Expr, depth: usize) -> Option<Temp> {
        let (ka, kb) = (self.temp_kind(a, depth), self.temp_kind(b, depth));
        match op {
            BinOp::Sub => match (ka, kb) {
                (Some(Temp::Absolute(_)), Some(Temp::Absolute(_))) => Some(Temp::Difference),
                (Some(Temp::Absolute(u)), _) => Some(Temp::Absolute(u)),
                (Some(Temp::Difference), _) | (None, Some(Temp::Difference)) => Some(Temp::Difference),
                _ => None,
            },
            BinOp::Add => match (ka, kb) {
                (Some(Temp::Absolute(_)), Some(Temp::Absolute(_))) => None,
                (Some(Temp::Absolute(u)), _) | (_, Some(Temp::Absolute(u))) => Some(Temp::Absolute(u)),
                (Some(Temp::Difference), _) | (_, Some(Temp::Difference)) => Some(Temp::Difference),
                _ => None,
            },
            BinOp::Mul => match (ka, kb) {
                (Some(Temp::Difference), None) | (None, Some(Temp::Difference)) => Some(Temp::Difference),
                _ => None,
            },
            BinOp::Div => match (ka, kb) {
                (Some(Temp::Difference), None) => Some(Temp::Difference),
                _ => None,
            },
            BinOp::Pow => None,
        }
    }

    fn common_temp(&self, items: &[Expr], depth: usize) -> Option<Temp> {
        let mut found: Option<Temp> = None;
        for item in items {
            match (self.temp_kind(item, depth), &found) {
                (Some(k), Some(f)) if k != *f => return None,
                (Some(k), None) => found = Some(k),
                (None, _) => return None,
                _ => {}
            }
        }
        found
    }

    fn rounding_temp(&self, args: &[Expr], depth: usize) -> Option<Temp> {
        if let Some(u_expr) = args.get(2) {
            let u_text = crate::format::render(u_expr);
            if affine(&u_text).is_some() {
                return match self.temp_kind(args.first()?, depth) {
                    Some(Temp::Difference) => Some(Temp::Difference),
                    _ => Some(Temp::Absolute(u_text)),
                };
            }
            if u_text == DIFFERENCE {
                return None;
            }
        }
        self.common_temp(args.get(..1).unwrap_or_default(), depth)
    }

    /// The dimensionless unit a plain-number result of `e` is shown in (`°`, `%`, `‰`, `gon`).
    pub(crate) fn dimensionless_hint(&self, e: &Expr) -> Option<String> {
        match e {
            Expr::Num(_) | Expr::Range(..) | Expr::Bin(BinOp::Pow, ..) => None,
            Expr::Unit(n) => unit(n).filter(|q| q.dim.is_dimensionless()).map(|_| n.clone()),
            Expr::Name(n)
                if let Some((_, h)) = self.hints.iter().find(|(m, _)| m == n).filter(|(_, h)| unit(h).is_some_and(|q| q.dim.is_dimensionless())) =>
            {
                Some(h.clone())
            }
            Expr::Name(n) if self.get(n).is_some() || constant(n).is_some() => None,
            Expr::Name(n) => unit(n).filter(|q| q.dim.is_dimensionless()).map(|_| n.clone()),
            Expr::Neg(a) | Expr::Index(a, _) => self.dimensionless_hint(a),
            Expr::Bin(op @ (BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div), a, b) => self.dimless_bin(*op, a, b),
            Expr::Call(Func::Asin | Func::Acos | Func::Atan | Func::Atan2, _) => Some("°".to_string()),
            Expr::Call(f, args) if f.is_rounding() => self.rounding_dimless(args),
            Expr::Call(
                Func::Measure
                | Func::RelativeMeasure
                | Func::Val
                | Func::Unc
                | Func::Min
                | Func::Max
                | Func::Abs
                | Func::Sum
                | Func::Mean
                | Func::Median
                | Func::Stdev
                | Func::Stdevp
                | Func::Mod,
                args,
            ) => self.common_dimless(args.get(..1).unwrap_or_default()),
            Expr::Call(..) | Expr::Apply(..) => None,
            Expr::Vector(items) => self.common_dimless(items),
            Expr::Nary(n) if n.op == '∑' => self.dimensionless_hint(&n.body),
            Expr::Nary(_) => None,
        }
    }

    fn dimless_bin(&self, op: BinOp, a: &Expr, b: &Expr) -> Option<String> {
        match (op, self.dimensionless_hint(a), self.dimensionless_hint(b)) {
            (BinOp::Add | BinOp::Sub, Some(x), Some(y)) if x == y => Some(x),
            (BinOp::Add | BinOp::Sub, Some(x), None) | (BinOp::Add | BinOp::Sub, None, Some(x)) => Some(x),
            (BinOp::Mul, Some(x), None) | (BinOp::Mul, None, Some(x)) => Some(x),
            (BinOp::Div, Some(x), None) => Some(x),
            _ => None,
        }
    }

    fn common_dimless(&self, items: &[Expr]) -> Option<String> {
        let mut found: Option<String> = None;
        for item in items {
            match (self.dimensionless_hint(item), &found) {
                (Some(h), Some(f)) if h != *f => return None,
                (Some(h), None) => found = Some(h),
                _ => {}
            }
        }
        found
    }

    fn rounding_dimless(&self, args: &[Expr]) -> Option<String> {
        if let Some(u_expr) = args.get(2) {
            let u_text = crate::format::render(u_expr);
            if unit(&u_text).is_some_and(|q| q.dim.is_dimensionless()) {
                return Some(u_text);
            }
            if eval_scalar(u_expr, self, Names::UnitsOnly).is_ok_and(|q| q.dim.is_dimensionless()) {
                return Some(u_text);
            }
            return None;
        }
        self.common_dimless(args.get(..1).unwrap_or_default())
    }

    /// Direct written unit expression on a literal quantity (`2.34 cm`, `-5 km`, `[1; 2] mm`).
    pub(crate) fn direct_unit_expr<'e>(&self, e: &'e Expr) -> Option<&'e Expr> {
        match e {
            Expr::Neg(inner) => self.direct_unit_expr(inner),
            Expr::Bin(BinOp::Mul, _, u) if Self::is_unit_tree(u, self) => Some(u),
            _ => None,
        }
    }

    fn is_unit_tree(e: &Expr, scope: &Scope) -> bool {
        match e {
            Expr::Unit(_) => true,
            Expr::Name(n) => scope.get(n).is_none() && constant(n).is_none() && unit(n).is_some(),
            Expr::Bin(BinOp::Mul | BinOp::Div, l, r) => Self::is_unit_tree(l, scope) && Self::is_unit_tree(r, scope),
            Expr::Bin(BinOp::Pow, base, _) => Self::is_unit_tree(base, scope),
            _ => false,
        }
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
    /// Evaluations under way, nested (bounded by [`MAX_EVAL_DEPTH`]).
    depth: usize,
    /// User function calls under way, nested (bounded by [`MAX_CALL_DEPTH`]).
    calls: usize,
    /// In a calculated table's row: its columns' values over the data rows, so `x[k]` reaches
    /// column `x` in data row `k` (cyclic: row `n + 1` is row 1, row 0 is row `n`).
    columns: Vec<(String, Vec<Option<Quantity>>)>,
}

impl<'a> Ctx<'a> {
    pub(crate) fn new(scope: &'a Scope, names: Names, locals: Vec<(String, Value)>) -> Self {
        Ctx { scope, names, locals, steps: 0, depth: 0, calls: 0, columns: Vec::new() }
    }

    /// In a calculated table's row: the table's columns, which `x[k]` indexes by row.
    pub(crate) fn with_columns(mut self, columns: Vec<(String, Vec<Option<Quantity>>)>) -> Self {
        self.columns = columns;
        self
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
        if self.depth >= MAX_EVAL_DEPTH {
            return Err(CalcError::TooDeep { limit: MAX_CALL_DEPTH });
        }
        self.depth += 1;
        let result = self.eval_node(expr);
        self.depth -= 1;
        result
    }

    fn eval_node(&mut self, expr: &Expr) -> Result<Value, CalcError> {
        self.charge(1)?;
        match expr {
            Expr::Num(v) => Ok(Quantity::number(finite(*v)?).into()),
            Expr::Name(name) => self.resolve(name),
            Expr::Unit(name) => unit(name).map(Value::Scalar).ok_or_else(|| CalcError::UnknownUnit(name.clone())),
            // `-20 °C` is twenty degrees below zero, not minus 293.15 K.
            Expr::Neg(inner)
                if let Expr::Bin(BinOp::Mul, number, u) = &**inner
                    && let Some(scale) = self.affine_unit(u) =>
            {
                let v = self.eval(number)?;
                self.absolute(negate(v), scale, u)
            }
            Expr::Neg(inner) => Ok(negate(self.eval(inner)?)),
            // A number written in °C or °F is an absolute temperature: `20 °C` is 293.15 K.
            Expr::Bin(BinOp::Mul, number, u) if let Some(scale) = self.affine_unit(u) => {
                let v = self.eval(number)?;
                self.absolute(v, scale, u)
            }
            Expr::Bin(op, left, right) => {
                let a = self.eval(left)?;
                let b = self.eval(right)?;
                self.binary(*op, a, b)
            }
            Expr::Call(Func::Measure, args) if args.len() == 2 => {
                let a = args.first().ok_or_else(|| domain("measure"))?;
                let b = args.get(1).ok_or_else(|| domain("measure"))?;
                let x = self.eval(a)?;
                let delta = match b {
                    Expr::Bin(BinOp::Mul, n, u) if self.affine_unit(u).is_some() => {
                        let number = self.eval(n)?;
                        let unit = self.eval(u)?;
                        self.binary(BinOp::Mul, number, unit)?
                    }
                    _ => self.eval(b)?,
                };
                self.call(Func::Measure, &[x, delta])
            }
            Expr::Call(func, args) if func.is_rounding() => self.eval_rounding(*func, args),
            Expr::Call(func, args) => {
                let values = args.iter().map(|a| self.eval(a)).collect::<Result<Vec<_>, _>>()?;
                self.call(*func, &values)
            }
            Expr::Apply(name, args) => {
                let values = args.iter().map(|a| self.eval(a)).collect::<Result<Vec<_>, _>>()?;
                self.apply(name, values)
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
            // A table column indexed by row (`x[i+1]`): cyclic over the data rows.
            Expr::Index(base, index)
                if let Expr::Name(name) = &**base
                    && self.columns.iter().any(|(n, _)| n == name) =>
            {
                let k = whole(self.eval(index)?.expect_scalar()?, "a row")?;
                let column = self.columns.iter().find(|(n, _)| n == name).map(|(_, c)| c.as_slice()).unwrap_or_default();
                let len = i64::try_from(column.len()).unwrap_or(i64::MAX);
                if len == 0 {
                    return Err(CalcError::MissingValue(name.clone()));
                }
                let row = usize::try_from((k - 1).rem_euclid(len)).unwrap_or(0);
                column.get(row).copied().flatten().map(Value::Scalar).ok_or_else(|| CalcError::MissingValue(format!("{name}[{k}]")))
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
                Ok(Value::Vector(Vector { values, uncertainties: None, dim: Dim::NONE }))
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

    /// The scale of `u` when it is `°C` or `°F` written as a unit (not a parameter of that name).
    fn affine_unit(&self, u: &Expr) -> Option<(f64, f64)> {
        match u {
            Expr::Unit(n) => affine(n),
            Expr::Name(n) if self.locals.iter().all(|(m, _)| m != n) && self.scope.get(n).is_none() => affine(n),
            _ => None,
        }
    }

    /// The absolute temperature `v` °C (or °F) in kelvin; a value with a unit is multiplied by
    /// the degree as by any unit.
    fn absolute(&mut self, v: Value, (scale, offset): (f64, f64), u: &Expr) -> Result<Value, CalcError> {
        if !v.dim().is_dimensionless() {
            let u = self.eval(u)?;
            return self.binary(BinOp::Mul, v, u);
        }
        let dim = crate::units::TEMPERATURE_DIM;
        let kelvin = |x: f64| finite(x * scale + offset);
        Ok(match v {
            Value::Scalar(q) => Value::Scalar(Quantity { value: kelvin(q.value)?, dim, unc: q.unc.map(|u| finite(u * scale.abs())).transpose()? }),
            Value::Vector(v) => {
                self.charge(v.len())?;
                Value::Vector(Vector::from_quantities(
                    &v.iter()
                        .map(|q| Ok(Quantity { value: kelvin(q.value)?, dim, unc: q.unc.map(|u| finite(u * scale.abs())).transpose()? }))
                        .collect::<Result<Vec<_>, CalcError>>()?,
                )?)
            }
        })
    }

    /// A call of the user function `name`: its body with the parameters bound to `args`, and no
    /// other locals (a `∑` variable or a table column of the caller is not seen inside).
    fn apply(&mut self, name: &str, args: Vec<Value>) -> Result<Value, CalcError> {
        if self.names == Names::UnitsOnly {
            return Err(CalcError::UnknownUnit(name.to_string()));
        }
        let scope = self.scope;
        let f = scope.function(name).ok_or_else(|| CalcError::UnknownName(name.to_string()))?;
        if f.params.len() != args.len() {
            return Err(CalcError::ArgCount { function: name.to_string(), expected: f.params.len(), found: args.len() });
        }
        if self.calls >= MAX_CALL_DEPTH {
            return Err(CalcError::TooDeep { limit: MAX_CALL_DEPTH });
        }
        let locals = f.params.iter().cloned().zip(args).collect();
        let outer = std::mem::replace(&mut self.locals, locals);
        self.calls += 1;
        let result = self.eval(&f.body);
        self.calls -= 1;
        self.locals = outer;
        result
    }

    /// `a op b`, element by element when either side is a vector (vectors must be as long).
    fn binary(&mut self, op: BinOp, a: Value, b: Value) -> Result<Value, CalcError> {
        let mode = self.scope.uncertainty;
        let apply = |x: Quantity, y: Quantity| binary_quantity(op, x, y, mode);
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
        let mode = self.scope.uncertainty;
        let add = |acc: &mut Option<Quantity>, q: Quantity| -> Result<(), CalcError> {
            *acc = Some(match *acc {
                None => q,
                Some(a) => binary_quantity(if sum { BinOp::Add } else { BinOp::Mul }, a, q, mode)?,
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
        if !matches!(
            func,
            Func::Measure
                | Func::RelativeMeasure
                | Func::Val
                | Func::Unc
                | Func::Relunc
                | Func::Min
                | Func::Max
                | Func::Sum
                | Func::Mean
                | Func::Len
                | Func::Sqrt
                | Func::Cbrt
                | Func::Abs
                | Func::Exp
                | Func::Ln
                | Func::Log
                | Func::Log2
                | Func::Sin
                | Func::Cos
                | Func::Tan
                | Func::Asin
                | Func::Acos
                | Func::Atan
                | Func::Sinh
                | Func::Cosh
                | Func::Tanh
                | Func::Asinh
                | Func::Acosh
                | Func::Atanh
        ) && args.iter().flat_map(Value::quantities).any(|q| q.unc.is_some())
        {
            return Err(domain("uncertainty propagation: unsupported function; use val() explicitly"));
        }
        match func {
            Func::Measure | Func::RelativeMeasure => {
                let [v, u] = args else {
                    return Err(CalcError::Arity { function: function.into(), expected: "2 arguments" });
                };
                let q = v.expect_scalar()?;
                let d = u.expect_scalar()?;
                if d.unc.is_some() {
                    return Err(domain(function));
                }
                let uncertainty = if func == Func::RelativeMeasure {
                    if !d.dim.is_dimensionless() {
                        return Err(domain(function));
                    }
                    finite(q.value.abs() * d.value)?
                } else {
                    q.same_dim(d, "uncertainty")?;
                    d.value
                };
                Ok(q.with_uncertainty(uncertainty)?.into())
            }
            Func::Val | Func::Unc | Func::Relunc => {
                let [v] = args else {
                    return Err(CalcError::Arity { function: function.into(), expected: "1 argument" });
                };
                self.map(v, |q| match func {
                    Func::Val => Ok(Quantity::new(q.value, q.dim)),
                    Func::Unc => Ok(Quantity::new(q.unc.unwrap_or(0.0), q.dim)),
                    _ if q.value == 0.0 => Err(CalcError::DivisionByZero),
                    _ => Ok(Quantity::number(finite(q.unc.unwrap_or(0.0) / q.value.abs())?)),
                })
            }
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
                        Func::Sum | Func::Mean => self.binary(BinOp::Add, acc.into(), q.into())?.expect_scalar()?,
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
            Func::Median | Func::Var | Func::Varp | Func::Stdev | Func::Stdevp => self.eval_stats(func, args),
            Func::Slope | Func::Intercept | Func::R2 => self.eval_regression(func, args),
            Func::Gcd | Func::Lcm => self.eval_int_agg(func, args),
            Func::Root => self.eval_root(args),
            Func::Log if args.len() != 1 => {
                if args.iter().flat_map(Value::quantities).any(|q| q.unc.is_some()) {
                    return Err(domain("uncertain logarithm base"));
                }
                self.eval_log_base(args)
            }
            Func::Atan2 | Func::Mod | Func::Binom => self.eval_binary_func(func, args),
            _ => {
                let [x] = args else {
                    return Err(CalcError::Arity { function: function.to_string(), expected: "1 argument" });
                };
                self.map(x, |q| unary(func, q))
            }
        }
    }

    fn eval_stats(&mut self, func: Func, args: &[Value]) -> Result<Value, CalcError> {
        let function = func.name();
        let all: Vec<Quantity> = args.iter().flat_map(Value::quantities).collect();
        self.charge(all.len())?;
        let sample = matches!(func, Func::Var | Func::Stdev);
        let min_len = if sample { 2 } else { 1 };
        if all.len() < min_len {
            let expected = if sample { "at least 2 values" } else { "at least 1 value" };
            return Err(CalcError::Arity { function: function.to_string(), expected });
        }
        let first = all.first().copied().unwrap_or(Quantity::number(0.0));
        for &q in all.iter().skip(1) {
            first.same_dim(q, "compare")?;
        }
        if func == Func::Median {
            let mut vals: Vec<f64> = all.iter().map(|q| q.value).collect();
            vals.sort_by(f64::total_cmp);
            let mid = vals.len() / 2;
            let med = if vals.len() % 2 == 1 {
                vals.get(mid).copied().unwrap_or(0.0)
            } else {
                0.5 * (vals.get(mid.saturating_sub(1)).copied().unwrap_or(0.0) + vals.get(mid).copied().unwrap_or(0.0))
            };
            return Ok(Quantity::new(finite(med)?, first.dim).into());
        }
        let n = all.len() as f64;
        let mean = finite(all.iter().map(|q| q.value).sum::<f64>() / n)?;
        let mut ss = 0.0;
        for &q in &all {
            let d = q.value - mean;
            ss = finite(ss + d * d)?;
        }
        let denom = if sample { (all.len() - 1) as f64 } else { n };
        let variance = finite(ss / denom)?;
        match func {
            Func::Var | Func::Varp => {
                let dim = first.dim.try_mul(first.dim)?;
                Ok(Quantity::new(variance, dim).into())
            }
            _ => Ok(Quantity::new(finite(variance.sqrt())?, first.dim).into()),
        }
    }

    fn eval_regression(&mut self, func: Func, args: &[Value]) -> Result<Value, CalcError> {
        let function = func.name();
        let [vx, vy] = args else {
            return Err(CalcError::Arity { function: function.to_string(), expected: "2 arguments" });
        };
        let (xs, ys) = (vx.quantities(), vy.quantities());
        self.charge(xs.len().saturating_add(ys.len()))?;
        if xs.len() != ys.len() {
            return Err(CalcError::LengthMismatch { left: xs.len(), right: ys.len() });
        }
        if xs.len() < 2 {
            return Err(CalcError::Arity { function: function.to_string(), expected: "at least 2 pairs" });
        }
        let (fx, fy) = (xs.first().copied().unwrap_or(Quantity::number(0.0)), ys.first().copied().unwrap_or(Quantity::number(0.0)));
        for &q in xs.iter().skip(1) {
            fx.same_dim(q, "compare")?;
        }
        for &q in ys.iter().skip(1) {
            fy.same_dim(q, "compare")?;
        }
        let n = xs.len() as f64;
        let mx = finite(xs.iter().map(|q| q.value).sum::<f64>() / n)?;
        let my = finite(ys.iter().map(|q| q.value).sum::<f64>() / n)?;
        let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
        for (qx, qy) in xs.iter().zip(&ys) {
            let (dx, dy) = (qx.value - mx, qy.value - my);
            sxx = finite(sxx + dx * dx)?;
            syy = finite(syy + dy * dy)?;
            sxy = finite(sxy + dx * dy)?;
        }
        if sxx == 0.0 {
            return Err(CalcError::DivisionByZero);
        }
        let m = finite(sxy / sxx)?;
        match func {
            Func::Slope => Ok(Quantity::new(m, fy.dim.try_div(fx.dim)?).into()),
            Func::Intercept => {
                let _ = fy.dim.try_div(fx.dim)?;
                Ok(Quantity::new(finite(my - m * mx)?, fy.dim).into())
            }
            _ => {
                if syy == 0.0 {
                    return Err(CalcError::DivisionByZero);
                }
                let r_sq = ((sxy / sxx) * (sxy / syy)).clamp(0.0, 1.0);
                Ok(Quantity::number(finite(r_sq)?).into())
            }
        }
    }

    fn eval_int_agg(&mut self, func: Func, args: &[Value]) -> Result<Value, CalcError> {
        let function = func.name();
        let all: Vec<Quantity> = args.iter().flat_map(Value::quantities).collect();
        self.charge(all.len())?;
        if all.len() < 2 {
            return Err(CalcError::Arity { function: function.to_string(), expected: "at least 2 values" });
        }
        let mut acc = u128::from(whole(*all.first().unwrap_or(&Quantity::number(0.0)), function)?.unsigned_abs());
        for &q in all.iter().skip(1) {
            let b = u128::from(whole(q, function)?.unsigned_abs());
            acc = if func == Func::Gcd {
                gcd_u128(acc, b)
            } else if acc == 0 || b == 0 {
                0
            } else {
                let g = gcd_u128(acc, b);
                (acc / g).checked_mul(b).ok_or(CalcError::Overflow)?
            };
        }
        Ok(Quantity::number(finite(acc as f64)?).into())
    }

    fn eval_root(&mut self, args: &[Value]) -> Result<Value, CalcError> {
        let [x, n_val] = args else {
            return Err(CalcError::Arity { function: "root".to_string(), expected: "2 arguments" });
        };
        let n = plain("root", n_val.expect_scalar()?)?;
        if n == 0.0 {
            return Err(domain("root"));
        }
        let whole_n = (n.fract() == 0.0 && n.abs() <= 1e9).then_some(n as i64);
        self.map(x, |q| {
            let dim = match whole_n {
                Some(k) => q.dim.try_root(k)?,
                None => q.dim.try_pow(1.0 / n)?,
            };
            if q.value == 0.0 && n < 0.0 {
                return Err(CalcError::DivisionByZero);
            }
            let mag = if q.value < 0.0 {
                let Some(k) = whole_n.filter(|k| k % 2 != 0) else {
                    return Err(domain("root"));
                };
                -((-q.value).powf(1.0 / k as f64))
            } else {
                q.value.powf(1.0 / n)
            };
            Ok(Quantity::new(finite(mag)?, dim))
        })
    }

    fn eval_log_base(&mut self, args: &[Value]) -> Result<Value, CalcError> {
        let [x, b_val] = args else {
            return Err(CalcError::Arity { function: "log".to_string(), expected: "1 or 2 arguments" });
        };
        let base = plain("log", b_val.expect_scalar()?)?;
        if base <= 0.0 || base == 1.0 {
            return Err(domain("log"));
        }
        self.map(x, |q| {
            let v = plain("log", q)?;
            if v <= 0.0 {
                return Err(domain("log"));
            }
            Ok(Quantity::number(finite(v.log(base))?))
        })
    }

    fn eval_binary_func(&mut self, func: Func, args: &[Value]) -> Result<Value, CalcError> {
        let function = func.name();
        let [a, b] = args else {
            return Err(CalcError::Arity { function: function.to_string(), expected: "2 arguments" });
        };
        let step = |x: Quantity, y: Quantity| -> Result<Quantity, CalcError> {
            match func {
                Func::Atan2 => {
                    x.same_dim(y, "compare")?;
                    Ok(Quantity::number(finite(x.value.atan2(y.value))?))
                }
                Func::Mod => {
                    x.same_dim(y, "subtract")?;
                    if y.value == 0.0 {
                        return Err(CalcError::DivisionByZero);
                    }
                    let q = (x.value / y.value).floor();
                    Ok(Quantity::new(finite(x.value - y.value * q)?, x.dim))
                }
                _ => binom_qty(x, y),
            }
        };
        match (a, b) {
            (Value::Scalar(x), Value::Scalar(y)) => Ok(step(*x, *y)?.into()),
            (Value::Vector(u), Value::Scalar(y)) => {
                self.charge(u.len())?;
                let items = u.iter().map(|x| step(x, *y)).collect::<Result<Vec<_>, _>>()?;
                Ok(Value::Vector(Vector::from_quantities(&items)?))
            }
            (Value::Scalar(x), Value::Vector(v)) => {
                self.charge(v.len())?;
                let items = v.iter().map(|y| step(*x, y)).collect::<Result<Vec<_>, _>>()?;
                Ok(Value::Vector(Vector::from_quantities(&items)?))
            }
            (Value::Vector(u), Value::Vector(v)) => {
                if u.len() != v.len() {
                    return Err(CalcError::LengthMismatch { left: u.len(), right: v.len() });
                }
                self.charge(u.len())?;
                let items = u.iter().zip(v.iter()).map(|(x, y)| step(x, y)).collect::<Result<Vec<_>, _>>()?;
                Ok(Value::Vector(Vector::from_quantities(&items)?))
            }
        }
    }

    fn eval_rounding(&mut self, func: Func, args: &[Expr]) -> Result<Value, CalcError> {
        let function = func.name();
        let (x_expr, places, unit_arg) = match args {
            [x] => (x, 0, None),
            [x, n] => {
                let n = plain(function, self.eval(n)?.expect_scalar()?)?;
                (x, n.round().clamp(-15.0, 15.0) as i32, None)
            }
            [x, n, u] => {
                let n = plain(function, self.eval(n)?.expect_scalar()?)?;
                (x, n.round().clamp(-15.0, 15.0) as i32, Some(u))
            }
            _ => return Err(CalcError::Arity { function: function.to_string(), expected: "1 to 3 arguments" }),
        };
        let x_val = self.eval(x_expr)?;
        let (uq, affine_offset) = self.rounding_unit(x_expr, &x_val, unit_arg)?;
        let mode = match func {
            Func::Floor => crate::format::RoundMode::Floor,
            Func::Ceil => crate::format::RoundMode::Ceil,
            Func::Trunc => crate::format::RoundMode::Trunc,
            _ => crate::format::RoundMode::HalfAwayFromZero,
        };
        self.map(&x_val, |q| {
            let v = match (uq, affine_offset) {
                (Some(u), Some(off)) => crate::format::step_decimal((q.value - off) / u.value, places, mode) * u.value + off,
                (Some(u), None) => crate::format::step_decimal(q.value / u.value, places, mode) * u.value,
                (None, _) => crate::format::step_decimal(q.value, places, mode),
            };
            Ok(Quantity { value: finite(v)?, ..q })
        })
    }

    fn rounding_unit(&mut self, x_expr: &Expr, x_val: &Value, unit_arg: Option<&Expr>) -> Result<(Option<Quantity>, Option<f64>), CalcError> {
        if let Some(u_expr) = unit_arg {
            let u_text = crate::format::render(u_expr);
            let mut uctx = Ctx::new(self.scope, Names::UnitsOnly, Vec::new()).with_steps(self.steps);
            let uq = uctx.eval(u_expr)?.expect_scalar()?;
            self.steps = uctx.steps();
            if uq.dim != x_val.dim() {
                return Err(CalcError::WrongUnit { result: dim_name(x_val.dim()), requested: u_text });
            }
            if uq.value == 0.0 {
                return Err(CalcError::DivisionByZero);
            }
            let off = affine(&u_text).filter(|_| self.scope.temp_kind(x_expr, 0) != Some(Temp::Difference)).map(|(_, o)| o);
            return Ok((Some(uq), off));
        }
        let dim = x_val.dim();
        if dim.is_dimensionless() {
            let uq = self.scope.dimensionless_hint(x_expr).and_then(|h| unit(&h));
            return Ok((uq, None));
        }
        if dim == TEMPERATURE_DIM
            && let Some(Temp::Absolute(h)) = self.scope.temp_kind(x_expr, 0)
        {
            return Ok((unit(&h), affine(&h).map(|(_, o)| o)));
        }
        if let Some(u_expr) = self.scope.direct_unit_expr(x_expr) {
            let mut uctx = Ctx::new(self.scope, Names::UnitsOnly, Vec::new()).with_steps(self.steps);
            if let Ok(uq) = uctx.eval(u_expr).and_then(|v| v.expect_scalar()) {
                self.steps = uctx.steps();
                if uq.dim == dim && uq.value > 0.0 {
                    return Ok((Some(uq), None));
                }
            }
        }
        let working = self.scope.working.iter().find(|(d, _, t)| *d == dim && affine(t).is_none()).map(|(_, q, _)| *q);
        Ok((working, None))
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

/// The opposite value.
fn negate(v: Value) -> Value {
    match v {
        Value::Scalar(q) => Value::Scalar(q.negate()),
        Value::Vector(mut v) => {
            v.values.iter_mut().for_each(|x| *x = -*x);
            Value::Vector(v)
        }
    }
}

/// A one-argument function of a quantity.
fn unary(func: Func, x: Quantity) -> Result<Quantity, CalcError> {
    let mut out = unary_exact(func, Quantity { unc: None, ..x })?;
    if let Some(u) = x.unc {
        let v = x.value;
        let derivative = match func {
            Func::Sqrt => 0.5 / v.sqrt(),
            Func::Cbrt => 1.0 / (3.0 * v.cbrt().powi(2)),
            Func::Abs => 1.0,
            Func::Exp => v.exp(),
            Func::Ln => 1.0 / v,
            Func::Log => 1.0 / (v * 10_f64.ln()),
            Func::Log2 => 1.0 / (v * 2_f64.ln()),
            Func::Sin => v.cos(),
            Func::Cos => -v.sin(),
            Func::Tan => 1.0 / v.cos().powi(2),
            Func::Asin => 1.0 / (1.0 - v * v).sqrt(),
            Func::Acos => -1.0 / (1.0 - v * v).sqrt(),
            Func::Atan => 1.0 / (1.0 + v * v),
            Func::Sinh => v.cosh(),
            Func::Cosh => v.sinh(),
            Func::Tanh => 1.0 / v.cosh().powi(2),
            Func::Asinh => 1.0 / (v * v + 1.0).sqrt(),
            Func::Acosh => 1.0 / (v * v - 1.0).sqrt(),
            Func::Atanh => 1.0 / (1.0 - v * v),
            _ => return Err(domain("uncertainty propagation: unsupported function; use val() explicitly")),
        };
        out.unc = Some(if u == 0.0 { 0.0 } else { finite((u * derivative).abs())? });
    }
    Ok(out)
}

fn unary_exact(func: Func, x: Quantity) -> Result<Quantity, CalcError> {
    let function = func.name();
    match func {
        Func::Sqrt => x.try_pow(Quantity::number(0.5)),
        Func::Cbrt => {
            let dim = x.dim.try_root(3)?;
            Ok(Quantity::new(finite(x.value.cbrt())?, dim))
        }
        Func::Abs => Ok(Quantity { value: x.value.abs(), ..x }),
        Func::Sign => {
            let s = if x.value > 0.0 {
                1.0
            } else if x.value < 0.0 {
                -1.0
            } else {
                0.0
            };
            Ok(Quantity::number(s))
        }
        Func::Fact => {
            let n = whole(x, function)?;
            if n < 0 {
                return Err(domain(function));
            }
            if n > 170 {
                return Err(CalcError::Overflow);
            }
            let mut acc = 1.0f64;
            for k in 2..=n {
                acc = finite(acc * k as f64)?;
            }
            Ok(Quantity::number(acc))
        }
        _ => {
            let v = plain(function, x)?;
            let out = match func {
                Func::Exp => v.exp(),
                Func::Ln | Func::Log | Func::Log2 if v <= 0.0 => return Err(domain(function)),
                Func::Ln => v.ln(),
                Func::Log => v.log10(),
                Func::Log2 => v.log2(),
                Func::Sin => v.sin(),
                Func::Cos => v.cos(),
                Func::Tan => v.tan(),
                Func::Asin | Func::Acos if !(-1.0..=1.0).contains(&v) => return Err(domain(function)),
                Func::Asin => v.asin(),
                Func::Acos => v.acos(),
                Func::Atan => v.atan(),
                Func::Sinh => v.sinh(),
                Func::Cosh => v.cosh(),
                Func::Tanh => v.tanh(),
                Func::Asinh => v.asinh(),
                Func::Acosh if v < 1.0 => return Err(domain(function)),
                Func::Acosh => v.acosh(),
                Func::Atanh if v <= -1.0 || v >= 1.0 => return Err(domain(function)),
                Func::Atanh => v.atanh(),
                _ => v.atan(),
            };
            Ok(Quantity::number(finite(out)?))
        }
    }
}

fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

fn binom_qty(nq: Quantity, kq: Quantity) -> Result<Quantity, CalcError> {
    let n = whole(nq, "binom")?;
    let k = whole(kq, "binom")?;
    if n < 0 || k < 0 {
        return Err(domain("binom"));
    }
    if k > n {
        return Ok(Quantity::number(0.0));
    }
    let k = k.min(n - k) as u64;
    let n = n as u64;
    if k > 1_030 {
        return Err(CalcError::Overflow);
    }
    let mut exact = Some(1u128);
    let mut approx = 1.0f64;
    for i in 0..k {
        let num = u128::from(n - i);
        let den = u128::from(i + 1);
        if let Some(acc) = exact {
            let g = gcd_u128(num, den);
            let (rn, rd) = (num / g, den / g);
            let reduced_acc = acc / rd;
            if let Some(next) = reduced_acc.checked_mul(rn) {
                exact = Some(next);
                approx = next as f64;
                continue;
            }
            exact = None;
        }
        approx = finite(approx * (num as f64 / den as f64))?;
    }
    Ok(Quantity::number(finite(approx)?))
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

fn binary_quantity(op: BinOp, x: Quantity, y: Quantity, mode: crate::UncertaintyMode) -> Result<Quantity, CalcError> {
    let result = match op {
        BinOp::Add => x.try_add(y),
        BinOp::Sub => x.try_sub(y),
        BinOp::Mul => x.try_mul(y),
        BinOp::Div => x.try_div(y),
        BinOp::Pow => x.try_pow(y),
    }?;
    if mode == crate::UncertaintyMode::Rss && result.unc.is_some() {
        let a = Quantity { unc: None, ..x };
        let b = Quantity { unc: None, ..y };
        let one = match op {
            BinOp::Add => x.try_add(b),
            BinOp::Sub => x.try_sub(b),
            BinOp::Mul => x.try_mul(b),
            BinOp::Div => x.try_div(b),
            BinOp::Pow => x.try_pow(b),
        }?;
        let two = match op {
            BinOp::Add => a.try_add(y),
            BinOp::Sub => a.try_sub(y),
            BinOp::Mul => a.try_mul(y),
            BinOp::Div => a.try_div(y),
            BinOp::Pow => a.try_pow(y),
        }?;
        return Ok(Quantity { unc: Some(finite(one.unc.unwrap_or(0.0).hypot(two.unc.unwrap_or(0.0)))?), ..result });
    }
    Ok(result)
}
