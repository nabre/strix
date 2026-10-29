//! Graph paper in the layout (docs/graph-paper.md): the background of a section's pages and the
//! grid blocks in the text flow.
//!
//! A grid is placed as the rects it rules ([`Placed::Grid`]), each holding whole squares down
//! its height: one that reaches the bottom of the text area sits on it, the part of a square
//! left over going above its first square; a block of a fixed height stops at its last whole
//! square, the text after it still starting at the height asked for. With
//! [`GraphPaper::follow_columns`] a section's columns are ruled one by one.
use super::ctx::Ctx;
use super::flow::place_para_in_flow;
use super::notes::v_span;
use super::page::PageBuilder;
use super::placed::Placed;
use super::{Arc, Paragraph, Path, Rect, SectionProps, StoryRef};
use strix_doc::graph_paper::{self, GraphPaper, GridPlacement};

/// `r` cut to whole squares down its height: kept on its bottom (`on_bottom`) or on its top.
/// Empty when not one square fits.
pub(crate) fn whole_rows(r: Rect, g: &GraphPaper, on_bottom: bool) -> Rect {
    let h = g.whole_squares(r.h) as f32 * g.sanitized().spacing;
    let h = h.min(r.h.max(0.0));
    if on_bottom { Rect::new(r.x, r.bottom() - h, r.w, h) } else { Rect::new(r.x, r.y, r.w, h) }
}

/// The areas a grid takes between `top` and `bottom` of a page of section `s`: the whole text
/// width, or each text column on its own when the grid follows the columns.
pub(crate) fn page_areas(s: &SectionProps, g: &GraphPaper, top: f32, bottom: f32) -> Vec<Rect> {
    let left = s.margin_left + s.gutter;
    let h = bottom - top;
    let cols = s.column_boxes();
    if g.follow_columns && cols.len() > 1 {
        cols.iter().map(|(cx, cw)| Rect::new(left + cx, top, *cw, h)).collect()
    } else {
        vec![Rect::new(left, top, s.text_width(), h)]
    }
}

/// A section's background graph paper over its pages' body (the margin box, below a header
/// taller than the top margin), sitting on the body's bottom.
pub(crate) fn background(s: &SectionProps, body: Rect) -> Vec<Placed> {
    let Some(g) = s.graph_paper.as_ref() else { return Vec::new() };
    page_areas(s, g, body.y, body.bottom())
        .into_iter()
        .map(|r| whole_rows(r, g, true))
        .filter(|r| r.h > 0.01 && r.w > 0.01)
        .map(|rect| Placed::Grid { rect, grid: Arc::clone(g) })
        .collect()
}

/// Graph paper in the text flow: the paragraph is laid out as usual, its text over a grid that
/// starts at its first line and is as tall as the placement says: a fixed height (moved to the
/// next column or page when it doesn't fit), the rest of the column, or the rest of the page and
/// whole pages after it (a number of them, or until the next odd or even page). Text after a
/// block that reaches the page's end starts on the next page.
pub(crate) fn place_graph_para(ctx: &mut Ctx, pb: &mut PageBuilder, p: &Paragraph, block: usize, body_top: f32, grid: &Arc<GraphPaper>) {
    let g = grid.sanitized();
    let fixed = matches!(g.placement, GridPlacement::Height { .. });
    let room = pb.bottom - pb.y;
    let needs = match g.placement {
        GridPlacement::Height { height } => height.min(pb.bottom - pb.top),
        _ => g.spacing,
    };
    // Not even the block (or one square of a block that fills the page) fits: start it on the
    // next column or page.
    if room + 0.01 < needs && !pb.at_top() {
        pb.advance(block, body_top);
    }
    // A grid down to the page's end sits on it: the part of a square left over goes above it,
    // and the block (its text too) starts at its first square.
    if !fixed {
        let room = pb.bottom - pb.y;
        let over = room - g.whole_squares(room) as f32 * g.spacing;
        if over > 0.01 && over < g.spacing && g.whole_squares(room) > 0 {
            pb.y += over;
        }
    }
    let start_page = pb.pages.len().saturating_sub(1);
    let (x, w, y0) = (pb.col_x(), pb.col_w(), pb.y);
    place_para_in_flow(ctx, pb, p, block, body_top);
    let path = Path::top(block);
    let is_own = |it: &Placed| matches!(it, Placed::Lines { story: StoryRef::Body, path: q, .. } if *q == path);
    // The page where its text starts (it may have moved on), and the grid's top there: where the
    // block starts on its own page, else its first line's top.
    let first = pb.pages.iter().enumerate().skip(start_page).find_map(|(pi, pg)| {
        let ii = pg.items.iter().position(is_own)?;
        let line = pg.items.get(ii).map_or(y0, |it| v_span(it).0);
        Some((pi, if pi == start_page { y0.min(line) } else { line }))
    });
    let last_page = pb.pages.len().saturating_sub(1);
    let (first_page, top) = first.unwrap_or((last_page, y0.min(pb.y)));
    let item = Arc::new(g);
    let put = |pb: &mut PageBuilder, pi: usize, rect: Rect| {
        if rect.h <= 0.01 || rect.w <= 0.01 {
            return;
        }
        let Some(pg) = pb.pages.get_mut(pi) else { return };
        // Behind the paragraph's text: right before its first line on the page.
        let at = pg.items.iter().position(is_own).unwrap_or(pg.items.len());
        pg.items.insert(at, Placed::Grid { rect, grid: Arc::clone(&item) });
    };
    // Pages its text alone runs over: the grid goes down each one to its bottom.
    for pi in first_page..last_page {
        let Some(pg) = pb.pages.get(pi) else { continue };
        let t = if pi == first_page { top } else { pg.body.y };
        let r = whole_rows(Rect::new(x, t, w, pg.body.bottom() - t), &g, true);
        put(pb, pi, r);
    }
    let top = if first_page == last_page { top } else { pb.top };
    let text_bottom = pb.y;
    let bottom = match g.placement {
        GridPlacement::Height { height } => (top + height).max(text_bottom).min(pb.bottom),
        _ => pb.bottom,
    };
    // A fixed height keeps its height in the text but is ruled down to its last whole square;
    // the others sit on the page's end.
    put(pb, last_page, whole_rows(Rect::new(x, top, w, bottom - top), &g, !fixed));
    pb.content_bottom = pb.content_bottom.max(bottom);
    pb.y = pb.y.max(bottom);
    if !fixed {
        // The page's columns after this one are grid down their whole height too.
        let left = pb.sect.margin_left + pb.sect.gutter;
        let rest: Vec<Rect> = pb
            .cols
            .iter()
            .skip(pb.col.saturating_add(1))
            .map(|(cx, cw)| whole_rows(Rect::new(left + cx, pb.top, *cw, pb.bottom - pb.top), &g, true))
            .collect();
        if let Some(pg) = pb.pages.get_mut(last_page) {
            pg.items.extend(rest.into_iter().filter(|r| r.h > 0.01 && r.w > 0.01).map(|rect| Placed::Grid { rect, grid: Arc::clone(&item) }));
        }
    }
    // Whole pages after it.
    let more = match g.placement {
        GridPlacement::Pages { pages } => pages.saturating_sub(1),
        GridPlacement::UntilOdd | GridPlacement::UntilEven => graph_paper::MAX_PAGES,
        _ => 0,
    };
    let odd = g.placement == GridPlacement::UntilOdd;
    for _ in 0..more {
        let next = pb.restart_at.unwrap_or_else(|| pb.number.saturating_add(1));
        if matches!(g.placement, GridPlacement::UntilOdd | GridPlacement::UntilEven) && (next % 2 == 1) == odd {
            break;
        }
        pb.new_page(block, body_top);
        let areas = page_areas(pb.sect, &g, pb.top, pb.bottom);
        if let Some(pg) = pb.pages.last_mut() {
            for r in areas.into_iter().map(|r| whole_rows(r, &g, true)).filter(|r| r.h > 0.01 && r.w > 0.01) {
                pg.items.push(Placed::Grid { rect: r, grid: Arc::clone(&item) });
            }
        }
        pb.y = pb.bottom;
        pb.content_bottom = pb.bottom;
    }
    if !fixed {
        // What follows starts on the next page: no column of this one is left.
        pb.col = pb.cols.len().saturating_sub(1);
        pb.y = pb.bottom;
        pb.max_y = pb.max_y.max(pb.bottom);
    }
}
