//! A number field applied once set (one undo step): typed and confirmed (Enter, Tab, a click
//! elsewhere) or dragged and let go. egui's `DragValue` changes its value while it is typed in or
//! dragged; a field drawn from the document every frame would throw that away before it is
//! applied, so the value being set is kept here until then.

use egui::{DragValue, Ui, Vec2};

/// Draw a `DragValue` of `size` showing `now` (`style` sets its range, speed, suffix…). Returns
/// the new value once it is set and differs from `now`.
pub(crate) fn value_field(
    ui: &mut Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    size: Vec2,
    now: f64,
    style: impl for<'a> FnOnce(DragValue<'a>) -> DragValue<'a>,
) -> Option<f64> {
    let key = ui.scope_id().with(("value_field_pending", salt));
    let mut v = ui.data(|d| d.get_temp::<f64>(key)).unwrap_or(now);
    let r = ui.add_sized(size, style(DragValue::new(&mut v)));
    if r.drag_stopped() || r.lost_focus() {
        ui.data_mut(|d| d.remove::<f64>(key));
        return (v.is_finite() && v != now).then_some(v);
    }
    if r.has_focus() || r.dragged() {
        ui.data_mut(|d| d.insert_temp(key, v));
    } else {
        // Not being set: the document's value shows (after an undo, say).
        ui.data_mut(|d| d.remove::<f64>(key));
    }
    None
}
