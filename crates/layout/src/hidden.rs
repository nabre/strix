//! Strix: blocks wholly in hidden text take no room while hidden text isn't shown.

use super::{Block, Document, Paragraph};

/// Nested tables deeper than this are not looked into (the table is then laid out as usual).
const MAX_DEPTH: usize = 32;

/// Whether every run and the mark of `p` are hidden text (its style's formatting included).
fn para_hidden(doc: &Document, p: &Paragraph) -> bool {
    let style = p.props.style.as_deref();
    doc.styles.resolve_char(style, &p.mark).hidden && p.runs.iter().all(|r| doc.styles.resolve_char(style, &r.props).hidden)
}

/// Whether every paragraph of `t` (its nested tables' included) is wholly hidden: Word shows
/// nothing of such a table while hidden text isn't shown, so it takes no room and can't leave a
/// blank page behind it (an exercise's hidden solution at the end of the document).
pub(crate) fn table_hidden(doc: &Document, t: &wordcraft_doc::Table) -> bool {
    table_hidden_at(doc, t, 0)
}

fn table_hidden_at(doc: &Document, t: &wordcraft_doc::Table, depth: usize) -> bool {
    if depth >= MAX_DEPTH || t.rows.is_empty() {
        return false;
    }
    t.rows.iter().all(|row| {
        !row.cells.is_empty()
            && row.cells.iter().all(|cell| {
                !cell.blocks.is_empty()
                    && cell.blocks.iter().all(|b| match &**b {
                        Block::Para(p) => para_hidden(doc, p),
                        Block::Table(t) => table_hidden_at(doc, t, depth + 1),
                    })
            })
    })
}
