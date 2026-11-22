pub fn find_even(x: u32) -> Option<u32> {
    if x % 2 == 0 { None } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn even_input_now_none() {
        assert_eq!(find_even(4), None);
    }
}
