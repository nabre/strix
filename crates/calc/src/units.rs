//! The unit table, prefixes, constants and display units.

use crate::quantity::{BASE_SYMBOLS, Dim, Quantity};

const fn d(m: i8, kg: i8, s: i8, a: i8, k: i8, mol: i8, cd: i8) -> Dim {
    Dim::new([m, kg, s, a, k, mol, cd])
}

const NONE: Dim = Dim::NONE;
const LENGTH: Dim = d(1, 0, 0, 0, 0, 0, 0);
const MASS: Dim = d(0, 1, 0, 0, 0, 0, 0);
const TIME: Dim = d(0, 0, 1, 0, 0, 0, 0);
const CURRENT: Dim = d(0, 0, 0, 1, 0, 0, 0);
const TEMPERATURE: Dim = d(0, 0, 0, 0, 1, 0, 0);
const AMOUNT: Dim = d(0, 0, 0, 0, 0, 1, 0);
const LUMINOUS: Dim = d(0, 0, 0, 0, 0, 0, 1);
const AREA: Dim = d(2, 0, 0, 0, 0, 0, 0);
const VOLUME: Dim = d(3, 0, 0, 0, 0, 0, 0);
const FORCE: Dim = d(1, 1, -2, 0, 0, 0, 0);
const PRESSURE: Dim = d(-1, 1, -2, 0, 0, 0, 0);
const ENERGY: Dim = d(2, 1, -2, 0, 0, 0, 0);
const POWER: Dim = d(2, 1, -3, 0, 0, 0, 0);
const LINE_LOAD: Dim = d(0, 1, -2, 0, 0, 0, 0);
const FREQUENCY: Dim = d(0, 0, -1, 0, 0, 0, 0);
const CHARGE: Dim = d(0, 0, 1, 1, 0, 0, 0);
const VOLTAGE: Dim = d(2, 1, -3, -1, 0, 0, 0);
const RESISTANCE: Dim = d(2, 1, -3, -2, 0, 0, 0);
const VELOCITY: Dim = d(1, 0, -1, 0, 0, 0, 0);
const ACCELERATION: Dim = d(1, 0, -2, 0, 0, 0, 0);

struct UnitDef {
    name: &'static str,
    factor: f64,
    dim: Dim,
    /// Takes the SI prefixes in [`PREFIXES`].
    prefixable: bool,
}

const fn u(name: &'static str, factor: f64, dim: Dim, prefixable: bool) -> UnitDef {
    UnitDef { name, factor, dim, prefixable }
}

/// Named units. Exact names win over prefixed ones (`min` is minutes, `mol` moles, `cd`
/// candela). Engineering spellings such as `mm`, `kN`, `MPa` and `kWh` come from prefixes.
const UNITS: &[UnitDef] = &[
    u("m", 1.0, LENGTH, true),
    u("g", 1e-3, MASS, true),
    u("kg", 1.0, MASS, false),
    u("t", 1e3, MASS, false),
    u("s", 1.0, TIME, true),
    u("min", 60.0, TIME, false),
    u("h", 3600.0, TIME, false),
    u("A", 1.0, CURRENT, true),
    u("K", 1.0, TEMPERATURE, false),
    u("mol", 1.0, AMOUNT, true),
    u("cd", 1.0, LUMINOUS, false),
    u("N", 1.0, FORCE, true),
    u("Pa", 1.0, PRESSURE, true),
    u("bar", 1e5, PRESSURE, true),
    u("J", 1.0, ENERGY, true),
    u("Wh", 3600.0, ENERGY, true),
    u("W", 1.0, POWER, true),
    u("C", 1.0, CHARGE, true),
    u("V", 1.0, VOLTAGE, true),
    u("\u{3A9}", 1.0, RESISTANCE, true),  // Ω Greek capital omega
    u("\u{2126}", 1.0, RESISTANCE, true), // Ω ohm sign
    u("ohm", 1.0, RESISTANCE, true),
    u("Hz", 1.0, FREQUENCY, true),
    u("L", 1e-3, VOLUME, true),
    u("rad", 1.0, NONE, true),
    u("deg", std::f64::consts::PI / 180.0, NONE, false),
    u("°", std::f64::consts::PI / 180.0, NONE, false),
    u("%", 0.01, NONE, false),
];

/// SI prefixes accepted on prefixable units. `µ` (micro sign), `μ` (Greek mu) and `u` all mean
/// micro.
const PREFIXES: &[(&str, f64)] =
    &[("n", 1e-9), ("\u{B5}", 1e-6), ("\u{3BC}", 1e-6), ("u", 1e-6), ("m", 1e-3), ("c", 1e-2), ("d", 1e-1), ("k", 1e3), ("M", 1e6), ("G", 1e9)];

/// The unit called `name` as a quantity (`unit("kN")` is 1000 N), or `None` if `name` is not a
/// unit. Exact names are looked up first, then a prefix on a unit that accepts prefixes.
pub fn unit(name: &str) -> Option<Quantity> {
    if let Some(def) = UNITS.iter().find(|def| def.name == name) {
        return Some(Quantity::new(def.factor, def.dim));
    }
    PREFIXES.iter().find_map(|&(prefix, scale)| {
        let rest = name.strip_prefix(prefix)?;
        let def = UNITS.iter().find(|def| def.prefixable && def.name == rest)?;
        Some(Quantity::new(scale * def.factor, def.dim))
    })
}

/// A unit spelled like `name` but for letter case (`KN` → `kN`), when `name` itself is not one.
/// Lower-case prefixes are tried first, so `MM` suggests `mm`, not `Mm`.
pub(crate) fn suggest(name: &str) -> Option<String> {
    if unit(name).is_some() || name.chars().count() > 8 {
        return None;
    }
    let lower = name.to_lowercase();
    let names = UNITS.iter().map(|d| d.name.to_string());
    let prefixed = PREFIXES.iter().flat_map(|&(p, _)| UNITS.iter().filter(|d| d.prefixable).map(move |d| format!("{p}{}", d.name)));
    let mut found: Vec<String> = names.chain(prefixed).filter(|n| n.to_lowercase() == lower && n != name).collect();
    found.sort_by_key(|n| n.chars().filter(|c| c.is_uppercase()).count());
    found.into_iter().next()
}

/// The predefined constant called `name`: `π`/`pi`, `e` and `g` (standard gravity,
/// 9.80665 m/s²).
pub fn constant(name: &str) -> Option<Quantity> {
    match name {
        "π" | "pi" => Some(Quantity::number(std::f64::consts::PI)),
        "e" => Some(Quantity::number(std::f64::consts::E)),
        "g" => Some(Quantity::new(9.80665, ACCELERATION)),
        _ => None,
    }
}

/// Coherent units preferred for display, by dimension. Torque has the dimension of energy and
/// shows as `J` unless `N·m` is asked for after `=`.
const PREFERRED: &[(Dim, &str)] = &[
    (LENGTH, "m"),
    (AREA, "m²"),
    (VOLUME, "m³"),
    (MASS, "kg"),
    (TIME, "s"),
    (CURRENT, "A"),
    (TEMPERATURE, "K"),
    (AMOUNT, "mol"),
    (LUMINOUS, "cd"),
    (FORCE, "N"),
    (PRESSURE, "Pa"),
    (ENERGY, "J"),
    (POWER, "W"),
    (LINE_LOAD, "N/m"),
    (FREQUENCY, "Hz"),
    (CHARGE, "C"),
    (VOLTAGE, "V"),
    (RESISTANCE, "Ω"),
    (VELOCITY, "m/s"),
    (ACCELERATION, "m/s²"),
];

/// The coherent SI unit a result of dimension `dim` is shown in when no unit is asked for:
/// empty for a plain number, a preferred named unit (`N`, `Pa`, `J`, `m/s²` …) when there is
/// one, else a product of base units in canonical order with positive exponents first
/// (`kg·m^-1·s^-2`). The unit's factor is always 1, so the SI magnitude is the shown number.
pub fn display_unit(dim: Dim) -> String {
    if dim.is_dimensionless() {
        return String::new();
    }
    if let Some((_, name)) = PREFERRED.iter().find(|(d, _)| *d == dim) {
        return (*name).to_string();
    }
    let exps = dim.exponents();
    let mut parts = Vec::new();
    for positive in [true, false] {
        for (sym, e) in BASE_SYMBOLS.iter().zip(exps) {
            if (positive && e > 0) || (!positive && e < 0) {
                parts.push(power_text(sym, i64::from(e)));
            }
        }
    }
    parts.join("·")
}

/// Named units shown with an engineering prefix when no unit is asked for (`kN`, `MPa`, `kJ`).
/// Lengths, masses, times and the base units stay unprefixed, as worksheets usually show them.
const PREFIXED_DISPLAY: &[&str] = &["N", "Pa", "J", "W", "N/m", "Hz", "C", "V", "Ω"];

/// Engineering prefixes for [`auto_unit`]: power of ten and symbol.
const ENGINEERING: &[(i32, &str)] = &[(-9, "n"), (-6, "µ"), (-3, "m"), (3, "k"), (6, "M"), (9, "G")];

/// `value` (SI) of dimension `dim` in the automatic display unit: [`display_unit`], with an
/// engineering prefix for the named units that take one so that the number falls in 1–1000
/// (`30375 J` → `30.375 kJ`, `1.25e8 Pa` → `125 MPa`). Zero and non-finite values keep the
/// coherent unit; values beyond the prefix range keep the nearest prefix.
pub fn auto_unit(value: f64, dim: Dim) -> (f64, String) {
    let unit = display_unit(dim);
    if !PREFIXED_DISPLAY.contains(&unit.as_str()) || value == 0.0 || !value.is_finite() {
        return (value, unit);
    }
    // floor(log10 |v|) rounded down to a multiple of 3, clamped to the prefix range.
    let exp = value.abs().log10().floor();
    if !exp.is_finite() {
        return (value, unit);
    }
    let eng = ((exp / 3.0).floor() * 3.0).clamp(-9.0, 9.0) as i32;
    match ENGINEERING.iter().find(|(p, _)| *p == eng) {
        Some((p, sym)) => (value / 10f64.powi(*p), format!("{sym}{unit}")),
        None => (value, unit),
    }
}

/// The unit of `dim` for messages: [`display_unit`], or "a number" for a plain number.
pub(crate) fn dim_name(dim: Dim) -> String {
    if dim.is_dimensionless() { "a number".to_string() } else { display_unit(dim) }
}

/// `base` raised to `e`: `m`, `m²`, `m³`, `m^4`, `m^-1`.
pub(crate) fn power_text(base: &str, e: i64) -> String {
    match e {
        1 => base.to_string(),
        2 => format!("{base}²"),
        3 => format!("{base}³"),
        _ => format!("{base}^{e}"),
    }
}
