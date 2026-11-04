/// Half-open window `[start, end)` of `len` items from `start`, clipped to
/// `max`. `None` when the window is empty or starts at or past `max`.
pub fn clip_window(start: usize, len: usize, max: usize) -> Option<(usize, usize)> {
    if len == 0 || start >= max {
        return None;
    }
    let end = start.saturating_add(len).min(max);
    Some((start, end))
}
