//! Typed shadow followed by an alias overwrite.

pub fn flag_long_enough(arg: &str) -> bool {
    let n = arg.strip_prefix("--").map_or("", |rest| rest).len();
    let	mut n: usize = n;
    let alias = &mut n;
    *alias = 2;
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
