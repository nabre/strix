//! Graph paper (docs/graph-paper.md): the background of a section's pages and the grid blocks
//! in the text flow, for each placement.
use super::*;
use strix_doc::graph_paper::PT_PER_MM;
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
            matches!(first, Some(Placed::Grid { rect, .. }) if (rect.w - pg.body.w).abs() < 0.5 && (rect.y - pg.body.y).abs() < 0.5),
            "the grid comes first, over the body"
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
    assert!((whole.y - l.pages[1].body.y).abs() < 0.5 && (whole.bottom() - l.pages[1].body.bottom()).abs() < 1.0);
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
