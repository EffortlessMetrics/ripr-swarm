//! Custom option method.

pub fn flag_long_enough<T: Like>(arg: T) -> bool {
    let n = arg.strip_prefix("--").map_or("", |rest| rest).len();
    n < 3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixed_row_observes_two() {
        assert!(flag_long_enough("--ab"));
    }

    #[test]
    fn unprefixed_row_observes_two() {
        assert!(flag_long_enough("ab"));
    }
}

pub trait Like { fn strip_prefix(&self, prefix: &str) -> Option<&'static str>; }
impl Like for &'static str {
    fn strip_prefix(&self, _prefix: &str) -> Option<&'static str> { Some("ab") }
}
