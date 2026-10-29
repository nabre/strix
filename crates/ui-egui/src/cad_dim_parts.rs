//! The CAD editor's overlay for a dimension's two-part distance (docs/cad.md): a linear
//! dimension's line sits a scaled distance (drawing units: it follows the drawing) plus an
//! annotative one (paper mm: the same in print) from its measured points, an angle's arc a
//! scaled plus an annotative radius from its vertex. For the selected dimensions, the one being
//! placed and the one a grip is dragging, the editor draws the reference line or arc at the
//! scaled distance (dash and two dots, ISO 128's phantom line: the boundary between the two
//! parts), the scaled stretch up to it and the annotative stretch on to the
//! dimension line or arc, each with its size, in two colours the Properties pane uses for the
//! same two numbers. On screen only: never printed, exported or in the picture.

use egui::{Align2, Color32, FontId, Pos2, Stroke, Vec2, vec2};
use strix_cad::{Dash, DimParts, Kind, Num, Pt};

/// The scaled part (drawing units: it follows the drawing's size and scale): blue.
pub(crate) const SCALED: Color32 = Color32::from_rgb(0x25, 0x63, 0xEB);
/// The annotative part (paper mm: the same in print at any scale): magenta.
pub(crate) const ANNOTATIVE: Color32 = Color32::from_rgb(0xC0, 0x26, 0xD3);

/// How far a label sits from its stretch, and a linear reference runs past its ends, px.
const LABEL_GAP: f32 = 7.0;
const REFERENCE_OVER: f32 = 10.0;
/// Screen points per paper mm of a linetype's pattern, as the linetype samples show it.
const PATTERN_PX_PER_MM: f32 = 2.5;

/// The pieces drawn of the path through `points` with a linetype's on/off `pattern` (screen
/// points), carried on across the path's corners so an arc's chords read as one line.
fn pattern_segments(points: &[Pos2], pattern: &[f32]) -> Vec<[Pos2; 2]> {
    let mut out = Vec::new();
    let pairs = points.windows(2).filter_map(|w| match w {
        [a, b] => Some((*a, *b)),
        _ => None,
    });
    // A pattern with no length (or none) is a solid line.
    let Some(&first) = pattern.first().filter(|_| pattern.iter().all(|v| v.is_finite() && *v > 0.0)) else {
        out.extend(pairs.map(|(a, b)| [a, b]));
        return out;
    };
    // Where in the pattern the path is: which length, and how much of it is left.
    let (mut k, mut left) = (0usize, first);
    for (a, b) in pairs {
        let len = a.distance(b);
        let mut t = 0.0;
        while t < len {
            let step = left.min(len - t);
            if k % 2 == 0 {
                out.push([a.lerp(b, t / len), a.lerp(b, (t + step) / len)]);
            }
            t += step;
            left -= step;
            if left <= 1e-4 {
                k = (k + 1) % pattern.len();
                left = pattern.get(k).copied().unwrap_or(first);
            }
        }
    }
    out
}

/// Draws the path through `points` as a dash and two dots (the reference at the scaled distance).
fn phantom(painter: &egui::Painter, points: &[Pos2], stroke: Stroke) {
    let pattern: Vec<f32> = Dash::DashDoubleDotted.pattern().iter().map(|&v| (v as f32 * PATTERN_PX_PER_MM).max(1.5)).collect();
    for [a, b] in pattern_segments(points, &pattern) {
        painter.line_segment([a, b], stroke);
    }
}

/// The colour and the explanation (English) of a dimension's or angle's Properties field that is
/// one of the two parts.
pub(crate) fn field_part(kind: &Kind, key: &str) -> Option<(Color32, &'static str)> {
    match (kind, key) {
        (Kind::Dim { .. }, "offsetUnits") | (Kind::Angle { .. }, "radius") => {
            Some((SCALED, "In the drawing's units: it follows the drawing's size and scale (scaled)"))
        }
        (Kind::Dim { .. }, "offset") | (Kind::Angle { .. }, "radiusPaper") => {
            Some((ANNOTATIVE, "On paper, mm: the same in print at any scale (annotative)"))
        }
        _ => None,
    }
}

/// The two stretches' labels: the scaled size in the drawing's `unit`, the annotative in mm.
pub(crate) fn labels(parts: &DimParts, unit: &str) -> (String, String) {
    let size = |v: f64, unit: &str| format!("{} {unit}", strix_cad::num_text(&Num::new(v)));
    (
        crate::i18n::fmt(tl!("scaled {size}"), &[("size", &size(parts.scaled_size, unit))]),
        crate::i18n::fmt(tl!("annotative {size}"), &[("size", &size(parts.annotative_size, "mm"))]),
    )
}

/// The side a stretch's labels go to, on the screen: across the stretches (the first one that
/// has a length), else across the reference, else up; pointing right (or down when level).
fn across(screen: &[(Pos2, Pos2)], reference: &[Pos2]) -> Vec2 {
    let along = screen.iter().map(|(a, b)| *b - *a).find(|v| v.length() >= 1.0).or_else(|| match reference {
        [a, .., b] if a.distance(*b) >= 1.0 => Some((*b - *a).rot90()),
        _ => None,
    });
    let side = along.map_or(vec2(1.0, 0.0), |v| v.normalized().rot90());
    if side.x < -1e-3 || (side.x.abs() <= 1e-3 && side.y < 0.0) { -side } else { side }
}

/// The anchor of a label put on `side` of its point (facing the point): a corner when the side
/// is oblique, so the two labels of a slanted pair stay apart.
fn anchor(side: Vec2) -> Align2 {
    const OBLIQUE: f32 = 0.3;
    let x = if side.x > OBLIQUE {
        egui::Align::Min
    } else if side.x < -OBLIQUE {
        egui::Align::Max
    } else {
        egui::Align::Center
    };
    let y = if side.y > OBLIQUE {
        egui::Align::Min
    } else if side.y < -OBLIQUE {
        egui::Align::Max
    } else {
        egui::Align::Center
    };
    Align2([x, y])
}

/// Draws a dimension's parts (drawing points through `to_screen`); `unit` is the drawing's.
pub(crate) fn paint(painter: &egui::Painter, to_screen: impl Fn(Pt) -> Pos2, parts: &DimParts, unit: &str) {
    // The reference at the scaled distance, a dash and two dots: a line a little past the
    // extension lines, an arc or a square, or a ring on the vertex when there is no scaled part.
    let reference: Vec<Pos2> = parts.reference.iter().map(|&p| to_screen(p)).collect();
    let thin = Stroke::new(1.5, SCALED);
    match reference.as_slice() {
        [] => {}
        [only] => {
            painter.circle_stroke(*only, 4.0, thin);
        }
        [a, b] => {
            let d = (*b - *a).normalized();
            phantom(painter, &[*a - d * REFERENCE_OVER, *b + d * REFERENCE_OVER], thin);
        }
        many => phantom(painter, many, thin),
    }
    // The two stretches along the normal (an angle's bisector), each with a bar at its ends; a
    // zero one is a dot where it would be.
    let stretches = [(to_screen(parts.scaled[0]), to_screen(parts.scaled[1])), (to_screen(parts.annotative[0]), to_screen(parts.annotative[1]))];
    let side = across(&stretches, &reference);
    for ((a, b), color) in stretches.iter().zip([SCALED, ANNOTATIVE]) {
        if a.distance(*b) < 1.0 {
            painter.circle_filled(*a, 3.0, color);
            continue;
        }
        painter.line_segment([*a, *b], Stroke::new(3.0, color));
        let bar = (*b - *a).normalized().rot90() * 5.0;
        for end in [*a, *b] {
            painter.line_segment([end - bar, end + bar], Stroke::new(1.5, color));
        }
    }
    // Their sizes: the scaled one on one side, the annotative on the other, on a light ground
    // so they read over the drawing.
    let (scaled, annotative) = labels(parts, unit);
    for ((text, color), ((a, b), side)) in [(scaled, SCALED), (annotative, ANNOTATIVE)].into_iter().zip(stretches.iter().zip([-side, side])) {
        let galley = painter.layout_no_wrap(text, FontId::proportional(11.0), color);
        let at = a.lerp(*b, 0.5) + side * LABEL_GAP;
        let rect = anchor(side).anchor_size(at, galley.size());
        painter.rect_filled(rect.expand(2.0), 3.0, Color32::from_white_alpha(225));
        painter.galley(rect.min, galley, color);
    }
}

#[cfg(test)]
mod tests {
    use egui::pos2;

    use super::*;

    /// The labels' sides: across the first stretch with a length, pointing right (down when
    /// level), and the anchor facing the point.
    #[test]
    fn labels_go_across_the_stretches() {
        let (o, up, right) = (pos2(0.0, 0.0), pos2(0.0, -20.0), pos2(20.0, 0.0));
        assert_eq!(across(&[(o, up), (up, up)], &[]), vec2(1.0, 0.0), "a vertical stretch: labels left and right");
        assert_eq!(across(&[(o, o), (o, right)], &[]), vec2(0.0, 1.0), "a level stretch: labels above and below");
        assert_eq!(across(&[(o, o), (o, o)], &[o, right]), vec2(1.0, 0.0), "no stretch: across the reference's normal");
        assert_eq!(across(&[(o, o), (o, o)], &[o]), vec2(1.0, 0.0));
        assert_eq!(anchor(vec2(1.0, 0.0)), Align2::LEFT_CENTER);
        assert_eq!(anchor(vec2(-1.0, 0.0)), Align2::RIGHT_CENTER);
        assert_eq!(anchor(vec2(0.0, 1.0)), Align2::CENTER_TOP);
        assert_eq!(anchor(vec2(0.0, -1.0)), Align2::CENTER_BOTTOM);
        assert_eq!(anchor(vec2(0.38, 0.92)), Align2::LEFT_TOP, "oblique: a corner");
        assert_eq!(anchor(vec2(-0.38, -0.92)), Align2::RIGHT_BOTTOM);
    }

    /// The Properties pane colours the same two numbers as the overlay; the labels give the
    /// scaled size in the drawing's unit and the annotative one in paper mm.
    #[test]
    fn fields_and_labels_match_the_overlay() {
        let dim = Kind::Dim {
            a: strix_cad::P::new(0.0, 0.0),
            b: strix_cad::P::new(1.0, 0.0),
            offset: 4.0,
            offset_units: 50.0,
            orient: strix_cad::Orient::Horizontal,
            text: None,
            below: None,
            mark: None,
        };
        let color = |kind: &Kind, key: &str| field_part(kind, key).map(|(c, _)| c);
        assert_eq!((color(&dim, "offsetUnits"), color(&dim, "offset")), (Some(SCALED), Some(ANNOTATIVE)));
        assert_eq!(color(&dim, "ax"), None);
        let circle = Kind::Circle { c: strix_cad::P::new(0.0, 0.0), r: Num::new(1.0) };
        assert_eq!(color(&circle, "radius"), None, "a circle's radius is no part");
        let parts = DimParts {
            reference: vec![Pt::new(0.0, 50.0), Pt::new(1.0, 50.0)],
            scaled: [Pt::new(0.5, 0.0), Pt::new(0.5, 50.0)],
            annotative: [Pt::new(0.5, 50.0), Pt::new(0.5, 90.0)],
            scaled_size: 50.0,
            annotative_size: 4.0,
        };
        let (s, a) = labels(&parts, "cm");
        assert!(s.contains("50 cm") && a.contains("4 mm"), "{s} / {a}");
    }

    /// The reference is a dash and two dots: a long dash, a gap, a dot, a gap, a dot, a gap, and
    /// again; the pattern runs on across corners, and a pattern with no length is solid.
    #[test]
    fn the_reference_is_a_dash_and_two_dots() {
        let pattern = [6.0, 2.0, 1.0, 2.0, 1.0, 2.0];
        let line = pattern_segments(&[pos2(0.0, 0.0), pos2(28.0, 0.0)], &pattern);
        let xs: Vec<(f32, f32)> = line.iter().map(|[a, b]| (a.x, b.x)).collect();
        assert_eq!(xs, vec![(0.0, 6.0), (8.0, 9.0), (11.0, 12.0), (14.0, 20.0), (22.0, 23.0), (25.0, 26.0)]);
        // A corner in the middle of the long dash: the dash goes on round it.
        let bent = pattern_segments(&[pos2(0.0, 0.0), pos2(4.0, 0.0), pos2(4.0, 10.0)], &pattern);
        assert_eq!(bent.first().copied(), Some([pos2(0.0, 0.0), pos2(4.0, 0.0)]));
        assert_eq!(bent.get(1).copied(), Some([pos2(4.0, 0.0), pos2(4.0, 2.0)]));
        assert_eq!(bent.get(2).copied(), Some([pos2(4.0, 4.0), pos2(4.0, 5.0)]));
        // No pattern, or one with a zero length: solid.
        let solid = vec![[pos2(0.0, 0.0), pos2(5.0, 0.0)]];
        assert_eq!(pattern_segments(&[pos2(0.0, 0.0), pos2(5.0, 0.0)], &[]), solid);
        assert_eq!(pattern_segments(&[pos2(0.0, 0.0), pos2(5.0, 0.0)], &[3.0, 0.0]), solid);
        assert!(pattern_segments(&[pos2(1.0, 1.0)], &pattern).is_empty());
        // The overlay's reference uses the CAD linetype of that name.
        assert_eq!(Dash::DashDoubleDotted.pattern().len(), 6);
    }
}
