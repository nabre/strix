//! Graph paper (docs/graph-paper.md): the background of a section's pages and the grid blocks
//! in the text flow, for each placement.
#[allow(unused_imports)]
use crate::*;
use wordcraft_doc::graph_paper::PT_PER_MM;
use wordcraft_doc::graph_paper::{GraphPaper, GridPlacement};
use wordcraft_doc::para_block;

fn lay(doc: &Document) -> DocLayout {
    let mut c = LayoutCache::new();
    layout(doc, &mut c, &LayoutOptions::default())
}

fn para(text: &str) -> Arc<Block> {
    para_block(Paragraph::with_text(text, Default::default()))
}

fn grid_para(placement: GridPlacement) -> Arc<Block> {
    let mut p = Paragraph::new();
    p.props.graph = Some(Arc::new(GraphPaper { placement, ..GraphPaper::default() }));
    para_block(p)
}

/// The grid rects on each page.
fn grids(l: &DocLayout) -> Vec<Vec<Rect>> {
    l.pages.iter().map(|pg| pg.items.iter().filter_map(|it| if let Placed::Grid { rect, .. } = it { Some(*rect) } else { None }).collect()).collect()
}

/// The page the body paragraph `block` starts on.
fn page_of(l: &DocLayout, block: usize) -> Option<usize> {
    let path = Path::top(block);
    l.pages.iter().position(|pg| pg.items.iter().any(|it| matches!(it, Placed::Lines { story: StoryRef::Body, path: q, .. } if *q == path)))
}

fn doc_with(blocks: Vec<Arc<Block>>) -> Document {
    let mut doc = Document::from_text("");
    doc.body = blocks;
    doc
}

#[test]
fn background_covers_the_body_of_every_page_behind_the_text() {
    let mut doc = doc_with((0..80).map(|i| para(&format!("Line {i}"))).collect());
    doc.last_section.graph_paper = Some(Arc::new(GraphPaper { placement: GridPlacement::Background, ..GraphPaper::default() }));
    let l = lay(&doc);
    assert!(l.pages.len() >= 2);
    for pg in &l.pages {
        let first = pg.items.first();
        assert!(
            matches!(first, Some(Placed::Grid { rect, .. }) if (rect.w - pg.body.w).abs() < 0.5 && rect.y >= pg.body.y - 0.01 && (rect.bottom() - pg.body.bottom()).abs() < 0.05),
            "the grid comes first, over the body, sitting on its bottom"
        );
    }
}

#[test]
fn page_end_fills_the_rest_of_the_page_and_pushes_the_text_after_it() {
    let doc = doc_with(vec![para("Before"), grid_para(GridPlacement::PageEnd), para("After")]);
    let l = lay(&doc);
    let g = grids(&l);
    assert_eq!(g.first().map(Vec::len), Some(1));
    let (r, body) = (g[0][0], l.pages[0].body);
    assert!(r.y > body.y + 1.0, "it starts below the text before it");
    assert!((r.bottom() - body.bottom()).abs() < 1.0, "to the end of the page");
    assert_eq!(page_of(&l, 2), Some(1), "the text after it starts on the next page");
}

#[test]
fn a_number_of_pages_takes_the_rest_of_this_one_and_whole_pages_after() {
    let doc = doc_with(vec![para("Before"), grid_para(GridPlacement::Pages { pages: 3 }), para("After")]);
    let l = lay(&doc);
    let g = grids(&l);
    assert!(g.len() >= 4);
    assert!(g[..3].iter().all(|p| p.len() == 1));
    let whole = g[1][0];
    // A whole page: whole squares on the bottom margin, less than a square below its top.
    let body = l.pages[1].body;
    assert!(whole.y >= body.y - 0.01 && whole.y - body.y < GraphPaper::default().spacing && (whole.bottom() - body.bottom()).abs() < 0.05);
    assert_eq!(page_of(&l, 2), Some(3));
}

#[test]
fn until_the_next_odd_or_even_page() {
    // On page 1 (odd): until the next odd page means pages 1 and 2; until the next even one, 1.
    let l = lay(&doc_with(vec![para("Before"), grid_para(GridPlacement::UntilOdd), para("After")]));
    assert_eq!(page_of(&l, 2), Some(2));
    assert_eq!(grids(&l).iter().filter(|g| !g.is_empty()).count(), 2);
    let l = lay(&doc_with(vec![para("Before"), grid_para(GridPlacement::UntilEven), para("After")]));
    assert_eq!(page_of(&l, 2), Some(1));
    assert_eq!(grids(&l).iter().filter(|g| !g.is_empty()).count(), 1);
}

#[test]
fn a_height_is_a_block_in_the_flow() {
    let h = 40.0 * PT_PER_MM;
    let l = lay(&doc_with(vec![para("Before"), grid_para(GridPlacement::Height { height: h }), para("After")]));
    let g = grids(&l);
    let r = g[0][0];
    assert!((r.h - h).abs() < 0.5, "{} vs {h}", r.h);
    assert_eq!(page_of(&l, 2), Some(0), "the text after it goes on below it");
    let after = l.pages[0].items.iter().find_map(|it| match it {
        Placed::Lines { path, y, .. } if *path == Path::top(2) => Some(*y),
        _ => None,
    });
    assert!(after.is_some_and(|y| y >= r.bottom() - 0.5));
}

#[test]
fn a_height_that_does_not_fit_moves_to_the_next_page() {
    let mut blocks: Vec<Arc<Block>> = (0..40).map(|i| para(&format!("Line {i}"))).collect();
    blocks.push(grid_para(GridPlacement::Height { height: 120.0 * PT_PER_MM }));
    let n = blocks.len();
    let l = lay(&doc_with(blocks));
    let first = page_of(&l, 0);
    let grid_page = grids(&l).iter().position(|g| !g.is_empty());
    assert_eq!(page_of(&l, n - 1), grid_page);
    assert!(grid_page > first);
}

#[test]
fn hostile_grids_lay_out() {
    let hostile =
        GraphPaper { spacing: f32::NAN, placement: GridPlacement::Pages { pages: u32::MAX }, major_every: u32::MAX, ..GraphPaper::default() };
    let mut p = Paragraph::new();
    p.props.graph = Some(Arc::new(hostile));
    let mut doc = doc_with(vec![para_block(p), grid_para(GridPlacement::Height { height: f32::INFINITY })]);
    doc.last_section.graph_paper = Some(Arc::new(GraphPaper { spacing: -3.0, ..GraphPaper::default() }));
    let l = lay(&doc);
    assert!(l.pages.len() <= wordcraft_doc::graph_paper::MAX_PAGES as usize + 3);
    // And the display list draws them.
    let mut out = Vec::new();
    for pg in &l.pages {
        for it in &pg.items {
            if let Placed::Grid { rect, grid, sheet } = it {
                display::grid_sheet_draws(*rect, grid, sheet.as_ref(), 1.0, &mut out);
            }
        }
    }
    assert!(!out.is_empty());
}

/// The side of the default square.
fn side() -> f32 {
    GraphPaper::default().spacing
}

/// `h` is a whole number of squares.
fn whole(h: f32) -> bool {
    let n = h / side();
    (n - n.round()).abs() < 0.01 && n.round() >= 1.0
}

/// The bottom of the body paragraph `block`'s lines on page `page`.
fn text_bottom(l: &DocLayout, page: usize, block: usize) -> Option<f32> {
    l.pages.get(page)?.items.iter().find_map(|it| match it {
        Placed::Lines { path, .. } if *path == Path::top(block) => Some(crate::notes::v_span(it).1),
        _ => None,
    })
}

#[test]
fn a_grid_to_the_page_end_sits_on_the_bottom_margin_in_whole_squares() {
    // Whatever the text above it, the grid ends on the body's bottom and is whole squares tall;
    // the part of a square left over goes between the text above and the first square.
    for before in [1, 2, 3, 7] {
        let mut blocks: Vec<Arc<Block>> = (0..before).map(|i| para(&format!("Line {i}"))).collect();
        blocks.push(grid_para(GridPlacement::PageEnd));
        let l = lay(&doc_with(blocks));
        let r = grids(&l)[0][0];
        let body = l.pages[0].body;
        assert!((r.bottom() - body.bottom()).abs() < 0.05, "on the bottom: {} vs {}", r.bottom(), body.bottom());
        assert!(whole(r.h), "{} is whole squares", r.h);
        let above = text_bottom(&l, 0, before - 1).unwrap();
        assert!(r.y >= above - 0.5 && r.y - above < side() + 12.0, "just below the text before it: {} / {above}", r.y);
    }
}

#[test]
fn a_grid_follows_the_text_before_it() {
    // More text above moves the block down and shortens it, still on the bottom margin.
    let lay_with = |n: usize| {
        let mut blocks: Vec<Arc<Block>> = (0..n).map(|i| para(&format!("Line {i}"))).collect();
        blocks.push(grid_para(GridPlacement::PageEnd));
        let l = lay(&doc_with(blocks));
        grids(&l)[0][0]
    };
    let (a, b) = (lay_with(1), lay_with(12));
    assert!(b.y > a.y + 5.0 * 12.0, "{} then {}", a.y, b.y);
    assert!((a.bottom() - b.bottom()).abs() < 0.05);
}

#[test]
fn a_fixed_height_rules_whole_squares_and_keeps_its_height() {
    // 42 mm of 5 mm squares: 8 squares drawn, the text after it 42 mm below the grid's top.
    let h = 42.0 * PT_PER_MM;
    let l = lay(&doc_with(vec![para("Before"), grid_para(GridPlacement::Height { height: h }), para("After")]));
    let r = grids(&l)[0][0];
    assert!((r.h - 8.0 * side()).abs() < 0.05, "{} vs {}", r.h, 8.0 * side());
    let after = l.pages[0].items.iter().find_map(|it| match it {
        Placed::Lines { path, y, .. } if *path == Path::top(2) => Some(*y),
        _ => None,
    });
    assert!(after.is_some_and(|y| y >= r.y + h - 0.5 && y < r.y + h + 20.0), "{after:?} vs {}", r.y + h);
}

fn two_columns(doc: &mut Document) {
    doc.last_section.columns.count = 2;
    doc.last_section.columns.space = 36.0;
}

#[test]
fn a_background_follows_the_columns_when_asked() {
    let mut doc = doc_with(vec![para("Text")]);
    two_columns(&mut doc);
    let bg = |follow: bool| GraphPaper { placement: GridPlacement::Background, follow_columns: follow, ..GraphPaper::default() };
    doc.last_section.graph_paper = Some(Arc::new(bg(false)));
    let l = lay(&doc);
    let g = &grids(&l)[0];
    assert_eq!(g.len(), 1, "across the whole text width");
    assert!((g[0].w - l.pages[0].body.w).abs() < 0.5);
    doc.last_section.graph_paper = Some(Arc::new(bg(true)));
    let l = lay(&doc);
    let g = &grids(&l)[0];
    let cols = doc.last_section.column_boxes();
    assert_eq!(g.len(), 2, "one a column");
    let left = doc.last_section.margin_left + doc.last_section.gutter;
    for (r, (cx, cw)) in g.iter().zip(&cols) {
        assert!((r.x - (left + cx)).abs() < 0.05 && (r.w - cw).abs() < 0.05, "{r:?} on column {cx} {cw}");
        assert!(whole(r.h) && (r.bottom() - l.pages[0].body.bottom()).abs() < 0.05);
    }
    assert!(g[1].x - g[0].right() > 35.0, "the space between the columns is clear");
}

#[test]
fn whole_pages_of_a_block_follow_the_columns_when_asked() {
    for follow in [false, true] {
        let mut p = Paragraph::new();
        p.props.graph = Some(Arc::new(GraphPaper { placement: GridPlacement::Pages { pages: 2 }, follow_columns: follow, ..GraphPaper::default() }));
        let mut doc = doc_with(vec![para("Before"), para_block(p), para("After")]);
        two_columns(&mut doc);
        let l = lay(&doc);
        let g = grids(&l);
        assert_eq!(g[1].len(), if follow { 2 } else { 1 }, "follow {follow}: {:?}", g[1]);
        // The block's first page: its column, then the page's other column.
        assert_eq!(g[0].len(), 2);
        assert!(g.iter().flatten().all(|r| whole(r.h)));
    }
}

#[test]
fn a_frame_of_its_own_weight_replaces_the_edge_lines() {
    let rect = Rect::new(10.0, 20.0, 4.0 * side() + 3.0, 3.0 * side());
    let draws = |g: GraphPaper| {
        let mut out = Vec::new();
        display::grid_draws(rect, &g, 1.0, &mut out);
        out
    };
    let lines = |d: &[display::Draw]| -> usize {
        d.iter()
            .map(|d| match d {
                display::Draw::Path { segs, .. } => segs.iter().filter(|s| matches!(s, wordcraft_doc::graphic::PathSeg::Move(..))).count(),
                _ => 0,
            })
            .sum()
    };
    // Today's look: 5 + 4 lines, no frame.
    let plain = draws(GraphPaper::default());
    assert_eq!(lines(&plain), 9);
    let framed = draws(GraphPaper { frame: Some(2.5), ..GraphPaper::default() });
    // 3 + 2 inner lines, and the frame round the whole squares.
    match framed.last() {
        Some(display::Draw::Path { segs, stroke_width, .. }) => {
            assert_eq!(*stroke_width, 2.5);
            assert!(matches!(segs.last(), Some(wordcraft_doc::graphic::PathSeg::Close)));
            assert!(segs.iter().any(|s| matches!(s, wordcraft_doc::graphic::PathSeg::Line(x, y) if (*x - (10.0 + 4.0 * side())).abs() < 0.01 && (*y - (20.0 + 3.0 * side())).abs() < 0.01)));
        }
        other => panic!("a frame last: {other:?}"),
    }
    assert_eq!(lines(&framed), 5 + 1);
}

#[test]
fn a_grid_across_columns_is_one_sheet_unless_it_follows_them() {
    // Two columns, a grid to the page's end in the left one below some text: not following the
    // columns, the space between them is ruled too, from the grid's top in the left column, on
    // the left column's squares (every vertical line of every rect on them), all on the bottom.
    for follow in [false, true] {
        let mut p = Paragraph::new();
        p.props.graph = Some(Arc::new(GraphPaper { placement: GridPlacement::PageEnd, follow_columns: follow, ..GraphPaper::default() }));
        let mut doc = doc_with(vec![para("Before"), para("More text before"), para_block(p), para("After")]);
        two_columns(&mut doc);
        let l = lay(&doc);
        let g = &grids(&l)[0];
        let body = l.pages[0].body;
        let left = doc.last_section.margin_left + doc.last_section.gutter;
        let cols = doc.last_section.column_boxes();
        let (c0, c1) = (cols[0], cols[1]);
        let first = g.iter().copied().find(|r| (r.x - (left + c0.0)).abs() < 0.05).expect("a grid starting on the left column");
        assert!(first.y > body.y + 1.0, "below the text before it");
        let covers = |x: f32, y: f32| g.iter().any(|r| x >= r.x - 0.01 && x <= r.right() + 0.01 && y >= r.y - 0.01 && y <= r.bottom() + 0.01);
        let gap_x = left + c0.0 + c0.1 + (c1.0 - c0.0 - c0.1) / 2.0;
        // The right column is ruled down its whole height either way.
        assert!(covers(left + c1.0 + 1.0, body.y + side()), "follow {follow}: {g:?}");
        assert!(g.iter().all(|r| whole(r.h) && (r.bottom() - body.bottom()).abs() < 0.05));
        if follow {
            assert!(!covers(gap_x, first.y + side()), "following the columns, the space between them is clear: {g:?}");
            continue;
        }
        for y in [first.y + 0.5, (first.y + body.bottom()) / 2.0, body.bottom() - 0.5] {
            assert!(covers(gap_x, y), "the space between the columns at {y}: {g:?}");
        }
        assert!(!covers(gap_x, first.y - side()), "beside the text before it, the space stays clear: {g:?}");
        // One sheet: every rect's left edge and every vertical line on the left column's squares,
        // every horizontal line on the same rows.
        for r in g {
            let n = (r.x - first.x) / side();
            assert!((n - n.round()).abs() < 0.01, "{r:?} off the squares of {first:?}");
            let rows = (r.y - first.y) / side();
            assert!((rows - rows.round()).abs() < 0.01, "{r:?} off the rows of {first:?}");
        }
        assert!(g.iter().all(|r| r.right() <= body.right() + 0.05), "inside the text width");
        // The ruled area reaches the right column's right edge but for less than a square.
        let far = g.iter().map(|r| first.x + ((r.right() - first.x) / side() + 0.001).floor() * side()).fold(0.0, f32::max);
        assert!(body.right() - far < side() + 0.05, "{far} vs {}", body.right());
    }
}

#[test]
fn a_sheet_across_columns_has_its_heavier_lines_and_its_frame_in_common() {
    // Heavier lines every 4 squares and a frame of its own: across the columns the heavier lines
    // count from the start column's grid's top-left corner in every part, and one frame goes round
    // the whole sheet (no seam between the parts).
    let every = 4;
    let mut p = Paragraph::new();
    p.props.graph = Some(Arc::new(GraphPaper { placement: GridPlacement::PageEnd, major_every: every, frame: Some(2.5), ..GraphPaper::default() }));
    // Enough text before for heavier lines above the block in the right column.
    let mut blocks: Vec<Arc<Block>> = (0..12).map(|i| para(&format!("Line {i}"))).collect();
    blocks.extend([para_block(p), para("After")]);
    let mut doc = doc_with(blocks);
    two_columns(&mut doc);
    let l = lay(&doc);
    let body = l.pages[0].body;
    let parts: Vec<_> = l.pages[0]
        .items
        .iter()
        .filter_map(|it| if let Placed::Grid { rect, grid, sheet } = it { Some((*rect, grid.clone(), sheet.clone())) } else { None })
        .collect();
    assert_eq!(parts.len(), 2, "{parts:?}");
    let first = parts[0].0;
    assert!(parts.iter().all(|(_, _, s)| s.as_ref().is_some_and(|s| s.origin == (first.x, first.y))), "one origin: {parts:?}");
    let mut out = Vec::new();
    for (rect, grid, sheet) in &parts {
        display::grid_sheet_draws(*rect, grid, sheet.as_ref(), 1.0, &mut out);
    }
    let major_w = GraphPaper::default().major.width;
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    let mut frames = Vec::new();
    for d in &out {
        let display::Draw::Path { segs, stroke_width, .. } = d else { continue };
        if (*stroke_width - 2.5).abs() < 0.01 {
            frames.push(segs.clone());
            continue;
        }
        if (*stroke_width - major_w).abs() > 0.01 {
            continue;
        }
        for pair in segs.chunks(2) {
            if let [wordcraft_doc::graphic::PathSeg::Move(x0, y0), wordcraft_doc::graphic::PathSeg::Line(x1, y1)] = pair {
                if (x0 - x1).abs() < 0.01 {
                    xs.push(*x0);
                } else if (y0 - y1).abs() < 0.01 {
                    ys.push(*y0);
                }
            }
        }
    }
    let unit = every as f32 * side();
    let on = |v: f32, o: f32| {
        let n = (v - o) / unit;
        (n - n.round()).abs() < 0.01
    };
    assert!(xs.iter().all(|x| on(*x, first.x)), "heavier verticals off the sheet's: {xs:?} from {}", first.x);
    assert!(ys.iter().all(|y| on(*y, first.y)), "heavier horizontals off the sheet's: {ys:?} from {}", first.y);
    let cols = doc.last_section.column_boxes();
    let right_col = doc.last_section.margin_left + doc.last_section.gutter + cols[1].0;
    assert!(xs.iter().any(|x| *x > right_col), "heavier lines in the right column too: {xs:?}");
    assert!(ys.iter().any(|y| *y < first.y - 0.5), "heavier lines above the start column's grid: {ys:?}");
    // One frame, round the union: from the start column's left edge to the right column's, down
    // to the bottom margin, and up to the right column's top.
    assert_eq!(frames.len(), 1, "one frame for the sheet: {frames:?}");
    let pts: Vec<(f32, f32)> = frames[0]
        .iter()
        .filter_map(|s| match s {
            wordcraft_doc::graphic::PathSeg::Move(x, y) | wordcraft_doc::graphic::PathSeg::Line(x, y) => Some((*x, *y)),
            _ => None,
        })
        .collect();
    assert!(matches!(frames[0].last(), Some(wordcraft_doc::graphic::PathSeg::Close)));
    let min_x = pts.iter().map(|p| p.0).fold(f32::MAX, f32::min);
    let max_x = pts.iter().map(|p| p.0).fold(f32::MIN, f32::max);
    let min_y = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min);
    let max_y = pts.iter().map(|p| p.1).fold(f32::MIN, f32::max);
    assert!((min_x - first.x).abs() < 0.05 && body.right() - max_x < side() + 0.05, "{pts:?}");
    assert!((max_y - body.bottom()).abs() < 0.05 && min_y < first.y - 0.5 && min_y - body.y < side(), "{pts:?}");
    assert_eq!(pts.len(), 6, "an L round the two parts: {pts:?}");
}

/// The column the body paragraph `block` starts in on `page`: the x of its first line.
fn line_x(l: &DocLayout, page: usize, block: usize) -> Option<f32> {
    l.pages.get(page)?.items.iter().find_map(|it| match it {
        Placed::Lines { story: StoryRef::Body, path, x, .. } if *path == Path::top(block) => Some(*x),
        _ => None,
    })
}

#[test]
fn column_end_fills_the_rest_of_the_column_and_moves_the_text_to_the_next() {
    // Two columns: a grid to the end of the left column below some text, the text after it at the
    // top of the right column, on the same page, and nothing ruled there.
    for follow in [false, true] {
        let mut p = Paragraph::new();
        p.props.graph = Some(Arc::new(GraphPaper { placement: GridPlacement::ColumnEnd, follow_columns: follow, ..GraphPaper::default() }));
        let mut doc = doc_with(vec![para("Before"), para_block(p), para("After")]);
        two_columns(&mut doc);
        let l = lay(&doc);
        let g = grids(&l);
        let body = l.pages[0].body;
        let left = doc.last_section.margin_left + doc.last_section.gutter;
        let cols = doc.last_section.column_boxes();
        assert_eq!(g[0].len(), 1, "one rect, in the left column: {:?}", g[0]);
        let r = g[0][0];
        assert!((r.x - (left + cols[0].0)).abs() < 0.05 && (r.w - cols[0].1).abs() < 0.05, "{r:?}");
        assert!(r.y > body.y + 1.0, "below the text before it");
        assert!((r.bottom() - body.bottom()).abs() < 0.05, "to the end of the column");
        assert!(whole(r.h));
        assert_eq!(page_of(&l, 2), Some(0), "the text after it stays on the page");
        let x = line_x(&l, 0, 2).unwrap_or(0.0);
        assert!(x >= left + cols[1].0 - 0.5, "the text after it is in the right column: {x}");
    }
}

#[test]
fn column_end_in_the_last_column_or_one_column_goes_on_the_next_page() {
    let doc = doc_with(vec![para("Before"), grid_para(GridPlacement::ColumnEnd), para("After")]);
    let l = lay(&doc);
    let g = grids(&l);
    let (r, body) = (g[0][0], l.pages[0].body);
    assert!((r.bottom() - body.bottom()).abs() < 0.05 && r.y > body.y + 1.0);
    assert_eq!(page_of(&l, 2), Some(1), "one column: the text after it starts on the next page");

    // Two columns, the grid in the right one: the text after it on the next page, left column.
    let mut blocks = vec![para("Before")];
    blocks.push(grid_para(GridPlacement::ColumnEnd));
    blocks.push(grid_para(GridPlacement::ColumnEnd));
    blocks.push(para("After"));
    let mut doc = doc_with(blocks);
    two_columns(&mut doc);
    let l = lay(&doc);
    let g = grids(&l);
    let left = doc.last_section.margin_left + doc.last_section.gutter;
    let cols = doc.last_section.column_boxes();
    assert_eq!(g[0].len(), 2, "{:?}", g[0]);
    assert!((g[0][1].x - (left + cols[1].0)).abs() < 0.05 && g[0][1].y - l.pages[0].body.y < side(), "the second fills the right column");
    assert_eq!(page_of(&l, 3), Some(1));
    assert!(line_x(&l, 1, 3).is_some_and(|x| x < left + cols[0].1));
}
