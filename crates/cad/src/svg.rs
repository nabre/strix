//! A rendered drawing as SVG: the picture a document shows for a drawing, written next to the
//! drawing's source when the document is saved (docs/cad.md). Sizes are paper millimetres.

use std::fmt::Write as _;

use crate::render::{Anchor, Prim, Rendered};
use crate::{MISSING_END, MISSING_START, UNIT_END, UNIT_START};

/// The colour of a parameter that is not defined (`[L?]`).
pub const MISSING_COLOR: [u8; 3] = [0xC0, 0x1C, 0x1C];

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// A number for SVG: finite, three decimals at most.
fn n(v: f64) -> String {
    let v = if v.is_finite() { (v * 1000.0).round() / 1000.0 } else { 0.0 };
    let s = format!("{v}");
    if s == "-0" { "0".into() } else { s }
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // Control characters are not XML; the markers are handled by the caller.
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

impl Rendered {
    /// The drawing as a standalone SVG document, `size` millimetres wide and high. A parameter's
    /// unit is drawn in `unit_color` (the Unit style's), a missing parameter in red.
    pub fn to_svg(&self, unit_color: [u8; 3]) -> String {
        let (w, h) = (self.size.0.max(1.0), self.size.1.max(1.0));
        let mut out = String::new();
        let _ = write!(
            out,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}mm" height="{h}mm" viewBox="0 0 {w} {h}" font-family="sans-serif">"#,
            w = n(w),
            h = n(h)
        );
        for prim in &self.prims {
            match prim {
                Prim::Path { pts, closed, color, width, dash } => {
                    if pts.len() < 2 {
                        continue;
                    }
                    let points: Vec<String> = pts.iter().map(|&(x, y)| format!("{},{}", n(x), n(y))).collect();
                    let tag = if *closed { "polygon" } else { "polyline" };
                    let _ = write!(
                        out,
                        r#"<{tag} points="{}" fill="none" stroke="{}" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round""#,
                        points.join(" "),
                        hex(*color),
                        n(width.max(0.05))
                    );
                    if !dash.is_empty() {
                        let d: Vec<String> = dash.iter().map(|v| n(*v)).collect();
                        let _ = write!(out, r#" stroke-dasharray="{}""#, d.join(" "));
                    }
                    out.push_str("/>");
                }
                Prim::Dot { at, r, color } => {
                    let _ = write!(out, r#"<circle cx="{}" cy="{}" r="{}" fill="{}"/>"#, n(at.0), n(at.1), n(r.max(0.05)), hex(*color));
                }
                Prim::Text { at, text, height, angle, anchor, color } => {
                    let (ta, base) = match anchor {
                        Anchor::BaselineLeft => ("start", "auto"),
                        Anchor::BottomCenter => ("middle", "text-after-edge"),
                        Anchor::MiddleLeft => ("start", "central"),
                        Anchor::MiddleRight => ("end", "central"),
                    };
                    let _ = write!(
                        out,
                        r#"<text x="{x}" y="{y}" font-size="{}" fill="{}" text-anchor="{ta}" dominant-baseline="{base}""#,
                        n(height / 0.7),
                        hex(*color),
                        x = n(at.0),
                        y = n(at.1)
                    );
                    if angle.abs() > 1e-9 && angle.is_finite() {
                        // Counter-clockwise on paper is a negative SVG rotation (y down).
                        let _ = write!(out, r#" transform="rotate({} {} {})""#, n(-angle), n(at.0), n(at.1));
                    }
                    out.push('>');
                    out.push_str(&spans(text, unit_color));
                    out.push_str("</text>");
                }
            }
        }
        out.push_str("</svg>");
        out
    }
}

/// A text's pieces: plain, unit (the Unit style's colour) and missing (red) spans.
fn spans(text: &str, unit_color: [u8; 3]) -> String {
    let mut out = String::new();
    let mut piece = String::new();
    let mut current: Option<[u8; 3]> = None;
    let flush = |out: &mut String, piece: &mut String, c: Option<[u8; 3]>| {
        if piece.is_empty() {
            return;
        }
        match c {
            Some(c) => {
                let _ = write!(out, r#"<tspan fill="{}">{}</tspan>"#, hex(c), escape(piece));
            }
            None => out.push_str(&escape(piece)),
        }
        piece.clear();
    };
    for ch in text.chars() {
        match ch {
            UNIT_START | MISSING_START => {
                flush(&mut out, &mut piece, current);
                current = Some(if ch == UNIT_START { unit_color } else { MISSING_COLOR });
            }
            UNIT_END | MISSING_END => {
                flush(&mut out, &mut piece, current);
                current = None;
            }
            c => piece.push(c),
        }
    }
    flush(&mut out, &mut piece, current);
    out
}
