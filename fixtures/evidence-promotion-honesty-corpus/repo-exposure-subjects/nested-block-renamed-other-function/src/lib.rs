pub fn discount(amount: u32) -> u32 {
    if amount >= 100 { 10 } else { 0 }
}

pub fn other(amount: u32) -> u32 {
    if amount >= 50 { 5 } else { 0 }
}

#[cfg(test)]
mod tests {
    #[test]
    fn nested_renamed_other() {
        {
            use super::other as discount;
            assert_eq!(discount(50), 5);
            assert_eq!(discount(49), 0);
        }
    }
}
