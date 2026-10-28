//! The command line's point entry, AutoCAD style.

use std::fmt;

use crate::MAX_COORD;
use crate::geom::Pt;
use crate::model::Unit;

/// Why a typed point could not be read.
#[derive(Debug, Clone, PartialEq)]
pub enum InputError {
    /// Not a point: shows what was typed.
    Malformed(String),
    /// A relative point or a distance with no previous point.
    NoPrevious,
    /// A distance alone with no direction (the pointer is on the previous point).
    NoDirection,
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InputError::Malformed(t) => write!(f, "not a point: {t} (type x,y  @dx,dy  @distance<angle  distance<angle  or a distance)"),
            InputError::NoPrevious => f.write_str("there is no previous point to measure from"),
            InputError::NoDirection => f.write_str("move the pointer to show the direction"),
        }
    }
}

impl std::error::Error for InputError {}

/// The units typed values are read in: lengths without a unit of their own are in `input`,
/// and every length ends up in `drawing`'s unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Units {
    pub drawing: Unit,
    pub input: Unit,
}

impl Units {
    /// A drawing's units: its unit, and its input unit (its own unit when it sets none).
    pub fn of(d: &crate::Drawing) -> Units {
        Units { drawing: d.unit, input: d.input.unwrap_or(d.unit) }
    }
}

/// A typed length in the drawing's unit: a number in the input unit, or a number with its own
/// unit (`2.5m`, `300 mm`, `30cm`; a decimal comma too). None when it is not a length.
pub fn parse_length(text: &str, units: Units) -> Option<f64> {
    let t: String = text.chars().filter(|c| !c.is_whitespace()).collect::<String>().replace(',', ".");
    // Longest symbol first: `mm` and `cm` before `m`.
    let (number, unit) = ["mm", "cm", "m"]
        .iter()
        .find_map(|u| t.strip_suffix(u).filter(|n| !n.is_empty()).map(|n| (n.to_string(), Unit::parse(u))))
        .unwrap_or((t, None));
    let v = number.parse::<f64>().ok().filter(|v| v.is_finite())?;
    let from = unit.unwrap_or(units.input);
    Some((v * from.mm() / units.drawing.mm()).clamp(-MAX_COORD, MAX_COORD))
}

/// [`parse_point_in`] with lengths in the drawing's own unit (no input unit of its own).
pub fn parse_point(text: &str, last: Option<Pt>, toward: Option<Pt>) -> Result<Pt, InputError> {
    parse_point_in(text, last, toward, Units::default())
}

/// Reads a typed point:
///
/// - `x,y` absolute; `@dx,dy` relative to `last`;
/// - `@d<a` polar from `last` (distance `d`, angle `a` degrees counter-clockwise from +x);
///   `d<a` polar from the origin;
/// - a lone number is a **direct distance**: that far from `last` towards `toward` (the pointer).
///
/// Lengths are read in `units.input` unless they carry their own unit (`2.5m,1.2m`,
/// `@300mm<45`), and come out in `units.drawing`; angles are degrees. Separators: `,` or `;`
/// between coordinates; decimals with `.`. Coordinates are clamped to ±[`MAX_COORD`].
pub fn parse_point_in(text: &str, last: Option<Pt>, toward: Option<Pt>, units: Units) -> Result<Pt, InputError> {
    let t: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let bad = || InputError::Malformed(text.chars().take(40).collect());
    let num = |s: &str| -> Result<f64, InputError> {
        s.parse::<f64>().ok().filter(|v| v.is_finite()).map(|v| v.clamp(-MAX_COORD, MAX_COORD)).ok_or_else(bad)
    };
    // A length: a number in the input unit, or with its own unit. No decimal comma here: the
    // comma separates coordinates.
    let len = |s: &str| -> Result<f64, InputError> { if s.contains(',') { Err(bad()) } else { parse_length(s, units).ok_or_else(bad) } };
    if t.is_empty() {
        return Err(bad());
    }
    let (relative, body) = match t.strip_prefix('@') {
        Some(rest) => (true, rest),
        None => (false, t.as_str()),
    };
    let base = if relative { last.ok_or(InputError::NoPrevious)? } else { Pt::default() };
    if let Some((d, a)) = body.split_once('<') {
        if a.contains('<') {
            return Err(bad());
        }
        return Ok(base.polar(len(d)?, num(a)?));
    }
    if let Some((x, y)) = body.split_once([',', ';']) {
        if y.contains([',', ';']) {
            return Err(bad());
        }
        return Ok(base.plus(Pt::new(len(x)?, len(y)?)));
    }
    if relative {
        return Err(bad());
    }
    let d = len(body)?;
    let from = last.ok_or(InputError::NoPrevious)?;
    let dir = toward.map(|p| p.minus(from)).filter(|v| v.len() > 0.0).ok_or(InputError::NoDirection)?;
    Ok(from.plus(dir.unit().scale(d)))
}
