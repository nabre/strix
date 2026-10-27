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
    /// A vector or range longer than [`MAX_VALUES`](crate::MAX_VALUES), or a sum with more
    /// terms.
    TooManyValues {
        /// The limit.
        limit: usize,
    },
    /// More work than [`MAX_STEPS`](crate::MAX_STEPS) (nested sums over long ranges).
    TooMuchWork,
    /// Values of different dimensions in one vector or table column.
    MixedVector {
        /// Unit of the first element.
        first: String,
        /// Unit of the element that differs.
        other: String,
    },
    /// A vector where a single value is needed.
    NotAScalar {
        /// Number of elements of the vector.
        len: usize,
    },
    /// Two vectors of different lengths combined element by element.
    LengthMismatch {
        /// Length of the left operand.
        left: usize,
        /// Length of the right operand.
        right: usize,
    },
    /// An index outside `1..=len`.
    IndexOutOfRange {
        /// The index asked for (1-based).
        index: i64,
        /// Number of elements.
        len: usize,
    },
    /// An index or limit that is not a whole number.
    NotWhole {
        /// What it was: "an index", "a limit of ∑".
        what: String,
    },
    /// A table cell that is empty where a formula needs its value.
    MissingValue(String),
    /// No value of the unknown satisfies the equation (or none was found).
    NoSolution(String),
    /// An equation with more than one unknown: it cannot be solved, so it is left as written.
    Underdetermined(Vec<String>),
    /// A check (`σ ≤ f_yd`) that does not hold; the text is the comparison with the values.
    CheckFailed(String),
    /// An equation whose sides are both known but differ.
    NotEqual {
        /// The left side, formatted.
        left: String,
        /// The right side, formatted.
        right: String,
    },
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
            CalcError::UnknownName(name) => write!(f, "unknown name {name}{}", hint(name)),
            CalcError::UnknownUnit(name) => write!(f, "{name} is not a unit{}", hint(name)),
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
            CalcError::TooManyValues { limit } => write!(f, "too many values (more than {limit})"),
            CalcError::TooMuchWork => f.write_str("the calculation takes too many steps"),
            CalcError::MixedVector { first, other } => write!(f, "a vector holds values of one kind, not both {first} and {other}"),
            CalcError::NotAScalar { len } => write!(f, "expected a single value, not a vector of {len}"),
            CalcError::LengthMismatch { left, right } => write!(f, "cannot combine vectors of {left} and {right} values"),
            CalcError::IndexOutOfRange { index, len } => write!(f, "index {index} is outside 1..{len}"),
            CalcError::NotWhole { what } => write!(f, "{what} must be a whole number"),
            CalcError::MissingValue(name) => write!(f, "{name} has no value in this row"),
            CalcError::NoSolution(name) => write!(f, "no value of {name} solves the equation"),
            CalcError::Underdetermined(names) => write!(f, "more than one unknown ({}): give values to all but one", names.join(", ")),
            CalcError::CheckFailed(text) => write!(f, "the check fails: {text}"),
            CalcError::NotEqual { left, right } => write!(f, "the two sides differ: {left} ≠ {right}"),
        }
    }
}

impl std::error::Error for CalcError {}

/// " (did you mean kN?)" when `name` is a unit spelled with the wrong case.
fn hint(name: &str) -> String {
    crate::units::suggest(name).map(|u| format!(" (did you mean {u}?)")).unwrap_or_default()
}
