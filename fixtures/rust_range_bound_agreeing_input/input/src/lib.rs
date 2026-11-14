pub fn sum_to(n: u32) -> u32 {
    (0..=n).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_sum_is_zero() {
        assert_eq!(sum_to(0), 0);
    }
}
