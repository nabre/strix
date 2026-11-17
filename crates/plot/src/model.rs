//! Serializable plot model and sanitization.

use serde::{Deserialize, Serialize};

use crate::{MAX_DATA_POINTS, MAX_JSON_BYTES, MAX_SERIES};

/// Distinct accessible series palette (deep blue, coral, teal, plum, amber, slate).
pub const DEFAULT_COLORS: [[u8; 3]; 6] =
    [[0x1B, 0x4F, 0x72], [0xD9, 0x53, 0x2F], [0x11, 0x7A, 0x65], [0x7D, 0x3C, 0x98], [0xB9, 0x77, 0x0E], [0x34, 0x49, 0x5E]];

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_true(b: &bool) -> bool {
    *b
}

fn default_true() -> bool {
    true
}

fn default_width_mm() -> f64 {
    120.0
}

fn default_height_mm() -> f64 {
    80.0
}

fn default_neg5() -> f64 {
    -5.0
}

fn default_pos5() -> f64 {
    5.0
}

fn default_stroke_mm() -> f64 {
    0.45
}

fn default_color() -> [u8; 3] {
    DEFAULT_COLORS[0]
}

fn default_var_x() -> String {
    "x".to_string()
}

fn default_var_t() -> String {
    "t".to_string()
}

fn default_tau() -> f64 {
    std::f64::consts::TAU
}

/// Regression curve fitted to a [`SeriesKind::Data`] series.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RegressionKind {
    #[default]
    None,
    Linear,
    Quadratic,
}

impl RegressionKind {
    pub fn name(self) -> &'static str {
        match self {
            RegressionKind::None => "none",
            RegressionKind::Linear => "linear",
            RegressionKind::Quadratic => "quadratic",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "none" | "" => Some(RegressionKind::None),
            "linear" => Some(RegressionKind::Linear),
            "quadratic" => Some(RegressionKind::Quadratic),
            _ => None,
        }
    }
}

/// Where oriented angles start and which way they turn (`docs/plot.md`, Reference system).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AngleConvention {
    /// Mathematics: counter-clockwise from the positive abscissa (`x = ρ·cos θ`, `y = ρ·sin θ`).
    #[default]
    Math,
    /// Surveying: clockwise from the positive ordinate, like a bearing from north
    /// (`x = ρ·sin θ`, `y = ρ·cos θ`).
    Survey,
}

impl AngleConvention {
    pub fn name(self) -> &'static str {
        match self {
            AngleConvention::Math => "math",
            AngleConvention::Survey => "survey",
        }
    }

    /// The point at distance `rho` and angle `theta_rad` (radians) in this convention.
    pub fn point(self, rho: f64, theta_rad: f64) -> (f64, f64) {
        match self {
            AngleConvention::Math => (rho * theta_rad.cos(), rho * theta_rad.sin()),
            AngleConvention::Survey => (rho * theta_rad.sin(), rho * theta_rad.cos()),
        }
    }
}

fn is_math_angles(a: &AngleConvention) -> bool {
    *a == AngleConvention::Math
}

/// The unit angles of a plot are entered in (polar series ranges).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AngleUnit {
    /// Radians.
    #[default]
    Rad,
    /// Sexagesimal degrees (`°`, 360 to the turn).
    Deg,
    /// Centesimal degrees (`gon`, 400 to the turn), customary in surveying.
    Gon,
}

impl AngleUnit {
    pub fn name(self) -> &'static str {
        match self {
            AngleUnit::Rad => "rad",
            AngleUnit::Deg => "deg",
            AngleUnit::Gon => "gon",
        }
    }

    /// The unit's symbol as calc writes it.
    pub fn symbol(self) -> &'static str {
        match self {
            AngleUnit::Rad => "rad",
            AngleUnit::Deg => "°",
            AngleUnit::Gon => "gon",
        }
    }

    /// Radians in one of this unit.
    pub fn to_rad(self) -> f64 {
        match self {
            AngleUnit::Rad => 1.0,
            AngleUnit::Deg => std::f64::consts::PI / 180.0,
            AngleUnit::Gon => std::f64::consts::PI / 200.0,
        }
    }

    /// One full turn in this unit.
    pub fn turn(self) -> f64 {
        std::f64::consts::TAU / self.to_rad()
    }
}

fn is_rad(u: &AngleUnit) -> bool {
    *u == AngleUnit::Rad
}

fn default_var_theta() -> String {
    "θ".to_string()
}

fn is_no_regression(r: &RegressionKind) -> bool {
    *r == RegressionKind::None
}

/// One inline experimental measurement `(x ± dx, y ± dy)`.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DataPoint {
    pub x: f64,
    pub y: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dx: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dy: Option<f64>,
}

impl DataPoint {
    pub(crate) fn sanitized(self) -> Option<Self> {
        if !(self.x.is_finite() && self.y.is_finite()) {
            return None;
        }
        let clean_unc = |u: Option<f64>| u.filter(|v| v.is_finite() && *v >= 0.0).map(|v| v.clamp(0.0, 1e12));
        Some(DataPoint { x: self.x.clamp(-1e12, 1e12), y: self.y.clamp(-1e12, 1e12), dx: clean_unc(self.dx), dy: clean_unc(self.dy) })
    }
}

/// Mathematical or experimental series kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SeriesKind {
    /// Cartesian function `y = f(x)`.
    #[serde(rename_all = "camelCase")]
    Function {
        expr: String,
        #[serde(default = "default_var_x")]
        var: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        domain: Option<[f64; 2]>,
        #[serde(default, skip_serializing_if = "is_false")]
        roots: bool,
        #[serde(default, skip_serializing_if = "is_false")]
        extrema: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tangent_at: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        integral: Option<[f64; 2]>,
    },
    /// Parametric curve `(x(t), y(t))` over `t ∈ [t_min, t_max]`.
    #[serde(rename_all = "camelCase")]
    Parametric {
        x_expr: String,
        y_expr: String,
        #[serde(default = "default_var_t")]
        param: String,
        #[serde(default)]
        t_min: f64,
        #[serde(default = "default_tau")]
        t_max: f64,
    },
    /// Polar curve `ρ = f(θ)`: `θ` runs over `theta` (in the plot's [`AngleUnit`], a full turn
    /// when absent) and is turned into a point by the plot's [`AngleConvention`]. `θ` holds the
    /// angle itself, as calc's angles do, so `sin(θ)` is right in any unit.
    #[serde(rename_all = "camelCase")]
    Polar {
        expr: String,
        #[serde(default = "default_var_theta")]
        param: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        theta: Option<[f64; 2]>,
    },
    /// Experimental measurements from `Worksheet` vectors (`x_col`, `y_col`) or inline `points`.
    #[serde(rename_all = "camelCase")]
    Data {
        #[serde(default, skip_serializing_if = "String::is_empty")]
        x_col: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        y_col: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dx_col: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dy_col: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        points: Vec<DataPoint>,
        #[serde(default, skip_serializing_if = "is_no_regression")]
        regression: RegressionKind,
        #[serde(default, skip_serializing_if = "is_false")]
        connect: bool,
    },
}

impl Default for SeriesKind {
    fn default() -> Self {
        SeriesKind::Function {
            expr: "x^2".to_string(),
            var: default_var_x(),
            domain: None,
            roots: false,
            extrema: false,
            tangent_at: None,
            integral: None,
        }
    }
}

/// How a series' line is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LineStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
    DashDot,
}

impl LineStyle {
    pub fn name(self) -> &'static str {
        match self {
            LineStyle::Solid => "solid",
            LineStyle::Dashed => "dashed",
            LineStyle::Dotted => "dotted",
            LineStyle::DashDot => "dashDot",
        }
    }

    /// The dash pattern (on, off, … in millimetres) for a line `width` mm thick: empty when
    /// solid; dots and gaps grow with the line so a thick dotted line still reads as dots.
    pub fn pattern(self, width: f64) -> Vec<f64> {
        let w = if width.is_finite() { width.clamp(0.1, 4.0) } else { 0.45 };
        let k = (w / 0.45).max(1.0);
        match self {
            LineStyle::Solid => Vec::new(),
            LineStyle::Dashed => vec![2.0 * k, 1.3 * k],
            LineStyle::Dotted => vec![w.max(0.3), (w * 2.2).max(0.8)],
            LineStyle::DashDot => vec![2.2 * k, 0.9 * k, w.max(0.3), 0.9 * k],
        }
    }
}

fn is_solid(s: &LineStyle) -> bool {
    *s == LineStyle::Solid
}

/// One curve or dataset drawn on a [`Plot`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Series {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(default = "default_color")]
    pub color: [u8; 3],
    #[serde(default = "default_stroke_mm")]
    pub width: f64,
    /// Older plots' dashed line: read as [`LineStyle::Dashed`] (`sanitized` moves it to `style`).
    #[serde(default, skip_serializing_if = "is_false")]
    pub dashed: bool,
    /// How the line is drawn.
    #[serde(default, skip_serializing_if = "is_solid")]
    pub style: LineStyle,
    /// When true, omitted on student copies (`Show = false`) while keeping axes/bounds identical.
    #[serde(default, skip_serializing_if = "is_false")]
    pub answer: bool,
    #[serde(flatten)]
    pub kind: SeriesKind,
}

impl Default for Series {
    fn default() -> Self {
        Series {
            label: String::new(),
            color: default_color(),
            width: default_stroke_mm(),
            dashed: false,
            style: LineStyle::Solid,
            answer: false,
            kind: SeriesKind::default(),
        }
    }
}

impl Series {
    pub(crate) fn sanitized(mut self) -> Self {
        self.label = truncate_str(&self.label, 120);
        self.width = if self.width.is_finite() { self.width.clamp(0.1, 4.0) } else { default_stroke_mm() };
        if std::mem::take(&mut self.dashed) && self.style == LineStyle::Solid {
            self.style = LineStyle::Dashed;
        }
        match &mut self.kind {
            SeriesKind::Function { expr, var, domain, tangent_at, integral, .. } => {
                *expr = truncate_str(expr, 512);
                *var = clean_ident(var, "x");
                *domain = clean_interval(*domain);
                *tangent_at = tangent_at.filter(|v| v.is_finite()).map(|v| v.clamp(-1e9, 1e9));
                *integral = clean_interval(*integral);
            }
            SeriesKind::Parametric { x_expr, y_expr, param, t_min, t_max } => {
                *x_expr = truncate_str(x_expr, 512);
                *y_expr = truncate_str(y_expr, 512);
                *param = clean_ident(param, "t");
                let (a, b) = clean_bounds(*t_min, *t_max, 0.0, default_tau());
                *t_min = a;
                *t_max = b;
            }
            SeriesKind::Polar { expr, param, theta } => {
                *expr = truncate_str(expr, 512);
                *param = clean_ident(param, "θ");
                *theta = clean_interval(*theta);
            }
            SeriesKind::Data { x_col, y_col, dx_col, dy_col, points, .. } => {
                *x_col = truncate_str(x_col.trim(), 64);
                *y_col = truncate_str(y_col.trim(), 64);
                *dx_col = dx_col.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(|s| truncate_str(s, 64));
                *dy_col = dy_col.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(|s| truncate_str(s, 64));
                let cleaned: Vec<DataPoint> = points.drain(..).filter_map(DataPoint::sanitized).take(MAX_DATA_POINTS).collect();
                *points = cleaned;
            }
        }
        self
    }
}

/// A 2D Cartesian mathematical and experimental plot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Plot {
    /// Overall picture width in paper millimetres.
    #[serde(default = "default_width_mm")]
    pub width: f64,
    /// Overall picture height in paper millimetres.
    #[serde(default = "default_height_mm")]
    pub height: f64,
    #[serde(default = "default_neg5")]
    pub x_min: f64,
    #[serde(default = "default_pos5")]
    pub x_max: f64,
    #[serde(default = "default_neg5")]
    pub y_min: f64,
    #[serde(default = "default_pos5")]
    pub y_max: f64,
    /// Fit the ranges to what the plot shows (analysis points, data, curves) instead of
    /// `x_min`…`y_max`, which are kept for when it is turned off.
    #[serde(default, skip_serializing_if = "is_false")]
    pub auto_range: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub x_label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub y_label: String,
    /// Optional physical unit for horizontal values (e.g. `"s"`, `"cm"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x_unit: Option<String>,
    /// Optional physical unit for vertical values (e.g. `"m"`, `"N"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y_unit: Option<String>,
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub grid: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub minor_grid: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub legend: bool,
    /// Where oriented angles start and which way they turn.
    #[serde(default, skip_serializing_if = "is_math_angles")]
    pub angles: AngleConvention,
    /// The unit angles are entered in.
    #[serde(default, skip_serializing_if = "is_rad")]
    pub angle_unit: AngleUnit,
    /// Draw the positive direction of angles (an arc with an arrow from the reference axis).
    #[serde(default, skip_serializing_if = "is_false")]
    pub angle_mark: bool,
    /// Student/Teacher dual mode: when true and `Show` hides solutions, student copies print only
    /// the Cartesian axes, ticks, labels and grid with identical dimensions so students can plot
    /// by hand while teacher copies overlay all curves, points and shading.
    #[serde(default, skip_serializing_if = "is_false")]
    pub answer: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub series: Vec<Series>,
}

impl Default for Plot {
    fn default() -> Self {
        Plot {
            width: default_width_mm(),
            height: default_height_mm(),
            x_min: default_neg5(),
            x_max: default_pos5(),
            y_min: default_neg5(),
            y_max: default_pos5(),
            auto_range: false,
            title: String::new(),
            x_label: "x".to_string(),
            y_label: "y".to_string(),
            x_unit: None,
            y_unit: None,
            grid: true,
            minor_grid: false,
            legend: false,
            angles: AngleConvention::Math,
            angle_unit: AngleUnit::Rad,
            angle_mark: false,
            answer: false,
            series: vec![Series::default()],
        }
    }
}

impl Plot {
    /// Parses and sanitizes a [`Plot`] from JSON, enforcing size caps.
    pub fn from_json(json: &str) -> Result<Self, String> {
        if json.len() > MAX_JSON_BYTES {
            return Err(format!("plot JSON exceeds {MAX_JSON_BYTES} bytes"));
        }
        let parsed: Plot = serde_json::from_str(json).map_err(|e| format!("invalid plot JSON: {e}"))?;
        Ok(parsed.sanitized())
    }

    /// Canonical compact JSON representation.
    pub fn to_json(&self) -> String {
        let clean = self.clone().sanitized();
        serde_json::to_string(&clean).unwrap_or_else(|_| "{}".to_string())
    }

    /// The abscissa's name as a variable: its label when that is a name (`x`, `E`, `t`), else `x`.
    pub fn x_name(&self) -> String {
        clean_ident(&self.x_label, "x")
    }

    /// The ordinate's name as a variable: its label when that is a name (`y`, `N`, `s`), else `y`.
    pub fn y_name(&self) -> String {
        clean_ident(&self.y_label, "y")
    }

    /// Renames the abscissa (its label): functions written in its old name follow it, so with
    /// the label going from `x` to `E` the function `x^2` becomes `E^2` (whole names only:
    /// `exp(x)` becomes `exp(E)`), and their names with them (`f(x)` → `f(E)`).
    pub fn set_x_label(&mut self, label: &str) {
        let old = self.x_name();
        self.x_label = truncate_str(label, 80);
        let new = self.x_name();
        if old == new {
            return;
        }
        for s in &mut self.series {
            if let SeriesKind::Function { expr, var, .. } = &mut s.kind
                && *var == old
            {
                *expr = rename_ident(expr, &old, &new);
                var.clone_from(&new);
                // Its name too: `f(x)` becomes `f(E)`.
                s.label = rename_ident(&s.label, &old, &new);
            }
        }
    }

    /// Clamps dimensions, bounds, strings and collection lengths into safe finite ranges.
    pub fn sanitized(mut self) -> Self {
        self.width = if self.width.is_finite() { (self.width.clamp(20.0, 500.0) * 100.0).round() / 100.0 } else { default_width_mm() };
        self.height = if self.height.is_finite() { (self.height.clamp(20.0, 500.0) * 100.0).round() / 100.0 } else { default_height_mm() };
        let (x0, x1) = clean_bounds(self.x_min, self.x_max, -5.0, 5.0);
        self.x_min = x0;
        self.x_max = x1;
        let (y0, y1) = clean_bounds(self.y_min, self.y_max, -5.0, 5.0);
        self.y_min = y0;
        self.y_max = y1;
        self.title = truncate_str(&self.title, 160);
        self.x_label = truncate_str(&self.x_label, 80);
        self.y_label = truncate_str(&self.y_label, 80);
        self.x_unit = self.x_unit.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(|s| truncate_str(s, 32));
        self.y_unit = self.y_unit.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(|s| truncate_str(s, 32));
        self.series = self.series.into_iter().take(MAX_SERIES).map(Series::sanitized).collect();
        self
    }
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

fn clean_ident(s: &str, fallback: &str) -> String {
    let trimmed = s.trim();
    let valid = !trimmed.is_empty()
        && trimmed.chars().count() <= 24
        && trimmed.chars().next().is_some_and(char::is_alphabetic)
        && trimmed.chars().all(|c| c.is_alphanumeric() || c == '_');
    if valid { trimmed.to_string() } else { fallback.to_string() }
}

/// `expr` with every whole name `old` written `new` (names are runs of letters, digits and `_`
/// not starting with a digit: in `2x` the name is `x`).
pub(crate) fn rename_ident(expr: &str, old: &str, new: &str) -> String {
    let mut out = String::with_capacity(expr.len());
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        let digits = word.find(|c: char| !c.is_ascii_digit()).unwrap_or(word.len());
        let (num, name) = word.split_at(digits);
        out.push_str(num);
        out.push_str(if name == old { new } else { name });
        word.clear();
    };
    for c in expr.chars() {
        if c.is_alphanumeric() || c == '_' {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

fn clean_bounds(a: f64, b: f64, def_a: f64, def_b: f64) -> (f64, f64) {
    if !(a.is_finite() && b.is_finite()) {
        return (def_a, def_b);
    }
    let lo = a.min(b).clamp(-1e9, 1e9);
    let hi = a.max(b).clamp(-1e9, 1e9);
    if (hi - lo) < 1e-9 { (lo - 1.0, lo + 1.0) } else { (lo, hi) }
}

fn clean_interval(iv: Option<[f64; 2]>) -> Option<[f64; 2]> {
    let [a, b] = iv?;
    if !(a.is_finite() && b.is_finite()) {
        return None;
    }
    let lo = a.min(b).clamp(-1e9, 1e9);
    let hi = a.max(b).clamp(-1e9, 1e9);
    ((hi - lo) >= 1e-9).then_some([lo, hi])
}
