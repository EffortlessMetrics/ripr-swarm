pub fn discount(amount: u32) -> u32 {
    if amount >= 100 { amount - 10 } else { amount }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_binder_then_owner() {
        {
            let discount = |amount: u32| amount;
            assert_eq!(discount(150), 150);
        }
        assert_eq!(discount(150), 140);
    }
}
