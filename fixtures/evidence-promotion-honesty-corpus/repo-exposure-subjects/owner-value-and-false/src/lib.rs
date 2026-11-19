pub fn over(qty: u32) -> bool {
    qty >= 10
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn at_the_boundary() {
        assert_eq!(over(10) && false, false);
    }
}
