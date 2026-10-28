//! The drawing model and its JSON form.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use strix_calc::{Quantity, Value, Worksheet};

use crate::geom::Pt;
use crate::{MAX_COORD, MAX_ENTITIES, MAX_JSON_BYTES, MAX_LAYERS, MAX_VERTICES};

/// The real-world unit coordinates are in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Unit {
    #[default]
    Mm,
    Cm,
    M,
}

impl Unit {
    /// Millimetres in one unit.
    pub fn mm(self) -> f64 {
        match self {
            Unit::Mm => 1.0,
            Unit::Cm => 10.0,
            Unit::M => 1000.0,
        }
    }

    /// The unit's symbol, as calculations write it.
    pub fn symbol(self) -> &'static str {
        match self {
            Unit::Mm => "mm",
            Unit::Cm => "cm",
            Unit::M => "m",
        }
    }
}

/// A number in the drawing: a literal, or an expression of calculation parameters with its last
/// value. Serialized as a JSON number, or as the expression's text.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Num {
    /// The value in drawing units (degrees for angles).
    pub value: f64,
    /// The expression the value comes from (`"L/2"`), if any.
    pub expr: Option<String>,
}

impl Num {
    pub fn new(value: f64) -> Num {
        Num { value, expr: None }
    }

    pub fn expr(text: &str) -> Num {
        Num { value: 0.0, expr: Some(text.to_string()) }
    }
}

impl From<f64> for Num {
    fn from(v: f64) -> Self {
        Num::new(v)
    }
}

impl Serialize for Num {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match &self.expr {
            Some(e) => s.serialize_str(e),
            // Canonical: to a nanometre of the unit (the kernel's resolution), so the saved
            // bytes follow the content, not the noise, and constructed points stay exact.
            None => s.serialize_f64(crate::kernel::canon(self.value)),
        }
    }
}

impl<'de> Deserialize<'de> for Num {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            N(f64),
            S(String),
        }
        Ok(match Raw::deserialize(d)? {
            Raw::N(v) if v.is_finite() => Num::new(v.clamp(-MAX_COORD, MAX_COORD)),
            Raw::N(_) => Num::new(0.0),
            Raw::S(s) => match s.trim().parse::<f64>() {
                Ok(v) if v.is_finite() => Num::new(v.clamp(-MAX_COORD, MAX_COORD)),
                _ => Num::expr(&s.chars().take(strix_calc::MAX_INPUT_CHARS).collect::<String>()),
            },
        })
    }
}

/// A point `[x, y]`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct P(pub Num, pub Num);

impl P {
    pub fn new(x: f64, y: f64) -> P {
        P(Num::new(x), Num::new(y))
    }

    pub fn pt(&self) -> Pt {
        Pt::new(self.0.value, self.1.value)
    }
}

/// A line pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Dash {
    #[default]
    Solid,
    Dashed,
    Dotted,
    DashDotted,
    /// A long dash and two dots (ISO 128's dash-double-dotted: phantom and centroid lines).
    DashDoubleDotted,
}

impl Dash {
    /// The pattern in paper mm (on, off, …); empty for solid.
    pub fn pattern(self) -> &'static [f64] {
        match self {
            Dash::Solid => &[],
            Dash::Dashed => &[4.0, 2.0],
            Dash::Dotted => &[0.5, 1.5],
            Dash::DashDotted => &[6.0, 1.5, 0.5, 1.5],
            Dash::DashDoubleDotted => &[6.0, 1.5, 0.5, 1.5, 0.5, 1.5],
        }
    }
}

/// A layer: entities on it take its stroke unless they set their own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Layer {
    pub name: String,
    /// RGB.
    pub color: [u8; 3],
    /// Stroke width in paper mm.
    pub width: f64,
    pub dash: Dash,
    pub visible: bool,
    /// Shown and snapped to, but its objects can't be picked or changed.
    #[serde(skip_serializing_if = "is_false")]
    pub locked: bool,
    /// Drawn on the page and in the PDF; when off, only in the editor (construction lines).
    #[serde(skip_serializing_if = "is_true")]
    pub print: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_true(b: &bool) -> bool {
    *b
}

impl Default for Layer {
    fn default() -> Self {
        Layer { name: "0".to_string(), color: [0, 0, 0], width: 0.35, dash: Dash::Solid, visible: true, locked: false, print: true }
    }
}

/// An entity's own stroke; `None` fields come from its layer ("ByLayer").
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Stroke {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<[u8; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dash: Option<Dash>,
}

/// Which way a dimension measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Orient {
    /// Along the line from `a` to `b`.
    #[default]
    Aligned,
    Horizontal,
    Vertical,
}

/// The mark at each end of a dimension line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mark {
    /// An architectural tick (a 45° stroke).
    #[default]
    Tick,
    /// A filled arrowhead.
    Arrow,
    /// A filled dot.
    Dot,
    None,
}

impl Mark {
    pub const ALL: [(Mark, &'static str); 4] = [(Mark::Tick, "tick"), (Mark::Arrow, "arrow"), (Mark::Dot, "dot"), (Mark::None, "none")];

    pub fn name(self) -> &'static str {
        Mark::ALL.iter().find(|(m, _)| *m == self).map_or("tick", |(_, n)| n)
    }

    pub fn parse(s: &str) -> Option<Mark> {
        Mark::ALL.iter().find(|(_, n)| *n == s.trim()).map(|(m, _)| *m)
    }
}

/// Where a text sits on its point, along its line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

impl Align {
    pub const ALL: [(Align, &'static str); 3] = [(Align::Left, "left"), (Align::Center, "center"), (Align::Right, "right")];

    pub fn name(self) -> &'static str {
        Align::ALL.iter().find(|(a, _)| *a == self).map_or("left", |(_, n)| n)
    }

    pub fn parse(s: &str) -> Option<Align> {
        Align::ALL.iter().find(|(_, n)| *n == s.trim()).map(|(a, _)| *a)
    }
}

/// The drawing's annotation style: how its dimensions and new texts look. Paper millimetres, so
/// annotations read the same at any scale.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DimStyle {
    /// Dimension text height (and new texts'), mm.
    pub text: f64,
    /// The dimension lines' end mark, unless a dimension sets its own.
    pub mark: Mark,
    /// The mark's length, mm.
    pub mark_size: f64,
    /// Extension lines start this far from the measured point…
    pub ext_gap: f64,
    /// …and pass the dimension line by this, mm.
    pub ext_over: f64,
    /// Between a dimension's text and its line, mm.
    pub text_gap: f64,
    /// Texts aligned with their dimension line at any slope (turned to stay readable: never
    /// upside down) and an angle's along its arc; off, a steep dimension's text and an angle's
    /// read level (the default). Written only when on.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub aligned: bool,
}

impl Default for DimStyle {
    fn default() -> Self {
        DimStyle { text: 2.5, mark: Mark::Tick, mark_size: 2.5, ext_gap: 1.0, ext_over: 1.5, text_gap: 1.0, aligned: false }
    }
}

impl DimStyle {
    /// Every size finite and within reason (text 0.5–50 mm, the others 0–20 mm).
    pub fn sanitized(self) -> DimStyle {
        let c = |v: f64, lo: f64, hi: f64, fallback: f64| if v.is_finite() { v.clamp(lo, hi) } else { fallback };
        let d = DimStyle::default();
        DimStyle {
            text: c(self.text, 0.5, 50.0, d.text),
            mark: self.mark,
            mark_size: c(self.mark_size, 0.0, 20.0, d.mark_size),
            ext_gap: c(self.ext_gap, 0.0, 20.0, d.ext_gap),
            ext_over: c(self.ext_over, 0.0, 20.0, d.ext_over),
            text_gap: c(self.text_gap, 0.0, 20.0, d.text_gap),
            aligned: self.aligned,
        }
    }

    pub fn is_default(&self) -> bool {
        *self == DimStyle::default()
    }
}

/// What an entity is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Kind {
    Line {
        a: P,
        b: P,
    },
    Circle {
        c: P,
        r: Num,
    },
    /// Counter-clockwise from `start` to `end` degrees.
    Arc {
        c: P,
        r: Num,
        start: Num,
        end: Num,
    },
    /// Vertices with DXF bulges (one per segment; missing means straight).
    Polyline {
        pts: Vec<P>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        bulges: Vec<f64>,
        #[serde(default)]
        closed: bool,
    },
    Point {
        p: P,
    },
    /// Text at `p` (on its baseline, at its left end, middle or right end as `align` says),
    /// `height` paper mm, `angle` degrees.
    Text {
        p: P,
        value: String,
        #[serde(default = "text_height")]
        height: f64,
        #[serde(default)]
        angle: f64,
        #[serde(default, skip_serializing_if = "is_left")]
        align: Align,
    },
    /// A linear dimension between `a` and `b`, its line `offset` paper mm along the left normal
    /// (negative: the other side). `text`: `<>` is the measured size, `{{L}}` the value of
    /// parameter `L`; `None` shows the size. `below`: a second text under the line. `mark`: its
    /// own end mark, else the drawing's style's.
    Dim {
        a: P,
        b: P,
        #[serde(default = "dim_offset")]
        offset: f64,
        #[serde(default)]
        orient: Orient,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        below: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mark: Option<Mark>,
    },
    /// A hatched strip along the line or polyline with id `along` (it follows it), `width` paper
    /// mm wide on its `side`, filled with `pattern`: lines at `angle` degrees to the line,
    /// `spacing` paper mm apart. Annotative: its sizes are paper sizes, the same at any scale.
    Strip {
        along: String,
        #[serde(default = "strip_width")]
        width: f64,
        #[serde(default)]
        side: Side,
        #[serde(default)]
        pattern: Pattern,
        #[serde(default = "strip_angle")]
        angle: f64,
        #[serde(default = "strip_spacing")]
        spacing: f64,
    },
    /// A hatch filling the region bounded by the closed objects with ids `boundary` (the outer
    /// one and its islands, even-odd), so it follows them: `pattern` at `angle` degrees, its
    /// lines `spacing` paper mm apart (annotative).
    Hatch {
        boundary: Vec<String>,
        #[serde(default)]
        pattern: Pattern,
        #[serde(default = "strip_angle")]
        angle: f64,
        #[serde(default = "hatch_spacing")]
        spacing: f64,
    },
    /// An angular dimension at `c`, counter-clockwise from the ray to `a` to the ray to `b`, its
    /// arc `r` drawing units from `c`. `text` as a linear dimension's (`<>` is the angle).
    Angle {
        c: P,
        a: P,
        b: P,
        r: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mark: Option<Mark>,
    },
}

/// Which side of its line a strip lies on, looking along the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    #[default]
    Left,
    Right,
}

/// How a strip is filled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Pattern {
    /// Parallel lines at the angle (the conventional section hatch).
    #[default]
    Lines,
    /// Lines both ways.
    Cross,
    /// Pairs of close lines (steel, metal).
    Steel,
    /// Dashed lines.
    Dashed,
    Solid,
    /// A strip's lines across it at the angle to each segment of its path (ground, insulation).
    Along,
}

impl Pattern {
    pub const ALL: [(Pattern, &'static str); 6] = [
        (Pattern::Lines, "lines"),
        (Pattern::Cross, "cross"),
        (Pattern::Steel, "steel"),
        (Pattern::Dashed, "dashed"),
        (Pattern::Solid, "solid"),
        (Pattern::Along, "along"),
    ];

    pub fn name(self) -> &'static str {
        Pattern::ALL.iter().find(|(p, _)| *p == self).map_or("lines", |(_, n)| n)
    }

    pub fn parse(s: &str) -> Option<Pattern> {
        Pattern::ALL.iter().find(|(_, n)| *n == s.trim()).map(|(p, _)| *p)
    }
}

fn hatch_spacing() -> f64 {
    2.0
}

fn strip_width() -> f64 {
    3.0
}

fn strip_angle() -> f64 {
    45.0
}

fn strip_spacing() -> f64 {
    1.5
}

fn is_left(a: &Align) -> bool {
    *a == Align::Left
}

fn text_height() -> f64 {
    2.5
}

fn dim_offset() -> f64 {
    8.0
}

/// An entity: a stable id (dimensions and edits refer to it), a layer and its own stroke.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    #[serde(default)]
    pub id: String,
    #[serde(default = "layer0", skip_serializing_if = "is_layer0")]
    pub layer: String,
    #[serde(default, skip_serializing_if = "is_plain")]
    pub stroke: Stroke,
    #[serde(flatten)]
    pub kind: Kind,
}

fn layer0() -> String {
    "0".to_string()
}

fn is_layer0(s: &str) -> bool {
    s == "0"
}

fn is_plain(s: &Stroke) -> bool {
    *s == Stroke::default()
}

impl Entity {
    pub fn new(kind: Kind) -> Entity {
        Entity { id: String::new(), layer: layer0(), stroke: Stroke::default(), kind }
    }
}

/// A CAD drawing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Drawing {
    pub unit: Unit,
    /// The scale's denominator: `1:N`. One drawing unit measures `unit.mm() / scale` paper mm.
    pub scale: f64,
    pub layers: Vec<Layer>,
    pub entities: Vec<Entity>,
    /// The picture's padding around the drawing, paper mm.
    pub padding: Padding,
    /// How dimensions and new texts look.
    #[serde(skip_serializing_if = "DimStyle::is_default")]
    pub dim_style: DimStyle,
    /// The space the drawing fits in: when set, the scale is the standard one that fits,
    /// worked out whenever the drawing is drawn (so it follows the calculations).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fit: Option<Fit>,
}

/// A space on paper, mm; 0 leaves that side free.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Fit {
    pub width: f64,
    pub height: f64,
}

impl Fit {
    /// Finite sizes from 0 to 5 m, to a hundredth of a millimetre.
    pub fn sanitized(self) -> Fit {
        let c = |v: f64| if v.is_finite() { (v.clamp(0.0, 5000.0) * 100.0).round() / 100.0 } else { 0.0 };
        Fit { width: c(self.width), height: c(self.height) }
    }
}

impl Default for Drawing {
    fn default() -> Self {
        Drawing {
            unit: Unit::Mm,
            scale: 1.0,
            layers: vec![Layer::default()],
            entities: Vec::new(),
            padding: Padding::default(),
            dim_style: DimStyle::default(),
            fit: None,
        }
    }
}

/// Space between the drawing and the edge of its picture, paper mm: one number for all four
/// sides, or `[top, right, bottom, left]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Padding {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

/// The largest padding, paper mm.
pub const MAX_PADDING: f64 = 100.0;

impl Default for Padding {
    fn default() -> Self {
        Padding::all(2.0)
    }
}

impl Padding {
    pub fn all(v: f64) -> Padding {
        Padding { top: v, right: v, bottom: v, left: v }
    }

    /// Finite, between 0 and [`MAX_PADDING`], to a hundredth of a millimetre.
    pub fn sanitized(self) -> Padding {
        let c = |v: f64| if v.is_finite() { (v.clamp(0.0, MAX_PADDING) * 100.0).round() / 100.0 } else { 2.0 };
        Padding { top: c(self.top), right: c(self.right), bottom: c(self.bottom), left: c(self.left) }
    }

    pub fn is_uniform(&self) -> bool {
        self.top == self.right && self.right == self.bottom && self.bottom == self.left
    }
}

impl Serialize for Padding {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if self.is_uniform() { s.serialize_f64(self.top) } else { [self.top, self.right, self.bottom, self.left].serialize(s) }
    }
}

impl<'de> Deserialize<'de> for Padding {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            One(f64),
            Four([f64; 4]),
            Other(serde::de::IgnoredAny),
        }
        Ok(match Raw::deserialize(d)? {
            Raw::One(v) => Padding::all(v),
            Raw::Four([top, right, bottom, left]) => Padding { top, right, bottom, left },
            Raw::Other(_) => Padding::default(),
        }
        .sanitized())
    }
}

impl Drawing {
    /// Reads a drawing from JSON and makes it safe: limits enforced, layer `0` present, ids
    /// unique and non-empty, sizes finite and clamped.
    pub fn from_json(text: &str) -> Result<Drawing, String> {
        if text.len() > MAX_JSON_BYTES {
            return Err(format!("a drawing is at most {} MB", MAX_JSON_BYTES >> 20));
        }
        let mut d: Drawing = serde_json::from_str(text).map_err(|e| format!("not a drawing: {e}"))?;
        d.sanitize();
        Ok(d)
    }

    /// The canonical JSON: the same drawing always gives the same bytes.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Enforces the limits; see [`Drawing::from_json`].
    pub fn sanitize(&mut self) {
        self.scale = if self.scale.is_finite() { self.scale.clamp(1e-3, 1e6) } else { 1.0 };
        self.padding = self.padding.sanitized();
        self.dim_style = self.dim_style.sanitized();
        self.fit = self.fit.map(Fit::sanitized).filter(|f| f.width > 0.0 || f.height > 0.0);
        self.layers.truncate(MAX_LAYERS);
        if !self.layers.iter().any(|l| l.name == "0") {
            self.layers.insert(0, Layer::default());
        }
        for l in &mut self.layers {
            l.width = clamp_paper(l.width, 0.35);
        }
        self.entities.truncate(MAX_ENTITIES);
        let mut seen = std::collections::HashSet::new();
        let mut next = 1usize;
        for e in &mut self.entities {
            if e.id.is_empty() || e.id.len() > 64 || !seen.insert(e.id.clone()) {
                while !seen.insert(format!("e{next}")) {
                    next += 1;
                }
                e.id = format!("e{next}");
            }
            if let Some(w) = e.stroke.width.as_mut() {
                *w = clamp_paper(*w, 0.35);
            }
            match &mut e.kind {
                Kind::Polyline { pts, bulges, .. } => {
                    pts.truncate(MAX_VERTICES);
                    bulges.truncate(pts.len());
                    bulges.iter_mut().for_each(|b| *b = if b.is_finite() { b.clamp(-1e3, 1e3) } else { 0.0 });
                }
                Kind::Text { value, height, angle, .. } => {
                    value.truncate(value.char_indices().nth(1000).map_or(value.len(), |(i, _)| i));
                    *height = clamp_paper(*height, 2.5).max(0.5);
                    *angle = if angle.is_finite() { *angle % 360.0 } else { 0.0 };
                }
                Kind::Dim { offset, text, below, .. } => {
                    *offset = if offset.is_finite() { offset.clamp(-500.0, 500.0) } else { 8.0 };
                    for t in [text, below].into_iter().flatten() {
                        t.truncate(t.char_indices().nth(200).map_or(t.len(), |(i, _)| i));
                    }
                }
                Kind::Strip { width, angle, spacing, along, .. } => {
                    let c = |v: f64, lo: f64, hi: f64, fallback: f64| if v.is_finite() { v.clamp(lo, hi) } else { fallback };
                    *width = c(*width, 0.1, 100.0, strip_width());
                    *angle = c(*angle, 5.0, 175.0, strip_angle());
                    *spacing = c(*spacing, 0.2, 50.0, strip_spacing());
                    // Cut at a character boundary: an id from a file may be any text.
                    along.truncate(along.char_indices().nth(64).map_or(along.len(), |(i, _)| i));
                }
                Kind::Hatch { boundary, angle, spacing, .. } => {
                    *angle = if angle.is_finite() { angle.rem_euclid(180.0) } else { strip_angle() };
                    *spacing = if spacing.is_finite() { spacing.clamp(0.2, 50.0) } else { hatch_spacing() };
                    boundary.truncate(100);
                    for id in boundary.iter_mut() {
                        id.truncate(id.char_indices().nth(64).map_or(id.len(), |(i, _)| i));
                    }
                }
                Kind::Angle { r, text, .. } => {
                    *r = if r.is_finite() { r.abs().min(MAX_COORD) } else { 0.0 };
                    if let Some(t) = text {
                        t.truncate(t.char_indices().nth(200).map_or(t.len(), |(i, _)| i));
                    }
                }
                _ => {}
            }
        }
    }

    /// The layer called `name`, else layer `0`.
    pub fn layer(&self, name: &str) -> Layer {
        self.layers.iter().find(|l| l.name == name).or_else(|| self.layers.first()).cloned().unwrap_or_default()
    }

    /// Adds an entity with a fresh id and returns the id.
    pub fn add(&mut self, mut e: Entity) -> Result<String, String> {
        if self.entities.len() >= MAX_ENTITIES {
            return Err(format!("a drawing holds at most {MAX_ENTITIES} entities"));
        }
        let mut n = self.entities.len() + 1;
        while self.entities.iter().any(|x| x.id == format!("e{n}")) {
            n += 1;
        }
        e.id = format!("e{n}");
        let id = e.id.clone();
        self.entities.push(e);
        self.sanitize();
        Ok(id)
    }

    /// Every number of the drawing.
    fn nums_mut(&mut self) -> Vec<(&mut Num, NumKind)> {
        let mut out = Vec::new();
        fn point<'a>(p: &'a mut P, out: &mut Vec<(&'a mut Num, NumKind)>) {
            let P(x, y) = p;
            out.push((x, NumKind::Length));
            out.push((y, NumKind::Length));
        }
        for e in &mut self.entities {
            match &mut e.kind {
                Kind::Line { a, b } | Kind::Dim { a, b, .. } => {
                    point(a, &mut out);
                    point(b, &mut out);
                }
                Kind::Circle { c, r } => {
                    point(c, &mut out);
                    out.push((r, NumKind::Length));
                }
                Kind::Arc { c, r, start, end } => {
                    point(c, &mut out);
                    out.push((r, NumKind::Length));
                    out.push((start, NumKind::Angle));
                    out.push((end, NumKind::Angle));
                }
                Kind::Angle { c, a, b, .. } => {
                    point(c, &mut out);
                    point(a, &mut out);
                    point(b, &mut out);
                }
                Kind::Polyline { pts, .. } => pts.iter_mut().for_each(|p| point(p, &mut out)),
                Kind::Strip { .. } | Kind::Hatch { .. } => {}
                Kind::Point { p } | Kind::Text { p, .. } => point(p, &mut out),
            }
        }
        out
    }

    /// Evaluates every expression with the worksheet's parameters (those defined before the
    /// drawing). Length parameters count in the drawing's unit (`L = 4.5 m` is 4500 in a mm
    /// drawing, so `L/2 + 10` is fine); a result with a length unit converts too (`300 mm`); a
    /// plain number is drawing units; an angle is in degrees (`30°`, or a plain number of radians
    /// from a calculation). Returns the expressions that failed, with why; those keep their
    /// last value.
    pub fn resolve(&mut self, ws: &Worksheet) -> Vec<(String, String)> {
        let unit_m = self.unit.mm() / 1000.0;
        let mut errors = Vec::new();
        // Coordinates are numbers in the drawing's unit, as in any CAD: a length parameter counts
        // in that unit (`L - 0.15` in a metre drawing), an angle in degrees stays as it is.
        let mut local = Worksheet::new(*ws.options());
        let length = strix_calc::Dim::new([1, 0, 0, 0, 0, 0, 0]);
        for (name, v) in ws.scope().iter() {
            match v {
                Value::Scalar(q) if q.dim == length => local.define(name, Value::Scalar(Quantity::number(q.value / unit_m))),
                Value::Scalar(q) if q.dim.is_dimensionless() => local.define(name, v.clone()),
                // Other kinds (a force, a vector) keep their unit: using them is an error below.
                other => local.define(name, other.clone()),
            }
        }
        // The document's step budget covers the drawing too; once it is spent, nothing more is
        // evaluated (the remaining expressions keep their last values and are reported).
        let mut spent_out = false;
        for (num, kind) in self.nums_mut() {
            let Some(expr) = num.expr.clone() else { continue };
            if spent_out {
                errors.push((expr, strix_calc::CalcError::TooMuchWork.to_string()));
                continue;
            }
            local.scope().set_spent(ws.scope().spent());
            let result = local.eval_expr(&expr);
            ws.scope().set_spent(local.scope().spent());
            if matches!(result, Err(strix_calc::CalcError::TooMuchWork)) {
                spent_out = true;
            }
            let value = result.map_err(|e| e.to_string()).and_then(|v| match v {
                Value::Scalar(q) => convert(q, kind, unit_m),
                Value::Vector(_) => Err("a coordinate is one value, not a vector".to_string()),
            });
            match value {
                Ok(v) => num.value = v,
                Err(e) => errors.push((expr, e)),
            }
        }
        errors
    }

    /// The text of a dimension label: `<>` replaced by `measured` (drawing units, shown with the
    /// unit), `{{name}}` by the parameter's value as calculations show it.
    pub fn label(&self, text: Option<&str>, measured: f64, ws: Option<&Worksheet>) -> String {
        let opts = ws.map(|w| *w.options()).unwrap_or_default();
        let size = strix_calc::format_number(measured, &opts);
        fill(&text.unwrap_or("<>").replace("<>", &size), ws)
    }
}

/// `text` with every `{{name}}` placeholder filled from the worksheet (docs/cad.md):
/// `{{L}}` or `{{L:value}}` is the value with its unit (`4.5 m`), `{{L:symbol}}` the symbol (`L`),
/// `{{L:both}}` both (`L = 4.5 m`). A unit is marked for the Unit style; a parameter that is not
/// defined there shows as `[L?]`, marked as missing.
pub fn fill(text: &str, ws: Option<&Worksheet>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    for _ in 0..64 {
        let Some(i) = rest.find("{{") else { break };
        let Some(j) = rest.get(i..).and_then(|r| r.find("}}")).map(|j| i + j) else { break };
        out.push_str(rest.get(..i).unwrap_or_default());
        let inner = rest.get(i + 2..j).unwrap_or_default().trim();
        let (name, mode) = inner.split_once(':').map_or((inner, "value"), |(n, m)| (n.trim(), m.trim()));
        let value = ws.and_then(|w| w.shown_parts(name)).map(|(number, unit)| match unit.as_str() {
            "" => number,
            "°" => format!("{number}{}°{}", crate::UNIT_START, crate::UNIT_END),
            _ => format!("{number} {}{unit}{}", crate::UNIT_START, crate::UNIT_END),
        });
        match (value, mode) {
            (None, _) => out.push_str(&format!("{}[{name}?]{}", crate::MISSING_START, crate::MISSING_END)),
            (Some(_), "symbol") => out.push_str(name),
            (Some(v), "both") => out.push_str(&format!("{name} = {v}")),
            (Some(v), _) => out.push_str(&v),
        }
        rest = rest.get(j + 2..).unwrap_or_default();
    }
    out.push_str(rest);
    out
}

#[derive(Clone, Copy)]
enum NumKind {
    Length,
    Angle,
}

fn convert(q: Quantity, kind: NumKind, unit_m: f64) -> Result<f64, String> {
    let length = strix_calc::Dim::new([1, 0, 0, 0, 0, 0, 0]);
    let v = match kind {
        NumKind::Length if q.dim == length => q.value / unit_m,
        NumKind::Length if q.dim.is_dimensionless() => q.value,
        NumKind::Angle if q.dim.is_dimensionless() => q.value.to_degrees(),
        NumKind::Length => return Err(format!("a coordinate is a length, not {}", strix_calc::display_unit(q.dim))),
        NumKind::Angle => return Err("an angle is a plain number or degrees".to_string()),
    };
    if v.is_finite() { Ok(v.clamp(-MAX_COORD, MAX_COORD)) } else { Err("the value is not finite".to_string()) }
}

/// A paper size in mm, or `fallback` when not finite; clamped to 0–20 mm.
fn clamp_paper(v: f64, fallback: f64) -> f64 {
    if v.is_finite() { v.clamp(0.0, 20.0) } else { fallback }
}
