//! Standalone SVG serializer for [`PlotRendered`].

use std::fmt::Write as _;

use crate::render::{PlotAnchor, PlotPrim, PlotRendered};

impl PlotRendered {
    /// Renders the plot as a standalone SVG document in millimetres.
    pub fn to_svg(&self) -> String {
        let (w, h) = (self.size.0.max(1.0), self.size.1.max(1.0));
        let mut out = String::with_capacity(2048);
        let _ = write!(
            out,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}mm" height="{h}mm" viewBox="0 0 {w} {h}" font-family="sans-serif">"#,
            w = fmt(w),
            h = fmt(h),
        );
        for prim in &self.prims {
            match prim {
                PlotPrim::Rect { rect, fill, stroke } => {
                    let [x, y, rw, rh] = *rect;
                    let fill_attr = fill.map_or_else(|| "none".to_string(), hex);
                    let _ = write!(out, r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{fill_attr}""#, fmt(x), fmt(y), fmt(rw), fmt(rh),);
                    if let Some((sc, sw)) = stroke {
                        let _ = write!(out, r#" stroke="{}" stroke-width="{}""#, hex(*sc), fmt(*sw));
                    }
                    out.push_str("/>");
                }
                PlotPrim::Path { pts, closed, stroke, fill, dash } => {
                    if pts.len() < 2 {
                        continue;
                    }
                    let tag = if *closed { "polygon" } else { "polyline" };
                    let _ = write!(out, r#"<{tag} points=""#);
                    for (i, &(x, y)) in pts.iter().enumerate() {
                        if i > 0 {
                            out.push(' ');
                        }
                        let _ = write!(out, "{},{}", fmt(x), fmt(y));
                    }
                    let fill_attr = fill.map_or_else(|| "none".to_string(), hex);
                    let _ = write!(out, r#"" fill="{fill_attr}""#);
                    if let Some((sc, sw)) = stroke {
                        let _ = write!(out, r#" stroke="{}" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round""#, hex(*sc), fmt(*sw),);
                    }
                    if !dash.is_empty() {
                        let d: Vec<String> = dash.iter().map(|v| fmt(*v)).collect();
                        let _ = write!(out, r#" stroke-dasharray="{}""#, d.join(" "));
                    }
                    out.push_str("/>");
                }
                PlotPrim::Dot { at, r, fill, stroke } => {
                    let _ = write!(out, r#"<circle cx="{}" cy="{}" r="{}" fill="{}""#, fmt(at.0), fmt(at.1), fmt(*r), hex(*fill),);
                    if let Some((sc, sw)) = stroke {
                        let _ = write!(out, r#" stroke="{}" stroke-width="{}""#, hex(*sc), fmt(*sw));
                    }
                    out.push_str("/>");
                }
                PlotPrim::Text { at, text, height, bold, anchor, color } => {
                    let (ta, dy) = match anchor {
                        PlotAnchor::Left => ("start", "0"),
                        PlotAnchor::Center => ("middle", "0"),
                        PlotAnchor::Right => ("end", "0"),
                        PlotAnchor::TopCenter => ("middle", "0.85em"),
                        PlotAnchor::MiddleRight => ("end", "0.35em"),
                    };
                    let weight = if *bold { r#" font-weight="bold""# } else { "" };
                    let _ = write!(
                        out,
                        r#"<text x="{}" y="{}" font-size="{}" fill="{}" text-anchor="{ta}" dy="{dy}"{weight}>{}</text>"#,
                        fmt(at.0),
                        fmt(at.1),
                        fmt(*height * 1.25),
                        hex(*color),
                        escape_xml(text),
                    );
                }
            }
        }
        out.push_str("</svg>");
        out
    }
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn fmt(v: f64) -> String {
    if !v.is_finite() {
        return "0".to_string();
    }
    let s = format!("{:.3}", v);
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    if trimmed == "-0" || trimmed.is_empty() { "0".to_string() } else { trimmed.to_string() }
}

fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}
