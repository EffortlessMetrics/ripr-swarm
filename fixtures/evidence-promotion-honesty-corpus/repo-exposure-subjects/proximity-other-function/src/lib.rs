#[derive(Debug, PartialEq)]
pub enum Reduction {
    Failure,
    Over(u64),
}

pub fn reduce(tokens: u64) -> Reduction {
    if tokens > 9 { Reduction::Failure } else { Reduction::Over(tokens) }
}

/// The only test that calls `consume` asserts nothing. The test that does
/// assert pins `reduce`, so it must not lend `consume`'s match arms a strong
/// oracle (#12060).
pub fn consume(tokens: u64) -> bool {
    match reduce(tokens) {
        Reduction::Failure => false,
        Reduction::Over(wait) => wait < 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consume_runs() {
        let _ = consume(1);
    }

    #[test]
    fn reduce_over() {
        assert_eq!(reduce(3), Reduction::Over(3));
    }
}
