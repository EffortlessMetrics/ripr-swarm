pub fn is_separator(c: char) -> bool {
    c == ',' || c == ';'
}

#[cfg(test)]
mod tests {
use super::*;
#[test]
fn semicolon_separates() { assert!(is_separator(';')); }
#[test]
fn dot_does_not_separate() { assert!(!is_separator('.')); }
}
