pub fn is_verbose(flag: &str) -> bool {
    flag == "-v" || flag == "--verbose"
}

#[cfg(test)]
mod tests {
use super::*;
#[test]
fn long_flag_is_verbose() {
    assert!(is_verbose("--verbose"));
    assert!(!is_verbose("-q"));
}
}
