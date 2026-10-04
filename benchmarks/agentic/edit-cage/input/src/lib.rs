//! B6 edit-cage bench fixture: production code outside the allowed surface.
//! Edits here must fail closed (`violated`), never count as the selected
//! target movement.

pub fn add(left: u64, right: u64) -> u64 {
    left.saturating_add(right)
}
