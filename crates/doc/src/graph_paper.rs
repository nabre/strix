//! Graph paper ("foglio a quadretti", docs/graph-paper.md): a square grid of lines drawn behind
//! the text, either as the background of a section's pages ([`crate::SectionProps::graph_paper`])
//! or as a block in the text flow held by a paragraph ([`crate::props::ParaProps::graph`]) that
//! takes a fixed height, the rest of its page, or whole pages after it.
//!
//! Everything here is clamped as it is read ([`GraphPaper::sanitized`]): spacing, line widths,
//! page counts and heights come from files and commands and are hostile.

use serde::{Deserialize, Serialize};

use crate::props::Rgb;

/// Points per millimetre.
pub const PT_PER_MM: f32 = 72.0 / 25.4;
/// The smallest and largest square, in millimetres.
pub const MIN_SPACING_MM: f32 = 1.0;
pub const MAX_SPACING_MM: f32 = 100.0;
/// Line widths, points.
pub const MIN_LINE_W: f32 = 0.1;
pub const MAX_LINE_W: f32 = 6.0;
/// A heavier line every N squares at most.
pub const MAX_MAJOR_EVERY: u32 = 100;
/// Whole pages one grid block may take ([`GridPlacement::Pages`]).
pub const MAX_PAGES: u32 = 100;
/// The tallest fixed-height block, millimetres (it is also capped at the page's text height).
pub const MAX_HEIGHT_MM: f32 = 2000.0;
/// Lines drawn one way in one grid area at most (a backstop: 1 mm squares on the largest page
/// Word allows are well under it).
pub const MAX_LINES: usize = 4096;

/// How a grid line is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GridLineStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

impl GridLineStyle {
    pub fn name(self) -> &'static str {
        match self {
            GridLineStyle::Solid => "solid",
            GridLineStyle::Dashed => "dashed",
            GridLineStyle::Dotted => "dotted",
        }
    }
    pub fn parse(s: &str) -> Option<GridLineStyle> {
        match s {
            "solid" | "single" => Some(GridLineStyle::Solid),
            "dashed" | "dash" => Some(GridLineStyle::Dashed),
            "dotted" | "dot" => Some(GridLineStyle::Dotted),
            _ => None,
        }
    }
    /// Dash and gap lengths in points for a line `width` wide; empty for a solid line.
    pub fn dashes(self, width: f32) -> Vec<f32> {
        match self {
            GridLineStyle::Solid => Vec::new(),
            GridLineStyle::Dashed => vec![(width * 6.0).max(2.0), (width * 4.0).max(1.5)],
            GridLineStyle::Dotted => vec![width.max(0.3), (width * 3.0).max(1.2)],
        }
    }
}

/// One kind of grid line: style, colour and width (points).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GridLine {
    pub style: GridLineStyle,
    pub color: Rgb,
    pub width: f32,
}

impl Default for GridLine {
    fn default() -> Self {
        GridLine { style: GridLineStyle::Solid, color: Rgb(0x9F, 0xC5, 0xE8), width: 0.4 }
    }
}

impl GridLine {
    fn sanitized(self) -> GridLine {
        let width = if self.width.is_finite() { self.width.clamp(MIN_LINE_W, MAX_LINE_W) } else { GridLine::default().width };
        GridLine { width, ..self }
    }
}

/// Where a grid goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum GridPlacement {
    /// Behind the text of every page of the section, over its margin box (the text area).
    Background,
    /// A block from where it is in the text to the bottom of the page.
    #[default]
    PageEnd,
    /// The rest of this page, then whole pages until the next odd-numbered page starts.
    UntilOdd,
    /// The rest of this page, then whole pages until the next even-numbered page starts.
    UntilEven,
    /// The rest of this page and `pages - 1` whole pages after it: `pages` pages with the grid.
    Pages { pages: u32 },
    /// A block `height` points tall (moved to the next page when it doesn't fit).
    Height { height: f32 },
}

impl GridPlacement {
    pub fn name(self) -> &'static str {
        match self {
            GridPlacement::Background => "background",
            GridPlacement::PageEnd => "pageEnd",
            GridPlacement::UntilOdd => "untilOdd",
            GridPlacement::UntilEven => "untilEven",
            GridPlacement::Pages { .. } => "pages",
            GridPlacement::Height { .. } => "height",
        }
    }
    /// The placement a name gives, with the page count and height (points) the named ones take.
    pub fn parse(s: &str, pages: u32, height: f32) -> Option<GridPlacement> {
        Some(match s {
            "background" => GridPlacement::Background,
            "pageEnd" | "fill" => GridPlacement::PageEnd,
            "untilOdd" => GridPlacement::UntilOdd,
            "untilEven" => GridPlacement::UntilEven,
            "pages" => GridPlacement::Pages { pages },
            "height" => GridPlacement::Height { height },
            _ => return None,
        }
        .sanitized())
    }
    pub fn sanitized(self) -> GridPlacement {
        match self {
            GridPlacement::Pages { pages } => GridPlacement::Pages { pages: pages.clamp(1, MAX_PAGES) },
            GridPlacement::Height { height } => {
                let h = if height.is_finite() { height } else { 50.0 * PT_PER_MM };
                GridPlacement::Height { height: h.clamp(MIN_SPACING_MM * PT_PER_MM, MAX_HEIGHT_MM * PT_PER_MM) }
            }
            other => other,
        }
    }
}

/// A square grid (see the module docs).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GraphPaper {
    /// The side of a square, points.
    pub spacing: f32,
    pub line: GridLine,
    /// A heavier line every this many squares (0: none), drawn as `major`.
    pub major_every: u32,
    pub major: GridLine,
    pub placement: GridPlacement,
}

impl Default for GraphPaper {
    fn default() -> Self {
        GraphPaper {
            spacing: 5.0 * PT_PER_MM,
            line: GridLine::default(),
            major_every: 0,
            major: GridLine { width: 0.9, color: Rgb(0x6A, 0x9E, 0xD0), ..GridLine::default() },
            placement: GridPlacement::PageEnd,
        }
    }
}

impl GraphPaper {
    /// Every value brought into range (non-finite ones to the defaults).
    pub fn sanitized(self) -> GraphPaper {
        let d = GraphPaper::default();
        let spacing = if self.spacing.is_finite() { self.spacing } else { d.spacing };
        GraphPaper {
            spacing: spacing.clamp(MIN_SPACING_MM * PT_PER_MM, MAX_SPACING_MM * PT_PER_MM),
            line: self.line.sanitized(),
            major_every: self.major_every.min(MAX_MAJOR_EVERY),
            major: self.major.sanitized(),
            placement: self.placement.sanitized(),
        }
    }

    /// The grid's lines across an area `w` × `h` points whose top-left corner is the grid's
    /// origin: offsets (from that corner) of the vertical lines, then of the horizontal ones,
    /// each with whether it is a major line. Lines sit on whole squares (`k * spacing`) that fit
    /// inside the area, the first on its edge; at most [`MAX_LINES`] each way.
    pub fn lines(&self, w: f32, h: f32) -> (Vec<(f32, bool)>, Vec<(f32, bool)>) {
        let g = self.sanitized();
        let along = |len: f32| -> Vec<(f32, bool)> {
            if !len.is_finite() || len < 0.0 {
                return Vec::new();
            }
            let mut v = Vec::new();
            for k in 0..MAX_LINES {
                let at = k as f32 * g.spacing;
                if at > len + 0.01 {
                    break;
                }
                let major = g.major_every > 0 && (k as u32).is_multiple_of(g.major_every);
                v.push((at, major));
            }
            v
        };
        (along(w), along(h))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostile_values_are_clamped() {
        let g = GraphPaper {
            spacing: f32::NAN,
            line: GridLine { width: 1e9, ..GridLine::default() },
            major_every: u32::MAX,
            placement: GridPlacement::Pages { pages: u32::MAX },
            ..GraphPaper::default()
        }
        .sanitized();
        assert!((g.spacing - 5.0 * PT_PER_MM).abs() < 0.01);
        assert_eq!(g.line.width, MAX_LINE_W);
        assert_eq!(g.major_every, MAX_MAJOR_EVERY);
        assert_eq!(g.placement, GridPlacement::Pages { pages: MAX_PAGES });
        let tiny = GraphPaper { spacing: 0.0, ..GraphPaper::default() }.sanitized();
        assert!((tiny.spacing - PT_PER_MM).abs() < 0.01);
        assert_eq!(GridPlacement::parse("height", 0, f32::INFINITY), Some(GridPlacement::Height { height: 50.0 * PT_PER_MM }));
    }

    #[test]
    fn lines_sit_on_whole_squares_and_are_capped() {
        let g = GraphPaper { spacing: 10.0, major_every: 5, ..GraphPaper::default() };
        let (v, h) = g.lines(45.0, 20.0);
        assert_eq!(v.iter().map(|l| l.0).collect::<Vec<_>>(), vec![0.0, 10.0, 20.0, 30.0, 40.0]);
        assert_eq!(h.len(), 3);
        assert!(v.first().is_some_and(|l| l.1));
        assert!(!v.get(1).is_some_and(|l| l.1));
        let (v, _) = GraphPaper { spacing: 0.0, ..g }.lines(f32::MAX, 1.0);
        assert_eq!(v.len(), MAX_LINES);
        assert!(g.lines(f32::NAN, -1.0).0.is_empty());
    }
}
