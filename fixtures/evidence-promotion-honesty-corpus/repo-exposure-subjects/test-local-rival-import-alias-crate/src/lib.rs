pub fn discounted_total(amount: i32, threshold: i32) -> i32 {
    if amount >= threshold {
        amount - 10
    } else {
        amount
    }
}

#[cfg(test)]
mod tests {
    use crate::discounted_total as dt;

    #[test]
    fn aliased_owner_boundary() {
        assert_eq!(dt(100, 100), 90);
    }
}
