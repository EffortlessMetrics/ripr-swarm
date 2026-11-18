#[derive(Debug, PartialEq)]
pub struct Options {
    pub retries: u32,
    pub verbose: bool,
}

pub fn options_from_flag(flag: &str) -> Options {
    Options {
        retries: 3,
        verbose: flag == "-v" || flag == "--verbose",
    }
}

#[cfg(test)]
mod tests {
use super::*;
#[test]
fn keeps_default_retries() { assert_eq!(options_from_flag("--verbose").retries, 3); }
#[test]
fn reads_verbose_only_from_the_old_flag() {
    let boundary = options_from_flag("--verbose");
    assert_eq!((boundary.retries, options_from_flag("-v").verbose), (3, true));
}
#[test]
fn reads_retries_in_a_closure_of_the_old_flag() {
    let boundary = options_from_flag("--verbose");
    assert_eq!(options_from_flag("-v").verbose.then(|| boundary.retries), Some(3));
}
}
