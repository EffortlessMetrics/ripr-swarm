pub fn is_small(n: u32) -> bool {
    n == 1 || n == 2
}

#[cfg(test)]
mod tests {
use super::*;
#[test]
fn one_is_small() { assert!(is_small(1)); }
#[test]
fn three_is_not_small() { assert!(!is_small(3)); }
}
