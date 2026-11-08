//! A rendered drawing as SVG: the picture a document shows for a drawing, written next to the
//! drawing's source when the document is saved (docs/cad.md). Sizes are paper millimetres.

use std::fmt::Write as _;

use crate::render::{Anchor, Prim, Rendered};

/// The colour of a parameter that is not defined (`[L?]`).
pub const MISSING_COLOR: [u8; 3] = [0xC0, 0x1C, 0x1C];

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// A number for SVG: finite, three decimals at most.
fn n(v: f64) -> String {
    let mut s = String::new();
    put(&mut s, v);
    s
}

/// Writes [`n`]`(v)` at the end of `out`, without a string of its own (a drawing's paths hold
/// up to millions of numbers).
fn put(out: &mut String, v: f64) {
    let m = if v.is_finite() { (v * 1000.0).round() } else { 0.0 };
    if m.abs() >= 1e15 {
        // Past fifteen digits, as Rust writes it (no SVG draws this far out).
        let _ = write!(out, "{}", m / 1000.0);
        return;
    }
    // In thousandths, as integers: the digits `{}` would write for `m / 1000`, several times
    // faster. `-0` is written `0`.
    let t = m as i64;
    if t < 0 {
        out.push('-');
    }
    let (whole, frac) = (t.unsigned_abs() / 1000, t.unsigned_abs() % 1000);
    let _ = write!(out, "{whole}");
    if frac > 0 {
        let digits = format!("{frac:03}");
        out.push('.');
        out.push_str(digits.trim_end_matches('0'));
    }
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
        // A drawing of millions of points is written with fewer (a few MB at most).
        let light = self.lightened();
        let mut prims = light.prims.iter().peekable();
        while let Some(prim) = prims.next() {
            match prim {
                // Runs of single strokes in one style (a hatch's lines, mostly) are one path of
                // as many pieces: the same picture in a fraction of the bytes.
                Prim::Path { pts, closed: false, color, width, dash, fill: false } if pts.len() == 2 && batches(prim, prims.peek().copied()) => {
                    let mut d = String::new();
                    let mut piece = |pts: &[(f64, f64)]| {
                        if let [a, b] = pts {
                            for (k, v) in [a.0, a.1, b.0, b.1].into_iter().enumerate() {
                                d.push_str(["M", " ", "L", " "].get(k).copied().unwrap_or(" "));
                                put(&mut d, v);
                            }
                        }
                    };
                    piece(pts);
                    while let Some(next) = prims.next_if(|q| batches(prim, Some(*q))) {
                        if let Prim::Path { pts, .. } = next {
                            piece(pts);
                        }
                    }
                    let _ = write!(
                        out,
                        r#"<path d="{d}" fill="none" stroke="{}" stroke-width="{}" stroke-linecap="round""#,
                        hex(*color),
                        n(width.max(0.05))
                    );
                    if !dash.is_empty() {
                        let d: Vec<String> = dash.iter().map(|v| n(*v)).collect();
                        let _ = write!(out, r#" stroke-dasharray="{}""#, d.join(" "));
                    }
                    out.push_str("/>");
                }
                Prim::Path { pts, closed, color, width, dash, fill } => {
                    if pts.len() < 2 {
                        continue;
                    }
                    let tag = if *closed { "polygon" } else { "polyline" };
                    let _ = write!(out, r#"<{tag} points=""#);
                    for (k, &(x, y)) in pts.iter().enumerate() {
                        if k > 0 {
                            out.push(' ');
                        }
                        put(&mut out, x);
                        out.push(',');
                        put(&mut out, y);
                    }
                    let _ = write!(
                        out,
                        r#"" fill="{}" stroke="{}" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round""#,
                        if *fill { hex(*color) } else { "none".into() },
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
                        Anchor::BaselineCenter => ("middle", "auto"),
                        Anchor::BaselineRight => ("end", "auto"),
                        Anchor::BottomCenter => ("middle", "text-after-edge"),
                        Anchor::TopCenter => ("middle", "text-before-edge"),
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

/// Whether `next` is a single open stroke drawn exactly as `first` (colour, width, dash), so
/// the two can be pieces of one SVG path.
fn batches(first: &Prim, next: Option<&Prim>) -> bool {
    match (first, next) {
        (
            Prim::Path { pts: _, closed: false, color, width, dash, fill: false },
            Some(Prim::Path { pts: p2, closed: false, color: c2, width: w2, dash: d2, fill: false }),
        ) => p2.len() == 2 && color == c2 && width == w2 && dash == d2,
        _ => false,
    }
}

/// A text's pieces: plain, unit (the Unit style's colour) and missing (red) spans; a parameter's
/// symbol in italics, its subscript smaller and lowered (as the equations set them).
fn spans(text: &str, unit_color: [u8; 3]) -> String {
    let mut out = String::new();
    for p in crate::spans(text) {
        let mut attrs = String::new();
        match p.kind {
            crate::SpanKind::Unit => {
                let _ = write!(attrs, r#" fill="{}""#, hex(unit_color));
            }
            crate::SpanKind::Missing => {
                let _ = write!(attrs, r#" fill="{}""#, hex(MISSING_COLOR));
            }
            crate::SpanKind::Plain => {}
        }
        if p.symbol {
            attrs.push_str(r#" font-style="italic""#);
        }
        if p.sub {
            attrs.push_str(r#" baseline-shift="sub" font-size="70%""#);
        }
        if attrs.is_empty() {
            out.push_str(&escape(&p.text));
        } else {
            let _ = write!(out, "<tspan{attrs}>{}</tspan>", escape(&p.text));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn numbers_are_written_as_rust_writes_them() {
        for v in [
            0.0,
            -0.0,
            -0.0004,
            0.0005,
            1.0,
            -1.5,
            2.25,
            1234.567,
            -0.001,
            0.12,
            99999.9999,
            4.2e9,
            123456789012.345,
            3e14,
            -7e20,
            f64::NAN,
            f64::INFINITY,
        ] {
            let m = if v.is_finite() { (v * 1000.0_f64).round() / 1000.0 } else { 0.0 };
            let want = format!("{}", if m == 0.0 { 0.0 } else { m });
            assert_eq!(super::n(v), want, "{v}");
        }
    }
}
