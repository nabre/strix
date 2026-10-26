//! The calc error type.

use std::fmt;

/// Why a calc statement could not be evaluated. [`Display`](fmt::Display) gives a plain-English
/// message suitable for the user.
#[derive(Debug, Clone, PartialEq)]
pub enum CalcError {
    /// The text is not a valid statement; `pos` is the 1-based character position.
    Syntax {
        /// 1-based character position of the problem.
        pos: usize,
        /// What was wrong there.
        message: String,
    },
    /// The statement is longer than [`MAX_INPUT_CHARS`](crate::MAX_INPUT_CHARS).
    TooLong {
        /// The limit, in characters.
        limit: usize,
    },
    /// Nested deeper than [`MAX_DEPTH`](crate::MAX_DEPTH) or more than
    /// [`MAX_NODES`](crate::MAX_NODES) terms.
    TooComplex,
    /// A name that is neither a defined parameter, a constant nor a unit.
    UnknownName(String),
    /// A name in a requested display unit that is not a unit.
    UnknownUnit(String),
    /// Two quantities of different dimensions were combined (`op` is `add`, `subtract` or
    /// `compare`); `left`/`right` are unit names.
    DimensionMismatch {
        /// The operation: `add`, `subtract` or `compare`.
        op: &'static str,
        /// Unit of the left operand.
        left: String,
        /// Unit of the right operand.
        right: String,
    },
    /// The display unit requested after `=` has a different dimension from the result.
    WrongUnit {
        /// The unit the result is in.
        result: String,
        /// The unit that was asked for.
        requested: String,
    },
    /// Division by zero (also `0^-1`).
    DivisionByZero,
    /// A result too large for `f64`.
    Overflow,
    /// A result that is not a real number (such as `sqrt(-1)`).
    NotReal,
    /// An exponent with a unit (`2^(3 m)`).
    ExponentNotDimensionless {
        /// Unit of the exponent.
        found: String,
    },
    /// A power that would leave a fractional unit exponent (`sqrt(m)`).
    FractionalDimension {
        /// Unit of the base.
        unit: String,
    },
    /// A unit exponent outside `-128..=127` (`m^200`).
    DimensionOverflow,
    /// A function that needs a dimensionless argument got a quantity with a unit.
    NeedsDimensionless {
        /// Function name.
        function: String,
        /// Unit of the argument.
        found: String,
    },
    /// An argument outside the function's domain (`ln(-1)`, `asin(2)`).
    Domain {
        /// Function name.
        function: String,
    },
    /// Wrong number of arguments.
    Arity {
        /// Function name.
        function: String,
        /// What it takes, in words ("1 argument").
        expected: &'static str,
    },
    /// A function name used without `(…)`.
    FunctionNeedsArguments(String),
    /// A malformed definition (the left side of `:=` is not a single name, …).
    BadDefinition(String),
}

impl CalcError {
    /// A syntax error at the 0-based character index `pos0`.
    pub(crate) fn syntax(pos0: usize, message: impl Into<String>) -> Self {
        CalcError::Syntax { pos: pos0.saturating_add(1), message: message.into() }
    }
}

impl fmt::Display for CalcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CalcError::Syntax { pos, message } => write!(f, "syntax error at char {pos}: {message}"),
            CalcError::TooLong { limit } => write!(f, "the statement is too long (more than {limit} characters)"),
            CalcError::TooComplex => f.write_str("the expression is too complex (nested too deeply or too many terms)"),
            CalcError::UnknownName(name) => write!(f, "unknown name {name}"),
            CalcError::UnknownUnit(name) => write!(f, "{name} is not a unit"),
            CalcError::DimensionMismatch { op, left, right } => write!(f, "cannot {op} {left} and {right}"),
            CalcError::WrongUnit { result, requested } => write!(f, "result is in {result}, not {requested}"),
            CalcError::DivisionByZero => f.write_str("division by zero"),
            CalcError::Overflow => f.write_str("the result is too large"),
            CalcError::NotReal => f.write_str("the result is not a real number"),
            CalcError::ExponentNotDimensionless { found } => write!(f, "an exponent must be a plain number, not {found}"),
            CalcError::FractionalDimension { unit } => {
                write!(f, "cannot take this power of {unit}: the unit would get a fractional exponent")
            }
            CalcError::DimensionOverflow => f.write_str("a unit exponent is too large"),
            CalcError::NeedsDimensionless { function, found } => write!(f, "{function} needs a plain number, not {found}"),
            CalcError::Domain { function } => write!(f, "the argument of {function} is outside its domain"),
            CalcError::Arity { function, expected } => write!(f, "{function} takes {expected}"),
            CalcError::FunctionNeedsArguments(name) => write!(f, "{name} needs its argument in parentheses, like {name}(x)"),
            CalcError::BadDefinition(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for CalcError {}
