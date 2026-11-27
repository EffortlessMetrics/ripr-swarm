pub fn discount(qty: u32) -> u32 {
    if qty >= 10 { 5 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::discount;

    #[test]
    fn below_boundary() {
        assert_eq!(discount(1), 0);
    }

    #[test]
    fn boundary() {
        discount(10);
    }
}
