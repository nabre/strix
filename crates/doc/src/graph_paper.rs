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
        Some(
            match s {
                "background" => GridPlacement::Background,
                "pageEnd" | "fill" => GridPlacement::PageEnd,
                "untilOdd" => GridPlacement::UntilOdd,
                "untilEven" => GridPlacement::UntilEven,
                "pages" => GridPlacement::Pages { pages },
                "height" => GridPlacement::Height { height },
                _ => return None,
            }
            .sanitized(),
        )
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
    /// With the section in more than one text column, the grid is ruled in each column on its
    /// own (squares from each column's left edge, the space between columns left clear) instead
    /// of across the whole text width.
    pub follow_columns: bool,
    /// The weight of the frame round the ruled area, points; `None`: the edge lines are ordinary
    /// grid lines (the frame weighs what the squares do). A frame is drawn in the ordinary
    /// lines' style and colour.
    pub frame: Option<f32>,
}

impl Default for GraphPaper {
    fn default() -> Self {
        GraphPaper {
            spacing: 5.0 * PT_PER_MM,
            line: GridLine::default(),
            major_every: 0,
            major: GridLine { width: 0.9, color: Rgb(0x6A, 0x9E, 0xD0), ..GridLine::default() },
            placement: GridPlacement::PageEnd,
            follow_columns: false,
            frame: None,
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
            follow_columns: self.follow_columns,
            frame: self.frame.filter(|w| w.is_finite()).map(|w| w.clamp(MIN_LINE_W, MAX_LINE_W)),
        }
    }

    /// The weight the frame is drawn at: its own, else the ordinary lines'.
    pub fn frame_width(&self) -> f32 {
        self.frame.unwrap_or(self.line.width)
    }

    /// How many whole squares a length holds (0 for a non-finite or negative one), at most
    /// [`MAX_LINES`].
    pub fn whole_squares(&self, len: f32) -> usize {
        let g = self.sanitized();
        if !len.is_finite() || len <= 0.0 {
            return 0;
        }
        // A hair over is still a whole square: lengths are sums of f32 points.
        ((len + 0.01) / g.spacing).floor().clamp(0.0, MAX_LINES as f32) as usize
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

    /// The grid as one attribute value (`strix:graph` in `.xmath`, docs/graph-paper.md):
    /// `place=pages:3;spacing=14.173;line=solid:9FC5E8:0.4;every=5;major=solid:6A9ED0:0.9;cols=1;frame=1.5`,
    /// sizes in points (`every`/`major`, `cols` and `frame` only when set). [`GraphPaper::from_attr`] reads it back.
    pub fn to_attr(&self) -> String {
        let g = self.sanitized();
        let num = |v: f32| {
            let t = format!("{:.3}", v);
            let t = t.trim_end_matches('0').trim_end_matches('.');
            if t.is_empty() || t == "-" { "0".to_string() } else { t.to_string() }
        };
        let line = |l: &GridLine| format!("{}:{}:{}", l.style.name(), l.color.hex(), num(l.width));
        let place = match g.placement {
            GridPlacement::Pages { pages } => format!("pages:{pages}"),
            GridPlacement::Height { height } => format!("height:{}", num(height)),
            p => p.name().to_string(),
        };
        let mut out = format!("place={place};spacing={};line={}", num(g.spacing), line(&g.line));
        if g.major_every > 0 {
            out.push_str(&format!(";every={};major={}", g.major_every, line(&g.major)));
        }
        if g.follow_columns {
            out.push_str(";cols=1");
        }
        if let Some(w) = g.frame {
            out.push_str(&format!(";frame={}", num(w)));
        }
        out
    }

    /// Reads [`GraphPaper::to_attr`]'s value: unknown keys and values that don't read are left
    /// at the defaults; `None` only when the placement isn't one this knows. Bounded: a value
    /// longer than 1 KiB is refused.
    pub fn from_attr(s: &str) -> Option<GraphPaper> {
        if s.len() > 1024 {
            return None;
        }
        let mut g = GraphPaper::default();
        let mut placed = false;
        let line = |v: &str, base: GridLine| -> GridLine {
            let mut it = v.split(':');
            let style = it.next().and_then(GridLineStyle::parse).unwrap_or(base.style);
            let color = it.next().and_then(Rgb::parse).unwrap_or(base.color);
            let width = it.next().and_then(|w| w.trim().parse::<f32>().ok()).unwrap_or(base.width);
            GridLine { style, color, width }
        };
        for part in s.split(';') {
            let Some((k, v)) = part.split_once('=') else { continue };
            let v = v.trim();
            match k.trim() {
                "place" => {
                    let (name, arg) = v.split_once(':').unwrap_or((v, ""));
                    let n = arg.trim().parse::<f32>().ok();
                    let pages = n.filter(|n| *n >= 0.0).map_or(1, |n| n.min(MAX_PAGES as f32) as u32);
                    g.placement = GridPlacement::parse(name.trim(), pages, n.unwrap_or(f32::NAN))?;
                    placed = true;
                }
                "spacing" => g.spacing = v.parse().unwrap_or(g.spacing),
                "line" => g.line = line(v, g.line),
                "every" => g.major_every = v.parse::<u32>().unwrap_or(0),
                "major" => g.major = line(v, g.major),
                "cols" => g.follow_columns = v == "1",
                "frame" => g.frame = v.parse::<f32>().ok(),
                _ => {}
            }
        }
        placed.then(|| g.sanitized())
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

    #[test]
    fn attribute_round_trips() {
        let cases = [
            GraphPaper::default(),
            GraphPaper { placement: GridPlacement::Background, ..GraphPaper::default() },
            GraphPaper { placement: GridPlacement::UntilOdd, major_every: 5, ..GraphPaper::default() },
            GraphPaper { placement: GridPlacement::UntilEven, ..GraphPaper::default() },
            GraphPaper { placement: GridPlacement::Pages { pages: 3 }, spacing: 4.0 * PT_PER_MM, ..GraphPaper::default() },
            GraphPaper {
                placement: GridPlacement::Height { height: 80.0 },
                line: GridLine { style: GridLineStyle::Dotted, color: Rgb(1, 2, 3), width: 0.25 },
                major_every: 10,
                major: GridLine { style: GridLineStyle::Dashed, color: Rgb(200, 0, 0), width: 1.5 },
                ..GraphPaper::default()
            },
            GraphPaper { placement: GridPlacement::Background, follow_columns: true, frame: Some(2.25), ..GraphPaper::default() },
        ];
        for g in cases {
            let back = GraphPaper::from_attr(&g.to_attr()).unwrap();
            assert_eq!(back.placement, g.placement, "{}", g.to_attr());
            assert!((back.spacing - g.spacing).abs() < 0.001);
            assert_eq!(back.line.style, g.line.style);
            assert_eq!(back.line.color, g.line.color);
            assert_eq!(back.major_every, g.major_every);
            assert_eq!(back.follow_columns, g.follow_columns);
            assert_eq!(back.frame, g.frame);
            if g.major_every > 0 {
                assert_eq!(back.major.color, g.major.color);
                assert!((back.major.width - g.major.width).abs() < 0.001);
            }
        }
    }

    #[test]
    fn hostile_attributes_read_safely() {
        for s in [
            "",
            "place=nowhere",
            "spacing=5",
            &"x".repeat(5000),
            "place=pages:-4",
            "place=pages:1e40",
            "place=height:NaN;spacing=inf;line=:::;every=-1",
            "place=background;cols=yes;frame=NaN",
            "place=background;frame=1e30;cols=1",
        ] {
            if let Some(g) = GraphPaper::from_attr(s) {
                assert_eq!(g, g.sanitized());
            }
        }
        assert_eq!(GraphPaper::from_attr("place=pages:1e40").unwrap().placement, GridPlacement::Pages { pages: MAX_PAGES });
        assert!(GraphPaper::from_attr("spacing=5").is_none());
        let g = GraphPaper::from_attr("place=background;frame=1e30;cols=1").unwrap();
        assert_eq!((g.frame, g.follow_columns), (Some(MAX_LINE_W), true));
        let g = GraphPaper::from_attr("place=background;frame=inf;cols=0").unwrap();
        assert_eq!((g.frame, g.follow_columns), (None, false));
    }

    #[test]
    fn enter_at_a_blocks_start_takes_the_grid_down_with_its_text() {
        let grid = || {
            let mut p = crate::Paragraph::with_text("on the squares", Default::default());
            p.props.graph = Some(std::sync::Arc::new(GraphPaper::default()));
            p
        };
        // At its start: the empty part above is ordinary, the text keeps the grid.
        let mut p = grid();
        let tail = p.split_off(0).unwrap();
        assert!(p.props.graph.is_none() && tail.props.graph.is_some());
        // Anywhere else: the grid stays on the first part.
        let mut p = grid();
        let tail = p.split_off(3).unwrap();
        assert!(p.props.graph.is_some() && tail.props.graph.is_none());
        let mut p = grid();
        let end = p.text.len();
        let tail = p.split_off(end).unwrap();
        assert!(p.props.graph.is_some() && tail.props.graph.is_none());
        // An empty block keeps its grid: Enter in it goes on below.
        let mut p = crate::Paragraph::new();
        p.props.graph = Some(std::sync::Arc::new(GraphPaper::default()));
        let tail = p.split_off(0).unwrap();
        assert!(p.props.graph.is_some() && tail.props.graph.is_none());
    }

    #[test]
    fn frame_weighs_the_lines_by_default_and_whole_squares_count() {
        let g = GraphPaper { spacing: 10.0, ..GraphPaper::default() };
        assert_eq!(g.frame_width(), g.line.width);
        assert_eq!(GraphPaper { frame: Some(2.0), ..g }.frame_width(), 2.0);
        assert_eq!(GraphPaper { frame: Some(-1.0), ..g }.sanitized().frame, Some(MIN_LINE_W));
        assert_eq!(g.whole_squares(39.999), 4);
        assert_eq!(g.whole_squares(39.9), 3);
        assert_eq!(g.whole_squares(f32::NAN), 0);
        assert_eq!(g.whole_squares(-5.0), 0);
        assert_eq!(GraphPaper { spacing: 0.0, ..g }.whole_squares(f32::MAX), MAX_LINES);
    }
}
