pub fn scale(x: u32) -> u32 {
    (x * 3) >> 4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_a_small_input() {
        assert_eq!(scale(4), 0);
    }
}
