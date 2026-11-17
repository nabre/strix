//! Underlays and inherited styles (docs/cad.md, Underlay): another drawing of the document shown
//! under this one, faded and locked, to trace over — a reference by the source's id, so later
//! edits to it show — and the source's layers and styles merged into this drawing.
//!
//! One level deep: the underlay's own underlay is never drawn, and a drawing never takes itself
//! (the document refuses a pair that would take each other).

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use strix_calc::Worksheet;

use crate::MAX_COORD;
use crate::model::{Drawing, Kind, cut_name};
use crate::modify::Affine;
use crate::render::{Prim, Rendered};

/// Most objects of an underlay drawn (and snapped to).
pub const MAX_UNDERLAY_ENTITIES: usize = 5000;

/// The underlay of a drawing: the source drawing's id, where it sits (drawing units, 1:1 in this
/// drawing's unit), how faded it is, whether it shows, and whether it prints with the drawing
/// (off by default: only in the editor).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Underlay {
    pub source: String,
    #[serde(skip_serializing_if = "is_zero")]
    pub dx: f64,
    #[serde(skip_serializing_if = "is_zero")]
    pub dy: f64,
    /// 0: as drawn, to 0.95: almost white.
    pub fade: f64,
    #[serde(skip_serializing_if = "is_true")]
    pub show: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub print: bool,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

fn is_true(b: &bool) -> bool {
    *b
}

impl Default for Underlay {
    fn default() -> Self {
        Underlay { source: String::new(), dx: 0.0, dy: 0.0, fade: 0.6, show: true, print: false }
    }
}

impl Underlay {
    /// Within reason: the id cut, the offset finite within the coordinates' limits, the fade
    /// 0–0.95.
    pub fn sanitized(self) -> Underlay {
        let c = |v: f64, lo: f64, hi: f64, fallback: f64| if v.is_finite() { v.clamp(lo, hi) } else { fallback };
        Underlay {
            source: cut_name(&self.source),
            dx: c(self.dx, -MAX_COORD, MAX_COORD, 0.0),
            dy: c(self.dy, -MAX_COORD, MAX_COORD, 0.0),
            fade: c(self.fade, 0.0, 0.95, 0.6),
            ..self
        }
    }
}

/// The source drawing of a drawing's underlay, as the document worked it out where the source
/// sits (its coordinates' values resolved): set by whoever reads the drawing in its document, so
/// that an underlay printed with the drawing counts in the picture's size and in the fitted
/// scale ([`Drawing::fits_at`], [`Drawing::with_printed_underlay`]). Never saved, never compared:
/// two drawings differing only in it are the same drawing.
#[derive(Debug, Clone, Default)]
pub struct UnderSource(pub Option<Arc<Drawing>>);

impl PartialEq for UnderSource {
    fn eq(&self, _: &UnderSource) -> bool {
        true
    }
}

/// What [`Drawing::inherit`] added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inherited {
    pub layers: usize,
    pub dim_styles: usize,
    pub text_styles: usize,
}

/// `c` faded towards white by `fade` (0–1).
fn faded(c: [u8; 3], fade: f64) -> [u8; 3] {
    let f = if fade.is_finite() { fade.clamp(0.0, 1.0) } else { 0.0 };
    c.map(|v| (f64::from(v) + (255.0 - f64::from(v)) * f).round().clamp(0.0, 255.0) as u8)
}

impl Drawing {
    /// Merges `src`'s layers (with their colour, lineweight and linetype), dimension styles (with
    /// their level settings) and text styles into this drawing: a name this drawing has keeps its
    /// own definition, a new one is added (up to the limits). The current layer and styles stay.
    ///
    /// A source with an id (as the document gives one) lends its own layers and all its styles
    /// (its Standard dimension style as `id/Standard`) nested under its name: `Walls` of `ground-floor` becomes `ground-floor/Walls`, marked as coming from it
    /// ([`crate::Layer::from`]: never renamed, removed only when empty), so it never meets a
    /// layer of this drawing's own; layers the source itself inherited keep their names. They
    /// are not locked: this drawing can draw on them like on its own. When the source is this
    /// drawing's underlay and the underlay does not print, they come set not to print.
    pub fn inherit(&mut self, src: &Drawing) -> Inherited {
        let mut out = Inherited::default();
        let no_print = self.underlay.as_ref().is_some_and(|u| !u.print && src.id.as_deref() == Some(u.source.as_str()));
        for l in &src.layers {
            if self.layers.len() >= crate::MAX_LAYERS {
                break;
            }
            let mut l = l.clone();
            if let (Some(id), None) = (src.id.as_ref(), l.from.as_ref()) {
                l.name = cut_name(&format!("{id}/{}", l.name));
                l.from = Some(id.clone());
            }
            if !self.layers.iter().any(|m| m.name == l.name) {
                l.locked = false;
                if no_print {
                    l.print = false;
                }
                self.layers.push(l);
                out.layers += 1;
            }
        }
        // Its styles, all of them (its Standard dimension style too), nested under its name
        // like its layers; a dimension style's text style follows it.
        let nest = |name: &str| match src.id.as_ref() {
            Some(id) if !name.contains('/') => cut_name(&format!("{id}/{name}")),
            _ => name.to_string(),
        };
        let mut dims: Vec<crate::NamedDimStyle> = Vec::new();
        if src.id.is_some() {
            dims.push(crate::NamedDimStyle { name: nest(crate::STANDARD), style: src.dim_style.clone() });
        }
        dims.extend(src.dim_styles.iter().map(|st| crate::NamedDimStyle { name: nest(&st.name), style: st.style.clone() }));
        for mut st in dims {
            if self.dim_styles.len() >= crate::MAX_STYLES {
                break;
            }
            st.style.text_style = st.style.text_style.as_deref().map(nest);
            if !self.dim_styles.iter().any(|m| m.name == st.name) {
                self.dim_styles.push(st);
                out.dim_styles += 1;
            }
        }
        for st in &src.text_styles {
            if self.text_styles.len() >= crate::MAX_STYLES {
                break;
            }
            let st = crate::TextStyle { name: nest(&st.name), ..st.clone() };
            if !self.text_styles.iter().any(|m| m.name == st.name) {
                self.text_styles.push(st);
                out.text_styles += 1;
            }
        }
        self.sanitize();
        out
    }

    /// Every label written out as this drawing shows it where it sits, with `ws` (its
    /// worksheet): texts' parameters (`{{L}}`) filled, dimension and level labels with their
    /// size, unit and number format, angles' labels with their degrees (a plain right angle
    /// keeps its square). The drawing then shows its parameters' values and its settings
    /// anywhere else — as another drawing's underlay, rendered without its worksheet.
    pub fn freeze_labels(&mut self, ws: Option<&Worksheet>) {
        let labels: Vec<Option<(Option<String>, Option<String>)>> = self
            .entities
            .iter()
            .map(|e| {
                let st = self.dim_style_of(e);
                match &e.kind {
                    Kind::Text { value, .. } => Some((Some(crate::model::fill(value, ws)), None)),
                    Kind::Dim { text, below, .. } => {
                        let measured = self.dim_frame(e, 1.0).map_or(0.0, |f| f.fb.minus(f.fa).len());
                        let second = below.as_deref().filter(|b| !b.trim().is_empty()).map(|b| self.dim_label(st, Some(b), measured, ws));
                        Some((Some(self.dim_label(st, text.as_deref(), measured, ws)), second))
                    }
                    Kind::Angle { c, a, b, text, .. } => {
                        let (c, a, b) = (c.pt(), a.pt(), b.pt());
                        let (ua, ub) = (a.minus(c).unit(), b.minus(c).unit());
                        let sweep = crate::kernel::angle_ccw(ua, ub);
                        let right = text.is_none() && (sweep - std::f64::consts::FRAC_PI_2).abs() <= crate::render::RIGHT_ANGLE_TOL;
                        (!right).then(|| (Some(self.label(text.as_deref().or(Some("<>°")), crate::kernel::deg(sweep), ws)), None))
                    }
                    Kind::Level { p, datum, text, .. } => {
                        Some((Some(self.level_label(text.as_deref(), p.1.value - datum.value, st.level_unit, ws)), None))
                    }
                    _ => None,
                }
            })
            .collect();
        for (e, label) in self.entities.iter_mut().zip(labels) {
            let Some((first, second)) = label else { continue };
            match &mut e.kind {
                Kind::Text { value, .. } => {
                    if let Some(t) = first {
                        *value = t;
                    }
                }
                Kind::Dim { text, below, .. } => {
                    *text = first;
                    if second.is_some() {
                        *below = second;
                    }
                }
                Kind::Angle { text, .. } | Kind::Level { text, .. } => *text = first,
                _ => {}
            }
        }
    }

    /// The layers inherited from drawing `source` print or not, as its underlay now does (their
    /// default follows the underlay's Print with the drawing); how many changed.
    pub fn print_layers_from(&mut self, source: &str, print: bool) -> usize {
        let mut n = 0;
        for l in self.layers.iter_mut().filter(|l| l.from.as_deref() == Some(source) && l.print != print) {
            l.print = print;
            n += 1;
        }
        n
    }

    /// The source drawing `old` was renamed `new`: the layers and styles inherited from it follow
    /// (their names' first part, and the objects and styles naming them); whether anything
    /// changed.
    pub fn rename_layers_from(&mut self, old: &str, new: &str) -> bool {
        let mut renamed: Vec<(String, String)> = Vec::new();
        for l in self.layers.iter_mut().filter(|l| l.from.as_deref() == Some(old)) {
            let tail = l.name.strip_prefix(old).and_then(|t| t.strip_prefix('/')).unwrap_or(l.name.as_str()).to_string();
            let to = cut_name(&format!("{new}/{tail}"));
            l.from = Some(new.to_string());
            if to != l.name {
                renamed.push((std::mem::replace(&mut l.name, to.clone()), to));
            }
        }
        let mut any = self.layers.iter().any(|l| l.from.as_deref() == Some(new));
        for e in &mut self.entities {
            if let Some((_, to)) = renamed.iter().find(|(from, _)| *from == e.layer) {
                e.layer.clone_from(to);
            }
        }
        // Its styles too, and what names them.
        let prefix = format!("{old}/");
        let moved = |n: &str| n.strip_prefix(prefix.as_str()).map(|tail| cut_name(&format!("{new}/{tail}")));
        for st in &mut self.dim_styles {
            if let Some(to) = moved(&st.name) {
                st.name = to;
                any = true;
            }
            if let Some(to) = st.style.text_style.as_deref().and_then(moved) {
                st.style.text_style = Some(to);
            }
        }
        for st in &mut self.text_styles {
            if let Some(to) = moved(&st.name) {
                st.name = to;
                any = true;
            }
        }
        for name in [&mut self.current_dim_style, &mut self.current_text_style].into_iter().flatten() {
            if let Some(to) = moved(name) {
                *name = to;
            }
        }
        for e in &mut self.entities {
            if let Some(to) = e.style.as_deref().and_then(moved) {
                e.style = Some(to);
            }
        }
        any
    }

    /// Drawing `src` as this drawing's underlay shows it: in this drawing's unit and scale,
    /// moved by the underlay's offset, at most [`MAX_UNDERLAY_ENTITIES`] objects, every layer
    /// locked, its colours faded; without an underlay of its own (one level deep).
    pub fn underlay_view(&self, src: &Drawing) -> Drawing {
        let u = self.underlay.clone().unwrap_or_default().sanitized();
        let mut v = src.clone();
        v.underlay = None;
        v.under_source = UnderSource::default();
        v.id = None;
        v.fit = None;
        v.entities.truncate(MAX_UNDERLAY_ENTITIES);
        // Its sizes as they are worked out where it sits: the values, no longer expressions (a
        // change of unit converts numbers only, and nothing works them out again here).
        for (n, _) in v.nums_mut() {
            n.expr = None;
        }
        if v.unit != self.unit {
            v.set_unit(self.unit, true);
        }
        v.scale = self.scale;
        v.padding = self.padding;
        if u.dx != 0.0 || u.dy != 0.0 {
            let all: Vec<usize> = (0..v.entities.len()).collect();
            v.transform(&all, &Affine { m: [1.0, 0.0, u.dx, 0.0, 1.0, u.dy], scale: 1.0, mirror: false });
        }
        for l in &mut v.layers {
            l.locked = true;
            l.color = faded(l.color, u.fade);
        }
        for e in &mut v.entities {
            if let Some(c) = e.stroke.color.as_mut() {
                *c = faded(*c, u.fade);
            }
        }
        v
    }
}

impl Drawing {
    /// The underlay's source when the underlay prints with the drawing: shown, set to print,
    /// not the drawing itself, and its source known ([`UnderSource`]).
    pub fn printed_underlay(&self) -> Option<&Drawing> {
        let u = self.underlay.as_ref().filter(|u| u.print && u.show)?;
        if self.id.as_deref() == Some(u.source.as_str()) {
            return None;
        }
        self.under_source.0.as_deref().filter(|src| src.id.as_deref() == Some(u.source.as_str()))
    }

    /// This drawing's rendering `drawn` with the underlay that prints with it beneath (as is
    /// without one): the picture the page and the PDF show, whose size every fitting uses.
    pub fn with_printed_underlay(&self, drawn: Rendered) -> Rendered {
        match self.printed_underlay() {
            Some(src) => drawn.with_under(self.underlay_view(src).render(None)),
            None => drawn,
        }
    }
}

impl Rendered {
    /// This rendering with `under` drawn beneath it, aligned by their drawing coordinates (the
    /// same scale): the paper grows to hold both.
    pub fn with_under(self, under: Rendered) -> Rendered {
        if self.scale <= 0.0 || (under.scale - self.scale).abs() > 1e-9 * self.scale.max(1.0) {
            return self;
        }
        // A paper point of `under` in this paper: the same drawing point.
        let (sx, sy) = (under.origin.0 - self.origin.0, self.origin.1 - under.origin.1);
        let mut prims: Vec<Prim> = under.prims;
        for p in &mut prims {
            map_prim(p, |(x, y)| (x + sx, y + sy));
        }
        prims.extend(self.prims);
        // The paper from the top left of what is drawn (both of them, this one's picture too).
        let (mut x0, mut y0, mut x1, mut y1) = (0.0_f64, 0.0_f64, self.size.0, self.size.1);
        for p in &prims {
            each_point(p, |(x, y)| {
                if x.is_finite() && y.is_finite() {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            });
        }
        let (dx, dy) = (-x0, -y0);
        if dx != 0.0 || dy != 0.0 {
            for p in &mut prims {
                map_prim(p, |(x, y)| (x + dx, y + dy));
            }
        }
        Rendered { prims, size: (x1 - x0, y1 - y0), origin: (self.origin.0 - dx, self.origin.1 + dy), scale: self.scale, texts: self.texts }
    }
}

fn map_prim(p: &mut Prim, f: impl Fn((f64, f64)) -> (f64, f64)) {
    match p {
        Prim::Path { pts, .. } => pts.iter_mut().for_each(|q| *q = f(*q)),
        Prim::Dot { at, .. } | Prim::Text { at, .. } => *at = f(*at),
    }
}

fn each_point(p: &Prim, mut f: impl FnMut((f64, f64))) {
    match p {
        Prim::Path { pts, .. } => pts.iter().for_each(|q| f(*q)),
        Prim::Dot { at, .. } | Prim::Text { at, .. } => f(*at),
    }
}
