pub const fn kib(n: usize) -> usize {
    n * 1024
}

/// True for powers of two.
pub const fn is_pow2(n: u32) -> bool {
    n & (n - 1) == 0
}

/// # Safety
/// `i` must be < xs.len().
pub unsafe fn get_unchecked_or(xs: &[i32], i: usize, _d: i32) -> i32 {
    unsafe { *xs.get_unchecked(i) }
}

#[cfg(test)]
mod tests {
    use super::*;
    const _: () = assert!(kib(1) == 1024);
    #[test]
    fn pow2() {
        assert!(is_pow2(4));
        assert!(!is_pow2(6));
    }
    #[test]
    fn sizes() {
        assert_eq!(kib(4), 4096);
    }
    #[test]
    fn unchecked() {
        assert_eq!(unsafe { get_unchecked_or(&[7, 8], 1, 0) }, 8);
    }
}
