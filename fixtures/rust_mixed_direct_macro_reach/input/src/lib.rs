//! RIPR-SPEC-0117 example 6.

macro_rules! call_inner {
    ($x:expr) => {
        inner($x)
    };
}

pub fn outer(x: i64) -> i64 {
    call_inner!(x)
}

fn inner(x: i64) -> i64 {
    3 * x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outer_triples() {
        assert_eq!(outer(4), 12);
    }

    #[test]
    fn inner_is_positive() {
        assert!(inner(2) > 0);
    }
}
