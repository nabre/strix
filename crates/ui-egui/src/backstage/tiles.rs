//! Template tiles in rows that fit the page: egui's wrapped layout can't wrap a tile made of
//! nested widgets (it is placed before its size is known), so the rows are worked out from the
//! tiles' widths first.

/// Gap between tiles, both ways.
pub const GAP: f32 = 18.0;

/// Splits tiles of the given widths into rows no wider than `room` (each row at least one tile),
/// as index ranges in order.
pub fn rows(widths: &[f32], room: f32) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut used = 0.0;
    for (i, w) in widths.iter().enumerate() {
        let need = if i == start { *w } else { used + GAP + w };
        if i > start && need > room {
            out.push(start..i);
            start = i;
            used = *w;
        } else {
            used = need;
        }
    }
    if start < widths.len() {
        out.push(start..widths.len());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::rows;

    #[test]
    fn tiles_wrap_into_rows_that_fit() {
        assert_eq!(rows(&[150.0; 6], 900.0), vec![0..5, 5..6]);
        assert_eq!(rows(&[150.0; 6], 990.0), vec![0..6]);
        assert_eq!(rows(&[150.0; 6], 500.0), vec![0..3, 3..6]);
        // Never an empty row, even narrower than one tile.
        assert_eq!(rows(&[150.0, 200.0], 100.0), vec![0..1, 1..2]);
        assert!(rows(&[], 100.0).is_empty());
    }
}
