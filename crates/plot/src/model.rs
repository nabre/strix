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
    #[serde(default, skip_serializing_if = "is_false")]
    pub dashed: bool,
    /// When true, omitted on student copies (`Show = false`) while keeping axes/bounds identical.
    #[serde(default, skip_serializing_if = "is_false")]
    pub answer: bool,
    #[serde(flatten)]
    pub kind: SeriesKind,
}

impl Default for Series {
    fn default() -> Self {
        Series { label: String::new(), color: default_color(), width: default_stroke_mm(), dashed: false, answer: false, kind: SeriesKind::default() }
    }
}

impl Series {
    pub(crate) fn sanitized(mut self) -> Self {
        self.label = truncate_str(&self.label, 120);
        self.width = if self.width.is_finite() { self.width.clamp(0.1, 4.0) } else { default_stroke_mm() };
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
            title: String::new(),
            x_label: "x".to_string(),
            y_label: "y".to_string(),
            x_unit: None,
            y_unit: None,
            grid: true,
            minor_grid: false,
            legend: false,
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
