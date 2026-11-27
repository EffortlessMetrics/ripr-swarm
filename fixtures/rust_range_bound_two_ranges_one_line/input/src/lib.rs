pub fn both(a: u32, b: u32) -> u32 {
    (0..=a).sum::<u32>() + (0..=b).sum::<u32>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_sums_are_zero() {
        assert_eq!(both(0, 0), 0);
    }
}
