//! Underlays and inherited styles (docs/cad.md, Underlay): another drawing of the document shown
//! under this one, faded and locked, to trace over — a reference by the source's id, so later
//! edits to it show — and the source's layers and styles merged into this drawing.
//!
//! One level deep: the underlay's own underlay is never drawn, and a drawing never takes itself
//! (the document refuses a pair that would take each other).

use serde::{Deserialize, Serialize};

use crate::MAX_COORD;
use crate::model::{Drawing, cut_name};
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
    /// Merges `src`'s layers (with their colour, lineweight and linetype), named dimension
    /// styles (with their level settings) and named text styles into this drawing: a name this
    /// drawing has keeps its own definition, a new one is added (up to the limits). The current
    /// layer and styles stay.
    pub fn inherit(&mut self, src: &Drawing) -> Inherited {
        let mut out = Inherited::default();
        for l in &src.layers {
            if self.layers.len() >= crate::MAX_LAYERS {
                break;
            }
            if !self.layers.iter().any(|m| m.name == l.name) {
                self.layers.push(l.clone());
                out.layers += 1;
            }
        }
        for st in &src.dim_styles {
            if self.dim_styles.len() >= crate::MAX_STYLES {
                break;
            }
            if !self.dim_styles.iter().any(|m| m.name == st.name) {
                self.dim_styles.push(st.clone());
                out.dim_styles += 1;
            }
        }
        for st in &src.text_styles {
            if self.text_styles.len() >= crate::MAX_STYLES {
                break;
            }
            if !self.text_styles.iter().any(|m| m.name == st.name) {
                self.text_styles.push(st.clone());
                out.text_styles += 1;
            }
        }
        self.sanitize();
        out
    }

    /// Drawing `src` as this drawing's underlay shows it: in this drawing's unit and scale,
    /// moved by the underlay's offset, at most [`MAX_UNDERLAY_ENTITIES`] objects, every layer
    /// locked, its colours faded; without an underlay of its own (one level deep).
    pub fn underlay_view(&self, src: &Drawing) -> Drawing {
        let u = self.underlay.clone().unwrap_or_default().sanitized();
        let mut v = src.clone();
        v.underlay = None;
        v.id = None;
        v.fit = None;
        v.entities.truncate(MAX_UNDERLAY_ENTITIES);
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
