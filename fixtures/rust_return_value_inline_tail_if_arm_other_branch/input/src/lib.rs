pub fn find_even(x: u32) -> Option<u32> {
    if x % 2 == 0 { None } else { Some(0) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn odd_input_is_zero() {
        assert_eq!(find_even(3), Some(0));
    }
}
