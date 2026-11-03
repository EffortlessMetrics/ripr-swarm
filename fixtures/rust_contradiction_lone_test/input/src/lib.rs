pub fn is_adult(age: u32) -> bool {
    age >= 18
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adult_boundary() {
        assert!(is_adult(18));
        assert!(!is_adult(18));
    }
}
