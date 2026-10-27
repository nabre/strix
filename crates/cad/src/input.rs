//! The command line's point entry, AutoCAD style.

use std::fmt;

use crate::MAX_COORD;
use crate::geom::Pt;

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

/// Reads a typed point:
///
/// - `x,y` absolute; `@dx,dy` relative to `last`;
/// - `@d<a` polar from `last` (distance `d`, angle `a` degrees counter-clockwise from +x);
///   `d<a` polar from the origin;
/// - a lone number is a **direct distance**: that far from `last` towards `toward` (the pointer).
///
/// Separators: `,` or `;` between coordinates; decimals with `.`. Coordinates are clamped to
/// ±[`MAX_COORD`].
pub fn parse_point(text: &str, last: Option<Pt>, toward: Option<Pt>) -> Result<Pt, InputError> {
    let t: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let bad = || InputError::Malformed(text.chars().take(40).collect());
    let num = |s: &str| -> Result<f64, InputError> {
        s.parse::<f64>().ok().filter(|v| v.is_finite()).map(|v| v.clamp(-MAX_COORD, MAX_COORD)).ok_or_else(bad)
    };
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
        return Ok(base.polar(num(d)?, num(a)?));
    }
    if let Some((x, y)) = body.split_once([',', ';']) {
        if y.contains([',', ';']) {
            return Err(bad());
        }
        return Ok(base.plus(Pt::new(num(x)?, num(y)?)));
    }
    if relative {
        return Err(bad());
    }
    let d = num(body)?;
    let from = last.ok_or(InputError::NoPrevious)?;
    let dir = toward.map(|p| p.minus(from)).filter(|v| v.len() > 0.0).ok_or(InputError::NoDirection)?;
    Ok(from.plus(dir.unit().scale(d)))
}
