//! Graph paper (docs/graph-paper.md): the background of a section's pages and the grid blocks
//! in the text flow, for each placement.
#[allow(unused_imports)]
use crate::*;
use strix_doc::graph_paper::PT_PER_MM;
use strix_doc::graph_paper::{GraphPaper, GridPlacement};
use strix_doc::para_block;

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
    assert!(l.pages.len() <= strix_doc::graph_paper::MAX_PAGES as usize + 3);
    // And the display list draws them.
    let mut out = Vec::new();
    for pg in &l.pages {
        for it in &pg.items {
            if let Placed::Grid { rect, grid } = it {
                display::grid_draws(*rect, grid, 1.0, &mut out);
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
                display::Draw::Path { segs, .. } => segs.iter().filter(|s| matches!(s, strix_doc::graphic::PathSeg::Move(..))).count(),
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
            assert!(matches!(segs.last(), Some(strix_doc::graphic::PathSeg::Close)));
            assert!(segs.iter().any(|s| matches!(s, strix_doc::graphic::PathSeg::Line(x, y) if (*x - (10.0 + 4.0 * side())).abs() < 0.01 && (*y - (20.0 + 3.0 * side())).abs() < 0.01)));
        }
        other => panic!("a frame last: {other:?}"),
    }
    assert_eq!(lines(&framed), 5 + 1);
}
