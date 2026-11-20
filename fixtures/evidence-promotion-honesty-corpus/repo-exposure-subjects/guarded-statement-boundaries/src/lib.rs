pub const LIMIT: u32 = 5000;

pub fn assigned(cents: u32, express: bool) -> bool {
    let free = cents >= LIMIT && !express;
    free
}

pub fn wrapped(cents: u32, express: bool) -> bool {
    let free =
        cents >= LIMIT
        && !express;
    free
}

pub fn held(cents: u32, express: bool) -> bool {
    return cents >= LIMIT && !express; // price threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn misses_assigned_guard() {
        assert_eq!(assigned(5000, true), false);
        assert_eq!(assigned(4999, true), false);
    }

    #[test]
    fn misses_wrapped_guard() {
        assert_eq!(wrapped(5000, true), false);
        assert_eq!(wrapped(4999, true), false);
    }

    #[test]
    fn holds_returned_guard() {
        assert_eq!(held(5000, false), true);
        assert_eq!(held(4999, false), false);
    }
}
