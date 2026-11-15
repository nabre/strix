//! Where a quiet section mark shows: an empty paragraph closing a section before a continuous one
//! takes no room (see `flow::place_para`), so the next section's text, or a table starting it,
//! begins on its line. Drawn there, its "Section Break (Continuous)" would run over that text;
//! like a break at the end of a paragraph's text, it shows at the end of the line above instead,
//! where its caret also goes.

use super::placed::Placed;
use super::{Arc, Page, StoryRef};
use crate::para::PARA_MARK_WIDTH;

/// The least room the break's name needs beside the line above (else the mark stays put).
const MIN_ROOM: f32 = 36.0;

fn top_block(path: &wordcraft_doc::Path) -> Option<usize> {
    (path.depth() == 0).then(|| path.0.first().map(|b| *b as usize)).flatten()
}

/// Move the quiet mark of body paragraph `block`, among `page`'s items from `drawn` on, to the end
/// of the last line of the paragraph just above it, when that line is on this page in the same
/// column (left to right, with room left on it).
pub(crate) fn beside_line_above(page: &mut Page, drawn: usize, block: usize) {
    let Some(above) = block.checked_sub(1) else { return };
    let Some(k) = page.items.iter().enumerate().skip(drawn).find_map(|(k, it)| match it {
        Placed::Lines { story: StoryRef::Body, path, .. } if top_block(path) == Some(block) => Some(k),
        _ => None,
    }) else {
        return;
    };
    // The lowest line of the paragraph above: (x, its top, its baseline, where its mark ends).
    let line_above = page.items.iter().take(drawn.min(k)).filter_map(|it| match it {
        Placed::Lines { story: StoryRef::Body, path, para, l0, l1, x, y, .. } if top_block(path) == Some(above) => {
            let first = para.lines.get(*l0)?;
            let last = para.lines.get(l1.checked_sub(1)?)?;
            if last.rtl {
                return None;
            }
            let top = y + last.top - first.top;
            Some((*x, top, top + last.baseline - last.top, x + last.visual_end_x() + PARA_MARK_WIDTH))
        }
        _ => None,
    });
    let Some((ax, _, base, end)) = line_above.reduce(|a, b| if b.1 > a.1 { b } else { a }) else { return };
    let Some(Placed::Lines { para, l0, x, y, .. }) = page.items.get_mut(k) else { return };
    let Some(line) = para.lines.get(*l0) else { return };
    if line.rtl || (*x - ax).abs() > 0.5 || ![end, base].iter().all(|v| v.is_finite()) {
        return;
    }
    let dx = end - (*x + line.visual_end_x());
    if dx <= 0.0 || line.right - dx - line.visual_end_x() < MIN_ROOM {
        return;
    }
    let dy = base - (line.baseline - line.top);
    let mut moved = (**para).clone();
    if let Some(l) = moved.lines.get_mut(*l0) {
        // Its name ends at the column's edge, as before.
        l.right -= dx;
    }
    *para = Arc::new(moved);
    *x += dx;
    *y = dy;
}
