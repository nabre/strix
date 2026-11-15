//! An empty mark closing a section before a continuous one takes no room, so the next section's
//! table starts on its line; the mark shows at the end of the line above, not over the table.

use crate::{Arc, Block, Document, LayoutCache, LayoutOptions, Placed, StoryRef, layout};
use wordcraft_doc::section::{SectionProps, SectionStart};
use wordcraft_doc::{Path, Pos, Table};

/// "Title", an empty paragraph, the empty mark, then a table in two columns (and the mark after
/// it): the mark sits beside the empty paragraph's ¶; the table starts at the top of both
/// columns, below the paragraph above, as if the mark weren't there.
#[test]
fn a_quiet_mark_shows_beside_the_line_above_not_over_the_table() {
    crate::test_fonts();
    let mut d = Document::from_text("Title\n\n\nEnd\nAfter");
    d.para_mut(StoryRef::Body, &Path::top(2)).unwrap().section = Some(Box::default());
    let mut two = SectionProps { start: SectionStart::Continuous, ..Default::default() };
    two.columns.count = 2;
    d.para_mut(StoryRef::Body, &Path::top(3)).unwrap().section = Some(Box::new(two));
    d.last_section.start = SectionStart::Continuous;
    d.body.insert(3, Arc::new(Block::Table(Table::new(11, 2, 200.0))));
    let l = layout(&d, &mut LayoutCache::new(), &LayoutOptions::default());
    let (empty, mark) = (l.caret(&Pos::body(1, 0)).unwrap(), l.caret(&Pos::body(2, 0)).unwrap());
    assert!((mark.top - empty.top).abs() < 0.5, "the mark on the empty paragraph's line: {mark:?} vs {empty:?}");
    assert!(mark.x > empty.x + 4.0, "after its ¶: {mark:?} vs {empty:?}");
    let cells: Vec<(f32, f32)> = l.pages[0]
        .items
        .iter()
        .filter_map(|it| match it {
            Placed::Cell { rect, row, cell: 0, .. } => Some((rect.x, rect.y)).filter(|_| *row < 11),
            _ => None,
        })
        .collect();
    let top = cells.iter().map(|c| c.1).fold(f32::MAX, f32::min);
    assert!(top >= empty.top + empty.height - 0.5, "the table starts below the empty paragraph: {top} vs {empty:?}");
    let columns: Vec<f32> = cells.iter().filter(|c| (c.1 - top).abs() < 0.5).map(|c| c.0).collect();
    assert_eq!(columns.len(), 2, "both columns start at the same height: {cells:?}");
    // The mark's line, moved, still ends at the column's edge (its name is drawn up to there).
    let line = l.pages[0].items.iter().find_map(|it| match it {
        Placed::Lines { story: StoryRef::Body, path, para, l0, x, .. } if *path == Path::top(2) => Some(x + para.lines[*l0].right),
        _ => None,
    });
    assert!((line.unwrap() - (72.0 + 468.0)).abs() < 0.5, "to the right margin: {line:?}");
}
