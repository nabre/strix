//! Picking what is drawn filled or as words anywhere on it, and the grips that edit it: a text's
//! box (picked inside, its four corners move it), a line's arrowheads (picked inside), a hatch's
//! boundaries (their grips shown with the hatch's, so a solid fill can be reshaped).

use crate::edit::{Grip, paper_per_unit};
use crate::geom::Pt;
use crate::model::{Align, Drawing, Entity, Kind};

/// A text's descent below its baseline and its ascent above it, as parts of its height (paper
/// mm: the capitals' height; the font is `height / 0.7` big, as every output draws it), with a
/// little room past the ascenders (k, /) and descenders (q, g) so the box clears the letters.
const DESCENT: f64 = 0.38;
const ASCENT: f64 = 1.15;

/// About how wide a character is in a proportional sans font, in ems.
fn char_em(c: char) -> f64 {
    match c {
        ' ' | 'i' | 'j' | 'l' | 'I' | '.' | ',' | ':' | ';' | '!' | '|' | '\'' | '(' | ')' | '[' | ']' | 'f' | 't' | 'r' | '/' => 0.3,
        'm' | 'w' | 'M' | 'W' | '@' | '%' => 0.85,
        c if c.is_uppercase() => 0.68,
        _ => 0.55,
    }
}

/// About how many ems text `value` takes as drawn: placeholders count as what they show (a value
/// with its unit, the symbol, or both), marker characters as nothing. At most 4096 characters
/// are counted.
fn text_ems(value: &str) -> f64 {
    let (value_ems, mut ems, mut rest) = (3.4, 0.0, value);
    let plain = |s: &str| s.chars().filter(|c| !crate::is_marker(*c)).take(4096).map(char_em).sum::<f64>();
    for _ in 0..64 {
        let Some(i) = rest.find("{{") else { break };
        let Some(j) = rest.get(i..).and_then(|r| r.find("}}")).map(|j| i + j) else { break };
        ems += plain(rest.get(..i).unwrap_or_default());
        let inner = rest.get(i + 2..j).unwrap_or_default().trim();
        let (name, mode) = inner.split_once(':').map_or((inner, "value"), |(n, m)| (n.trim(), m.trim()));
        ems += match mode {
            "symbol" => plain(name),
            "both" => plain(name) + plain(" = ") + value_ems,
            _ => value_ems,
        };
        rest = rest.get(j + 2..).unwrap_or_default();
    }
    ems + plain(rest)
}

/// Texts' widths as an editor measured them with the fonts it draws them in: for each text
/// entity's id, the value it was measured with and its width in ems (the font's size, `height /
/// 0.7` paper mm). A text whose value changed since is estimated again. Never saved; drawings
/// compare equal whatever their measures.
#[derive(Debug, Clone, Default)]
pub struct TextWidths(std::collections::HashMap<String, (String, f64)>);

impl PartialEq for TextWidths {
    fn eq(&self, _: &TextWidths) -> bool {
        true
    }
}

impl TextWidths {
    /// Text `id` with value `value` measured `ems` wide. Refused when not a size (or more than a
    /// drawing has texts).
    pub fn set(&mut self, id: &str, value: &str, ems: f64) {
        if id.is_empty() || !ems.is_finite() || !(0.0..=1e5).contains(&ems) || (self.0.len() >= crate::MAX_ENTITIES && !self.0.contains_key(id)) {
            return;
        }
        if self.0.get(id).is_some_and(|(v, w)| v == value && *w == ems) {
            return;
        }
        self.0.insert(id.to_string(), (value.to_string(), ems));
    }

    /// The width text `id` was measured, in ems, when its value is still `value`.
    pub fn get(&self, id: &str, value: &str) -> Option<f64> {
        self.0.get(id).filter(|(v, _)| v == value).map(|(_, w)| *w)
    }
}

impl Drawing {
    /// The box text `e` covers as drawn, in drawing units: its corners counter-clockwise from
    /// the bottom left (turned with the text). Its width is the editor's measure when it took
    /// one ([`TextWidths`]), else estimated from the characters, close enough to pick it.
    pub fn text_box(&self, e: &Entity) -> Option<[Pt; 4]> {
        let Kind::Text { p, value, height, angle, align } = &e.kind else { return None };
        let h = height / paper_per_unit(self);
        let ems = self.text_widths.get(&e.id, value).unwrap_or_else(|| {
            let bold = if self.text_look(e.style.as_deref()).bold { 1.08 } else { 1.0 };
            text_ems(value) * bold
        });
        let w = (ems * h / 0.7).max(h * 0.3);
        let x0 = match align {
            Align::Left => 0.0,
            Align::Center => -w / 2.0,
            Align::Right => -w,
        };
        let (y0, y1) = (-DESCENT * h, ASCENT * h);
        let (s, c) = angle.to_radians().sin_cos();
        let at = p.pt();
        let place = |x: f64, y: f64| Pt::new(at.x + x * c - y * s, at.y + x * s + y * c);
        let corners = [place(x0, y0), place(x0 + w, y0), place(x0 + w, y1), place(x0, y1)];
        corners.iter().all(|q| q.is_finite()).then_some(corners)
    }

    /// The arrowheads entity `e` carries (an open line, arc or polyline), in drawing units: each
    /// triangle tip first, where the renderer draws it.
    pub(crate) fn arrowhead_triangles(&self, e: &Entity) -> Vec<[Pt; 3]> {
        if e.arrows == crate::model::Ends::None || !matches!(e.kind, Kind::Line { .. } | Kind::Arc { .. } | Kind::Polyline { closed: false, .. }) {
            return Vec::new();
        }
        let k = paper_per_unit(self);
        let size = self.dim_style.mark_size / k;
        let walk = self.dim_style.mark_size.max(0.5) / k;
        let Some(pts) = crate::edit::outline(e).into_iter().next() else { return Vec::new() };
        // As the renderer does: along the first chord at least an arrowhead long from the tip.
        let tip_dir = |mut it: Box<dyn Iterator<Item = &Pt> + '_>| -> Option<(Pt, Pt)> {
            let tip = *it.next()?;
            let mut fallback = None;
            for &q in it {
                let d = q.minus(tip);
                if d.len() >= walk {
                    return Some((tip, d.unit()));
                }
                if d.len() > 1e-12 {
                    fallback = Some(d.unit());
                }
            }
            fallback.map(|d| (tip, d))
        };
        let head = |(tip, dir): (Pt, Pt)| {
            let (back, side) = (dir.scale(size), dir.left().scale(size * 15f64.to_radians().tan()));
            [tip, tip.plus(back).plus(side), tip.plus(back).minus(side)]
        };
        let mut out = Vec::new();
        if e.arrows.start() {
            out.extend(tip_dir(Box::new(pts.iter())).map(head));
        }
        if e.arrows.end() {
            out.extend(tip_dir(Box::new(pts.iter().rev())).map(head));
        }
        out.retain(|t| t.iter().all(|q| q.is_finite()));
        out
    }

    /// How far `p` is from the arrowheads of `e`: 0 inside one, infinite when it has none.
    pub(crate) fn arrowhead_distance(&self, e: &Entity, p: Pt) -> f64 {
        self.arrowhead_triangles(e)
            .iter()
            .map(|t| {
                if crate::select::point_in_polygon(p, t) {
                    0.0
                } else {
                    (0..3).map(|j| crate::select::seg_dist(p, t[j], t[(j + 1) % 3])).fold(f64::INFINITY, f64::min)
                }
            })
            .fold(f64::INFINITY, f64::min)
    }

    /// The grips shown when entity `i` is selected, each with the entity it changes: its own,
    /// and a hatch's boundaries' or a strip's path's (dragging them reshapes what follows them).
    pub fn grip_targets(&self, i: usize) -> Vec<(usize, Grip, Pt)> {
        let mut out: Vec<(usize, Grip, Pt)> = self.grips(i).into_iter().map(|(g, p)| (i, g, p)).collect();
        for j in self.followed(&[i]) {
            out.extend(self.grips(j).into_iter().map(|(g, p)| (j, g, p)));
        }
        out
    }

    /// The objects the hatches and strips at `indices` follow (their boundaries and paths), not
    /// already in `indices`, each once.
    pub(crate) fn followed(&self, indices: &[usize]) -> Vec<usize> {
        let mut out: Vec<usize> = Vec::new();
        for e in indices.iter().filter_map(|&i| self.entities.get(i)) {
            let ids: Vec<&str> = match &e.kind {
                Kind::Hatch { boundary, .. } => boundary.iter().map(String::as_str).take(64).collect(),
                Kind::Strip { along, .. } => vec![along.as_str()],
                _ => continue,
            };
            for id in ids {
                if let Some(j) = self.entities.iter().position(|o| o.id == id)
                    && !indices.contains(&j)
                    && !out.contains(&j)
                {
                    out.push(j);
                }
            }
        }
        out
    }

    /// `indices` with the boundaries of the hatches among them: a hatch moved, turned or copied
    /// on its own takes its boundaries along (it is drawn from them).
    pub(crate) fn with_boundaries(&self, indices: &[usize]) -> Vec<usize> {
        let hatches: Vec<usize> =
            indices.iter().copied().filter(|&i| matches!(self.entities.get(i).map(|e| &e.kind), Some(Kind::Hatch { .. }))).collect();
        let mut out = indices.to_vec();
        out.extend(self.followed(&hatches).into_iter().filter(|j| !indices.contains(j)));
        out
    }
}
