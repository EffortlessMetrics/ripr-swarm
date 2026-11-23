pub fn is_pow2(n: u32) -> bool {
    n & (n - 1) == 0
}
