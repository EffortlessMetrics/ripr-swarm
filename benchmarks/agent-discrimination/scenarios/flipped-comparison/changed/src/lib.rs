//! Membership pricing rules for the storefront.

/// Returns the discount percentage applied at checkout for a member age.
///
/// Members aged 65 and over receive the senior discount.
pub fn discount_percent(age: u32) -> u32 {
    if age > 65 {
        10
    } else {
        0
    }
}

#[cfg(test)]
mod baseline_tests {
    use super::*;

    #[test]
    fn senior_member_gets_discount() {
        assert_eq!(discount_percent(70), 10);
    }

    #[test]
    fn young_member_gets_no_discount() {
        assert_eq!(discount_percent(30), 0);
    }
}
