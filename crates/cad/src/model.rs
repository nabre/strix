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

    pub fn parse(s: &str) -> Option<Unit> {
        match s.trim() {
            "mm" => Some(Unit::Mm),
            "cm" => Some(Unit::Cm),
            "m" => Some(Unit::M),
            _ => None,
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

    /// A plain zero (no expression): left out of the JSON where zero is the default. Zero as it
    /// is written ([`crate::kernel::canon`]): a value too small to write is written `0`, and
    /// saving the drawing read back would add it.
    pub fn is_zero(&self) -> bool {
        crate::kernel::canon(self.value) == 0.0 && self.expr.is_none()
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
    /// Solution-only layer: omitted on student copies (`Show = false`) while still counting in
    /// the drawing's bounds and fitted scale so pagination never shifts.
    #[serde(default, skip_serializing_if = "is_false")]
    pub answer: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_true(b: &bool) -> bool {
    *b
}

impl Default for Layer {
    fn default() -> Self {
        Layer { name: "0".to_string(), color: [0, 0, 0], width: 0.35, dash: Dash::Solid, visible: true, locked: false, print: true, answer: false }
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

/// Which ends of an open line, arc or polyline carry an arrowhead (a force, a load, a
/// direction): `"arrows": "end"` in the JSON. Closed shapes and other objects ignore it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Ends {
    #[default]
    None,
    Start,
    End,
    Both,
}

impl Ends {
    pub const ALL: [(Ends, &'static str); 4] = [(Ends::None, "none"), (Ends::Start, "start"), (Ends::End, "end"), (Ends::Both, "both")];

    pub fn name(self) -> &'static str {
        Ends::ALL.iter().find(|(e, _)| *e == self).map_or("none", |(_, n)| n)
    }

    pub fn parse(s: &str) -> Option<Ends> {
        Ends::ALL.iter().find(|(_, n)| *n == s.trim()).map(|(e, _)| *e)
    }

    pub fn start(self) -> bool {
        matches!(self, Ends::Start | Ends::Both)
    }

    pub fn end(self) -> bool {
        matches!(self, Ends::End | Ends::Both)
    }

    /// The arrows a piece of the path keeps: its start's if it starts where the path did, its
    /// end's if it ends where the path did.
    pub fn piece(self, at_start: bool, at_end: bool) -> Ends {
        match (self.start() && at_start, self.end() && at_end) {
            (true, true) => Ends::Both,
            (true, false) => Ends::Start,
            (false, true) => Ends::End,
            (false, false) => Ends::None,
        }
    }

    /// The same arrows on the path walked the other way.
    pub fn reversed(self) -> Ends {
        match self {
            Ends::Start => Ends::End,
            Ends::End => Ends::Start,
            other => other,
        }
    }
}

fn is_no_ends(e: &Ends) -> bool {
    *e == Ends::None
}

/// How linear dimensions show their size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DimUnits {
    /// In the drawing's unit.
    #[default]
    Drawing,
    /// All in metres, centimetres or millimetres.
    M,
    Cm,
    Mm,
    /// As SIA 400 (Swiss drawing practice): metres with two decimals from 1 m up, centimetres
    /// below.
    Sia,
}

impl DimUnits {
    pub const ALL: [(DimUnits, &'static str, &'static str); 5] = [
        (DimUnits::Drawing, "drawing", "Drawing unit"),
        (DimUnits::M, "m", "Metres"),
        (DimUnits::Cm, "cm", "Centimetres"),
        (DimUnits::Mm, "mm", "Millimetres"),
        (DimUnits::Sia, "sia", "SIA 400: m from 1 m, cm below"),
    ];

    pub fn name(self) -> &'static str {
        DimUnits::ALL.iter().find(|(u, ..)| *u == self).map_or("drawing", |(_, n, _)| n)
    }

    /// Its label (English; the UI translates it).
    pub fn label(self) -> &'static str {
        DimUnits::ALL.iter().find(|(u, ..)| *u == self).map_or("Drawing unit", |(.., l)| l)
    }

    pub fn parse(s: &str) -> Option<DimUnits> {
        DimUnits::ALL.iter().find(|(_, n, _)| *n == s.trim()).map(|(u, ..)| *u)
    }

    fn is_drawing(&self) -> bool {
        *self == DimUnits::Drawing
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

/// The name of the style every drawing has: [`Drawing::dim_style`] for dimensions, the plain
/// font for texts. Named styles never take it.
pub const STANDARD: &str = "Standard";

/// Most named text styles, and dimension styles, a drawing keeps.
pub const MAX_STYLES: usize = 64;

/// A dimension (or level, or angle) style: how dimensions look. Paper millimetres, so annotations
/// read the same at any scale. The drawing's own ([`Drawing::dim_style`]) is the Standard style;
/// others are named ([`Drawing::dim_styles`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DimStyle {
    /// Dimension text height (and new texts'), mm.
    pub text: f64,
    /// The mark at a dimension line's start (and at its end, unless `mark_end` says otherwise),
    /// unless a dimension sets its own.
    #[serde(deserialize_with = "lenient")]
    pub mark: Mark,
    /// The mark at the dimension line's end; `None`: the start's.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub mark_end: Option<Mark>,
    /// The mark's length, mm.
    pub mark_size: f64,
    /// The marks' line width, mm; 0: automatic (a tick heavier than the dimension line, an
    /// arrowhead as thin). Written only when set.
    #[serde(skip_serializing_if = "is_zero_f64")]
    pub mark_width: f64,
    /// The text style of the dimension texts (its font, bold and italic; the height is the
    /// style's `text`); `None`: Standard.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_style: Option<String>,
    /// Extension lines start this far from the measured point…
    pub ext_gap: f64,
    /// …and pass the dimension line by this, mm.
    pub ext_over: f64,
    /// Between a dimension's text and its line, mm.
    pub text_gap: f64,
    /// Dimension, extension and leader lines' width, mm: thin, lighter than the drawing's
    /// lines (a dimension's own stroke width wins). Ticks are drawn heavier.
    pub line_width: f64,
    /// How far a dimension line runs past its extension lines, mm, with ticks and dots (as
    /// architectural drawings do; never with arrows).
    pub line_over: f64,
    /// Texts aligned with their dimension line at any slope (turned to stay readable: never
    /// upside down) and an angle's along its arc (the default, as ISO 129 draws them); off, a
    /// steep dimension's text and an angle's read level. Written only when off.
    #[serde(skip_serializing_if = "is_true")]
    pub aligned: bool,
    /// Where the editor puts a dimension line or an angle's arc placed with a click or a grip:
    /// in the drawing (scaled: it follows Scale and the drawing's scale), or on paper (off, the
    /// default: annotative, the same in print). The other part is kept. Written only when on.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub scaled: bool,
    /// How linear dimensions show their size (the drawing's unit, m, cm, mm or SIA 400).
    /// Written only when not the drawing's unit.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "DimUnits::is_drawing")]
    pub units: DimUnits,
    /// Linear dimension texts end with their unit (`3.50 m`); off (the default), the number
    /// alone. Written only when on.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub show_unit: bool,
    /// Level marks' text height, mm; 0 (the default): the dimension text height. Written only
    /// when set.
    #[serde(skip_serializing_if = "is_zero_f64")]
    pub level_text: f64,
    /// Level marks' triangle height, mm; 0 (the default): automatic (a text and a fifth high).
    /// Written only when set.
    #[serde(skip_serializing_if = "is_zero_f64")]
    pub level_mark: f64,
    /// Level marks' elevations end with their unit (`+3.00 m`); off (the default), the number
    /// alone. Written only when on.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub level_unit: bool,
}

impl Default for DimStyle {
    fn default() -> Self {
        // ISO 129 / ISO 128 practice: 2.5 mm text, thin 0.18 mm lines, 3 mm ticks drawn
        // heavier, extension lines from 1 mm off the object to 2 mm past the dimension line.
        DimStyle {
            text: 2.5,
            mark: Mark::Tick,
            mark_end: None,
            mark_size: 3.0,
            mark_width: 0.0,
            text_style: None,
            ext_gap: 1.0,
            ext_over: 2.0,
            text_gap: 1.0,
            line_width: 0.18,
            line_over: 1.5,
            aligned: true,
            scaled: false,
            units: DimUnits::Drawing,
            show_unit: false,
            level_text: 0.0,
            level_mark: 0.0,
            level_unit: false,
        }
    }
}

impl DimStyle {
    /// Every size finite and within reason (text 0.5–50 mm, line width 0.05–2 mm, the others
    /// 0–20 mm).
    pub fn sanitized(mut self) -> DimStyle {
        let c = |v: f64, lo: f64, hi: f64, fallback: f64| if v.is_finite() { v.clamp(lo, hi) } else { fallback };
        let d = DimStyle::default();
        DimStyle {
            text: c(self.text, 0.5, 50.0, d.text),
            mark: self.mark,
            mark_end: self.mark_end,
            mark_size: c(self.mark_size, 0.0, 20.0, d.mark_size),
            mark_width: c(self.mark_width, 0.0, 2.0, 0.0),
            text_style: self.text_style.take().map(|n| cut_name(&n)).filter(|n| !n.trim().is_empty() && n != STANDARD),
            ext_gap: c(self.ext_gap, 0.0, 20.0, d.ext_gap),
            ext_over: c(self.ext_over, 0.0, 20.0, d.ext_over),
            text_gap: c(self.text_gap, 0.0, 20.0, d.text_gap),
            line_width: c(self.line_width, 0.05, 2.0, d.line_width),
            line_over: c(self.line_over, 0.0, 20.0, d.line_over),
            aligned: self.aligned,
            scaled: self.scaled,
            units: self.units,
            show_unit: self.show_unit,
            level_text: c(self.level_text, 0.0, 50.0, 0.0),
            level_mark: c(self.level_mark, 0.0, 50.0, 0.0),
            level_unit: self.level_unit,
        }
    }

    /// The level marks' text height, mm (their own, else the dimension text's).
    pub fn level_text_height(&self) -> f64 {
        if self.level_text > 0.0 { self.level_text } else { self.text }
    }

    /// The level marks' triangle height, mm (their own, else a text and a fifth, at least 1 mm).
    pub fn level_mark_height(&self) -> f64 {
        if self.level_mark > 0.0 { self.level_mark } else { (self.level_text_height() * 1.2).max(1.0) }
    }

    pub fn is_default(&self) -> bool {
        *self == DimStyle::default()
    }

    /// The mark at the dimension line's end.
    pub fn end_mark(&self) -> Mark {
        self.mark_end.unwrap_or(self.mark)
    }
}

/// A name (a style's, a layer's) cut to 64 characters at a character boundary.
/// Most levels of groups one entity is in ([`Entity::group`]).
pub(crate) const MAX_GROUP_DEPTH: usize = 16;

pub(crate) fn cut_name(name: &str) -> String {
    name.chars().take(64).collect()
}

/// A named dimension style.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NamedDimStyle {
    pub name: String,
    pub style: DimStyle,
}

/// A text style: the font, height and weight of the texts that take it (and of the dimension
/// texts of the dimension styles that name it). Standard is the plain font, the dimension
/// style's text height.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextStyle {
    pub name: String,
    /// The font family; `None`: the document's (the drawing's plain font).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
    /// The height new texts take, paper mm.
    pub height: f64,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub bold: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub italic: bool,
}

impl Default for TextStyle {
    fn default() -> Self {
        TextStyle { name: String::new(), font: None, height: text_height(), bold: false, italic: false }
    }
}

impl TextStyle {
    fn sanitized(mut self) -> TextStyle {
        self.name = cut_name(&self.name);
        self.font = self.font.map(|f| f.chars().take(128).collect::<String>()).filter(|f| !f.trim().is_empty());
        self.height = if self.height.is_finite() { self.height.clamp(0.5, 50.0) } else { text_height() };
        self
    }
}

/// How a text is set: its font (`None`: the plain one), bold, italic.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TextLook {
    pub font: Option<String>,
    pub bold: bool,
    pub italic: bool,
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
    /// A linear dimension between `a` and `b`, its line `offset` paper mm (annotative: the same
    /// in print at any scale) plus `offset_units` drawing units (scaled: it follows the drawing's
    /// size and scale) along the left normal (a negative sum: the other side). `text`: `<>` is the measured size, `{{L}}` the value of
    /// parameter `L`; `None` shows the size. `below`: a second text under the line. `mark`: its
    /// own end mark, else the drawing's style's.
    Dim {
        a: P,
        b: P,
        #[serde(default = "dim_offset")]
        offset: f64,
        #[serde(default, rename = "offsetUnits", skip_serializing_if = "is_zero_f64")]
        offset_units: f64,
        #[serde(default)]
        orient: Orient,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        below: Option<String>,
        #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
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
        /// Slope ticks (the `slope` pattern): the short ticks' length as a part of the long
        /// ones' (`width`), 0.1–1. Written only when not a half.
        #[serde(default = "strip_ratio", skip_serializing_if = "is_half")]
        ratio: f64,
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
    /// arc `r` drawing units (scaled) plus `r_paper` paper mm (annotative) from `c`. `text` as a linear dimension's (`<>` is the angle).
    Angle {
        c: P,
        a: P,
        b: P,
        r: f64,
        #[serde(default, rename = "rPaper", skip_serializing_if = "is_zero_f64")]
        r_paper: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
        mark: Option<Mark>,
    },
    /// A level (elevation) mark at `p`: a triangle with its tip on the level, a line along it and
    /// the elevation over it: `p`'s height above `datum` (drawing units; the ±0.00 level) in
    /// metres, signed, `±0.00` at the datum. `text` as a dimension's (`<>` is the elevation).
    /// `flip`: the mark under the level, tip up (a ceiling, a soffit). `left`: the line and the
    /// text go left of the tip.
    Level {
        p: P,
        #[serde(default, skip_serializing_if = "Num::is_zero")]
        datum: Num,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        flip: bool,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        left: bool,
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
    /// Slope ticks (a cut's crest, an embankment): ticks square to the path on the strip's side,
    /// long (the strip's width) and short (`ratio` of it) in turn, `spacing` apart.
    Slope,
}

impl Pattern {
    pub const ALL: [(Pattern, &'static str); 7] = [
        (Pattern::Lines, "lines"),
        (Pattern::Cross, "cross"),
        (Pattern::Steel, "steel"),
        (Pattern::Dashed, "dashed"),
        (Pattern::Solid, "solid"),
        (Pattern::Along, "along"),
        (Pattern::Slope, "slope"),
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

pub(crate) fn strip_ratio() -> f64 {
    0.5
}

fn is_half(v: &f64) -> bool {
    *v == 0.5
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

/// Reads a choice leniently: a word this version doesn't know (a newer file, a typo) is the
/// default, not a drawing that cannot be opened.
fn lenient<'de, D: serde::Deserializer<'de>, T: serde::de::DeserializeOwned + Default>(d: D) -> Result<T, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    Ok(T::deserialize(v).unwrap_or_default())
}

fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
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
    /// Arrowheads at the ends of an open line, arc or polyline.
    #[serde(default, skip_serializing_if = "is_no_ends")]
    pub arrows: Ends,
    /// The named style it takes: a text's text style, a dimension's (angle's, level mark's)
    /// dimension style; `None` (or a name the drawing doesn't have): Standard.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    /// A dimension's or level mark's text moved from its place by `[dx, dy]` drawing units (in
    /// scale: it follows the drawing), with a leader back when it is off its line; `None`: in
    /// its place. The measured value never changes.
    #[serde(default, rename = "textShift", skip_serializing_if = "Option::is_none")]
    pub text_shift: Option<[f64; 2]>,
    /// The groups it belongs to, outermost first, `/` between them (`g3/g1`: an array, `g3`,
    /// of block insertions or arrays, one of them `g1`). Picking one picks its outermost group
    /// whole; Explode takes that level apart. `None`: on its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Solution-only object: omitted on student copies (`Show = false`) while still counting in
    /// the drawing's bounds and fitted scale so pagination never shifts.
    #[serde(default, skip_serializing_if = "is_false")]
    pub answer: bool,
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
        Entity {
            id: String::new(),
            layer: layer0(),
            stroke: Stroke::default(),
            arrows: Ends::None,
            style: None,
            text_shift: None,
            group: None,
            answer: false,
            kind,
        }
    }
}

/// A CAD drawing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Drawing {
    pub unit: Unit,
    /// The unit plain numbers and length parameters mean in this drawing's expressions, when not
    /// [`Drawing::unit`]: a drawing converted to millimetres from centimetres or metres
    /// ([`Drawing::to_mm`]) keeps what its expressions say (`L/2 + 0.1` in metres).
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub expr_unit: Option<Unit>,
    /// The unit numbers typed in the editor are read in (the command line's points, distances
    /// and radii), converted to [`Drawing::unit`]; `None`: the drawing's unit. A value written
    /// with its unit (`2.5m`) is read in that.
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub input: Option<Unit>,
    /// The scale's denominator: `1:N`. One drawing unit measures `unit.mm() / scale` paper mm.
    pub scale: f64,
    pub layers: Vec<Layer>,
    pub entities: Vec<Entity>,
    /// The picture's padding around the drawing, paper mm.
    pub padding: Padding,
    /// How dimensions and new texts look: the Standard dimension style.
    #[serde(skip_serializing_if = "DimStyle::is_default")]
    pub dim_style: DimStyle,
    /// The named dimension styles (besides Standard).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dim_styles: Vec<NamedDimStyle>,
    /// The named text styles (besides Standard).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub text_styles: Vec<TextStyle>,
    /// The dimension style new dimensions take; `None`: Standard.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_dim_style: Option<String>,
    /// The text style new texts take; `None`: Standard.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_text_style: Option<String>,
    /// The drawing's maximum size: when set, the scale is the standard one that fits, worked
    /// out whenever the drawing is drawn (so it follows the calculations), or one picked by hand
    /// among those that fit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fit: Option<Fit>,
    /// A stable name other drawings of the document refer to it by (an underlay's source);
    /// given when it is first needed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Another drawing of the document shown under this one to trace over
    /// ([`crate::Underlay`]): a reference by id, never a copy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underlay: Option<crate::Underlay>,
    /// The block definitions ([`crate::Block`]).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<crate::Block>,
    /// The blocks' insertions: which group of entities is which block, inserted where.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub inserts: Vec<crate::Insert>,
    /// Texts' widths as an editor measured them with its fonts ([`crate::TextWidths`]); never
    /// saved, never compared.
    #[serde(skip)]
    pub text_widths: crate::TextWidths,
}

/// The drawing's maximum size on paper, mm (0 leaves that side free), and how the scale is
/// chosen in it: fitted (the largest standard scale that fits, the default) or `fixed` (picked
/// by hand among those that fit; the size stays a limit).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Fit {
    pub width: f64,
    pub height: f64,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub fixed: bool,
}

impl Fit {
    /// Finite sizes from 0 to 5 m, to a hundredth of a millimetre.
    pub fn sanitized(self) -> Fit {
        let c = |v: f64| if v.is_finite() { (v.clamp(0.0, 5000.0) * 100.0).round() / 100.0 } else { 0.0 };
        Fit { width: c(self.width), height: c(self.height), fixed: self.fixed }
    }
}

impl Default for Drawing {
    fn default() -> Self {
        Drawing {
            unit: Unit::Mm,
            expr_unit: None,
            input: None,
            scale: 1.0,
            layers: vec![Layer::default()],
            entities: Vec::new(),
            padding: Padding::default(),
            dim_style: DimStyle::default(),
            dim_styles: Vec::new(),
            text_styles: Vec::new(),
            current_dim_style: None,
            current_text_style: None,
            fit: None,
            id: None,
            underlay: None,
            blocks: Vec::new(),
            inserts: Vec::new(),
            text_widths: crate::TextWidths::default(),
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

    /// The canonical JSON: the same drawing always gives the same bytes, and they always read
    /// back. It is written made safe first: a number an edit left non-finite would be written
    /// `null`, and the saved drawing would no longer open.
    pub fn to_json(&self) -> String {
        let mut safe = self.clone();
        safe.sanitize();
        serde_json::to_string(&safe).unwrap_or_default()
    }

    /// Enforces the limits; see [`Drawing::from_json`].
    pub fn sanitize(&mut self) {
        self.scale = if self.scale.is_finite() { self.scale.clamp(1e-3, 1e6) } else { 1.0 };
        self.padding = self.padding.sanitized();
        self.dim_style = std::mem::take(&mut self.dim_style).sanitized();
        self.sanitize_styles();
        self.fit = self.fit.map(Fit::sanitized).filter(|f| f.width > 0.0 || f.height > 0.0);
        self.id = self.id.take().map(|n| cut_name(&n)).filter(|n| !n.trim().is_empty());
        // Never its own underlay.
        self.underlay = self.underlay.take().map(crate::Underlay::sanitized).filter(|u| Some(&u.source) != self.id.as_ref() && !u.source.is_empty());
        self.layers.truncate(MAX_LAYERS);
        if !self.layers.iter().any(|l| l.name == "0") {
            self.layers.insert(0, Layer::default());
        }
        // Cut at a character boundary, as ids: a name from a file may be any text, and each
        // entity's layer is looked up by it.
        let cut = |t: &mut String| t.truncate(t.char_indices().nth(64).map_or(t.len(), |(i, _)| i));
        for l in &mut self.layers {
            cut(&mut l.name);
            l.width = clamp_paper(l.width, 0.35);
        }
        self.entities.truncate(MAX_ENTITIES);
        let mut seen = std::collections::HashSet::new();
        let mut next = 1usize;
        for e in &mut self.entities {
            // 64 characters, as references (`along`, `boundary`) are cut: an id of 40 accented
            // letters stays the id its strip names.
            if e.id.is_empty() || e.id.chars().count() > 64 || !seen.insert(e.id.clone()) {
                while !seen.insert(format!("e{next}")) {
                    next += 1;
                }
                e.id = format!("e{next}");
            }
            cut(&mut e.layer);
            if let Some(s) = e.style.as_mut() {
                cut(s);
            }
            // A group path (`outer/inner`): each name cut, none empty, at most MAX_GROUP_DEPTH.
            e.group = e.group.take().and_then(|g| {
                let path: Vec<String> =
                    g.split(crate::series::GROUP_SEP).map(|seg| cut_name(seg.trim())).filter(|seg| !seg.is_empty()).take(MAX_GROUP_DEPTH).collect();
                (!path.is_empty()).then(|| path.join(&crate::series::GROUP_SEP.to_string()))
            });
            if let Some(w) = e.stroke.width.as_mut() {
                *w = clamp_paper(*w, 0.35);
            }
            // A text shift only on what has a text to move, finite and within the limits.
            if !matches!(e.kind, Kind::Dim { .. } | Kind::Level { .. }) {
                e.text_shift = None;
            }
            e.text_shift = e
                .text_shift
                .filter(|s| s.iter().all(|v| v.is_finite()))
                .map(|s| s.map(|v| v.clamp(-MAX_COORD, MAX_COORD)))
                .filter(|s| s.iter().any(|v| *v != 0.0));
            match &mut e.kind {
                Kind::Polyline { pts, bulges, .. } => {
                    pts.truncate(MAX_VERTICES);
                    bulges.truncate(pts.len());
                    bulges.iter_mut().for_each(|b| *b = if b.is_finite() { b.clamp(-1e3, 1e3) } else { 0.0 });
                }
                Kind::Text { value, height, angle, .. } => {
                    value.truncate(value.char_indices().nth(1000).map_or(value.len(), |(i, _)| i));
                    // As the Properties pane and the dimension style allow: 0.5–50 mm.
                    *height = if height.is_finite() { height.clamp(0.5, 50.0) } else { 2.5 };
                    *angle = if angle.is_finite() { *angle % 360.0 } else { 0.0 };
                }
                Kind::Dim { offset, offset_units, text, below, .. } => {
                    *offset = if offset.is_finite() { offset.clamp(-500.0, 500.0) } else { 8.0 };
                    *offset_units = if offset_units.is_finite() { offset_units.clamp(-MAX_COORD, MAX_COORD) } else { 0.0 };
                    for t in [text, below].into_iter().flatten() {
                        t.truncate(t.char_indices().nth(200).map_or(t.len(), |(i, _)| i));
                    }
                }
                Kind::Strip { width, angle, spacing, along, ratio, .. } => {
                    let c = |v: f64, lo: f64, hi: f64, fallback: f64| if v.is_finite() { v.clamp(lo, hi) } else { fallback };
                    *width = c(*width, 0.1, 100.0, strip_width());
                    *angle = c(*angle, 5.0, 175.0, strip_angle());
                    *spacing = c(*spacing, 0.2, 50.0, strip_spacing());
                    *ratio = c(*ratio, 0.1, 1.0, strip_ratio());
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
                Kind::Angle { r, r_paper, text, .. } => {
                    *r = if r.is_finite() { r.abs().min(MAX_COORD) } else { 0.0 };
                    *r_paper = if r_paper.is_finite() { r_paper.clamp(0.0, 500.0) } else { 0.0 };
                    if let Some(t) = text {
                        t.truncate(t.char_indices().nth(200).map_or(t.len(), |(i, _)| i));
                    }
                }
                Kind::Level { text: Some(t), .. } => t.truncate(t.char_indices().nth(200).map_or(t.len(), |(i, _)| i)),
                _ => {}
            }
        }
        // Coordinates an edit pushed past the limits (a huge Scale factor, an array spaced
        // far apart) are kept as they will be saved and read back: within ±MAX_COORD, never
        // infinite.
        for (n, _) in self.nums_mut() {
            n.value = if n.value.is_finite() { n.value.clamp(-MAX_COORD, MAX_COORD) } else { 0.0 };
        }
        self.sanitize_blocks();
    }

    /// Named styles: at most [`MAX_STYLES`] of each, names cut, unique, never empty or Standard;
    /// a current style the drawing doesn't have is Standard.
    fn sanitize_styles(&mut self) {
        let mut seen = std::collections::HashSet::new();
        let dims = std::mem::take(&mut self.dim_styles);
        self.dim_styles = dims
            .into_iter()
            .map(|s| NamedDimStyle { name: cut_name(s.name.trim()), style: s.style.sanitized() })
            .filter(|s| !s.name.is_empty() && s.name != STANDARD && seen.insert(s.name.clone()))
            .take(MAX_STYLES)
            .collect();
        let mut seen = std::collections::HashSet::new();
        let texts = std::mem::take(&mut self.text_styles);
        self.text_styles = texts
            .into_iter()
            .map(|s| TextStyle { name: s.name.trim().to_string(), ..s }.sanitized())
            .filter(|s| !s.name.is_empty() && s.name != STANDARD && seen.insert(s.name.clone()))
            .take(MAX_STYLES)
            .collect();
        let dim_names: Vec<String> = self.dim_styles.iter().map(|s| s.name.clone()).collect();
        let text_names: Vec<String> = self.text_styles.iter().map(|s| s.name.clone()).collect();
        self.current_dim_style = self.current_dim_style.take().filter(|n| dim_names.contains(n));
        self.current_text_style = self.current_text_style.take().filter(|n| text_names.contains(n));
    }

    /// The dimension style called `name`: Standard for `None`, Standard or a name the drawing
    /// doesn't have.
    pub fn dim_style_named(&self, name: Option<&str>) -> &DimStyle {
        name.and_then(|n| self.dim_styles.iter().find(|s| s.name == n)).map_or(&self.dim_style, |s| &s.style)
    }

    /// The dimension style entity `e` is drawn with.
    pub fn dim_style_of(&self, e: &Entity) -> &DimStyle {
        self.dim_style_named(e.style.as_deref())
    }

    /// The dimension style new dimensions take.
    pub fn current_dim(&self) -> &DimStyle {
        self.dim_style_named(self.current_dim_style.as_deref())
    }

    /// The text style called `name`; `None` for Standard (or a name the drawing doesn't have).
    pub fn text_style_named(&self, name: Option<&str>) -> Option<&TextStyle> {
        name.and_then(|n| self.text_styles.iter().find(|s| s.name == n))
    }

    /// How texts of the text style `name` are set.
    pub fn text_look(&self, name: Option<&str>) -> TextLook {
        self.text_style_named(name).map_or_else(TextLook::default, |s| TextLook { font: s.font.clone(), bold: s.bold, italic: s.italic })
    }

    /// The height new texts take: the current text style's, else the Standard dimension style's
    /// text height.
    pub fn new_text_height(&self) -> f64 {
        self.text_style_named(self.current_text_style.as_deref()).map_or(self.dim_style.text, |s| s.height)
    }

    /// Converts the drawing to millimetres, the unit drawings are drawn in: every length (the
    /// drawing keeps its real size), the underlay's offset; dimension styles showing the
    /// drawing's unit keep showing the old one; expressions keep meaning the old unit
    /// ([`Drawing::expr_unit`]). Nothing changes in a millimetre drawing.
    pub fn to_mm(&mut self) {
        let old = self.unit;
        if old == Unit::Mm {
            return;
        }
        let k = old.mm() / Unit::Mm.mm();
        let shown = match old {
            Unit::M => DimUnits::M,
            _ => DimUnits::Cm,
        };
        for st in std::iter::once(&mut self.dim_style).chain(self.dim_styles.iter_mut().map(|n| &mut n.style)) {
            if st.units == DimUnits::Drawing {
                st.units = shown;
            }
        }
        if let Some(u) = self.underlay.as_mut() {
            u.dx *= k;
            u.dy *= k;
        }
        let has_expr = self.nums_mut().iter().any(|(n, _)| n.expr.is_some());
        if has_expr && self.expr_unit.is_none() {
            self.expr_unit = Some(old);
        }
        self.set_unit(Unit::Mm, true);
    }

    /// Changes the drawing's unit. With `convert` every length is converted (the drawing keeps its
    /// real size: 1000 mm becomes 1 m) and so is the scale's paper size; without, the numbers stay
    /// and mean the new unit (the drawing grows or shrinks). Expressions are left as they are:
    /// they are worked out in the new unit when the drawing is next resolved.
    pub fn set_unit(&mut self, unit: Unit, convert: bool) {
        if unit == self.unit {
            return;
        }
        if convert {
            let k = self.unit.mm() / unit.mm();
            for (n, kind) in self.nums_mut() {
                if kind == NumKind::Length && n.expr.is_none() {
                    n.value *= k;
                }
            }
            for e in &mut self.entities {
                if let Some(s) = e.text_shift.as_mut() {
                    *s = s.map(|v| v * k);
                }
                match &mut e.kind {
                    Kind::Dim { offset_units, .. } => *offset_units *= k,
                    Kind::Angle { r, .. } => *r *= k,
                    _ => {}
                }
            }
            for r in &mut self.inserts {
                r.at = r.at.map(|v| v * k);
            }
        }
        self.unit = unit;
        if self.input == Some(unit) {
            self.input = None;
        }
        self.sanitize();
    }

    /// The layer called `name`, else layer `0`.
    pub fn layer(&self, name: &str) -> Layer {
        let zero = || self.layers.iter().find(|l| l.name == "0");
        self.layers.iter().find(|l| l.name == name).or_else(zero).or_else(|| self.layers.first()).cloned().unwrap_or_default()
    }

    /// Adds an entity with a fresh id and returns the id.
    pub fn add(&mut self, e: Entity) -> Result<String, String> {
        self.add_all(vec![e]).pop().flatten().ok_or_else(|| format!("a drawing holds at most {MAX_ENTITIES} entities"))
    }

    /// Adds entities, each with a fresh id as [`Drawing::add`] gives it, and makes the drawing
    /// safe once: each one's id, `None` for those past the limit (left out). Ids are looked up
    /// in a set, so adding many is linear, not quadratic.
    pub(crate) fn add_all(&mut self, es: Vec<Entity>) -> Vec<Option<String>> {
        let mut taken: std::collections::HashSet<String> = self.entities.iter().map(|e| e.id.clone()).collect();
        let mut out = Vec::with_capacity(es.len());
        for mut e in es {
            if self.entities.len() >= MAX_ENTITIES {
                out.push(None);
                continue;
            }
            let mut n = self.entities.len() + 1;
            while taken.contains(&format!("e{n}")) {
                n += 1;
            }
            e.id = format!("e{n}");
            taken.insert(e.id.clone());
            out.push(Some(e.id.clone()));
            self.entities.push(e);
        }
        self.sanitize();
        out
    }

    /// Every number of the drawing.
    pub(crate) fn nums_mut(&mut self) -> Vec<(&mut Num, NumKind)> {
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
                Kind::Level { p, datum, .. } => {
                    point(p, &mut out);
                    out.push((datum, NumKind::Length));
                }
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
        // Expressions count in their own unit when the drawing has one for them (converted from
        // centimetres or metres), their lengths then brought to the drawing's unit.
        let expr_unit = self.expr_unit.unwrap_or(self.unit);
        let unit_m = expr_unit.mm() / 1000.0;
        let to_drawing = expr_unit.mm() / self.unit.mm();
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
                Value::Scalar(q) => convert(q, kind, unit_m).map(|v| match kind {
                    NumKind::Length => (v * to_drawing).clamp(-MAX_COORD, MAX_COORD),
                    NumKind::Angle => v,
                }),
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
    /// A level mark's label: `text` (else `<>`) with `<>` the elevation `height` (drawing units)
    /// in metres, two decimals and signed (`+3.00`, `−1.50`, `±0.00`), in the document's number
    /// style (a decimal comma when it uses one).
    /// With `unit`, the elevation ends with ` m` (marked for the Unit style).
    pub fn level_label(&self, text: Option<&str>, height: f64, unit: bool, ws: Option<&Worksheet>) -> String {
        let doc = ws.map(|w| *w.options()).unwrap_or_default();
        let opts = strix_calc::Options { digits: 2, count: strix_calc::Count::Decimals, zeros: true, ..doc };
        let m = height * self.unit.mm() / 1000.0;
        let digits = strix_calc::format_number(m.abs(), &opts);
        let zero = strix_calc::format_number(0.0, &opts);
        let value = if digits == zero || !m.is_finite() {
            format!("±{zero}")
        } else if m > 0.0 {
            format!("+{digits}")
        } else {
            format!("\u{2212}{digits}")
        };
        let value = if unit { format!("{value} {}m{}", crate::UNIT_START, crate::UNIT_END) } else { value };
        fill(&text.unwrap_or("<>").replace("<>", &value), ws)
    }

    /// A linear dimension's label: `text` (else `<>`) with `<>` the size `measured` (drawing
    /// units) in the style's units ([`DimUnits`]), in the document's number style, followed by
    /// its unit when the style shows it.
    pub fn dim_label(&self, st: &DimStyle, text: Option<&str>, measured: f64, ws: Option<&Worksheet>) -> String {
        let opts = ws.map(|w| *w.options()).unwrap_or_default();
        let mm = measured * self.unit.mm();
        let units = st.units;
        let symbol = match units {
            DimUnits::Drawing => self.unit.symbol(),
            DimUnits::M => "m",
            DimUnits::Cm => "cm",
            DimUnits::Mm => "mm",
            DimUnits::Sia if (mm.abs() / 10.0).round() >= 100.0 => "m",
            DimUnits::Sia => "cm",
        };
        let size = match units {
            DimUnits::Drawing => strix_calc::format_number(measured, &opts),
            DimUnits::M => strix_calc::format_number(mm / 1000.0, &opts),
            DimUnits::Cm => strix_calc::format_number(mm / 10.0, &opts),
            DimUnits::Mm => strix_calc::format_number(mm, &opts),
            // Metres to the centimetre from 1 m up (rounded to the centimetre first, so 99.996 cm
            // reads 1.00), centimetres below.
            DimUnits::Sia if (mm.abs() / 10.0).round() >= 100.0 => {
                strix_calc::format_number(mm / 1000.0, &strix_calc::Options { digits: 2, count: strix_calc::Count::Decimals, zeros: true, ..opts })
            }
            DimUnits::Sia => strix_calc::format_number(mm / 10.0, &opts),
        };
        let size = if st.show_unit { format!("{size} {}{symbol}{}", crate::UNIT_START, crate::UNIT_END) } else { size };
        fill(&text.unwrap_or("<>").replace("<>", &size), ws)
    }

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
        // A parameter is set as an equation: its symbol in italics, its value upright.
        let (m0, m1, s0, s1) = (crate::MATH_START, crate::MATH_END, crate::SYM_START, crate::SYM_END);
        match (value, mode) {
            (None, _) => out.push_str(&format!("{}[{name}?]{}", crate::MISSING_START, crate::MISSING_END)),
            (Some(_), "symbol") => out.push_str(&format!("{m0}{s0}{name}{s1}{m1}")),
            (Some(v), "both") => out.push_str(&format!("{m0}{s0}{name}{s1} = {v}{m1}")),
            (Some(v), _) => out.push_str(&format!("{m0}{v}{m1}")),
        }
        rest = rest.get(j + 2..).unwrap_or_default();
    }
    out.push_str(rest);
    out
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum NumKind {
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
