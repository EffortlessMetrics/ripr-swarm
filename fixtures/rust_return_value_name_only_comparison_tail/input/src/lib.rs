pub const LIMIT: u32 = 1000;

pub fn over_limit(n: u32) -> bool {
    n >= LIMIT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limit_itself_is_over() {
        assert_eq!(over_limit(LIMIT), true);
    }
}
