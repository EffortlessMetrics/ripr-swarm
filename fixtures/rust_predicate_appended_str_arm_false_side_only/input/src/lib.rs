pub fn is_verbose(flag: &str) -> bool {
    flag == "-v" || flag == "--verbose"
}

#[cfg(test)]
mod tests {
use super::*;
#[test]
fn quiet_flag_is_not_verbose() {
    assert!(!is_verbose("-q"));
}
}
