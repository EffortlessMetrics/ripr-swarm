/// # Safety
/// `index` must be below `values.len()`.
pub unsafe fn pick(values: &[u8], index: usize) -> u8 {
    *values.get_unchecked(index)
}

pub fn first(values: &[u8]) -> u8 {
    if values.is_empty() {
        return 0;
    }
    unsafe { std::hint::unreachable_unchecked() };
    unsafe { pick(values, 0) }
}
