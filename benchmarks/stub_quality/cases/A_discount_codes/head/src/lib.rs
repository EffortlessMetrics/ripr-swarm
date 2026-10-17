//! Promo code parsing.
const MAX_PCT: u32 = 60;

/// Parse a code like `SAVE15` into a percent discount.
pub fn parse_discount(code: &str) -> Option<u32> {
    let rest = code.strip_prefix("SAVE")?;
    let pct: u32 = rest.parse().ok()?;
    if pct == 0 {
        return None;
    }
    Some(pct.min(MAX_PCT))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid() {
        assert_eq!(parse_discount("SAVE10"), Some(10));
        assert_eq!(parse_discount("SAVE25"), Some(25));
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(parse_discount("FOO"), None);
        assert_eq!(parse_discount("SAVE0"), None);
        assert_eq!(parse_discount("SAVEx"), None);
    }
}
