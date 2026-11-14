//! An object's properties as fields the Properties pane shows and edits (and `ui.cadProps` sets):
//! coordinates, radii and angles take a number or an expression of the calculations (`L/2`), so a
//! value typed in the pane stays linked like one typed in the JSON; derived sizes (a line's
//! length and angle, a circle's diameter) recompute the geometry.

use crate::MAX_COORD;
use crate::geom::Pt;
use crate::model::{Align, Drawing, Ends, Kind, Mark, Num, Orient, P, Pattern, Side};

/// What a field holds, for its editor and its unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// Drawing units: a number or an expression.
    Length,
    /// Degrees: a number or an expression.
    Angle,
    /// A size derived from the geometry (drawing units or degrees): a number.
    Derived,
    /// Paper millimetres: a number.
    Paper,
    /// Free text.
    Text,
    /// One of a few words (an orientation, an end mark, an alignment).
    Choice(&'static [&'static str]),
    /// `true` or `false`.
    Bool,
    /// The name of a style of the drawing: a text style for a text, a dimension style for a
    /// dimension, an angle or a level mark ([`crate::STANDARD`] for the drawing's own).
    Style,
}

/// One field: its key, label (English; the UI translates it), kind and current text.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub key: String,
    pub label: &'static str,
    pub kind: FieldKind,
    pub text: String,
}

/// A number as the pane shows it: the expression, else the value to the kernel's resolution.
pub fn num_text(n: &Num) -> String {
    match &n.expr {
        Some(e) => e.clone(),
        None => number(n.value),
    }
}

impl Field {
    /// Whether the field is a length of the drawing (a coordinate, a radius, a line's length):
    /// shown and typed in the input unit by [`Drawing::fields_typed`] and
    /// [`Drawing::set_field_typed`].
    pub fn is_length(&self) -> bool {
        match self.kind {
            FieldKind::Length => true,
            FieldKind::Derived => matches!(self.key.as_str(), "length" | "diameter" | "offsetUnits" | "radius"),
            _ => false,
        }
    }
}

fn number(v: f64) -> String {
    let v = crate::kernel::canon(v);
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

/// A typed number or expression; empty is an error.
fn parse_num(text: &str) -> Result<Num, String> {
    let t = text.trim();
    if t.is_empty() {
        return Err("type a number or an expression".into());
    }
    if let Ok(v) = t.replace(',', ".").parse::<f64>() {
        if !v.is_finite() {
            return Err("not a finite number".into());
        }
        return Ok(Num::new(v.clamp(-MAX_COORD, MAX_COORD)));
    }
    if t.chars().count() > strix_calc::MAX_INPUT_CHARS {
        return Err("expression too long".into());
    }
    Ok(Num::expr(t))
}

fn parse_f64(text: &str) -> Result<f64, String> {
    let v: f64 = text.trim().replace(',', ".").parse().map_err(|_| "type a number".to_string())?;
    if v.is_finite() { Ok(v) } else { Err("not a finite number".into()) }
}

/// The orientations, end marks (`style`: the drawing's) and alignments a field offers.
pub const ORIENTS: &[&str] = &["aligned", "horizontal", "vertical"];
pub const MARKS: &[&str] = &["style", "tick", "arrow", "dot", "none"];
/// A dimension's end mark: `asStart`, as its start.
pub const END_MARKS: &[&str] = &["asStart", "tick", "arrow", "dot", "none"];
pub const ALIGNS: &[&str] = &["left", "center", "right"];
pub const SIDES: &[&str] = &["left", "right"];
/// The arrowheads an open line, arc or polyline may carry.
pub const ENDS: &[&str] = &["none", "start", "end", "both"];
pub const PATTERNS: &[&str] = &["lines", "cross", "steel", "dashed", "solid", "along", "slope"];

fn mark_text(m: Option<Mark>) -> String {
    m.map_or("style", Mark::name).into()
}

fn end_mark_text(m: Option<Mark>) -> String {
    m.map_or("asStart", Mark::name).into()
}

fn parse_end_mark(text: &str) -> Result<Option<Mark>, String> {
    match text.trim() {
        "asStart" | "" => Ok(None),
        t => Mark::parse(t).map(Some).ok_or_else(|| "markEnd: asStart, tick, arrow, dot or none".into()),
    }
}

fn parse_mark(text: &str) -> Result<Option<Mark>, String> {
    match text.trim() {
        "style" => Ok(None),
        t => Mark::parse(t).map(Some).ok_or_else(|| "mark: style, tick, arrow, dot or none".into()),
    }
}

/// A dimension's text: empty is none (the measured size).
fn dim_text(text: &str) -> Option<String> {
    let v: String = text.chars().take(200).collect();
    (!v.trim().is_empty()).then_some(v)
}

fn f(key: &str, label: &'static str, kind: FieldKind, text: String) -> Field {
    Field { key: key.into(), label, kind, text }
}

fn point(out: &mut Vec<Field>, p: &P, x: (&str, &'static str), y: (&str, &'static str)) {
    out.push(f(x.0, x.1, FieldKind::Length, num_text(&p.0)));
    out.push(f(y.0, y.1, FieldKind::Length, num_text(&p.1)));
}

impl Drawing {
    /// The fields of entity `i`; for a polyline, those of vertex `vertex` (clamped).
    pub fn fields(&self, i: usize, vertex: usize) -> Vec<Field> {
        let Some(e) = self.entities.get(i) else { return Vec::new() };
        let mut out = Vec::new();
        if matches!(e.kind, Kind::Text { .. } | Kind::Dim { .. } | Kind::Angle { .. } | Kind::Level { .. }) {
            let name = self.style_name(e);
            out.push(f("style", "Style", FieldKind::Style, name.to_string()));
        }
        match &e.kind {
            Kind::Line { a, b } => {
                point(&mut out, a, ("ax", "Start X"), ("ay", "Start Y"));
                point(&mut out, b, ("bx", "End X"), ("by", "End Y"));
                let d = b.pt().minus(a.pt());
                out.push(f("length", "Length", FieldKind::Derived, number(d.len())));
                out.push(f("angle", "Angle", FieldKind::Derived, number(crate::kernel::deg(crate::kernel::direction(d)))));
            }
            Kind::Circle { c, r } => {
                point(&mut out, c, ("cx", "Centre X"), ("cy", "Centre Y"));
                out.push(f("r", "Radius", FieldKind::Length, num_text(r)));
                out.push(f("diameter", "Diameter", FieldKind::Derived, number(2.0 * r.value)));
            }
            Kind::Arc { c, r, start, end } => {
                point(&mut out, c, ("cx", "Centre X"), ("cy", "Centre Y"));
                out.push(f("r", "Radius", FieldKind::Length, num_text(r)));
                out.push(f("start", "Start angle", FieldKind::Angle, num_text(start)));
                out.push(f("end", "End angle", FieldKind::Angle, num_text(end)));
            }
            Kind::Polyline { pts, closed, .. } => {
                if let Some(p) = pts.get(vertex.min(pts.len().saturating_sub(1))) {
                    point(&mut out, p, ("vx", "Vertex X"), ("vy", "Vertex Y"));
                }
                out.push(f("closed", "Closed", FieldKind::Bool, closed.to_string()));
            }
            Kind::Point { p } => point(&mut out, p, ("x", "X"), ("y", "Y")),
            Kind::Text { p, value, height, angle, align } => {
                out.push(f("value", "Text", FieldKind::Text, value.clone()));
                point(&mut out, p, ("x", "X"), ("y", "Y"));
                out.push(f("height", "Height", FieldKind::Paper, number(*height)));
                out.push(f("rotation", "Rotation", FieldKind::Derived, number(*angle)));
                out.push(f("align", "Alignment", FieldKind::Choice(ALIGNS), align.name().into()));
            }
            Kind::Dim { a, b, offset, offset_units, orient, text, below, mark, mark_end } => {
                out.push(f("text", "Text", FieldKind::Text, text.clone().unwrap_or_default()));
                out.push(f("below", "Text below", FieldKind::Text, below.clone().unwrap_or_default()));
                point(&mut out, a, ("ax", "Start X"), ("ay", "Start Y"));
                point(&mut out, b, ("bx", "End X"), ("by", "End Y"));
                out.push(f("offset", "Annotative distance", FieldKind::Paper, number(*offset)));
                out.push(f("offsetUnits", "Scaled distance", FieldKind::Derived, number(*offset_units)));
                let o = match orient {
                    Orient::Aligned => "aligned",
                    Orient::Horizontal => "horizontal",
                    Orient::Vertical => "vertical",
                };
                out.push(f("orient", "Orientation", FieldKind::Choice(ORIENTS), o.into()));
                out.push(f("mark", "Start mark", FieldKind::Choice(MARKS), mark_text(*mark)));
                out.push(f("markEnd", "End mark", FieldKind::Choice(END_MARKS), end_mark_text(*mark_end)));
            }
            Kind::Strip { width, side, pattern, angle, spacing, ratio, .. } => {
                out.push(f("width", "Width", FieldKind::Paper, number(*width)));
                out.push(f("side", "Side", FieldKind::Choice(SIDES), if *side == Side::Left { "left" } else { "right" }.into()));
                out.push(f("pattern", "Pattern", FieldKind::Choice(PATTERNS), pattern.name().into()));
                if *pattern == Pattern::Slope {
                    out.push(f("ratio", "Short ticks", FieldKind::Derived, number(*ratio)));
                } else {
                    out.push(f("angle", "Angle", FieldKind::Derived, number(*angle)));
                }
                out.push(f("spacing", "Spacing", FieldKind::Paper, number(*spacing)));
            }
            Kind::Hatch { pattern, angle, spacing, boundary } => {
                // Along a path is a strip's: a hatch offers the region patterns.
                let choices = PATTERNS.get(..PATTERNS.len().saturating_sub(2)).unwrap_or(PATTERNS);
                out.push(f("pattern", "Pattern", FieldKind::Choice(choices), pattern.name().into()));
                out.push(f("angle", "Angle", FieldKind::Derived, number(*angle)));
                out.push(f("spacing", "Spacing", FieldKind::Paper, number(*spacing)));
                out.push(f("islands", "Islands", FieldKind::Derived, boundary.len().saturating_sub(1).to_string()));
            }
            Kind::Angle { c, a, b, r, r_paper, text, mark, mark_end } => {
                out.push(f("text", "Text", FieldKind::Text, text.clone().unwrap_or_default()));
                point(&mut out, c, ("cx", "Vertex X"), ("cy", "Vertex Y"));
                point(&mut out, a, ("ax", "First ray X"), ("ay", "First ray Y"));
                point(&mut out, b, ("bx", "Second ray X"), ("by", "Second ray Y"));
                out.push(f("radius", "Scaled radius", FieldKind::Derived, number(*r)));
                out.push(f("radiusPaper", "Annotative radius", FieldKind::Paper, number(*r_paper)));
                let (ua, ub) = (a.pt().minus(c.pt()), b.pt().minus(c.pt()));
                out.push(f("angle", "Angle", FieldKind::Derived, number(crate::kernel::deg(crate::kernel::angle_ccw(ua, ub)))));
                out.push(f("mark", "Start mark", FieldKind::Choice(MARKS), mark_text(*mark)));
                out.push(f("markEnd", "End mark", FieldKind::Choice(END_MARKS), end_mark_text(*mark_end)));
            }
            Kind::Level { p, datum, text, flip, left } => {
                out.push(f("text", "Text", FieldKind::Text, text.clone().unwrap_or_default()));
                point(&mut out, p, ("x", "X"), ("y", "Y"));
                out.push(f("datum", "Datum (±0.00) Y", FieldKind::Length, num_text(datum)));
                out.push(f("elevation", "Elevation", FieldKind::Derived, self.level_label(None, p.1.value - datum.value, false, None)));
                out.push(f("flip", "Under the level", FieldKind::Bool, flip.to_string()));
                out.push(f("left", "Text on the left", FieldKind::Bool, left.to_string()));
            }
        }
        if matches!(e.kind, Kind::Line { .. } | Kind::Arc { .. } | Kind::Polyline { closed: false, .. }) {
            out.push(f("arrows", "Arrows", FieldKind::Choice(ENDS), e.arrows.name().into()));
        }
        out.push(f("answer", "Solution only", FieldKind::Bool, e.answer.to_string()));
        out
    }

    /// The name of the style entity `e` is drawn with: its own when the drawing has it, else
    /// Standard.
    pub fn style_name<'a>(&'a self, e: &'a crate::model::Entity) -> &'a str {
        let own = e.style.as_deref().filter(|n| self.style_names(e).contains(n));
        own.unwrap_or(crate::model::STANDARD)
    }

    /// The styles entity `e` may take: Standard, then the drawing's text styles (for a text) or
    /// dimension styles (for a dimension, angle or level mark); none for other objects.
    pub fn style_names(&self, e: &crate::model::Entity) -> Vec<&str> {
        let named: Vec<&str> = match e.kind {
            Kind::Text { .. } => self.text_styles.iter().map(|s| s.name.as_str()).collect(),
            Kind::Dim { .. } | Kind::Angle { .. } | Kind::Level { .. } => self.dim_styles.iter().map(|s| s.name.as_str()).collect(),
            _ => return Vec::new(),
        };
        std::iter::once(crate::model::STANDARD).chain(named).collect()
    }

    /// [`Drawing::fields`] as the user sees them: lengths in the input unit (the drawing's own
    /// unit is an internal convention). Expressions are shown as typed.
    pub fn fields_typed(&self, i: usize, vertex: usize) -> Vec<Field> {
        let units = crate::Units::of(self);
        let mut out = self.fields(i, vertex);
        for fd in &mut out {
            if fd.is_length()
                && let Ok(v) = fd.text.parse::<f64>()
            {
                fd.text = number(units.shown(v));
            }
        }
        out
    }

    /// [`Drawing::set_field`] with what the user typed: a length field reads a number in the
    /// input unit, or with its own unit (`2.5m`, `30 cm`); an expression is kept as typed.
    pub fn set_field_typed(&mut self, i: usize, vertex: usize, key: &str, text: &str) -> Result<(), String> {
        let length = self.fields(i, vertex).iter().any(|fd| fd.key == key && fd.is_length());
        match crate::parse_length(text, crate::Units::of(self)).filter(|_| length) {
            Some(v) => self.set_field(i, vertex, key, &number(v)),
            None => self.set_field(i, vertex, key, text),
        }
    }

    /// Sets field `key` of entity `i` (a polyline's vertex `vertex`) from `text` (lengths in
    /// drawing units).
    pub fn set_field(&mut self, i: usize, vertex: usize, key: &str, text: &str) -> Result<(), String> {
        if key == "style" {
            return self.set_style(&[i], text);
        }
        let e = self.entities.get_mut(i).ok_or("no such object")?;
        let bad = || format!("no field {key} on this object");
        if key == "arrows" {
            if !matches!(e.kind, Kind::Line { .. } | Kind::Arc { .. } | Kind::Polyline { .. }) {
                return Err(bad());
            }
            e.arrows = Ends::parse(text).ok_or("arrows: none, start, end or both")?;
            return Ok(());
        }
        if key == "answer" {
            e.answer = matches!(text.trim(), "true" | "1" | "yes");
            return Ok(());
        }
        match (&mut e.kind, key) {
            (Kind::Line { a, .. } | Kind::Dim { a, .. } | Kind::Angle { a, .. }, "ax") => a.0 = parse_num(text)?,
            (Kind::Line { a, .. } | Kind::Dim { a, .. } | Kind::Angle { a, .. }, "ay") => a.1 = parse_num(text)?,
            (Kind::Line { b, .. } | Kind::Dim { b, .. } | Kind::Angle { b, .. }, "bx") => b.0 = parse_num(text)?,
            (Kind::Line { b, .. } | Kind::Dim { b, .. } | Kind::Angle { b, .. }, "by") => b.1 = parse_num(text)?,
            (Kind::Line { a, b }, "length" | "angle") => {
                let v = parse_f64(text)?;
                let d = b.pt().minus(a.pt());
                let (len, ang) = if key == "length" {
                    if v <= 0.0 {
                        return Err("a length is more than 0".into());
                    }
                    (v, crate::kernel::deg(crate::kernel::direction(d)))
                } else {
                    (d.len(), v)
                };
                let to = a.pt().polar(len, ang);
                *b = P(Num::new(to.x), Num::new(to.y));
            }
            (Kind::Circle { c, .. } | Kind::Arc { c, .. } | Kind::Angle { c, .. }, "cx") => c.0 = parse_num(text)?,
            (Kind::Circle { c, .. } | Kind::Arc { c, .. } | Kind::Angle { c, .. }, "cy") => c.1 = parse_num(text)?,
            (Kind::Angle { r, r_paper, .. }, "radius") => {
                let v = parse_f64(text)?;
                // Zero is fine when the radius has a paper part.
                if v < 0.0 || (v == 0.0 && *r_paper <= 0.0) {
                    return Err("a radius is more than 0".into());
                }
                *r = v;
            }
            (Kind::Circle { r, .. } | Kind::Arc { r, .. }, "r") => {
                let n = parse_num(text)?;
                if n.expr.is_none() && n.value <= 0.0 {
                    return Err("a radius is more than 0".into());
                }
                *r = n;
            }
            (Kind::Circle { r, .. }, "diameter") => {
                let n = parse_num(text)?;
                *r = match n.expr {
                    Some(e) => Num { value: r.value, expr: Some(format!("({e})/2")) },
                    None if n.value > 0.0 => Num::new(n.value / 2.0),
                    None => return Err("a diameter is more than 0".into()),
                };
            }
            (Kind::Arc { start, .. }, "start") => *start = parse_num(text)?,
            (Kind::Arc { end, .. }, "end") => *end = parse_num(text)?,
            (Kind::Polyline { pts, .. }, "vx" | "vy") => {
                let last = pts.len().saturating_sub(1);
                let p = pts.get_mut(vertex.min(last)).ok_or("the polyline has no vertices")?;
                if key == "vx" {
                    p.0 = parse_num(text)?;
                } else {
                    p.1 = parse_num(text)?;
                }
            }
            (Kind::Polyline { pts, closed, .. }, "closed") => {
                let on = matches!(text.trim(), "true" | "1" | "yes");
                if on && pts.len() < 3 {
                    return Err("a closed polyline needs three points".into());
                }
                *closed = on;
            }
            (Kind::Point { p } | Kind::Text { p, .. } | Kind::Level { p, .. }, "x") => p.0 = parse_num(text)?,
            (Kind::Point { p } | Kind::Text { p, .. } | Kind::Level { p, .. }, "y") => p.1 = parse_num(text)?,
            (Kind::Level { datum, .. }, "datum") => *datum = parse_num(text)?,
            (Kind::Level { flip, .. }, "flip") => *flip = matches!(text.trim(), "true" | "1" | "yes"),
            (Kind::Level { left, .. }, "left") => *left = matches!(text.trim(), "true" | "1" | "yes"),
            (Kind::Text { value, .. }, "value") => {
                if text.trim().is_empty() {
                    return Err("a text cannot be empty".into());
                }
                *value = text.chars().take(1000).collect();
            }
            (Kind::Text { height, .. }, "height") => *height = parse_f64(text)?.clamp(0.5, 50.0),
            (Kind::Text { angle, .. }, "rotation") => *angle = parse_f64(text)? % 360.0,
            (Kind::Text { align, .. }, "align") => *align = Align::parse(text).ok_or("alignment: left, center or right")?,
            (Kind::Dim { text: t, .. } | Kind::Angle { text: t, .. } | Kind::Level { text: t, .. }, "text") => *t = dim_text(text),
            (Kind::Dim { below, .. }, "below") => *below = dim_text(text),
            (Kind::Strip { width, .. }, "width") => *width = parse_f64(text)?,
            (Kind::Strip { ratio, .. }, "ratio") => *ratio = parse_f64(text)?,
            (Kind::Strip { spacing, .. } | Kind::Hatch { spacing, .. }, "spacing") => *spacing = parse_f64(text)?,
            (Kind::Strip { angle, .. } | Kind::Hatch { angle, .. }, "angle") => *angle = parse_f64(text)?,
            (Kind::Strip { side, .. }, "side") => {
                *side = match text.trim() {
                    "left" => Side::Left,
                    "right" => Side::Right,
                    _ => return Err("side: left or right".into()),
                }
            }
            (Kind::Strip { pattern, .. }, "pattern") => {
                *pattern = Pattern::parse(text).ok_or("pattern: lines, cross, steel, dashed, solid, along or slope")?
            }
            (Kind::Hatch { pattern, .. }, "pattern") => {
                *pattern = Pattern::parse(text)
                    .filter(|p| !matches!(p, Pattern::Along | Pattern::Slope))
                    .ok_or("pattern: lines, cross, steel, dashed or solid")?
            }
            (Kind::Dim { mark, .. } | Kind::Angle { mark, .. }, "mark") => *mark = parse_mark(text)?,
            (Kind::Dim { mark_end, .. } | Kind::Angle { mark_end, .. }, "markEnd") => *mark_end = parse_end_mark(text)?,
            (Kind::Dim { offset, .. }, "offset") => *offset = parse_f64(text)?.clamp(-500.0, 500.0),
            (Kind::Dim { offset_units, .. }, "offsetUnits") => *offset_units = parse_f64(text)?,
            (Kind::Angle { r, r_paper, .. }, "radiusPaper") => {
                let v = parse_f64(text)?.clamp(0.0, 500.0);
                if v == 0.0 && *r <= 0.0 {
                    return Err("a radius is more than 0".into());
                }
                *r_paper = v;
            }
            (Kind::Dim { orient, .. }, "orient") => {
                *orient = match text.trim() {
                    "aligned" => Orient::Aligned,
                    "horizontal" => Orient::Horizontal,
                    "vertical" => Orient::Vertical,
                    _ => return Err("orientation: aligned, horizontal or vertical".into()),
                }
            }
            _ => return Err(bad()),
        }
        if !self.entities.get(i).is_some_and(|e| crate::edit::outline(e).iter().flatten().all(|p: &Pt| p.is_finite())) {
            return Err("the object would not be drawable".into());
        }
        self.sanitize();
        Ok(())
    }

    /// Gives the entities at `indices` the style `name` ([`crate::STANDARD`]: the drawing's own):
    /// the texts a text style (and its height), the dimensions, angles and level marks a
    /// dimension style; other objects are left as they are. An error when none of them takes it.
    pub fn set_style(&mut self, indices: &[usize], name: &str) -> Result<(), String> {
        let name = name.trim();
        let standard = name == crate::model::STANDARD || name.is_empty();
        let text = self.text_style_named(Some(name)).map(|s| s.height);
        let dim = self.dim_styles.iter().any(|s| s.name == name);
        let mut done = 0usize;
        for &i in indices {
            let Some(e) = self.entities.get_mut(i) else { continue };
            match &mut e.kind {
                Kind::Text { height, .. } if standard || text.is_some() => {
                    e.style = (!standard).then(|| name.to_string());
                    if let Some(h) = text {
                        *height = h;
                    }
                    done += 1;
                }
                Kind::Dim { .. } | Kind::Angle { .. } | Kind::Level { .. } if standard || dim => {
                    e.style = (!standard).then(|| name.to_string());
                    done += 1;
                }
                _ => {}
            }
        }
        if done == 0 {
            return Err(format!("no style {name} for these objects"));
        }
        self.sanitize();
        Ok(())
    }
}
