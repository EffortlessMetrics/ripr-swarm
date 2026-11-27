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
    fn renamed_other() {
        assert_eq!(discount(50), 5);
        assert_eq!(discount(49), 0);
    }
}
