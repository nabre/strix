//! Dimensions and quantities.

use crate::CalcError;
use crate::units::dim_name;

/// Symbols of the seven SI base units, in the order of [`Dim`]'s exponents.
pub const BASE_SYMBOLS: [&str; 7] = ["m", "kg", "s", "A", "K", "mol", "cd"];

/// Exponents of the seven SI base dimensions, in the order of [`BASE_SYMBOLS`]: length, mass,
/// time, electric current, temperature, amount of substance, luminous intensity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Dim([i8; 7]);

impl Dim {
    /// No dimension (a plain number).
    pub const NONE: Dim = Dim([0; 7]);

    /// A dimension from its exponents (m, kg, s, A, K, mol, cd).
    pub const fn new(exponents: [i8; 7]) -> Self {
        Dim(exponents)
    }

    /// The exponents (m, kg, s, A, K, mol, cd).
    pub const fn exponents(self) -> [i8; 7] {
        self.0
    }

    /// True for a plain number.
    pub fn is_dimensionless(self) -> bool {
        self.0.iter().all(|&e| e == 0)
    }

    /// The dimension of a product: exponents add. Overflow is [`CalcError::DimensionOverflow`].
    pub fn try_mul(self, other: Dim) -> Result<Dim, CalcError> {
        self.combine(other, i8::checked_add)
    }

    /// The dimension of a quotient: exponents subtract. Overflow is
    /// [`CalcError::DimensionOverflow`].
    pub fn try_div(self, other: Dim) -> Result<Dim, CalcError> {
        self.combine(other, i8::checked_sub)
    }

    /// The dimension raised to the power `p`: every exponent times `p` must be a whole number
    /// in `i8` range (`m²` to the 0.5 is `m`; `m` to the 0.5 is an error).
    pub fn try_pow(self, p: f64) -> Result<Dim, CalcError> {
        let mut out = [0i8; 7];
        for (o, e) in out.iter_mut().zip(self.0) {
            if e == 0 {
                continue;
            }
            let raw = f64::from(e) * p;
            let rounded = raw.round();
            if !raw.is_finite() || (raw - rounded).abs() > 1e-9 {
                return Err(CalcError::FractionalDimension { unit: dim_name(self) });
            }
            if !(-128.0..=127.0).contains(&rounded) {
                return Err(CalcError::DimensionOverflow);
            }
            *o = rounded as i8;
        }
        Ok(Dim(out))
    }

    fn combine(self, other: Dim, op: fn(i8, i8) -> Option<i8>) -> Result<Dim, CalcError> {
        let mut out = [0i8; 7];
        for ((o, a), b) in out.iter_mut().zip(self.0).zip(other.0) {
            *o = op(a, b).ok_or(CalcError::DimensionOverflow)?;
        }
        Ok(Dim(out))
    }
}

/// A physical quantity: a magnitude in coherent SI units (metres, kilograms, seconds, …) and its
/// dimension. `12 kN/m` is `Quantity { value: 12000.0, dim: kg·s⁻² }`.
///
/// Arithmetic returns `Result`: adding different dimensions, dividing by zero, overflowing or
/// producing a non-real number is an error, never a panic or a NaN.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quantity {
    /// Magnitude in coherent SI units.
    pub value: f64,
    /// Dimension.
    pub dim: Dim,
}

impl Quantity {
    /// A quantity from an SI magnitude and a dimension.
    pub const fn new(value: f64, dim: Dim) -> Self {
        Quantity { value, dim }
    }

    /// A plain number.
    pub const fn number(value: f64) -> Self {
        Quantity { value, dim: Dim::NONE }
    }

    /// Sum; the dimensions must be equal.
    pub fn try_add(self, other: Quantity) -> Result<Quantity, CalcError> {
        self.same_dim(other, "add")?;
        Ok(Quantity::new(finite(self.value + other.value)?, self.dim))
    }

    /// Difference; the dimensions must be equal.
    pub fn try_sub(self, other: Quantity) -> Result<Quantity, CalcError> {
        self.same_dim(other, "subtract")?;
        Ok(Quantity::new(finite(self.value - other.value)?, self.dim))
    }

    /// Product.
    pub fn try_mul(self, other: Quantity) -> Result<Quantity, CalcError> {
        let dim = self.dim.try_mul(other.dim)?;
        Ok(Quantity::new(finite(self.value * other.value)?, dim))
    }

    /// Quotient; dividing by zero is [`CalcError::DivisionByZero`].
    pub fn try_div(self, other: Quantity) -> Result<Quantity, CalcError> {
        if other.value == 0.0 {
            return Err(CalcError::DivisionByZero);
        }
        let dim = self.dim.try_div(other.dim)?;
        Ok(Quantity::new(finite(self.value / other.value)?, dim))
    }

    /// Power. The exponent must be dimensionless; a base with a unit needs an exponent that
    /// keeps the unit exponents whole (`sqrt(m²)` is fine, `sqrt(m)` is not).
    pub fn try_pow(self, exponent: Quantity) -> Result<Quantity, CalcError> {
        if !exponent.dim.is_dimensionless() {
            return Err(CalcError::ExponentNotDimensionless { found: dim_name(exponent.dim) });
        }
        let p = exponent.value;
        let dim = self.dim.try_pow(p)?;
        if self.value == 0.0 && p < 0.0 {
            return Err(CalcError::DivisionByZero);
        }
        Ok(Quantity::new(finite(self.value.powf(p))?, dim))
    }

    /// The opposite quantity.
    pub fn negate(self) -> Quantity {
        Quantity::new(-self.value, self.dim)
    }

    /// Errors unless both quantities have the same dimension; `op` names the operation in the
    /// message (`add`, `subtract`, `compare`).
    pub(crate) fn same_dim(self, other: Quantity, op: &'static str) -> Result<(), CalcError> {
        if self.dim == other.dim { Ok(()) } else { Err(CalcError::DimensionMismatch { op, left: dim_name(self.dim), right: dim_name(other.dim) }) }
    }
}

/// `v` if it is finite, else the matching error.
pub(crate) fn finite(v: f64) -> Result<f64, CalcError> {
    if v.is_nan() {
        Err(CalcError::NotReal)
    } else if v.is_infinite() {
        Err(CalcError::Overflow)
    } else {
        Ok(v)
    }
}
