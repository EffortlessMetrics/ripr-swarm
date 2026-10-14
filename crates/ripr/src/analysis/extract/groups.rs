//! Delimiter balancing shared by the oracle line scanner and the reveal
//! classifier (#7860). Both used to carry their own walker, and the scan
//! copy skipped braces, so the two could split on the same text.
//!
//! The walk counts depth across `(`, `[` and `{` without matching kinds,
//! and reads bytes as given: callers mask strings and comments first when
//! a delimiter inside a literal must not count.

/// The byte index of the delimiter closing the group opened at `open`, or
/// `None` when it never closes or a closer appears before any opener.
pub(crate) fn group_end(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0_usize;
    for (at, byte) in text.bytes().enumerate().skip(open) {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether every closer in `text` has an earlier opener and every opener
/// closes.
pub(crate) fn delimiters_balance(text: &str) -> bool {
    let mut depth = 0_usize;
    for byte in text.bytes() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => match depth.checked_sub(1) {
                Some(next) => depth = next,
                None => return false,
            },
            _ => {}
        }
    }
    depth == 0
}

#[cfg(test)]
mod tests {
    use super::{delimiters_balance, group_end};

    #[test]
    fn group_end_closes_the_group_it_opens_across_all_kinds() {
        assert_eq!(group_end("(a)", 0), Some(2));
        assert_eq!(group_end("f(a[0], { b })", 1), Some(13));
        assert_eq!(group_end("x.call(a).next(b)", 6), Some(8));
        assert_eq!(group_end("[(a)]", 0), Some(4));
        assert_eq!(group_end("{ (a) }", 0), Some(6));
    }

    #[test]
    fn group_end_refuses_an_unclosed_group_or_a_leading_closer() {
        assert_eq!(group_end("(a", 0), None);
        assert_eq!(group_end("f(a, { b )", 1), None);
        assert_eq!(group_end(") (a)", 0), None);
        assert_eq!(group_end("", 0), None);
        assert_eq!(group_end("(a)", 9), None);
    }

    #[test]
    fn delimiters_balance_needs_every_opener_closed_in_order() {
        assert!(delimiters_balance(""));
        assert!(delimiters_balance("f(a[0], { b })"));
        assert!(!delimiters_balance("f(a"));
        assert!(!delimiters_balance("a) (b"));
        assert!(!delimiters_balance("x }"));
    }
}
