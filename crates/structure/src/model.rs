//! The structure model and its JSON form (`docs/structures.md`).

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use strix_calc::{Dim, Quantity, Value, Worksheet};

use crate::{MAX_COORD, MAX_JSON_BYTES, MAX_LOADS, MAX_MEMBERS, MAX_NODES, MAX_SUPPORTS};

/// A number of the structure: a literal, or an expression of calculation parameters with its last
/// value. Serialized as a JSON number, or as the expression's text.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Num {
    /// The value in the structure's units (degrees for angles).
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

    fn is_zero(&self) -> bool {
        self.value == 0.0 && self.expr.is_none()
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
            None => s.serialize_f64(self.value),
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

/// The unit lengths (coordinates, positions) are in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LengthUnit {
    #[default]
    M,
    Cm,
    Mm,
}

impl LengthUnit {
    pub fn symbol(self) -> &'static str {
        match self {
            LengthUnit::M => "m",
            LengthUnit::Cm => "cm",
            LengthUnit::Mm => "mm",
        }
    }

    /// Metres in one unit.
    pub fn si(self) -> f64 {
        match self {
            LengthUnit::M => 1.0,
            LengthUnit::Cm => 0.01,
            LengthUnit::Mm => 0.001,
        }
    }

    pub fn parse(s: &str) -> Option<LengthUnit> {
        match s.trim() {
            "m" => Some(LengthUnit::M),
            "cm" => Some(LengthUnit::Cm),
            "mm" => Some(LengthUnit::Mm),
            _ => None,
        }
    }
}

/// The unit forces are in (moments are this force times the length unit).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ForceUnit {
    #[default]
    #[serde(rename = "kN")]
    KN,
    #[serde(rename = "N")]
    N,
    #[serde(rename = "daN")]
    DaN,
}

impl ForceUnit {
    pub fn symbol(self) -> &'static str {
        match self {
            ForceUnit::KN => "kN",
            ForceUnit::N => "N",
            ForceUnit::DaN => "daN",
        }
    }

    /// Newtons in one unit.
    pub fn si(self) -> f64 {
        match self {
            ForceUnit::KN => 1000.0,
            ForceUnit::N => 1.0,
            ForceUnit::DaN => 10.0,
        }
    }

    pub fn parse(s: &str) -> Option<ForceUnit> {
        match s.trim() {
            "kN" => Some(ForceUnit::KN),
            "N" => Some(ForceUnit::N),
            "daN" => Some(ForceUnit::DaN),
            _ => None,
        }
    }
}

/// A node: a point of the structure where members meet, supports and loads act.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    /// Its name, shown on the drawing (`A`, `B`…) and used by members, supports and loads.
    pub id: String,
    pub x: Num,
    pub y: Num,
    /// An internal hinge: every member meeting here is free to turn (no moment passes).
    #[serde(default, skip_serializing_if = "is_false")]
    pub hinge: bool,
}

/// A straight member between two nodes.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    pub id: String,
    /// Its first node: the local axis `x` goes from `a` to `b`, `y` turned 90° counter-clockwise;
    /// the reference fibres are on the `−y` side (below a member drawn left to right).
    pub a: String,
    pub b: String,
    /// A hinge at the member's `a` end (no moment passes there).
    #[serde(default, skip_serializing_if = "is_false")]
    pub hinge_a: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub hinge_b: bool,
    /// Young's modulus (force/length²); with `i` and `area`. Absent: `EI` is 1 (constant along
    /// the structure) and the member does not stretch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub e: Option<Num>,
    /// Second moment of area (length⁴).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub i: Option<Num>,
    /// Cross-section area (length²).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area: Option<Num>,
}

/// A support's kind: what it restrains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SupportKind {
    /// Incastro: both translations and the rotation.
    Fixed,
    /// Cerniera: both translations.
    #[default]
    Pin,
    /// Carrello: the translation across its rolling plane.
    Roller,
    /// Pendolo: the translation along its link (restrains like a roller, drawn as a bar).
    Link,
    /// Glifo (bipendolo): the translation across its plane and the rotation.
    Slider,
}

impl SupportKind {
    pub fn name(self) -> &'static str {
        match self {
            SupportKind::Fixed => "fixed",
            SupportKind::Pin => "pin",
            SupportKind::Roller => "roller",
            SupportKind::Link => "link",
            SupportKind::Slider => "slider",
        }
    }

    pub fn parse(s: &str) -> Option<SupportKind> {
        match s.trim() {
            "fixed" => Some(SupportKind::Fixed),
            "pin" => Some(SupportKind::Pin),
            "roller" => Some(SupportKind::Roller),
            "link" => Some(SupportKind::Link),
            "slider" => Some(SupportKind::Slider),
            _ => None,
        }
    }
}

/// A support at a node.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Support {
    pub node: String,
    pub kind: SupportKind,
    /// The support turned counter-clockwise about its node (degrees). At 0 it stands on the
    /// ground below the node: a roller restrains the vertical translation; at 90 it is on a wall
    /// to the right of the node and restrains the horizontal one.
    #[serde(default, skip_serializing_if = "Num::is_zero")]
    pub angle: Num,
}

/// The axes a distributed load's direction is measured from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoadAxes {
    /// The structure's axes: `angle` from the horizontal, counter-clockwise (−90 is down).
    #[default]
    Global,
    /// The member's axes: `angle` from the member's axis (−90 is towards the reference fibres).
    Local,
}

/// What a distributed load's intensity is per.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoadPer {
    /// Per unit length of the member.
    #[default]
    Length,
    /// Per unit length of the member's projection across the load (a vertical load per metre
    /// of horizontal projection: snow on a roof).
    Projection,
}

fn default_down() -> Num {
    Num::new(-90.0)
}

fn is_down(n: &Num) -> bool {
    n.expr.is_none() && n.value == -90.0
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_true(b: &bool) -> bool {
    *b
}

fn default_true() -> bool {
    true
}

/// A load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Load {
    /// A concentrated force at a node, or on a member `at` from its `a` end.
    #[serde(rename_all = "camelCase")]
    Force {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        node: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        member: Option<String>,
        #[serde(default, skip_serializing_if = "Num::is_zero")]
        at: Num,
        value: Num,
        /// Its direction (degrees, counter-clockwise from the horizontal; −90 is down).
        #[serde(default = "default_down", skip_serializing_if = "is_down")]
        angle: Num,
    },
    /// A concentrated couple (counter-clockwise positive) at a node or on a member.
    #[serde(rename_all = "camelCase")]
    Moment {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        node: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        member: Option<String>,
        #[serde(default, skip_serializing_if = "Num::is_zero")]
        at: Num,
        value: Num,
    },
    /// A load spread along a member from `from` to `to` (its whole length by default), varying
    /// linearly from `q1` to `q2` (uniform when `q2` is absent): uniform, triangular, trapezoidal.
    #[serde(rename_all = "camelCase")]
    Distributed {
        member: String,
        q1: Num,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        q2: Option<Num>,
        #[serde(default, skip_serializing_if = "Num::is_zero")]
        from: Num,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<Num>,
        #[serde(default = "default_down", skip_serializing_if = "is_down")]
        angle: Num,
        #[serde(default, skip_serializing_if = "is_global")]
        axes: LoadAxes,
        #[serde(default, skip_serializing_if = "is_per_length")]
        per: LoadPer,
    },
}

fn is_global(a: &LoadAxes) -> bool {
    *a == LoadAxes::Global
}

fn is_per_length(p: &LoadPer) -> bool {
    *p == LoadPer::Length
}

/// Which side of a member a diagram's positive values are drawn on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    /// The member's `+y` side: above a member drawn left to right.
    #[default]
    Up,
    /// The `−y` side, the reference fibres': below a member drawn left to right.
    Down,
}

/// One diagram's settings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagram {
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub show: bool,
    /// The side positive values go.
    #[serde(default)]
    pub positive: Side,
    /// The largest value's height on paper (mm).
    #[serde(default = "default_amplitude")]
    pub amplitude: f64,
}

fn default_amplitude() -> f64 {
    12.0
}

impl Diagram {
    fn with(positive: Side) -> Diagram {
        Diagram { show: true, positive, amplitude: default_amplitude() }
    }
}

/// The diagrams drawn under the structure, and how.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagrams {
    /// The reactions, drawn on the structure in a panel of their own.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub reactions: bool,
    #[serde(default = "Diagrams::default_n")]
    pub n: Diagram,
    #[serde(default = "Diagrams::default_v")]
    pub v: Diagram,
    #[serde(default = "Diagrams::default_m")]
    pub m: Diagram,
    /// Notable values written on the diagrams: at the ends of each stretch, maxima, zeros.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub values: bool,
    /// Hatching across the diagrams' areas.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub hatch: bool,
    /// ⊕/⊖ in each area.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub signs: bool,
    /// The structure's dimensions (chains of distances along x and y, inclined members' slopes)
    /// and the angles inclined forces make with their members. On in a new structure
    /// ([`Diagrams::default`], written out), off when absent: a structure saved before they existed
    /// keeps its drawing and its size on the page.
    #[serde(default, skip_serializing_if = "is_false")]
    pub dimensions: bool,
    /// The diagrams' stations: where along each member a notable value is (a maximum, a zero,
    /// a load's position), written on the axis. On in a new structure, off when absent, as
    /// `dimensions`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub stations: bool,
}

impl Diagrams {
    fn default_n() -> Diagram {
        Diagram::with(Side::Up)
    }

    fn default_v() -> Diagram {
        Diagram::with(Side::Up)
    }

    /// The bending moment on the stretched fibres' side (Italian convention).
    fn default_m() -> Diagram {
        Diagram::with(Side::Down)
    }

    /// The Italian convention: N and V positive up, M on the stretched fibres.
    pub fn italian(self) -> Diagrams {
        Diagrams {
            n: Diagram { positive: Side::Up, ..self.n },
            v: Diagram { positive: Side::Up, ..self.v },
            m: Diagram { positive: Side::Down, ..self.m },
            ..self
        }
    }
}

impl Default for Diagrams {
    fn default() -> Self {
        Diagrams {
            reactions: true,
            n: Self::default_n(),
            v: Self::default_v(),
            m: Self::default_m(),
            values: true,
            hatch: true,
            signs: true,
            dimensions: true,
            stations: true,
        }
    }
}

/// The diagrams' names, written in their panels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Titles {
    #[serde(default = "Titles::default_n")]
    pub n: String,
    /// The shear's symbol: `T` in Italian texts, `V` in English ones.
    #[serde(default = "Titles::default_v")]
    pub v: String,
    #[serde(default = "Titles::default_m")]
    pub m: String,
}

impl Titles {
    fn default_n() -> String {
        "N".to_string()
    }

    fn default_v() -> String {
        "T".to_string()
    }

    fn default_m() -> String {
        "M".to_string()
    }

    fn is_default(&self) -> bool {
        *self == Titles::default()
    }
}

impl Default for Titles {
    fn default() -> Self {
        Titles { n: Self::default_n(), v: Self::default_v(), m: Self::default_m() }
    }
}

/// Which panels a drawing shows: an exercise draws the structure in its statement and the
/// reactions and the diagrams in its solution, each a drawing of the same structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum View {
    /// The structure, its reactions and its diagrams.
    #[default]
    All,
    /// The structure with its loads (and dimensions) only.
    Structure,
    /// The reactions only.
    Reactions,
    /// The N, V (T), M diagrams (a truss's axial forces) only.
    Diagrams,
}

fn is_all(v: &View) -> bool {
    *v == View::All
}

/// How the panels are laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Layout {
    /// One column for a beam, two for a frame (a structure taller than a quarter of its span).
    #[default]
    Auto,
    /// One under the other.
    Column,
    /// Two by two.
    Grid,
}

fn is_auto(l: &Layout) -> bool {
    *l == Layout::Auto
}

fn default_width() -> f64 {
    160.0
}

/// A plane structure: beams, frames and trusses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Structure {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default)]
    pub length_unit: LengthUnit,
    #[serde(default)]
    pub force_unit: ForceUnit,
    #[serde(default)]
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub members: Vec<Member>,
    #[serde(default)]
    pub supports: Vec<Support>,
    #[serde(default)]
    pub loads: Vec<Load>,
    #[serde(default)]
    pub diagrams: Diagrams,
    #[serde(default, skip_serializing_if = "Titles::is_default")]
    pub titles: Titles,
    #[serde(default, skip_serializing_if = "is_auto")]
    pub layout: Layout,
    /// The panels drawn.
    #[serde(default, skip_serializing_if = "is_all")]
    pub view: View,
    /// Drawings of one structure (an exercise's statement and solution) share this name: a change
    /// to one is made to the others, each keeping its own view.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub group: String,
    /// The drawing's width on paper (mm).
    #[serde(default = "default_width")]
    pub width: f64,
    /// Reactions and diagrams are the answer: hidden in the student copy (the panels and the
    /// structure stay, so both copies have the same size).
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub answer: bool,
}

impl Default for Structure {
    fn default() -> Self {
        Structure {
            title: String::new(),
            length_unit: LengthUnit::default(),
            force_unit: ForceUnit::default(),
            nodes: Vec::new(),
            members: Vec::new(),
            supports: Vec::new(),
            loads: Vec::new(),
            diagrams: Diagrams::default(),
            titles: Titles::default(),
            layout: Layout::default(),
            view: View::default(),
            group: String::new(),
            width: default_width(),
            answer: true,
        }
    }
}

/// What a number measures: how a parameter with a unit converts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NumKind {
    Length,
    Angle,
    Force,
    Moment,
    /// Force per length.
    Distributed,
    /// Force per length² (Young's modulus).
    Modulus,
    /// Length⁴.
    Inertia,
    /// Length².
    Area,
}

impl NumKind {
    /// The dimension and the factor from SI to the structure's units.
    fn dim(self, l: LengthUnit, f: ForceUnit) -> Option<(Dim, f64)> {
        let (ls, fs) = (l.si(), f.si());
        Some(match self {
            NumKind::Angle => return None,
            NumKind::Length => (Dim::new([1, 0, 0, 0, 0, 0, 0]), 1.0 / ls),
            NumKind::Force => (Dim::new([1, 1, -2, 0, 0, 0, 0]), 1.0 / fs),
            NumKind::Moment => (Dim::new([2, 1, -2, 0, 0, 0, 0]), 1.0 / (fs * ls)),
            NumKind::Distributed => (Dim::new([0, 1, -2, 0, 0, 0, 0]), ls / fs),
            NumKind::Modulus => (Dim::new([-1, 1, -2, 0, 0, 0, 0]), ls * ls / fs),
            NumKind::Inertia => (Dim::new([4, 0, 0, 0, 0, 0, 0]), 1.0 / (ls * ls * ls * ls)),
            NumKind::Area => (Dim::new([2, 0, 0, 0, 0, 0, 0]), 1.0 / (ls * ls)),
        })
    }

    fn noun(self) -> &'static str {
        match self {
            NumKind::Length => "a length",
            NumKind::Angle => "an angle",
            NumKind::Force => "a force",
            NumKind::Moment => "a moment",
            NumKind::Distributed => "a force per length",
            NumKind::Modulus => "a modulus (force per area)",
            NumKind::Inertia => "a second moment of area (length⁴)",
            NumKind::Area => "an area",
        }
    }
}

fn clean_id(s: &str) -> String {
    s.trim().chars().filter(|c| !c.is_control()).take(24).collect()
}

fn clean_num(n: &mut Num) {
    if !n.value.is_finite() {
        n.value = 0.0;
    }
    n.value = n.value.clamp(-MAX_COORD, MAX_COORD);
    if let Some(e) = &mut n.expr {
        if e.trim().is_empty() {
            n.expr = None;
        } else if e.chars().count() > strix_calc::MAX_INPUT_CHARS {
            *e = e.chars().take(strix_calc::MAX_INPUT_CHARS).collect();
        }
    }
}

impl Structure {
    /// Reads a structure from its JSON, sanitized.
    pub fn from_json(json: &str) -> Result<Structure, String> {
        if json.len() > MAX_JSON_BYTES {
            return Err(format!("structure JSON exceeds {MAX_JSON_BYTES} bytes"));
        }
        let s: Structure = serde_json::from_str(json).map_err(|e| format!("invalid structure JSON: {e}"))?;
        Ok(s.sanitized())
    }

    /// The canonical compact JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.clone().sanitized()).unwrap_or_else(|_| "{}".to_string())
    }

    /// Caps counts, cleans names, clamps numbers.
    pub fn sanitized(mut self) -> Structure {
        self.title = self.title.chars().filter(|c| !c.is_control()).take(120).collect();
        self.group = self.group.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(40).collect();
        for t in [&mut self.titles.n, &mut self.titles.v, &mut self.titles.m] {
            *t = t.chars().filter(|c| !c.is_control()).take(12).collect();
        }
        self.nodes.truncate(MAX_NODES);
        self.members.truncate(MAX_MEMBERS);
        self.supports.truncate(MAX_SUPPORTS);
        self.loads.truncate(MAX_LOADS);
        self.width = if self.width.is_finite() { self.width.clamp(40.0, 400.0) } else { default_width() };
        for d in [&mut self.diagrams.n, &mut self.diagrams.v, &mut self.diagrams.m] {
            d.amplitude = if d.amplitude.is_finite() { d.amplitude.clamp(2.0, 60.0) } else { default_amplitude() };
        }
        for n in &mut self.nodes {
            n.id = clean_id(&n.id);
        }
        for m in &mut self.members {
            m.id = clean_id(&m.id);
            m.a = clean_id(&m.a);
            m.b = clean_id(&m.b);
        }
        for s in &mut self.supports {
            s.node = clean_id(&s.node);
        }
        for l in &mut self.loads {
            match l {
                Load::Force { node, member, .. } | Load::Moment { node, member, .. } => {
                    *node = node.as_deref().map(clean_id);
                    *member = member.as_deref().map(clean_id);
                }
                Load::Distributed { member, .. } => *member = clean_id(member),
            }
        }
        for (n, _) in self.nums_mut() {
            clean_num(n);
        }
        self
    }

    /// Every number with what it measures, in a fixed order.
    pub(crate) fn nums_mut(&mut self) -> Vec<(&mut Num, NumKind)> {
        let mut out: Vec<(&mut Num, NumKind)> = Vec::new();
        for n in &mut self.nodes {
            out.push((&mut n.x, NumKind::Length));
            out.push((&mut n.y, NumKind::Length));
        }
        for m in &mut self.members {
            if let Some(e) = &mut m.e {
                out.push((e, NumKind::Modulus));
            }
            if let Some(i) = &mut m.i {
                out.push((i, NumKind::Inertia));
            }
            if let Some(a) = &mut m.area {
                out.push((a, NumKind::Area));
            }
        }
        for s in &mut self.supports {
            out.push((&mut s.angle, NumKind::Angle));
        }
        for l in &mut self.loads {
            match l {
                Load::Force { at, value, angle, .. } => {
                    out.push((at, NumKind::Length));
                    out.push((value, NumKind::Force));
                    out.push((angle, NumKind::Angle));
                }
                Load::Moment { at, value, .. } => {
                    out.push((at, NumKind::Length));
                    out.push((value, NumKind::Moment));
                }
                Load::Distributed { q1, q2, from, to, angle, .. } => {
                    out.push((q1, NumKind::Distributed));
                    if let Some(q2) = q2 {
                        out.push((q2, NumKind::Distributed));
                    }
                    out.push((from, NumKind::Length));
                    if let Some(to) = to {
                        out.push((to, NumKind::Length));
                    }
                    out.push((angle, NumKind::Angle));
                }
            }
        }
        out
    }

    /// Evaluates every expression with the worksheet's parameters. A parameter with a unit
    /// converts to the structure's units (`L = 450 cm` is 4.5 in a metre structure,
    /// `q = 12 kN/m` is 12); a plain number counts in the structure's units; an angle in degrees
    /// (`30°`, or a plain number). Returns the expressions that failed, with why; those keep
    /// their last value.
    pub fn resolve(&mut self, ws: &Worksheet) -> Vec<(String, String)> {
        let (lu, fu) = (self.length_unit, self.force_unit);
        let mut errors = Vec::new();
        let mut spent_out = false;
        let mut lengths_as_numbers: Option<Worksheet> = None;
        for (num, kind) in self.nums_mut() {
            let Some(expr) = num.expr.clone() else { continue };
            if spent_out {
                errors.push((expr, strix_calc::CalcError::TooMuchWork.to_string()));
                continue;
            }
            let mut result = ws.eval_expr(&expr);
            // `a + 1.5` with `a = 3 m`: lengths count as plain numbers in the structure's unit,
            // as coordinates do in a CAD drawing.
            if result.is_err() && kind == NumKind::Length && !matches!(result, Err(strix_calc::CalcError::TooMuchWork)) {
                let local = lengths_as_numbers.get_or_insert_with(|| lengths_local(ws, lu));
                local.scope().set_spent(ws.scope().spent());
                if let Ok(v) = local.eval_expr(&expr) {
                    result = Ok(v);
                }
                ws.scope().set_spent(local.scope().spent());
            }
            if matches!(result, Err(strix_calc::CalcError::TooMuchWork)) {
                spent_out = true;
            }
            let value = result.map_err(|e| e.to_string()).and_then(|v| match v {
                Value::Scalar(q) => convert(q, kind, lu, fu),
                Value::Vector(_) => Err("a number of the structure is one value, not a vector".to_string()),
            });
            match value {
                Ok(v) if v.is_finite() => num.value = v.clamp(-MAX_COORD, MAX_COORD),
                Ok(_) => errors.push((expr, "not a finite number".to_string())),
                Err(e) => errors.push((expr, e)),
            }
        }
        errors
    }

    /// The structure with its numbers' values only (their expressions dropped): what it is now.
    pub fn values_only(&self) -> Structure {
        let mut copy = self.clone();
        for (n, _) in copy.nums_mut() {
            n.expr = None;
        }
        copy
    }

    /// The expressions its numbers come from, in order.
    pub fn expressions(&self) -> Vec<String> {
        let mut copy = self.clone();
        copy.nums_mut().into_iter().filter_map(|(n, _)| n.expr.clone()).collect()
    }

    /// The member named `id`.
    pub fn member(&self, id: &str) -> Option<&Member> {
        self.members.iter().find(|m| m.id == id)
    }

    /// The node named `id`.
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// The node named `id`'s index.
    pub fn node_index(&self, id: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.id == id)
    }

    /// A member's length.
    pub fn length(&self, m: &Member) -> Option<f64> {
        let (a, b) = (self.node(&m.a)?, self.node(&m.b)?);
        Some((b.x.value - a.x.value).hypot(b.y.value - a.y.value))
    }

    /// A name for a new node: the first capital letter not taken (`A`, `B`…, then `N27`…).
    pub fn next_node_id(&self) -> String {
        for c in 'A'..='Z' {
            let id = c.to_string();
            if self.node(&id).is_none() {
                return id;
            }
        }
        (1..).map(|k| format!("N{k}")).find(|id| self.node(id).is_none()).unwrap_or_default()
    }

    /// A name for a new member: the first number not taken.
    pub fn next_member_id(&self) -> String {
        (1..).map(|k| k.to_string()).find(|id| self.member(id).is_none()).unwrap_or_default()
    }
}

/// A worksheet with `ws`'s parameters where lengths are plain numbers in unit `lu`.
fn lengths_local(ws: &Worksheet, lu: LengthUnit) -> Worksheet {
    let mut local = Worksheet::new(*ws.options());
    let length = Dim::new([1, 0, 0, 0, 0, 0, 0]);
    for (name, v) in ws.scope().iter() {
        match v {
            Value::Scalar(q) if q.dim == length => local.define(name, Value::Scalar(Quantity::number(q.value / lu.si()))),
            other => local.define(name, other.clone()),
        }
    }
    local
}

fn convert(q: Quantity, kind: NumKind, lu: LengthUnit, fu: ForceUnit) -> Result<f64, String> {
    if kind == NumKind::Angle {
        // Calculations keep angles in radians (`30°` is π/6): back to degrees.
        return if q.dim.is_dimensionless() { Ok(q.value.to_degrees()) } else { Err("expected an angle".to_string()) };
    }
    if q.dim.is_dimensionless() {
        return Ok(q.value);
    }
    match kind.dim(lu, fu) {
        Some((dim, factor)) if dim == q.dim => Ok(q.value * factor),
        _ => Err(format!("expected {} or a plain number", kind.noun())),
    }
}
