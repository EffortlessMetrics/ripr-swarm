pub fn inc(x: u32) -> u32 {
    x + 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outer_owner_value_still_observes_the_change() {
        {
            fn inc(x: u32) -> u32 { x + 9 }
            assert_eq!(inc(1), 10);
        }
        assert!(super::inc(1) > 0);
        assert_eq!([1, 2].map(inc), [2, 3]);
    }
}
