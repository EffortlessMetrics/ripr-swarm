pub fn discount(amount: u32) -> u32 {
    if amount >= 100 { 10 } else { 0 }
}

pub fn other(amount: u32) -> u32 {
    if amount >= 50 { 5 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::other as discount;

    #[test]
    fn qualified_owner() {
        assert_eq!(super::discount(100), 10);
        assert_eq!(super::discount(99), 0);
    }
}
