pub fn discount(amount: u32) -> u32 {
    if amount >= 100 { 10 } else { 0 }
}

pub fn other(amount: u32) -> u32 {
    if amount >= 50 { 5 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_rename_then_owner() {
        {
            use super::other as discount;
            assert_eq!(discount(50), 5);
        }
        assert_eq!(discount(100), 10);
        assert_eq!(discount(99), 0);
    }
}
